//! A new OctoSense app (the sidebar's "+"): a dialog for its template, its
//! name and id, where it goes and what it may do.
//!
//! The templates are a repository of their own
//! (github.com/tyreseluo/octosense-app-templates), so they can be fixed
//! without a new OctoBuddy: a copy is kept in `<data>/templates/` and brought
//! up to date each time the dialog opens (`OCTOBUDDY_TEMPLATES_REPO` names
//! another). A blank app is OctoBuddy's own (`blank`), first in the dialog,
//! offline too.
use crate::events::{self, LoopEvent};
use crate::plugins::octosense_app;
use crate::plugins::workbench::{save_permissions, CAPS};
use crate::{i18n, model, OctoBuddyView};
use makepad_widgets::*;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

pub const TEMPLATES_REPO: &str = "https://github.com/tyreseluo/octosense-app-templates.git";

/// The dialog's template cards (the first the blank app's), and its
/// capability boxes (in `CAPS`' order).
const CARDS: [LiveId; 8] = [live_id!(nt0), live_id!(nt1), live_id!(nt2), live_id!(nt3), live_id!(nt4), live_id!(nt5), live_id!(nt6), live_id!(nt7)];
const CAP_BOXES: [LiveId; 9] = [live_id!(nc_storage), live_id!(nc_net), live_id!(nc_images), live_id!(nc_web), live_id!(nc_camera),
    live_id!(nc_microphone), live_id!(nc_library), live_id!(nc_location), live_id!(nc_mail)];

/// A template, as the repository's `index.json` and its manifest say.
#[derive(Clone, Debug)]
pub struct Template {
    pub id: String,
    pub name: (String, String),
    pub summary: (String, String),
    pub features: (Vec<String>, Vec<String>),
    pub dir: PathBuf,
    /// Its manifest's id: the new app's, when its name gives none.
    pub app_id: String,
    pub capabilities: Vec<String>,
    pub hosts: Vec<String>,
    pub icon: Option<&'static str>,
}

impl Template {
    pub fn name(&self) -> &str {
        if i18n::zh() { &self.name.1 } else { &self.name.0 }
    }
    fn summary(&self) -> &str {
        if i18n::zh() { &self.summary.1 } else { &self.summary.0 }
    }
    fn features(&self) -> String {
        if i18n::zh() { self.features.1.join(" · ") } else { self.features.0.join(" · ") }
    }
}

fn repo_url() -> String {
    std::env::var("OCTOBUDDY_TEMPLATES_REPO").ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| TEMPLATES_REPO.into())
}

/// The local copy of the templates repository.
pub fn cache_dir() -> PathBuf {
    model::data_dir().join("templates/octosense-app-templates")
}

fn git(args: &[&str], dir: &Path) -> Result<(), String> {
    let out = Command::new("git").args(args).current_dir(dir)
        // Never wait on a password prompt; give up on a stalled connection.
        .env("GIT_TERMINAL_PROMPT", "0").env("GIT_HTTP_LOW_SPEED_LIMIT", "1000").env("GIT_HTTP_LOW_SPEED_TIME", "20")
        .output().map_err(|e| format!("git: {e}"))?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Brings the local copy up to the repository's default branch. Blocks: off
/// the UI thread.
pub fn sync() -> Result<(), String> {
    let dir = cache_dir();
    let url = repo_url();
    if dir.join(".git").is_dir() {
        git(&["remote", "set-url", "origin", &url], &dir)?;
        git(&["fetch", "-q", "--depth", "1", "origin", "HEAD"], &dir)?;
        return git(&["reset", "-q", "--hard", "FETCH_HEAD"], &dir);
    }
    let parent = dir.parent().ok_or("no templates folder")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    // Cloned beside it, then moved in: a clone cut off leaves no half copy.
    let incoming = parent.join(".incoming");
    let _ = std::fs::remove_dir_all(&incoming);
    git(&["clone", "-q", "--depth", "1", &url, &incoming.to_string_lossy()], parent)?;
    std::fs::rename(&incoming, &dir).map_err(|e| e.to_string())
}

/// A template's icon as the Svg widget takes it (made once per content).
fn icon(text: String) -> &'static str {
    static MADE: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut made = MADE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(known) = made.iter().find(|k| **k == text) {
        return known;
    }
    let leaked: &'static str = Box::leak(text.into_boxed_str());
    made.push(leaked);
    leaked
}

