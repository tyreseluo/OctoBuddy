//! The app factory (`app-factory`): OctoSense apps asked for from outside
//! OctoBuddy — the OctoSense system agent's `octobuddy.request`, or another
//! app's agent granted it — as GOSIM 2026's "software factory" bounty has it
//! (task progress, agent status, acceptance, handover).
//!
//! A request waits on the Apps page (the sidebar's) for the person: nothing
//! is made, and no model is paid for, before they press Build. Then
//! OctoBuddy makes the app project from the design flow's template (under
//! `<data>/apps/`), reads the data API it names (`app_data`), and opens a new
//! session with the request's brief in its composer: the person picks its
//! models and sends it to the outer loop, as their message. Publish
//! on its card publishes it to the local App Hub, and its live watch starts
//! (`card_loop`). `octobuddy.status` shows where each request is.
//!
//! The requests: `<data>/requests.json`.
use super::app_data::Found;
use crate::model::{data_dir, now_secs};
use crate::{i18n, OctoBuddyView};
use makepad_widgets::*;
use octosense_app_peers::host_tools::{ToolOutcome, ToolReply};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Where a request is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// For the person to decide.
    #[default]
    Waiting,
    Declined,
    /// Its project being made, its data read.
    Preparing,
    /// Its session's outer loop has it.
    Building,
    Published,
    Failed,
}

impl Status {
    fn key(self) -> &'static str {
        match self {
            Status::Waiting => "waiting_for_the_person",
            Status::Declined => "declined",
            Status::Preparing => "preparing",
            Status::Building => "building",
            Status::Published => "published",
            Status::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Status {
        [Status::Waiting, Status::Declined, Status::Preparing, Status::Building, Status::Published, Status::Failed]
            .into_iter().find(|st| st.key() == s).unwrap_or(Status::Failed)
    }
}

/// An app someone asked OctoBuddy for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub id: String,
    pub at: u64,
    /// Who called: `system` (the system agent) or an app's id, as the shell
    /// stamped it; and its client (a Rinx mini app), if one.
    pub caller: String,
    pub client: Option<String>,
    /// Whom it is for, as the caller says.
    pub for_whom: String,
    pub what: String,
    pub acceptance: String,
    /// A JSON API (https) it is built on, or what its data is.
    pub data: String,
    pub name: String,
    pub status: Status,
    pub project: Option<String>,
    pub session: Option<String>,
    pub published: Option<String>,
    pub error: Option<String>,
}

impl Request {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id, "at": self.at, "caller": self.caller, "client": self.client, "for": self.for_whom,
            "what": self.what, "acceptance": self.acceptance, "data": self.data, "name": self.name,
            "status": self.status.key(), "project": self.project, "session": self.session,
            "published": self.published, "error": self.error,
        })
    }

    fn from_json(v: &Value) -> Option<Request> {
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let o = |k: &str| v[k].as_str().map(String::from);
        Some(Request {
            id: v["id"].as_str()?.to_string(), at: v["at"].as_u64().unwrap_or(0), caller: s("caller"), client: o("client"),
            for_whom: s("for"), what: s("what"), acceptance: s("acceptance"), data: s("data"), name: s("name"),
            status: Status::parse(v["status"].as_str().unwrap_or("")), project: o("project"), session: o("session"),
            published: o("published"), error: o("error"),
        })
    }

    /// Who asked, for a person.
    pub fn who(&self) -> String {
        let by = match self.caller.as_str() {
            "system" | "" => i18n::t("the OctoSense assistant", "系统 agent").to_string(),
            // Its own agent in OctoSense (the system agent's message to it, say).
            crate::system::APP_ID => i18n::t("OctoBuddy's agent in OctoSense", "OctoBuddy 的应用 agent").to_string(),
            app => i18n::pick(format!("{app}'s agent"), format!("应用 {app} 的 agent")),
        };
        let client = self.client.as_ref().map(|c| format!(" · {c}")).unwrap_or_default();
        if self.for_whom.is_empty() { format!("{by}{client}") } else { i18n::pick(format!("{by}{client}, for {}", self.for_whom), format!("{by}{client}（替{}）", self.for_whom)) }
    }

    /// The API it names, if its data is one.
    fn api(&self) -> Option<&str> {
        self.data.split_whitespace().find(|w| w.starts_with("https://"))
    }
}

fn file() -> PathBuf {
    data_dir().join("requests.json")
}

