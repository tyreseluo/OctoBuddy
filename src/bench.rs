//! A repeatable benchmark of real inner loops against OctoBuddy's own
//! checks, ignored by default (`#[ignore]`); its writers are unit-tested,
//! the harness itself needs real agents and real provider keys.
//!
//! Where `e2e_tests.rs` drives a stub inner loop (no model is called), this
//! harness runs real ones — octos, Claude Code, Codex, pi — on the providers
//! OctoBuddy's AI providers has enabled, with OctoBuddy's real first prompt
//! (`orchestrate::first_prompt`) as the task, and then does exactly what the
//! host does with a turn: commits the loop's files through `receipts`, runs
//! the slice's check itself (`verify::run`), holds the report's claims
//! against those facts (`claims::facts`, `claims::reconcile`), and asks what
//! an accept would meet (`claims::gate`, `claims::refuse`).
//!
//! Run it (one line, from the repository root):
//!
//! ```text
//! OCTOBUDDY_BENCH_AGENTS="octos:MiniMax-M3,claude:minimax-cn/MiniMax-M3" \
//!   cargo test --locked bench_real_inner_loops -- --ignored --nocapture
//! ```
//!
//! * `OCTOBUDDY_BENCH_AGENTS`: one `agent:model` per combo (agent: octos,
//!   claude, codex, pi). A model is a provider label (`family/model`, as the
//!   AI providers page lists it) or one provider model's name
//!   (`MiniMax-M3`); for `claude` a bare Claude Code model name (`sonnet`,
//!   `opus`, `haiku`) runs on the person's own sign-in instead.
//! * `OCTOBUDDY_BENCH_REPS` (2), `OCTOBUDDY_BENCH_TASKS` (all of
//!   src/stubs/bench: calc-add, bounds-clamp, words-top),
//!   `OCTOBUDDY_BENCH_TIMEOUT_SECS` (600), `OCTOBUDDY_BENCH_MAX_COST` (1.00;
//!   octos only, whose costs stream in), `OCTOBUDDY_BENCH_OUT`
//!   (docs/evidence/bench), `OCTOBUDDY_BENCH_KEEP` (keep the scratch repos).
//!
//! Real data is never touched: the run's `OCTOBUDDY_HOME` (receipts, pi's
//! sessions, octos's serve directory) is a scratch folder, with a link to
//! the agents OctoBuddy already keeps. The providers are read from the
//! person's own AI providers (`OCTOBUDDY_PROVIDERS` points at them) exactly
//! as OctoBuddy reads them; on macOS the keys stay in the keychain and the
//! `security` tool reads them without a prompt (the items OctoBuddy wrote
//! allow it). When a keychain read would block a headless run, export the
//! provider's key env var (e.g. `MINIMAX_CN_API_KEY`) before running: octos
//! falls back to the process environment, and the claude/codex/pi routes
//! take it from there too. Keys are never printed and never written to the
//! bench's files.
//!
//! Output: `bench-<date>.json` (one record per run) and `bench-<date>.md`
//! (a summary per agent·model, what could not run and why, and a few
//! contradicted claims verbatim), written by `json_text` / `markdown`
//! below, which are pure and tested.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::claims::{self, Mark};
use crate::events::{take, Inbox, LoopEvent};
use crate::inner::Serve;
use crate::lead::{Lead, Mode, INNER_RULES};
use crate::orchestrate::{auto_message, commit_message, first_prompt};
use crate::{handoff, plan, providers, receipts, verify, workspace};

/// One task fixture (src/stubs/bench/<slug>/): the card, the check OctoBuddy
/// runs, and the repository's initial content.
#[derive(Clone, Debug, PartialEq)]
struct Task {
    slug: String,
    brief: String,
    check: String,
    /// (relative path, content) of the repository's files.
    files: Vec<(String, String)>,
}

/// One `agent:model` combo the run was asked for (`model` as asked).
#[derive(Clone, Debug, PartialEq)]
struct Combo {
    agent: String,
    model: String,
}

/// One run's record (bench-<date>.json).
#[derive(Clone, Debug, Default, PartialEq)]
struct Run {
    agent: String,
    /// The model as asked, and the provider label it resolved to.
    model: String,
    label: Option<String>,
    task: String,
    rep: u32,
    /// A real agent process ran (false: see `note`).
    real_agent_called: bool,
    /// Why it could not run, or what of it is worth knowing.
    note: Option<String>,
    /// How its turn ended (`completed`, `failed`, `timeout`, …).
    outcome: Option<String>,
    check_passed: Option<bool>,
    check_status: Option<String>,
    /// Its octobuddy-report, verbatim.
    report: Option<String>,
    holds: usize,
    contradicted: usize,
    unverifiable: usize,
    /// The `[claims]` lines OctoBuddy would give the outer loop.
    lines: Vec<String>,
    /// Files it changed outside its card's Files.
    stray: Vec<String>,
    /// Why an accept would be refused (None: it would not be).
    accept_refused: Option<String>,
    /// The commit the host made (`%h %s`), and its receipt's line.
    commit: Option<String>,
    receipt: Option<String>,
    seconds: f64,
    /// Its model-loop iterations (Claude Code: its messages), when counted.
    rounds: Option<u64>,
    cost: Option<f64>,
    /// The model the agent said it ran on.
    model_reported: Option<String>,
    /// Approvals the harness granted it (octos: tool calls).
    approvals: u64,
}

/// What the run was made with and as (the files' header).
#[derive(Clone, Debug, Default, PartialEq)]
struct Meta {
    date: String,
    commit: String,
    command: String,
    agents: String,
    tasks: String,
    reps: u32,
    timeout_secs: u64,
    max_cost: f64,
    host: String,
}

