//! The production loop (`card-loop`): OctoBuddy keeps watch over the version
//! of an app it published to the local App Hub — the one people run — and
//! says when it breaks (GOSIM 2026's "production-loop agent": read its own
//! runtime signals, find a failing card, repair it, publish it again).
//!
//! What runs now: the watch. Every few minutes (`watch.json`'s `every`) it
//! runs the published copy headless on live data, as the app check does,
//! and reads what it showed: whether it started, its script errors, the
//! widgets on screen, a screenshot. The first healthy run of a version is
//! its baseline; a later run is judged against it ([`judge`]): it did not
//! start (down), it has script errors (broken), its key widgets are gone or
//! it shows a failure it did not before (degraded). Each run is a line of
//! `<project>/.octobuddy/card-loop/health.jsonl`; a change of health is a
//! message in the session the watch was started from. Repairing it and
//! publishing again come next.
//!
//! A drill for the demo (`watch.json`'s `drill`): `offline` runs the copy
//! with its network hosts swapped for one that never answers, as if its API
//! were down. The published copy itself is never touched.
use crate::model::now_secs;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Health {
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
    let new_failures: Vec<String> = failure_texts(p).into_iter()
        .filter(|t| baseline.is_none_or(|b| !b.failure_texts.contains(t))).collect();
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
bundle="$1"; octo="$2"; shot="$3"; drill="$4"
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
if drill == "offline":
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
started=1
python3 "$octo" run "$work/bundle" --port "$port" --hidden --detach --timeout 30 > "$work/run.txt" 2>&1 || started=0
if [ $started = 1 ]; then
  # Its live data comes in.
  sleep 4
  python3 "$octo" shot "$port" "$shot" > /dev/null 2>&1
  curl -s -m 5 "http://127.0.0.1:$port/snap" > "$work/snap.json"
  curl -s -m 5 "http://127.0.0.1:$port/quit" > /dev/null 2>&1
  sleep 1
fi
python3 - "$work" "$started" <<'PY'
import json, os, re, sys
work, started = sys.argv[1], sys.argv[2] == "1"
def lines(p):
    try:
        return open(p, errors="replace").read().splitlines()
    except Exception:
        return []
script = re.compile(r'\[E\]|splash:[0-9]+:|on_render closure failed|callback error')
if started:
    errors = [l.strip() for l in lines(os.path.join(work, ".local-state/card-host.log")) if script.search(l)][:20]
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

/// A watched app: `<project>/.octobuddy/card-loop/watch.json`.
#[derive(Clone, Debug, Default)]
pub struct Watch {
    pub watching: bool,
    pub every: u64,
    /// The session told about it.
    pub session: String,
    pub baseline: Option<Baseline>,
    pub health: Option<Health>,
    /// Since when it has been as it is.
    pub since: u64,
    pub last: u64,
    pub drill: Option<String>,
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
            since: v["since"].as_u64().unwrap_or(0),
            last: v["last"].as_u64().unwrap_or(0),
            drill: v["drill"].as_str().filter(|d| !d.is_empty()).map(String::from),
        })
    }

    fn write(&self, project: &str) {
        let dir = loop_dir(project);
        let _ = std::fs::create_dir_all(&dir);
        let v = json!({
            "watching": self.watching, "every": self.every, "session": self.session,
            "baseline": self.baseline.as_ref().map(Baseline::to_json), "health": self.health.map(Health::key),
            "since": self.since, "last": self.last, "drill": self.drill,
        });
        let _ = std::fs::write(dir.join("watch.json"), serde_json::to_string_pretty(&v).unwrap_or_default());
    }
}

fn log(project: &str, p: &Probe, health: Health, why: &[String]) {
    use std::io::Write;
    let line = json!({
        "at": p.at, "version": p.version, "health": health.key(), "why": why, "started": p.started,
        "errors": p.errors.len(), "widgets": p.widgets.len(), "texts": texts(p), "took_ms": p.took_ms,
        "shot": p.shot.as_ref().and_then(|s| s.strip_prefix(project).ok()).map(|s| s.display().to_string()), "drill": p.drill,
    });
    let dir = loop_dir(project);
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("health.jsonl")) {
        let _ = writeln!(f, "{line}");
    }
}

/// The watches of this window's projects.
#[derive(Default)]
pub struct Loops {
    pub watches: BTreeMap<String, Watch>,
    pub running: HashSet<String>,
    loaded: bool,
}

