//! Codex and pi as OctoBuddy agents — outer loops and inner loops — beside
//! Claude Code. Each runs as one process per agent speaking JSON lines on
//! stdio, and its events are said as Claude Code's are (`Lead*`), so
//! everything after (the outer loop's turns, an inner loop's report, the
//! timeline, the cards) is the same.
//!
//! - **Codex**: `codex app-server`, JSON-RPC: `initialize`, `thread/start`
//!   (or `thread/resume`), then a `turn/start` per message; its
//!   notifications (`item/agentMessage/delta`, `item/started|completed`,
//!   `turn/completed`) become `Lead*` events. On a Chat Completions provider
//!   (GLM, …) it talks to OctoBuddy's proxy, which translates (`responses_bridge.rs`).
//! - **pi**: `pi --mode rpc`, JSON lines: `prompt`, `abort`,
//!   `switch_session`; events `agent_start`, `message_update`,
//!   `tool_execution_*`, `message_end`, `agent_settled`. Its provider is an
//!   `anthropic-messages` one in its own `models.json`, pointed at OctoBuddy's
//!   proxy with a placeholder key.
//!
//! Neither queues a message sent mid-turn the way Claude Code does: OctoBuddy
//! keeps it and starts it when the turn ends. The real key never reaches
//! either child.
use crate::events::{post, Inbox, LoopEvent};
use crate::lead::{Lead, Mode};
use crate::workspace::{find_bin, search_path};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

pub const CODEX: &str = "codex";
pub const PI: &str = "pi";

/// What a Codex or pi agent is doing, shared by its writer and its reader.
#[derive(Default)]
struct State {
    /// The thread (Codex) or session file (pi): set once it is ready.
    thread: Option<String>,
    /// The turn running now (Codex's turn id; pi: "pi").
    turn: Option<String>,
    queue: VecDeque<String>,
    next_id: u64,
    /// The turn's text so far, and its cost (pi).
    text: String,
    cost: f64,
    /// pi: the session file, once asked for.
    asked_state: bool,
    /// Codex: the new thread's parameters, if resuming the old one fails.
    fresh: Option<Value>,
    /// Codex: the resume asked for, and how often it was retried (the
    /// process it replaces may still hold the thread for a moment).
    resume: Option<(Value, u8)>,
    /// pi: the session file being resumed, and the model to go on with. pi
    /// runs its commands at once, so a prompt sent before the switch ends
    /// would be lost in it: ready only once both are answered.
    pi_resume: Option<(String, String)>,
    /// Messages steered into the running turn, by request id, until the
    /// agent says it took them (or turned them down: sent the usual way).
    steers: std::collections::HashMap<String, String>,
    /// User messages seen in the running turn: its prompt, then each one
    /// steered into it as the agent takes it up.
    users_in_turn: usize,
    /// It could not start (its handshake or thread failed): why. What is
    /// sent to it then fails at once, rather than waiting for nothing.
    failed: Option<String>,
}

pub struct Rpc {
    kind: &'static str,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    state: Arc<Mutex<State>>,
}

fn write_line(stdin: &Mutex<Option<ChildStdin>>, v: &Value) -> Result<(), String> {
    let mut guard = stdin.lock().map_err(|_| "input lock poisoned".to_string())?;
    let pipe = guard.as_mut().ok_or("the agent has exited")?;
    pipe.write_all(format!("{v}\n").as_bytes()).and_then(|_| pipe.flush()).map_err(|e| format!("could not write to the agent: {e}"))
}

impl Rpc {
    fn id(&self) -> u64 {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.next_id += 1;
        s.next_id + 100
    }

    /// A message: started now when idle and ready, else when the turn ends.
    pub fn send(&self, text: &str) -> Result<(), String> {
        let start = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(why) = s.failed.clone() {
                return Err(why);
            }
            if s.thread.is_some() && s.turn.is_none() {
                s.turn = Some(String::new());
                true
            } else {
                s.queue.push_back(text.to_string());
                false
            }
        };
        if start {
            self.start_turn(text)
        } else {
            Ok(())
        }
    }

    fn start_turn(&self, text: &str) -> Result<(), String> {
        match self.kind {
            CODEX => {
                let thread = self.state.lock().unwrap_or_else(|e| e.into_inner()).thread.clone().unwrap_or_default();
                write_line(&self.stdin, &json!({"id": self.id(), "method": "turn/start", "params": {"threadId": thread, "input": [{"type": "text", "text": text}]}}))
            }
            _ => write_line(&self.stdin, &json!({"type": "prompt", "id": format!("p{}", self.id()), "message": text})),
        }
    }

    /// A message into the running turn (Codex's `turn/steer`, pi's `steer`).
    pub fn steer(&self, text: &str) -> Result<(), String> {
        let (thread, turn) = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            (s.thread.clone(), s.turn.clone())
        };
        let no_turn = || "no turn running to steer".to_string();
        match self.kind {
            CODEX => {
                // Its turn's id names the turn meant: one that ended meanwhile turns it down.
                let (Some(thread), Some(turn)) = (thread, turn.filter(|t| !t.is_empty())) else { return Err(no_turn()) };
                let id = self.id();
                self.state.lock().unwrap_or_else(|e| e.into_inner()).steers.insert(id.to_string(), text.to_string());
                write_line(&self.stdin, &json!({"id": id, "method": "turn/steer", "params": {"threadId": thread, "input": [{"type": "text", "text": text}], "expectedTurnId": turn}}))
            }
            _ => {
                if turn.is_none() {
                    return Err(no_turn());
                }
                let id = format!("s{}", self.id());
                self.state.lock().unwrap_or_else(|e| e.into_inner()).steers.insert(id.clone(), text.to_string());
                write_line(&self.stdin, &json!({"type": "steer", "id": id, "message": text}))
            }
        }
    }

    /// The answer to a steered message, if `v` is one: turned down, it is
    /// sent the usual way (`LeadSteerRefused`).
    fn steer_answer(&self, v: &Value, session: &str, lead: &dyn Fn(LoopEvent)) -> bool {
        let Some(id) = v.get("id").map(|id| id.as_str().map(String::from).unwrap_or_else(|| id.to_string())) else { return false };
        let Some(text) = self.state.lock().unwrap_or_else(|e| e.into_inner()).steers.remove(&id) else { return false };
        let refused = v.get("error").is_some_and(|e| !e.is_null()) || v["success"] == false;
        if refused {
            lead(LoopEvent::LeadSteerRefused { session: session.to_string(), text });
        }
        true
    }

    /// A user message of the running turn seen: the first is its prompt;
    /// any after it, a steered one taken up.
    fn user_in_turn(&self, session: &str, lead: &dyn Fn(LoopEvent)) {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        s.users_in_turn += 1;
        if s.users_in_turn > 1 {
            drop(s);
            lead(LoopEvent::LeadSteerTaken { session: session.to_string() });
        }
    }

    pub fn interrupt(&self) -> Result<(), String> {
        let (thread, turn) = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            (s.thread.clone(), s.turn.clone())
        };
        match (self.kind, thread, turn) {
            (_, _, None) => Ok(()),
            (CODEX, Some(thread), Some(turn)) => write_line(&self.stdin, &json!({"id": self.id(), "method": "turn/interrupt", "params": {"threadId": thread, "turnId": turn}})),
            (CODEX, _, _) => Ok(()),
            _ => write_line(&self.stdin, &json!({"type": "abort", "id": format!("a{}", self.id())})),
        }
    }

    /// The turn ended: the next message waiting, if any, starts.
    fn next(&self) {
        let next = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.turn = None;
            s.text.clear();
            s.cost = 0.0;
            let next = s.queue.pop_front();
            if next.is_some() {
                s.turn = Some(String::new());
            }
            next
        };
        if let Some(text) = next {
            let _ = self.start_turn(&text);
        }
    }

    /// Ready (its thread or session known): what waited starts.
    fn ready(&self, thread: String) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).thread = Some(thread);
        let idle = self.state.lock().unwrap_or_else(|e| e.into_inner()).turn.is_none();
        if idle {
            self.next();
        }
    }
}

