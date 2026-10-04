//! The timeline: one session's run on a time axis, a lane for its outer loop
//! and one for each inner loop, with a playhead that replays it.
//!
//! - A bar is a turn: when it began, how long it ran, its number in its lane.
//! - Between the lanes: the outer loop sending work down to an inner loop, a
//!   report coming back up; a dot where the person spoke; a diamond where
//!   OctoBuddy committed an inner loop's files.
//! - Stretches where nobody worked are squeezed to a few seconds (marked with
//!   their real length), unless the person turns that off.
//! - Play runs the playhead along the axis; what lies ahead of it is faint.
//!   Press or drag on the canvas to move it; the wheel zooms around the
//!   pointer, a sideways scroll pans. A click on a bar picks it; a double
//!   click opens it in the conversation.
//!
//! The model is built from what the store already keeps (messages, each
//! peer's exchanges): the timeline is the conversations' state on an axis,
//! not a second record of them.
//!
//! Rework is read from the same record (`Rework`): a turn the outer loop sent
//! back, one that ended without a report or a commit, a failed turn tried
//! again, a slice started afresh (another agent or model), an inner loop
//! closed and replaced, a reference several inner loops each learned on their
//! own. Each is marked where it happened, with why (the outer loop's own words
//! when it said why), and the attempts it replaced are drawn as history:
//! hatched, apart from the work that stood.
use crate::i18n;
use crate::model::{Peer, Role, Session};
use makepad_widgets::*;

/// What a lane (and its bars) stands for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Outer,
    Inner,
    /// An independent reviewer.
    Review,
}

#[derive(Clone, Debug)]
pub struct Lane {
    /// `s:<session>` for the outer loop, `p:<peer>` for an inner one.
    pub key: String,
    pub name: String,
    /// Its engine and model, short.
    pub sub: String,
    pub kind: Kind,
    pub closed: bool,
    /// Its work did not stand (closed unaccepted, or replaced): why. Drawn
    /// as history.
    pub history: Option<String>,
}

/// One turn of one loop.
#[derive(Clone, Debug)]
pub struct Span {
    pub lane: usize,
    /// Seconds from the timeline's start.
    pub start: f64,
    pub end: f64,
    /// Still running (its end is now).
    pub open: bool,
    /// Its number in its lane, from 1.
    pub turn: usize,
    /// Who it answered: `you`, `outer`, `reports`, `octobuddy`, `itself`.
    pub from: String,
    /// `completed`, `failed`, `interrupted`, or empty.
    pub outcome: String,
    pub steps: usize,
    pub failed: usize,
    /// What it cost (USD), when that is known.
    pub cost: Option<f64>,
    pub input: String,
    pub reply: String,
    /// An outer turn's last message, to open it in the conversation.
    pub message: Option<usize>,
    /// Its work was done again later (or thrown away): drawn as history.
    pub redone: bool,
    /// When what it answered was sent to it (its wait before it began is a `Wait`).
    pub asked: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkKind {
    /// The person spoke.
    Person,
    /// The outer loop sent an inner loop work.
    Dispatch,
    /// An inner loop's report went up.
    Report,
    /// OctoBuddy committed an inner loop's files.
    Commit,
    /// Work done again: why is its text.
    Rework,
}

/// One piece of rework, for the list under the timeline (and the project's memory).
#[derive(Clone, Debug, PartialEq)]
pub struct Rework {
    pub lane: usize,
    /// Seconds from the timeline's start.
    pub at: f64,
    /// `sent back`, `no result`, `failed`, `restarted`, `replaced`, `relearned`.
    pub kind: &'static str,
    pub why: String,
    /// The time the work it replaced took (seconds), when known.
    pub lost: f64,
}

#[derive(Clone, Debug)]
pub struct Mark {
    pub lane: usize,
    /// The lane it goes to (work down, a report up).
    pub to: Option<usize>,
    pub at: f64,
    pub kind: MarkKind,
    pub text: String,
    /// Where it lands on `to`: the turn there that took it up (work down:
    /// the inner turn's start; a report up: the outer turn that read it).
    /// None: not taken up yet. Its line joins the two turns, its wait shown.
    pub until: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct Timeline {
    pub lanes: Vec<Lane>,
    pub spans: Vec<Span>,
    pub marks: Vec<Mark>,
    /// Where each round of planning began, when there is more than one.
    pub rounds: Vec<(f64, u64)>,
    /// The epoch second time 0 stands for.
    pub origin: u64,
    pub end: f64,
    pub rework: Vec<Rework>,
    /// Where a loop waited, and why (no one was idle by mistake).
    pub waits: Vec<Wait>,
}

/// A stretch a loop waited, and why: for finding where the time goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Wait {
    pub lane: usize,
    pub from: f64,
    pub to: f64,
    /// `start` (an agent starting, its model's first answer), `wave` (an
    /// earlier wave to be accepted), `queue` (behind its own turn, or a free
    /// slot), `check` (OctoBuddy checking and committing), `person` (the
    /// person's next message), `restart` (OctoBuddy restarted, or stopped).
    pub kind: &'static str,
}

impl Wait {
    pub fn why(&self) -> &'static str {
        match self.kind {
            "start" => i18n::t("starting: the agent, its model's first answer", "启动中：拉起 agent、等模型的第一个回应"),
            "wave" => i18n::t("waits for the earlier wave to be accepted", "等前一个 wave 验收"),
            "queue" => i18n::t("queued: behind its own turn, or for a free slot", "排队：等它上一轮做完，或等空位"),
            "check" => i18n::t("OctoBuddy checks and commits its work", "OctoBuddy 在检查和提交"),
            "person" => i18n::t("waits for you: the outer loop answered", "等你：外环答完了，在等你的下一条消息"),
            _ => i18n::t("OctoBuddy restarted, or was stopped", "OctoBuddy 重启或被停止"),
        }
    }
}

fn first_line(text: &str, n: usize) -> String {
    text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(n).collect()
}

fn has_commit(reply: &str) -> Option<String> {
    reply.lines().rev().map(|l| l.trim().trim_start_matches('*').trim()).find_map(|l| {
        l.strip_prefix("Commit:").or_else(|| l.strip_prefix("commit:")).map(|m| m.trim_matches(['*', ' ', '`']).to_string())
    }).filter(|m| !m.is_empty())
}

/// The timeline of `session` as of `now` (epoch seconds). `outer_busy`: its
/// outer loop is in a turn. `sub` names a lane's engine and model (`None`:
/// the outer loop's).
pub fn build(session: &Session, now: u64, outer_busy: bool, sub: &dyn Fn(Option<&Peer>) -> String) -> Timeline {
    let mut peers: Vec<&Peer> = session.peers().iter().filter(|p| !p.log().is_empty()).collect();
    peers.sort_by_key(|p| p.log().first().map(|e| e.at).unwrap_or(p.started_at));
    let starts = session.messages.iter().map(|m| m.started.unwrap_or(m.at))
        .chain(peers.iter().flat_map(|p| p.log().iter().map(|e| e.at)));
    let Some(first) = starts.min() else { return Timeline::default() };
    let rel = |t: u64| t.saturating_sub(first) as f64;
    let now_rel = rel(now);
    let mut tl = Timeline { origin: first, ..Default::default() };
    let detached = session.is_detached();
    if !detached {
        tl.lanes.push(Lane { key: format!("s:{}", session.id), name: "Outer".into(), sub: sub(None), kind: Kind::Outer, closed: false, history: None });
    }
    let base = tl.lanes.len();
    for p in &peers {
        let kind = if p.reviews_for.is_some() { Kind::Review } else { Kind::Inner };
        tl.lanes.push(Lane { key: format!("p:{}", p.id), name: p.slug.clone(), sub: sub(Some(p)), kind, closed: p.status == "closed", history: replaced(p, &peers) });
    }
    let lane_of = |slug: &str| peers.iter().position(|p| p.slug == slug).map(|i| base + i);

    // The outer loop's turns: its messages, up to the one a turn ends on.
    if !detached {
        let msgs = &session.messages;
        let mut turn: Vec<usize> = Vec::new();
        let (mut cause, mut pending_cause): (Option<usize>, Option<usize>) = (None, None);
        let mut n = 0;
        // `cut`: the turn ended there without finishing (OctoBuddy restarted,
        // the person stopped it): drawn up to then, as interrupted.
        // `asked`: when the message it took up was sent (the latest before it began).
        let mut close = |tl: &mut Timeline, turn: &mut Vec<usize>, cause: Option<usize>, asked: Option<f64>, open: bool, cut: Option<f64>| {
            let (Some(&a), Some(&z)) = (turn.first(), turn.last()) else { return };
            n += 1;
            let last = &msgs[z];
            let start = last.started.map(rel).unwrap_or_else(|| rel(msgs[a].at));
            let end = match (last.took, last.started, cut) {
                (None, _, Some(cut)) => cut.max(start + 1.0),
                (Some(t), Some(_), _) => start + t as f64,
                (Some(t), None, _) => (start + t as f64).max(rel(last.at)),
                (None, _, _) if open => now_rel,
                (None, _, _) => rel(last.at) + 1.0,
            };
            let steps: Vec<_> = turn.iter().filter_map(|&i| msgs[i].steps.as_ref()).flatten().collect();
            let (from, input) = match cause.map(|c| &msgs[c]) {
                Some(m) if m.role() == Role::User => ("you", first_line(&m.text, 200)),
                Some(m) if m.role() == Role::Peer => ("reports", format!("{}: {}", m.author, first_line(&m.text, 180))),
                Some(m) => ("octobuddy", first_line(&m.text, 200)),
                None => ("", String::new()),
            };
            let reply: String = turn.iter().map(|&i| msgs[i].text.as_str()).collect::<Vec<_>>().join("\n").chars().take(400).collect();
            tl.spans.push(Span {
                lane: 0, start, end: end.max(start + 1.0), open: open && cut.is_none(), turn: n, from: from.into(),
                outcome: if cut.is_some() && last.took.is_none() { "interrupted".into() } else { String::new() },
                steps: steps.len(), failed: steps.iter().filter(|s| s.status == "failed").count(), cost: last.cost,
                input, reply, message: Some(z), redone: false, asked,
            });
            turn.clear();
        };
        // The latest message sent to it, and when its turn under way was asked for.
        let (mut last_in, mut asked): (Option<f64>, Option<f64>) = (None, None);
        for (i, m) in msgs.iter().enumerate() {
            match m.role() {
                Role::Lead => {
                    if turn.is_empty() {
                        cause = pending_cause.take();
                        asked = last_in.take();
                    }
                    turn.push(i);
                    if m.took.is_some() {
                        close(&mut tl, &mut turn, cause, asked, false, None);
                    }
                }
                Role::User => {
                    last_in = Some(rel(m.at));
                    pending_cause = Some(i);
                    tl.marks.push(Mark { lane: 0, to: None, at: rel(m.at), kind: MarkKind::Person, text: first_line(&m.text, 120), until: None });
                }
                Role::Peer => {
                    last_in = Some(rel(m.at));
                    pending_cause = pending_cause.or(Some(i));
                    if let Some(lane) = lane_of(&m.author) {
                        tl.marks.push(Mark { lane, to: Some(0), at: rel(m.at), kind: MarkKind::Report, text: first_line(&m.text, 120), until: None });
                    }
                }
                // A restart or a stop ends the turn under way there.
                Role::System if cut_off(&m.text) && !turn.is_empty() => {
                    close(&mut tl, &mut turn, cause, asked, false, Some(rel(m.at)));
                }
                Role::System => {
                    last_in = Some(rel(m.at));
                    pending_cause = pending_cause.or(Some(i));
                }
                _ => {}
            }
        }
        if !turn.is_empty() {
            close(&mut tl, &mut turn, cause, asked, outer_busy, None);
        }
    }

    // Each inner loop's turns: its exchanges.
    for (j, p) in peers.iter().enumerate() {
        let lane = base + j;
        let mut before = Some(0.0);
        for (k, e) in p.log().iter().enumerate() {
            let open = e.outcome.is_none() && e.took.is_none() && p.is_active();
            // Sent but not taken up yet (queued, its process starting): no
            // work to draw. A turn starts when the agent began on it.
            if open && e.began.is_none() && p.status != "running" {
                continue;
            }
            let start = rel(e.began.unwrap_or(e.at));
            let end = match e.took {
                Some(t) => start + t as f64,
                None if open => now_rel,
                None => start + 1.0,
            };
            let steps = e.steps.as_deref().unwrap_or(&[]);
            let cost = match (e.cost_total, before) {
                (Some(total), Some(was)) => Some((total - was).max(0.0)),
                _ => None,
            };
            before = e.cost_total;
            let from = match e.from.as_str() { "lead" => "outer", "person" => "you", "subagents" => "itself", other => other };
            let reply = e.reply.clone().unwrap_or_default();
            if let Some(message) = has_commit(&reply).filter(|_| e.outcome.as_deref() == Some("completed")) {
                tl.marks.push(Mark { lane, to: None, at: end, kind: MarkKind::Commit, text: message, until: None });
            }
            match from {
                // Sent when OctoBuddy handed it over; taken up when the turn began.
                "outer" if !detached => tl.marks.push(Mark { lane: 0, to: Some(lane), at: rel(e.at).min(start), kind: MarkKind::Dispatch, text: first_line(&e.input, 120), until: Some(start) }),
                "you" => tl.marks.push(Mark { lane, to: None, at: start, kind: MarkKind::Person, text: first_line(&e.input, 120), until: None }),
                _ => {}
            }
            tl.spans.push(Span {
                lane, start, end: end.max(start + 1.0), open, turn: k + 1, from: from.into(), outcome: e.outcome.clone().unwrap_or_default(),
                steps: steps.len(), failed: steps.iter().filter(|s| s.status == "failed").count(), cost,
                input: e.input.chars().take(400).collect(), reply: reply.chars().take(400).collect(), message: None, redone: false,
                // Its first: asked when the slice was made (a later wave waits from then).
                asked: Some(if k == 0 && p.started_at > 0 && p.started_at < e.at { rel(p.started_at) } else { rel(e.at) }),
            });
        }
    }

    // Where each round began, when the session had more than one.
    let mut rounds: Vec<(f64, u64)> = Vec::new();
    for p in &peers {
        let Some(e) = p.log().first() else { continue };
        match rounds.iter_mut().find(|(_, r)| *r == p.round) {
            Some(r) => r.0 = r.0.min(rel(e.at)),
            None => rounds.push((rel(e.at), p.round)),
        }
    }
    rounds.sort_by(|a, b| a.0.total_cmp(&b.0));
    if rounds.len() > 1 {
        tl.rounds = rounds;
    }
    if !detached {
        join_marks(&mut tl);
    }
    rework(&mut tl, &peers, base, &rel);
    let running = tl.spans.iter().any(|s| s.open);
    tl.waits = waits(&tl, session, &peers, base, &rel, if running || outer_busy { None } else { Some(now_rel) });
    tl.marks.sort_by(|a, b| a.at.total_cmp(&b.at));
    let open = tl.spans.iter().any(|s| s.open);
    tl.end = tl.spans.iter().map(|s| s.end).chain(tl.marks.iter().map(|m| m.at)).fold(if open { now_rel } else { 0.0 }, f64::max).max(1.0);
    tl
}

