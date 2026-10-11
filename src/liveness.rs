//! Whether a loop is at work now, waiting (on whom, and why), silent too
//! long, or done: told apart, from when it was last heard from.
//!
//! `of` is pure: the caller fills `Inputs` from the session and the
//! runtime, and `label` says it in the interface's language. A loop with a
//! turn open that nothing was heard from for `silent_secs()` is silent: its
//! agent may be stuck, and the person can act on it (Interrupt & send, Stop).
use crate::events::LoopEvent;
use crate::i18n;
use crate::model::{Peer, Session};
use std::collections::HashMap;

/// A turn unheard from this long is silent, unless `OCTOBUDDY_SILENT_SECS` says otherwise.
pub const SILENT_SECS: u64 = 120;

/// How long a turn may go unheard from before it is silent:
/// `OCTOBUDDY_SILENT_SECS` (10 at least: a demo shortens it), else `SILENT_SECS`.
pub fn silent_secs() -> u64 {
    silent_from(std::env::var("OCTOBUDDY_SILENT_SECS").ok().as_deref())
}

fn silent_from(var: Option<&str>) -> u64 {
    var.and_then(|v| v.trim().parse::<u64>().ok()).map(|s| s.max(10)).unwrap_or(SILENT_SECS)
}

/// Whom a loop waits for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    /// The person: an approval, an answer, a budget, their next message.
    Person,
    /// The outer loop: its review of the loop's report.
    OuterLoop,
    /// OctoBuddy, checking and committing its work.
    Check,
    /// The earlier wave of its round, to be accepted.
    Wave,
    /// A free slot: as many inner loops as may run are at work.
    Queue,
    /// Its own agent, starting (its process, its model's first answer).
    Agent,
    /// For an outer loop: its inner loops, at work on its tasks.
    Inner,
}

impl Who {
    /// Its name in `octobuddy.status`.
    pub fn key(self) -> &'static str {
        match self {
            Who::Person => "person",
            Who::OuterLoop => "outer loop",
            Who::Check => "check",
            Who::Wave => "wave",
            Who::Queue => "queue",
            Who::Agent => "agent",
            Who::Inner => "inner loops",
        }
    }
}

/// What it waits for, more closely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The person: a tool call to approve, a question to answer, a budget
    /// used up, a loop they stopped or took over in its terminal, their
    /// next message (an outer loop that answered).
    Approval,
    Question,
    Budget,
    Stopped,
    Terminal,
    Next,
    /// Its report, for the outer loop's review: how many wait in all (0:
    /// the outer loop has them).
    Review(usize),
    Check,
    /// The wave before its own (0: not known).
    Wave(u32),
    /// A slot: how many inner loops are at work.
    Slot(usize),
    Start,
    /// For an outer loop: how many of its inner loops are at work.
    Reports(usize),
}

impl Why {
    pub fn who(self) -> Who {
        match self {
            Why::Approval | Why::Question | Why::Budget | Why::Stopped | Why::Terminal | Why::Next => Who::Person,
            Why::Review(_) => Who::OuterLoop,
            Why::Check => Who::Check,
            Why::Wave(_) => Who::Wave,
            Why::Slot(_) => Who::Queue,
            Why::Start => Who::Agent,
            Why::Reports(_) => Who::Inner,
        }
    }
}

/// How a loop ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    Accepted,
    /// Accepted, then another loop changed its files (`stale`).
    Changed,
    Rejected,
    Merged,
    Discarded,
    Closed,
    Failed,
    Interrupted,
    Stopped,
    /// Its turn is done and nothing waits on it.
    Idle,
    /// An outer loop's session, archived.
    Archived,
    /// No outer loop: where a project keeps the peers made without one.
    Alone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// At work: what it does (may be empty), and how long since it was heard from.
    Working { doing: String, secs: u64 },
    Waiting { on: Who, why: Why },
    /// A turn open, nothing heard from it for `secs`.
    Silent { secs: u64 },
    Done { how: How },
}

