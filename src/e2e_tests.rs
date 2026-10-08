//! End-to-end test of the dual loop: outer plan → inner slice writes files →
//! host check → commit → review → wave gating.
//!
//! Drives the highest testable entries in `orchestrate.rs` / `inner.rs`:
//!
//! * Outer loop side: the lead's reply parses into a plan (`plan::split_reply`)
//!   and the review into verdicts; the wave gating the orchestration runs once
//!   both waves have settled (`orchestrate::wave_may_start`).
//! * Inner loop side: a real `inner::Serve` runs against a stub `octos
//!   serve --stdio` (`src/stubs/stub_octos.py`); on `turn/start` the stub
//!   writes the slice's source file in its session cwd and streams the
//!   envelopes a real turn would — they come back as `LoopEvent`s
//!   (`PeerTurnStarted`, `PeerTool`, `PeerToolEnd`, `PeerDelta`,
//!   `PeerTurnEnded`).
//! * Host side: the slice's check runs (`verify::run`) and its files are
//!   committed with the message the peer named (`workspace::commit_files`),
//!   exactly as `OctoBuddyView::on_peer_turn_ended` does.
//!
//! What this does NOT cover (CI cannot construct the Makepad widget tree):
//!
//! * `OctoBuddyView::apply_event` — the state machine that joins the inner
//!   loop's `PeerTurnEnded` to the outer loop's `LeadMessage`. The pure
//!   functions it composes (plan parsing, report parsing, review parsing,
//!   check running, committing, wave gating) ARE covered here.
//! * The actual lead agent process (`claude -p`, `codex app-server`,
//!   `pi --mode rpc`). The plan text the lead would write is fixed in
//!   `src/stubs/plan_two_waves.md`; the real agent would format the same
//!   shape.
//! * Real approval gates, MCP servers, model questions, steering.
//!
//! To keep this test hermetic it uses only what is in this worktree (the
//!   stub script, `python3` on PATH, `git` on PATH). No network, no real
//!   API keys, no real agent CLI.

use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use crate::chat::tests::peer as make_peer;
use crate::events::{take, Inbox, LoopEvent};
use crate::inner;
use crate::model::Peer;
use crate::orchestrate::{auto_message, commit_message, wave_may_start};
use crate::plan;
use crate::verify;
use crate::workspace;

/// The plan block the outer loop "wrote" for this run: two slices, the
/// second depending on the first (a wave-2 slice).
const PLAN_TWO_WAVES: &str = include_str!("stubs/plan_two_waves.md");

/// The review block the outer loop "wrote" after wave 1 passed: accept the
/// first slice so wave 2 can start.
const REVIEW_WAVE_1: &str = include_str!("stubs/review_wave_1.md");

/// Builds a fresh worktree-less git repo at `dir`, with one initial commit.
fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.to_string_lossy().into_owned();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "e2e@example.invalid"],
        &["config", "user.name", "e2e"][..],
        &["config", "commit.gpgsign", "false"][..],
    ] {
        workspace::git(&p, args).unwrap();
    }
    std::fs::write(dir.join("README.md"), "# stub project\n").unwrap();
    std::fs::write(dir.join("calc.py"), "# placeholder\n").unwrap();
    workspace::git(&p, &["add", "-A"]).unwrap();
    workspace::git(&p, &["commit", "-qm", "chore: init"]).unwrap();
}

/// Sets `OCTOBUDDY_OCTOS_BIN` so `find_bin("octos")` returns our stub
/// script, and `OCTOBUDDY_HOME` so `data_dir()` resolves under our temp.
struct Env {
    saved_octos_bin: Option<std::ffi::OsString>,
    saved_home: Option<std::ffi::OsString>,
    /// The launcher copy kept alive for the duration of the test; with its
    /// executable bit set so `find_bin("octos").is_file()` is true.
    _launcher: std::fs::File,
}

