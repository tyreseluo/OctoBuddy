//! An outer loop on octos instead of Claude Code: a session of OctoBuddy's
//! own octos server (the one the inner loops use), on the model the person
//! picked from the OctoSense AI providers.
//!
//! It speaks the same protocol as a Claude Code outer loop — its replies
//! carry the same plan, send and review blocks — so the rest of OctoBuddy
//! does not know which one runs. Its events arrive as a peer's (the server
//! names it `outer:<session id>`) and are said again as the outer loop's
//! here, before anything else sees them.
//!
//! What differs: octos takes one turn at a time, so what the person sends
//! while it works waits here; its rules go in its first message (octos has
//! no appended system prompt); it only reads, so whatever approval it asks
//! for (a write, a command) is refused.
use crate::events::LoopEvent;
use crate::i18n;
use crate::model::SessionRef;
use crate::OctoBuddyView;
use std::collections::VecDeque;

/// How the server names an outer loop's session among its peers.
const PREFIX: &str = "outer:";

/// One session's outer loop on octos, in this run.
#[derive(Debug, Default)]
pub struct OctosOuter {
    /// Its octos session is open on this server.
    opened: bool,
    /// The turn it runs, and the text that started it (sent again when
    /// octos refuses it).
    turn: Option<(String, String)>,
    /// What waits for it, in order.
    queue: VecDeque<String>,
    /// What its reply has said so far (octos may end a turn with no text).
    text: String,
    /// What its session has cost so far, as octos last said.
    cost: Option<f64>,
}

/// What its octos session opens with beyond the directory: the model the
/// person picked (none: the server's profile's own).
fn open_params(model: Option<&str>) -> serde_json::Value {
    match model {
        Some(label) => serde_json::json!({"profile_id": crate::providers::model_profile_id(label)}),
        None => serde_json::json!({}),
    }
}

pub fn peer_id(session: &str) -> String {
    format!("{PREFIX}{session}")
}

fn key(session: &str) -> String {
    format!("local:octobuddy:{}", peer_id(session))
}

impl OctoBuddyView {
    /// Sends `text` to a session's octos outer loop: now, or after the turn
    /// it runs. `rules` go first in its conversation's first message.
    pub(crate) fn octos_outer_send(&mut self, at: SessionRef, cwd: &str, text: &str, rules: &str) -> Result<(), String> {
        let Some(session) = self.store.session(at).cloned() else { return Err("no session".into()) };
        // A new conversation starts with the outer loop's rules.
        let text = if session.lead_session.is_none() {
            format!("{rules}\n\n---\n\n{text}")
        } else {
            text.to_string()
        };
        let busy = self.rt.octos_outers.get(&session.id).is_some_and(|o| o.turn.is_some());
        if busy {
            self.rt.octos_outers.get_mut(&session.id).unwrap().queue.push_back(text);
            return Ok(());
        }
        self.octos_outer_start(at, cwd, text)
    }

    fn octos_outer_start(&mut self, at: SessionRef, cwd: &str, text: String) -> Result<(), String> {
        let Some(session) = self.store.session(at).cloned() else { return Err("no session".into()) };
        let (peer, key) = (peer_id(&session.id), key(&session.id));
        let opened = self.rt.octos_outers.get(&session.id).is_some_and(|o| o.opened);
        let model = session.outer_model().map(String::from);
        let serve = self.serve()?;
        let turn = if opened {
            serve.start_turn(&peer, &key, &text)?
        } else {
            let turn = serve.open_with(&peer, &key, cwd, Some(&text), open_params(model.as_deref()), true)?;
            turn.ok_or("octos opened no turn")?
        };
        let o = self.rt.octos_outers.entry(session.id.clone()).or_default();
        o.opened = true;
        o.text.clear();
        o.turn = Some((turn, text));
        if let Some(s) = self.store.session_mut(at) {
            // Its conversation lives in octos under this key: said once, resumed after.
            s.lead_session = Some(key);
        }
        Ok(())
    }

    /// octos stopped under the outer loops: the turn each ran waits first.
    pub(crate) fn octos_outers_requeue(&mut self) {
        for o in self.rt.octos_outers.values_mut() {
            o.opened = false;
            if let Some((_, text)) = o.turn.take() {
                o.queue.push_front(text);
            }
        }
    }

    /// octos is back: each outer loop's next message goes.
    pub(crate) fn octos_outers_restart(&mut self) {
        let waiting: Vec<(String, String)> = self.rt.octos_outers.iter_mut()
            .filter(|(_, o)| o.turn.is_none())
            .filter_map(|(id, o)| o.queue.pop_front().map(|t| (id.clone(), t)))
            .collect();
        for (session, text) in waiting {
            let Some(at) = self.store.find_session(&session) else { continue };
            let cwd = self.store.session(at).and_then(|s| s.work_dir.clone()).unwrap_or_else(|| self.store.projects[at.0].path.clone());
            if let Err(err) = self.octos_outer_start(at, &cwd, text) {
                self.system(at, &i18n::pick(format!("Could not reach the outer loop: {err}"), format!("无法连接 outer：{err}")));
            }
        }
    }

    /// Stops the turn a session's octos outer loop runs, and what waits for it.
    pub(crate) fn octos_outer_stop(&mut self, session: &str) {
        let Some(o) = self.rt.octos_outers.get_mut(session) else { return };
        o.queue.clear();
        let turn = o.turn.as_ref().map(|(t, _)| t.clone());
        if let (Some(turn), Some(serve)) = (turn, self.rt.serve.as_ref()) {
            let _ = serve.interrupt(&key(session), &turn);
        }
    }

