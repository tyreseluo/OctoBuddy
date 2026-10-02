//! The production loop (`card-loop`): OctoBuddy keeps watch over the version
//! of an app it published to the local App Hub — the one people run — and
//! says when it breaks (GOSIM 2026's "production-loop agent": read its own
//! runtime signals, find a failing card, repair it, publish it again).
//!
//! The watch: every few minutes (`watch.json`'s `every`) it runs the
//! published copy headless on live data, as the app check does, and reads
//! what it showed: whether it started, its script errors, the widgets on
//! screen, a screenshot. The first healthy run of a version is its baseline;
//! a later run is judged against it ([`judge`]): it did not start (down), it
//! has script errors (broken), its key widgets are gone, its content shrank or
//! it shows a failure it did not before (degraded). Each run is a line of
//! `<project>/.octobuddy/card-loop/health.jsonl`.
//!
//! An incident: from the run that found it ill to the one that finds it
//! well. Its session hears both ends. The Live page (the sidebar's, every
//! published app on it) hands it to that session's outer loop to repair
//! (`repair_brief`; or at once, with Auto repair on), and publishes the
//! fixed version once the project's app differs from the published one; the
//! next run checks it, and the incident closes with its story (found,
//! handed over, published, well again).
//!
//! A run keeps the app's storage (`card-loop/state/`), as a device does, so
//! a fixed version that keeps its last data can show it when its API is
//! down; a failure said with the content kept is judged coping, not broken.
//!
//! Drills for the demo: `offline` runs the copy with its network hosts
//! swapped for one that never answers, as if its API were down;
//! `offline-fresh` does it on a new device (no storage kept). The published
//! copy itself is never touched.
use crate::model::now_secs;
use octosense_app_peers::host_tools::{ToolOutcome, ToolReply};
use crate::{i18n, tapped, OctoBuddyView};
use makepad_widgets::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

/// How often a watched app is run, by default (seconds).
const EVERY: u64 = 300;
/// Screenshots kept per app.
const SHOTS_KEPT: usize = 20;
/// Words of an app saying something failed (`judge`: new since its baseline).
const FAILURE_WORDS: &[&str] = &["失败", "错误", "出错", "无法", "不可用", "超时", "离线", "error", "failed", "unavailable", "timeout", "offline"];

/// An app's health, worst last.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Health {
    #[default]
    Healthy,
    Degraded,
    Broken,
    Down,
}

impl Health {
    pub fn key(self) -> &'static str {
        match self {
            Health::Healthy => "healthy",
            Health::Degraded => "degraded",
            Health::Broken => "broken",
            Health::Down => "down",
        }
    }

    fn parse(s: &str) -> Option<Health> {
        [Health::Healthy, Health::Degraded, Health::Broken, Health::Down].into_iter().find(|h| h.key() == s)
    }

    pub fn word(self) -> &'static str {
        match self {
            Health::Healthy => i18n::t("healthy", "健康"),
            Health::Degraded => i18n::t("degraded", "异常"),
            Health::Broken => i18n::t("broken", "出错"),
            Health::Down => i18n::t("down", "起不来"),
        }
    }
}

/// One run of the published copy.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub at: u64,
    pub version: String,
    pub started: bool,
    pub errors: Vec<String>,
    /// (type, id, text) of each widget on screen.
    pub widgets: Vec<(String, String, String)>,
    pub shot: Option<PathBuf>,
    pub took_ms: u64,
    pub drill: Option<String>,
}

/// What a healthy run of a version showed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Baseline {
    pub version: String,
    /// The named widgets on screen.
    pub ids: Vec<String>,
    /// How many widgets showed text.
    pub texts: usize,
    /// Failure words it showed anyway (a help line, say): not news later.
    pub failure_texts: Vec<String>,
}

impl Baseline {
    fn of(p: &Probe) -> Baseline {
        let mut ids: Vec<String> = p.widgets.iter().map(|w| w.1.clone()).filter(|i| !i.is_empty()).collect();
        ids.sort();
        ids.dedup();
        Baseline { version: p.version.clone(), ids, texts: texts(p), failure_texts: failure_texts(p) }
    }

    fn to_json(&self) -> Value {
        json!({"version": self.version, "ids": self.ids, "texts": self.texts, "failure_texts": self.failure_texts})
    }

    fn from_json(v: &Value) -> Option<Baseline> {
        let list = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default();
        Some(Baseline { version: v["version"].as_str()?.to_string(), ids: list("ids"), texts: v["texts"].as_u64().unwrap_or(0) as usize, failure_texts: list("failure_texts") })
    }
}

fn texts(p: &Probe) -> usize {
    p.widgets.iter().filter(|w| !w.2.trim().is_empty()).count()
}

fn failure_texts(p: &Probe) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in p.widgets.iter().map(|w| w.2.trim().to_string()) {
        let lower = t.to_lowercase();
        if FAILURE_WORDS.iter().any(|w| lower.contains(w)) && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// A run's health, and why, against its version's baseline (if it has one).
pub fn judge(p: &Probe, baseline: Option<&Baseline>) -> (Health, Vec<String>) {
    if !p.started {
        let mut why = vec![i18n::t("it did not start", "应用没能启动").to_string()];
        why.extend(p.errors.iter().take(5).cloned());
        return (Health::Down, why);
    }
    if !p.errors.is_empty() {
        return (Health::Broken, p.errors.iter().take(5).cloned().collect());
    }
    let mut why = Vec::new();
    // A failure it says with its content kept (the last data, and that it is
    // offline) is it coping, not failing.
    let kept = baseline.is_some_and(|b| b.texts > 0 && texts(p) * 5 >= b.texts * 4);
    let new_failures: Vec<String> = failure_texts(p).into_iter()
        .filter(|t| !kept && baseline.is_none_or(|b| !b.failure_texts.contains(t))).collect();
    for t in new_failures.iter().take(3) {
        let t: String = t.chars().take(80).collect();
        why.push(i18n::pick(format!("it shows “{t}”"), format!("页面上显示「{t}」")));
    }
    if let Some(b) = baseline.filter(|b| b.version == p.version) {
        let now: HashSet<&str> = p.widgets.iter().map(|w| w.1.as_str()).collect();
        let gone: Vec<&str> = b.ids.iter().map(String::as_str).filter(|i| !now.contains(i)).collect();
        if b.ids.len() >= 3 && gone.len() * 10 >= b.ids.len() * 4 {
            let names: Vec<&str> = gone.iter().take(6).copied().collect();
            why.push(i18n::pick(format!("{} of its {} named widgets are gone: {}", gone.len(), b.ids.len(), names.join(", ")),
                format!("{} 个命名控件里有 {} 个不见了：{}", b.ids.len(), gone.len(), names.join("、"))));
        }
        let shown = texts(p);
        if b.texts >= 4 && shown * 2 < b.texts {
            why.push(i18n::pick(format!("it shows text in {shown} widgets, {} when it was well", b.texts),
                format!("有文字的控件从 {} 个降到 {shown} 个", b.texts)));
        }
    }
    if why.is_empty() { (Health::Healthy, why) } else { (Health::Degraded, why) }
}

fn loop_dir(project: &str) -> PathBuf {
    Path::new(project).join(".octobuddy/card-loop")
}

fn app_id(project: &str) -> Result<String, String> {
    let text = std::fs::read_to_string(super::octosense_app::bundle(project).join("manifest.json")).map_err(|e| format!("no bundle/manifest.json: {e}"))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    v["id"].as_str().map(String::from).ok_or_else(|| "its manifest names no id".into())
}

fn version_key(v: &str) -> Vec<u64> {
    v.split(['.', '-']).map(|p| p.parse().unwrap_or(0)).collect()
}

/// The newest version of `id` a catalog lists, and its artifact (relative to the hub).
fn newest_in(catalog: &Value, id: &str) -> Option<(String, String)> {
    catalog["entries"].as_array()?.iter()
        .filter(|e| e["manifest"]["id"].as_str() == Some(id))
        .filter_map(|e| Some((e["manifest"]["version"].as_str()?.to_string(), e["artifact"].as_str()?.to_string())))
        .max_by_key(|(v, _)| version_key(v))
}

/// The copy of `project`'s app the local App Hub serves: its version and folder.
pub fn published(project: &str) -> Result<(String, PathBuf), String> {
    let id = app_id(project)?;
    let hub = super::app_publish::hub_dir();
    let text = std::fs::read_to_string(hub.join("catalog.json")).map_err(|_| i18n::t("it is not published yet", "还没有发布").to_string())?;
    let catalog: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let (version, artifact) = newest_in(&catalog, &id).ok_or_else(|| i18n::t("it is not published yet", "还没有发布").to_string())?;
    let dir = hub.join(artifact);
    if !dir.join("manifest.json").is_file() {
        return Err(format!("its published copy is missing ({})", dir.display()));
    }
    Ok((version, dir))
}

/// The bad-release drill's change: of the functions `main.splash` defines,
/// the one called most has its definition renamed and its calls left, as a
/// refactor that missed its call sites. Its name, and the source so broken.
pub fn break_a_function(source: &str) -> Option<(String, String)> {
    let mut best: Option<(usize, String)> = None;
    for line in source.lines() {
        let Some(rest) = line.trim_start().strip_prefix("fn ") else { continue };
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if name.is_empty() {
            continue;
        }
        // Its calls: the name and `(`, not the tail of a longer name, not its definition.
        let call = format!("{name}(");
        let calls = source.match_indices(&call)
            .filter(|(at, _)| !source[..*at].ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_'))
            .filter(|(at, _)| !source[..*at].ends_with("fn "))
            .count();
        if calls > 0 && best.as_ref().is_none_or(|(n, _)| calls > *n) {
            best = Some((calls, name));
        }
    }
    let (_, name) = best?;
    let broken = source.replacen(&format!("fn {name}("), &format!("fn {name}_v2("), 1);
    Some((name, broken))
}

/// The bad-release drill (blocks: off the UI thread): the project's app
/// broken by [`break_a_function`], committed as the drill, and published to
/// the local App Hub as its next version. Its version and the function.
pub fn bad_release(project: &str) -> Result<(String, String), String> {
    let path = super::octosense_app::bundle(project).join("main.splash");
    let source = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (name, broken) = break_a_function(&source).ok_or("main.splash defines no function it calls: nothing to break")?;
    std::fs::write(&path, broken).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| Command::new("git").args(args).current_dir(project).output().map_err(|e| e.to_string())
        .and_then(|o| if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).trim().to_string()) });
    git(&["add", "bundle/main.splash"])?;
    git(&["commit", "-q", "-m", &format!("drill: 坏版本演练（{name} 的定义改了名，调用处没改）")])?;
    let published = super::app_publish::publish(project)?;
    Ok((published.version, name))
}

