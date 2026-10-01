//! The inner loop: one `octos serve --stdio` for the view, speaking octos's
//! AppUI JSON-RPC; every peer is one of its sessions, with its own working
//! directory (its clone). A session keeps its conversation between turns,
//! so a peer can be given more work after its first one.
//!
//! What this octos does (checked against 2.0.3, f4a31d9): `turn/start` on a
//! session with a turn running is refused (`turn_in_progress`), so queueing
//! is the caller's; `turn/interrupt` ends the turn and DISCARDS it,
//! including its input; the end of a turn is a `projection/envelope` whose
//! payload is `turn_terminal` (later versions may also send
//! `turn/completed`, handled the same way).
use crate::events::{post, Inbox, LoopEvent};
use crate::workspace::{find_bin, search_path};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A running octos server. Dropping it stops the process.
pub struct Serve {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Option<Child>>>,
    state: Arc<Mutex<State>>,
    next_id: AtomicU64,
}

#[derive(Default)]
struct State {
    /// octos session key → peer id.
    peers: HashMap<String, String>,
    /// JSON-RPC request id → what it was for.
    pending: HashMap<String, Pending>,
    /// Turn id → what the peer has said in it so far.
    turns: HashMap<String, Turn>,
    ended: HashSet<String>,
    /// Session key → the model-loop iteration it is on, for activity lines.
    steps: HashMap<String, u64>,
}

enum Pending {
    /// `session/open`, and the turn to start once it is open.
    Open { peer: String, key: String, then: Option<(String, String)>, read_only: bool },
    /// `permission/profile/set` read-only, then the turn to start.
    ReadOnly { peer: String, key: String, then: Option<(String, String)> },
    Start { peer: String },
    Steer { peer: String },
    Other,
}

#[derive(Default)]
struct Turn {
    peer: String,
    persisted: Vec<String>,
    deltas: String,
    /// What a reasoning model thought: said only when it gives no reply.
    reasoning: String,
    /// Its octos session.
    key: String,
    /// Envelopes of this turn received so far.
    seen: u64,
    /// Its `turn_terminal`, held until the envelopes before it arrive:
    /// (its seq, outcome, error). octos can send it first.
    terminal: Option<(u64, String, Option<String>)>,
}

impl Drop for Serve {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.as_mut() {
                let _ = child.kill();
            }
        }
    }
}

/// A new turn id (octos wants a UUID).
pub fn turn_id() -> String {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let seq = TURN_SEQ.fetch_add(1, Ordering::Relaxed) as u128;
    let n = nanos ^ (seq << 64) ^ (std::process::id() as u128) << 96;
    let h = format!("{n:032x}");
    format!("{}-{}-4{}-8{}-{}", &h[0..8], &h[8..12], &h[13..16], &h[17..20], &h[20..32])
}
static TURN_SEQ: AtomicU64 = AtomicU64::new(1);

