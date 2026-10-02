//! The loops' live logs: what each outer and inner loop does as it does it,
//! as a terminal's lines (ANSI colors), a file per loop under
//! `<data>/live/` (`s-<session>.log`, `p-<peer>.log`). The read-only live
//! view follows one (`tail -F` in Makepad's terminal, `native_tui.rs`): the
//! person watches a loop at work without taking its conversation over, and
//! scrolls back through what it did before.
//!
//! What goes in: each turn's start (what it was asked) and end (how it
//! ended, what it cost), what it says as it writes it, each tool call (its
//! command, its file) and the ones that failed, its subagents' calls, and
//! its status when that changes. Nothing of a key ever reaches a loop's
//! events, so nothing of one is here.
use crate::events::LoopEvent;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;

/// A log grows to this, then keeps its last quarter.
const MAX_BYTES: u64 = 4 << 20;

const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const MAGENTA: &str = "\x1b[35m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const BOLD: &str = "\x1b[1m";
const RESET: &str = "\x1b[0m";

pub fn dir() -> PathBuf {
    crate::model::data_dir().join("live")
}

/// The live log of a loop: `s-<session>` for an outer loop, `p-<peer>` for an inner one.
pub fn path(key: &str) -> PathBuf {
    dir().join(format!("{key}.log"))
}

pub fn outer_key(session: &str) -> String {
    format!("s-{session}")
}

pub fn inner_key(peer: &str) -> String {
    format!("p-{peer}")
}

fn clock() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // Local time: the offset the system says (`date +%z` is not to be run per line).
    let offset = local_offset();
    let t = (secs as i64 + offset).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60)
}

fn local_offset() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        let out = std::process::Command::new("date").arg("+%z").output().ok();
        let z = out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        let (sign, digits) = match z.split_at_checked(1) { Some(("-", d)) => (-1, d), Some((_, d)) => (1, d), None => (1, "") };
        let h: i64 = digits.get(..2).and_then(|h| h.parse().ok()).unwrap_or(0);
        let m: i64 = digits.get(2..4).and_then(|m| m.parse().ok()).unwrap_or(0);
        sign * (h * 3600 + m * 60)
    })
}

fn first_lines(text: &str, lines: usize, chars: usize) -> String {
    let mut out: Vec<String> = text.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).take(lines).map(|l| l.chars().take(chars).collect()).collect();
    if text.lines().filter(|l| !l.trim().is_empty()).count() > lines {
        out.push("…".into());
    }
    out.join("\n  ")
}

/// The live logs, as they are written.
#[derive(Default)]
pub struct Live {
    /// Where its logs are: `dir()` when none is set (a test sets one).
    root: Option<PathBuf>,
    files: HashMap<String, std::fs::File>,
    /// A loop whose last write left a line open (its text, as it streams).
    mid_line: HashSet<String>,
    /// Its last status, not written again.
    status: HashMap<String, String>,
    /// Tool calls already written (id): a later detail of one adds a line.
    tools: HashMap<String, (String, bool)>,
    /// A loop at work (a turn begun): the first event after its end starts one.
    working: HashSet<String>,
}

