//! Projects, their sessions and each session's messages, kept as one JSON
//! file. A session is one OctoBuddy campaign: the person talks to the lead
//! (the outer loop), which splits the work and starts inner-loop peers.
use makepad_widgets::makepad_micro_serde::*;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    /// The person.
    User,
    /// The outer loop: the lead that plans, reviews and verifies.
    Lead,
    /// An inner-loop peer working on one part of the task.
    Peer,
    /// Status lines from OctoBuddy itself.
    System,
    /// A message the lead sent to a peer (`author` is the peer, `meta` the mode).
    ToPeer,
    /// The person talked to a peer; the lead was told (`author` is the peer).
    Note,
}

impl Role {
    /// The name stored in the file: plain strings, so a script or another
    /// tool can write a message without knowing the serializer's enum form.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Lead => "lead",
            Role::Peer => "peer",
            Role::System => "system",
            Role::ToPeer => "to_peer",
            Role::Note => "note",
        }
    }

    /// An unknown name reads as a system line rather than failing the file.
    pub fn parse(name: &str) -> Role {
        match name {
            "user" => Role::User,
            "lead" => Role::Lead,
            "peer" => Role::Peer,
            "to_peer" => Role::ToPeer,
            "note" => Role::Note,
            _ => Role::System,
        }
    }
}

#[derive(Clone, Debug, SerJson, DeJson)]
pub struct Message {
    /// A [`Role`] name; read it with [`Message::role`].
    pub role: String,
    /// Who spoke, for a lead or a peer (for example "claude" or a peer slug).
    pub author: String,
    pub text: String,
    pub at: u64,
    /// The tools used while writing it (a lead's message).
    pub steps: Option<Vec<Step>>,
    /// More about it: a peer's role, a message's delivery mode.
    pub meta: Option<String>,
    /// For the last message of a lead's turn: how long the turn took, in seconds.
    pub took: Option<u64>,
    /// For the same: when the turn began, and what it cost (USD).
    pub started: Option<u64>,
    pub cost: Option<f64>,
    /// For a lead's message: the model it was written on, as the picker
    /// names it then (`minimax-cn/MiniMax-M3` on a provider, or the agent's own).
    pub model: Option<String>,
}

/// One tool call, as the stream shows it.
#[derive(Clone, Debug, PartialEq, SerJson, DeJson)]
pub struct Step {
    pub id: String,
    pub name: String,
    pub detail: String,
    /// `running`, `ok` or `failed`.
    pub status: String,
}

/// Adds a tool call to `steps`, or fills in one already there (same id).
pub fn upsert_step(steps: &mut Vec<Step>, id: &str, name: &str, detail: &str) {
    match steps.iter_mut().find(|s| s.id == id) {
        Some(s) => {
            if !detail.is_empty() { s.detail = detail.to_string(); }
            if !name.is_empty() { s.name = name.to_string(); }
        }
        None => steps.push(Step { id: id.into(), name: name.into(), detail: detail.into(), status: "running".into() }),
    }
}

pub fn end_step(steps: &mut [Step], id: &str, ok: bool) {
    if let Some(s) = steps.iter_mut().find(|s| s.id == id) {
        s.status = if ok { "ok" } else { "failed" }.into();
    }
}

impl Message {
    pub fn role(&self) -> Role {
        Role::parse(&self.role)
    }
}

