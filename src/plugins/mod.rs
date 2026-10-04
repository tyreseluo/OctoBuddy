//! OctoBuddy's plugins: what a project kind adds to OctoBuddy — buttons over
//! the conversation, tools for the agents, rules for the outer loop — kept
//! out of OctoBuddy's core and switched on or off in Settings › Plugins.
//!
//! As Cindy does it (its "ghosts"), the host owns the heavy runtimes (the
//! app preview pane, App Hub publishing, the headless app run) as slots;
//! a plugin declares the capability, and the slot appears — its button, its
//! tools, its rules — only where an enabled plugin that applies to the
//! project declares it. A disabled plugin's tools are refused at call time.
//!
//! Two kinds:
//! - **Built-in** plugins live in this crate (`builtins()`), each in its own
//!   file of this folder: `octosense_app` (OctoSense apps: the design-flow
//!   rules, `tools/octo`, the app check), `app_preview` and `app_publish`
//!   (the app beside the conversation, and on this device's App Hub),
//!   `app_data` (a CSV or an API the app is built on), `native_tui` (an
//!   agent's own terminal UI). `view` is their part of the window:
//!   Settings › Plugins and Tools, and the buttons over the conversation.
//! - **External** plugins are a folder in `<data>/plugins/<id>/` with a
//!   `plugin.json` and a program. OctoBuddy runs the program once per call,
//!   out of process (never inside OctoBuddy): `<command> tool <name>` with the
//!   tool's arguments as JSON on stdin, its answer on stdout; `<command>
//!   action <id>` for a button, its Markdown on stdout shown in the session.
//!   The outer loop calls their tools through `octobuddy_plugin`.
//!
//! ```json
//! {
//!   "id": "lint-report", "name": "Lint report", "version": "0.1.0",
//!   "description": "Runs the project's linter and summarises it.",
//!   "when_to_use": "Before reviewing a slice that touched Rust code.",
//!   "applies_to": {"files": ["Cargo.toml"]},
//!   "command": "./run.sh",
//!   "tools": [{"name": "lint", "description": "Lint the project; findings ranked."}],
//!   "buttons": [{"id": "lint", "name": "Lint", "sub": "report"}]
//! }
//! ```
pub mod app_data;
pub mod app_hub;
pub mod design_flow;
pub mod app_factory;
pub mod app_preview;
pub mod app_publish;
pub mod card_loop;
pub(crate) mod native_tui;
pub mod new_app;
pub mod octosense_app;
pub mod workbench;
pub(crate) mod view;

use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// An OctoSense app project (`bundle/manifest.json`), everything about it
/// in one plugin: the outer loop's design-flow rules and the docs it plans
/// with (bundled), the app check for its slices, the agents' app tools, and
/// the app workbench beside the conversation (preview, files, permissions,
/// and getting it to App Hub).
pub const OCTOSENSE_APP: &str = "octosense-app";
/// Data for the app: a CSV or an API it is built on.
pub const APP_DATA: &str = "app-data";
/// The production loop: the published app watched, its breakage said.
pub const CARD_LOOP: &str = "card-loop";
/// Apps asked for from outside (`octobuddy.request`), built once the person says.
pub const APP_FACTORY: &str = "app-factory";

/// The plugins that only mean something inside OctoSense: the app factory
/// (apps asked for by OctoSense's assistant) and the production loop (the
/// apps installed there, and what its agents report). On the host they are
/// neither listed nor on.
pub const OCTOSENSE_ONLY: [&str; 2] = [APP_FACTORY, CARD_LOOP];

/// Whether plugin `id` exists in this OctoBuddy: inside OctoSense every
/// built-in one; on the host all but `OCTOSENSE_ONLY`.
pub fn here(id: &str) -> bool {
    crate::system::hosted() || !OCTOSENSE_ONLY.contains(&id)
}
/// An agent's own terminal UI in place of OctoBuddy's messages (off until
/// the person turns it on: some prefer the CLI they know).
pub const NATIVE_TUI: &str = "native-tui";
/// Plugins that start off.
const OFF_BY_DEFAULT: &[&str] = &[NATIVE_TUI];

