//! The context pack: what an outer loop would otherwise spend its first
//! minutes fetching, fetched by OctoBuddy before the request reaches it, as
//! Cindy does — the project's memory searched with the request's own words
//! (its wing, and for an app the wing every app shares), and, on a
//! session's first request, an index of the docs it plans with: each one's
//! headings with their line numbers, so it reads the sections it needs, not
//! the whole files. It goes ahead of the request in the outer loop's message
//! (never into its system prompt, which stays the same and cached), and
//! into `.octobuddy/context/<session>/pack.md` for the inner loops.
use crate::memory;
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Hits per wing, and how much of each.
const HITS: usize = 5;
const HIT_CHARS: usize = 600;
/// Headings per doc.
const HEADINGS: usize = 40;

/// mempal's best hits for `query` in `wing` (`room`: one room only).
fn search(bin: &Path, wing: &str, room: Option<&str>, query: &str) -> Vec<(String, String)> {
    let mut cmd = Command::new(bin);
    cmd.args(["search", "--wing", wing, "--top-k", &HITS.to_string(), "--json"]);
    if let Some(room) = room {
        cmd.args(["--room", room]);
    }
    let Ok(out) = cmd.arg(query).output() else { return Vec::new() };
    let Ok(v) = serde_json::from_slice::<Value>(&out.stdout) else { return Vec::new() };
    v.as_array().into_iter().flatten().filter_map(|hit| {
        let text = hit.get("content")?.as_str()?.trim();
        let room = hit.get("room").and_then(Value::as_str).unwrap_or("").to_string();
        let mut text: String = text.chars().take(HIT_CHARS).collect();
        if hit["content"].as_str().is_some_and(|t| t.chars().count() > HIT_CHARS) {
            text.push_str(" …");
        }
        Some((room, text))
    }).collect()
}

/// A doc's headings with their line numbers (`L12 ## Storage`).
pub fn headings(text: &str) -> Vec<String> {
    let mut fence = false;
    text.lines().enumerate().filter_map(|(i, l)| {
        if l.trim_start().starts_with("```") {
            fence = !fence;
        }
        (!fence && (l.starts_with("## ") || l.starts_with("### ") || l.starts_with("# "))).then(|| format!("L{} {}", i + 1, l.trim()))
    }).take(HEADINGS).collect()
}

/// The pack for a request to the outer loop of `project` (working in `cwd`):
/// `first`, the session's first request (the docs index then too);
/// `app`, an OctoSense app (its shared wing, its docs). None: nothing found.
/// A wing's (and room's) search: what it found, as (drawer, text).
type Found<'a> = (String, Option<&'a str>, Vec<(String, String)>);

pub fn build(project: &str, cwd: &str, request: &str, first: bool, app: bool) -> Option<String> {
    let query: String = request.chars().take(300).collect();
    let mut parts = Vec::new();
    if let Some(bin) = memory::mempal() {
        let wing = memory::wing(project);
        let mut wings = vec![(wing.clone(), None)];
        if app {
            wings.push((crate::plugins::octosense_app::SHARED_WING.to_string(), None));
        }
        wings.push((wing, Some("calibration")));
        // The wings at once: each search takes a second or two.
        let found: Vec<Found> = std::thread::scope(|s| {
            let handles: Vec<_> = wings.iter().map(|(w, room)| {
                let (bin, query) = (&bin, &query);
                s.spawn(move || (w.clone(), *room, search(bin, w, *room, query)))
            }).collect();
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        let mut memory = String::new();
        for (wing, room, hits) in found.into_iter().filter(|(_, _, h)| !h.is_empty()) {
            let place = room.map(|r| format!("wing \"{wing}\", room \"{r}\"")).unwrap_or_else(|| format!("wing \"{wing}\""));
            memory.push_str(&format!("\n{place}:\n"));
            for (room, text) in hits {
                memory.push_str(&format!("- [{room}] {}\n", text.replace('\n', " ⏎ ")));
            }
        }
        if !memory.is_empty() {
            parts.push(format!("MEMORY (OctoBuddy searched mempal with this request's words; search again only for what is missing):{memory}"));
        }
    }
    if first && app {
        let docs = Path::new(cwd).join(".octobuddy/docs");
        let mut names: Vec<_> = std::fs::read_dir(&docs).into_iter().flatten().flatten()
            .map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "md")).collect();
        names.sort();
        let mut index = String::new();
        for path in names {
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let rel = path.strip_prefix(cwd).map(|p| p.display().to_string()).unwrap_or_else(|_| path.display().to_string());
            index.push_str(&format!("\n{rel} ({} lines): {}\n", text.lines().count(), headings(&text).join(" · ")));
        }
        if !index.is_empty() {
            parts.push(format!("DOCS INDEX (read only the sections your plan needs: Read with offset/limit at these lines; the inner loops have the same copies):{index}"));
        }
    }
    (!parts.is_empty()).then(|| format!("CONTEXT (prepared by OctoBuddy before your turn)\n\n{}", parts.join("\n\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_doc_is_indexed_by_its_headings() {
        let doc = "# Title\nintro\n## Storage\n```\n## not a heading\n```\n### Keys\n";
        assert_eq!(headings(doc), ["L1 # Title", "L3 ## Storage", "L7 ### Keys"]);
    }
}