/// One inner-loop worker: an octos run on one slice of the lead's plan.
#[derive(Clone, Debug, SerJson, DeJson)]
pub struct Peer {
    pub id: String,
    /// The slice's short name, from the lead's plan.
    pub slug: String,
    /// What the lead asked it to be: developer, reviewer, tester, writer…
    pub role: Option<String>,
    /// The agent that runs it (`octos` for now).
    pub agent: Option<String>,
    pub brief: String,
    /// `queued`, `running`, `done`, `failed`, `stopped` or `interrupted`.
    pub status: String,
    /// Where it works: its own git worktree, or the project directory.
    pub dir: String,
    /// Its branch, when it has a worktree.
    pub branch: Option<String>,
    /// Which round of the session's plan it belongs to.
    pub round: u64,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    /// The latest thing it did (iteration, tools), while it runs.
    pub activity: Option<String>,
    /// Its final report, or why it failed.
    pub result: Option<String>,
    /// Its session in the octos server, once it has one.
    pub session_key: Option<String>,
    /// Every message it was given, and its reply, oldest first.
    pub log: Option<Vec<Exchange>>,
    /// Its brief as an agent-spec contract file, when the lead wrote one.
    pub contract: Option<String>,
    /// What its session has used: (input tokens, output tokens, cost, context window).
    pub usage: Option<Usage>,
    /// The command that runs its tests, from the lead's plan.
    pub check: Option<String>,
    /// What OctoBuddy's own checks found last (agent-spec, tests).
    pub verdict: Option<String>,
    /// The lead's call on its branch: `merge: why`, `fix: why`, `discard: why`.
    pub review: Option<String>,
    /// What the person did with its branch: merged or discarded.
    pub landed: Option<String>,
    /// The model its session runs, as octos reported it.
    pub model: Option<String>,
    /// Its reasoning effort, as octos reported it (none: the model's default).
    pub effort: Option<String>,
    /// The files its file tools changed, relative to where it works.
    pub touched: Option<Vec<String>>,
    /// The commit where it started: its diff is against this.
    pub base: Option<String>,
    /// The commits OctoBuddy made of its work (`abc1234 subject`).
    pub commits: Option<Vec<String>>,
    /// Its subagents, as octos last reported them.
    pub subagents: Option<Vec<Subagent>>,
    /// The lead's agent-estimation for it: effective rounds, and its wave.
    pub estimate: Option<f64>,
    pub wave: Option<u32>,
    /// Rounds it has used (octos's model-loop iterations, all its turns).
    pub rounds_used: Option<u64>,
    /// What the person allows it: past it, OctoBuddy stops it and starts no
    /// more of its work until the person raises it.
    pub budget: Option<Budget>,
    /// Set while its budget is used up: why.
    pub over_budget: Option<String>,
    /// Where its card sits on the flow canvas.
    pub flow: Option<FlowPos>,
    /// The outer loop it came from, when it moved to this session's.
    pub joined_from: Option<String>,
    /// What waited in its line when OctoBuddy last saved, in order.
    pub queued: Option<Vec<Waited>>,
    /// The message its turn was answering then (a restart cut it off).
    pub inflight: Option<Waited>,
    /// Files its tools changed that OctoBuddy had not committed.
    pub uncommitted: Option<Vec<String>>,
    /// The model its slice named (`family/model`); none: the default.
    pub model_pick: Option<String>,
    /// Its slice named its agent (`agent`): it runs there, whatever its
    /// session's engine for inner loops is or becomes.
    pub agent_named: Option<bool>,
    /// Its files as the outer loop accepted them: a later change by another
    /// loop makes that review stale.
    pub accepted: Option<Vec<FileHash>>,    /// Its slice asked for an independent reviewer.
    pub review_wanted: Option<bool>,
    /// It is the independent reviewer of this peer.
    pub reviews_for: Option<String>,
    /// The person made it (not an outer loop's plan).
    pub by_person: Option<bool>,
    /// Its Claude Code session, when it runs on Claude Code (to resume it).
    pub claude_session: Option<String>,
    /// The tasks it was given as files (an agent-spec contract or a plain
    /// brief), in order: the last is the one in force.
    pub specs: Option<Vec<SpecRef>>,
}

/// A task an inner loop was given, kept as a file.
#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct SpecRef {
    pub path: String,
    pub at: u64,
    /// The round (the person's request) it came with; `None`: sent later.
    pub round: Option<u32>,
    /// An agent-spec contract (linted and checked) or a plain brief.
    pub contract: bool,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct FileHash {
    pub path: String,
    pub hash: String,
}

/// Something that waited in OctoBuddy's memory: a message (`kind`: `lead`,
/// `person`, `reports`, `note`, and its id), or a held peer (`kind`: its
/// id, `text`: its directory).
#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct Waited {
    pub kind: String,
    pub id: Option<String>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct WaitedReport {
    pub peer: String,
    pub report: String,
    pub forwarded: bool,
}

