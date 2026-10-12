//! OctoBuddy reports to a hagency project room (a Matrix room on a Palpo
//! homeserver, where the team works in Rinx), and the room's coordinator
//! can accept a round from there.
//!
//! What goes to the room, each at most once (its dedupe key kept on disk,
//! so a restart never reposts): a round's acceptance sheet made and how to
//! accept it, a round accepted and by whom, a loop gone silent, a Live
//! incident opened or closed, an App Hub review request, a publish or
//! submission receipt confirmed or mismatched. Never code, keys or
//! transcripts. Reading back: every `OCTOBUDDY_HAGENCY_EVERY` seconds the
//! room's new messages are read; a message from an allow-listed
//! coordinator matching `accept round N` / `验收 第 N 轮` / `确认验收 第 N 轮`
//! confirms that round exactly as the Confirm acceptance button does, and
//! the outcome is posted back. Anyone else's command-looking message is
//! answered once. Room text is never fed to an agent.
//!
//! The link is per project in `<data>/hagency.json` (not a secret):
//! ```json
//! { "projects": { "/path/to/project": {
//!     "homeserver": "https://matrix.example.org",
//!     "room": "!project:example.org",
//!     "as": "@octobuddy:example.org",
//!     "coordinators": ["@you:example.org"] } } }
//! ```
//! The access token is a secret: `OCTOBUDDY_MATRIX_TOKEN`, else the
//! keychain item `OCTOBUDDY_MATRIX_TOKEN::octobuddy` (service `octos`),
//! the way OctoBuddy keeps provider keys. It is never logged and never on
//! a child process's command line: curl gets it from its config on stdin.
use crate::i18n;
use crate::model::{data_dir, now_secs};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

/// The environment variable the demo token is read from.
pub const TOKEN_ENV: &str = "OCTOBUDDY_MATRIX_TOKEN";

/// A project's link to its room, from `<data>/hagency.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub homeserver: String,
    pub room: String,
    /// The Matrix user OctoBuddy posts as (its own messages are skipped).
    pub as_user: String,
    /// Whose replies count as the person's decisions.
    pub coordinators: Vec<String>,
}

/// The config file: `<data>/hagency.json`.
pub fn config_path() -> PathBuf {
    data_dir().join("hagency.json")
}

/// The links a config file's text folds into: a project whose entry lacks
/// a homeserver, room, sender or any coordinator is skipped.
pub fn fold(text: &str) -> BTreeMap<String, Link> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else { return BTreeMap::new() };
    let mut out = BTreeMap::new();
    for (path, l) in v["projects"].as_object().into_iter().flatten() {
        let field = |k: &str| l[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
        let (Some(homeserver), Some(room), Some(as_user)) = (field("homeserver"), field("room"), field("as")) else { continue };
        let coordinators: Vec<String> = l["coordinators"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)).collect();
        if coordinators.is_empty() {
            continue;
        }
        out.insert(path.clone(), Link { homeserver, room, as_user, coordinators });
    }
    out
}

/// The configured links (none when there is no file or it cannot be read).
pub fn load() -> BTreeMap<String, Link> {
    std::fs::read_to_string(config_path()).map(|t| fold(&t)).unwrap_or_default()
}

/// The token: `OCTOBUDDY_MATRIX_TOKEN`, else the keychain item octos
/// reads, under OctoBuddy's own account (never logged, never in argv).
pub fn token() -> Option<String> {
    std::env::var(TOKEN_ENV).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
        .or_else(|| crate::providers::resolve_key(&format!("keychain:{TOKEN_ENV}{}", crate::own_providers::ACCOUNT_SUFFIX), TOKEN_ENV))
}

/// What a project last did in its room, for `octobuddy.status`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Last {
    pub posted: Option<String>,
    pub at: Option<u64>,
    pub error: Option<String>,
}

/// What is kept across runs: the dedupe keys of what was posted (at most
/// once, ever), each room's read marker, each project's last doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub posted: Vec<String>,
    pub since: BTreeMap<String, String>,
    pub last: BTreeMap<String, Last>,
}

