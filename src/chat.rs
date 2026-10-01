//! One conversation view for both loops. A session's messages (the person
//! and the outer loop) and an inner loop's exchanges (the outer loop's
//! tasks, the person's messages, its replies) become the same rows, drawn
//! by the same templates: the person on the right, the agents on the left
//! with their steps, messages between the loops as folded cards.
use crate::model::{Peer, Role, Session};
use crate::{i18n, plan, stream};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// The person, on the right.
    User,
    /// The outer loop (Claude): its steps, when it used tools, then its text.
    Outer,
    /// An inner loop (octos).
    Inner,
    /// A message from one loop to the other, folded until opened.
    Card,
    /// OctoBuddy's own line.
    System,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub kind: Kind,
    /// Who speaks (`Outer · claude`), or a card's header.
    pub title: String,
    pub body: String,
    pub steps: String,
    /// How long the turn behind it took, in seconds (an agent's reply).
    pub took: Option<u64>,
    /// Being written now: drawn at an even pace (`reveal.rs`).
    pub live: bool,
    /// When its agent began on it (a live row's clock counts from here).
    pub since: Option<u64>,
    /// What its agent is doing before its first word (starting, thinking,
    /// reconnecting): said beside the clock.
    pub status: Option<String>,
}

impl Row {
    fn new(kind: Kind, title: impl Into<String>, body: impl Into<String>) -> Row {
        Row { kind, title: title.into(), body: body.into(), steps: String::new(), took: None, live: false, since: None, status: None }
    }
}

/// The session's conversation. `streaming` is the outer message being
/// written. The person's direct talks with inner loops are not in it: they
/// stay in each inner loop's own conversation.
pub fn session_rows(session: &Session, streaming: Option<usize>) -> Vec<Row> {
    session_rows_at(session, streaming, false).into_iter().map(|(_, row)| row).collect()
}

/// A plain chat's: its agent's replies under the agent's name, no "Outer".
pub fn chat_rows(session: &Session, streaming: Option<usize>) -> Vec<Row> {
    session_rows_at(session, streaming, true).into_iter().map(|(_, row)| row).collect()
}

/// Who answers in a plain chat, by the engine that wrote it.
fn agent_name(engine: &str) -> &str {
    match engine {
        "" | "claude" => "Claude Code",
        "codex" => "Codex",
        "system" => "octos · OctoSense",
        other => other,
    }
}

/// A reply not begun: the outer loop (or a plain chat's agent) has the
/// person's message, its first word not out yet — its process starting, the
/// model thinking. Drawn live (the dots), with `status` and a clock from `since`.
pub fn waiting_row(session: &Session, plain: bool, status: &str, since: u64) -> Row {
    let cli = session.engine();
    let title = if plain { agent_name(cli).to_string() } else { format!("Outer · {cli}") };
    let mut row = Row::new(Kind::Outer, title, "");
    row.live = true;
    row.since = Some(since);
    row.status = Some(status.to_string());
    row
}

/// The row a message is drawn as (the first at or after it).
pub fn row_of_message(session: &Session, message: usize) -> Option<usize> {
    session_rows_at(session, None, false).iter().position(|(i, _)| *i >= message)
}

/// The same, each row with the index of its message.
fn session_rows_at(session: &Session, streaming: Option<usize>, plain: bool) -> Vec<(usize, Row)> {
    let mut rows = Vec::new();
    for (i, m) in session.messages.iter().enumerate() {
        let meta = m.meta.clone().unwrap_or_default();
        let row = match m.role() {
            // Steered into a running turn: said beside it, and whether it was taken.
            Role::User => Row::new(Kind::User, match meta.as_str() {
                crate::orchestrate::STEER => i18n::t("steered in", "插话"),
                crate::orchestrate::STEER_TAKEN => i18n::t("steered in · taken", "插话 · 已送达"),
                crate::orchestrate::STEER_QUEUED => i18n::t("steered in · sent next", "插话 · 改为下一条"),
                _ => "",
            }, m.text.clone()),
            Role::Lead => {
                let text = stream::strip_think(&m.text);
                let body = if text.is_empty() && streaming == Some(i) { "…".to_string() } else { text };
                // Claude Code's own line for a dropped API call (it retries):
                // said as such, not as what the outer loop says.
                let body = match body.trim_start().strip_prefix("API Error:") {
                    Some(why) => i18n::pick(format!("_(Its connection to the model dropped: {}. Claude Code goes on by itself.)_", why.trim()),
                        format!("_（和模型的连接断了一下：{}。Claude Code 会自己接着做。）_", why.trim())),
                    None => body,
                };
                // Who wrote it then, not what the picker says now.
                let cli = if m.author.is_empty() { "claude" } else { m.author.as_str() };
                let title = if plain { agent_name(cli).to_string() } else { format!("Outer · {cli}") };
                let mut row = Row::new(Kind::Outer, title, body);
                row.steps = stream::steps_text(m.steps.as_deref().unwrap_or(&[]));
                row.took = m.took;
                row.live = streaming == Some(i);
                row.since = Some(m.at);
                row
            }
            Role::Peer => Row::new(Kind::Card, i18n::pick(format!("Report from {} ({meta})", m.author), format!("来自 {}（{meta}）的汇报", m.author)), m.text.clone()),
            Role::ToPeer => Row::new(Kind::Card, format!("Outer → {} · {meta}", m.author), m.text.clone()),
            Role::System => Row::new(Kind::System, "", m.text.clone()),
            Role::Note => continue,
        };
        if row.body.trim().is_empty() && row.steps.is_empty() && row.kind != Kind::Outer {
            continue;
        }
        rows.push((i, row));
    }
    rows
}