/// The MCP servers given as Claude Code's `--mcp-config` JSON, as Codex's
/// `-c mcp_servers.<name>.…` settings.
fn codex_mcp_args(mcp: &[(String, String)]) -> Vec<String> {
    let toml = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let mut out = Vec::new();
    for (_, config) in mcp {
        let Ok(v) = serde_json::from_str::<Value>(config) else { continue };
        for (name, server) in v["mcpServers"].as_object().into_iter().flatten() {
            if let Some(url) = server["url"].as_str() {
                out.push("-c".into());
                out.push(format!("mcp_servers.{name}.url={}", toml(url)));
            } else if let Some(cmd) = server["command"].as_str() {
                out.push("-c".into());
                out.push(format!("mcp_servers.{name}.command={}", toml(cmd)));
                let args: Vec<String> = server["args"].as_array().into_iter().flatten().filter_map(Value::as_str).map(toml).collect();
                out.push("-c".into());
                out.push(format!("mcp_servers.{name}.args=[{}]", args.join(", ")));
            }
            out.push("-c".into());
            out.push(format!("mcp_servers.{name}.tool_timeout_sec=900"));
        }
    }
    out
}

/// A tool call as the cards and timeline name it.
fn tool_name(kind: &str, raw: &str) -> String {
    match (kind, raw) {
        (_, "bash") | ("commandExecution", _) => "Bash".into(),
        (_, "read") => "Read".into(),
        (_, "edit") | ("fileChange", _) => "Edit".into(),
        (_, "write") => "Write".into(),
        (_, "grep") => "Grep".into(),
        (_, "find") | (_, "ls") => "Glob".into(),
        _ => raw.to_string(),
    }
}

fn first_text(v: &Value) -> String {
    let s = match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    s.lines().next().unwrap_or("").chars().take(160).collect()
}

/// Whether a provider's error is its rate limit (an HTTP 429, Z.ai's 1302).
fn rate_limited(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("429") || t.contains("too many") || t.contains("rate limit") || t.contains("1302")
}

/// A retry of the model call, said plainly: why (the provider's rate limit,
/// a dropped connection) and how far along (`2/5`).
fn retry_said(rate: bool, step: &str) -> String {
    match (rate, step.is_empty()) {
        (true, false) => crate::i18n::pick(format!("the model's provider is rate-limiting: retrying {step}"), format!("模型限流（429），正在重试 {step}")),
        (true, true) => crate::i18n::t("the model's provider is rate-limiting: retrying", "模型限流（429），正在重试").into(),
        (false, false) => crate::i18n::pick(format!("connection dropped: retrying {step}"), format!("连接断开，正在重试 {step}")),
        (false, true) => crate::i18n::t("connection dropped: retrying", "连接断开，正在重试").into(),
    }
}

/// Codex retrying its model call (its `error` notice with `willRetry`).
fn retry_status(p: &Value) -> String {
    let msg = p.pointer("/error/message").and_then(Value::as_str).unwrap_or("");
    let details = format!("{} {}", p.pointer("/error/additionalDetails").and_then(Value::as_str).unwrap_or(""), p.pointer("/error/codexErrorInfo").cloned().unwrap_or(Value::Null));
    let step = msg.rsplit(' ').next().filter(|s| s.contains('/')).unwrap_or("").to_string();
    retry_said(rate_limited(&details), &step)
}

/// The reader: the child's lines, said as `Lead*` events; its exit, `LeadExited`.
#[allow(clippy::too_many_arguments)]
fn read(kind: &'static str, rpc: Arc<Rpc>, inbox: Inbox, session: String, mode: Mode, out: std::process::ChildStdout, err: Option<std::process::ChildStderr>,
        child: Arc<Mutex<Option<std::process::Child>>>, model: String, stopped: Arc<std::sync::atomic::AtomicBool>, gen: u64) {
    std::thread::spawn(move || {
        let stderr_text = std::thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut pipe) = err {
                let _ = pipe.read_to_string(&mut text);
            }
            text
        });
        let lead = |e: LoopEvent| post(&inbox, e);
        let mut reader = BufReader::new(out);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            // Split on \n only (pi's JSON may hold U+2028).
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            let Ok(v) = serde_json::from_slice::<Value>(&buf) else { continue };
            if kind == CODEX {
                codex_event(&rpc, &v, &session, mode, &model, &lead);
            } else {
                pi_event(&rpc, &v, &session, &lead);
            }
        }
        let code = child.lock().ok().and_then(|mut c| c.as_mut().and_then(|c| c.wait().ok()));
        let err = stderr_text.join().unwrap_or_default();
        // Ended by OctoBuddy (another engine, the loop closed): no failure.
        let ended = stopped.load(std::sync::atomic::Ordering::SeqCst);
        let error = (!ended && !code.is_some_and(|c| c.success())).then(|| {
            let tail: String = err.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n");
            format!("{kind} exited{}", if tail.trim().is_empty() { String::new() } else { format!(": {tail}") })
        });
        lead(LoopEvent::LeadExited { session, error, gen });
    });
}

