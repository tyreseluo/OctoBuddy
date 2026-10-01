//! What OctoBuddy runs on, for Settings › Tools: each tool, what it is for,
//! the version found on this machine, where it comes from and where it is;
//! for the agents' programs OctoBuddy keeps, which copy runs (`agents`).
//! Versions are probed off the UI thread (`probe`), when the page opens.
use crate::i18n;
use crate::workspace::find_bin;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug, Default)]
pub struct ToolInfo {
    pub name: String,
    pub what: String,
    pub version: String,
    pub repo: String,
    pub path: String,
    /// An agent's program OctoBuddy may keep (its name in `agents`), and
    /// which copy runs.
    pub agent: Option<String>,
    pub source: String,
}

/// Which copy of `name` runs, for its card.
fn agent_source(name: &str, found: Option<&str>) -> String {
    let Some(pin) = crate::agents::pin(name) else { return String::new() };
    let (label, version) = (pin.label, pin.version);
    if !crate::agents::installable(name) {
        return i18n::pick(format!("OctoBuddy has no copy of {label} for this platform: yours runs."), format!("OctoBuddy 没有适合这台机器的 {label}：运行的是你自己的。"));
    }
    if crate::agents::prefers_own(name) {
        return match found {
            Some(_) => i18n::pick(format!("Runs yours. OctoBuddy keeps {version} too."), format!("运行你自己的。OctoBuddy 也自带 {version}。")),
            None => i18n::pick(format!("Yours is not on this machine: choose OctoBuddy's ({version}) to install it."), format!("本机没有你自己的：可选 OctoBuddy 自带的（{version}），会自动安装。")),
        };
    }
    if crate::agents::kept(name).is_some() {
        return i18n::pick(format!("Runs OctoBuddy's copy ({version}, checked against its publisher's digest)."), format!("运行 OctoBuddy 自带的版本（{version}，已按发布方的摘要校验）。"));
    }
    match (crate::agents::job(name), found) {
        (Some(crate::agents::Job::Installing), _) => i18n::pick(format!("Installing OctoBuddy's copy ({version})…"), format!("正在安装 OctoBuddy 自带的版本（{version}）…")),
        (Some(crate::agents::Job::Failed(err)), _) => i18n::pick(format!("OctoBuddy's copy ({version}) could not be installed: {err}"), format!("OctoBuddy 自带的版本（{version}）安装失败：{err}")),
        (None, Some(_)) => i18n::pick(format!("OctoBuddy's copy ({version}) is not installed yet: yours runs until it is."), format!("OctoBuddy 自带的版本（{version}）还没装：装好前运行你自己的。")),
        (None, None) => i18n::pick(format!("Not installed: OctoBuddy installs its copy ({version}) the first time it is needed."), format!("未安装：第一次用到时 OctoBuddy 会安装自带的版本（{version}）。")),
    }
}

/// The first line of `<bin> <args>`, if it runs.
fn first_line(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).env("PATH", crate::workspace::search_path()).output().ok()?;
    let text = if out.stdout.is_empty() { out.stderr } else { out.stdout };
    let line = String::from_utf8_lossy(&text).lines().next().unwrap_or("").trim().to_string();
    (out.status.success() && !line.is_empty()).then_some(line)
}

/// A crate's version as `cargo install` recorded it (for tools without `--version`).
fn cargo_installed(name: &str) -> Option<String> {
    let home = std::env::var_os("CARGO_HOME").map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cargo")))?;
    let text = std::fs::read_to_string(home.join(".crates2.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v.get("installs")?.as_object()?.keys().find_map(|k| {
        let mut parts = k.split(' ');
        (parts.next()? == name).then(|| parts.next().map(String::from)).flatten()
    })
}