impl State {
    fn path() -> PathBuf {
        data_dir().join("hagency-state.json")
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "posted": self.posted,
            "since": self.since,
            "last": self.last.iter().map(|(p, l)| (p.clone(), serde_json::json!({"posted": l.posted, "at": l.at, "error": l.error}))).collect::<serde_json::Map<_, _>>(),
        })
    }

    /// The state as its JSON has it (default on anything unreadable).
    fn from_json(v: &serde_json::Value) -> State {
        let s = |v: &serde_json::Value| v.as_str().filter(|s| !s.is_empty()).map(String::from);
        State {
            posted: v["posted"].as_array().into_iter().flatten().filter_map(|k| k.as_str().map(String::from)).collect(),
            since: v["since"].as_object().into_iter().flatten().filter_map(|(r, t)| t.as_str().map(|t| (r.clone(), t.to_string()))).collect(),
            last: v["last"].as_object().into_iter().flatten().map(|(p, l)| (p.clone(), Last { posted: s(&l["posted"]), at: l["at"].as_u64(), error: s(&l["error"]) })).collect(),
        }
    }

    /// The state as saved (default when there is none or it cannot be read).
    pub fn load() -> State {
        std::fs::read_to_string(Self::path()).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).map(|v| Self::from_json(&v)).unwrap_or_default()
    }

    fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&path, serde_json::to_string_pretty(&self.to_json()).unwrap_or_default()).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            }
        }
    }

    /// `key` was posted already?
    pub fn seen(&self, key: &str) -> bool {
        self.posted.iter().any(|k| k == key)
    }

    /// `key` goes out now for the first time (false: it was posted already).
    pub fn mark(&mut self, key: &str) -> bool {
        if self.seen(key) {
            return false;
        }
        self.posted.push(key.to_string());
        if self.posted.len() > 1000 {
            let over = self.posted.len() - 1000;
            self.posted.drain(..over);
        }
        true
    }

    /// A post went out: what it said, and no error standing.
    fn ok(&mut self, project: &str, text: &str, now: u64) {
        let text: String = text.chars().take(200).collect();
        self.last.insert(project.to_string(), Last { posted: Some(text), at: Some(now), error: None });
    }

    /// A failure stands: true when it is a new one (said once, not again).
    fn error(&mut self, project: &str, err: &str) -> bool {
        let err: String = err.chars().take(200).collect();
        let last = self.last.entry(project.to_string()).or_default();
        let changed = last.error.as_deref() != Some(err.as_str());
        last.error = Some(err);
        changed
    }
}

/// A message waiting to go out (`attempts` failures so far, not before
/// `next_at`). Its dedupe key is already marked in the state when it queues.
#[derive(Clone, Debug)]
pub struct Pending {
    pub project: String,
    pub body: String,
    pub html: String,
    pub attempts: u32,
    pub next_at: u64,
}

/// A room message read back.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub id: String,
    pub sender: String,
    pub body: String,
}

/// What one pass did: failures to say once in the project's session, and
/// each linked room's new messages (its own skipped).
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub failed: Vec<(String, String)>,
    pub heard: Vec<(String, Vec<Heard>)>,
}

impl Outcome {
    pub fn is_empty(&self) -> bool {
        self.failed.is_empty() && self.heard.is_empty()
    }
}

/// The send queue, drained on a background thread (never the UI thread).
static QUEUE: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
/// One pass at a time.
static RUNNING: AtomicBool = AtomicBool::new(false);
/// When the rooms were last read.
static LAST: AtomicU64 = AtomicU64::new(0);
/// Guards the state file against a tell landing mid-pass.
static STATE_LOCK: Mutex<()> = Mutex::new(());

fn state_lock() -> std::sync::MutexGuard<'static, ()> {
    STATE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn queue_lock() -> std::sync::MutexGuard<'static, Vec<Pending>> {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

/// How often the rooms are read, in seconds (`OCTOBUDDY_HAGENCY_EVERY`,
/// default 15, at least 5).
pub fn every() -> u64 {
    std::env::var("OCTOBUDDY_HAGENCY_EVERY").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(15).max(5)
}

/// Seconds before the next try after `failures` in a row: 30, 60, 120,
/// then it is given up on (its error stands in the state and was said once).
pub fn backoff(failures: u32) -> u64 {
    match failures {
        0 | 1 => 30,
        2 => 60,
        _ => 120,
    }
}

/// A failed send again later, or nothing left of it after three failures.
pub fn retry(p: Pending, now: u64) -> Option<Pending> {
    let attempts = p.attempts + 1;
    (attempts < 3).then(|| Pending { next_at: now + backoff(attempts), attempts, ..p })
}

/// `text` as a Matrix `formatted_body`: escaped, newlines as `<br>`.
pub fn html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            _ => out.push(c),
        }
    }
    out
}