fn read() -> Vec<Request> {
    let v: Value = std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    v["requests"].as_array().map(|a| a.iter().filter_map(Request::from_json).collect()).unwrap_or_default()
}

fn write(requests: &[Request]) {
    let _ = std::fs::create_dir_all(data_dir());
    let v = json!({"requests": requests.iter().map(Request::to_json).collect::<Vec<_>>()});
    let _ = std::fs::write(file(), serde_json::to_string_pretty(&v).unwrap_or_default());
}

/// A folder name for it: its name's ASCII, else `app-<id>`; free under `root`.
fn folder_for(root: &Path, r: &Request) -> PathBuf {
    let ascii: String = r.name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-').collect();
    let slug = ascii.split_whitespace().collect::<Vec<_>>().join("-").to_ascii_lowercase();
    // A name with too little ASCII to say what it is (`上海天气 v2`): its id.
    let letters = slug.chars().filter(char::is_ascii_alphabetic).count();
    let base = if letters < 3 { format!("app-{}", r.id.trim_start_matches('r')) } else { slug.trim_matches('-').to_string() };
    let mut dir = root.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = root.join(format!("{base}-{n}"));
        n += 1;
    }
    dir
}

/// What its session's outer loop is told.
fn brief(r: &Request) -> String {
    let mut lines = vec![i18n::pick(format!("{} asked for an OctoSense app (I accepted it):", r.who()), format!("{}请求做一个 OctoSense 应用（我已确认开始）：", r.who())), r.what.clone()];
    if !r.acceptance.is_empty() {
        lines.push(i18n::pick(format!("Done when: {}", r.acceptance), format!("验收标准：{}", r.acceptance)));
    }
    if !r.name.is_empty() {
        lines.push(i18n::pick(format!("Its name: {}", r.name), format!("名字：{}", r.name)));
    }
    if !r.data.is_empty() && r.api().is_none() {
        lines.push(i18n::pick(format!("Its data: {}", r.data), format!("数据：{}", r.data)));
    }
    lines.push(i18n::t("Plan and build it as an OctoSense app is built; tell me what you made once its check passes. Do not publish it: I publish it from the Apps page.",
        "按 OctoSense 应用的做法规划和实现，检查通过后告诉我做了什么。先不要发布：我会在应用页发布。").to_string());
    lines.join("\n")
}

/// A project made for a request (its path, its data read), or why not.
pub type Made = Result<(String, Option<Found>), String>;

/// The requests of this window, and what came back from making projects.
#[derive(Default)]
pub struct Factory {
    pub requests: Vec<Request>,
    /// A project made (its path, its data read) or why not: handled where a `Cx` is.
    pub ready: Vec<(String, Made)>,
    loaded: bool,
}

impl Factory {
    /// A request waits for the person (the sidebar's mark).
    pub fn waiting(&self) -> bool {
        self.requests.iter().any(|r| r.status == Status::Waiting)
    }
}

const CARDS: [LiveId; 4] = [live_id!(rq0), live_id!(rq1), live_id!(rq2), live_id!(rq3)];

fn ago(secs: u64) -> String {
    match secs {
        0..=59 => i18n::t("just now", "刚刚").to_string(),
        60..=3599 => i18n::pick(format!("{} min ago", secs / 60), format!("{} 分钟前", secs / 60)),
        3600..=86_399 => i18n::pick(format!("{} h ago", secs / 3600), format!("{} 小时前", secs / 3600)),
        _ => i18n::pick(format!("{} d ago", secs / 86_400), format!("{} 天前", secs / 86_400)),
    }
}

impl OctoBuddyView {
    pub(crate) fn factory_waiting(&mut self) -> bool {
        self.factory_load();
        self.factory.waiting()
    }

    fn factory_load(&mut self) {
        if !self.factory.loaded {
            self.factory.loaded = true;
            self.factory.requests = read();
        }
    }

    fn factory_save(&self) {
        write(&self.factory.requests);
    }

    /// The requests the page shows: those still open, then the last done.
    fn shown_requests(&self) -> Vec<usize> {
        let mut open: Vec<usize> = (0..self.factory.requests.len()).filter(|&i| matches!(self.factory.requests[i].status, Status::Waiting | Status::Preparing | Status::Building | Status::Failed)).collect();
        let done: Vec<usize> = (0..self.factory.requests.len()).filter(|&i| matches!(self.factory.requests[i].status, Status::Published)).collect();
        open.reverse();
        open.extend(done.into_iter().rev().take(1));
        open.truncate(CARDS.len());
        open
    }