#[derive(Clone, Debug, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub id: String,
    pub name: String,
    pub sub: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// For the outer loop: when to reach for it.
    pub when_to_use: String,
    /// It applies to a project with one of these files (relative); empty: every project.
    pub applies_to: Vec<String>,
    /// Other plugins it works with only when they are on too.
    pub requires: Vec<String>,
    pub tools: Vec<Tool>,
    pub buttons: Vec<Button>,
    /// None: built in. Some: its folder and the program run for it.
    pub external: Option<(PathBuf, String)>,
    /// A plugin.json that could not be read: why (it is listed, not run).
    pub broken: Option<String>,
}

impl Plugin {
    pub fn builtin(&self) -> bool {
        self.external.is_none()
    }

    pub fn applies(&self, project: &Path) -> bool {
        self.applies_to.is_empty() || self.applies_to.iter().any(|f| project.join(f).exists())
    }
}

fn tr(en: &'static str, zh: &'static str) -> String {
    crate::i18n::t(en, zh).to_string()
}

/// The plugins built into OctoBuddy.
pub fn builtins() -> Vec<Plugin> {
    let version = env!("CARGO_PKG_VERSION").to_string();
    let app = vec!["bundle/manifest.json".to_string()];
    let tool = |n: &str, d: &str| Tool { name: n.into(), description: d.into() };
    vec![
        Plugin {
            id: OCTOSENSE_APP.into(), name: tr("OctoSense apps", "OctoSense 应用"), version: version.clone(),
            description: tr("A project with bundle/manifest.json is an OctoSense app. The outer loop follows OctoScript App Design Flow (its docs and template come with OctoBuddy), slices are checked by running the app headless, and every agent can look at, drive and probe the running app. The app workbench beside the conversation runs it as you build (reloaded as it changes), shows its files and sources, sets its permissions, and gets it to App Hub: on this computer it fills in its listing and submits it for review; inside OctoSense it publishes it to the local App Hub to install.",
                "带 bundle/manifest.json 的项目就是 OctoSense 应用。外环按 OctoScript App Design Flow 规划（文档和模板随 OctoBuddy 自带），切片用 headless 运行应用来检查，每种 agent 都能查看、操作、试运行这个应用。对话旁的应用工作台：边做边运行（文件一改就重载）、看它的文件和源码、设置权限，并把它送上 App Hub——在这台电脑上填好上架信息、提交审批；在 OctoSense 里发布到本机 App Hub 安装。"),
            when_to_use: "Building or changing an OctoSense app.".into(),
            applies_to: app.clone(), requires: Vec::new(),
            tools: vec![tool("octobuddy_app_look", "run the app headless: script errors, a screenshot, the widgets on screen")],
            buttons: vec![Button { id: "app".into(), name: tr("App", "应用"), sub: tr("workbench", "工作台") }],
            external: None, broken: None,
        },
        Plugin {
            id: NATIVE_TUI.into(), name: tr("Native TUI", "原生 TUI"), version: version.clone(),
            description: tr("Shows an agent's own terminal UI — Claude Code's, Codex's, pi's — in place of OctoBuddy's messages, on the same conversation: the outer loop under Terminal at the top left, an inner loop with TUI in its panel. Switching back goes on in OctoBuddy's view.",
                "用 agent 自己的终端界面（Claude Code、Codex、pi 的 TUI）代替 OctoBuddy 的消息视图，接着同一个会话：外环在左上角「终端」，inner 在它面板上的「TUI」。切回来后在 OctoBuddy 的视图里继续。"),
            when_to_use: String::new(), applies_to: Vec::new(), requires: Vec::new(),
            tools: Vec::new(),
            buttons: vec![Button { id: "tui".into(), name: tr("Terminal", "终端"), sub: "TUI".into() }],
            external: None, broken: None,
        },
        Plugin {
            id: APP_DATA.into(), name: tr("App data", "应用数据"), version: version.clone(),
            description: tr("Builds the app on your data: a CSV embedded in it, or an API it fetches; the outer loop is told its shape.",
                "用你的数据做应用：CSV 嵌进应用，或让应用去取一个 API；外环会拿到数据结构。"),
            when_to_use: String::new(), applies_to: app.clone(), requires: vec![OCTOSENSE_APP.into()],
            tools: Vec::new(),
            buttons: vec![Button { id: "data".into(), name: tr("Data", "数据"), sub: tr("add", "添加") }],
            external: None, broken: None,
        },
        Plugin {
            id: APP_FACTORY.into(), name: tr("App factory", "应用工厂"), version: version.clone(),
            description: tr("Apps asked for from outside OctoBuddy: the OctoSense assistant's octobuddy.request, or another app's agent granted it. Each waits on the Apps page (the sidebar's) until you press Build; then OctoBuddy makes the app project, reads its data and hands the request to a new session's outer loop. Published, its live watch starts.",
                "OctoBuddy 之外请求做的应用：系统 agent 的 octobuddy.request，或被授权的其他应用的 agent。每个请求都在侧栏的「应用」页等你点「开始做」；之后 OctoBuddy 建项目、读取数据，并交给一个新会话的外环。发布后自动开启巡检。"),
            when_to_use: String::new(), applies_to: Vec::new(), requires: vec![OCTOSENSE_APP.into()],
            tools: Vec::new(), buttons: Vec::new(), external: None, broken: None,
        },
        Plugin {
            id: CARD_LOOP.into(), name: tr("Production loop", "生产回路"), version,
            description: tr("Keeps watch over the version published to the local App Hub: runs it headless every few minutes on live data, records its health in .octobuddy/card-loop/health.jsonl, and says in the session when it breaks (it does not start, script errors, its key widgets gone, a failure shown) and when it is well again.",
                "守护发布到本地 App Hub 的那个版本：每隔几分钟用实时数据 headless 运行一次，健康记录写在 .octobuddy/card-loop/health.jsonl；坏了（起不来、脚本报错、关键控件不见、显示失败提示）和恢复时都会在会话里说。"),
            // It watches what was published, with or without the publish slot on now.
            when_to_use: String::new(), applies_to: app, requires: vec![OCTOSENSE_APP.into()],
            tools: Vec::new(),
            buttons: vec![Button { id: "loop".into(), name: tr("Live", "巡检"), sub: tr("watch", "守护") }],
            external: None, broken: None,
        },
    ].into_iter().filter(|p| here(&p.id)).collect()
}

