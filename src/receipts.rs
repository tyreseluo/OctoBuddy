//! Receipts of what OctoBuddy did outside its own memory (a commit, a
//! publish, a submission): prepared, confirmed, done, then read back.
//!
//! A receipt is prepared before anything runs (what is to be done, its
//! `inputs`, and the snapshot the person approved it on), marked executing
//! just before it runs, and settled by reading back what was written, by
//! another way than the one that wrote it (git's own record of a commit,
//! the catalog App Hub serves, the remote's tags, GitHub's issues):
//! confirmed when every check agrees, a mismatch when one disagrees,
//! unknown when one could not be made. A snapshot that moved since the
//! person looked leaves it stale, nothing done. One left executing or
//! unknown (OctoBuddy closed mid-way, a check could not be made) is only
//! read back again ([`reconcile`]), never done again; the same asked for
//! again finds it done ([`Prior::Done`]) or still open ([`Prior::Open`]).
//!
//! The ledger: `<data>/receipts.jsonl`, a line per change of a receipt
//! (all of it), read folded by id. It is OctoBuddy's, beside the local hub,
//! its keys and `requests.json`: out of the projects, where inner loops write.
use crate::i18n;
use crate::model::{data_dir, new_id, now_secs};
use crate::workspace;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// What a receipt is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An inner loop's files committed.
    Commit,
    /// An app published to the local App Hub (the person's Publish).
    Publish,
    /// An incident's fix published (the Live page's Publish fix).
    PublishFix,
    /// The bad-release drill's broken version, published on purpose.
    Drill,
    /// An app submitted to OctoSense's App Hub: its tag pushed, its issue opened.
    HubSubmit,
    /// A round's acceptance sheet (`acceptance`): made by OctoBuddy, confirmed by the person.
    Acceptance,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Commit => "commit",
            Kind::Publish => "publish",
            Kind::PublishFix => "publish-fix",
            Kind::Drill => "drill",
            Kind::HubSubmit => "hub-submit",
            Kind::Acceptance => "acceptance",
        }
    }

    fn parse(s: &str) -> Option<Kind> {
        [Kind::Commit, Kind::Publish, Kind::PublishFix, Kind::Drill, Kind::HubSubmit, Kind::Acceptance].into_iter().find(|k| k.key() == s)
    }

    /// What it changes: kinds that change the same thing (a publish, a fix,
    /// a drill: the local catalog) answer for each other.
    fn target(self) -> &'static str {
        match self {
            Kind::Commit => "git",
            Kind::Publish | Kind::PublishFix | Kind::Drill => "catalog",
            Kind::HubSubmit => "hub",
            Kind::Acceptance => "acceptance",
        }
    }

    /// Whether the same inputs, confirmed before, are done: a publish is
    /// (the catalog serves that content already), a submission is (that
    /// version is in); a commit never is (files that differ from HEAD are
    /// not committed, whatever was before), nor a drill (it breaks on purpose).
    /// A sheet is: the person confirmed it.
    fn repeats(self) -> bool {
        !matches!(self, Kind::Commit | Kind::Drill)
    }
}

/// Where a receipt is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Written down, nothing run.
    #[default]
    Prepared,
    /// Running, or cut off running: not known until read back.
    Executing,
    /// Read back as meant.
    Confirmed,
    /// Read back otherwise.
    Mismatch,
    /// A check could not be made.
    Unknown,
    /// Not done: it could not or should not be.
    Refused,
    /// Not done: what the person approved moved first.
    Stale,
}

impl Status {
    pub fn key(self) -> &'static str {
        match self {
            Status::Prepared => "prepared",
            Status::Executing => "executing",
            Status::Confirmed => "confirmed",
            Status::Mismatch => "mismatch",
            Status::Unknown => "unknown",
            Status::Refused => "refused",
            Status::Stale => "stale",
        }
    }

    fn parse(s: &str) -> Option<Status> {
        [Status::Prepared, Status::Executing, Status::Confirmed, Status::Mismatch, Status::Unknown, Status::Refused, Status::Stale]
            .into_iter().find(|st| st.key() == s)
    }

    /// Done or not, it is not known: read back before anything is done again.
    pub fn open(self) -> bool {
        matches!(self, Status::Executing | Status::Unknown)
    }
}

/// What an approval was given on: the commit, a digest of the content, the version.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub head: Option<String>,
    pub content: String,
    pub version: Option<String>,
}

impl Snapshot {
    /// Its content's digest, short, for a person.
    pub fn short(&self) -> &str {
        short(&self.content)
    }
}

/// One thing read back.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Check {
    /// What (a key: `catalog`, `digest`, `content`, `commit`, `files`,
    /// `tree`, `tag`, `issue`, `script`; [`line`] names it for a person).
    pub what: String,
    /// It agrees; `None`: it could not be checked.
    pub ok: Option<bool>,
    pub detail: String,
}

impl Check {
    pub fn new(what: &str, ok: Option<bool>, detail: impl Into<String>) -> Check {
        Check { what: what.to_string(), ok, detail: detail.into() }
    }
}

/// A receipt: what was done, on what, for whom, and how it read back.
#[derive(Clone, Debug, PartialEq)]
pub struct Receipt {
    pub id: String,
    pub kind: Kind,
    /// The project (a commit: the directory) it was done in.
    pub project: String,
    /// What is done, as a digest (a submission: `<id> <version>`): the
    /// same inputs are the same thing done.
    pub inputs: String,
    /// What the person approved it on.
    pub snap: Snapshot,
    /// `person`, `octosense:<tool>`, `outer loop`, `drill`.
    pub by: String,
    pub status: Status,
    /// When it last changed.
    pub at: u64,
    pub checks: Vec<Check>,
    /// What came of it: the commit (`%h %s`), the version, the issue; what
    /// stopped it, when it was not done.
    pub result: Option<String>,
}

impl Receipt {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id, "kind": self.kind.key(), "project": self.project, "inputs": self.inputs,
            "snap": {"head": self.snap.head, "content": self.snap.content, "version": self.snap.version},
            "by": self.by, "status": self.status.key(), "at": self.at,
            "checks": self.checks.iter().map(|c| json!({"what": c.what, "ok": c.ok, "detail": c.detail})).collect::<Vec<_>>(),
            "result": self.result,
        })
    }

    fn from_json(v: &Value) -> Option<Receipt> {
        let s = |v: &Value| v.as_str().unwrap_or("").to_string();
        Some(Receipt {
            id: v["id"].as_str()?.to_string(),
            kind: Kind::parse(v["kind"].as_str()?)?,
            project: s(&v["project"]),
            inputs: s(&v["inputs"]),
            snap: Snapshot { head: v["snap"]["head"].as_str().map(String::from), content: s(&v["snap"]["content"]), version: v["snap"]["version"].as_str().map(String::from) },
            by: s(&v["by"]),
            status: Status::parse(v["status"].as_str()?)?,
            at: v["at"].as_u64().unwrap_or(0),
            checks: v["checks"].as_array().map(|a| a.iter().map(|c| Check { what: s(&c["what"]), ok: c["ok"].as_bool(), detail: s(&c["detail"]) }).collect()).unwrap_or_default(),
            result: v["result"].as_str().map(String::from),
        })
    }
}

