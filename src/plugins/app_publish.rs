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
//! A publish: bound to the app as the person was shown it (one that
//! changed since is refused), not done again when that content is the
//! catalog's latest already; then the version goes up when that version is
//! in the catalog already (committed in the project), the copy gets the
//! screenshots its listing names (from a headless run), then `hub stamp`,
//! `sign-manifest`, `check`, `publish` and `verify`; then the catalog is
//! read back, its receipt settled (`receipts`).
use super::octosense_app::{self, Tools};
use crate::i18n;
use crate::model::data_dir;
use crate::receipts::{self, Check, Kind, Prior, Receipt, Snapshot, Status};
use serde_json::Value;
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
    /// As the catalog lists it, read back (else the one it meant).
    pub version: String,
    /// The commit that raised the version, if it had to.
    pub bumped: Option<String>,
    pub log: String,
    /// Its receipt, and the line that says how the catalog read back.
    pub receipt: String,
    pub line: String,
    /// The catalog read back as meant.
    pub confirmed: bool,
    /// That content was the catalog's latest already: nothing done again.
    pub again: bool,
}

fn manifest(project: &Path) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(project.join("bundle/manifest.json")).map_err(|e| format!("no bundle/manifest.json: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("bundle/manifest.json is not JSON: {e}"))
}

/// The app's id, as its manifest names it.
pub fn app_id(project: &str) -> Result<String, String> {
    manifest(Path::new(project))?["id"].as_str().map(String::from).ok_or_else(|| "the manifest has no id".into())
}

/// A JSON value as a digest takes it: keys in order, and what says nothing
/// (null, empty) left out, as App Hub writes the defaults it reads in.
fn canonical(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let fields: Vec<String> = keys.into_iter().filter_map(|k| {
                let inner = canonical(&o[k]);
                (!matches!(inner.as_str(), "null" | "[]" | "{}")).then(|| format!("{}:{inner}", Value::String(k.clone())))
            }).collect();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(a) => format!("[{}]", a.iter().map(canonical).collect::<Vec<_>>().join(",")),
        other => other.to_string(),
    }
}

/// What an app's bundle holds, as a digest: its main.splash, its listing,
/// its manifest but for `version` and `integrity` (a publish raises the one
/// and stamps the other). The same app has the same, published or not.
pub fn content_from(read: impl Fn(&str) -> Option<Vec<u8>>) -> String {
    let json = |file: &str, leave: &[&str]| match read(file) {
        Some(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(mut v) => {
                if let Some(o) = v.as_object_mut() {
                    for k in leave {
                        o.remove(*k);
                    }
                }
                canonical(&v).into_bytes()
            }
            Err(_) => bytes,
        },
        None => Vec::new(),
    };
    receipts::digest(&[&json("manifest.json", &["version", "integrity"]), &read("main.splash").unwrap_or_default(), &json("listing.json", &[])])
}

/// `project`'s app as a publish would publish it (its main.splash put
/// together from its parts).
pub fn content(project: &str) -> String {
    let bundle = octosense_app::bundle(project);
    let main = octosense_app::assembled(project);
    content_from(|f| match (f, &main) {
        ("main.splash", Some(text)) => Some(text.clone().into_bytes()),
        _ => std::fs::read(bundle.join(f)).ok(),
    })
}

/// A bundle folder's (a published copy's).
pub fn content_in(dir: &Path) -> String {
    content_from(|f| std::fs::read(dir.join(f)).ok())
}

/// The app as a Publish binds to it: its commit, its content, its version.
pub fn snapshot(project: &str) -> Snapshot {
    Snapshot {
        head: crate::workspace::git(project, &["rev-parse", "HEAD"]).ok(),
        content: content(project),
        version: manifest(Path::new(project)).ok().and_then(|m| m["version"].as_str().map(String::from)),
    }
}

/// What a press of Publish comes to, before anything runs.
#[derive(Debug, PartialEq)]
enum Decision {
    Go,
    /// The app is not as the person was shown it: what moved.
    Stale(Vec<String>),
    /// That content is the catalog's latest already, as its receipt says.
    Already(Receipt),
    /// A publish of it is not known done or not: App Hub first.
    Unsettled(Receipt),
}

