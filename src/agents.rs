//! The agents' programs OctoBuddy keeps itself, as Cindy does: Claude Code,
//! Codex, pi and octos at the versions it was tested with (their protocols
//! move between releases), so a build behaves the same on every machine.
//!
//! Each is downloaded the first time it is needed (or from Settings › Tools)
//! into `<data>/agents/<name>/<version>/` and checked against the digest its
//! publisher recorded, pinned here: Claude Code and Codex are native builds
//! on npm, one tarball per platform (npm's `sha512-` integrity); octos is
//! its GitHub release's bundle (GitHub's `sha256:` digest); pi is a Node
//! program, installed with `npm ci` from the lockfile in
//! `resources/agents/pi/` (npm checks every package against it, and no
//! install script runs). The version's folder appears only once it is whole
//! and checked.
//!
//! The person's own installs stay usable: per agent, Settings › Tools picks
//! OctoBuddy's copy (the default) or theirs (`agents.json`). Logins and
//! settings are theirs either way (`~/.claude`, Codex's and pi's homes).
use crate::events::{post, Inbox, LoopEvent};
use crate::i18n;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// One agent's program OctoBuddy may keep.
pub struct Pin {
    pub name: &'static str,
    pub label: &'static str,
    pub version: &'static str,
    pub source: Source,
}

pub enum Source {
    /// A tarball per platform (platform, url, digest) and where the program
    /// is inside it (a `*` segment stands for the one folder there).
    Archive { builds: &'static [(&'static str, &'static str, &'static str)], program: &'static str },
    /// A Node program, installed with `npm ci` from its lockfile.
    Npm { package_json: &'static str, lock: &'static str, program: &'static str },
}

pub const PINS: [Pin; 4] = [
    Pin {
        name: "claude", label: "Claude Code", version: "2.1.286",
        source: Source::Archive {
            builds: &[
                ("darwin-arm64", "https://registry.npmjs.org/@anthropic-ai/claude-code-darwin-arm64/-/claude-code-darwin-arm64-2.1.286.tgz",
                 "sha512-QlU1+S7cNO1BKzlMJGRR/ZqdsP6c3+QAA2u9EX7dTO6HUo1/RAenn/q+mlyX60MZzTZOjPkAmTt77eYrH1IAwg=="),
                ("darwin-x64", "https://registry.npmjs.org/@anthropic-ai/claude-code-darwin-x64/-/claude-code-darwin-x64-2.1.286.tgz",
                 "sha512-RlnglbMpkdvIdKzPphXI+KMwXefKpMJXYGpGAfpTI6XvkDcjgj5j8WXxvoySyl2qTetWOEqjkFB/V8rtW4qp4g=="),
                ("linux-x64", "https://registry.npmjs.org/@anthropic-ai/claude-code-linux-x64/-/claude-code-linux-x64-2.1.286.tgz",
                 "sha512-PnVL8ZCEev3IexLeOc9/QRfqRDUZRgnsaMLdpjlBN0VaieeTrf7mZ2JJi29ue2KAqHvq6SsOadU5G/shnFE4MA=="),
                ("linux-arm64", "https://registry.npmjs.org/@anthropic-ai/claude-code-linux-arm64/-/claude-code-linux-arm64-2.1.286.tgz",
                 "sha512-dHQ/ObyC4jaGHVk+gFE8P8PALMIVLhQSyhiC3y3kkuLp0H0MHgLxHt/srQ3zJs892ClDbtl1swxv4bw1ioruZA=="),
            ],
            program: "package/claude",
        },
    },
    Pin {
        name: "codex", label: "Codex", version: "0.152.0",
        source: Source::Archive {
            builds: &[
                ("darwin-arm64", "https://registry.npmjs.org/@openai/codex/-/codex-0.152.0-darwin-arm64.tgz",
                 "sha512-DOnDA6EKOs+aRytYH4ffIuUutok9ovpvCNax8aaTZvBB2bKUPjUleLrP1pk+hGarddqU1ySuvXMOfp6ufXyOVQ=="),
                ("darwin-x64", "https://registry.npmjs.org/@openai/codex/-/codex-0.152.0-darwin-x64.tgz",
                 "sha512-p2XLFWU+Lflke89zNK67ad2/yaFndUIZkyk+gjl35NCDz+LncvAUOeiTFB2MtmC2kxvudcYo6/yezQqwM29Gnw=="),
                ("linux-x64", "https://registry.npmjs.org/@openai/codex/-/codex-0.152.0-linux-x64.tgz",
                 "sha512-Isn/g5EZTaNbwZtaIuz67U1FNDEVVeMpS4xiR+c1dpOPL2xxrGNtdAothOW2YKsb96OsS+QfClziFb4qxesWDA=="),
                ("linux-arm64", "https://registry.npmjs.org/@openai/codex/-/codex-0.152.0-linux-arm64.tgz",
                 "sha512-OTmO6y5gCpcjzybwzgK/nSM8VI68Pw+T2ohBK+8cwVM+7PCRqSqtNo7qTLG0bgv9ff6QtwiZ9sl0KtY42H0Wqg=="),
            ],
            program: "package/vendor/*/bin/codex",
        },
    },
    Pin {
        name: "pi", label: "pi", version: "0.99.2",
        source: Source::Npm {
            package_json: include_str!("../resources/agents/pi/package.json"),
            lock: include_str!("../resources/agents/pi/package-lock.json"),
            program: "node_modules/.bin/pi",
        },
    },
    // Its release has no Intel Mac bundle: there the person's own runs.
    Pin {
        name: "octos", label: "octos", version: "2.0.3-rc.12",
        source: Source::Archive {
            builds: &[
                ("darwin-arm64", "https://github.com/octos-org/octos/releases/download/v2.0.3-rc.12/octos-bundle-aarch64-apple-darwin.tar.gz",
                 "sha256:164957481d6550d8cdfee0a0dc3040d1a0cff79c7ee3b5a0dec3dc2ee060fff3"),
                ("linux-x64", "https://github.com/octos-org/octos/releases/download/v2.0.3-rc.12/octos-bundle-x86_64-unknown-linux-gnu.tar.gz",
                 "sha256:6b479b5f3fa3bb85351bcac050d575ce3a296d48e95e1e496ce8658c30ec70fb"),
                ("linux-arm64", "https://github.com/octos-org/octos/releases/download/v2.0.3-rc.12/octos-bundle-aarch64-unknown-linux-gnu.tar.gz",
                 "sha256:b14b3a32a38413cb74aff334af11df73d4bd1d18cdb6613ca11f794a8309536c"),
            ],
            program: "octos",
        },
    },
];

