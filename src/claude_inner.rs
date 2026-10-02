//! Inner loops on Claude Code instead of octos: one `claude` process per
//! inner loop (stream-json, as the outer loop runs), working in its folder
//! with the tools to edit and to run commands (in Claude Code's sandbox),
//! on Anthropic or, through OctoBuddy's loopback proxy, on one of the
//! person's other providers (Z.ai's GLM, …: `claude_proxy.rs`).
//!
//! Its events come as an outer loop's (`Lead*`, session `inner:<peer>`) and
//! are said here as an inner loop's (`Peer*`), so everything after — the
//! line, the commits, the checks, the reports — is the octos inner loop's.
use crate::events::LoopEvent;
use crate::lead::{Lead, Mode};
use crate::{i18n, providers, OctoBuddyView};

/// The prefix of an inner loop's process in `Lead*` events.
const PREFIX: &str = "inner:";

/// What runs a session's inner loops: `claude` (Claude Code) or octos.
pub const ENGINE: &str = "claude";

impl OctoBuddyView {
    /// Whether `peer` runs on Claude Code: its session says so for new inner
    /// loops; one that ran on octos already stays there (its context is there).
    /// (Or on Codex or pi: they run as Claude Code does, a Lead each.)
    pub(crate) fn on_claude(&self, peer: &str) -> bool {
        let Some(session) = self.store.find_peer(peer).and_then(|at| self.store.session(at)) else { return false };
        let Some(p) = session.peers().iter().find(|p| p.id == peer) else { return false };
        // Its slice named its agent: there.
        if p.agent_named == Some(true) {
            return p.agent.as_deref().is_some_and(crate::rpc_lead::lead_engine);
        }
        p.claude_session.is_some() || self.rt.claude_inners.contains_key(peer)
            || (crate::rpc_lead::lead_engine(session.inner_engine()) && !self.rt.opened.contains(peer) && p.log().iter().all(|e| e.outcome.is_none() && e.reply.is_none()))
    }

    /// The engine an inner loop runs on: the one it began on, else its session's.
    fn inner_engine_of(&self, p: &crate::model::Peer) -> String {
        let session = self.store.find_peer(&p.id).and_then(|at| self.store.session(at)).map(|s| s.inner_engine().to_string()).unwrap_or_else(|| ENGINE.into());
        match p.agent.as_deref() {
            Some(a) if (p.claude_session.is_some() || p.agent_named == Some(true)) && crate::rpc_lead::lead_engine(a) => a.to_string(),
            _ if crate::rpc_lead::lead_engine(&session) => session,
            _ => ENGINE.into(),
        }
    }