const BUTTONS: [LiveId; 3] = [live_id!(loop_btn), live_id!(loop_btn_ok), live_id!(loop_btn_bad)];

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

    /// The Live pill of an app project: off, well, or not.
    pub(crate) fn sync_card_loop(&mut self, cx: &mut Cx, project: Option<&str>) {
        self.card_loop_load();
        let shown = project.filter(|p| super::active(super::CARD_LOOP, p));
        let watch = shown.and_then(|p| self.card_loop.watches.get(p)).filter(|w| w.watching).cloned();
        let running = shown.is_some_and(|p| self.card_loop.running.contains(p));
        let which = match &watch {
            None => 0,
            Some(w) if w.health.is_none_or(|h| h == Health::Healthy) => 1,
            Some(_) => 2,
        };
        for (i, id) in BUTTONS.iter().enumerate() {
            self.view.view(cx, &[*id]).set_visible(cx, shown.is_some() && i == which);
        }
        let id = BUTTONS[which];
        self.view.label(cx, &[id, live_id!(name)]).set_text(cx, i18n::t("Live", "巡检"));
        let sub = match (&watch, running) {
            (None, _) => i18n::t("off", "关").to_string(),
            (Some(_), true) => i18n::t("checking…", "检查中…").to_string(),
            (Some(w), false) => match w.health {
                None => i18n::t("on", "开").to_string(),
                Some(h) => format!("{} · {}", h.word(), ago(now_secs().saturating_sub(w.last))),
            },
        };
        self.view.label(cx, &[id, live_id!(sub)]).set_text(cx, &sub);
    }

    /// The Live pill pressed: the watch on (run now) or off.
    pub(crate) fn card_loop_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if !BUTTONS.iter().any(|id| tapped(&self.view.view(cx, &[*id]), actions)) {
            return;
        }
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        let project = self.store.projects[at.0].path.clone();
        let session = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
        let mut w = Watch::read(&project).unwrap_or_default();
        if self.card_loop.watches.get(&project).is_some_and(|w| w.watching) {
            w.watching = false;
            w.write(&project);
            self.card_loop.watches.remove(&project);
            self.system(at, i18n::t("Live watch off: OctoBuddy no longer runs the published app.", "已关闭巡检：OctoBuddy 不再运行已发布的应用。"));
        } else {
            match published(&project) {
                Err(err) => {
                    self.system(at, &i18n::pick(format!("Live watch needs the app on the local App Hub ({err}): publish it first; the watch runs the published version."),
                        format!("巡检需要应用已发布到本地 App Hub（{err}）：先点「发布」；巡检的是发布出去的那个版本。")));
                }
                Ok((version, _)) => {
                    let name = Watch::app_name(&project);
                    w.watching = true;
                    w.every = if w.every == 0 { EVERY } else { w.every };
                    w.session = session;
                    w.last = 0;
                    w.write(&project);
                    self.card_loop.watches.insert(project.clone(), w.clone());
                    self.system(at, &i18n::pick(format!("Live watch on: OctoBuddy runs the published {name} {version} every {} min on live data and says here when it breaks. Its record: .octobuddy/card-loop/health.jsonl", w.every / 60),
                        format!("已开启巡检：每 {} 分钟用实时数据运行一次已发布的 {name} {version}，出问题会在这里说。记录在 .octobuddy/card-loop/health.jsonl", w.every / 60)));
                }
            }
        }
        self.relayout(cx);
    }

    /// Each second: a watched app whose run is due is run.
    pub(crate) fn card_loop_tick(&mut self) {
        self.card_loop_load();
        // Its pace and drill as watch.json has them now (a demo changes them there).
        for (project, w) in self.card_loop.watches.iter_mut() {
            if let Some(file) = Watch::read(project) {
                w.every = file.every;
                w.drill = file.drill;
            }
        }
        let now = now_secs();
        let due: Vec<(String, Option<String>)> = self.card_loop.watches.iter()
            .filter(|(p, w)| w.watching && !self.card_loop.running.contains(*p) && now.saturating_sub(w.last) >= w.every)
            .map(|(p, w)| (p.clone(), w.drill.clone())).collect();
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

    /// A run came back: its health logged, and a change of it said.
    pub(crate) fn card_loop_probed(&mut self, project: &str, result: Result<Probe, String>) {
        self.card_loop.running.remove(project);
        let Some(mut w) = self.card_loop.watches.get(project).cloned().filter(|w| w.watching) else { return };
        let at = self.store.find_session(&w.session);
        let p = match result {
            Ok(p) => p,
            Err(err) => {
                log!("octobuddy: card-loop {project}: {err}");
                if let Some(at) = at.filter(|_| w.health.is_some()) {
                    self.system(at, &i18n::pick(format!("Live watch could not run the app: {err}"), format!("巡检没能运行应用：{err}")));
                }
                return;
            }
        };
        let name = Watch::app_name(project);
        // A new version: it is judged against itself once it runs well.
        if w.baseline.as_ref().is_some_and(|b| b.version != p.version) {
            w.baseline = None;
        }
        let (health, why) = judge(&p, w.baseline.as_ref());
        log(project, &p, health, &why);
        let was = w.health;
        if health == Health::Healthy && w.baseline.is_none() && p.drill.is_none() {
            w.baseline = Some(Baseline::of(&p));
        }
        // How long it was as it was, before this run.
        let lasted = p.at.saturating_sub(w.since);
        if was != Some(health) {
            w.since = p.at;
        }
        w.health = Some(health);
        w.last = p.at;
        w.write(project);
        let shot = p.shot.as_ref().and_then(|s| s.strip_prefix(project).ok()).map(|s| s.display().to_string()).unwrap_or_default();
        let drill = if p.drill.is_some() { i18n::t(" (drill: its API cut off)", "（演练：断开了它的 API）") } else { "" };
        let text = match (was, health) {
            (None, Health::Healthy) => Some(i18n::pick(
                format!("Live watch: the published {name} {} runs well{drill}. That run is its baseline ({} named widgets).", p.version, w.baseline.as_ref().map(|b| b.ids.len()).unwrap_or(0)),
                format!("巡检：已发布的 {name} {} 运行正常{drill}。这次运行作为基线（{} 个命名控件）。", p.version, w.baseline.as_ref().map(|b| b.ids.len()).unwrap_or(0)))),
            (Some(Health::Healthy), Health::Healthy) => None,
            (_, Health::Healthy) => Some(i18n::pick(
                format!("✓ Live watch: the published {name} {} is well again{drill}, after {}.", p.version, span(lasted)),
                format!("✓ 巡检：已发布的 {name} {} 恢复正常{drill}，故障持续了 {}。", p.version, span(lasted)))),
            (_, bad) if was == Some(bad) => None,
            (_, bad) => {
                let lines: Vec<String> = why.iter().map(|l| format!("· {l}")).collect();
                Some(i18n::pick(
                    format!("⚠ Live watch: the published {name} {} is {}{drill}.\n{}\nScreenshot: {shot}\nRecord: .octobuddy/card-loop/health.jsonl", p.version, bad.word(), lines.join("\n")),
                    format!("⚠ 巡检发现故障：已发布的 {name} {} {}{drill}。\n{}\n截图：{shot}\n记录：.octobuddy/card-loop/health.jsonl", p.version, bad.word(), lines.join("\n"))))
            }
        };
        self.card_loop.watches.insert(project.to_string(), w);
        if let (Some(at), Some(text)) = (at, text) {
            self.system(at, &text);
        }
    }
}

