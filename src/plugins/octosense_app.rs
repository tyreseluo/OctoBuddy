//! OctoSense apps made with OctoBuddy. A project is an app when its `bundle/`
//! holds a script app's manifest: OctoScript's `main.splash`, the manifest,
//! the store listing and the artwork, in OctoScript App Design Flow's format.
//!
//! OctoBuddy starts one from the design flow's template (`tools/octo new`),
//! tells the outer loop how such an app is built (`outer_rules`), gives the
//! inner loops the API reference inside the project (`.octobuddy/docs`),
//! checks each one's work with App Hub's gate and a headless run
//! ([`CHECK`], a script it writes) and runs the app in its own window
//! (`app_preview`). Publishing to App Hub stays the person's step: a publisher
//! key, the listing, the store screenshots.
use crate::workspace::search_path;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What an app project's inner loop is told: the app runs outside its sandbox.
pub const INNER_NOTE: &str = "\nThis project is an OctoSense app. You cannot run it: its hosts (card-host, the shell) are \
not in your sandbox, so do not look for them, install them, or spawn helpers to test it. Start with \
.octobuddy/docs/SPLASH-COOKBOOK.md: OctoBuddy's verified patterns for what an app needs (pages, lists, forms, \
storage, dates, money, bars, gotchas and the errors they give). Read .octobuddy/docs/SCRIPT-API.md only for what \
it lacks, and other apps' sources not at all unless both lack it. When the project has app/parts/, edit only \
the parts your brief gives you: bundle/main.splash is generated from them (never edit it). When you finish, OctoBuddy runs the app headless and App Hub's \
gate (octobuddy-app-check) and shows it running in its preview; the result comes back to you if it fails.";

/// The same for an inner loop on Claude Code, which has OctoBuddy's tools to
/// run the app itself (`octobuddy_check`, `octobuddy_app_look`).
pub const INNER_NOTE_TOOLS: &str = "\nThis project is an OctoSense app. Its hosts (card-host, the shell) cannot run in your \
sandbox, so do not look for them or install them: OctoBuddy runs the app for you, outside it, through two tools. \
`octobuddy_app_look` runs the app headless now and gives you its script errors, a screenshot (a PNG under \
.octobuddy/shots/, which you can read) and the widgets on screen (type, id, text: what a test finds them by). \
`octobuddy_check` runs your slice's own check command, the one OctoBuddy runs when you finish, and gives you its \
output. `octobuddy_app_probe` runs a few lines of Splash of your own (a whole small main.splash) and gives you its \
errors and widgets: when unsure how Splash behaves, try it there, in seconds, rather than guess or read sources. Use them as you work: after each meaningful change, look; before you report, run your check until it \
passes. Start with .octobuddy/docs/SPLASH-COOKBOOK.md: OctoBuddy's verified patterns for what an app needs (pages, \
lists, forms, storage, dates, money, bars, gotchas and the errors they give). Read .octobuddy/docs/SCRIPT-API.md \
only for what it lacks, and other apps' sources not at all unless both lack it. When the project has \
app/parts/, edit only the parts your brief gives you: bundle/main.splash is generated from them (never edit it).";

/// OctoBuddy's tools from a shell, for an agent with no MCP (pi): `look`,
/// `check`, `probe <file>`, to the endpoint `OCTOBUDDY_MCP_URL` names.
pub const APP_CLI: &str = r#"#!/usr/bin/env python3
# octobuddy-app: OctoBuddy's tools for an inner loop, from a shell (OctoBuddy rewrites this file).
import json, os, sys, urllib.request
url = os.environ.get("OCTOBUDDY_MCP_URL")
if not url:
    print("OCTOBUDDY_MCP_URL is not set: OctoBuddy did not start this agent with its tools")
    sys.exit(2)
names = {"look": "octobuddy_app_look", "check": "octobuddy_check", "probe": "octobuddy_app_probe"}
cmd = sys.argv[1] if len(sys.argv) > 1 else ""
if cmd not in names or (cmd == "probe" and len(sys.argv) < 3):
    print("usage: octobuddy-app look | check | probe <file.splash>")
    sys.exit(2)
args = {"source": open(sys.argv[2]).read()} if cmd == "probe" else {}
req = {"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": names[cmd], "arguments": args}}
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
r = opener.open(urllib.request.Request(url, data=json.dumps(req).encode(), headers={"Content-Type": "application/json", "Accept": "application/json"}), timeout=660)
res = json.loads(r.read().decode()).get("result", {})
print("".join(c.get("text", "") for c in res.get("content", [])))
sys.exit(1 if res.get("isError") else 0)
"#;

