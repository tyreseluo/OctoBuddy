//! What passes between the outer loop and an inner loop, kept small (after
//! Cindy's team mode, where a worker's task is a thin envelope and its
//! answer one short message): the outer loop writes a task card, OctoBuddy
//! adds what the inner loop would otherwise go and read (its files' map, the
//! excerpts the outer loop already read, the memory that bears on it), and
//! the inner loop answers with a short report OctoBuddy completes with the
//! facts it has (files, check, commit).
//!
//! A task card:
//!
//! ```text
//! ## Goal
//! <one to three lines: what, and why>
//! ## Files
//! - <the paths it owns: src/calc.py, app/parts/20-*.splash, tests/>
//! ## Facts
//! - <what it must know that its files do not say: decisions, names, interfaces>
//! ## Done when
//! - <at most three results its check proves>
//! ```
use std::path::Path;

/// The most a report reaches the outer loop with: past it, its start and
/// where the whole of it is.
pub const REPORT_MAX: usize = 1200;
/// The end of a reply OctoBuddy forwards when an inner loop did not report.
pub const FORWARD_MAX: usize = 2500;
/// The excerpts the outer loop hands on, all together, and each.
pub const READ_MAX: usize = 6000;
const READ_EACH: usize = 2500;
/// An inner loop's map of its own files.
const MAP_MAX: usize = 1500;
/// What of the memory an inner loop is given.
pub const MEMORY_MAX: usize = 1000;
/// Reads of the same thing in one turn before OctoBuddy says so.
pub const READ_LOOP: usize = 3;
/// The project's map, in the folder the loops share (kept out of git).
pub const KNOWLEDGE: &str = ".octobuddy/knowledge/MAP.md";

/// The lines under a card's `## <heading>` (or a contract's `### Allowed
/// Changes`), up to the next heading.
fn section<'a>(brief: &'a str, names: &[&str]) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in brief.lines() {
        let t = line.trim();
        if t.starts_with('#') {
            let title = t.trim_start_matches('#').trim().to_ascii_lowercase();
            inside = names.iter().any(|n| title == *n);
            continue;
        }
        if inside && !t.is_empty() {
            out.push(t);
        }
    }
    out
}

/// The paths a task card (or a contract) gives its inner loop.
pub fn card_files(brief: &str) -> Vec<String> {
    section(brief, &["files", "allowed changes"]).into_iter()
        .filter_map(|l| l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")))
        // "src/a.rs (the parser)": the path, without what is said of it.
        .map(|l| l.split([' ', '\t']).next().unwrap_or("").trim_matches('`').trim_end_matches([',', ';']).to_string())
        .filter(|p| !p.is_empty() && (p.contains('/') || p.contains('.') || p.contains('*')))
        .collect()
}

/// Whether `path` is one of `pattern`'s: the same path, a folder's
/// (`tests/`), or a glob (`*`, `**`).
pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_start_matches("./");
    let path = path.trim_start_matches("./");
    if pattern.ends_with('/') {
        return path.starts_with(pattern);
    }
    if !pattern.contains('*') {
        return path == pattern || path.starts_with(&format!("{pattern}/"));
    }
    glob(pattern.as_bytes(), path.as_bytes())
}

fn glob(p: &[u8], s: &[u8]) -> bool {
    match p {
        [] => s.is_empty(),
        [b'*', b'*', rest @ ..] => {
            let rest = rest.strip_prefix(b"/").unwrap_or(rest);
            (0..=s.len()).any(|i| glob(rest, &s[i..]))
        }
        [b'*', rest @ ..] => (0..=s.len()).take_while(|&i| i == 0 || s[i - 1] != b'/').any(|i| glob(rest, &s[i..])),
        [c, rest @ ..] => s.first() == Some(c) && glob(rest, &s[1..]),
    }
}

/// The files an inner loop changed outside its card's: empty when its card
/// names none (nothing to hold it to).
pub fn outside(card: &[String], touched: &[String]) -> Vec<String> {
    if card.is_empty() {
        return Vec::new();
    }
    touched.iter().filter(|f| !card.iter().any(|p| matches(p, f))).cloned().collect()
}