/// What the ledger has of the same thing asked for again.
#[derive(Clone, Debug, PartialEq)]
pub enum Prior {
    /// Confirmed already, and still the latest of its kind there.
    Done(Box<Receipt>),
    /// Executing or unknown: read it back before anything is done again.
    Open(Box<Receipt>),
}

/// A digest (sha-256, hex) of `parts`, each kept apart from the next.
pub fn digest(parts: &[&[u8]]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    for p in parts {
        h.update((p.len() as u64).to_le_bytes());
        h.update(p);
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// A digest or a commit, short, for a person.
pub fn short(s: &str) -> &str {
    s.get(..if s.len() == 40 { 7 } else { 6 }).unwrap_or(s)
}

/// Guards the ledger: a receipt's look at the ledger and its line, one at a time.
static LEDGER: Mutex<()> = Mutex::new(());
/// A ledger as read: its file, its length and time then, its receipts.
type Read = (PathBuf, u64, Option<std::time::SystemTime>, Vec<Receipt>);
/// The ledger as last read, so the pages that show receipts do not parse it each time.
static READ: Mutex<Option<Read>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LEDGER.lock().unwrap_or_else(|e| e.into_inner())
}

/// A ledger file: OctoBuddy's, or a test's own.
struct Ledger(PathBuf);

fn ledger() -> Ledger {
    Ledger(data_dir().join("receipts.jsonl"))
}

impl Ledger {
    /// Every receipt as its last line has it, in the order they were made.
    fn read(&self) -> Vec<Receipt> {
        let meta = std::fs::metadata(&self.0).ok();
        let (len, modified) = (meta.as_ref().map(|m| m.len()).unwrap_or(0), meta.and_then(|m| m.modified().ok()));
        let mut cache = READ.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((path, l, m, all)) = cache.as_ref() {
            if *path == self.0 && *l == len && *m == modified {
                return all.clone();
            }
        }
        let all = fold(&std::fs::read_to_string(&self.0).unwrap_or_default());
        *cache = Some((self.0.clone(), len, modified, all.clone()));
        all
    }

    fn append(&self, r: &Receipt) {
        use std::io::Write;
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.0) {
            let _ = writeln!(f, "{}", r.to_json());
        }
    }

    fn record(&self, r: &Receipt) {
        let _guard = lock();
        self.append(r);
    }

    fn prepare(&self, kind: Kind, project: &str, inputs: &str, snap: Snapshot, by: &str) -> Result<Receipt, Prior> {
        let _guard = lock();
        if let Some(prior) = prior_in(&self.read(), kind, project, inputs) {
            return Err(prior);
        }
        let r = draft(kind, project, inputs, snap, by);
        self.append(&r);
        Ok(r)
    }

    fn still(&self, r: &Receipt, now: &Snapshot) -> Result<(), String> {
        let moved = moved(&r.snap, now);
        if moved.is_empty() {
            return Ok(());
        }
        let mut stale = r.clone();
        stale.status = Status::Stale;
        stale.at = now_secs();
        stale.result = Some(moved.join(i18n::t("; ", "；")));
        self.record(&stale);
        Err(stale.result.unwrap_or_default())
    }

    fn executing(&self, r: &mut Receipt) {
        r.status = Status::Executing;
        r.at = now_secs();
        self.record(r);
    }

    fn settle(&self, r: &mut Receipt, checks: Vec<Check>, result: Option<String>) {
        r.status = status_of(&checks);
        r.checks = checks;
        if result.is_some() {
            r.result = result;
        }
        r.at = now_secs();
        self.record(r);
    }

    fn refuse(&self, r: &mut Receipt, why: &str) {
        r.status = Status::Refused;
        r.result = Some(why.to_string());
        r.at = now_secs();
        self.record(r);
    }
}

/// The ledger's lines folded by id: each receipt as its last line has it,
/// where its first line was.
fn fold(text: &str) -> Vec<Receipt> {
    let mut out: Vec<Receipt> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for r in text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter_map(|v| Receipt::from_json(&v)) {
        match at.get(&r.id) {
            Some(&i) => out[i] = r,
            None => {
                at.insert(r.id.clone(), out.len());
                out.push(r);
            }
        }
    }
    out
}

/// What the ledger has of `inputs` done in `project` by `kind` or its kin:
/// open (not known yet), or done (the latest confirmed there, the same).
fn prior_in(all: &[Receipt], kind: Kind, project: &str, inputs: &str) -> Option<Prior> {
    let kin: Vec<&Receipt> = all.iter().filter(|r| r.project == project && r.kind.target() == kind.target()).collect();
    if let Some(r) = kin.iter().rev().find(|r| r.inputs == inputs && r.status.open()) {
        return Some(Prior::Open(Box::new((*r).clone())));
    }
    let done = kin.iter().rev().find(|r| r.status == Status::Confirmed)?;
    (kind.repeats() && done.inputs == inputs).then(|| Prior::Done(Box::new((*done).clone())))
}

/// What moved between the snapshot approved and the one now, a line each.
pub fn moved(was: &Snapshot, now: &Snapshot) -> Vec<String> {
    let mut out = Vec::new();
    let name = |h: &Option<String>| h.as_deref().map(|h| short(h).to_string()).unwrap_or_else(|| "—".into());
    if was.head != now.head {
        out.push(i18n::pick(format!("its commit moved ({} → {})", name(&was.head), name(&now.head)), format!("提交变了（{} → {}）", name(&was.head), name(&now.head))));
    }
    if was.content != now.content {
        out.push(i18n::pick(format!("its content changed ({} → {})", short(&was.content), short(&now.content)), format!("内容变了（{} → {}）", short(&was.content), short(&now.content))));
    }
    if was.version != now.version {
        let v = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".into());
        out.push(i18n::pick(format!("its version changed ({} → {})", v(&was.version), v(&now.version)), format!("版本变了（{} → {}）", v(&was.version), v(&now.version))));
    }
    out
}

/// A settled receipt's status by its checks: confirmed when all agree, a
/// mismatch when one disagrees, else unknown.
pub fn status_of(checks: &[Check]) -> Status {
    if checks.iter().any(|c| c.ok == Some(false)) {
        Status::Mismatch
    } else if !checks.is_empty() && checks.iter().all(|c| c.ok == Some(true)) {
        Status::Confirmed
    } else {
        Status::Unknown
    }
}