/// Where external plugins are installed.
pub fn dir() -> PathBuf {
    crate::model::data_dir().join("plugins")
}

/// An external plugin from its folder.
fn read_external(folder: &Path) -> Plugin {
    let id = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut p = Plugin {
        id: id.clone(), name: id, version: String::new(), description: String::new(), when_to_use: String::new(),
        applies_to: Vec::new(), requires: Vec::new(), tools: Vec::new(), buttons: Vec::new(),
        external: Some((folder.to_path_buf(), String::new())), broken: None,
    };
    let v: Value = match std::fs::read_to_string(folder.join("plugin.json")).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) {
        Ok(v) => v,
        Err(err) => {
            p.broken = Some(format!("plugin.json: {err}"));
            return p;
        }
    };
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let list = |k: &str| v.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
    if !s("id").is_empty() {
        p.id = s("id");
    }
    p.name = if s("name").is_empty() { p.id.clone() } else { s("name") };
    p.version = s("version");
    p.description = s("description");
    p.when_to_use = s("when_to_use");
    p.applies_to = v.pointer("/applies_to/files").and_then(Value::as_array).into_iter().flatten()
        .filter_map(Value::as_str).map(String::from).collect();
    p.requires = list("requires").iter().filter_map(Value::as_str).map(String::from).collect();
    p.tools = list("tools").iter().filter_map(|t| Some(Tool {
        name: t.get("name")?.as_str()?.to_string(),
        description: t.get("description").and_then(Value::as_str).unwrap_or("").to_string(),
    })).collect();
    p.buttons = list("buttons").iter().filter_map(|b| Some(Button {
        id: b.get("id")?.as_str()?.to_string(),
        name: b.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        sub: b.get("sub").and_then(Value::as_str).unwrap_or("").to_string(),
    })).collect();
    let command = s("command");
    if command.is_empty() {
        p.broken = Some("plugin.json names no command".into());
    } else if command.contains("..") || command.starts_with('/') {
        // Its program is its own: inside its folder.
        p.broken = Some("its command must be a path inside its own folder".into());
    }
    p.external = Some((folder.to_path_buf(), command));
    p
}