/// Where each loop waited, and why (`Wait`): what an idle stretch was for.
fn waits(tl: &Timeline, session: &Session, peers: &[&Peer], base: usize, rel: &dyn Fn(u64) -> f64, idle_now: Option<f64>) -> Vec<Wait> {
    const MIN: f64 = 3.0;
    let mut out = Vec::new();
    let lane_spans = |lane: usize| -> Vec<&Span> { let mut v: Vec<&Span> = tl.spans.iter().filter(|s| s.lane == lane).collect(); v.sort_by(|a, b| a.start.total_cmp(&b.start)); v };
    // Each turn's start: from when it was asked (or its lane was free again).
    for lane in 0..tl.lanes.len() {
        let spans = lane_spans(lane);
        let peer = (lane >= base).then(|| peers.get(lane - base)).flatten();
        let first_wave = peer.map(|p| peers.iter().filter(|o| o.round == p.round).filter_map(|o| o.wave).min().unwrap_or(1));
        // A later wave's slice was made with its round (OctoBuddy said so: "Round
        // N: …"): its first turn of that round waited from then. An inner loop
        // used again keeps its earlier rounds' turns before it.
        let planned = peer.filter(|p| p.wave.unwrap_or(1) > first_wave.unwrap_or(1)).and_then(|p| {
            let (en, zh) = (format!("Round {}:", p.round), format!("第 {} 轮：", p.round));
            session.messages.iter().filter(|m| m.role() == Role::System && (m.text.starts_with(&en) || m.text.starts_with(&zh)))
                .map(|m| rel(m.at)).reduce(f64::max)
        });
        let wave_turn = planned.and_then(|t| spans.iter().position(|s| s.start >= t - 1.0));
        for (k, s) in spans.iter().enumerate() {
            let Some(mut asked) = s.asked else { continue };
            if Some(k) == wave_turn {
                asked = asked.min(planned.unwrap_or(asked));
            }
            let free = k.checked_sub(1).map(|i| spans[i].end).unwrap_or(f64::MIN);
            let from = asked.max(free);
            if s.start - from < MIN {
                continue;
            }
            let kind = match peer {
                Some(_) if Some(k) == wave_turn => "wave",
                Some(_) if free > asked => "start",
                Some(_) if k > 0 => "queue",
                _ => "start",
            };
            out.push(Wait { lane, from, to: s.start, kind });
        }
    }
    // A report: its inner loop's turn ended, OctoBuddy checked it, then it was said.
    for m in session.messages.iter().filter(|m| m.role() == Role::Peer) {
        let Some(j) = peers.iter().position(|p| p.slug == m.author) else { continue };
        let said = rel(m.at);
        let wrote = lane_spans(base + j).iter().filter(|s| s.end <= said + 1.0).map(|s| s.end).fold(f64::MIN, f64::max);
        if wrote > f64::MIN && said - wrote >= MIN {
            out.push(Wait { lane: base + j, from: wrote, to: said, kind: "check" });
        }
    }
    // No one at work: why, by what came next.
    let mut iv: Vec<(f64, f64)> = tl.spans.iter().map(|s| (s.start, s.end)).collect();
    iv.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut until = iv.first().map(|i| i.0).unwrap_or(0.0);
    for (a, b) in iv {
        if a - until >= 15.0 {
            // Why, by what ended it: you spoke, or (a short one) OctoBuddy came
            // back from a restart; a long stretch with nothing asked waited for you.
            let inside: Vec<&crate::model::Message> = session.messages.iter().filter(|m| rel(m.at) >= until && rel(m.at) <= a + 1.0).collect();
            let you = inside.iter().any(|m| m.role() == Role::User);
            let restarted = inside.iter().any(|m| m.role() == Role::System && cut_off(&m.text));
            let kind = if you || a - until > 600.0 { "person" } else if restarted { "restart" } else { "" };
            if !kind.is_empty() {
                // Up to where the next turn was asked (its own start is a wait of its own).
                let asked = tl.spans.iter().filter(|s| (s.start - a).abs() < 0.5).filter_map(|s| s.asked).fold(a, f64::min).max(until);
                if asked - until >= MIN {
                    out.push(Wait { lane: 0, from: until, to: asked, kind });
                }
            }
        }
        until = until.max(b);
    }
    // After the last turn: you were asked to go on (up to your message, or now).
    if let Some(now) = idle_now {
        let next = session.messages.iter().filter(|m| m.role() == Role::User).map(|m| rel(m.at)).find(|t| *t >= until);
        let to = next.unwrap_or(now);
        if to - until >= 15.0 {
            out.push(Wait { lane: 0, from: until, to, kind: "person" });
        }
    }
    out.sort_by(|a, b| a.from.total_cmp(&b.from));
    out
}

/// A message that says the outer loop's turn was cut off there.
fn cut_off(text: &str) -> bool {
    ["OctoBuddy restarted", "OctoBuddy 已重启", "Stopped by the person", "已由你停止"].iter().any(|s| text.starts_with(s))
}

/// Each line between the outer loop and an inner one, joined to the turns
/// at both its ends, so neither end points at a lane where nothing ran.
/// Work down leaves the outer turn that sent it (the one running then, or
/// the last before: OctoBuddy may hold it for a wave or a slot) and lands
/// where the inner turn began. A report leaves the end of the inner turn
/// that wrote it and lands where the outer turn that read it began (the
/// first after it; none yet: it waits).
fn join_marks(tl: &mut Timeline) {
    let outer: Vec<(f64, f64)> = tl.spans.iter().filter(|s| s.lane == 0).map(|s| (s.start, s.end)).collect();
    let lane_spans = |lane: usize| -> Vec<(f64, f64)> { tl.spans.iter().filter(|s| s.lane == lane).map(|s| (s.start, s.end)).collect() };
    let mut joined = Vec::with_capacity(tl.marks.len());
    for m in &tl.marks {
        let mut m = m.clone();
        match (m.kind, m.to) {
            (MarkKind::Dispatch, Some(_)) => {
                let (sent, lands) = (m.at, m.until.unwrap_or(m.at));
                // The outer turn it came from: running when it was sent, or the last before.
                let from = outer.iter().find(|(a, b)| *a <= sent + 0.5 && sent <= *b + 0.5).map(|_| sent)
                    .or_else(|| outer.iter().filter(|(_, b)| *b <= sent).map(|(_, b)| *b).fold(None, |acc: Option<f64>, b| Some(acc.map_or(b, |a: f64| a.max(b)))))
                    .unwrap_or(sent);
                m.at = from.min(lands);
                m.until = Some(lands);
            }
            (MarkKind::Report, Some(_)) => {
                let said = m.at;
                // The inner turn that wrote it: the last one ended by then.
                let wrote = lane_spans(m.lane).into_iter().filter(|(_, b)| *b <= said + 1.0).map(|(_, b)| b)
                    .fold(None, |acc: Option<f64>, b| Some(acc.map_or(b, |a: f64| a.max(b))));
                m.at = wrote.unwrap_or(said);
                // The outer turn that read it: the first to begin after it was said.
                m.until = outer.iter().map(|(a, _)| *a).filter(|a| *a >= said - 1.0)
                    .fold(None, |acc: Option<f64>, a| Some(acc.map_or(a, |x: f64| x.min(a))));
            }
            _ => {}
        }
        joined.push(m);
    }
    tl.marks = joined;
}