/// The tasks these env settings name (`OCTOBUDDY_BENCH_*`).
const TASKS_DIR: &str = "src/stubs/bench";

/// The bench sets process-wide variables (`OCTOBUDDY_HOME`,
/// `OCTOBUDDY_PROVIDERS`): one run of it at a time.
static BENCH: Mutex<()> = Mutex::new(());

/// `OCTOBUDDY_BENCH_AGENTS`: `agent:model,…` (the default: octos on the
/// primary provider's model, Claude Code through it too).
fn combos() -> Vec<Combo> {
    let text = std::env::var("OCTOBUDDY_BENCH_AGENTS").unwrap_or_else(|_| "octos:MiniMax-M3,claude:MiniMax-M3".into());
    text.split(',').filter_map(|c| {
        let c = c.trim();
        let (agent, model) = c.split_once(':')?;
        Some(Combo { agent: agent.trim().to_string(), model: model.trim().to_string() })
    }).collect()
}

/// The tasks `OCTOBUDDY_BENCH_TASKS` names (all of `TASKS_DIR` when unset).
fn tasks() -> Vec<Task> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(TASKS_DIR);
    let mut slugs: Vec<String> = std::fs::read_dir(&dir).into_iter().flatten().flatten()
        .filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    slugs.sort();
    if let Some(want) = std::env::var("OCTOBUDDY_BENCH_TASKS").ok().filter(|v| !v.trim().is_empty()) {
        let want: Vec<&str> = want.split(',').map(str::trim).collect();
        slugs.retain(|s| want.contains(&s.as_str()));
    }
    slugs.into_iter().filter_map(|slug| {
        let tdir = dir.join(&slug);
        let brief = std::fs::read_to_string(tdir.join("task.md")).ok()?;
        let check = std::fs::read_to_string(tdir.join("check.txt")).ok()?.trim().to_string();
        let mut files = Vec::new();
        collect_files(&tdir.join("files"), Path::new(""), &mut files);
        Some(Task { slug, brief, check, files })
    }).collect()
}

/// Every file under `dir`, as (its path relative to it, its content).
fn collect_files(dir: &Path, rel: &Path, out: &mut Vec<(String, String)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let rel = rel.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            collect_files(&path, &rel, out);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            out.push((rel.to_string_lossy().into_owned(), text));
        }
    }
}