/// The templates in the local copy, in the index's order.
pub fn list() -> Vec<Template> {
    let root = cache_dir();
    let Some(index) = std::fs::read_to_string(root.join("index.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else {
        return Vec::new();
    };
    let pair = |v: &Value| (v["en"].as_str().unwrap_or("").to_string(), v["zh"].as_str().or(v["en"].as_str()).unwrap_or("").to_string());
    let words = |v: &Value| v.as_array().into_iter().flatten().filter_map(|w| w.as_str().map(String::from)).collect::<Vec<_>>();
    index["templates"].as_array().into_iter().flatten().filter_map(|t| {
        let dir = root.join(t["path"].as_str()?);
        let manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("bundle/manifest.json")).ok()?).ok()?;
        let listing: Value = std::fs::read_to_string(dir.join("bundle/listing.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let icon_text = listing["icon"].as_str().and_then(|rel| std::fs::read_to_string(dir.join("bundle").join(rel)).ok())
            .filter(|svg| svg.trim_start().starts_with("<svg"));
        Some(Template {
            id: t["id"].as_str()?.to_string(),
            name: pair(&t["name"]),
            summary: pair(&t["summary"]),
            features: (words(&t["features"]["en"]), words(&t["features"]["zh"])),
            app_id: manifest["id"].as_str().unwrap_or("my-app").to_string(),
            capabilities: words(&manifest["capabilities"]),
            hosts: words(&manifest["network"]["hosts"]),
            icon: icon_text.map(icon),
            dir,
        })
    }).collect()
}

/// An app id from a name: `[a-z0-9.-]`, the rest as `-`; empty when nothing is left.
pub fn id_from(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches(['-', '.']).chars().take(64).collect()
}

/// Why `id` cannot be an app's id, if it cannot (App Hub's rule).
pub fn id_problem(id: &str) -> Option<String> {
    let ok = !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
        && !id.starts_with('.') && !id.contains("..");
    if !ok {
        return Some(i18n::t("The id is 1 to 64 of a-z, 0-9, “.” and “-”, not starting with “.”.", "ID 只能用 a-z、0-9、「.」和「-」，1 到 64 个字符，不能以「.」开头。").into());
    }
    id.starts_with("os.").then(|| i18n::t("Ids under “os.” are the system's own apps.", "「os.」开头的 ID 留给系统应用。").into())
}

/// A blank app: a page and the skeleton the templates share (its parts,
/// its manifest asking for nothing, a listing to fill in, an icon, the
/// design flow's AGENTS.md), and nothing else.
const BLANK: &[(&str, &str)] = &[
    ("app/parts/00-state.splash", "// 整个应用共用的东西：标题、状态和样式。\n// OctoBuddy 新建应用时会把下面这行换成你起的名字。\nlet app_title = \"我的应用\"\n\nlet ink = #x1c1c1e\nlet secondary = #x8e8e93\n"),
    ("app/parts/10-home.splash", "// 首页：它的模板和函数（id 以 home_ 开头）。\nlet HomePage = View{width: Fill height: Fill flow: Down spacing: 8 align: Align{x: 0.5 y: 0.45}\n    Label{text: \"这里还什么都没有\" draw_text.color: secondary draw_text.text_style.font_size: 15}\n}\n"),
    ("app/parts/90-root.splash", "// 启动和根视图：每个页面都在这里放进来。\nfn boot(){\n    // 打开时要做的事放在这里（ui 在这之后才能用）。\n}\nstart_timeout(0.05, || boot())\n\nSolidView{width: Fill height: Fill flow: Down padding: 20 spacing: 12 draw_bg.color: #xffffff\n    Label{text: app_title draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 28}}\n    home := HomePage{}\n}\n"),
    ("bundle/manifest.json", "{\n  \"schema\": 1,\n  \"id\": \"my-app\",\n  \"name\": \"My App\",\n  \"version\": \"0.1.0\",\n  \"integrity\": {\"bundle_blake3\": \"\"},\n  \"capabilities\": []\n}\n"),
    ("bundle/listing.json", "{\n  \"schema\": 1,\n  \"subtitle\": \"Replace with your app's short description\",\n  \"description\": \"Replace with what your app does and who it helps.\",\n  \"category\": \"utilities\",\n  \"keywords\": [],\n  \"screenshots\": [\"screenshots/01-main.png\"],\n  \"icon\": \"assets/icon.svg\",\n  \"platforms\": [\"android\"],\n  \"publisher\": {\n    \"name\": \"Replace with your publisher name\",\n    \"support\": \"https://example.com/support\",\n    \"privacy_policy_url\": \"https://example.com/privacy\"\n  },\n  \"release_notes\": \"Replace with the changes in this version.\",\n  \"age_rating\": \"all\",\n  \"license\": \"Apache-2.0\"\n}\n"),
    ("bundle/assets/icon.svg", BLANK_ICON),
    ("CLAUDE.md", include_str!("../../resources/design-flow/templates/script-app/CLAUDE.md")),
    ("GEMINI.md", include_str!("../../resources/design-flow/templates/script-app/GEMINI.md")),
    (".gitignore", include_str!("../../resources/design-flow/templates/script-app/.gitignore")),
];
const BLANK_ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
  <rect x="4" y="4" width="56" height="56" rx="14" fill="#636366"/>
  <path d="M32 21V43M21 32H43" stroke="#ffffff" stroke-width="5" stroke-linecap="round"/>
</svg>
"##;
const PARTS_NOTE: &str = "
## This app is built from parts

`bundle/main.splash` is generated: it is `app/parts/*.splash` put together in
name order. Edit the parts, never `bundle/main.splash`; OctoBuddy puts them
together. Each part owns one page or one concern and its ids start with that
page's name, so separate pages can be worked on at once.
";

fn write_blank(dest: &Path, name: &str) -> Result<(), String> {
    let write = |rel: &str, text: &str| -> Result<(), String> {
        let path = dest.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{rel}: {e}"))
    };
    for (rel, text) in BLANK {
        write(rel, text)?;
    }
    let agents = include_str!("../../resources/design-flow/templates/script-app/AGENTS.md");
    write("AGENTS.md", &format!("{}\n{PARTS_NOTE}", agents.trim_end()))?;
    write("README.md", &format!("# {}\n\n{}\n", name.trim(), i18n::t(
        "An OctoSense script app. Edit `app/parts/*.splash`; how to run, check and publish it: `AGENTS.md`.",
        "一个 OctoSense 脚本应用。改 `app/parts/*.splash`；怎么运行、检查和发布，见 `AGENTS.md`。")))
}

/// What the dialog makes.
pub struct Request<'a> {
    pub template: Option<&'a Template>,
    pub name: &'a str,
    pub id: &'a str,
    pub dest: &'a Path,
    pub caps: Vec<&'a str>,
    pub hosts: &'a str,
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(name.to_str(), Some(".git" | ".local-state" | ".DS_Store")) {
            continue;
        }
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&entry.path(), &to.join(&name))?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