/// Runs the published copy once (blocks: seconds, off the UI thread).
pub fn probe(project: &str, drill: Option<&str>) -> Result<Probe, String> {
    let (version, artifact) = published(project)?;
    let tools = super::octosense_app::tools()?;
    let shots = loop_dir(project).join("shots");
    std::fs::create_dir_all(&shots).map_err(|e| e.to_string())?;
    let at = now_secs();
    let shot = shots.join(format!("{at}.png"));
    let started = std::time::Instant::now();
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(PROBE_SCRIPT).arg("octobuddy-card-loop").arg(&artifact).arg(tools.octo()).arg(&shot).arg(drill.unwrap_or(""))
        .arg(loop_dir(project).join("state"))
        .env("PATH", crate::workspace::search_path());
    if let Some(hub) = &tools.app_hub {
        cmd.env("OCTOSENSE_APP_HUB", hub);
    }
    let out = cmd.output().map_err(|e| format!("could not run the app: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let v: Value = text.lines().rev().find_map(|l| serde_json::from_str(l).ok())
        .ok_or_else(|| format!("the run said nothing readable: {}", String::from_utf8_lossy(&out.stderr).trim()))?;
    let strings = |k: &str| v[k].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default();
    let widgets = v["widgets"].as_array().map(|a| a.iter().filter_map(|w| {
        let w = w.as_array()?;
        Some((w.first()?.as_str()?.to_string(), w.get(1)?.as_str()?.to_string(), w.get(2)?.as_str()?.to_string()))
    }).collect()).unwrap_or_default();
    prune(&shots);
    Ok(Probe {
        at, version, started: v["started"].as_bool().unwrap_or(false), errors: strings("errors"), widgets,
        shot: shot.is_file().then_some(shot), took_ms: started.elapsed().as_millis() as u64, drill: drill.map(String::from),
    })
}

fn prune(shots: &Path) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(shots).into_iter().flatten().flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
    files.sort();
    for old in files.iter().take(files.len().saturating_sub(SHOTS_KEPT)) {
        let _ = std::fs::remove_file(old);
    }
}

const PROBE_SCRIPT: &str = r#"
bundle="$1"; octo="$2"; shot="$3"; drill="$4"; state="$5"
work=$(mktemp -d "${TMPDIR:-/tmp}/octobuddy-card-loop.XXXXXX") || exit 2
trap 'rm -rf "$work"' EXIT
cp -R "$bundle" "$work/bundle" || exit 2
# The hub's copy is signed for the shell; card-host runs it as the unsigned
# copy it is (its digest stamped anew). Drill `offline`: its API down, every
# host it may reach swapped for one that never answers.
python3 - "$work/bundle" "$drill" <<'PY' || exit 2
import json, os, sys
b, drill = sys.argv[1], sys.argv[2]
m = json.load(open(os.path.join(b, "manifest.json")))
(m.get("integrity") or {}).pop("signature", None)
if drill in ("offline", "offline-fresh"):
    hosts = (m.get("network") or {}).get("hosts") or []
    m.setdefault("network", {})["hosts"] = ["octobuddy-drill.invalid"]
    p = os.path.join(b, "main.splash")
    s = open(p).read()
    for h in hosts:
        s = s.replace("://" + h, "://octobuddy-drill.invalid")
    open(p, "w").write(s)
json.dump(m, open(os.path.join(b, "manifest.json"), "w"), ensure_ascii=False, indent=2)
PY
port=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
# Its storage stays between runs, as on a device (the last data it kept);
# its log is this run's only. Drill `offline-fresh`: a new device, nothing kept.
[ "$drill" = offline-fresh ] && state="$work/fresh-state"
mkdir -p "$state" && rm -f "$state/card-host.log"
started=1
python3 "$octo" run "$work/bundle" --port "$port" --hidden --detach --timeout 30 --app-data "$state" > "$work/run.txt" 2>&1 || started=0
if [ $started = 1 ]; then
  # Its live data comes in.
  sleep 4
  python3 "$octo" shot "$port" "$shot" > /dev/null 2>&1
  curl -s -m 5 "http://127.0.0.1:$port/snap" > "$work/snap.json"
  curl -s -m 5 "http://127.0.0.1:$port/quit" > /dev/null 2>&1
  sleep 1
fi
python3 - "$work" "$started" "$state" <<'PY'
import json, os, re, sys
work, started, state = sys.argv[1], sys.argv[2] == "1", sys.argv[3]
def lines(p):
    try:
        return open(p, errors="replace").read().splitlines()
    except Exception:
        return []
script = re.compile(r'\[E\]|splash:[0-9]+:|on_render closure failed|callback error')
if started:
    errors = [l.strip() for l in lines(os.path.join(state, "card-host.log")) if script.search(l)][:20]
else:
    run = lines(os.path.join(work, "run.txt"))
    errors = [l.strip() for l in run if script.search(l) or re.search(r'refused|did not', l)][:20] or [l.strip() for l in run[-5:]]
skip = {"View", "SolidView", "RoundedView", "Window", "KeyboardView", "ScrollYView", "ScrollXView"}
widgets = []
try:
    for w in json.load(open(os.path.join(work, "snap.json"))).get("s", []):
        ty, t = w.get("ty") or "", (w.get("t") or "").replace("\n", " ")
        if ty == "Splash" or (ty in skip and not t):
            continue
        widgets.append([ty, w.get("i") or "", t[:120]])
except Exception:
    pass
print(json.dumps({"started": started, "errors": errors, "widgets": widgets[:400]}, ensure_ascii=False))
PY
"#;

/// A problem with an app someone else saw: the system agent's
/// `octobuddy.report` (the person told it, or another app's agent saw it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub at: u64,
    /// Who saw it, as the system agent says.
    pub from: String,
    pub problem: String,
    /// What the run of the published version that followed found: `None`
    /// until it ran.
    pub reproduced: Option<bool>,
}

impl Report {
    fn to_json(&self) -> Value {
        json!({"at": self.at, "from": self.from, "problem": self.problem, "reproduced": self.reproduced})
    }

    fn from_json(v: &Value) -> Option<Report> {
        Some(Report {
            at: v["at"].as_u64()?, from: v["from"].as_str().unwrap_or("").to_string(),
            problem: v["problem"].as_str()?.to_string(), reproduced: v["reproduced"].as_bool(),
        })
    }

    fn who(&self) -> String {
        if self.from.is_empty() { i18n::t("the OctoSense assistant", "系统 agent").to_string() } else { self.from.clone() }
    }
}

fn reports_of(v: &Value) -> Vec<Report> {
    v.as_array().map(|a| a.iter().filter_map(Report::from_json).collect()).unwrap_or_default()
}

/// A spell of ill health: from the run that saw it to the one that found the
/// app well again, with what was done about it on the way.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Incident {
    pub since: u64,
    pub version: String,
    pub health: Health,
    pub why: Vec<String>,
    pub shot: Option<String>,
    /// Widgets with text when it began (its baseline had more).
    pub texts: usize,
    /// The drill it happened under, if one.
    pub drill: Option<String>,
    /// Handed to an outer loop to repair: its session, and when.
    pub repair: Option<(String, u64)>,
    /// The fixed version published, and when.
    pub published: Option<(String, u64)>,
    /// What others reported of it (the run confirmed them).
    pub reports: Vec<Report>,
}

