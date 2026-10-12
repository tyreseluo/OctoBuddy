//! The lifecycle view, derived from what the store keeps.
//!
//! Two views share one job: read the conversations and the per-peer log,
//! and draw what they did, not a second record of it.
//!
//! - The state machine: each slice's path through the lifecycle, the round's
//!   path, and how many of each edge the session saw.
//! - The doc stream: every document the project produced, in time order,
//!   grouped by round, with versions for the ones that came back changed.
//!
//! LIVING.md is just the doc stream as markdown: a project story from its
//! first request to now, with the open ends marked.
use crate::model::{Gate, Peer, Role, Session, Waiting};
use std::collections::BTreeMap;

/// Where one slice is in the lifecycle.
///
/// The happy path runs Waiting -> Running -> Reported -> Checked -> Accepted
/// -> Landed, with SentBack as a loop back to Running. Side exits are
/// Failed, Stopped, OverBudget. Closed without an accept is Discarded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SliceState {
    Waiting,
    Running,
    Reported,
    Checked,
    Accepted,
    SentBack,
    Landed,
    Discarded,
    Failed,
    Stopped,
    OverBudget,
}

impl SliceState {
    pub fn label(self) -> &'static str {
        match self {
            SliceState::Waiting => "Waiting",
            SliceState::Running => "Running",
            SliceState::Reported => "Reported",
            SliceState::Checked => "Checked",
            SliceState::Accepted => "Accepted",
            SliceState::SentBack => "SentBack",
            SliceState::Landed => "Landed",
            SliceState::Discarded => "Discarded",
            SliceState::Failed => "Failed",
            SliceState::Stopped => "Stopped",
            SliceState::OverBudget => "OverBudget",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, SliceState::Landed | SliceState::Discarded | SliceState::Failed | SliceState::Stopped | SliceState::OverBudget)
    }
}

/// Where the round is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RoundState {
    Request,
    Planned,
    WavesRunning,
    AllAccepted,
    AcceptanceSheet,
    Confirmed,
    Published,
}

impl RoundState {
    pub fn label(self) -> &'static str {
        match self {
            RoundState::Request => "Request",
            RoundState::Planned => "Planned",
            RoundState::WavesRunning => "WavesRunning",
            RoundState::AllAccepted => "AllAccepted",
            RoundState::AcceptanceSheet => "AcceptanceSheet",
            RoundState::Confirmed => "Confirmed",
            RoundState::Published => "Published",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, RoundState::Confirmed | RoundState::Published)
    }
}

/// One step a slice (or a round) took: what state it moved to, and when.
#[derive(Clone, Debug)]
pub struct Transition {
    pub to: TransitionKind,
    pub at: u64,
    pub note: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransitionKind {
    // slice
    Waiting,
    Running,
    Reported,
    Checked,
    Accepted,
    SentBack,
    Landed,
    Discarded,
    Failed,
    Stopped,
    OverBudget,
    // round
    Request,
    Planned,
    WavesRunning,
    AllAccepted,
    AcceptanceSheet,
    Confirmed,
    Published,
}

impl TransitionKind {
    pub fn from_slice(s: SliceState) -> Self {
        match s {
            SliceState::Waiting => TransitionKind::Waiting,
            SliceState::Running => TransitionKind::Running,
            SliceState::Reported => TransitionKind::Reported,
            SliceState::Checked => TransitionKind::Checked,
            SliceState::Accepted => TransitionKind::Accepted,
            SliceState::SentBack => TransitionKind::SentBack,
            SliceState::Landed => TransitionKind::Landed,
            SliceState::Discarded => TransitionKind::Discarded,
            SliceState::Failed => TransitionKind::Failed,
            SliceState::Stopped => TransitionKind::Stopped,
            SliceState::OverBudget => TransitionKind::OverBudget,
        }
    }

    pub fn from_round(s: RoundState) -> Self {
        match s {
            RoundState::Request => TransitionKind::Request,
            RoundState::Planned => TransitionKind::Planned,
            RoundState::WavesRunning => TransitionKind::WavesRunning,
            RoundState::AllAccepted => TransitionKind::AllAccepted,
            RoundState::AcceptanceSheet => TransitionKind::AcceptanceSheet,
            RoundState::Confirmed => TransitionKind::Confirmed,
            RoundState::Published => TransitionKind::Published,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TransitionKind::Waiting => "Waiting",
            TransitionKind::Running => "Running",
            TransitionKind::Reported => "Reported",
            TransitionKind::Checked => "Checked",
            TransitionKind::Accepted => "Accepted",
            TransitionKind::SentBack => "SentBack",
            TransitionKind::Landed => "Landed",
            TransitionKind::Discarded => "Discarded",
            TransitionKind::Failed => "Failed",
            TransitionKind::Stopped => "Stopped",
            TransitionKind::OverBudget => "OverBudget",
            TransitionKind::Request => "Request",
            TransitionKind::Planned => "Planned",
            TransitionKind::WavesRunning => "WavesRunning",
            TransitionKind::AllAccepted => "AllAccepted",
            TransitionKind::AcceptanceSheet => "AcceptanceSheet",
            TransitionKind::Confirmed => "Confirmed",
            TransitionKind::Published => "Published",
        }
    }
}

/// How many times the session crossed an edge (slice or round), for the
/// graph view.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EdgeCount<K: Ord> {
    pub from: K,
    pub to: K,
    pub count: usize,
}

/// A slice's path through the lifecycle, with the steps in time order.
#[derive(Clone, Debug)]
pub struct SlicePath {
    pub peer_id: String,
    pub slug: String,
    pub round: u64,
    pub wave: u32,
    pub transitions: Vec<Transition>,
    pub current: SliceState,
    /// How many times the lead sent this slice back: the rework count.
    pub rework_count: usize,
}

/// The slice state machine for one session.
#[derive(Clone, Debug, Default)]
pub struct SliceMachine {
    pub paths: Vec<SlicePath>,
    /// Counts of (from, to) edges across all the paths.
    pub edges: Vec<EdgeCount<TransitionKind>>,
}