impl Live {
    fn file(&mut self, key: &str) -> Option<&mut std::fs::File> {
        if !self.files.contains_key(key) {
            let root = self.root.clone().unwrap_or_else(dir);
            let path = root.join(format!("{key}.log"));
            std::fs::create_dir_all(&root).ok()?;
            // Grown past its size: its last quarter is kept.
            if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
                if let Ok(all) = std::fs::read(&path) {
                    let keep = &all[all.len() - (MAX_BYTES / 4) as usize..];
                    let from = keep.iter().position(|&b| b == b'\n').map(|i| i + 1).unwrap_or(0);
                    let _ = std::fs::write(&path, &keep[from..]);
                }
            }
            let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok()?;
            self.files.insert(key.to_string(), file);
        }
        self.files.get_mut(key)
    }

    fn write(&mut self, key: &str, text: &str) {
        if let Some(f) = self.file(key) {
            let _ = f.write_all(text.as_bytes());
        }
    }

    /// A line of its own (the open one ends first).
    fn line(&mut self, key: &str, text: &str) {
        let open = self.mid_line.remove(key);
        let text = format!("{}{text}\n", if open { "\n" } else { "" });
        self.write(key, &text);
    }

    fn begin(&mut self, key: &str, what: &str) {
        if self.working.insert(key.to_string()) {
            self.status.remove(key);
            self.line(key, &format!("\n{BOLD}── {} {what} ──{RESET}", clock()));
        }
    }

    /// What a loop was asked: its turn begins with it.
    pub fn asked(&mut self, key: &str, who: &str, text: &str) {
        self.working.remove(key);
        self.begin(key, &format!("← {who}"));
        self.line(key, &format!("{DIM}  {}{RESET}", first_lines(text, 8, 160)));
    }

    /// A line OctoBuddy says in a loop's log (a check, a restart).
    pub fn note(&mut self, key: &str, text: &str) {
        self.line(key, &format!("{YELLOW}{} {text}{RESET}", clock()));
    }

    fn tool(&mut self, key: &str, id: &str, name: &str, detail: &str, sub: bool) {
        let tag = format!("{key}:{id}");
        match self.tools.get(&tag).cloned() {
            // Its detail, come later.
            Some((_, false)) if !detail.is_empty() => {
                self.tools.insert(tag, (name.to_string(), true));
                self.line(key, &format!("{DIM}    {}{RESET}", first_lines(detail, 3, 200)));
            }
            Some(_) => {}
            None => {
                self.tools.insert(tag, (name.to_string(), !detail.is_empty()));
                let (color, lead) = if sub { (MAGENTA, "  ↳ ") } else { (CYAN, "▶ ") };
                let detail = if detail.is_empty() { String::new() } else { format!(" {}", first_lines(detail, 3, 200)) };
                self.line(key, &format!("{DIM}{}{RESET} {color}{lead}{name}{RESET}{detail}", clock()));
            }
        }
    }

    fn tool_end(&mut self, key: &str, id: &str, ok: bool) {
        let tag = format!("{key}:{id}");
        let name = self.tools.remove(&tag).map(|(n, _)| n).unwrap_or_default();
        if !ok {
            self.line(key, &format!("{RED}  ✗ {name} failed{RESET}"));
        }
    }

    fn status(&mut self, key: &str, status: &str) {
        let status = status.trim();
        if status.is_empty() || self.status.get(key).map(String::as_str) == Some(status) {
            return;
        }
        self.status.insert(key.to_string(), status.to_string());
        self.line(key, &format!("{DIM}{} · {status}{RESET}", clock()));
    }

    fn text(&mut self, key: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        self.write(key, text);
        if text.ends_with('\n') {
            self.mid_line.remove(key);
        } else {
            self.mid_line.insert(key.to_string());
        }
    }

    fn end(&mut self, key: &str, how: &str, ok: bool) {
        self.working.remove(key);
        self.status.remove(key);
        let color = if ok { GREEN } else { RED };
        self.line(key, &format!("{color}── {} {how} ──{RESET}", clock()));
    }

    /// One of the loops' events, in its loop's log.
    pub fn log(&mut self, event: &LoopEvent) {
        use crate::i18n::t;
        match event {
            LoopEvent::LeadStatus { session, status } => {
                let key = outer_key(session);
                self.begin(&key, t("at work", "开始工作"));
                self.status(&key, status);
            }
            LoopEvent::LeadInfo { session, model } => self.status(&outer_key(session), &format!("model {model}")),
            LoopEvent::LeadDelta { session, text } => {
                let key = outer_key(session);
                self.begin(&key, t("at work", "开始工作"));
                self.text(&key, text);
            }
            LoopEvent::LeadTool { session, id, name, detail } => {
                let key = outer_key(session);
                self.begin(&key, t("at work", "开始工作"));
                self.tool(&key, id, name, detail, false);
            }
            LoopEvent::LeadToolEnd { session, id, ok } => self.tool_end(&outer_key(session), id, *ok),
            LoopEvent::LeadSubTool { session, parent, name, detail } => {
                let key = outer_key(session);
                let id = format!("{parent}/{name}/{}", detail.chars().take(80).collect::<String>());
                self.tool(&key, &id, name, detail, true);
            }
            LoopEvent::LeadMessage { session, .. } => {
                let key = outer_key(session);
                if self.mid_line.remove(&key) {
                    self.write(&key, "\n");
                }
            }
            LoopEvent::LeadTurnDone { session, ok, error, cost, .. } => {
                let cost = cost.map(|c| format!(" · ${c:.3}")).unwrap_or_default();
                let how = match (ok, error) {
                    (true, _) => format!("{}{cost}", t("turn done", "这一轮结束")),
                    (false, Some(e)) => format!("{}: {}{cost}", t("turn failed", "这一轮失败"), e.chars().take(160).collect::<String>()),
                    (false, None) => format!("{}{cost}", t("turn failed", "这一轮失败")),
                };
                self.end(&outer_key(session), &how, *ok);
            }
            LoopEvent::LeadExited { session, error: Some(e), .. } => {
                self.note(&outer_key(session), &format!("{}: {e}", t("its process ended", "进程退出")));
            }
            LoopEvent::PeerTurnStarted { peer } => self.begin(&inner_key(peer), t("at work", "开始工作")),
            LoopEvent::PeerActivity { peer, line } => {
                let key = inner_key(peer);
                self.begin(&key, t("at work", "开始工作"));
                self.status(&key, line);
            }
            LoopEvent::PeerDelta { peer, text } => {
                let key = inner_key(peer);
                self.begin(&key, t("at work", "开始工作"));
                self.text(&key, text);
            }
            LoopEvent::PeerTool { peer, id, name, detail } => {
                let key = inner_key(peer);
                self.begin(&key, t("at work", "开始工作"));
                self.tool(&key, id, name, detail, false);
            }
            LoopEvent::PeerToolEnd { peer, id, ok } => self.tool_end(&inner_key(peer), id, *ok),
            LoopEvent::PeerSubagent { peer, role, title, status, summary, .. } => {
                let what = if title.is_empty() { role } else { title };
                let summary = if summary.is_empty() { String::new() } else { format!(": {}", summary.chars().take(160).collect::<String>()) };
                self.line(&inner_key(peer), &format!("{MAGENTA}  ↳ [{what}] {status}{summary}{RESET}"));
            }
            LoopEvent::PeerTurnEnded { peer, outcome, .. } => {
                let how = match outcome.as_str() {
                    "completed" => t("turn done", "这一轮结束"),
                    "interrupted" => t("turn interrupted", "这一轮被打断"),
                    _ => t("turn failed", "这一轮失败"),
                };
                self.end(&inner_key(peer), how, outcome == "completed");
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_reads_as_a_terminal_would_show_it() {
        let home = std::env::temp_dir().join(format!("octobuddy-live-{}", std::process::id()));
        let mut live = Live { root: Some(home.clone()), ..Default::default() };
        live.asked("p-w1", "outer", "spec: task\nbuild it");
        live.log(&LoopEvent::PeerDelta { peer: "w1".into(), text: "Reading ".into() });
        live.log(&LoopEvent::PeerDelta { peer: "w1".into(), text: "the file".into() });
        live.log(&LoopEvent::PeerTool { peer: "w1".into(), id: "t1".into(), name: "bash".into(), detail: "ls -la".into() });
        live.log(&LoopEvent::PeerToolEnd { peer: "w1".into(), id: "t1".into(), ok: false });
        live.log(&LoopEvent::PeerActivity { peer: "w1".into(), line: "step 2 · thinking".into() });
        live.log(&LoopEvent::PeerActivity { peer: "w1".into(), line: "step 2 · thinking".into() });
        live.log(&LoopEvent::PeerTurnEnded { peer: "w1".into(), outcome: "completed".into(), text: String::new() });
        let text = std::fs::read_to_string(home.join("p-w1.log")).unwrap();
        assert!(text.contains("build it") && text.contains("Reading the file\n"), "{text}");
        assert!(text.contains("bash") && text.contains("ls -la") && text.contains("failed"));
        assert_eq!(text.matches("step 2 · thinking").count(), 1, "a status is written when it changes");
        let _ = std::fs::remove_dir_all(home);
    }
}
