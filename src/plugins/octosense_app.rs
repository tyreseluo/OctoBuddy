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
it lacks, and other apps' sources not at all unless both lack it. When you finish, OctoBuddy runs the app headless and App Hub's \
gate (octobuddy-app-check) and shows it running in its preview; the result comes back to you if it fails.";

/// The same for an inner loop on Claude Code, which has OctoBuddy's tools to
/// run the app itself (`octobuddy_check`, `octobuddy_app_look`).
pub const INNER_NOTE_TOOLS: &str = "\nThis project is an OctoSense app. Its hosts (card-host, the shell) cannot run in your \
sandbox, so do not look for them or install them: OctoBuddy runs the app for you, outside it, through two tools. \
`octobuddy_app_look` runs the app headless now and gives you its script errors, a screenshot (a PNG under \
.octobuddy/shots/, which you can read) and the widgets on screen (type, id, text: what a test finds them by). \
`octobuddy_check` runs your slice's own check command, the one OctoBuddy runs when you finish, and gives you its \
output. Use them as you work: after each meaningful change, look; before you report, run your check until it \
passes. Start with .octobuddy/docs/SPLASH-COOKBOOK.md: OctoBuddy's verified patterns for what an app needs (pages, \
lists, forms, storage, dates, money, bars, gotchas and the errors they give). Read .octobuddy/docs/SCRIPT-API.md \
only for what it lacks, and other apps' sources not at all unless both lack it.";

/// The command a slice of an app is checked with.
pub const CHECK: &str = "octobuddy-app-check";

/// Runs `project`'s app headless once (a copy of its bundle): its script
/// errors, a screenshot under `<project>/.octobuddy/shots/`, and the widgets
/// on screen, compact. Blocks (seconds): call it off the UI thread.
pub fn look(project: &str) -> Result<String, String> {
    let tools = tools()?;
    let shots = Path::new(project).join(".octobuddy/shots");
    std::fs::create_dir_all(&shots).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let shot = shots.join(format!("{stamp}.png"));
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(LOOK_SCRIPT).arg("octobuddy-app-look").arg(bundle(project)).arg(tools.octo()).arg(&shot)
        .env("PATH", crate::workspace::search_path());
    if let Some(hub) = &tools.app_hub {
        cmd.env("OCTOSENSE_APP_HUB", hub);
    }
    let out = cmd.output().map_err(|e| format!("could not run the app: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
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
    t = (w.get("t") or "").replace("
", " ")
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
    let _ = std::fs::write(docs.join("SPLASH-COOKBOOK.md"), COOKBOOK);
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