/// `expected`: the app as the person was shown it (none: a drill); `now`:
/// as it is; `prior`: what the ledger has of its content.
fn decide(expected: Option<&Snapshot>, now: &Snapshot, prior: Option<&Prior>) -> Decision {
    if let Some(moved) = expected.map(|e| receipts::moved(e, now)).filter(|m| !m.is_empty()) {
        return Decision::Stale(moved);
    }
    match prior {
        Some(Prior::Done(r)) => Decision::Already(r.as_ref().clone()),
        Some(Prior::Open(r)) => Decision::Unsettled(r.as_ref().clone()),
        None => Decision::Go,
    }
}

/// The digest `hub stamp` gave the copy just signed.
fn staged_digest() -> Option<String> {
    let text = std::fs::read_to_string(hub_dir().join("staging/bundle/manifest.json")).ok()?;
    serde_json::from_str::<Value>(&text).ok()?["integrity"]["bundle_blake3"].as_str().map(String::from)
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

/// Publishes `project`'s app to the local hub as `kind`, for `by` (blocks:
/// run it off the UI thread). `expected`: the app as the person was shown
/// it when they pressed Publish (none: a drill); one that moved since is
/// refused, and content the catalog serves already is not published again.
pub fn publish(project: &str, kind: Kind, expected: Option<&Snapshot>, by: &str) -> Result<Published, String> {
    let tools = octosense_app::tools()?;
    let hub = tools.app_hub.clone().map(|d| d.join("target/release/hub")).filter(|h| h.is_file())
        .ok_or("App Hub's `hub` command was not found: build OctoSense-App-Hub (cargo build --release) beside OctoSense, or set OCTOSENSE_APP_HUB")?;
    let dir = Path::new(project);
    let m = manifest(dir)?;
    let id = m["id"].as_str().ok_or("the manifest has no id")?.to_string();
    let name = m["name"].as_str().unwrap_or(&id).to_string();
    let now = snapshot(project);
    // A publish of it left open: read back first (never published again).
    let mut prior = receipts::prior(kind, project, &now.content);
    if let Decision::Unsettled(mut open) = decide(expected, &now, prior.as_ref()) {
        receipts::reconcile(&mut open);
        prior = receipts::prior(kind, project, &now.content);
    }
    match decide(expected, &now, prior.as_ref()) {
        Decision::Go => {}
        Decision::Stale(_) => {
            let draft = receipts::draft(kind, project, &now.content, expected.cloned().unwrap_or_default(), by);
            let moved = receipts::still(&draft, &now).err().unwrap_or_default();
            return Err(i18n::pick(format!("Out of date: the app changed since you looked ({moved}) — review it again, then publish."),
                format!("已过期：你看过之后应用又变了（{moved}）——请重新查看后再发布。")));
        }
        Decision::Already(r) => {
            let version = r.result.clone().unwrap_or_default();
            return Ok(Published {
                id, name, version: version.clone(), bumped: None, log: String::new(), receipt: r.id,
                line: i18n::pick(format!("already published as {version}, not repeated"), format!("已经发布为 {version}，没有重复发布")), confirmed: true, again: true,
            });
        }
        Decision::Unsettled(r) => {
            return Err(i18n::pick(format!("A publish of this content is not confirmed ({}): check App Hub before publishing again.", receipts::line(&r)),
                format!("这份内容之前的一次发布还没确认（{}）：先去 App Hub 看看，再决定是否重新发布。", receipts::line(&r))));
        }
    }
    let mut r = receipts::prepare(kind, project, &now.content, expected.cloned().unwrap_or_else(|| now.clone()), by)
        .map_err(|_| i18n::t("Another publish of this content is under way.", "这份内容的另一次发布正在进行。").to_string())?;
    // Built from parts: its main.splash as they are now.
    if let Err(err) = octosense_app::assemble(project) {
        receipts::refuse(&mut r, &err);
        return Err(err);
    }
    let Some(mut version) = m["version"].as_str().map(String::from) else {
        receipts::refuse(&mut r, "the manifest has no version");
        return Err("the manifest has no version".into());
    };
    // A version the catalog has already: one up, committed in the project.
    let mut bumped = None;
    let taken = listed(&id);
    if taken.contains(&version) {
        while taken.contains(&version) {
            version = next_patch(&version);
        }
        let path = dir.join("bundle/manifest.json");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let old = format!("\"version\": \"{}\"", m["version"].as_str().unwrap_or(""));
        let text = if text.contains(&old) {
            text.replacen(&old, &format!("\"version\": \"{version}\""), 1)
        } else {
            let mut v = m.clone();
            v["version"] = serde_json::json!(version);
            serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
        };
        if let Err(err) = std::fs::write(&path, text) {
            receipts::refuse(&mut r, &err.to_string());
            return Err(err.to_string());
        }
        let message = format!("chore(release): 版本 {version}");
        let git = |args: &[&str]| Command::new("git").args(args).current_dir(dir).output();
        let _ = git(&["add", "bundle/manifest.json"]);
        if git(&["commit", "-q", "-m", &message]).is_ok_and(|o| o.status.success()) {
            bumped = git(&["rev-parse", "--short", "HEAD"]).ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        }
    }
    // The version it means, should it be cut off; then the commit the catalog will name.
    r.result = Some(version.clone());
    receipts::executing(&mut r);
    let head = crate::workspace::git(project, &["rev-parse", "HEAD"]).ok();
    let ran = write_script(&tools).and_then(|script| Command::new("sh").arg(&script).arg(project).arg(hub_dir()).arg(&hub)
        .env("PATH", crate::workspace::search_path()).output().map_err(|e| format!("could not run the publish: {e}")));
    let out = match ran {
        Ok(out) => out,
        Err(err) => {
            receipts::refuse(&mut r, &err);
            return Err(err);
        }
    };
    let log = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)).trim().to_string();
    // What the catalog has now, read back whatever the script said.
    let (found, mut checks) = receipts::read_back_publish(&hub_dir(), &id, Some(&version), &now.content, staged_digest().as_deref(), head.as_deref());
    if !out.status.success() {
        checks.insert(0, Check::new("script", Some(false), out.status.code().map(|c| format!("exit {c}")).unwrap_or_default()));
        receipts::settle(&mut r, checks, found);
        return Err(log);
    }
    receipts::settle(&mut r, checks, found.clone());
    Ok(Published {
        id, name, version: found.unwrap_or(version), bumped, log, receipt: r.id.clone(), line: receipts::line(&r),
        confirmed: r.status == Status::Confirmed, again: false,
    })
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

    fn snap(head: &str, content: &str) -> Snapshot {
        Snapshot { head: Some(head.into()), content: content.into(), version: Some("1.0.0".into()) }
    }

    /// Publish twice: the second finds it done, nothing bumped or published;
    /// a press on an app that changed since it was shown is refused first.
    #[test]
    fn a_publish_is_refused_out_of_date_and_not_repeated() {
        let shown = snap("a1", "c1");
        let done = Receipt { status: Status::Confirmed, result: Some("1.0.3".into()), ..receipts::draft(Kind::Publish, "/p", "c1", shown.clone(), "person") };
        assert_eq!(decide(Some(&shown), &shown, None), Decision::Go);
        assert_eq!(decide(Some(&shown), &shown, Some(&Prior::Done(Box::new(done.clone())))), Decision::Already(done.clone()));
        let open = Receipt { status: Status::Unknown, ..done.clone() };
        assert_eq!(decide(Some(&shown), &shown, Some(&Prior::Open(Box::new(open.clone())))), Decision::Unsettled(open));
        // Changed since: stale, whatever the ledger has.
        assert!(matches!(decide(Some(&shown), &snap("a2", "c1"), Some(&Prior::Done(Box::new(done)))), Decision::Stale(m) if m.len() == 1));
        assert!(matches!(decide(Some(&shown), &snap("a1", "c2"), None), Decision::Stale(_)));
        // A drill binds to nothing shown.
        assert_eq!(decide(None, &snap("a2", "c2"), None), Decision::Go);
    }

    #[test]
    fn content_leaves_out_the_version_and_what_app_hub_adds() {
        let read = |manifest: &'static str| move |f: &str| match f {
            "manifest.json" => Some(manifest.as_bytes().to_vec()),
            "main.splash" => Some(b"View{}".to_vec()),
            _ => None,
        };
        let plain = content_from(read(r#"{"id": "a", "version": "1.0.0", "capabilities": ["storage"]}"#));
        assert_eq!(plain, content_from(read(r#"{"capabilities": ["storage"], "id": "a", "version": "1.0.9", "agent": null, "compute": {"memory_bytes": null}, "integrity": {"bundle_blake3": "x"}}"#)));
        assert_ne!(plain, content_from(read(r#"{"id": "a", "version": "1.0.0", "capabilities": ["net"]}"#)));
    }
}