/// The app's title in its parts: the line `let app_title = "…"` a template
/// keeps for it.
fn retitle(dest: &Path, name: &str) {
    let title = name.replace(['"', '\\', '\n', '\r'], "");
    for part in std::fs::read_dir(octosense_app::parts_dir(&dest.to_string_lossy())).into_iter().flatten().flatten() {
        let Ok(text) = std::fs::read_to_string(part.path()) else { continue };
        if !text.lines().any(|l| l.starts_with("let app_title = ")) {
            continue;
        }
        let out: Vec<String> = text.lines().map(|l| if l.starts_with("let app_title = ") { format!("let app_title = \"{title}\"") } else { l.to_string() }).collect();
        let _ = std::fs::write(part.path(), out.join("\n") + "\n");
    }
}

/// Makes the app at `r.dest` (absent or an empty folder): the template's
/// files with its name, id and grants, put together, stamped and committed.
pub fn create(r: &Request) -> Result<(), String> {
    if let Some(problem) = id_problem(r.id) {
        return Err(problem);
    }
    if r.name.trim().is_empty() {
        return Err(i18n::t("Give it a name.", "请填写应用名称。").into());
    }
    if r.dest.exists() && r.dest.read_dir().map_err(|e| e.to_string())?.next().is_some() {
        return Err(i18n::pick(format!("{} is not empty: choose another place or id.", r.dest.display()),
            format!("{} 不是空文件夹：换个位置或 ID。", r.dest.display())));
    }
    match r.template {
        Some(t) => copy_dir(&t.dir, r.dest).map_err(|e| e.to_string())?,
        None => write_blank(r.dest, r.name)?,
    }
    let dest = r.dest.to_string_lossy().into_owned();
    retitle(r.dest, r.name);
    let path = r.dest.join("bundle/manifest.json");
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    m["id"] = Value::from(r.id);
    m["name"] = Value::from(r.name.trim());
    m["version"] = Value::from("0.1.0");
    std::fs::write(&path, serde_json::to_string_pretty(&m).map_err(|e| e.to_string())? + "\n").map_err(|e| e.to_string())?;
    save_permissions(r.dest, &r.caps, r.hosts, "")?;
    if let Some(t) = r.template {
        let readme = r.dest.join("README.md");
        if let Ok(text) = std::fs::read_to_string(&readme) {
            let rest = text.split_once('\n').map(|(_, rest)| rest).unwrap_or("");
            let _ = std::fs::write(&readme, format!("# {}\n\n{}{rest}", r.name.trim(),
                i18n::pick(format!("Made from the “{}” template.\n", t.name()), format!("从模板「{}」创建。\n", t.name()))));
        }
    }
    octosense_app::assemble(&dest)?;
    // Its files changed: stamped again.
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(mut m) = serde_json::from_str::<Value>(&text) {
            m["integrity"]["bundle_blake3"] = Value::from("");
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&m).unwrap_or(text) + "\n");
        }
    }
    octosense_app::repair_digest(&dest);
    if !r.dest.join(".git").exists() {
        git(&["init", "-q"], r.dest)?;
    }
    // OctoBuddy's own files in it (`.octobuddy/`: the parts' line map…) stay out of git.
    crate::workspace::exclude_agent_files(&dest);
    git(&["add", "-A"], r.dest)?;
    let message = match r.template {
        Some(t) => format!("chore: 从模板「{}」创建应用", t.name.1),
        None => "chore: 新建空白应用".to_string(),
    };
    git(&["commit", "-q", "-m", &message], r.dest)
        .map_err(|e| i18n::pick(format!("the first commit failed: {e}"), format!("第一次提交失败：{e}")))
}