impl Watch {
    fn app_name(project: &str) -> String {
        std::fs::read_to_string(super::octosense_app::bundle(project).join("manifest.json")).ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|v| v["name"].as_str().map(String::from))
            .unwrap_or_else(|| Path::new(project).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
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

    /// A published app run for real, as it is and with its API cut off:
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
        let cut = probe(&project, Some("offline")).unwrap();
        let (health, why) = judge(&cut, Some(&base));
        println!("cut off: {health:?} {why:?}, {} widgets, shot {:?}", cut.widgets.len(), cut.shot);
        assert_ne!(health, Health::Healthy);
    }

    #[test]
    fn a_watch_keeps_its_state() {
        let dir = std::env::temp_dir().join(format!("octobuddy-card-loop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.to_string_lossy().into_owned();
        let w = Watch { watching: true, every: 120, session: "s-1".into(), baseline: Some(Baseline { version: "0.1.0".into(), ids: vec!["a".into()], texts: 3, failure_texts: vec![] }),
            health: Some(Health::Degraded), since: 5, last: 9, drill: Some("offline".into()) };
        w.write(&project);
        let back = Watch::read(&project).unwrap();
        assert_eq!((back.watching, back.every, back.session.as_str(), back.health, back.since, back.last, back.drill.as_deref()), (true, 120, "s-1", Some(Health::Degraded), 5, 9, Some("offline")));
        assert_eq!(back.baseline, w.baseline);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