impl Env {
    fn install(home: &Path, stub: &Path) -> Self {
        let saved_octos_bin = std::env::var_os("OCTOBUDDY_OCTOS_BIN");
        let saved_home = std::env::var_os("OCTOBUDDY_HOME");
        // `find_bin("octos").is_file()` must be true (otherwise `agents::ready`
        // tries to download the real one). Copy the stub to a path inside
        // `home` and ship a tiny POSIX shell wrapper alongside it that runs
        // the script with `python3` — both executable, both inside `home`,
        // so a stray directory can be removed without leaving stragglers.
        let bin_dir = home.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let launcher = bin_dir.join("octos");
        std::fs::write(
            &launcher,
            format!(
                "#!/bin/sh\nexec {} \"$(dirname \"$0\")/stub_octos.py\" \"$@\"\n",
                std::env::var("OCTOBUDDY_OCTOS_PYTHON").unwrap_or_else(|_| "python3".to_string()),
            ),
        )
        .unwrap();
        std::fs::copy(stub, bin_dir.join("stub_octos.py")).unwrap();
        set_executable(&launcher);
        std::env::set_var("OCTOBUDDY_OCTOS_BIN", &launcher);
        std::env::set_var("OCTOBUDDY_HOME", home);
        let _launcher = std::fs::OpenOptions::new().read(true).open(&launcher).unwrap();
        Self { saved_octos_bin, saved_home, _launcher }
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = std::fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    std::fs::set_permissions(path, perm).unwrap();
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) {}

impl Drop for Env {
    fn drop(&mut self) {
        match &self.saved_octos_bin {
            Some(v) => std::env::set_var("OCTOBUDDY_OCTOS_BIN", v),
            None => std::env::remove_var("OCTOBUDDY_OCTOS_BIN"),
        }
        match &self.saved_home {
            Some(v) => std::env::set_var("OCTOBUDDY_HOME", v),
            None => std::env::remove_var("OCTOBUDDY_HOME"),
        }
    }
}

/// Drains the inbox until `pred` finds an event, or `timeout` elapses.
fn wait_for(inbox: &Inbox, pred: impl Fn(&LoopEvent) -> bool, timeout: Duration) -> Option<LoopEvent> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        for e in take(inbox) {
            if pred(&e) {
                return Some(e);
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Mutates `base` (the inner-loop test peer constructor) with the fields
/// the wave-gating tests care about.
fn peer_with(slug: &str, wave: u32, review: Option<&str>, status: &str) -> Peer {
    let mut p = make_peer(Vec::new());
    p.slug = slug.into();
    p.wave = Some(wave);
    p.review = review.map(String::from);
    p.status = status.into();
    p
}

/// The chain: the outer loop's plan block parses, the inner loop's turn
/// completes via a real `Serve` against the stub octos (which writes the
/// slice's source into the session's cwd), the host runs the slice's
/// check, commits its files, and the wave-2 slice is held until wave-1's
/// review accepts.
#[test]
fn dual_loop_plans_two_slices_runs_inner_then_checks_commits_and_accepts() {
    let workspace_root = std::env::temp_dir().join(format!(
        "octobuddy-e2e-{}",
        std::process::id()
    ));
    let project_dir = workspace_root.join("project");
    let home_dir = workspace_root.join("home");
    let data_dir = home_dir.join("octos").join("serve");
    let _ = std::fs::remove_dir_all(&workspace_root);
    std::fs::create_dir_all(&project_dir).unwrap();
    std::fs::create_dir_all(&data_dir).unwrap();
    let stub = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/stubs/stub_octos.py");
    let _env = Env::install(&home_dir, &stub);

    init_repo(&project_dir);

    // The outer loop's plan block parses into the orchestration's `Reply`.
    let parsed = plan::split_reply(PLAN_TWO_WAVES);
    assert_eq!(parsed.slices.len(), 2, "two slices in the outer loop's reply");
    assert_eq!(parsed.slices[0].slug, "feat-add");
    assert_eq!(parsed.slices[0].wave, Some(1));
    assert_eq!(parsed.slices[1].slug, "docs-write");
    assert_eq!(parsed.slices[1].wave, Some(2), "wave 2 depends on wave 1");
    assert!(parsed.plan_error.is_none(), "plan block parses cleanly: {:?}", parsed.plan_error);
    assert_eq!(parsed.estimate.as_ref().and_then(|e| e.waves), Some(2));

    // The inner loop: real Serve, stub octos, one turn. The stub writes
    // the slice's source into the session's cwd.
    let inbox: Inbox = Arc::new(Mutex::new(Vec::new()));
    let serve = inner::Serve::spawn(&inbox, &data_dir).expect("stub octos starts (OCTOBUDDY_OCTOS_BIN points at it)");
    let peer = "w-feat-add";
    let key = format!("local:octobuddy:{peer}");
    let cwd = project_dir.to_string_lossy().into_owned();
    let brief = parsed.slices[0].brief.clone();
    let _turn_id = serve
        .open_with(peer, &key, &cwd, Some(&brief), serde_json::json!({}), false)
        .expect("session opens and the first turn starts");
    // The stub wrote `calc.py` while its `turn/start` handler ran.
    assert!(wait_for(&inbox, |e| matches!(e, LoopEvent::PeerTurnEnded { outcome, .. } if outcome == "completed"), Duration::from_secs(20)).is_some(),
        "the inner loop's turn ends");
    let calc = std::fs::read_to_string(project_dir.join("calc.py")).expect("the stub wrote calc.py");
    assert!(calc.contains("def add"), "the stub wrote the slice's source: {calc}");

    // The host runs the slice's check on the file the inner loop just
    // wrote.
    let outcome = verify::run(parsed.slices[0].check.as_deref().unwrap(), &cwd, Duration::from_secs(20));
    assert!(outcome.passed, "check passed: status={} tail={}", outcome.status, outcome.tail);
    // The host commits what the inner loop owns.
    let commit_msg = format!("feat({}): add feature\n\ndone", parsed.slices[0].slug);
    let committed = workspace::commit_files(&cwd, &["calc.py".into()], &commit_msg)
        .expect("the host commits the slice's files");
    assert!(committed.len() >= 7, "git returned a sha: {committed}");
    let log = workspace::git(&cwd, &["log", "--oneline"]).unwrap();
    assert!(log.contains(&committed), "the commit is in the log: {log}");

    // The outer loop's review block parses into the orchestration's verdict.
    let reviewed = plan::split_reply(REVIEW_WAVE_1);
    assert_eq!(reviewed.reviews.len(), 1);
    assert_eq!(reviewed.reviews[0].slug, "feat-add");
    assert_eq!(reviewed.reviews[0].verdict, "accept");

    // Wave gating (`orchestrate::wave_may_start`).
    let wave_1_accepted = peer_with("feat-add", 1, Some("accept: tests pass"), "idle");
    let wave_2 = peer_with("docs-write", 2, None, "queued");
    assert!(wave_may_start(&wave_2, &[wave_1_accepted.clone(), wave_2.clone()]),
        "wave 2 starts once wave 1 is accepted");
    let wave_1_stale = peer_with("feat-add", 1, Some("stale: docs changed README.md after wave 1 was accepted"), "idle");
    assert!(wave_may_start(&wave_2, &[wave_1_stale, wave_2.clone()]),
        "a stale accept does not hold the next wave");
    let wave_1_open = peer_with("feat-add", 1, Some("fix: add more tests"), "idle");
    assert!(!wave_may_start(&wave_2, &[wave_1_open, wave_2.clone()]),
        "wave 2 waits while wave 1 has a fix verdict");
    let wave_1_closed = peer_with("feat-add", 1, None, "closed");
    assert!(!wave_may_start(&wave_2, &[wave_1_closed, wave_2.clone()]),
        "a wave closed whole starts nothing after it");

    // `commit_message` (peer-named) and `auto_message` (host fallback).
    assert_eq!(commit_message("Done.\n\nCommit: feat(calc): 新增 add\n"),
        Some("feat(calc): 新增 add".to_string()),
        "the host uses the peer's Commit: line, conventional-commit-shaped");
    assert_eq!(commit_message("no commit here"), None);
    assert_eq!(auto_message("feat-add", &["calc.py".into()]), "chore(feat-add): update calc.py (check passed)");
    assert_eq!(auto_message("docs-write", &["README.md".into(), "calc.py".into()]),
        "chore(docs-write): update 2 files (check passed)");

    // Cleanup.
    drop(_env);
    drop(serve);
    let _ = std::fs::remove_dir_all(&workspace_root);
}