/// The dialog's state.
#[derive(Default)]
pub struct NewApp {
    pub open: bool,
    pub templates: Vec<Template>,
    /// The template picked (an index into `templates`); none: the blank app.
    pub picked: Option<usize>,
    pub syncing: bool,
    pub sync_error: Option<String>,
    /// The person typed the id: the name no longer makes it.
    pub id_typed: bool,
    pub parent: PathBuf,
    pub status: Option<String>,
    /// Picking the folder it goes in (the folder dialog is open).
    pub picking_dir: bool,
}

/// Where new apps go when the person never chose: `~/Projects`, else home.
fn default_parent() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    let projects = home.join("Projects");
    if projects.is_dir() { projects } else { home }
}

impl OctoBuddyView {
    /// The dialog opened: the templates kept, then fresh ones from the repository.
    pub(crate) fn open_new_app(&mut self, cx: &mut Cx) {
        let parent = self.store.new_app_dir.as_ref().map(PathBuf::from).filter(|p| p.is_dir()).unwrap_or_else(default_parent);
        self.new_app = NewApp { open: true, templates: list(), parent, syncing: true, ..Default::default() };
        self.new_app_defaults(cx);
        let inbox = self.rt.inbox.clone();
        std::thread::spawn(move || events::post(&inbox, LoopEvent::TemplatesSynced(sync())));
        self.relayout(cx);
        self.view.text_input(cx, ids!(na_name)).set_key_focus(cx);
    }