    /// Starts a turn of `peer` on Claude Code (its process first, when it has none).
    pub(crate) fn claude_inner_start(&mut self, peer: &str, text: &str) -> Result<Option<String>, String> {
        if !self.rt.claude_inners.contains_key(peer) {
            let p = self.store.find_peer(peer).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| p.id == peer)).cloned()
                .ok_or("no such inner loop")?;
            let pick = self.store.find_peer(peer).and_then(|at| self.store.session(at)).and_then(|s| s.inner_model().map(String::from));
            // A slice's own model, else the session's pick for its inner loops.
            let pick = p.model_pick.clone().or(pick);
            let (env, model) = match pick.as_deref() {
                // A provider of the person's (`family/model`): through the proxy.
                Some(label) if label.contains('/') => {
                    let route = providers::claude_route(label)?;
                    if self.rt.claude_proxy.is_none() {
                        self.rt.claude_proxy = crate::claude_proxy::Proxy::start();
                    }
                    let proxy = self.rt.claude_proxy.as_ref().ok_or("OctoBuddy's proxy could not start")?;
                    (proxy.env_for(&route), Some(route.model.clone()))
                }
                // Anthropic, on the person's own Claude Code login.
                other => (Vec::new(), other.map(String::from)),
            };
            let resume = p.claude_session.clone();
            // OctoBuddy's tools for it: its check and its app, run outside its sandbox.
            let mcp: Vec<(String, String)> = self.mcp.as_ref().map(|m| vec![("octobuddy".to_string(), m.inner_config(peer))]).unwrap_or_default();
            let engine = self.inner_engine_of(&p);
            // The effort its session picked for its inner loops, if its engine takes it.
            let effort = self.store.find_peer(peer).and_then(|at| self.store.session(at)).and_then(|s| s.inner_effort.clone())
                .filter(|e| crate::model::effort_levels(&engine).contains(&e.as_str()));
            let lead = if engine == ENGINE {
                Lead::spawn(&self.rt.inbox, &format!("{PREFIX}{peer}"), &p.dir, resume.as_deref(), "", model.as_deref(), effort.as_deref(), &mcp, Mode::Inner, &env)?
            } else {
                let label = pick.clone().filter(|l| l.contains('/'));
                self.spawn_rpc(&engine, &format!("{PREFIX}{peer}"), &p.dir, resume.as_deref(), crate::lead::INNER_RULES, label.as_deref(), effort.as_deref(), &mcp, Mode::Inner)?
            };
            self.rt.claude_inners.insert(peer.to_string(), lead);
            // Its card and messages say what it runs on.
            if let Some(p) = self.store.peer_mut(peer) {
                p.agent = Some(engine);
            }
        }
        self.rt.claude_inners[peer].send(text)?;
        Ok(Some(crate::model::new_id("t")))
    }

    pub(crate) fn claude_inner_interrupt(&mut self, peer: &str) -> Result<(), String> {
        match self.rt.claude_inners.get(peer) {
            Some(lead) => lead.interrupt(),
            None => Ok(()),
        }
    }

    /// Its process goes (the inner loop was closed).
    pub(crate) fn claude_inner_stop(&mut self, peer: &str) {
        self.rt.claude_inners.remove(peer);
    }

    /// An event of an inner loop on Claude Code, said as an inner loop's
    /// (`None`: taken care of here). Every other event passes unchanged.
    pub(crate) fn claude_inner_event(&mut self, e: LoopEvent) -> Option<LoopEvent> {
        let session = match &e {
            LoopEvent::LeadStatus { session, .. } | LoopEvent::LeadDelta { session, .. } | LoopEvent::LeadTool { session, .. }
            | LoopEvent::LeadToolEnd { session, .. } | LoopEvent::LeadSubTool { session, .. } | LoopEvent::LeadInfo { session, .. }
            | LoopEvent::LeadMessage { session, .. } | LoopEvent::LeadTurnDone { session, .. } | LoopEvent::LeadExited { session, .. } => session.clone(),
            _ => return Some(e),
        };
        let Some(peer) = session.strip_prefix(PREFIX).map(String::from) else { return Some(e) };
        match e {
            LoopEvent::LeadStatus { status, .. } if status == "thinking" => Some(LoopEvent::PeerTurnStarted { peer }),
            LoopEvent::LeadStatus { status, .. } => Some(LoopEvent::PeerActivity { peer, line: status }),
            LoopEvent::LeadDelta { text, .. } => Some(LoopEvent::PeerDelta { peer, text }),
            LoopEvent::LeadTool { id, name, detail, .. } => {
                // What it writes is its own: committed from its `Commit:` line.
                if matches!(name.as_str(), "Edit" | "Write" | "MultiEdit" | "NotebookEdit") && !detail.is_empty() {
                    self.apply_event(LoopEvent::PeerFileChanged { peer: peer.clone(), path: detail.clone() });
                }
                Some(LoopEvent::PeerTool { peer, id, name, detail })
            }
            LoopEvent::LeadToolEnd { id, ok, .. } => Some(LoopEvent::PeerToolEnd { peer, id, ok }),
            LoopEvent::LeadSubTool { .. } => None,
            LoopEvent::LeadInfo { model, .. } => Some(LoopEvent::PeerInfo { peer, model: Some(model), effort: None }),
            // One answer of its model: a step, as octos counts them.
            LoopEvent::LeadMessage { .. } => Some(LoopEvent::PeerRound { peer }),
            LoopEvent::LeadTurnDone { ok, error, lead_session, text, cost, .. } => {
                if let Some(p) = self.store.peer_mut(&peer) {
                    if lead_session.is_some() {
                        p.claude_session = lead_session;
                    }
                }
                if let Some(cost) = cost {
                    self.apply_event(LoopEvent::PeerCost { peer: peer.clone(), input: 0, output: 0, cost, context_window: None });
                }
                let outcome = match (ok, error.as_deref()) {
                    (true, _) => "completed",
                    (false, Some("interrupted")) => "interrupted",
                    _ => "failed",
                };
                let text = if ok { text } else { error.map(|e| if text.is_empty() { e } else { format!("{text}\n\n{e}") }).unwrap_or(text) };
                Some(LoopEvent::PeerTurnEnded { peer, outcome: outcome.into(), text })
            }
            LoopEvent::LeadExited { error, gen, .. } => {
                // A process already replaced: its late exit is nothing to it.
                if self.rt.claude_inners.get(&peer).is_none_or(|l| l.gen != gen) {
                    return None;
                }
                self.rt.claude_inners.remove(&peer);
                let busy = self.rt.lines.get(&peer).is_some_and(|l| l.is_busy());
                busy.then(|| LoopEvent::PeerTurnEnded {
                    peer,
                    outcome: "failed".into(),
                    text: error.unwrap_or_else(|| i18n::t("Claude Code exited", "Claude Code 退出了").into()),
                })
            }
            other => Some(other),
        }
    }
}