/// A file's definitions with their line numbers, by its kind: what an inner
/// loop would grep for to find its way in it.
fn definitions(rel: &str, text: &str) -> Vec<String> {
    let ext = rel.rsplit('.').next().unwrap_or("");
    let starts: &[&str] = match ext {
        "splash" => &["fn ", "let "],
        "rs" => &["pub fn ", "fn ", "pub struct ", "struct ", "pub enum ", "enum ", "impl ", "pub trait ", "trait ", "mod ", "pub mod ", "pub(crate) fn ", "const ", "pub const "],
        "py" => &["def ", "class ", "async def "],
        "js" | "ts" | "tsx" | "jsx" | "mjs" => &["function ", "export function ", "export default function ", "class ", "export class ", "const ", "export const ", "interface ", "export interface "],
        "go" => &["func ", "type "],
        "md" => &["# ", "## ", "### "],
        _ => &[],
    };
    text.lines().enumerate().filter_map(|(i, line)| {
        // Top level only (a Splash or Rust item, a Python def), and named
        // widgets in a Splash file (`list := ScrollYView{…}`).
        let top = !line.starts_with([' ', '\t']);
        let t = line.trim();
        let named_widget = ext == "splash" && t.contains(" := ") && t.split(" := ").next().is_some_and(|n| n.chars().all(|c| c.is_alphanumeric() || c == '_'));
        let def = top && starts.iter().any(|s| t.starts_with(s));
        (def || named_widget).then(|| {
            let head: String = t.chars().take(60).collect();
            let head = head.split('{').next().unwrap_or(&head).trim_end().to_string();
            format!("L{} {head}", i + 1)
        })
    }).collect()
}

/// The project's files a list of patterns names, that exist: tracked by git
/// when it can tell, else those it can see.
fn expand(dir: &Path, patterns: &[String]) -> Vec<String> {
    let listed = std::process::Command::new("git").args(["ls-files", "--cached", "--others", "--exclude-standard"]).current_dir(dir).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect::<Vec<_>>());
    match listed {
        Some(files) => files.into_iter().filter(|f| patterns.iter().any(|p| matches(p, f))).collect(),
        None => patterns.iter().filter(|p| !p.contains('*') && dir.join(p).is_file()).cloned().collect(),
    }
}

/// The map of an inner loop's own files (the ones its card names): each with
/// its length and its definitions, so it reads the ranges it needs.
pub fn file_map(dir: &Path, card: &[String]) -> String {
    let mut out = String::new();
    for rel in expand(dir, card).into_iter().take(12) {
        let Ok(text) = std::fs::read_to_string(dir.join(&rel)) else { continue };
        let defs = definitions(&rel, &text);
        let line = format!("{rel} ({} lines){}{}\n", text.lines().count(), if defs.is_empty() { "" } else { ": " }, defs.join(" · "));
        if out.len() + line.len() > MAP_MAX {
            out.push_str(&format!("{rel} ({} lines)\n", text.lines().count()));
            continue;
        }
        out.push_str(&line);
    }
    out
}