/// A receipt not yet written down.
pub fn draft(kind: Kind, project: &str, inputs: &str, snap: Snapshot, by: &str) -> Receipt {
    // Two in the same tick are two receipts.
    static MADE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = MADE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Receipt {
        id: format!("{}{n:x}", new_id("rc")), kind, project: project.to_string(), inputs: inputs.to_string(), snap, by: by.to_string(),
        status: Status::Prepared, at: now_secs(), checks: Vec::new(), result: None,
    }
}

/// Writes down that `inputs` are to be done in `project`, unless the ledger
/// has them done already or open.
pub fn prepare(kind: Kind, project: &str, inputs: &str, snap: Snapshot, by: &str) -> Result<Receipt, Prior> {
    ledger().prepare(kind, project, inputs, snap, by)
}

/// What [`prepare`] would find, nothing written.
pub fn prior(kind: Kind, project: &str, inputs: &str) -> Option<Prior> {
    prior_in(&ledger().read(), kind, project, inputs)
}

pub fn executing(r: &mut Receipt) {
    ledger().executing(r);
}

/// Whether what `r` was approved on is still so: if not, it is written
/// down stale, and what moved said.
pub fn still(r: &Receipt, now: &Snapshot) -> Result<(), String> {
    ledger().still(r, now)
}

/// The read-back's checks into `r`: confirmed, a mismatch, or unknown.
pub fn settle(r: &mut Receipt, checks: Vec<Check>, result: Option<String>) {
    ledger().settle(r, checks, result);
    // A publish or submission that read back (or did not) goes to the
    // project's hagency room, at most once per receipt.
    if matches!(r.kind, Kind::Publish | Kind::PublishFix | Kind::HubSubmit) && matches!(r.status, Status::Confirmed | Status::Mismatch) {
        crate::hagency::tell(&r.project, &format!("receipt:{}", r.id), &line(r));
    }
}

/// `r` not done, and why.
pub fn refuse(r: &mut Receipt, why: &str) {
    ledger().refuse(r, why);
}

pub fn all() -> Vec<Receipt> {
    ledger().read()
}

pub fn find(id: &str) -> Option<Receipt> {
    all().into_iter().find(|r| r.id == id)
}

/// The latest receipt of `kind` in `project`.
pub fn latest(project: &str, kind: Kind) -> Option<Receipt> {
    latest_of(project, &[kind])
}

/// The latest receipt of any of `kinds` in `project`.
pub fn latest_of(project: &str, kinds: &[Kind]) -> Option<Receipt> {
    all().into_iter().rev().find(|r| r.project == project && kinds.contains(&r.kind))
}

/// The receipts not known to be done or not: executing or unknown.
pub fn open() -> Vec<Receipt> {
    all().into_iter().filter(|r| r.status.open()).collect()
}

/// Commits exactly `files` in `dir` as [`workspace::commit_files`] does,
/// with a receipt, then reads the commit back from git: the files it
/// changed (deletions too), its tree, what they hold. The same answer.
pub fn commit(dir: &str, files: &[String], message: &str, by: &str) -> Result<String, String> {
    commit_in(&ledger(), dir, files, message, by)
}

fn commit_in(l: &Ledger, dir: &str, files: &[String], message: &str, by: &str) -> Result<String, String> {
    let changes = changes(dir, files);
    // Nothing differs from HEAD: nothing to do, nothing to keep a receipt of.
    if changes.is_empty() {
        return workspace::commit_files(dir, files, message);
    }
    let inputs = commit_inputs(&changes);
    let snap = Snapshot { head: workspace::git(dir, &["rev-parse", "HEAD"]).ok(), content: inputs.clone(), version: None };
    let mut r = match l.prepare(Kind::Commit, dir, &inputs, snap.clone(), by) {
        Ok(r) => r,
        Err(Prior::Open(mut open)) => {
            // Cut off before: what it made, read back, never committed again.
            let (checks, made) = recover_commit(&open);
            l.settle(&mut open, checks, made);
            if open.status == Status::Confirmed {
                return Ok(open.result.unwrap_or_default());
            }
            l.prepare(Kind::Commit, dir, &inputs, snap, by).map_err(|_| "not committed: an earlier commit of these files is not settled".to_string())?
        }
        Err(Prior::Done(done)) => return Ok(done.result.unwrap_or_default()),
    };
    l.executing(&mut r);
    match workspace::commit_files(dir, files, message) {
        Ok(made) => {
            let sha = made.split_whitespace().next().unwrap_or("").to_string();
            let paths: Vec<String> = changes.iter().map(|(p, _)| p.clone()).collect();
            let mut checks = read_back_commit(dir, &sha, &paths);
            checks.push(content_check(&commit_inputs(&in_commit(dir, &sha)), &inputs));
            l.settle(&mut r, checks, Some(made.clone()));
            Ok(made)
        }
        Err(err) => {
            l.refuse(&mut r, &err);
            Err(err)
        }
    }
}

/// Of `files`, those that differ from HEAD in `dir`, and what each holds
/// now (git's blob id; none: deleted).
fn changes(dir: &str, files: &[String]) -> Vec<(String, Option<String>)> {
    let head: HashMap<String, String> = tree_blobs(dir, "HEAD", files);
    let now: HashMap<String, String> = workspace::file_hashes(dir, files).into_iter().collect();
    let mut out: Vec<(String, Option<String>)> = files.iter().filter_map(|f| match (head.get(f), now.get(f)) {
        (Some(h), Some(n)) if h == n => None,
        (None, None) => None,
        (_, n) => Some((f.clone(), n.cloned())),
    }).collect();
    out.sort();
    out.dedup();
    out
}

/// The blob of each of `files` in `rev`'s tree (those it has).
fn tree_blobs(dir: &str, rev: &str, files: &[String]) -> HashMap<String, String> {
    if files.is_empty() {
        return HashMap::new();
    }
    let mut args = vec!["ls-tree", "-z", rev, "--"];
    args.extend(files.iter().map(String::as_str));
    let out = workspace::git(dir, &args).unwrap_or_default();
    out.split('\0').filter_map(|e| {
        let (meta, path) = e.split_once('\t')?;
        Some((path.to_string(), meta.split_whitespace().nth(2)?.to_string()))
    }).collect()
}

/// What a commit's changes are, as [`prepare`] keys them.
fn commit_inputs(changes: &[(String, Option<String>)]) -> String {
    let mut sorted = changes.to_vec();
    sorted.sort();
    let text: String = sorted.iter().map(|(p, b)| format!("{p}\0{}\n", b.as_deref().unwrap_or("-"))).collect();
    digest(&[text.as_bytes()])
}

/// `(status, path)` of each file `sha` changed, as git records it.
fn name_status(dir: &str, sha: &str) -> Result<Vec<(String, String)>, String> {
    let out = workspace::git(dir, &["show", "--name-status", "--no-renames", "-z", "--format=", sha])?;
    let fields: Vec<&str> = out.split('\0').map(str::trim).filter(|f| !f.is_empty()).collect();
    Ok(fields.chunks(2).filter_map(|c| Some((c.first()?.to_string(), c.get(1)?.to_string()))).collect())
}