/// Why `p`'s work did not stand: closed unaccepted (replaced by a later
/// inner loop for the same slice, when one has its slug's stem).
fn replaced(p: &Peer, peers: &[&Peer]) -> Option<String> {
    let accepted = p.review.as_deref().is_some_and(|r| r.starts_with("accept"));
    if p.status != "closed" || accepted {
        return None;
    }
    let stem = |slug: &str| slug.rsplit_once('-').filter(|(_, n)| n.chars().all(|c| c.is_ascii_digit())).map(|(s, _)| s.to_string()).unwrap_or_else(|| slug.to_string());
    let after = peers.iter().find(|o| o.id != p.id && stem(&o.slug) == stem(&p.slug) && o.started_at >= p.started_at);
    Some(match after {
        Some(o) => i18n::pick(format!("replaced by {}", o.slug), format!("被 {} 取代", o.slug)),
        None => i18n::t("ended, its work not accepted", "已结束，工作未被接受").to_string(),
    })
}

/// Words the outer loop sends back work with.
fn sent_back(input: &str) -> bool {
    let low = input.to_lowercase();
    ["fix", "redo", "again", "did not", "didn't", "not deliver", "still", "nothing was", "ended before", "broken", "fails", "wrong", "missing",
        "修", "重做", "退回", "没有", "未", "仍", "还是", "不对", "失败"].iter().any(|w| low.contains(w))
}

/// A reference an inner loop reads to learn how to write an app (a doc of
/// the design flow, another app's source), by a step's detail.
fn reference(detail: &str) -> Option<String> {
    let names = ["SCRIPT-API.md", "AGENTS.md", "CAPABILITIES.md", "FLOW.md", "HOST-SERVICES.md", "QUICKSTART.md", "PUBLISHING.md"];
    if let Some(n) = names.iter().find(|n| detail.contains(*n)) {
        return Some(n.to_string());
    }
    // Another app's entry: `…/apps/<name>/bundle/main.splash`.
    let at = detail.find("/apps/")?;
    let rest = &detail[at + 6..];
    let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '*').collect();
    (rest[name.len()..].starts_with("/bundle/") && !name.is_empty()).then(|| format!("apps/{name} (source)"))
}

/// The rework in `tl`'s inner lanes (`peers` from lane `base` on).
fn rework(tl: &mut Timeline, peers: &[&Peer], base: usize, rel: &dyn Fn(u64) -> f64) {
    let mut found: Vec<Rework> = Vec::new();
    let mut learned: std::collections::BTreeMap<String, (std::collections::BTreeSet<usize>, usize)> = Default::default();
    for (j, p) in peers.iter().enumerate() {
        let lane = base + j;
        let spans: Vec<usize> = tl.spans.iter().enumerate().filter(|(_, s)| s.lane == lane).map(|(i, _)| i).collect();
        let log = p.log();
        for (k, e) in log.iter().enumerate() {
            for st in e.steps.iter().flatten() {
                if let Some(r) = reference(&st.detail) {
                    let entry = learned.entry(r).or_default();
                    entry.0.insert(lane);
                    entry.1 += 1;
                }
            }
            let Some(prev) = k.checked_sub(1).map(|i| &log[i]) else { continue };
            let reply = prev.reply.as_deref().unwrap_or("");
            let no_result = prev.outcome.as_deref() == Some("completed") && !reply.contains("octobuddy-report") && has_commit(reply).is_none();
            let (kind, why) = if e.from == "lead" && e.input.trim_start().starts_with("spec: task") {
                ("restarted", i18n::t("started afresh from its brief (another agent or model, or a new plan)", "从任务重新开始（换了 agent 或模型，或重新规划）").to_string())
            } else if prev.outcome.as_deref() == Some("failed") {
                ("failed", i18n::pick(format!("its turn failed: {}", first_line(reply, 140)), format!("上一轮失败：{}", first_line(reply, 140))))
            } else if e.from == "lead" && no_result {
                ("no result", first_line(&e.input, 160))
            } else if e.from == "lead" && sent_back(&e.input) {
                ("sent back", first_line(&e.input, 160))
            } else {
                continue;
            };
            let lost = prev.took.unwrap_or(0) as f64;
            if let Some(&i) = spans.get(k - 1) {
                tl.spans[i].redone = true;
            }
            let at = rel(e.began.unwrap_or(e.at));
            tl.marks.push(Mark { lane, to: None, at, kind: MarkKind::Rework, text: why.clone(), until: None });
            found.push(Rework { lane, at, kind, why, lost });
        }
        // Closed unaccepted: the whole lane is history.
        if let Some(why) = tl.lanes[lane].history.clone() {
            for &i in &spans {
                tl.spans[i].redone = true;
            }
            let at = spans.last().map(|&i| tl.spans[i].end).unwrap_or(0.0);
            let lost = spans.iter().map(|&i| tl.spans[i].end - tl.spans[i].start).sum();
            found.push(Rework { lane, at, kind: "replaced", why, lost });
        }
    }
    // The same reference learned by several inner loops, each on its own.
    for (r, (lanes, times)) in learned {
        if lanes.len() > 1 {
            let lane = *lanes.iter().next().unwrap();
            found.push(Rework { lane, at: 0.0, kind: "relearned", lost: 0.0,
                why: i18n::pick(format!("{} inner loops each read {r} ({times} reads): a shared, short reference would spare it", lanes.len()),
                    format!("{} 个 inner 各自读了 {r}（共 {times} 次）：给一份共享的精简参考就能省掉", lanes.len())) });
        }
    }
    found.sort_by(|a, b| a.at.total_cmp(&b.at));
    tl.rework = found;
}

impl Timeline {
    /// The rework found, as lines (for the list, and for the project's memory).
    pub fn rework_lines(&self) -> Vec<String> {
        self.rework.iter().map(|r| {
            let lane = self.lanes.get(r.lane).map(|l| format!("{} ({})", l.name, l.sub)).unwrap_or_default();
            let kind = match r.kind {
                "sent back" => i18n::t("sent back", "退回重做"),
                "no result" => i18n::t("turn with no result", "白跑一轮"),
                "failed" => i18n::t("failed, tried again", "失败后重来"),
                "restarted" => i18n::t("started afresh", "从头重来"),
                "replaced" => i18n::t("replaced", "被取代"),
                _ => i18n::t("learned again", "重复学习"),
            };
            let lost = if r.lost >= 1.0 { i18n::pick(format!(" · {} lost", span_len(r.lost)), format!(" · 浪费 {}", span_len(r.lost))) } else { String::new() };
            if r.kind == "relearned" {
                format!("{kind}: {}", r.why)
            } else {
                format!("{} {kind} · {lane}{lost}: {}", clock(r.at), r.why)
            }
        }).collect()
    }

    /// How long something ran, all lanes together (overlaps once).
    pub fn busy(&self) -> f64 {
        let mut iv: Vec<(f64, f64)> = self.spans.iter().map(|s| (s.start, s.end)).collect();
        iv.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (mut sum, mut until) = (0.0, f64::MIN);
        for (a, b) in iv {
            let a = a.max(until);
            if b > a {
                sum += b - a;
                until = b;
            }
        }
        sum
    }

    pub fn turns(&self, kind: Kind) -> usize {
        self.spans.iter().filter(|s| self.lanes[s.lane].kind == kind).count()
    }

    /// Each lane at time `t`: what it does.
    pub fn state_at(&self, t: f64) -> Vec<String> {
        self.lanes.iter().enumerate().map(|(i, lane)| {
            let mine: Vec<&Span> = self.spans.iter().filter(|s| s.lane == i).collect();
            if let Some(s) = mine.iter().find(|s| s.start <= t && t < s.end) {
                let from = who(&s.from);
                let from = if from.is_empty() { String::new() } else { i18n::pick(format!(" · for {from}"), format!(" · 来自 {from}")) };
                i18n::pick(format!("{}: turn {}, {} in{from}", lane.name, s.turn, span_len(t - s.start)),
                    format!("{}：第 {} 轮 · 已 {}{from}", lane.name, s.turn, span_len(t - s.start)))
            } else if let Some(s) = mine.iter().rev().find(|s| s.end <= t) {
                i18n::pick(format!("{}: idle, {} after turn {}", lane.name, span_len(t - s.end), s.turn),
                    format!("{}：空闲 · 第 {} 轮后 {}", lane.name, s.turn, span_len(t - s.end)))
            } else {
                i18n::pick(format!("{}: not started", lane.name), format!("{}：未开始", lane.name))
            }
        }).collect()
    }

    /// What happened up to `t`, the latest last: (time, line).
    pub fn events_until(&self, t: f64, n: usize) -> Vec<(f64, String)> {
        let name = |lane: usize| self.lanes.get(lane).map(|l| l.name.clone()).unwrap_or_default();
        let mut out: Vec<(f64, String)> = Vec::new();
        for m in self.marks.iter().filter(|m| m.at <= t) {
            let line = match m.kind {
                MarkKind::Person => i18n::pick(format!("you → {}: {}", name(m.lane), m.text), format!("你 → {}：{}", name(m.lane), m.text)),
                MarkKind::Dispatch => i18n::pick(format!("outer → {}: {}", name(m.to.unwrap_or(0)), m.text), format!("outer → {}：{}", name(m.to.unwrap_or(0)), m.text)),
                MarkKind::Report => i18n::pick(format!("{} → outer: report", name(m.lane)), format!("{} → outer：汇报", name(m.lane))),
                MarkKind::Commit => i18n::pick(format!("{} committed: {}", name(m.lane), m.text), format!("{} 已提交：{}", name(m.lane), m.text)),
                MarkKind::Rework => i18n::pick(format!("{} rework: {}", name(m.lane), m.text), format!("{} 返工：{}", name(m.lane), m.text)),
            };
            out.push((m.at, line));
        }
        for s in self.spans.iter().filter(|s| s.end <= t && !s.open) {
            let how = match s.outcome.as_str() {
                "failed" => i18n::t(" failed", " 失败"),
                "interrupted" => i18n::t(" interrupted", " 被中断"),
                _ => "",
            };
            out.push((s.end, i18n::pick(format!("{} turn {} ended{how} · {}", name(s.lane), s.turn, span_len(s.end - s.start)),
                format!("{} 第 {} 轮结束{how} · {}", name(s.lane), s.turn, span_len(s.end - s.start)))));
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        let skip = out.len().saturating_sub(n);
        out.into_iter().skip(skip).collect()
    }
}

/// Who a turn answered, in the interface's language.
pub fn who(from: &str) -> &'static str {
    match from {
        "you" => i18n::t("you", "你"),
        "outer" => "outer",
        "reports" => i18n::t("reports", "汇报"),
        "octobuddy" => "OctoBuddy",
        "itself" => i18n::t("itself (its subagents)", "自己（子 agent 完成）"),
        _ => "",
    }
}