impl State {
    fn wait(why: Why) -> State {
        State::Waiting { on: why.who(), why }
    }

    /// Its name in `octobuddy.status`.
    pub fn key(&self) -> &'static str {
        match self {
            State::Working { .. } => "working",
            State::Waiting { .. } => "waiting",
            State::Silent { .. } => "silent",
            State::Done { .. } => "done",
        }
    }

    pub fn on(&self) -> Option<Who> {
        match self {
            State::Waiting { on, .. } => Some(*on),
            _ => None,
        }
    }
}

/// What `of` reads of a loop: the caller fills it from the session and the runtime.
#[derive(Clone, Debug, Default)]
pub struct Inputs<'a> {
    /// An outer loop (else an inner one).
    pub outer: bool,
    /// An inner loop's status (`running`, `idle`, `closed`…); an outer
    /// loop's `running` while it has a turn, else `idle`, `stopped`,
    /// `archived` or `alone`.
    pub status: &'a str,
    /// Its agent has not taken its open turn up yet.
    pub starting: bool,
    /// When it was last heard from, or given its turn (epoch seconds).
    pub heard: Option<u64>,
    /// What it does now, as its agent last said.
    pub doing: &'a str,
    pub approval: bool,
    pub question: bool,
    pub over_budget: bool,
    /// The person took it over in its terminal.
    pub taken: bool,
    /// OctoBuddy checks its work (an outer loop: its inner loops' work).
    pub checking: bool,
    /// Held until the earlier waves are accepted: its own wave.
    pub held: Option<u32>,
    /// Waits for a free slot: how many inner loops are at work.
    pub slot: Option<usize>,
    /// Its report waits for the outer loop's review: how many wait in all.
    pub review: Option<usize>,
    /// The outer loop's call on it (`accept: …`).
    pub verdict: Option<&'a str>,
    /// For an outer loop: its inner loops at work, and the reports it has
    /// not been given (held when its review budget is used up).
    pub inner: usize,
    pub unread: usize,
    /// Silent past this many seconds unheard (`silent_secs`).
    pub silent_after: u64,
}

/// What a loop is now, as of `now` (epoch seconds).
pub fn of(i: &Inputs, now: u64) -> State {
    let done = |how| State::Done { how };
    match i.status {
        "closed" => return done(How::Closed),
        "merged" => return done(How::Merged),
        "discarded" => return done(How::Discarded),
        "archived" => return done(How::Archived),
        "alone" => return done(How::Alone),
        _ => {}
    }
    let person = [(i.approval, Why::Approval), (i.question, Why::Question), (i.taken, Why::Terminal), (i.over_budget, Why::Budget)];
    if let Some((_, why)) = person.into_iter().find(|(on, _)| *on) {
        return State::wait(why);
    }
    if let Some(wave) = i.held {
        return State::wait(Why::Wave(wave.saturating_sub(1)));
    }
    if let Some(running) = i.slot {
        return State::wait(Why::Slot(running));
    }
    // A turn open (or about to be: its session opening): heard from lately, or silent.
    if matches!(i.status, "running" | "queued") {
        let secs = i.heard.map(|h| now.saturating_sub(h));
        if let Some(secs) = secs.filter(|s| *s >= i.silent_after) {
            return State::Silent { secs };
        }
        if i.starting || i.status == "queued" {
            return State::wait(Why::Start);
        }
        return State::Working { doing: i.doing.trim().to_string(), secs: secs.unwrap_or(0) };
    }
    if i.checking || i.status == "checking" {
        return State::wait(Why::Check);
    }
    if let Some(n) = i.review {
        return State::wait(Why::Review(n));
    }
    let verdict = i.verdict.unwrap_or("");
    for (word, how) in [("accept", How::Accepted), ("stale", How::Changed), ("reject", How::Rejected)] {
        if verdict.starts_with(word) {
            return done(how);
        }
    }
    if i.outer {
        let why = if i.status == "stopped" {
            Why::Stopped
        } else if i.inner > 0 {
            Why::Reports(i.inner)
        } else if i.unread > 0 {
            Why::Budget
        } else {
            Why::Next
        };
        return State::wait(why);
    }
    done(match i.status {
        "failed" => How::Failed,
        "interrupted" => How::Interrupted,
        "stopped" => How::Stopped,
        _ => How::Idle,
    })
}