    pub(crate) fn templates_synced(&mut self, result: Result<(), String>) {
        let app = &mut self.new_app;
        app.syncing = false;
        app.sync_error = result.err();
        if !app.open {
            return;
        }
        let picked_id = app.picked.and_then(|i| app.templates.get(i)).map(|t| t.id.clone());
        app.templates = list();
        app.picked = picked_id.and_then(|id| app.templates.iter().position(|t| t.id == id));
        self.new_app_reset = true;
    }

    /// The form as the picked template has it: its name, id, grants and hosts.
    fn new_app_defaults(&mut self, cx: &mut Cx) {
        let t = self.new_app.picked.and_then(|i| self.new_app.templates.get(i)).cloned();
        let name = t.as_ref().map(|t| t.name().to_string()).unwrap_or_else(|| i18n::t("My App", "我的应用").into());
        self.view.text_input(cx, ids!(na_name)).set_text(cx, &name);
        let id = t.as_ref().map(|t| t.app_id.clone()).unwrap_or_else(|| "my-app".into());
        self.view.text_input(cx, ids!(na_id)).set_text(cx, &id);
        self.new_app.id_typed = false;
        // The blank app asks for nothing until a screen needs it.
        let caps: Vec<String> = t.as_ref().map(|t| t.capabilities.clone()).unwrap_or_default();
        for ((cap, ..), slot) in CAPS.iter().zip(CAP_BOXES) {
            self.view.check_box(cx, &[slot]).set_active(cx, caps.iter().any(|c| c == cap), Animate::No);
        }
        let hosts = t.as_ref().map(|t| t.hosts.join(", ")).unwrap_or_default();
        self.view.text_input(cx, ids!(na_hosts)).set_text(cx, &hosts);
        self.new_app.status = None;
    }

    fn new_app_dest(&self, cx: &mut Cx) -> PathBuf {
        let id = self.view.text_input(cx, ids!(na_id)).text();
        self.new_app.parent.join(id.trim())
    }

