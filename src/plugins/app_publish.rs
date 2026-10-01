//! Publishing an app OctoBuddy built to a local App Hub: a signed catalog on
//! this device that OctoSense's App Hub reads when the shell starts with
//! `OCTOSENSE_HUB=<its folder>` and `OCTOSENSE_HUB_ANCHOR=<its anchor>`
//! (OctoScript App Design Flow's PUBLISHING §4, the store path rehearsed
//! locally). The shell installs only signed bundles listed in a catalog its
//! anchor signed, so OctoBuddy keeps a throwaway anchor and keys of its own
//! (under its data, never in a project), and publishes a signed COPY of the
//! bundle: the project's own stays unsigned, as the preview and the checks
//! need it.
//!
//! A publish: the version goes up when that version is in the catalog
//! already (committed in the project), the copy gets the screenshots its
//! listing names (from a headless run), then `hub stamp`, `sign-manifest`,
//! `check`, `publish` and `verify`.
use super::octosense_app::{self, Tools};
use crate::model::data_dir;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The local hub's folder.
pub fn hub_dir() -> PathBuf {
    data_dir().join("hub")
}

/// The anchor the shell must trust for it, once made.
pub fn anchor() -> Option<String> {
    std::fs::read_to_string(hub_dir().join("keys/anchor.pub")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Whether this shell reads the local hub now (it is started with it).
pub fn shell_reads_it() -> bool {
    let hub = std::env::var_os("OCTOSENSE_HUB").map(PathBuf::from);
    let same = hub.is_some_and(|h| h.canonicalize().ok() == hub_dir().canonicalize().ok());
    same && std::env::var("OCTOSENSE_HUB_ANCHOR").ok().map(|a| a.trim().to_string()) == anchor()
}

/// What was published: the app's id, name and version.
#[derive(Clone, Debug)]
pub struct Published {
    pub id: String,
    pub name: String,
    pub version: String,
    /// The commit that raised the version, if it had to.
    pub bumped: Option<String>,
    pub log: String,
}

fn manifest(project: &Path) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(project.join("bundle/manifest.json")).map_err(|e| format!("no bundle/manifest.json: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("bundle/manifest.json is not JSON: {e}"))
}

/// `1.2.3` one patch up.
fn next_patch(version: &str) -> String {
    let mut parts: Vec<u64> = version.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    parts.resize(3, 0);
    parts[2] += 1;
    format!("{}.{}.{}", parts[0], parts[1], parts[2])
}

/// The versions of `id` the local catalog lists.
fn listed(id: &str) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(hub_dir().join("catalog.json")) else { return Vec::new() };
    let Ok(catalog) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    let mut out = Vec::new();
    let mut walk = vec![&catalog];
    // Wherever the catalog keeps its entries: any object with this id and a version.
    while let Some(v) = walk.pop() {
        match v {
            serde_json::Value::Object(o) => {
                if o.get("id").and_then(|i| i.as_str()) == Some(id) {
                    if let Some(ver) = o.get("version").and_then(|v| v.as_str()) {
                        out.push(ver.to_string());
                    }
                }
                walk.extend(o.values());
            }
            serde_json::Value::Array(a) => walk.extend(a.iter()),
            _ => {}
        }
    }
    out
}

/// Publishes `project`'s app to the local hub (blocks: run it off the UI thread).
pub fn publish(project: &str) -> Result<Published, String> {
    let tools = octosense_app::tools()?;
    let hub = tools.app_hub.clone().map(|d| d.join("target/release/hub")).filter(|h| h.is_file())
        .ok_or("App Hub's `hub` command was not found: build OctoSense-App-Hub (cargo build --release) beside OctoSense, or set OCTOSENSE_APP_HUB")?;
    let dir = Path::new(project);
    let m = manifest(dir)?;
    let id = m["id"].as_str().ok_or("the manifest has no id")?.to_string();
    let name = m["name"].as_str().unwrap_or(&id).to_string();
    let mut version = m["version"].as_str().ok_or("the manifest has no version")?.to_string();
    // A version the catalog has already: one up, committed in the project.
    let mut bumped = None;
    let taken = listed(&id);
    if taken.contains(&version) {
        while taken.contains(&version) {
            version = next_patch(&version);
        }
        let path = dir.join("bundle/manifest.json");
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let old = format!("\"version\": \"{}\"", m["version"].as_str().unwrap_or(""));
        let text = if text.contains(&old) {
            text.replacen(&old, &format!("\"version\": \"{version}\""), 1)
        } else {
            let mut v = m.clone();
            v["version"] = serde_json::json!(version);
            serde_json::to_string_pretty(&v).map_err(|e| e.to_string())? + "\n"
        };
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
        let message = format!("chore(release): 版本 {version}");
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(dir).output();
        let _ = git(&["add", "bundle/manifest.json"]);
        if git(&["commit", "-q", "-m", &message]).is_ok_and(|o| o.status.success()) {
            bumped = git(&["rev-parse", "--short", "HEAD"]).ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        }
    }
    let script = write_script(&tools)?;
    let out = Command::new("sh").arg(&script).arg(project).arg(hub_dir()).arg(&hub)
        .env("PATH", crate::workspace::search_path()).output().map_err(|e| format!("could not run the publish: {e}"))?;
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    if !out.status.success() {
        return Err(log);
    }
    Ok(Published { id, name, version, bumped, log })
}

fn write_script(tools: &Tools) -> Result<PathBuf, String> {
    let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', r"'\''"));
    // `tools/octo run` finds card-host through App Hub's folder.
    let hub = tools.app_hub.as_deref().map(|h| format!("OCTOSENSE_APP_HUB={}; export OCTOSENSE_APP_HUB\n", quote(h))).unwrap_or_default();
    let script = PUBLISH_SCRIPT.replace("{hub}", &hub).replace("{octo}", &quote(&tools.flow.join("tools/octo")));
    let path = octosense_app::bin_dir().join("octobuddy-app-publish");
    std::fs::create_dir_all(octosense_app::bin_dir()).map_err(|e| e.to_string())?;
    std::fs::write(&path, script).map_err(|e| e.to_string())?;
    Ok(path)
}

/// `sh octobuddy-app-publish <project> <hub dir> <hub>`.
const PUBLISH_SCRIPT: &str = r#"#!/bin/sh
# OctoBuddy's publish to its local App Hub (OctoBuddy rewrites this file).
set -u
project="$1"; hubdir="$2"; hub="$3"
{hub}octo={octo}
keys="$hubdir/keys"
mkdir -p "$keys" "$hubdir/staging" || exit 2
chmod 700 "$keys"
umask 077
# Once: a throwaway anchor (this device's only), its working key, a publisher key.
if [ ! -f "$keys/anchor.key" ]; then
  "$hub" keygen "$keys/anchor.key" > "$keys/anchor.pub" || exit 2
  "$hub" keygen "$keys/working.key" > /dev/null || exit 2
  "$hub" keygen "$keys/publisher.key" > /dev/null || exit 2
  "$hub" certify --anchor "$keys/anchor.key" --working "$keys/working.key" > "$keys/working.cert" || exit 2
  echo "made a local anchor and keys in $keys"
fi
umask 022
anchor=$(cat "$keys/anchor.pub"); cert=$(cat "$keys/working.cert")
pub=$("$hub" pubkey "$keys/publisher.key") || exit 2
# The copy that is signed: the project's bundle stays unsigned.
b="$hubdir/staging/bundle"
rm -rf "$b"
cp -R "$project/bundle" "$b" || exit 2
find "$b" -name .DS_Store -delete
# The screenshots its listing names, from a headless run, when it lacks them.
missing=$(python3 - "$b" <<'PY'
import json, os, sys
listing = json.load(open(os.path.join(sys.argv[1], "listing.json")))
for shot in listing.get("screenshots", []):
    if not os.path.exists(os.path.join(sys.argv[1], shot)):
        print(shot)
PY
)
if [ -n "$missing" ]; then
  run="$hubdir/staging/run"
  rm -rf "$run"; cp -R "$b" "$run"
  port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
  if python3 "$octo" run "$run" --port "$port" --hidden --detach --timeout 20 > "$hubdir/staging/run.txt" 2>&1; then
    for shot in $missing; do
      mkdir -p "$b/$(dirname "$shot")"
      python3 "$octo" shot "$port" "$b/$shot" > /dev/null 2>&1 && echo "screenshot $shot taken"
    done
    curl -s -m 5 "http://127.0.0.1:$port/quit" > /dev/null 2>&1
  else
    echo "the app did not start headless, so its screenshots could not be taken:"
    tail -5 "$hubdir/staging/run.txt"
  fi
  rm -rf "$run"
fi
echo "== stamp and sign"
"$hub" stamp "$b" || exit 1
"$hub" sign-manifest "$b" --key "$keys/publisher.key" --key-id local || exit 1
echo "== App Hub's gate"
"$hub" check "$b" --publisher-key "local=$pub" --catalog "$hubdir/catalog.json" || exit 1
echo "== publish"
commit=$(git -C "$project" rev-parse HEAD 2>/dev/null || echo "")
"$hub" publish "$b" --catalog "$hubdir/catalog.json" --key "$keys/working.key" --anchor-cert "$cert" \
  --publisher local --publisher-key "local=$pub" ${commit:+--commit "$commit"} --out "$hubdir" || exit 1
"$hub" verify "$hubdir/catalog.json" --anchor "$anchor" || exit 1
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_version_goes_one_patch_up() {
        assert_eq!(next_patch("0.1.0"), "0.1.1");
        assert_eq!(next_patch("1.2"), "1.2.1");
    }
}
