//! OctoBuddy's tools for the outer loop, over MCP. Claude Code reaches them
//! at a loopback URL (`--mcp-config`, streamable HTTP, JSON answers): a plan,
//! a message, a review each is a tool call that OctoBuddy checks at once, so
//! a mistake comes back as the tool's error within the same turn instead of
//! costing one. The fenced blocks stay as they were, for an outer loop
//! without the tools (octos).
//!
//! A call goes to the view (its state lives on the UI thread) and waits for
//! its answer; the arguments are read by the same code as the blocks.
use crate::events::{post, Inbox, LoopEvent};
use crate::model::SessionRef;
use crate::{plan, OctoBuddyView};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::time::Duration;

/// The loopback server: its port, and the secret its URLs carry.
pub struct Server {
    pub port: u16,
    token: String,
}

impl Server {
    pub fn start(inbox: &Inbox) -> Option<Server> {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        let token = format!("{:x}{:x}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos());
        let (inbox, secret) = (inbox.clone(), token.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (inbox, secret) = (inbox.clone(), secret.clone());
                std::thread::spawn(move || serve(stream, &inbox, &secret));
            }
        });
        Some(Server { port, token })
    }

    /// `--mcp-config` for one session's outer loop.
    pub fn config(&self, session: &str) -> String {
        json!({"mcpServers": {"octobuddy": {"type": "http", "url": format!("http://127.0.0.1:{}/mcp/{}/{}", self.port, session, self.token)}}}).to_string()
    }

    /// An inner loop's own endpoint (its tools), for the shim octos runs.
    pub fn inner_url(&self, peer: &str) -> String {
        format!("http://127.0.0.1:{}/mcp/inner:{}/{}", self.port, peer, self.token)
    }

    /// `--mcp-config` for an inner loop on Claude Code: its own tools
    /// (run its check, look at its app), outside its sandbox.
    pub fn inner_config(&self, peer: &str) -> String {
        json!({"mcpServers": {"octobuddy": {"type": "http", "url": format!("http://127.0.0.1:{}/mcp/inner:{}/{}", self.port, peer, self.token)}}}).to_string()
    }
}

fn serve(mut stream: TcpStream, inbox: &Inbox, secret: &str) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut reader = BufReader::new(match stream.try_clone() { Ok(s) => s, Err(_) => return });
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
            break;
        }
        if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
    }
    // /mcp/<session>/<secret>
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let session = match segs.as_slice() {
        ["mcp", session, token] if *token == secret => session.to_string(),
        _ => return respond(&mut stream, "404 Not Found", ""),
    };
    if method != "POST" {
        // No stream of server messages: the answers come with the requests.
        return respond(&mut stream, "405 Method Not Allowed", "");
    }
    let mut body = vec![0; length.min(4 << 20)];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let Ok(request) = serde_json::from_slice::<Value>(&body) else {
        return respond(&mut stream, "400 Bad Request", "");
    };
    let answers: Vec<Value> = match &request {
        Value::Array(batch) => batch.iter().filter_map(|r| answer(r, &session, inbox)).collect(),
        one => answer(one, &session, inbox).into_iter().collect(),
    };
    match (&request, answers.len()) {
        (_, 0) => respond(&mut stream, "202 Accepted", ""),
        (Value::Array(_), _) => respond(&mut stream, "200 OK", &Value::Array(answers).to_string()),
        _ => respond(&mut stream, "200 OK", &answers[0].to_string()),
    }
}

fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