/// The provider label `want` is (`family/model`, or one model's name, in the
/// AI providers' rows): `Ok(Some)` a route through OctoBuddy's proxy,
/// `Ok(None)` Claude Code's own sign-in (a bare model name that is no
/// provider's — only `claude` may take it).
fn resolve(agent: &str, rows: &[String], want: &str) -> Result<Option<String>, String> {
    if want.is_empty() {
        return match agent {
            "claude" => Ok(None),
            _ => Err("no model named (a provider's `family/model`, or a model's name)".into()),
        };
    }
    if want.contains('/') {
        return if rows.iter().any(|r| r == want) {
            Ok(Some(want.to_string()))
        } else {
            Err(format!("{want} is not one of the AI providers (configured: {})", rows.join(", ")))
        };
    }
    let found: Vec<&String> = rows.iter().filter(|r| r.rsplit('/').next().is_some_and(|m| m.eq_ignore_ascii_case(want))).collect();
    match found.len() {
        1 => Ok(Some(found[0].clone())),
        0 if agent == "claude" => Ok(None),
        0 => Err(format!("no provider's model is {want} (configured: {})", rows.join(", "))),
        _ => Err(format!("{want} names more than one provider's model: {}", found.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "))),
    }
}

/// A run's whole environment: the scratch root, the providers' labels, the
/// shared octos server and proxy (started lazily), restored on drop.
struct Rig {
    root: PathBuf,
    primary: Option<String>,
    rows: Vec<String>,
    inbox: Inbox,
    serve: Option<Serve>,
    proxy: Option<crate::claude_proxy::Proxy>,
    timeout: Duration,
    max_cost: f64,
    keep: bool,
    saved_home: Option<std::ffi::OsString>,
    saved_providers: Option<std::ffi::OsString>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        match &self.saved_home {
            Some(v) => std::env::set_var("OCTOBUDDY_HOME", v),
            None => std::env::remove_var("OCTOBUDDY_HOME"),
        }
        match &self.saved_providers {
            Some(v) => std::env::set_var("OCTOBUDDY_PROVIDERS", v),
            None => std::env::remove_var("OCTOBUDDY_PROVIDERS"),
        }
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

/// Sets up the scratch home and points OctoBuddy at the person's own AI
/// providers (read exactly as OctoBuddy reads them, keys included).
fn rig(timeout: Duration, max_cost: f64) -> Rig {
    let root = std::env::temp_dir().join(format!("octobuddy-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let saved_home = std::env::var_os("OCTOBUDDY_HOME");
    let saved_providers = std::env::var_os("OCTOBUDDY_PROVIDERS");
    // The person's own data and providers, before OCTOBUDDY_HOME moves.
    let real_data = crate::model::data_dir();
    if saved_providers.is_none() {
        std::env::set_var("OCTOBUDDY_PROVIDERS", real_data.join("providers"));
    }
    std::env::set_var("OCTOBUDDY_HOME", &home);
    // The agents OctoBuddy already keeps (pi's pinned npm install): theirs,
    // not downloaded again into the scratch.
    #[cfg(unix)]
    {
        let agents = real_data.join("agents");
        if agents.is_dir() {
            let _ = std::os::unix::fs::symlink(&agents, home.join("agents"));
        }
    }
    let read = providers::read();
    assert!(read.error.is_none(), "the AI providers could not be read: {:?}", read.error);
    assert!(!read.rows.is_empty(), "no AI providers are configured (Settings › AI Providers)");
    Rig {
        root,
        primary: read.primary().map(|r| r.label.clone()),
        rows: read.rows.iter().map(|r| r.label.clone()).collect(),
        inbox: Arc::new(Mutex::new(Vec::new())),
        serve: None,
        proxy: None,
        timeout,
        max_cost,
        keep: std::env::var_os("OCTOBUDDY_BENCH_KEEP").is_some(),
        saved_home,
        saved_providers,
    }
}

/// The shared octos server, (re)started, its profiles the providers'.
fn serve(rig: &mut Rig) -> Result<&Serve, String> {
    if rig.serve.is_none() {
        let dir = rig.root.join("home/octos/serve");
        let profile = providers::peer_profile(&providers::read());
        providers::serve_data_dir(&dir, &profile).map_err(|err| format!("could not prepare octos: {err}"))?;
        providers::model_profiles(&dir, &profile).map_err(|err| format!("could not write the model profiles: {err}"))?;
        rig.serve = Some(Serve::spawn(&rig.inbox, &dir)?);
    }
    Ok(rig.serve.as_ref().unwrap())
}

/// What a turn came back with.
#[derive(Default)]
struct TurnOut {
    outcome: String,
    text: String,
    rounds: u64,
    cost: Option<f64>,
    model_reported: Option<String>,
    approvals: u64,
    note: Option<String>,
    /// The octos server may be wedged (its turn never ended): start it again.
    wedged: bool,
}

/// One run on octos: a fresh session of the shared server, its profile the
/// model's; approvals granted (a headless run has no person to ask), a
/// question answered "use your judgment".
fn run_octos(rig: &mut Rig, label: &str, peer: &str, cwd: &str, first: &str) -> Result<TurnOut, String> {
    let base_profile = rig.primary.as_deref() == Some(label);
    let profile = if base_profile { "_main".into() } else { providers::model_profile_id(label) };
    let (inbox, timeout, max_cost) = (rig.inbox.clone(), rig.timeout, rig.max_cost);
    let serve = serve(rig)?;
    let key = format!("local:octobuddy:{peer}");
    let turn = serve.open_with(peer, &key, cwd, Some(first), serde_json::json!({"profile_id": profile}), false)?;
    let turn = turn.ok_or("no turn started")?;
    let mut out = TurnOut::default();
    let start = Instant::now();
    let mut interrupted = false;
    loop {
        if start.elapsed() >= timeout && !interrupted {
            interrupted = true;
            let _ = serve.interrupt(&key, &turn);
            out.note = Some(format!("interrupted at the {}s timeout", timeout.as_secs()));
        }
        if out.cost.is_some_and(|c| c >= max_cost) && !interrupted {
            interrupted = true;
            let _ = serve.interrupt(&key, &turn);
            out.note = Some(format!("stopped at the ${max_cost:.2} cost cap"));
        }
        for e in take(&inbox) {
            match e {
                LoopEvent::PeerTurnEnded { peer: p, outcome, text } if p == peer => {
                    out.outcome = if interrupted { "timeout".into() } else { outcome };
                    out.text = text;
                    return Ok(out);
                }
                LoopEvent::PeerRound { peer: p } if p == peer => out.rounds += 1,
                LoopEvent::PeerCost { peer: p, cost, .. } if p == peer => out.cost = (cost > 0.0).then_some(cost),
                LoopEvent::PeerInfo { peer: p, model: Some(model), .. } if p == peer => out.model_reported = Some(model),
                LoopEvent::PeerApproval { peer: p, approval_id, .. } if p == peer => {
                    out.approvals += 1;
                    let _ = serve.answer_approval(&key, &approval_id, true, Some("session"));
                }
                LoopEvent::PeerQuestion { peer: p, question_id, options, .. } if p == peer => {
                    let _ = serve.answer_question(&key, &question_id, &options, "Use your own judgment and finish the task.");
                }
                LoopEvent::ServeExited { error } => return Err(format!("octos serve exited: {error}")),
                _ => {}
            }
        }
        if start.elapsed() >= timeout + Duration::from_secs(60) {
            out.outcome = "timeout".into();
            out.wedged = true;
            return Ok(out);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// One run on a Lead-backed agent (Claude Code, Codex, pi): its events come
/// as a lead's, under `inner:<peer>`.
fn run_lead(rig: &mut Rig, lead: &Lead, session: &str, first: &str) -> Result<TurnOut, String> {
    lead.send(first)?;
    let mut out = TurnOut::default();
    let start = Instant::now();
    let mut interrupted = false;
    loop {
        if start.elapsed() >= rig.timeout && !interrupted {
            interrupted = true;
            let _ = lead.interrupt();
            out.note = Some(format!("interrupted at the {}s timeout", rig.timeout.as_secs()));
        }
        for e in take(&rig.inbox) {
            match e {
                LoopEvent::LeadTurnDone { session: s, ok, error, text, cost, .. } if s == session => {
                    out.outcome = match (interrupted, ok) {
                        (true, _) => "timeout".into(),
                        (_, true) => "completed".into(),
                        (_, false) => "failed".into(),
                    };
                    out.text = if ok { text } else { format!("{text}\n{}", error.unwrap_or_default()).trim().to_string() };
                    out.cost = cost;
                    return Ok(out);
                }
                LoopEvent::LeadExited { session: s, error, .. } if s == session => {
                    out.outcome = "failed".into();
                    out.text = error.unwrap_or_else(|| "the agent process exited".into());
                    return Ok(out);
                }
                // One assistant message of it: a step, as octos counts them.
                LoopEvent::LeadMessage { session: s, .. } if s == session => out.rounds += 1,
                LoopEvent::LeadInfo { session: s, model } if s == session => out.model_reported = Some(model),
                _ => {}
            }
        }
        if start.elapsed() >= rig.timeout + Duration::from_secs(30) {
            out.outcome = "timeout".into();
            return Ok(out);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// One run on Claude Code: on the provider `label` through OctoBuddy's
/// proxy, or on the person's own sign-in (`label` None) with `--model`.
fn run_claude(rig: &mut Rig, label: Option<&str>, model: &str, session: &str, cwd: &str, first: &str) -> Result<TurnOut, String> {
    let (env, resolved) = match label {
        Some(label) => {
            let route = route(label)?;
            let proxy = rig.proxy.get_or_insert_with(|| crate::claude_proxy::Proxy::start().expect("OctoBuddy's proxy starts"));
            (proxy.env_for(&route), Some(route.model.clone()))
        }
        None => (Vec::new(), (!model.is_empty()).then(|| model.to_string())),
    };
    let lead = Lead::spawn(&rig.inbox, session, cwd, None, "", resolved.as_deref(), None, &[], Mode::Inner, &env)?;
    run_lead(rig, &lead, session, first)
}

/// One run on Codex or pi, on the provider `label` through OctoBuddy's
/// proxy (Codex: the Responses bridge; pi: the Anthropic endpoint).
fn run_rpc(rig: &mut Rig, agent: &str, label: &str, session: &str, cwd: &str, first: &str) -> Result<TurnOut, String> {
    let route = route(label)?;
    let proxy = rig.proxy.get_or_insert_with(|| crate::claude_proxy::Proxy::start().expect("OctoBuddy's proxy starts"));
    let lead = if agent == "codex" {
        let base = proxy.responses_base(&route)?;
        let route = crate::rpc_lead::Route { model: route.model.clone(), base, effort: None };
        crate::rpc_lead::spawn_codex(&rig.inbox, session, cwd, None, INNER_RULES, Some(&route), &[], Mode::Inner)?
    } else {
        let route = crate::rpc_lead::Route { model: route.model.clone(), base: proxy.anthropic_base(&route), effort: None };
        crate::rpc_lead::spawn_pi(&rig.inbox, session, cwd, None, INNER_RULES, &route, Mode::Inner, None)?
    };
    run_lead(rig, &lead, session, first)
}

/// The route for `label`: the way OctoBuddy reads it (the key from the
/// profile or the keychain), or with the key env var the person exported —
/// read into memory only, never printed or written.
fn route(label: &str) -> Result<providers::ClaudeRoute, String> {
    match providers::claude_route(label) {
        Ok(route) => Ok(route),
        Err(err) => env_route(label).ok_or_else(|| format!("{err} (or export its key env var before running the bench: the keychain read needs no prompt for items OctoBuddy wrote)")),
    }
}

/// `label`'s route with the key from the process environment, when the
/// profile's own key could not be read (a keychain prompt a headless run
/// cannot answer). The key is never printed, never written.
fn env_route(label: &str) -> Option<providers::ClaudeRoute> {
    let core = std::env::var_os("OCTOBUDDY_PROVIDERS").map(PathBuf::from)?;
    let text = std::fs::read_to_string(octosense_llm_config::profile::profile_path(&core)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let llm = &v["config"]["llm"];
    let lanes = llm["fallbacks"].as_array().cloned().unwrap_or_default().into_iter().chain([llm["primary"].clone()]);
    let lane = lanes.into_iter().find(|l| format!("{}/{}", l["family_id"].as_str().unwrap_or(""), l["model_id"].as_str().unwrap_or("")) == label)?;
    let family = octosense_llm_config::registry::lookup(lane["family_id"].as_str().unwrap_or(""))?;
    let base = lane["route"]["base_url"].as_str().map(String::from)
        .or_else(|| family.default_base_url.map(String::from)).unwrap_or_default();
    let base_url = providers::anthropic_url(lane["family_id"].as_str().unwrap_or(""), &base)?;
    let env_name = lane["route"]["api_key_env"].as_str().map(String::from)
        .or_else(|| family.key_env.map(String::from)).unwrap_or_default();
    let names: Vec<String> = std::iter::once(env_name).chain(family.key_env_aliases.iter().map(|a| a.to_string())).collect();
    let key = names.iter().find_map(|n| std::env::var(n).ok().filter(|v| !v.is_empty()))?;
    let model = lane["model_id"].as_str().filter(|m| !m.is_empty()).map(String::from)
        .or_else(|| family.default_model.map(String::from)).unwrap_or_default();
    let chat_url = providers::chat_endpoint(lane["family_id"].as_str().unwrap_or(""), &base);
    Some(providers::ClaudeRoute { label: label.to_string(), base_url, model, key, chat_url })
}

/// A fresh repository of `task`'s files at `dir`, one initial commit; its HEAD.
fn init_repo(dir: &Path, task: &Task) -> String {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.to_string_lossy().into_owned();
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.email", "bench@example.invalid"],
        &["config", "user.name", "bench"][..],
        &["config", "commit.gpgsign", "false"][..],
    ] {
        workspace::git(&p, args).unwrap();
    }
    for (rel, text) in &task.files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, text).unwrap();
    }
    workspace::git(&p, &["add", "-A"]).unwrap();
    workspace::git(&p, &["commit", "-qm", "chore: init"]).unwrap();
    workspace::exclude_agent_files(&p);
    workspace::git(&p, &["rev-parse", "HEAD"]).unwrap()
}

/// One run of `combo` on `task`: the real inner loop, then what OctoBuddy
/// does with it (commit, check, claims, the gate).
fn one_run(rig: &mut Rig, combo: &Combo, task: &Task, rep: u32) -> Run {
    let mut run = Run { agent: combo.agent.clone(), model: combo.model.clone(), task: task.slug.clone(), rep, ..Default::default() };
    let started = Instant::now();
    // Its model: a provider's, or Claude Code's own.
    let label = match resolve(&combo.agent, &rig.rows, &combo.model) {
        Ok(label) => label,
        Err(err) => {
            run.note = Some(err);
            return run;
        }
    };
    run.label = label.clone();
    let peer = format!("bench-{}-{}-r{rep}", task.slug, combo.agent);
    let project = rig.root.join(&peer);
    let base = init_repo(&project, task);
    let cwd = project.to_string_lossy().into_owned();
    let first = first_prompt(&task.brief, &cwd, None, Some(&task.check), ".octobuddy/context/s1", "");
    run.real_agent_called = true;
    // The real inner loop, one turn of it, as OctoBuddy asks it.
    let turn = match combo.agent.as_str() {
        "octos" => run_octos(rig, label.as_deref().unwrap_or_default(), &peer, &cwd, &first),
        "claude" => run_claude(rig, label.as_deref(), &combo.model, &format!("inner:{peer}"), &cwd, &first),
        "codex" | "pi" => match label.as_deref() {
            Some(label) => run_rpc(rig, &combo.agent, label, &format!("inner:{peer}"), &cwd, &first),
            None => Err(format!("{} runs on one of the AI providers (a `family/model` label or a provider model's name)", combo.agent)),
        },
        other => Err(format!("{other} is not an agent the bench runs (octos, claude, codex, pi)")),
    };
    let turn = match turn {
        Ok(turn) => turn,
        Err(err) => {
            // The shared octos server may have died for it: start it again.
            if combo.agent == "octos" {
                rig.serve = None;
            }
            run.note = Some(err);
            run.seconds = started.elapsed().as_secs_f64();
            return run;
        }
    };
    if turn.wedged {
        rig.serve = None;
    }
    run.seconds = started.elapsed().as_secs_f64();
    run.outcome = Some(turn.outcome.clone());
    run.rounds = Some(turn.rounds);
    run.cost = turn.cost;
    run.model_reported = turn.model_reported;
    run.approvals = turn.approvals;
    run.note = turn.note;
    if turn.outcome != "completed" {
        // The last line says why (a failed Lead's text ends with its error).
        let why = turn.text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
        run.note = Some(format!("its turn ended {}: {}{}", turn.outcome, why,
            run.note.as_deref().map(|n| format!(" ({n})")).unwrap_or_default()));
        return run;
    }
    // The host's side, as `OctoBuddyView::finish` does it: its files (with
    // its `Commit:` line, else when its check passes), the check itself, the
    // report against those facts, and what an accept would meet.
    let (_, report) = plan::take_report(&turn.text);
    run.report = report.clone();
    if report.is_none() {
        run.note = Some("no octobuddy-report block in its reply".into());
    }
    let report = report.unwrap_or_default();
    let touched = workspace::uncommitted(&cwd);
    let card = handoff::card_files(&task.brief);
    run.stray = handoff::outside(&card, &touched);
    let mut made = match (commit_message(&turn.text), touched.is_empty()) {
        (Some(message), false) => receipts::commit(&cwd, &touched, &message, "OctoBuddy's bench").ok(),
        _ => None,
    };
    let outcome = verify::run(&task.check, &cwd, verify::TIMEOUT);
    run.check_passed = Some(outcome.passed);
    run.check_status = Some(outcome.status.clone());
    if made.is_none() && outcome.passed && !touched.is_empty() {
        made = receipts::commit(&cwd, &touched, &auto_message(&task.slug, &touched), "OctoBuddy's bench").ok();
    }
    run.commit = made.clone();
    let sha = made.as_deref().and_then(|m| m.split_whitespace().next()).map(String::from);
    run.receipt = sha.as_ref().and_then(|_| receipts::latest(&cwd, receipts::Kind::Commit))
        .filter(|r| r.result.as_deref() == made.as_deref()).map(|r| receipts::line(&r));
    let ran = (task.check.clone(), outcome.passed, outcome.status);
    let facts = claims::facts(&cwd, Some(&base), &report, sha.as_deref(), Some(ran.clone()), &touched);
    let found = claims::reconcile(&report, &facts);
    run.holds = found.iter().filter(|c| c.mark == Mark::Holds).count();
    run.contradicted = found.iter().filter(|c| c.mark == Mark::Contradicted).count();
    run.unverifiable = found.iter().filter(|c| c.mark == Mark::Unverifiable).count();
    run.lines = claims::lines(&found, &facts);
    let gate = claims::gate(Some(&found[..]), Some(&ran), Some(&run.stray[..]), Some(sha.as_deref().unwrap_or(&base)));
    let mut slice = crate::chat::tests::peer(Vec::new());
    slice.commits = sha.map(|s| vec![s]);
    slice.gate = Some(gate);
    run.accept_refused = claims::refuse(&slice);
    run
}

/// The bench: every combo × task × rep, then the two files.
#[test]
#[ignore]
fn bench_real_inner_loops() {
    let _one = BENCH.lock().unwrap_or_else(|e| e.into_inner());
    let combos = combos();
    let tasks = tasks();
    let reps = std::env::var("OCTOBUDDY_BENCH_REPS").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(2);
    let timeout_secs = std::env::var("OCTOBUDDY_BENCH_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(600);
    let max_cost = std::env::var("OCTOBUDDY_BENCH_MAX_COST").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let out_dir = std::env::var("OCTOBUDDY_BENCH_OUT").map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/evidence/bench"));
    assert!(!combos.is_empty(), "OCTOBUDDY_BENCH_AGENTS names no `agent:model` combo");
    assert!(!tasks.is_empty(), "no tasks under {TASKS_DIR} (OCTOBUDDY_BENCH_TASKS)");
    let agents_env = std::env::var("OCTOBUDDY_BENCH_AGENTS").unwrap_or_else(|_| "octos:MiniMax-M3,claude:MiniMax-M3 (the default)".into());
    let meta = Meta {
        date: date_of(crate::model::now_secs()),
        commit: octobuddy_commit(),
        command: format!("OCTOBUDDY_BENCH_AGENTS=\"{agents_env}\" OCTOBUDDY_BENCH_REPS={reps} cargo test --locked bench_real_inner_loops -- --ignored --nocapture"),
        agents: agents_env.clone(),
        tasks: tasks.iter().map(|t| t.slug.clone()).collect::<Vec<_>>().join(", "),
        reps,
        timeout_secs,
        max_cost,
        host: host(),
    };
    let mut rig = rig(Duration::from_secs(timeout_secs), max_cost);
    let mut runs = Vec::new();
    for combo in &combos {
        for task in &tasks {
            for rep in 1..=reps {
                let run = one_run(&mut rig, combo, task, rep);
                println!("bench {} · {} · {} r{rep}: ran={} check={:?} ✗={} refused={} {:.0}s cost={:?} {}",
                    run.agent, run.model_reported.as_deref().unwrap_or(&run.model), run.task, run.real_agent_called,
                    run.check_passed, run.contradicted, run.accept_refused.is_some(), run.seconds, run.cost,
                    run.note.as_deref().unwrap_or(""));
                runs.push(run);
            }
        }
    }
    // The files, beside each other (bench-<date>.{json,md}); a later run on
    // the same day gets a suffix, the earlier evidence stays.
    std::fs::create_dir_all(&out_dir).unwrap();
    let mut name = meta.date.clone();
    for n in 2.. {
        if !out_dir.join(format!("bench-{name}.json")).exists() {
            break;
        }
        name = format!("{}-{n}", meta.date);
    }
    std::fs::write(out_dir.join(format!("bench-{name}.json")), json_text(&meta, &runs)).unwrap();
    std::fs::write(out_dir.join(format!("bench-{name}.md")), markdown(&meta, &runs)).unwrap();
    println!("bench: wrote {}", out_dir.join(format!("bench-{name}.md")).display());
    drop(rig);
}

/// One record's JSON object.
fn run_json(run: &Run, commit: &str) -> serde_json::Value {
    serde_json::json!({
        "agent": run.agent, "model": run.model, "label": run.label, "task": run.task, "rep": run.rep,
        "real_agent_called": run.real_agent_called, "note": run.note, "outcome": run.outcome,
        "check": {"passed": run.check_passed, "status": run.check_status},
        "claims": {"holds": run.holds, "contradicted": run.contradicted, "unverifiable": run.unverifiable, "lines": run.lines},
        "stray": run.stray, "accept_refused": run.accept_refused,
        "commit": run.commit, "receipt": run.receipt,
        "seconds": (run.seconds * 10.0).round() / 10.0, "rounds": run.rounds, "cost": run.cost,
        "model_reported": run.model_reported, "approvals": run.approvals,
        "report": run.report, "octobuddy": commit,
    })
}

/// bench-<date>.json: the run's header and one record per run.
fn json_text(meta: &Meta, runs: &[Run]) -> String {
    let v = serde_json::json!({
        "bench": "real inner loops on real providers, then OctoBuddy's own commit, check, claims and accept gate",
        "date": meta.date, "octobuddy": meta.commit, "host": meta.host,
        "command": meta.command, "agents": meta.agents, "tasks": meta.tasks, "reps": meta.reps,
        "timeout_secs": meta.timeout_secs, "max_cost": meta.max_cost,
        "runs": runs.iter().map(|r| run_json(r, &meta.commit)).collect::<Vec<_>>(),
    });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// bench-<date>.md: the summary a person reads first.
fn markdown(meta: &Meta, runs: &[Run]) -> String {
    let mut out = format!(
        "# Real-agent benchmark — {}\n\n\
        {}\n\n\
        - Command: `{}`\n- OctoBuddy `{}` · {}\n- Tasks: {} · {} reps · timeout {}s · cost cap ${:.2}\n\
        - REAL: real agent processes on the person's own providers and keys ({}), a scratch OCTOBUDDY_HOME; keys never printed or written.\n\n",
        meta.date, "Real inner loops do three small Python tasks, then OctoBuddy does its side: the commit through receipts, the check itself, the report's claims, the accept gate.",
        meta.command, meta.commit, meta.host, meta.tasks, meta.reps, meta.timeout_secs, meta.max_cost,
        runs.iter().filter(|r| r.real_agent_called).map(|r| r.label.clone().unwrap_or_else(|| "Claude Code's own sign-in".into()))
            .collect::<std::collections::BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", "));
    // What could not run, and why (no key, the agent missing): said plainly.
    let skipped: Vec<&Run> = runs.iter().filter(|r| !r.real_agent_called).collect();
    if !skipped.is_empty() {
        out.push_str("## Could not run\n\n| combo | why |\n| --- | --- |\n");
        let mut seen = std::collections::BTreeSet::new();
        for r in skipped {
            if seen.insert((r.agent.clone(), r.model.clone())) {
                out.push_str(&format!("| {}:{} | {} |\n", r.agent, r.model, r.note.as_deref().unwrap_or("?")));
            }
        }
        out.push('\n');
    }
    // The summary, one row per agent·model that ran.
    let ran: Vec<&Run> = runs.iter().filter(|r| r.real_agent_called).collect();
    if !ran.is_empty() {
        out.push_str("## Summary\n\n| agent · model | runs | check passed | runs with ✗ | accept would refuse | mean time | mean cost |\n| --- | --- | --- | --- | --- | --- | --- |\n");
        let mut groups: Vec<(String, String)> = ran.iter().map(|r| (r.agent.clone(), r.model.clone())).collect();
        groups.sort();
        groups.dedup();
        for (agent, model) in groups {
            let rs: Vec<&&Run> = ran.iter().filter(|r| r.agent == agent && r.model == model).collect();
            let passed = rs.iter().filter(|r| r.check_passed == Some(true)).count();
            let wrong = rs.iter().filter(|r| r.contradicted > 0).count();
            let refused = rs.iter().filter(|r| r.accept_refused.is_some()).count();
            let secs = rs.iter().map(|r| r.seconds).sum::<f64>() / rs.len() as f64;
            let costs: Vec<f64> = rs.iter().filter_map(|r| r.cost).collect();
            let cost = if costs.is_empty() { "—".into() } else { format!("${:.3}", costs.iter().sum::<f64>() / costs.len() as f64) };
            out.push_str(&format!("| {agent} · {model} | {} | {}/{} | {} | {} | {:.0}s | {} |\n", rs.len(), passed, rs.len(), wrong, refused, secs, cost));
        }
        out.push('\n');
        // Every run, one line.
        out.push_str("## Runs\n\n| agent · model | task | rep | check | ✓/✗/? | accept refused | commit | time | cost | note |\n| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n");
        for r in runs.iter().filter(|r| r.real_agent_called) {
            let check = match r.check_passed {
                Some(true) => format!("passed ({})", r.check_status.as_deref().unwrap_or("")),
                Some(false) => format!("FAILED ({})", r.check_status.as_deref().unwrap_or("")),
                None => "—".into(),
            };
            let marks = format!("{}/{}/{}", r.holds, r.contradicted, r.unverifiable);
            let refused = r.accept_refused.as_ref().map(|w| w.chars().take(80).collect::<String>()).unwrap_or_else(|| "—".into());
            let commit = r.commit.as_deref().unwrap_or("—");
            let cost = r.cost.map(|c| format!("${c:.3}")).unwrap_or_else(|| "—".into());
            let note = r.note.as_deref().map(|n| cell(n, 100)).unwrap_or_else(|| "—".into());
            out.push_str(&format!("| {} · {} | {} | {} | {} | {} | {} | {} | {:.0}s | {} | {} |\n",
                r.agent, r.model_reported.as_deref().unwrap_or(&r.model), r.task, r.rep, check, marks, refused, commit, r.seconds, cost, note));
        }
        out.push('\n');
        // What the facts contradicted, verbatim: the evidence this bench is for.
        let wrong: Vec<&Run> = runs.iter().filter(|r| r.contradicted > 0).collect();
        if !wrong.is_empty() {
            out.push_str("## ✗ claims, verbatim\n\n");
            for r in wrong.iter().take(6) {
                for line in r.lines.iter().filter(|l| l.starts_with("[claims] ✗")) {
                    out.push_str(&format!("> {line}\n>\n> — {} · {}, {} rep {}\n\n", r.agent, r.model_reported.as_deref().unwrap_or(&r.model), r.task, r.rep));
                }
            }
        }
    }
    out
}

/// A table cell: one line, no pipes, `n` chars at most.
fn cell(text: &str, n: usize) -> String {
    let flat = text.replace('|', "\\|").replace('\n', " ");
    let flat = flat.trim();
    if flat.chars().count() <= n { flat.to_string() } else { format!("{}…", flat.chars().take(n).collect::<String>()) }
}

/// Today's date, UTC (`2026-10-12`).
fn date_of(secs: u64) -> String {    let days = (secs / 86_400) as i64;
    // The civil date of a day count (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// OctoBuddy's own commit, short, `+` when the worktree has changes.
fn octobuddy_commit() -> String {
    let dir = env!("CARGO_MANIFEST_DIR");
    let sha = workspace::git(dir, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|_| "?".into());
    let dirty = workspace::git(dir, &["status", "--porcelain"]).map(|s| !s.is_empty()).unwrap_or(false);
    format!("{sha}{}", if dirty { "+" } else { "" })
}

/// The machine this ran on (`macOS 26.0.1 arm64`).
fn host() -> String {
    let arch = std::env::consts::ARCH;
    if std::env::consts::OS == "macos" {
        if let Ok(out) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
            return format!("macOS {} {arch}", String::from_utf8_lossy(&out.stdout).trim());
        }
    }
    format!("{} {arch}", std::env::consts::OS)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run that passed its check with one claim contradicted, and one
    /// that could not run at all.
    fn sample() -> (Meta, Vec<Run>) {
        let meta = Meta {
            date: "2026-10-12".into(), commit: "3ba84c3".into(), command: "OCTOBUDDY_BENCH_AGENTS=\"octos:MiniMax-M3\" cargo test --locked bench_real_inner_loops -- --ignored --nocapture".into(),
            agents: "octos:MiniMax-M3".into(), tasks: "calc-add, bounds-clamp".into(), reps: 2, timeout_secs: 600, max_cost: 1.0, host: "macOS 26.0.1 arm64".into(),
        };
        let ran = Run {
            agent: "octos".into(), model: "MiniMax-M3".into(), label: Some("minimax-cn/MiniMax-M3".into()), task: "bounds-clamp".into(), rep: 1,
            real_agent_called: true, outcome: Some("completed".into()), check_passed: Some(false), check_status: Some("exit 1".into()),
            report: Some("status: done\nverified: `python3 -m unittest test_bounds` passes; all tests pass".into()),
            holds: 0, contradicted: 2, unverifiable: 1,
            lines: vec!["[claims] ✗ \"all tests pass\" — OctoBuddy's run of `python3 -c …` FAILED (exit 1)".into()],
            stray: vec![], accept_refused: Some("its check failed (exit 1); its report claims what OctoBuddy found otherwise: …".into()),
            commit: Some("a1b2c3d feat(bounds-clamp): add clamp".into()), receipt: Some("commit a1b2c3d ✓ confirmed".into()),
            seconds: 42.0, rounds: Some(7), cost: Some(0.012), model_reported: Some("MiniMax-M3".into()), approvals: 0, note: None,
        };
        let skipped = Run { agent: "pi".into(), model: "kimi-for-coding".into(), task: "calc-add".into(), rep: 1,
            note: Some("kimi-for-coding is not one of the AI providers (configured: minimax-cn/MiniMax-M3)".into()), ..Default::default() };
        let failed = Run { agent: "codex".into(), model: "glm-5.3".into(), label: Some("zai-coding/glm-5.3".into()), task: "calc-add".into(), rep: 1,
            real_agent_called: true, outcome: Some("failed".into()), seconds: 15.0,
            note: Some("its turn ended failed: exceeded retry limit, last status: 429 Too Many Requests".into()), ..Default::default() };
        (meta, vec![ran, skipped, failed])
    }

    #[test]
    fn the_json_holds_one_record_per_run() {
        let (meta, runs) = sample();
        let v: serde_json::Value = serde_json::from_str(&json_text(&meta, &runs)).unwrap();
        assert_eq!(v["octobuddy"], "3ba84c3");
        assert_eq!(v["runs"].as_array().unwrap().len(), 3);
        let ran = &v["runs"][0];
        assert_eq!(ran["real_agent_called"], true);
        assert_eq!(ran["check"]["passed"], false);
        assert_eq!(ran["claims"]["contradicted"], 2);
        assert!(ran["claims"]["lines"][0].as_str().unwrap().starts_with("[claims] ✗"), "{ran}");
        assert!(ran["accept_refused"].as_str().unwrap().contains("failed"));
        assert_eq!(ran["octobuddy"], "3ba84c3");
        let skipped = &v["runs"][1];
        assert_eq!(skipped["real_agent_called"], false);
        assert!(skipped["note"].as_str().unwrap().contains("not one of the AI providers"));
        assert!(!json_text(&meta, &runs).contains("::octobuddy"), "no keychain account name in the files");
    }

    #[test]
    fn the_markdown_sums_up_and_shows_what_was_contradicted() {
        let (meta, runs) = sample();
        let md = markdown(&meta, &runs);
        assert!(md.contains("REAL: real agent processes"), "{md}");
        assert!(md.contains("| octos · MiniMax-M3 | 1 | 0/1 | 1 | 1 | 42s | $0.012 |"), "{md}");
        assert!(md.contains("## Could not run"), "{md}");
        assert!(md.contains("| pi:kimi-for-coding | kimi-for-coding is not one of the AI providers"), "{md}");
        assert!(md.contains("## ✗ claims, verbatim"), "{md}");
        assert!(md.contains("> [claims] ✗ \"all tests pass\""), "{md}");
        assert!(md.contains("— octos · MiniMax-M3, bounds-clamp rep 1"), "{md}");
        assert!(md.contains("| codex · glm-5.3 | calc-add | 1 | — | 0/0/0 | — | — | 15s | — | its turn ended failed: exceeded retry limit, last status: 429 Too Many Requests |"), "{md}: a failed run says why in the table");
    }

    #[test]
    fn models_resolve_against_the_providers() {
        let rows = vec!["minimax-cn/MiniMax-M3".to_string(), "zai-coding/glm-5.3".to_string()];
        let r = |agent: &str, want: &str| resolve(agent, &rows, want);
        assert_eq!(r("octos", "minimax-cn/MiniMax-M3").unwrap(), Some("minimax-cn/MiniMax-M3".into()));
        assert_eq!(r("octos", "MiniMax-M3").unwrap(), Some("minimax-cn/MiniMax-M3".into()), "a bare model name resolves");
        assert_eq!(r("pi", "GLM-5.3").unwrap(), Some("zai-coding/glm-5.3".into()), "case does not matter");
        assert_eq!(r("claude", "sonnet").unwrap(), None, "a bare Claude Code model: the person's own sign-in");
        let err = r("octos", "kimi-for-coding").unwrap_err();
        assert!(err.contains("not one of the AI providers") || err.contains("no provider's model"), "{err}");
        assert!(r("pi", "minimax-cn/MiniMax-M2").unwrap_err().contains("not one of the AI providers"));
        assert!(r("pi", "").is_err());
        let two = vec!["a/same".to_string(), "b/same".to_string()];
        assert!(resolve("octos", &two, "same").unwrap_err().contains("more than one"));
    }

    #[test]
    fn the_fixtures_parse_into_tasks() {
        let tasks = tasks();
        assert!(tasks.len() >= 3, "{tasks:?}");
        let clamp = tasks.iter().find(|t| t.slug == "bounds-clamp").unwrap();
        assert!(clamp.brief.contains("## Done when") && clamp.brief.contains("ValueError"));
        assert!(clamp.check.contains("bounds.clamp(1, 10, 0)"));
        assert!(clamp.files.iter().any(|(p, _)| p == "bounds.py") && clamp.files.iter().any(|(p, _)| p == "test_bounds.py"));
        for t in &tasks {
            assert_eq!(handoff::card_files(&t.brief).len(), 1, "{}: one file on its card", t.slug);
            assert!(!t.check.is_empty() && !t.check.contains('\n'), "{}: the check is one line", t.slug);
        }
    }

    #[test]
    fn dates_come_out_civil() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(1_760_716_800), "2025-10-17");
        assert_eq!(date_of(1_791_763_200), "2026-10-12");
        assert_eq!(date_of(951_782_400), "2000-02-29");
        assert_eq!(date_of(4_102_444_800), "2100-01-01");
    }
}