/// What a session's loops left waiting when OctoBuddy last saved: the
/// processes do not survive a restart, this does, and OctoBuddy goes on
/// from it.
#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct Waiting {
    /// What waits for the outer loop, in order.
    pub outer: Vec<Waited>,
    /// What the outer loop was given and had not answered.
    pub outer_inflight: Vec<String>,
    /// Reports it has not read.
    pub reports: Vec<WaitedReport>,
    /// What the person and the inner loops said to each other since it last heard.
    pub notes: Vec<String>,
    /// Peers held for their wave.
    pub held: Vec<Waited>,
    /// Reviews this request may still start.
    pub rounds_left: Option<u32>,
    /// Peers with a task from it they have not reported on.
    pub pending: Vec<String>,
    /// Peers the person made for it whose first task (the person's) it has
    /// not heard back on.
    pub person_tasks: Option<Vec<String>>,
}

impl Waiting {
    pub fn is_empty(&self) -> bool {
        *self == Waiting::default()
    }
}

/// A limit on one inner loop: its steps (model calls, all its turns) and
/// what it may cost, in dollars.
#[derive(Clone, Copy, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct Budget {
    pub steps: Option<u64>,
    pub cost: Option<f64>,
}

impl Budget {
    /// Why `used` steps and `cost` dollars are past this budget, if they are.
    pub fn exceeded(&self, used: u64, cost: f64) -> Option<String> {
        if let Some(steps) = self.steps.filter(|s| used >= *s) {
            return Some(format!("{used} of {steps} steps"));
        }
        if let Some(limit) = self.cost.filter(|c| cost >= *c) {
            return Some(format!("${cost:.2} of ${limit:.2}"));
        }
        None
    }
}

/// A card's place on the flow canvas, in canvas points.
#[derive(Clone, Copy, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct FlowPos {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct Subagent {
    pub id: String,
    pub role: String,
    pub title: String,
    /// `running`, `completed`, `failed`, `interrupted`.
    pub status: String,
    pub summary: String,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cost: f64,
    pub context_window: Option<u64>,
}

/// One message to a peer and what it answered.
#[derive(Clone, Debug, SerJson, DeJson)]
pub struct Exchange {
    /// `lead` or `person`.
    pub from: String,
    pub input: String,
    pub reply: Option<String>,
    /// `completed`, `interrupted`, `failed`; none while it runs.
    pub outcome: Option<String>,
    pub at: u64,
    /// The tools the peer used answering it.
    pub steps: Option<Vec<Step>>,
    /// How long its turn took, in seconds.
    pub took: Option<u64>,
    /// What the peer had cost in all (USD) when the turn ended: a turn's own
    /// cost is the difference from the one before.
    pub cost_total: Option<f64>,
    /// When the agent took it up (its turn began): the wait before — queued,
    /// its process starting — is not its work. None: at `at`.
    pub began: Option<u64>,
}

impl Peer {
    pub fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "queued" | "running" | "checking")
    }

    pub fn role(&self) -> &str {
        self.role.as_deref().filter(|r| !r.is_empty()).unwrap_or("developer")
    }

    pub fn agent(&self) -> &str {
        self.agent.as_deref().unwrap_or("octos")
    }

    pub fn log(&self) -> &[Exchange] {
        self.log.as_deref().unwrap_or(&[])
    }

    pub fn log_mut(&mut self) -> &mut Vec<Exchange> {
        self.log.get_or_insert_with(Vec::new)
    }

    /// The exchange its running turn answers: the last one without an outcome.
    pub fn open_exchange(&mut self) -> Option<&mut Exchange> {
        self.log_mut().iter_mut().rev().find(|e| e.outcome.is_none())
    }
}