    /// An event of an octos outer loop, said as the outer loop's (`None`:
    /// OctoBuddy has taken care of it). Every other event passes unchanged.
    pub(crate) fn octos_outer_event(&mut self, e: LoopEvent) -> Option<LoopEvent> {
        let peer = match &e {
            LoopEvent::PeerTurnStarted { peer } | LoopEvent::PeerActivity { peer, .. } | LoopEvent::PeerDelta { peer, .. }
            | LoopEvent::PeerTool { peer, .. } | LoopEvent::PeerToolEnd { peer, .. } | LoopEvent::PeerQuestion { peer, .. }
            | LoopEvent::PeerSteered { peer, .. } | LoopEvent::PeerSteerDropped { peer, .. } | LoopEvent::PeerTurnEnded { peer, .. }
            | LoopEvent::PeerTurnRejected { peer, .. } | LoopEvent::PeerInfo { peer, .. } | LoopEvent::PeerRound { peer }
            | LoopEvent::PeerFileChanged { peer, .. } | LoopEvent::PeerAgents { peer, .. } | LoopEvent::PeerSubagent { peer, .. }
            | LoopEvent::PeerCost { peer, .. } | LoopEvent::PeerApprovalGone { peer, .. } | LoopEvent::PeerApproval { peer, .. }
            | LoopEvent::PeerFailed { peer, .. } => peer.clone(),
            _ => return Some(e),
        };
        let Some(session) = peer.strip_prefix(PREFIX).map(String::from) else { return Some(e) };
        let at = self.store.find_session(&session)?;
        match e {
            LoopEvent::PeerTurnStarted { .. } => Some(LoopEvent::LeadStatus { session, status: "thinking".into() }),
            LoopEvent::PeerActivity { line, .. } => Some(LoopEvent::LeadStatus { session, status: line }),
            LoopEvent::PeerDelta { text, .. } => {
                if let Some(o) = self.rt.octos_outers.get_mut(&session) {
                    o.text.push_str(&text);
                }
                Some(LoopEvent::LeadDelta { session, text })
            }
            LoopEvent::PeerTool { id, name, detail, .. } => Some(LoopEvent::LeadTool { session, id, name, detail }),
            LoopEvent::PeerToolEnd { id, ok, .. } => Some(LoopEvent::LeadToolEnd { session, id, ok }),
            // Its subagents show as its Agent steps (and on the flow canvas).
            LoopEvent::PeerSubagent { id, title, status, .. } => {
                if matches!(status.as_str(), "done" | "completed" | "complete" | "failed" | "error" | "cancelled" | "closed") {
                    Some(LoopEvent::LeadToolEnd { session, id, ok: !matches!(status.as_str(), "failed" | "error") })
                } else {
                    Some(LoopEvent::LeadTool { session, id, name: "Agent".into(), detail: title })
                }
            }
            LoopEvent::PeerInfo { model, .. } => model.map(|model| LoopEvent::LeadInfo { session, model }),
            LoopEvent::PeerCost { cost, .. } => {
                self.rt.octos_outers.entry(session).or_default().cost = Some(cost);
                None
            }
            LoopEvent::PeerTurnEnded { outcome, text, .. } => {
                let o = self.rt.octos_outers.entry(session.clone()).or_default();
                o.turn = None;
                let said = std::mem::take(&mut o.text);
                let cost = o.cost;
                let next = o.queue.pop_front();
                let ok = outcome == "completed";
                let (text, error) = if ok { (if text.trim().is_empty() { said } else { text }, None) } else { (said, Some(if outcome == "interrupted" { outcome } else { text })) };
                // What waited goes now; its reply follows this one.
                if let Some(next) = next {
                    let cwd = self.store.session(at).and_then(|s| s.work_dir.clone()).unwrap_or_else(|| self.store.projects[at.0].path.clone());
                    if let Err(err) = self.octos_outer_start(at, &cwd, next) {
                        self.system(at, &i18n::pick(format!("Could not reach the outer loop: {err}"), format!("无法连接 outer：{err}")));
                    }
                }
                // Its reply is one message: closed here, so the next turn's is a new one.
                self.apply_event(LoopEvent::LeadMessage { session: session.clone(), text: text.clone() });
                self.rt.lead_live.remove(&session);
                Some(LoopEvent::LeadTurnDone { session, ok, error, lead_session: None, text, cost })
            }
            LoopEvent::PeerTurnRejected { .. } => {
                // A turn ran after all: the text waits for it.
                let o = self.rt.octos_outers.entry(session).or_default();
                if let Some((_, text)) = o.turn.take() {
                    o.queue.push_front(text);
                }
                None
            }
            LoopEvent::PeerFailed { error, .. } => {
                if let Some(o) = self.rt.octos_outers.get_mut(&session) {
                    o.turn = None;
                    o.opened = false;
                }
                Some(LoopEvent::LeadTurnDone { session, ok: false, error: Some(error), lead_session: None, text: String::new(), cost: None })
            }
            // The outer loop only reads: what it asks leave to do is refused.
            LoopEvent::PeerApproval { approval_id, title, .. } => {
                if let Some(serve) = self.rt.serve.as_ref() {
                    let _ = serve.answer_approval(&key(&session), &approval_id, false, None);
                }
                self.system(at, &i18n::pick(format!("The outer loop asked to {title}: refused, it only reads (the inner loops change files)."),
                    format!("outer 请求 {title}：已拒绝，outer 只读（改文件交给 inner）。")));
                None
            }
            LoopEvent::PeerQuestion { question_id, options, .. } => {
                if let Some(serve) = self.rt.serve.as_ref() {
                    let _ = serve.answer_question(&key(&session), &question_id, &options, "Decide yourself, and say what you chose.");
                }
                None
            }
            _ => None,
        }
    }
}