    /// `octobuddy.request`: an app asked for. It waits for the person; the
    /// call is answered at once with its id.
    pub(crate) fn factory_request(&mut self, args: &Value, caller: &str, client: Option<&str>, reply: ToolReply) {
        self.factory_load();
        if !super::enabled(super::APP_FACTORY) || !super::enabled(super::OCTOSENSE_APP) {
            reply.finish(ToolOutcome::error("switched_off", "OctoBuddy's App factory plugin is off (its Settings › Plugins)"));
            return;
        }
        let s = |k: &str| args[k].as_str().unwrap_or("").trim().to_string();
        let what = s("what");
        if what.is_empty() {
            reply.finish(ToolOutcome::error("bad_args", "say what the app is for: what it shows, what it does"));
            return;
        }
        let at = now_secs();
        let r = Request {
            id: format!("r{at:x}{}", self.factory.requests.len()), at, caller: caller.to_string(), client: client.map(String::from),
            for_whom: s("for"), what, acceptance: s("acceptance"), data: s("data"), name: s("name"), ..Default::default()
        };
        let answer = json!({
            "request": r.id, "status": r.status.key(),
            "next": "It waits for the person: OctoBuddy shows it on its Apps page (its sidebar), and builds it only if they press Build. octobuddy.status lists it under requests: waiting_for_the_person, declined, preparing, building (an outer loop works on it), published (installable from App Hub), failed.",
        });
        log!("octobuddy: app request {} from {caller}", r.id);
        self.factory.requests.push(r);
        self.factory_save();
        reply.finish(ToolOutcome::Ok(answer));
    }

