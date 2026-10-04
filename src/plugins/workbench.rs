//! The app workbench, beside the conversation (the OctoSense app plugin):
//! everything about an app project in one place. Preview runs the app as
//! it is built (`app_preview.rs`); Files shows the project's folder and the
//! source of a file picked; Permissions edits what the manifest asks the
//! person for (its capabilities, the hosts it reaches, its storage); App
//! Hub gets it to the store (`app_hub.rs` on the host, the local App Hub
//! inside OctoSense).
use crate::events::{self, LoopEvent};
use crate::plugins::app_hub::{self, Listing};
use crate::{i18n, system, tapped, OctoBuddyView};
use makepad_widgets::*;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The workbench's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Preview,
    Files,
    Permissions,
    Hub,
}

const TABS: [(Tab, LiveId, LiveId, LiveId); 4] = [
    (Tab::Preview, live_id!(wb_t0), live_id!(wb_t0_on), live_id!(wb_preview)),
    (Tab::Files, live_id!(wb_t1), live_id!(wb_t1_on), live_id!(wb_files)),
    (Tab::Permissions, live_id!(wb_t2), live_id!(wb_t2_on), live_id!(wb_perms)),
    (Tab::Hub, live_id!(wb_t3), live_id!(wb_t3_on), live_id!(wb_hub)),
];

/// The capabilities a store app can ask for today (OctoScript App Design
/// Flow's CAPABILITIES), each with the line the person reads before
/// installing. The rest serve system apps or no shell yet.
const CAPS: [(&str, LiveId, &str, &str, &str, &str); 9] = [
    ("storage", live_id!(cap_storage), "Storage", "存储", "Keep its own data on this device, where only it can read it.", "在本机保存自己的数据，只有它能读。"),
    ("net", live_id!(cap_net), "Network", "网络", "Reach only the hosts listed below.", "只访问下面列出的域名。"),
    ("images", live_id!(cap_images), "Pictures from the web", "网络图片", "Show pictures from any public https website.", "显示任意公开 https 网站的图片。"),
    ("web", live_id!(cap_web), "Web pages", "网页", "Open web pages in a browser view, which cannot reach back into the app.", "在浏览器视图中打开网页，网页无法反过来访问应用。"),
    ("camera", live_id!(cap_camera), "Camera", "相机", "Use the camera (the system still asks).", "使用相机（系统仍会询问）。"),
    ("microphone", live_id!(cap_microphone), "Microphone", "麦克风", "Record sound with videos (with the camera).", "录像时录音（需与相机一起）。"),
    ("library", live_id!(cap_library), "Photo library", "相册", "Save photos and videos to the photo library, where other apps can see them.", "把照片和视频存进相册，其他应用也能看到。"),
    ("location", live_id!(cap_location), "Location", "定位", "Use your location (the system still asks).", "使用你的位置（系统仍会询问）。"),
    ("mail", live_id!(cap_mail), "Mail", "邮件", "Read and send mail from accounts you add on the device (OctoSense's desktop and Home only).", "读取和发送你在设备上添加的邮箱的邮件（仅 OctoSense 桌面版和 Home 提供）。"),
];

/// A step of the App Hub tab that runs off the UI thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HubStep {
    /// Screenshots, the gate, the review packet.
    Prepare,
    /// What Submit will do (nothing done).
    Plan,
    Submit,
    /// Whether GitHub's gh is signed in.
    Gh,
}

/// The platforms' ticks, in `app_hub::PLATFORMS`' order.
const PLATS: [LiveId; 7] = [live_id!(plat_macos), live_id!(plat_ios), live_id!(plat_android), live_id!(plat_windows),
    live_id!(plat_linux), live_id!(plat_openharmony), live_id!(plat_web)];

/// What the workbench shows and keeps between frames.
#[derive(Default)]
pub struct Workbench {
    pub tab: Tab,
    root: Option<PathBuf>,
    /// The folders opened in the tree.
    open: BTreeSet<PathBuf>,
    /// The rows: path, depth, a folder.
    rows: Vec<(PathBuf, usize, bool)>,
    shown: Option<PathBuf>,
    /// The project whose manifest the Permissions form holds.
    perms_for: Option<PathBuf>,
    perms_status: Option<Result<String, String>>,
    /// The project whose listing the App Hub form holds.
    hub_for: Option<PathBuf>,
    /// The step running now.
    hub_busy: Option<HubStep>,
    hub_status: Option<Result<String, String>>,
    /// The gate's output, from the last check.
    hub_check: String,
    /// What Submit will do, shown for the person to confirm.
    hub_plan: Option<String>,
    /// GitHub's gh: signed in (as whom), or why not.
    gh: Option<Result<String, String>>,
    /// What the tab says about the tools, the repository and the last
    /// submission (read when the form is: not on every frame).
    hub_env: String,
}

