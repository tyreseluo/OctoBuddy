//! Claude Code on the person's other providers (Z.ai's GLM, MiniMax, …),
//! as Cindy runs it: the `claude` child talks to this loopback proxy with a
//! placeholder key, and the proxy forwards each request to the provider's
//! Anthropic-compatible endpoint with the real key added. The key never
//! reaches a child (not its environment, not its argv): an inner loop's bash
//! cannot print it.
//!
//! Forwarding goes through `curl` (no HTTP client is linked here): the key
//! reaches it on stdin, as config, the body through a private temp file,
//! and the answer comes back byte for byte (`--raw --http1.1`), streamed.
use crate::providers::ClaudeRoute;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

/// The placeholder Claude Code sends as its key: it only passes the CLI's
/// own check; the proxy drops it.
pub const PLACEHOLDER_KEY: &str = "octobuddy-proxy";

pub struct Proxy {
    port: u16,
    secret: String,
    routes: Arc<Mutex<HashMap<String, ClaudeRoute>>>,
}

impl Proxy {
    pub fn start() -> Option<Proxy> {
        let listener = TcpListener::bind("127.0.0.1:0").ok()?;
        let port = listener.local_addr().ok()?.port();
        let secret = crate::model::new_id("k");
        let routes: Arc<Mutex<HashMap<String, ClaudeRoute>>> = Arc::default();
        let (theirs, prefix) = (routes.clone(), format!("/{secret}/"));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (routes, prefix) = (theirs.clone(), prefix.clone());
                std::thread::spawn(move || {
                    let _ = serve(stream, &routes, &prefix);
                });
            }
        });
        Some(Proxy { port, secret, routes })
    }

    fn route_id(route: &ClaudeRoute) -> String {
        route.label.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect()
    }

    /// The base URL of `route`'s Anthropic-compatible endpoint, through
    /// this proxy (for pi's `anthropic-messages` provider).
    pub fn anthropic_base(&self, route: &ClaudeRoute) -> String {
        let id = Self::route_id(route);
        self.routes.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), route.clone());
        format!("http://127.0.0.1:{}/{}/{id}", self.port, self.secret)
    }

    /// The base URL Codex talks to for `route`: OpenAI's Responses API here,
    /// translated to the provider's Chat Completions (`responses_bridge.rs`).
    pub fn responses_base(&self, route: &ClaudeRoute) -> Result<String, String> {
        if route.chat_url.is_none() {
            return Err(format!("{} has no Chat Completions endpoint, so Codex cannot use it", route.label));
        }
        let id = format!("{}{BRIDGE}", Self::route_id(route));
        self.routes.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), route.clone());
        Ok(format!("http://127.0.0.1:{}/{}/{id}", self.port, self.secret))
    }

    /// The environment that has a `claude` child run on `route` through this
    /// proxy (and never on the person's own Claude login).
    pub fn env_for(&self, route: &ClaudeRoute) -> Vec<(String, String)> {
        let id = Self::route_id(route);
        self.routes.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), route.clone());
        let m = route.model.clone();
        vec![
            ("ANTHROPIC_BASE_URL".into(), format!("http://127.0.0.1:{}/{}/{id}", self.port, self.secret)),
            ("ANTHROPIC_API_KEY".into(), PLACEHOLDER_KEY.into()),
            ("CLAUDE_CODE_PROVIDER_MANAGED_BY_HOST".into(), "1".into()),
            ("ANTHROPIC_MODEL".into(), m.clone()),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), m.clone()),
            ("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), m.clone()),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), m.clone()),
            ("ANTHROPIC_SMALL_FAST_MODEL".into(), m.clone()),
            ("CLAUDE_CODE_SUBAGENT_MODEL".into(), m),
            ("API_TIMEOUT_MS".into(), "900000".into()),
            ("CLAUDE_STREAM_IDLE_TIMEOUT_MS".into(), "300000".into()),
            ("DISABLE_TELEMETRY".into(), "1".into()),
            ("DISABLE_ERROR_REPORTING".into(), "1".into()),
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC".into(), "1".into()),
        ]
    }
}

/// A route id's ending for Codex's Responses bridge.
const BRIDGE: &str = "~responses";

/// What a child must not inherit when it runs through the proxy: any
/// Anthropic credential or config of the person's.
pub const SCRUB: &[&str] = &["ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CONFIG_DIR", "ANTHROPIC_BASE_URL", "ANTHROPIC_API_KEY"];

/// A curl run for one request, ended and reaped however the request ends:
/// an agent that hangs up mid-answer (stopped) must not leave curl reading
/// the rest of the answer (paid for) and then lingering as a zombie.
struct Upstream(std::process::Child);