fn codex_event(rpc: &Rpc, v: &Value, session: &str, mode: Mode, model: &str, lead: &dyn Fn(LoopEvent)) {
    let s = || session.to_string();
    if v.get("method").is_none() && rpc.steer_answer(v, session, lead) {
        return;
    }
    // A request of the server's (an approval): an inner loop's sandboxed
    // work goes ahead; the outer loop changes nothing.
    if let (Some(id), Some(method)) = (v.get("id"), v.get("method").and_then(Value::as_str)) {
        let decision = if mode == Mode::Inner && method.contains("requestApproval") { "accept" } else { "decline" };
        let _ = write_line(&rpc.stdin, &json!({"id": id, "result": {"decision": decision}}));
        return;
    }
    // Answers: the thread started, or a turn did.
    if let Some(result) = v.get("result") {
        if let Some(thread) = result.pointer("/thread/id").and_then(Value::as_str) {
            lead(LoopEvent::LeadInfo { session: s(), model: model.to_string() });
            rpc.ready(thread.to_string());
        }
        if let Some(turn) = result.pointer("/turn/id").and_then(Value::as_str) {
            rpc.state.lock().unwrap_or_else(|e| e.into_inner()).turn = Some(turn.to_string());
        }
        return;
    }
    if let Some(error) = v.get("error").filter(|_| v.get("id").is_some()) {
        let msg = error.get("message").and_then(Value::as_str).unwrap_or("error").to_string();
        let ready = rpc.state.lock().unwrap_or_else(|e| e.into_inner()).thread.is_some();
        if !ready {
            // The process it replaces still holds the thread: again shortly.
            let again = {
                let mut st = rpc.state.lock().unwrap_or_else(|e| e.into_inner());
                match st.resume.as_mut() {
                    Some((params, tries)) if msg.contains("active writer") && *tries < 3 => {
                        *tries += 1;
                        Some((params.clone(), *tries))
                    }
                    _ => None,
                }
            };
            if let Some((params, tries)) = again {
                std::thread::sleep(std::time::Duration::from_millis(400 * tries as u64));
                let _ = write_line(&rpc.stdin, &json!({"id": 2, "method": "thread/resume", "params": params}));
                return;
            }
            // Its old conversation could not be resumed: a new one.
            let fresh = rpc.state.lock().unwrap_or_else(|e| e.into_inner()).fresh.take();
            if let Some(params) = fresh {
                lead(LoopEvent::LeadStatus { session: s(), status: "its earlier conversation could not be resumed: a new one".into() });
                let _ = write_line(&rpc.stdin, &json!({"id": 3, "method": "thread/start", "params": params}));
                return;
            }
            // It cannot start: what waited fails, and what comes after too.
            let why = format!("codex could not start: {msg}");
            let waiting: Vec<String> = {
                let mut st = rpc.state.lock().unwrap_or_else(|e| e.into_inner());
                st.failed = Some(why.clone());
                st.queue.drain(..).collect()
            };
            for _ in 0..waiting.len().max(1) {
                lead(LoopEvent::LeadTurnDone { session: s(), ok: false, error: Some(why.clone()), lead_session: None, text: String::new(), cost: None });
            }
            return;
        }
        let busy = rpc.state.lock().unwrap_or_else(|e| e.into_inner()).turn.is_some();
        if busy {
            let thread = rpc.state.lock().unwrap_or_else(|e| e.into_inner()).thread.clone();
            lead(LoopEvent::LeadTurnDone { session: s(), ok: false, error: Some(msg), lead_session: thread, text: String::new(), cost: None });
            rpc.next();
        }
        return;
    }
    let p = &v["params"];
    match v.get("method").and_then(Value::as_str).unwrap_or("") {
        "turn/started" => {
            rpc.state.lock().unwrap_or_else(|e| e.into_inner()).users_in_turn = 0;
            if let Some(turn) = p.pointer("/turn/id").and_then(Value::as_str) {
                rpc.state.lock().unwrap_or_else(|e| e.into_inner()).turn = Some(turn.to_string());
            }
            lead(LoopEvent::LeadStatus { session: s(), status: "thinking".into() });
        }
        "item/agentMessage/delta" => {
            let delta = p["delta"].as_str().unwrap_or("").to_string();
            rpc.state.lock().unwrap_or_else(|e| e.into_inner()).text.push_str(&delta);
            lead(LoopEvent::LeadDelta { session: s(), text: delta });
        }
        "item/started" => {
            let item = &p["item"];
            let (id, kind) = (item["id"].as_str().unwrap_or("").to_string(), item["type"].as_str().unwrap_or(""));
            if kind == "userMessage" {
                rpc.user_in_turn(session, lead);
                return;
            }
            let (name, detail) = match kind {
                "commandExecution" => ("Bash".to_string(), first_text(&item["command"])),
                "fileChange" => ("Edit".to_string(), item["changes"].as_array().into_iter().flatten().filter_map(|c| c["path"].as_str()).collect::<Vec<_>>().join(", ")),
                "mcpToolCall" => (format!("mcp__{}__{}", item["server"].as_str().unwrap_or(""), item["tool"].as_str().unwrap_or("")), first_text(&item["arguments"])),
                "webSearch" => ("WebSearch".to_string(), first_text(&item["query"])),
                _ => return,
            };
            lead(LoopEvent::LeadStatus { session: s(), status: crate::stream::tool_label(&name).to_string() });
            lead(LoopEvent::LeadTool { session: s(), id, name, detail });
        }
        "item/completed" => {
            let item = &p["item"];
            let id = item["id"].as_str().unwrap_or("").to_string();
            match item["type"].as_str().unwrap_or("") {
                "agentMessage" => lead(LoopEvent::LeadMessage { session: s(), text: item["text"].as_str().unwrap_or("").to_string() }),
                "commandExecution" => lead(LoopEvent::LeadToolEnd { session: s(), id, ok: item["exitCode"].as_i64().is_none_or(|c| c == 0) && item["status"] != "failed" }),
                "fileChange" | "mcpToolCall" | "webSearch" => lead(LoopEvent::LeadToolEnd { session: s(), id, ok: item["status"] != "failed" && item["error"].is_null() }),
                _ => {}
            }
        }
        "turn/completed" => {
            let turn = &p["turn"];
            let status = turn["status"].as_str().unwrap_or("completed");
            let (text, thread) = {
                let st = rpc.state.lock().unwrap_or_else(|e| e.into_inner());
                (st.text.clone(), st.thread.clone())
            };
            let error = match status {
                "completed" => None,
                "interrupted" => Some("interrupted".to_string()),
                _ => Some(turn.pointer("/error/message").and_then(Value::as_str).unwrap_or("the turn failed").to_string()),
            };
            lead(LoopEvent::LeadTurnDone { session: s(), ok: error.is_none(), error, lead_session: thread, text, cost: None });
            rpc.next();
        }
        "error" if p["willRetry"].as_bool() == Some(true) || p.pointer("/error/message").and_then(Value::as_str).is_some_and(|m| m.starts_with("Reconnecting")) => {
            lead(LoopEvent::LeadStatus { session: s(), status: retry_status(p) });
        }
        _ => {}
    }
}