    /// The requests as `octobuddy.status` gives them.
    pub(crate) fn factory_status(&self) -> Value {
        let now = now_secs();
        let requests: Vec<Value> = self.factory.requests.iter().rev().take(10).map(|r| {
            let busy = r.session.as_deref().and_then(|s| self.store.find_session(s)).is_some_and(|at| self.session_busy(at));
            json!({
                "id": r.id, "asked_secs_ago": now.saturating_sub(r.at), "what": r.what, "for": r.for_whom,
                "status": r.status.key(), "outer_loop_working": busy, "published_version": r.published, "error": r.error,
                "project": r.project.as_deref().map(|p| Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            })
        }).collect();
        json!(requests)
    }

    /// The Apps page's requests section.
    pub(crate) fn sync_requests(&mut self, cx: &mut Cx) {
        self.factory_load();
        let shown = self.shown_requests();
        self.view.label(cx, ids!(live_requests_title)).set_visible(cx, !shown.is_empty());
        self.view.label(cx, ids!(live_requests_title)).set_text(cx, i18n::t("Requests: apps asked for by the OctoSense assistant or other apps", "请求：系统 agent 或其他应用请你做的应用"));
        self.view.label(cx, ids!(live_apps_title)).set_text(cx, i18n::t("Published apps: the live watch", "已发布的应用：巡检"));
        let now = now_secs();
        for (slot, id) in CARDS.iter().enumerate() {
            let card = self.view.view(cx, &[*id]);
            let Some(&i) = shown.get(slot) else {
                card.set_visible(cx, false);
                continue;
            };
            card.set_visible(cx, true);
            let r = self.factory.requests[i].clone();
            card.label(cx, ids!(who)).set_text(cx, &format!("{} · {}", r.who(), ago(now.saturating_sub(r.at))));
            card.label(cx, ids!(what)).set_text(cx, &r.what);
            let mut details = Vec::new();
            if !r.acceptance.is_empty() {
                details.push(i18n::pick(format!("Done when: {}", r.acceptance), format!("验收标准：{}", r.acceptance)));
            }
            if !r.data.is_empty() {
                details.push(i18n::pick(format!("Data: {}", r.data), format!("数据：{}", r.data)));
            }
            if !r.name.is_empty() {
                details.push(i18n::pick(format!("Name: {}", r.name), format!("名字：{}", r.name)));
            }
            let busy = r.session.as_deref().and_then(|s| self.store.find_session(s)).is_some_and(|at| self.session_busy(at));
            let unsent = r.session.as_deref().and_then(|s| self.store.find_session(s)).and_then(|at| self.store.session(at)).is_some_and(|s| s.messages.is_empty());
            details.push(match r.status {
                Status::Building if unsent => i18n::t("Its brief is in its session's composer: pick the models for its outer and inner loops under it, then send.",
                    "说明已经放在会话的输入框里：在输入框下方选好外环和内环用的模型，然后发送。").to_string(),
                Status::Waiting => i18n::pick(format!("Waiting for you. Build makes its project in {} and hands it to a new session's outer loop.", data_dir().join("apps").display()),
                    format!("等你决定。点「开始做」会在 {} 下建项目，并交给一个新会话的外环。", data_dir().join("apps").display())),
                Status::Declined => i18n::t("Declined.", "已拒绝。").to_string(),
                Status::Preparing => i18n::t("Making its project from the template, reading its data…", "正在从模板建项目、读取数据……").to_string(),
                Status::Building if busy => i18n::t("Its outer loop is at work on it.", "外环正在做。").to_string(),
                Status::Building => i18n::t("Its outer loop has stopped: publish it once its check passed, or tell the session what to change.", "外环已停下：检查通过后可以发布，或者在会话里告诉它还要改什么。").to_string(),
                Status::Published => i18n::pick(format!("Published {} to the local App Hub; its live watch is on.", r.published.clone().unwrap_or_default()),
                    format!("已发布 {} 到本地 App Hub，已开启巡检。", r.published.clone().unwrap_or_default())),
                Status::Failed => i18n::pick(format!("Could not make it: {}", r.error.clone().unwrap_or_default()), format!("没能建好：{}", r.error.clone().unwrap_or_default())),
            });
            card.label(cx, ids!(details)).set_text(cx, &details.join("\n"));
            let show = [
                (ids!(build), r.status == Status::Waiting, i18n::t("Build", "开始做")),
                (ids!(decline), r.status == Status::Waiting, i18n::t("Decline", "不做")),
                (ids!(open), r.session.is_some(), i18n::t("Session", "打开会话")),
                (ids!(publish), r.status == Status::Building && !busy && !unsent && self.publishing.is_none(), i18n::t("Publish", "发布")),
                (ids!(dismiss), matches!(r.status, Status::Failed), i18n::t("Dismiss", "移除")),
            ];
            for (b, visible, text) in show {
                card.button(cx, b).set_visible(cx, visible);
                card.button(cx, b).set_text(cx, text);
            }
        }
    }

    /// The requests' buttons on the Apps page.
    pub(crate) fn requests_actions(&mut self, cx: &mut Cx, actions: &Actions) -> bool {
        for (slot, i) in CARDS.iter().zip(self.shown_requests()) {
            let card = self.view.view(cx, &[*slot]);
            let hit = |id: &[LiveId]| card.button(cx, id).clicked(actions);
            if hit(ids!(build)) {
                self.factory_build(i);
            } else if hit(ids!(decline)) {
                self.factory.requests[i].status = Status::Declined;
                self.factory_save();
            } else if hit(ids!(dismiss)) {
                self.factory.requests.remove(i);
                self.factory_save();
            } else if hit(ids!(open)) {
                if let Some(at) = self.factory.requests[i].session.as_deref().and_then(|s| self.store.find_session(s)) {
                    self.selected = Some(at);
                    self.page = crate::Page::Chat;
                    // Its brief not sent yet: back in the composer.
                    if self.store.session(at).is_some_and(|s| s.messages.is_empty()) {
                        let brief = brief(&self.factory.requests[i]);
                        self.view.text_input(cx, ids!(composer)).set_text(cx, &brief);
                    }
                }
            } else if hit(ids!(publish)) {
                let project = self.factory.requests[i].project.clone().unwrap_or_default();
                if let Some(pi) = self.store.projects.iter().position(|p| p.path == project) {
                    self.card_loop_publish(cx, pi, &project);
                }
            } else {
                continue;
            }
            self.relayout(cx);
            return true;
        }
        false
    }

    /// Build pressed: its project made off the UI thread (`FactoryReady`).
    fn factory_build(&mut self, i: usize) {
        let r = self.factory.requests[i].clone();
        let dir = folder_for(&data_dir().join("apps"), &r);
        self.factory.requests[i].status = Status::Preparing;
        self.factory_save();
        let (inbox, lang) = (self.rt.inbox.clone(), i18n::lang());
        std::thread::spawn(move || {
            i18n::set(lang);
            let result = (|| {
                std::fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
                super::octosense_app::create(&dir)?;
                let project = dir.to_string_lossy().into_owned();
                // Its data's shape, for the outer loop (an API that does not answer is only noted).
                let found = match r.api().map(|url| super::app_data::add_api(&project, url).map_err(|e| format!("{url}: {e}"))) {
                    Some(Ok(found)) => Some(found),
                    Some(Err(err)) => {
                        log!("octobuddy: request {}: its data: {err}", r.id);
                        None
                    }
                    None => None,
                };
                Ok((project, found))
            })();
            crate::events::post(&inbox, crate::events::LoopEvent::FactoryReady { id: r.id, result });
        });
    }

    pub(crate) fn factory_ready(&mut self, id: String, result: Made) {
        self.factory.ready.push((id, result));
    }

    /// Projects made: each its session, its data, its brief to the outer loop.
    pub(crate) fn factory_pending(&mut self, cx: &mut Cx) {
        for (id, result) in std::mem::take(&mut self.factory.ready) {
            let Some(i) = self.factory.requests.iter().position(|r| r.id == id) else { continue };
            let (project, found) = match result {
                Ok(made) => made,
                Err(err) => {
                    self.factory.requests[i].status = Status::Failed;
                    self.factory.requests[i].error = Some(err);
                    self.factory_save();
                    continue;
                }
            };
            let Some(pi) = self.store.add_project(&project) else { continue };
            let Some(at) = self.store.add_session(pi) else { continue };
            if let Some(found) = found {
                let notes = self.store.session_mut(at).map(|s| s.data.get_or_insert_with(Vec::new));
                if let Some(notes) = notes {
                    notes.push(crate::model::DataNote { name: found.name, summary: found.summary, told: false });
                }
            }
            let r = self.factory.requests[i].clone();
            let sid = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
            self.factory.requests[i].project = Some(project);
            self.factory.requests[i].session = Some(sid);
            self.factory.requests[i].status = Status::Building;
            self.factory_save();
            // The person pressed Build and waits: the session is shown, its
            // brief in its composer, to send once its models are picked.
            if self.page == crate::Page::Live {
                self.selected = Some(at);
                self.page = crate::Page::Chat;
                self.view.text_input(cx, ids!(composer)).set_text(cx, &brief(&r));
            }
            self.save();
            self.relayout(cx);
        }
    }

    /// A version of `project` published: a request made there is done, and
    /// its live watch starts.
    pub(crate) fn factory_published(&mut self, project: &str, version: Option<String>) {
        self.factory_load();
        let Some(version) = version else { return };
        let Some(i) = self.factory.requests.iter().position(|r| r.project.as_deref() == Some(project) && r.status != Status::Published) else { return };
        self.factory.requests[i].status = Status::Published;
        self.factory.requests[i].published = Some(version);
        self.factory_save();
        if let Some(pi) = self.store.projects.iter().position(|p| p.path == project) {
            if !self.card_loop.watches.get(project).is_some_and(|w| w.watching) {
                self.card_loop_watch(pi, project, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_keeps_its_fields() {
        let r = Request { id: "r1".into(), at: 3, caller: "system".into(), client: Some("mini".into()), for_whom: "日历".into(), what: "做一个新闻卡片".into(),
            acceptance: "显示 5 条".into(), data: "https://example.com/news.json".into(), name: "News".into(), status: Status::Building,
            project: Some("/p".into()), session: Some("s-1".into()), published: None, error: None };
        assert_eq!(Request::from_json(&r.to_json()), Some(r.clone()));
        assert_eq!(r.api(), Some("https://example.com/news.json"));
    }

    #[test]
    fn a_folder_is_named_for_the_app_and_free() {
        let root = std::env::temp_dir().join(format!("octobuddy-factory-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("news-card")).unwrap();
        let r = |name: &str| Request { id: "r1a".into(), name: name.into(), ..Default::default() };
        assert_eq!(folder_for(&root, &r("News Card")), root.join("news-card-2"));
        assert_eq!(folder_for(&root, &r("新闻卡片")), root.join("app-1a"));
        assert_eq!(folder_for(&root, &r("上海天气小卡片 v2")), root.join("app-1a"), "two characters of ASCII do not name it");
        let _ = std::fs::remove_dir_all(&root);
    }
}