/// What `sha` changed, read from git: each file and its blob (none: deleted).
fn in_commit(dir: &str, sha: &str) -> Vec<(String, Option<String>)> {
    let changed = name_status(dir, sha).unwrap_or_default();
    let paths: Vec<String> = changed.iter().map(|(_, p)| p.clone()).collect();
    let blobs = tree_blobs(dir, sha, &paths);
    changed.into_iter().map(|(st, p)| {
        let blob = if st.starts_with('D') { None } else { blobs.get(&p).cloned() };
        (p, blob)
    }).collect()
}

fn content_check(found: &str, meant: &str) -> Check {
    Check::new("content", Some(found == meant), if found == meant { short(found).to_string() } else { format!("{} ≠ {}", short(found), short(meant)) })
}

/// A commit read back from git: `git show --name-status` lists exactly
/// `files` (deletions too), and its tree (`git ls-tree`) has those not
/// deleted and not those deleted.
pub fn read_back_commit(dir: &str, sha: &str, files: &[String]) -> Vec<Check> {
    let changed = match name_status(dir, sha) {
        Ok(c) => c,
        Err(err) => return vec![Check::new("files", None, err)],
    };
    let listed: BTreeSet<&str> = changed.iter().map(|(_, p)| p.as_str()).collect();
    let meant: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    let missing: Vec<&str> = meant.difference(&listed).copied().collect();
    let extra: Vec<&str> = listed.difference(&meant).copied().collect();
    let files_check = if missing.is_empty() && extra.is_empty() {
        Check::new("files", Some(true), listed.len().to_string())
    } else {
        Check::new("files", Some(false), i18n::pick(format!("not in it: {}; not meant: {}", missing.join(", "), extra.join(", ")),
            format!("缺少：{}；多出：{}", missing.join("、"), extra.join("、"))))
    };
    let paths: Vec<String> = changed.iter().map(|(_, p)| p.clone()).collect();
    let tree = tree_blobs(dir, sha, &paths);
    let wrong: Vec<&str> = changed.iter().filter(|(st, p)| st.starts_with('D') == tree.contains_key(p)).map(|(_, p)| p.as_str()).collect();
    let tree_check = if wrong.is_empty() {
        Check::new("tree", Some(true), short(sha).to_string())
    } else {
        Check::new("tree", Some(false), i18n::pick(format!("its tree has it otherwise: {}", wrong.join(", ")), format!("文件树里不符：{}", wrong.join("、"))))
    };
    vec![files_check, tree_check]
}

/// A commit receipt cut off: the commit made since its HEAD that holds its
/// changes, read back; none: not made.
fn recover_commit(r: &Receipt) -> (Vec<Check>, Option<String>) {
    let range = r.snap.head.as_deref().map(|h| format!("{h}..HEAD")).unwrap_or_else(|| "HEAD".into());
    let shas = workspace::git(&r.project, &["rev-list", "--reverse", "--max-count=50", &range]).unwrap_or_default();
    for sha in shas.lines().filter(|l| !l.is_empty()) {
        let made = in_commit(&r.project, sha);
        if commit_inputs(&made) == r.inputs {
            let paths: Vec<String> = made.iter().map(|(p, _)| p.clone()).collect();
            let mut checks = read_back_commit(&r.project, sha, &paths);
            checks.push(content_check(&r.inputs, &r.inputs));
            return (checks, workspace::git(&r.project, &["log", "-1", "--format=%h %s", sha]).ok());
        }
    }
    (vec![Check::new("commit", Some(false), i18n::t("no commit since holds these files", "之后没有包含这些文件的提交"))], None)
}

fn version_key(v: &str) -> Vec<u64> {
    v.split(['.', '-']).map(|p| p.parse().unwrap_or(0)).collect()
}

/// A local App Hub (`hub`: its folder) read back for a publish of `id`:
/// the catalog's entry for `version` (none: its newest entry holding
/// `content`), its digest against the staged bundle's (`staged`; else the
/// published copy's own), what the published copy holds against
/// `content`, its commit against `head`. The version found, and the checks.
pub fn read_back_publish(hub: &Path, id: &str, version: Option<&str>, content: &str, staged: Option<&str>, head: Option<&str>) -> (Option<String>, Vec<Check>) {
    let catalog = std::fs::read_to_string(hub.join("catalog.json")).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok());
    let Some(catalog) = catalog else {
        return (None, vec![Check::new("catalog", None, i18n::t("its catalog.json could not be read", "读不了 catalog.json"))]);
    };
    let entries: Vec<&Value> = catalog["entries"].as_array().into_iter().flatten().filter(|e| e["manifest"]["id"].as_str() == Some(id)).collect();
    let copy = |e: &Value| e["artifact"].as_str().map(|a| hub.join(a));
    let held = |e: &Value| copy(e).filter(|d| d.join("manifest.json").is_file()).map(|d| crate::plugins::app_publish::content_in(&d));
    let entry = match version {
        Some(v) => entries.iter().find(|e| e["manifest"]["version"].as_str() == Some(v)).copied(),
        None => entries.iter().filter(|e| held(e).as_deref() == Some(content))
            .max_by_key(|e| version_key(e["manifest"]["version"].as_str().unwrap_or(""))).copied(),
    };
    let Some(e) = entry else {
        let what = version.map(|v| format!("{id} {v}")).unwrap_or_else(|| format!("{id} ({})", short(content)));
        return (None, vec![Check::new("catalog", Some(false), i18n::pick(format!("no entry for {what}"), format!("没有 {what} 的条目")))]);
    };
    let found = e["manifest"]["version"].as_str().unwrap_or("").to_string();
    let mut checks = vec![Check::new("catalog", Some(true), format!("{id} {found}"))];
    let listed = e["manifest"]["integrity"]["bundle_blake3"].as_str().unwrap_or("");
    let own = copy(e).and_then(|d| std::fs::read_to_string(d.join("manifest.json")).ok())
        .and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|m| m["integrity"]["bundle_blake3"].as_str().map(String::from));
    checks.push(match staged.map(String::from).or(own) {
        Some(d) if !listed.is_empty() => Check::new("digest", Some(d == listed), if d == listed { short(listed).to_string() } else { format!("{} ≠ {}", short(listed), short(&d)) }),
        _ => Check::new("digest", None, i18n::t("no digest to compare", "没有可比的摘要")),
    });
    checks.push(match held(e) {
        Some(h) => content_check(&h, content),
        None => Check::new("content", Some(false), i18n::t("its published copy is missing", "发布的副本不见了")),
    });
    if let (Some(c), Some(h)) = (e["source"]["commit"].as_str().filter(|c| !c.is_empty()), head) {
        checks.push(Check::new("commit", Some(c == h), if c == h { short(c).to_string() } else { format!("{} ≠ {}", short(c), short(h)) }));
    }
    (Some(found), checks)
}