impl SliceMachine {
    /// How many slices are currently at `state`.
    pub fn count_at(&self, state: SliceState) -> usize {
        self.paths.iter().filter(|p| p.current == state).count()
    }
}

/// A round's path through the lifecycle.
#[derive(Clone, Debug)]
pub struct RoundPath {
    pub round: u64,
    pub transitions: Vec<Transition>,
    pub current: RoundState,
}

/// The round state machine for one session.
#[derive(Clone, Debug, Default)]
pub struct RoundMachine {
    pub rounds: Vec<RoundPath>,
    pub edges: Vec<EdgeCount<TransitionKind>>,
}

/// What kind of document an item in the doc stream is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DocKind {
    /// The person's message that opened the round.
    Request,
    /// The outer loop's plan for the round.
    Plan,
    /// A slice's task file (the brief or the agent-spec contract).
    Spec,
    /// An inner loop's reply.
    Report,
    /// OctoBuddy's check on a slice's work.
    Gate,
    /// The outer loop's accept / fix verdict.
    Review,
    /// A repo document a slice touched in a commit (`*.md`, `docs/**`).
    RepoDoc,
    /// An acceptance sheet.
    Acceptance,
    /// A publish / submit receipt.
    Receipt,
}

impl DocKind {
    pub fn label(self) -> &'static str {
        match self {
            DocKind::Request => "Request",
            DocKind::Plan => "Plan",
            DocKind::Spec => "Spec",
            DocKind::Report => "Report",
            DocKind::Gate => "Gate",
            DocKind::Review => "Review",
            DocKind::RepoDoc => "RepoDoc",
            DocKind::Acceptance => "Acceptance",
            DocKind::Receipt => "Receipt",
        }
    }
}

/// One document the project produced, with where it came from and when.
#[derive(Clone, Debug)]
pub struct DocItem {
    /// A stable id for the view to point at: `<session>/<kind>/<...>`.
    pub id: String,
    pub session: String,
    pub round: Option<u64>,
    pub kind: DocKind,
    pub title: String,
    /// Who produced it: the person's name, `outer`, the peer slug, or
    /// `OctoBuddy`. A repo doc also has the commit author in `version`.
    pub author: String,
    pub at: u64,
    /// For a repo doc, the acceptance sheet, or a receipt: its path.
    pub path: Option<String>,
    /// The version label for a repo doc that came back changed (`v1`, `v2`).
    pub version: Option<u32>,
    /// The commit hash for a repo doc.
    pub commit: Option<String>,
    /// The slice it came from, when there is one.
    pub peer: Option<String>,
    /// The text, when it is already on hand (request, report, gate, review).
    pub body: Option<String>,
}

/// A project's doc stream, in time order.
#[derive(Clone, Debug, Default)]
pub struct DocStream {
    pub items: Vec<DocItem>,
    /// Repo doc paths, with their version numbers, in version order.
    pub path_versions: BTreeMap<String, Vec<u32>>,
}

impl DocStream {
    /// Items matching `query` (case-insensitive on title, author, path) and
    /// the `kind` filter, capped at `limit`.
    pub fn search(&self, query: Option<&str>, kind: Option<DocKind>, limit: usize) -> Vec<&DocItem> {
        let q = query.map(|q| q.to_ascii_lowercase());
        self.items.iter()
            .filter(|i| kind.is_none_or(|k| i.kind == k))
            .filter(|i| q.as_ref().is_none_or(|q| {
                i.title.to_ascii_lowercase().contains(q)
                    || i.author.to_ascii_lowercase().contains(q)
                    || i.path.as_deref().is_some_and(|p| p.to_ascii_lowercase().contains(q))
            }))
            .take(limit)
            .collect()
    }
}

/// Find the first line of `text` with at most `n` characters.
fn first_line(text: &str, n: usize) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(n).collect()
}

/// What `p` is doing — its slice state — derived from its log and gates.
fn slice_state(p: &Peer, all_peers: &[&Peer], waiting: Option<&Waiting>) -> SliceState {
    if p.over_budget.is_some() {
        return SliceState::OverBudget;
    }
    if p.status == "failed" {
        return SliceState::Failed;
    }
    if p.status == "interrupted" || p.status == "stopped" {
        return SliceState::Stopped;
    }
    if p.landed.is_some() {
        return SliceState::Landed;
    }
    if p.status == "closed" {
        return if accepted(p) { SliceState::Accepted } else { SliceState::Discarded };
    }
    if let Some(r) = p.review.as_deref() {
        if r.starts_with("accept") {
            return SliceState::Accepted;
        }
        if r.starts_with("fix") {
            return SliceState::SentBack;
        }
    }
    if p.gate.is_some() {
        return SliceState::Checked;
    }
    // Held for its wave wins over active: a queued slice waiting on an
    // earlier wave is Waiting, not Running.
    if is_held_for_wave(p, all_peers, waiting) {
        return SliceState::Waiting;
    }
    // A reply that finished: its report is in. An entry without a reply is
    // still running (the agent is on it now).
    if let Some(last) = p.log().last() {
        if last.reply.is_some() && last.outcome.is_some() {
            return SliceState::Reported;
        }
    }
    if p.is_active() {
        return SliceState::Running;
    }
    if p.status == "queued" && p.log().is_empty() {
        return SliceState::Waiting;
    }
    SliceState::Running
}

fn accepted(p: &Peer) -> bool {
    p.review.as_deref().is_some_and(|r| r.starts_with("accept"))
}