/// A length of time, short: `45s`, `3m 05s`, `1h 02m`.
pub fn span_len(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
    }
}

/// A point on the axis: `+mm:ss` (or `+h:mm:ss`) from the start.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    if s >= 3600 { format!("+{}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60) } else { format!("+{:02}:{:02}", s / 60, s % 60) }
}

/// Idle longer than this is squeezed…
const IDLE: f64 = 20.0;
/// …to this many seconds on the axis.
const SQUEEZED: f64 = 4.0;

/// Real time to axis time: one to one where something ran, idle squeezed.
#[derive(Clone, Debug, Default)]
pub struct Axis {
    /// (real, axis) points, both increasing; linear between.
    points: Vec<(f64, f64)>,
    /// The squeezed stretches: (real from, real to, axis from, axis to).
    pub gaps: Vec<(f64, f64, f64, f64)>,
}

impl Axis {
    pub fn new(tl: &Timeline, squeeze: bool) -> Axis {
        let mut iv: Vec<(f64, f64)> = tl.spans.iter().map(|s| (s.start, s.end)).chain(tl.marks.iter().map(|m| (m.at, m.at))).collect();
        iv.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut axis = Axis { points: vec![(0.0, 0.0)], gaps: Vec::new() };
        let (mut covered, mut real, mut vis) = (0.0f64, 0.0f64, 0.0f64);
        if squeeze {
            for (a, b) in iv {
                if a - covered > IDLE {
                    let from = vis + (covered - real);
                    axis.points.push((covered, from));
                    axis.points.push((a, from + SQUEEZED));
                    axis.gaps.push((covered, a, from, from + SQUEEZED));
                    real = a;
                    vis = from + SQUEEZED;
                }
                covered = covered.max(b);
            }
        }
        let end = tl.end.max(real);
        axis.points.push((end, vis + (end - real)));
        axis
    }

    /// Its length, in axis seconds.
    pub fn len(&self) -> f64 {
        self.points.last().map(|p| p.1).unwrap_or(0.0).max(1.0)
    }

    fn map(points: &[(f64, f64)], x: f64, from: fn(&(f64, f64)) -> f64, to: fn(&(f64, f64)) -> f64) -> f64 {
        let Some(last) = points.last() else { return x };
        if x >= from(last) {
            return to(last) + (x - from(last));
        }
        for w in points.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            if x <= from(b) {
                let span = from(b) - from(a);
                return if span <= 0.0 { to(a) } else { to(a) + (x - from(a)) / span * (to(b) - to(a)) };
            }
        }
        x
    }

    pub fn vis(&self, real: f64) -> f64 {
        Self::map(&self.points, real, |p| p.0, |p| p.1)
    }

    pub fn real(&self, vis: f64) -> f64 {
        Self::map(&self.points, vis, |p| p.1, |p| p.0)
    }
}

#[derive(Clone, Debug, Default)]
pub enum TimeAction {
    #[default]
    None,
    /// The playhead moved (real seconds).
    Moved(f64),
    /// A bar was picked, or the pick dropped.
    Selected(Option<usize>),
    /// A bar was double-clicked: open its turn in the conversation.
    Open(usize),
}

const GUTTER: f64 = 150.0;
const AXIS_H: f64 = 30.0;
/// Every lane's height, however many there are: more lanes scroll (the
/// wheel over the names, or the bar at the right), they never get thinner.
const LANE_H: f64 = 40.0;
const RIGHT: f64 = 14.0;

fn rgb(hex: u32, a: f32) -> Vec4f {
    Vec4f { x: ((hex >> 16) & 0xff) as f32 / 255.0, y: ((hex >> 8) & 0xff) as f32 / 255.0, z: (hex & 0xff) as f32 / 255.0, w: a }
}

#[derive(Script, Widget)]
pub struct TimelineCanvas {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_bg: DrawColor,
    #[live]
    draw_vector: DrawVector,
    #[live]
    draw_text: DrawText,
    #[rust]
    tl: Timeline,
    #[rust]
    axis: Axis,
    /// Idle is shown at its real length.
    #[rust]
    unsqueezed: bool,
    /// The axis window shown (axis seconds); `None`: all of it.
    #[rust]
    window: Option<(f64, f64)>,
    /// The playhead, in axis seconds.
    #[rust]
    t: f64,
    /// It is not at the end: a new timeline does not move it there.
    #[rust]
    placed: bool,
    #[rust]
    playing: bool,
    /// Axis seconds per second of playback (0: the default).
    #[rust]
    speed: f64,
    #[rust]
    last_frame: Option<f64>,
    #[rust]
    hover: Option<usize>,
    #[rust]
    pointer: DVec2,
    #[rust]
    selected: Option<usize>,
    #[rust]
    scrubbing: bool,
    /// How far the lanes are scrolled up (points).
    #[rust]
    scroll_y: f64,
    /// Dragging the scroll bar: where on its thumb it was taken.
    #[rust]
    dragging_bar: Option<f64>,
    #[rust]
    rect: Rect,
    #[rust]
    area: Area,
    #[rust]
    next_frame: NextFrame,
}

impl ScriptHook for TimelineCanvas {}

impl TimelineCanvas {
    fn speed(&self) -> f64 {
        if self.speed > 0.0 { self.speed } else { 20.0 }
    }

    fn shown(&self) -> (f64, f64) {
        self.window.unwrap_or((0.0, self.axis.len()))
    }

    fn lane_h(&self) -> f64 {
        LANE_H
    }

    fn lanes_top(&self) -> f64 {
        self.rect.pos.y + AXIS_H
    }

    fn view_h(&self) -> f64 {
        (self.rect.size.y - AXIS_H).max(0.0)
    }

    /// How far the lanes can scroll (0: they all fit).
    fn max_scroll(&self) -> f64 {
        (self.tl.lanes.len() as f64 * LANE_H + 6.0 - self.view_h()).max(0.0)
    }

    fn scroll_to(&mut self, y: f64) {
        self.scroll_y = y.clamp(0.0, self.max_scroll());
    }

    /// Whether any of a lane shows (below the axis, above the bottom).
    fn lane_visible(&self, lane: usize) -> bool {
        let y = self.lane_y(lane);
        y + LANE_H > self.lanes_top() && y < self.rect.pos.y + self.rect.size.y
    }

    /// The scroll bar's thumb (`None`: nothing to scroll).
    fn thumb(&self) -> Option<Rect> {
        let max = self.max_scroll();
        if max <= 0.0 {
            return None;
        }
        let view = self.view_h();
        let h = (view * view / (view + max)).max(24.0).min(view);
        let y = self.lanes_top() + self.scroll_y / max * (view - h);
        Some(Rect { pos: dvec2(self.rect.pos.x + self.rect.size.x - RIGHT + 5.0, y), size: dvec2(5.0, h) })
    }

    fn plot(&self) -> (f64, f64) {
        (self.rect.pos.x + GUTTER, (self.rect.pos.x + self.rect.size.x - RIGHT).max(self.rect.pos.x + GUTTER + 10.0))
    }

    /// Axis seconds to a screen x.
    fn x(&self, vis: f64) -> f64 {
        let (v0, v1) = self.shown();
        let (x0, x1) = self.plot();
        x0 + (vis - v0) / (v1 - v0).max(1e-6) * (x1 - x0)
    }

    fn vis_at(&self, x: f64) -> f64 {
        let (v0, v1) = self.shown();
        let (x0, x1) = self.plot();
        v0 + (x - x0) / (x1 - x0).max(1.0) * (v1 - v0)
    }

    fn lane_y(&self, lane: usize) -> f64 {
        self.rect.pos.y + AXIS_H + lane as f64 * self.lane_h() - self.scroll_y.min(self.max_scroll())
    }

    /// A turn's bar, the part of it inside the plot (`None`: none of it is,
    /// zoomed in elsewhere). At least 3 points wide, so a short turn shows.
    fn bar(&self, s: &Span) -> Option<Rect> {
        if !self.lane_visible(s.lane) {
            return None;
        }
        let h = (self.lane_h() - 10.0).clamp(10.0, 18.0);
        let (x0, x1) = self.plot();
        let (ra, rb) = (self.x(self.axis.vis(s.start)), self.x(self.axis.vis(s.end)));
        let rb = rb.max(ra + 3.0);
        if rb < x0 || ra > x1 {
            return None;
        }
        let (a, b) = (ra.max(x0), rb.min(x1));
        Some(Rect { pos: dvec2(a, self.lane_y(s.lane) + (self.lane_h() - h) * 0.5), size: dvec2((b - a).max(1.0), h) })
    }

    fn span_at(&self, p: DVec2) -> Option<usize> {
        if p.y < self.lanes_top() {
            return None;
        }
        self.tl.spans.iter().enumerate().rev()
            .find(|(_, s)| self.bar(s).is_some_and(|r| {
                p.x >= r.pos.x - 2.0 && p.x <= r.pos.x + r.size.x + 2.0 && p.y >= r.pos.y - 3.0 && p.y <= r.pos.y + r.size.y + 3.0
            }))
            .map(|(i, _)| i)
    }

    fn move_to(&mut self, cx: &mut Cx, vis: f64) {
        let len = self.axis.len();
        self.t = vis.clamp(0.0, len);
        self.placed = self.t < len - 0.5;
        cx.widget_action(self.uid, TimeAction::Moved(self.axis.real(self.t)));
        self.redraw(cx);
    }

    /// Keeps the playhead in the window while it plays.
    fn follow(&mut self) {
        let Some((v0, v1)) = self.window else { return };
        let w = v1 - v0;
        if self.t > v1 - w * 0.05 || self.t < v0 {
            let a = (self.t - w * 0.2).max(0.0);
            self.window = Some((a, a + w));
        }
    }

    fn text(&mut self, cx: &mut Cx2d, pos: DVec2, text: &str, size: f32, color: Vec4f) {
        self.draw_text.text_style.font_size = size;
        self.draw_text.color = color;
        self.draw_text.draw_abs(cx, pos, text);
    }

    fn text_w(&mut self, cx: &mut Cx2d, text: &str, size: f32) -> f64 {
        self.draw_text.text_style.font_size = size;
        self.draw_text.layout(cx, 0.0, 0.0, None, false, Align::default(), text).size_in_lpxs.width as f64
    }

