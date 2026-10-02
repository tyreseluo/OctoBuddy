//! The lead's plan: the slices it wants inner-loop peers to run, written as
//! one fenced block at the end of its reply:
//!
//! ````text
//! ```octobuddy-plan
//! {"slices":[{"slug":"cli-flag","brief":"Add --version to …"}]}
//! ```
//! ````
use makepad_widgets::makepad_micro_serde::*;

/// At most this many peers per round, whatever the plan asks for.
pub const MAX_SLICES: usize = 8;

const FENCE: &str = "```octobuddy-plan";
const SEND_FENCE: &str = "```octobuddy-send";
const REPORT_FENCE: &str = "```octobuddy-report";
const REVIEW_FENCE: &str = "```octobuddy-review";

/// The lead's call on a peer's work after reviewing it: `accept` or `fix`,
/// shown in the peer's panel.
#[derive(Clone, Debug, PartialEq)]
pub struct Review {
    pub slug: String,
    pub verdict: String,
    pub why: String,
}

#[derive(DeJson)]
struct ReviewItem {
    slug: String,
    verdict: String,
    why: Option<String>,
}

#[derive(DeJson)]
struct ReviewDoc {
    branches: Vec<ReviewItem>,
}

/// A peer's report to the lead, cut out of its reply: the rest, and the report.
pub fn take_report(reply: &str) -> (String, Option<String>) {
    let (text, body) = take_block(reply, REPORT_FENCE);
    (text, body.map(|b| b.trim().to_string()).filter(|b| !b.is_empty()))
}

/// The lead changing a message it queued for a peer.
#[derive(Clone, Debug, PartialEq)]
pub struct QueueOp {
    /// `cancel`, `replace` or `merge`.
    pub op: String,
    pub to: String,
    pub ids: Vec<String>,
    pub message: String,
}

#[derive(DeJson)]
struct QueueOpItem {
    op: String,
    to: String,
    id: Option<String>,
    ids: Option<Vec<String>>,
    message: Option<String>,
}

/// The lead runs a slice again from its brief, on another agent or model
/// (its agent failed): none of them named, the one it had.
#[derive(Clone, Debug, PartialEq)]
pub struct Rerun {
    pub to: String,
    pub agent: Option<String>,
    pub model: Option<String>,
}

#[derive(DeJson)]
struct RerunItem {
    to: String,
    agent: Option<String>,
    model: Option<String>,
}

/// A message the lead sends to a peer it started: `mode` is `queue` or `interrupt`.
#[derive(Clone, Debug, PartialEq)]
pub struct Send {
    pub to: String,
    pub mode: String,
    pub message: String,
}

#[derive(DeJson)]
struct SendItem {
    to: String,
    mode: Option<String>,
    message: String,
}

#[derive(DeJson)]
struct SendDoc {
    messages: Option<Vec<SendItem>>,
    queue_ops: Option<Vec<QueueOpItem>>,
    /// Peers the lead is done with: closed, they take no more work.
    close: Option<Vec<String>>,
    rerun: Option<Vec<RerunItem>>,
}

#[derive(Clone, Debug, PartialEq, DeJson)]
pub struct Slice {
    pub slug: String,
    /// developer, reviewer, tester, writer… (free text, one word).
    pub role: Option<String>,
    pub brief: String,
    /// A shell command that runs the slice's tests; OctoBuddy runs it in the
    /// peer's clone when the peer is done.
    pub check: Option<String>,
    /// Its agent-estimation: effective rounds (risk-adjusted).
    pub rounds: Option<f64>,
    /// Its wave: slices of one wave do not depend on each other.
    pub wave: Option<u32>,
    /// Review cycles the lead expects before accepting it (1: accepted as sent).
    pub reviews: Option<u32>,
    /// The model it runs on (`family/model`, one of STATUS's MODELS): none,
    /// the default one.
    pub model: Option<String>,
    /// The agent it runs on (`octos`, `codex`, `pi`, `claude`: one of
    /// STATUS's AGENTS): none, the session's.
    pub agent: Option<String>,
    /// A fresh read-only reviewer looks at its work before the lead does.
    pub independent_review: Option<bool>,
}

/// The whole plan's estimate, from agent-estimation's wave mode.
#[derive(Clone, Debug, Default, PartialEq, DeJson)]
pub struct Estimate {
    /// Risk-adjusted rounds of the plan, waves and integration included.
    pub rounds: Option<f64>,
    pub waves: Option<u32>,
    /// Its wallclock, in minutes, at the method's minutes per round.
    pub minutes: Option<f64>,
}