/// What the tree leaves out: version control, build output, the OS's.
fn skipped(name: &str) -> bool {
    matches!(name, ".git" | ".DS_Store" | "target" | "node_modules" | "dist" | "run" | ".local-state")
}

/// The tree's rows under `root`, the open folders opened.
fn tree(root: &Path, open: &BTreeSet<PathBuf>) -> Vec<(PathBuf, usize, bool)> {
    fn walk(dir: &Path, depth: usize, open: &BTreeSet<PathBuf>, out: &mut Vec<(PathBuf, usize, bool)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<(PathBuf, bool)> = entries.flatten()
            .filter(|e| !skipped(&e.file_name().to_string_lossy()))
            .map(|e| (e.path(), e.path().is_dir())).collect();
        entries.sort_by_key(|(p, d)| (!d, p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default()));
        for (path, dir) in entries {
            if out.len() >= 2000 {
                return;
            }
            out.push((path.clone(), depth, dir));
            if dir && open.contains(&path) {
                walk(&path, depth + 1, open, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, 0, open, &mut out);
    out
}

/// A file's text as the source view shows it: numbered lines, or why not.
fn source_text(path: &Path) -> String {
    let Ok(meta) = std::fs::metadata(path) else { return String::new() };
    if meta.len() > 400_000 {
        return i18n::pick(format!("(too big to show here: {} KB)", meta.len() / 1024), format!("（文件太大，这里不显示：{} KB）", meta.len() / 1024));
    }
    let Ok(bytes) = std::fs::read(path) else { return String::new() };
    let Ok(text) = String::from_utf8(bytes) else {
        return i18n::pick(format!("(a binary file, {} bytes)", meta.len()), format!("（二进制文件，{} 字节）", meta.len()));
    };
    let lines: Vec<&str> = text.lines().collect();
    let width = lines.len().max(1).to_string().len();
    let mut out: String = lines.iter().take(4000).enumerate()
        .map(|(i, l)| format!("{:>width$}  {}\n", i + 1, l.replace('\t', "    ")))
        .collect();
    if lines.len() > 4000 {
        out.push_str(&i18n::pick(format!("… {} more lines\n", lines.len() - 4000), format!("…… 还有 {} 行\n", lines.len() - 4000)));
    }
    out
}

/// A bare, exact, lowercase host name (no scheme, path, port or wildcard).
fn bare_host(h: &str) -> bool {
    !h.is_empty() && h.contains('.') && h.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
        && !h.starts_with('.') && !h.ends_with('.')
}

/// Writes the form's permissions into `project`'s manifest: the
/// capabilities ticked (any others it had are kept), its hosts, its storage.
pub fn save_permissions(project: &Path, ticked: &[&str], hosts: &str, storage_mb: &str) -> Result<String, String> {
    let path = project.join("bundle/manifest.json");
    let mut m: Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok())
        .ok_or("bundle/manifest.json is missing or not JSON")?;
    let hosts: Vec<String> = hosts.split([',', ' ', '\n', '\t']).map(str::trim).filter(|h| !h.is_empty()).map(|h| h.to_ascii_lowercase()).collect();
    if let Some(bad) = hosts.iter().find(|h| !bare_host(h)) {
        return Err(i18n::pick(format!("\"{bad}\" is not a bare host name (like api.example.org: no https://, path, port or *)."),
            format!("「{bad}」不是纯域名（应写成 api.example.org 这样：不带 https://、路径、端口或 *）。")));
    }
    if !hosts.is_empty() && !ticked.contains(&"net") {
        return Err(i18n::t("Hosts need Network ticked.", "列了域名就要勾选「网络」。").into());
    }
    let storage = storage_mb.trim();
    let max_bytes = if storage.is_empty() { None } else {
        let mb: f64 = storage.parse().map_err(|_| i18n::t("Storage is a number of MB.", "存储上限填 MB 数。").to_string())?;
        Some(((mb.clamp(0.0, 16.0)) * 1024.0 * 1024.0) as u64)
    };
    let others: Vec<String> = m["capabilities"].as_array().into_iter().flatten().filter_map(|c| c.as_str())
        .filter(|c| !CAPS.iter().any(|(id, ..)| id == c)).map(String::from).collect();
    let caps: Vec<String> = CAPS.iter().map(|(id, ..)| *id).filter(|id| ticked.contains(id)).map(String::from).chain(others).collect();
    m["capabilities"] = json!(caps);
    m["network"] = json!({"hosts": hosts});
    match max_bytes {
        Some(b) => m["storage"] = json!({"max_bytes": b}),
        None => {
            if let Some(o) = m.as_object_mut() {
                o.remove("storage");
            }
        }
    }
    std::fs::write(&path, serde_json::to_string_pretty(&m).map_err(|e| e.to_string())? + "\n").map_err(|e| e.to_string())?;
    Ok(i18n::pick(format!("Saved: {}.", if caps.is_empty() { "no capabilities".to_string() } else { caps.join(", ") }),
        format!("已保存：{}。", if caps.is_empty() { "不要任何能力".to_string() } else { caps.join("、") })))
}

/// What the App Hub tab says about its tools, the app's repository and its
/// last submission (a line each).
fn hub_env(project: &Path) -> String {
    let tools = match crate::plugins::octosense_app::tools() {
        Ok(t) if t.app_hub.is_some() => i18n::t("App Hub's hub and card-host: found.", "App Hub 的 hub 和 card-host：已找到。").to_string(),
        _ => i18n::t("App Hub's hub and card-host were not found (build OctoSense-App-Hub, or set OCTOSENSE_APP_HUB): Screenshots & check needs them.",
            "没找到 App Hub 的 hub 和 card-host（编译 OctoSense-App-Hub，或设置 OCTOSENSE_APP_HUB）：截图并检查要用到它们。").to_string(),
    };
    let remote = std::process::Command::new("git").args(["remote", "get-url", "origin"]).current_dir(project).output().ok()
        .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let repo = match remote {
        Some(r) => i18n::pick(format!("Its repository: {r}."), format!("应用仓库：{r}。")),
        None => i18n::t("No repository yet: Submit makes a public one on your GitHub.", "还没有仓库：提交时会在你的 GitHub 上建一个公开仓库。").to_string(),
    };
    let last = app_hub::last_submission(project).map(|s| i18n::pick(format!("\nSubmitted {}: {}", s.version, s.issue), format!("\n已提交 {}：{}", s.version, s.issue))).unwrap_or_default();
    format!("{tools}\n{repo}{last}")
}

impl OctoBuddyView {
    /// The workbench's tabs and the one shown, for the app project `project`.
    pub(crate) fn sync_workbench(&mut self, cx: &mut Cx, project: Option<&str>) {
        let t = |en: &'static str, zh: &'static str| i18n::t(en, zh);
        let names = [t("Preview", "预览"), t("Files", "文件"), t("Permissions", "权限"), t("App Hub", "上架")];
        for ((tab, off, on, pane), name) in TABS.iter().zip(names) {
            let shown = self.wb.tab == *tab;
            self.view.button(cx, &[*off]).set_visible(cx, !shown);
            self.view.button(cx, &[*on]).set_visible(cx, shown);
            self.view.button(cx, &[*off]).set_text(cx, name);
            self.view.button(cx, &[*on]).set_text(cx, name);
            self.view.view(cx, &[*pane]).set_visible(cx, shown);
        }
        let Some(project) = project.map(PathBuf::from) else { return };
        match self.wb.tab {
            Tab::Files => self.sync_files(cx, &project),
            Tab::Permissions => self.sync_permissions(cx, &project),
            Tab::Hub => self.sync_hub(cx, &project),
            Tab::Preview => {}
        }
    }

    fn sync_files(&mut self, cx: &mut Cx, project: &Path) {
        if self.wb.root.as_deref() != Some(project) {
            self.wb.root = Some(project.to_path_buf());
            self.wb.open = [project.join("bundle"), project.join("app"), project.join("app/parts")].into_iter().filter(|p| p.is_dir()).collect();
            self.wb.shown = [project.join("bundle/main.splash")].into_iter().find(|p| p.is_file());
        }
        self.wb.rows = tree(project, &self.wb.open);
        self.view.label(cx, ids!(files_root)).set_text(cx, &project.display().to_string());
        self.view.button(cx, ids!(files_reveal)).set_text(cx, i18n::t("Show in Finder", "在访达中显示"));
        let (title, text) = match &self.wb.shown {
            Some(p) => (p.strip_prefix(project).unwrap_or(p).display().to_string(), source_text(p)),
            None => (String::new(), i18n::t("Pick a file above to see its source.", "在上面选一个文件查看源码。").to_string()),
        };
        self.view.label(cx, ids!(source_path)).set_text(cx, &title);
        self.view.label(cx, ids!(source_text)).set_text(cx, &text);
        self.view.portal_list(cx, ids!(files_list)).redraw(cx);
    }

    /// The Files tab's tree.
    pub(crate) fn draw_files(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        list.set_item_range(cx, 0, self.wb.rows.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some((path, depth, dir)) = self.wb.rows.get(index).cloned() else { continue };
            let item = list.item(cx, index, id!(File));
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mark = if !dir { "  " } else if self.wb.open.contains(&path) { "▾ " } else { "▸ " };
            let text = format!("{}{mark}{name}{}", "    ".repeat(depth), if dir { "/" } else { "" });
            item.label(cx, ids!(name)).set_text(cx, &text);
            let picked = self.wb.shown.as_deref() == Some(path.as_path());
            let mut row = item.view(cx, ids!(row));
            let bg = if picked { crate::theme::vec4("accent_soft") } else { vec4(0.0, 0.0, 0.0, 0.0) };
            script_apply_eval!(cx, row, { draw_bg +: {color: #(bg)} });
            item.draw_all_unscoped(cx);
        }
    }

    fn sync_permissions(&mut self, cx: &mut Cx, project: &Path) {
        self.view.label(cx, ids!(perms_hint)).set_text(cx, i18n::t(
            "What the app asks the person for before it is installed: one line each in the store. Ask for the least it needs; App Hub's reviewer asks about any grant nothing on screen needs.",
            "应用安装前要向用户申请的权限：商店里每项显示一行。只申请它真正用到的；App Hub 审核时会问到任何界面上用不到的权限。"));
        self.view.label(cx, ids!(perms_hosts_label)).set_text(cx, i18n::t("Hosts it reaches (with Network)", "可访问的域名（需勾选「网络」）"));
        self.view.label(cx, ids!(perms_storage_label)).set_text(cx, i18n::t("Storage limit, MB (16 at most; empty: the most)", "存储上限，MB（最多 16；留空即最大）"));
        self.view.button(cx, ids!(perms_save)).set_text(cx, i18n::t("Save to manifest", "保存到 manifest"));
        let status = self.wb.perms_status.as_ref().map(|r| match r { Ok(s) => format!("✓ {s}"), Err(e) => format!("✗ {e}") }).unwrap_or_default();
        self.view.label(cx, ids!(perms_status)).set_text(cx, &status);
        // The form from the manifest: when it is another project's, or after a save.
        if self.wb.perms_for.as_deref() == Some(project) {
            return;
        }
        self.wb.perms_for = Some(project.to_path_buf());
        let m: Value = std::fs::read_to_string(project.join("bundle/manifest.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let has: Vec<String> = m["capabilities"].as_array().into_iter().flatten().filter_map(|c| c.as_str().map(String::from)).collect();
        for (id, widget, en, zh, note_en, note_zh) in CAPS {
            let row = self.view.view(cx, &[widget]);
            let check = row.check_box(cx, ids!(check));
            check.set_text(&format!("{}  {id}", i18n::t(en, zh)));
            check.set_active(cx, has.iter().any(|c| c == id), Animate::No);
            row.label(cx, ids!(note)).set_text(cx, i18n::t(note_en, note_zh));
        }
        let others: Vec<&String> = has.iter().filter(|c| !CAPS.iter().any(|(id, ..)| id == c)).collect();
        let other = self.view.label(cx, ids!(perms_other));
        other.set_visible(cx, !others.is_empty());
        other.set_text(cx, &i18n::pick(
            format!("It also asks for {}: a store app gains nothing from these today (they serve OctoSense's own apps, or no device yet). They are kept as they are; remove them in the manifest unless you mean them.",
                others.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
            format!("它还申请了 {}：商店应用目前用不上（只给 OctoSense 自带应用用，或还没有设备提供）。保存时原样保留；除非确实需要，请在 manifest 里去掉。",
                others.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("、"))));
        let hosts: Vec<String> = m["network"]["hosts"].as_array().into_iter().flatten().filter_map(|h| h.as_str().map(String::from)).collect();
        self.view.text_input(cx, ids!(perms_hosts)).set_text(cx, &hosts.join(", "));
        let mb = m["storage"]["max_bytes"].as_u64().map(|b| format!("{}", (b as f64 / 1024.0 / 1024.0 * 10.0).round() / 10.0)).unwrap_or_default();
        self.view.text_input(cx, ids!(perms_storage)).set_text(cx, &mb);
    }

    fn sync_hub(&mut self, cx: &mut Cx, project: &Path) {
        let hosted = system::hosted();
        self.view.view(cx, ids!(hub_local)).set_visible(cx, hosted);
        self.view.view(cx, ids!(hub_form)).set_visible(cx, !hosted);
        let status = self.wb.hub_status.as_ref().map(|r| match r { Ok(s) => format!("✓ {s}"), Err(e) => format!("✗ {e}") }).unwrap_or_default();
        let busy = self.wb.hub_busy.map(|s| match s {
            HubStep::Prepare => i18n::t("Taking screenshots and checking…", "正在截图并检查……"),
            HubStep::Plan => i18n::t("Working out what Submit does…", "正在列出提交要做的事……"),
            HubStep::Submit => i18n::t("Submitting…", "正在提交……"),
            HubStep::Gh => "",
        }).filter(|s| !s.is_empty());
        self.view.label(cx, ids!(hub_status)).set_text(cx, busy.unwrap_or(&status));
        if hosted {
            // Inside OctoSense: the local App Hub, where OctoSense installs it from.
            self.view.label(cx, ids!(hub_hint)).set_text(cx, i18n::t(
                "Publishes a signed copy to this device's App Hub (screenshots taken, App Hub's gate run); then install and run it from OctoSense's App Hub.",
                "把签名副本发布到本机 App Hub（自动补截图、跑 App Hub 的检查），之后在 OctoSense 的 App Hub 里安装运行。"));
            self.view.button(cx, ids!(hub_publish)).set_text(cx, if self.publishing.is_some() { i18n::t("Publishing…", "发布中……") } else { i18n::t("Publish to this device's App Hub", "发布到本机 App Hub") });
            return;
        }
        // On the host: who and what it needs, then the form.
        if self.wb.gh.is_none() && self.wb.hub_busy.is_none() {
            self.wb.hub_busy = Some(HubStep::Gh);
            let (inbox, project) = (self.rt.inbox.clone(), project.to_string_lossy().into_owned());
            std::thread::spawn(move || {
                let out = std::process::Command::new("gh").args(["api", "user", "--jq", ".login"]).env("PATH", crate::workspace::search_path()).output();
                let result = match out {
                    Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).trim().to_string()),
                    Ok(_) => Err(i18n::t("GitHub's gh is not signed in: run gh auth login", "GitHub 的 gh 还没登录：在终端运行 gh auth login").to_string()),
                    Err(_) => Err(i18n::t("GitHub's gh is not installed (brew install gh): Submit needs it", "没装 GitHub 的 gh（brew install gh）：提交要用到它").to_string()),
                };
                events::post(&inbox, LoopEvent::HubDone { project, step: HubStep::Gh, result });
            });
        }
        if self.wb.hub_for.as_deref() != Some(project) {
            self.wb.hub_env = hub_env(project);
        }
        let env = self.wb.hub_env.clone();
        let gh = match &self.wb.gh {
            Some(Ok(who)) => i18n::pick(format!("GitHub: signed in as {who}."), format!("GitHub：已登录 {who}。")),
            Some(Err(e)) => e.clone(),
            None => i18n::t("GitHub: checking…", "GitHub：检查中……").to_string(),
        };
        self.view.label(cx, ids!(hub_hint)).set_text(cx, &i18n::pick(
            format!("To App Hub, the way its maintainers take apps: fill in what the store shows and who publishes it, take its screenshots and check it, answer the review questions, then submit. Submit pushes the app to its public repository and opens a \"Submit\" issue in {}; App Hub's maintainers review it.\n{gh}\n{env}", app_hub::hub_repo()),
            format!("按 App Hub 维护者接收应用的方式上架：填好商店展示的信息和发布者，截图并检查，回答审查问题，然后提交。提交会把应用推送到它的公开仓库，并在 {} 开一个「Submit」issue，由 App Hub 维护者审核。\n{gh}\n{env}", app_hub::hub_repo())));
        let t = |en: &'static str, zh: &'static str| i18n::t(en, zh);
        for (id, text) in [
            (ids!(hub_sec_app), t("What the store shows", "商店展示的信息")),
            (ids!(hub_name_l), t("Name", "名称")), (ids!(hub_version_l), t("Version", "版本")), (ids!(hub_category_l), t("Category", "分类")),
            (ids!(hub_subtitle_l), t("Subtitle (one line, ≤ 80)", "副标题（一行，≤ 80 字）")),
            (ids!(hub_description_l), t("Description (what it does, truthfully)", "描述（如实写它做什么）")),
            (ids!(hub_keywords_l), t("Keywords (comma-separated, ≤ 10)", "关键词（逗号分隔，≤ 10 个）")),
            (ids!(hub_age_l), t("Age rating", "年龄分级")), (ids!(hub_license_l), t("License", "许可证")),
            (ids!(hub_notes_l), t("Release notes", "更新说明")),
            (ids!(hub_platforms_l), t("Platforms (only those it ran on: card-host on a Mac is macOS)", "平台（只勾实际运行过的：Mac 上的 card-host 算 macOS）")),
            (ids!(hub_sec_pub), t("Who publishes it", "发布者")),
            (ids!(hub_pub_name_l), t("Publisher name", "发布者名称")), (ids!(hub_support_l), t("Support contact", "支持联系方式")),
            (ids!(hub_privacy_l), t("Privacy policy (https)", "隐私政策（https）")),
            (ids!(hub_pub_id_l), t("Publisher id (for signing; the key is made once and kept on this Mac)", "发布者 ID（签名用；密钥只生成一次，保存在这台 Mac 上）")),
            (ids!(hub_sec_review), t("Review questions", "审查问题")),
            (ids!(hub_sec_submit), t("Submit", "提交")),
        ] {
            self.view.label(cx, id).set_text(cx, text);
        }
        for (id, text) in [
            (ids!(hub_save), t("Save", "保存")), (ids!(hub_prepare), t("Screenshots & check", "截图并检查")),
            (ids!(hub_draft), t("Ask the outer loop to draft them", "让外环起草回答")), (ids!(hub_answers_save), t("Save the answers", "保存回答")),
            (ids!(hub_submit), t("Submit for review…", "提交审批…")), (ids!(hub_confirm), t("Submit", "确认提交")), (ids!(hub_cancel), t("Cancel", "取消")),
        ] {
            self.view.button(cx, id).set_text(cx, text);
        }
        self.view.check_box(cx, ids!(hub_sign)).set_text(i18n::t("Sign it (optional the first time; needed for updates once a key is on record)", "签名（首次可不签；登记过密钥后，更新必须签名）"));
        self.view.label(cx, ids!(hub_check_out)).set_text(cx, &self.wb.hub_check);
        let questions = app_hub::questions(project);
        self.view.label(cx, ids!(hub_questions)).set_text(cx, &if questions.is_empty() {
            i18n::t("Screenshots & check writes the review packet; its questions show here.", "「截图并检查」会生成审查包，问题会显示在这里。").to_string()
        } else {
            questions.iter().enumerate().map(|(i, q)| format!("{}. {q}", i + 1)).collect::<Vec<_>>().join("\n")
        });
        self.view.label(cx, ids!(hub_plan)).set_text(cx, self.wb.hub_plan.as_deref().unwrap_or(""));
        self.view.view(cx, ids!(hub_confirm_row)).set_visible(cx, self.wb.hub_plan.is_some() && self.wb.hub_busy.is_none());
        // The form from the files: when it is another project's, or after a step changed them.
        if self.wb.hub_for.as_deref() == Some(project) {
            return;
        }
        self.wb.hub_for = Some(project.to_path_buf());
        let l = app_hub::read(project);
        for (id, v) in [(ids!(hub_name), &l.name), (ids!(hub_version), &l.version), (ids!(hub_subtitle), &l.subtitle), (ids!(hub_description), &l.description),
            (ids!(hub_license), &l.license), (ids!(hub_notes), &l.release_notes), (ids!(hub_pub_name), &l.publisher_name),
            (ids!(hub_support), &l.support), (ids!(hub_privacy), &l.privacy_url), (ids!(hub_pub_id), &l.publisher_id)] {
            self.view.text_input(cx, id).set_text(cx, v);
        }
        self.view.text_input(cx, ids!(hub_keywords)).set_text(cx, &l.keywords.join(", "));
        let category = app_hub::CATEGORIES.iter().position(|c| *c == l.category).unwrap_or(0);
        self.view.drop_down(cx, ids!(hub_category)).set_selected_item(cx, category);
        let age = app_hub::AGE_RATINGS.iter().position(|a| *a == l.age_rating).unwrap_or(0);
        self.view.drop_down(cx, ids!(hub_age)).set_selected_item(cx, age);
        for (id, plat) in PLATS.iter().zip(app_hub::PLATFORMS) {
            self.view.check_box(cx, &[*id]).set_active(cx, l.platforms.iter().any(|p| p == plat), Animate::No);
        }
        self.view.check_box(cx, ids!(hub_sign)).set_active(cx, l.sign, Animate::No);
        let answers = app_hub::answers(project);
        let answers = if answers.trim().is_empty() && !questions.is_empty() {
            questions.iter().enumerate().map(|(i, q)| format!("## {}. {q}\n\n", i + 1)).collect::<String>()
        } else {
            answers
        };
        self.view.text_input(cx, ids!(hub_answers)).set_text(cx, &answers);
        self.view.text_input(cx, ids!(hub_answers)).set_empty_text(cx, i18n::t("Answer each review question truthfully, citing the source", "逐条如实回答审查问题，引用源码作依据").into());
    }

    /// The App Hub form as the person filled it in.
    fn hub_form(&self, cx: &mut Cx, project: &Path) -> Listing {
        let text = |id: &[LiveId]| self.view.text_input(cx, id).text().trim().to_string();
        let mut l = app_hub::read(project);
        l.name = text(ids!(hub_name));
        l.version = text(ids!(hub_version));
        l.subtitle = text(ids!(hub_subtitle));
        l.description = text(ids!(hub_description));
        l.license = text(ids!(hub_license));
        l.release_notes = text(ids!(hub_notes));
        l.publisher_name = text(ids!(hub_pub_name));
        l.support = text(ids!(hub_support));
        l.privacy_url = text(ids!(hub_privacy));
        l.publisher_id = text(ids!(hub_pub_id));
        l.keywords = text(ids!(hub_keywords)).split([',', '，']).map(str::trim).filter(|k| !k.is_empty()).map(String::from).collect();
        l.category = app_hub::CATEGORIES.get(self.view.drop_down(cx, ids!(hub_category)).selected_item()).unwrap_or(&"").to_string();
        l.age_rating = app_hub::AGE_RATINGS.get(self.view.drop_down(cx, ids!(hub_age)).selected_item()).unwrap_or(&"all").to_string();
        l.platforms = PLATS.iter().zip(app_hub::PLATFORMS).filter(|(id, _)| self.view.check_box(cx, &[**id]).active(cx)).map(|(_, p)| p.to_string()).collect();
        l.sign = self.view.check_box(cx, ids!(hub_sign)).active(cx);
        l
    }

    /// Saves the form (and the answers); what stops a submission, if anything.
    fn hub_save(&mut self, cx: &mut Cx, project: &Path) -> Result<(), String> {
        let l = self.hub_form(cx, project);
        app_hub::write(project, &l)?;
        let answers = self.view.text_input(cx, ids!(hub_answers)).text();
        if !answers.trim().is_empty() {
            app_hub::save_answers(project, &answers)?;
        }
        let problems = app_hub::problems(&l);
        if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
    }

    fn hub_run(&mut self, project: &Path, step: HubStep) {
        self.wb.hub_busy = Some(step);
        self.wb.hub_status = None;
        let (inbox, project) = (self.rt.inbox.clone(), project.to_path_buf());
        std::thread::spawn(move || {
            let result = match step {
                HubStep::Prepare => app_hub::prepare(&project),
                HubStep::Plan => app_hub::plan(&project),
                HubStep::Submit => app_hub::submit(&project),
                HubStep::Gh => Ok(String::new()),
            };
            events::post(&inbox, LoopEvent::HubDone { project: project.to_string_lossy().into_owned(), step, result });
        });
    }

    /// A step of the App Hub tab done.
    pub(crate) fn hub_done(&mut self, project: &str, step: HubStep, result: Result<String, String>) {
        if self.wb.hub_busy == Some(step) {
            self.wb.hub_busy = None;
        }
        match step {
            HubStep::Gh => self.wb.gh = Some(result),
            HubStep::Prepare => {
                self.wb.hub_check = result.clone().unwrap_or_else(|e| e);
                self.wb.hub_status = Some(result.map(|_| i18n::t("Screenshots taken, the gate passed, the review packet written: answer its questions below.",
                    "截图完成，检查通过，审查包已生成：请在下面回答问题。").to_string()));
                // Its screenshots and digest are new: the form reads the files again.
                self.wb.hub_for = None;
            }
            HubStep::Plan => match result {
                Ok(plan) => {
                    self.wb.hub_plan = Some(i18n::pick(format!("Submit will:\n{plan}"), format!("提交将会：\n{plan}")));
                }
                Err(e) => self.wb.hub_status = Some(Err(e)),
            },
            HubStep::Submit => {
                self.wb.hub_plan = None;
                self.wb.hub_for = None;
                let issue = result.as_ref().ok().and_then(|log| log.lines().rev().find_map(|l| l.strip_prefix("issue: ")).map(String::from));
                self.wb.hub_status = Some(result.map(|_| match &issue {
                    Some(url) => i18n::pick(format!("Submitted: {url}. App Hub's maintainers review it there."), format!("已提交：{url}。App Hub 维护者会在那里审核。")),
                    None => i18n::t("Submitted.", "已提交。").to_string(),
                }));
                if let (Some(url), Some(at)) = (issue, self.store.projects.iter().position(|p| p.path == project).and_then(|pi| {
                    self.store.projects[pi].sessions.len().checked_sub(1).map(|si| (pi, si))
                })) {
                    self.system(at, &i18n::pick(format!("Submitted to App Hub for review: {url}"), format!("已提交 App Hub 审核：{url}")));
                }
            }
        }
    }

    /// The workbench's presses.
    pub(crate) fn workbench_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let project = self.selected.filter(|at| self.store.session(*at).is_some()).map(|at| PathBuf::from(&self.store.projects[at.0].path));
        for (tab, off, _, _) in TABS {
            if self.view.button(cx, &[off]).clicked(actions) {
                self.wb.tab = tab;
                // Opened again: what is on disk now.
                self.wb.perms_for = None;
                self.wb.hub_for = None;
                self.relayout(cx);
            }
        }
        let Some(project) = project else { return };
        // Files: a folder opens or closes; a file shows its source.
        let list = self.view.portal_list(cx, ids!(files_list));
        let mut hit = None;
        for (index, item) in list.items_with_actions(actions) {
            if tapped(&item.view(cx, ids!(row)), actions) {
                hit = Some(index);
            }
        }
        if let Some((path, _, dir)) = hit.and_then(|i| self.wb.rows.get(i).cloned()) {
            if dir {
                if !self.wb.open.remove(&path) {
                    self.wb.open.insert(path);
                }
            } else {
                self.wb.shown = Some(path);
            }
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(files_reveal)).clicked(actions) {
            let target = self.wb.shown.clone().unwrap_or_else(|| project.clone());
            let _ = std::process::Command::new("open").arg("-R").arg(&target).spawn();
        }
        // App Hub: inside OctoSense the local publish; on the host the form and its steps.
        if self.view.button(cx, ids!(hub_publish)).clicked(actions) {
            self.publish_app(cx);
        }
        let busy = self.wb.hub_busy.is_some_and(|s| s != HubStep::Gh);
        if self.view.button(cx, ids!(hub_save)).clicked(actions) {
            self.wb.hub_status = Some(self.hub_save(cx, &project).map(|_| i18n::t("Saved: ready to submit.", "已保存：可以提交了。").to_string()));
            self.wb.hub_for = None;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(hub_prepare)).clicked(actions) && !busy {
            let _ = self.hub_save(cx, &project);
            self.hub_run(&project, HubStep::Prepare);
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(hub_answers_save)).clicked(actions) {
            let answers = self.view.text_input(cx, ids!(hub_answers)).text();
            self.wb.hub_status = Some(app_hub::save_answers(&project, &answers).map(|_| i18n::t("Answers saved.", "回答已保存。").to_string()));
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(hub_draft)).clicked(actions) {
            if let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) {
                let text = "Draft the answers to App Hub's review questions for this app, for me to check before I submit it. \
The questions are in build/review.json (`questions`; the rest of the packet is what the reviewer sees: manifest, listing, grants, screenshots, source). \
Read the app's source and listing, then write build/REVIEW-ANSWERS.md: one section per question (`## 1. <question>`), answered truthfully and citing the source, \
and the route (pass, human-review or reject) with its reasons last. Do not change the app or its listing.".to_string();
                self.send_text(cx, at, text);
                self.wb.hub_status = Some(Ok(i18n::t("Asked the outer loop: its draft goes to build/REVIEW-ANSWERS.md; open this tab again to see it.",
                    "已请外环起草：会写到 build/REVIEW-ANSWERS.md，写好后重新打开这个标签即可看到。").to_string()));
                self.wb.hub_for = None;
            }
        }
        if self.view.button(cx, ids!(hub_submit)).clicked(actions) && !busy {
            match self.hub_save(cx, &project) {
                Ok(()) => self.hub_run(&project, HubStep::Plan),
                Err(e) => self.wb.hub_status = Some(Err(e)),
            }
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(hub_cancel)).clicked(actions) {
            self.wb.hub_plan = None;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(hub_confirm)).clicked(actions) && !busy && self.wb.hub_plan.is_some() {
            self.hub_run(&project, HubStep::Submit);
            self.relayout(cx);
        }
        // Permissions: the form into the manifest (the preview reloads from it).
        if self.view.button(cx, ids!(perms_save)).clicked(actions) {
            let ticked: Vec<&str> = CAPS.iter().filter(|(_, widget, ..)| self.view.view(cx, &[*widget]).check_box(cx, ids!(check)).active(cx)).map(|(id, ..)| *id).collect();
            let hosts = self.view.text_input(cx, ids!(perms_hosts)).text();
            let storage = self.view.text_input(cx, ids!(perms_storage)).text();
            self.wb.perms_status = Some(save_permissions(&project, &ticked, &hosts, &storage));
            if self.wb.perms_status.as_ref().is_some_and(|r| r.is_ok()) {
                self.wb.perms_for = None;
            }
            self.relayout(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_go_into_the_manifest_as_app_hub_takes_them() {
        let dir = std::env::temp_dir().join(format!("octobuddy-perms-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("bundle")).unwrap();
        std::fs::write(dir.join("bundle/manifest.json"), r#"{"schema": 1, "id": "x", "capabilities": ["storage", "llm"], "network": {"hosts": []}}"#).unwrap();
        assert!(save_permissions(&dir, &["storage"], "https://api.x.org", "").is_err(), "a bare host");
        assert!(save_permissions(&dir, &["storage"], "api.x.org", "").is_err(), "hosts need net");
        save_permissions(&dir, &["net", "storage"], "API.x.org, cdn.x.org", "8").unwrap();
        let m: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("bundle/manifest.json")).unwrap()).unwrap();
        assert_eq!(m["capabilities"], json!(["storage", "net", "llm"]), "the form's order, others kept");
        assert_eq!(m["network"]["hosts"], json!(["api.x.org", "cdn.x.org"]));
        assert_eq!(m["storage"]["max_bytes"], json!(8 * 1024 * 1024));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_tree_lists_folders_first_and_leaves_out_build_output() {
        let dir = std::env::temp_dir().join(format!("octobuddy-tree-{}", std::process::id()));
        for d in ["bundle/assets", ".git", "target"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        std::fs::write(dir.join("README.md"), "x").unwrap();
        std::fs::write(dir.join("bundle/main.splash"), "View{}").unwrap();
        let open: BTreeSet<PathBuf> = [dir.join("bundle")].into_iter().collect();
        let names: Vec<String> = tree(&dir, &open).iter().map(|(p, d, _)| format!("{d}:{}", p.file_name().unwrap().to_string_lossy())).collect();
        assert_eq!(names, ["0:bundle", "1:assets", "1:main.splash", "0:README.md"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