/// Fields added after the first release are `Option`s: the JSON reader
/// refuses a missing field unless it is one, and old files must still load.
#[derive(Clone, Debug, SerJson, DeJson)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub messages: Vec<Message>,
    /// The outer-loop CLI the person picked (`claude`, `codex`, `pi`).
    pub cli: Option<String>,
    /// The lead's own session id, so each message resumes the same conversation.
    pub lead_session: Option<String>,
    pub peers: Option<Vec<Peer>>,
    /// What the lead has cost in this session, in dollars.
    pub lead_cost: Option<f64>,
    /// From before one shared directory: each peer had its own clone. Read
    /// so old files load; not used.
    pub isolate: Option<bool>,
    /// Whether the session works in its own git worktree of the project
    /// (on its own branch), instead of the project directory itself.
    pub worktree: Option<bool>,
    /// Where the outer and inner loops work, fixed at the first message: the
    /// project directory, or the session's worktree.
    pub work_dir: Option<String>,
    /// The worktree's branch, when there is one.
    pub work_branch: Option<String>,
    /// The lead's model, as its process reported it.
    pub lead_model: Option<String>,
    /// The latest plan's agent-estimation, in one line.
    pub estimate: Option<String>,
    /// Its inner loops' approvals: Auto (octos asks nothing, the sandbox
    /// stays) when set, else Ask.
    pub auto: Option<bool>,
    /// Where its outer loop's card sits on the project's flow canvas.
    pub flow: Option<FlowPos>,
    /// What its loops left waiting when OctoBuddy last saved.
    pub waiting: Option<Waiting>,
    /// Data the person gave its app: what the outer loop is (to be) told.
    pub data: Option<Vec<DataNote>>,
    /// Not an outer loop: where a project keeps the peer agents the person
    /// made without one (no lead runs here; the sidebar does not list it).
    pub detached: Option<bool>,
    /// What runs its outer loop: the engine (`claude`: Claude Code; `octos`:
    /// octos on the OctoSense AI providers) and the model (none: the
    /// engine's own default).
    pub outer: Option<OuterPick>,
    /// What runs its inner loops: the engine (`octos`, the default, or
    /// `claude`: Claude Code) and the model (`family/model` of the AI
    /// providers through OctoBuddy's proxy, or a Claude Code model).
    pub inner: Option<OuterPick>,
    /// The reasoning effort picked for its outer loop and for its new inner
    /// loops (`effort_levels` of the engine; none: the agent's own setting).
    pub outer_effort: Option<String>,
    pub inner_effort: Option<String>,
    /// Kept at the top of its project's sessions.
    pub pinned: Option<bool>,
    /// Out of the list, under "Archived" (its gist saved in project memory).
    pub archived: Option<bool>,
    /// What it said before it moved here, for its agent's next message
    /// (when its own transcript could not move with it).
    pub carried: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct DataNote {
    pub name: String,
    pub summary: String,
    /// Said to the outer loop already.
    pub told: bool,
}

/// The reasoning efforts an engine takes, least first (what its CLI flag
/// accepts: Claude Code `--effort`, Codex `model_reasoning_effort`, pi
/// `--thinking`); none: it has no setting OctoBuddy passes.
pub fn effort_levels(engine: &str) -> &'static [&'static str] {
    match engine {
        "claude" => &["low", "medium", "high", "xhigh", "max"],
        "codex" => &["minimal", "low", "medium", "high", "xhigh"],
        "pi" => &["off", "minimal", "low", "medium", "high", "xhigh", "max"],
        _ => &[],
    }
}

/// An effort level, said as Cindy says it.
pub fn effort_word(level: &str) -> &'static str {
    match level {
        "off" => crate::i18n::t("off", "关"),
        "minimal" => crate::i18n::t("minimal", "最小"),
        "low" => crate::i18n::t("low", "低"),
        "medium" => crate::i18n::t("medium", "中"),
        "high" => crate::i18n::t("high", "高"),
        "xhigh" => crate::i18n::t("extra high", "超高"),
        "max" => crate::i18n::t("max", "最高"),
        _ => crate::i18n::t("default", "默认"),
    }
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct OuterPick {
    pub engine: String,
    pub model: Option<String>,
}

impl Session {
    pub fn is_pinned(&self) -> bool {
        self.pinned == Some(true)
    }

    pub fn is_archived(&self) -> bool {
        self.archived == Some(true)
    }

    /// What runs its inner loops: `octos` unless it picked Claude Code.
    pub fn inner_engine(&self) -> &str {
        self.inner.as_ref().map(|o| o.engine.as_str()).unwrap_or("octos")
    }

    pub fn inner_model(&self) -> Option<&str> {
        self.inner.as_ref().and_then(|o| o.model.as_deref())
    }

    /// Not a session with an outer loop (see `detached`).
    pub fn is_detached(&self) -> bool {
        self.detached == Some(true)
    }

    pub fn cli(&self) -> &str {
        self.cli.as_deref().unwrap_or("claude")
    }

    /// The outer loop's engine: `claude` or `octos`.
    pub fn engine(&self) -> &str {
        self.outer.as_ref().map(|o| o.engine.as_str()).unwrap_or("claude")
    }