/// Writes `octobuddy-app` into OctoBuddy's commands (`bin_dir`): where.
pub fn ensure_app_cli() -> Option<PathBuf> {
    let path = bin_dir().join("octobuddy-app");
    let _ = std::fs::create_dir_all(bin_dir());
    if std::fs::read_to_string(&path).ok().as_deref() != Some(APP_CLI) {
        std::fs::write(&path, APP_CLI).ok()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
    }
    Some(path)
}

/// The same note for an inner loop with no MCP (pi): the tools as commands.
pub const INNER_NOTE_CLI: &str = "\nThis project is an OctoSense app. Its hosts (card-host, the shell) cannot run in your \
shell, so do not look for them or install them: OctoBuddy runs the app for you, outside, through a command. \
`octobuddy-app look` runs the app headless now and prints its script errors, a screenshot (a PNG under \
.octobuddy/shots/, which you can read) and the widgets on screen. `octobuddy-app check` runs your slice's own check, \
the one OctoBuddy runs when you finish. `octobuddy-app probe <file>` runs a small Splash program of yours (a whole \
main.splash, in a file of your own outside bundle/) and prints its errors and widgets: when unsure how Splash \
behaves, try it there, in seconds. Look after each meaningful change; run your check until it passes before you \
report. Start with .octobuddy/docs/SPLASH-COOKBOOK.md (verified patterns and gotchas); SCRIPT-API.md only for what it \
lacks. When the project has app/parts/, edit only the parts your brief gives you: bundle/main.splash is generated \
from them (never edit it).";

/// The command a slice of an app is checked with.
pub const CHECK: &str = "octobuddy-app-check";

/// Runs `project`'s app headless once (a copy of its bundle): its script
/// errors, a screenshot under `<project>/.octobuddy/shots/`, and the widgets
/// on screen, compact. Blocks (seconds): call it off the UI thread.
pub fn look(project: &str) -> Result<String, String> {
    look_at(project, &bundle(project), "")
}

