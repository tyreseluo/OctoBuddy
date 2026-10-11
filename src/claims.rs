//! What an inner loop says it did, against what OctoBuddy finds it did.
//!
//! A report's `verified:` and `notes:` are claims: the commands it says it
//! ran and how they ended, the files it says it changed or deleted. Each is
//! held against OctoBuddy's own run of the slice's check, the commit it made
//! and the files as they are now: it holds, the facts contradict it, or
//! nothing says (its own word). A false "contradicted" costs more than a
//! "?", so a claim is contradicted only by a fact that plainly says
//! otherwise. What contradicts it weighs on its outer loop's `accept`.
use crate::model::{Gate, Peer};
use std::path::Path;

/// How a claim stands against the facts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Holds,
    Contradicted,
    /// Nothing OctoBuddy ran or found says either way.
    Unverifiable,
}

impl Mark {
    fn sign(self) -> &'static str {
        match self {
            Mark::Holds => "✓",
            Mark::Contradicted => "✗",
            Mark::Unverifiable => "?",
        }
    }
}

/// One thing a report says (`said`), and what OctoBuddy found (`fact`).
#[derive(Clone, Debug, PartialEq)]
pub struct Claim {
    pub mark: Mark,
    pub said: String,
    pub fact: String,
}

/// A path a report names, looked up.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Probe {
    /// As the report wrote it.
    pub said: String,
    /// The project's path it is (a bare name: the one file of that name).
    pub path: String,
    /// It was there when the loop began.
    pub in_base: bool,
    /// How it differs from then (`git diff --name-status`); None: it does not.
    pub diff: Option<char>,
    /// It is there now.
    pub exists: bool,
}

/// What OctoBuddy found of a turn's work, to hold its report against.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    /// Its run of the slice's check: (command, passed, how it ended).
    pub check: Option<(String, bool, String)>,
    /// What the commit it made holds (`git show --name-status`): (`M`, `A`, `D`…, path).
    pub committed: Vec<(char, String)>,
    /// The paths the report names.
    pub probed: Vec<Probe>,
    /// The files its tools changed, over all its turns.
    pub touched: Vec<String>,
}

/// At most this many lines of claims go in a verdict.
const MAX_LINES: usize = 6;

/// What a commit holds: its files and how each changed (a rename by its new path).
pub fn committed(dir: &str, sha: &str) -> Vec<(char, String)> {
    let out = crate::workspace::git(dir, &["show", "--name-status", "--format=", sha]).unwrap_or_default();
    out.lines().filter_map(|l| {
        let mut parts = l.split('\t');
        let status = parts.next()?.chars().next()?;
        Some((status, parts.next_back()?.to_string()))
    }).collect()
}

/// The facts a report is held against: the check's run, the commit `sha`
/// made of its files, and each path it names, looked up in `dir` against
/// `base` (where the loop began).
pub fn facts(dir: &str, base: Option<&str>, report: &str, sha: Option<&str>, check: Option<(String, bool, String)>, touched: &[String]) -> Facts {
    let committed = sha.map(|sha| committed(dir, sha)).unwrap_or_default();
    let named = path_claims(report, check.as_ref().map(|c| c.0.as_str()));
    let mut probed: Vec<Probe> = Vec::new();
    if let Some(base) = base.filter(|_| !named.is_empty()) {
        let tree = crate::workspace::git(dir, &["ls-tree", "-r", "--name-only", base]).unwrap_or_default();
        let tree: Vec<&str> = tree.lines().collect();
        for (_, said) in named {
            if probed.iter().any(|p| p.said == said) {
                continue;
            }
            let Some(path) = resolve(&said, &tree) else { continue };
            // git could not say: nothing is claimed of it.
            let Ok(diff) = crate::workspace::git(dir, &["diff", "--name-status", base, "--", &path]) else { continue };
            let in_base = tree.iter().any(|f| *f == path || (path.ends_with('/') && f.starts_with(path.as_str())));
            let exists = Path::new(dir).join(&path).exists();
            probed.push(Probe { diff: diff.lines().next().and_then(|l| l.chars().next()), in_base, exists, said, path });
        }
    }
    Facts { check, committed, probed, touched: touched.to_vec() }
}

/// The project's path a report's path is: as written when it was there at
/// the start, else the one file whose path ends in it (`calc.py` naming
/// `src/calc.py`); None when several do (which one is not said).
fn resolve(said: &str, tree: &[&str]) -> Option<String> {
    let folder = said.ends_with('/');
    if folder || tree.contains(&said) {
        return Some(said.to_string());
    }
    let tail = format!("/{said}");
    let mut found = tree.iter().filter(|f| f.ends_with(&tail));
    match (found.next(), found.next()) {
        (Some(one), None) => Some(one.to_string()),
        (Some(_), Some(_)) => None,
        (None, _) => Some(said.to_string()),
    }
}

/// Each claim of `report`, marked against `f`. Pure.
pub fn reconcile(report: &str, f: &Facts) -> Vec<Claim> {
    let host = f.check.as_ref();
    let mut out = Vec::new();
    let mut general = false;
    for clause in clauses(&field(report, "verified")) {
        let toks = tokens(&clause);
        let cmds = commands(&toks, host.map(|h| h.0.as_str()));
        let text = plain(&toks);
        let said = polarity(&text);
        for cmd in &cmds {
            out.push(of_command(&clause, cmd, said, host));
        }
        // "tests pass": of the check, as nothing else is named (said once).
        if cmds.is_empty() && !general && matches!(said, Said::Pass | Said::Fail) && generic(&text) {
            general = true;
            out.push(of_tests(&clause, said, host));
        }
    }
    for (gone, said) in path_claims(report, host.map(|h| h.0.as_str())) {
        out.extend(of_path(gone, &said, f));
    }
    out
}