    fn span_color(&self, s: &Span) -> u32 {
        match (s.outcome.as_str(), self.tl.lanes[s.lane].kind) {
            ("failed", _) => crate::theme::hex("danger"),
            ("interrupted", _) => crate::theme::hex("faint"),
            (_, Kind::Outer) => crate::theme::hex("accent"),
            (_, Kind::Inner) => crate::theme::hex("success"),
            (_, Kind::Review) => crate::theme::hex("purple"),
        }
    }

    /// The hovered bar's lines, for its tooltip.
    fn tip(&self, s: &Span) -> Vec<String> {
        let lane = &self.tl.lanes[s.lane];
        let from = who(&s.from);
        let head = if from.is_empty() {
            i18n::pick(format!("{} · turn {}", lane.name, s.turn), format!("{} · 第 {} 轮", lane.name, s.turn))
        } else {
            i18n::pick(format!("{} · turn {} · for {from}", lane.name, s.turn), format!("{} · 第 {} 轮 · 来自 {from}", lane.name, s.turn))
        };
        let mut when = format!("{} → {} · {}", clock(s.start), if s.open { i18n::t("now", "现在").to_string() } else { clock(s.end) }, span_len(s.end - s.start));
        if s.steps > 0 {
            when.push_str(&i18n::pick(format!(" · {} tool call(s)", s.steps), format!(" · {} 次工具调用", s.steps)));
        }
        if let Some(c) = s.cost {
            when.push_str(&format!(" · ≈${c:.3}"));
        }
        let mut lines = vec![head, when];
        let input: String = first_line(&s.input, 46);
        if !input.is_empty() {
            lines.push(input);
        }
        if s.redone {
            lines.push(i18n::t("Done again later: see Rework below", "这段工作后来重做了：见下方「返工」").to_string());
        }
        if let Some(w) = self.tl.waits.iter().find(|w| w.lane == s.lane && (w.to - s.start).abs() < 0.5) {
            lines.push(i18n::pick(format!("Before it: {} ({})", w.why(), span_len(w.to - w.from)), format!("开始前：{}（{}）", w.why(), span_len(w.to - w.from))));
        }
        lines
    }
}

impl Widget for TimelineCanvas {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        if let Some(ev) = self.next_frame.is_event(event) {
            if self.playing {
                let dt = self.last_frame.map(|l| ev.time - l).unwrap_or(0.0).clamp(0.0, 0.25);
                self.last_frame = Some(ev.time);
                let len = self.axis.len();
                self.t = (self.t + dt * self.speed()).min(len);
                if self.t >= len {
                    self.playing = false;
                    self.placed = false;
                }
                self.follow();
                cx.widget_action(uid, TimeAction::Moved(self.axis.real(self.t)));
                self.redraw(cx);
            }
        }
        match event.hits(cx, self.area) {
            Hit::FingerDown(fe) if fe.is_primary_hit() && fe.abs.x >= self.rect.pos.x + self.rect.size.x - RIGHT && self.thumb().is_some() => {
                // The scroll bar: taken by its thumb, or jumped to.
                let t = self.thumb().unwrap();
                let grab = if fe.abs.y >= t.pos.y && fe.abs.y <= t.pos.y + t.size.y { fe.abs.y - t.pos.y } else { t.size.y * 0.5 };
                self.dragging_bar = Some(grab);
                let span = (self.view_h() - t.size.y).max(1.0);
                self.scroll_to((fe.abs.y - grab - self.lanes_top()) / span * self.max_scroll());
                self.redraw(cx);
            }
            Hit::FingerMove(fe) if self.dragging_bar.is_some() => {
                let (grab, h) = (self.dragging_bar.unwrap_or(0.0), self.thumb().map(|t| t.size.y).unwrap_or(0.0));
                let span = (self.view_h() - h).max(1.0);
                self.scroll_to((fe.abs.y - grab - self.lanes_top()) / span * self.max_scroll());
                self.redraw(cx);
            }
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                match self.span_at(fe.abs) {
                    Some(i) if fe.tap_count >= 2 => cx.widget_action(uid, TimeAction::Open(i)),
                    Some(i) => {
                        self.selected = Some(i);
                        cx.widget_action(uid, TimeAction::Selected(Some(i)));
                        self.redraw(cx);
                    }
                    None => {
                        self.scrubbing = true;
                        self.playing = false;
                        let v = self.vis_at(fe.abs.x);
                        self.move_to(cx, v);
                    }
                }
            }
            Hit::FingerMove(fe) if self.scrubbing => {
                let v = self.vis_at(fe.abs.x);
                self.move_to(cx, v);
            }
            Hit::FingerUp(_) => {
                self.scrubbing = false;
                self.dragging_bar = None;
            }
            // Over the names or the scroll bar: the lanes scroll.
            Hit::FingerScroll(fe) if self.max_scroll() > 0.0
                && (fe.abs.x < self.rect.pos.x + GUTTER || fe.abs.x >= self.rect.pos.x + self.rect.size.x - RIGHT) => {
                let y = self.scroll_y + fe.scroll.y;
                self.scroll_to(y);
                self.redraw(cx);
            }
            Hit::FingerScroll(fe) => {
                let (v0, v1) = self.shown();
                let len = self.axis.len();
                let w = v1 - v0;
                if fe.scroll.y.abs() > fe.scroll.x.abs() {
                    // Zoom around the pointer.
                    let at = self.vis_at(fe.abs.x).clamp(v0, v1);
                    let k = (fe.scroll.y * 0.004).exp();
                    let nw = (w * k).clamp(5.0_f64.min(len), len);
                    if nw >= len - 1e-6 {
                        self.window = None;
                    } else {
                        let f = (at - v0) / w.max(1e-6);
                        let a = (at - f * nw).clamp(0.0, len - nw);
                        self.window = Some((a, a + nw));
                    }
                } else if self.window.is_some() {
                    let (x0, x1) = self.plot();
                    let d = fe.scroll.x / (x1 - x0).max(1.0) * w;
                    let a = (v0 + d).clamp(0.0, (len - w).max(0.0));
                    self.window = Some((a, a + w));
                }
                self.redraw(cx);
            }
            Hit::FingerHoverIn(fe) | Hit::FingerHoverOver(fe) => {
                self.pointer = fe.abs;
                let hover = self.span_at(fe.abs);
                cx.set_cursor(if hover.is_some() { MouseCursor::Hand } else { MouseCursor::Default });
                if hover != self.hover {
                    self.hover = hover;
                }
                if self.hover.is_some() || hover.is_some() {
                    self.redraw(cx);
                }
            }
            Hit::FingerHoverOut(_) if self.hover.take().is_some() => self.redraw(cx),
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.walk_turtle(walk);
        self.rect = rect;
        self.draw_bg.draw_abs(cx, rect);
        cx.begin_turtle(
            Walk { abs_pos: Some(rect.pos), width: Size::Fixed(rect.size.x), height: Size::Fixed(rect.size.y), ..Default::default() },
            Layout { clip_x: true, clip_y: true, ..Layout::default() },
        );
        let (x0, x1) = self.plot();
        let lane_h = self.lane_h();
        let lanes = self.tl.lanes.len();
        let bottom = self.lane_y(lanes).min(rect.pos.y + rect.size.y);
        let top = self.lanes_top();
        let now = self.axis.real(self.t);
        let (v0, v1) = self.shown();
        // What the texts are, drawn after the shapes.
        let mut texts: Vec<(DVec2, String, f32, Vec4f)> = Vec::new();