#[derive(DeJson)]
struct PlanDoc {
    slices: Vec<Slice>,
    estimate: Option<Estimate>,
    /// What every slice needs alike (data, keys, names), said once.
    shared: Option<String>,
}

/// What the lead said, split into the text to show, the plan it asked for
/// and the messages it sends to peers.
#[derive(Debug, PartialEq)]
pub struct Reply {
    pub text: String,
    pub slices: Vec<Slice>,
    /// Set when there was a plan block that could not be read.
    pub plan_error: Option<String>,
    pub sends: Vec<Send>,
    pub queue_ops: Vec<QueueOp>,
    pub send_error: Option<String>,
    pub reviews: Vec<Review>,
    pub estimate: Option<Estimate>,
    /// Peers to close, by slug.
    pub close: Vec<String>,
    /// Slices to run again on another agent or model.
    pub reruns: Vec<Rerun>,
    /// The plan's part every slice shares, given to each with its brief.
    pub shared: Option<String>,
}

pub fn split_reply(reply: &str) -> Reply {
    let (text, plan) = take_block(reply, FENCE);
    let (text, send) = take_block(&text, SEND_FENCE);
    let (text, review) = take_block(&text, REVIEW_FENCE);
    let mut out = Reply { text, slices: Vec::new(), plan_error: None, sends: Vec::new(), queue_ops: Vec::new(), send_error: None, reviews: Vec::new(), estimate: None, close: Vec::new(), reruns: Vec::new(), shared: None };
    // A review that cannot be read only loses the recommendation: the text says it too.
    if let Some(Ok(doc)) = review.map(|b| ReviewDoc::deserialize_json_lenient(b.trim())) {
        out.reviews = doc.branches.into_iter()
            .map(|r| {
                // "merge" is how it said accept before one shared directory.
                let verdict = match r.verdict.trim().to_ascii_lowercase().as_str() { "merge" => "accept".to_string(), v => v.to_string() };
                Review { slug: slugify(&r.slug), verdict, why: r.why.unwrap_or_default().trim().to_string() }
            })
            .filter(|r| !r.slug.is_empty() && matches!(r.verdict.as_str(), "accept" | "fix"))
            .collect();
    }
    if let Some(body) = plan {
        match PlanDoc::deserialize_json_lenient(body.trim()) {
            Ok(doc) => {
                out.estimate = doc.estimate;
                out.shared = doc.shared.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                let mut seen = Vec::new();
                out.slices = doc.slices.into_iter()
                    .filter(|s| !s.brief.trim().is_empty())
                    .take(MAX_SLICES)
                    .map(|s| {
                        let mut slug = slugify(&s.slug);
                        if slug.is_empty() { slug = "slice".to_string(); }
                        let base = slug.clone();
                        let mut n = 2;
                        while seen.contains(&slug) { slug = format!("{base}-{n}"); n += 1; }
                        seen.push(slug.clone());
                        let role = s.role.map(|r| slugify(&r)).filter(|r| !r.is_empty());
                        let check = s.check.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
                        Slice { slug, role, brief: s.brief.trim().to_string(), check, rounds: s.rounds, wave: s.wave, reviews: s.reviews, model: s.model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()),
                            agent: s.agent.map(|a| a.trim().to_ascii_lowercase()).filter(|a| !a.is_empty()), independent_review: s.independent_review }
                    })
                    .collect();
            }
            Err(err) => out.plan_error = Some(format!("{err:?}")),
        }
    }
    if let Some(body) = send {
        match SendDoc::deserialize_json_lenient(body.trim()) {
            Ok(doc) => {
                out.sends = doc.messages.unwrap_or_default().into_iter()
                    .filter(|m| !m.message.trim().is_empty() && !m.to.trim().is_empty())
                    .map(|m| Send { to: slugify(&m.to), mode: m.mode.unwrap_or_default(), message: m.message.trim().to_string() })
                    .collect();
                out.close = doc.close.unwrap_or_default().iter().map(|s| slugify(s)).filter(|s| !s.is_empty()).collect();
                let given = |v: Option<String>| v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
                out.reruns = doc.rerun.unwrap_or_default().into_iter()
                    .map(|r| Rerun { to: slugify(&r.to), agent: given(r.agent).map(|a| a.to_ascii_lowercase()), model: given(r.model) })
                    .filter(|r| !r.to.is_empty())
                    .collect();
                out.queue_ops = doc.queue_ops.unwrap_or_default().into_iter()
                    .map(|o| QueueOp {
                        op: o.op.to_ascii_lowercase(), to: slugify(&o.to),
                        ids: o.ids.unwrap_or_default().into_iter().chain(o.id).collect(),
                        message: o.message.unwrap_or_default(),
                    })
                    .collect();
            }
            Err(err) => out.send_error = Some(format!("{err:?}")),
        }
    }
    out
}