/// How long ago, for a person: `just now`, `12 s ago`, `3 min ago`.
fn ago(secs: u64) -> String {
    match secs {
        0..=4 => i18n::t("just now", "刚刚").to_string(),
        5..=59 => i18n::pick(format!("{secs} s ago"), format!("{secs} 秒前")),
        60..=3599 => i18n::pick(format!("{} min ago", secs / 60), format!("{} 分钟前", secs / 60)),
        _ => i18n::pick(format!("{} h {} min ago", secs / 3600, secs % 3600 / 60), format!("{} 小时 {} 分钟前", secs / 3600, secs % 3600 / 60)),
    }
}

/// A spell's length, for a person: `40 s`, `4 min`, `1 h 5 min`.
fn span(secs: u64) -> String {
    match secs {
        0..=59 => i18n::pick(format!("{secs} s"), format!("{secs} 秒")),
        60..=3599 => i18n::pick(format!("{} min", secs / 60), format!("{} 分钟", secs / 60)),
        _ => i18n::pick(format!("{} h {} min", secs / 3600, secs % 3600 / 60), format!("{} 小时 {} 分钟", secs / 3600, secs % 3600 / 60)),
    }
}

fn how_word(how: How) -> &'static str {
    match how {
        How::Accepted => i18n::t("accepted", "已验收"),
        How::Changed => i18n::t("accepted, since changed by another loop", "已验收，之后被别的 loop 改动"),
        How::Rejected => i18n::t("rejected", "已驳回"),
        How::Merged => i18n::t("merged", "已合并"),
        How::Discarded => i18n::t("discarded", "已丢弃"),
        How::Closed => i18n::t("closed", "已关闭"),
        How::Failed => i18n::t("failed", "失败"),
        How::Interrupted => i18n::t("interrupted", "被打断"),
        How::Stopped => i18n::t("stopped", "已停止"),
        How::Idle => i18n::t("its turn is done", "这一轮已完成"),
        How::Archived => i18n::t("archived", "已归档"),
        How::Alone => i18n::t("no outer loop here", "这里没有外环"),
    }
}

/// What it waits for, in a few words (after "waiting for").
fn why_words(why: Why) -> String {
    match why {
        Why::Approval => i18n::t("waiting for you: an approval", "等你：一个审批").into(),
        Why::Question => i18n::t("waiting for you: an answer to its question", "等你：回答它的问题").into(),
        Why::Budget => i18n::t("waiting for you: its budget is used up", "等你：预算已用完").into(),
        Why::Stopped => i18n::t("waiting for you: you stopped it", "等你：你让它停下了").into(),
        Why::Terminal => i18n::t("waiting for you: you took it over in its terminal", "等你：你在终端里接管了它").into(),
        Why::Next => i18n::t("waiting for you: your next message", "等你：你的下一条消息").into(),
        Why::Review(n) if n > 1 => i18n::pick(format!("waiting for the outer loop's review ({n} reports)"), format!("等外环审查（{n} 份汇报）")),
        Why::Review(_) => i18n::t("waiting for the outer loop's review", "等外环审查").into(),
        Why::Check => i18n::t("waiting for OctoBuddy's check and commit", "等 OctoBuddy 检查和提交").into(),
        Why::Wave(0) => i18n::t("waiting for the earlier wave to be accepted", "等前一个 wave 验收").into(),
        Why::Wave(w) => i18n::pick(format!("waiting for wave {w} to be accepted"), format!("等 wave {w} 验收")),
        Why::Slot(n) => i18n::pick(format!("queued for a slot ({n} at work)"), format!("排队等空位（已有 {n} 个在做）")),
        Why::Start => i18n::t("waiting for its agent to start", "等它的 agent 启动").into(),
        Why::Reports(n) => i18n::pick(format!("waiting for its inner loops ({n} at work)"), format!("等 inner 汇报（{n} 个在做）")),
    }
}

