//! The two loops, wired together: the person talks to the lead; the lead's
//! plans start peers and its messages steer them; the person may talk to a
//! peer too; peers' results go back to the lead. `lib.rs` draws; this runs.
use crate::dispatch::{Delivery, From, Line, Mode, Step};
use crate::events::{self, Inbox, LoopEvent};
use crate::inner::Serve;
use crate::lead::Lead;
use crate::model::{self, end_step, now_secs, upsert_step, Exchange, Peer, Role, SessionRef};
use crate::{i18n, plan, providers, stream, workspace, OctoBuddyView, EXTENSION, MAX_EXTENSIONS, MAX_ROUNDS};
use makepad_widgets::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// What waits for the outer loop while it works (OctoBuddy holds it, so the
/// person can reorder or drop it; Claude's own queue could not be changed).
#[derive(Clone, Debug, PartialEq)]
pub enum OuterWork {
    /// A message from the person.
    Person(String),
    /// The inner loops' reports that are ready when its turn comes.
    Reports,
    /// OctoBuddy asking again (a block it could not read).
    Note(String),
    /// A message of the person's already in the conversation: steered into
    /// a turn that ended before it was taken. Sent as theirs, not shown again.
    Steered(String),
}

#[derive(Clone, Debug)]
pub struct OuterItem {
    pub id: String,
    pub work: OuterWork,
}

/// The live side of the loops: processes and lines, never saved.
#[derive(Default)]
pub struct Runtime {
    pub inbox: Inbox,
    /// Session id → its lead.
    pub leads: HashMap<String, Lead>,
    /// Session id → messages the lead has been given and not answered yet.
    pub(crate) lead_pending: HashMap<String, u32>,
    pub(crate) serve: Option<Serve>,
    /// Peer id → its line.
    pub(crate) lines: HashMap<String, Line>,
    /// Peers whose octos session is open in the current server.
    pub(crate) opened: HashSet<String>,
    /// Peer id → the message its running turn answers (to re-queue a refused start).
    pub(crate) inflight: HashMap<String, Delivery>,
    /// Peer id → (approval id, title, body) waiting for the person.
    pub approvals: HashMap<String, (String, String, String)>,
    /// Session id → reports the lead has not seen: (peer id, report, forwarded
    /// by OctoBuddy because the peer did not report).
    pub(crate) unreported: HashMap<String, Vec<(String, String, bool)>>,
    /// Peers with a task from the lead they have not reported on yet: at the
    /// end of that task's turn OctoBuddy forwards the reply if they did not.
    pub(crate) pending: HashSet<String>,
    /// Peers the person made for an outer loop, on their first task (the
    /// person's): its report goes to that outer loop, as a task of its own
    /// would. Whatever the person says to a peer after stays between them.
    pub(crate) person_tasks: HashSet<String>,
    /// Session id → what the person and peers said to each other since the
    /// lead last heard: told in the next STATUS, without waking the lead.
    pub(crate) notes: HashMap<String, Vec<String>>,
    /// Session id → the lead's cumulative cost last reported by its process.
    lead_cost_seen: HashMap<String, f64>,
    /// Session id → the lead message being streamed (its index) and its raw text.
    pub(crate) lead_live: HashMap<String, (usize, String)>,
    /// Peer id → (question id, question, options) waiting for the person.
    pub questions: HashMap<String, (String, String, Vec<String>)>,
    /// Peers whose work OctoBuddy is checking (agent-spec, tests).
    pub(crate) checking: HashSet<String>,
    /// Peers whose checks the person re-ran: the verdict is a note, not a review.
    rechecks: HashSet<String>,
    /// Project path → Claude Code's (model, effort) settings there, read when first shown.
    claude_settings: HashMap<String, (Option<String>, Option<String>)>,
    /// The lead's Agent call id → (its step's own detail, tool calls its subagent made).
    subagents: HashMap<String, (String, u32)>,
    /// Session id → its worktree's branch against the project's, and what is
    /// uncommitted in it; read when first shown, dropped when work lands.
    pub worktrees: HashMap<String, (Result<workspace::BranchDiff, String>, Vec<String>)>,
    /// Session id → when the lead's running turn was given its message.
    lead_started: HashMap<String, u64>,
    /// Session id → what waits for its outer loop, in order.
    pub outer_queue: HashMap<String, std::collections::VecDeque<OuterItem>>,
    /// Peers of a later wave, held until the earlier waves are accepted: their directory.
    pub(crate) held: HashMap<String, String>,
    /// Sessions told their review budget is used up (said once).
    budget_said: HashSet<String>,
    /// Sessions whose work moved on (a check passed, or a commit went in)
    /// since their review budget was last given: it is extended when used up.
    progress: HashSet<String>,
    /// Sessions whose context pack is being built (their outer loop is busy).
    pub(crate) packing: HashSet<String>,
    /// Session id → when its outer loop took the current turn up (its first
    /// event after the send): the timeline and "took" start there.
    pub(crate) lead_began: HashMap<String, u64>,
    /// Session id → how many times its budget was extended this request.
    pub(crate) extended: HashMap<String, u32>,
    /// Peers the outer loop interrupted, or told not to commit: their next
    /// turn's files are not committed for them (a pause is not a finished task).
    no_auto_commit: HashSet<String>,
    /// Session id → since when its ready reports wait for the others' (one
    /// review for several: see `maybe_review`).
    pub(crate) batch_since: HashMap<String, u64>,
    /// Peer id → the files OctoBuddy is committing for it now.
    committing: HashMap<String, Vec<String>>,
    /// Peer id → files its tools changed that OctoBuddy has not committed yet.
    pub(crate) to_commit: HashMap<String, Vec<String>>,
    /// Peer id → how many of its subagents run now.
    pub running_agents: HashMap<String, u64>,
    /// Peer id → its branch against the project's, read when first shown.
    pub diffs: HashMap<String, Result<workspace::BranchDiff, String>>,
    /// Peers waiting for a slot (too many run at once), in order.
    pub(crate) slot_wait: std::collections::VecDeque<String>,
    /// How often octos was started again for a held data directory.
    pub(crate) serve_retries: u32,
    /// Session id → what its outer loop was given and has not answered, in order.
    pub(crate) lead_inflight: HashMap<String, std::collections::VecDeque<String>>,
    /// Session id → its outer loop on octos (when it runs on octos).
    pub(crate) octos_outers: HashMap<String, crate::octos_outer::OctosOuter>,
    /// Inner loops on Claude Code: peer id → its process.
    pub(crate) claude_inners: HashMap<String, crate::lead::Lead>,
    /// OctoBuddy's loopback proxy to other providers for Claude Code.
    pub(crate) claude_proxy: Option<crate::claude_proxy::Proxy>,
    /// Plain chats on OctoSense's agent: session id → its request context.
    pub(crate) system_chats: HashMap<String, std::sync::Arc<dyn octosense_app_peers::OctosContext>>,
    /// Peers that moved to another outer loop and have not described
    /// themselves to it yet: their next report says so.
    joined: HashSet<String>,
}

impl Runtime {
    /// The outer loop, twice: for its chip (what it does now, its
    /// subagents, its cost) and under the input (its model and effort).
    pub fn outer_label(&mut self, session: &crate::model::Session, project: &str, status: &str) -> (String, String) {
        let (model, effort) = self.claude_settings.entry(project.to_string()).or_insert_with(|| crate::lead::claude_settings(project)).clone();
        // The one picked, until its process says which it resolved to.
        let picked = session.outer_model().map(String::from).filter(|_| session.lead_model.is_none());
        let model = picked.or(session.lead_model.clone()).or(model).map(|m| m.trim_start_matches("claude-").to_string()).unwrap_or_else(|| "default model".into());
        let effort = effort.map(|e| format!(" · {e} effort")).unwrap_or_default();
        let running = session.messages.iter().rev().take(6).filter_map(|m| m.steps.as_ref()).flatten()
            .filter(|st| matches!(st.name.as_str(), "Agent" | "Task") && st.status == "running").count();
        let subagents = if running > 0 { i18n::pick(format!(" · {running} subagent(s)"), format!(" · {running} 个子 agent")) } else { String::new() };
        let cost = session.lead_cost.map(|c| format!(" · ${c:.2}")).unwrap_or_default();
        if session.engine() == crate::system_chat::ENGINE {
            let model = session.lead_model.clone().unwrap_or_else(|| crate::i18n::t("its model", "主模型").into());
            return (format!("Outer · {status}{cost}"), format!("OctoSense · octos · {model}"));
        }
        if session.engine() == "octos" {
            // On octos: the model it reported, else the one picked, else the inner loops' own.
            let model = session.lead_model.clone().or(session.outer_model().map(String::from))
                .unwrap_or_else(|| crate::i18n::t("default model", "默认模型").into());
            return (format!("Outer · {status}{subagents}{cost}"), format!("octos · {model}"));
        }
        // Codex or pi: on the provider picked (its model as it reported it).
        if session.engine() == crate::rpc_lead::CODEX || session.engine() == crate::rpc_lead::PI {
            let name = if session.engine() == crate::rpc_lead::CODEX { "Codex" } else { "pi" };
            let model = session.lead_model.clone().or(session.outer_model().map(String::from))
                .unwrap_or_else(|| crate::i18n::t("default model", "默认模型").into());
            return (format!("Outer · {status}{cost}"), format!("{name} · {model}"));
        }
        (format!("Outer · {status}{subagents}{cost}"), format!("Claude Code · {model}{effort}"))
    }

    /// The index of the lead message being streamed in a session.
    pub fn streaming_lead(&self, session: &str) -> Option<usize> {
        self.lead_live.get(session).map(|(i, _)| *i)
    }

    /// What waits in a peer's line: (id, whose, text).
    pub fn waiting(&self, peer: &str) -> Vec<(String, &'static str, String)> {
        self.lines.get(peer).map(|l| l.queue.iter().map(|d| {
            (d.id.clone(), if d.from == From::Lead { "outer" } else { "you" }, d.text.clone())
        }).collect()).unwrap_or_default()
    }

    /// Files the peer changed that OctoBuddy has not committed.
    pub fn uncommitted(&self, peer: &str) -> Vec<String> {
        self.to_commit.get(peer).cloned().unwrap_or_default()
    }

    pub fn queued(&self, peer: &str) -> usize {
        self.lines.get(peer).map(|l| l.queue.len()).unwrap_or(0)
    }
}

fn session_key(peer: &str) -> String {
    format!("local:octobuddy:{peer}")
}

fn first_prompt(brief: &str, dir: &str, branch: Option<&str>, check: Option<&str>, context: &str) -> String {
    let place = match branch {
        Some(branch) => format!("You work in {dir}, the session's git worktree of the project, on branch {branch}."),
        None => format!("You work in {dir}, the project directory."),
    };
    let place = format!("{place} The outer loop and other inner loops work in the same directory at the same time: \
touch only what your task needs, and never revert or reformat changes you did not make.");
    let check = match check {
        Some(cmd) => format!("\nWhen you finish, OctoBuddy runs `{cmd}` in your directory and gives the lead its result: make it pass."),
        None => String::new(),
    };
    let context = format!("\nThe outer loop's plan and decisions are in {context}/outer.md, and the other inner loops' reports \
in {context}/reports/: read them when your task touches theirs (OctoBuddy keeps them current; do not edit them). \
{context}/pack.md, when there, holds what OctoBuddy gathered for this request — the project's memory hits and an \
index of the docs (headings with line numbers): read it first, then only the doc sections your task needs."); 
    format!("You are an inner-loop worker of OctoBuddy. {place}\n\nTask from the lead:\n{brief}\n\n{REPORT_RULES}{COMMIT_RULE}{check}{context}")
}

/// How many inner loops may run turns at once, across sessions
/// (`OCTOBUDDY_MAX_INNERS`, else 4): the rest wait for a slot.
/// How long ready reports wait for other inner loops' before the outer loop
/// reviews them (one review for several, as the outer loop costs the most).
/// A user message's `meta` when it was steered into a running turn: sent,
/// taken up by the agent, or sent after the turn instead.
pub(crate) const STEER: &str = "steer";
pub(crate) const STEER_TAKEN: &str = "steer-taken";
pub(crate) const STEER_QUEUED: &str = "steer-queued";

/// How long an outer loop asked to stop may take before it is ended.
const STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// What `继续` asks of an outer loop whose turn the person stopped (as
/// Cindy's: it goes on in the same conversation, nothing replayed).
const CONTINUE: &str = "The person stopped your last turn and now asks you to go on. Look over the conversation, continue from where \
you were cut off, and do not repeat steps that already had effects outside this conversation (commands run, files written, \
messages sent): check what they left first.";

const BATCH_SECS: u64 = 120;

fn max_inners() -> usize {
    std::env::var("OCTOBUDDY_MAX_INNERS").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(4)
}

/// Loops share one directory, so OctoBuddy commits each loop's own files (in
/// a worktree octos's sandbox cannot write git's data anyway).
const COMMIT_RULE: &str = "\nDo not run git commit (or git add): OctoBuddy commits for you, exactly the files your file tools \
changed, as the person. When a reply of yours changed files, end it with one line `Commit: <message>`, \
the message written the way this project writes them (see git log).";

/// How a peer reports (after Cindy's worker rules): the lead sees only the
/// report block, once per task, never progress.
const REPORT_RULES: &str = "Rules:\n\
1. Do the task. Build and test when you can.\n\
2. When the lead's task is complete, or you are blocked, end your reply with ONE report block; it is the only \
thing the lead sees:\n```octobuddy-report\nwhat you changed, what you ran to check it, whether it passed \
(verified, partially verified or unverified), and anything the lead must decide\n```\n\
3. Report once per task; never send progress. If you need an answer from the lead, say so in the report.\n\
4. Messages from the person are between you and them: the lead does not see them. Answer the person in your \
reply, without a report block.\n\
5. If critical context is missing before a destructive or broad change, report that you are blocked instead of guessing.";

/// A later message to a peer. `lead_waits`: a task from the lead is still
/// open, which the peer reports on once done, whoever spoke last.
fn later_prompt(from: From, text: &str, lead_waits: bool) -> String {
    match (from, lead_waits) {
        (From::Lead, _) => format!("Message from the lead:\n{text}\n\n(Report on it with an octobuddy-report block when done or blocked.)"),
        (From::Person, true) => format!("Message from the person (the user you work for):\n{text}\n\n\
(The lead's task is still open: when it is done, report on it with an octobuddy-report block.)"),
        (From::Person, false) => format!("Message from the person (the user you work for):\n{text}\n\n\
(This stays between you and the person: answer them directly, without a report block.)"),
    }
}

impl OctoBuddyView {
    pub(crate) fn system(&mut self, at: SessionRef, text: &str) {
        self.store.push_message(at, Role::System, "", text);
    }

    /// The peers' state for the lead: each peer, what waits in its line (the
    /// lead may change its own queued messages by id), and what the person
    /// and peers said since the lead last heard. Taking it clears the notes.
    /// The models a slice may name: the providers' other models (the
    /// primary is the default). None when there is no choice.
    pub(crate) fn models_line(&self, at: SessionRef) -> Option<String> {
        // The person pinned this session's inner model: every slice runs on it.
        if let Some(pinned) = self.store.session(at).and_then(|s| s.inner_model().map(String::from)) {
            return Some(format!("MODELS: every inner loop of this session runs on {pinned} (the person's choice): leave `model` out."));
        }
        let others: Vec<&str> = self.providers.rows.iter().filter(|r| r.role != "primary").map(|r| r.label.as_str()).collect();
        if others.is_empty() {
            return None;
        }
        let default = self.providers.primary().map(|r| r.label.as_str()).unwrap_or("the profile's own");
        Some(format!("MODELS the inner loops can run on (a slice's `model`): {} ; default (leave it out): {default}", others.join(", ")))
    }

    pub(crate) fn status_block(&mut self, at: SessionRef) -> Option<String> {
        let session = self.store.session(at)?.clone();
        if session.peers().is_empty() {
            return None;
        }
        let mut lines: Vec<String> = Vec::new();
        let closed: Vec<&str> = session.peers().iter().filter(|p| p.status == "closed").map(|p| p.slug.as_str()).collect();
        for p in session.peers().iter().filter(|p| p.status != "closed") {
            // octos steps are model calls; an estimated round is a whole
            // write-run-verify cycle, several steps: said as such.
            let rounds = match (p.rounds_used, p.estimate) {
                (Some(used), Some(est)) => format!(", estimated {est} rounds, took {used} steps (model calls)"),
                (Some(used), None) => format!(", took {used} steps (model calls)"),
                _ => String::new(),
            };
            lines.push(format!("- {} ({}): {}{rounds}", p.slug, p.role(), p.status));
            if let Some(line) = self.rt.lines.get(&p.id) {
                for d in &line.queue {
                    let who = if d.from == From::Lead { "yours" } else { "the person's" };
                    let text: String = d.text.chars().take(100).collect();
                    lines.push(format!("    queued {} ({who}): {text}", d.id));
                }
            }
        }
        if !closed.is_empty() {
            lines.push(format!("- closed (take no more work): {}", closed.join(", ")));
        }
        if let Some(models) = self.models_line(at) {
            lines.push(models);
        }
        if let Some(notes) = self.rt.notes.remove(&session.id).filter(|n| !n.is_empty()) {
            lines.push("Since you last heard:".into());
            lines.extend(notes.into_iter().map(|n| format!("    {n}")));
        }
        Some(format!("STATUS\n{}", lines.join("\n")))
    }