/// A few lines of Splash tried at once: `source` as the app's main.splash
/// (in a copy of its bundle, with its manifest), run headless the way
/// `look` runs the app: its script errors and widgets, in seconds. How
/// Splash behaves is answered by trying it, not by reading the runtime.
pub fn probe(project: &str, source: &str) -> Result<String, String> {
    if source.trim().is_empty() {
        return Err("give the program to try in `source` (a whole main.splash)".into());
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let work = std::env::temp_dir().join(format!("octobuddy-probe-{stamp}"));
    let copy = work.join("bundle");
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let copied = Command::new("cp").arg("-R").arg(bundle(project)).arg(&copy).status().map_err(|e| e.to_string())?;
    if !copied.success() {
        let _ = std::fs::remove_dir_all(&work);
        return Err("could not copy the app's bundle".into());
    }
    let out = std::fs::write(copy.join("main.splash"), source).map_err(|e| e.to_string()).and_then(|_| look_at(project, &copy, "probe-"));
    let _ = std::fs::remove_dir_all(&work);
    out
}

fn look_at(project: &str, bundle: &Path, prefix: &str) -> Result<String, String> {
    let tools = tools()?;
    if prefix.is_empty() {
        assemble(project)?;
    }
    let shots = Path::new(project).join(".octobuddy/shots");
    std::fs::create_dir_all(&shots).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let shot = shots.join(format!("{prefix}{stamp}.png"));
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(LOOK_SCRIPT).arg("octobuddy-app-look").arg(bundle).arg(tools.octo()).arg(&shot)
        .env("PATH", crate::workspace::search_path());
    if let Some(hub) = &tools.app_hub {
        cmd.env("OCTOSENSE_APP_HUB", hub);
    }
    let out = cmd.output().map_err(|e| format!("could not run the app: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let text = if prefix.is_empty() { name_parts(project, &text) } else { text };
    let rel = shot.strip_prefix(project).map(|p| p.display().to_string()).unwrap_or_else(|_| shot.display().to_string());
    Ok(if shot.is_file() { format!("{}\nscreenshot: {rel}", text.trim()) } else { text.trim().to_string() })
}

const LOOK_SCRIPT: &str = r#"
bundle="$1"; octo="$2"; shot="$3"
work=$(mktemp -d "${TMPDIR:-/tmp}/octobuddy-app-look.XXXXXX") || exit 2
trap 'rm -rf "$work"' EXIT
cp -R "$bundle" "$work/bundle" || exit 2
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
if ! python3 "$octo" run "$work/bundle" --port "$port" --hidden --detach --timeout 20 > "$work/run.txt" 2>&1; then
  echo "the app did not start:"
  grep -E '\[E\]|splash:[0-9]+:|refused|did not' "$work/run.txt" | head -20
  exit 1
fi
# Its first data comes in (a fetch, a timer) before it is looked at.
sleep 4
python3 "$octo" shot "$port" "$shot" > /dev/null 2>&1
curl -s -m 5 "http://127.0.0.1:$port/snap" > "$work/snap.json"
curl -s -m 5 "http://127.0.0.1:$port/quit" > /dev/null 2>&1
sleep 1
echo "== script errors"
grep -hE '\[E\]|splash:[0-9]+:|on_render closure failed|callback error' "$work/.local-state/card-host.log" 2>/dev/null | head -20 || true
echo "== widgets on screen (type id "text")"
python3 - "$work/snap.json" <<'PY'
import json, sys
try:
    nodes = json.load(open(sys.argv[1])).get("s", [])
except Exception as e:
    print("(no widget list:", e, ")"); sys.exit(0)
skip = {"View", "SolidView", "RoundedView", "Window", "KeyboardView", "ScrollYView", "ScrollXView", "Splash"}
n = 0
for w in nodes:
    t = (w.get("t") or "").replace(chr(10), " ")
    if w.get("ty") in skip and not t:
        continue
    if w.get("ty") == "Splash":
        continue
    val = w.get("val")
    print(w.get("ty"), w.get("i"), repr(t[:60]) + (f" val={val!r}" if val else ""), w.get("r"))
    n += 1
    if n >= 150:
        print("…"); break
PY
"#;

/// Where an app's own files live, inside its project.
pub fn bundle(project: &str) -> PathBuf {
    Path::new(project).join("bundle")
}

/// An app built from parts keeps them here: `app/parts/*.splash`, put
/// together in name order as its `bundle/main.splash` (`assemble`), so
/// slices that own different parts (pages) work at once.
pub fn parts_dir(project: &str) -> PathBuf {
    Path::new(project).join("app/parts")
}

const GENERATED: &str = "// Generated by OctoBuddy from app/parts/*.splash, in name order: edit the parts, not this file.\n";

/// The app's parts, in name order.
fn parts(project: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(parts_dir(project)).into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "splash")).collect();
    out.sort();
    out
}

/// `bundle/main.splash` put together from the app's parts, when it has
/// them: whether that changed the file. Where each part's lines landed is
/// kept in `.octobuddy/parts-map.json`, for errors to name the part.
pub fn assemble(project: &str) -> Result<bool, String> {
    let parts = parts(project);
    if parts.is_empty() {
        return Ok(false);
    }
    let mut text = String::from(GENERATED);
    let mut map = Vec::new();
    for part in &parts {
        let rel = part.strip_prefix(project).map(|p| p.display().to_string()).unwrap_or_else(|_| part.display().to_string());
        let body = std::fs::read_to_string(part).map_err(|e| format!("{rel}: {e}"))?;
        text.push_str(&format!("// ---- {rel}\n"));
        let from = text.lines().count() + 1;
        text.push_str(body.trim_end());
        text.push('\n');
        map.push(serde_json::json!({"part": rel, "from": from, "to": text.lines().count()}));
    }
    let _ = std::fs::create_dir_all(Path::new(project).join(".octobuddy"));
    let _ = std::fs::write(Path::new(project).join(".octobuddy/parts-map.json"), serde_json::Value::Array(map).to_string());
    let main = bundle(project).join("main.splash");
    if std::fs::read_to_string(&main).ok().as_deref() == Some(text.as_str()) {
        return Ok(false);
    }
    std::fs::write(&main, text).map_err(|e| format!("could not write bundle/main.splash: {e}"))?;
    Ok(true)
}

/// `text` (a check's output) with each `splash:…:<line>:<col>` naming the
/// part and its line too, when the app is built from parts.
pub fn name_parts(project: &str, text: &str) -> String {
    let Ok(map) = std::fs::read_to_string(Path::new(project).join(".octobuddy/parts-map.json")) else { return text.to_string() };
    let Ok(serde_json::Value::Array(map)) = serde_json::from_str::<serde_json::Value>(&map) else { return text.to_string() };
    let find = |line: usize| map.iter().find(|m| m["from"].as_u64().is_some_and(|f| f as usize <= line) && m["to"].as_u64().is_some_and(|t| line <= t as usize))
        .map(|m| (m["part"].as_str().unwrap_or("").to_string(), line + 1 - m["from"].as_u64().unwrap_or(1) as usize));
    let mut out = String::new();
    for l in text.lines() {
        out.push_str(l);
        // `splash:<id>:<line>:<col>`: its line is main.splash's.
        if let Some(at) = l.find("splash:") {
            let mut fields = l[at + 7..].split(':');
            let (_, line) = (fields.next(), fields.next().and_then(|n| n.trim().parse::<usize>().ok()));
            if let Some((part, n)) = line.and_then(find) {
                out.push_str(&format!("  ← {part}:{n}"));
            }
        }
        out.push('\n');
    }
    out
}

pub fn is_app(project: &str) -> bool {
    bundle(project).join("manifest.json").is_file()
}

/// The design flow's checkout (its `tools/octo`, its template and docs) and
/// App Hub's (its `hub` and `card-host`).
#[derive(Clone, Debug)]
pub struct Tools {
    pub flow: PathBuf,
    pub app_hub: Option<PathBuf>,
}

impl Tools {
    pub(crate) fn octo(&self) -> PathBuf {
        self.flow.join("tools/octo")
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new("python3");
        cmd.arg(self.octo()).env("PATH", search_path());
        if let Some(hub) = &self.app_hub {
            cmd.env("OCTOSENSE_APP_HUB", hub);
        }
        cmd
    }
}

/// The OctoSense checkout OctoBuddy runs from, if any: `$OCTOSENSE_CHECKOUT`,
/// else the current folder (a shell started from its checkout), else an
/// `OctoSense` beside this repository's checkout.
fn octosense_checkout() -> Option<PathBuf> {
    let is_checkout = |dir: &Path| dir.join("native-apps.json").is_file();
    std::env::var_os("OCTOSENSE_CHECKOUT").map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .filter(|dir| is_checkout(dir))
        .or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).parent().map(|dir| dir.join("OctoSense")).filter(|dir| is_checkout(dir)))
}