/// The verdict's lines for what was found: what the facts contradict
/// first, then what nothing says, then what holds; and the files its commit
/// deletes. At most `MAX_LINES`.
pub fn lines(found: &[Claim], f: &Facts) -> Vec<String> {
    let gone: Vec<&str> = f.committed.iter().filter(|(s, _)| *s == 'D').map(|(_, p)| p.as_str()).collect();
    let room = if gone.is_empty() { MAX_LINES } else { MAX_LINES - 1 };
    let mut sorted: Vec<&Claim> = found.iter().collect();
    sorted.sort_by_key(|c| match c.mark { Mark::Contradicted => 0, Mark::Unverifiable => 1, Mark::Holds => 2 });
    let shown = if sorted.len() > room { room - 1 } else { sorted.len() };
    let mut out: Vec<String> = sorted[..shown].iter().map(|c| format!("[claims] {} {} — {}", c.mark.sign(), c.said, c.fact)).collect();
    if sorted.len() > shown {
        out.push(format!("[claims] … and {} more", sorted.len() - shown));
    }
    if !gone.is_empty() {
        out.push(format!("[files] its commit deletes: {}", gone.join(", ")));
    }
    out
}

/// What a check found, for its outer loop's `accept`: `found` none when no
/// report was held against it, `stray` none when its card was not looked
/// at, `at` the commit its check ran on (empty: before its first).
pub fn gate(found: Option<&[Claim]>, ran: Option<&(String, bool, String)>, stray: Option<&[String]>, at: Option<&str>) -> Gate {
    Gate {
        check: ran.map(|r| r.1),
        cmd: ran.map(|r| r.0.clone()),
        status: ran.map(|r| r.2.clone()),
        at: ran.and(at).map(String::from),
        claims: found.map(|c| c.iter().filter(|c| c.mark == Mark::Contradicted).map(|c| format!("{} — {}", c.said, c.fact)).collect()),
        stray: stray.map(<[String]>::to_vec),
    }
}

/// A later check's findings over an earlier's: what it found, else what was.
pub fn merge(old: Option<Gate>, new: Gate) -> Gate {
    let old = old.unwrap_or_default();
    Gate {
        check: new.check.or(old.check),
        cmd: new.cmd.or(old.cmd),
        status: new.status.or(old.status),
        at: new.at.or(old.at),
        claims: new.claims.or(old.claims),
        stray: new.stray.or(old.stray),
    }
}

/// What goes back to an inner loop whose report the facts contradict.
pub fn bounce(wrong: &[String]) -> String {
    let list: Vec<String> = wrong.iter().map(|w| format!("- {w}")).collect();
    format!("OctoBuddy held your report against its own run and the files, and these claims do not hold:\n{}\n\n\
Fix the work if it is wrong; if the work is right, correct the report. Then end with your octobuddy-report block again, \
claiming only what OctoBuddy's run will confirm (say plainly what you did not run).", list.join("\n"))
}

/// Why OctoBuddy's findings say no to accepting `p`'s work as it is now
/// (none: they do not).
pub fn reasons(p: &Peer) -> Vec<String> {
    let Some(g) = p.gate.as_ref() else { return Vec::new() };
    let mut out = Vec::new();
    if g.check == Some(false) {
        out.push(format!("its check `{}` failed ({})", g.cmd.as_deref().unwrap_or("?"), g.status.as_deref().unwrap_or("?")));
    }
    // Committed again after its check: the check is of other work.
    let last = p.commits.as_ref().and_then(|c| c.last()).and_then(|c| c.split_whitespace().next());
    if let (Some(at), Some(last)) = (g.at.as_deref(), last) {
        if at != last {
            out.push(format!("it committed again ({last}) after its check ran, so the check does not cover its work as it is now"));
        }
    }
    if let Some(claims) = g.claims.as_ref().filter(|c| !c.is_empty()) {
        out.push(format!("its report claims what OctoBuddy found otherwise: {}", claims.join("; ")));
    }
    if let Some(stray) = g.stray.as_ref().filter(|s| !s.is_empty()) {
        out.push(format!("it changed files outside its card's Files: {}", stray.join(", ")));
    }
    out
}

/// Why an `accept` of `p` is refused, with the ways on; None: it is not.
pub fn refuse(p: &Peer) -> Option<String> {
    let why = reasons(p);
    (!why.is_empty()).then(|| format!("{}. Send it back with what to fix (verdict fix), close it, or accept it with \
\"override\": \"<why it is right anyway>\" (the person sees it).", why.join("; ")))
}

/// The review a verdict records on `p` (`accept: why`; an override's
/// `accept (override: …): why`), or why OctoBuddy refuses it: an `accept`
/// its findings say no to needs an override.
pub fn record(p: &Peer, r: &crate::plan::Review) -> Result<String, String> {
    let refused = if r.verdict == "accept" { refuse(p) } else { None };
    let verdict = match (refused, &r.over) {
        (Some(why), None) => return Err(why),
        (Some(_), Some(over)) => format!("accept (override: {over})"),
        (None, _) => r.verdict.clone(),
    };
    Ok(if r.why.is_empty() { verdict } else { format!("{verdict}: {}", r.why) })
}

/// A report's field (`verified`, `notes`): its text, up to the next field or
/// OctoBuddy's own lines (`[tests] …`).
fn field(report: &str, name: &str) -> String {
    let mut out = Vec::new();
    let mut inside = false;
    for line in report.lines() {
        let t = line.trim().trim_start_matches(['-', '*', ' ']);
        let low = t.to_ascii_lowercase();
        if let Some(key) = ["status:", "verified:", "decide:", "notes:"].into_iter().find(|k| low.starts_with(k)) {
            inside = key.trim_end_matches(':') == name;
            if inside {
                out.push(t[key.len()..].trim_start_matches(['*', ' ']).to_string());
            }
            continue;
        }
        if t.starts_with('[') || t.starts_with("```") {
            inside = false;
        }
        if inside {
            out.push(line.trim().to_string());
        }
    }
    out.join("\n")
}