/// An inner loop's conversation: what it was given, by whom, and what it
/// answered, as it streams.
pub fn peer_rows(p: &Peer) -> Vec<Row> {
    let mut rows = Vec::new();
    let who = format!("{} · {}", p.role(), p.agent());
    for (i, e) in p.log().iter().enumerate() {
        match e.from.as_str() {
            "person" => rows.push(Row::new(Kind::User, "", e.input.trim())),
            "subagents" => rows.push(Row::new(Kind::System, "", e.input.trim())),
            _ => {
                let title = if i == 0 { i18n::t("Task from outer", "来自 outer 的任务") } else { i18n::t("Message from outer", "来自 outer 的消息") }.to_string();
                rows.push(Row::new(Kind::Card, title, e.input.trim()));
            }
        }
        let reply = stream::strip_think(e.reply.as_deref().unwrap_or(""));
        let (text, report) = plan::take_report(&reply);
        let text = match e.outcome.as_deref() {
            None if text.trim().is_empty() => "…".to_string(),
            None => format!("{} …", text.trim()),
            Some("completed") => text.trim().to_string(),
            Some(outcome) => format!("[{outcome}] {}", text.trim()),
        };
        let mut row = Row::new(Kind::Inner, who.clone(), text);
        row.steps = stream::steps_text(e.steps.as_deref().unwrap_or(&[]));
        row.took = e.took;
        row.live = e.outcome.is_none();
        row.since = Some(e.began.unwrap_or(e.at));
        rows.push(row);
        if let Some(report) = report {
            rows.push(Row::new(Kind::Card, i18n::t("Report to outer", "给 outer 的汇报"), report));
        }
    }
    rows
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::model::{Exchange, Message, Step};

    pub(crate) fn peer(log: Vec<Exchange>) -> Peer {
        Peer {
            id: "w".into(), slug: "feat".into(), role: Some("developer".into()), agent: None, brief: "b".into(), status: "idle".into(),
            dir: String::new(), branch: None, round: 1, started_at: 0, finished_at: None, activity: None, result: None, session_key: None,
            log: Some(log), contract: None, usage: None, check: None, verdict: None, review: None, landed: None, model: None, effort: None,
            touched: None, base: None, commits: None, subagents: None, estimate: None, wave: None, rounds_used: None, budget: None, over_budget: None, flow: None, joined_from: None, queued: None, inflight: None, uncommitted: None, model_pick: None, accepted: None, review_wanted: None, reviews_for: None, by_person: None, claude_session: None, specs: None,
        }
    }

    fn ex(from: &str, input: &str, reply: Option<&str>, outcome: Option<&str>) -> Exchange {
        Exchange { from: from.into(), input: input.into(), reply: reply.map(Into::into), outcome: outcome.map(Into::into), at: 0, steps: None, took: None, cost_total: None, began: None }
    }

    #[test]
    fn an_inner_conversation_reads_like_the_outer_one() {
        let mut first = ex("lead", "Add multiply.", Some("Done.\n```octobuddy-report\nAdded multiply.\n```"), Some("completed"));
        first.steps = Some(vec![Step { id: "1".into(), name: "edit_file".into(), detail: "calc.py".into(), status: "ok".into() }]);
        let rows = peer_rows(&peer(vec![first, ex("person", "hello?", None, None)]));
        let kinds: Vec<Kind> = rows.iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [Kind::Card, Kind::Inner, Kind::Card, Kind::User, Kind::Inner]);
        assert_eq!(rows[0].title, "Task from outer");
        assert_eq!((rows[1].title.as_str(), rows[1].body.as_str()), ("developer · octos", "Done."));
        assert_eq!(rows[1].steps, "✓ edit_file · calc.py");
        assert_eq!((rows[2].title.as_str(), rows[2].body.as_str()), ("Report to outer", "Added multiply."));
        assert_eq!((rows[3].body.as_str(), rows[4].body.as_str()), ("hello?", "…"), "the person's message, and the reply being written");
    }

    #[test]
    fn a_person_inner_talk_is_not_in_the_outer_conversation() {
        let msg = |role: &str, text: &str| Message { role: role.into(), author: "feat".into(), text: text.into(), at: 0, steps: None, meta: None, took: None, started: None, cost: None };
        let session = Session {
            id: "s".into(), title: "t".into(), created_at: 0, cli: None, lead_session: None, peers: None, lead_cost: None,
            isolate: None, worktree: None, work_dir: None, work_branch: None, lead_model: None, estimate: None, auto: None, flow: None, waiting: None, data: None, detached: None, outer: None, inner: None, outer_effort: None, inner_effort: None, pinned: None, archived: None, carried: None,
            messages: vec![msg("user", "hi"), msg("note", "You: hello"), msg("lead", ""), msg("system", "Round 1")],
        };
        let kinds: Vec<Kind> = session_rows(&session, Some(2)).iter().map(|r| r.kind).collect();
        assert_eq!(kinds, [Kind::User, Kind::Outer, Kind::System]);
        let mut picked_pi = session.clone();
        picked_pi.cli = Some("pi".into());
        picked_pi.messages[2].author = "claude".into();
        assert_eq!(session_rows(&picked_pi, None)[1].title, "Outer · claude", "picking another CLI does not rename what claude wrote");
        assert_eq!(session_rows(&session, Some(2))[1].body, "…");
    }
}