impl Drop for Upstream {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn reply(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    write!(stream, "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len())
}

fn serve(mut stream: TcpStream, routes: &Mutex<HashMap<String, ClaudeRoute>>, prefix: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.trim_end().split_once(':') {
            let (name, value) = (name.trim().to_ascii_lowercase(), value.trim().to_string());
            if name == "content-length" {
                length = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }
    // `/<secret>/<route>/<path…>`: anything else is refused.
    let Some(rest) = target.strip_prefix(prefix) else { return reply(&mut stream, "404 Not Found", "{}") };
    let (id, path) = rest.split_once('/').map(|(a, b)| (a.to_string(), format!("/{b}"))).unwrap_or((rest.to_string(), "/".into()));
    let Some(route) = routes.lock().unwrap_or_else(|e| e.into_inner()).get(&id).cloned() else {
        return reply(&mut stream, "404 Not Found", r#"{"type":"error","error":{"type":"not_found_error","message":"no such OctoBuddy route"}}"#);
    };
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    if id.ends_with(BRIDGE) {
        return bridge(&mut stream, &route, &method, &path, &body);
    }
    // The body, privately, for curl to read.
    let file = std::env::temp_dir().join(format!("octobuddy-proxy-{}", crate::model::new_id("b")));
    {
        let mut f = std::fs::File::create(&file)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
        }
        f.write_all(&body)?;
    }
    let url = format!("{}{}", route.base_url.trim_end_matches('/'), path);
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    // curl's config: on stdin, so neither the key nor the headers are on argv.
    let mut config = format!("url = \"{}\"\nrequest = \"{}\"\n", quote(&url), quote(&method));
    for (name, value) in &headers {
        if matches!(name.as_str(), "host" | "x-api-key" | "authorization" | "content-length" | "connection" | "accept-encoding" | "transfer-encoding") {
            continue;
        }
        config.push_str(&format!("header = \"{}: {}\"\n", quote(name), quote(value)));
    }
    config.push_str(&format!("header = \"x-api-key: {}\"\nheader = \"authorization: Bearer {}\"\n", quote(&route.key), quote(&route.key)));
    if length > 0 {
        config.push_str(&format!("data-binary = \"@{}\"\n", quote(&file.to_string_lossy())));
    }
    let child = Command::new("curl").args(["-sS", "-i", "--raw", "--http1.1", "-N", "-K", "-"])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
    let mut child = match child {
        Ok(c) => Upstream(c),
        Err(e) => {
            let _ = std::fs::remove_file(&file);
            return reply(&mut stream, "502 Bad Gateway", &format!(r#"{{"type":"error","error":{{"type":"api_error","message":"OctoBuddy's proxy could not run curl: {e}"}}}}"#));
        }
    };
    if let Some(mut stdin) = child.0.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let mut out = child.0.stdout.take().unwrap();
    // The answer's head, with `connection: close` (one request per connection).
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if out.read(&mut byte)? == 0 {
            break;
        }
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            // A 100 Continue before the real answer: skip it.
            if head.starts_with(b"HTTP/1.1 100") {
                head.clear();
                continue;
            }
            break;
        }
    }
    let _ = std::fs::remove_file(&file);
    if head.is_empty() {
        let mut err = String::new();
        if let Some(mut e) = child.0.stderr.take() {
            let _ = e.read_to_string(&mut err);
        }
        let _ = child.0.wait();
        let msg = err.trim().replace('"', "'");
        return reply(&mut stream, "502 Bad Gateway", &format!(r#"{{"type":"error","error":{{"type":"api_error","message":"upstream: {msg}"}}}}"#));
    }
    let text = String::from_utf8_lossy(&head).to_string();
    let mut lines = text.split("\r\n").filter(|l| !l.is_empty());
    let mut fixed = format!("{}\r\n", lines.next().unwrap_or("HTTP/1.1 502 Bad Gateway"));
    for line in lines {
        if line.to_ascii_lowercase().starts_with("connection:") || line.to_ascii_lowercase().starts_with("keep-alive:") {
            continue;
        }
        fixed.push_str(line);
        fixed.push_str("\r\n");
    }
    fixed.push_str("connection: close\r\n\r\n");
    stream.write_all(fixed.as_bytes())?;
    // The body, as it comes (an event stream, chunked).
    let mut buf = [0u8; 8192];
    loop {
        let n = out.read(&mut buf)?;
        if n == 0 {
            break;
        }
        stream.write_all(&buf[..n])?;
        stream.flush()?;
    }
    let _ = child.0.wait();
    Ok(())
}

/// Codex's request on a Chat Completions provider: translated, forwarded
/// with the real key, the chat stream translated back as it comes.
/// curl asked for `config`: the child, its output past the headers, and the
/// answer's status line (past a 100 Continue).
fn upstream(config: &str) -> std::io::Result<(Upstream, BufReader<std::process::ChildStdout>, String)> {
    let mut child = Upstream(Command::new("curl").args(["-sS", "-i", "-N", "-K", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?);
    if let Some(mut stdin) = child.0.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let mut out = BufReader::new(child.0.stdout.take().ok_or_else(|| std::io::Error::other("no output"))?);
    let mut status = String::new();
    loop {
        let mut line = String::new();
        if out.read_line(&mut line)? == 0 {
            break;
        }
        if line.starts_with("HTTP/") {
            status = line.trim().to_string();
        } else if line == "\r\n" || line == "\n" {
            if status.contains(" 100") {
                continue;
            }
            break;
        }
    }
    Ok((child, out, status))
}

fn bridge(stream: &mut TcpStream, route: &ClaudeRoute, method: &str, path: &str, body: &[u8]) -> std::io::Result<()> {
    use crate::responses_bridge::{sse, to_chat, Translator};
    if method == "GET" && path.ends_with("/models") {
        let list = serde_json::json!({"object": "list", "data": [{"id": route.model, "object": "model", "owned_by": "octobuddy"}]});
        return reply(stream, "200 OK", &list.to_string());
    }
    if method != "POST" || !path.ends_with("/responses") {
        return reply(stream, "404 Not Found", r#"{"error":{"message":"OctoBuddy's bridge serves only /responses"}}"#);
    }
    let Ok(req) = serde_json::from_slice::<serde_json::Value>(body) else {
        return reply(stream, "400 Bad Request", r#"{"error":{"message":"not JSON"}}"#);
    };
    let (mut chat, custom) = to_chat(&req);
    if chat["model"].as_str().is_none_or(str::is_empty) {
        chat["model"] = route.model.clone().into();
    }
    let model = chat["model"].as_str().unwrap_or("").to_string();
    let file = std::env::temp_dir().join(format!("octobuddy-bridge-{}", crate::model::new_id("b")));
    {
        let mut f = std::fs::File::create(&file)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
        }
        f.write_all(chat.to_string().as_bytes())?;
    }
    let url = format!("{}/chat/completions", route.chat_url.as_deref().unwrap_or("").trim_end_matches('/'));
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let config = format!("url = \"{}\"\nrequest = \"POST\"\nheader = \"content-type: application/json\"\nheader = \"accept: text/event-stream\"\nheader = \"authorization: Bearer {}\"\ndata-binary = \"@{}\"\n",
        quote(&url), quote(&route.key), quote(&file.to_string_lossy()));
    // The provider's rate limit (Z.ai's is per account, shared with every
    // other app on the key) passes in a moment: asked again, waiting longer
    // each time, before anything was said to the agent.
    let mut wait = std::time::Duration::from_secs(1);
    let (mut child, mut out, status) = loop {
        let (mut child, mut out, status) = match upstream(&config) {
            Ok(up) => up,
            Err(e) => {
                let _ = std::fs::remove_file(&file);
                return reply(stream, "502 Bad Gateway", &format!(r#"{{"error":{{"message":"OctoBuddy's bridge could not run curl: {e}"}}}}"#));
            }
        };
        if !status.contains(" 429") || wait > std::time::Duration::from_secs(4) {
            break (child, out, status);
        }
        let mut rest = String::new();
        let _ = out.read_to_string(&mut rest);
        let _ = child.0.wait();
        std::thread::sleep(wait);
        wait *= 2;
    };
    let _ = std::fs::remove_file(&file);
    if !status.contains(" 200") {
        let mut rest = String::new();
        let _ = out.read_to_string(&mut rest);
        let _ = child.0.wait();
        // `OCTOBUDDY_BRIDGE_LOG=<file>`: what was sent and what came back (no key).
        if let Some(log) = std::env::var_os("OCTOBUDDY_BRIDGE_LOG") {
            let line = format!("{status}\nrequest: {chat}\nanswer: {}\n\n", rest.trim());
            let _ = std::fs::OpenOptions::new().create(true).append(true).open(log).and_then(|mut f| f.write_all(line.as_bytes()));
        }
        let code = status.split_whitespace().nth(1).unwrap_or("502").to_string();
        let msg = rest.trim().replace('"', "'");
        return reply(stream, &format!("{code} Upstream"), &format!(r#"{{"error":{{"message":"{}: {msg}"}}}}"#, route.label));
    }
    stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n")?;
    let mut t = Translator::new(&model, custom);
    loop {
        let mut line = String::new();
        if out.read_line(&mut line)? == 0 {
            break;
        }
        let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else { continue };
        if data == "[DONE]" {
            break;
        }
        let Ok(chunk) = serde_json::from_str::<serde_json::Value>(data) else { continue };
        for (name, event) in t.feed(&chunk) {
            stream.write_all(sse(&name, &event).as_bytes())?;
        }
        stream.flush()?;
    }
    for (name, event) in t.finish() {
        stream.write_all(sse(&name, &event).as_bytes())?;
    }
    stream.flush()?;
    let _ = child.0.wait();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_given_up_ends_its_curl() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id().to_string();
        drop(Upstream(child));
        // Ended and reaped: no such process left, not even a zombie.
        let ps = Command::new("ps").args(["-o", "stat=", "-p", &pid]).output().unwrap();
        assert!(String::from_utf8_lossy(&ps.stdout).trim().is_empty(), "still there: {:?}", String::from_utf8_lossy(&ps.stdout));
    }

    /// Live (`cargo test -- --ignored claude_code_on_a_provider`): Claude
    /// Code answers on one of the person's providers through the Anthropic
    /// endpoint the provider runs beside its own API (`OCTOBUDDY_LIVE_PROVIDER`,
    /// `family/model`; MiniMax China by default); the key stays here.
    #[test]
    #[ignore]
    fn claude_code_on_a_provider() {
        let label = std::env::var("OCTOBUDDY_LIVE_PROVIDER").unwrap_or_else(|_| "minimax-cn/MiniMax-M3".into());
        let route = crate::providers::claude_route(&label).expect("in AI providers");
        eprintln!("{label}: {} ({})", route.base_url, route.model);
        let proxy = Proxy::start().unwrap();
        let dir = std::env::temp_dir().join(format!("octobuddy-claude-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cmd = Command::new("claude");
        for k in SCRUB {
            cmd.env_remove(k);
        }
        let out = cmd.current_dir(&dir).envs(proxy.env_for(&route))
            .args(["-p", "Reply with exactly the word: pineapple", "--output-format", "text"])
            .output().unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(!text.contains(&route.key), "the key never reaches Claude Code");
        assert!(text.to_lowercase().contains("pineapple"), "{text}");
    }

    /// Live (`cargo test -- --ignored codex_on_glm`): Codex answers on the
    /// person's GLM through the Responses bridge; the key stays here.
    #[test]
    #[ignore]
    fn codex_on_glm_through_the_bridge() {
        let route = crate::providers::claude_route("zai-coding/glm-5.3").expect("GLM in AI providers");
        let proxy = Proxy::start().unwrap();
        let base = proxy.responses_base(&route).unwrap();
        let dir = std::env::temp_dir().join(format!("octobuddy-codex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = Command::new("codex").current_dir(&dir).env("OCTOBUDDY_CODEX_KEY", PLACEHOLDER_KEY)
            .args(["exec", "--skip-git-repo-check", "-c", "model_provider=\"octobuddy\"", "-c", "model_providers.octobuddy.name=\"OctoBuddy\"",
                "-c", &format!("model_providers.octobuddy.base_url=\"{base}\""), "-c", "model_providers.octobuddy.wire_api=\"responses\"",
                "-c", "model_providers.octobuddy.env_key=\"OCTOBUDDY_CODEX_KEY\"", "-c", "model_providers.octobuddy.supports_websockets=false",
                "-m", &route.model, "Reply with exactly the word: pineapple"])
            .output().unwrap();
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        assert!(!text.contains(&route.key), "the key never reaches Codex");
        assert!(text.to_lowercase().contains("pineapple"), "{text}");
    }

    #[test]
    fn a_child_gets_the_proxy_never_the_key() {
        let proxy = Proxy::start().unwrap();
        let route = ClaudeRoute { label: "zai-coding/glm-5.3".into(), base_url: "https://api.z.ai/api/anthropic".into(), model: "glm-5.3".into(), key: "sk-very-secret".into(), chat_url: Some("https://api.z.ai/api/coding/paas/v4".into()) };
        let env = proxy.env_for(&route);
        assert!(env.iter().all(|(_, v)| !v.contains("sk-very-secret")), "the key stays in the proxy");
        let base = &env.iter().find(|(k, _)| k == "ANTHROPIC_BASE_URL").unwrap().1;
        assert!(base.starts_with("http://127.0.0.1:") && base.ends_with("/zai-coding-glm-5-3"), "{base}");
        assert!(env.iter().any(|(k, v)| k == "ANTHROPIC_MODEL" && v == "glm-5.3"));
        // An unknown path is refused without reaching anything.
        let mut s = TcpStream::connect(("127.0.0.1", proxy.port)).unwrap();
        s.write_all(b"GET /nope HTTP/1.1\r\nhost: x\r\n\r\n").unwrap();
        let mut answer = String::new();
        s.read_to_string(&mut answer).unwrap();
        assert!(answer.starts_with("HTTP/1.1 404"), "{answer}");
    }
}