impl Incident {
    fn to_json(&self) -> Value {
        json!({
            "since": self.since, "version": self.version, "health": self.health.key(), "why": self.why, "shot": self.shot,
            "texts": self.texts, "drill": self.drill,
            "repair": self.repair.as_ref().map(|(s, at)| json!({"session": s, "at": at})),
            "published": self.published.as_ref().map(|(v, at)| json!({"version": v, "at": at})),
            "reports": self.reports.iter().map(Report::to_json).collect::<Vec<_>>(),
        })
    }

    fn from_json(v: &Value) -> Option<Incident> {
        Some(Incident {
            since: v["since"].as_u64()?,
            version: v["version"].as_str().unwrap_or("").to_string(),
            health: v["health"].as_str().and_then(Health::parse).unwrap_or(Health::Degraded),
            why: v["why"].as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default(),
            shot: v["shot"].as_str().map(String::from),
            texts: v["texts"].as_u64().unwrap_or(0) as usize,
            drill: v["drill"].as_str().map(String::from),
            repair: v["repair"]["session"].as_str().map(|s| (s.to_string(), v["repair"]["at"].as_u64().unwrap_or(0))),
            published: v["published"]["version"].as_str().map(|s| (s.to_string(), v["published"]["at"].as_u64().unwrap_or(0))),
            reports: reports_of(&v["reports"]),
        })
    }
}

/// A watched app: `<project>/.octobuddy/card-loop/watch.json`.
#[derive(Clone, Debug, Default)]
pub struct Watch {
    pub watching: bool,
    pub every: u64,
    /// The session told about it (and its repairs go to).
    pub session: String,
    pub baseline: Option<Baseline>,
    pub health: Option<Health>,
    pub last: u64,
    /// Widgets with text at its last run.
    pub texts: usize,
    pub drill: Option<String>,
    /// An incident goes to the outer loop without asking.
    pub auto_repair: bool,
    /// A version just published runs once without the drill first, as a
    /// device online when it updated (its storage filled by it).
    pub warm: bool,
    pub incident: Option<Incident>,
    /// The last reports of it (five at most), newest last.
    pub reports: Vec<Report>,
    /// The version the bad-release drill published, and the function it broke.
    pub bad_release: Option<(String, String)>,
}

impl Watch {
    fn read(project: &str) -> Option<Watch> {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(loop_dir(project).join("watch.json")).ok()?).ok()?;
        Some(Watch {
            watching: v["watching"].as_bool().unwrap_or(false),
            every: v["every"].as_u64().filter(|e| *e >= 30).unwrap_or(EVERY),
            session: v["session"].as_str().unwrap_or("").to_string(),
            baseline: Baseline::from_json(&v["baseline"]),
            health: v["health"].as_str().and_then(Health::parse),
            last: v["last"].as_u64().unwrap_or(0),
            texts: v["texts"].as_u64().unwrap_or(0) as usize,
            drill: v["drill"].as_str().filter(|d| !d.is_empty()).map(String::from),
            auto_repair: v["auto_repair"].as_bool().unwrap_or(false),
            warm: v["warm"].as_bool().unwrap_or(false),
            incident: Incident::from_json(&v["incident"]),
            reports: reports_of(&v["reports"]),
            bad_release: v["bad_release"]["version"].as_str().map(|ver| (ver.to_string(), v["bad_release"]["function"].as_str().unwrap_or("").to_string())),
        })
    }

    fn write(&self, project: &str) {
        let dir = loop_dir(project);
        let _ = std::fs::create_dir_all(&dir);
        let v = json!({
            "watching": self.watching, "every": self.every, "session": self.session,
            "baseline": self.baseline.as_ref().map(Baseline::to_json), "health": self.health.map(Health::key),
            "last": self.last, "texts": self.texts, "drill": self.drill, "auto_repair": self.auto_repair, "warm": self.warm,
            "incident": self.incident.as_ref().map(Incident::to_json),
            "reports": self.reports.iter().map(Report::to_json).collect::<Vec<_>>(),
            "bad_release": self.bad_release.as_ref().map(|(v, f)| json!({"version": v, "function": f})),
        });
        let _ = std::fs::write(dir.join("watch.json"), serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    fn app_name(project: &str) -> String {
        std::fs::read_to_string(super::octosense_app::bundle(project).join("manifest.json")).ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["name"].as_str().map(String::from))
            .unwrap_or_else(|| Path::new(project).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    }
}

fn log(project: &str, p: &Probe, health: Health, why: &[String]) {
    use std::io::Write;
    let line = json!({
        "at": p.at, "version": p.version, "health": health.key(), "why": why, "started": p.started,
        "errors": p.errors.len(), "widgets": p.widgets.len(), "texts": texts(p), "took_ms": p.took_ms,
        "shot": shot_of(project, p), "drill": p.drill,
    });
    let dir = loop_dir(project);
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("health.jsonl")) {
        let _ = writeln!(f, "{line}");
    }
}

fn shot_of(project: &str, p: &Probe) -> Option<String> {
    p.shot.as_ref().and_then(|s| s.strip_prefix(project).ok()).map(|s| s.display().to_string())
}

/// The health of its last `n` runs, oldest first.
fn history(project: &str, n: usize) -> Vec<Health> {
    let text = std::fs::read_to_string(loop_dir(project).join("health.jsonl")).unwrap_or_default();
    let all: Vec<Health> = text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| v["health"].as_str().and_then(Health::parse)).collect();
    all[all.len().saturating_sub(n)..].to_vec()
}

/// Whether the project's app differs from its published copy (a fix to publish).
fn changed_since_published(project: &str, published: &Path) -> bool {
    let read = |p: PathBuf| std::fs::read(p).unwrap_or_default();
    let bundle = super::octosense_app::bundle(project);
    ["main.splash", "listing.json"].iter().any(|f| read(bundle.join(f)) != read(published.join(f)))
}

/// What the outer loop is asked when an incident goes to it.
fn repair_brief(name: &str, i: &Incident) -> String {
    let mut why: Vec<String> = i.why.iter().map(|w| format!("- {w}")).collect();
    for r in &i.reports {
        why.push(i18n::pick(format!("- reported by {}: “{}”", r.who(), r.problem), format!("- {}报告：「{}」", r.who(), r.problem)));
    }
    let shot = i.shot.as_deref().unwrap_or("");
    let drill = match i.drill.as_deref() {
        Some("bad-release") => i18n::t("\n(This was a drill: OctoBuddy published a version with a bug on purpose, as a change that shipped without its check. Find the cause from what the run showed and fix it; do not just revert the whole version.)",
            "\n（这是一次演练：OctoBuddy 故意发布了一个带 bug 的版本，模拟一次没走检查就上线的改动。请根据巡检看到的错误找到原因并修好，不要直接回退整个版本。）"),
        Some("offline-fresh") => i18n::t("\n(This was a drill: the watch ran it as on a new device, nothing kept, with its API cut off on purpose. There is no data to show then: check that it says so plainly and offers to try again, and change only what falls short of that.)",
            "\n（这是一次演练：巡检按新设备运行，没有任何缓存，并故意断开了它的 API。这时本来就没有数据可显示：请确认它清楚地说明情况并能重试，只修改做得不够的地方。）"),
        Some(_) => i18n::t("\n(This was a drill: the watch cut its API off on purpose, on a device that used it before. The fix to make: when its API does not answer, the app still gives something to use, such as the last data it fetched and when, not only an error.)",
            "\n（这是一次演练：巡检时故意断开了它的 API，设备之前用过它。要做的修复：API 不响应时，应用仍然给出能用的界面，比如显示上次取到的数据和时间，而不是只显示报错。）"),
        None => "",
    };
    i18n::pick(
        format!("OctoBuddy's live watch found the published {name} {} {}:\n{}\nScreenshot: {shot} (octobuddy_app_look shows the app as it is now).{drill}\nRepair the app in this project so it works under these conditions, check it, and tell me what you changed. Do not publish it: I publish the fixed version from the Live page.",
            i.version, i.health.word(), why.join("\n")),
        format!("巡检发现已发布的 {name} {} {}：\n{}\n截图：{shot}（octobuddy_app_look 可以看应用现在的样子）。{drill}\n请修好这个项目里的应用，让它在这种情况下也能正常工作，检查通过后告诉我改了什么。先不要发布：我会在巡检页发布修复版。",
            i.version, i.health.word(), why.join("\n")))
}

/// The watches of this window's projects.
#[derive(Default)]
pub struct Loops {
    pub watches: BTreeMap<String, Watch>,
    pub running: HashSet<String>,
    /// Incidents to hand to their outer loop (auto repair), on the next event with a `Cx`.
    pub pending_repairs: Vec<String>,
    /// Bad-release drills under way (their project).
    pub drilling: HashSet<String>,
    /// `octobuddy.report` calls answered once their run is back (project, asked at).
    pub waiting: Vec<(String, u64, ToolReply)>,
    loaded: bool,
}