    /// The model picked for the outer loop, if any.
    pub fn outer_model(&self) -> Option<&str> {
        self.outer.as_ref().and_then(|o| o.model.as_deref())
    }

    /// The effort picked for the outer loop, if its engine takes it.
    pub fn outer_effort(&self) -> Option<&str> {
        self.outer_effort.as_deref().filter(|e| effort_levels(self.engine()).contains(e))
    }

    /// The same for its new inner loops.
    pub fn inner_effort(&self) -> Option<&str> {
        self.inner_effort.as_deref().filter(|e| effort_levels(self.inner_engine()).contains(e))
    }

    pub fn auto(&self) -> bool {
        self.auto.unwrap_or(false)
    }

    pub fn worktree(&self) -> bool {
        self.worktree.unwrap_or(false)
    }

    pub fn peers(&self) -> &[Peer] {
        self.peers.as_deref().unwrap_or(&[])
    }

    pub fn peers_mut(&mut self) -> &mut Vec<Peer> {
        self.peers.get_or_insert_with(Vec::new)
    }

    pub fn peer_mut(&mut self, id: &str) -> Option<&mut Peer> {
        self.peers_mut().iter_mut().find(|p| p.id == id)
    }

    /// The highest round so far (0 before any plan).
    pub fn round(&self) -> u64 {
        self.peers().iter().map(|p| p.round).max().unwrap_or(0)
    }
}

#[derive(Clone, Debug, SerJson, DeJson)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// The project's directory: where the loops work.
    pub path: String,
    pub expanded: bool,
    pub sessions: Vec<Session>,
    /// `chats`: the holder of plain chats — conversations with an agent that
    /// belong to no project (each works in a folder of its own, under
    /// OctoBuddy's data). None: a project.
    pub kind: Option<String>,
    /// Kept at the top of the list.
    pub pinned: Option<bool>,
    /// Out of the list, under "Archived".
    pub archived: Option<bool>,
}

impl Project {
    pub fn is_chats(&self) -> bool {
        self.kind.as_deref() == Some("chats")
    }

    pub fn is_pinned(&self) -> bool {
        self.pinned == Some(true)
    }

    pub fn is_archived(&self) -> bool {
        self.archived == Some(true)
    }
}

#[derive(Clone, Debug, Default, SerJson, DeJson)]
pub struct Store {
    pub projects: Vec<Project>,
    /// The interface's language (`en`, `zh`); none: the system's.
    pub lang: Option<String>,
    /// The plugins the person switched off (Settings › Plugins), and those
    /// that start off that they switched on.
    pub disabled_plugins: Option<Vec<String>>,
    pub enabled_plugins: Option<Vec<String>>,
    /// The widths the person dragged the panes to (`layout.rs`), and
    /// whether the sidebar is folded away.
    pub panes: Option<Vec<PaneWidth>>,
    pub sidebar_closed: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, SerJson, DeJson)]
pub struct PaneWidth {
    pub id: String,
    pub width: f64,
}

/// A session by position: (project index, session index).
pub type SessionRef = (usize, usize);

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn new_id(prefix: &str) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{prefix}-{nanos:x}")
}

impl Store {
    /// Adds a project for `path` and returns its index. The name is the
    /// directory's last component; an existing project with the same path is
    /// reused instead of added twice.
    pub fn add_project(&mut self, path: &str) -> Option<usize> {
        let path = path.trim().trim_end_matches('/');
        if !is_project_dir(path) {
            return None;
        }
        if let Some(index) = self.projects.iter().position(|p| p.path == path) {
            return Some(index);
        }
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        self.projects.push(Project {
            id: new_id("p"),
            name,
            path: path.to_string(),
            expanded: true,
            sessions: Vec::new(),
            kind: None, pinned: None, archived: None,
        });
        Some(self.projects.len() - 1)
    }

    /// The holder of plain chats, made on first use.
    pub fn chats(&mut self) -> usize {
        if let Some(i) = self.projects.iter().position(Project::is_chats) {
            return i;
        }
        let dir = data_dir().join("chats");
        let _ = std::fs::create_dir_all(&dir);
        self.projects.push(Project {
            id: new_id("p"), name: "Chats".into(), path: dir.to_string_lossy().into_owned(),
            expanded: true, sessions: Vec::new(), kind: Some("chats".into()), pinned: None, archived: None,
        });
        self.projects.len() - 1
    }

