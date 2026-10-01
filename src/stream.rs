//! What the lead and the peers are writing, for the screen: the same shape
//! for both, whatever the agent (Claude's stream-json, octos's projection
//! frames): tool steps, then the text as it grows.
use crate::model::Step;

/// The fence OctoBuddy's blocks start with: while a reply streams, a block
/// being written is not shown (it is read when the message is complete).
const BLOCK: &str = "```octobuddy-";

/// The part of a streaming reply to show: everything before an OctoBuddy block.
/// A reply without the reasoning some models write into it (MiniMax, GLM
/// through a chat API: `<think>…</think>`); an open one hides what follows
/// (it is still thinking).
pub fn strip_think(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("<think>") {
        out.push_str(&rest[..at]);
        match rest[at..].find("</think>") {
            Some(end) => rest = &rest[at + end + "</think>".len()..],
            None => return out.trim_end().to_string(),
        }
    }
    out.push_str(rest);
    // An answer that began with its reasoning: no blank lines left before it.
    if text.trim_start().starts_with("<think>") { out.trim_start().to_string() } else { out }
}

pub fn visible(text: &str) -> String {
    match text.find(BLOCK) {
        Some(at) => {
            let before = text[..at].trim_end();
            if before.is_empty() { "(writing a plan…)".to_string() } else { format!("{before}\n\n(writing a plan…)") }
        }
        None => text.to_string(),
    }
}

/// An absolute path as its last two parts (`src/calc.py`): the directory
/// is the session's, said once elsewhere.
fn short_path(detail: &str) -> String {
    if !detail.starts_with('/') || detail.contains(' ') {
        return detail.to_string();
    }
    let parts: Vec<&str> = detail.rsplitn(3, '/').collect();
    match parts.as_slice() {
        [file, dir, _] => format!("{dir}/{file}"),
        _ => detail.to_string(),
    }
}

/// Steps as lines: "✓ Read calc.py", "… bash cargo test", "✗ Grep todo".
/// A tool's name as a person reads it: an MCP tool without its
/// `mcp__<server>__` prefix (`mcp__octobuddy__octobuddy_status` is
/// `octobuddy_status`), which is too long for one line.
pub fn tool_label(name: &str) -> &str {
    name.strip_prefix("mcp__").and_then(|rest| rest.split_once("__")).map(|(_, tool)| tool).unwrap_or(name)
}

pub fn steps_text(steps: &[Step]) -> String {
    steps.iter().map(|s| {
        let mark = match s.status.as_str() { "ok" => "✓", "failed" => "✗", _ => "…" };
        let detail = s.detail.lines().next().unwrap_or("").trim();
        let detail = short_path(detail);
        let detail: String = detail.chars().take(90).collect();
        // Claude's Agent tool and octos's spawn both start a subagent.
        let name = match s.name.as_str() { "Agent" | "Task" | "spawn" | "spawn_agent" | "delegate" => "Subagent", name => tool_label(name) };
        if detail.is_empty() { format!("{mark} {name}") } else { format!("{mark} {name} · {detail}") }
    }).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_models_reasoning_is_not_its_answer() {
        assert_eq!(strip_think("<think>The user asks…</think>\n\n我是 MiniMax-M3。"), "我是 MiniMax-M3。");
        assert_eq!(strip_think("Sure.<think>still thinking"), "Sure.");
        assert_eq!(strip_think("no reasoning here"), "no reasoning here");
    }

    #[test]
    fn an_mcp_tool_is_named_without_its_server() {
        assert_eq!(tool_label("mcp__octobuddy__octobuddy_status"), "octobuddy_status");
        assert_eq!(tool_label("mcp__mempal__mempal_search"), "mempal_search");
        assert_eq!(tool_label("Read"), "Read");
    }
    use crate::model::{end_step, upsert_step};

    #[test]
    fn a_block_being_written_is_hidden() {
        assert_eq!(visible("Two slices.\n```octobuddy-plan\n{\"sli"), "Two slices.\n\n(writing a plan…)");
        assert_eq!(visible("```octobuddy-send\n{"), "(writing a plan…)");
        assert_eq!(visible("plain"), "plain");
    }

    #[test]
    fn steps_read_as_lines() {
        let mut steps = Vec::new();
        upsert_step(&mut steps, "t1", "Read", "");
        upsert_step(&mut steps, "t1", "", "calc.py");
        upsert_step(&mut steps, "t2", "bash", "cargo test\nmore");
        end_step(&mut steps, "t1", true);
        end_step(&mut steps, "t2", false);
        assert_eq!(steps_text(&steps), "✓ Read · calc.py\n✗ bash · cargo test");
    }

    #[test]
    fn long_paths_read_short() {
        assert_eq!(short_path("/private/tmp/x/demo-calc/calc.py"), "demo-calc/calc.py");
        assert_eq!(short_path("src/calc.py"), "src/calc.py");
        assert_eq!(short_path("path: \"/a/b/c.py\""), "path: \"/a/b/c.py\"");
    }
}