/// A JSON-RPC answer (none for a notification).
fn answer(request: &Value, session: &str, inbox: &Inbox) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request["method"].as_str().unwrap_or("");
    let result = match method {
        "initialize" => json!({
            "protocolVersion": request.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("2025-03-26"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "octobuddy", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "OctoBuddy's controls for the outer loop: start inner loops, message them, record reviews, read their status."
        }),
        "ping" => json!({}),
        "tools/list" => json!({"tools": if session.starts_with("inner:") { inner_tools() } else { tools() }}),
        "tools/call" => {
            let name = request.pointer("/params/name").and_then(Value::as_str).unwrap_or("").to_string();
            let args = request.pointer("/params/arguments").cloned().unwrap_or(json!({}));
            let (tx, rx) = mpsc::channel();
            post(inbox, LoopEvent::McpCall { session: session.to_string(), name, args, reply: tx });
            // An inner loop's check runs a while (a headless app, its tests).
            let wait = if session.starts_with("inner:") { 600 } else { 120 };
            let outcome = rx.recv_timeout(Duration::from_secs(wait)).unwrap_or_else(|_| Err("OctoBuddy did not answer in time".into()));
            let (text, error) = match outcome { Ok(t) => (t, false), Err(e) => (e, true) };
            json!({"content": [{"type": "text", "text": text}], "isError": error})
        }
        _ => return Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": format!("no method {method}")}})),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// An inner loop's tools.
fn inner_tools() -> Value {
    json!([
        {"name": "octobuddy_check", "description": "Run your slice's own check command (the one OctoBuddy runs when you finish; the lead set it) in your project, outside your sandbox, and get its exit status and output. Run it until it passes before you report.",
         "inputSchema": {"type": "object", "properties": {}}},
        {"name": "octobuddy_app_look", "description": "For an OctoSense app: run it headless now and get its script errors, a screenshot (a PNG under .octobuddy/shots/ you can read) and the widgets on screen (type, id, text), the way a test finds them.",
         "inputSchema": {"type": "object", "properties": {}}},
        probe_tool(),
        drive_tool()
    ])
}

/// The app driven as a person would (outer and inner loops alike).
fn drive_tool() -> Value {
    json!({"name": "octobuddy_app_drive", "description": "For an OctoSense app: run it headless and do what a person would, step by step, then get what each step did, its script errors and the widgets on screen (and a screenshot). Verify a flow by doing it (add an entry, delete it, switch a tab) instead of writing a test script or a probe driver.",
        "inputSchema": {"type": "object", "required": ["steps"], "properties": {"steps": {"type": "array", "description": "in order: {\"click\": \"<widget id or its text>\"}, {\"type\": \"<text into the focused input>\"}, {\"key\": \"Return\"}, {\"wait\": <seconds>}, {\"look\": true}", "items": {"type": "object"}}}}})
}

/// Splash tried at once (outer and inner loops alike).
fn probe_tool() -> Value {
    json!({"name": "octobuddy_app_probe", "description": "For an OctoSense app: run a small Splash program of your own headless (as the app's main.splash, with its manifest) and get its script errors and widgets in seconds. When unsure how Splash behaves (a widget lookup, a string function, a layout), try it here instead of guessing or reading the runtime's source.",
        "inputSchema": {"type": "object", "required": ["source"], "properties": {"source": {"type": "string", "description": "a whole main.splash: declarations, then one root widget"}}}})
}

fn tools() -> Value {
    let slice = json!({"type": "object", "required": ["slug", "brief"], "properties": {
        "slug": {"type": "string", "description": "short kebab-case name"},
        "role": {"type": "string", "description": "developer, tester, writer, reviewer…"},
        "brief": {"type": "string", "description": "its task card: ## Goal, ## Files, ## Facts, ## Done when (about 15 lines)"},
        "read": {"type": "array", "items": {"type": "string"}, "description": "what you or your scouts already read that it needs, as path:L10-L40 (or a small file's path): OctoBuddy pastes those lines in, current, so it does not read them again (6,000 characters at most)"},
        "check": {"type": "string", "description": "shell command OctoBuddy runs when it is done"},
        "rounds": {"type": "number"}, "wave": {"type": "integer"}, "reviews": {"type": "integer"},
        "model": {"type": "string", "description": "one of STATUS's MODELS (for its agent: STATUS's AGENTS); leave out for the default"},
        "agent": {"type": "string", "enum": ["octos", "codex", "pi", "claude"], "description": "one of STATUS's AGENTS; leave out for the session's"},
        "independent_review": {"type": "boolean"}
    }});
    json!([
        {"name": "octobuddy_plan", "description": "Start inner loops, one per slice (1-8, no shared files; later waves wait until you accept the earlier). Answers {ok, started, held}: what began now and which wave waits. Same fields as the octobuddy-plan block.",
         "inputSchema": {"type": "object", "required": ["slices"], "properties": {
            "estimate": {"type": "object", "properties": {"rounds": {"type": "number"}, "waves": {"type": "integer"}, "minutes": {"type": "number"}}},
            "shared": {"type": "string", "description": "what every slice needs alike (data structures, storage keys, names), under 1,500 characters: said once, given to each slice (OctoBuddy adds who owns which files from the cards)"},
            "slices": {"type": "array", "items": slice}}}},
        {"name": "octobuddy_send", "description": "Message inner loops by slug (mode interrupt, steer or queue), close the ones done for good, change what you queued. Answers per message with its mode and queue id (queued_as). Same fields as the octobuddy-send block.",
         "inputSchema": {"type": "object", "properties": {
            "messages": {"type": "array", "items": {"type": "object", "required": ["to", "message"], "properties": {"to": {"type": "string"}, "mode": {"type": "string", "enum": ["queue", "steer", "interrupt"]}, "message": {"type": "string"}}}},
            "close": {"type": "array", "items": {"type": "string"}},
            "rerun": {"type": "array", "description": "run a slice again from its brief on another agent or model (its agent or model failed): one of STATUS's AGENTS, with a model from its list", "items": {"type": "object", "required": ["to"], "properties": {"to": {"type": "string"}, "agent": {"type": "string", "enum": ["octos", "codex", "pi", "claude"]}, "model": {"type": "string"}}}},
            "queue_ops": {"type": "array", "items": {"type": "object", "required": ["op", "to"], "properties": {"op": {"type": "string", "enum": ["cancel", "replace", "merge"]}, "to": {"type": "string"}, "id": {"type": "string"}, "ids": {"type": "array", "items": {"type": "string"}}, "message": {"type": "string"}}}}}}},
        {"name": "octobuddy_review", "description": "Your verdict on inner loops' reported work: accept (done) or fix (send the fix with octobuddy_send). Accepting the last of a wave starts the next.",
         "inputSchema": {"type": "object", "required": ["reviews"], "properties": {
            "reviews": {"type": "array", "items": {"type": "object", "required": ["slug", "verdict"], "properties": {"slug": {"type": "string"}, "verdict": {"type": "string", "enum": ["accept", "fix"]}, "why": {"type": "string"}}}}}}},
        {"name": "octobuddy_plugin", "description": "Call a tool of one of the PLUGINS your rules list (an external plugin at work for this project): its answer as text.",
         "inputSchema": {"type": "object", "required": ["plugin", "tool"], "properties": {
            "plugin": {"type": "string"}, "tool": {"type": "string"}, "args": {"type": "object"}}}},
        {"name": "octobuddy_status", "description": "What each inner loop of this session does now, what waits for it, its estimate against the steps it took, the models.",
         "inputSchema": {"type": "object", "properties": {}}},
        probe_tool(),
        drive_tool(),
        {"name": "octobuddy_learn", "description": "Keep a lesson OctoBuddy's later runs start from (every project): topic splash for how Splash and the app runtime behave (verify it with octobuddy_app_probe first), orchestration for how to plan, split and pick agents and models. Say it as a rule an agent can follow; evidence is what showed it.",
         "inputSchema": {"type": "object", "required": ["topic", "lesson", "evidence"], "properties": {
            "topic": {"type": "string", "enum": ["splash", "orchestration"]}, "lesson": {"type": "string"}, "evidence": {"type": "string"}}}}
    ])
}

impl OctoBuddyView {
    /// A tool call of a session's outer loop: its answer, or what is wrong.
    /// An inner loop's tool call: run off the UI thread; the answer goes to
    /// `reply` when it is done.
    pub(crate) fn inner_mcp_call(&mut self, peer: &str, name: &str, args: &Value, reply: mpsc::Sender<Result<String, String>>) {
        let Some(p) = self.store.find_peer(peer).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| p.id == peer)).cloned() else {
            let _ = reply.send(Err("no such inner loop".into()));
            return;
        };
        let (name, dir, check) = (name.to_string(), p.dir.clone(), p.check.clone());
        let source = args.get("source").and_then(Value::as_str).unwrap_or("").to_string();
        let steps = args.get("steps").cloned().unwrap_or(Value::Null);
        if let Some(q) = self.store.peer_mut(peer) {
            q.activity = Some(match name.as_str() {
                "octobuddy_app_look" => crate::i18n::t("OctoBuddy runs its app for it", "OctoBuddy 在替它运行应用").into(),
                "octobuddy_app_probe" => crate::i18n::t("OctoBuddy tries its Splash for it", "OctoBuddy 在替它试运行一段 Splash").into(),
                "octobuddy_app_drive" => crate::i18n::t("OctoBuddy drives its app for it", "OctoBuddy 在替它操作应用").into(),
                _ => crate::i18n::t("OctoBuddy runs its check for it", "OctoBuddy 在替它跑检查").into(),
            });
        }
        std::thread::spawn(move || {
            let answer = match name.as_str() {
                "octobuddy_app_look" if crate::plugins::active(crate::plugins::OCTOSENSE_APP, &dir) => crate::plugins::octosense_app::look(&dir),
                "octobuddy_app_look" | "octobuddy_app_probe" | "octobuddy_app_drive" if !crate::plugins::active(crate::plugins::OCTOSENSE_APP, &dir) => Err("this project is not an OctoSense app (or its plugin is switched off in Settings › Plugins)".into()),
                "octobuddy_app_drive" => crate::plugins::octosense_app::drive(&dir, &steps),
                "octobuddy_app_probe" => crate::plugins::octosense_app::probe(&dir, &source),
                "octobuddy_check" => match check {
                    None => Err("your slice has no check command: the lead set none".into()),
                    Some(cmd) => crate::plugins::octosense_app::ensure_check(&cmd).map(|_| {
                        let _ = crate::plugins::octosense_app::assemble(&dir);
                        let outcome = crate::verify::run(&cmd, &dir, crate::verify::TIMEOUT);
                        crate::plugins::octosense_app::name_parts(&dir, &outcome.summary(&cmd))
                    }),
                },
                other => Err(format!("no tool {other}")),
            };
            let _ = reply.send(answer);
        });
    }

    pub(crate) fn mcp_call(&mut self, session: &str, name: &str, args: &Value) -> Result<String, String> {
        let at = self.store.find_session(session).ok_or("no such session")?;
        match name {
            "octobuddy_status" => Ok(self.status_block(at).unwrap_or_else(|| "STATUS\n(no inner loops yet)".into())),
            "octobuddy_learn" => {
                let get = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                let said = crate::lessons::add(&get("topic"), &get("lesson"), &get("evidence"), &self.store.projects[at.0].path)?;
                self.system(at, &crate::i18n::pick(format!("The outer loop kept a lesson for later runs ({}): {}", get("topic"), get("lesson")),
                    format!("外环沉淀了一条经验（{}），之后的运行都会用到：{}", get("topic"), get("lesson"))));
                Ok(said)
            }
            "octobuddy_plan" => {
                let reply = plan::split_reply(&format!("```octobuddy-plan\n{args}\n```"));
                if let Some(err) = reply.plan_error {
                    return Err(format!("the plan could not be read: {err}"));
                }
                if reply.slices.is_empty() {
                    return Err("no slice with a brief".into());
                }
                if self.halted.contains(session) {
                    return Err("the person stopped this session: no new inner loops".into());
                }
                let first_wave = reply.slices.iter().map(|s| s.wave.unwrap_or(1)).min().unwrap_or(1);
                let started: Vec<String> = reply.slices.iter().filter(|s| s.wave.unwrap_or(1) <= first_wave).map(|s| s.slug.clone()).collect();
                let held: Vec<(String, u32)> = reply.slices.iter().filter(|s| s.wave.unwrap_or(1) > first_wave)
                    .map(|s| (s.slug.clone(), s.wave.unwrap_or(1))).collect();
                self.apply_reply(at, session, reply);
                let held = held.into_iter().map(|(slug, wave)| format!("\"{slug}\":{wave}")).collect::<Vec<_>>().join(",");
                Ok(format!("{{\"ok\":true,\"started\":{:?},\"held\":{{{held}}}}}", started))
            }
            "octobuddy_send" => {
                let reply = plan::split_reply(&format!("```octobuddy-send\n{args}\n```"));
                if let Some(err) = reply.send_error {
                    return Err(format!("the message could not be read: {err}"));
                }
                let mut named: Vec<&str> = reply.sends.iter().map(|s| s.to.as_str()).collect();
                named.extend(reply.close.iter().map(String::as_str));
                named.extend(reply.queue_ops.iter().map(|o| o.to.as_str()));
                self.known_slugs(at, &named)?;
                let before = self.queued_ids(at);
                let (n_close, n_ops) = (reply.close.len(), reply.queue_ops.len());
                let sends: Vec<(String, String)> = reply.sends.iter().map(|s| (s.to.clone(), s.mode.clone())).collect();
                self.apply_reply(at, session, reply);
                let after = self.queued_ids(at);
                let messages = sends.iter().map(|(to, mode)| send_confirmation(to, mode, &before, &after)).collect::<Vec<_>>().join(",");
                Ok(format!("{{\"ok\":true,\"messages\":[{messages}],\"closed\":{n_close},\"queue_changes\":{n_ops}}}"))
            }
            "octobuddy_review" => {
                let doc = json!({"branches": args.get("reviews").cloned().unwrap_or(json!([]))});
                let reply = plan::split_reply(&format!("```octobuddy-review\n{doc}\n```"));
                if reply.reviews.is_empty() {
                    return Err("no review could be read: each needs a slug and a verdict (accept or fix)".into());
                }
                let named: Vec<&str> = reply.reviews.iter().map(|r| r.slug.as_str()).collect();
                self.known_slugs(at, &named)?;
                let n = reply.reviews.len();
                self.apply_reply(at, session, reply);
                Ok(format!("Recorded {n} review(s)."))
            }
            other => Err(format!("no tool {other}")),
        }
    }

    /// Every slug names an inner loop of the session, or the error lists them.
    fn known_slugs(&self, at: SessionRef, named: &[&str]) -> Result<(), String> {
        let session = self.store.session(at).ok_or("no session")?;
        let slugs: Vec<&str> = session.peers().iter().map(|p| p.slug.as_str()).collect();
        let unknown: Vec<&str> = named.iter().copied().filter(|n| !slugs.contains(n)).collect();
        if unknown.is_empty() {
            return Ok(());
        }
        // Said to the model: in English, whatever the interface speaks.
        let known = if slugs.is_empty() { "none yet (start them with octobuddy_plan)".to_string() } else { slugs.join(", ") };
        Err(format!("no inner loop named {}; the inner loops are: {known}", unknown.join(", ")))
    }
}