/// This machine's platform, as npm names it.
pub fn platform() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "x86_64") => Some("linux-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        _ => None,
    }
}

pub fn pin(name: &str) -> Option<&'static Pin> {
    PINS.iter().find(|p| p.name == name)
}

/// Whether OctoBuddy has a copy of `name` for this machine.
pub fn installable(name: &str) -> bool {
    match pin(name).map(|p| &p.source) {
        Some(Source::Archive { builds, .. }) => platform().is_some_and(|os| builds.iter().any(|b| b.0 == os)),
        Some(Source::Npm { .. }) => true,
        None => false,
    }
}

fn home(pin: &Pin) -> PathBuf {
    crate::model::data_dir().join("agents").join(pin.name).join(pin.version)
}

/// `pattern` under `dir`, a file there (a `*` segment: any one folder).
fn locate(dir: &Path, pattern: &str) -> Option<PathBuf> {
    let mut found = vec![dir.to_path_buf()];
    for part in pattern.split('/') {
        found = found.into_iter().flat_map(|d| match part {
            "*" => std::fs::read_dir(&d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect(),
            _ => vec![d.join(part)],
        }).collect();
    }
    found.into_iter().find(|p| p.is_file())
}

/// OctoBuddy's copy of `name`, if installed.
pub fn kept(name: &str) -> Option<PathBuf> {
    let pin = pin(name)?;
    let program = match &pin.source {
        Source::Archive { program, .. } | Source::Npm { program, .. } => program,
    };
    locate(&home(pin), program)
}

/// Whether `bin` is one of OctoBuddy's copies.
pub fn is_kept(bin: &Path) -> bool {
    bin.starts_with(crate::model::data_dir().join("agents"))
}

fn prefs_file() -> PathBuf {
    crate::model::data_dir().join("agents.json")
}

/// The agents the person runs from their own install instead.
fn own_ones() -> Vec<String> {
    let text = std::fs::read_to_string(prefs_file()).unwrap_or_default();
    serde_json::from_str::<serde_json::Value>(&text).ok()
        .and_then(|v| v["own"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()))
        .unwrap_or_default()
}

pub fn prefers_own(name: &str) -> bool {
    own_ones().iter().any(|n| n == name)
}

pub fn set_prefers_own(name: &str, own: bool) {
    let mut names = own_ones();
    names.retain(|n| n != name);
    if own {
        names.push(name.to_string());
    }
    let _ = std::fs::create_dir_all(crate::model::data_dir());
    let _ = std::fs::write(prefs_file(), serde_json::json!({"own": names}).to_string());
}

/// What `find_bin` runs for `name` before the search path: OctoBuddy's
/// copy, unless the person prefers theirs (or it is not installed yet).
pub fn chosen(name: &str) -> Option<PathBuf> {
    pin(name)?;
    if prefers_own(name) {
        return None;
    }
    kept(name)
}

/// Folders the chosen programs want on PATH: Codex's ripgrep.
pub fn extra_path() -> Vec<PathBuf> {
    chosen("codex").and_then(|bin| Some(bin.parent()?.parent()?.join("codex-path"))).filter(|d| d.is_dir()).into_iter().collect()
}

/// An install under way, or the last one's failure.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    Installing,
    Failed(String),
}