/// The slice is held because its wave is not yet unblocked: at least one
/// peer of an earlier wave in its round is not yet accepted.
fn is_held_for_wave(p: &Peer, all_peers: &[&Peer], waiting: Option<&Waiting>) -> bool {
    let wave = p.wave.unwrap_or(1);
    if wave <= 1 {
        return false;
    }
    let earlier = all_peers.iter().filter(|o| o.round == p.round && o.wave.unwrap_or(1) < wave);
    let earlier_done = earlier.clone().all(|p| accepted(p));
    if earlier_done {
        return false;
    }
    // The held list is the source of truth when OctoBuddy saved: a peer
    // named in `waiting.held` is being held for its wave.
    if let Some(w) = waiting {
        if w.held.iter().any(|h| h.kind == p.id) {
            return true;
        }
    }
    true
}

/// The end-of-turn time for `peer`'s last work (or its `started_at` when
/// it never began). A coarse clock for transition timestamps.
fn last_work_at(peer: &Peer) -> u64 {
    peer.log().iter().filter_map(|e| e.began).max()
        .or_else(|| peer.log().last().map(|e| e.at))
        .unwrap_or(peer.started_at)
}

/// Build a slice's transitions: the ordered list of states it passed
/// through, deduped (no two Waiting -> Waiting).
fn slice_path(peer: &Peer, all_peers: &[&Peer], waiting: Option<&Waiting>) -> SlicePath {
    let log = peer.log();
    let rework_count = count_rework(peer);
    let mut transitions: Vec<Transition> = Vec::new();
    push_transition(&mut transitions, TransitionKind::Waiting, 0, "init");

    // The slice was made at started_at (or first log entry if later).
    let started = if log.is_empty() { peer.started_at } else { log[0].at.min(peer.started_at.max(1)) };
    if is_held_for_wave(peer, all_peers, waiting) {
        push_transition(&mut transitions, TransitionKind::Waiting, started, "held for an earlier wave");
    } else {
        push_transition(&mut transitions, TransitionKind::Running, started, "queued or first turn");
    }

    // Each log entry moves Running -> Reported (the agent wrote a reply).
    for e in log {
        let running_at = e.began.unwrap_or(e.at);
        let last_kind = transitions.last().map(|t| t.to);
        if last_kind != Some(TransitionKind::Running) {
            push_transition(&mut transitions, TransitionKind::Running, running_at, &first_line(&e.input, 60));
        }
        if e.reply.is_some() && e.outcome.is_some() {
            let reply_at = e.at + e.took.unwrap_or(0);
            push_transition(&mut transitions, TransitionKind::Reported, reply_at, &first_line(e.reply.as_deref().unwrap_or(""), 60));
        }
    }

    // The gate: checked at the end of the slice's last work.
    if peer.gate.is_some() {
        let at = last_work_at(peer);
        push_transition(&mut transitions, TransitionKind::Checked, at, &gate_note(peer.gate.as_ref()));
    }

    // The review: accept (or fix, which loops back).
    if let Some(r) = peer.review.as_deref() {
        let at = last_work_at(peer);
        if r.starts_with("fix") {
            push_transition(&mut transitions, TransitionKind::SentBack, at, &first_line(r, 80));
            // The loop back to running: count it once.
            push_transition(&mut transitions, TransitionKind::Running, at, "rerun");
        } else if r.starts_with("accept") {
            push_transition(&mut transitions, TransitionKind::Accepted, at, &first_line(r, 80));
        }
    }

    // Landed / closed.
    if peer.landed.is_some() {
        let at = last_work_at(peer);
        push_transition(&mut transitions, TransitionKind::Landed, at, peer.landed.as_deref().unwrap_or(""));
    }
    if peer.status == "closed" && !accepted(peer) {
        let at = last_work_at(peer);
        push_transition(&mut transitions, TransitionKind::Discarded, at, "closed without an accept");
    }

    // Side exits last, so they win.
    if peer.over_budget.is_some() {
        let at = last_work_at(peer);
        push_transition(&mut transitions, TransitionKind::OverBudget, at, peer.over_budget.as_deref().unwrap_or(""));
    }
    if peer.status == "failed" {
        let at = last_work_at(peer);
        push_transition(&mut transitions, TransitionKind::Failed, at, "turn failed");
    }
    if peer.status == "interrupted" || peer.status == "stopped" {
        let at = last_work_at(peer);
        push_transition(&mut transitions, TransitionKind::Stopped, at, &peer.status);
    }

    let current = slice_state(peer, all_peers, waiting);
    // Make sure the last transition matches the current state (a slice whose
    // current is computed but no transition was pushed yet gets one).
    let last_kind = transitions.last().map(|t| t.to);
    if last_kind != Some(TransitionKind::from_slice(current)) {
        transitions.push(Transition { to: TransitionKind::from_slice(current), at: last_work_at(peer), note: String::new() });
    }

    // Drop the placeholder "init" transition if it is not also the current state.
    if transitions.first().is_some_and(|t| t.to == TransitionKind::Waiting && t.note == "init") {
        let current_kind = transitions.last().map(|t| t.to);
        if current_kind != Some(TransitionKind::Waiting) {
            transitions.remove(0);
        }
    }

    SlicePath {
        peer_id: peer.id.clone(),
        slug: peer.slug.clone(),
        round: peer.round,
        wave: peer.wave.unwrap_or(1),
        transitions,
        current,
        rework_count,
    }
}

/// Push a transition unless the last one is already the same kind.
fn push_transition(out: &mut Vec<Transition>, kind: TransitionKind, at: u64, note: &str) {
    if out.last().is_some_and(|t| t.to == kind) {
        return;
    }
    out.push(Transition { to: kind, at, note: note.to_string() });
}

/// Count the sent-back times: each `review: fix:` (the lead reworked it).
/// The log entries themselves carry the why in their input.
fn count_rework(p: &Peer) -> usize {
    let mut n = 0;
    for r in p.review.as_deref().map(|s| s.split('\n')).into_iter().flatten() {
        if r.starts_with("fix") {
            n += 1;
        }
    }
    if n == 0 {
        // Fall back to log-level signal: each fix message from the lead.
        n = p.log().iter().filter(|e| e.from == "lead" && super::timeline::sent_back(&e.input)).count();
    }
    n
}