/// A field's clauses: split at `;`, `；`, `。` and lines, never inside backticks.
fn clauses(text: &str) -> Vec<String> {
    let (mut out, mut cur, mut tick) = (Vec::new(), String::new(), false);
    for c in text.chars() {
        if c == '`' {
            tick = !tick;
        }
        if c == '\n' || (!tick && matches!(c, ';' | '；' | '。')) {
            tick = false;
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out.into_iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect()
}

/// A clause's words, a span in backticks kept whole.
#[derive(Debug, PartialEq)]
enum Tok {
    Word(String),
    Tick(String),
}

fn tokens(clause: &str) -> Vec<Tok> {
    let parts: Vec<&str> = clause.split('`').collect();
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        // An odd count of backticks: the last one opens nothing.
        if i % 2 == 1 && i + 1 < parts.len() + (parts.len() % 2) {
            if !part.trim().is_empty() {
                out.push(Tok::Tick(part.trim().to_string()));
            }
        } else {
            out.extend(part.split_whitespace().map(|w| Tok::Word(w.to_string())));
        }
    }
    out
}

/// A word without what is around it ("calc.py," → "calc.py"), lower case.
fn bare(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric() && !"./_-'".contains(c)).trim_end_matches('.').to_lowercase()
}

/// The clause's words outside backticks, without paths and flags: what says
/// how it went.
fn plain(toks: &[Tok]) -> String {
    toks.iter().filter_map(|t| match t {
        Tok::Word(w) if path(w).is_none() && !w.starts_with('-') => Some(w.as_str()),
        _ => None,
    }).collect::<Vec<_>>().join(" ")
}

/// How a clause says something ended.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Said {
    Pass,
    Fail,
    NotRun,
    Unclear,
}

/// Counts of nothing wrong ("0 failed"): no failure.
const ZERO: &[&str] = &["0 failed", "0 failures", "0 failure", "0 failing", "0 errors", "0 error", "0 panics", "failed: 0", "failures: 0", "errors: 0",
    "failed=0", "no failures", "no failure", "no errors", "no error", "without errors", "without error", "without failures",
    "0 失败", "0 个失败", "0 错误", "0 个错误", "无失败", "没有失败", "无错误", "没有错误", "零失败"];
const NOT_RUN: &[&str] = &["could not run", "couldn't run", "did not run", "didn't run", "not run", "skipped", "未运行", "没跑", "没有运行", "无法运行", "未执行", "没有执行"];
const FAILED: &[&str] = &["未通过", "没通过", "没有通过", "不通过", "未成功", "不成功", "没成功", "失败", "没过", "报错", "出错",
    "not pass", "n't pass", "not ok", "not green", "not clean", "not succeed", "❌", "✗", "✘"];
const PASSED: &[&str] = &["通过", "成功", "全绿", "跑通", "✅", "✔", "✓"];
/// Words that leave how it went open ("all pass except one", "大部分通过").
const HEDGES: &[&str] = &["except", "but", "most", "mostly", "some", "partly", "partially", "flaky", "apart", "besides"];
const HEDGES_ZH: &[&str] = &["除了", "但", "部分", "基本"];

fn polarity(text: &str) -> Said {
    let mut low = text.to_lowercase();
    for zero in ZERO {
        low = drop_phrase(&low, zero);
    }
    if take(&mut low, HEDGES_ZH) || low.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| HEDGES.contains(&w)) {
        return Said::Unclear;
    }
    let not_run = take(&mut low, NOT_RUN);
    let mut fail = take(&mut low, FAILED);
    let mut pass = take(&mut low, PASSED);
    let words: Vec<&str> = low.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).collect();
    for (i, w) in words.iter().enumerate() {
        // "no script errors", "did not fail": none.
        let none = words[i.saturating_sub(2)..i].iter().any(|p| matches!(*p, "no" | "zero" | "without" | "not" | "never" | "t"));
        match *w {
            "pass" | "passed" | "passes" | "passing" | "ok" | "okay" | "clean" | "green" => pass = true,
            _ if w.starts_with("succe") => pass = true,
            _ if (w.starts_with("fail") || w.starts_with("error") || w.starts_with("panic")) && !none => fail = true,
            _ => {}
        }
    }
    match (pass, fail, not_run) {
        (true, false, false) => Said::Pass,
        (false, true, false) => Said::Fail,
        (false, false, true) => Said::NotRun,
        _ => Said::Unclear,
    }
}

/// Whether `text` has any of `phrases`, which are then taken out of it.
fn take(text: &mut String, phrases: &[&str]) -> bool {
    let mut found = false;
    for p in phrases {
        if text.contains(p) {
            found = true;
            *text = text.replace(p, " ");
        }
    }
    found
}

/// `text` without `phrase`; one that starts with a digit only where it is
/// the whole number ("10 failed" keeps its "0 failed").
fn drop_phrase(text: &str, phrase: &str) -> String {
    let digit = phrase.starts_with(|c: char| c.is_ascii_digit());
    let (mut out, mut rest) = (String::new(), text);
    while let Some(i) = rest.find(phrase) {
        let number = rest[..i].ends_with(|c: char| c.is_ascii_digit()) || (i == 0 && out.ends_with(|c: char| c.is_ascii_digit()));
        out.push_str(&rest[..i]);
        out.push_str(if digit && number { phrase } else { " " });
        rest = &rest[i + phrase.len()..];
    }
    out.push_str(rest);
    out
}