/// The folder the sibling checkouts (the design flow, App Hub) sit in: the
/// one holding OctoSense's checkout, else the one holding this repository's.
fn beside_checkout() -> Option<PathBuf> {
    octosense_checkout().and_then(|dir| dir.canonicalize().ok()?.parent().map(Path::to_path_buf))
        .or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize().ok()?.parent().map(Path::to_path_buf))
}

/// `$OCTOBUDDY_DESIGN_FLOW`, else OctoScript-App-Design-Flow beside this
/// checkout; App Hub from `$OCTOSENSE_APP_HUB`, else a built one beside the
/// design flow (or in its quickstart's `octosense-ws`).
pub fn tools() -> Result<Tools, String> {
    let flow = std::env::var_os("OCTOBUDDY_DESIGN_FLOW").map(PathBuf::from)
        .or_else(|| beside_checkout().map(|dir| dir.join("OctoScript-App-Design-Flow")))
        .filter(|dir| dir.join("tools/octo").is_file())
        .ok_or_else(|| "OctoScript App Design Flow was not found: clone it (github.com/OctoSense-org/OctoScript-App-Design-Flow) \
beside OctoSense, or set OCTOBUDDY_DESIGN_FLOW to its folder".to_string())?;
    let built = |dir: &PathBuf| dir.join("target/release/hub").is_file() && dir.join("target/release/card-host").is_file();
    let app_hub = std::env::var_os("OCTOSENSE_APP_HUB").map(PathBuf::from).or_else(|| {
        let parent = flow.parent()?;
        [parent.join("OctoSense-App-Hub"), parent.join("octosense-ws/OctoSense-App-Hub")].into_iter().find(built)
    });
    Ok(Tools { flow, app_hub })
}