    /// A plain chat: a session of the chats holder.
    pub fn is_plain(&self, at: SessionRef) -> bool {
        self.projects.get(at.0).is_some_and(Project::is_chats)
    }

    /// Moves a session to another project (a chat into a project, say);
    /// where it is now.
    pub fn move_session(&mut self, at: SessionRef, to: usize) -> Option<SessionRef> {
        if at.0 == to || to >= self.projects.len() {
            return None;
        }
        let session = self.projects.get_mut(at.0)?.sessions.remove(at.1);
        let p = &mut self.projects[to];
        p.sessions.push(session);
        p.expanded = true;
        Some((to, p.sessions.len() - 1))
    }

    /// Adds a session to a project and returns where it is.
    pub fn add_session(&mut self, project: usize) -> Option<SessionRef> {
        let p = self.projects.get_mut(project)?;
        let title = if p.is_chats() { format!("Chat {}", p.sessions.len() + 1) } else { format!("Session {}", p.sessions.len() + 1) };
        p.sessions.push(Session {
            id: new_id("s"), title, created_at: now_secs(), messages: Vec::new(),
            cli: None, lead_session: None, peers: None, isolate: None, worktree: None, work_dir: None, work_branch: None, lead_cost: None, lead_model: None, estimate: None, auto: None, flow: None, waiting: None, data: None, detached: None, outer: None, inner: None, outer_effort: None, inner_effort: None, pinned: None, archived: None, carried: None,
        });
        p.expanded = true;
        Some((project, p.sessions.len() - 1))
    }

    pub fn session(&self, at: SessionRef) -> Option<&Session> {
        self.projects.get(at.0)?.sessions.get(at.1)
    }

    pub fn session_mut(&mut self, at: SessionRef) -> Option<&mut Session> {
        self.projects.get_mut(at.0)?.sessions.get_mut(at.1)
    }

    /// Finds a session by id (background events name sessions by id, since
    /// positions shift when projects are added).
    pub fn find_session(&self, id: &str) -> Option<SessionRef> {
        self.projects.iter().enumerate().find_map(|(pi, p)| {
            p.sessions.iter().position(|s| s.id == id).map(|si| (pi, si))
        })
    }

    /// The session a peer belongs to, and the peer.
    pub fn find_peer(&self, id: &str) -> Option<SessionRef> {
        self.projects.iter().enumerate().find_map(|(pi, p)| {
            p.sessions.iter().position(|s| s.peers().iter().any(|peer| peer.id == id)).map(|si| (pi, si))
        })
    }

    pub fn peer_mut(&mut self, id: &str) -> Option<&mut Peer> {
        let at = self.find_peer(id)?;
        self.session_mut(at)?.peer_mut(id)
    }

    /// After a restart nothing is running any more: say so on every peer
    /// that was running, or queued after a turn. One that never began (it
    /// waits for its wave) still waits, as it says.
    pub fn mark_interrupted(&mut self) {
        for p in &mut self.projects {
            for s in &mut p.sessions {
                for peer in s.peers_mut().iter_mut().filter(|peer| peer.is_active() && !(peer.status == "queued" && peer.log().is_empty())) {
                    peer.status = "interrupted".to_string();
                    peer.activity = None;
                    if let Some(open) = peer.open_exchange() {
                        open.outcome = Some("interrupted".into());
                    }
                }
            }
        }
    }

    /// Appends a message. A session still named "Session N" takes its title
    /// from the person's first message.
    pub fn push_message(&mut self, at: SessionRef, role: Role, author: &str, text: &str) -> bool {
        let Some(session) = self.session_mut(at) else { return false };
        if role == Role::User && !session.messages.iter().any(|m| m.role() == Role::User) {
            session.title = title_from(text);
        }
        let model = (role == Role::Lead).then(|| session.outer_model().map(String::from)).flatten();
        session.messages.push(Message { role: role.as_str().to_string(), author: author.to_string(), text: text.to_string(), at: now_secs(), steps: None, meta: None, took: None, started: None, cost: None, model });
        true
    }