/// Whether a clause speaks of the tests as a whole ("all tests pass",
/// "测试通过"); a build or a lint is not the tests.
fn generic(text: &str) -> bool {
    let low = text.to_lowercase();
    ["测试", "全部", "全绿", "用例"].iter().any(|w| low.contains(w))
        || low.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| matches!(w, "test" | "tests" | "suite" | "all" | "everything"))
}

/// Runners a report may name without backticks, the longest first.
/// `octobuddy_check` is the inner loop's tool that runs the slice's check.
const RUNNERS: &[&str] = &["python3 -m pytest", "python -m pytest", "octobuddy-app-check", "cargo clippy", "cargo check", "cargo build", "cargo test",
    "npm run test", "npm test", "pnpm test", "yarn test", "go test", "make test", "pytest", CHECK_TOOL];
const CHECK_TOOL: &str = "octobuddy_check";
/// What a command in backticks starts with.
const BINARIES: &[&str] = &["cargo", "pytest", "python", "python3", "npm", "npx", "pnpm", "yarn", "go", "make", "node", "deno", "bun", "sh", "bash",
    "octobuddy-app-check", "jest", "vitest", "ruff", "mypy", "tsc", "swift", "xcodebuild", "gradle", "./gradlew", "mvn", "dotnet", "rspec", "bundle",
    "mix", "flutter", "dart", "zig", "ctest", "cmake", "just", "grep", "cd", "env", "timeout"];

fn norm(cmd: &str) -> String {
    cmd.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How a command a report names stands to the one OctoBuddy ran.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Rel {
    Same,
    /// One holds the other (`cargo test` and `cargo test --locked`).
    Related,
    Other,
}

fn relate(said: &str, host: &str) -> Rel {
    let (s, h) = (norm(&said.to_lowercase()), norm(&host.to_lowercase()));
    // Its own run of the check, earlier, through OctoBuddy's tool.
    if s == CHECK_TOOL {
        return Rel::Related;
    }
    let within = |a: &str, b: &str| {
        let (a, b): (Vec<&str>, Vec<&str>) = (a.split(' ').collect(), b.split(' ').collect());
        b.len() <= a.len() && a.windows(b.len()).any(|w| w == b.as_slice())
    };
    if s == h {
        Rel::Same
    } else if within(&h, &s) || within(&s, &h) {
        Rel::Related
    } else {
        Rel::Other
    }
}

fn is_command(span: &str, host: Option<&str>) -> bool {
    if host.is_some_and(|h| relate(span, h) == Rel::Same) {
        return true;
    }
    // `RUST_LOG=1 cargo test`: the command after its variables.
    let first = span.split_whitespace().find(|w| !(w.contains('=') && !w.starts_with('-')));
    first.is_some_and(|w| BINARIES.contains(&w) || w.starts_with("./") || w == CHECK_TOOL)
}

/// The commands a clause names: in backticks, or a runner with its flags.
fn commands(toks: &[Tok], host: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |cmd: String| if !out.contains(&cmd) { out.push(cmd) };
    for t in toks {
        if let Tok::Tick(span) = t {
            if is_command(span, host) {
                add(norm(span));
            }
        }
    }
    let words: Vec<String> = toks.iter().filter_map(|t| match t {
        Tok::Word(w) => Some(w.trim_matches(|c: char| ",:;.()\"'".contains(c)).to_lowercase()),
        Tok::Tick(_) => None,
    }).collect();
    let mut used = vec![false; words.len()];
    for runner in RUNNERS {
        let parts: Vec<&str> = runner.split(' ').collect();
        let mut i = 0;
        while i + parts.len() <= words.len() {
            if used[i..i + parts.len()].contains(&true) || !words[i..i + parts.len()].iter().zip(&parts).all(|(w, p)| w == p) {
                i += 1;
                continue;
            }
            let mut j = i + parts.len();
            while j < words.len() && words[j].len() > 1 && words[j].starts_with('-') {
                j += 1;
            }
            used[i..j].fill(true);
            add(words[i..j].join(" "));
            i = j;
        }
    }
    out
}

/// A word that names a file or folder (`src/a.rs`, `README.md`, `tests/`):
/// it bare (no quotes, no `:12` after it), or None.
fn path(word: &str) -> Option<String> {
    let w = word.trim_matches(|c: char| "\"'()[]<>,;:!?*，。；：、（）“”‘’".contains(c)).trim_end_matches('.');
    // A line or a range after it: src/a.rs:12, src/a.rs:L10-L40.
    let w = match w.split_once(':') {
        Some((p, at)) if at.chars().all(|c| c.is_ascii_digit() || c == 'L' || c == '-') => p,
        _ => w,
    };
    let w = w.strip_prefix("./").unwrap_or(w);
    if w.is_empty() || w.len() > 160 || w.starts_with(['-', '/']) || w.contains("..") || w.contains("//")
        || !w.chars().all(|c| c.is_ascii_alphanumeric() || "._/-+@".contains(c))
        || ["e.g", "i.e", "etc", "vs"].contains(&w.to_ascii_lowercase().as_str()) {
        return None;
    }
    let name = w.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let extension = name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !ext.is_empty() && ext.len() <= 8 && ext.chars().any(|c| c.is_ascii_alphabetic())
            && (stem.is_empty() || stem.chars().any(|c| c.is_ascii_alphabetic()))
    });
    (w.contains('/') || extension).then(|| w.to_string())
}

const CHANGE: &[&str] = &["change", "changed", "changes", "add", "added", "adds", "update", "updated", "updates", "write", "wrote", "written", "writes",
    "edit", "edited", "edits", "modify", "modified", "modifies", "create", "created", "creates", "rewrite", "rewrote", "rewritten", "implement", "implemented"];