/// Every plugin: the built-in ones, then the installed ones (by folder name).
pub fn all() -> Vec<Plugin> {
    let mut out = builtins();
    let mut folders: Vec<PathBuf> = std::fs::read_dir(dir()).into_iter().flatten().flatten()
        .map(|e| e.path()).filter(|p| p.join("plugin.json").is_file()).collect();
    folders.sort();
    for folder in folders {
        let p = read_external(&folder);
        if !out.iter().any(|o| o.id == p.id) {
            out.push(p);
        }
    }
    out
}

/// The plugins the person switched off, and those that start off that they
/// switched on, for every thread to see.
static DISABLED: RwLock<Option<HashSet<String>>> = RwLock::new(None);
static ENABLED: RwLock<Option<HashSet<String>>> = RwLock::new(None);

pub fn set_disabled(ids: &[String]) {
    *DISABLED.write().unwrap_or_else(|e| e.into_inner()) = Some(ids.iter().cloned().collect());
}

pub fn set_enabled(ids: &[String]) {
    *ENABLED.write().unwrap_or_else(|e| e.into_inner()) = Some(ids.iter().cloned().collect());
}

/// Whether plugin `id` starts off (the person turns it on).
pub fn off_by_default(id: &str) -> bool {
    OFF_BY_DEFAULT.contains(&id)
}

pub fn enabled(id: &str) -> bool {
    if !here(id) {
        return false;
    }
    if DISABLED.read().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|d| d.contains(id)) {
        return false;
    }
    !off_by_default(id) || ENABLED.read().unwrap_or_else(|e| e.into_inner()).as_ref().is_some_and(|e| e.contains(id))
}

/// Whether plugin `id` is at work for `project`: on, applying to it, and
/// the plugins it requires too.
pub fn active(id: &str, project: &str) -> bool {
    let all = all();
    active_in(&all, id, Path::new(project), 0)
}

fn active_in(all: &[Plugin], id: &str, project: &Path, depth: usize) -> bool {
    let Some(p) = all.iter().find(|p| p.id == id) else { return false };
    depth < 4 && p.broken.is_none() && enabled(id) && p.applies(project)
        && p.requires.iter().all(|r| active_in(all, r, project, depth + 1))
}

/// The external plugins at work for `project`.
pub fn external_active(project: &str) -> Vec<Plugin> {
    let all = all();
    all.iter().filter(|p| !p.builtin() && active_in(&all, &p.id, Path::new(project), 0)).cloned().collect()
}

/// For the outer loop's rules: the external plugins it can call here.
pub fn roster(project: &str) -> Option<String> {
    let active = external_active(project);
    if active.is_empty() {
        return None;
    }
    let mut out = String::from("PLUGINS. Call their tools with octobuddy_plugin {plugin, tool, args}:\n");
    for p in active {
        let when = if p.when_to_use.is_empty() { p.description.clone() } else { p.when_to_use.clone() };
        let tools: Vec<String> = p.tools.iter().map(|t| format!("{} ({})", t.name, t.description)).collect();
        out.push_str(&format!("- {}: {when} Tools: {}\n", p.id, tools.join("; ")));
    }
    Some(out)
}