impl Serve {
    /// Starts `octos serve --stdio` on `data_dir` (its `profiles/_main.json`
    /// is the model sessions run with). Its log goes to `data_dir/serve.log`.
    pub fn spawn(inbox: &Inbox, data_dir: &Path) -> Result<Serve, String> {
        crate::agents::ready(inbox, "octos")?;
        std::fs::create_dir_all(data_dir).map_err(|err| format!("could not create {}: {err}", data_dir.display()))?;
        let log = std::fs::File::create(data_dir.join("serve.log")).map_err(|err| format!("could not write the octos log: {err}"))?;
        let mut child = Command::new(find_bin("octos"))
            .args(["serve", "--stdio", "--data-dir"]).arg(data_dir)
            .env("PATH", search_path()).env("NO_COLOR", "1")
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(log)
            .spawn().map_err(|err| format!("could not start octos: {err}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let serve = Serve {
            stdin: Arc::new(Mutex::new(stdin)),
            child: Arc::new(Mutex::new(Some(child))),
            state: Default::default(),
            next_id: AtomicU64::new(1),
        };
        let (inbox, state, stdin, child) = (inbox.clone(), serve.state.clone(), serve.stdin.clone(), serve.child.clone());
        let log_path = data_dir.join("serve.log");
        // OCTOBUDDY_TRACE_FRAMES=1 keeps every frame in <data dir>/frames.jsonl, for diagnosis.
        let mut trace = std::env::var_os("OCTOBUDDY_TRACE_FRAMES")
            .and_then(|_| std::fs::File::create(data_dir.join("frames.jsonl")).ok());
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(file) = trace.as_mut() {
                    let _ = writeln!(file, "{line}");
                }
                let Ok(frame) = serde_json::from_str::<Value>(&line) else { continue };
                for (event, follow_up) in handle_frame(&state, &frame) {
                    if let Some(event) = event {
                        post(&inbox, event);
                    }
                    if let Some(request) = follow_up {
                        let _ = write_line(&stdin, &request);
                    }
                }
            }
            let _ = child.lock().ok().and_then(|mut c| c.take()).map(|mut c| c.wait());
            let tail = std::fs::read_to_string(&log_path).unwrap_or_default();
            let error = tail.lines().rev().find(|l| l.contains("ERROR") || l.contains("error")).unwrap_or("octos serve exited").to_string();
            post(&inbox, LoopEvent::ServeExited { error });
        });
        Ok(serve)
    }

    /// Opens `peer`'s session in `cwd` and, once it is open, starts a turn
    /// with `first`. Returns that turn's id.
    pub fn open(&self, peer: &str, key: &str, cwd: &str, first: Option<&str>) -> Result<Option<String>, String> {
        self.open_with(peer, key, cwd, first, json!({}), false)
    }

    /// `open`, with more `session/open` parameters (a profile), and
    /// `read_only`: the session may read, not write (an outer loop), set
    /// before its first turn starts.
    pub fn open_with(&self, peer: &str, key: &str, cwd: &str, first: Option<&str>, more: Value, read_only: bool) -> Result<Option<String>, String> {
        let then = first.map(|text| (turn_id(), text.to_string()));
        let turn = then.as_ref().map(|(t, _)| t.clone());
        let id = self.request_id();
        {
            let mut st = self.state.lock().map_err(|_| "octos state lock poisoned")?;
            st.peers.insert(key.to_string(), peer.to_string());
            if let Some((t, _)) = &then {
                st.turns.insert(t.clone(), Turn { peer: peer.to_string(), ..Default::default() });
            }
            st.pending.insert(id.clone(), Pending::Open { peer: peer.to_string(), key: key.to_string(), then, read_only });
        }
        let mut params = json!({"session_id": key, "cwd": cwd});
        if let (Some(p), Some(more)) = (params.as_object_mut(), more.as_object()) {
            p.extend(more.clone());
        }
        self.request(&id, "session/open", params)?;
        Ok(turn)
    }

    /// Starts a turn in an open session. octos refuses it while another turn
    /// runs there (`PeerTurnRejected`): queue first.
    pub fn start_turn(&self, peer: &str, key: &str, text: &str) -> Result<String, String> {
        let turn = turn_id();
        let id = self.request_id();
        {
            let mut st = self.state.lock().map_err(|_| "octos state lock poisoned")?;
            st.turns.insert(turn.clone(), Turn { peer: peer.to_string(), ..Default::default() });
            st.pending.insert(id.clone(), Pending::Start { peer: peer.to_string() });
        }
        self.request(&id, "turn/start", start_params(key, &turn, text))?;
        Ok(turn)
    }

    /// Ends a running turn. octos discards it, with its input.
    pub fn interrupt(&self, key: &str, turn: &str) -> Result<(), String> {
        let id = self.request_id();
        self.state.lock().map_err(|_| "octos state lock poisoned")?.pending.insert(id.clone(), Pending::Other);
        self.request(&id, "turn/interrupt", json!({"session_id": key, "turn_id": turn}))
    }

    /// Adds a message to the running turn (read at its next step); with no
    /// turn running, octos starts one (`PeerSteered { steered: false }`).
    pub fn steer(&self, peer: &str, key: &str, text: &str) -> Result<(), String> {
        let id = self.request_id();
        self.state.lock().map_err(|_| "octos state lock poisoned")?.pending.insert(id.clone(), Pending::Steer { peer: peer.to_string() });
        self.request(&id, "turn/steer", json!({"session_id": key, "input": [{"kind": "text", "text": text}]}))
    }

    /// Answers a peer's `ask_user_question`: `choice` when it names an
    /// option, else as free text.
    pub fn answer_question(&self, key: &str, question_id: &str, options: &[String], text: &str) -> Result<(), String> {
        let id = self.request_id();
        self.state.lock().map_err(|_| "octos state lock poisoned")?.pending.insert(id.clone(), Pending::Other);
        let picked = options.iter().find(|o| o.eq_ignore_ascii_case(text.trim()));
        let answer = match picked {
            Some(label) => json!({"selected_labels": [label], "free_text": null}),
            None => json!({"selected_labels": [], "free_text": text}),
        };
        self.request(&id, "user_question/respond", json!({"session_id": key, "question_id": question_id, "answers": [answer]}))
    }

    /// `scope` `session` also approves what the session asks for next
    /// (octos's approval scopes: request, turn, session, tool).
    pub fn answer_approval(&self, key: &str, approval_id: &str, approve: bool, scope: Option<&str>) -> Result<(), String> {
        let id = self.request_id();
        self.state.lock().map_err(|_| "octos state lock poisoned")?.pending.insert(id.clone(), Pending::Other);
        let decision = if approve { "approve" } else { "deny" };
        let mut params = json!({"session_id": key, "approval_id": approval_id, "decision": decision});
        if let Some(scope) = scope.filter(|_| approve) {
            params["approval_scope"] = json!(scope);
        }
        self.request(&id, "approval/respond", params)
    }

    fn request_id(&self) -> String {
        format!("ol-{}", self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn request(&self, id: &str, method: &str, params: Value) -> Result<(), String> {
        write_line(&self.stdin, &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
    }
}

fn start_params(key: &str, turn: &str, text: &str) -> Value {
    json!({"session_id": key, "turn_id": turn, "input": [{"kind": "text", "text": text}]})
}

fn write_line(stdin: &Mutex<Option<ChildStdin>>, value: &Value) -> Result<(), String> {
    let mut stdin = stdin.lock().map_err(|_| "octos input lock poisoned".to_string())?;
    let pipe = stdin.as_mut().ok_or("octos has exited")?;
    let mut line = value.to_string();
    line.push('\n');
    pipe.write_all(line.as_bytes()).and_then(|_| pipe.flush()).map_err(|err| format!("could not write to octos: {err}"))
}

/// One frame from octos: the events it means for the view, and a request to
/// send back (the turn after an open). Pure but for `state`, so it is tested.
fn handle_frame(state: &Mutex<State>, frame: &Value) -> Vec<(Option<LoopEvent>, Option<Value>)> {
    let Ok(mut st) = state.lock() else { return Vec::new() };
    let mut out = Vec::new();
    // A response to one of our requests.
    if let Some(id) = frame.get("id").and_then(Value::as_str) {
        if frame.get("method").is_none() {
            let error = frame.get("error").map(|e| e.get("message").and_then(Value::as_str).unwrap_or("error").to_string());
            match st.pending.remove(id) {
                Some(Pending::Open { peer, key, then, read_only }) => match (error, then) {
                    (Some(error), _) => out.push((Some(LoopEvent::PeerFailed { session: String::new(), peer, error: format!("octos could not open the session: {error}") }), None)),
                    (None, then) => {
                        // What the session runs with: octos names its reasoning effort here (none: the model's default).
                        let effort = frame.pointer("/result/opened/reasoning_effort").and_then(Value::as_str).map(str::to_string);
                        out.push((Some(LoopEvent::PeerInfo { peer: peer.clone(), model: None, effort }), None));
                        if read_only {
                            // Its write tools fail from here on; its turn starts once that holds.
                            let set_id = format!("{id}-ro");
                            st.pending.insert(set_id.clone(), Pending::ReadOnly { peer, key: key.clone(), then });
                            out.push((None, Some(json!({"jsonrpc": "2.0", "id": set_id, "method": "permission/profile/set",
                                "params": {"session_id": key, "update": {"mode": "read_only"}}}))));
                            return out;
                        }
                        let Some((turn, text)) = then else { return out };
                        let start_id = format!("{id}-start");
                        st.pending.insert(start_id.clone(), Pending::Start { peer });
                        out.push((None, Some(json!({"jsonrpc": "2.0", "id": start_id, "method": "turn/start", "params": start_params(&key, &turn, &text)}))));
                    }
                },
                Some(Pending::ReadOnly { peer, key, then }) => {
                    if let Some(error) = error {
                        // Refused (an older octos): it still only reads by its rules.
                        out.push((Some(LoopEvent::PeerActivity { peer: peer.clone(), line: format!("read-only not set: {error}") }), None));
                    }
                    if let Some((turn, text)) = then {
                        let start_id = format!("{id}-start");
                        st.pending.insert(start_id.clone(), Pending::Start { peer });
                        out.push((None, Some(json!({"jsonrpc": "2.0", "id": start_id, "method": "turn/start", "params": start_params(&key, &turn, &text)}))));
                    }
                }
                Some(Pending::Start { peer }) => {
                    if let Some(error) = error {
                        out.push((Some(LoopEvent::PeerTurnRejected { peer, error }), None));
                    }
                }
                Some(Pending::Steer { peer }) => match error {
                    Some(error) => out.push((Some(LoopEvent::PeerTurnRejected { peer, error }), None)),
                    None => {
                        let steered = frame.pointer("/result/steered").and_then(Value::as_bool).unwrap_or(false);
                        let turn = frame.pointer("/result/turn_id").and_then(Value::as_str).map(str::to_string);
                        if let (false, Some(t)) = (steered, &turn) {
                            st.turns.insert(t.clone(), Turn { peer: peer.clone(), ..Default::default() });
                        }
                        out.push((Some(LoopEvent::PeerSteered { peer, steered, turn }), None));
                    }
                },
                Some(Pending::Other) | None => {}
            }
            return out;
        }
    }
    let method = frame.get("method").and_then(Value::as_str).unwrap_or("");
    let params = frame.get("params").cloned().unwrap_or(Value::Null);
    let key = params.get("session_id").and_then(Value::as_str).unwrap_or("").to_string();
    let turn = params.get("turn_id").and_then(Value::as_str).unwrap_or("").to_string();
    let peer_of = |st: &State| st.turns.get(&turn).map(|t| t.peer.clone()).or_else(|| st.peers.get(&key).cloned());
    match method {
        "turn/started" => {
            // A new turn: any turn of this session still held is over.
            out.extend(flush_held(&mut st, &key, Some(&turn)));
            if let Some(peer) = peer_of(&st) {
                st.turns.entry(turn.clone()).or_insert_with(|| Turn { peer: peer.clone(), key: key.clone(), ..Default::default() });
                st.steps.insert(key.clone(), 0);
                out.push((Some(LoopEvent::PeerTurnStarted { peer }), None));
            }
        }
        "progress/updated" => {
            let meta = params.get("metadata").cloned().unwrap_or(Value::Null);
            let kind = meta.get("kind").and_then(Value::as_str).unwrap_or("");
            if let Some(i) = meta.get("iteration").and_then(Value::as_u64) {
                // A new iteration of its model loop: one agent-estimation round.
                if i > st.steps.get(&key).copied().unwrap_or(0) {
                    if let Some(peer) = peer_of(&st) {
                        out.push((Some(LoopEvent::PeerRound { peer }), None));
                    }
                }
                st.steps.insert(key.clone(), i);
            }
            let step = st.steps.get(&key).copied().unwrap_or(0);
            let line = match kind {
                "thinking" => Some(format!("step {step} · thinking")),
                "tool_started" => meta.get("tool").and_then(Value::as_str).map(|t| format!("step {step} · running {t}")),
                "tool_completed" => meta.get("tool").and_then(Value::as_str).map(|t| {
                    let ok = meta.get("success").and_then(Value::as_bool).unwrap_or(true);
                    format!("step {step} · {} {t}", if ok { "ran" } else { "failed" })
                }),
                "retry_backoff" => Some(format!("step {step} · retrying the model")),
                _ => None,
            };
            if let (Some(line), Some(peer)) = (line, peer_of(&st)) {
                out.push((Some(LoopEvent::PeerActivity { peer, line }), None));
            }
            if kind == "token_cost_update" {
                if let (Some(tc), Some(peer)) = (meta.get("token_cost"), peer_of(&st)) {
                    let n = |k: &str| tc.get(k).and_then(Value::as_u64).unwrap_or(0);
                    if let Some(model) = tc.get("model").and_then(Value::as_str) {
                        out.push((Some(LoopEvent::PeerInfo { peer: peer.clone(), model: Some(model.to_string()), effort: None }), None));
                    }
                    out.push((Some(LoopEvent::PeerCost {
                        peer, input: n("input_tokens"), output: n("output_tokens"),
                        cost: tc.get("session_cost").and_then(Value::as_f64).unwrap_or(0.0),
                        context_window: tc.get("context_window").and_then(Value::as_u64),
                    }), None));
                }
            }
            // A file its file tools wrote (shell commands are not reported).
            if kind == "file_mutation" {
                if let (Some(path), Some(peer)) = (meta.pointer("/file_mutation/path").and_then(Value::as_str), peer_of(&st)) {
                    out.push((Some(LoopEvent::PeerFileChanged { peer, path: path.to_string() }), None));
                }
            }
        }
        "projection/envelope" => {
            let payload = params.get("payload").cloned().unwrap_or(Value::Null);
            let text = payload.pointer("/data/text").and_then(Value::as_str).unwrap_or("").to_string();
            let peer = peer_of(&st);
            if let Some(peer) = peer.clone().filter(|_| !turn.is_empty() && !st.ended.contains(&turn)) {
                st.turns.entry(turn.clone()).or_insert_with(|| Turn { peer, key: key.clone(), ..Default::default() }).seen += 1;
            }
            // octos escapes HTML in the text it sends (`->` arrives as `-&gt;`).
            let text = unescape_html(&text);
            let data = payload.get("data").cloned().unwrap_or(Value::Null);
            let field = |k: &str| data.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            match payload.get("type").and_then(Value::as_str).unwrap_or("") {
                "assistant_delta" => {
                    if let Some(t) = st.turns.get_mut(&turn) { t.deltas.push_str(&text) }
                    if let Some(peer) = peer.filter(|_| !text.is_empty()) {
                        out.push((Some(LoopEvent::PeerDelta { peer, text: text.clone() }), None));
                    }
                }
                "tool_start" => if let Some(peer) = peer {
                    out.push((Some(LoopEvent::PeerTool { peer, id: field("tool_call_id"), name: field("name"), detail: field("arguments_preview") }), None));
                },
                "tool_end" => if let Some(peer) = peer {
                    // octos says `complete`; only a failure is marked as one.
                    let ok = !matches!(field("status").as_str(), "error" | "failed" | "failure" | "cancelled" | "canceled" | "timeout");
                    out.push((Some(LoopEvent::PeerToolEnd { peer, id: field("tool_call_id"), ok }), None));
                },
                "reasoning_delta" => if let Some(t) = st.turns.get_mut(&turn) { t.reasoning.push_str(&text) },
                "assistant_persisted" => if let Some(t) = st.turns.get_mut(&turn) { t.persisted.push(text) },
                "turn_terminal" => {
                    let outcome = payload.pointer("/data/outcome").and_then(Value::as_str).unwrap_or("completed").to_string();
                    let error = payload.pointer("/data/error/message").and_then(Value::as_str).map(str::to_string);
                    let seq = params.get("seq").and_then(Value::as_u64).unwrap_or(0);
                    if let Some(t) = st.turns.get_mut(&turn) {
                        t.terminal = Some((seq, outcome, error));
                    }
                }
                _ => {}
            }
            // The turn ends once every envelope before its terminal is in.
            let complete = st.turns.get(&turn).and_then(|t| t.terminal.as_ref().map(|(seq, _, _)| t.seen >= *seq)).unwrap_or(false);
            if complete {
                let (_, outcome, error) = st.turns.get_mut(&turn).and_then(|t| t.terminal.take()).unwrap();
                if let Some(event) = end_turn(&mut st, &turn, &outcome, error) {
                    out.push((Some(event), None));
                }
            }
        }
        "turn/completed" => {
            if let Some(event) = end_turn(&mut st, &turn, "completed", None) {
                out.push((Some(event), None));
            }
        }
        "turn/error" => {
            let error = params.get("message").or_else(|| params.pointer("/error/message")).and_then(Value::as_str).map(str::to_string);
            if let Some(event) = end_turn(&mut st, &turn, "failed", error) {
                out.push((Some(event), None));
            }
        }
        "user_question/requested" => {
            if let Some(peer) = peer_of(&st) {
                let mut text = params.get("title").and_then(Value::as_str).unwrap_or("").to_string();
                let mut options = Vec::new();
                for q in params.get("questions").and_then(Value::as_array).cloned().unwrap_or_default() {
                    let ask = q.get("question").and_then(Value::as_str).unwrap_or("");
                    if !text.is_empty() { text.push('\n'); }
                    text.push_str(ask);
                    for o in q.get("options").and_then(Value::as_array).cloned().unwrap_or_default() {
                        if let Some(label) = o.get("label").and_then(Value::as_str) {
                            options.push(label.to_string());
                        }
                    }
                }
                let question_id = params.get("question_id").and_then(Value::as_str).unwrap_or("").to_string();
                out.push((Some(LoopEvent::PeerQuestion { peer, question_id, text, options }), None));
            }
        }
        "turn/steer_dropped" => {
            if let Some(peer) = peer_of(&st) {
                let texts = params.get("inputs").and_then(Value::as_array).cloned().unwrap_or_default().iter()
                    .filter_map(|i| i.get("text").and_then(Value::as_str).or_else(|| i.as_str()).map(str::to_string))
                    .collect();
                out.push((Some(LoopEvent::PeerSteerDropped { peer, texts }), None));
            }
        }
        // Its subagents (spawn / spawn_agent / delegate): how many run, and each one's record.
        "session/orchestration" => {
            // The session went quiet: whatever it held back is not coming.
            if params.get("active").and_then(Value::as_bool) == Some(false) {
                out.extend(flush_held(&mut st, &key, None));
            }
            if let Some(peer) = peer_of(&st) {
                let running = params.get("running_agents").and_then(Value::as_u64).unwrap_or(0);
                out.push((Some(LoopEvent::PeerAgents { peer, running }), None));
            }
        }
        "agent/updated" => {
            if let (Some(peer), Some(agent)) = (peer_of(&st), params.get("agent")) {
                let get = |k: &str| agent.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                let title = [get("title"), get("last_task"), get("nickname")].into_iter().find(|t| !t.is_empty()).unwrap_or_default();
                out.push((Some(LoopEvent::PeerSubagent {
                    peer, id: get("agent_id"), role: get("role"), title: unescape_html(&title), status: get("status"), summary: unescape_html(&get("summary")),
                }), None));
            }
        }
        "approval/decided" | "approval/auto_resolved" | "approval/cancelled" => {
            if let Some(peer) = peer_of(&st) {
                let approval_id = params.get("approval_id").and_then(Value::as_str).unwrap_or("").to_string();
                out.push((Some(LoopEvent::PeerApprovalGone { peer, approval_id }), None));
            }
        }
        "approval/requested" => {
            if let Some(peer) = peer_of(&st) {
                let get = |k: &str| params.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                out.push((Some(LoopEvent::PeerApproval { peer, approval_id: get("approval_id"), title: get("title"), body: get("body") }), None));
            }
        }
        _ => {}
    }
    out
}

/// The entities octos's text uses, back to characters.
fn unescape_html(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&#x27;", "'").replace("&amp;", "&")
}

/// Ends the turns of session `key` whose terminal came and whose missing
/// envelopes never did (all but `keep`).
fn flush_held(st: &mut State, key: &str, keep: Option<&str>) -> Vec<(Option<LoopEvent>, Option<Value>)> {
    let held: Vec<String> = st.turns.iter()
        .filter(|(id, t)| t.key == key && t.terminal.is_some() && Some(id.as_str()) != keep)
        .map(|(id, _)| id.clone()).collect();
    held.into_iter().filter_map(|id| {
        let (_, outcome, error) = st.turns.get_mut(&id)?.terminal.take()?;
        end_turn(st, &id, &outcome, error).map(|e| (Some(e), None))
    }).collect()
}

fn end_turn(st: &mut State, turn: &str, outcome: &str, error: Option<String>) -> Option<LoopEvent> {
    if turn.is_empty() || !st.ended.insert(turn.to_string()) {
        return None;
    }
    let t = st.turns.remove(turn)?;
    let said = if t.persisted.is_empty() { t.deltas } else { t.persisted.join("\n\n") };
    // A reasoning model (glm) can put its whole answer in its reasoning and
    // end with no reply: say that rather than nothing.
    let text = match (said.trim().is_empty(), error) {
        (true, Some(error)) => error,
        (true, None) if !t.reasoning.trim().is_empty() => format!("(no reply; it only reasoned:)\n{}", t.reasoning.trim()),
        (_, _) => said.trim().to_string(),
    };
    Some(LoopEvent::PeerTurnEnded { peer: t.peer, outcome: outcome.to_string(), text })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    #[test]
    fn a_turn_from_open_to_its_end() {
        let state = Mutex::new(State::default());
        // open → the follow-up turn/start is sent back
        state.lock().unwrap().peers.insert("local:octobuddy:w1".into(), "w1".into());
        state.lock().unwrap().turns.insert("T".into(), Turn { peer: "w1".into(), ..Default::default() });
        state.lock().unwrap().pending.insert("ol-1".into(), Pending::Open { peer: "w1".into(), key: "local:octobuddy:w1".into(), then: Some(("T".into(), "do it".into())), read_only: false });
        let out = handle_frame(&state, &frame(r#"{"jsonrpc":"2.0","id":"ol-1","result":{"opened":{"reasoning_effort":"high"}}}"#));
        assert!(matches!(&out[0].0, Some(LoopEvent::PeerInfo { effort: Some(e), model: None, .. }) if e == "high"), "the session's effort is reported");
        let start = out[1].1.as_ref().unwrap();
        assert_eq!(start["method"], "turn/start");
        assert_eq!(start["params"]["input"][0]["text"], "do it");
        // octos refusing that follow-up start reaches the right peer
        let refused = handle_frame(&state, &frame(r#"{"id":"ol-1-start","error":{"message":"no"}}"#));
        assert!(matches!(&refused[0].0, Some(LoopEvent::PeerTurnRejected { peer, .. }) if peer == "w1"));

        let started = handle_frame(&state, &frame(r#"{"method":"turn/started","params":{"session_id":"local:octobuddy:w1","turn_id":"T"}}"#));
        assert!(matches!(&started[0].0, Some(LoopEvent::PeerTurnStarted { peer }) if peer == "w1"));
        let act = handle_frame(&state, &frame(r#"{"method":"progress/updated","params":{"session_id":"local:octobuddy:w1","turn_id":"T","metadata":{"kind":"thinking","iteration":2}}}"#));
        assert!(matches!(&act[0].0, Some(LoopEvent::PeerRound { peer }) if peer == "w1"), "a new iteration is a round");
        assert!(matches!(&act[1].0, Some(LoopEvent::PeerActivity { line, .. }) if line == "step 2 · thinking"));
        let tool = handle_frame(&state, &frame(r#"{"method":"progress/updated","params":{"session_id":"local:octobuddy:w1","turn_id":"T","metadata":{"kind":"tool_completed","tool":"bash","success":true}}}"#));
        assert!(matches!(&tool[0].0, Some(LoopEvent::PeerActivity { line, .. }) if line == "step 2 · ran bash"));
        let delta = handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"local:octobuddy:w1","turn_id":"T","payload":{"type":"assistant_delta","data":{"text":"Do"}}}}"#));
        assert!(matches!(&delta[0].0, Some(LoopEvent::PeerDelta { text, .. }) if text == "Do"), "deltas stream");
        assert!(handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"local:octobuddy:w1","turn_id":"T","payload":{"type":"assistant_persisted","data":{"text":"Done: added it."}}}}"#)).is_empty());
        let end = handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"local:octobuddy:w1","turn_id":"T","payload":{"type":"turn_terminal","data":{"outcome":"completed"}}}}"#));
        assert!(matches!(&end[0].0, Some(LoopEvent::PeerTurnEnded { peer, outcome, text }) if peer == "w1" && outcome == "completed" && text == "Done: added it."));
        // a later turn/completed for the same turn is not a second end
        assert!(handle_frame(&state, &frame(r#"{"method":"turn/completed","params":{"session_id":"local:octobuddy:w1","turn_id":"T"}}"#)).is_empty());
    }

    #[test]
    fn a_busy_session_refuses_and_an_interrupt_ends_the_turn() {
        let state = Mutex::new(State::default());
        state.lock().unwrap().pending.insert("ol-2".into(), Pending::Start { peer: "w1".into() });
        let out = handle_frame(&state, &frame(r#"{"id":"ol-2","error":{"code":-32600,"message":"a turn is already running for this session"}}"#));
        assert!(matches!(&out[0].0, Some(LoopEvent::PeerTurnRejected { peer, .. }) if peer == "w1"));

        state.lock().unwrap().turns.insert("U".into(), Turn { peer: "w1".into(), ..Default::default() });
        let end = handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"k","turn_id":"U","payload":{"type":"turn_terminal","data":{"outcome":"interrupted","error":{"code":"interrupted","message":"turn interrupted by client"}}}}}"#));
        assert!(matches!(&end[0].0, Some(LoopEvent::PeerTurnEnded { outcome, text, .. }) if outcome == "interrupted" && text == "turn interrupted by client"));
    }

    #[test]
    fn a_terminal_that_comes_first_waits_for_the_reply() {
        let state = Mutex::new(State::default());
        state.lock().unwrap().peers.insert("k".into(), "w1".into());
        let env = |seq: u64, payload: &str| frame(&format!(r#"{{"method":"projection/envelope","params":{{"session_id":"k","turn_id":"T","seq":{seq},"payload":{payload}}}}}"#));
        assert!(handle_frame(&state, &frame(r#"{"method":"turn/started","params":{"session_id":"k","turn_id":"T"}}"#))
            .iter().any(|(e, _)| matches!(e, Some(LoopEvent::PeerTurnStarted { .. }))));
        // octos sent the end first (seq 3), then what came before it.
        assert!(handle_frame(&state, &env(3, r#"{"type":"turn_terminal","data":{"outcome":"completed"}}"#)).is_empty(), "not over yet");
        let delta = handle_frame(&state, &env(1, r#"{"type":"assistant_delta","data":{"text":"能，"}}"#));
        assert!(matches!(&delta[0].0, Some(LoopEvent::PeerDelta { text, .. }) if text == "能，"));
        let end = handle_frame(&state, &env(2, r#"{"type":"assistant_persisted","data":{"text":"能，我可以说中文。"}}"#));
        assert!(matches!(&end[0].0, Some(LoopEvent::PeerTurnEnded { text, outcome, .. }) if text == "能，我可以说中文。" && outcome == "completed"));

        // One that never gets its missing envelope ends when the session goes quiet.
        let env2 = |seq: u64, payload: &str| frame(&format!(r#"{{"method":"projection/envelope","params":{{"session_id":"k","turn_id":"U","seq":{seq},"payload":{payload}}}}}"#));
        handle_frame(&state, &frame(r#"{"method":"turn/started","params":{"session_id":"k","turn_id":"U"}}"#));
        handle_frame(&state, &env2(1, r#"{"type":"assistant_persisted","data":{"text":"part"}}"#));
        assert!(handle_frame(&state, &env2(3, r#"{"type":"turn_terminal","data":{"outcome":"completed"}}"#)).is_empty());
        let quiet = handle_frame(&state, &frame(r#"{"method":"session/orchestration","params":{"session_id":"k","active":false,"running_agents":0}}"#));
        assert!(quiet.iter().any(|(e, _)| matches!(e, Some(LoopEvent::PeerTurnEnded { text, .. }) if text == "part")));
    }

    #[test]
    fn a_turn_that_only_reasoned_says_so() {
        let state = Mutex::new(State::default());
        state.lock().unwrap().turns.insert("R".into(), Turn { peer: "w1".into(), ..Default::default() });
        for piece in ["It is ", "30 lines."] {
            let f = format!(r#"{{"method":"projection/envelope","params":{{"session_id":"k","turn_id":"R","payload":{{"type":"reasoning_delta","data":{{"text":"{piece}"}}}}}}}}"#);
            assert!(handle_frame(&state, &frame(&f)).is_empty(), "reasoning is not streamed as reply");
        }
        let end = handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"k","turn_id":"R","payload":{"type":"turn_terminal","data":{"outcome":"completed"}}}}"#));
        assert!(matches!(&end[0].0, Some(LoopEvent::PeerTurnEnded { text, .. }) if text == "(no reply; it only reasoned:)\nIt is 30 lines."));
    }

    #[test]
    fn stream_questions_and_steers() {
        let state = Mutex::new(State::default());
        state.lock().unwrap().peers.insert("k".into(), "w1".into());
        state.lock().unwrap().turns.insert("T".into(), Turn { peer: "w1".into(), ..Default::default() });
        let delta = handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"k","turn_id":"T","payload":{"type":"assistant_delta","data":{"text":"Hi"}}}}"#));
        assert!(matches!(&delta[0].0, Some(LoopEvent::PeerDelta { text, .. }) if text == "Hi"));
        let start = handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"k","turn_id":"T","payload":{"type":"tool_start","data":{"tool_call_id":"c1","name":"bash","arguments_preview":"cargo test"}}}}"#));
        assert!(matches!(&start[0].0, Some(LoopEvent::PeerTool { id, name, detail, .. }) if id == "c1" && name == "bash" && detail == "cargo test"));
        let done = handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"k","turn_id":"T","payload":{"type":"tool_end","data":{"tool_call_id":"c0","status":"complete","duration_ms":2}}}}"#));
        assert!(matches!(&done[0].0, Some(LoopEvent::PeerToolEnd { ok: true, .. })), "octos's complete is a success");
        let end = handle_frame(&state, &frame(r#"{"method":"projection/envelope","params":{"session_id":"k","turn_id":"T","payload":{"type":"tool_end","data":{"tool_call_id":"c1","status":"error"}}}}"#));
        assert!(matches!(&end[0].0, Some(LoopEvent::PeerToolEnd { ok: false, .. })));
        let q = handle_frame(&state, &frame(r#"{"method":"user_question/requested","params":{"session_id":"k","turn_id":"T","question_id":"q1","title":"Pick","body":"","questions":[{"header":"h","question":"Which db?","options":[{"label":"sqlite","description":""},{"label":"postgres","description":""}],"multi_select":false,"allow_free_text":true}]}}"#));
        assert!(matches!(&q[0].0, Some(LoopEvent::PeerQuestion { question_id, text, options, .. }) if question_id == "q1" && text == "Pick\nWhich db?" && options.len() == 2));
        state.lock().unwrap().pending.insert("s1".into(), Pending::Steer { peer: "w1".into() });
        let steered = handle_frame(&state, &frame(r#"{"id":"s1","result":{"turn_id":"T","steered":true}}"#));
        assert!(matches!(&steered[0].0, Some(LoopEvent::PeerSteered { steered: true, .. })));
        let dropped = handle_frame(&state, &frame(r#"{"method":"turn/steer_dropped","params":{"session_id":"k","turn_id":"T","inputs":[{"kind":"text","text":"late"}],"reason":"turn_ended"}}"#));
        assert!(matches!(&dropped[0].0, Some(LoopEvent::PeerSteerDropped { texts, .. }) if texts == &vec!["late".to_string()]));
    }

    #[test]
    fn token_costs_reach_the_peer() {
        let state = Mutex::new(State::default());
        state.lock().unwrap().peers.insert("k".into(), "w1".into());
        let out = handle_frame(&state, &frame(r#"{"method":"progress/updated","params":{"session_id":"k","turn_id":"T","metadata":{"kind":"token_cost_update","token_cost":{"input_tokens":83,"output_tokens":129,"session_cost":0.0028646,"model":"glm-5.3","context_window":1000000}}}}"#));
        assert!(matches!(&out[0].0, Some(LoopEvent::PeerInfo { model: Some(m), .. }) if m == "glm-5.3"), "the model comes with the costs");
        assert!(matches!(&out[1].0, Some(LoopEvent::PeerCost { input: 83, output: 129, context_window: Some(1000000), .. })));
    }

    #[test]
    fn files_and_subagents() {
        let state = Mutex::new(State::default());
        state.lock().unwrap().peers.insert("k".into(), "w1".into());
        let file = handle_frame(&state, &frame(r#"{"method":"progress/updated","params":{"session_id":"k","turn_id":"T","metadata":{"kind":"file_mutation","file_mutation":{"path":"/w/calc.py","operation":"modify","preview_id":"p1"}}}}"#));
        assert!(matches!(&file[0].0, Some(LoopEvent::PeerFileChanged { peer, path }) if peer == "w1" && path == "/w/calc.py"));
        let busy = handle_frame(&state, &frame(r#"{"method":"session/orchestration","params":{"session_id":"k","active":true,"running_agents":2,"pending_continuations":0,"phase":"orchestrating"}}"#));
        assert!(matches!(&busy[0].0, Some(LoopEvent::PeerAgents { running: 2, .. })));
        let agent = handle_frame(&state, &frame(r#"{"method":"agent/updated","params":{"session_id":"k","agent":{"agent_id":"task-1","parent_agent_id":"master","role":"explorer","nickname":"n","title":"find tests","status":"completed","summary":"3 test files"}}}"#));
        assert!(matches!(&agent[0].0, Some(LoopEvent::PeerSubagent { id, role, title, status, summary, .. })
            if id == "task-1" && role == "explorer" && title == "find tests" && status == "completed" && summary == "3 test files"));
    }

    #[test]
    fn html_entities_are_undone() {
        assert_eq!(unescape_html("fn f(a) -&gt; float &amp;&amp; x &lt; 3"), "fn f(a) -> float && x < 3");
        assert_eq!(unescape_html("&amp;gt;"), "&gt;", "decoded once");
    }

    #[test]
    fn turn_ids_look_like_uuids() {
        let (a, b) = (turn_id(), turn_id());
        assert_ne!(a, b);
        assert_eq!(a.len(), 36);
        assert_eq!(a.matches('-').count(), 4);
    }
}