const CHANGE_ZH: &[&str] = &["修改", "新增", "更新", "改", "写", "创建", "添加", "增加", "实现"];
const DELETE: &[&str] = &["delete", "deleted", "deletes", "remove", "removed", "removes"];
const DELETE_ZH: &[&str] = &["删除", "删掉", "移除", "删了"];
const NEGATION: &[&str] = &["not", "no", "never", "without", "nothing", "none", "unchanged", "untouched"];
const NEGATION_ZH: &[&str] = &["没", "未", "不", "无"];
/// Words after which a path is what a change refers to, not what it changed
/// ("from utils.py", "tests for calc.py"): the paths after are not claimed.
const REFERENCE: &[&str] = &["from", "like", "see", "per", "using", "use", "uses", "via", "than", "following", "matching", "against", "as", "with",
    "for", "on", "about", "covering", "of", "so", "which", "that", "because", "since", "when", "if", "where", "while", "reads", "calls", "imports", "by",
    "参考", "参照", "按照", "根据"];
/// Words between a delete verb and its files ("deleted the old file a.py").
const FILLER: &[&str] = &["the", "a", "an", "file", "files", "folder", "old", "unused", "stale", "dead", "obsolete", "and", "&", "了"];
/// How many words after a change verb its files may come.
const CHANGE_REACH: usize = 8;

fn verb(word: &str, en: &[&str], zh: &[&str]) -> bool {
    en.contains(&word) || (!word.is_ascii() && zh.iter().any(|z| word.contains(z)))
}

/// The files a report claims it changed (`false`) or deleted (`true`), in
/// `verified:` and `notes:`: those a change verb takes ("updated calc.py"),
/// never one a clause denies ("did not change …") or refers to ("… from
/// utils.py"); deleted only right after its verb ("deleted old.py", not
/// "removed a print from calc.py").
fn path_claims(report: &str, host: Option<&str>) -> Vec<(bool, String)> {
    let mut out: Vec<(bool, String)> = Vec::new();
    for clause in clauses(&field(report, "verified")).into_iter().chain(clauses(&field(report, "notes"))) {
        let toks = tokens(&clause);
        let words: Vec<String> = toks.iter().map(|t| match t { Tok::Word(w) => bare(w), Tok::Tick(_) => String::new() }).collect();
        if words.iter().any(|w| NEGATION.contains(&w.as_str()) || w.contains("n't") || (!w.is_ascii() && NEGATION_ZH.iter().any(|n| w.contains(n)))) {
            continue;
        }
        let file = |t: &Tok| match t {
            Tok::Tick(s) if !s.contains(char::is_whitespace) && !is_command(s, host) => path(s),
            Tok::Word(w) => path(w),
            Tok::Tick(_) => None,
        };
        let mut gone = Vec::new();
        if let Some(at) = words.iter().position(|w| verb(w, DELETE, DELETE_ZH)) {
            let mut i = at + 1;
            while i < toks.len() {
                match file(&toks[i]) {
                    Some(p) => gone.push(p),
                    None if FILLER.contains(&words[i].as_str()) => {}
                    None => break,
                }
                i += 1;
            }
            // A part of a file ("deleted foo.rs's helper", "删除了 a.py 中的…"), not the file.
            if i < toks.len() && (matches!(words[i].as_str(), "from" | "in" | "inside" | "within" | "of") || words[i].starts_with(['中', '里'])) {
                gone.clear();
            }
        }
        let mut reach = 0;
        for (t, w) in toks.iter().zip(&words) {
            if verb(w, CHANGE, CHANGE_ZH) {
                reach = CHANGE_REACH;
                continue;
            }
            if REFERENCE.contains(&w.as_str()) || verb(w, DELETE, DELETE_ZH) {
                reach = 0;
            }
            if reach == 0 {
                continue;
            }
            reach -= 1;
            if let Some(p) = file(t).filter(|p| !gone.contains(p)) {
                if !out.contains(&(false, p.clone())) {
                    out.push((false, p));
                }
            }
        }
        for p in gone {
            if !out.contains(&(true, p.clone())) {
                out.push((true, p));
            }
        }
    }
    out
}

/// A clause as said, short, in quotes.
fn quote(clause: &str) -> String {
    let short: String = clause.chars().take(100).collect();
    if short.chars().count() < clause.chars().count() { format!("\"{short}…\"") } else { format!("\"{short}\"") }
}

/// How OctoBuddy's run ended, said.
fn run_said(cmd: &str, passed: bool, status: &str) -> String {
    format!("`{cmd}` {} ({status})", if passed { "passed" } else { "FAILED" })
}

/// A run that says how the work stands: it passed, or its command exited
/// failing (not one that could not start, or ran out of time).
fn decisive(passed: bool, status: &str) -> bool {
    passed || status.starts_with("exit ")
}

fn of_command(clause: &str, cmd: &str, said: Said, host: Option<&(String, bool, String)>) -> Claim {
    let claim = |mark, fact: String| Claim { mark, said: quote(clause), fact };
    let Some((h, passed, status)) = host else {
        return claim(Mark::Unverifiable, "no check ran: this is its own word".into());
    };
    let run = run_said(h, *passed, status);
    match (relate(cmd, h), said, *passed) {
        (Rel::Other, _, _) => claim(Mark::Unverifiable, format!("OctoBuddy did not run `{cmd}` (its check: {run})")),
        _ if !decisive(*passed, status) => claim(Mark::Unverifiable, format!("OctoBuddy's run of `{h}` did not finish ({status})")),
        (_, Said::NotRun, _) => claim(Mark::Unverifiable, format!("it says it did not run it; OctoBuddy's run of {run}")),
        (_, Said::Unclear, _) => claim(Mark::Unverifiable, format!("it does not say how it ended; OctoBuddy's run of {run}")),
        (_, Said::Pass, true) => claim(Mark::Holds, format!("OctoBuddy's run of {run}")),
        (_, Said::Fail, false) => claim(Mark::Holds, format!("OctoBuddy's run of {run} too")),
        (Rel::Same, Said::Pass, false) => claim(Mark::Contradicted, format!("OctoBuddy's run of {run}")),
        (Rel::Same, Said::Fail, true) => claim(Mark::Unverifiable, format!("OctoBuddy's run of {run}: flaky, or fixed since")),
        (Rel::Related, _, _) => claim(Mark::Unverifiable, format!("OctoBuddy ran {run}, not quite the same command")),
    }
}

