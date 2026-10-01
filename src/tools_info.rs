//! What OctoBuddy runs on, for Settings › Tools: each tool, what it is for,
//! the version found on this machine, where it comes from and where it is.
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
        out.push(ToolInfo { name: name.into(), what, version, repo: repo.into(), path: path.unwrap_or_default() });
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
    });
    let flow = crate::app::tools().ok();
    out.push(ToolInfo {
        name: "OctoScript App Design Flow".into(),
        what: t("How an OctoSense app is built: the docs the outer loop plans with, `tools/octo` (new, run, shot, check).", "OctoSense 应用的做法：外环规划时读的文档，以及 `tools/octo`（new、run、shot、check）。"),
        version: flow.as_ref().and_then(|f| git_version(&f.flow)).unwrap_or_else(missing),
        repo: "https://github.com/OctoSense-org/OctoScript-App-Design-Flow".into(),
        path: flow.as_ref().map(|f| f.flow.display().to_string()).unwrap_or_default(),
    });
    let hub = flow.as_ref().and_then(|f| f.app_hub.clone());
    out.push(ToolInfo {
        name: "OctoSense App Hub".into(),
        what: t("`card-host` runs an app headless for checks and screenshots; `hub` signs and publishes it.", "`card-host` 用来 headless 运行应用（检查、截图），`hub` 用来签名和发布。"),
        version: hub.as_deref().and_then(git_version).unwrap_or_else(missing),
        repo: "https://github.com/OctoSense-org/OctoSense-App-Hub".into(),
        path: hub.map(|h| h.display().to_string()).unwrap_or_default(),
    });
    out
}