    /// Gives the lead a message: now if it is free, else it waits in the
    /// outer queue (where the person can reorder or drop it).
    pub(crate) fn tell_lead(&mut self, at: SessionRef, text: String) {
        let Some(id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        if self.lead_busy(&id) {
            self.queue_outer(&id, OuterWork::Note(text));
        } else {
            self.send_lead_now(at, text);
        }
    }

    /// The outer loop's turn cut short and its process ended (another
    /// engine or model picked while it worked): what it was doing is let go.
    pub(crate) fn cut_outer(&mut self, session: &str) {
        if let Some(lead) = self.rt.leads.get(session) {
            let _ = lead.interrupt();
        }
        self.rt.leads.remove(session);
        self.octos_outer_stop(session);
        self.rt.lead_pending.remove(session);
        self.rt.lead_inflight.remove(session);
        self.rt.lead_live.remove(session);
        self.rt.lead_started.remove(session);
        self.rt.lead_began.remove(session);
        self.lead_status.remove(session);
    }

    pub(crate) fn lead_busy(&self, session: &str) -> bool {
        self.rt.lead_pending.get(session).copied().unwrap_or(0) > 0 || self.rt.packing.contains(session)
            // Its own CLI holds it (the native TUI plugin): what is for it waits.
            || self.tui_holds_outer(session)
    }

    fn queue_outer(&mut self, session: &str, work: OuterWork) {
        let item = OuterItem { id: Delivery::new(From::Lead, "").id, work };
        self.rt.outer_queue.entry(session.to_string()).or_default().push_back(item);
    }

    /// The next thing waiting for the outer loop, if it is free now.
    pub(crate) fn drain_outer(&mut self, at: SessionRef) {
        let Some(id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        if self.lead_busy(&id) || self.halted.contains(&id) {
            return;
        }
        let Some(item) = self.rt.outer_queue.get_mut(&id).and_then(|q| q.pop_front()) else { return };
        match item.work {
            OuterWork::Person(text) => self.deliver_person(at, text),
            OuterWork::Steered(text) => self.deliver_person_shown(at, text, true),
            OuterWork::Note(text) => self.send_lead_now(at, text),
            OuterWork::Reports => {
                if !self.review_now(at) {
                    // Nothing ready after all: the next one goes.
                    self.drain_outer(at);
                }
            }
        }
    }

    /// The person's message reaches the outer loop: shown now, with STATUS.
    fn deliver_person(&mut self, at: SessionRef, text: String) {
        self.deliver_person_shown(at, text, false);
    }

    /// The same; `shown`: the message is in the conversation already.
    fn deliver_person_shown(&mut self, at: SessionRef, text: String, shown: bool) {
        let Some(id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        if !shown {
            self.store.push_message(at, Role::User, "", &text);
        }
        self.halted.remove(&id);
        self.resumable.remove(&id);
        let left = self.rounds_left.get(&id).copied().unwrap_or(0);
        self.rounds_left.insert(id.clone(), left.max(MAX_ROUNDS));
        self.rt.budget_said.remove(&id);
        self.rt.progress.remove(&id);
        self.rt.extended.remove(&id);
        // Moved here from elsewhere, its transcript left behind: what it said, first.
        // Kept until a turn of it went through (see LeadTurnDone).
        let text = match self.store.session(at).and_then(|s| s.carried.clone()) {
            Some(said) => format!("Earlier in this conversation (before it moved here):\n{said}\n\n---\n\n{text}"),
            None => text,
        };
        let message = match self.status_block(at).filter(|_| !self.store.is_plain(at)) {
            Some(status) => format!("{status}\n\n{text}"),
            None => text.clone(),
        };
        // The context pack goes first (memory searched, docs indexed: what
        // the outer loop would spend its first minutes on), built off the UI
        // thread; the outer loop counts as busy meanwhile.
        if !self.store.is_plain(at) {
            let project = self.store.projects[at.0].path.clone();
            let cwd = self.store.session(at).and_then(|s| s.work_dir.clone()).unwrap_or_else(|| project.clone());
            let first = !self.store.session(at).is_some_and(|s| s.messages.iter().any(|m| m.role() == Role::Lead));
            let app = crate::plugins::active(crate::plugins::OCTOSENSE_APP, &project);
            if crate::memory::mempal().is_some() || (first && app) {
                self.rt.packing.insert(id.clone());
                self.lead_status.insert(id.clone(), i18n::t("OctoBuddy gathers context…", "OctoBuddy 正在准备上下文…").into());
                let inbox = self.rt.inbox.clone();
                std::thread::spawn(move || {
                    if app {
                        let _ = crate::plugins::octosense_app::prepare(&cwd);
                    }
                    let pack = crate::pack::build(&project, &cwd, &text, first, app);
                    events::post(&inbox, LoopEvent::PackReady { session: id, message, pack });
                });
                return;
            }
        }
        self.send_lead_now(at, message);
    }

    /// The context pack is ready: kept for the inner loops, and sent ahead of
    /// the request.
    fn pack_ready(&mut self, session: &str, message: String, pack: Option<String>) {
        self.rt.packing.remove(session);
        let Some(at) = self.store.find_session(session) else { return };
        let message = match pack {
            Some(pack) => {
                self.write_pack(at, &pack);
                format!("{pack}\n\n---\n\n{message}")
            }
            None => message,
        };
        self.send_lead_now(at, message);
    }

    /// What waits for the outer loop, for the queue panel: (id, from whom, text).
    pub(crate) fn outer_waiting(&self, session: &str) -> Vec<(String, String, String)> {
        let ready: Vec<String> = self.rt.unreported.get(session).map(|v| v.iter()
            .filter(|(id, _, _)| !self.rt.checking.contains(id))
            .filter_map(|(id, _, _)| self.store.find_peer(id).and_then(|at| self.store.session(at))
                .and_then(|s| s.peers().iter().find(|p| &p.id == id).map(|p| p.slug.clone())))
            .collect()).unwrap_or_default();
        self.rt.outer_queue.get(session).map(|q| q.iter().map(|i| match &i.work {
            OuterWork::Person(t) | OuterWork::Steered(t) => (i.id.clone(), "you".to_string(), t.clone()),
            OuterWork::Note(t) => (i.id.clone(), "OctoBuddy".to_string(), t.lines().next().unwrap_or("").to_string()),
            OuterWork::Reports => (i.id.clone(), "reports".to_string(), if ready.is_empty() { "reports from the inner loops".into() } else { format!("from {}", ready.join(", ")) }),
        }).collect()).unwrap_or_default()
    }

    /// The person moves an item of the outer queue up or down.
    pub(crate) fn shift_outer(&mut self, cx: &mut Cx, item: &str, by: isize) {
        let Some(id) = self.selected.and_then(|at| self.store.session(at)).map(|s| s.id.clone()) else { return };
        if let Some(q) = self.rt.outer_queue.get_mut(&id) {
            if let Some(at) = q.iter().position(|i| i.id == item) {
                let to = at as isize + by;
                if to >= 0 && (to as usize) < q.len() {
                    q.swap(at, to as usize);
                }
            }
        }
        self.sync(cx);
    }

    /// The person drops an item of the outer queue. Dropped reports never
    /// reach the outer loop; its STATUS says whose they were.
    pub(crate) fn drop_outer(&mut self, cx: &mut Cx, item: &str) {
        let Some(at) = self.selected else { return };
        let Some(id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        let Some(q) = self.rt.outer_queue.get_mut(&id) else { return };
        let Some(pos) = q.iter().position(|i| i.id == item) else { return };
        let dropped = q.remove(pos);
        if let Some(OuterItem { work: OuterWork::Reports, .. }) = dropped {
            let checking = self.rt.checking.clone();
            let all = self.rt.unreported.remove(&id).unwrap_or_default();
            let (gone, kept): (Vec<_>, Vec<_>) = all.into_iter().partition(|(p, _, _)| !checking.contains(p));
            self.rt.unreported.insert(id.clone(), kept);
            let slugs: Vec<String> = gone.iter().filter_map(|(p, _, _)| self.store.peer_mut(p).map(|p| p.slug.clone())).collect();
            if !slugs.is_empty() {
                self.system(at, &i18n::pick(format!("You dropped the reports of {}: the outer loop will not review them.", slugs.join(", ")), format!("你删除了 {} 的汇报：outer 不会审查它们。", slugs.join(", "))));
                self.rt.notes.entry(id).or_default().push(format!("the person dropped the reports of {} before you saw them", slugs.join(", ")));
            }
        }
        self.save();
        self.sync(cx);
    }

    /// The person moves a message waiting for the shown inner loop.
    pub(crate) fn shift_peer_queue(&mut self, cx: &mut Cx, item: &str, by: isize) {
        let Some(peer) = self.selected.and_then(|at| self.shown_peer(at)).map(|p| p.id.clone()) else { return };
        if let Some(line) = self.rt.lines.get_mut(&peer) {
            line.shift(item, by);
        }
        self.sync(cx);
    }

    /// The person drops a message waiting for the shown inner loop; when it
    /// was the outer loop's, the outer loop hears of it in STATUS.
    pub(crate) fn drop_peer_queue(&mut self, cx: &mut Cx, item: &str) {
        let Some(at) = self.selected else { return };
        let Some((peer, slug)) = self.shown_peer(at).map(|p| (p.id.clone(), p.slug.clone())) else { return };
        let dropped = self.rt.lines.get_mut(&peer).and_then(|l| l.drop_waiting(item));
        if let Some(d) = dropped.filter(|d| d.from == From::Lead) {
            let session = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
            let text: String = d.text.chars().take(100).collect();
            self.rt.notes.entry(session).or_default().push(format!("the person removed your queued {} for {slug}: {text}", d.id));
        }
        self.sync(cx);
    }

    /// `octobuddy.status`: what runs now, for the system agent.
    fn tool_status(&self, project: Option<&str>) -> serde_json::Value {
        use serde_json::json;
        let wanted = project.map(|p| p.to_ascii_lowercase());
        let projects: Vec<serde_json::Value> = self.store.projects.iter()
            .filter(|p| wanted.as_ref().is_none_or(|w| p.name.to_ascii_lowercase().contains(w.as_str())))
            .map(|p| json!({
                "name": p.name,
                "path": p.path,
                "sessions": p.sessions.iter().map(|s| {
                    let outer = self.lead_status.get(&s.id).cloned().unwrap_or_else(|| "idle".into());
                    let waiting = self.rt.outer_queue.get(&s.id).map(|q| q.len()).unwrap_or(0);
                    json!({
                        "id": s.id,
                        "title": s.title,
                        "outer": {"cli": s.cli(), "status": outer, "cost_usd": s.lead_cost, "queued": waiting, "estimate": s.estimate},
                        "inner": s.peers().iter().map(|q| json!({
                            "slug": q.slug, "role": q.role(), "status": q.status,
                            "estimated_rounds": q.estimate, "steps_used": q.rounds_used,
                            "cost_usd": q.usage.as_ref().map(|u| u.cost),
                            "outer_verdict": q.review, "commits": q.commits,
                        })).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
            }))
            .collect();
        json!({"projects": projects})
    }

    /// `octobuddy.send`: the system agent writes to a session's outer loop,
    /// as the person would (the host has approved the call). The session is
    /// its id or words of its title; one must match.
    fn tool_send(&mut self, session: &str, message: &str) -> Result<serde_json::Value, String> {
        if message.is_empty() {
            return Err("the message is empty".into());
        }
        let needle = session.trim().to_ascii_lowercase();
        let mut found: Vec<SessionRef> = Vec::new();
        for (pi, p) in self.store.projects.iter().enumerate() {
            for (si, s) in p.sessions.iter().enumerate() {
                if s.id == session.trim() || (!needle.is_empty() && s.title.to_ascii_lowercase().contains(&needle)) {
                    found.push((pi, si));
                }
            }
        }
        let at = match found.as_slice() {
            [one] => *one,
            [] => return Err(format!("no OctoBuddy session matches \"{session}\"; octobuddy.status lists them")),
            many => return Err(format!("{} sessions match \"{session}\": name it by its id", many.len())),
        };
        let (id, title) = self.store.session(at).map(|s| (s.id.clone(), s.title.clone())).unwrap_or_default();
        self.work_dir(at)?;
        let text = format!("(from the OctoSense assistant) {message}");
        let queued = self.lead_busy(&id);
        if queued {
            self.queue_outer(&id, OuterWork::Person(text));
        } else {
            self.deliver_person(at, text);
        }
        self.save();
        Ok(serde_json::json!({"session": id, "title": title, "queued": queued}))
    }

    /// Writes to the lead now (it queues it itself if a turn runs).
    fn send_lead_now(&mut self, at: SessionRef, text: String) {
        if self.store.session(at).is_some_and(|s| s.is_detached()) {
            return;
        }
        // Its model changed with the conversation kept: said once, first.
        let note = self.store.session(at).and_then(|s| self.model_note.remove(&s.id));
        let text = match note {
            Some(note) => format!("{note}\n\n{text}"),
            None => text,
        };
        if let Some(id) = self.store.session(at).map(|s| s.id.clone()) {
            self.rt.lead_started.entry(id).or_insert_with(now_secs);
        }
        let project = self.store.projects[at.0].clone();
        let Some(session) = self.store.session(at).cloned() else { return };
        let cwd = session.work_dir.clone().unwrap_or_else(|| project.path.clone());
        let plain = project.is_chats();
        // An OctoSense app: the outer loop is told how one is built.
        let app_rules = |view: &mut Self| if !plain && crate::plugins::active(crate::plugins::OCTOSENSE_APP, &project.path) {
            crate::plugins::octosense_app::prepare(&cwd).unwrap_or_else(|err| {
                view.system(at, &i18n::pick(format!("This is an OctoSense app, but its tools are missing: {err}"), format!("这是 OctoSense 应用项目，但缺少工具：{err}")));
                String::new()
            })
        } else {
            String::new()
        };
        // The models a slice may name, known from its first plan on; the
        // plugins it can call here.
        let models = if plain { String::new() } else { self.models_line(at).map(|m| format!("\n\n{m}")).unwrap_or_default() };
        let models = match crate::plugins::roster(&project.path).filter(|_| !plain) {
            Some(roster) => format!("{models}\n\n{roster}"),
            None => models,
        };
        let sent = if session.engine() == crate::system_chat::ENGINE {
            self.system_chat_send(at, &text)
        } else if session.engine() == "octos" {
            let rules = match (session.lead_session.is_none(), plain) {
                (false, _) => String::new(),
                (true, true) => crate::lead::PLAIN_CHAT.to_string(),
                (true, false) => format!("{}{}{models}", crate::lead::lead_prompt(), app_rules(self)),
            };
            self.octos_outer_send(at, &cwd, &text, &rules)
        } else {
            if !self.rt.leads.contains_key(&session.id) {
                // It starts with the settings as they are now.
                self.rt.claude_settings.remove(&project.path);
                // Its project's memory, when mempal is installed.
                let (memory, mut mcp) = match crate::memory::mempal().filter(|_| !plain) {
                    Some(bin) => (crate::memory::outer_rules(&project.path), vec![("mempal".to_string(), crate::memory::mcp_config(&bin))]),
                    None => (String::new(), Vec::new()),
                };
                // OctoBuddy's own tools: a plan or a message checked at once.
                if let Some(server) = self.mcp.as_ref().filter(|_| !plain) {
                    mcp.push(("octobuddy".to_string(), server.config(&session.id)));
                }
                let extra = format!("{}{models}{memory}", app_rules(self));
                let mode = if plain { crate::lead::Mode::Plain } else { crate::lead::Mode::Outer };
                // Codex or pi: their own process, the same rules.
                let engine = session.engine().to_string();
                if engine == crate::rpc_lead::CODEX || engine == crate::rpc_lead::PI {
                    let rules = if plain { crate::lead::PLAIN_CHAT.to_string() } else { format!("{}{extra}", crate::lead::lead_prompt()) };
                    match self.spawn_rpc(&engine, &session.id, &cwd, session.lead_session.as_deref(), &rules, session.outer_model(), session.outer_effort(), &mcp, mode) {
                        Ok(lead) => { self.rt.leads.insert(session.id.clone(), lead); }
                        Err(err) => {
                            self.system(at, &i18n::pick(format!("The outer loop could not start: {err}"), format!("outer 无法启动：{err}")));
                            return;
                        }
                    }
                }
                // A provider of the person's (`family/model`): through OctoBuddy's proxy.
                let (env, model) = match session.outer_model().filter(|m| m.contains('/') && !self.rt.leads.contains_key(&session.id)) {
                    Some(label) => match providers::claude_route(label) {
                        Ok(route) => {
                            if self.rt.claude_proxy.is_none() {
                                self.rt.claude_proxy = crate::claude_proxy::Proxy::start();
                            }
                            let env = self.rt.claude_proxy.as_ref().map(|p| p.env_for(&route)).unwrap_or_default();
                            (env, Some(route.model.clone()))
                        }
                        Err(err) => {
                            self.system(at, &i18n::pick(format!("The outer loop could not start: {err}"), format!("outer 无法启动：{err}")));
                            return;
                        }
                    },
                    None => (Vec::new(), session.outer_model().map(String::from)),
                };
                // Claude Code (Codex and pi started above).
                if !self.rt.leads.contains_key(&session.id) {
                    match Lead::spawn(&self.rt.inbox, &session.id, &cwd, session.lead_session.as_deref(), &extra, model.as_deref(), session.outer_effort(), &mcp, mode, &env) {
                        Ok(lead) => { self.rt.leads.insert(session.id.clone(), lead); }
                        Err(err) => {
                            self.system(at, &i18n::pick(format!("The outer loop could not start: {err}"), format!("outer 无法启动：{err}")));
                            return;
                        }
                    }
                }
            }
            self.rt.leads[&session.id].send(&text)
        };
        match sent {
            Ok(()) => {
                self.rt.lead_inflight.entry(session.id.clone()).or_default().push_back(text.clone());
                let pending = self.rt.lead_pending.entry(session.id.clone()).or_insert(0);
                *pending += 1;
                let status = if *pending > 1 { format!("{} queued", *pending - 1) } else { "starting".into() };
                self.lead_status.insert(session.id, status);
            }
            Err(err) => {
                self.rt.leads.remove(&session.id);
                self.system(at, &i18n::pick(format!("Could not reach the outer loop: {err}"), format!("无法连接 outer：{err}")));
            }
        }
    }

    /// Where the session's loops work, set up at its first message: the
    /// project directory, or a worktree of it on the session's own branch.
    pub(crate) fn work_dir(&mut self, at: SessionRef) -> Result<String, String> {
        let project = self.store.projects[at.0].clone();
        let session = self.store.session(at).cloned().ok_or("no session")?;
        if let Some(dir) = session.work_dir.clone().filter(|d| Path::new(d).is_dir()) {
            return Ok(dir);
        }
        // A plain chat: a folder of its own, under OctoBuddy's data.
        if project.is_chats() {
            let dir = Path::new(&project.path).join(&session.id);
            std::fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
            let dir = dir.to_string_lossy().into_owned();
            if let Some(s) = self.store.session_mut(at) {
                s.work_dir = Some(dir.clone());
            }
            return Ok(dir);
        }
        let git = workspace::is_git_repo(&project.path);
        let (dir, branch) = if session.worktree() {
            if !git {
                return Err(format!("{} is not a git repository, so it cannot have a worktree. Turn off \"Use a git worktree\" to work in the folder itself.", project.path));
            }
            let short: String = session.id.chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect();
            let dir = model::data_dir().join("worktrees").join(&project.id).join(&short);
            let branch = format!("octobuddy/{short}");
            workspace::add_worktree(&project.path, &dir, &branch)?;
            (dir.to_string_lossy().into_owned(), Some(branch))
        } else {
            if git {
                workspace::exclude_agent_files(&project.path);
            }
            (project.path.clone(), None)
        };
        if let Some(s) = self.store.session_mut(at) {
            s.work_dir = Some(dir.clone());
            s.work_branch = branch.clone();
        }
        if let Some(branch) = branch {
            self.system(at, &i18n::pick(format!("This session works in its own worktree, on branch {branch}: {dir}"), format!("本会话在自己的 worktree 中工作，分支 {branch}：{dir}")));
        }
        Ok(dir)
    }

    /// The person sends the composer's text to the lead.
    pub(crate) fn send(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        let input = self.view.text_input(cx, ids!(composer));
        let text = input.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        if self.send_text(cx, at, text) {
            input.set_text(cx, "");
        }
    }

    /// The person's message to a session's outer loop (queued while it
    /// works). False when it could not be sent (said in the session).
    pub(crate) fn send_text(&mut self, cx: &mut Cx, at: SessionRef, text: String) -> bool {
        // Data the person added since: its description goes with the message.
        let fresh: Vec<String> = self.store.session(at).and_then(|s| s.data.as_ref())
            .map(|d| d.iter().filter(|n| !n.told).map(|n| format!("- {}", n.summary)).collect()).unwrap_or_default();
        let text = if fresh.is_empty() { text } else {
            if let Some(notes) = self.store.session_mut(at).and_then(|s| s.data.as_mut()) {
                notes.iter_mut().for_each(|n| n.told = true);
            }
            format!("{text}\n\nDATA the person gave this app (build it around this data; read the files named for the details):\n{}", fresh.join("\n"))
        };
        // How this project's slices went against their estimates: for the next one.
        let text = match crate::estimate::calibrate(self.store.projects[at.0].sessions.iter()).filter(|_| !self.store.is_plain(at)) {
            Some(c) => format!("{text}\n\n{}", c.line()),
            None => text,
        };
        let Some(session) = self.store.session(at) else { return false };
        let id = session.id.clone();
        let mut sent = false;
        if session.is_detached() {
            self.system(at, i18n::t("No outer loop here: message one of its peer agents (open its card).", "这里没有外环：请打开某个 peer agent 的卡片给它发消息。"));
            return false;
        }
        if let Err(err) = self.work_dir(at) {
            self.system(at, &i18n::pick(format!("Not sent: {err}"), format!("未发送：{err}")));
        } else {
            sent = true;
            if self.lead_busy(&id) {
                // It works: the message waits, in the queue above the input.
                self.halted.remove(&id);
                self.queue_outer(&id, OuterWork::Person(text));
            } else {
                self.deliver_person(at, text);
            }
        }
        self.save();
        self.sync(cx);
        sent
    }

    /// Whether the outer loop of `session` can be steered now: its turn under
    /// way (not getting ready, not stopping, not in its terminal), on an
    /// engine that takes steering (Claude Code, Codex, pi). Why not, if not.
    pub(crate) fn steerable(&self, session: &str) -> Result<(), String> {
        if self.rt.lead_pending.get(session).copied().unwrap_or(0) == 0 {
            return Err(i18n::t("it is not at work", "它没在运行").into());
        }
        if self.rt.packing.contains(session) || self.tui_holds_outer(session) || self.stopping.contains_key(session) {
            return Err(i18n::t("it is getting ready or stopping", "它正在准备或停止中").into());
        }
        if !self.rt.leads.contains_key(session) {
            return Err(i18n::t("this engine cannot be steered", "这个引擎不支持插话").into());
        }
        Ok(())
    }

    /// `text` steered into the running turn of the session at `at`, shown
    /// as such; why not, if it could not be.
    fn try_steer(&mut self, at: SessionRef, text: &str) -> Result<(), String> {
        let id = self.store.session(at).map(|s| s.id.clone()).ok_or("no session")?;
        self.steerable(&id)?;
        self.rt.leads[&id].steer(text)?;
        self.store.push_message(at, Role::User, "", text);
        if let Some(m) = self.store.session_mut(at).and_then(|s| s.messages.last_mut()) {
            m.meta = Some(STEER.into());
        }
        self.halted.remove(&id);
        self.resumable.remove(&id);
        Ok(())
    }

    /// ⌘↵ in the composer: the person's message into the outer loop's
    /// running turn (Cindy's 插话), not after it: the agent takes it up at
    /// its next step, its work not cut off. Not at work: sent the usual way;
    /// one that cannot be steered now: queued, and said why.
    pub(crate) fn steer(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        let Some(id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        let input = self.view.text_input(cx, ids!(composer));
        let text = input.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        match self.try_steer(at, &text) {
            Ok(()) => {
                input.set_text(cx, "");
                self.save();
                self.sync(cx);
            }
            Err(why) => {
                if self.lead_busy(&id) {
                    self.system(at, &i18n::pick(format!("Not steered ({why}): your message waits for its turn."), format!("没能插话（{why}）：消息排在这一轮之后。")));
                }
                self.send(cx);
            }
        }
    }

    /// A message of the person's waiting in the outer queue, steered into
    /// the running turn instead (its row's 插话).
    pub(crate) fn steer_queued(&mut self, cx: &mut Cx, item: &str) {
        let Some(at) = self.selected else { return };
        let Some(id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        let Some(pos) = self.rt.outer_queue.get(&id).and_then(|q| q.iter().position(|i| i.id == item)) else { return };
        let text = match &self.rt.outer_queue[&id][pos].work {
            OuterWork::Person(t) | OuterWork::Steered(t) => t.clone(),
            _ => return,
        };
        match self.try_steer(at, &text) {
            Ok(()) => {
                if let Some(q) = self.rt.outer_queue.get_mut(&id) {
                    q.remove(pos);
                }
            }
            Err(why) => self.system(at, &i18n::pick(format!("Not steered ({why}): it stays in the queue."), format!("没能插话（{why}）：它留在队列里。"))),
        }
        self.save();
        self.sync(cx);
    }

    /// The oldest steered message of `session` still marked `from`, marked `to`.
    fn mark_steer(&mut self, session: &str, from: &str, to: &str, text: Option<&str>) {
        let Some(at) = self.store.find_session(session) else { return };
        if let Some(m) = self.store.session_mut(at).and_then(|s| s.messages.iter_mut()
            .find(|m| m.role() == Role::User && m.meta.as_deref() == Some(from) && text.is_none_or(|t| m.text == t))) {
            m.meta = Some(to.into());
        }
    }

    /// The inner loops of a plan: all in the session's one directory (the
    /// project, or its worktree), where the outer loop works too.
    fn start_round(&mut self, at: SessionRef, mut slices: Vec<plan::Slice>, estimate: Option<plan::Estimate>, shared: Option<String>) {
        // An app's slices are checked the way an app is: by its gate and a headless run.
        if crate::plugins::active(crate::plugins::OCTOSENSE_APP, &self.store.projects[at.0].path) {
            for slice in slices.iter_mut().filter(|s| s.check.is_none()) {
                slice.check = Some(crate::plugins::octosense_app::CHECK.into());
            }
        }
        let dir = match self.work_dir(at) {
            Ok(dir) => dir,
            Err(err) => {
                self.system(at, &i18n::pick(format!("Not starting the inner loops: {err}"), format!("未启动 inner：{err}")));
                return;
            }
        };
        self.providers = providers::read();
        let octos = providers::peer_profile(&self.providers);
        // Each slice's model: one of the providers' others (the primary is the default).
        let models: Vec<String> = self.providers.rows.iter().filter(|r| r.role != "primary").map(|r| r.label.clone()).collect();
        let primary = self.providers.primary().map(|r| r.label.clone());
        let mut unknown = Vec::new();
        let pinned = self.store.session(at).is_some_and(|s| s.inner_model().is_some());
        let picks: Vec<Option<String>> = slices.iter().map(|slice| match &slice.model {
            None => None,
            // The session's own pick wins (the person's choice).
            Some(_) if pinned => None,
            Some(m) if Some(m) == primary.as_ref() => None,
            Some(m) if models.contains(m) => Some(m.clone()),
            Some(m) => {
                unknown.push(format!("{} ({m})", slice.slug));
                None
            }
        }).collect();
        let session = self.store.session_mut(at).unwrap();
        // What its inner loops run on, for their cards from the start (a
        // pinned Claude model shows as theirs while they wait).
        let on_lead = crate::rpc_lead::lead_engine(session.inner_engine());
        let pinned = if on_lead { session.inner_model().map(String::from) } else { None };
        let agent = if on_lead { session.inner_engine().to_string() } else { "octos".to_string() };
        let round = session.round() + 1;
        let branch = session.work_branch.clone();
        let taken: Vec<String> = session.peers().iter().map(|p| p.slug.clone()).collect();
        let mut started = Vec::new();
        for (slice, pick) in slices.iter().zip(&picks) {
            // Slugs name peers across rounds: keep them unique in the session.
            let mut slug = slice.slug.clone();
            let mut n = 2;
            while taken.contains(&slug) || started.iter().any(|(_, s, _): &(String, String, String)| *s == slug) {
                slug = format!("{}-{n}", slice.slug);
                n += 1;
            }
            let id = model::new_id("w");
            session.peers_mut().push(Peer {
                id: id.clone(), slug: slug.clone(), role: slice.role.clone(), agent: Some(agent.clone()),
                brief: slice.brief.clone(), status: "queued".into(), dir: dir.clone(), branch: None,
                round, started_at: now_secs(), finished_at: None, activity: Some("preparing".into()), result: None,
                session_key: Some(session_key(&id)), log: None, contract: None, usage: None, check: slice.check.clone(), verdict: None, review: None, landed: None, model: None, effort: None, touched: None, base: None, commits: None, subagents: None, estimate: slice.rounds, wave: slice.wave, rounds_used: None, budget: None, over_budget: None, flow: None, joined_from: None, queued: None, inflight: None, uncommitted: None, model_pick: pick.clone().or_else(|| pinned.clone()), accepted: None, review_wanted: slice.independent_review, reviews_for: None, by_person: None, claude_session: None, specs: None,
            });
            started.push((id, slug, slice.brief.clone()));
        }
        if !unknown.is_empty() {
            let list = unknown.join(", ");
            self.system(at, &i18n::pick(format!("Not one of the models: {list}; those slices run on the default model."), format!("不是可用的模型：{list}；这些切片用默认模型。")));
            let id = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
            self.rt.notes.entry(id).or_default().push(format!("these slices named a model that is not one of the MODELS and run on the default: {list}"));
        }
        let names: Vec<_> = started.iter().map(|(_, s, _)| s.as_str()).collect();
        let place = match &branch {
            Some(b) => i18n::pick(format!("in the session's worktree (branch {b})"), format!("在会话的 worktree 中（分支 {b}）")),
            None => i18n::t("in the project directory", "在项目目录中").into(),
        };
        let est = estimate.as_ref().map(|e| {
            let rounds = e.rounds.map(|r| format!("~{r} rounds")).unwrap_or_default();
            let waves = e.waves.map(|w| format!(", {w} wave(s)")).unwrap_or_default();
            let minutes = e.minutes.map(|m| format!(", ~{m:.0} min")).unwrap_or_default();
            format!("{rounds}{waves}{minutes}")
        }).filter(|e| !e.is_empty());
        if let Some(s) = self.store.session_mut(at) {
            s.estimate = est.clone();
        }
        // The review budget: one report per slice, plus the fix cycles the lead expects.
        let budget = (slices.iter().map(|s| s.reviews.unwrap_or(1).clamp(1, 4) + 1).sum::<u32>() + 2).min(40);
        let session_id = self.store.session(at).unwrap().id.clone();
        let left = self.rounds_left.get(&session_id).copied().unwrap_or(0);
        self.rounds_left.insert(session_id.clone(), left.max(budget));
        self.rt.budget_said.remove(&session_id);
        let est_text = est.map(|e| i18n::pick(format!(" Estimate (agent-estimation): {e}."), format!(" 估算（agent-estimation）：{e}。"))).unwrap_or_default();
        let (count, names, budget) = (started.len(), names.join(", "), left.max(budget));
        self.system(at, &i18n::pick(format!("Round {round}: {count} slice(s) ({names}), {place}, on {}.{est_text} Review budget: {budget} report(s).", octos.source),
            format!("第 {round} 轮：{count} 个切片（{names}），{place}，使用 {}。{est_text} 审查预算：{budget} 次汇报。", octos.source)));
        // Every brief kept as a file (the Spec tab lists them); those written
        // as agent-spec contracts are linted now and checked when done.
        let spec_dir = model::data_dir().join("specs").join(&session_id);
        let mut lints: HashMap<String, String> = HashMap::new();
        for (id, slug, brief) in &started {
            let contract = crate::contract::is_contract(brief);
            let kept = if contract { crate::contract::write(&spec_dir, slug, brief) } else { crate::contract::write_brief(&spec_dir, slug, brief) };
            match kept {
                Ok(path) => {
                    if contract {
                        lints.insert(slug.clone(), crate::contract::lint(&path).unwrap_or_else(|err| err));
                    }
                    if let Some(p) = self.store.peer_mut(id) {
                        let path = path.to_string_lossy().into_owned();
                        if contract {
                            p.contract = Some(path.clone());
                        }
                        p.specs.get_or_insert_with(Vec::new).push(model::SpecRef { path, at: now_secs(), round: Some(round as u32), contract });
                    }
                }
                Err(err) if contract => { lints.insert(slug.clone(), format!("could not keep it: {err}")); }
                Err(_) => {}
            }
        }
        // A task card per slice, in the outer conversation: what the inner loop was given.
        for ((_, slug, brief), slice) in started.iter().zip(&slices) {
            let mut meta = vec![format!("task · {}", slice.role.as_deref().unwrap_or("developer"))];
            if let Some(r) = slice.rounds { meta.push(format!("est. {r} rounds")); }
            if let Some(w) = slice.wave { meta.push(format!("wave {w}")); }
            let mut body = brief.clone();
            if let Some(cmd) = &slice.check {
                body.push_str(&format!("\n\nCheck: OctoBuddy runs `{cmd}` when it is done (on this machine, outside octos's sandbox)."));
            }
            if let Some(lint) = lints.get(slug) {
                body.push_str(&format!("\nContract lint: {lint}"));
            }
            self.store.push_meta(at, Role::ToPeer, slug, &body, &meta.join(" · "));
        }
        self.selected_peer = started.first().map(|(id, _, _)| id.clone());
        let first_wave = slices.iter().map(|s| s.wave.unwrap_or(1)).min().unwrap_or(1);
        for ((id, _, brief), slice) in started.into_iter().zip(&slices) {
            let mut line = Line::default();
            // What every slice shares, said once by the outer loop, with each.
            let brief = match &shared {
                Some(shared) => format!("{brief}\n\n## Shared by every slice of this plan\n\n{shared}"),
                None => brief,
            };
            line.queue.push_back(Delivery::new(From::Lead, brief));
            self.rt.lines.insert(id.clone(), line);
            let wave = slice.wave.unwrap_or(1);
            if wave > first_wave {
                // A later wave waits for the earlier ones to be accepted.
                self.rt.held.insert(id.clone(), dir.clone());
                if let Some(p) = self.store.peer_mut(&id) {
                    p.activity = Some(i18n::pick(format!("waits for wave {} to be accepted", wave - 1), format!("等待 wave {} 被接受", wave - 1)));
                }
            } else {
                events::post(&self.rt.inbox, LoopEvent::PeerReady { session: session_id.clone(), peer: id, dir: dir.clone(), branch: None });
            }
        }
        // Not over the app's preview the person is watching, nor over the
        // flow canvas (it shows the inner loops itself).
        self.show_inner |= !self.show_preview && self.stage == crate::Stage::Chat;
        self.write_context(at);
    }

    pub(crate) fn serve(&mut self) -> Result<&Serve, String> {
        if self.rt.serve.is_none() {
            let dir = model::data_dir().join("octos").join("serve");
            let profile = providers::peer_profile(&self.providers);
            providers::serve_data_dir(&dir, &profile).map_err(|err| format!("could not prepare octos: {err}"))?;
            // The profiles a loop on octos may run under, one per model.
            if let Err(err) = providers::model_profiles(&dir, &profile) {
                log!("octobuddy: outer loop profiles: {err}");
            }
            self.rt.serve = Some(Serve::spawn(&self.rt.inbox, &dir)?);
            self.rt.opened.clear();
        }
        Ok(self.rt.serve.as_ref().unwrap())
    }

    /// Hands a message to a peer, as queue or interrupt (see `dispatch`).
    pub(crate) fn deliver(&mut self, peer: &str, d: Delivery, mode: Mode) {
        if let Some(p) = self.store.peer_mut(peer).filter(|p| matches!(p.status.as_str(), "discarded" | "closed")).map(|p| p.slug.clone()) {
            if let Some(at) = self.store.find_peer(peer) {
                self.system(at, &i18n::pick(format!("{p} is closed: it takes no more work. Start a new slice instead."), format!("{p} 已关闭，不再接收任务，请新建切片。")));
            }
            return;
        }
        // Its own CLI holds it (the native TUI plugin): it waits in line.
        if self.tui_holds_peer(peer) {
            let line = self.rt.lines.entry(peer.to_string()).or_default();
            match mode {
                Mode::Interrupt | Mode::Steer => line.queue.push_front(d),
                Mode::Queue => line.queue.push_back(d),
            }
            return;
        }
        // A peer from before a restart has its clone already.
        let ready = self.store.peer_mut(peer).map(|p| p.status != "queued").unwrap_or(false);
        let line = self.rt.lines.entry(peer.to_string()).or_default();
        if ready {
            line.ready = true;
        }
        match line.deliver(d, mode) {
            Step::Start(d) => self.start_peer_turn(peer, d),
            Step::Interrupt(turn) => {
                let key = session_key(peer);
                let claude = self.on_claude(peer);
                let result = if claude { self.claude_inner_interrupt(peer) } else { self.serve().and_then(|s| s.interrupt(&key, &turn)) };
                if let Err(err) = result {
                    self.peer_failed(peer, &err);
                }
            }
            // Claude Code takes no message in the middle of a turn: it waits, first.
            Step::Steer(d) if self.on_claude(peer) => {
                if let Some(line) = self.rt.lines.get_mut(peer) {
                    line.queue.push_front(d);
                }
            }
            Step::Steer(d) => {
                let key = session_key(peer);
                let prompt = later_prompt(d.from, &d.text, self.rt.pending.contains(peer));
                let result = self.serve().and_then(|s| s.steer(peer, &key, &prompt));
                match result {
                    Ok(()) => if let Some(open) = self.store.peer_mut(peer).and_then(|p| p.open_exchange()) {
                        open.input.push_str(&format!("\n\n[added while it worked] {}", d.text));
                    },
                    Err(err) => self.peer_failed(peer, &err),
                }
            }
            Step::Wait => {}
        }
    }

    /// Starts what waited for `peer` (its CLI let it go).
    pub(crate) fn start_peer_turn_now(&mut self, peer: &str, d: Delivery) {
        self.start_peer_turn(peer, d);
    }

    fn start_peer_turn(&mut self, peer: &str, d: Delivery) {
        let Some(p) = self.store.peer_mut(peer).cloned() else { return };
        // Past its budget: the work waits, first in line, until it is raised.
        if p.over_budget.is_some() {
            if let Some(line) = self.rt.lines.get_mut(peer) {
                line.refused(d);
            }
            return;
        }
        // No free slot: it waits, first in line, for one.
        let running = self.rt.lines.iter().filter(|(id, l)| l.is_busy() && id.as_str() != peer).count();
        if running >= max_inners() {
            if let Some(line) = self.rt.lines.get_mut(peer) {
                line.refused(d);
            }
            if !self.rt.slot_wait.iter().any(|w| w == peer) {
                self.rt.slot_wait.push_back(peer.to_string());
            }
            if let Some(p) = self.store.peer_mut(peer) {
                p.activity = Some(i18n::pick(format!("waiting for a free slot ({running} running)"), format!("等待空位（已有 {running} 个在运行）")));
            }
            return;
        }
        self.rt.slot_wait.retain(|w| w != peer);
        let key = p.session_key.clone().unwrap_or_else(|| session_key(peer));
        let first = p.log().is_empty();
        let session = self.store.find_peer(peer).and_then(|at| self.store.session(at));
        let work_branch = session.and_then(|s| s.work_branch.clone());
        let context = session.map(|s| crate::context::relative(&s.id)).unwrap_or_default();
        let text = match (first, p.reviews_for.is_some()) {
            // A reviewer's brief is its whole prompt.
            (true, true) => d.text.clone(),
            (true, false) => {
                let mut text = first_prompt(&d.text, &p.dir, work_branch.as_deref(), p.check.as_deref(), &context);
                // Made by the person: its first task is theirs.
                if p.by_person == Some(true) {
                    text = text.replacen("Task from the lead:", "Task from the person:", 1);
                    // Made for an outer loop: this task is reported to it.
                    if self.rt.person_tasks.contains(peer) {
                        text.push_str("\n\nThis first task is reported to the lead: end it with your octobuddy-report block. \
What the person says to you after it stays between you and them.");
                    }
                }
                // An app's inner loop does not run the app: OctoBuddy does (for
                // one on Claude Code, through its tools, as it works).
                if crate::plugins::active(crate::plugins::OCTOSENSE_APP, &p.dir) {
                    text.push_str(if self.on_claude(peer) { crate::plugins::octosense_app::INNER_NOTE_TOOLS } else { crate::plugins::octosense_app::INNER_NOTE });
                }
                text
            }
            (false, _) => later_prompt(d.from, &d.text, self.rt.pending.contains(peer)),
        };
        let opened = self.rt.opened.contains(peer);
        // Its slice's model: the profile octos keeps for it.
        let profile = p.model_pick.as_deref().map(|m| serde_json::json!({"profile_id": providers::model_profile_id(m)})).unwrap_or_else(|| serde_json::json!({}));
        let result = if self.on_claude(peer) {
            self.claude_inner_start(peer, &text)
        } else {
            self.serve().and_then(|serve| if opened {
                serve.start_turn(peer, &key, &text).map(Some)
            } else {
                serve.open_with(peer, &key, &p.dir, Some(&text), profile, p.reviews_for.is_some())
            })
        };
        match result {
            Ok(turn) => {
                self.rt.opened.insert(peer.to_string());
                if let Some(line) = self.rt.lines.get_mut(peer) {
                    line.started(turn.unwrap_or_default(), d.from);
                }
                self.rt.inflight.insert(peer.to_string(), d.clone());
                if d.from == From::Lead {
                    self.rt.pending.insert(peer.to_string());
                }
                if let Some(p) = self.store.peer_mut(peer) {
                    p.status = "running".into();
                    p.finished_at = None;
                    p.activity = Some(i18n::t("starting", "启动中").into());
                    p.log_mut().push(Exchange {
                        from: if d.from == From::Person { "person".into() } else { "lead".into() },
                        input: d.text, reply: None, outcome: None, at: now_secs(), steps: None, took: None, cost_total: None, began: None,
                    });
                }
            }
            Err(err) => self.peer_failed(peer, &err),
        }
    }

    /// A peer with a task from the lead that stopped without reporting on it
    /// (stopped by the person, say, with nothing sent after) and has stayed
    /// idle a minute: the lead gets its last reply, marked as such, so it
    /// never waits on a report that will not come.
    /// Reports held for the others' (`maybe_review`) whose wait is over.
    pub(crate) fn flush_batches(&mut self) {
        let now = now_secs();
        let due: Vec<String> = self.rt.batch_since.iter().filter(|(_, since)| now.saturating_sub(**since) >= BATCH_SECS).map(|(id, _)| id.clone()).collect();
        for id in due {
            match self.store.find_session(&id) {
                Some(at) if self.rt.unreported.get(&id).is_some_and(|v| !v.is_empty()) => self.maybe_review(at),
                _ => { self.rt.batch_since.remove(&id); }
            }
        }
    }

    pub(crate) fn check_stalls(&mut self) {
        const STALL_SECS: u64 = 60;
        let now = now_secs();
        let stalled: Vec<String> = self.rt.pending.iter().filter(|peer| {
            let Some(p) = self.store.find_peer(peer).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| &p.id == *peer)) else { return false };
            let line_idle = self.rt.lines.get(*peer).is_none_or(|l| !l.is_busy() && l.queue.is_empty());
            !p.is_active() && line_idle && !self.rt.checking.contains(*peer) && !self.rt.held.contains_key(*peer)
                && p.finished_at.is_some_and(|f| now.saturating_sub(f) >= STALL_SECS)
        }).cloned().collect();
        for peer in stalled {
            self.rt.pending.remove(&peer);
            let Some(at) = self.store.find_peer(&peer) else { continue };
            let Some(session_id) = self.store.session(at).map(|s| s.id.clone()) else { continue };
            let Some(p) = self.store.peer_mut(&peer).cloned() else { continue };
            let last = p.log().last().and_then(|e| e.reply.clone()).unwrap_or_default();
            let tail: String = last.chars().rev().take(8000).collect::<Vec<_>>().into_iter().rev().collect();
            self.store.push_meta(at, Role::Peer, &p.slug, &i18n::pick(format!("(it stopped without reporting; its last reply)\n{}", short(&tail, 4000)),
                format!("（它停下了，没有汇报；这是它最后的回复）\n{}", short(&tail, 4000))), p.role());
            self.system(at, &i18n::pick(format!("{} stopped without reporting on the outer loop's task; its last reply goes to the outer loop.", p.slug),
                format!("{} 停下了，没有就 outer 的任务汇报；把它最后的回复交给 outer。", p.slug)));
            self.queue_report(at, &session_id, &peer, &p, format!("(OctoBuddy: {} stopped without reporting on your task, and nothing more was sent to it. This is its last reply.)\n{tail}", p.slug), true, "interrupted", false, None);
            self.maybe_review(at);
        }
    }

    fn peer_failed(&mut self, peer: &str, error: &str) {
        if let Some(line) = self.rt.lines.get_mut(peer) {
            line.clear();
            line.turn = None;
        }
        self.rt.slot_wait.retain(|w| w != peer);
        self.release_slots();
        let Some(at) = self.store.find_peer(peer) else { return };
        let session_id = self.store.session(at).unwrap().id.clone();
        if let Some(p) = self.store.peer_mut(peer) {
            p.status = "failed".into();
            p.finished_at = Some(now_secs());
            p.activity = None;
            p.result = Some(error.to_string());
            if let Some(open) = p.open_exchange() {
                open.outcome = Some("failed".into());
                open.reply = Some(error.to_string());
            }
        }
        self.rt.pending.remove(peer);
        self.rt.unreported.entry(session_id).or_default().push((peer.to_string(), format!("Failed: {error}"), true));
        self.maybe_review(at);
    }

    /// Hands the lead the results that are ready (their checks done), as
    /// soon as it is free: each report reaches it when its peer finishes,
    /// not when the whole round does (within the request's budget).
    fn maybe_review(&mut self, at: SessionRef) {
        let Some(id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        // No outer loop here: its peers' reports stay with them and the person.
        if self.halted.contains(&id) || self.store.session(at).is_some_and(|s| s.is_detached()) {
            self.rt.unreported.remove(&id);
            return;
        }
        let checking = self.rt.checking.clone();
        let ready = self.rt.unreported.get(&id).is_some_and(|v| v.iter().any(|(p, _, _)| !checking.contains(p)));
        if !ready {
            return;
        }
        // Others still on the outer loop's tasks: their reports go with these,
        // in one review, if they come within `BATCH_SECS`.
        let others = self.store.session(at).map(|s| s.peers().iter().filter(|p| {
            p.is_active() && !self.rt.held.contains_key(&p.id) && self.rt.pending.contains(&p.id)
                && !self.rt.unreported.get(&id).is_some_and(|v| v.iter().any(|(r, _, _)| r == &p.id))
        }).count()).unwrap_or(0);
        let since = *self.rt.batch_since.entry(id.clone()).or_insert_with(now_secs);
        if others > 0 && now_secs().saturating_sub(since) < BATCH_SECS {
            return;
        }
        let waiting = self.rt.outer_queue.get(&id).is_some_and(|q| !q.is_empty());
        if self.lead_busy(&id) || waiting {
            // Its turn comes in the outer queue (one slot for all ready reports).
            if !self.rt.outer_queue.get(&id).is_some_and(|q| q.iter().any(|i| i.work == OuterWork::Reports)) {
                self.queue_outer(&id, OuterWork::Reports);
            }
            return;
        }
        self.review_now(at);
    }

    /// Sends the ready reports to the lead now; says whether it sent any.
    fn review_now(&mut self, at: SessionRef) -> bool {
        let Some(session) = self.store.session(at).cloned() else { return false };
        let checking = self.rt.checking.clone();
        let Some(all) = self.rt.unreported.get(&session.id).filter(|v| !v.is_empty()).cloned() else { return false };
        let (reports, later): (Vec<_>, Vec<_>) = all.into_iter().partition(|(id, _, _)| !checking.contains(id));
        if reports.is_empty() {
            return false;
        }
        let mut left = self.rounds_left.get(&session.id).copied().unwrap_or(0);
        // Used up while the work still moves on: more, without the person.
        let times = self.rt.extended.get(&session.id).copied().unwrap_or(0);
        if left == 0 && times < MAX_EXTENSIONS && !self.halted.contains(&session.id) && self.rt.progress.remove(&session.id) {
            left = EXTENSION;
            self.rt.extended.insert(session.id.clone(), times + 1);
            self.system(at, &i18n::pick(
                format!("The review budget was used up while checks kept passing: {EXTENSION} more review(s), by itself ({} of {MAX_EXTENSIONS}).", times + 1),
                format!("审查预算用完了，但检查还在通过、工作还在推进：自动再加 {EXTENSION} 次（第 {}/{MAX_EXTENSIONS} 次）。", times + 1)));
        }
        if left == 0 {
            if self.rt.budget_said.insert(session.id.clone()) {
                self.system(at, i18n::t("Inner results are waiting: this request's review budget is used up. Send a message to continue.", "有 inner 结果在等待：本次请求的审查预算已用完。发一条消息即可继续。"));
            }
            return false;
        }
        self.rounds_left.insert(session.id.clone(), left - 1);
        self.rt.unreported.insert(session.id.clone(), later);
        self.rt.batch_since.remove(&session.id);
        let mut prompt = String::from("INNER RESULTS\n");
        for (id, report, forwarded) in &reports {
            let Some(p) = session.peers().iter().find(|p| &p.id == id) else { continue };
            let how = if *forwarded { " — it did not report; OctoBuddy forwarded the end of its reply" } else { "" };
            prompt.push_str(&format!("\n## {} ({}){how}\n", p.slug, p.role()));
            if self.rt.joined.remove(id) {
                let from = p.joined_from.as_deref().unwrap_or("another outer loop");
                prompt.push_str(&format!("(The person moved this inner loop to you from the outer loop \"{from}\": this is how it describes itself. \
It works for you now: message it, review its work, or close it.)\n"));
            }
            if let Some(files) = p.touched.as_ref().filter(|f| !f.is_empty()) {
                prompt.push_str(&format!("files it changed: {}\n", files.join(", ")));
            }
            prompt.push_str(report);
            prompt.push('\n');
        }
        if let Some(status) = self.status_block(at) {
            prompt.push_str(&format!("\n{status}\n"));
        }
        let running = session.peers().iter().filter(|p| p.is_active() && !self.rt.held.contains_key(&p.id)).count();
        let waiting = session.peers().iter().filter(|p| self.rt.held.contains_key(&p.id)).count();
        let still = match (running, waiting) {
            (0, 0) => String::new(),
            (r, 0) => i18n::pick(format!(" ({r} inner loop(s) still working)"), format!("（{r} 个 inner 仍在工作）")),
            (0, w) => i18n::pick(format!(" ({w} waiting for their wave)"), format!("（{w} 个在等待自己的 wave）")),
            (r, w) => i18n::pick(format!(" ({r} still working, {w} waiting for their wave)"), format!("（{r} 个仍在工作，{w} 个在等待 wave）")),
        };
        let (sent, left) = (reports.len(), left - 1);
        self.system(at, &i18n::pick(format!("Sent {sent} report(s) to the outer loop for review{still}; {left} review(s) left in the budget."),
            format!("已把 {sent} 份汇报发给 outer 审查{still}；审查预算还剩 {left} 次。")));
        self.send_lead_now(at, prompt);
        true
    }

    pub(crate) fn stop(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        let Some(session) = self.store.session(at).cloned() else { return };
        self.halted.insert(session.id.clone());
        self.rounds_left.insert(session.id.clone(), 0);
        // What waited for it is kept, paused: `继续` sends it on.
        self.resumable.insert(session.id.clone());
        if let Some(lead) = self.rt.leads.get(&session.id) {
            let _ = lead.interrupt();
            // Its turn ends when it has stopped; one that does not is ended.
            if self.rt.lead_pending.get(&session.id).copied().unwrap_or(0) > 0 {
                self.stopping.insert(session.id.clone(), std::time::Instant::now());
                self.lead_status.insert(session.id.clone(), i18n::t("stopping…", "正在停止…").into());
            }
        }
        self.octos_outer_stop(&session.id);
        self.system_chat_stop(&session.id);
        self.rt.lead_inflight.remove(&session.id);
        for p in session.peers() {
            let turn = self.rt.lines.get_mut(&p.id).and_then(Line::clear);
            if let Some(lead) = self.rt.claude_inners.get(&p.id) {
                let _ = lead.interrupt();
            } else if let (Some(turn), Some(serve)) = (turn, self.rt.serve.as_ref()) {
                let _ = serve.interrupt(&session_key(&p.id), &turn);
            }
        }
        self.system(at, i18n::t("Stopped by the person.", "已由你停止。"));
        self.save();
        self.sync(cx);
    }

    /// Outer loops asked to stop that have not within `STOP_GRACE`: ended
    /// (their process and what it started), as Cindy does. The conversation
    /// is kept; the next message resumes it.
    pub(crate) fn stop_overdue(&mut self, cx: &mut Cx) {
        let late: Vec<String> = self.stopping.iter().filter(|(_, at)| at.elapsed() >= STOP_GRACE).map(|(s, _)| s.clone()).collect();
        for session in late {
            self.stopping.remove(&session);
            self.cut_outer(&session);
            if let Some(at) = self.store.find_session(&session) {
                self.system(at, i18n::t("It did not stop in time: its process was ended. Your next message goes on with the same conversation.",
                    "它没能及时停下：已结束它的进程。下一条消息会接着原来的对话。"));
            }
        }
        self.sync(cx);
    }

    /// `继续` after a stop: what waited for the outer loop goes on; with
    /// nothing waiting, it is asked to pick up where it was cut off (the
    /// same conversation; steps it already took are not taken again).
    pub(crate) fn go_on(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        let Some(id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        if !self.resumable.remove(&id) || self.lead_busy(&id) {
            return;
        }
        self.halted.remove(&id);
        let left = self.rounds_left.get(&id).copied().unwrap_or(0);
        self.rounds_left.insert(id.clone(), left.max(MAX_ROUNDS));
        if self.rt.outer_queue.get(&id).is_some_and(|q| !q.is_empty()) {
            self.drain_outer(at);
        } else {
            self.store.push_message(at, Role::User, "", i18n::t("Go on", "继续"));
            self.send_lead_now(at, CONTINUE.to_string());
        }
        self.save();
        self.sync(cx);
    }

    /// The person writes to the peer shown in the panel.
    pub(crate) fn send_to_peer(&mut self, cx: &mut Cx, mode: Mode) {
        let Some(at) = self.selected else { return };
        let Some(peer) = self.shown_peer(at).map(|p| p.id.clone()) else { return };
        let input = self.view.text_input(cx, ids!(peer_input));
        let text = input.text().trim().to_string();
        if text.is_empty() {
            return;
        }
        input.set_text(cx, "");
        self.send_text_to_peer(cx, &peer, text, mode);
    }

    /// The person's message to a peer (its answer, when it asked a question).
    pub(crate) fn send_text_to_peer(&mut self, cx: &mut Cx, peer: &str, text: String, mode: Mode) {
        let peer = peer.to_string();
        if let Some(id) = self.store.find_peer(&peer).and_then(|at| self.store.session(at)).map(|s| s.id.clone()) {
            self.halted.remove(&id);
        }
        // A peer waiting on a question gets the text as its answer.
        if let Some((question_id, _, options)) = self.rt.questions.remove(&peer) {
            if let Some(serve) = self.rt.serve.as_ref() {
                let _ = serve.answer_question(&session_key(&peer), &question_id, &options, &text);
            }
            if let Some(p) = self.store.peer_mut(&peer) {
                p.activity = Some(i18n::t("answered", "已回答").into());
                if let Some(open) = p.open_exchange() {
                    open.input.push_str(&format!("\n\n[you answered its question] {text}"));
                }
            }
            self.save();
            self.sync(cx);
            return;
        }
        self.deliver(&peer, Delivery::new(From::Person, text), mode);
        self.save();
        self.sync(cx);
    }

    /// `scope` `session`: octos approves what this peer's session asks next, too.
    pub(crate) fn answer_approval(&mut self, cx: &mut Cx, approve: bool, scope: Option<&str>) {
        let Some(at) = self.selected else { return };
        let Some(peer) = self.shown_peer(at).map(|p| p.id.clone()) else { return };
        let Some((approval_id, _, _)) = self.rt.approvals.remove(&peer) else { return };
        if let Some(serve) = self.rt.serve.as_ref() {
            let _ = serve.answer_approval(&session_key(&peer), &approval_id, approve, scope);
        }
        self.sync(cx);
    }

    /// The person switches the session's inner loops between Ask (octos
    /// asks before what needs approval) and Auto (they run in their sandbox
    /// without asking; network stays off). Waiting approvals are approved.
    pub(crate) fn set_auto(&mut self, cx: &mut Cx, on: bool) {
        let Some(at) = self.selected else { return };
        let Some(session) = self.store.session_mut(at) else { return };
        if session.auto() == on {
            return;
        }
        session.auto = Some(on);
        let peers: Vec<String> = session.peers().iter().filter(|p| p.status != "closed").map(|p| p.id.clone()).collect();
        for peer in &peers {
            if on {
                if let Some((approval_id, _, _)) = self.rt.approvals.remove(peer) {
                    let _ = self.serve().and_then(|s| s.answer_approval(&session_key(peer), &approval_id, true, Some("session")));
                }
            }
        }
        self.system(at, if on {
            i18n::t("Approvals: Auto. OctoBuddy approves what the inner loops ask (their sandbox stays on); each approval is listed in its conversation. What waited is approved.",
                "审批：自动。inner 请求的审批由 OctoBuddy 代为批准（沙箱照常生效），每次批准都会记在它的对话里；正在等待的也已批准。")
        } else {
            i18n::t("Approvals: Ask. Inner loops ask before what octos wants approved.", "审批：询问。需要审批的操作会先询问你。")
        });
        self.save();
        self.sync(cx);
    }

    pub(crate) fn apply_event(&mut self, event: LoopEvent) {
        // `OCTOBUDDY_DEBUG_EVENTS=1`: each event, short, on stderr.
        if std::env::var_os("OCTOBUDDY_DEBUG_EVENTS").is_some() {
            let line: String = format!("{event:?}").chars().take(160).collect();
            eprintln!("octobuddy event: {line}");
        }
        // An outer loop on octos speaks as a peer of the server: said again as the outer loop's.
        let Some(event) = self.octos_outer_event(event) else { return };
        let Some(event) = self.claude_inner_event(event) else { return };
        // The outer loop's first sign of work on the message it was sent:
        // its turn begins here (the wait to get there is not its work).
        if let LoopEvent::LeadStatus { session, .. } | LoopEvent::LeadDelta { session, .. } | LoopEvent::LeadTool { session, .. } = &event {
            if self.rt.lead_started.contains_key(session) && !self.rt.lead_began.contains_key(session) {
                self.rt.lead_began.insert(session.clone(), now_secs());
            }
        }
        match event {
            LoopEvent::Published { session, result } => {
                self.publishing = None;
                let Some(at) = self.store.find_session(&session) else { return };
                match result {
                    Ok(p) => {
                        let bumped = p.bumped.map(|c| i18n::pick(format!(" (version raised, commit {c})"), format!("（版本已提升，提交 {c}）"))).unwrap_or_default();
                        let how = if crate::plugins::app_publish::shell_reads_it() {
                            i18n::t("Open App Hub in the dock, choose it, Get, then Install (below its permissions) and Open.",
                                "在 Dock 里打开 App Hub，选中它，点 Get，再点 Install（在权限说明下方），然后 Open。").to_string()
                        } else {
                            let anchor = crate::plugins::app_publish::anchor().unwrap_or_default();
                            let dir = crate::plugins::app_publish::hub_dir().display().to_string();
                            i18n::pick(format!("This OctoSense does not read the local App Hub yet: start it with OCTOSENSE_HUB={dir} OCTOSENSE_HUB_ANCHOR={anchor}, then App Hub lists it (Get, Install, Open)."),
                                format!("这个 OctoSense 还没读取本地 App Hub：用 OCTOSENSE_HUB={dir} OCTOSENSE_HUB_ANCHOR={anchor} 启动它，App Hub 里就会列出（Get、Install、Open）。"))
                        };
                        self.system(at, &i18n::pick(format!("Published {} {} ({}) to the local App Hub{bumped}. {how}", p.name, p.version, p.id),
                            format!("已把 {} {}（{}）发布到本地 App Hub{bumped}。{how}", p.name, p.version, p.id)));
                    }
                    Err(err) => {
                        let tail: String = err.lines().rev().take(14).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
                        self.system(at, &i18n::pick(format!("Could not publish it:\n{tail}"), format!("发布失败：\n{tail}")));
                    }
                }
            }
            LoopEvent::DataFound { session, found, commit } => {
                let Some(at) = self.store.find_session(&session) else { return };
                match found {
                    Ok(found) => {
                        let made = commit.map(|c| i18n::pick(format!(" (committed {c})"), format!("（已提交 {c}）"))).unwrap_or_default();
                        self.system(at, &i18n::pick(format!("Data added: {}{made}. It goes to the outer loop with your next message.", found.summary),
                            format!("已添加数据：{}{made}。它会随你的下一条消息交给 outer。", found.summary)));
                        let notes = self.store.session_mut(at).unwrap().data.get_or_insert_with(Vec::new);
                        notes.retain(|n| n.name != found.name);
                        notes.push(model::DataNote { name: found.name, summary: found.summary, told: false });
                        self.data_added = Some((session, Ok(())));
                    }
                    Err(err) => {
                        self.system(at, &i18n::pick(format!("Could not add that data: {err}"), format!("无法添加这份数据：{err}")));
                        self.data_added = Some((session, Err(err)));
                    }
                }
            }
            LoopEvent::McpCall { session, name, reply, .. } if session.starts_with("inner:") => {
                let peer = session.trim_start_matches("inner:").to_string();
                self.inner_mcp_call(&peer, &name, reply);
            }
            // A plugin's program may take its time: off the UI thread.
            LoopEvent::McpCall { session, name, args, reply } if name == "octobuddy_plugin" => {
                self.plugin_mcp_call(&session, &args, reply);
            }
            LoopEvent::McpCall { session, name, args, reply } => {
                let answer = self.mcp_call(&session, &name, &args);
                let _ = reply.send(answer);
            }
            LoopEvent::ToolsProbed(tools) => self.tools_probed(tools),
            LoopEvent::AgentInstalled { name, result, awaited } => self.agent_installed(&name, result, awaited),
            LoopEvent::CardLoopProbed { project, result } => self.card_loop_probed(&project, result),
            LoopEvent::PackReady { session, message, pack } => self.pack_ready(&session, message, pack),
            LoopEvent::PluginSaid { session, plugin, result } => self.plugin_said(&session, &plugin, result),
            LoopEvent::ServeRetry => {
                let next: Vec<(String, Delivery)> = self.rt.lines.iter_mut()
                    .filter(|(_, l)| !l.is_busy())
                    .filter_map(|(peer, l)| l.queue.pop_front().map(|d| (peer.clone(), d)))
                    .collect();
                for (peer, d) in next {
                    self.deliver(&peer, d, Mode::Queue);
                }
                self.octos_outers_restart();
            }
            LoopEvent::LeadStatus { session, status } => {
                let pending = self.rt.lead_pending.get(&session).copied().unwrap_or(0);
                let status = if pending > 1 { format!("{status} · {} queued", pending - 1) } else { status };
                self.lead_status.insert(session, status);
            }
            LoopEvent::LeadDelta { session, text } => {
                let Some(at) = self.store.find_session(&session) else { return };
                let (index, raw) = self.live_lead(at, &session);
                let raw = format!("{raw}{text}");
                if let Some(m) = self.store.session_mut(at).and_then(|s| s.messages.get_mut(index)) {
                    m.text = stream::visible(&raw);
                }
                self.rt.lead_live.insert(session, (index, raw));
            }
            LoopEvent::LeadTool { session, id, name, detail } => {
                let Some(at) = self.store.find_session(&session) else { return };
                let (index, _) = self.live_lead(at, &session);
                if let Some(m) = self.store.session_mut(at).and_then(|s| s.messages.get_mut(index)) {
                    upsert_step(m.steps.get_or_insert_with(Vec::new), &id, &name, &detail);
                }
            }
            LoopEvent::LeadInfo { session, model } => {
                if let Some(s) = self.store.find_session(&session).and_then(|at| self.store.session_mut(at)) {
                    s.lead_model = Some(model);
                }
            }
            LoopEvent::LeadSubTool { session, parent, name, detail } => {
                let Some(s) = self.store.find_session(&session).and_then(|at| self.store.session_mut(at)) else { return };
                let Some(step) = s.messages.iter_mut().rev().take(6)
                    .filter_map(|m| m.steps.as_mut())
                    .find_map(|steps| steps.iter_mut().find(|x| x.id == parent)) else { return };
                let (base, calls) = self.rt.subagents.entry(parent).or_insert_with(|| (step.detail.clone(), 0));
                *calls += 1;
                let what = detail.rsplit('/').next().unwrap_or("").chars().take(40).collect::<String>();
                step.detail = format!("{base} · {calls} tool call(s), last {name} {what}").trim_end().to_string();
            }
            LoopEvent::LeadToolEnd { session, id, ok } => {
                let Some(at) = self.store.find_session(&session) else { return };
                // The step is in the latest lead messages (a tool result can
                // arrive after its message ended).
                if let Some(s) = self.store.session_mut(at) {
                    for m in s.messages.iter_mut().rev().take(6) {
                        if let Some(steps) = m.steps.as_mut().filter(|st| st.iter().any(|x| x.id == id)) {
                            end_step(steps, &id, ok);
                            break;
                        }
                    }
                }
            }
            LoopEvent::LeadMessage { session, text } => {
                let Some(at) = self.store.find_session(&session) else { return };
                // A message with only tool calls stays open: the steps and the
                // text that follow read as one reply.
                if plan::split_reply(&text).text.is_empty() && !text.contains("```octobuddy-") && self.rt.lead_live.contains_key(&session) {
                    if let Some((index, _)) = self.rt.lead_live.get(&session).cloned() {
                        self.rt.lead_live.insert(session, (index, String::new()));
                    }
                    return;
                }
                let Some((index, _)) = self.rt.lead_live.remove(&session) else {
                    let shown = plan::split_reply(&text).text;
                    if !shown.is_empty() {
                        let author = self.store.session(at).map(|s| s.engine().to_string()).unwrap_or_default();
                        self.store.push_message(at, Role::Lead, &author, &shown);
                    }
                    return;
                };
                let shown = plan::split_reply(&text).text;
                if let Some(s) = self.store.session_mut(at) {
                    let empty = shown.is_empty() && s.messages.get(index).is_some_and(|m| m.steps.as_ref().is_none_or(|st| st.is_empty()));
                    if empty && index + 1 == s.messages.len() {
                        s.messages.pop();
                    } else if let Some(m) = s.messages.get_mut(index) {
                        m.text = shown;
                    }
                }
            }
            LoopEvent::LeadTurnDone { session, ok, error, lead_session, text, cost } => {
                let Some(at) = self.store.find_session(&session) else { return };
                let mut turn_cost = None;
                if let Some(total) = cost {
                    // Claude reports what its process has cost so far.
                    let seen = self.rt.lead_cost_seen.insert(session.clone(), total).unwrap_or(0.0);
                    let turn = if total >= seen { total - seen } else { total };
                    let s = self.store.session_mut(at).unwrap();
                    s.lead_cost = Some(s.lead_cost.unwrap_or(0.0) + turn);
                    turn_cost = Some(turn);
                }
                if let Some(q) = self.rt.lead_inflight.get_mut(&session) {
                    q.pop_front();
                }
                let pending = self.rt.lead_pending.entry(session.clone()).or_insert(1);
                *pending = pending.saturating_sub(1);
                if *pending == 0 {
                    self.lead_status.remove(&session);
                    // Its effort changed meanwhile: its next message starts it again with it.
                    if self.respawn.remove(&session) {
                        self.rt.leads.remove(&session);
                    }
                } else {
                    let queued = *pending;
                    self.lead_status.insert(session.clone(), format!("{queued} queued"));
                }
                if let Some(id) = lead_session {
                    self.store.session_mut(at).unwrap().lead_session = Some(id);
                }
                // A tool call whose end never came (its connection dropped)
                // is not running any more: the turn is over.
                if let Some(s) = self.store.session_mut(at) {
                    for m in s.messages.iter_mut().rev().take(12).filter(|m| m.role() == Role::Lead) {
                        for st in m.steps.iter_mut().flatten().filter(|st| st.status == "running") {
                            st.status = "failed".into();
                        }
                    }
                }
                // How long it took, on the turn's last message of the lead.
                // From when it took the message up (its first event), not when sent.
                let began = self.rt.lead_began.remove(&session);
                if let Some(started) = self.rt.lead_started.remove(&session).map(|s| began.unwrap_or(s).max(s)) {
                    if let Some(m) = self.store.session_mut(at).and_then(|s| s.messages.iter_mut().rev().find(|m| m.role() == Role::Lead)) {
                        m.took = Some(now_secs().saturating_sub(started));
                        // For the timeline: when the turn began, what it cost.
                        m.started = Some(started);
                        m.cost = turn_cost;
                    }
                }
                // Its reply so far stays as it is; the next turn writes a new one.
                let live = self.rt.lead_live.remove(&session);
                let stopped = self.stopping.remove(&session).is_some() || (!ok && self.halted.contains(&session));
                // Stopped while it only thought (or had not begun): no empty reply left behind.
                if let Some((index, raw)) = live.filter(|_| stopped) {
                    if let Some(s) = self.store.session_mut(at) {
                        let blank = s.messages.get(index).is_some_and(|m| m.role() == Role::Lead && m.steps.as_ref().is_none_or(|st| st.is_empty()));
                        if blank && stream::strip_think(&raw).trim().is_empty() {
                            s.messages.remove(index);
                        }
                    }
                }
                if !ok {
                    let why = error.unwrap_or_default();
                    if why != "interrupted" && !stopped {
                        // A provider's rate limit: what to do about it, too.
                        let hint = if why.contains("429") || why.to_lowercase().contains("too many requests") {
                            i18n::t(" — the model's provider is rate-limiting this key: try again in a minute, or pick another model.", "——模型的 provider 在限流：等一分钟再试，或换个模型。")
                        } else { "" };
                        self.system(at, &i18n::pick(format!("The outer loop failed: {why}{hint}"), format!("outer 出错：{why}{hint}")));
                    }
                    return;
                }
                // Its agent has heard what was said before the move.
                if let Some(s) = self.store.session_mut(at) {
                    s.carried = None;
                }
                // A plain chat: an answer, no plan to read, no loops to run.
                if !self.store.is_plain(at) {
                    self.on_lead_reply(at, &session, &text);
                    self.write_context(at);
                }
                // It is free again: what waited goes to it now, in order.
                self.drain_outer(at);
                if !self.store.is_plain(at) {
                    self.maybe_review(at);
                }
            }
            LoopEvent::LeadSteerTaken { session } => {
                // What it says from now on answers it: below it, in a reply of its own.
                self.rt.lead_live.remove(&session);
                self.mark_steer(&session, STEER, STEER_TAKEN, None);
                // Taken up as a turn of its own after the last had ended: at work again.
                let pending = self.rt.lead_pending.entry(session.clone()).or_insert(0);
                if *pending == 0 {
                    *pending = 1;
                    self.rt.lead_started.insert(session.clone(), now_secs());
                    self.lead_status.insert(session.clone(), "thinking".into());
                }
            }
            LoopEvent::LeadSteerRefused { session, text } => {
                // Its turn had just ended: it goes first the usual way.
                self.mark_steer(&session, STEER, STEER_QUEUED, Some(&text));
                let item = OuterItem { id: Delivery::new(From::Lead, "").id, work: OuterWork::Steered(text) };
                self.rt.outer_queue.entry(session.clone()).or_default().push_front(item);
                if let Some(at) = self.store.find_session(&session) {
                    self.drain_outer(at);
                }
            }
            LoopEvent::LeadSteerCarried { session, turns } => {
                // They make its next turns: the one ending now does not free it.
                *self.rt.lead_pending.entry(session).or_insert(0) += turns as u32;
            }
            LoopEvent::LeadExited { session, error, gen } => {
                // A process already replaced (another engine or model
                // picked): its late exit leaves the new one alone.
                if self.rt.leads.get(&session).is_none_or(|l| l.gen != gen) {
                    return;
                }
                self.rt.leads.remove(&session);
                self.rt.lead_inflight.remove(&session);
                self.rt.lead_cost_seen.remove(&session);
                self.rt.lead_pending.remove(&session);
                self.lead_status.remove(&session);
                if let (Some(error), Some(at)) = (error, self.store.find_session(&session)) {
                    if !self.halted.contains(&session) {
                        self.system(at, &i18n::pick(format!("The outer loop stopped: {error}"), format!("outer 已退出：{error}")));
                    }
                }
            }
            LoopEvent::PeerReady { peer, dir, .. } => {
                if let Some(p) = self.store.peer_mut(&peer) {
                    p.status = "idle".into();
                    // Its diff is against where the directory was when it started.
                    p.base = workspace::git(&dir, &["rev-parse", "HEAD"]).ok();
                    p.dir = dir;
                    p.activity = None;
                }
                let next = self.rt.lines.entry(peer.clone()).or_default().opened();
                if let Some(d) = next {
                    self.start_peer_turn(&peer, d);
                }
            }
            LoopEvent::PeerFailed { peer, error, .. } => self.peer_failed(&peer, &error),
            LoopEvent::PeerTurnStarted { peer } => {
                // octos runs: a held directory later may be retried afresh.
                self.rt.serve_retries = 0;
                // A turn OctoBuddy did not start: octos went on by itself when
                // the peer's background subagents finished.
                let ours = self.rt.lines.get(&peer).is_some_and(Line::is_busy);
                if let Some(p) = self.store.peer_mut(&peer) {
                    p.status = "running".into();
                    p.started_at = now_secs();
                    // Its work starts now, not when the message was sent.
                    if let Some(open) = p.open_exchange().filter(|e| e.began.is_none()) {
                        open.began = Some(now_secs());
                    }
                    if !ours && p.open_exchange().is_none() {
                        p.log_mut().push(Exchange { from: "subagents".into(), input: "(its subagents finished: it goes on)".into(), reply: None, outcome: None, at: now_secs(), steps: None, took: None, cost_total: None, began: None });
                    }
                }
            }
            LoopEvent::ToolCall { name, args, reply } => {
                use octosense_app_peers::host_tools::ToolOutcome;
                let outcome = match name.as_str() {
                    "octobuddy.status" => ToolOutcome::Ok(self.tool_status(args.get("project").and_then(|v| v.as_str()))),
                    "octobuddy.send" => {
                        let session = args.get("session").and_then(|v| v.as_str()).unwrap_or("");
                        let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("").trim();
                        match self.tool_send(session, message) {
                            Ok(data) => ToolOutcome::Ok(data),
                            Err(err) => ToolOutcome::error("not_sent", err),
                        }
                    }
                    other => ToolOutcome::error("unknown_tool", format!("OctoBuddy has no tool {other}")),
                };
                reply.finish(outcome);
            }
            LoopEvent::SystemLink { result } => {
                self.system_link = Some(result);
            }
            LoopEvent::PeerRound { peer } => {
                if let Some(p) = self.store.peer_mut(&peer) {
                    p.rounds_used = Some(p.rounds_used.unwrap_or(0) + 1);
                }
                self.check_budget(&peer);
            }
            LoopEvent::PeerInfo { peer, model, effort } => {
                if let Some(p) = self.store.peer_mut(&peer) {
                    if model.is_some() { p.model = model; }
                    if effort.is_some() { p.effort = effort; }
                }
            }
            LoopEvent::PeerFileChanged { peer, path } => {
                let Some(p) = self.store.peer_mut(&peer) else { return };
                let rel = Path::new(&path).strip_prefix(&p.dir).map(|r| r.to_string_lossy().into_owned()).unwrap_or(path);
                if crate::workspace::is_agent_file(&rel) {
                    return;
                }
                let touched = p.touched.get_or_insert_with(Vec::new);
                if !touched.contains(&rel) { touched.push(rel.clone()); }
                let pending = self.rt.to_commit.entry(peer).or_default();
                if !pending.contains(&rel) { pending.push(rel); }
            }
            LoopEvent::PeerAgents { peer, running } => {
                self.rt.running_agents.insert(peer, running);
            }
            LoopEvent::PeerSubagent { peer, id, role, title, status, summary } => {
                if let Some(p) = self.store.peer_mut(&peer) {
                    let list = p.subagents.get_or_insert_with(Vec::new);
                    let one = crate::model::Subagent { id: id.clone(), role, title, status, summary };
                    match list.iter_mut().find(|a| a.id == id) {
                        Some(a) => *a = one,
                        None => list.push(one),
                    }
                }
            }
            LoopEvent::PeerActivity { peer, line } => {
                if let Some(p) = self.store.peer_mut(&peer) {
                    if p.is_active() {
                        p.activity = Some(line);
                    }
                }
            }
            LoopEvent::PeerDelta { peer, text } => {
                if let Some(open) = self.store.peer_mut(&peer).and_then(|p| p.open_exchange()) {
                    open.reply.get_or_insert_with(String::new).push_str(&text);
                }
            }
            LoopEvent::PeerTool { peer, id, name, detail } => {
                if let Some(p) = self.store.peer_mut(&peer) {
                    p.activity = Some(format!("{name} {}", detail.lines().next().unwrap_or("")).trim().to_string());
                    if let Some(open) = p.open_exchange() {
                        upsert_step(open.steps.get_or_insert_with(Vec::new), &id, &name, &detail);
                    }
                }
            }
            LoopEvent::PeerToolEnd { peer, id, ok } => {
                if let Some(steps) = self.store.peer_mut(&peer).and_then(|p| p.open_exchange()).and_then(|e| e.steps.as_mut()) {
                    end_step(steps, &id, ok);
                }
            }
            LoopEvent::PeerQuestion { peer, question_id, text, options } => {
                let slug = self.store.peer_mut(&peer).map(|p| {
                    p.activity = Some(i18n::t("asking you a question", "在向你提问").into());
                    p.slug.clone()
                });
                if let (Some(slug), Some(at)) = (slug, self.store.find_peer(&peer)) {
                    self.system(at, &i18n::pick(format!("{slug} asks you: {text} (answer in its tab under Inner)"), format!("{slug} 问你：{text}（请在 Inner 里它的标签页回答）")));
                }
                self.rt.questions.insert(peer, (question_id, text, options));
                // It needs the person: its panel, in place of the app's preview.
                self.show_inner = true;
                self.show_preview = false;
            }
            LoopEvent::PeerSteered { peer, steered, turn } => {
                if !steered {
                    // No turn was running: octos started one with it.
                    if let (Some(turn), Some(line)) = (turn, self.rt.lines.get_mut(&peer)) {
                        line.started(turn, From::Lead);
                    }
                }
            }
            LoopEvent::PeerSteerDropped { peer, texts } => {
                if let Some(line) = self.rt.lines.get_mut(&peer) {
                    line.dropped(From::Lead, texts);
                }
            }
            LoopEvent::PeerTurnEnded { peer, outcome, text } => self.on_peer_turn_ended(&peer, &outcome, text),
            LoopEvent::PeerCost { peer, input, output, cost, context_window } => {
                if let Some(p) = self.store.peer_mut(&peer) {
                    p.usage = Some(model::Usage { input, output, cost, context_window });
                    // Counted after its turn ended: that turn's total.
                    if !p.is_active() {
                        if let Some(last) = p.log_mut().last_mut().filter(|e| e.outcome.is_some()) {
                            last.cost_total = Some(cost);
                        }
                    }
                }
                self.check_budget(&peer);
            }
            LoopEvent::PeerChecked { peer, verdict, passed, commit } => {
                self.rt.checking.remove(&peer);
                self.rt.diffs.remove(&peer);
                self.rt.worktrees.clear();
                let files = self.rt.committing.remove(&peer).unwrap_or_default();
                let Some(at) = self.store.find_peer(&peer) else { return };
                let session_id = self.store.session(at).unwrap().id.clone();
                if let Some(Ok(_)) = &commit {
                    self.stale_reviews(at, &peer, &files);
                }
                if passed == Some(true) || matches!(&commit, Some(Ok(_))) {
                    self.rt.progress.insert(session_id.clone());
                }
                // Not committed: its files wait for the next commit.
                if !matches!(&commit, Some(Ok(_))) {
                    let pending = self.rt.to_commit.entry(peer.clone()).or_default();
                    for f in files { if !pending.contains(&f) { pending.push(f); } }
                }
                let slug = self.store.peer_mut(&peer).map(|p| {
                    if p.status == "checking" {
                        p.status = "idle".into();
                        p.activity = None;
                    }
                    if let Some(Ok(made)) = &commit {
                        p.commits.get_or_insert_with(Vec::new).push(made.clone());
                    }
                    if !verdict.is_empty() {
                        p.verdict = Some(verdict.clone());
                    }
                    if let Some(last) = p.log_mut().last_mut() {
                        last.reply = Some(format!("{}\n\n{verdict}", last.reply.clone().unwrap_or_default()));
                    }
                    p.slug.clone()
                }).unwrap_or_default();
                // The person's own run (a re-check, a chat's commit) stays in the
                // peer's panel: it is in its conversation already.
                if self.rt.rechecks.remove(&peer) {
                    let _ = passed;
                    return;
                }
                let head = verdict.lines().filter(|l| l.starts_with('[')).collect::<Vec<_>>().join(" · ");
                self.system(at, &format!("{slug}: {head}"));
                let entry = self.rt.unreported.entry(session_id).or_default();
                match entry.iter_mut().find(|(id, _, _)| id == &peer) {
                    Some((_, report, _)) => report.push_str(&format!("\n\n{verdict}")),
                    None => entry.push((peer.clone(), verdict, true)),
                }
                // Its report is there for its siblings to read.
                self.write_context(at);
                self.on_report_ready(at, &peer);
                self.maybe_review(at);
            }
            LoopEvent::PeerTurnRejected { peer, error } => {
                // octos had a turn running after all: back in line; its end starts this.
                let d = self.rt.inflight.remove(&peer);
                if let Some(p) = self.store.peer_mut(&peer) {
                    if p.log().last().is_some_and(|e| e.outcome.is_none()) {
                        p.log_mut().pop();
                    }
                }
                match d {
                    Some(d) => if let Some(line) = self.rt.lines.get_mut(&peer) { line.refused(d) },
                    None => self.peer_failed(&peer, &error),
                }
            }
            LoopEvent::PeerApprovalGone { peer, approval_id } => {
                if self.rt.approvals.get(&peer).is_some_and(|(id, _, _)| approval_id.is_empty() || *id == approval_id) {
                    self.rt.approvals.remove(&peer);
                }
            }
            LoopEvent::PeerApproval { peer, approval_id, title, body } => {
                // Auto: OctoBuddy approves for the person, for the rest of the
                // session, and keeps it in the peer's conversation. octos's
                // own "never ask" level needs its solo mode, which this serve
                // is not; the sandbox applies either way.
                let auto = self.store.find_peer(&peer).and_then(|at| self.store.session(at)).is_some_and(|s| s.auto());
                if auto {
                    let _ = self.serve().and_then(|s| s.answer_approval(&session_key(&peer), &approval_id, true, Some("session")));
                    if let Some(open) = self.store.peer_mut(&peer).and_then(|p| p.open_exchange()) {
                        let what = body.lines().next().unwrap_or("").chars().take(120).collect::<String>();
                        open.steps.get_or_insert_with(Vec::new).push(crate::model::Step { id: approval_id, name: "auto-approved".into(), detail: format!("{title}: {what}"), status: "ok".into() });
                    }
                    return;
                }
                let slug = self.store.peer_mut(&peer).map(|p| {
                    p.activity = Some(i18n::pick(format!("waiting for your approval: {title}"), format!("等待你的审批：{title}")));
                    p.slug.clone()
                });
                if let (Some(slug), Some(at)) = (slug, self.store.find_peer(&peer)) {
                    // Said in the conversation too: the panel may be closed.
                    self.system(at, &i18n::pick(format!("{slug} is waiting for your approval ({title}). Open Inner to approve or deny."), format!("{slug} 在等待你的审批（{title}）。打开 Inner 批准或拒绝。")));
                }
                self.rt.approvals.insert(peer, (approval_id, title, body));
                // It needs the person: its panel, in place of the app's preview.
                self.show_inner = true;
                self.show_preview = false;
            }
            LoopEvent::ServeExited { error } => {
                self.rt.serve = None;
                self.rt.opened.clear();
                // A new octos finds the old one (OctoBuddy restarted quickly)
                // still holding its directory: the work waits, octos starts
                // again shortly.
                let log = std::fs::read_to_string(model::data_dir().join("octos/serve/serve.log")).unwrap_or_default();
                if log.contains("different --data-dir") && self.rt.serve_retries < 5 {
                    self.rt.serve_retries += 1;
                    let busy: Vec<String> = self.rt.lines.iter().filter(|(_, l)| l.is_busy()).map(|(id, _)| id.clone()).collect();
                    for peer in busy {
                        if let Some(d) = self.rt.inflight.remove(&peer) {
                            if let Some(line) = self.rt.lines.get_mut(&peer) {
                                line.refused(d);
                            }
                            // The turn never ran: not in its log.
                            if let Some(p) = self.store.peer_mut(&peer) {
                                if p.log().last().is_some_and(|e| e.outcome.is_none()) {
                                    p.log_mut().pop();
                                }
                            }
                        }
                    }
                    self.octos_outers_requeue();
                    let inbox = self.rt.inbox.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(1500));
                        events::post(&inbox, LoopEvent::ServeRetry);
                    });
                    return;
                }
                let busy: Vec<String> = self.rt.lines.iter().filter(|(_, l)| l.is_busy()).map(|(id, _)| id.clone()).collect();
                for peer in busy {
                    self.peer_failed(&peer, &format!("octos stopped: {error}"));
                }
            }
        }
    }

    /// A report for the lead, kept for the next review; the peer's work is
    /// committed and checked first when its task is done.
    #[allow(clippy::too_many_arguments)]
    fn queue_report(&mut self, at: SessionRef, session_id: &str, peer: &str, p: &Peer, report: String, forwarded: bool, outcome: &str, more_queued: bool, commit: Option<String>) {
        self.rt.unreported.entry(session_id.to_string()).or_default().push((peer.to_string(), report, forwarded));
        // Nothing to commit or check: the report is complete now.
        if !self.finish(peer, p, commit, outcome == "completed" && !more_queued) {
            self.on_report_ready(at, peer);
        }
    }

    /// After a peer's turn, on a thread: commits the files its tools changed
    /// (when it named a commit message), then runs its checks when `check` —
    /// agent-spec against its contract, its test command. The verdict comes
    /// back as `PeerChecked`. Says whether there was anything to do.
    fn finish(&mut self, peer: &str, p: &Peer, message: Option<String>, check: bool) -> bool {
        let spec = p.contract.clone().filter(|_| check);
        let tests = p.check.clone().filter(|_| check);
        let pending = self.rt.to_commit.get(peer).cloned().unwrap_or_default();
        let commit = match (message, pending.is_empty()) {
            (Some(message), false) => Some(message),
            _ => None,
        };
        // Changed files without a message: committed for it when its check
        // passes (`auto`); else said in the verdict, kept for the next commit.
        let held = check && self.rt.no_auto_commit.remove(peer);
        let auto = (commit.is_none() && !pending.is_empty() && check && tests.is_some() && !held).then(|| auto_message(&p.slug, &pending));
        let why = if held { "it gave no `Commit:` line, and the outer loop interrupted it or said not to commit" } else { "it gave no `Commit:` line" };
        let loose = (commit.is_none() && auto.is_none() && !pending.is_empty() && check)
            .then(|| format!("[commit] {} file(s) not committed ({}): {why}", pending.len(), pending.join(", ")));
        if spec.is_none() && tests.is_none() && commit.is_none() && loose.is_none() {
            return false;
        }
        let files = if commit.is_some() || auto.is_some() { self.rt.to_commit.remove(peer).unwrap_or_default() } else { Vec::new() };
        self.rt.committing.insert(peer.to_string(), files.clone());
        self.rt.checking.insert(peer.to_string());
        if let Some(p) = self.store.peer_mut(peer) {
            p.status = "checking".into();
            p.activity = Some(match (&commit, &tests) {
                (_, Some(cmd)) => i18n::pick(format!("OctoBuddy is checking its work (running `{cmd}`)"), format!("OctoBuddy 正在检查它的工作（运行 `{cmd}`）")),
                (Some(_), None) => i18n::t("OctoBuddy is committing its files", "OctoBuddy 正在提交它的文件").into(),
                (None, None) => i18n::t("agent-spec is checking the contract", "agent-spec 正在检查契约").into(),
            });
        }
        let (inbox, peer_id, dir, touched) = (self.rt.inbox.clone(), peer.to_string(), p.dir.clone(), p.touched.clone().unwrap_or_default());
        std::thread::spawn(move || {
            let mut verdict = Vec::new();
            // An app's manifest the peer rewrote: its digest made right first.
            if (commit.is_some() || auto.is_some()) && files.iter().any(|f| f == "bundle/manifest.json") {
                crate::plugins::octosense_app::repair_digest(&dir);
            }
            let mut commit = commit.map(|message| workspace::commit_files(&dir, &files, &message));
            match &commit {
                Some(Ok(made)) => verdict.push(format!("[commit] {made} ({})", files.join(", "))),
                Some(Err(err)) => verdict.push(format!("[commit] {err}")),
                None => verdict.extend(loose),
            }
            if let Some(spec) = spec {
                verdict.push(crate::contract::check(std::path::Path::new(&spec), &dir, &touched).unwrap_or_else(|err| format!("[agent-spec] {err}")));
            }
            let mut passed = None;
            if let Some(cmd) = tests {
                let outcome = match crate::plugins::octosense_app::ensure_check(&cmd) {
                    Ok(()) => crate::verify::run(&cmd, &dir, crate::verify::TIMEOUT),
                    Err(err) => crate::verify::Outcome { passed: false, status: format!("could not start: {err}"), tail: String::new() },
                };
                passed = Some(outcome.passed);
                verdict.push(outcome.summary(&cmd));
            }
            // No `Commit:` line: its files go in when its check passed.
            if let Some(message) = auto {
                if passed == Some(true) {
                    let made = workspace::commit_files(&dir, &files, &message);
                    verdict.insert(0, match &made {
                        Ok(made) => format!("[commit] {made} ({}; auto: no `Commit:` line, its check passed)", files.join(", ")),
                        Err(err) => format!("[commit] {err}"),
                    });
                    commit = Some(made);
                } else {
                    verdict.insert(0, format!("[commit] {} file(s) not committed ({}): no `Commit:` line, and its check did not pass", files.len(), files.join(", ")));
                }
            }
            events::post(&inbox, LoopEvent::PeerChecked { peer: peer_id, verdict: verdict.join("\n"), passed, commit });
        });
        true
    }

    /// The person re-runs the shown peer's checks; the result goes to the
    /// lead with the next STATUS, not as a new review.
    pub(crate) fn recheck_peer(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        let Some(p) = self.shown_peer(at).cloned() else { return };
        if p.is_active() {
            return;
        }
        if self.finish(&p.id, &p, None, true) {
            self.rt.rechecks.insert(p.id.clone());
        } else {
            self.system(at, &i18n::pick(format!("{}: nothing to check (no contract, no test command).", p.slug), format!("{}：没有可检查的内容（没有契约，也没有测试命令）。", p.slug)));
        }
        self.save();
        self.sync(cx);
    }

    /// The person moves a slice still waiting for its wave to another wave
    /// of its round (on the flow canvas): it starts now if that wave's turn
    /// has come, and the outer loop hears of it.
    pub(crate) fn set_wave(&mut self, peer: &str, wave: u32) -> Result<(), String> {
        if !self.rt.held.contains_key(peer) {
            return Err(i18n::t("Only a slice still waiting for its wave can move to another.", "只有还在等待自己 wave 的切片才能调整 wave。").into());
        }
        let at = self.store.find_peer(peer).ok_or("no such inner loop")?;
        let session_id = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
        let Some(p) = self.store.peer_mut(peer) else { return Err("no such inner loop".into()) };
        let (slug, from) = (p.slug.clone(), p.wave.unwrap_or(1));
        if from == wave {
            return Ok(());
        }
        p.wave = Some(wave);
        // Back into its wave's place on the canvas.
        p.flow = None;
        p.activity = Some(if wave > 1 {
            i18n::pick(format!("waits for wave {} to be accepted", wave - 1), format!("等待 wave {} 被接受", wave - 1))
        } else {
            i18n::t("starting", "即将开始").to_string()
        });
        self.system(at, &i18n::pick(format!("You moved {slug} from wave {from} to wave {wave}."), format!("你把 {slug} 从 wave {from} 移到了 wave {wave}。")));
        self.rt.notes.entry(session_id).or_default().push(format!("the person moved slice {slug} from wave {from} to wave {wave}: it now runs after wave {} is accepted", wave.saturating_sub(1)));
        self.release_waves(at);
        Ok(())
    }

    /// Starts the held peers whose earlier waves (same round) are all
    /// accepted by the lead, or closed.
    fn release_waves(&mut self, at: SessionRef) {
        let Some(session) = self.store.session(at).cloned() else { return };
        let accepted = |p: &Peer| p.review.as_deref().is_some_and(|r| r.starts_with("accept"));
        let settled = |p: &Peer| p.status == "closed" || accepted(p);
        // Its earlier waves are all settled, and one of them was accepted: a
        // wave the outer loop closed whole (it gave up on the plan, or is
        // stopping everything) starts nothing after it.
        let ready: Vec<(String, String)> = session.peers().iter()
            .filter_map(|p| self.rt.held.get(&p.id).map(|dir| (p, dir)))
            .filter(|(p, _)| p.status != "closed" && !self.halted.contains(&session.id))
            .filter(|(p, _)| {
                let earlier: Vec<&Peer> = session.peers().iter().filter(|o| o.round == p.round && o.wave.unwrap_or(1) < p.wave.unwrap_or(1)).collect();
                earlier.iter().all(|o| settled(o)) && (earlier.is_empty() || earlier.iter().any(|o| accepted(o)))
            })
            .map(|(p, dir)| (p.id.clone(), dir.clone()))
            .collect();
        for (peer, dir) in ready {
            self.rt.held.remove(&peer);
            let slug = self.store.peer_mut(&peer).map(|p| p.slug.clone()).unwrap_or_default();
            self.system(at, &i18n::pick(format!("{slug}: its earlier wave is accepted, starting it."), format!("{slug}：前一个 wave 已被接受，开始执行。")));
            events::post(&self.rt.inbox, LoopEvent::PeerReady { session: session.id.clone(), peer, dir, branch: None });
        }
    }

    /// Deletes a session: its outer loop stops, its inner loops are closed,
    /// what waits for them goes; the session leaves the project.
    pub(crate) fn delete_session(&mut self, at: SessionRef) {
        let Some(session) = self.store.session(at).cloned() else { return };
        for p in session.peers().iter().filter(|p| p.status != "closed") {
            self.close_peer(&p.id, "by you");
        }
        self.rt.leads.remove(&session.id);
        self.octos_outer_stop(&session.id);
        self.rt.lead_pending.remove(&session.id);
        self.rt.outer_queue.remove(&session.id);
        self.rt.unreported.remove(&session.id);
        self.rt.notes.remove(&session.id);
        self.rt.lead_inflight.remove(&session.id);
        self.lead_status.remove(&session.id);
        self.rounds_left.remove(&session.id);
        self.store.projects[at.0].sessions.remove(at.1);
        self.selected = None;
        self.flow_open = None;
    }

    /// A peer agent the person makes: on `model` (none: the default), for
    /// the outer loop `attach` (none: on its own), with a first `task` if
    /// given. The outer loop it joins is told. Its id.
    pub(crate) fn create_peer(&mut self, project: usize, name: &str, role: &str, model: Option<String>, attach: Option<SessionRef>, task: Option<String>) -> Result<String, String> {
        // On its own: the project's holder session for peers without an outer loop.
        let at = match attach {
            Some(at) => at,
            None => {
                let p = self.store.projects.get(project).ok_or("no such project")?;
                match p.sessions.iter().position(|s| s.is_detached()) {
                    Some(si) => (project, si),
                    None => {
                        let at = self.store.add_session(project).ok_or("no such project")?;
                        let s = self.store.session_mut(at).unwrap();
                        s.title = i18n::t("Without an outer loop", "未挂外环").into();
                        s.detached = Some(true);
                        at
                    }
                }
            }
        };
        let dir = match self.work_dir(at) {
            Ok(dir) => dir,
            Err(_) => self.store.projects[project].path.clone(),
        };
        let session = self.store.session(at).cloned().ok_or("no session")?;
        let mut slug = plan::slugify(name);
        if slug.is_empty() {
            slug = "peer".into();
        }
        let taken: Vec<String> = session.peers().iter().map(|p| p.slug.clone()).collect();
        let (base, mut n) = (slug.clone(), 2);
        while taken.contains(&slug) {
            slug = format!("{base}-{n}");
            n += 1;
        }
        let role = plan::slugify(role);
        let role = if role.is_empty() { "developer".to_string() } else { role };
        let id = model::new_id("w");
        let brief = task.clone().unwrap_or_else(|| i18n::t("(made by the person: it waits for their message)", "（由你创建：等待你的消息）").into());
        let round = session.round().max(1);
        self.store.session_mut(at).unwrap().peers_mut().push(Peer {
            id: id.clone(), slug: slug.clone(), role: Some(role.clone()), agent: Some("octos".into()), brief: brief.clone(),
            status: "idle".into(), dir: dir.clone(), branch: None, round, started_at: now_secs(), finished_at: None, activity: None, result: None,
            session_key: Some(session_key(&id)), log: None, contract: None, usage: None, check: None, verdict: None, review: None, landed: None,
            model: None, effort: None, touched: None, base: workspace::git(&dir, &["rev-parse", "HEAD"]).ok(), commits: None, subagents: None,
            estimate: None, wave: None, rounds_used: None, budget: None, over_budget: None, flow: None, joined_from: None, queued: None,
            inflight: None, uncommitted: None, model_pick: model.clone(), accepted: None, review_wanted: None, reviews_for: None, by_person: Some(true), claude_session: None, specs: None,
        });
        let mut line = Line::default();
        line.ready = true;
        self.rt.lines.insert(id.clone(), line);
        let on = model.as_deref().unwrap_or(i18n::t("the default model", "默认模型"));
        if session.is_detached() {
            self.system(at, &i18n::pick(format!("You made {slug} ({role}, {on}), on its own."), format!("你创建了 {slug}（{role}，{on}），未挂外环。")));
        } else {
            self.system(at, &i18n::pick(format!("You made {slug} ({role}, {on}) for this outer loop."), format!("你为这个外环创建了 {slug}（{role}，{on}）。")));
            // Its outer loop hears of it with the next news it gets.
            let what = task.as_deref().map(|t| format!(", its first task from the person: {}", t.chars().take(300).collect::<String>())).unwrap_or_default();
            self.rt.notes.entry(session.id.clone()).or_default()
                .push(format!("the person added an inner loop {slug} ({role}, {on}) to you{what}; message it by slug: it reports to you on that \
first task and on what you send it, while what the person says to it after stays between them"));
        }
        if let Some(task) = task {
            if !session.is_detached() {
                self.rt.person_tasks.insert(id.clone());
            }
            self.deliver(&id, Delivery::new(From::Person, task), Mode::Queue);
        }
        Ok(id)
    }

    /// Stops a peer that used its budget: its turn is interrupted, and its
    /// next work waits until the person raises the budget.
    fn check_budget(&mut self, peer: &str) {
        let Some(p) = self.store.peer_mut(peer).filter(|p| p.over_budget.is_none()) else { return };
        let cost = p.usage.as_ref().map(|u| u.cost).unwrap_or(0.0);
        let Some(why) = p.budget.and_then(|b| b.exceeded(p.rounds_used.unwrap_or(0), cost)) else { return };
        p.over_budget = Some(why.clone());
        p.activity = Some(i18n::pick(format!("budget used ({why})"), format!("预算已用完（{why}）")));
        let (slug, key) = (p.slug.clone(), p.session_key.clone().unwrap_or_else(|| session_key(peer)));
        if let Some((turn, _)) = self.rt.lines.get(peer).and_then(|l| l.turn.clone()) {
            let _ = self.serve().and_then(|s| s.interrupt(&key, &turn));
        }
        let Some(at) = self.store.find_peer(peer) else { return };
        self.system(at, &i18n::pick(format!("{slug} used its budget ({why}) and was stopped; raise its budget to let it go on."),
            format!("{slug} 用完了预算（{why}），已停下；调高预算即可继续。")));
        let session_id = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
        self.rt.notes.entry(session_id).or_default().push(format!("{slug} was stopped: it used the budget the person gave it ({why})"));
    }

    /// The person's budget for a peer (`None`: no limit). Raised past what it
    /// used, the peer goes on with what waits for it.
    pub(crate) fn set_budget(&mut self, peer: &str, budget: Option<model::Budget>) {
        let Some(p) = self.store.peer_mut(peer) else { return };
        p.budget = budget.filter(|b| b.steps.is_some() || b.cost.is_some());
        let cost = p.usage.as_ref().map(|u| u.cost).unwrap_or(0.0);
        let still = p.budget.and_then(|b| b.exceeded(p.rounds_used.unwrap_or(0), cost));
        let freed = still.is_none() && p.over_budget.take().is_some();
        if !freed {
            p.over_budget = p.over_budget.take().or(still);
            return;
        }
        p.activity = None;
        let slug = p.slug.clone();
        if let Some(at) = self.store.find_peer(peer) {
            self.system(at, &i18n::pick(format!("{slug}'s budget was raised: it goes on."), format!("{slug} 的预算已调高，继续工作。")));
        }
        let next = self.rt.lines.get_mut(peer).filter(|l| l.ready && !l.is_busy()).and_then(|l| l.queue.pop_front());
        if let Some(d) = next {
            self.start_peer_turn(peer, d);
        }
    }

    /// Moves a peer to another outer loop of the same project: it asks the
    /// peer to describe itself (its task, the goal of the outer loop it came
    /// from, what it did, what is left) and that description is the new
    /// outer loop's first report from it; the old one is told it left.
    pub(crate) fn move_peer(&mut self, peer: &str, to: SessionRef) -> Result<(), String> {
        let from = self.store.find_peer(peer).ok_or("no such inner loop")?;
        if from == to {
            return Ok(());
        }
        if from.0 != to.0 {
            return Err(i18n::t("an inner loop moves only between outer loops of its project", "inner 只能在同一个项目的外环之间移动").into());
        }
        if self.rt.held.contains_key(peer) {
            return Err(i18n::t("it waits for its wave and has not started", "它还在等自己的 wave，尚未开始").into());
        }
        let source = self.store.session(from).cloned().ok_or("no session")?;
        let goal = source.messages.iter().find(|m| m.role() == Role::User).map(|m| m.text.chars().take(400).collect::<String>());
        let target = self.store.session_mut(to).ok_or("no session")?;
        match (&target.work_dir, &source.work_dir) {
            (None, dir) => {
                target.work_dir = dir.clone();
                target.work_branch = source.work_branch.clone();
            }
            (Some(a), Some(b)) if a == b => {}
            _ => return Err(i18n::t("that outer loop works in another directory (its own worktree)", "那个外环在另一个目录（它自己的 worktree）里工作").into()),
        }
        let (to_id, to_title) = (target.id.clone(), target.title.clone());
        let Some(index) = self.store.session(from).and_then(|s| s.peers().iter().position(|p| p.id == peer)) else { return Err("no such inner loop".into()) };
        let mut moved = self.store.session_mut(from).unwrap().peers_mut().remove(index);
        if moved.status == "closed" {
            self.store.session_mut(from).unwrap().peers_mut().insert(index, moved);
            return Err(i18n::t("it is closed", "它已关闭").into());
        }
        moved.joined_from = Some(source.title.clone());
        moved.flow = None;
        let slug = moved.slug.clone();
        self.store.session_mut(to).unwrap().peers_mut().push(moved);
        // What it reported and the old outer loop has not read goes with it.
        if let Some(list) = self.rt.unreported.get_mut(&source.id) {
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(list).into_iter().partition(|(id, _, _)| id == peer);
            *list = rest;
            self.rt.unreported.entry(to_id.clone()).or_default().extend(mine);
        }
        let left = self.rounds_left.entry(to_id.clone()).or_insert(0);
        *left = (*left).max(2);
        self.rt.joined.insert(peer.to_string());
        self.rt.notes.entry(source.id.clone()).or_default()
            .push(format!("the person moved {slug} to another outer loop (\"{to_title}\"): it no longer works for you"));
        self.system(from, &i18n::pick(format!("{slug} moved to the outer loop “{to_title}”."), format!("{slug} 已移到外环“{to_title}”。")));
        self.system(to, &i18n::pick(format!("{slug} joined from the outer loop “{}”; it describes itself next.", source.title),
            format!("{slug} 从外环“{}”加入；它接下来会介绍自己。", source.title)));
        let goal = goal.map(|g| format!(" (the person asked it: \"{g}\")")).unwrap_or_default();
        let text = format!("You now work for a different outer loop: \"{to_title}\". Describe yourself to it in your report block: \
your role, the task you were given, what the outer loop that gave it (\"{}\") set out to do{goal}, what you have done so far \
(the files you changed, what you ran and whether it passed) and what is left. Then wait for its instructions.", source.title);
        self.deliver(peer, Delivery::new(From::Lead, text), Mode::Queue);
        Ok(())
    }

    /// Closes a peer: its running turn is cancelled, what waits for it is
    /// dropped, and it takes no more work; its conversation stays readable.
    pub(crate) fn close_peer(&mut self, peer: &str, by: &str) {
        let Some(at) = self.store.find_peer(peer) else { return };
        let running = self.rt.lines.get_mut(peer).and_then(Line::clear);
        if let Some(turn) = running {
            let key = session_key(peer);
            let _ = self.serve().and_then(|s| s.interrupt(&key, &turn));
        }
        self.claude_inner_stop(peer);
        self.rt.lines.remove(peer);
        self.rt.slot_wait.retain(|w| w != peer);
        self.release_slots();
        self.rt.held.remove(peer);
        self.rt.questions.remove(peer);
        self.rt.approvals.remove(peer);
        self.rt.pending.remove(peer);
        let session_id = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
        let Some(p) = self.store.peer_mut(peer) else { return };
        p.status = "closed".into();
        p.activity = None;
        let slug = p.slug.clone();
        if self.selected_peer.as_deref() == Some(peer) {
            self.selected_peer = None;
        }
        let who = match by {
            "by you" => i18n::t("by you", "由你关闭"),
            "reviewed" => i18n::t("its review is done", "审查已完成"),
            _ => i18n::t("by the outer loop", "由 outer 关闭"),
        };
        self.system(at, &i18n::pick(format!("{slug} closed ({who})."), format!("{slug} 已关闭（{who}）。")));
        self.release_waves(at);
        if by == "by you" {
            self.rt.notes.entry(session_id).or_default().push(format!("the person closed {slug}"));
        }
    }

    /// The person closes the inner loop shown in the panel.
    pub(crate) fn close_shown_peer(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        let Some(peer) = self.shown_peer(at).map(|p| p.id.clone()) else { return };
        self.close_peer(&peer, "by you");
        self.save();
        self.sync(cx);
    }

    /// The person merges the session's worktree branch into the project's
    /// current branch: once, when the work is done.
    pub(crate) fn merge_worktree(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        let project = self.store.projects[at.0].path.clone();
        let Some(session) = self.store.session(at).cloned() else { return };
        let (Some(dir), Some(branch)) = (session.work_dir.clone(), session.work_branch.clone()) else { return };
        self.rt.worktrees.remove(&session.id);
        let loose = workspace::uncommitted(&dir);
        let result = if loose.is_empty() {
            workspace::merge_branch(&project, &branch)
        } else {
            Err(format!("Not merged: {} file(s) are not committed in the worktree ({}). Ask the outer loop to have them committed, or commit them in {dir}.", loose.len(), loose.join(", ")))
        };
        match result {
            Ok(done) => {
                self.system(at, &done);
                self.rt.notes.entry(session.id.clone()).or_default().push(format!("the person merged the worktree branch {branch} into the project"));
            }
            Err(err) => self.system(at, &err),
        }
        self.rt.diffs.clear();
        self.save();
        self.sync(cx);
    }

    /// The person drops the session's worktree and branch. The next message
    /// starts a new worktree, at the same place, from the project's commit.
    pub(crate) fn discard_worktree(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        let project = self.store.projects[at.0].path.clone();
        let Some(session) = self.store.session(at).cloned() else { return };
        let (Some(dir), Some(branch)) = (session.work_dir.clone(), session.work_branch.clone()) else { return };
        self.rt.worktrees.remove(&session.id);
        match workspace::remove_worktree(&project, &dir, &branch) {
            Ok(done) => {
                // Its process sat in the removed directory: the next message
                // starts it again in the new one (same path, same conversation).
                self.rt.leads.remove(&session.id);
                self.system(at, &done);
                self.rt.notes.entry(session.id.clone()).or_default().push(format!("the person discarded the worktree and its branch {branch}; the work in it is gone"));
            }
            Err(err) => self.system(at, &err),
        }
        self.save();
        self.sync(cx);
    }

    /// The lead message being streamed in a session, started when needed.
    fn live_lead(&mut self, at: SessionRef, session: &str) -> (usize, String) {
        if let Some(live) = self.rt.lead_live.get(session) {
            return live.clone();
        }
        // Who writes it: the engine running the outer loop now.
        let author = self.store.session(at).map(|s| s.engine().to_string()).unwrap_or_default();
        self.store.push_message(at, Role::Lead, &author, "");
        let index = self.store.session(at).map(|s| s.messages.len() - 1).unwrap_or(0);
        self.rt.lead_live.insert(session.to_string(), (index, String::new()));
        (index, String::new())
    }

    fn on_lead_reply(&mut self, at: SessionRef, session: &str, text: &str) {
        let reply = plan::split_reply(text);
        let left = self.rounds_left.get(session).copied().unwrap_or(0);
        if let Some(err) = reply.plan_error.clone().or(reply.send_error.clone()) {
            // Ask once more, within the request's rounds, rather than leave the
            // person to notice and retry.
            if left == 0 {
                self.system(at, &i18n::pick(format!("Could not read the outer loop's block ({err}); nothing was started."), format!("无法解析 outer 的指令块（{err}）；没有启动任何任务。")));
            } else {
                self.rounds_left.insert(session.to_string(), left - 1);
                self.system(at, &i18n::pick(format!("Could not read the outer loop's block ({err}); asking it to send it again."), format!("无法解析 outer 的指令块（{err}）；已请它重新发送。")));
                self.tell_lead(at, format!("Your octobuddy block could not be read ({err}). Send it again as one fenced block holding valid JSON, with the closing fence on its own line."));
            }
            return;
        }
        self.apply_reply(at, session, reply);
    }

    /// What the outer loop decided (its blocks, or its tool calls): reviews,
    /// closes, queue changes, messages, then a new round.
    pub(crate) fn apply_reply(&mut self, at: SessionRef, session: &str, reply: plan::Reply) {
        let mut calibration = Vec::new();
        for review in reply.reviews {
            let target = self.store.session(at).and_then(|s| s.peers().iter().rev().find(|p| p.slug == review.slug).map(|p| p.id.clone()));
            if let Some(p) = target.and_then(|id| self.store.peer_mut(&id)) {
                // How it went against its estimate: the project's memory, for the next one.
                if review.verdict.starts_with("accept") {
                    let text = format!("Accepted slice \"{}\" ({}{}): estimated {} rounds, took {} steps (model calls) over {} turn(s); \
checks: {}. Task: {}", p.slug, p.role(), p.model_pick.as_deref().map(|m| format!(", model {m}")).unwrap_or_default(),
                        p.estimate.map(|e| e.to_string()).unwrap_or_else(|| "no".into()), p.rounds_used.unwrap_or(0), p.log().len(),
                        p.verdict.as_deref().unwrap_or("none").lines().next().unwrap_or(""),
                        p.brief.lines().find(|l| !l.trim().is_empty() && !l.starts_with("spec:") && !l.starts_with("---")).unwrap_or("").trim());
                    calibration.push(text);
                }
                // Accepted: its files as they are now, to tell a later change.
                p.accepted = review.verdict.starts_with("accept").then(|| {
                    workspace::file_hashes(&p.dir, p.touched.as_deref().unwrap_or(&[])).into_iter()
                        .map(|(path, hash)| model::FileHash { path, hash }).collect()
                });
                p.review = Some(if review.why.is_empty() { review.verdict } else { format!("{}: {}", review.verdict, review.why) });
            }
        }
        for text in calibration {
            crate::memory::remember(&self.store.projects[at.0].path, "calibration", text);
        }
        for slug in &reply.close {
            let target = self.store.session(at).and_then(|s| s.peers().iter().rev().find(|p| &p.slug == slug && p.status != "closed").map(|p| p.id.clone()));
            match target {
                Some(peer) => self.close_peer(&peer, "by the outer loop"),
                None => self.system(at, &i18n::pick(format!("The outer loop asked to close \"{slug}\", but no open inner loop has that name."), format!("outer 要关闭“{slug}”，但没有这个名字的 inner。"))),
            }
        }
        // Accepting (or closing) the last of a wave starts the next.
        self.release_waves(at);
        for op in reply.queue_ops {
            let target = self.store.session(at).and_then(|s| s.peers().iter().rev().find(|p| p.slug == op.to).map(|p| p.id.clone()));
            // Closed (this reply, say): what waited for it went with it.
            let closed = target.as_ref().and_then(|id| self.store.find_peer(id).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| &p.id == id).map(|p| p.status == "closed"))).unwrap_or(false);
            let result = match (target.as_ref().and_then(|id| self.rt.lines.get_mut(id)), op.op.as_str()) {
                (None, _) if closed => Ok(()),
                (None, _) => Err(format!("no peer named {}", op.to)),
                (Some(line), "cancel") => op.ids.first().ok_or("no id".to_string()).and_then(|id| line.cancel(id)),
                (Some(line), "replace") => op.ids.first().ok_or("no id".to_string()).and_then(|id| line.replace(id, &op.message)),
                (Some(line), "merge") => line.merge(&op.ids, &op.message),
                (Some(_), other) => Err(format!("unknown queue op {other}")),
            };
            let what = format!("{} {} for {}", op.op, op.ids.join("+"), op.to);
            match result {
                Ok(()) => self.system(at, &i18n::pick(format!("Outer: {what}."), format!("Outer：{what}。"))),
                Err(err) => self.system(at, &i18n::pick(format!("Outer tried to {what}: {err}"), format!("Outer 尝试 {what} 失败：{err}"))),
            }
        }
        for send in reply.sends {
            let target = self.store.session(at).and_then(|s| s.peers().iter().rev().find(|p| p.slug == send.to).map(|p| p.id.clone()));
            match target {
                Some(peer) => {
                    let mode = Mode::parse(&send.mode);
                    let how = match mode { Mode::Interrupt => "interrupt", Mode::Steer => "steer", Mode::Queue => "queue" };
                    self.store.push_meta(at, Role::ToPeer, &send.to, &send.message, how);
                    // Interrupted (paused, redirected) or told not to commit:
                    // what it leaves is not committed for it.
                    let lower = send.message.to_lowercase();
                    if mode == Mode::Interrupt || ["不要提交", "别提交", "不要 commit", "do not commit", "don't commit", "dont commit"].iter().any(|w| lower.contains(w)) {
                        self.rt.no_auto_commit.insert(peer.clone());
                    }
                    // A new task as a contract: kept, and in force from now on.
                    if crate::contract::is_contract(&send.message) {
                        self.keep_later_spec(at, &peer, &send.message);
                    }
                    self.deliver(&peer, Delivery::new(From::Lead, send.message), mode);
                }
                None => self.system(at, &i18n::pick(format!("The outer loop wrote to \"{}\", but no inner loop has that name.", send.to), format!("outer 给“{}”发了消息，但没有这个名字的 inner。", send.to))),
            }
        }
        if !reply.slices.is_empty() {
            if self.halted.contains(session) {
                self.system(at, i18n::t("Not starting new inner loops: the session was stopped.", "会话已停止，不再启动新的 inner。"));
            } else {
                self.start_round(at, reply.slices, reply.estimate, reply.shared);
            }
        }
    }

    /// A contract the outer loop sent an inner loop after its first task:
    /// kept beside the first (`<slug>-2.spec.md`, …), and the one checked now.
    fn keep_later_spec(&mut self, at: SessionRef, peer: &str, text: &str) {
        let Some(session_id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        let Some(p) = self.store.peer_mut(peer) else { return };
        let n = p.specs.as_ref().map(|v| v.len()).unwrap_or(0) + 1;
        let name = if n == 1 { p.slug.clone() } else { format!("{}-{n}", p.slug) };
        let dir = model::data_dir().join("specs").join(&session_id);
        if let Ok(path) = crate::contract::write(&dir, &name, text) {
            let path = path.to_string_lossy().into_owned();
            p.contract = Some(path.clone());
            p.specs.get_or_insert_with(Vec::new).push(model::SpecRef { path, at: now_secs(), round: None, contract: true });
        }
    }

    /// A peer committed `files`: an accepted review of another peer whose
    /// files these changed is stale, and the outer loop is told to look again.
    fn stale_reviews(&mut self, at: SessionRef, by: &str, files: &[String]) {
        let Some(session) = self.store.session(at).cloned() else { return };
        let who = session.peers().iter().find(|p| p.id == by).map(|p| p.slug.clone()).unwrap_or_default();
        for p in session.peers().iter().filter(|p| p.id != by) {
            let Some(accepted) = p.accepted.as_ref() else { continue };
            let touched: Vec<String> = accepted.iter().filter(|f| files.contains(&f.path)).map(|f| f.path.clone()).collect();
            let now = workspace::file_hashes(&p.dir, &touched);
            let changed: Vec<String> = accepted.iter()
                .filter(|f| touched.contains(&f.path) && !now.iter().any(|(path, hash)| path == &f.path && hash == &f.hash))
                .map(|f| f.path.clone()).collect();
            if changed.is_empty() {
                continue;
            }
            let (slug, list) = (p.slug.clone(), changed.join(", "));
            if let Some(q) = self.store.peer_mut(&p.id) {
                q.accepted = None;
                q.review = Some(format!("stale: {who} changed {list} after it was accepted"));
            }
            self.system(at, &i18n::pick(format!("{slug}'s accepted work is out of date: {who} changed {list} afterwards."),
                format!("{slug} 已接受的工作过期了：{who} 之后改了 {list}。")));
            self.rt.notes.entry(session.id.clone()).or_default()
                .push(format!("{slug}'s review is stale: {who} changed {list} after you accepted it; review {slug} again as its files are now"));
        }
    }

    /// Starts the peers that wait for a slot, in order, while there is one.
    pub(crate) fn release_slots(&mut self) {
        while let Some(peer) = self.rt.slot_wait.front().cloned() {
            if self.rt.lines.values().filter(|l| l.is_busy()).count() >= max_inners() {
                break;
            }
            self.rt.slot_wait.pop_front();
            let next = self.rt.lines.get_mut(&peer).filter(|l| l.ready && !l.is_busy()).and_then(|l| l.queue.pop_front());
            if let Some(d) = next {
                self.start_peer_turn(&peer, d);
            }
        }
    }

    fn on_peer_turn_ended(&mut self, peer: &str, outcome: &str, text: String) {
        self.rt.inflight.remove(peer);
        self.rt.diffs.remove(peer);
        self.rt.worktrees.clear();
        let (from, next) = self.rt.lines.get_mut(peer).map(Line::ended).unwrap_or((None, None));
        let Some(at) = self.store.find_peer(peer) else { return };
        let session_id = self.store.session(at).unwrap().id.clone();
        let Some(p) = self.store.peer_mut(peer).cloned() else { return };
        let commit = (outcome != "interrupted").then(|| commit_message(&text)).flatten();
        if let Some(p) = self.store.peer_mut(peer) {
            p.activity = None;
            p.finished_at = Some(now_secs());
            p.status = match (outcome, next.is_some()) {
                (_, true) => "running",
                ("failed", _) => "failed",
                ("interrupted", _) => "interrupted",
                _ => "idle",
            }.into();
            if outcome != "interrupted" {
                p.result = Some(text.clone());
            }
            let cost_total = p.usage.as_ref().map(|u| u.cost);
            if let Some(open) = p.open_exchange() {
                open.outcome = Some(outcome.to_string());
                open.reply = Some(text.clone());
                open.took = Some(now_secs().saturating_sub(open.began.unwrap_or(open.at)));
                open.cost_total = cost_total;
            }
        }
        let (_, report) = plan::take_report(&text);
        let from_lead_task = self.rt.pending.contains(peer);
        // The person's first task for a peer they made for an outer loop is
        // reported to it; anything else the person says to a peer is a chat.
        let person_task = from == Some(From::Person) && outcome != "interrupted" && self.rt.person_tasks.remove(peer);
        let chat = from == Some(From::Person) && !from_lead_task && !person_task;
        match (outcome, from, report) {
            ("interrupted", _, _) => {}
            // The person and the peer talked: that stays between them, as in
            // Cindy (a person–worker chat never reaches the lead), even when
            // the peer wrote a report block.
            (_, Some(From::Person), _) if chat => {
                if self.finish(peer, &p, commit, false) {
                    self.rt.rechecks.insert(peer.to_string());
                }
            }
            // It reported: that is what the lead gets.
            (_, _, Some(report)) => {
                self.rt.pending.remove(peer);
                self.store.push_meta(at, Role::Peer, &p.slug, &report, p.role());
                self.queue_report(at, &session_id, peer, &p, report, false, outcome, next.is_some(), commit);
            }
            // A task from the lead ended without a report: forward the reply,
            // marked as such, so the lead never waits for a report that will not come.
            (_, Some(From::Lead), None) if from_lead_task => {
                self.rt.pending.remove(peer);
                let tail: String = text.chars().rev().take(8000).collect::<Vec<_>>().into_iter().rev().collect();
                self.store.push_meta(at, Role::Peer, &p.slug, &format!("(forwarded: it did not report)\n{}", short(&tail, 4000)), p.role());
                self.queue_report(at, &session_id, peer, &p, tail, true, outcome, next.is_some(), commit);
            }
            // The person cut in on a task from the lead, which still waits for
            // it: what the peer finished with goes to the lead, marked as such.
            (_, Some(From::Person), None) if from_lead_task && outcome == "completed" => {
                self.rt.pending.remove(peer);
                let tail: String = text.chars().rev().take(8000).collect::<Vec<_>>().into_iter().rev().collect();
                self.store.push_meta(at, Role::Peer, &p.slug, &format!("(the person cut in on its task; its reply after)\n{}", short(&tail, 4000)), p.role());
                self.queue_report(at, &session_id, peer, &p, format!("(The person cut in on your task with their own message; this is its reply after.)\n{tail}"), true, outcome, next.is_some(), commit);
            }
            // The person's own first task, ended without a report: the outer
            // loop was not waiting on it.
            (_, Some(From::Person), None) => {
                if self.finish(peer, &p, commit, false) {
                    self.rt.rechecks.insert(peer.to_string());
                }
            }
            // A turn it went on with by itself (its subagents finished).
            (_, None, None) if self.finish(peer, &p, commit, false) => {
                self.rt.rechecks.insert(peer.to_string());
            }
            _ => {}
        }
        if let Some(d) = next {
            self.start_peer_turn(peer, d);
        }
        // Its slot is free (unless it took it again): the next waiting peer's.
        self.release_slots();
        self.maybe_review(at);
    }
}

/// The commit message a peer named: its last `Commit: …` line.
fn commit_message(text: &str) -> Option<String> {
    text.lines().rev()
        .find_map(|l| {
            let l = l.trim().trim_start_matches(['*', '`', '-', ' ']);
            l.strip_prefix("Commit:").or_else(|| l.strip_prefix("commit:"))
        })
        .map(|m| m.trim_start_matches(['*', ' ']).trim().trim_matches('`').trim().to_string())
        .filter(|m| !m.is_empty())
}

/// The message OctoBuddy commits a peer's files with when it named none and
/// its check passed: conventional (`chore(<slug>): …`, the scope lowercase
/// kebab without digits, as the projects' commit hooks want).
fn auto_message(slug: &str, files: &[String]) -> String {
    let scope: String = slug.to_lowercase().chars().filter(|c| !c.is_ascii_digit())
        .map(|c| if c.is_ascii_lowercase() { c } else { '-' }).collect();
    let scope = scope.split('-').filter(|w| !w.is_empty()).collect::<Vec<_>>().join("-");
    let scope = if scope.is_empty() { "slice".to_string() } else { scope };
    let what = match files {
        [one] => one.rsplit('/').next().unwrap_or(one).to_string(),
        _ => format!("{} files", files.len()),
    };
    short(&format!("chore({scope}): update {what} (check passed)"), 96).trim_end_matches(" …").to_string()
}

fn short(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push_str(" …");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_auto_commit_is_conventional() {
        assert_eq!(auto_message("ledger-ui2", &["bundle/main.splash".into()]), "chore(ledger-ui): update main.splash (check passed)");
        assert_eq!(auto_message("Fix_3 Stats", &["a".into(), "b".into()]), "chore(fix-stats): update 2 files (check passed)");
        assert_eq!(auto_message("42", &["a".into()]), "chore(slice): update a (check passed)");
    }

    #[test]
    fn prompts_say_where_and_from_whom() {
        let first = first_prompt("Add power.", "/c/p", Some("octobuddy/x/p"), None, ".octobuddy/context/s1");
        assert!(first_prompt("t", "/p", None, Some("pytest -q"), ".octobuddy/context/s1").contains("OctoBuddy runs `pytest -q`"));
        assert!(first.contains("/c/p") && first.contains("octobuddy/x/p") && first.contains("Add power."));
        assert!(first.contains("Do not run git commit") && first.contains("`Commit: <message>`"), "OctoBuddy commits for it");
        assert!(first.contains("```octobuddy-report") && first.contains("Report once"), "peers learn how to report");
        let shared = first_prompt("t", "/p", None, None, ".octobuddy/context/s1");
        assert!(shared.contains(".octobuddy/context/s1/outer.md"));
        assert!(shared.contains("the project directory") && shared.contains("same directory at the same time"), "loops share the directory");
        assert!(later_prompt(From::Person, "hi", false).starts_with("Message from the person"));
        assert!(later_prompt(From::Person, "hi", false).contains("without a report block"));
        assert!(later_prompt(From::Person, "hi", true).contains("report on it"), "the lead's open task is still reported");
        assert!(later_prompt(From::Lead, "hi", false).starts_with("Message from the lead"));
        assert_eq!(session_key("w-1"), "local:octobuddy:w-1");
        assert_eq!(commit_message("Done.\n\nCommit: feat(calc): 新增 subtract\n"), Some("feat(calc): 新增 subtract".into()));
        assert_eq!(commit_message("**Commit:** `fix: x`"), Some("fix: x".into()), "markdown around it is not the message");
        assert_eq!(commit_message("Commit: `fix: y`"), Some("fix: y".into()));
        assert_eq!(commit_message("no commit here"), None);
    }
}