/// "Tests pass", no command named: of the slice's check.
fn of_tests(clause: &str, said: Said, host: Option<&(String, bool, String)>) -> Claim {
    let claim = |mark, fact: String| Claim { mark, said: quote(clause), fact };
    let Some((cmd, passed, status)) = host else {
        return claim(Mark::Unverifiable, "no check ran: this is its own word".into());
    };
    let run = run_said(cmd, *passed, status);
    if !decisive(*passed, status) {
        return claim(Mark::Unverifiable, format!("OctoBuddy's run of `{cmd}` did not finish ({status})"));
    }
    match (said, *passed) {
        (Said::Pass, true) => claim(Mark::Holds, format!("OctoBuddy's run of {run}")),
        (Said::Pass, false) => claim(Mark::Contradicted, format!("OctoBuddy's run of {run}")),
        (_, false) => claim(Mark::Holds, format!("OctoBuddy's run of {run} too")),
        (_, true) => claim(Mark::Unverifiable, format!("OctoBuddy's run of {run}: other tests, flaky, or fixed since")),
    }
}

/// A file claimed changed or deleted: contradicted only when it plainly is
/// not (the same as at the start, never touched, not in its commit; or still
/// there); nothing said of one OctoBuddy cannot place.
fn of_path(gone: bool, said: &str, f: &Facts) -> Option<Claim> {
    let p = f.probed.iter().find(|p| p.said == said)?;
    let of = |other: &str| other == p.path || (p.path.ends_with('/') && other.starts_with(p.path.as_str()));
    let commit = f.committed.iter().find(|(_, c)| of(c)).map(|(s, _)| *s);
    let (mark, fact) = if gone {
        match (p.exists, p.diff, commit) {
            (true, _, _) => (Mark::Contradicted, format!("`{}` is still there", p.path)),
            (false, Some('D'), _) | (false, _, Some('D')) => (Mark::Holds, format!("`{}` is gone", p.path)),
            _ => return None,
        }
    } else if commit.is_some() {
        (Mark::Holds, format!("`{}` is in its commit", p.path))
    } else if p.diff.is_some() {
        (Mark::Holds, format!("`{}` changed since it began", p.path))
    } else if !p.in_base && p.exists {
        (Mark::Holds, format!("`{}` is new since it began", p.path))
    } else if p.in_base && p.exists && !f.touched.iter().chain(f.committed.iter().map(|(_, c)| c)).any(|t| of(t) || t.ends_with(&format!("/{said}"))) {
        (Mark::Contradicted, format!("`{}` is the same as when it began, and not in its commit", p.path))
    } else {
        return None;
    };
    Some(Claim { mark, said: format!("it {} `{said}`", if gone { "deleted" } else { "changed" }), fact })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Review;

    fn host(cmd: &str, passed: bool) -> Option<(String, bool, String)> {
        Some((cmd.into(), passed, if passed { "exit 0" } else { "exit 1" }.into()))
    }

    fn report(verified: &str, notes: &str) -> String {
        format!("status: done\nverified: {verified}\ndecide: none\nnotes: {notes}")
    }

    /// The marks of a `verified:` line against a check's run.
    fn marks(verified: &str, check: Option<(String, bool, String)>) -> Vec<Mark> {
        reconcile(&report(verified, "none"), &Facts { check, ..Default::default() }).into_iter().map(|c| c.mark).collect()
    }

    use Mark::{Contradicted as No, Holds as Yes, Unverifiable as Q};

    #[test]
    fn a_command_claim_against_the_check_run() {
        let cmd = "cargo test --locked";
        assert_eq!(marks("`cargo test --locked` passes", host(cmd, true)), [Yes]);
        assert_eq!(marks("`cargo test --locked` passes", host(cmd, false)), [No]);
        assert_eq!(marks("`cargo test --locked`: 12 passed, 0 failed", host(cmd, true)), [Yes], "a count of none is no failure");
        assert_eq!(marks("`cargo test --locked`: 12 passed, 0 failed", host(cmd, false)), [No]);
        assert_eq!(marks("`cargo test --locked`: 2 passed, 10 failed", host(cmd, false)), [Q], "10 failed is not 0 failed: mixed");
        assert_eq!(marks("ran cargo test --locked: ok", host(cmd, false)), [No], "a runner without backticks, with its flags");
        assert_eq!(marks("`cargo test --locked` fails on one test", host(cmd, false)), [Yes]);
        assert_eq!(marks("`cargo test --locked` fails on one test", host(cmd, true)), [Q], "it failed for it: flaky, not a lie");
        assert_eq!(marks("`cargo test --locked` could not run (no network)", host(cmd, false)), [Q]);
        assert_eq!(marks("ran `cargo test --locked`", host(cmd, false)), [Q], "no result said");
        assert_eq!(marks("`cargo test --no-fail-fast --locked` passes", host("cargo test --no-fail-fast --locked", false)), [No],
            "a flag's words are not a result");
        assert_eq!(marks("`cargo test --locked` passed: it starts and draws with no script errors", host(cmd, false)), [No], "no errors is none");
        assert_eq!(marks("`cargo test --locked` ✅", host(cmd, false)), [No]);
        assert_eq!(marks("octobuddy_check passed", host(cmd, false)), [Q], "its own earlier run of the check: flaky or changed since");
        assert_eq!(marks("octobuddy_check passed", host(cmd, true)), [Yes]);
    }

    #[test]
    fn a_command_not_quite_the_check_is_never_contradicted() {
        assert_eq!(marks("`cargo test` passes", host("cargo test --locked", false)), [Q]);
        assert_eq!(marks("`cargo test` passes", host("cargo test --locked", true)), [Yes]);
        let other = reconcile(&report("`pytest -q` passes", ""), &Facts { check: host("cargo test", true), ..Default::default() });
        assert_eq!(other[0].mark, Q);
        assert!(other[0].fact.contains("did not run `pytest -q`") && other[0].fact.contains("`cargo test` passed"), "{}", other[0].fact);
        let none = reconcile(&report("`cargo test` passes", ""), &Facts::default());
        assert_eq!((none[0].mark, none[0].fact.as_str()), (Q, "no check ran: this is its own word"));
        assert_eq!(marks("pytest tests/test_errors.py passes", host("pytest tests/test_errors.py", true)), [Yes], "a path's words are not a result");
    }

    #[test]
    fn a_semicolon_in_backticks_is_part_of_the_command() {
        let cmd = "python3 -c 'import calc; assert calc.add(1, 2) == 3'";
        let found = reconcile(&report(&format!("`{cmd}` passes"), ""), &Facts { check: host(cmd, false), ..Default::default() });
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mark, No);
        assert!(found[0].fact.contains("FAILED (exit 1)"), "{}", found[0].fact);
    }

    #[test]
    fn tests_said_to_pass_against_a_failed_check() {
        let cmd = "python3 -m pytest -q";
        assert_eq!(marks("all tests pass", host(cmd, false)), [No]);
        assert_eq!(marks("all tests pass", host(cmd, true)), [Yes]);
        assert_eq!(marks("测试全部通过", host(cmd, false)), [No]);
        assert_eq!(marks("全部通过；测试通过", host(cmd, false)), [No], "said once");
        assert_eq!(marks("测试未通过", host(cmd, false)), [Yes]);
        assert_eq!(marks("all tests pass", None), [Q]);
        assert_eq!(marks("looks clean", host(cmd, false)), [] as [Mark; 0], "nothing said of the tests");
        assert_eq!(marks("none", host(cmd, false)), [] as [Mark; 0]);
        assert_eq!(marks("the build passes", host(cmd, false)), [] as [Mark; 0], "a build is not the tests");
        assert_eq!(marks("all tests pass except the network one", host(cmd, false)), [] as [Mark; 0], "hedged: open");
        assert_eq!(marks("all tests pass", Some((cmd.into(), false, "timed out after 600s".into()))), [Q], "a run that did not finish says nothing");
    }

    #[test]
    fn only_the_verified_field_is_held_to_a_run() {
        let r = "status: done\nverified: `pytest` passes\ndecide: whether the tests fail on CI\nnotes: none\n\n[tests] `pytest` FAILED (exit 1)";
        let found = reconcile(r, &Facts { check: host("pytest", false), ..Default::default() });
        assert_eq!(found.iter().map(|c| c.mark).collect::<Vec<_>>(), [No]);
        assert_eq!(field("**Verified:** `x` ok\n  and more\n- notes: n", "verified"), "`x` ok\nand more");
    }

    fn probe(said: &str, in_base: bool, diff: Option<char>, exists: bool) -> Probe {
        Probe { said: said.into(), path: said.into(), in_base, diff, exists }
    }

    #[test]
    fn files_claimed_changed_or_deleted() {
        let r = report("tests pass", "updated README.md; deleted `old.py`");
        let mut f = Facts { probed: vec![probe("README.md", true, None, true), probe("old.py", true, None, true)], ..Default::default() };
        let found = reconcile(&r, &f);
        assert_eq!(found.iter().map(|c| (c.mark, c.said.as_str())).collect::<Vec<_>>(),
            [(Q, "\"tests pass\""), (No, "it changed `README.md`"), (No, "it deleted `old.py`")]);
        assert!(found[1].fact.contains("same as when it began"), "{}", found[1].fact);
        f.probed = vec![probe("README.md", true, Some('M'), true), probe("old.py", true, Some('D'), false)];
        assert_eq!(reconcile(&r, &f).iter().map(|c| c.mark).collect::<Vec<_>>(), [Q, Yes, Yes]);
        f.probed = vec![probe("README.md", true, None, true)];
        f.committed = vec![('M', "README.md".into())];
        assert_eq!(reconcile(&r, &f)[1].mark, Yes, "in its commit");
        f.committed.clear();
        f.touched = vec!["README.md".into()];
        assert_eq!(reconcile(&r, &f).len(), 1, "its tools changed it once: nothing said");
        f.touched = vec!["docs/README.md".into()];
        assert_eq!(reconcile(&r, &f).len(), 1, "a README.md it did change, elsewhere: nothing said");
        f.touched.clear();
        f.probed = vec![probe("README.md", false, None, false)];
        assert_eq!(reconcile(&r, &f).len(), 1, "a file not there at all: nothing said");
    }

    #[test]
    fn only_what_a_change_verb_takes_is_claimed() {
        let claimed = |notes: &str| path_claims(&report("ok", notes), None);
        assert_eq!(claimed("updated calc.py to use helpers from utils.py"), [(false, "calc.py".to_string())]);
        assert_eq!(claimed("removed a debug print from calc.py"), []);
        assert_eq!(claimed("wrote tests for calc.py"), []);
        assert_eq!(claimed("did not change README.md"), []);
        assert_eq!(claimed("README.md unchanged"), []);
        assert_eq!(claimed("删除了 calc.py 中的调试输出"), []);
        assert_eq!(claimed("删除了 `old.py`，修改了 `src/a.rs:12`"), [(false, "src/a.rs".to_string()), (true, "old.py".to_string())]);
        assert_eq!(claimed("deleted the old file legacy.py and updated docs/api.md"), [(false, "docs/api.md".to_string()), (true, "legacy.py".to_string())]);
        assert_eq!(claimed("see v1.2 e.g. the notes"), []);
        assert_eq!(claimed("added `divide` to calc.py, matching `utils.py`"), [(false, "calc.py".to_string())]);
    }

    #[test]
    fn its_lines_put_what_is_wrong_first() {
        let claim = |mark, said: &str| Claim { mark, said: said.into(), fact: "f".into() };
        let found = [claim(Yes, "a"), claim(Q, "b"), claim(No, "c")];
        let f = Facts { committed: vec![('D', "old.py".into()), ('M', "a.py".into())], ..Default::default() };
        assert_eq!(lines(&found, &f), ["[claims] ✗ c — f", "[claims] ? b — f", "[claims] ✓ a — f", "[files] its commit deletes: old.py"]);
        let many: Vec<Claim> = (0..9).map(|i| claim(Yes, &i.to_string())).collect();
        let out = lines(&many, &Facts::default());
        assert_eq!(out.len(), MAX_LINES);
        assert_eq!(out[MAX_LINES - 1], "[claims] … and 4 more");
        assert!(lines(&[], &Facts::default()).is_empty());
    }

    fn gated(g: Gate) -> Peer {
        let mut p = crate::chat::tests::peer(Vec::new());
        p.commits = Some(vec!["abc1234 feat: x".into()]);
        p.gate = Some(g);
        p
    }

    fn passed() -> Gate {
        Gate { check: Some(true), cmd: Some("pytest".into()), status: Some("exit 0".into()), at: Some("abc1234".into()), claims: Some(vec![]), stray: Some(vec![]) }
    }

    #[test]
    fn an_accept_is_refused_while_the_facts_say_no() {
        assert_eq!(refuse(&crate::chat::tests::peer(Vec::new())), None, "never checked: nothing to hold it to");
        assert_eq!(refuse(&gated(passed())), None);
        let failed = refuse(&gated(Gate { check: Some(false), status: Some("exit 1".into()), ..passed() })).unwrap();
        assert!(failed.contains("its check `pytest` failed (exit 1)") && failed.contains("\"override\"") && failed.contains("close"), "{failed}");
        let mut later = gated(passed());
        later.commits.as_mut().unwrap().push("def5678 fix: y".into());
        assert!(refuse(&later).unwrap().contains("committed again (def5678)"));
        let before = gated(Gate { at: Some(String::new()), ..passed() });
        assert!(refuse(&before).is_some(), "checked before its first commit, committed since");
        assert!(refuse(&gated(Gate { claims: Some(vec!["it changed `README.md` — the same".into()]), ..passed() })).unwrap().contains("README.md"));
        assert!(refuse(&gated(Gate { stray: Some(vec!["x.py".into()]), ..passed() })).unwrap().contains("outside its card's Files: x.py"));
        // No check ran, nothing contradicted: a later commit is not "after its check".
        let mut unchecked = gated(Gate::default());
        unchecked.commits.as_mut().unwrap().push("def5678 fix: y".into());
        assert_eq!(refuse(&unchecked), None);
    }

    #[test]
    fn an_override_is_recorded_with_its_reason() {
        let p = gated(Gate { check: Some(false), ..passed() });
        let review = |verdict: &str, over: Option<&str>| Review { slug: "feat".into(), verdict: verdict.into(), why: "ok".into(), over: over.map(String::from) };
        assert!(record(&p, &review("accept", None)).is_err());
        assert_eq!(record(&p, &review("accept", Some("the failing test is the flaky network one"))).unwrap(),
            "accept (override: the failing test is the flaky network one): ok");
        assert_eq!(record(&p, &review("fix", None)).unwrap(), "fix: ok", "a fix is never refused");
        assert_eq!(record(&gated(passed()), &review("accept", Some("x"))).unwrap(), "accept: ok", "nothing to override");
    }

    #[test]
    fn a_later_check_keeps_what_it_did_not_look_at() {
        let old = Gate { check: Some(false), claims: Some(vec!["c".into()]), stray: Some(vec!["x".into()]), ..passed() };
        let new = Gate { check: Some(true), at: Some("def5678".into()), claims: None, stray: None, ..passed() };
        let merged = merge(Some(old.clone()), new);
        assert_eq!((merged.check, merged.at.as_deref(), merged.claims, merged.stray), (Some(true), Some("def5678"), Some(vec!["c".to_string()]), Some(vec!["x".to_string()])));
        let cleared = merge(Some(old), Gate { claims: Some(vec![]), ..Gate::default() });
        assert_eq!((cleared.check, cleared.claims), (Some(false), Some(vec![])));
    }

    #[test]
    fn a_gate_holds_what_is_contradicted() {
        let found = [Claim { mark: No, said: "\"tests pass\"".into(), fact: "OctoBuddy's run of `pytest` FAILED (exit 1)".into() },
            Claim { mark: Yes, said: "s".into(), fact: "f".into() }];
        let ran = ("pytest".to_string(), false, "exit 1".to_string());
        let g = gate(Some(&found[..]), Some(&ran), Some(&["x.py".to_string()][..]), Some("abc1234"));
        assert_eq!(g, Gate { check: Some(false), cmd: Some("pytest".into()), status: Some("exit 1".into()), at: Some("abc1234".into()),
            claims: Some(vec!["\"tests pass\" — OctoBuddy's run of `pytest` FAILED (exit 1)".into()]), stray: Some(vec!["x.py".into()]) });
        assert_eq!(gate(None, None, None, Some("abc1234")), Gate::default(), "no check ran: no commit to be out of date against");
        let text = bounce(g.claims.as_deref().unwrap());
        assert!(text.contains("- \"tests pass\" — OctoBuddy's run of `pytest` FAILED") && text.contains("octobuddy-report"), "{text}");
    }
}