/// The state, said in the interface's language.
pub fn label(s: &State) -> String {
    match s {
        State::Working { doing, secs } if doing.is_empty() => i18n::pick(format!("working · {}", ago(*secs)), format!("工作中 · {}", ago(*secs))),
        State::Working { doing, secs } => i18n::pick(format!("working · {doing} · {}", ago(*secs)), format!("工作中 · {doing} · {}", ago(*secs))),
        State::Waiting { why, .. } => why_words(*why),
        State::Silent { secs } => i18n::pick(format!("silent for {} — no sign from its agent", span(*secs)), format!("已 {}没有动静——它的 agent 没有任何回应", span(*secs))),
        State::Done { how: How::Idle } => i18n::pick(format!("idle: {}", how_word(How::Idle)), format!("空闲：{}", how_word(How::Idle))),
        State::Done { how: How::Alone } => how_word(How::Alone).into(),
        State::Done { how } => i18n::pick(format!("ended: {}", how_word(*how)), format!("已结束：{}", how_word(*how))),
    }
}

/// The key an outer loop is heard from under (an inner loop's is its id).
pub fn outer_key(session: &str) -> String {
    format!("lead:{session}")
}

/// Whose sign of life an event is: an inner loop's id, or its session's
/// `outer_key` for an outer loop's. A process's end, or a failure to
/// start, is none.
pub fn heard_key(event: &LoopEvent) -> Option<String> {
    use LoopEvent::*;
    match event {
        LeadStatus { session, .. } | LeadDelta { session, .. } | LeadTool { session, .. } | LeadToolEnd { session, .. }
        | LeadSubTool { session, .. } | LeadInfo { session, .. } | LeadMessage { session, .. } | LeadTurnDone { session, .. }
        | LeadSteerTaken { session } | LeadSteerRefused { session, .. } | LeadSteerCarried { session, .. } => Some(outer_key(session)),
        PeerReady { peer, .. } | PeerTurnStarted { peer } | PeerActivity { peer, .. } | PeerDelta { peer, .. } | PeerTool { peer, .. }
        | PeerToolEnd { peer, .. } | PeerQuestion { peer, .. } | PeerSteered { peer, .. } | PeerSteerDropped { peer, .. }
        | PeerTurnEnded { peer, .. } | PeerInfo { peer, .. } | PeerRound { peer } | PeerFileChanged { peer, .. } | PeerAgents { peer, .. }
        | PeerSubagent { peer, .. } | PeerCost { peer, .. } | PeerChecked { peer, .. } | PeerApproval { peer, .. }
        | PeerApprovalGone { peer, .. } => Some(peer.clone()),
        _ => None,
    }
}

/// When an inner loop was last heard from: its last event, the turn it
/// was given, or its start, whichever is latest.
pub fn last_heard(p: &Peer, event: Option<u64>) -> Option<u64> {
    let given = p.log().iter().rev().find(|e| e.outcome.is_none()).map(|e| e.at);
    [event, given, Some(p.started_at).filter(|s| *s > 0)].into_iter().flatten().max()
}

/// Its open turn not taken up by its agent yet: no sign of it (no step,
/// no word) since it was given.
pub fn starting(p: &Peer) -> bool {
    p.log().iter().rev().find(|e| e.outcome.is_none())
        .is_some_and(|e| e.began.is_none() && e.steps.as_ref().is_none_or(|s| s.is_empty()) && e.reply.as_deref().is_none_or(str::is_empty))
}