/// What `origin` has for `tag` (its commit), read with `git ls-remote`:
/// `Ok(None)`: not there.
pub fn remote_tag(dir: &str, tag: &str) -> Result<Option<String>, String> {
    let out = workspace::git(dir, &["ls-remote", "origin", &format!("refs/tags/{tag}"), &format!("refs/tags/{tag}^{{}}")])?;
    let mut plain = None;
    for l in out.lines() {
        let Some((sha, name)) = l.split_once('\t') else { continue };
        if name.ends_with("^{}") {
            return Ok(Some(sha.to_string()));
        }
        plain = Some(sha.to_string());
    }
    Ok(plain)
}

/// A tag read back from the remote: `git ls-remote origin` has it on the
/// commit `git rev-list -n1 <tag>` has here.
pub fn read_back_tag(dir: &str, tag: &str) -> Check {
    let Ok(here) = workspace::git(dir, &["rev-list", "-n", "1", tag]) else {
        return Check::new("tag", Some(false), i18n::pick(format!("no tag {tag} here"), format!("本地没有标签 {tag}")));
    };
    match remote_tag(dir, tag) {
        Ok(Some(there)) => Check::new("tag", Some(there == here), if there == here { format!("{tag} {}", short(&here)) } else { format!("{tag}: {} ≠ {}", short(&there), short(&here)) }),
        Ok(None) => Check::new("tag", Some(false), i18n::pick(format!("{tag} is not on the remote"), format!("远端没有 {tag}"))),
        Err(err) => Check::new("tag", None, err),
    }
}

/// The issues of `repo` titled `title` (their links): GitHub's list first
/// (it has a new one at once), then its search (older ones).
pub fn find_issue(repo: &str, title: &str) -> Result<Vec<String>, String> {
    let gh = |args: &[&str]| -> Result<Vec<String>, String> {
        let out = Command::new("gh").args(args).env("PATH", workspace::search_path()).output().map_err(|e| format!("could not run gh: {e}"))?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("gh issue list: {e}"))?;
        Ok(v.as_array().into_iter().flatten().filter(|i| i["title"].as_str() == Some(title))
            .map(|i| i["url"].as_str().map(String::from).unwrap_or_else(|| format!("#{}", i["number"]))).collect())
    };
    let recent = gh(&["issue", "list", "-R", repo, "--state", "all", "--limit", "100", "--json", "number,title,url"])?;
    if !recent.is_empty() {
        return Ok(recent);
    }
    gh(&["issue", "list", "-R", repo, "--search", &format!("\"{title}\" in:title"), "--state", "all", "--json", "number,title,url"])
}

/// An issue read back from GitHub: `repo` has one titled `title`.
pub fn read_back_issue(repo: &str, title: &str) -> Check {
    match find_issue(repo, title) {
        Ok(found) if !found.is_empty() => Check::new("issue", Some(true), found[0].clone()),
        Ok(_) => Check::new("issue", Some(false), i18n::pick(format!("no issue “{title}” in {repo}"), format!("{repo} 里没有「{title}」"))),
        Err(err) => Check::new("issue", None, err),
    }
}

/// What the person reads for a check's key.
fn what_word(what: &str) -> String {
    match what {
        "catalog" => i18n::t("catalog entry", "目录条目"),
        "digest" => i18n::t("digest", "摘要"),
        "content" => i18n::t("content", "内容"),
        "commit" => i18n::t("commit", "提交"),
        "files" => i18n::t("files", "文件"),
        "tree" => i18n::t("tree", "文件树"),
        "tag" => i18n::t("tag", "标签"),
        "issue" => "issue",
        "script" => i18n::t("its script", "脚本"),
        other => return other.to_string(),
    }.to_string()
}

/// One line for the person: what was done and how it read back (`✓`
/// confirmed, `✗` it differs or was not done, `?` not confirmed).
pub fn line(r: &Receipt) -> String {
    let result = r.result.clone().unwrap_or_default();
    let version = r.inputs.split_once(' ').map(|(_, v)| v.to_string()).unwrap_or_default();
    // A sheet's inputs: `<session> r<round> <digest>`.
    let round = r.inputs.split_whitespace().nth(1).and_then(|w| w.strip_prefix('r')).unwrap_or("?").to_string();
    let sheet = short(&r.snap.content).to_string();
    let did = match r.kind {
        Kind::Commit => i18n::pick(format!("committed {}", result.split_whitespace().next().unwrap_or("")), format!("已提交 {}", result.split_whitespace().next().unwrap_or(""))),
        Kind::Publish => i18n::pick(format!("published {result}"), format!("已发布 {result}")),
        Kind::PublishFix => i18n::pick(format!("published the fix {result}"), format!("已发布修复版 {result}")),
        Kind::Drill => i18n::pick(format!("published the drill's {result}"), format!("已发布演练版本 {result}")),
        Kind::HubSubmit => i18n::pick(format!("submitted {version} to App Hub"), format!("已提交 {version} 到 App Hub")),
        Kind::Acceptance => i18n::pick(format!("round {round} accepted by the person (sheet {sheet})"), format!("第 {round} 轮已由本人验收（验收单 {sheet}）")),
    };
    let noun = match r.kind {
        Kind::Commit => i18n::t("the commit", "提交").to_string(),
        Kind::Publish => i18n::t("the publish", "发布").to_string(),
        Kind::PublishFix => i18n::t("the fix's publish", "修复版发布").to_string(),
        Kind::Drill => i18n::t("the drill", "演练").to_string(),
        Kind::HubSubmit => i18n::t("the submission", "提交 App Hub").to_string(),
        Kind::Acceptance => i18n::pick(format!("round {round}'s acceptance"), format!("第 {round} 轮的验收")),
    };
    let named = |ok: Option<bool>, with_detail: bool| -> String {
        r.checks.iter().filter(|c| c.ok == ok).map(|c| if with_detail && !c.detail.is_empty() { format!("{} ({})", what_word(&c.what), c.detail) } else { what_word(&c.what) })
            .collect::<Vec<_>>().join(i18n::t(", ", "、"))
    };
    match r.status {
        // A sheet nobody has confirmed yet: nothing was done, the person is to look.
        Status::Prepared if r.kind == Kind::Acceptance => i18n::pick(format!("? round {round}'s acceptance sheet ({sheet}) waits for the person"), format!("? 第 {round} 轮的验收单（{sheet}）等待本人确认")),
        Status::Confirmed => i18n::pick(format!("✓ {did} — read back: {}", named(Some(true), false)), format!("✓ {did}——已回读核对：{}", named(Some(true), false))),
        Status::Mismatch => i18n::pick(format!("✗ {did} — read back differs: {}", named(Some(false), true)), format!("✗ {did}——回读不符：{}", named(Some(false), true))),
        Status::Unknown | Status::Executing | Status::Prepared if r.checks.is_empty() => i18n::pick(format!("? {did} — not read back yet"), format!("? {did}——还没有回读")),
        Status::Unknown | Status::Executing | Status::Prepared => i18n::pick(format!("? {did} — not confirmed: {}", named(None, true)), format!("? {did}——未能确认：{}", named(None, true))),
        Status::Refused => i18n::pick(format!("✗ {noun} not done: {result}"), format!("✗ {noun}没有进行：{result}")),
        Status::Stale => i18n::pick(format!("✗ {noun} not done, out of date: {result}"), format!("✗ {noun}没有进行，已过期：{result}")),
    }
}