    /// The dialog, as its state says.
    pub(crate) fn sync_new_app(&mut self, cx: &mut Cx) {
        self.view.view(cx, ids!(new_app_layer)).set_visible(cx, self.new_app.open);
        if !self.new_app.open {
            return;
        }
        if std::mem::take(&mut self.new_app_reset) {
            self.new_app_defaults(cx);
        }
        use i18n::t;
        self.view.label(cx, ids!(na_title)).set_text(cx, t("New OctoSense app", "新建 OctoSense 应用"));
        self.view.label(cx, ids!(na_tpl_label)).set_text(cx, t("Template", "模板"));
        let where_from = TEMPLATES_REPO.trim_start_matches("https://").trim_end_matches(".git");
        let note = match (&self.new_app, self.new_app.templates.is_empty()) {
            (a, true) if a.syncing => t("Fetching the templates…", "正在获取模板…").to_string(),
            (a, true) => i18n::pick(format!("Could not fetch the templates ({}): a blank app can be made now.", a.sync_error.as_deref().unwrap_or("?")),
                format!("没拿到模板（{}）：可以先建空白应用。", a.sync_error.as_deref().unwrap_or("?"))),
            (a, false) if a.syncing => i18n::pick(format!("From {where_from} · checking for newer ones…"), format!("来自 {where_from} · 正在检查更新…")),
            (a, false) if a.sync_error.is_some() => i18n::pick(format!("From {where_from} · offline: the copy fetched before."), format!("来自 {where_from} · 离线：用上次获取的。")),
            _ => i18n::pick(format!("From {where_from} · up to date."), format!("来自 {where_from} · 已是最新。")),
        };
        self.view.label(cx, ids!(na_tpl_note)).set_text(cx, &note);
        let (accent, line, soft, raised) = (crate::theme::hex("accent"), crate::theme::hex("line"), crate::theme::hex("accent_soft"), crate::theme::hex("raised"));
        // The first card is the blank app; the templates follow.
        for (i, slot) in CARDS.iter().enumerate() {
            let card = self.view.view(cx, &[*slot]);
            let template = i.checked_sub(1);
            let shown = template.is_none_or(|j| j < self.new_app.templates.len());
            card.set_visible(cx, shown);
            if !shown {
                continue;
            }
            let chosen = self.new_app.picked == template;
            match template.and_then(|j| self.new_app.templates.get(j)) {
                Some(tpl) => {
                    card.label(cx, ids!(card.name)).set_text(cx, tpl.name());
                    card.label(cx, ids!(card.summary)).set_text(cx, tpl.summary());
                    card.label(cx, ids!(card.features)).set_text(cx, &tpl.features());
                    let svg = tpl.icon.unwrap_or(include_str!("../../resources/octos.svg"));
                    crate::provider_icons::show(&card.widget(cx, ids!(card.icon)), svg, &mut self.icons_shown);
                }
                None => {
                    card.label(cx, ids!(card.name)).set_text(cx, t("Blank app", "空白应用"));
                    card.label(cx, ids!(card.summary)).set_text(cx, t("An empty page and the skeleton every app shares (parts, manifest, listing, icon). It asks for no permission: start from nothing.",
                        "一个空页面，加上每个应用都有的骨架（parts、manifest、listing、图标）。不申请任何权限，从零开始。"));
                    card.label(cx, ids!(card.features)).set_text(cx, t("No permissions · Works offline", "不申请权限 · 离线可用"));
                    crate::provider_icons::show(&card.widget(cx, ids!(card.icon)), BLANK_ICON, &mut self.icons_shown);
                }
            }
            let (bg, border, size) = if chosen { (soft, accent, 2.0) } else { (raised, line, 1.0) };
            let (bg, border) = (crate::hex_color(bg), crate::hex_color(border));
            let mut w = card.widget(cx, ids!(card));
            script_apply_eval!(cx, w, { draw_bg +: {color: #(bg) border_color: #(border) border_size: #(size)} });
        }
        self.view.label(cx, ids!(na_name_label)).set_text(cx, t("Name", "应用名称"));
        self.view.label(cx, ids!(na_id_label)).set_text(cx, t("Id (in the store, never changes)", "应用 ID（商店里用，之后不能改）"));
        self.view.label(cx, ids!(na_where_label)).set_text(cx, t("Location", "保存位置"));
        let dest = self.new_app_dest(cx);
        let home = std::env::var("HOME").unwrap_or_default();
        let shown = dest.display().to_string();
        let shown = match shown.strip_prefix(&home) { Some(rest) if !home.is_empty() => format!("~{rest}"), _ => shown };
        self.view.label(cx, ids!(na_where)).set_text(cx, &shown);
        self.view.button(cx, ids!(na_pick_dir)).set_text(cx, t("Choose…", "选择…"));
        self.view.label(cx, ids!(na_perm_label)).set_text(cx, t("Permissions", "权限"));
        self.view.label(cx, ids!(na_perm_note)).set_text(cx, t(
            "The store tells people each one before they install it. Tick only what a screen uses; you can change them later in the workbench.",
            "商店会在安装前把每一项讲给用户。只勾选界面真正用得到的；之后也能在工作台里改。"));
        for ((cap, _, en, zh, ..), slot) in CAPS.iter().zip(CAP_BOXES) {
            self.view.check_box(cx, &[slot]).set_text(&format!("{}  {cap}", t(en, zh)));
        }
        self.view.label(cx, ids!(na_hosts_label)).set_text(cx, t("Hosts it reaches (with Network)", "可访问的域名（需勾选「网络」）"));
        self.view.button(cx, ids!(na_cancel)).set_text(cx, t("Cancel", "取消"));
        self.view.button(cx, ids!(na_create)).set_text(cx, t("Create", "创建"));
        let status = self.new_app.status.clone().unwrap_or_default();
        self.view.label(cx, ids!(na_status)).set_text(cx, &status);
    }

    pub(crate) fn new_app_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if !self.new_app.open {
            return;
        }
        if self.view.button(cx, ids!(na_cancel)).clicked(actions) {
            self.new_app.open = false;
            self.relayout(cx);
            return;
        }
        for (i, slot) in CARDS.iter().enumerate() {
            let template = i.checked_sub(1).filter(|j| *j < self.new_app.templates.len());
            if crate::tapped(&self.view.view(cx, &[*slot, live_id!(card)]), actions) && (i == 0 || template.is_some()) && self.new_app.picked != template {
                self.new_app.picked = template;
                self.new_app_defaults(cx);
                self.relayout(cx);
            }
        }
        if let Some(name) = self.view.text_input(cx, ids!(na_name)).changed(actions) {
            if !self.new_app.id_typed {
                let fallback = self.new_app.picked.and_then(|i| self.new_app.templates.get(i)).map(|t| t.app_id.clone()).unwrap_or_else(|| "my-app".into());
                let id = Some(id_from(&name)).filter(|id| !id.is_empty()).unwrap_or(fallback);
                self.view.text_input(cx, ids!(na_id)).set_text(cx, &id);
            }
            self.relayout(cx);
        }
        if self.view.text_input(cx, ids!(na_id)).changed(actions).is_some() {
            self.new_app.id_typed = true;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(na_pick_dir)).clicked(actions) {
            self.new_app.picking_dir = true;
            let dialog = FileDialog::new().set_title(i18n::t("Choose where the new app goes", "选择新应用放在哪个文件夹").into())
                .set_location(self.new_app.parent.clone());
            cx.open_select_folder_dialog(dialog);
        }
        if self.view.button(cx, ids!(na_create)).clicked(actions) {
            self.create_new_app(cx);
        }
    }

    /// The folder dialog answered for the dialog's location.
    pub(crate) fn new_app_dir_picked(&mut self, cx: &mut Cx, dir: Option<&Path>) {
        self.new_app.picking_dir = false;
        if let Some(dir) = dir {
            self.new_app.parent = dir.to_path_buf();
            self.relayout(cx);
        }
    }

    fn create_new_app(&mut self, cx: &mut Cx) {
        let name = self.view.text_input(cx, ids!(na_name)).text();
        let id = self.view.text_input(cx, ids!(na_id)).text().trim().to_string();
        let hosts = self.view.text_input(cx, ids!(na_hosts)).text();
        let caps: Vec<&str> = CAPS.iter().zip(CAP_BOXES).filter(|(_, slot)| self.view.check_box(cx, &[*slot]).active(cx)).map(|((cap, ..), _)| *cap).collect();
        let dest = self.new_app.parent.join(&id);
        let template = self.new_app.picked.and_then(|i| self.new_app.templates.get(i)).cloned();
        let request = Request { template: template.as_ref(), name: &name, id: &id, dest: &dest, caps, hosts: &hosts };
        let existed = dest.exists();
        match create(&request) {
            Ok(()) => {
                self.store.new_app_dir = Some(self.new_app.parent.to_string_lossy().into_owned());
                self.new_app.open = false;
                self.add_app(cx, &dest);
                // Listed by the app's name, not its folder's.
                let path = dest.to_string_lossy();
                if let Some(p) = self.store.projects.iter_mut().find(|p| p.path == path.trim_end_matches('/')) {
                    p.name = name.trim().to_string();
                }
                self.save();
                self.relayout(cx);
            }
            Err(e) => {
                // A half-made app is not left behind: only a folder this made.
                if !existed && dest.exists() {
                    let _ = std::fs::remove_dir_all(&dest);
                }
                self.new_app.status = Some(format!("✗ {e}"));
                self.relayout(cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_makes_an_id_the_store_takes() {
        assert_eq!(id_from("Pocket Ledger 2"), "pocket-ledger-2");
        assert_eq!(id_from("  天气 Weather!! "), "weather");
        assert_eq!(id_from("资讯"), "");
        assert_eq!(id_from("a..b"), "a..b");
        assert!(id_problem("my-app").is_none());
        assert!(id_problem("a..b").is_some());
        assert!(id_problem(".x").is_some());
        assert!(id_problem("Big").is_some());
        assert!(id_problem("os.camera").is_some());
        assert!(id_problem("").is_some());
    }

    #[test]
    fn a_blank_app_is_made_offline_and_asks_for_nothing() {
        let dest = std::env::temp_dir().join(format!("octobuddy-blank-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        create(&Request { template: None, name: "空的", id: "blank-test", dest: &dest, caps: vec![], hosts: "" }).unwrap();
        let m: Value = serde_json::from_str(&std::fs::read_to_string(dest.join("bundle/manifest.json")).unwrap()).unwrap();
        assert_eq!((m["id"].as_str(), m["name"].as_str()), (Some("blank-test"), Some("空的")));
        assert_eq!(m["capabilities"], serde_json::json!([]));
        assert_eq!(m["integrity"]["bundle_blake3"].as_str().unwrap().len(), 64);
        let main = std::fs::read_to_string(dest.join("bundle/main.splash")).unwrap();
        assert!(main.contains("let app_title = \"空的\"") && main.contains("home := HomePage{}"), "{main}");
        assert!(std::fs::read_to_string(dest.join("AGENTS.md")).unwrap().contains("built from parts"));
        assert!(dest.join(".git").is_dir() && dest.join("bundle/assets/icon.svg").is_file());
        let tracked = Command::new("git").args(["ls-files"]).current_dir(&dest).output().unwrap();
        assert!(!String::from_utf8_lossy(&tracked.stdout).contains(".octobuddy"), "OctoBuddy's own files are not committed");
        let _ = std::fs::remove_dir_all(&dest);
    }

    /// Makes an app from the templates kept in `OCTOBUDDY_HOME` (fetch them
    /// first: `sync`), into a scratch folder.
    #[test]
    #[ignore]
    fn makes_an_app_from_each_template() {
        assert!(std::env::var_os("OCTOBUDDY_HOME").is_some(), "only with a scratch OCTOBUDDY_HOME");
        sync().unwrap();
        let templates = list();
        assert!(!templates.is_empty());
        let scratch = std::env::temp_dir().join(format!("octobuddy-new-app-{}", std::process::id()));
        for t in &templates {
            let dest = scratch.join(&t.app_id);
            let caps: Vec<&str> = t.capabilities.iter().map(String::as_str).collect();
            create(&Request { template: Some(t), name: "测试应用", id: &t.app_id, dest: &dest, caps, hosts: &t.hosts.join(",") }).unwrap();
            let m: Value = serde_json::from_str(&std::fs::read_to_string(dest.join("bundle/manifest.json")).unwrap()).unwrap();
            assert_eq!(m["name"], "测试应用");
            assert_eq!(m["integrity"]["bundle_blake3"].as_str().unwrap().len(), 64);
            let main = std::fs::read_to_string(dest.join("bundle/main.splash")).unwrap();
            assert!(main.contains("let app_title = \"测试应用\""), "{}", t.id);
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }
}