/// Something the room should see, at most once ever. Nothing happens for a
/// project with no link, and a key posted before goes nowhere again. The
/// send itself is queued for the next pass (a background thread).
pub fn tell(project: &str, key: &str, text: &str) {
    if text.trim().is_empty() || !load().contains_key(project) {
        return;
    }
    let html = html(text);
    let _guard = state_lock();
    let mut st = State::load();
    if !st.mark(key) {
        return;
    }
    st.save();
    queue_lock().push(Pending { project: project.to_string(), body: text.to_string(), html, attempts: 0, next_at: 0 });
}

/// The message for a round's sheet: what it holds and how to accept it.
pub fn sheet_message(round: u64, c: &crate::acceptance::Counts, open: &[String]) -> String {
    let mut text = i18n::pick(
        format!("Acceptance sheet for round {round}: {} slice(s); their reports' claims ✓ {} held, ✗ {} contradicted, ? {} not checked by OctoBuddy; {} accepted over OctoBuddy's findings; {} thing(s) still need you. Reply \"accept round {round}\" to confirm it.", c.slices, c.held, c.contradicted, c.unchecked, c.overrides, open.len()),
        format!("第 {round} 轮的验收单：{} 个切片；说法 ✓ {} 成立、✗ {} 不符、? {} 未核实；{} 个不顾 OctoBuddy 的检查接受；{} 件事仍需你确认。回复「验收 第 {round} 轮」即确认验收。", c.slices, c.held, c.contradicted, c.unchecked, c.overrides, open.len()));
    if !open.is_empty() {
        let items: Vec<String> = open.iter().take(3).map(|i| format!("· {}", i.chars().take(100).collect::<String>())).collect();
        text.push_str(&i18n::pick(format!("\nStill needs you: {}", items.join(" ")), format!("\n仍需你：{}", items.join(" "))));
    }
    text
}

/// The message for a round accepted (`by`: the person, or a coordinator's
/// Matrix id).
pub fn accepted_message(round: u64, by: &str, commit: &str) -> String {
    i18n::pick(
        format!("✓ Round {round} accepted by {by}: the project is still at {commit} and the round's facts are as the sheet has them."),
        format!("✓ 第 {round} 轮已由 {by} 验收：项目仍在 {commit}，这一轮的事实与验收单一致。"))
}

/// A message that asks to accept: its round. `accept round 3`,
/// `验收 第 3 轮`, `确认验收 第 3 轮` (any case, spaces optional).
pub fn parse_accept(body: &str) -> Option<u64> {
    let t: String = body.trim().to_lowercase().split_whitespace().collect();
    if let Some(rest) = t.strip_prefix("acceptround") {
        return rest.parse().ok();
    }
    let t = t.strip_prefix("确认").unwrap_or(&t);
    t.strip_prefix("验收第").and_then(|rest| rest.strip_suffix('轮')).and_then(|n| n.parse().ok())
}

/// What a room message asks for, when it asks for anything.
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    /// A coordinator accepts this round.
    Accept(u64),
    /// Someone not on the allow-list tried; answered once with who can.
    NotYours(String),
}

/// A heard message as a decision: a command from a coordinator counts, a
/// command from anyone else is answered once, anything else is ignored.
pub fn decision(link: &Link, h: &Heard) -> Option<Decision> {
    let round = parse_accept(&h.body)?;
    if link.coordinators.iter().any(|c| c == &h.sender) {
        Some(Decision::Accept(round))
    } else {
        Some(Decision::NotYours(h.sender.clone()))
    }
}

/// A path segment or query value, percent-encoded (room ids hold `!` and `:`).
pub fn encode(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

/// One HTTP call: curl's config (with the token) on its stdin, the body in
/// a 0600 temp file — never in argv, never logged. Its status and answer.
fn http(method: &str, url: &str, token: &str, body: Option<&str>) -> Result<(u16, String), String> {
    use std::io::Write;
    let file = body.map(|b| -> Result<PathBuf, String> {
        let file = std::env::temp_dir().join(format!("octobuddy-hagency-{}", crate::model::new_id("h")));
        {
            let mut f = std::fs::File::create(&file).map_err(|e| e.to_string())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
            }
            f.write_all(b.as_bytes()).map_err(|e| e.to_string())?;
        }
        Ok(file)
    }).transpose()?;
    let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let mut config = format!("url = \"{}\"\nrequest = \"{}\"\nheader = \"authorization: Bearer {}\"\n", quote(url), quote(method), quote(token));
    if let Some(f) = &file {
        config.push_str(&format!("header = \"content-type: application/json\"\ndata-binary = \"@{}\"\n", quote(&f.to_string_lossy())));
    }
    let child = std::process::Command::new("curl").args(["-sS", "-m", "20", "-K", "-", "-w", "\n%{http_code}"])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            if let Some(f) = &file {
                let _ = std::fs::remove_file(f);
            }
            return Err(format!("could not run curl: {e}"));
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(config.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string());
    if let Some(f) = &file {
        let _ = std::fs::remove_file(f);
    }
    let out = out?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = text.rsplit_once('\n').unwrap_or((text.as_str(), ""));
    Ok((code.trim().parse().unwrap_or(0), body.to_string()))
}