fn gate_note(g: Option<&Gate>) -> String {
    let Some(g) = g else { return String::new() };
    let mut bits: Vec<String> = Vec::new();
    match g.check {
        Some(true) => bits.push("check passed".into()),
        Some(false) => bits.push("check failed".into()),
        None => {}
    }
    if !g.claims.as_deref().unwrap_or(&[]).is_empty() {
        bits.push("claims contradicted".into());
    }
    if !g.stray.as_deref().unwrap_or(&[]).is_empty() {
        bits.push("stray files".into());
    }
    if bits.is_empty() { "no findings".into() } else { bits.join(", ") }
}

/// All the edges a session's slice paths crossed, counted.
fn count_edges(paths: &[SlicePath]) -> Vec<EdgeCount<TransitionKind>> {
    let mut map: BTreeMap<(TransitionKind, TransitionKind), usize> = BTreeMap::new();
    for p in paths {
        for w in p.transitions.windows(2) {
            *map.entry((w[0].to, w[1].to)).or_default() += 1;
        }
    }
    map.into_iter().map(|((from, to), count)| EdgeCount { from, to, count }).collect()
}

/// Build the slice machine for one session.
pub fn build_slice_machine(session: &Session) -> SliceMachine {
    let peers: Vec<&Peer> = session.peers().iter().collect();
    let waiting = session.waiting.as_ref();
    let mut paths: Vec<SlicePath> = peers.iter().map(|p| slice_path(p, &peers, waiting)).collect();
    paths.sort_by(|a, b| a.slug.cmp(&b.slug));
    let edges = count_edges(&paths);
    SliceMachine { paths, edges }
}

/// A round's transitions, derived from messages and peers.
fn round_path(round: u64, session: &Session) -> RoundPath {
    let mut transitions: Vec<Transition> = Vec::new();

    // The user's request that opened this round.
    let request = session.messages.iter().filter(|m| m.role() == Role::User).find(|m| {
        // The round number a peer of this round was started at: pick the
        // earliest user message before any peer of this round began.
        let peer_first = session.peers().iter().filter(|p| p.round == round).filter_map(|p| p.log().first().map(|e| e.at)).min();
        match peer_first {
            Some(at) => m.at <= at + 60,
            None => false,
        }
    });
    let req_at = request.map(|m| m.at).unwrap_or_else(|| {
        session.peers().iter().filter(|p| p.round == round).filter_map(|p| p.log().first().map(|e| e.at)).min().unwrap_or(0)
    });
    let req_text = request.map(|m| m.text.as_str()).unwrap_or("");
    push_transition(&mut transitions, TransitionKind::Request, req_at, &first_line(req_text, 80));

    // Planned: the first peer's log entry of this round.
    let planned_at = session.peers().iter().filter(|p| p.round == round).filter_map(|p| p.log().first().map(|e| e.began.unwrap_or(e.at))).min();
    if let Some(at) = planned_at {
        push_transition(&mut transitions, TransitionKind::Planned, at, "slices started");
    }

    // Waves running: any peer of this round not yet accepted (and not discarded).
    let mut all_done = planned_at.is_some();
    let mut any_running = false;
    let mut last_done_at = 0u64;
    for p in session.peers().iter().filter(|p| p.round == round) {
        if accepted(p) || p.landed.is_some() {
            last_done_at = last_done_at.max(last_work_at(p));
        } else if p.status == "closed" {
            // discarded: still "done" for the round's purposes
            last_done_at = last_done_at.max(last_work_at(p));
        } else {
            all_done = false;
            any_running = true;
        }
    }
    if any_running {
        push_transition(&mut transitions, TransitionKind::WavesRunning, planned_at.unwrap_or(req_at), "still running");
    }
    if all_done && planned_at.is_some() {
        push_transition(&mut transitions, TransitionKind::AllAccepted, last_done_at.max(req_at), "every slice accepted or closed");
    }

    // Acceptance sheet: derived — see `acceptances.rs`. We only know from a
    // system message that the round had a sheet if the orchestrator emits one.
    // The lifecycle view treats it as present when there is a follow-up user
    // confirmation message that mentions "confirmed" or `confirm acceptance`.
    let sheet_at = session.messages.iter().find(|m| {
        m.role() == Role::System && (m.text.contains(&format!("acceptance sheet for round {round}")) || m.text.contains(&format!("第 {round} 轮的验收单")))
    }).map(|m| m.at);
    if let Some(at) = sheet_at {
        push_transition(&mut transitions, TransitionKind::AcceptanceSheet, at, "sheet written");
    }

    // Confirmed: a later user message saying so.
    let confirmed_at = session.messages.iter().filter(|m| m.role() == Role::User)
        .find(|m| m.text.contains("Confirm acceptance") || m.text.contains("确认验收") || m.text.contains("confirmed the acceptance") || m.text.contains("已确认验收"));
    if let Some(m) = confirmed_at {
        push_transition(&mut transitions, TransitionKind::Confirmed, m.at, "person confirmed");
    }

    // Published: a system message that names a version that was published.
    let published_at = session.messages.iter().find(|m| {
        m.role() == Role::System && (m.text.starts_with("Published ") || m.text.starts_with("已发布 ") || m.text.contains("submitted to App Hub") || m.text.contains("已提交到 App Hub"))
    }).map(|m| m.at);
    if let Some(at) = published_at {
        push_transition(&mut transitions, TransitionKind::Published, at, "publish / submit");
    }

    let current = transitions.last().map(|t| match t.to {
        TransitionKind::Request => RoundState::Request,
        TransitionKind::Planned => RoundState::Planned,
        TransitionKind::WavesRunning => RoundState::WavesRunning,
        TransitionKind::AllAccepted => RoundState::AllAccepted,
        TransitionKind::AcceptanceSheet => RoundState::AcceptanceSheet,
        TransitionKind::Confirmed => RoundState::Confirmed,
        TransitionKind::Published => RoundState::Published,
        _ => RoundState::Request,
    }).unwrap_or(RoundState::Request);

    RoundPath { round, transitions, current }
}

