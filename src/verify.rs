//! OctoBuddy runs a slice's tests itself. The lead can only read, and a
//! peer's word that its tests pass is a claim: the command the lead names
//! for a slice (`check` in the plan) runs in the peer's clone after its
//! work, and its exit code and the end of its output go into the report.
//!
//! It runs on this machine with the person's rights, not in octos's
//! sandbox: the command is shown when the round starts and in the peer's
//! panel, and it only ever runs in the peer's own directory.
use crate::workspace::search_path;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long a check may run before it is stopped.
pub const TIMEOUT: Duration = Duration::from_secs(600);
const TAIL_LINES: usize = 25;

#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub passed: bool,
    /// `exit 0`, `exit 1`, `timed out after 600s`, `could not start: …`.
    pub status: String,
    pub tail: String,
}

impl Outcome {
    /// One line for the report's verdicts, then the output's end.
    pub fn summary(&self, command: &str) -> String {
        let verdict = if self.passed { "passed" } else { "FAILED" };
        let mut out = format!("[tests] `{command}` {verdict} ({})", self.status);
        if !self.tail.is_empty() {
            // Fenced: a test runner's rule of dashes would make the line
            // above it a heading.
            let longest = self.tail.split(|c| c != '`').map(str::len).max().unwrap_or(0);
            let fence = "`".repeat(longest.max(2) + 1);
            out.push_str(&format!("\n{fence}\n{}\n{fence}", self.tail));
        }
        out
    }
}

pub fn run(command: &str, dir: &str, timeout: Duration) -> Outcome {
    let child = Command::new("sh").arg("-c").arg(format!("exec 2>&1\n{command}")).current_dir(dir)
        .env("PATH", search_path()).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(err) => return Outcome { passed: false, status: format!("could not start: {err}"), tail: String::new() },
    };
    // Read on a thread so a chatty command never blocks on a full pipe.
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut out);
        }
        out
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().map(|c| format!("exit {c}")).unwrap_or_else(|| "killed by a signal".into()),
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break format!("timed out after {}s", timeout.as_secs());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(err) => break format!("could not wait: {err}"),
        }
    };
    let out = String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned();
    let lines: Vec<&str> = out.trim_end().lines().collect();
    let tail = lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n");
    Outcome { passed: status == "exit 0", status, tail }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_passes_fails_or_times_out() {
        let dir = std::env::temp_dir();
        let dir = dir.to_string_lossy();
        let ok = run("echo one; echo two >&2", &dir, TIMEOUT);
        assert_eq!(ok, Outcome { passed: true, status: "exit 0".into(), tail: "one\ntwo".into() });
        assert_eq!(ok.summary("make test"), "[tests] `make test` passed (exit 0)\n```\none\ntwo\n```");
        let bad = run("seq 1 40; exit 3", &dir, TIMEOUT);
        assert!(!bad.passed && bad.status == "exit 3");
        assert!(bad.tail.starts_with("16\n") && bad.tail.ends_with("40"), "only the end is kept");
        let slow = run("sleep 5", &dir, Duration::from_millis(300));
        assert!(!slow.passed && slow.status.starts_with("timed out"));
    }
}