static JOBS: Mutex<BTreeMap<String, Job>> = Mutex::new(BTreeMap::new());

pub fn job(name: &str) -> Option<Job> {
    JOBS.lock().ok()?.get(name).cloned()
}

/// `file`'s digest in `want`'s form: `sha512-<base64>` (npm) or
/// `sha256:<hex>` (GitHub).
fn digest_of(file: &Path, want: &str) -> Result<String, String> {
    use base64::Engine;
    use sha2::Digest;
    let mut f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    if want.starts_with("sha512-") {
        let mut h = sha2::Sha512::new();
        std::io::copy(&mut f, &mut h).map_err(|e| e.to_string())?;
        Ok(format!("sha512-{}", base64::engine::general_purpose::STANDARD.encode(h.finalize())))
    } else if want.starts_with("sha256:") {
        let mut h = sha2::Sha256::new();
        std::io::copy(&mut f, &mut h).map_err(|e| e.to_string())?;
        Ok(format!("sha256:{}", h.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()))
    } else {
        Err(format!("no digest of the kind {want}"))
    }
}

fn run(cmd: &mut Command, what: &str) -> Result<(), String> {
    let out = cmd.output().map_err(|e| format!("{what}: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    Err(format!("{what}: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("failed").trim()))
}

/// Installs OctoBuddy's copy of `name` (blocking: off the UI thread).
pub fn install(name: &str) -> Result<PathBuf, String> {
    let pin = pin(name).ok_or_else(|| format!("{name} is not one OctoBuddy keeps"))?;
    if let Some(bin) = kept(name) {
        return Ok(bin);
    }
    let dir = home(pin);
    let partial = dir.with_extension("partial");
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(&partial).map_err(|e| e.to_string())?;
    let fill = || match &pin.source {
        Source::Archive { builds, .. } => {
            let (_, url, want) = platform().and_then(|os| builds.iter().find(|b| b.0 == os))
                .ok_or("OctoBuddy has no copy of it for this platform")?;
            let tarball = partial.join("download.tgz");
            run(Command::new("curl").args(["-sSfL", "--retry", "2", "-o"]).arg(&tarball).arg(url), "download")?;
            let got = digest_of(&tarball, want)?;
            if got != *want {
                return Err(format!("the download is not the one pinned ({got}, not {want})"));
            }
            run(Command::new("tar").arg("xzf").arg(&tarball).arg("-C").arg(&partial), "unpack")?;
            std::fs::remove_file(&tarball).map_err(|e| e.to_string())
        }
        Source::Npm { package_json, lock, .. } => {
            std::fs::write(partial.join("package.json"), package_json).map_err(|e| e.to_string())?;
            std::fs::write(partial.join("package-lock.json"), lock).map_err(|e| e.to_string())?;
            let npm = crate::workspace::find_bin("npm");
            if !npm.is_file() {
                return Err(i18n::t("pi needs Node.js and npm (not found)", "pi 需要 Node.js 和 npm（未找到）").into());
            }
            run(Command::new(npm).args(["ci", "--no-audit", "--no-fund", "--ignore-scripts", "--loglevel=error"])
                .current_dir(&partial).env("PATH", crate::workspace::search_path()), "npm ci")
        }
    };
    if let Err(err) = fill() {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(err);
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&partial, &dir).map_err(|e| e.to_string())?;
    kept(name).ok_or_else(|| "installed, but its program is not where it should be".into())
}

/// `install`, off the UI thread (once at a time); `AgentInstalled` says how
/// it went. `awaited`: something could not start without it.
pub fn install_in_background(inbox: &Inbox, name: &str, awaited: bool) {
    {
        let Ok(mut jobs) = JOBS.lock() else { return };
        if jobs.get(name) == Some(&Job::Installing) {
            return;
        }
        jobs.insert(name.to_string(), Job::Installing);
    }
    let (inbox, name, lang) = (inbox.clone(), name.to_string(), i18n::lang());
    std::thread::spawn(move || {
        i18n::set(lang);
        let result = install(&name).map(|p| p.display().to_string());
        if let Ok(mut jobs) = JOBS.lock() {
            match &result {
                Ok(_) => jobs.remove(&name),
                Err(err) => jobs.insert(name.clone(), Job::Failed(err.clone())),
            };
        }
        post(&inbox, LoopEvent::AgentInstalled { name, result, awaited });
    });
}