/// The excerpts the outer loop names (`path`, `path:L10-L40`, `path:10-40`),
/// read now: current when the inner loop starts.
pub fn excerpts(dir: &Path, reads: &[String]) -> String {
    let mut out = String::new();
    for want in reads {
        let (rel, range) = match want.rsplit_once(':') {
            Some((rel, range)) if range.trim_start_matches('L').starts_with(|c: char| c.is_ascii_digit()) => (rel.trim(), Some(range)),
            _ => (want.trim(), None),
        };
        let Ok(text) = std::fs::read_to_string(dir.join(rel)) else {
            out.push_str(&format!("\n{rel}: (not found)\n"));
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        let (from, to) = match range.and_then(|r| r.split_once('-')) {
            Some((a, b)) => {
                let n = |s: &str| s.trim().trim_start_matches('L').parse::<usize>().ok();
                (n(a).unwrap_or(1).max(1), n(b).unwrap_or(lines.len()).min(lines.len()))
            }
            None => (1, lines.len()),
        };
        if from > to {
            continue;
        }
        let mut body = String::new();
        let mut cut = false;
        for (i, l) in lines[from - 1..to].iter().enumerate() {
            let line = format!("{:>4}  {l}\n", from + i);
            if body.len() + line.len() > READ_EACH || out.len() + body.len() + line.len() > READ_MAX {
                cut = true;
                body.push_str(&format!("      … (cut at line {}: read on from there)\n", from + i));
                break;
            }
            body.push_str(&line);
        }
        out.push_str(&format!("\n{rel}:{from}-{to}\n{body}"));
        if cut && out.len() >= READ_MAX {
            break;
        }
    }
    out
}

/// A report as the outer loop gets it: whole when short, else its start and
/// where the rest is.
pub fn cap_report(report: &str, whole_at: &str) -> String {
    let report = report.trim();
    if report.chars().count() <= REPORT_MAX {
        return report.to_string();
    }
    let start: String = report.chars().take(REPORT_MAX).collect();
    format!("{start} …\n(The report is longer than {REPORT_MAX} characters: the whole of it is in {whole_at}.)")
}

/// Who owns which files, from the cards: for every slice, said once.
pub fn ownership(cards: &[(String, String)]) -> String {
    cards.iter().filter_map(|(slug, brief)| {
        let files = card_files(brief);
        (!files.is_empty()).then(|| format!("- {slug}: {}", files.join(", ")))
    }).collect::<Vec<_>>().join("\n")
}

/// The memory part of a context pack, for an inner loop: without the outer
/// loop's estimation calibration, and short.
pub fn inner_memory(pack: &str) -> String {
    let Some(start) = pack.find("MEMORY") else { return String::new() };
    let rest = &pack[start..];
    let end = rest.find("\n\nDOCS INDEX").unwrap_or(rest.len());
    let mut skip = false;
    let mut out = String::new();
    for line in rest[..end].lines().skip(1) {
        if line.starts_with("wing ") {
            skip = line.contains("\"calibration\"");
            if !skip {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        }
        if !skip && !line.trim().is_empty() {
            out.push_str(line);
            out.push('\n');
        }
    }
    if out.chars().count() > MEMORY_MAX {
        out = out.chars().take(MEMORY_MAX).collect::<String>() + " …\n";
    }
    out
}

/// A read tool's step, as what it read (`Read src/a.rs:10-20`, a `sed -n`
/// or `cat` in a shell): the same text twice is the same read.
pub fn read_of(name: &str, detail: &str) -> Option<String> {
    let low = name.to_ascii_lowercase();
    let first = detail.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        return None;
    }
    let reads = ["read", "read_file", "view", "cat"].iter().any(|r| low == *r || low.ends_with(&format!("_{r}")));
    let shell_read = matches!(low.as_str(), "bash" | "shell" | "exec" | "run_shell_command" | "shell_exec")
        && ["sed -n", "cat ", "head ", "tail ", "nl "].iter().any(|c| first.starts_with(c) || first.contains(&format!("&& {c}")));
    (reads || shell_read).then(|| format!("{name} {first}"))
}

/// The read an inner loop repeated `READ_LOOP` times or more this turn.
pub fn repeated_read<'a>(steps: impl Iterator<Item = (&'a str, &'a str)>) -> Option<(String, usize)> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for (name, detail) in steps {
        let Some(read) = read_of(name, detail) else { continue };
        match seen.iter_mut().find(|(r, _)| *r == read) {
            Some((_, n)) => *n += 1,
            None => seen.push((read, 1)),
        }
    }
    seen.into_iter().filter(|(_, n)| *n >= READ_LOOP).max_by_key(|(_, n)| *n)
}