/// One message to the room (`m.text` with an HTML form for Rinx).
pub fn send(link: &Link, token: &str, p: &Pending) -> Result<(), String> {
    let txn = format!("octobuddy-{}-{}", crate::model::new_id("h"), p.attempts);
    let url = format!("{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}", link.homeserver.trim_end_matches('/'), encode(&link.room), encode(&txn));
    let body = serde_json::json!({"msgtype": "m.text", "body": p.body, "format": "org.matrix.custom.html", "formatted_body": p.html}).to_string();
    let (code, text) = http("PUT", &url, token, Some(&body))?;
    if (200..300).contains(&code) {
        Ok(())
    } else {
        Err(format!("{code}: {}", text.chars().take(300).collect::<String>()))
    }
}

/// The room's messages since the marker kept (`dir=f`, oldest first), and
/// the marker to keep for next time.
pub fn fetch(link: &Link, token: &str, since: Option<&str>) -> Result<(Vec<Heard>, String), String> {
    let mut url = format!("{}/_matrix/client/v3/rooms/{}/messages?dir=f&limit=100", link.homeserver.trim_end_matches('/'), encode(&link.room));
    if let Some(s) = since.filter(|s| !s.is_empty()) {
        url.push_str(&format!("&from={}", encode(s)));
    }
    let (code, text) = http("GET", &url, token, None)?;
    if !(200..300).contains(&code) {
        return Err(format!("{code}: {}", text.chars().take(300).collect::<String>()));
    }
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("could not read the room's answer: {e}"))?;
    let heard = v["chunk"].as_array().into_iter().flatten().filter(|e| e["type"].as_str() == Some("m.room.message")).filter_map(|e| {
        let body = e["content"]["body"].as_str()?.to_string();
        Some(Heard { id: e["event_id"].as_str().unwrap_or_default().to_string(), sender: e["sender"].as_str().unwrap_or_default().to_string(), body })
    }).collect();
    Ok((heard, v["end"].as_str().unwrap_or_default().to_string()))
}

/// One pass over the queue and the rooms (the pure shape, for tests:
/// everything it needs handed in). Sends what is due, backed off on
/// failure; reads each room from its marker. What it could not do is
/// classified (`failure`) and returned to be said once.
pub fn run(links: &BTreeMap<String, Link>, token: Option<&str>, st: &mut State, queue: &mut Vec<Pending>, now: u64) -> Outcome {
    let mut out = Outcome::default();
    if links.is_empty() {
        queue.clear();
        return out;
    }
    let mut keep = Vec::new();
    for p in std::mem::take(queue) {
        if p.next_at > now {
            keep.push(p);
            continue;
        }
        let Some(link) = links.get(&p.project) else { continue };
        let Some(token) = token else {
            if p.attempts == 0 {
                out.failed.push((p.project.clone(), crate::failure::say(crate::failure::Kind::NoAuth, "hagency (the Matrix room)", &format!("no {TOKEN_ENV} and no keychain item"))));
            }
            st.error(&p.project, "no token");
            continue;
        };
        match send(link, token, &p) {
            Ok(()) => st.ok(&p.project, &p.body, now),
            Err(err) => {
                if p.attempts == 0 {
                    out.failed.push((p.project.clone(), crate::failure::say(crate::failure::classify(&err), "hagency (the Matrix room)", &err)));
                }
                st.error(&p.project, &err);
                if let Some(p) = retry(p, now) {
                    keep.push(p);
                }
            }
        }
    }
    *queue = keep;
    for (project, link) in links {
        let Some(token) = token else { break };
        match fetch(link, token, st.since.get(&link.room).map(String::as_str)) {
            Ok((msgs, end)) => {
                if !end.is_empty() {
                    st.since.insert(link.room.clone(), end);
                }
                let msgs: Vec<Heard> = msgs.into_iter().filter(|m| m.sender != link.as_user).collect();
                if !msgs.is_empty() {
                    out.heard.push((project.clone(), msgs));
                }
            }
            Err(err) => {
                if st.error(project, &err) {
                    out.failed.push((project.clone(), crate::failure::say(crate::failure::classify(&err), "hagency (the Matrix room)", &err)));
                }
            }
        }
    }
    out
}