/// Makes `dir` (empty or new) an app from the design flow's template, named
/// after the folder, and commits it as the project's first commit.
/// A new app's parts (verified on card-host): the shared state, a page as a
/// template with its functions, and the root that instantiates the pages
/// and boots. A page more is a part more and a line in the root.
const SCAFFOLD: &[(&str, &str)] = &[
    ("00-state.splash", "// What every page shares: state, storage, helpers.\nlet items = []\nfn count_text(){ return \"\" + items.len() + \" items\" }\n"),
    ("10-home.splash", "// The home page: its template and its functions (its ids start with home_).\nfn home_add(){ items.push(\"x\") ui.home_count.set_text(count_text()) }\nlet HomePage = View{ width: Fill height: Fit flow: Down spacing: 8\n    home_count := Label{text: \"0 items\" draw_text.color: #x1c1c1e}\n    home_add_btn := Button{text: \"Add\" on_click: || home_add()}\n}\n"),
    ("90-root.splash", "// Boot, navigation and the root widget: every page instantiated here.\nfn boot(){ ui.home_count.set_text(count_text()) }\nstart_timeout(0.05, || boot())\nSolidView{ width: Fill height: Fill flow: Down padding: 16 draw_bg.color: #xffffff\n    home := HomePage{}\n}\n"),
];

pub fn create(dir: &Path) -> Result<(), String> {
    let tools = tools()?;
    if dir.read_dir().is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(format!("{} is not empty: choose an empty folder for a new app (or an app's own folder)", dir.display()));
    }
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "My App".into());
    let out = tools.command().arg("new").arg(dir).args(["--name", &name]).output().map_err(|e| format!("could not run tools/octo: {e}"))?;
    if !out.status.success() {
        return Err(format!("tools/octo new failed: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string());
    }
    // Built from parts, so its pages can be made at once.
    let parts = dir.join("app/parts");
    std::fs::create_dir_all(&parts).map_err(|e| e.to_string())?;
    for (name, text) in SCAFFOLD {
        std::fs::write(parts.join(name), text).map_err(|e| e.to_string())?;
    }
    assemble(&dir.to_string_lossy())?;
    let git = |args: &[&str]| Command::new("git").args(args).current_dir(dir).output().map_err(|e| e.to_string())
        .and_then(|o| if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) });
    if !dir.join(".git").exists() {
        git(&["init", "-q"])?;
    }
    git(&["add", "-A"])?;
    git(&["commit", "-q", "-m", "chore: 从 OctoSense 模板创建应用"]).map_err(|e| format!("the first commit failed: {e}"))
}

/// A manifest whose digest is not one (a peer rewrote the file from what it
/// was shown, where octos masks long hex strings) gets the bundle's real
/// digest again. A valid one is left alone.
pub fn repair_digest(project: &str) {
    let path = bundle(project).join("manifest.json");
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(mut manifest) = serde_json::from_str::<serde_json::Value>(&text) else { return };
    let digest = manifest.pointer("/integrity/bundle_blake3").and_then(|d| d.as_str()).unwrap_or("");
    if digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()) {
        return;
    }
    let Ok(real) = octosense_app_policy::digest_dir(&bundle(project)) else { return };
    manifest["integrity"]["bundle_blake3"] = serde_json::Value::String(real);
    if let Ok(out) = serde_json::to_string_pretty(&manifest) {
        let _ = std::fs::write(&path, format!("{out}\n"));
    }
}

/// The reference the inner loops read, copied into the project (they may not
/// read outside it): what the design flow says an app is made of.
const DOCS: &[&str] = &["AGENTS.md", "flows/script-app/FLOW.md", "docs/SCRIPT-API.md", "docs/CAPABILITIES.md", "docs/HOST-SERVICES.md"];

/// OctoBuddy's own reference, verified against card-host: the patterns an app
/// needs and the gotchas, short, so no inner loop learns them again from the
/// docs and other apps' sources (`splash_cookbook.md`).
pub const COOKBOOK: &str = include_str!("splash_cookbook.md");

/// Refreshes `.octobuddy/docs` and OctoBuddy's check script; the outer loop's
/// rules for this app, or why the design flow cannot be used.
pub fn prepare(project: &str) -> Result<String, String> {
    let tools = tools()?;
    let docs = Path::new(project).join(".octobuddy/docs");
    for doc in DOCS {
        let (from, to) = (tools.flow.join(doc), docs.join(doc.rsplit('/').next().unwrap_or(doc)));
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(&from, &to);
    }
    // With what OctoBuddy learned in its own runs since (`lessons`).
    let learned = crate::lessons::section("splash", "## Learned in OctoBuddy's own runs (verified with its probe; newest last)");
    let _ = std::fs::write(docs.join("SPLASH-COOKBOOK.md"), format!("{COOKBOOK}{learned}"));
    write_check(&tools)?;
    Ok(outer_rules(&tools))
}