/// Cuts the last block opened by `fence` out of `reply`: the rest, and the block's body.
fn take_block(reply: &str, fence: &str) -> (String, Option<String>) {
    let Some(start) = reply.rfind(fence) else {
        return (reply.trim().to_string(), None);
    };
    let rest = &reply[start + fence.len()..];
    // The closing fence is a line of its own. A brief may quote code with
    // ``` inside a JSON string, but a JSON string never holds a raw newline,
    // so a fence at the start of a line is never inside one.
    let (body, after) = match closing_fence(rest) {
        Some((end, len)) => (&rest[..end], &rest[end + len..]),
        None => (rest, ""),
    };
    let (before, after) = (reply[..start].trim(), after.trim());
    let text = match (before.is_empty(), after.is_empty()) {
        (false, false) => format!("{before}\n\n{after}"),
        _ => format!("{before}{after}"),
    };
    (text, Some(body.to_string()))
}

/// Where the fence closing `rest` starts, and its length: the first line
/// that is ``` alone (surrounding spaces allowed).
fn closing_fence(rest: &str) -> Option<(usize, usize)> {
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim() == "```" && at > 0 {
            return Some((at, line.len()));
        }
        at += line.len();
    }
    None
}

/// Lower-case letters, digits and single dashes, at most 32 characters: safe
/// in a branch name and a directory name.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 32 { break; }
    }
    out.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_plan_and_keeps_the_text() {
        let reply = "Two slices.\n\n```octobuddy-plan\n{\"slices\":[{\"slug\":\"CLI Flag!\",\"role\":\"Developer\",\"brief\":\"add it\",\"files\":[\"a\"]},{\"slug\":\"cli flag\",\"brief\":\"test it\"},{\"slug\":\"x\",\"brief\":\" \"}]}\n```\n";
        let r = split_reply(reply);
        assert_eq!(r.text, "Two slices.");
        assert_eq!(r.plan_error, None);
        let slugs: Vec<_> = r.slices.iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(slugs, ["cli-flag", "cli-flag-2"], "unknown fields are ignored, empty briefs dropped, slugs unique");
        assert_eq!((r.slices[0].role.as_deref(), r.slices[1].role.as_deref()), (Some("developer"), None));
    }

    #[test]
    fn the_lead_may_run_a_slice_again_elsewhere() {
        let r = split_reply("```octobuddy-send\n{\"rerun\":[{\"to\":\"App Core\",\"agent\":\"Codex\",\"model\":\" minimax/MiniMax-M3 \"},{\"to\":\"x\"}]}\n```");
        assert_eq!(r.send_error, None);
        assert_eq!(r.reruns, [
            Rerun { to: "app-core".into(), agent: Some("codex".into()), model: Some("minimax/MiniMax-M3".into()) },
            Rerun { to: "x".into(), agent: None, model: None },
        ]);
    }

    #[test]
    fn a_slice_may_name_its_agent() {
        let r = split_reply("```octobuddy-plan\n{\"slices\":[{\"slug\":\"core\",\"brief\":\"build it\",\"agent\":\" Codex \",\"model\":\"minimax/MiniMax-M3\"},{\"slug\":\"copy\",\"brief\":\"write it\",\"agent\":\"\"}]}\n```");
        assert_eq!(r.slices[0].agent.as_deref(), Some("codex"), "trimmed, lower case");
        assert_eq!(r.slices[1].agent, None, "an empty agent is the session's");
    }

    #[test]
    fn a_slice_may_name_its_model() {
        let r = split_reply("```octobuddy-plan\n{\"slices\":[{\"slug\":\"copy\",\"brief\":\"write it\",\"model\":\" deepseek/deepseek-chat \"},{\"slug\":\"core\",\"brief\":\"build it\",\"model\":\"\"}]}\n```");
        assert_eq!(r.slices[0].model.as_deref(), Some("deepseek/deepseek-chat"));
        assert_eq!(r.slices[1].model, None, "an empty model is the default");
    }

    #[test]
    fn a_brief_may_quote_code() {
        let reply = "Two slices.\n```octobuddy-plan\n{\"slices\":[{\"slug\":\"divide\",\"brief\":\"calc.py has:\\n```python\\ndef add(a, b): ...\\n```\\nAdd divide.\"}]}\n```\nDone.";
        let r = split_reply(reply);
        assert_eq!(r.plan_error, None, "{:?}", r.plan_error);
        assert_eq!(r.slices.len(), 1);
        assert!(r.slices[0].brief.contains("```python"));
        assert_eq!(r.text, "Two slices.\n\nDone.");
    }

    #[test]
    fn sends_come_with_or_without_a_plan() {
        let reply = "Changing course.\n```octobuddy-send\n{\"messages\":[{\"to\":\"Feat Power\",\"mode\":\"interrupt\",\"message\":\"Stop: use math.pow.\"},{\"to\":\"docs\",\"message\":\"Also note it.\"},{\"to\":\"x\",\"message\":\" \"}]}\n```\n```octobuddy-plan\n{\"slices\":[{\"slug\":\"tests\",\"role\":\"tester\",\"brief\":\"b\"}]}\n```";
        let r = split_reply(reply);
        assert_eq!(r.text, "Changing course.");
        assert_eq!(r.slices.len(), 1);
        assert_eq!(r.sends, vec![
            Send { to: "feat-power".into(), mode: "interrupt".into(), message: "Stop: use math.pow.".into() },
            Send { to: "docs".into(), mode: String::new(), message: "Also note it.".into() },
        ]);
    }

    #[test]
    fn reports_and_queue_ops() {
        let (text, report) = take_report("Done.\n```octobuddy-report\nAdded clamp; 6 tests pass.\n```");
        assert_eq!((text.as_str(), report.as_deref()), ("Done.", Some("Added clamp; 6 tests pass.")));
        assert_eq!(take_report("no report").1, None);
        let r = split_reply("```octobuddy-send\n{\"queue_ops\":[{\"op\":\"Cancel\",\"to\":\"docs\",\"id\":\"q3\"},{\"op\":\"merge\",\"to\":\"docs\",\"ids\":[\"q4\",\"q5\"],\"message\":\"both\"}]}\n```");
        assert_eq!(r.send_error, None);
        assert_eq!(r.queue_ops[0], QueueOp { op: "cancel".into(), to: "docs".into(), ids: vec!["q3".into()], message: String::new() });
        assert_eq!(r.queue_ops[1].ids, vec!["q4".to_string(), "q5".to_string()]);
    }

    #[test]
    fn no_plan_and_bad_plans() {
        assert_eq!(split_reply("All verified.").slices, vec![]);
        let bad = split_reply("Plan:\n```octobuddy-plan\n{not json}\n```");
        assert!(bad.plan_error.is_some());
        assert_eq!(bad.text, "Plan:");
        let many = format!("```octobuddy-plan\n{{\"slices\":[{}]}}\n```",
            (0..9).map(|i| format!("{{\"slug\":\"s{i}\",\"brief\":\"b\"}}")).collect::<Vec<_>>().join(","));
        assert_eq!(split_reply(&many).slices.len(), MAX_SLICES);
    }

    #[test]
    fn checks_and_reviews() {
        let r = split_reply("```octobuddy-plan\n{\"slices\":[{\"slug\":\"a\",\"brief\":\"b\",\"check\":\" python3 -m pytest -q \"},{\"slug\":\"c\",\"brief\":\"d\",\"check\":\"\"}]}\n```");
        assert_eq!(r.slices[0].check.as_deref(), Some("python3 -m pytest -q"));
        assert_eq!(r.slices[1].check, None);
        let r = split_reply("Both verified.\n```octobuddy-review\n{\"branches\":[{\"slug\":\"Feat A\",\"verdict\":\"Merge\",\"why\":\"tests pass\"},{\"slug\":\"b\",\"verdict\":\"maybe\"}]}\n```");
        assert_eq!(r.text, "Both verified.");
        assert_eq!(r.reviews, vec![Review { slug: "feat-a".into(), verdict: "accept".into(), why: "tests pass".into() }]);
    }

    #[test]
    fn estimates_and_closing() {
        let r = split_reply("```octobuddy-plan\n{\"estimate\":{\"rounds\":14.5,\"waves\":2,\"minutes\":44},\"slices\":[{\"slug\":\"a\",\"brief\":\"b\",\"rounds\":4,\"wave\":1,\"reviews\":2}]}\n```");
        assert_eq!(r.estimate, Some(Estimate { rounds: Some(14.5), waves: Some(2), minutes: Some(44.0) }));
        assert_eq!((r.slices[0].rounds, r.slices[0].wave, r.slices[0].reviews), (Some(4.0), Some(1), Some(2)));
        let c = split_reply("```octobuddy-send\n{\"close\":[\"Feat A\",\" \"]}\n```");
        assert_eq!(c.close, vec!["feat-a".to_string()]);
    }

    #[test]
    fn slugs_are_safe() {
        assert_eq!(slugify("  Fix: the Login bug / 登录 "), "fix-the-login-bug");
        assert_eq!(slugify("---"), "");
    }
}