/// One pass over the live queue and the configured rooms, its state saved.
/// The lock is not held over the network: a tell from the UI thread never
/// waits on curl. Keys marked during the pass survive (they land in the
/// file the merge reads back); the pass's `since` and `last` overlay them.
pub fn run_once(now: u64) -> Outcome {
    let (links, mut st, mut queue) = {
        let _guard = state_lock();
        (load(), State::load(), std::mem::take(&mut *queue_lock()))
    };
    let token = token();
    let out = run(&links, token.as_deref(), &mut st, &mut queue, now);
    {
        let _guard = state_lock();
        let mut fresh = State::load();
        fresh.since.extend(st.since);
        fresh.last.extend(st.last);
        queue_lock().extend(queue);
        fresh.save();
    }
    out
}

/// A project's `hagency` field for `octobuddy.status` (null when not linked).
pub fn status(project: &str) -> serde_json::Value {
    let Some(link) = load().get(project).cloned() else { return serde_json::Value::Null };
    let st = State::load();
    let last = st.last.get(project);
    serde_json::json!({
        "room": link.room,
        "last_posted": last.and_then(|l| l.posted.clone()),
        "last_error": last.and_then(|l| l.error.clone()),
    })
}

impl crate::OctoBuddyView {
    /// The clock's tick: what is queued goes out as it comes, and every
    /// `every()` seconds the rooms are read — all off the UI thread; what
    /// comes back arrives as `LoopEvent::HagencyDone`.
    pub(crate) fn hagency_tick(&mut self) {
        let now = now_secs();
        let due = now >= LAST.load(Ordering::Relaxed).saturating_add(every());
        if !due && queue_lock().is_empty() {
            return;
        }
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        LAST.store(now, Ordering::Relaxed);
        let (inbox, lang) = (self.rt.inbox.clone(), i18n::lang());
        std::thread::spawn(move || {
            i18n::set(lang);
            let outcome = run_once(now);
            RUNNING.store(false, Ordering::SeqCst);
            if !outcome.is_empty() {
                crate::events::post(&inbox, crate::events::LoopEvent::HagencyDone(outcome));
            }
        });
    }

    /// A session of a project for its hagency lines: its latest.
    fn hagency_session(&self, project: &str) -> Option<crate::model::SessionRef> {
        let pi = self.store.projects.iter().position(|p| p.path == project)?;
        self.store.projects[pi].sessions.iter().rposition(|s| !s.is_detached() && !s.is_archived()).map(|si| (pi, si))
    }