fn pi_event(rpc: &Rpc, v: &Value, session: &str, lead: &dyn Fn(LoopEvent)) {
    let s = || session.to_string();
    match v["type"].as_str().unwrap_or("") {
        "response" => {
            if v["command"] == "steer" {
                rpc.steer_answer(v, session, lead);
                return;
            }
            // Resuming: the session switched, then the model set, then ready.
            let resuming = rpc.state.lock().unwrap_or_else(|e| e.into_inner()).pi_resume.clone();
            if let Some((file, model)) = resuming {
                if v["id"] == "resume" {
                    if v["success"] == true && v.pointer("/data/cancelled") != Some(&Value::Bool(true)) {
                        let _ = write_line(&rpc.stdin, &json!({"type": "set_model", "id": "model", "provider": "octobuddy", "modelId": model}));
                    } else {
                        rpc.state.lock().unwrap_or_else(|e| e.into_inner()).pi_resume = None;
                        lead(LoopEvent::LeadStatus { session: s(), status: "its earlier conversation could not be resumed: a new one".into() });
                        let _ = write_line(&rpc.stdin, &json!({"type": "get_state", "id": "state"}));
                        rpc.ready(String::new());
                    }
                    return;
                }
                if v["id"] == "model" {
                    rpc.state.lock().unwrap_or_else(|e| e.into_inner()).pi_resume = None;
                    rpc.ready(file);
                    return;
                }
            }
            // get_state: the session file, to resume it later.
            if v["command"] == "get_state" {
                if let Some(file) = v.pointer("/data/sessionFile").or_else(|| v.pointer("/data/sessionId")).and_then(Value::as_str) {
                    rpc.state.lock().unwrap_or_else(|e| e.into_inner()).thread = Some(file.to_string());
                }
            }
            if v["success"] == false && rpc.state.lock().unwrap_or_else(|e| e.into_inner()).turn.is_some() && v["command"] == "prompt" {
                let error = v["error"].as_str().unwrap_or("pi refused the message").to_string();
                lead(LoopEvent::LeadTurnDone { session: s(), ok: false, error: Some(error), lead_session: None, text: String::new(), cost: None });
                rpc.next();
            }
        }
        "agent_start" => {
            rpc.state.lock().unwrap_or_else(|e| e.into_inner()).users_in_turn = 0;
            lead(LoopEvent::LeadStatus { session: s(), status: "thinking".into() });
        }
        // pi retries a failed call itself (a provider's rate limit, say): said.
        "auto_retry_start" => {
            let step = format!("{}/{}", v["attempt"].as_u64().unwrap_or(1), v["maxAttempts"].as_u64().unwrap_or(3));
            lead(LoopEvent::LeadStatus { session: s(), status: retry_said(rate_limited(v["errorMessage"].as_str().unwrap_or("")), &step) });
        }
        "message_update" => {
            let e = &v["assistantMessageEvent"];
            if e["type"] == "text_delta" {
                let delta = e["delta"].as_str().unwrap_or("").to_string();
                rpc.state.lock().unwrap_or_else(|e| e.into_inner()).text.push_str(&delta);
                lead(LoopEvent::LeadDelta { session: s(), text: delta });
            }
        }
        "tool_execution_start" => {
            let name = tool_name("", v["toolName"].as_str().unwrap_or(""));
            let args = &v["args"];
            let detail = first_text(args.get("command").or(args.get("path")).or(args.get("pattern")).unwrap_or(args));
            lead(LoopEvent::LeadStatus { session: s(), status: crate::stream::tool_label(&name).to_string() });
            lead(LoopEvent::LeadTool { session: s(), id: v["toolCallId"].as_str().unwrap_or("").to_string(), name, detail });
        }
        "tool_execution_end" => lead(LoopEvent::LeadToolEnd { session: s(), id: v["toolCallId"].as_str().unwrap_or("").to_string(), ok: v["isError"] != true }),
        "message_end" => {
            let m = &v["message"];
            if m["role"] == "user" {
                rpc.user_in_turn(session, lead);
            }
            if m["role"] == "assistant" {
                let text: String = m["content"].as_array().into_iter().flatten().filter(|c| c["type"] == "text").filter_map(|c| c["text"].as_str()).collect();
                if let Some(cost) = m.pointer("/usage/cost/total").and_then(Value::as_f64) {
                    rpc.state.lock().unwrap_or_else(|e| e.into_inner()).cost += cost;
                }
                if let Some(model) = m["model"].as_str() {
                    lead(LoopEvent::LeadInfo { session: s(), model: model.to_string() });
                }
                lead(LoopEvent::LeadMessage { session: s(), text });
                if let Some(err) = m["errorMessage"].as_str().filter(|e| !e.is_empty()) {
                    rpc.state.lock().unwrap_or_else(|e| e.into_inner()).text.push_str(&format!("\n\n{err}"));
                }
            }
        }
        "agent_settled" => {
            let (text, cost, thread, ask) = {
                let mut st = rpc.state.lock().unwrap_or_else(|e| e.into_inner());
                let ask = !st.asked_state;
                st.asked_state = true;
                (st.text.clone(), st.cost, st.thread.clone(), ask)
            };
            let aborted = v["stopReason"] == "aborted" || v.pointer("/messages").is_some_and(|_| false);
            let error = aborted.then(|| "interrupted".to_string());
            let thread = thread.filter(|t| !t.is_empty());
            lead(LoopEvent::LeadTurnDone { session: s(), ok: error.is_none(), error, lead_session: thread, text, cost: (cost > 0.0).then_some(cost) });
            if ask {
                let _ = write_line(&rpc.stdin, &json!({"type": "get_state", "id": "state"}));
            }
            rpc.next();
        }
        _ => {}
    }
}