/// The project's map, kept in `.octobuddy/knowledge/MAP.md` and brought up
/// to date as work is accepted: its source files with their definitions,
/// and what each accepted slice did. Any loop starts from it instead of
/// finding its way through the project again.
pub fn project_map(dir: &Path, accepted: &[String]) -> String {
    let files = std::process::Command::new("git").args(["ls-files"]).current_dir(dir).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(String::from).collect::<Vec<_>>())
        .unwrap_or_default();
    let source = ["splash", "rs", "py", "js", "ts", "tsx", "jsx", "go", "json", "md", "toml"];
    let mut out = String::from("# Project map\n\n(OctoBuddy keeps this file: the project's files with what is defined where, \
and the work accepted so far. Read a file's ranges from here instead of searching for them.)\n\n## Files\n\n");
    let sources = files.iter().filter(|f| source.iter().any(|e| f.ends_with(&format!(".{e}"))) && !f.starts_with(".octobuddy/"));
    for (listed, rel) in sources.enumerate() {
        if listed >= 150 {
            out.push_str("- … (more files: `git ls-files`)\n");
            break;
        }
        let Ok(text) = std::fs::read_to_string(dir.join(rel)) else { continue };
        let defs = if rel.ends_with(".json") || rel.ends_with(".toml") { Vec::new() } else { definitions(rel, &text) };
        let shown: Vec<&String> = defs.iter().take(12).collect();
        let more = if defs.len() > shown.len() { format!(" · … {} more", defs.len() - shown.len()) } else { String::new() };
        out.push_str(&format!("- {rel} ({} lines){}{}{more}\n", text.lines().count(), if shown.is_empty() { "" } else { ": " },
            shown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" · ")));
    }
    if !accepted.is_empty() {
        out.push_str("\n## Work accepted\n\n");
        for a in accepted.iter().rev().take(40).rev() {
            out.push_str(&format!("- {a}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "## Goal\nAdd the stats page.\n## Files\n- app/parts/20-stats.splash (its page)\n- `tests/stats/`\n- docs/*.md\n## Facts\n- totals are cents\n## Done when\n- the page draws\n";

    #[test]
    fn a_card_names_its_files_and_holds_its_loop_to_them() {
        assert_eq!(card_files(CARD), ["app/parts/20-stats.splash", "tests/stats/", "docs/*.md"]);
        let contract = "spec: task\n## Boundaries\n### Allowed Changes\n- src/calc.py\n### Forbidden\n- src/db.py\n";
        assert_eq!(card_files(contract), ["src/calc.py"]);
        let card = card_files(CARD);
        let touched = ["app/parts/20-stats.splash", "tests/stats/a.py", "docs/x.md", "docs/sub/y.md", "app/parts/10-home.splash"].map(String::from);
        assert_eq!(outside(&card, &touched), ["docs/sub/y.md", "app/parts/10-home.splash"]);
        assert!(outside(&[], &touched).is_empty(), "no files named: nothing to hold it to");
        assert!(matches("src/**/*.rs", "src/a/b/c.rs") && matches("src/**/*.rs", "src/c.rs") && !matches("src/*.rs", "src/a/c.rs"));
        assert!(matches("src", "src/a.rs") && !matches("src", "srcs/a.rs"));
    }

    #[test]
    fn its_files_are_mapped_and_excerpts_read_now() {
        let dir = std::env::temp_dir().join(format!("octobuddy-handoff-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("app/parts")).unwrap();
        std::fs::write(dir.join("app/parts/20-stats.splash"), "// stats\nlet total = 0\nfn stats_sum(){\n    total = 1\n}\nlet StatsPage = View{\n    stats_list := ScrollYView{}\n}\n").unwrap();
        let map = file_map(&dir, &["app/parts/20-stats.splash".to_string()]);
        assert!(map.contains("app/parts/20-stats.splash (8 lines): L2 let total = 0 · L3 fn stats_sum() · L6 let StatsPage = View · L7 stats_list := ScrollYView"), "{map}");
        let got = excerpts(&dir, &["app/parts/20-stats.splash:L3-L5".to_string(), "missing.rs".to_string()]);
        assert!(got.contains("app/parts/20-stats.splash:3-5\n   3  fn stats_sum(){\n   4      total = 1\n   5  }\n"), "{got}");
        assert!(got.contains("missing.rs: (not found)"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_long_report_is_cut_and_pointed_at() {
        assert_eq!(cap_report("status: done", "x"), "status: done");
        let long = "a".repeat(REPORT_MAX + 50);
        let cut = cap_report(&long, "reports/s.md");
        assert!(cut.starts_with(&"a".repeat(REPORT_MAX)) && cut.ends_with("the whole of it is in reports/s.md.)"));
    }

    #[test]
    fn the_inner_memory_leaves_out_calibration() {
        let pack = "CONTEXT\n\nMEMORY (OctoBuddy searched…):\nwing \"p\":\n- [lessons] use cents\nwing \"p\", room \"calibration\":\n- [calibration] 1.5 min a round\n\nDOCS INDEX (…):\n.octobuddy/docs/A.md";
        let m = inner_memory(pack);
        assert!(m.contains("use cents") && !m.contains("1.5 min") && !m.contains("DOCS"), "{m}");
        assert_eq!(ownership(&[("stats".into(), CARD.into()), ("x".into(), "no card".into())]), "- stats: app/parts/20-stats.splash, tests/stats/, docs/*.md");
    }

    #[test]
    fn the_same_read_three_times_is_a_loop() {
        let steps = [("Bash", "sed -n '329,342p' bundle/main.splash"), ("Read", "a.rs"), ("Bash", "sed -n '329,342p' bundle/main.splash"),
            ("Edit", "a.rs"), ("Bash", "sed -n '329,342p' bundle/main.splash"), ("Bash", "cargo test")];
        assert_eq!(repeated_read(steps.into_iter()), Some(("Bash sed -n '329,342p' bundle/main.splash".to_string(), 3)));
        assert_eq!(repeated_read([("Read", "a.rs"), ("Read", "a.rs")].into_iter()), None);
        assert!(read_of("Bash", "cargo test").is_none());
    }
}