/// Before `name` starts: its program is here, or OctoBuddy starts
/// installing its copy and says so (the start waits for the next message).
pub fn ready(inbox: &Inbox, name: &str) -> Result<(), String> {
    if crate::workspace::find_bin(name).is_file() || !installable(name) {
        return Ok(());
    }
    let pin = pin(name).ok_or("not kept")?;
    if prefers_own(name) {
        return Err(i18n::pick(
            format!("{} is not on this machine (Settings › Tools runs yours): there, choose OctoBuddy's copy to install it.", pin.label),
            format!("本机没有 {}（设置 › 工具 里选的是你自己的）：可以在那里改用 OctoBuddy 自带的，它会装好。", pin.label)));
    }
    install_in_background(inbox, name, true);
    Err(i18n::pick(
        format!("{} is not on this machine yet: OctoBuddy is installing its copy ({}). Settings › Tools shows how it goes; send again once it is ready.", pin.label, pin.version),
        format!("本机还没有 {}：OctoBuddy 正在安装自带的版本（{}），进度见 设置 › 工具；装好后再发一次。", pin.label, pin.version)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pin_names_its_builds_and_digests() {
        for pin in &PINS {
            match &pin.source {
                Source::Archive { builds, program } => {
                    for (os, url, digest) in builds.iter() {
                        assert!(["darwin-arm64", "darwin-x64", "linux-x64", "linux-arm64"].contains(os), "{}: {os}", pin.name);
                        assert!(url.starts_with("https://") && url.contains(pin.version), "{}: {url}", pin.name);
                        let ok = digest.strip_prefix("sha512-").is_some_and(|b| b.len() == 88)
                            || digest.strip_prefix("sha256:").is_some_and(|h| h.len() == 64 && h.bytes().all(|c| c.is_ascii_hexdigit()));
                        assert!(ok, "{}: {digest}", pin.name);
                    }
                    assert!(!program.is_empty());
                }
                Source::Npm { package_json, lock, .. } => {
                    assert!(package_json.contains(&format!("\"{}\"", pin.version)), "{}: package.json pins it", pin.name);
                    let lock: serde_json::Value = serde_json::from_str(lock).unwrap();
                    assert_eq!(lock["packages"]["node_modules/@earendil-works/pi-coding-agent"]["version"], pin.version, "the lockfile holds it");
                }
            }
        }
        // The two native CLIs come for every platform OctoBuddy runs on.
        for name in ["claude", "codex"] {
            let Some(Source::Archive { builds, .. }) = pin(name).map(|p| &p.source) else { panic!("{name}") };
            assert_eq!(builds.len(), 4, "{name}");
        }
    }

    #[test]
    fn a_program_is_found_through_one_wildcard() {
        let dir = std::env::temp_dir().join(format!("octobuddy-locate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("package/vendor/aarch64-apple-darwin/bin")).unwrap();
        std::fs::write(dir.join("package/vendor/aarch64-apple-darwin/bin/codex"), "").unwrap();
        assert_eq!(locate(&dir, "package/vendor/*/bin/codex"), Some(dir.join("package/vendor/aarch64-apple-darwin/bin/codex")));
        assert_eq!(locate(&dir, "package/claude"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every copy for this machine, for real (the network, ~600 MB), into a
    /// scratch data dir: `OCTOBUDDY_HOME=<dir> cargo test -- --ignored installs_every`.
    #[test]
    #[ignore]
    fn installs_every_agent_this_machine_has_a_copy_of() {
        assert!(std::env::var_os("OCTOBUDDY_HOME").is_some(), "only into a scratch OCTOBUDDY_HOME");
        for pin in PINS.iter().filter(|p| installable(p.name)) {
            let bin = install(pin.name).unwrap_or_else(|err| panic!("{}: {err}", pin.name));
            assert_eq!(kept(pin.name).as_ref(), Some(&bin));
            let out = Command::new(&bin).arg("--version").env("PATH", crate::workspace::search_path()).output().unwrap();
            let said = String::from_utf8_lossy(&out.stdout);
            assert!(said.contains(pin.version), "{}: {said}", pin.name);
        }
    }

    #[test]
    fn digests_come_in_the_publishers_forms() {
        let file = std::env::temp_dir().join(format!("octobuddy-agent-digest-{}", std::process::id()));
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(digest_of(&file, "sha256:").unwrap(), "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(digest_of(&file, "sha512-").unwrap(),
            "sha512-3a81oZNherrMQXNJriBBMRLm+k6JqX6iCp7u5ktV05ohkpkqJ0/BqDa6PCOj/uu9RU1EI2Q86A4qmslPpUyknw==");
        let _ = std::fs::remove_file(&file);
    }
}