/// A task of the outer loop's it finished, whose verdict has not come
/// (none yet, or the last one sent it back): its report is the outer
/// loop's to review. A reviewer's report is about another loop.
pub fn verdict_due(p: &Peer) -> bool {
    p.reviews_for.is_none()
        && p.review.as_deref().is_none_or(|r| r.starts_with("revise") || r.starts_with("fix"))
        && p.log().last().is_some_and(|e| e.from == "lead" && e.outcome.as_deref() == Some("completed"))
}

/// A watched app with no run for more than twice its pace: its watch has
/// stopped, whatever its last health said.
pub fn stale_watch(last: u64, every: u64, now: u64) -> bool {
    last > 0 && now.saturating_sub(last) > every.saturating_mul(2)
}

/// One loop as `octobuddy.status` gives it.
pub struct Seen {
    pub state: &'static str,
    pub waiting_on: Option<&'static str>,
    pub why: String,
    pub ago: Option<u64>,
}

impl Seen {
    fn of(state: &State, heard: Option<u64>, now: u64) -> Seen {
        Seen { state: state.key(), waiting_on: state.on().map(Who::key), why: label(state), ago: heard.map(|h| now.saturating_sub(h)) }
    }
}

impl crate::OctoBuddyView {
    /// An inner loop's state now, and when it was last heard from.
    pub(crate) fn peer_state(&self, session: &Session, p: &Peer, now: u64) -> (State, Option<u64>) {
        let rt = &self.rt;
        let heard = last_heard(p, rt.heard.get(&p.id).copied());
        let unread = rt.unreported.get(&session.id);
        let review = match unread.filter(|v| v.iter().any(|(id, _, _)| id == &p.id)) {
            Some(v) => Some(v.len()),
            None => (!session.is_detached() && verdict_due(p)).then_some(0),
        };
        let at_work = rt.lines.values().filter(|l| l.is_busy()).count();
        let inputs = Inputs {
            status: &p.status,
            starting: starting(p),
            heard,
            doing: p.activity.as_deref().unwrap_or(""),
            approval: rt.approvals.contains_key(&p.id),
            question: rt.questions.contains_key(&p.id),
            over_budget: p.over_budget.is_some(),
            taken: self.tui_holds_peer(&p.id),
            checking: rt.checking.contains(&p.id),
            held: rt.held.contains_key(&p.id).then(|| p.wave.unwrap_or(2)),
            slot: rt.slot_wait.iter().any(|w| w == &p.id).then_some(at_work),
            review,
            verdict: p.review.as_deref(),
            silent_after: silent_secs(),
            ..Default::default()
        };
        (of(&inputs, now), heard)
    }

    /// A session's outer loop's state now, and when it was last heard from.
    pub(crate) fn outer_state(&self, session: &Session, now: u64) -> (State, Option<u64>) {
        let rt = &self.rt;
        let id = &session.id;
        let heard = [rt.heard.get(&outer_key(id)).copied(), rt.lead_started.get(id).copied()].into_iter().flatten().max();
        let turn = rt.lead_pending.get(id).copied().unwrap_or(0) > 0 || rt.packing.contains(id);
        let status = if session.is_archived() {
            "archived"
        } else if session.is_detached() {
            "alone"
        } else if turn {
            "running"
        } else if self.halted.contains(id) {
            "stopped"
        } else {
            "idle"
        };
        let doing = self.lead_status.get(id).map(|s| crate::status_word(s)).unwrap_or("");
        let inputs = Inputs {
            outer: true,
            status,
            starting: rt.packing.contains(id) || (rt.lead_started.contains_key(id) && !rt.lead_began.contains_key(id)),
            heard,
            doing,
            taken: self.tui_holds_outer(id),
            checking: session.peers().iter().any(|p| rt.checking.contains(&p.id)),
            inner: session.peers().iter().filter(|p| p.is_active() && !rt.held.contains_key(&p.id)).count(),
            unread: rt.unreported.get(id).map(Vec::len).unwrap_or(0),
            silent_after: silent_secs(),
            ..Default::default()
        };
        (of(&inputs, now), heard)
    }