    /// Appends a message with its `meta`; returns its index.
    pub fn push_meta(&mut self, at: SessionRef, role: Role, author: &str, text: &str, meta: &str) -> Option<usize> {
        self.push_message(at, role, author, text);
        let session = self.session_mut(at)?;
        let last = session.messages.last_mut()?;
        last.meta = Some(meta.to_string());
        Some(session.messages.len() - 1)
    }

    /// Reads the store. A missing file is an empty store; a file that does not
    /// parse is moved aside (`state.json.corrupt-<secs>`) before starting
    /// empty, so the next save never overwrites it.
    pub fn load(path: &Path) -> Store {
        let Ok(text) = std::fs::read_to_string(path) else { return Store::default() };
        match Store::deserialize_json(&text) {
            Ok(store) => store,
            Err(err) => {
                let aside = path.with_extension(format!("json.corrupt-{}", now_secs()));
                let moved = std::fs::rename(path, &aside).is_ok();
                eprintln!("octobuddy: {} does not parse ({err:?}); {}", path.display(),
                    if moved { format!("moved to {}", aside.display()) } else { "could not move it aside".to_string() });
                Store::default()
            }
        }
    }

    /// Writes through a temporary file, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.serialize_json())?;
        std::fs::rename(tmp, path)
    }
}