        self.draw_vector.begin();
        // Lanes: a light stripe every other one; a lane whose work did not
        // stand (history) hatched, apart from the work that did.
        for i in 0..lanes {
            if i % 2 == 1 && self.lane_visible(i) {
                self.draw_vector.set_color_hex(crate::theme::hex("panel"), 1.0);
                self.draw_vector.rect(rect.pos.x as f32, self.lane_y(i) as f32, rect.size.x as f32, lane_h as f32);
                self.draw_vector.fill();
            }
            if self.tl.lanes[i].history.is_some() && self.lane_visible(i) {
                let (y, h) = (self.lane_y(i), lane_h);
                let mut x = x0 - h;
                while x < x1 {
                    self.draw_vector.set_color_hex(crate::theme::hex("faint"), 0.28);
                    self.draw_vector.move_to(x.max(x0) as f32, (y + h - (x.max(x0) - x)) as f32);
                    self.draw_vector.line_to((x + h).min(x1) as f32, (y + h - ((x + h).min(x1) - x)) as f32);
                    self.draw_vector.stroke(1.0);
                    x += 9.0;
                }
            }
        }
        // Idle squeezed away: a band, and how long it was.
        for (r0, r1, a, b) in self.axis.gaps.clone() {
            if b < v0 || a > v1 {
                continue;
            }
            let (xa, xb) = (self.x(a).max(x0), self.x(b).min(x1));
            self.draw_vector.set_color_hex(crate::theme::hex("hover"), 1.0);
            self.draw_vector.rect(xa as f32, (rect.pos.y + AXIS_H) as f32, (xb - xa).max(1.0) as f32, (bottom - rect.pos.y - AXIS_H) as f32);
            self.draw_vector.fill();
            if xb - xa > 18.0 {
                texts.push((dvec2(xa + 2.0, rect.pos.y + 16.0), format!("≈{}", span_len(r1 - r0)), 7.0, rgb(crate::theme::hex("faint"), 1.0)));
            }
        }
        // Rounds: a dashed line where each began (a label each, unless the
        // last one is too near to read).
        let mut last_label = f64::MIN;
        for (at, round) in self.tl.rounds.clone() {
            let x = self.x(self.axis.vis(at));
            if x < x0 || x > x1 {
                continue;
            }
            let labelled = x - last_label > 56.0;
            let mut y = rect.pos.y + AXIS_H;
            while y < bottom {
                self.draw_vector.set_color_hex(crate::theme::hex("purple"), 0.5);
                self.draw_vector.rect(x as f32, y as f32, 1.0, 4.0);
                self.draw_vector.fill();
                y += 7.0;
            }
            // At the line's foot: the axis's top is the playhead's.
            if labelled {
                last_label = x;
                texts.push((dvec2(x + 3.0, bottom - 12.0), i18n::pick(format!("round {round}"), format!("第 {round} 轮")), 7.0, rgb(crate::theme::hex("purple"), 1.0)));
            }
        }
        // The turns: solid up to the playhead, faint after.
        let tip_rect = self.hover.and_then(|h| self.tl.spans.get(h)).map(|_| Rect::default());
        let _ = tip_rect;
        for (i, s) in self.tl.spans.clone().iter().enumerate() {
            let Some(r) = self.bar(s) else { continue };
            let color = self.span_color(s);
            let cut = if s.start >= now { r.pos.x } else if s.end <= now { r.pos.x + r.size.x } else { self.x(self.t).clamp(r.pos.x, r.pos.x + r.size.x) };
            self.draw_vector.set_color_hex(color, 0.16);
            self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, r.size.x as f32, r.size.y as f32, 3.0);
            self.draw_vector.fill();
            if cut > r.pos.x + 0.5 {
                self.draw_vector.set_color_hex(color, if s.redone { 0.38 } else if s.open { 0.75 } else { 1.0 });
                self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, (cut - r.pos.x).max(2.0) as f32, r.size.y as f32, 3.0);
                self.draw_vector.fill();
            }
            // Done again later: its outline says so.
            if s.redone {
                self.draw_vector.set_color_hex(crate::theme::hex("warning"), 0.9);
                self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, r.size.x as f32, r.size.y as f32, 3.0);
                self.draw_vector.stroke(1.0);
            }
            if self.selected == Some(i) || self.hover == Some(i) {
                self.draw_vector.set_color_hex(if self.selected == Some(i) { crate::theme::hex("ink") } else { crate::theme::hex("muted_strong") }, 1.0);
                self.draw_vector.rounded_rect((r.pos.x - 1.5) as f32, (r.pos.y - 1.5) as f32, (r.size.x + 3.0) as f32, (r.size.y + 3.0) as f32, 4.0);
                self.draw_vector.stroke(1.3);
            }
            // Its number and length, inside, when there is room.
            let label = format!("#{} · {}", s.turn, span_len(s.end - s.start));
            let label = if s.steps > 0 { i18n::pick(format!("{label} · {} calls", s.steps), format!("{label} · {} 次调用", s.steps)) } else { label };
            let w = self.text_w(cx, &label, 7.5);
            let short = format!("#{}", s.turn);
            let ws = self.text_w(cx, &short, 7.5);
            let solid = cut - r.pos.x;
            let ink = if solid > 8.0 { rgb(crate::theme::hex("on_accent"), 1.0) } else { rgb(color, 1.0) };
            if r.pos.y < top {
                // Partly under the axis: no label.
            } else if r.size.x > w + 8.0 {
                texts.push((dvec2(r.pos.x + 4.0, r.pos.y + (r.size.y - 10.0) * 0.5), label, 7.5, ink));
            } else if r.size.x > ws + 6.0 {
                texts.push((dvec2(r.pos.x + 3.0, r.pos.y + (r.size.y - 10.0) * 0.5), short, 7.5, ink));
            }
        }
        // Waits: a dotted line along the lane, with why (when there is room).
        for w in self.tl.waits.clone() {
            if !self.lane_visible(w.lane) {
                continue;
            }
            let (xa, xb) = (self.x(self.axis.vis(w.from)).max(x0), self.x(self.axis.vis(w.to)).min(x1));
            if xb - xa < 2.0 {
                continue;
            }
            let y = self.lane_y(w.lane) + lane_h * 0.5;
            let color = if w.kind == "person" { crate::theme::hex("warning") } else { crate::theme::hex("muted") };
            let mut x = xa;
            while x < xb {
                self.draw_vector.set_color_hex(color, if w.from <= now { 0.7 } else { 0.2 });
                self.draw_vector.rect(x as f32, (y - 0.6) as f32, (3.0f64).min(xb - x) as f32, 1.2);
                self.draw_vector.fill();
                x += 6.0;
            }
            if xb - xa > 70.0 {
                // Within its own stretch, measured: the next one's label starts after it.
                let full = format!("{} · {}", w.why(), span_len(w.to - w.from));
                let room = xb - xa - 8.0;
                let mut label = full.clone();
                while label.chars().count() > 3 && self.text_w(cx, &label, 7.0) > room {
                    let keep = label.chars().count() - 2;
                    label = format!("{}…", full.chars().take(keep.saturating_sub(1)).collect::<String>());
                }
                if self.text_w(cx, &label, 7.0) <= room {
                    texts.push((dvec2(xa + 4.0, y - 12.0), label, 7.0, rgb(color, 0.95)));
                }
            }
        }
        // Between the lanes: work down, reports up; the person; commits.
        for m in self.tl.marks.clone() {
            let x = self.x(self.axis.vis(m.at));
            // A line between two lanes is drawn when either end shows.
            let xb = m.until.map(|u| self.x(self.axis.vis(u))).unwrap_or(x);
            let joins = matches!(m.kind, MarkKind::Dispatch | MarkKind::Report) && m.to.is_some();
            let off = |x: f64| x < x0 - 1.0 || x > x1 + 1.0;
            if if joins { off(x) && off(xb) && (x < x0) == (xb < x0) } else { off(x) } {
                continue;
            }
            let alpha = if m.at <= now + 0.01 { 1.0 } else { 0.18 };
            let mid = |lane: usize| self.lane_y(lane) + lane_h * 0.5;
            match (m.kind, m.to) {
                (MarkKind::Dispatch | MarkKind::Report, Some(to)) => {
                    let color = if m.kind == MarkKind::Dispatch { crate::theme::hex("accent") } else { crate::theme::hex("purple") };
                    // From the edge of the bar it leaves to the edge of the bar it
                    // lands on: the bars' tops and bottoms, as `bar` places them.
                    let h = (lane_h - 10.0).clamp(10.0, 18.0);
                    let edge = |lane: usize, low: bool| self.lane_y(lane) + (lane_h - h) * 0.5 + if low { h } else { 0.0 };
                    let down = self.lane_y(to) > self.lane_y(m.lane);
                    let (ya, yb) = (edge(m.lane, down), edge(to, !down));
                    let dir = if down { 1.0 } else { -1.0 };
                    let _ = mid;
                    // Both ends scrolled out on the same side: nothing shows.
                    if (ya < top && yb < top) || (ya > bottom && yb > bottom) {
                        continue;
                    }
                    // Not taken up yet: a dashed line to where it waits.
                    let waiting = m.until.is_none();
                    // Landed after the playhead: on its way.
                    let alpha = match m.until { Some(u) if u > now + 0.01 && m.at <= now + 0.01 => 0.5, _ => alpha };
                    // An S from one end to the other (straight down when they meet).
                    let pts: Vec<(f64, f64)> = (0..=18).map(|i| {
                        let t = i as f64 / 18.0;
                        let (c1, c2) = ((x, ya + (yb - ya) * 0.55), (xb, ya + (yb - ya) * 0.45));
                        let u = 1.0 - t;
                        let px = u * u * u * x + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * xb;
                        let py = u * u * u * ya + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * (yb - dir * 4.0);
                        (px, py)
                    }).collect();
                    self.draw_vector.set_color_hex(color, alpha * 0.85);
                    for (k, w) in pts.windows(2).enumerate() {
                        if waiting && k % 2 == 1 {
                            continue;
                        }
                        self.draw_vector.move_to(w[0].0 as f32, w[0].1 as f32);
                        self.draw_vector.line_to(w[1].0 as f32, w[1].1 as f32);
                        self.draw_vector.stroke(1.2);
                    }
                    // Its head, on the bar it lands on.
                    self.draw_vector.move_to(xb as f32, yb as f32);
                    self.draw_vector.line_to((xb - 3.5) as f32, (yb - dir * 5.0) as f32);
                    self.draw_vector.line_to((xb + 3.5) as f32, (yb - dir * 5.0) as f32);
                    self.draw_vector.close();
                    self.draw_vector.fill();
                    // How long it waited, between its ends, when that shows.
                    let wait = m.until.map(|u| u - m.at).unwrap_or(0.0);
                    let label = if waiting {
                        Some(i18n::t("waits for the outer loop", "等外环处理").to_string())
                    } else if wait >= 20.0 && (xb - x).abs() > 46.0 {
                        Some(i18n::pick(format!("waited {}", span_len(wait)), format!("等了 {}", span_len(wait))))
                    } else {
                        None
                    };
                    if let Some(label) = label {
                        // Inside the plot, its right end too.
                        let w = self.text_w(cx, &label, 7.0);
                        let (lx, ly) = (((x + xb) * 0.5 + 4.0).min(x1 - w - 6.0).max(x0 + 2.0), (ya + yb) * 0.5 - 5.0);
                        if ly > top && ly < bottom {
                            texts.push((dvec2(lx, ly), label, 7.0, rgb(color, 0.9 * alpha)));
                        }
                    }
                }
                (MarkKind::Person, _) if self.lane_visible(m.lane) => {
                    let y = self.lane_y(m.lane) + 4.0;
                    self.draw_vector.set_color_hex(crate::theme::hex("warning"), alpha);
                    self.draw_vector.circle(x as f32, y as f32, 3.2);
                    self.draw_vector.fill();
                }
                (MarkKind::Rework, _) if self.lane_visible(m.lane) => {
                    // A ring above the bar's start: work done again here (not a
                    // triangle: those are the lines' heads).
                    let h = (lane_h - 10.0).clamp(10.0, 18.0);
                    let y = self.lane_y(m.lane) + (lane_h - h) * 0.5 - 4.0;
                    self.draw_vector.set_color_hex(crate::theme::hex("warning"), alpha);
                    self.draw_vector.circle((x + 6.0) as f32, y as f32, 3.6);
                    self.draw_vector.stroke(1.6);
                }
                (MarkKind::Commit, _) if self.lane_visible(m.lane) => {
                    let y = self.lane_y(m.lane) + lane_h - 5.0;
                    self.draw_vector.set_color_hex(crate::theme::hex("success"), alpha);
                    self.draw_vector.move_to(x as f32, (y - 3.5) as f32);
                    self.draw_vector.line_to((x + 3.5) as f32, y as f32);
                    self.draw_vector.line_to(x as f32, (y + 3.5) as f32);
                    self.draw_vector.line_to((x - 3.5) as f32, y as f32);
                    self.draw_vector.close();
                    self.draw_vector.fill();
                }
                _ => {}
            }
        }
        // The gutter: over anything that ran past the left edge.
        self.draw_vector.set_color_hex(crate::theme::hex("raised"), 1.0);
        self.draw_vector.rect(rect.pos.x as f32, top as f32, (GUTTER - 6.0) as f32, (bottom - top).max(0.0) as f32);
        self.draw_vector.fill();
        for i in 0..lanes {
            if !self.lane_visible(i) {
                continue;
            }
            if i % 2 == 1 {
                self.draw_vector.set_color_hex(crate::theme::hex("panel"), 1.0);
                self.draw_vector.rect(rect.pos.x as f32, self.lane_y(i) as f32, (GUTTER - 6.0) as f32, lane_h as f32);
                self.draw_vector.fill();
            }
            let dot = match self.tl.lanes[i].kind { Kind::Outer => crate::theme::hex("accent"), Kind::Inner => crate::theme::hex("success"), Kind::Review => crate::theme::hex("purple") };
            self.draw_vector.set_color_hex(dot, if self.tl.lanes[i].closed { 0.4 } else { 1.0 });
            self.draw_vector.circle((rect.pos.x + 10.0) as f32, (self.lane_y(i) + if lane_h >= 30.0 { 11.0 } else { lane_h * 0.5 }) as f32, 3.5);
            self.draw_vector.fill();
        }
        // The axis' band, over the lanes scrolled under it.
        self.draw_vector.set_color_hex(crate::theme::hex("raised"), 1.0);
        self.draw_vector.rect(rect.pos.x as f32, rect.pos.y as f32, rect.size.x as f32, (AXIS_H - 1.0) as f32);
        self.draw_vector.fill();
        // The axis: a rule, ticks every round interval of real time.
        self.draw_vector.set_color_hex(crate::theme::hex("line"), 1.0);
        self.draw_vector.rect(x0 as f32, (rect.pos.y + AXIS_H - 1.0) as f32, (x1 - x0) as f32, 1.0);
        self.draw_vector.fill();
        let (r0, r1) = (self.axis.real(v0), self.axis.real(v1));
        let px_per_s = (x1 - x0) / (v1 - v0).max(1e-6);
        let step = [1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0, 600.0, 900.0, 1800.0, 3600.0, 7200.0]
            .into_iter().find(|s| s * px_per_s >= 70.0).unwrap_or(7200.0);
        let mut tick = (r0 / step).ceil() * step;
        while tick <= r1 {
            let squeezed = self.axis.gaps.iter().any(|g| tick > g.0 && tick < g.1);
            if !squeezed {
                let x = self.x(self.axis.vis(tick));
                self.draw_vector.set_color_hex(crate::theme::hex("line"), 1.0);
                self.draw_vector.rect(x as f32, (rect.pos.y + AXIS_H - 5.0) as f32, 1.0, 4.0);
                self.draw_vector.fill();
                let label = clock(tick);
                if x + 3.0 + self.text_w(cx, &label, 7.5) <= x1 + RIGHT - 2.0 {
                    texts.push((dvec2(x + 3.0, rect.pos.y + 4.0), label, 7.5, rgb(crate::theme::hex("muted"), 1.0)));
                }
            }
            tick += step;
        }
        // The scroll bar, when the lanes do not all fit.
        if let Some(t) = self.thumb() {
            self.draw_vector.set_color_hex(crate::theme::hex("hover"), 1.0);
            self.draw_vector.rounded_rect(t.pos.x as f32, top as f32, t.size.x as f32, self.view_h() as f32, 2.5);
            self.draw_vector.fill();
            self.draw_vector.set_color_hex(if self.dragging_bar.is_some() { crate::theme::hex("muted_strong") } else { crate::theme::hex("faint") }, 1.0);
            self.draw_vector.rounded_rect(t.pos.x as f32, t.pos.y as f32, t.size.x as f32, t.size.y as f32, 2.5);
            self.draw_vector.fill();
        }
        // The playhead.
        let xp = self.x(self.t);
        if xp >= x0 - 1.0 && xp <= x1 + 1.0 {
            self.draw_vector.set_color_hex(crate::theme::hex("danger"), 1.0);
            self.draw_vector.rect((xp - 0.75) as f32, (rect.pos.y + AXIS_H - 6.0) as f32, 1.5, (bottom - rect.pos.y - AXIS_H + 6.0) as f32);
            self.draw_vector.fill();
            self.draw_vector.move_to((xp - 5.0) as f32, (rect.pos.y + AXIS_H - 10.0) as f32);
            self.draw_vector.line_to((xp + 5.0) as f32, (rect.pos.y + AXIS_H - 10.0) as f32);
            self.draw_vector.line_to(xp as f32, (rect.pos.y + AXIS_H - 4.0) as f32);
            self.draw_vector.close();
            self.draw_vector.fill();
        }
        // The hovered bar's tooltip, last of the shapes: no text but its own on it.
        let mut tip: Option<(Rect, Vec<String>)> = None;
        if let Some(s) = self.hover.and_then(|h| self.tl.spans.get(h)).cloned() {
            let lines = self.tip(&s);
            let w = lines.iter().map(|l| self.text_w(cx, l, 8.0)).fold(0.0, f64::max) + 16.0;
            let h = lines.len() as f64 * 15.0 + 10.0;
            let mut p = self.pointer + dvec2(12.0, 14.0);
            p.x = p.x.min(rect.pos.x + rect.size.x - w - 4.0).max(rect.pos.x + 4.0);
            if p.y + h > rect.pos.y + rect.size.y - 4.0 {
                p.y = self.pointer.y - h - 8.0;
            }
            let r = Rect { pos: p, size: dvec2(w, h) };
            self.draw_vector.set_color_hex(crate::theme::hex("raised"), 1.0);
            self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, r.size.x as f32, r.size.y as f32, 4.0);
            self.draw_vector.fill();
            self.draw_vector.set_color_hex(crate::theme::hex("line"), 1.0);
            self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, r.size.x as f32, r.size.y as f32, 4.0);
            self.draw_vector.stroke(1.0);
            tip = Some((r, lines));
        }
        self.draw_vector.end(cx);
        // The texts in a draw call of their own, after the shapes: else Makepad
        // may append them to an earlier text call, under the shapes (a redraw
        // while playing did: the names and the axis went under the gutter).
        self.draw_text.new_draw_call(cx);
        // And in front of every shape in depth, not only in order: each shape
        // lifts the next one's depth a little (1e-6) and a later call is only
        // 0.001 in front, so past a thousand shapes (a whole run, its hatching)
        // the gutter and the axis band, drawn last, hid the names and the axis.
        self.draw_text.draw_depth = self.draw_vector.draw_depth + self.draw_vector.cur_zbias;

        // The texts: lane names, then the bars', axis', marks' — none under the tooltip.
        for i in 0..lanes {
            let lane = self.tl.lanes[i].clone();
            let y = self.lane_y(i);
            if y + 3.0 < top || y > rect.pos.y + rect.size.y - 12.0 {
                continue;
            }
            let ink = if lane.closed { rgb(crate::theme::hex("faint"), 1.0) } else { rgb(crate::theme::hex("ink"), 1.0) };
            let name: String = lane.name.chars().take(18).collect();
            if lane_h >= 30.0 {
                self.text(cx, dvec2(rect.pos.x + 18.0, y + 4.0), &name, 8.5, ink);
                let sub = match &lane.history {
                    Some(why) => format!("{} · {why}", i18n::t("history", "历史")),
                    None => lane.sub.clone(),
                };
                let sub: String = sub.chars().take(24).collect();
                self.text(cx, dvec2(rect.pos.x + 18.0, y + 18.0), &sub, 7.0, rgb(crate::theme::hex("muted"), 1.0));
            } else {
                self.text(cx, dvec2(rect.pos.x + 18.0, y + lane_h * 0.5 - 6.0), &name, 8.0, ink);
            }
        }
        let under_tip = |p: DVec2| tip.as_ref().is_some_and(|(r, _)| r.contains(p) || r.contains(p + dvec2(20.0, 8.0)));
        for (p, text, size, color) in texts {
            if p.x + 2.0 < x0 && p.y > rect.pos.y + AXIS_H {
                continue;
            }
            if !under_tip(p) {
                self.text(cx, p, &text, size, color);
            }
        }
        if xp >= x0 - 1.0 && xp <= x1 + 1.0 {
            let label = clock(now);
            let w = self.text_w(cx, &label, 7.5);
            let x = if xp + 6.0 + w > x1 { xp - 6.0 - w } else { xp + 6.0 };
            self.text(cx, dvec2(x, rect.pos.y + AXIS_H - 16.0), &label, 7.5, rgb(crate::theme::hex("danger"), 1.0));
        }
        if let Some((r, lines)) = tip {
            for (k, line) in lines.iter().enumerate() {
                let color = if k == 0 { rgb(crate::theme::hex("ink"), 1.0) } else { rgb(crate::theme::hex("muted_strong"), 1.0) };
                self.text(cx, r.pos + dvec2(8.0, 5.0 + k as f64 * 15.0), line, 8.0, color);
            }
        }
        if self.playing {
            self.next_frame = cx.new_next_frame();
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

impl TimelineCanvasRef {
    /// A new timeline; the playhead stays where the person put it, or at the end.
    pub fn set_timeline(&self, cx: &mut Cx, tl: Timeline) {
        let Some(mut c) = self.borrow_mut() else { return };
        let real = c.axis.real(c.t);
        c.axis = Axis::new(&tl, !c.unsqueezed);
        if c.selected.is_some_and(|i| i >= tl.spans.len()) {
            c.selected = None;
        }
        c.tl = tl;
        let y = c.scroll_y;
        c.scroll_to(y);
        let len = c.axis.len();
        c.t = if c.placed || c.playing { c.axis.vis(real).min(len) } else { len };
        if let Some((a, b)) = c.window {
            if b > len {
                c.window = (len - (b - a) > 0.0).then_some((len - (b - a), len));
            }
        }
        c.redraw(cx);
    }

    /// Plays from the playhead (from the start, when it is at the end), or pauses.
    pub fn toggle_play(&self, cx: &mut Cx) -> bool {
        let Some(mut c) = self.borrow_mut() else { return false };
        c.playing = !c.playing;
        if c.playing {
            if c.t >= c.axis.len() - 0.5 {
                c.t = 0.0;
                if let Some((a, b)) = c.window {
                    c.window = Some((0.0, b - a));
                }
            }
            c.placed = true;
            c.last_frame = None;
            c.next_frame = cx.new_next_frame();
        }
        c.redraw(cx);
        c.playing
    }

    pub fn playing(&self) -> bool {
        self.borrow().is_some_and(|c| c.playing)
    }

    pub fn set_speed(&self, speed: f64) {
        if let Some(mut c) = self.borrow_mut() {
            c.speed = speed;
        }
    }

    pub fn set_squeeze(&self, cx: &mut Cx, on: bool) {
        let Some(mut c) = self.borrow_mut() else { return };
        let real = c.axis.real(c.t);
        c.unsqueezed = !on;
        c.axis = Axis::new(&c.tl, on);
        c.t = c.axis.vis(real);
        c.window = None;
        c.redraw(cx);
    }

    /// The playhead to the start, or to the end (live).
    pub fn jump(&self, cx: &mut Cx, to_end: bool) {
        let Some(mut c) = self.borrow_mut() else { return };
        c.playing = false;
        let len = c.axis.len();
        c.t = if to_end { len } else { 0.0 };
        c.placed = !to_end;
        c.window = None;
        let real = c.axis.real(c.t);
        let uid = c.widget_uid();
        cx.widget_action(uid, TimeAction::Moved(real));
        c.redraw(cx);
    }

    /// The playhead, in real seconds from the start.
    pub fn now(&self) -> f64 {
        self.borrow().map(|c| c.axis.real(c.t)).unwrap_or(0.0)
    }

    pub fn selected(&self) -> Option<usize> {
        self.borrow().and_then(|c| c.selected)
    }

    pub fn actions(&self, actions: &Actions) -> Vec<TimeAction> {
        let uid = self.widget_uid();
        actions.iter().filter_map(|a| a.as_widget_action()).filter(|a| a.widget_uid == uid)
            .map(|a| a.cast::<TimeAction>()).filter(|a| !matches!(a, TimeAction::None)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Exchange, Message, Store};

    fn msg(role: &str, text: &str, at: u64, took: Option<u64>) -> Message {
        Message { role: role.into(), author: "claude".into(), text: text.into(), at, steps: None, meta: None, took, started: took.map(|t| at - t / 2), cost: None, model: None }
    }

    #[test]
    fn a_session_becomes_lanes_turns_and_marks() {
        let mut store = Store::default();
        let dir = std::env::temp_dir();
        let pi = store.add_project(&dir.to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        let s = store.session_mut(at).unwrap();
        s.messages.push(msg("user", "make it", 1000, None));
        s.messages.push(msg("lead", "plan", 1010, Some(20)));
        let mut p = crate::chat::tests::peer(Vec::new());
        p.slug = "calc".into();
        p.log_mut().push(Exchange { from: "lead".into(), input: "do calc".into(), reply: Some("done\nCommit: feat: calc".into()), outcome: Some("completed".into()), at: 1030, steps: None, took: Some(40), cost_total: Some(0.02), began: None });
        p.log_mut().push(Exchange { from: "person".into(), input: "and this".into(), reply: Some("ok".into()), outcome: Some("completed".into()), at: 1200, steps: None, took: Some(10), cost_total: Some(0.03), began: None });
        s.peers_mut().push(p);
        s.messages.push(Message { author: "calc".into(), ..msg("peer", "report", 1071, None) });
        let tl = build(store.session(at).unwrap(), 2000, false, &|p| p.map(|p| p.slug.clone()).unwrap_or_else(|| "claude".into()));
        assert_eq!(tl.lanes.len(), 2);
        assert_eq!(tl.spans.len(), 3, "an outer turn and two inner turns");
        let outer = &tl.spans[0];
        assert_eq!((outer.lane, outer.from.as_str()), (0, "you"));
        let inner = &tl.spans[1];
        assert_eq!((inner.start, inner.end, inner.from.as_str()), (30.0, 70.0, "outer"));
        assert!((tl.spans[2].cost.unwrap() - 0.01).abs() < 1e-9, "a turn's cost is the difference from the one before");
        let kinds: Vec<MarkKind> = tl.marks.iter().map(|m| m.kind).collect();
        assert!(kinds.contains(&MarkKind::Dispatch) && kinds.contains(&MarkKind::Report) && kinds.contains(&MarkKind::Commit));
        assert_eq!(tl.marks.iter().filter(|m| m.kind == MarkKind::Person).count(), 2, "to the outer loop and to the peer");
        // 70 → 200 is idle: squeezed.
        let axis = Axis::new(&tl, true);
        assert_eq!(axis.gaps.len(), 1);
        assert!(axis.len() < tl.end - 100.0);
        assert!((axis.real(axis.vis(150.0)) - 150.0).abs() < 1e-6, "the mapping goes both ways");
        assert!(tl.state_at(35.0)[1].contains("1"), "{:?}", tl.state_at(35.0));
        assert!(!tl.events_until(100.0, 10).is_empty());
    }

    #[test]
    fn rework_is_found_with_why_and_old_attempts_are_history() {
        let mut store = Store::default();
        let dir = std::env::temp_dir();
        let pi = store.add_project(&dir.to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        let s = store.session_mut(at).unwrap();
        s.messages.push(msg("user", "make it", 1000, None));
        let read = |d: &str| crate::model::Step { id: "x".into(), name: "Bash".into(), detail: d.into(), status: "ok".into() };
        // The first attempt: failed, closed, replaced by app-core-2.
        let mut old = crate::chat::tests::peer(Vec::new());
        old.slug = "app-core".into();
        old.status = "closed".into();
        old.started_at = 1001;
        old.log_mut().push(Exchange { from: "lead".into(), input: "spec: task core".into(), reply: Some("400: model not supported".into()), outcome: Some("failed".into()), at: 1010, steps: Some(vec![read("cat docs/SCRIPT-API.md")]), took: Some(4), cost_total: None, began: None });
        // The second: a turn with nothing done, sent on, then done.
        let mut new = crate::chat::tests::peer(Vec::new());
        new.id = "w2".into();
        new.slug = "app-core-2".into();
        new.started_at = 1100;
        new.log_mut().push(Exchange { from: "lead".into(), input: "spec: task core".into(), reply: Some("Now let me rewrite the file:".into()), outcome: Some("completed".into()), at: 1100, steps: Some(vec![read("cat .octobuddy/docs/SCRIPT-API.md")]), took: Some(300), cost_total: None, began: None });
        new.log_mut().push(Exchange { from: "lead".into(), input: "Your last turn ended before any write: write the file now".into(), reply: Some("done\nCommit: feat: core".into()), outcome: Some("completed".into()), at: 1500, steps: None, took: Some(200), cost_total: None, began: None });
        s.peers_mut().push(old);
        s.peers_mut().push(new);
        let tl = build(store.session(at).unwrap(), 2000, false, &|p| p.map(|p| p.slug.clone()).unwrap_or_else(|| "claude".into()));
        let kinds: Vec<&str> = tl.rework.iter().map(|r| r.kind).collect();
        assert!(kinds.contains(&"replaced") && kinds.contains(&"no result") && kinds.contains(&"relearned"), "{kinds:?}");
        let sent = tl.rework.iter().find(|r| r.kind == "no result").unwrap();
        assert!(sent.why.contains("ended before any write"), "why is the outer loop's own words: {}", sent.why);
        assert_eq!(sent.lost, 300.0, "the turn it replaced");
        assert!(tl.lanes[1].history.as_deref().is_some_and(|h| h.contains("app-core-2")), "{:?}", tl.lanes[1].history);
        assert!(tl.spans.iter().filter(|s| s.lane == 1).all(|s| s.redone), "a replaced lane's turns are history");
        assert!(tl.marks.iter().any(|m| m.kind == MarkKind::Rework));
        assert_eq!(tl.rework_lines().len(), tl.rework.len());
    }

    #[test]
    fn lines_join_the_turns_at_both_their_ends() {
        let span = |lane: usize, start: f64, end: f64| Span { lane, start, end, open: false, turn: 1, from: String::new(), outcome: String::new(), steps: 0, failed: 0, cost: None, input: String::new(), reply: String::new(), message: None, redone: false, asked: None };
        let mut tl = Timeline {
            lanes: Vec::new(),
            // The outer loop: 0–60 (it plans), 300–330 (it reviews).
            spans: vec![span(0, 0.0, 60.0), span(0, 300.0, 330.0), span(1, 90.0, 200.0)],
            marks: vec![
                // Sent at 50 (in its planning turn), begun at 90 (a slot was free then).
                Mark { lane: 0, to: Some(1), at: 50.0, kind: MarkKind::Dispatch, text: String::new(), until: Some(90.0) },
                // Said at 230 (after its check), read by the turn at 300.
                Mark { lane: 1, to: Some(0), at: 230.0, kind: MarkKind::Report, text: String::new(), until: None },
                // Said at 340: no outer turn has read it yet.
                Mark { lane: 1, to: Some(0), at: 340.0, kind: MarkKind::Report, text: String::new(), until: None },
            ],
            ..Default::default()
        };
        join_marks(&mut tl);
        assert_eq!((tl.marks[0].at, tl.marks[0].until), (50.0, Some(90.0)), "from the turn that sent it to the turn that took it up");
        assert_eq!((tl.marks[1].at, tl.marks[1].until), (200.0, Some(300.0)), "from the end of the turn that wrote it to the turn that read it");
        assert_eq!(tl.marks[2].until, None, "not read yet: it waits");
    }

    #[test]
    fn every_wait_says_why() {
        let mut store = Store::default();
        let dir = std::env::temp_dir();
        let pi = store.add_project(&dir.to_string_lossy()).unwrap();
        let at = store.add_session(pi).unwrap();
        let s = store.session_mut(at).unwrap();
        // You ask at 1000; the outer loop's turn begins at 1008 (its agent starting) and plans till 1030.
        s.messages.push(msg("user", "make it", 1000, None));
        s.messages.push(Message { started: Some(1008), ..msg("lead", "plan", 1030, Some(22)) });
        s.messages.push(msg("system", "Round 1: 2 slice(s) (core, stats), here, on octos.", 1040, None));
        // Wave 1 (core) runs 1040–1100; wave 2 (stats), made with it, is held: it
        // is sent at 1145, after the accept, and begins at 1150 (its start then).
        let mut core = crate::chat::tests::peer(Vec::new());
        core.slug = "core".into();
        core.wave = Some(1);
        core.log_mut().push(Exchange { from: "lead".into(), input: "spec: task core".into(), reply: Some("done".into()), outcome: Some("completed".into()), at: 1040, steps: None, took: Some(60), cost_total: None, began: Some(1040) });
        let mut stats = crate::chat::tests::peer(Vec::new());
        stats.id = "w2".into();
        stats.slug = "stats".into();
        stats.wave = Some(2);
        stats.started_at = 1150;
        stats.log_mut().push(Exchange { from: "lead".into(), input: "spec: task stats".into(), reply: Some("done".into()), outcome: Some("completed".into()), at: 1145, steps: None, took: Some(30), cost_total: None, began: Some(1150) });
        s.peers_mut().push(core);
        s.peers_mut().push(stats);
        // core's report is said at 1120 (OctoBuddy checked it from 1100); the outer reads it 1130–1140.
        s.messages.push(Message { author: "core".into(), ..msg("peer", "report", 1120, None) });
        s.messages.push(Message { started: Some(1130), ..msg("lead", "accept", 1140, Some(10)) });
        // Then nothing until you speak again at 1400.
        s.messages.push(msg("user", "more", 1400, None));
        let tl = build(store.session(at).unwrap(), 1500, false, &|p| p.map(|p| p.slug.clone()).unwrap_or_default());
        let kinds: Vec<(&str, usize)> = tl.waits.iter().map(|w| (w.kind, w.lane)).collect();
        assert!(kinds.contains(&("start", 0)), "the outer loop starting: {kinds:?}");
        assert!(kinds.contains(&("wave", 2)), "stats waits for wave 1: {kinds:?}");
        assert!(kinds.contains(&("check", 1)), "OctoBuddy checks core: {kinds:?}");
        assert!(kinds.contains(&("person", 0)), "then it waits for you: {kinds:?}");
        let wave = tl.waits.iter().find(|w| w.kind == "wave").unwrap();
        assert_eq!((wave.from, wave.to), (40.0, 150.0));
    }

    #[test]
    fn lengths_read_short() {
        assert_eq!(span_len(45.0), "45s");
        assert_eq!(span_len(185.0), "3m 05s");
        assert_eq!(clock(65.0), "+01:05");
        assert_eq!(has_commit("x\n**Commit:** `fix: y`"), Some("fix: y".into()));
    }
}