    /// Every loop as `octobuddy.status` gives it: by inner loop id, and by
    /// `outer_key` for the outer loops.
    pub(crate) fn seen_all(&self, now: u64) -> HashMap<String, Seen> {
        let mut out = HashMap::new();
        for s in self.store.projects.iter().flat_map(|p| p.sessions.iter()) {
            let (state, heard) = self.outer_state(s, now);
            out.insert(outer_key(&s.id), Seen::of(&state, heard, now));
            for p in s.peers() {
                let (state, heard) = self.peer_state(s, p, now);
                out.insert(p.id.clone(), Seen::of(&state, heard, now));
            }
        }
        out
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{set, Lang};
    use crate::model::Exchange;

    const NOW: u64 = 1_000_000;

    fn inner(status: &str) -> Inputs<'_> {
        Inputs { status, heard: Some(NOW - 12), silent_after: 120, ..Default::default() }
    }

    #[test]
    fn each_loop_is_working_waiting_silent_or_done() {
        let cases: Vec<(Inputs, State)> = vec![
            (Inputs { doing: "step 3 · ran bash", ..inner("running") }, State::Working { doing: "step 3 · ran bash".into(), secs: 12 }),
            (Inputs { heard: Some(NOW - 240), ..inner("running") }, State::Silent { secs: 240 }),
            (Inputs { heard: Some(NOW - 120), ..inner("running") }, State::Silent { secs: 120 }),
            (Inputs { starting: true, ..inner("running") }, State::wait(Why::Start)),
            (Inputs { starting: true, heard: Some(NOW - 300), ..inner("running") }, State::Silent { secs: 300 }),
            (inner("queued"), State::wait(Why::Start)),
            (Inputs { held: Some(2), ..inner("queued") }, State::wait(Why::Wave(1))),
            (Inputs { slot: Some(4), ..inner("queued") }, State::wait(Why::Slot(4))),
            (Inputs { approval: true, heard: Some(NOW - 600), ..inner("running") }, State::wait(Why::Approval)),
            (Inputs { question: true, ..inner("running") }, State::wait(Why::Question)),
            (Inputs { over_budget: true, ..inner("stopped") }, State::wait(Why::Budget)),
            (Inputs { checking: true, ..inner("checking") }, State::wait(Why::Check)),
            (Inputs { review: Some(3), verdict: Some("accept: ok"), ..inner("idle") }, State::wait(Why::Review(3))),
            (Inputs { verdict: Some("accept: ok"), ..inner("idle") }, State::Done { how: How::Accepted }),
            (Inputs { verdict: Some("stale: b changed x after it was accepted"), ..inner("idle") }, State::Done { how: How::Changed }),
            (inner("failed"), State::Done { how: How::Failed }),
            (inner("idle"), State::Done { how: How::Idle }),
            (Inputs { approval: true, ..inner("closed") }, State::Done { how: How::Closed }),
            (inner("merged"), State::Done { how: How::Merged }),
        ];
        for (i, want) in cases {
            assert_eq!(of(&i, NOW), want, "{i:?}");
        }
        // Never heard from: it cannot be called silent.
        assert_eq!(of(&Inputs { heard: None, ..inner("running") }, NOW), State::Working { doing: String::new(), secs: 0 });
    }