/// Whether a directory may be a project: not empty, not the file-system
/// root and not the home directory, where inner loops would work on
/// everything the person owns.
pub fn is_project_dir(path: &str) -> bool {
    let path = path.trim().trim_end_matches('/');
    if path.is_empty() {
        return false;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    path != home.trim_end_matches('/')
}

fn title_from(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Session");
    let mut title: String = line.chars().take(40).collect();
    if line.chars().count() > 40 {
        title.push('…');
    }
    title
}

/// OctoBuddy's own directory: `$OCTOBUDDY_HOME`, else `~/.octobuddy`.
pub fn data_dir() -> PathBuf {
    std::env::var_os("OCTOBUDDY_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".octobuddy")))
        .unwrap_or_else(|| PathBuf::from(".octobuddy"))
}

/// Where the store lives: `<data dir>/state.json`.
pub fn default_store_path() -> PathBuf {
    data_dir().join("state.json")
}

#[cfg(test)]
mod tests {
    #[test]
    fn what_waits_is_saved_and_read_back() {
        let dir = std::env::temp_dir().join(format!("octobuddy-waiting-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("p")).unwrap();
        let mut store = Store::default();
        let pi = store.add_project(&dir.join("p").to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        let waiting = Waiting {
            outer: vec![Waited { kind: "person".into(), id: Some("q7".into()), text: "next".into() }],
            outer_inflight: vec!["INNER RESULTS".into()],
            reports: vec![WaitedReport { peer: "w1".into(), report: "done".into(), forwarded: false }],
            notes: vec!["the person asked w1 something".into()],
            held: vec![Waited { kind: "w2".into(), id: None, text: "/p".into() }],
            rounds_left: Some(2),
            pending: vec!["w1".into()],
            person_tasks: Some(vec!["w3".into()]),
        };
        store.session_mut(at).unwrap().waiting = Some(waiting.clone());
        let path = dir.join("state.json");
        store.save(&path).unwrap();
        let back = Store::load(&path);
        assert_eq!(back.session(at).unwrap().waiting.as_ref(), Some(&waiting));
        assert!(Waiting::default().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_budget_is_used_up_by_steps_or_by_cost() {
        let budget = Budget { steps: Some(10), cost: Some(0.5) };
        assert_eq!(budget.exceeded(9, 0.49), None);
        assert_eq!(budget.exceeded(10, 0.1).as_deref(), Some("10 of 10 steps"));
        assert_eq!(budget.exceeded(3, 0.5).as_deref(), Some("$0.50 of $0.50"));
        assert_eq!(Budget::default().exceeded(1000, 99.0), None);
    }

    use super::*;

    #[test]
    fn projects_sessions_and_titles() {
        let mut store = Store::default();
        let p = store.add_project("/tmp/work/demo/").unwrap();
        assert_eq!(store.projects[p].name, "demo");
        assert_eq!(store.add_project("/tmp/work/demo"), Some(p), "same path is one project");
        assert_eq!(store.add_project("  "), None);
        assert_eq!(store.add_project("/"), None, "not the root");
        assert_eq!(store.add_project(&std::env::var("HOME").unwrap()), None, "not the home directory");

        let s = store.add_session(p).unwrap();
        assert_eq!(store.session(s).unwrap().title, "Session 1");
        store.push_message(s, Role::User, "", "\n  Fix the login bug in the web app and add tests for it please\nmore");
        assert_eq!(store.session(s).unwrap().title, "Fix the login bug in the web app and add…");
        store.push_message(s, Role::User, "", "second");
        assert!(store.session(s).unwrap().title.starts_with("Fix the login"), "only the first message names it");
    }

    #[test]
    fn round_trips_through_json() {
        let dir = std::env::temp_dir().join(format!("octobuddy-test-{}", now_secs()));
        let path = dir.join("state.json");
        let mut store = Store::default();
        let p = store.add_project("/tmp/a").unwrap();
        let s = store.add_session(p).unwrap();
        store.push_message(s, Role::Lead, "claude", "plan: 2 peers");
        store.save(&path).unwrap();
        let back = Store::load(&path);
        assert_eq!(back.projects.len(), 1);
        let m = &back.session(s).unwrap().messages[0];
        assert_eq!((m.role(), m.author.as_str(), m.text.as_str()), (Role::Lead, "claude", "plan: 2 peers"));
        assert!(std::fs::read_to_string(&path).unwrap().contains(r#""role":"lead""#), "roles are plain strings");
        assert!(Store::load(&dir.join("missing.json")).projects.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_file_from_before_peers_still_loads() {
        let old = r#"{"projects":[{"id":"p1","name":"a","path":"/tmp/a","expanded":true,"sessions":[
            {"id":"s1","title":"t","created_at":1,"messages":[{"role":"user","author":"","text":"hi","at":1}]}]}]}"#;
        let store = Store::deserialize_json(old).unwrap();
        let s = &store.projects[0].sessions[0];
        assert_eq!((s.cli(), s.peers().len(), s.round(), s.worktree()), ("claude", 0, 0, false));
    }

    #[test]
    fn restart_interrupts_active_peers() {
        let mut store = Store::default();
        let p = store.add_project("/tmp/a").unwrap();
        let at = store.add_session(p).unwrap();
        for (id, status) in [("a", "running"), ("b", "done"), ("c", "queued")] {
            store.session_mut(at).unwrap().peers_mut().push(Peer {
                id: id.into(), slug: id.into(), role: None, agent: None, brief: String::new(), status: status.into(), dir: String::new(),
                branch: None, round: 1, started_at: 0, finished_at: None, activity: Some("x".into()), result: None,
                session_key: None, log: None, contract: None, usage: None, check: None, verdict: None, review: None, landed: None, model: None, effort: None, touched: None, base: None, commits: None, subagents: None, estimate: None, wave: None, rounds_used: None, budget: None, over_budget: None, flow: None, joined_from: None, queued: None, inflight: None, uncommitted: None, model_pick: None, agent_named: None, accepted: None, review_wanted: None, reviews_for: None, by_person: None, claude_session: None, specs: None,
            });
        }
        // A queued one that had a turn already was cut off too.
        let mut d = store.session(at).unwrap().peers()[2].clone();
        d.id = "d".into();
        d.log_mut().push(Exchange { from: "lead".into(), input: "x".into(), reply: None, outcome: None, at: 0, steps: None, took: None, cost_total: None, began: None });
        store.session_mut(at).unwrap().peers_mut().push(d);
        store.mark_interrupted();
        let statuses: Vec<_> = store.session(at).unwrap().peers().iter().map(|p| p.status.as_str()).collect();
        assert_eq!(statuses, ["interrupted", "done", "queued", "interrupted"], "one waiting for its wave still waits");
        assert_eq!(store.session(at).unwrap().peers()[2].activity.as_deref(), Some("x"), "and says so");
        assert_eq!(store.find_session(&store.session(at).unwrap().id.clone()), Some(at));
    }

    #[test]
    fn a_file_that_does_not_parse_is_kept() {
        let dir = std::env::temp_dir().join(format!("octobuddy-bad-{}", std::process::id()));
        let path = dir.join("state.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert!(Store::load(&path).projects.is_empty());
        assert!(!path.exists(), "the bad file is moved aside");
        let kept: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("state.json.corrupt-")).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read_to_string(kept[0].path()).unwrap(), "{not json");
        let _ = std::fs::remove_dir_all(dir);
    }
}