/// Build the round state machine for one session.
pub fn build_round_machine(session: &Session) -> RoundMachine {
    let max_round = session.peers().iter().map(|p| p.round).max().unwrap_or(0);
    let mut rounds: Vec<RoundPath> = (1..=max_round).map(|r| round_path(r, session)).collect();
    rounds.sort_by_key(|r| r.round);
    let mut edges_map: BTreeMap<(TransitionKind, TransitionKind), usize> = BTreeMap::new();
    for r in &rounds {
        for w in r.transitions.windows(2) {
            *edges_map.entry((w[0].to, w[1].to)).or_default() += 1;
        }
    }
    let edges = edges_map.into_iter().map(|((from, to), count)| EdgeCount { from, to, count }).collect();
    RoundMachine { rounds, edges }
}

/// Build the doc stream for one project (every session in it).
///
/// `repo_docs` is what a project's git history says: for each path that
/// came back changed (only `*.md` and `docs/**` here), the commit hashes
/// that touched it, oldest first. The view uses it to label each version
/// of a repo doc.
pub fn build_doc_stream(sessions: &[&Session], repo_docs: &BTreeMap<String, Vec<String>>) -> DocStream {
    let mut stream = DocStream::default();
    for s in sessions {
        collect_session_docs(s, &mut stream);
    }
    // Repo docs: each commit produces one item, with the version label.
    for (path, commits) in repo_docs {
        for (i, commit) in commits.iter().enumerate() {
            stream.items.push(DocItem {
                id: format!("repo/{path}/v{}", i + 1),
                session: String::new(),
                round: None,
                kind: DocKind::RepoDoc,
                title: path.clone(),
                author: "OctoBuddy".into(),
                at: 0,
                path: Some(path.clone()),
                version: Some((i + 1) as u32),
                commit: Some(commit.clone()),
                peer: None,
                body: None,
            });
        }
        stream.path_versions.insert(path.clone(), (1..=commits.len() as u32).collect());
    }
    stream.items.sort_by_key(|i| (i.at, i.id.clone()));
    stream
}

/// Collect every doc item from one session.
fn collect_session_docs(s: &Session, out: &mut DocStream) {
    // The user's requests, by round: the round a peer of that round began
    // at is the same round the request opened (the lead plans on the user's
    // request).
    let mut request_for_round: BTreeMap<u64, (u64, String)> = BTreeMap::new();
    for m in s.messages.iter().filter(|m| m.role() == Role::User) {
        // The round this request opened: the round of the first peer whose
        // first log entry is at or after this message (within a small window).
        let r = s.peers().iter().filter_map(|p| {
            let first = p.log().first().map(|e| e.began.unwrap_or(e.at))?;
            if first + 60 >= m.at { Some((p.round, first)) } else { None }
        }).min_by_key(|(_, t)| *t);
        if let Some((round, _)) = r {
            request_for_round.entry(round).or_insert((m.at, m.text.clone()));
        }
    }

    for (round, (at, text)) in &request_for_round {
        out.items.push(DocItem {
            id: format!("{}/request/{}", s.id, round),
            session: s.id.clone(),
            round: Some(*round),
            kind: DocKind::Request,
            title: first_line(text, 80),
            author: "person".into(),
            at: *at,
            path: None,
            version: None,
            commit: None,
            peer: None,
            body: Some(text.clone()),
        });
    }

    // The outer loop's plan for each round: a lead message with the slice list.
    for m in s.messages.iter().filter(|m| m.role() == Role::Lead) {
        // The plan: a system or lead message that lists slices by slug.
        let round = s.peers().iter().filter(|p| {
            let first = p.log().first().map(|e| e.began.unwrap_or(e.at));
            first.is_some_and(|t| (t as i64 - m.at as i64).abs() < 600)
        }).map(|p| p.round).min();
        if let Some(round) = round {
            if m.text.contains("octobuddy-plan") || m.text.contains("Slice") || m.text.lines().any(|l| l.starts_with("- ") || l.starts_with("* ")) {
                out.items.push(DocItem {
                    id: format!("{}/plan/{}", s.id, m.at),
                    session: s.id.clone(),
                    round: Some(round),
                    kind: DocKind::Plan,
                    title: first_line(&m.text, 80),
                    author: "outer".into(),
                    at: m.at,
                    path: None,
                    version: None,
                    commit: None,
                    peer: None,
                    body: Some(m.text.clone()),
                });
            }
        }
    }

    // Specs (the slice's brief files), reports, gates, reviews, commits.
    for p in s.peers() {
        for spec in p.specs.as_deref().unwrap_or(&[]) {
            out.items.push(DocItem {
                id: format!("{}/spec/{}/{}", s.id, p.slug, spec.at),
                session: s.id.clone(),
                round: Some(p.round),
                kind: DocKind::Spec,
                title: format!("{} · {}", p.slug, spec.path.rsplit('/').next().unwrap_or(&spec.path)),
                author: "outer".into(),
                at: spec.at,
                path: Some(spec.path.clone()),
                version: None,
                commit: None,
                peer: Some(p.id.clone()),
                body: None,
            });
        }

        for e in p.log() {
            if let Some(reply) = e.reply.as_ref() {
                if e.outcome.is_some() {
                    out.items.push(DocItem {
                        id: format!("{}/report/{}/{}", s.id, p.slug, e.at),
                        session: s.id.clone(),
                        round: Some(p.round),
                        kind: DocKind::Report,
                        title: format!("{} · report", p.slug),
                        author: p.slug.clone(),
                        at: e.at,
                        path: None,
                        version: None,
                        commit: None,
                        peer: Some(p.id.clone()),
                        body: Some(reply.clone()),
                    });
                }
            }
        }

        if let Some(g) = p.gate.as_ref() {
            out.items.push(DocItem {
                id: format!("{}/gate/{}/{}", s.id, p.slug, last_work_at(p)),
                session: s.id.clone(),
                round: Some(p.round),
                kind: DocKind::Gate,
                title: format!("{} · {}", p.slug, gate_note(Some(g))),
                author: "OctoBuddy".into(),
                at: last_work_at(p),
                path: None,
                version: None,
                commit: g.at.clone(),
                peer: Some(p.id.clone()),
                body: Some(format!("{:?}", g)),
            });
        }

        if let Some(r) = p.review.as_deref() {
            out.items.push(DocItem {
                id: format!("{}/review/{}/{}", s.id, p.slug, last_work_at(p)),
                session: s.id.clone(),
                round: Some(p.round),
                kind: DocKind::Review,
                title: format!("{} · {}", p.slug, first_line(r, 60)),
                author: "outer".into(),
                at: last_work_at(p),
                path: None,
                version: None,
                commit: None,
                peer: Some(p.id.clone()),
                body: Some(r.to_string()),
            });
        }

        // Commits of repo docs: the commit list says what the slice touched.
        // We do not have the per-commit file list here; the caller layers
        // git's `show --name-status` on top in `octobuddy_docs`.
        for c in p.commits.as_deref().unwrap_or(&[]) {
            let hash = c.split_whitespace().next().unwrap_or("").to_string();
            if hash.is_empty() { continue; }
            // The path is unknown without git; the view adds it from `show`.
            out.items.push(DocItem {
                id: format!("{}/commit/{}/{}", s.id, p.slug, hash),
                session: s.id.clone(),
                round: Some(p.round),
                kind: DocKind::RepoDoc,
                title: format!("{} · {}", p.slug, first_line(c, 60)),
                author: "OctoBuddy".into(),
                at: last_work_at(p),
                path: None,
                version: None,
                commit: Some(hash),
                peer: Some(p.id.clone()),
                body: None,
            });
        }
    }

    // Acceptance sheets: a system message that says one was written.
    for m in s.messages.iter().filter(|m| m.role() == Role::System) {
        if m.text.contains("acceptance sheet") || m.text.contains("验收单") {
            let sheet = format!("{}/acceptance/{}-r1.md", project_path_for(s), s.id);
            out.items.push(DocItem {
                id: format!("{}/acceptance/{}", s.id, m.at),
                session: s.id.clone(),
                round: None,
                kind: DocKind::Acceptance,
                title: "Acceptance sheet".into(),
                author: "OctoBuddy".into(),
                at: m.at,
                path: Some(sheet),
                version: None,
                commit: None,
                peer: None,
                body: None,
            });
        }
    }
}