/// A checkout's commit (`git describe`), for what OctoBuddy reads from a clone.
fn git_version(dir: &Path) -> Option<String> {
    let out = Command::new("git").args(["describe", "--tags", "--always"]).current_dir(dir).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn located(bin: &str) -> Option<String> {
    let path = find_bin(bin);
    path.is_file().then(|| path.display().to_string())
}

/// Every tool, its version found now (slow: run it off the UI thread).
/// How a tool's version is read, given its path.
type VersionOf = Box<dyn Fn(&str) -> Option<String>>;

pub fn probe() -> Vec<ToolInfo> {
    let t = |en: &'static str, zh: &'static str| i18n::t(en, zh).to_string();
    let missing = || t("not found", "未找到");
    let mut out = Vec::new();
    let mut bin = |name: &str, bin: &str, what: String, repo: &str, version: VersionOf| {
        let path = located(bin);
        let version = path.as_deref().and_then(&version).unwrap_or_else(|| if path.is_some() { t("installed", "已安装") } else { missing() });
        let agent = crate::agents::pin(bin).map(|p| p.name.to_string());
        let source = agent_source(bin, path.as_deref());
        out.push(ToolInfo { name: name.into(), what, version, repo: repo.into(), path: path.unwrap_or_default(), agent, source });
    };
    bin("Claude Code", "claude", t("Anthropic's coding agent CLI: the outer loop, and inner loops on Claude Code (stream-json).", "Anthropic 的编程 agent CLI：外环，以及跑在 Claude Code 上的 inner（stream-json）。"),
        "https://github.com/anthropics/claude-code", Box::new(|p| first_line(p, &["--version"])));
    bin("octos", "octos", t("The octos agent kernel: inner loops on octos (`octos serve`), and OctoSense's own agent.", "octos agent 内核：跑在 octos 上的 inner（`octos serve`），也是 OctoSense 自带的 agent。"),
        "https://github.com/octos-org/octos", Box::new(|p| first_line(p, &["--version"])));
    bin("Codex", "codex", t("OpenAI's coding agent CLI (`codex app-server`): an engine for outer and inner loops.", "OpenAI 的编程 agent CLI（`codex app-server`）：可用作外环和 inner 的引擎。"),
        "https://github.com/openai/codex", Box::new(|p| first_line(p, &["--version"])));
    bin("pi", "pi", t("Mario Zechner's pi coding agent (`pi --mode rpc`): an engine for outer and inner loops.", "Mario Zechner 的 pi 编程 agent（`pi --mode rpc`）：可用作外环和 inner 的引擎。"),
        "https://github.com/earendil-works/pi", Box::new(|p| first_line(p, &["--version"])));
    bin("agent-spec", "agent-spec", t("Task contracts: a slice's brief as Intent / Decisions / Boundaries / Completion Criteria, linted when it starts and checked against the files it changed.", "任务契约：切片的 brief 写成 Intent / Decisions / Boundaries / Completion Criteria，开始时 lint，完成后对照改动文件检查。"),
        "https://github.com/Project-Robius-China/agent-spec", Box::new(|p| first_line(p, &["--version"]).map(|l| l.trim_start_matches("agent-spec").trim().to_string())));
    bin("mempal", "mempal", t("Long-term memory (wings and rooms): the outer loop searches it before it plans and saves what it learned; calibration and archives go there.", "长期记忆（wing / room）：外环规划前检索、结束后沉淀；估算校准和归档也存在这里。"),
        "https://github.com/tyreseluo/mempal", Box::new(|_| cargo_installed("mempal")));
    // Read from a clone, not run.
    let estimation = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/agent-estimation");
    let source = std::fs::read_to_string(estimation.join("SOURCE.md")).unwrap_or_default();
    let commit = source.split(" at ").nth(1).and_then(|r| r.split_whitespace().next()).unwrap_or("").to_string();
    out.push(ToolInfo {
        name: "agent-estimation".into(),
        what: t("The estimation skill the outer loop plans with: rounds per slice, waves, minutes; OctoBuddy calibrates it with what slices really took.", "外环规划用的估算技能：每个切片几轮、几个 wave、多少分钟；OctoBuddy 用切片的真实耗时校准它。"),
        version: if commit.is_empty() { t("bundled", "内置") } else { format!("{} ({commit})", t("bundled", "内置")) },
        repo: "https://github.com/tyreseluo/agent-estimation".into(),
        path: estimation.display().to_string(),
        ..Default::default()
    });
    let flow = crate::plugins::octosense_app::tools().ok();
    out.push(ToolInfo {
        name: "OctoScript App Design Flow".into(),
        what: t("How an OctoSense app is built: the docs the outer loop plans with, `tools/octo` (new, run, shot, check).", "OctoSense 应用的做法：外环规划时读的文档，以及 `tools/octo`（new、run、shot、check）。"),
        version: flow.as_ref().and_then(|f| git_version(&f.flow)).unwrap_or_else(missing),
        repo: "https://github.com/OctoSense-org/OctoScript-App-Design-Flow".into(),
        path: flow.as_ref().map(|f| f.flow.display().to_string()).unwrap_or_default(),
        ..Default::default()
    });
    let hub = flow.as_ref().and_then(|f| f.app_hub.clone());
    out.push(ToolInfo {
        name: "OctoSense App Hub".into(),
        what: t("`card-host` runs an app headless for checks and screenshots; `hub` signs and publishes it.", "`card-host` 用来 headless 运行应用（检查、截图），`hub` 用来签名和发布。"),
        version: hub.as_deref().and_then(git_version).unwrap_or_else(missing),
        repo: "https://github.com/OctoSense-org/OctoSense-App-Hub".into(),
        path: hub.map(|h| h.display().to_string()).unwrap_or_default(),
        ..Default::default()
    });
    out
}