/// Runs an external plugin's program (off the UI thread): `tool <name>` or
/// `action <id>`, `input` on its stdin, its stdout back. It runs in the
/// project, with a minute to answer.
pub fn run(plugin: &Plugin, verb: &str, name: &str, input: &Value, project: &str) -> Result<String, String> {
    use std::io::{Read, Write};
    let Some((folder, command)) = &plugin.external else { return Err(format!("{} is built in", plugin.id)) };
    if let Some(why) = &plugin.broken {
        return Err(format!("{}: {why}", plugin.id));
    }
    if !enabled(&plugin.id) {
        return Err(format!("the plugin {} is switched off (Settings › Plugins)", plugin.id));
    }
    let program = folder.join(command);
    let mut child = std::process::Command::new(&program).arg(verb).arg(name)
        .current_dir(project).env("OCTOBUDDY_PROJECT", project).env("OCTOBUDDY_PLUGIN_DIR", folder)
        .env("PATH", crate::workspace::search_path())
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("could not run {}: {e}", program.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.to_string().as_bytes());
    }
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let (mut out, mut err) = (String::new(), String::new());
                if let Some(mut o) = child.stdout.take() {
                    let _ = o.read_to_string(&mut out);
                }
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                let out: String = out.chars().take(20_000).collect();
                return if status.success() { Ok(out.trim().to_string()) } else { Err(format!("{} {verb} {name} failed ({status}): {}", plugin.id, err.trim())) };
            }
            Ok(None) if started.elapsed() > std::time::Duration::from_secs(60) => {
                let _ = child.kill();
                return Err(format!("{} {verb} {name} took over a minute", plugin.id));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("octobuddy-plugins-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_app_slots_follow_their_plugins() {
        let dir = scratch("slots");
        let project = dir.to_string_lossy().into_owned();
        assert!(!active(OCTOSENSE_APP, &project), "not an app: no workbench");
        std::fs::create_dir_all(dir.join("bundle")).unwrap();
        std::fs::write(dir.join("bundle/manifest.json"), "{}").unwrap();
        assert!(active(OCTOSENSE_APP, &project) && active(APP_DATA, &project));
        // Switching the app plugin off takes what requires it along.
        set_disabled(&[OCTOSENSE_APP.to_string()]);
        assert!(!active(OCTOSENSE_APP, &project) && !active(APP_DATA, &project));
        set_disabled(&[]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_external_plugin_is_read_and_run_in_its_folder() {
        let dir = scratch("echo");
        let folder = dir.join("echo");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("plugin.json"), r#"{"id":"echo","name":"Echo","version":"0.1.0","command":"run.sh",
            "tools":[{"name":"say","description":"says it back"}],"buttons":[{"id":"hi","name":"Hi","sub":"say"}]}"#).unwrap();
        std::fs::write(folder.join("run.sh"), "#!/bin/sh\nread input\necho \"$1 $2 $input\"\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(folder.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let p = read_external(&folder);
        assert_eq!((p.id.as_str(), p.version.as_str(), p.tools.len(), p.buttons.len()), ("echo", "0.1.0", 1, 1));
        assert!(p.broken.is_none());
        let out = run(&p, "tool", "say", &serde_json::json!({"x": 1}), &dir.to_string_lossy()).unwrap();
        assert_eq!(out, r#"tool say {"x":1}"#);
        // A command outside its folder is refused.
        std::fs::write(folder.join("plugin.json"), r#"{"id":"echo","command":"../evil.sh"}"#).unwrap();
        assert!(read_external(&folder).broken.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