/// Look up the project path for a session: the store keeps it, but we
/// don't have it here. The caller passes the path through `with_project`.
fn project_path_for(_s: &Session) -> &'static str { "." }

/// Build LIVING.md: a markdown story of the project so far. Returns the
/// full document text; the caller decides whether to write it (compare
/// with the existing file before overwriting).
pub fn build_living(project_name: &str, slices: &SliceMachine, rounds: &RoundMachine, docs: &DocStream) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {} · LIVING\n\n", project_name));
    out.push_str("_Auto-written from the conversations and the commits. Re-generated when a slice is accepted, an acceptance sheet is confirmed, or the doc stream opens._\n\n");

    // Per round: a section with what was asked, what was sliced, how it went.
    for r in &rounds.rounds {
        out.push_str(&format!("## Round {}\n\n", r.round));
        let req = docs.items.iter().find(|i| i.kind == DocKind::Request && i.round == Some(r.round));
        if let Some(req) = req {
            out.push_str(&format!("**Asked:** {}\n\n", first_line(&req.title, 200)));
        }
        // Slices of this round.
        let slices_here: Vec<&SlicePath> = slices.paths.iter().filter(|p| p.round == r.round).collect();
        if !slices_here.is_empty() {
            out.push_str("**Slices:**\n\n");
            for p in &slices_here {
                let rework = if p.rework_count > 0 { format!(" (rework ×{})", p.rework_count) } else { String::new() };
                out.push_str(&format!("- `{}` → {}{}\n", p.slug, p.current.label(), rework));
            }
            out.push('\n');
        }
        // Round's own state.
        out.push_str(&format!("**State:** {}\n\n", r.current.label()));
        // Acceptance.
        if r.current == RoundState::Confirmed || r.current == RoundState::Published {
            out.push_str("**Outcome:** accepted");
            if r.current == RoundState::Published { out.push_str(" and published"); }
            out.push_str(".\n\n");
        }
        // Docs of this round: links by path.
        let round_docs: Vec<&DocItem> = docs.items.iter().filter(|i| i.round == Some(r.round)).collect();
        if !round_docs.is_empty() {
            out.push_str("**Documents:**\n\n");
            for d in &round_docs {
                if let Some(path) = &d.path {
                    let ver = d.version.map(|v| format!(" v{v}")).unwrap_or_default();
                    out.push_str(&format!("- {} — `{}`{}\n", d.kind.label(), path, ver));
                }
            }
            out.push('\n');
        }
    }

    // Open ends.
    let open_slices: Vec<&SlicePath> = slices.paths.iter().filter(|p| !p.current.is_terminal() && p.current != SliceState::Waiting).collect();
    let waiting_slices: Vec<&SlicePath> = slices.paths.iter().filter(|p| p.current == SliceState::Waiting).collect();
    let unconfirmed_rounds: Vec<&RoundPath> = rounds.rounds.iter().filter(|r| !matches!(r.current, RoundState::Confirmed | RoundState::Published)).collect();

    if !open_slices.is_empty() || !waiting_slices.is_empty() || !unconfirmed_rounds.is_empty() {
        out.push_str("## Open\n\n");
        for p in &open_slices {
            out.push_str(&format!("- `{}` is {} now\n", p.slug, p.current.label()));
        }
        for p in &waiting_slices {
            out.push_str(&format!("- `{}` is held for its wave\n", p.slug));
        }
        for r in &unconfirmed_rounds {
            if r.current == RoundState::AcceptanceSheet {
                out.push_str(&format!("- Round {} has an acceptance sheet waiting for you to confirm\n", r.round));
            } else if r.current == RoundState::AllAccepted {
                out.push_str(&format!("- Round {} is accepted but the sheet is not yet written\n", r.round));
            }
        }
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::tests as chat_tests;
    use crate::model::{Exchange, Message, Store};

    fn user_msg(at: u64, text: &str) -> Message {
        Message { role: "user".into(), author: String::new(), text: text.into(), at, steps: None, meta: None, took: None, started: None, cost: None, model: None }
    }

    fn lead_msg(at: u64, text: &str) -> Message {
        Message { role: "lead".into(), author: "claude".into(), text: text.into(), at, steps: None, meta: None, took: None, started: None, cost: None, model: None }
    }

    fn peer_msg(at: u64, author: &str, text: &str) -> Message {
        Message { role: "peer".into(), author: author.into(), text: text.into(), at, steps: None, meta: None, took: None, started: None, cost: None, model: None }
    }

    fn sys_msg(at: u64, text: &str) -> Message {
        Message { role: "system".into(), author: String::new(), text: text.into(), at, steps: None, meta: None, took: None, started: None, cost: None, model: None }
    }

    fn make_peer(id: &str, slug: &str, round: u64, wave: u32) -> Peer {
        let mut p = chat_tests::peer(Vec::new());
        p.id = id.into();
        p.slug = slug.into();
        p.round = round;
        p.wave = Some(wave);
        p.started_at = 1000;
        p
    }

    fn exchange(from: &str, input: &str, reply: Option<&str>, outcome: Option<&str>, at: u64, took: Option<u64>) -> Exchange {
        Exchange { from: from.into(), input: input.into(), reply: reply.map(String::from), outcome: outcome.map(String::from), at, steps: None, took, cost_total: None, began: None }
    }

    #[test]
    fn slice_fixed_twice_then_accepted_and_landed_walks_the_path() {
        // A slice: fix, fix, accept, landed.
        let mut peer = make_peer("w1", "app-core", 1, 1);
        peer.log_mut().push(exchange("lead", "spec: build core", Some("first try\nCommit: aaa"), Some("completed"), 1100, Some(100)));
        peer.log_mut().push(exchange("lead", "fix: the tests are broken", Some("rebuilt\nCommit: bbb"), Some("completed"), 1300, Some(100)));
        peer.log_mut().push(exchange("lead", "fix: now the imports", Some("fixed\nCommit: ccc"), Some("completed"), 1500, Some(100)));
        peer.review = Some("accept: looks good now".into());
        peer.gate = Some(Gate { check: Some(true), cmd: Some("cargo test".into()), status: Some("ok".into()), at: Some("ccc".into()), claims: Some(vec![]), stray: Some(vec![]) });
        peer.landed = Some("merged to main".into());

        let machine = build_slice_machine_for(&[peer]);
        let path = &machine.paths[0];
        let kinds: Vec<&str> = path.transitions.iter().map(|t| t.to.label()).collect();
        assert!(kinds.contains(&"Running") && kinds.contains(&"Reported") && kinds.contains(&"Checked") && kinds.contains(&"Accepted") && kinds.contains(&"Landed"), "{kinds:?}");
        assert_eq!(path.current, SliceState::Landed);
        assert_eq!(path.rework_count, 2, "two fix: messages from the lead");
    }

    #[test]
    fn slice_with_no_review_but_landed_is_accepted_first() {
        // A slice the person landed without an explicit review.
        let mut peer = make_peer("w1", "doc", 1, 1);
        peer.log_mut().push(exchange("lead", "spec", Some("done"), Some("completed"), 1100, Some(50)));
        peer.landed = Some("merged".into());
        let machine = build_slice_machine_for(&[peer]);
        assert_eq!(machine.paths[0].current, SliceState::Landed, "landed wins over accepted");
    }

    #[test]
    fn slice_never_started_and_held_for_a_wave_is_waiting() {
        // A later wave's slice: no log, held.
        let held = make_peer("w1", "first", 1, 1);
        let mut second = make_peer("w2", "second", 1, 2);
        second.status = "queued".into();
        second.log_mut().clear();
        let machine = build_slice_machine_for(&[held, second]);
        let by_slug: Vec<&str> = machine.paths.iter().map(|p| p.slug.as_str()).collect();
        let second_path = machine.paths.iter().find(|p| p.slug == "second").unwrap();
        assert_eq!(second_path.current, SliceState::Waiting);
        assert!(by_slug.contains(&"first") && by_slug.contains(&"second"));
    }

    #[test]
    fn failed_and_over_budget_and_interrupted_are_terminal_side_exits() {
        let mut f = make_peer("w1", "fail", 1, 1);
        f.status = "failed".into();
        f.log_mut().push(exchange("lead", "x", Some("err"), Some("failed"), 1100, Some(5)));
        let mut ob = make_peer("w2", "over", 1, 1);
        ob.over_budget = Some("5 steps of 5".into());
        let mut st = make_peer("w3", "stopped", 1, 1);
        st.status = "interrupted".into();
        let m = build_slice_machine_for(&[f, ob, st]);
        let cur: Vec<SliceState> = m.paths.iter().map(|p| p.current).collect();
        assert!(cur.contains(&SliceState::Failed));
        assert!(cur.contains(&SliceState::OverBudget));
        assert!(cur.contains(&SliceState::Stopped));
    }

    #[test]
    fn discarded_when_closed_without_an_accept() {
        let mut p = make_peer("w1", "abandoned", 1, 1);
        p.status = "closed".into();
        p.log_mut().push(exchange("lead", "x", Some("no"), Some("completed"), 1100, Some(10)));
        let m = build_slice_machine_for(&[p]);
        assert_eq!(m.paths[0].current, SliceState::Discarded);
    }

    #[test]
    fn edge_counts_aggregate_across_paths() {
        // Two slices both went Running -> Reported -> Checked -> Accepted.
        let mut a = make_peer("a", "alpha", 1, 1);
        a.log_mut().push(exchange("lead", "x", Some("done"), Some("completed"), 1100, Some(50)));
        a.gate = Some(Gate { check: Some(true), cmd: None, status: None, at: Some("a".into()), claims: Some(vec![]), stray: Some(vec![]) });
        a.review = Some("accept: ok".into());
        let mut b = make_peer("b", "beta", 1, 1);
        b.log_mut().push(exchange("lead", "y", Some("done"), Some("completed"), 1100, Some(50)));
        b.gate = Some(Gate { check: Some(true), cmd: None, status: None, at: Some("b".into()), claims: Some(vec![]), stray: Some(vec![]) });
        b.review = Some("accept: ok".into());
        let m = build_slice_machine_for(&[a, b]);
        let edge = |from: TransitionKind, to: TransitionKind| m.edges.iter().find(|e| e.from == from && e.to == to).map(|e| e.count).unwrap_or(0);
        assert_eq!(edge(TransitionKind::Running, TransitionKind::Reported), 2);
        assert_eq!(edge(TransitionKind::Reported, TransitionKind::Checked), 2);
        assert_eq!(edge(TransitionKind::Checked, TransitionKind::Accepted), 2);
    }

    #[test]
    fn round_path_walks_request_planned_through_confirmed() {
        let mut store = Store::default();
        let dir = std::env::temp_dir();
        let pi = store.add_project(&dir.to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        let s = store.session_mut(at).unwrap();
        s.messages.push(user_msg(1000, "please add the thing"));
        s.messages.push(lead_msg(1010, "planning 2 slices"));
        let mut p = make_peer("w1", "thing", 1, 1);
        p.log_mut().push(exchange("lead", "spec", Some("done"), Some("completed"), 1100, Some(50)));
        p.gate = Some(Gate { check: Some(true), cmd: None, status: None, at: None, claims: Some(vec![]), stray: Some(vec![]) });
        p.review = Some("accept: ok".into());
        s.peers_mut().push(p);
        s.messages.push(sys_msg(1200, "the acceptance sheet for round 1 is at .octobuddy/acceptance/s-r1.md"));
        s.messages.push(user_msg(1300, "Confirm acceptance"));

        let m = build_round_machine(s);
        let r = &m.rounds[0];
        let kinds: Vec<&str> = r.transitions.iter().map(|t| t.to.label()).collect();
        assert_eq!(kinds.first().copied(), Some("Request"));
        assert!(kinds.contains(&"Planned") && kinds.contains(&"AllAccepted") && kinds.contains(&"AcceptanceSheet") && kinds.contains(&"Confirmed"), "{kinds:?}");
        assert_eq!(r.current, RoundState::Confirmed);
    }

    #[test]
    fn doc_stream_orders_items_in_time_and_versions_repo_docs() {
        let mut store = Store::default();
        let dir = std::env::temp_dir();
        let pi = store.add_project(&dir.to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        let s = store.session_mut(at).unwrap();
        s.messages.push(user_msg(1000, "first request"));
        s.messages.push(lead_msg(1010, "doing it"));
        s.messages.push(peer_msg(1080, "alpha", "report text"));
        let mut p = make_peer("w1", "alpha", 1, 1);
        p.log_mut().push(exchange("lead", "spec", Some("done"), Some("completed"), 1100, Some(50)));
        p.commits = Some(vec!["aaa1111 docs: tweak".into(), "bbb2222 docs: rewrite".into()]);
        s.peers_mut().push(p);

        let sessions = vec![store.session(at).unwrap()];
        let mut repo: BTreeMap<String, Vec<String>> = BTreeMap::new();
        repo.insert("README.md".into(), vec!["aaa1111".into(), "bbb2222".into()]);
        let stream = build_doc_stream(&sessions, &repo);

        // The order: request (1000) -> plan (1010) -> peer report (1080) -> commit aaa -> commit bbb.
        let kinds: Vec<&str> = stream.items.iter().map(|i| i.kind.label()).collect();
        assert!(kinds.iter().position(|&k| k == "Request").unwrap() < kinds.iter().position(|&k| k == "Report").unwrap());
        assert_eq!(stream.path_versions.get("README.md").unwrap(), &vec![1, 2]);
    }

    #[test]
    fn living_md_is_a_story_with_open_ends_and_is_stable_when_unchanged() {
        let mut store = Store::default();
        let dir = std::env::temp_dir();
        let pi = store.add_project(&dir.to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        {
            let s = store.session_mut(at).unwrap();
            s.messages.push(user_msg(1000, "ship the calc"));
            let mut p = make_peer("w1", "calc", 1, 1);
            p.log_mut().push(exchange("lead", "spec", Some("done"), Some("completed"), 1100, Some(50)));
            s.peers_mut().push(p);
        }
        let sessions = vec![store.session(at).unwrap()];
        let stream = build_doc_stream(&sessions, &BTreeMap::new());
        let sm = build_slice_machine(sessions[0]);
        let rm = build_round_machine(sessions[0]);
        let md1 = build_living("demo", &sm, &rm, &stream);
        let md2 = build_living("demo", &sm, &rm, &stream);
        assert_eq!(md1, md2, "re-building with the same data gives the same string");
        assert!(md1.contains("Round 1"));
        assert!(md1.contains("calc"));
    }

    fn build_slice_machine_for(peers: &[Peer]) -> SliceMachine {
        let mut store = Store::default();
        let dir = std::env::temp_dir();
        let pi = store.add_project(&dir.to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        let s = store.session_mut(at).unwrap();
        for p in peers {
            s.peers_mut().push(p.clone());
        }
        build_slice_machine(s)
    }
}