/// Writes OctoBuddy's check script when a check is about to use it (an inner
/// loop can finish before its outer loop has started in this run).
pub fn ensure_check(command: &str) -> Result<(), String> {
    if !command.contains(CHECK) {
        return Ok(());
    }
    write_check(&tools()?)
}

/// Where OctoBuddy keeps commands of its own, on every check's PATH.
pub fn bin_dir() -> PathBuf {
    crate::model::data_dir().join("bin")
}

fn write_check(tools: &Tools) -> Result<(), String> {
    let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', r"'\''"));
    let hub = tools.app_hub.as_deref().map(|h| format!("OCTOSENSE_APP_HUB={}; export OCTOSENSE_APP_HUB\n", quote(h))).unwrap_or_default();
    let script = CHECK_SCRIPT.replace("{hub}", &hub).replace("{octo}", &quote(&tools.octo()));
    let path = bin_dir().join(CHECK);
    std::fs::create_dir_all(bin_dir()).map_err(|e| e.to_string())?;
    if std::fs::read_to_string(&path).ok().as_deref() != Some(script.as_str()) {
        std::fs::write(&path, script).map_err(|e| e.to_string())?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(())
}

/// A slice's check for an app. It works on a copy, so the project keeps the
/// person's files as they are: it runs the copy headless and fails on any
/// script error in the log, captures the screenshots the listing names but
/// the bundle lacks (App Hub's gate refuses a listing without them), then
/// runs the gate.
const CHECK_SCRIPT: &str = r#"#!/bin/sh
# OctoBuddy's check for an OctoSense script app (OctoBuddy rewrites this file).
set -u
bundle="${1:-bundle}"
[ -f "$bundle/manifest.json" ] || { echo "no $bundle/manifest.json here"; exit 2; }
{hub}octo={octo}
work=$(mktemp -d "${TMPDIR:-/tmp}/octobuddy-app-check.XXXXXX") || exit 2
trap 'rm -rf "$work"' EXIT
cp -R "$bundle" "$work/bundle" || exit 2
find "$work/bundle" -name .DS_Store -delete
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
echo "== a headless run"
if ! python3 "$octo" run "$work/bundle" --port "$port" --hidden --detach --timeout 20 > "$work/run.txt" 2>&1; then
  grep -E '^\[E\]|splash:[0-9]+:|refused|did not' "$work/run.txt" | head -20
  echo "the app did not start"
  exit 1
fi
# Its first data comes in (a fetch, a timer): errors in what runs then count
# too, as the live watch sees them once it is published.
sleep 5
python3 - "$work/bundle" > "$work/missing.txt" <<'PY'
import json, os, sys
listing = json.load(open(os.path.join(sys.argv[1], "listing.json")))
for shot in listing.get("screenshots", []):
    if not os.path.exists(os.path.join(sys.argv[1], shot)):
        print(shot)
PY
while read -r shot; do
  mkdir -p "$work/bundle/$(dirname "$shot")"
  python3 "$octo" shot "$port" "$work/bundle/$shot" > /dev/null 2>&1
done < "$work/missing.txt"
curl -s -m 5 "http://127.0.0.1:$port/quit" > /dev/null 2>&1
sleep 1
if grep -nE '\[E\]|splash:[0-9]+:|refused|on_render closure failed|callback error' "$work/.local-state/card-host.log"; then
  echo "the app logged script errors (above)"
  exit 1
fi
echo "it starts and draws with no script errors"
echo "== App Hub's gate"
status=0
python3 "$octo" check "$work/bundle" > "$work/gate.txt" 2>&1 || status=1
grep -vE '^octo: (hub stamp|/)' "$work/gate.txt"
exit $status
"#;

/// What the outer loop is told about an app project.
fn outer_rules(tools: &Tools) -> String {
    let flow = tools.flow.display();
    // OctoSense's system apps, when its checkout is at hand.
    let examples = octosense_checkout().map(|dir| dir.join("apps")).filter(|apps| apps.join("camera/bundle").is_dir())
        .and_then(|apps| apps.canonicalize().ok())
        .map(|apps| format!(" Working apps, only for what the cookbook lacks (it lists what not to copy from them): {}/*/bundle/main.splash (camera, maps, news, photos).", apps.display()))
        .unwrap_or_default();
    format!("\n\nOCTOSENSE APP. This project is an OctoSense script app: `bundle/` holds it (manifest.json, listing.json, \
main.splash, assets/), in OctoScript App Design Flow's format. The person says what they want; you plan it and \
your inner loops build it. The docs you plan with are {flow}/AGENTS.md, {flow}/flows/script-app/FLOW.md and \
{flow}/docs/SCRIPT-API.md (CAPABILITIES.md or HOST-SERVICES.md when the app needs more than storage), with \
copies in .octobuddy/docs/ that the inner loops read, and OctoBuddy's own .octobuddy/docs/SPLASH-COOKBOOK.md: \
verified patterns and gotchas (pages, lists, forms, storage, dates, money, bars, the errors mistakes give). Point \
briefs at its sections instead of restating them, and at SCRIPT-API.md only for what it lacks. Your first message carries OctoBuddy's CONTEXT: memory \
hits and an index of those copies, each heading with its line number. Read only the sections your plan needs \
(Read with offset and limit), not whole docs, and do not search memory again for what the CONTEXT has: \
planning time is the person's waiting time.{examples} Point each brief at the sections it needs (file and \
heading), and quote the gotchas that matter.\n\
- Never invent an API: only what those docs show or a system app uses (name it in the brief).\n\
- Parts, so slices run at once: when the project has app/parts/, bundle/main.splash is generated from \
app/parts/*.splash (in name order) before every check, preview and publish: no one edits it. 00-state.splash \
holds what every page shares (state, storage, helpers); one NN-<page>.splash per page holds that page as a \
template (`let DetailPage = View{{…}}`) and its functions, its ids prefixed by its page; 90-root.splash boots, \
navigates and instantiates every page (`detail := DetailPage{{}}`). Plan: wave 1, one slice, 00-state and \
90-root and a stub part for every page (a template with a placeholder label); then one slice per page, each \
owning only its part, all in one wave. A check's error lines name the part and its line. A project without \
app/parts is one file: one slice at a time.\n\
- Try, don't read: never read the runtime's source (makepad, App Hub) to learn how Splash behaves, yourself or \
through a subagent: it takes tens of minutes. Look it up in the cookbook, or try it: octobuddy_app_probe runs a \
few lines of Splash headless (errors and widgets back in seconds). Tell inner loops to do the same.\n\
- main.splash is one file: one inner loop owns it at a time. A later change to it goes to the same inner \
loop (message it) or to a later wave. The listing (listing.json, every placeholder) and the icon \
(assets/icon.svg) can be a slice of their own.\n\
- The manifest asks only for the capabilities a screen uses; every https host is declared (network.hosts), \
never http://; no password, PIN, code or login field; no key or token in the bundle. Leave `integrity` alone.\n\
- Every slice's `check` is `{CHECK}`: a headless run of a copy of the bundle that fails on any script error, \
then App Hub's gate.\n\
- An inner loop cannot run the app: its sandbox has no card-host, no window and no GPU, and its shell \
writes only inside the project (not /tmp). OctoBuddy runs the app for it, outside the sandbox: the check above, and for \
an inner loop on Claude Code `octobuddy_app_look` (script errors, a screenshot, the widgets). So plan no slice \
that runs card-host or a probe script itself, and take a passing check as the run's evidence.\n\
- OctoBuddy runs the app in its Preview panel and reloads it when its files change: the person tries it \
there. Store screenshots and publishing come later, from the person.\n\
- Tests come after the app first starts, not before: put an end-to-end test slice in a later wave than the \
app's first runnable version, and have it written from the real screen (the widgets' types, ids and texts \
it sees), not from the contract alone. Start it with the 3 scenarios that matter most; add more once those pass.\n\
- What OctoSense apps teach (OctoScript idioms that work, pitfalls, how /snap shows widgets) is shared by every \
app, in mempal's wing \"{SHARED_WING}\" (the CONTEXT has its hits for this request): save there what this app \
taught (room \"conventions\" or \"pitfalls\"), so the next app's plan starts from it.")
}

/// mempal's wing for what every OctoSense app teaches.
pub const SHARED_WING: &str = "octosense-apps";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_is_a_project_with_a_bundle_manifest() {
        let dir = std::env::temp_dir().join(format!("octobuddy-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bundle")).unwrap();
        assert!(!is_app(&dir.to_string_lossy()));
        std::fs::write(dir.join("bundle/manifest.json"), "{}").unwrap();
        assert!(is_app(&dir.to_string_lossy()));
        assert!(create(&dir).is_err(), "a folder with files in it is not made an app");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_masked_digest_is_made_right() {
        let dir = std::env::temp_dir().join(format!("octobuddy-digest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bundle")).unwrap();
        std::fs::write(dir.join("bundle/main.splash"), "View{}").unwrap();
        std::fs::write(dir.join("bundle/manifest.json"), r#"{"id":"x","integrity":{"bundle_blake3":"[hex-redacted]"}}"#).unwrap();
        let project = dir.to_string_lossy().into_owned();
        repair_digest(&project);
        let m: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("bundle/manifest.json")).unwrap()).unwrap();
        let d = m["integrity"]["bundle_blake3"].as_str().unwrap().to_string();
        assert_eq!(d, octosense_app_policy::digest_dir(&dir.join("bundle")).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_check_script_names_its_tools() {
        let tools = Tools { flow: PathBuf::from("/flow"), app_hub: Some(PathBuf::from("/hub it's")) };
        let script = CHECK_SCRIPT.replace("{hub}", "H\n").replace("{octo}", &format!("'{}'", tools.octo().display()));
        assert!(script.contains("octo='/flow/tools/octo'"));
        assert!(script.starts_with("#!/bin/sh"));
        assert!(outer_rules(&tools).contains(CHECK));
    }
}

#[cfg(test)]
mod parts_tests {
    use super::*;

    /// The Python OctoBuddy writes for agents parses (a newline written into
    /// a string literal broke two of them once).
    #[test]
    fn its_python_scripts_parse() {
        for (name, source) in [("octobuddy-app", APP_CLI), ("octobuddy-mcp-shim", crate::providers::MCP_SHIM)] {
            let mut child = Command::new("python3").args(["-c", "import ast, sys; ast.parse(sys.stdin.read())"])
                .stdin(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
            use std::io::Write;
            child.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(out.status.success(), "{name}: {}", String::from_utf8_lossy(&out.stderr));
        }
    }

    #[test]
    fn parts_become_main_splash_and_errors_name_their_part() {
        let dir = std::env::temp_dir().join(format!("octobuddy-parts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bundle")).unwrap();
        std::fs::create_dir_all(dir.join("app/parts")).unwrap();
        for (name, text) in SCAFFOLD {
            std::fs::write(dir.join("app/parts").join(name), text).unwrap();
        }
        let project = dir.to_string_lossy().into_owned();
        assert!(assemble(&project).unwrap(), "written");
        assert!(!assemble(&project).unwrap(), "unchanged: not written again");
        let main = std::fs::read_to_string(dir.join("bundle/main.splash")).unwrap();
        assert!(main.starts_with(GENERATED) && main.contains("// ---- app/parts/10-home.splash") && main.contains("home := HomePage{}"));
        // The line of `fn home_add` in main.splash, as an error names it.
        let line = main.lines().position(|l| l.starts_with("fn home_add")).unwrap() + 1;
        let named = name_parts(&project, &format!("[E] splash:123:{line}:8 - widget 'x' not found in tree"));
        assert!(named.contains("← app/parts/10-home.splash:2"), "{named}");
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod probe_tests {
    /// A real headless run (card-host, the design flow): ignored by default;
    /// `OCTOBUDDY_PROBE_PROJECT=<an app project>` names the app.
    #[test]
    #[ignore]
    fn a_probe_answers_in_seconds() {
        let project = std::env::var("OCTOBUDDY_PROBE_PROJECT").expect("an app project");
        let started = std::time::Instant::now();
        let out = super::probe(&project, "fn boot(){ ui.missing.set_text(\"x\") }\nstart_timeout(0.05, || boot())\nView{ height: Fit label := Label{text: \"hi\" draw_text.color: #x1c1c1e} }\n").unwrap();
        eprintln!("{out}\n({:?})", started.elapsed());
        assert!(out.contains("missing"), "its error comes back: {out}");
        assert!(out.contains("hi"), "and its widgets: {out}");
        assert!(started.elapsed().as_secs() < 30);
    }

    /// A new app (its parts put together) runs with no script errors.
    #[test]
    #[ignore]
    fn a_new_app_from_parts_runs() {
        let dir = std::env::temp_dir().join(format!("octobuddy-new-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        super::create(&dir).unwrap();
        let out = super::look(&dir.to_string_lossy()).unwrap();
        eprintln!("{out}");
        assert!(!out.contains("[E]"), "no script errors: {out}");
        assert!(out.contains("home_count") && out.contains("0 items"), "its home page: {out}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