    /// What a pass brought back: a failure said once in its project's
    /// session; a coordinator's accept confirms the round exactly as the
    /// button does and the outcome goes back to the room; anyone else's
    /// command is answered once. Room text never reaches an agent.
    pub(crate) fn hagency_done(&mut self, outcome: Outcome) {
        for (project, line) in outcome.failed {
            if let Some(at) = self.hagency_session(&project) {
                self.system(at, &line);
            }
        }
        let links = load();
        for (project, heard) in outcome.heard {
            let Some(link) = links.get(&project).cloned() else { continue };
            for h in heard {
                match decision(&link, &h) {
                    Some(Decision::Accept(round)) => {
                        let text = self.accept_from_room(&project, round, &h.sender);
                        tell(&project, &format!("resp:{}", h.id), &text);
                    }
                    Some(Decision::NotYours(_)) => {
                        tell(&project, &format!("deny:{}", h.id), &i18n::pick(
                            format!("Only {} can accept a round here.", link.coordinators.join(", ")),
                            format!("只有 {} 能在这里验收一轮。", link.coordinators.join(", "))));
                    }
                    None => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn link() -> Link {
        Link { homeserver: "http://127.0.0.1:1".into(), room: "!p:hs".into(), as_user: "@octobuddy:hs".into(), coordinators: vec!["@coordinator:hs".into()] }
    }

    #[test]
    fn a_config_folds_and_skips_what_is_incomplete() {
        let text = r#"{"projects": {
            "/a": {"homeserver": "https://hs", "room": "!r:hs", "as": "@bot:hs", "coordinators": ["@you:hs"]},
            "/b": {"homeserver": "https://hs", "room": "!r2:hs"},
            "/c": {"homeserver": "https://hs", "room": "!r3:hs", "as": "@bot:hs", "coordinators": []},
            "/d": "not an object"
        }}"#;
        let links = fold(text);
        assert_eq!(links.len(), 1, "only the complete one");
        assert_eq!(links["/a"], Link { homeserver: "https://hs".into(), room: "!r:hs".into(), as_user: "@bot:hs".into(), coordinators: vec!["@you:hs".into()] });
        assert!(fold("not json").is_empty());
        assert!(fold("{}").is_empty());
    }

    #[test]
    fn html_escapes_what_a_slice_name_may_carry() {
        assert_eq!(html("a <b> & \"c\"\nd"), "a &lt;b&gt; &amp; &quot;c&quot;<br>d");
    }

    #[test]
    fn messages_speak_the_persons_language() {
        let was = i18n::lang();
        i18n::set(i18n::Lang::En);
        let c = crate::acceptance::Counts { slices: 2, held: 3, contradicted: 1, unchecked: 2, overrides: 1 };
        let en = sheet_message(2, &c, &["? it looks <off>".into()]);
        assert!(en.contains("Acceptance sheet for round 2") && en.contains("accept round 2"), "{en}");
        assert!(en.contains("· ? it looks <off>"), "the item is in the plain body unescaped");
        let acc = accepted_message(2, "@you:hs", "abc1234");
        assert!(acc.contains("accepted by @you:hs"), "{acc}");
        i18n::set(i18n::Lang::Zh);
        let zh = sheet_message(2, &c, &[]);
        assert!(zh.contains("第 2 轮的验收单") && zh.contains("验收 第 2 轮"), "{zh}");
        assert!(accepted_message(2, "@you:hs", "abc1234").contains("已由 @you:hs 验收"));
        i18n::set(was);
    }

    #[test]
    fn accept_commands_parse_in_both_languages() {
        assert_eq!(parse_accept("accept round 3"), Some(3));
        assert_eq!(parse_accept("Accept   Round  12"), Some(12));
        assert_eq!(parse_accept("验收 第 3 轮"), Some(3));
        assert_eq!(parse_accept("确认验收 第 3 轮"), Some(3));
        assert_eq!(parse_accept("验收第4轮"), Some(4));
        assert_eq!(parse_accept("accept round"), None);
        assert_eq!(parse_accept("accept round x"), None);
        assert_eq!(parse_accept("验收 第 x 轮"), None);
        assert_eq!(parse_accept("please accept round 3"), None, "only the bare command counts");
        assert_eq!(parse_accept("hello"), None);
    }

    #[test]
    fn only_a_coordinators_command_counts() {
        let l = link();
        let yes = Heard { id: "$1".into(), sender: "@coordinator:hs".into(), body: "accept round 3".into() };
        let no = Heard { id: "$2".into(), sender: "@mallory:hs".into(), body: "accept round 3".into() };
        let chat = Heard { id: "$3".into(), sender: "@mallory:hs".into(), body: "how is it going?".into() };
        assert_eq!(decision(&l, &yes), Some(Decision::Accept(3)));
        assert_eq!(decision(&l, &no), Some(Decision::NotYours("@mallory:hs".into())));
        assert_eq!(decision(&l, &chat), None);
    }

    #[test]
    fn a_key_is_posted_at_most_once() {
        let mut st = State::default();
        assert!(st.mark("k1"));
        assert!(!st.mark("k1"));
        assert!(st.seen("k1"));
        for i in 0..1100 {
            st.mark(&format!("k{i}"));
        }
        assert_eq!(st.posted.len(), 1000, "capped, the newest kept");
        assert!(st.seen("k1099"));
    }

    #[test]
    fn a_failed_send_backs_off_then_is_given_up() {
        assert_eq!((backoff(1), backoff(2), backoff(3), backoff(9)), (30, 60, 120, 120));
        let p = Pending { project: "/a".into(), body: "b".into(), html: "b".into(), attempts: 0, next_at: 0 };
        let p = retry(p, 1000).expect("first failure retries");
        assert_eq!((p.attempts, p.next_at), (1, 1030));
        let p = retry(p, 1000).expect("second failure retries");
        assert_eq!((p.attempts, p.next_at), (2, 1060));
        assert!(retry(p, 1000).is_none(), "then it is given up on");
    }

    #[test]
    fn a_message_for_an_unlinked_project_goes_nowhere() {
        tell("/definitely/not/linked", "k", "text");
        assert!(queue_lock().is_empty());
    }

    /// What the fake room saw: (method, path, authorization, body).
    type Seen = std::sync::Arc<Mutex<Vec<(String, String, String, String)>>>;

    /// A tiny fake Matrix server on 127.0.0.1: it records PUTs, and its
    /// first `/messages` answer holds a stranger's `accept round 2` and the
    /// coordinator's `accept round 1`; a read with a marker gets nothing new.
    fn fake_matrix() -> (String, Seen) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let seen: Seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let recording = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(10)));
                let mut buf = Vec::new();
                let head_end = loop {
                    if let Some(at) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break at;
                    }
                    let mut chunk = [0u8; 4096];
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => break usize::MAX,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                };
                if head_end == usize::MAX {
                    continue;
                }
                let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
                let len = head.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length: ").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                let mut body = buf[head_end + 4..].to_vec();
                while body.len() < len {
                    let mut chunk = [0u8; 4096];
                    match s.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => body.extend_from_slice(&chunk[..n]),
                    }
                }
                body.truncate(len);
                let mut words = head.lines().next().unwrap_or("").split_whitespace();
                let (method, path) = (words.next().unwrap_or("").to_string(), words.next().unwrap_or("").to_string());
                let auth = head.lines().find(|l| l.len() > 15 && l[..15].eq_ignore_ascii_case("authorization: ")).map(|l| l[15..].trim().to_string()).unwrap_or_default();
                let (code, resp) = if method == "PUT" && path.contains("/send/m.room.message/") {
                    (200, r#"{"event_id":"$posted"}"#.to_string())
                } else if method == "GET" && path.contains("/messages") && path.contains("from=") {
                    (200, r#"{"chunk":[],"end":"t2"}"#.to_string())
                } else if method == "GET" && path.contains("/messages") {
                    (200, concat!(
                        r#"{"chunk":["#,
                        r#"{"type":"m.room.message","event_id":"$e1","sender":"@mallory:hs","content":{"msgtype":"m.text","body":"accept round 2"}},"#,
                        r#"{"type":"m.room.message","event_id":"$e2","sender":"@coordinator:hs","content":{"msgtype":"m.text","body":"accept round 1"}},"#,
                        r#"{"type":"m.room.message","event_id":"$e3","sender":"@octobuddy:hs","content":{"msgtype":"m.text","body":"accept round 9"}},"#,
                        r#"{"type":"m.room.member","event_id":"$e4","sender":"@coordinator:hs","content":{}}],"end":"t2"}"#
                    ).to_string())
                } else {
                    (404, r#"{"errcode":"M_NOT_FOUND"}"#.to_string())
                };
                recording.lock().unwrap().push((method, path, auth, String::from_utf8_lossy(&body).to_string()));
                let _ = write!(s, "HTTP/1.1 {code} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp}", resp.len());
            }
        });
        (format!("http://{addr}"), seen)
    }

    /// The environment the pass needs, restored on drop.
    struct Env {
        home: PathBuf,
        saved_home: Option<std::ffi::OsString>,
        saved_token: Option<std::ffi::OsString>,
    }

    impl Env {
        fn install(homeserver: &str) -> (Env, String) {
            let home = std::env::temp_dir().join(format!("octobuddy-hagency-test-{}", crate::model::new_id("t")));
            let _ = std::fs::remove_dir_all(&home);
            std::fs::create_dir_all(&home).unwrap();
            let project = home.join("project").to_string_lossy().to_string();
            let config = serde_json::json!({"projects": {project.clone(): {
                "homeserver": homeserver, "room": "!p:hs", "as": "@octobuddy:hs", "coordinators": ["@coordinator:hs"] }}});
            std::fs::write(home.join("hagency.json"), config.to_string()).unwrap();
            let env = Env { saved_home: std::env::var_os("OCTOBUDDY_HOME"), saved_token: std::env::var_os(TOKEN_ENV), home };
            std::env::set_var("OCTOBUDDY_HOME", &env.home);
            std::env::set_var(TOKEN_ENV, "test-token");
            (env, project)
        }
    }

    impl Drop for Env {
        fn drop(&mut self) {
            match &self.saved_home {
                Some(v) => std::env::set_var("OCTOBUDDY_HOME", v),
                None => std::env::remove_var("OCTOBUDDY_HOME"),
            }
            match &self.saved_token {
                Some(v) => std::env::set_var(TOKEN_ENV, v),
                None => std::env::remove_var(TOKEN_ENV),
            }
            let _ = std::fs::remove_dir_all(&self.home);
        }
    }

    #[test]
    fn a_room_is_posted_to_and_its_coordinator_accepts() {
        let _one = crate::e2e_tests::E2E.lock().unwrap_or_else(|e| e.into_inner());
        let was = i18n::lang();
        i18n::set(i18n::Lang::En);
        let (homeserver, seen) = fake_matrix();
        let (_env, project) = Env::install(&homeserver);

        // A sheet's message, queued twice with the same key: one PUT, with
        // the token on the authorization header and HTML escaped.
        let c = crate::acceptance::Counts { slices: 1, held: 1, contradicted: 0, unchecked: 0, overrides: 0 };
        tell(&project, "sheet:s r1 d", &sheet_message(1, &c, &[]));
        tell(&project, "sheet:s r1 d", &sheet_message(1, &c, &[]));
        assert_eq!(queue_lock().len(), 1, "the same key is queued once");
        let out = run_once(now_secs());
        assert!(out.failed.is_empty(), "{:?}", out.failed);
        {
            let seen = seen.lock().unwrap();
            let puts: Vec<_> = seen.iter().filter(|(m, ..)| m == "PUT").collect();
            assert_eq!(puts.len(), 1, "{seen:?}");
            assert_eq!(puts[0].2, "Bearer test-token");
            assert!(puts[0].1.contains("/_matrix/client/v3/rooms/%21p%3Ahs/send/m.room.message/"), "{}", puts[0].1);
            let v: serde_json::Value = serde_json::from_str(&puts[0].3).unwrap();
            assert_eq!(v["msgtype"], "m.text");
            assert!(v["body"].as_str().unwrap().contains("round 1"));
            assert_eq!(v["format"], "org.matrix.custom.html");
            assert!(v["formatted_body"].as_str().unwrap().contains("round 1"));
        }

        // The same pass read the room: the coordinator's accept and the
        // stranger's, its own message and a non-message event skipped; the
        // marker kept.
        assert_eq!(out.heard.len(), 1);
        let (p, heard) = &out.heard[0];
        assert_eq!(p, &project);
        assert_eq!(heard.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), ["$e1", "$e2"]);
        let links = load();
        let l = &links[&project];
        assert_eq!(decision(l, &heard[0]), Some(Decision::NotYours("@mallory:hs".into())));
        assert_eq!(decision(l, &heard[1]), Some(Decision::Accept(1)));
        assert_eq!(State::load().since.get("!p:hs").map(String::as_str), Some("t2"));

        // After a "restart" (the state read back from disk) the same key
        // still goes nowhere, and the marker moves on: nothing new.
        tell(&project, "sheet:s r1 d", &sheet_message(1, &c, &[]));
        assert!(queue_lock().is_empty(), "posted keys are kept on disk");
        let out = run_once(now_secs());
        assert!(out.heard.is_empty(), "nothing new since the marker");

        // The decision confirms the round's receipt exactly as the button
        // does, with `by` the coordinator's Matrix id.
        let inputs = "sess-1 r1 digest-1";
        let snap = crate::receipts::Snapshot { head: Some("abc1234ffff".into()), content: "digest-1".into(), version: None };
        let r = crate::receipts::prepare(crate::receipts::Kind::Acceptance, &project, inputs, snap.clone(), "OctoBuddy").ok().unwrap();
        let r = crate::acceptance::confirmed(r, "@coordinator:hs", "abc1234", &snap);
        assert_eq!(r.by, "@coordinator:hs");
        assert_eq!(r.status, crate::receipts::Status::Confirmed);
        let kept = crate::receipts::find(&r.id).unwrap();
        assert_eq!((kept.by.as_str(), kept.status), ("@coordinator:hs", crate::receipts::Status::Confirmed), "settled in the ledger");

        // …and the outcome is posted back to the room.
        tell(&project, "resp:$e2", &accepted_message(1, "@coordinator:hs", "abc1234"));
        tell(&project, "deny:$e1", "Only @coordinator:hs can accept a round here.");
        let out = run_once(now_secs());
        assert!(out.failed.is_empty(), "{:?}", out.failed);
        let seen = seen.lock().unwrap();
        let puts: Vec<_> = seen.iter().filter(|(m, ..)| m == "PUT").collect();
        assert_eq!(puts.len(), 3, "{seen:?}");
        let accepted: serde_json::Value = serde_json::from_str(&puts[1].3).unwrap();
        assert!(accepted["body"].as_str().unwrap().contains("accepted by @coordinator:hs"), "{}", puts[1].3);
        assert!(puts[2].3.contains("Only @coordinator:hs can accept"), "{}", puts[2].3);
        drop(seen);
        i18n::set(was);
    }
}