impl Loops {
    /// Whether a watched app is not well (the sidebar's mark).
    pub fn alarm(&self) -> bool {
        self.watches.values().any(|w| w.watching && w.health.is_some_and(|h| h != Health::Healthy))
    }
}

const CARDS: [LiveId; 8] = [live_id!(lc0), live_id!(lc1), live_id!(lc2), live_id!(lc3), live_id!(lc4), live_id!(lc5), live_id!(lc6), live_id!(lc7)];
const DOTS: [LiveId; 12] = [live_id!(h0), live_id!(h1), live_id!(h2), live_id!(h3), live_id!(h4), live_id!(h5),
    live_id!(h6), live_id!(h7), live_id!(h8), live_id!(h9), live_id!(h10), live_id!(h11)];

fn ago(secs: u64) -> String {
    match secs {
        0..=59 => i18n::t("just now", "刚刚").to_string(),
        60..=3599 => i18n::pick(format!("{} min ago", secs / 60), format!("{} 分钟前", secs / 60)),
        _ => i18n::pick(format!("{} h ago", secs / 3600), format!("{} 小时前", secs / 3600)),
    }
}

/// A spell's length, for a person.
fn span(secs: u64) -> String {
    match secs {
        0..=59 => i18n::pick(format!("{secs} s"), format!("{secs} 秒")),
        60..=3599 => i18n::pick(format!("{} min", secs / 60), format!("{} 分钟", secs / 60)),
        _ => i18n::pick(format!("{} h {} min", secs / 3600, secs % 3600 / 60), format!("{} 小时 {} 分钟", secs / 3600, secs % 3600 / 60)),
    }
}

/// How often it runs, for a person.
fn pace(every: u64) -> String {
    if every < 60 {
        i18n::pick(format!("every {every} s"), format!("每 {every} 秒"))
    } else {
        i18n::pick(format!("every {} min", every / 60), format!("每 {} 分钟", every / 60))
    }
}

fn health_color(h: Option<Health>) -> u32 {
    crate::theme::hex(match h {
        Some(Health::Healthy) => "success",
        Some(Health::Degraded) => "warning",
        Some(Health::Broken | Health::Down) => "danger",
        None => "line_strong",
    })
}

impl OctoBuddyView {
    fn card_loop_load(&mut self) {
        if self.card_loop.loaded {
            return;
        }
        self.card_loop.loaded = true;
        let paths: Vec<String> = self.store.projects.iter().map(|p| p.path.clone()).collect();
        for path in paths {
            if let Some(w) = Watch::read(&path).filter(|w| w.watching) {
                self.card_loop.watches.insert(path, w);
            }
        }
    }

    /// The app projects the page lists: the plugin at work for them.
    fn live_apps(&self) -> Vec<(usize, String)> {
        self.store.projects.iter().enumerate().filter(|(_, p)| !p.is_chats() && super::active(super::CARD_LOOP, &p.path))
            .map(|(i, p)| (i, p.path.clone())).take(CARDS.len()).collect()
    }

    /// The session an app's watch speaks in: its own, else the project's latest.
    fn live_session(&self, pi: usize, project: &str) -> Option<crate::model::SessionRef> {
        let own = self.card_loop.watches.get(project).map(|w| w.session.clone()).or_else(|| Watch::read(project).map(|w| w.session));
        own.and_then(|id| self.store.find_session(&id))
            .or_else(|| self.store.projects.get(pi).filter(|p| !p.sessions.is_empty()).map(|p| (pi, p.sessions.len() - 1)))
    }

    /// The sidebar's Live button: marked when a watched app is not well.
    pub(crate) fn sync_live_button(&mut self, cx: &mut Cx) {
        self.card_loop_load();
        let alarm = self.card_loop.alarm();
        // A request waiting for the person marks it too (in the accent), less than an app not well.
        let asked = !alarm && self.factory_waiting();
        self.view.view(cx, ids!(live_button)).set_visible(cx, !alarm && !asked);
        self.view.view(cx, ids!(live_button_alert)).set_visible(cx, alarm);
        self.view.view(cx, ids!(live_button_request)).set_visible(cx, asked);
    }