/// An open receipt read back again (never done again) and settled.
pub fn reconcile(r: &mut Receipt) {
    match r.kind {
        Kind::Commit => {
            let (checks, made) = recover_commit(r);
            settle(r, checks, made);
        }
        Kind::Publish | Kind::PublishFix | Kind::Drill => {
            let id = crate::plugins::app_publish::app_id(&r.project).unwrap_or_default();
            // The version it meant (written down as it ran), else the newest holding its content.
            let meant = r.result.clone().filter(|v| !v.is_empty());
            let (found, checks) = read_back_publish(&crate::plugins::app_publish::hub_dir(), &id, meant.as_deref(), &r.inputs, None, None);
            settle(r, checks, found);
        }
        Kind::HubSubmit => {
            let (id, version) = r.inputs.split_once(' ').unwrap_or((r.inputs.as_str(), ""));
            let checks = vec![read_back_tag(&r.project, &format!("v{version}")), read_back_issue(&crate::plugins::app_hub::hub_repo(), &format!("Submit {id} {version}"))];
            let issue = checks.iter().find(|c| c.what == "issue" && c.ok == Some(true)).map(|c| c.detail.clone());
            settle(r, checks, issue);
        }
        // Never left open: the person's word, nothing done to read back.
        Kind::Acceptance => {}
    }
}

/// Receipts a previous run left open, each read back (blocks): `(project,
/// what was found)`.
pub fn recover() -> Vec<(String, String)> {
    open().into_iter().map(|mut r| {
        reconcile(&mut r);
        (r.project.clone(), line(&r))
    }).collect()
}

impl crate::OctoBuddyView {
    /// Receipts left mid-way when OctoBuddy last closed, read back off the
    /// UI thread (never done again): what was found comes as
    /// `ReceiptsReconciled`.
    pub(crate) fn receipts_recover(&mut self) {
        let (inbox, lang) = (self.rt.inbox.clone(), i18n::lang());
        std::thread::spawn(move || {
            i18n::set(lang);
            let lines = recover();
            if !lines.is_empty() {
                crate::events::post(&inbox, crate::events::LoopEvent::ReceiptsReconciled { lines });
            }
        });
    }