    #[test]
    fn an_outer_loop_waits_on_its_inner_loops_or_the_person() {
        let outer = |status| Inputs { outer: true, status, heard: Some(NOW - 5), silent_after: 120, ..Default::default() };
        assert_eq!(of(&Inputs { doing: "thinking", ..outer("running") }, NOW), State::Working { doing: "thinking".into(), secs: 5 });
        assert_eq!(of(&Inputs { heard: Some(NOW - 130), ..outer("running") }, NOW), State::Silent { secs: 130 });
        assert_eq!(of(&Inputs { inner: 2, ..outer("idle") }, NOW), State::wait(Why::Reports(2)));
        assert_eq!(of(&Inputs { checking: true, inner: 2, ..outer("idle") }, NOW), State::wait(Why::Check));
        assert_eq!(of(&Inputs { unread: 1, ..outer("idle") }, NOW), State::wait(Why::Budget), "reports held and nothing at work: its review budget");
        assert_eq!(of(&Inputs { inner: 1, ..outer("stopped") }, NOW), State::wait(Why::Stopped));
        assert_eq!(of(&Inputs { taken: true, ..outer("running") }, NOW), State::wait(Why::Terminal));
        assert_eq!(of(&outer("idle"), NOW), State::wait(Why::Next));
        assert_eq!(of(&outer("archived"), NOW), State::Done { how: How::Archived });
        let waiting = of(&outer("idle"), NOW);
        assert_eq!((waiting.key(), waiting.on().map(Who::key)), ("waiting", Some("person")));
    }

    #[test]
    fn labels_say_it_in_both_languages() {
        let cases: Vec<(State, &str, &str)> = vec![
            (State::Working { doing: String::new(), secs: 12 }, "working · 12 s ago", "工作中 · 12 秒前"),
            (State::Working { doing: "step 2 · ran bash".into(), secs: 2 }, "working · step 2 · ran bash · just now", "工作中 · step 2 · ran bash · 刚刚"),
            (State::wait(Why::Review(3)), "waiting for the outer loop's review (3 reports)", "等外环审查（3 份汇报）"),
            (State::wait(Why::Review(1)), "waiting for the outer loop's review", "等外环审查"),
            (State::wait(Why::Approval), "waiting for you: an approval", "等你：一个审批"),
            (State::wait(Why::Wave(1)), "waiting for wave 1 to be accepted", "等 wave 1 验收"),
            (State::wait(Why::Slot(4)), "queued for a slot (4 at work)", "排队等空位（已有 4 个在做）"),
            (State::wait(Why::Check), "waiting for OctoBuddy's check and commit", "等 OctoBuddy 检查和提交"),
            (State::wait(Why::Start), "waiting for its agent to start", "等它的 agent 启动"),
            (State::wait(Why::Reports(2)), "waiting for its inner loops (2 at work)", "等 inner 汇报（2 个在做）"),
            (State::wait(Why::Next), "waiting for you: your next message", "等你：你的下一条消息"),
            (State::Silent { secs: 245 }, "silent for 4 min — no sign from its agent", "已 4 分钟没有动静——它的 agent 没有任何回应"),
            (State::Silent { secs: 15 }, "silent for 15 s — no sign from its agent", "已 15 秒没有动静——它的 agent 没有任何回应"),
            (State::Done { how: How::Accepted }, "ended: accepted", "已结束：已验收"),
            (State::Done { how: How::Idle }, "idle: its turn is done", "空闲：这一轮已完成"),
        ];
        for (state, en, zh) in cases {
            set(Lang::En);
            assert_eq!(label(&state), en);
            set(Lang::Zh);
            assert_eq!(label(&state), zh);
        }
        set(Lang::En);
    }