    /// The Live page: a card per app, its health, its runs, what to do.
    pub(crate) fn sync_live_page(&mut self, cx: &mut Cx) {
        self.card_loop_load();
        self.view.label(cx, ids!(live_title)).set_text(cx, i18n::t("Apps", "应用"));
        self.view.label(cx, ids!(live_hint)).set_text(cx, i18n::t(
            "Apps asked for from outside OctoBuddy wait here for you, and the apps OctoBuddy published to the local App Hub are watched here, as people run them. A watched app is run headless every few minutes on live data and judged against its first healthy run; when it breaks, its session hears it, and Repair hands it to that session's outer loop. The fixed version is published from here.",
            "系统 agent 或其他应用请求做的应用在这里等你决定；OctoBuddy 发布到本地 App Hub 的应用（也就是大家实际在用的版本）在这里巡检。开启巡检后，每隔几分钟用实时数据 headless 运行一次，并和它第一次正常运行时的样子对比；出问题时会在它的会话里说，点「修复」就交给那个会话的外环去修，修好后在这里发布修复版。"));
        self.sync_requests(cx);
        let apps = self.live_apps();
        self.view.label(cx, ids!(live_empty)).set_visible(cx, apps.is_empty());
        self.view.label(cx, ids!(live_empty)).set_text(cx, i18n::t("No OctoSense app project yet (or the Production loop plugin is off in Settings › Plugins).",
            "还没有 OctoSense 应用项目（或者 设置 › 插件 里关掉了「生产回路」）。"));
        let now = now_secs();
        for (i, slot) in CARDS.iter().enumerate() {
            let card = self.view.view(cx, &[*slot]);
            let Some((pi, project)) = apps.get(i).cloned() else {
                card.set_visible(cx, false);
                continue;
            };
            card.set_visible(cx, true);
            let name = Watch::app_name(&project);
            let published = published(&project);
            let watch = self.card_loop.watches.get(&project).filter(|w| w.watching).cloned();
            let running = self.card_loop.running.contains(&project);
            card.label(cx, ids!(name)).set_text(cx, &name);
            card.label(cx, ids!(version)).set_text(cx, &match &published {
                Ok((v, _)) => i18n::pick(format!("published {v}"), format!("已发布 {v}")),
                Err(_) => i18n::t("not published", "未发布").to_string(),
            });
            let health = watch.as_ref().and_then(|w| w.health);
            let mut dot = card.view(cx, ids!(dot));
            let color = crate::hex_color(health_color(health));
            script_apply_eval!(cx, dot, { draw_bg +: {color: #(color)} });
            let state = match (&published, &watch, running) {
                (Err(_), _, _) => i18n::t("Publish it from its session first: the watch runs the published version.", "先在它的会话里发布：巡检的是发布出去的版本。").to_string(),
                (_, None, _) => i18n::t("Not watched", "未开启巡检").to_string(),
                (_, Some(_), true) => i18n::t("Checking…", "检查中…").to_string(),
                (_, Some(w), false) => match w.health {
                    None => i18n::t("Watching: first run soon", "巡检中：马上第一次检查").to_string(),
                    Some(h) => format!("{} · {} · {}", h.word(), ago(now.saturating_sub(w.last)), pace(w.every)),
                },
            };
            card.label(cx, ids!(state)).set_text(cx, &state);
            // Its last runs, a dot each.
            let runs = if watch.is_some() || loop_dir(&project).join("health.jsonl").is_file() { history(&project, DOTS.len()) } else { Vec::new() };
            for (j, id) in DOTS.iter().enumerate() {
                let mut d = card.view(cx, &[*id]);
                d.set_visible(cx, j < runs.len());
                if let Some(h) = runs.get(j) {
                    let color = crate::hex_color(health_color(Some(*h)));
                    script_apply_eval!(cx, d, { draw_bg +: {color: #(color)} });
                }
            }
            card.view(cx, ids!(runs)).set_visible(cx, !runs.is_empty());
            let incident = watch.as_ref().and_then(|w| w.incident.clone());
            // What is wrong, and what was done about it.
            let mut lines: Vec<String> = Vec::new();
            if let Some(inc) = &incident {
                lines.push(i18n::pick(format!("Found {} ({}):", ago(now.saturating_sub(inc.since)), inc.version), format!("{}发现问题（{}）：", ago(now.saturating_sub(inc.since)), inc.version)));
                lines.extend(inc.why.iter().map(|w| format!("· {w}")));
                if let (Some(base), Some(w)) = (watch.as_ref().and_then(|w| w.baseline.as_ref()), &watch) {
                    lines.push(i18n::pick(format!("Widgets with text: {} when well, {} when it broke, {} now", base.texts, inc.texts, w.texts),
                        format!("有文字的控件：正常时 {}，出问题时 {}，现在 {}", base.texts, inc.texts, w.texts)));
                }
                match (&inc.repair, &inc.published) {
                    (_, Some((v, at))) => lines.push(i18n::pick(format!("Fixed version {v} published {}; the next run checks it.", ago(now.saturating_sub(*at))),
                        format!("修复版 {v} 已在{}发布，下一次巡检会检查它。", ago(now.saturating_sub(*at))))),
                    (Some((sid, at)), None) => {
                        let busy = self.store.find_session(sid).is_some_and(|s| self.session_busy(s));
                        lines.push(if busy {
                            i18n::pick(format!("Repairing: its outer loop took it {}.", ago(now.saturating_sub(*at))), format!("修复中：已在{}交给外环。", ago(now.saturating_sub(*at))))
                        } else {
                            i18n::t("The outer loop is done with it: publish the fixed version once its check passed.", "外环已处理完：检查通过后可以发布修复版。").to_string()
                        });
                    }
                    (None, None) => {}
                }
            }
            match watch.as_ref().and_then(|w| w.drill.as_deref()) {
                Some("offline-fresh") => lines.push(i18n::t("Drill on: its API is cut off in the watch's runs, as on a new device with nothing kept (the published app is not touched).",
                    "演练中：巡检按新设备运行（没有缓存）并断开它的 API（不影响已发布的应用）。").to_string()),
                Some(_) => lines.push(i18n::t("Drill on: its API is cut off in the watch's runs (the published app is not touched).", "演练中：巡检运行时断开了它的 API（不影响已发布的应用）。").to_string()),
                None => {}
            }
            if let Some(r) = watch.as_ref().and_then(|w| w.reports.last()) {
                let found = match r.reproduced {
                    None => i18n::t("checking…", "复查中……"),
                    Some(true) => i18n::t("the run found it", "巡检确认了"),
                    Some(false) => i18n::t("the run did not show it", "巡检没有复现"),
                };
                lines.push(i18n::pick(format!("Reported {} by {}: “{}” — {found}", ago(now.saturating_sub(r.at)), r.who(), r.problem),
                    format!("{}{}报告：「{}」——{found}", ago(now.saturating_sub(r.at)), r.who(), r.problem)));
            }
            if let Some(shot) = incident.as_ref().and_then(|i| i.shot.clone()) {
                lines.push(i18n::pick(format!("Screenshot: {shot}"), format!("截图：{shot}")));
            }
            card.label(cx, ids!(why)).set_visible(cx, !lines.is_empty());
            card.label(cx, ids!(why)).set_text(cx, &lines.join("\n"));
            // Its buttons.
            let can = published.is_ok();
            let watching = watch.is_some();
            let drill = watch.as_ref().is_some_and(|w| w.drill.is_some());
            let auto = watch.as_ref().is_some_and(|w| w.auto_repair);
            let repairing = incident.as_ref().and_then(|i| i.repair.as_ref()).and_then(|(sid, _)| self.store.find_session(sid)).is_some_and(|s| self.session_busy(s));
            // A report the run did not confirm, in the last day: still the person's to hand over.
            let unconfirmed = incident.is_none() && watch.as_ref().and_then(|w| w.reports.last())
                .is_some_and(|r| r.reproduced == Some(false) && now.saturating_sub(r.at) < 86_400);
            let fix_ready = incident.as_ref().is_some_and(|i| i.published.is_none())
                && published.as_ref().is_ok_and(|(_, dir)| changed_since_published(&project, dir));
            let show = [
                (ids!(watch_on), watching, i18n::t("Watching", "巡检中")), (ids!(watch_off), can && !watching, i18n::t("Watch", "开启巡检")),
                (ids!(run_now), watching && !running, i18n::t("Run now", "立即巡检")),
                (ids!(drill_on), watching && drill, i18n::t("Drill: API off", "演练：断网中")), (ids!(drill_off), watching && !drill, i18n::t("Drill: cut API", "演练断网")),
                (ids!(auto_on), watching && auto, i18n::t("Auto repair", "自动修复")), (ids!(auto_off), watching && !auto, i18n::t("Auto repair", "自动修复")),
                (ids!(drill_bad), watching && incident.is_none() && !self.card_loop.drilling.contains(&project) && self.publishing.is_none(), i18n::t("Drill: bad release", "演练：发布坏版本")),
                (ids!(open), self.live_session(pi, &project).is_some(), i18n::t("Session", "打开会话")),
                (ids!(repair), (incident.is_some() || unconfirmed) && !repairing, i18n::t("Repair", "修复")),
                (ids!(publish_fix), fix_ready && self.publishing.is_none(), i18n::t("Publish fix", "发布修复版")),
            ];
            for (id, visible, text) in show {
                card.button(cx, id).set_visible(cx, visible);
                card.button(cx, id).set_text(cx, text);
            }
        }
    }

    /// The Live page's buttons, and the sidebar's.
    pub(crate) fn live_page_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let opened = [ids!(live_button), ids!(live_button_alert), ids!(live_button_request)].into_iter().any(|id| tapped(&self.view.view(cx, id), actions));
        if opened {
            self.page = crate::Page::Live;
            self.relayout(cx);
            return;
        }
        if tapped(&self.view.view(cx, ids!(live_back)), actions) {
            self.page = crate::Page::Chat;
            self.relayout(cx);
            return;
        }
        if self.page != crate::Page::Live || self.requests_actions(cx, actions) {
            return;
        }
        for (slot, (pi, project)) in CARDS.iter().zip(self.live_apps()) {
            let card = self.view.view(cx, &[*slot]);
            let hit = |id: &[LiveId]| card.button(cx, id).clicked(actions);
            if hit(ids!(watch_off)) {
                self.card_loop_watch(pi, &project, true);
            } else if hit(ids!(watch_on)) {
                self.card_loop_watch(pi, &project, false);
            } else if hit(ids!(run_now)) {
                self.card_loop_update(&project, |w| w.last = 0);
            } else if hit(ids!(drill_off)) {
                self.card_loop_update(&project, |w| { w.drill = Some("offline".into()); w.last = 0; });
            } else if hit(ids!(drill_on)) {
                self.card_loop_update(&project, |w| { w.drill = None; w.last = 0; });
            } else if hit(ids!(drill_bad)) {
                self.card_loop_bad_release(pi, &project);
            } else if hit(ids!(auto_off)) {
                self.card_loop_update(&project, |w| w.auto_repair = true);
            } else if hit(ids!(auto_on)) {
                self.card_loop_update(&project, |w| w.auto_repair = false);
            } else if hit(ids!(open)) {
                if let Some(at) = self.live_session(pi, &project) {
                    self.selected = Some(at);
                    self.page = crate::Page::Chat;
                }
            } else if hit(ids!(repair)) {
                self.card_loop_repair(cx, pi, &project, true);
            } else if hit(ids!(publish_fix)) {
                self.card_loop_publish(cx, pi, &project);
            } else {
                continue;
            }
            self.relayout(cx);
            return;
        }
    }

    fn card_loop_update(&mut self, project: &str, change: impl FnOnce(&mut Watch)) {
        if let Some(w) = self.card_loop.watches.get_mut(project) {
            change(w);
            w.write(project);
        }
    }

    /// A watch turned on (it runs at once) or off.
    pub(crate) fn card_loop_watch(&mut self, pi: usize, project: &str, on: bool) {
        let mut w = Watch::read(project).unwrap_or_default();
        let at = self.live_session(pi, project);
        let name = Watch::app_name(project);
        if !on {
            w.watching = false;
            w.write(project);
            self.card_loop.watches.remove(project);
            if let Some(at) = at {
                self.system(at, &i18n::pick(format!("Live watch off for {name}."), format!("已关闭 {name} 的巡检。")));
            }
            return;
        }
        let Ok((version, _)) = published(project) else { return };
        w.watching = true;
        // Afresh: its baseline is taken again, no incident carried over.
        w.health = None;
        w.incident = None;
        w.baseline = None;
        w.warm = false;
        w.every = if w.every == 0 { EVERY } else { w.every };
        w.session = at.and_then(|at| self.store.session(at)).map(|s| s.id.clone()).unwrap_or_default();
        w.last = 0;
        w.write(project);
        let every = pace(w.every);
        self.card_loop.watches.insert(project.to_string(), w);
        if let Some(at) = at {
            self.system(at, &i18n::pick(format!("Live watch on: OctoBuddy runs the published {name} {version} {every} on live data and says here when it breaks. Its record: .octobuddy/card-loop/health.jsonl"),
                format!("已开启巡检：{every}用实时数据运行一次已发布的 {name} {version}，出问题会在这里说。记录在 .octobuddy/card-loop/health.jsonl")));
        }
    }

    /// An incident handed to its session's outer loop (`show`: and that
    /// session opened).
    fn card_loop_repair(&mut self, cx: &mut Cx, pi: usize, project: &str, show: bool) {
        // An incident, or a report the run did not confirm (as one, for its brief).
        let Some(w) = self.card_loop.watches.get(project).cloned() else { return };
        let incident = w.incident.clone().or_else(|| w.reports.last().filter(|r| r.reproduced == Some(false)).map(|r| Incident {
            since: r.at, version: published(project).map(|(v, _)| v).unwrap_or_default(), health: Health::Degraded,
            why: vec![i18n::t("the watch's run did not show it: it may happen only on a real device", "巡检的运行没有复现：可能只在真实设备上出现").to_string()],
            reports: vec![r.clone()], ..Default::default()
        }));
        let Some(incident) = incident else { return };
        let at = match self.live_session(pi, project) {
            Some(at) => Some(at),
            None => self.store.add_session(pi),
        };
        let Some(at) = at else { return };
        let Some(sid) = self.store.session(at).map(|s| s.id.clone()) else { return };
        if show {
            self.selected = Some(at);
            self.page = crate::Page::Chat;
        }
        let brief = repair_brief(&Watch::app_name(project), &incident);
        if self.send_text(cx, at, brief) {
            let now = now_secs();
            self.card_loop_update(project, |w| {
                w.session = sid.clone();
                if let Some(i) = w.incident.as_mut() {
                    i.repair = Some((sid, now));
                }
            });
        }
    }

    /// The fixed version published to the local App Hub (as the session's
    /// Publish does); `card_loop_published` hears how it went.
    pub(crate) fn card_loop_publish(&mut self, cx: &mut Cx, pi: usize, project: &str) {
        if self.publishing.is_some() {
            return;
        }
        let Some(at) = self.live_session(pi, project) else { return };
        let Some(session) = self.store.session(at).map(|s| s.id.clone()) else { return };
        self.publishing = Some(session.clone());
        self.system(at, i18n::t("Publishing the fixed version to the local App Hub…", "正在把修复版发布到本地 App Hub……"));
        let (inbox, project) = (self.rt.inbox.clone(), project.to_string());
        std::thread::spawn(move || {
            let result = super::app_publish::publish(&project);
            crate::events::post(&inbox, crate::events::LoopEvent::Published { session, result });
        });
        self.relayout(cx);
    }

    /// The bad-release drill started: the project's app broken and published
    /// off the UI thread (`CardLoopDrilled`).
    fn card_loop_bad_release(&mut self, pi: usize, project: &str) {
        self.card_loop.drilling.insert(project.to_string());
        if let Some(at) = self.live_session(pi, project) {
            self.system(at, i18n::t("Drill: OctoBuddy breaks a function of the app (its definition renamed, its calls left) and publishes that as the next version, to see the live watch find it…",
                "演练：OctoBuddy 把应用的一个函数改坏（定义改了名，调用处没改），作为下一个版本发布，看巡检能不能发现……"));
        }
        let (inbox, project, lang) = (self.rt.inbox.clone(), project.to_string(), i18n::lang());
        std::thread::spawn(move || {
            i18n::set(lang);
            let result = bad_release(&project);
            crate::events::post(&inbox, crate::events::LoopEvent::CardLoopDrilled { project, result });
        });
    }

    pub(crate) fn card_loop_drilled(&mut self, project: &str, result: Result<(String, String), String>) {
        self.card_loop.drilling.remove(project);
        let pi = self.store.projects.iter().position(|p| p.path == project);
        let at = pi.and_then(|pi| self.live_session(pi, project));
        match result {
            Ok((version, name)) => {
                self.card_loop_update(project, |w| {
                    w.bad_release = Some((version.clone(), name.clone()));
                    w.last = 0;
                });
                if let Some(at) = at {
                    self.system(at, &i18n::pick(format!("Drill: published {version} with {name} broken (renamed where it is defined, not where it is called). The live watch runs it now."),
                        format!("演练：已发布 {version}，其中 {name} 被改坏（定义改了名，调用处没改）。巡检马上运行它。")));
                }
            }
            Err(err) => {
                if let Some(at) = at {
                    self.system(at, &i18n::pick(format!("The bad-release drill could not run: {err}"), format!("坏版本演练没能进行：{err}")));
                }
            }
        }
    }

    /// A version of `project` was published (from here or its session): the
    /// watch runs it next, and an open incident records the fix.
    pub(crate) fn card_loop_published(&mut self, project: &str, version: Option<String>) {
        let Some(version) = version else { return };
        let now = now_secs();
        self.card_loop_update(project, |w| {
            w.last = 0;
            w.warm = w.drill.is_some();
            if let Some(i) = w.incident.as_mut() {
                i.published = Some((version, now));
            }
        });
    }

    /// Incidents for their outer loop without asking (auto repair).
    pub(crate) fn card_loop_pending(&mut self, cx: &mut Cx) {
        for project in std::mem::take(&mut self.card_loop.pending_repairs) {
            if let Some(pi) = self.store.projects.iter().position(|p| p.path == project) {
                self.card_loop_repair(cx, pi, &project, false);
            }
        }
    }

    /// Each second: a watched app whose run is due is run.
    pub(crate) fn card_loop_tick(&mut self) {
        self.card_loop_load();
        // A report's call whose run is slow is answered before the host gives up on it.
        let now = now_secs();
        let (late, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.card_loop.waiting).into_iter().partition(|(_, at, _)| now.saturating_sub(*at) >= 20);
        self.card_loop.waiting = kept;
        for (_, _, reply) in late {
            reply.finish(ToolOutcome::Ok(json!({"checking": true,
                "next": "OctoBuddy is still running the published version; its session will say what it found, and octobuddy.status shows it in a minute."})));
        }
        // Its pace and drill as watch.json has them now (a demo may change them there).
        for (project, w) in self.card_loop.watches.iter_mut() {
            if let Some(file) = Watch::read(project) {
                w.every = file.every;
                w.drill = file.drill;
            }
        }
        let now = now_secs();
        let due: Vec<(String, Option<String>)> = self.card_loop.watches.iter()
            .filter(|(p, w)| w.watching && !self.card_loop.running.contains(*p) && now.saturating_sub(w.last) >= w.every)
            .map(|(p, w)| (p.clone(), if w.warm { None } else { w.drill.clone() })).collect();
        for (project, drill) in due {
            self.card_loop.running.insert(project.clone());
            if let Some(w) = self.card_loop.watches.get_mut(&project) {
                w.last = now;
            }
            let inbox = self.rt.inbox.clone();
            let lang = i18n::lang();
            std::thread::spawn(move || {
                i18n::set(lang);
                let result = probe(&project, drill.as_deref());
                crate::events::post(&inbox, crate::events::LoopEvent::CardLoopProbed { project, result });
            });
        }
    }

    /// A run came back: its health logged, an incident opened or closed, and
    /// a change said in the app's session.
    pub(crate) fn card_loop_probed(&mut self, project: &str, result: Result<Probe, String>) {
        self.card_loop.running.remove(project);
        let Some(mut w) = self.card_loop.watches.get(project).cloned().filter(|w| w.watching) else { return };
        let at = self.store.find_session(&w.session);
        let p = match result {
            Ok(p) => p,
            Err(err) => {
                log!("octobuddy: card-loop {project}: {err}");
                let (waiting, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.card_loop.waiting).into_iter().partition(|(p, _, _)| p == project);
                self.card_loop.waiting = kept;
                for (_, _, reply) in waiting {
                    reply.finish(ToolOutcome::error("check_failed", format!("OctoBuddy could not run the published app: {err}")));
                }
                if let Some(at) = at.filter(|_| w.health.is_some()) {
                    self.system(at, &i18n::pick(format!("Live watch could not run the app: {err}"), format!("巡检没能运行应用：{err}")));
                }
                return;
            }
        };
        let name = Watch::app_name(project);
        // The warm run of a version just published: logged, judged by its
        // drilled runs (the next ones), not by this one.
        if w.warm && p.drill.is_none() {
            let (health, why) = judge(&p, None);
            log(project, &p, health, &why);
            w.warm = false;
            w.last = p.at.saturating_sub(w.every.saturating_sub(5));
            w.write(project);
            self.card_loop.watches.insert(project.to_string(), w);
            return;
        }
        // A new version is judged against itself once it runs well; until
        // then against what its last version showed (content kept or not).
        let (health, why) = match w.baseline.as_ref() {
            // (its widgets may be new: only its content and failures count).
            Some(b) if b.version != p.version => judge(&p, Some(&Baseline { version: p.version.clone(), ids: Vec::new(), ..b.clone() })),
            other => judge(&p, other),
        };
        log(project, &p, health, &why);
        if health == Health::Healthy && p.drill.is_none() && w.baseline.as_ref().is_none_or(|b| b.version != p.version) {
            w.baseline = Some(Baseline::of(&p));
        }
        let was = w.health;
        w.health = Some(health);
        w.last = p.at;
        w.texts = texts(&p);
        let drill = if p.drill.is_some() { i18n::t(" (drill: its API cut off)", "（演练：断开了它的 API）") }
            else if w.bad_release.as_ref().is_some_and(|(v, _)| *v == p.version) { i18n::t(" (drill: a bad release)", "（演练：坏版本）") } else { "" };
        let mut text = None;
        match (w.incident.take(), health) {
            // Well, and was: nothing to say (but the first run).
            (None, Health::Healthy) => {
                if was.is_none() {
                    text = Some(i18n::pick(
                        format!("Live watch: the published {name} {} runs well{drill}. That run is its baseline ({} widgets with text).", p.version, w.texts),
                        format!("巡检：已发布的 {name} {} 运行正常{drill}。这次运行作为基线（{} 个有文字的控件）。", p.version, w.texts)));
                }
            }
            // Well again: the incident's story, start to end.
            (Some(inc), Health::Healthy) => {
                w.bad_release = None;
                let mut steps = Vec::new();
                if let Some((_, at)) = &inc.repair {
                    steps.push(i18n::pick(format!("handed to the outer loop after {}", span(at.saturating_sub(inc.since))), format!("{}后交给外环", span(at.saturating_sub(inc.since)))));
                }
                if let Some((v, at)) = &inc.published {
                    steps.push(i18n::pick(format!("{v} published after {}", span(at.saturating_sub(inc.since))), format!("{}后发布 {v}", span(at.saturating_sub(inc.since)))));
                }
                let steps = if steps.is_empty() { String::new() } else { format!(" {}", steps.join(i18n::t(", ", "，"))) };
                text = Some(i18n::pick(
                    format!("✓ Live watch: the published {name} {} is well again{drill}, {} after it broke.{steps}", p.version, span(p.at.saturating_sub(inc.since))),
                    format!("✓ 巡检：已发布的 {name} {} 恢复正常{drill}，距发现问题 {}。{steps}", p.version, span(p.at.saturating_sub(inc.since)))));
            }
            // Still not well: kept, with what it shows now.
            (Some(mut inc), bad) => {
                let worse = bad > inc.health;
                inc.health = bad;
                inc.why = why.clone();
                inc.version = p.version.clone();
                if worse {
                    text = Some(i18n::pick(format!("⚠ Live watch: {name} {} is now {}.", p.version, bad.word()), format!("⚠ 巡检：{name} {} 现在{}。", p.version, bad.word())));
                }
                w.incident = Some(inc);
            }
            // It broke: an incident.
            (None, bad) => {
                let bad_release = w.bad_release.as_ref().is_some_and(|(v, _)| *v == p.version).then(|| "bad-release".to_string());
                let inc = Incident { since: p.at, version: p.version.clone(), health: bad, why: why.clone(), shot: shot_of(project, &p), texts: w.texts, drill: p.drill.clone().or(bad_release), repair: None, published: None, reports: Vec::new() };
                let lines: Vec<String> = why.iter().map(|l| format!("· {l}")).collect();
                let shot = inc.shot.clone().unwrap_or_default();
                text = Some(i18n::pick(
                    format!("⚠ Live watch: the published {name} {} is {}{drill}.\n{}\nScreenshot: {shot}\nThe Live page (sidebar) can hand it to this session's outer loop to repair.", p.version, bad.word(), lines.join("\n")),
                    format!("⚠ 巡检发现故障：已发布的 {name} {} {}{drill}。\n{}\n截图：{shot}\n可以在侧栏的「巡检」页点「修复」，交给这个会话的外环去修。", p.version, bad.word(), lines.join("\n"))));
                if w.auto_repair {
                    self.card_loop.pending_repairs.push(project.to_string());
                }
                w.incident = Some(inc);
            }
        }
        // What others reported, found or not by this run; their calls answered.
        let fresh: Vec<Report> = w.reports.iter().filter(|r| r.reproduced.is_none()).cloned().collect();
        let found = health != Health::Healthy;
        for r in w.reports.iter_mut().filter(|r| r.reproduced.is_none()) {
            r.reproduced = Some(found);
        }
        let mut note = None;
        if !fresh.is_empty() {
            if let Some(i) = w.incident.as_mut().filter(|_| found) {
                i.reports.extend(fresh.iter().cloned().map(|mut r| { r.reproduced = Some(true); r }));
            }
            if !found {
                let r = &fresh[fresh.len() - 1];
                note = Some(i18n::pick(
                    format!("Live watch: the published {name} {} runs well; the run did not show what {} reported (“{}”). It may happen only on a real device (a permission, the host). The Live page can still hand the report to the outer loop.", p.version, r.who(), r.problem),
                    format!("巡检复查：已发布的 {name} {} 运行正常，没有看到{}报告的「{}」。这可能只在真实设备上出现（比如权限、宿主）。巡检页仍然可以点「修复」，把报告交给外环。", p.version, r.who(), r.problem)));
            }
        }
        let answer = json!({
            "app": name, "published": p.version, "health": health.key(), "found_a_problem": found, "why": why,
            // What it showed, as text (an agent may not reach the screenshot's folder).
            "on_screen": p.widgets.iter().map(|w| w.2.trim()).filter(|t| !t.is_empty()).take(24).collect::<Vec<_>>(),
            "screenshot_for_the_person": shot_of(project, &p).map(|s| Path::new(project).join(s).display().to_string()),
            "incident_open": w.incident.is_some(),
            "next": if found { "OctoBuddy opened an incident and keeps watching the app. The person hands it to the app's outer loop to repair from OctoBuddy's Live page (or it goes there by itself if they turned Auto repair on); octobuddy.status shows how it goes." }
                else { "The run of the published version shows nothing wrong. OctoBuddy keeps watching it; the person can still hand the report to the app's outer loop from OctoBuddy's Live page." },
        });
        let (waiting, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.card_loop.waiting).into_iter().partition(|(p, _, _)| p == project);
        self.card_loop.waiting = kept;
        for (_, _, reply) in waiting {
            reply.finish(ToolOutcome::Ok(answer.clone()));
        }
        w.write(project);
        self.card_loop.watches.insert(project.to_string(), w);
        for text in [text, note].into_iter().flatten() {
            if let Some(at) = at {
                self.system(at, &text);
            }
        }
    }

    /// The app projects a name may mean: its id, its name, its folder.
    fn find_app(&self, app: &str) -> Vec<(usize, String)> {
        let needle = app.trim().to_lowercase();
        let apps: Vec<(usize, String)> = self.store.projects.iter().enumerate()
            .filter(|(_, p)| !p.is_chats() && super::octosense_app::is_app(&p.path)).map(|(i, p)| (i, p.path.clone())).collect();
        let exact: Vec<(usize, String)> = apps.iter().filter(|(_, p)| {
            app_id(p).is_ok_and(|id| id.to_lowercase() == needle) || Watch::app_name(p).to_lowercase() == needle
                || Path::new(p).file_name().is_some_and(|f| f.to_string_lossy().to_lowercase() == needle)
        }).cloned().collect();
        if !exact.is_empty() {
            return exact;
        }
        apps.into_iter().filter(|(_, p)| {
            let name = Watch::app_name(p).to_lowercase();
            !needle.is_empty() && (name.contains(&needle) || needle.contains(&name))
        }).collect()
    }

    /// `octobuddy.report`: a problem with one of its apps, from the system
    /// agent. The published version runs now (its watch on, if it was not);
    /// the call is answered with what that run found.
    pub(crate) fn card_loop_report(&mut self, args: &Value, reply: ToolReply) {
        self.card_loop_load();
        let app = args["app"].as_str().unwrap_or("").trim().to_string();
        let problem = args["problem"].as_str().unwrap_or("").trim().to_string();
        let from = args["from"].as_str().unwrap_or("").trim().to_string();
        if app.is_empty() || problem.is_empty() {
            reply.finish(ToolOutcome::error("bad_args", "name the app and say what is wrong with it"));
            return;
        }
        let (pi, project) = match self.find_app(&app).as_slice() {
            [one] => one.clone(),
            [] => {
                let names: Vec<String> = self.store.projects.iter().filter(|p| !p.is_chats() && super::octosense_app::is_app(&p.path)).map(|p| Watch::app_name(&p.path)).collect();
                reply.finish(ToolOutcome::error("no_app", format!("OctoBuddy has no app project called \"{app}\" (its apps: {})", names.join(", "))));
                return;
            }
            many => {
                let names: Vec<String> = many.iter().map(|(_, p)| Watch::app_name(p)).collect();
                reply.finish(ToolOutcome::error("ambiguous", format!("\"{app}\" may mean {}: name one", names.join(", "))));
                return;
            }
        };
        let version = match published(&project) {
            Ok((v, _)) => v,
            Err(err) => {
                reply.finish(ToolOutcome::error("not_published", format!("{}: {err}; OctoBuddy watches only what it published to the local App Hub", Watch::app_name(&project))));
                return;
            }
        };
        if !self.card_loop.watches.get(&project).is_some_and(|w| w.watching) {
            self.card_loop_watch(pi, &project, true);
        }
        let report = Report { at: now_secs(), from: from.clone(), problem: problem.clone(), reproduced: None };
        let who = report.who();
        self.card_loop_update(&project, |w| {
            w.reports.push(report);
            let extra = w.reports.len().saturating_sub(5);
            w.reports.drain(..extra);
            w.last = 0;
        });
        if let Some(at) = self.live_session(pi, &project) {
            let name = Watch::app_name(&project);
            self.system(at, &i18n::pick(format!("The OctoSense assistant passes on a problem with {name} (seen by {who}): “{problem}”. Running the published {version} now to check…"),
                format!("系统 agent 转来 {name} 的问题（{who}看到的）：「{problem}」。正在复查已发布的 {version}……")));
        }
        self.card_loop.waiting.push((project, now_secs(), reply));
    }

    /// The Live page as the system agent reads it (`octobuddy.status`).
    pub(crate) fn card_loop_status(&self) -> Value {
        let now = now_secs();
        let apps: Vec<Value> = self.store.projects.iter().filter(|p| !p.is_chats() && super::octosense_app::is_app(&p.path)).map(|p| {
            let w = self.card_loop.watches.get(&p.path).filter(|w| w.watching).cloned().or_else(|| Watch::read(&p.path).filter(|w| w.watching));
            json!({
                "name": Watch::app_name(&p.path), "id": app_id(&p.path).ok(), "project": p.name,
                "published": published(&p.path).ok().map(|(v, _)| v),
                "watching": w.is_some(),
                "health": w.as_ref().and_then(|w| w.health).map(Health::key),
                "last_run_secs_ago": w.as_ref().filter(|w| w.last > 0).map(|w| now.saturating_sub(w.last)),
                "every_secs": w.as_ref().map(|w| w.every),
                "drill": w.as_ref().and_then(|w| w.drill.clone()),
                "incident": w.as_ref().and_then(|w| w.incident.as_ref()).map(|i| json!({
                    "since_secs_ago": now.saturating_sub(i.since), "version": i.version, "health": i.health.key(), "why": i.why,
                    "handed_to_outer_loop_secs_ago": i.repair.as_ref().map(|(_, at)| now.saturating_sub(*at)),
                    "fixed_version_published": i.published.as_ref().map(|(v, _)| v),
                    "reports": i.reports.iter().map(Report::to_json).collect::<Vec<_>>(),
                })),
                "last_report": w.as_ref().and_then(|w| w.reports.last()).map(Report::to_json),
            })
        }).collect();
        json!({"apps": apps})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(widgets: &[(&str, &str, &str)]) -> Probe {
        Probe {
            version: "0.1.0".into(), started: true,
            widgets: widgets.iter().map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string())).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn a_run_is_judged_against_its_baseline() {
        let well = run(&[("Label", "title", "汇率看板"), ("Label", "usd_cny", "7.10"), ("Label", "usd_eur", "0.92"),
            ("Label", "updated", "更新于 10:00"), ("Label", "help", "加载失败时会自动重试")]);
        let base = Baseline::of(&well);
        assert_eq!(judge(&well, Some(&base)).0, Health::Healthy, "its own help line is not news");
        // Its rates gone and a failure shown.
        let bad = run(&[("Label", "title", "汇率看板"), ("Label", "error", "汇率加载失败，请稍后再试"), ("Label", "help", "加载失败时会自动重试")]);
        let (h, why) = judge(&bad, Some(&base));
        assert_eq!(h, Health::Degraded);
        assert!(why.iter().any(|w| w.contains("汇率加载失败")), "{why:?}");
        assert!(why.iter().any(|w| w.contains("usd_cny")), "{why:?}");
        // Script errors, or no start at all, are worse.
        let mut broken = well.clone();
        broken.errors = vec!["splash:71: field rates missing".into()];
        assert_eq!(judge(&broken, Some(&base)).0, Health::Broken);
        let down = Probe { started: false, errors: vec!["refused: manifest".into()], ..Default::default() };
        assert_eq!(judge(&down, None).0, Health::Down);
        // Offline, but its last rates still shown: coping.
        let coping = run(&[("Label", "title", "汇率看板"), ("Label", "usd_cny", "7.10"), ("Label", "usd_eur", "0.92"),
            ("Label", "offline", "离线：显示的是 10:00 的汇率"), ("Label", "help", "加载失败时会自动重试")]);
        assert_eq!(judge(&coping, Some(&base)).0, Health::Healthy);
    }

    #[test]
    fn the_bad_release_breaks_the_function_called_most() {
        let source = "fn once(){ return 1 }\nfn twice(x){ return x }\nfn boot(){\n    once()\n    twice(1)\n    twice(2)\n}\nboot()\n";
        let (name, broken) = break_a_function(source).unwrap();
        assert_eq!(name, "twice");
        assert!(broken.contains("fn twice_v2(x)") && broken.contains("    twice(1)") && broken.contains("fn once()"));
        assert_eq!(break_a_function("fn lonely(){ return 1 }\n"), None, "a function nobody calls is not the one to break");
        // `cache(` inside `load_cache(` is not a call of `cache`.
        let (name, _) = break_a_function("fn cache(){ }\nfn load_cache(){ }\nfn boot(){\n    load_cache()\n    load_cache()\n    cache()\n}\n").unwrap();
        assert_eq!(name, "load_cache");
    }

    #[test]
    fn the_newest_published_version_is_watched() {
        let catalog = json!({"entries": [
            {"artifact": "artifacts/fx-board-0.1.0.bundle", "manifest": {"id": "fx-board", "version": "0.1.0"}},
            {"artifact": "artifacts/fx-board-0.1.10.bundle", "manifest": {"id": "fx-board", "version": "0.1.10"}},
            {"artifact": "artifacts/fx-board-0.1.9.bundle", "manifest": {"id": "fx-board", "version": "0.1.9"}},
            {"artifact": "artifacts/tip-calc-0.2.0.bundle", "manifest": {"id": "tip-calc", "version": "0.2.0"}},
        ]});
        assert_eq!(newest_in(&catalog, "fx-board"), Some(("0.1.10".into(), "artifacts/fx-board-0.1.10.bundle".into())));
        assert_eq!(newest_in(&catalog, "nope"), None);
    }

    /// A published app run for real, as it is and with its API cut off (where
    /// it ran before, and on a new device):
    /// `OCTOBUDDY_HOME=<data> OCTOBUDDY_CARD_LOOP_PROJECT=<app project> cargo test -- --ignored probes_a_published`.
    #[test]
    #[ignore]
    fn probes_a_published_app_well_and_cut_off() {
        let project = std::env::var("OCTOBUDDY_CARD_LOOP_PROJECT").expect("OCTOBUDDY_CARD_LOOP_PROJECT");
        let well = probe(&project, None).unwrap();
        let (health, why) = judge(&well, None);
        println!("as it is: {health:?} {why:?}, {} widgets, {} errors, {} ms, shot {:?}", well.widgets.len(), well.errors.len(), well.took_ms, well.shot);
        assert_eq!(health, Health::Healthy);
        let base = Baseline::of(&well);
        // Its API down where it ran before: what it kept carries it (fx-board does).
        let cut = probe(&project, Some("offline")).unwrap();
        let (health, why) = judge(&cut, Some(&base));
        println!("cut off, kept: {health:?} {why:?}, {} widgets, shot {:?}", cut.widgets.len(), cut.shot);
        // On a new device, nothing kept: it has nothing to show.
        let fresh = probe(&project, Some("offline-fresh")).unwrap();
        let (fresh_health, why) = judge(&fresh, Some(&base));
        println!("cut off, new device: {fresh_health:?} {why:?}, {} widgets, shot {:?}", fresh.widgets.len(), fresh.shot);
        assert_ne!(fresh_health, Health::Healthy);
    }

    #[test]
    fn a_watch_keeps_its_state() {
        let dir = std::env::temp_dir().join(format!("octobuddy-card-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.to_string_lossy().into_owned();
        let incident = Incident { since: 5, version: "0.1.0".into(), health: Health::Degraded, why: vec!["it shows “x”".into()], shot: Some("s.png".into()),
            texts: 8, drill: Some("offline".into()), repair: Some(("s-1".into(), 7)), published: Some(("0.1.1".into(), 8)),
            reports: vec![Report { at: 4, from: "the person".into(), problem: "空白".into(), reproduced: Some(true) }] };
        let w = Watch { watching: true, every: 120, session: "s-1".into(), baseline: Some(Baseline { version: "0.1.0".into(), ids: vec!["a".into()], texts: 3, failure_texts: vec![] }),
            health: Some(Health::Degraded), last: 9, texts: 8, drill: Some("offline".into()), auto_repair: true, warm: true, incident: Some(incident),
            reports: vec![Report { at: 10, from: String::new(), problem: "打不开".into(), reproduced: None }],
            bad_release: Some(("0.1.1".into(), "fetch".into())) };
        w.write(&project);
        let back = Watch::read(&project).unwrap();
        assert_eq!((back.watching, back.every, back.session.as_str(), back.health, back.last, back.texts, back.drill.as_deref(), back.auto_repair),
            (true, 120, "s-1", Some(Health::Degraded), 9, 8, Some("offline"), true));
        assert!(back.warm);
        assert_eq!((back.baseline, back.incident, back.reports, back.bad_release), (w.baseline, w.incident, w.reports, w.bad_release));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