    /// What those read-backs found, said in each project's latest session.
    pub(crate) fn receipts_reconciled(&mut self, lines: Vec<(String, String)>) {
        for (project, line) in lines {
            // A commit's is its directory: the project it is in.
            let pi = self.store.projects.iter().position(|p| p.path == project)
                .or_else(|| self.store.projects.iter().position(|p| !p.path.is_empty() && project.starts_with(&format!("{}/", p.path))));
            let Some(at) = pi.and_then(|pi| self.store.projects[pi].sessions.len().checked_sub(1).map(|si| (pi, si))) else { continue };
            self.system(at, &i18n::pick(format!("Left unsettled when OctoBuddy last closed, now read back (not done again): {line}"),
                format!("上次关闭时没有确认完的操作，已重新回读（没有再执行一次）：{line}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("octobuddy-receipts-{name}-{}-{}", std::process::id(), new_id("t")));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn snap(content: &str) -> Snapshot {
        Snapshot { head: Some("abc".into()), content: content.into(), version: Some("1.0.0".into()) }
    }

    fn check(ok: Option<bool>) -> Check {
        Check::new("catalog", ok, "")
    }

    #[test]
    fn the_ledger_folds_each_receipt_to_its_last_line() {
        let dir = scratch("fold");
        let l = Ledger(dir.join("receipts.jsonl"));
        let mut a = l.prepare(Kind::Publish, "/p", "c1", snap("c1"), "person").unwrap();
        let b = l.prepare(Kind::Commit, "/p", "c2", snap("c2"), "outer loop").unwrap();
        l.executing(&mut a);
        l.settle(&mut a, vec![check(Some(true))], Some("1.0.3".into()));
        let text = std::fs::read_to_string(&l.0).unwrap();
        assert_eq!(text.lines().count(), 4, "a line per change");
        let all = l.read();
        assert_eq!(all.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), [a.id.as_str(), b.id.as_str()], "in the order they were made");
        assert_eq!(all[0], a, "as its last line has it");
        assert_eq!((all[0].status, all[0].result.as_deref()), (Status::Confirmed, Some("1.0.3")));
        assert_eq!(all[1].status, Status::Prepared);
        // Read again: a line appended is seen (not the cache's).
        l.refuse(&mut b.clone(), "no");
        assert_eq!(l.read()[1].status, Status::Refused);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_same_asked_again_is_done_or_open() {
        let dir = scratch("prior");
        let l = Ledger(dir.join("receipts.jsonl"));
        let mut r = l.prepare(Kind::Publish, "/p", "x", snap("x"), "person").unwrap();
        l.executing(&mut r);
        // Executing: open, for the same content only, in the same project only.
        assert!(matches!(l.prepare(Kind::Publish, "/p", "x", snap("x"), "person"), Err(Prior::Open(o)) if o.id == r.id));
        assert!(matches!(l.prepare(Kind::PublishFix, "/p", "x", snap("x"), "person"), Err(Prior::Open(_))), "a fix publishes to the same catalog");
        assert!(l.prepare(Kind::Publish, "/q", "x", snap("x"), "person").is_ok());
        l.settle(&mut r, vec![check(None)], None);
        assert!(matches!(l.prepare(Kind::Publish, "/p", "x", snap("x"), "person"), Err(Prior::Open(_))), "unknown is open still");
        l.settle(&mut r, vec![check(Some(true))], Some("1.0.0".into()));
        assert!(matches!(l.prepare(Kind::Publish, "/p", "x", snap("x"), "person"), Err(Prior::Done(d)) if d.id == r.id));
        // Another content published since: x is not the catalog's latest any more.
        let mut y = l.prepare(Kind::PublishFix, "/p", "y", snap("y"), "person").unwrap();
        l.executing(&mut y);
        l.settle(&mut y, vec![check(Some(true))], Some("1.0.1".into()));
        assert!(l.prepare(Kind::Publish, "/p", "x", snap("x"), "person").is_ok(), "back to x: published again");
        // A drill is never done before: it breaks on purpose.
        assert!(l.prepare(Kind::Drill, "/p", "y", snap("y"), "drill").is_ok());
        // A mismatch is settled: done again.
        let mut z = l.prepare(Kind::HubSubmit, "/p", "app 1.0.0", snap("z"), "person").unwrap();
        l.settle(&mut z, vec![check(Some(true)), check(Some(false))], None);
        assert_eq!(z.status, Status::Mismatch);
        assert!(l.prepare(Kind::HubSubmit, "/p", "app 1.0.0", snap("z"), "person").is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn open_lists_only_executing_and_unknown() {
        let all: Vec<Receipt> = [Status::Prepared, Status::Executing, Status::Confirmed, Status::Mismatch, Status::Unknown, Status::Refused, Status::Stale]
            .into_iter().map(|status| Receipt { status, ..draft(Kind::Publish, "/p", "x", Snapshot::default(), "person") }).collect();
        let open: Vec<Status> = all.iter().filter(|r| r.status.open()).map(|r| r.status).collect();
        assert_eq!(open, [Status::Executing, Status::Unknown]);
        let dir = scratch("open");
        let l = Ledger(dir.join("receipts.jsonl"));
        for r in &all {
            l.record(r);
        }
        assert_eq!(l.read().into_iter().filter(|r| r.status.open()).count(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_receipt_settles_by_its_checks() {
        assert_eq!(status_of(&[check(Some(true)), check(Some(true))]), Status::Confirmed);
        assert_eq!(status_of(&[check(Some(true)), check(None)]), Status::Unknown);
        assert_eq!(status_of(&[check(None), check(Some(false))]), Status::Mismatch, "a no outweighs a don't know");
        assert_eq!(status_of(&[]), Status::Unknown, "nothing read back confirms nothing");
    }

    #[test]
    fn a_moved_snapshot_leaves_it_stale() {
        let dir = scratch("still");
        let l = Ledger(dir.join("receipts.jsonl"));
        let r = draft(Kind::Publish, "/p", "x", snap("x"), "person");
        assert!(l.still(&r, &snap("x")).is_ok());
        assert!(l.read().is_empty(), "nothing written when it is still so");
        let now = Snapshot { head: Some("def".into()), ..snap("y") };
        let moved = l.still(&r, &now).unwrap_err();
        assert!(moved.contains("abc → def") && moved.contains("content"), "{moved}");
        assert_eq!(l.read()[0].status, Status::Stale);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_line_says_how_it_read_back() {
        i18n::set(i18n::Lang::En);
        let mut r = draft(Kind::Publish, "/p", "x", snap("x"), "person");
        r.result = Some("1.0.3".into());
        r.checks = vec![Check::new("catalog", Some(true), "a 1.0.3"), Check::new("digest", Some(true), ""), Check::new("commit", Some(true), "")];
        r.status = status_of(&r.checks);
        assert_eq!(line(&r), "✓ published 1.0.3 — read back: catalog entry, digest, commit");
        r.checks[1] = Check::new("digest", Some(false), "1a ≠ 2b");
        r.status = status_of(&r.checks);
        assert_eq!(line(&r), "✗ published 1.0.3 — read back differs: digest (1a ≠ 2b)");
        r.checks[1] = Check::new("digest", None, "no digest");
        r.status = status_of(&r.checks);
        assert!(line(&r).starts_with("? published 1.0.3 — not confirmed: digest"));
    }

    #[test]
    fn a_sheets_line_says_it_waits_then_who_accepted_it() {
        i18n::set(i18n::Lang::En);
        let mut r = draft(Kind::Acceptance, "/p", "s-1 r2 d1e2f3a4", Snapshot { head: Some("abc".into()), content: "d1e2f3a4".into(), version: None }, "OctoBuddy");
        assert_eq!(line(&r), "? round 2's acceptance sheet (d1e2f3) waits for the person");
        r.by = "person".into();
        r.checks = vec![Check::new("commit", Some(true), "abc"), Check::new("content", Some(true), "d1e2f3")];
        r.status = status_of(&r.checks);
        assert_eq!(line(&r), "✓ round 2 accepted by the person (sheet d1e2f3) — read back: commit, content");
        r.status = Status::Stale;
        r.result = Some("its commit moved (abc → def)".into());
        assert_eq!(line(&r), "✗ round 2's acceptance not done, out of date: its commit moved (abc → def)");
        i18n::set(i18n::Lang::Zh);
        assert!(line(&r).starts_with("✗ 第 2 轮的验收没有进行，已过期"), "{}", line(&r));
        i18n::set(i18n::Lang::En);
        assert_eq!(Kind::parse("acceptance"), Some(Kind::Acceptance));
        let dir = scratch("sheet");
        let l = Ledger(dir.join("receipts.jsonl"));
        let mut a = l.prepare(Kind::Acceptance, "/p", "s-1 r2 d", snap("d"), "OctoBuddy").unwrap();
        assert!(l.prepare(Kind::Acceptance, "/p", "s-1 r3 e", snap("e"), "OctoBuddy").is_ok(), "another round's sheet is another");
        l.settle(&mut a, vec![check(Some(true))], None);
        assert!(matches!(l.prepare(Kind::Acceptance, "/p", "s-1 r2 d", snap("d"), "OctoBuddy"), Err(Prior::Done(_))), "the same sheet, confirmed, is done");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn repo(dir: &Path) -> String {
        let p = dir.to_string_lossy().into_owned();
        for args in [&["init", "-q", "-b", "main"][..], &["config", "user.email", "t@example.invalid"], &["config", "user.name", "t"], &["config", "commit.gpgsign", "false"]] {
            workspace::git(&p, args).unwrap();
        }
        p
    }

    #[test]
    fn a_commit_is_read_back_deletions_too() {
        let dir = scratch("commit");
        let p = repo(&dir);
        std::fs::write(dir.join("a.txt"), "a").unwrap();
        std::fs::write(dir.join("gone.txt"), "x").unwrap();
        std::fs::write(dir.join("same.txt"), "s").unwrap();
        workspace::git(&p, &["add", "-A"]).unwrap();
        workspace::git(&p, &["commit", "-qm", "init"]).unwrap();
        std::fs::write(dir.join("a.txt"), "a2").unwrap();
        std::fs::write(dir.join("b.txt"), "b").unwrap();
        std::fs::remove_file(dir.join("gone.txt")).unwrap();
        std::fs::write(dir.join("other.txt"), "not mine").unwrap();
        let files: Vec<String> = ["a.txt", "b.txt", "gone.txt", "same.txt"].iter().map(|s| s.to_string()).collect();
        let l = Ledger(dir.join("ledger/receipts.jsonl"));
        let made = commit_in(&l, &p, &files, "feat: a b", "outer loop").unwrap();
        assert!(made.ends_with("feat: a b"), "{made}");
        let r = l.read().pop().unwrap();
        assert_eq!(r.status, Status::Confirmed, "{:?}", r.checks);
        assert_eq!(r.checks.iter().map(|c| c.what.as_str()).collect::<Vec<_>>(), ["files", "tree", "content"]);
        let sha = made.split_whitespace().next().unwrap();
        // Read back against other files: what it lists is not that.
        let wrong = read_back_commit(&p, sha, &["a.txt".to_string(), "other.txt".to_string()]);
        assert_eq!(wrong[0].ok, Some(false));
        assert!(wrong[0].detail.contains("other.txt") && wrong[0].detail.contains("gone.txt"), "{}", wrong[0].detail);
        assert_eq!(wrong[1].ok, Some(true), "its tree is as it lists");
        // Cut off after git committed: read back, found, not committed again.
        std::fs::write(dir.join("c.txt"), "c").unwrap();
        let changes = changes(&p, &["c.txt".to_string()]);
        let head = workspace::git(&p, &["rev-parse", "HEAD"]).ok();
        let mut cut = l.prepare(Kind::Commit, &p, &commit_inputs(&changes), Snapshot { head, ..Default::default() }, "outer loop").unwrap();
        l.executing(&mut cut);
        workspace::commit_files(&p, &["c.txt".to_string()], "feat: c").unwrap();
        let (checks, found) = recover_commit(&cut);
        assert_eq!(status_of(&checks), Status::Confirmed, "{checks:?}");
        assert!(found.unwrap().ends_with("feat: c"));
        // Nothing of it in a commit: a mismatch.
        cut.inputs = "other".into();
        assert_eq!(status_of(&recover_commit(&cut).0), Status::Mismatch);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_publish_is_read_back_from_the_catalog() {
        let hub = scratch("hub");
        let copy = hub.join("artifacts/app-1.0.3.bundle");
        std::fs::create_dir_all(&copy).unwrap();
        // App Hub writes the defaults it reads in, and stamps it: the content is the same.
        std::fs::write(copy.join("manifest.json"), r#"{"id": "app", "version": "1.0.3", "agent": null, "network": {"hosts": []}, "integrity": {"bundle_blake3": "b3"}, "name": "A"}"#).unwrap();
        std::fs::write(copy.join("main.splash"), "View{}").unwrap();
        std::fs::write(copy.join("listing.json"), r#"{"subtitle": "s"}"#).unwrap();
        let project = scratch("project");
        std::fs::create_dir_all(project.join("bundle")).unwrap();
        std::fs::write(project.join("bundle/manifest.json"), "{\n  \"name\": \"A\",\n  \"id\": \"app\",\n  \"version\": \"1.0.2\"\n}\n").unwrap();
        std::fs::write(project.join("bundle/main.splash"), "View{}").unwrap();
        std::fs::write(project.join("bundle/listing.json"), "{\"subtitle\":\"s\"}").unwrap();
        let content = crate::plugins::app_publish::content(&project.to_string_lossy());
        assert_eq!(content, crate::plugins::app_publish::content_in(&copy), "its version, digest and App Hub's defaults aside");
        let entry = |digest: &str, commit: &str| format!(r#"{{"entries": [{{"artifact": "artifacts/app-1.0.3.bundle", "manifest": {{"id": "app", "version": "1.0.3", "integrity": {{"bundle_blake3": "{digest}"}}}}, "source": {{"commit": "{commit}"}}}}]}}"#);
        std::fs::write(hub.join("catalog.json"), entry("b3", "c0ffee")).unwrap();
        let (found, checks) = read_back_publish(&hub, "app", Some("1.0.3"), &content, Some("b3"), Some("c0ffee"));
        assert_eq!(found.as_deref(), Some("1.0.3"));
        assert_eq!(checks.iter().map(|c| (c.what.as_str(), c.ok)).collect::<Vec<_>>(),
            [("catalog", Some(true)), ("digest", Some(true)), ("content", Some(true)), ("commit", Some(true))]);
        // Its version not known (cut off): the newest entry holding the content.
        assert_eq!(read_back_publish(&hub, "app", None, &content, None, None).0.as_deref(), Some("1.0.3"));
        assert_eq!(status_of(&read_back_publish(&hub, "app", None, "other", None, None).1), Status::Mismatch);
        // Another digest, another commit, another version: each says so.
        std::fs::write(hub.join("catalog.json"), entry("b4", "beef")).unwrap();
        let (_, checks) = read_back_publish(&hub, "app", Some("1.0.3"), &content, Some("b3"), Some("c0ffee"));
        assert_eq!(checks.iter().filter(|c| c.ok == Some(false)).map(|c| c.what.as_str()).collect::<Vec<_>>(), ["digest", "commit"]);
        assert_eq!(status_of(&read_back_publish(&hub, "app", Some("1.0.4"), &content, None, None).1), Status::Mismatch);
        std::fs::remove_file(hub.join("catalog.json")).unwrap();
        assert_eq!(status_of(&read_back_publish(&hub, "app", Some("1.0.3"), &content, None, None).1), Status::Unknown, "no catalog: not known");
        let _ = std::fs::remove_dir_all(hub);
        let _ = std::fs::remove_dir_all(project);
    }

    /// This machine's local App Hub, read back for an app it published from
    /// `OCTOBUDDY_LIVE_APP` (else `../octobuddy-apps/pocket-ledger`): its
    /// HEAD, as committed, confirmed on every check. Its working copy is
    /// printed too: a change there since is told apart.
    #[test]
    #[ignore]
    fn reads_back_a_published_app_from_the_local_hub() {
        let project = std::env::var("OCTOBUDDY_LIVE_APP").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../octobuddy-apps/pocket-ledger").to_string());
        let id = crate::plugins::app_publish::app_id(&project).expect("an app project");
        let head = workspace::git(&project, &["rev-parse", "HEAD"]).ok();
        let hub = crate::plugins::app_publish::hub_dir();
        // Its HEAD as committed, out of the working copy's way.
        let committed = std::env::temp_dir().join(format!("octobuddy-live-readback-{}", std::process::id()));
        std::fs::create_dir_all(&committed).unwrap();
        let out = std::process::Command::new("sh").arg("-c").arg(format!("git -C '{project}' archive HEAD | tar -x -C '{}'", committed.display())).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let content = crate::plugins::app_publish::content(&committed.display().to_string());
        let (found, checks) = read_back_publish(&hub, &id, None, &content, None, head.as_deref());
        for c in &checks {
            println!("HEAD {:?}: {}: {:?} {}", head, c.what, c.ok, c.detail);
        }
        let worked = crate::plugins::app_publish::content(&project);
        let (now, _) = read_back_publish(&hub, &id, None, &worked, None, head.as_deref());
        println!("working copy ({}): {}", short(&worked), now.map(|v| format!("published as {v}")).unwrap_or_else(|| "not what is published".into()));
        let _ = std::fs::remove_dir_all(&committed);
        assert!(found.is_some() && checks.iter().all(|c| c.ok == Some(true)), "{checks:?}");
    }
}