/// The provider an RPC agent runs on: a model through OctoBuddy's proxy.
pub struct Route {
    pub model: String,
    /// Codex: its Responses base (the bridge). pi: its Anthropic base.
    pub base: String,
    /// The reasoning effort picked (`model::effort_levels`; none: its own).
    pub effort: Option<String>,
}

/// Starts Codex for `session` in `cwd`, resuming thread `resume` when given.
#[allow(clippy::too_many_arguments)]
pub fn spawn_codex(inbox: &Inbox, session: &str, cwd: &str, resume: Option<&str>, rules: &str, route: Option<&Route>, mcp: &[(String, String)], mode: Mode) -> Result<Lead, String> {
    let mut cmd = Command::new(find_bin("codex"));
    cmd.arg("app-server");
    if let Some(r) = route {
        for c in ["model_provider=\"octobuddy\"".to_string(), "model_providers.octobuddy.name=\"OctoBuddy\"".into(),
            format!("model_providers.octobuddy.base_url=\"{}\"", r.base), "model_providers.octobuddy.wire_api=\"responses\"".into(),
            "model_providers.octobuddy.env_key=\"OCTOBUDDY_CODEX_KEY\"".into(), "model_providers.octobuddy.supports_websockets=false".into(),
            "features.enable_request_compression=false".into()] {
            cmd.arg("-c").arg(c);
        }
        // A placeholder: the proxy holds the real key.
        cmd.env("OCTOBUDDY_CODEX_KEY", crate::claude_proxy::PLACEHOLDER_KEY);
    }
    if let Some(effort) = route.and_then(|r| r.effort.as_deref()) {
        cmd.arg("-c").arg(format!("model_reasoning_effort=\"{effort}\""));
    }
    cmd.args(codex_mcp_args(mcp));
    if mode != Mode::Outer {
        cmd.arg("-c").arg("sandbox_workspace_write.network_access=true");
    }
    for name in crate::claude_proxy::SCRUB {
        cmd.env_remove(name);
    }
    cmd.env_remove("OPENAI_API_KEY");
    cmd.current_dir(cwd).env("PATH", search_path()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::lead::own_group(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("could not start codex: {e}"))?;
    let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take().unwrap(), child.stderr.take());
    let child = Arc::new(Mutex::new(Some(child)));
    let rpc = Arc::new(Rpc { kind: CODEX, stdin: Arc::new(Mutex::new(stdin)), state: Arc::default() });
    let model = route.map(|r| r.model.clone()).unwrap_or_default();
    let stopped: Arc<std::sync::atomic::AtomicBool> = Arc::default();
    let gen = crate::lead::next_gen();
    read(CODEX, rpc.clone(), inbox.clone(), session.to_string(), mode, stdout, stderr, child.clone(), model.clone(), stopped.clone(), gen);
    // The handshake, then its thread: new, or the one it had.
    write_line(&rpc.stdin, &json!({"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "octobuddy", "version": env!("CARGO_PKG_VERSION")}, "capabilities": {"experimentalApi": true}}}))?;
    write_line(&rpc.stdin, &json!({"method": "initialized"}))?;
    // An inner loop writes its folder; a plain chat its own (empty, OctoBuddy's)
    // folder; both may reach the network. The outer loop only reads.
    let sandbox = if mode == Mode::Outer { "read-only" } else { "workspace-write" };
    let mut params = json!({"cwd": cwd, "approvalPolicy": "never", "sandbox": sandbox, "developerInstructions": rules});
    if !model.is_empty() {
        params["model"] = model.into();
    }
    match resume {
        Some(thread) => {
            // A new thread if this one cannot be resumed (another provider, gone).
            rpc.state.lock().unwrap_or_else(|e| e.into_inner()).fresh = Some(params.clone());
            params["threadId"] = thread.into();
            rpc.state.lock().unwrap_or_else(|e| e.into_inner()).resume = Some((params.clone(), 0));
            write_line(&rpc.stdin, &json!({"id": 2, "method": "thread/resume", "params": params}))?;
        }
        None => write_line(&rpc.stdin, &json!({"id": 2, "method": "thread/start", "params": params}))?,
    }
    Ok(Lead::rpc(rpc, child, stopped, gen))
}

/// Starts pi for `session` in `cwd` on `route`, resuming its session file `resume`.
#[allow(clippy::too_many_arguments)]
pub fn spawn_pi(inbox: &Inbox, session: &str, cwd: &str, resume: Option<&str>, rules: &str, route: &Route, mode: Mode) -> Result<Lead, String> {
    // Its own agent directory: the one provider, pointed at OctoBuddy's proxy.
    let safe: String = session.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let home = crate::model::data_dir().join("pi").join(&safe);
    std::fs::create_dir_all(home.join("sessions")).map_err(|e| e.to_string())?;
    let models = json!({"providers": {"octobuddy": {
        "baseUrl": route.base, "api": "anthropic-messages", "apiKey": crate::claude_proxy::PLACEHOLDER_KEY,
        "models": [{"id": route.model, "name": route.model, "reasoning": true, "input": ["text"], "contextWindow": 200000, "maxTokens": 32000,
            "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}}]
    }}});
    std::fs::write(home.join("models.json"), serde_json::to_string_pretty(&models).unwrap_or_default()).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(find_bin("pi"));
    cmd.args(["--mode", "rpc", "--session-dir"]).arg(home.join("sessions"))
        .args(["--provider", "octobuddy", "--model", &route.model]).arg("--append-system-prompt").arg(rules);
    if let Some(effort) = route.effort.as_deref() {
        cmd.arg("--thinking").arg(effort);
    }
    if mode != Mode::Inner {
        // The outer loop reads; it changes nothing.
        cmd.args(["--tools", "read,grep,find,ls"]);
    }
    for name in crate::claude_proxy::SCRUB {
        cmd.env_remove(name);
    }
    cmd.env("PI_CODING_AGENT_DIR", &home).env("PI_OFFLINE", "1").current_dir(cwd).env("PATH", search_path())
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    crate::lead::own_group(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("could not start pi: {e}"))?;
    let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take().unwrap(), child.stderr.take());
    let child = Arc::new(Mutex::new(Some(child)));
    let rpc = Arc::new(Rpc { kind: PI, stdin: Arc::new(Mutex::new(stdin)), state: Arc::default() });
    let stopped: Arc<std::sync::atomic::AtomicBool> = Arc::default();
    let gen = crate::lead::next_gen();
    read(PI, rpc.clone(), inbox.clone(), session.to_string(), mode, stdout, stderr, child.clone(), route.model.clone(), stopped.clone(), gen);
    post(inbox, LoopEvent::LeadInfo { session: session.to_string(), model: route.model.clone() });
    rpc.state.lock().unwrap_or_else(|e| e.into_inner()).asked_state = true;
    match resume.filter(|f| std::path::Path::new(f).is_file()) {
        // The conversation kept, on the model picked now (not the one it had):
        // messages wait until both are done (`pi_event`).
        Some(file) => {
            rpc.state.lock().unwrap_or_else(|e| e.into_inner()).pi_resume = Some((file.to_string(), route.model.clone()));
            write_line(&rpc.stdin, &json!({"type": "switch_session", "id": "resume", "sessionPath": file}))?;
        }
        // A new one: its session file, before the first answer (what resumes
        // it later); prompts go at once.
        None => {
            write_line(&rpc.stdin, &json!({"type": "get_state", "id": "state"}))?;
            rpc.ready(String::new());
        }
    }
    Ok(Lead::rpc(rpc, child, stopped, gen))
}

impl crate::OctoBuddyView {
    /// Starts Codex or pi (`engine`) for `session`: on the provider `label`
    /// (`family/model`, through OctoBuddy's proxy), or Codex's own setting.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_rpc(&mut self, engine: &str, session: &str, cwd: &str, resume: Option<&str>, rules: &str, label: Option<&str>, effort: Option<&str>, mcp: &[(String, String)], mode: Mode) -> Result<Lead, String> {
        let route = match label.filter(|l| l.contains('/')) {
            Some(label) => {
                let r = crate::providers::claude_route(label)?;
                if self.rt.claude_proxy.is_none() {
                    self.rt.claude_proxy = crate::claude_proxy::Proxy::start();
                }
                let proxy = self.rt.claude_proxy.as_ref().ok_or("OctoBuddy's proxy could not start")?;
                let base = if engine == CODEX { proxy.responses_base(&r)? } else { proxy.anthropic_base(&r) };
                Some(Route { model: r.model.clone(), base, effort: effort.map(String::from) })
            }
            None => None,
        };
        match engine {
            CODEX => spawn_codex(&self.rt.inbox, session, cwd, resume, rules, route.as_ref(), mcp, mode),
            _ => spawn_pi(&self.rt.inbox, session, cwd, resume, rules, route.as_ref().ok_or("pi runs on one of your AI providers: pick “pi · <provider>”")?, mode),
        }
    }
}

/// Whether `engine` runs as a Lead (Claude Code, Codex, pi), not on octos.
pub fn lead_engine(engine: &str) -> bool {
    matches!(engine, "claude" | CODEX | PI)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_servers_become_codex_settings() {
        let args = codex_mcp_args(&[
            ("octobuddy".into(), r#"{"mcpServers":{"octobuddy":{"type":"http","url":"http://127.0.0.1:1/mcp/s/t"}}}"#.into()),
            ("mempal".into(), r#"{"mcpServers":{"mempal":{"command":"/bin/mempal","args":["serve","--mcp"]}}}"#.into()),
        ]);
        assert!(args.contains(&"mcp_servers.octobuddy.url=\"http://127.0.0.1:1/mcp/s/t\"".to_string()), "{args:?}");
        assert!(args.contains(&"mcp_servers.mempal.command=\"/bin/mempal\"".to_string()));
        assert!(args.contains(&"mcp_servers.mempal.args=[\"serve\", \"--mcp\"]".to_string()));
    }

    /// Live (`cargo test -- --ignored codex_lead_on_glm`, with the AI
    /// providers profile's core dir in OCTOS_APP_CORE_DIR): Codex as an
    /// OctoBuddy agent, on GLM through the bridge, says what it is asked.
    #[test]
    #[ignore]
    fn codex_lead_on_glm() {
        let r = crate::providers::claude_route("zai-coding/glm-5.3").expect("GLM in AI providers");
        let proxy = crate::claude_proxy::Proxy::start().unwrap();
        let route = Route { model: r.model.clone(), base: proxy.responses_base(&r).unwrap(), effort: None };
        let dir = std::env::temp_dir().join(format!("octobuddy-codex-lead-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inbox = Inbox::default();
        let lead = spawn_codex(&inbox, "s1", &dir.to_string_lossy(), None, "Answer in one word.", Some(&route), &[], Mode::Outer).unwrap();
        lead.send("Reply with exactly the word: pineapple").unwrap();
        let started = std::time::Instant::now();
        let mut deltas = String::new();
        let done = loop {
            assert!(started.elapsed().as_secs() < 120, "no answer in two minutes; deltas: {deltas}");
            let mut found = None;
            for e in crate::events::take(&inbox) {
                match e {
                    LoopEvent::LeadDelta { text, .. } => deltas.push_str(&text),
                    LoopEvent::LeadTurnDone { ok, error, text, lead_session, .. } => found = Some((ok, error, text, lead_session)),
                    LoopEvent::LeadExited { error, .. } => panic!("codex exited: {error:?}"),
                    _ => {}
                }
            }
            if let Some(f) = found {
                break f;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        };
        assert!(done.0, "{:?}", done.1);
        assert!(done.2.to_lowercase().contains("pineapple") && deltas.to_lowercase().contains("pineapple"), "{done:?} / {deltas}");
        assert!(done.3.is_some(), "its thread, to resume");
    }

    /// Live: Codex on GLM and pi on GLM, steered while their command runs.
    #[test]
    #[ignore]
    fn codex_and_pi_steer_a_turn() {
        let proxy = crate::claude_proxy::Proxy::start().unwrap();
        let r = crate::providers::claude_route("zai-coding/glm-5.3").expect("GLM in AI providers");
        for engine in [CODEX, PI] {
            let dir = std::env::temp_dir().join(format!("octobuddy-{engine}-steer-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let inbox = Inbox::default();
            let lead = if engine == CODEX {
                let route = Route { model: r.model.clone(), base: proxy.responses_base(&r).unwrap(), effort: None };
                spawn_codex(&inbox, "steer", &dir.to_string_lossy(), None, "Be brief.", Some(&route), &[], Mode::Inner).unwrap()
            } else {
                let route = Route { model: r.model.clone(), base: proxy.anthropic_base(&r), effort: None };
                spawn_pi(&inbox, "steer", &dir.to_string_lossy(), None, "Be brief.", &route, Mode::Inner).unwrap()
            };
            crate::lead::steered_turn(&lead, &inbox, engine);
        }
    }

    /// Live: the same, steered while they write a reply with no tool to
    /// call (no tool boundary): the steer must still be heard, in this
    /// turn or as the next one.
    #[test]
    #[ignore]
    fn codex_and_pi_steer_a_reply() {
        let proxy = crate::claude_proxy::Proxy::start().unwrap();
        let r = crate::providers::claude_route("zai-coding/glm-5.3").expect("GLM in AI providers");
        for engine in [CODEX, PI] {
            let dir = std::env::temp_dir().join(format!("octobuddy-{engine}-steer2-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let inbox = Inbox::default();
            let lead = if engine == CODEX {
                let route = Route { model: r.model.clone(), base: proxy.responses_base(&r).unwrap(), effort: None };
                spawn_codex(&inbox, "steer", &dir.to_string_lossy(), None, "Be brief.", Some(&route), &[], Mode::Plain).unwrap()
            } else {
                let route = Route { model: r.model.clone(), base: proxy.anthropic_base(&r), effort: None };
                spawn_pi(&inbox, "steer", &dir.to_string_lossy(), None, "Be brief.", &route, Mode::Plain).unwrap()
            };
            lead.send("Write a 200-word story about a lighthouse keeper. Use no tools.").unwrap();
            let started = std::time::Instant::now();
            let (mut steered, mut taken, mut turns, mut texts, mut seen) = (false, false, 0, Vec::new(), Vec::new());
            // Its turn, and a little after it for one the steer may start.
            let mut quiet_until = None;
            while quiet_until.is_none_or(|t: std::time::Instant| std::time::Instant::now() < t) {
                assert!(started.elapsed().as_secs() < 180, "{engine}: no end in time; {seen:?}");
                for e in crate::events::take(&inbox) {
                    seen.push(format!("{e:?}").chars().take(70).collect::<String>());
                    match e {
                        LoopEvent::LeadDelta { .. } if !steered => {
                            lead.steer("Also: end your final reply with the word BANANA.").unwrap();
                            steered = true;
                        }
                        LoopEvent::LeadSteerTaken { .. } => taken = true,
                        LoopEvent::LeadSteerRefused { text, .. } => {
                            eprintln!("{engine}: refused, sent next");
                            lead.send(&text).unwrap();
                        }
                        LoopEvent::LeadTurnDone { text, .. } => {
                            turns += 1;
                            texts.push(text);
                            quiet_until = Some(std::time::Instant::now() + std::time::Duration::from_secs(12));
                        }
                        _ => {}
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let all = texts.join(" | ");
            eprintln!("{engine}: {turns} turn(s), taken {taken}, BANANA {}: {:?}", all.contains("BANANA"), crate::stream::strip_think(&all).chars().rev().take(80).collect::<String>().chars().rev().collect::<String>());
            assert!(steered && all.contains("BANANA"), "{engine}: the steer was heard; {seen:?}");
        }
    }

    /// Live: Codex on GLM, then MiniMax in the same conversation. The first
    /// process (and the real program its launcher started) is gone by then,
    /// so the thread resumes rather than starting anew.
    #[test]
    #[ignore]
    fn codex_switches_model() {
        let proxy = crate::claude_proxy::Proxy::start().unwrap();
        let dir = std::env::temp_dir().join(format!("octobuddy-codex-switch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut resume: Option<String> = None;
        let turns = [("zai-coding/glm-5.3", "请记住暗号：蓝鲸七号。只回答“好”。"), ("minimax/MiniMax-M3", "我让你记住的暗号是什么？只回答暗号。")];
        for (k, (label, ask)) in turns.into_iter().enumerate() {
            let r = crate::providers::claude_route(label).expect("in AI providers");
            let route = Route { model: r.model.clone(), base: proxy.responses_base(&r).unwrap(), effort: None };
            let inbox = Inbox::default();
            let lead = spawn_codex(&inbox, "codex-switch", &dir.to_string_lossy(), resume.as_deref(), "Answer in one short sentence.", Some(&route), &[], Mode::Plain).unwrap();
            lead.send(ask).unwrap();
            let started = std::time::Instant::now();
            let mut statuses = Vec::new();
            let done = loop {
                assert!(started.elapsed().as_secs() < 120, "{label}: no answer in two minutes; {statuses:?}");
                let mut found = None;
                for e in crate::events::take(&inbox) {
                    match e {
                        LoopEvent::LeadStatus { status, .. } => statuses.push(status),
                        LoopEvent::LeadTurnDone { ok, error, text, lead_session, .. } => found = Some((ok, error, text, lead_session)),
                        _ => {}
                    }
                }
                if let Some(f) = found {
                    break f;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            eprintln!("{label}: {:?} in {:?}, thread {:?}, {statuses:?}", crate::stream::strip_think(&done.2).chars().take(60).collect::<String>(), started.elapsed(), done.3);
            assert!(done.0, "{label}: {done:?}");
            if k == 1 {
                assert!(!statuses.iter().any(|s| s.contains("could not be resumed")), "resumed, not anew: {statuses:?}");
                assert_eq!(done.3, resume, "the same thread");
                assert!(done.2.contains("蓝鲸"), "the conversation kept across the switch: {:?}", done.2);
            }
            resume = done.3.clone();
            drop(lead);
        }
    }

    /// Live (`cargo test -- --ignored codex_chat_reaches_the_network`): in a
    /// plain chat (and an inner loop) Codex's sandbox lets commands out.
    #[test]
    #[ignore]
    fn codex_chat_reaches_the_network() {
        let r = crate::providers::claude_route("zai-coding/glm-5.3").expect("GLM in AI providers");
        let proxy = crate::claude_proxy::Proxy::start().unwrap();
        let route = Route { model: r.model.clone(), base: proxy.responses_base(&r).unwrap(), effort: None };
        let dir = std::env::temp_dir().join(format!("octobuddy-codex-net-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inbox = Inbox::default();
        let lead = spawn_codex(&inbox, "net", &dir.to_string_lossy(), None, "Run what you are asked; answer briefly.", Some(&route), &[], Mode::Plain).unwrap();
        lead.send("Run this shell command and reply with only the first line it printed: curl -sI https://example.com").unwrap();
        let started = std::time::Instant::now();
        let done = loop {
            assert!(started.elapsed().as_secs() < 180, "no answer in three minutes");
            let found = crate::events::take(&inbox).into_iter().find_map(|e| match e {
                LoopEvent::LeadTurnDone { ok, error, text, .. } => Some((ok, error, text)),
                LoopEvent::LeadExited { error, .. } => panic!("codex exited: {error:?}"),
                _ => None,
            });
            if let Some(f) = found {
                break f;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        };
        assert!(done.0, "{:?}", done.1);
        assert!(done.2.contains("HTTP/"), "{done:?}");
    }

    /// Live (`cargo test -- --ignored pi_lead_on_glm`, OCTOS_APP_CORE_DIR as
    /// above, `OCTOBUDDY_PI_BIN` if pi is not on PATH): pi on GLM through the
    /// proxy's Anthropic route, a placeholder key in its models.json.
    #[test]
    #[ignore]
    fn pi_lead_on_glm() {
        let r = crate::providers::claude_route("zai-coding/glm-5.3").expect("GLM in AI providers");
        let proxy = crate::claude_proxy::Proxy::start().unwrap();
        let route = Route { model: r.model.clone(), base: proxy.anthropic_base(&r), effort: None };
        let dir = std::env::temp_dir().join(format!("octobuddy-pi-lead-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inbox = Inbox::default();
        let lead = spawn_pi(&inbox, "pi-test", &dir.to_string_lossy(), None, "Answer in one word.", &route, Mode::Outer).unwrap();
        lead.send("Reply with exactly the word: pineapple").unwrap();
        let started = std::time::Instant::now();
        let mut deltas = String::new();
        let done = loop {
            assert!(started.elapsed().as_secs() < 120, "no answer in two minutes; deltas: {deltas}");
            let mut found = None;
            for e in crate::events::take(&inbox) {
                match e {
                    LoopEvent::LeadDelta { text, .. } => deltas.push_str(&text),
                    LoopEvent::LeadTurnDone { ok, error, text, .. } => found = Some((ok, error, text)),
                    LoopEvent::LeadExited { error, .. } => panic!("pi exited: {error:?}"),
                    _ => {}
                }
            }
            if let Some(f) = found {
                break f;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        };
        assert!(done.0, "{:?}", done.1);
        assert!(done.2.to_lowercase().contains("pineapple"), "{done:?} / {deltas}");
        let models = std::fs::read_to_string(crate::model::data_dir().join("pi/pi-test/models.json")).unwrap();
        assert!(!models.contains(&r.key), "the key never reaches pi");
    }

    /// Live (`cargo test -- --ignored pi_switches_model`): pi on GLM, then
    /// the same conversation on MiniMax (another process), both answer.
    #[test]
    #[ignore]
    fn pi_switches_model() {
        let proxy = crate::claude_proxy::Proxy::start().unwrap();
        let dir = std::env::temp_dir().join(format!("octobuddy-pi-switch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut resume: Option<String> = None;
        // The conversation goes on across the switch, on the model picked now.
        let turns = [("zai-coding/glm-5.3", "请记住暗号：蓝鲸七号。只回答“好”。"), ("minimax/MiniMax-M3", "我让你记住的暗号是什么？只回答暗号。")];
        for (k, (label, ask)) in turns.into_iter().enumerate() {
            let r = crate::providers::claude_route(label).expect("in AI providers");
            let route = Route { model: r.model.clone(), base: proxy.anthropic_base(&r), effort: None };
            let inbox = Inbox::default();
            let lead = spawn_pi(&inbox, "pi-switch", &dir.to_string_lossy(), resume.as_deref(), "Answer in one short sentence.", &route, Mode::Plain).unwrap();
            lead.send(ask).unwrap();
            let started = std::time::Instant::now();
            let mut seen = Vec::new();
            let mut models = Vec::new();
            let done = loop {
                assert!(started.elapsed().as_secs() < 120, "{label}: no answer in two minutes; events: {seen:?}");
                let mut found = None;
                for e in crate::events::take(&inbox) {
                    seen.push(format!("{e:?}").chars().take(80).collect::<String>());
                    match e {
                        LoopEvent::LeadInfo { model, .. } => models.push(model),
                        LoopEvent::LeadTurnDone { ok, error, text, lead_session, .. } => found = Some((ok, error, text, lead_session)),
                        _ => {}
                    }
                }
                if let Some(f) = found {
                    break f;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            eprintln!("{label}: {:?} in {:?}, models {models:?}, session {:?}", done.2.chars().take(60).collect::<String>(), started.elapsed(), done.3);
            assert!(done.0 && !done.2.trim().is_empty(), "{label}: {done:?}");
            assert_eq!(models.last(), Some(&r.model), "{label}: answered on the model picked");
            if k == 1 {
                assert!(done.2.contains("蓝鲸"), "the conversation kept across the switch: {:?}", done.2);
            }
            resume = done.3.filter(|s| !s.is_empty());
            assert!(resume.is_some(), "{label}: its session file, to resume");
            drop(lead);
        }
    }

    #[test]
    fn tools_are_named_as_claude_codes() {
        assert_eq!(tool_name("", "bash"), "Bash");
        assert_eq!(tool_name("fileChange", ""), "Edit");
        assert_eq!(tool_name("", "find"), "Glob");
    }
}