/// One send's confirmation: the queue id it waits under, or that it went
/// into a turn now. `before`/`after` are the queued ids by slug, around
/// the dispatch (the handle said to the lead).
fn send_confirmation(to: &str, mode: &str, before: &[(String, Vec<String>)], after: &[(String, Vec<String>)]) -> String {
    let new = after.iter().find(|(slug, _)| slug == to).and_then(|(_, ids)| ids.iter()
        .find(|id| before.iter().find(|(slug, _)| slug == to).is_none_or(|(_, old)| !old.contains(id)))
        .cloned());
    match new {
        Some(id) => format!("{{\"to\":\"{to}\",\"mode\":\"{}\",\"queued_as\":\"{id}\"}}", crate::dispatch::Mode::parse(mode).as_str()),
        None => format!("{{\"to\":\"{to}\",\"mode\":\"{}\",\"delivered\":\"now\"}}", crate::dispatch::Mode::parse(mode).as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_speaks_mcp_over_http() {
        let inbox = Inbox::default();
        assert!(answer(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}), "s", &inbox).is_none(), "a notification gets no answer");
        let tools = answer(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}), "s", &inbox).unwrap();
        let names: Vec<&str> = tools["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["octobuddy_plan", "octobuddy_send", "octobuddy_review", "octobuddy_plugin", "octobuddy_status", "octobuddy_app_probe", "octobuddy_app_drive", "octobuddy_learn"]);

        let server = Server::start(&inbox).unwrap();
        let config: Value = serde_json::from_str(&server.config("s1")).unwrap();
        let url = config["mcpServers"]["octobuddy"]["url"].as_str().unwrap().to_string();
        let path = url.trim_start_matches(&format!("http://127.0.0.1:{}", server.port)).to_string();
        let post = |path: &str| {
            let body = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}).to_string();
            let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
            write!(s, "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        };
        let ok = post(&path);
        assert!(ok.starts_with("HTTP/1.1 200"), "{ok}");
        assert!(ok.contains("\"protocolVersion\":\"2025-06-18\"") && ok.contains("\"name\":\"octobuddy\""));
        assert!(post("/mcp/s1/wrong-secret").starts_with("HTTP/1.1 404"), "the secret guards it");
    }

    #[test]
    fn a_send_answers_with_its_queue_id_or_that_it_went_now() {
        let none: Vec<(String, Vec<String>)> = Vec::new();
        let queued = vec![("stats".to_string(), vec!["q3".to_string(), "q7".to_string()])];
        // A new id in the line after the dispatch: the handle the lead holds.
        assert_eq!(
            send_confirmation("stats", "queue", &none, &queued),
            "{\"to\":\"stats\",\"mode\":\"queue\",\"queued_as\":\"q3\"}"
        );
        // It went into a running turn: no id to hold, and that is said.
        assert_eq!(
            send_confirmation("stats", "steer", &none, &none),
            "{\"to\":\"stats\",\"mode\":\"steer\",\"delivered\":\"now\"}"
        );
        // An id the line already had before the dispatch is not the new one.
        let before = vec![("stats".to_string(), vec!["q3".to_string()])];
        assert_eq!(
            send_confirmation("stats", "queue", &before, &queued),
            "{\"to\":\"stats\",\"mode\":\"queue\",\"queued_as\":\"q7\"}"
        );
    }
}