    #[test]
    fn octobuddy_status_says_each_loop_and_since_when() {
        set(Lang::En);
        let fields = |l: &Seen| serde_json::json!({"state": l.state, "waiting_on": l.waiting_on, "why": l.why, "last_heard_secs_ago": l.ago});
        let silent = Seen::of(&of(&Inputs { heard: Some(NOW - 250), ..inner("running") }, NOW), Some(NOW - 250), NOW);
        assert_eq!(fields(&silent), serde_json::json!({"state": "silent", "waiting_on": null, "why": "silent for 4 min — no sign from its agent", "last_heard_secs_ago": 250}));
        let review = Seen::of(&of(&Inputs { review: Some(3), ..inner("idle") }, NOW), Some(NOW - 40), NOW);
        assert_eq!(fields(&review), serde_json::json!({"state": "waiting", "waiting_on": "outer loop", "why": "waiting for the outer loop's review (3 reports)", "last_heard_secs_ago": 40}));
        let outer = Seen::of(&of(&Inputs { outer: true, status: "idle", inner: 2, ..Default::default() }, NOW), None, NOW);
        assert_eq!(fields(&outer), serde_json::json!({"state": "waiting", "waiting_on": "inner loops", "why": "waiting for its inner loops (2 at work)", "last_heard_secs_ago": null}));
    }

    #[test]
    fn the_silent_threshold_comes_from_the_environment() {
        assert_eq!(silent_from(None), SILENT_SECS);
        assert_eq!(silent_from(Some("30")), 30);
        assert_eq!(silent_from(Some("3")), 10, "10 s at least");
        assert_eq!(silent_from(Some("soon")), SILENT_SECS);
        std::env::set_var("OCTOBUDDY_SILENT_SECS", "20");
        assert_eq!(silent_secs(), 20);
        std::env::remove_var("OCTOBUDDY_SILENT_SECS");
        assert_eq!(silent_secs(), SILENT_SECS);
    }

    #[test]
    fn a_watch_is_stale_past_twice_its_pace() {
        for (last, every, now, stale) in [(1000, 300, 1600, false), (1000, 300, 1601, true), (0, 300, 99_999, false), (1000, 30, 1061, true)] {
            assert_eq!(stale_watch(last, every, now), stale, "last {last}, every {every}, now {now}");
        }
    }

    #[test]
    fn signs_of_life_name_their_loop() {
        assert_eq!(heard_key(&LoopEvent::PeerDelta { peer: "w1".into(), text: "x".into() }).as_deref(), Some("w1"));
        assert_eq!(heard_key(&LoopEvent::LeadTool { session: "s".into(), id: "1".into(), name: "Bash".into(), detail: String::new() }).as_deref(), Some("lead:s"));
        assert_eq!(heard_key(&LoopEvent::LeadExited { session: "s".into(), error: None, gen: 1 }), None, "a process gone is no sign of life");
        assert_eq!(heard_key(&LoopEvent::ServeRetry), None);
    }

    #[test]
    fn an_inner_loop_is_heard_from_its_turn_and_its_report_waits_for_a_verdict() {
        let ex = |from: &str, at: u64, outcome: Option<&str>| Exchange { from: from.into(), input: "do it".into(), reply: None, outcome: outcome.map(String::from), at, steps: None, took: None, cost_total: None, began: None };
        let mut p = crate::chat::tests::peer(vec![ex("lead", 100, Some("completed")), ex("lead", 200, None)]);
        p.started_at = 150;
        assert_eq!(last_heard(&p, None), Some(200), "given its turn at 200");
        assert_eq!(last_heard(&p, Some(260)), Some(260));
        assert!(starting(&p), "nothing from its agent on the open turn");
        p.log_mut()[1].reply = Some("Reading".into());
        assert!(!starting(&p));
        p.log_mut()[1].outcome = Some("completed".into());
        assert!(verdict_due(&p), "a finished task of the outer loop's, no verdict");
        p.review = Some("revise: add a test".into());
        assert!(verdict_due(&p), "sent back: the next report waits for a verdict too");
        p.review = Some("accept: ok".into());
        assert!(!verdict_due(&p));
        p.review = None;
        p.log_mut()[1].from = "person".into();
        assert!(!verdict_due(&p), "the person's own talk is not the outer loop's to review");
    }
}
