//! The page a round leaves for the person to accept: what was asked, what
//! each slice did and how OctoBuddy checked it, and what still needs them.
//!
//! Made once, when a round is settled (each of its slices accepted, over
//! OctoBuddy's findings or not, or closed: what its rework is kept on), into
//! `<project>/.octobuddy/acceptance/<session>-r<round>.md`, for the person
//! and whoever they show it to. Its receipt (`Kind::Acceptance`) is the
//! record: prepared by OctoBuddy on the round's facts and HEAD, waiting for
//! the person; confirmed by their Confirm acceptance while both are as the
//! sheet has them; stale, and the sheet made again, when either moved. Which
//! rounds have a sheet is read from the ledger.
use crate::model::{now_secs, Peer, Role, Session, SessionRef};
use crate::receipts::{self, Check, Kind, Receipt, Snapshot, Status};
use crate::{claims, handoff, i18n, workspace, OctoBuddyView};
use makepad_widgets::*;
use std::path::{Path, PathBuf};

/// What a sheet is made of: a round's facts as OctoBuddy has them.
#[derive(Clone, Debug, Default)]
pub struct SheetInput {
    pub session: String,
    pub title: String,
    /// The project's name.
    pub project: String,
    pub round: u64,
    /// The commit the loops' directory is on.
    pub head: Option<String>,
    /// When it is made (epoch seconds).
    pub at: u64,
    /// The person's message the round answers, in their words.
    pub request: Option<String>,
    pub slices: Vec<Slice>,
}

/// One slice of the round: its inner loop as it stands.
#[derive(Clone, Debug)]
pub struct Slice {
    pub peer: Peer,
    /// What it ran on: its agent and model.
    pub on: String,
    /// The rework its timeline lane shows, a line each.
    pub rework: Vec<String>,
}

/// A sheet: its page, what on it still needs the person, and a digest of
/// the facts it was made of (not of its words: a sheet read in the other
/// language, or another day, is the same sheet).
#[derive(Clone, Debug, Default)]
pub struct Sheet {
    pub markdown: String,
    pub open: Vec<String>,
    pub digest: String,
    pub counts: Counts,
}

/// What the session's line says of a sheet.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counts {
    pub slices: usize,
    /// Its reports' claims: ✓ held, ✗ contradicted, ? not checked by OctoBuddy.
    pub held: usize,
    pub contradicted: usize,
    pub unchecked: usize,
    /// Slices the outer loop accepted over OctoBuddy's findings.
    pub overrides: usize,
}

/// How a slice of a round ended.
#[derive(Clone, Copy, Debug, PartialEq)]
enum End {
    Accepted,
    /// Accepted over OctoBuddy's findings (`accept (override: …)`).
    Overridden,
    Closed,
    /// Neither: its review is something else now (a stale accept, a fix).
    Open,
}

fn end(p: &Peer) -> End {
    match p.review.as_deref() {
        Some(r) if r.starts_with("accept (override") => End::Overridden,
        Some(r) if r.starts_with("accept") => End::Accepted,
        _ if p.status == "closed" => End::Closed,
        _ => End::Open,
    }
}

/// The latest round of `peers` once it is settled: each of its slices not
/// closed is accepted, and there is one. What `save_rework` keeps rework on.
pub fn settled_round(peers: &[Peer]) -> Option<u64> {
    let round = peers.iter().map(|p| p.round).max()?;
    let mine: Vec<&Peer> = peers.iter().filter(|p| p.round == round && p.status != "closed").collect();
    let accepted = |p: &Peer| p.review.as_deref().is_some_and(|r| r.starts_with("accept"));
    (!mine.is_empty() && mine.iter().all(|p| accepted(p))).then_some(round)
}

/// The person's message a round that began at `began` answers: the last
/// before it.
pub fn request_of(session: &Session, began: u64) -> Option<String> {
    session.messages.iter().rev().find(|m| m.role() == Role::User && m.at <= began).map(|m| m.text.trim().to_string())
}

/// Where the sheet of `round` goes.
pub fn path(project: &str, session: &str, round: u64) -> PathBuf {
    Path::new(project).join(".octobuddy/acceptance").join(format!("{session}-r{round}.md"))
}

/// A sheet receipt's inputs: its session and round, and its digest.
fn key(session: &str, round: u64, digest: &str) -> String {
    format!("{session} r{round} {digest}")
}

/// The session and round a sheet receipt's inputs name.
pub fn parse_key(inputs: &str) -> Option<(&str, u64)> {
    let mut words = inputs.split_whitespace();
    let session = words.next()?;
    let round = words.next()?.strip_prefix('r')?.parse().ok()?;
    Some((session, round))
}

/// What a sheet's receipt says to OctoSense (`octobuddy.status`).
pub fn status_word(status: Status) -> &'static str {
    match status {
        Status::Prepared => "waiting for the person",
        Status::Confirmed => "accepted",
        Status::Stale => "out of date",
        other => other.key(),
    }
}

/// What the person's Confirm does with a sheet made on `was`, the round
/// being `now`: confirms it, or, its commit or its facts moved since, makes
/// it again (what moved).
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    Confirm,
    Remake(Vec<String>),
}

pub fn decide(was: &Snapshot, now: &Snapshot) -> Decision {
    let moved = receipts::moved(was, now);
    if moved.is_empty() { Decision::Confirm } else { Decision::Remake(moved) }
}

/// The facts a sheet is made of, as one digest: the request, and each
/// slice's card, end, review, check, verdict and commits. Not its HEAD (the
/// snapshot's own), nor its words.
pub fn digest_of(input: &SheetInput) -> String {
    let mut parts = vec![input.session.clone(), input.round.to_string(), input.request.clone().unwrap_or_default()];
    for s in &input.slices {
        let p = &s.peer;
        parts.push(format!("{}\0{:?}\0{:?}\0{}\0{:?}\0{:?}\0{:?}\0{:?}\0{:?}", p.slug, end(p), p.review, p.brief, p.check, p.gate, p.verdict, p.commits, p.uncommitted));
    }
    receipts::digest(&parts.iter().map(|p| p.as_bytes()).collect::<Vec<_>>())
}

/// `secs` as a day and a time (UTC): `2026-10-12 09:30`.
fn when(secs: u64) -> String {
    // Days to a civil date (Howard Hinnant's `civil_from_days`).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs % 86_400 / 3600, secs % 3600 / 60)
}

/// The lines of a verdict that start with `tag` (`[claims]`), without it.
fn tagged<'a>(verdict: &'a str, tag: &str) -> Vec<&'a str> {
    verdict.lines().filter_map(|l| l.strip_prefix(tag)).map(str::trim).collect()
}

/// A card's goal: its `## Goal` (a contract's `## Intent`), else its brief's first line.
fn goal(brief: &str) -> String {
    let said = handoff::section(brief, &["goal", "intent"]).join(" ");
    if !said.is_empty() {
        return said;
    }
    brief.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("spec:") && !l.starts_with("---") && !l.starts_with('#')).unwrap_or("").to_string()
}

/// What a card says it is done when.
fn done_when(brief: &str) -> Vec<String> {
    handoff::section(brief, &["done when", "completion criteria", "acceptance criteria"]).into_iter()
        .map(|l| l.trim_start_matches(['-', '*']).trim().to_string()).filter(|l| !l.is_empty()).collect()
}

/// The reason an override gave: `accept (override: R): why` → `R`.
fn override_reason(review: &str) -> &str {
    let rest = review.strip_prefix("accept (override: ").unwrap_or(review);
    rest.split_once("): ").map(|(r, _)| r).unwrap_or(rest.trim_end_matches(')'))
}

/// What OctoBuddy cannot check, whatever a sheet says.
fn beyond() -> [&'static str; 3] {
    [
        i18n::t("how it looks and feels in use (layout, visuals, the experience)", "用起来的样子和感受（布局、视觉、体验）"),
        i18n::t("whether it does what you meant beyond each card's “Done when” and the checks that ran", "除了每张任务卡的「Done when」和跑过的检查之外，它是否真是你要的"),
        i18n::t("anything its checks do not run: other devices, real data, the network", "检查没有覆盖的：别的设备、真实数据、网络"),
    ]
}

/// The sheet of a round. Pure.
pub fn sheet(input: &SheetInput) -> Sheet {
    let mut counts = Counts { slices: input.slices.len(), ..Default::default() };
    let mut open: Vec<String> = Vec::new();
    let mut body = String::new();
    for s in &input.slices {
        let p = &s.peer;
        let slug = &p.slug;
        let ended = end(p);
        let verdict = p.verdict.as_deref().unwrap_or("");
        let claimed = tagged(verdict, "[claims]");
        let gate = p.gate.clone().unwrap_or_default();
        // What still needs the person, from this slice. One closed without an
        // accept is not part of the result: its claims and checks are moot.
        let weighed = ended != End::Closed;
        if ended == End::Overridden {
            counts.overrides += 1;
            let reasons = claims::reasons(p);
            let over = override_reason(p.review.as_deref().unwrap_or(""));
            open.push(if reasons.is_empty() {
                i18n::pick(format!("{slug}: the outer loop accepted it over OctoBuddy's findings, because “{over}”: decide whether that holds"),
                    format!("{slug}：外环不顾 OctoBuddy 的检查接受了它，理由是「{over}」：请判断这个理由是否成立"))
            } else {
                i18n::pick(format!("{slug}: the outer loop accepted it over OctoBuddy's findings ({}), because “{over}”: decide whether that holds", reasons.join("; ")),
                    format!("{slug}：外环不顾 OctoBuddy 的检查（{}）接受了它，理由是「{over}」：请判断这个理由是否成立", reasons.join("；")))
            });
        }
        for c in claimed.iter().filter(|_| weighed) {
            match c.chars().next() {
                Some('✓') => counts.held += 1,
                Some('✗') => {
                    counts.contradicted += 1;
                    // Overridden: its override above names it.
                    if ended != End::Overridden {
                        open.push(i18n::pick(format!("{slug}: {c} (the facts contradict its report)"), format!("{slug}：{c}（事实与它的汇报不符）")));
                    }
                }
                Some('?') => {
                    counts.unchecked += 1;
                    open.push(i18n::pick(format!("{slug}: {c} (the agent's own word: OctoBuddy did not check it)"), format!("{slug}：{c}（这是 agent 自己的说法，OctoBuddy 没有核实）")));
                }
                _ => {}
            }
        }
        if gate.check == Some(false) && weighed && ended != End::Overridden {
            open.push(i18n::pick(format!("{slug}: its check `{}` failed ({})", gate.cmd.as_deref().unwrap_or("?"), gate.status.as_deref().unwrap_or("?")),
                format!("{slug}：它的检查 `{}` 没有通过（{}）", gate.cmd.as_deref().unwrap_or("?"), gate.status.as_deref().unwrap_or("?"))));
        }
        let spec = tagged(verdict, "[agent-spec]");
        if gate.check.is_none() && spec.is_empty() && weighed {
            open.push(match p.check.as_deref() {
                Some(cmd) => i18n::pick(format!("{slug}: no check ran: nothing was executed (its check `{cmd}` did not run)"), format!("{slug}：没有跑过任何检查，什么都没执行（它的检查 `{cmd}` 没有运行）")),
                None => i18n::pick(format!("{slug}: no check ran: nothing was executed (its card names none)"), format!("{slug}：没有跑过任何检查，什么都没执行（任务卡没有写检查）")),
            });
        }
        if let Some(stray) = gate.stray.as_ref().filter(|s| !s.is_empty()) {
            if weighed && ended != End::Overridden {
                open.push(i18n::pick(format!("{slug}: it changed files outside its card: {} (check they belong to it)", stray.join(", ")),
                    format!("{slug}：它改了任务卡以外的文件：{}（请确认这些改动该由它做）", stray.join("、"))));
            }
        }
        let loose = p.uncommitted.clone().unwrap_or_default();
        let unsaved: Vec<&str> = tagged(verdict, "[commit]").into_iter().filter(|l| l.contains("not committed")).collect();
        if !loose.is_empty() {
            open.push(i18n::pick(format!("{slug}: {} file(s) left uncommitted: {}", loose.len(), loose.join(", ")), format!("{slug}：{} 个文件没有提交：{}", loose.len(), loose.join("、"))));
        } else if let Some(line) = unsaved.first() {
            open.push(format!("{slug}: {line}"));
        }
        match ended {
            End::Closed => {
                let commits = p.commits.clone().unwrap_or_default();
                open.push(if commits.is_empty() {
                    i18n::pick(format!("{slug}: closed without an accept: its work is not part of this round's result"), format!("{slug}：没有被接受就结束了：它的工作不算这一轮的成果"))
                } else {
                    let list: Vec<&str> = commits.iter().filter_map(|c| c.split_whitespace().next()).collect();
                    i18n::pick(format!("{slug}: closed without an accept, yet its commits are in the project ({}): keep or revert them", list.join(", ")),
                        format!("{slug}：没有被接受就结束了，但它的提交还在项目里（{}）：请决定保留还是撤回", list.join("、")))
                });
            }
            End::Open => open.push(i18n::pick(format!("{slug}: not accepted now (its review: {})", p.review.as_deref().unwrap_or("none")),
                format!("{slug}：现在不是已接受状态（审查：{}）", p.review.as_deref().unwrap_or("无")))),
            End::Accepted | End::Overridden => {}
        }

        // The slice as it went.
        let model = s.on.clone();
        body.push_str(&format!("\n### {slug} · {} · {model}\n\n", p.role()));
        let result = match ended {
            End::Accepted => i18n::t("accepted", "已接受"),
            End::Overridden => i18n::t("accepted over OctoBuddy's findings", "外环不顾 OctoBuddy 的检查接受"),
            End::Closed => i18n::t("closed without an accept", "未接受即结束"),
            End::Open => i18n::t("not accepted", "未接受"),
        };
        body.push_str(&i18n::pick(format!("- Result: {result}\n"), format!("- 结果：{result}\n")));
        let g = goal(&p.brief);
        if !g.is_empty() {
            body.push_str(&i18n::pick(format!("- Goal: {g}\n"), format!("- 目标：{g}\n")));
        }
        let done = done_when(&p.brief);
        if !done.is_empty() {
            body.push_str(i18n::t("- Done when:\n", "- 完成标准（Done when）：\n"));
            for d in &done {
                body.push_str(&format!("  - {d}\n"));
            }
        }
        let on = gate.at.as_deref().map(|at| i18n::pick(format!(", on `{at}`"), format!("，在 `{at}` 上"))).unwrap_or_default();
        let check = match (gate.check, gate.cmd.as_deref().or(p.check.as_deref())) {
            (Some(true), Some(cmd)) => i18n::pick(format!("`{cmd}` passed ({}{on})", gate.status.as_deref().unwrap_or("")), format!("`{cmd}` 通过（{}{on}）", gate.status.as_deref().unwrap_or(""))),
            (Some(false), Some(cmd)) => i18n::pick(format!("`{cmd}` FAILED ({}{on})", gate.status.as_deref().unwrap_or("")), format!("`{cmd}` 没有通过（{}{on}）", gate.status.as_deref().unwrap_or(""))),
            (None, Some(cmd)) => i18n::pick(format!("`{cmd}` did not run: nothing was executed"), format!("`{cmd}` 没有运行：什么都没执行")),
            _ => i18n::t("no check ran: nothing was executed", "没有跑过任何检查：什么都没执行").to_string(),
        };
        body.push_str(&i18n::pick(format!("- Check (run by OctoBuddy): {check}\n"), format!("- 检查（由 OctoBuddy 运行）：{check}\n")));
        for line in &spec {
            body.push_str(&i18n::pick(format!("- Contract (agent-spec): {line}\n"), format!("- 契约（agent-spec）：{line}\n")));
        }
        if !claimed.is_empty() {
            body.push_str(i18n::t("- Its report against OctoBuddy's facts (✓ held, ✗ contradicted, ? its own word):\n", "- 它的汇报对照 OctoBuddy 查到的事实（✓ 成立，✗ 不符，? 只是它自己的说法）：\n"));
            for c in &claimed {
                body.push_str(&format!("  - {c}\n"));
            }
        }
        if let Some(commits) = p.commits.as_ref().filter(|c| !c.is_empty()) {
            let list: Vec<String> = commits.iter().map(|c| match c.split_once(' ') {
                Some((sha, subject)) => format!("`{sha}` {subject}"),
                None => format!("`{c}`"),
            }).collect();
            body.push_str(&i18n::pick(format!("- Commits: {}\n", list.join("; ")), format!("- 提交：{}\n", list.join("；"))));
        }
        let receipts = tagged(verdict, "[receipt]");
        if !receipts.is_empty() {
            body.push_str(i18n::t("- Receipts (read back from git):\n", "- 回执（从 git 回读核对）：\n"));
            for r in &receipts {
                body.push_str(&format!("  - {r}\n"));
            }
        }
        if !loose.is_empty() {
            body.push_str(&i18n::pick(format!("- Not committed: {}\n", loose.join(", ")), format!("- 未提交：{}\n", loose.join("、"))));
        }
        for line in &unsaved {
            body.push_str(&format!("- [commit] {line}\n"));
        }
        if let Some(stray) = gate.stray.as_ref().filter(|s| !s.is_empty()) {
            body.push_str(&i18n::pick(format!("- Outside its card: {}\n", stray.join(", ")), format!("- 任务卡以外：{}\n", stray.join("、"))));
        }
        body.push_str(&match p.review.as_deref() {
            Some(r) => i18n::pick(format!("- The outer loop's review: {r}\n"), format!("- 外环的审查：{r}\n")),
            None => i18n::t("- The outer loop's review: none\n", "- 外环的审查：无\n").to_string(),
        });
        if !s.rework.is_empty() {
            body.push_str(i18n::t("- Rework:\n", "- 返工：\n"));
            for r in &s.rework {
                body.push_str(&format!("  - {r}\n"));
            }
        }
        let mut spent = Vec::new();
        if let Some(n) = p.rounds_used {
            spent.push(i18n::pick(format!("{n} steps (model calls)"), format!("{n} 步（模型调用）")));
        }
        if let Some(e) = p.estimate {
            spent.push(i18n::pick(format!("estimated {e} rounds"), format!("估算 {e} 轮")));
        }
        if let Some(cost) = p.usage.as_ref().map(|u| u.cost).filter(|c| *c > 0.0) {
            spent.push(format!("${cost:.4}"));
        }
        if !spent.is_empty() {
            body.push_str(&i18n::pick(format!("- Spent: {}\n", spent.join(" · ")), format!("- 花费：{}\n", spent.join(" · "))));
        }
    }

    let mut md = String::new();
    md.push_str(&i18n::pick(format!("# Acceptance sheet · round {}\n\n", input.round), format!("# 验收单 · 第 {} 轮\n\n", input.round)));
    let head = input.head.as_deref().map(|h| format!("`{}`", receipts::short(h))).unwrap_or_else(|| "—".into());
    md.push_str(&i18n::pick(format!("Project **{}** · session “{}” (`{}`) · HEAD {head} · {} UTC\n\n", input.project, input.title, input.session, when(input.at)),
        format!("项目 **{}** · 会话「{}」（`{}`） · HEAD {head} · {} UTC\n\n", input.project, input.title, input.session, when(input.at))));
    md.push_str(i18n::t("## Your request\n\n", "## 你的需求\n\n"));
    match input.request.as_deref().filter(|r| !r.is_empty()) {
        Some(r) => {
            let r: String = if r.chars().count() > 3000 { format!("{}…", r.chars().take(3000).collect::<String>()) } else { r.to_string() };
            for line in r.lines() {
                md.push_str(&format!("> {line}\n"));
            }
        }
        None => md.push_str(i18n::t("(no message of yours found before this round)\n", "（这一轮之前没有找到你的消息）\n")),
    }
    md.push_str(i18n::t("\n## Still needs you\n\n", "\n## 仍需你确认/处理\n\n"));
    if open.is_empty() {
        md.push_str(i18n::t("Nothing: everything OctoBuddy could check holds.\n", "没有：OctoBuddy 能检查的都成立。\n"));
    } else {
        for item in &open {
            md.push_str(&format!("- [ ] {item}\n"));
        }
    }
    md.push_str(i18n::t("\nWhat OctoBuddy cannot check, whatever this sheet says:\n", "\nOctoBuddy 检查不了的（无论这张单子怎么写）：\n"));
    for b in beyond() {
        md.push_str(&format!("- {b}\n"));
    }
    md.push_str(&i18n::pick(format!("\n## Slices ({})\n", input.slices.len()), format!("\n## 切片（{} 个）\n", input.slices.len())));
    md.push_str(&body);
    md.push_str(i18n::t("\n---\n\nMade by OctoBuddy from what it ran, committed and read back itself; the reviews are the outer loop's, the claims the inner loops'. Confirm acceptance in the session once you have checked it.\n",
        "\n---\n\n由 OctoBuddy 根据它自己运行、提交和回读的结果整理；审查出自外环，说法出自各个 inner。核对后在会话里点「确认验收」。\n"));
    Sheet { markdown: md, open, digest: digest_of(input), counts }
}

/// The rework `tl` found in `peer`'s lane, as its list has it.
fn lane_rework(tl: &crate::timeline::Timeline, peer: &str) -> Vec<String> {
    let key = format!("p:{peer}");
    tl.rework.iter().zip(tl.rework_lines()).filter(|(r, _)| tl.lanes.get(r.lane).is_some_and(|l| l.key == key)).map(|(_, l)| l).collect()
}

/// The acceptance receipts of `session`, in the order they were made.
fn sheets_of(session: &str) -> Vec<Receipt> {
    receipts::all().into_iter().filter(|r| r.kind == Kind::Acceptance && parse_key(&r.inputs).is_some_and(|(s, _)| s == session)).collect()
}

/// What confirming a waiting sheet came to.
pub(crate) enum ConfirmResult {
    /// Its HEAD and its facts were still as it has them (the commit, short).
    Confirmed(String),
    /// The project moved: that sheet is stale, a new one was made.
    Remade,
}

/// A waiting sheet's receipt confirmed (`by`: the person at the button, a
/// coordinator's Matrix id from the hagency room): settled by its checks.
pub(crate) fn confirmed(mut r: Receipt, by: &str, commit: &str, now: &Snapshot) -> Receipt {
    r.by = by.into();
    receipts::settle(&mut r, vec![Check::new("commit", Some(true), commit.to_string()), Check::new("content", Some(true), receipts::short(&now.content).to_string())], None);
    r
}

impl OctoBuddyView {
    /// The facts of `round` in the session at `at`, for its sheet; its
    /// timeline's rework when `rework` (words, not facts: not digested).
    fn sheet_input(&self, at: SessionRef, round: u64, head: Option<String>, rework: bool) -> Option<SheetInput> {
        let session = self.store.session(at)?;
        // An independent reviewer's findings went into the report of the slice it reviewed.
        let peers: Vec<&Peer> = session.peers().iter().filter(|p| p.round == round && p.reviews_for.is_none()).collect();
        let began = peers.iter().map(|p| p.started_at).min().unwrap_or_else(now_secs);
        let tl = rework.then(|| crate::timeline::build(session, now_secs(), false, &|p| p.map(|p| self.ran_on(p)).unwrap_or_default()));
        let slices = peers.into_iter().map(|p| {
            let mut peer = p.clone();
            // What waits to be committed is the runtime's (the peer's own field is only as last saved).
            let loose = self.rt.uncommitted(&p.id);
            if !loose.is_empty() {
                peer.uncommitted = Some(loose);
            }
            Slice { on: self.ran_on(p), rework: tl.as_ref().map(|tl| lane_rework(tl, &p.id)).unwrap_or_default(), peer }
        }).collect();
        Some(SheetInput {
            session: session.id.clone(), title: session.title.clone(), project: self.store.projects[at.0].name.clone(), round, head, at: now_secs(),
            request: request_of(session, began), slices,
        })
    }

    /// The commit the session's loops work on.
    fn sheet_head(&self, at: SessionRef) -> Option<String> {
        let session = self.store.session(at)?;
        let dir = session.work_dir.clone().unwrap_or_else(|| self.store.projects[at.0].path.clone());
        workspace::git(&dir, &["rev-parse", "HEAD"]).ok()
    }

    /// The latest acceptance receipt of `session`: as last read, else the ledger's.
    pub(crate) fn sheet_of(&self, session: &str) -> Option<Receipt> {
        match self.sheets.get(session) {
            Some(r) => r.clone(),
            None => sheets_of(session).pop(),
        }
    }

    /// A round just settled (an accept, a close): its sheet, once.
    pub(crate) fn acceptance_due(&mut self, at: SessionRef) {
        let Some(session) = self.store.session(at) else { return };
        if self.store.is_plain(at) || session.is_detached() {
            return;
        }
        let Some(round) = settled_round(session.peers()) else { return };
        let id = session.id.clone();
        if sheets_of(&id).iter().any(|r| parse_key(&r.inputs).is_some_and(|(_, n)| n == round)) {
            return;
        }
        self.make_sheet(at, round);
    }

    /// Writes the sheet of `round`, prepares its receipt, and says so in the session.
    fn make_sheet(&mut self, at: SessionRef, round: u64) {
        let head = self.sheet_head(at);
        let Some(input) = self.sheet_input(at, round, head.clone(), true) else { return };
        let sheet = sheet(&input);
        let project = self.store.projects[at.0].path.clone();
        let file = path(&project, &input.session, round);
        let written = file.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&file, &sheet.markdown));
        if let Err(err) = written {
            self.system(at, &i18n::pick(format!("Could not write the acceptance sheet for round {round} ({}): {err}", file.display()),
                format!("没能写出第 {round} 轮的验收单（{}）：{err}", file.display())));
            return;
        }
        let snap = Snapshot { head, content: sheet.digest.clone(), version: None };
        let receipt = receipts::prepare(Kind::Acceptance, &project, &key(&input.session, round, &sheet.digest), snap, "OctoBuddy").ok();
        self.sheets.insert(input.session.clone(), receipt.or_else(|| sheets_of(&input.session).pop()));
        let c = sheet.counts;
        let open = sheet.open.len();
        let shown = file.display();
        let rest = if open == 0 {
            i18n::pick(format!("nothing OctoBuddy could check is left for you (it cannot check how it looks or feels). It is at {shown}. Read it, then press Confirm acceptance; until then nothing happens."),
                format!("OctoBuddy 能检查的都没有留给你的事（它检查不了用起来的样子和感受）。文件：{shown}。看过后点「确认验收」；不点就什么都不会发生。"))
        } else {
            i18n::pick(format!("{open} thing(s) still need you. It is at {shown}. Read it, then press Confirm acceptance; until then nothing happens."),
                format!("{open} 件事仍需你确认/处理。文件：{shown}。看过后点「确认验收」；不点就什么都不会发生。"))
        };
        self.system(at, &i18n::pick(
            format!("Acceptance sheet for round {round}: {} slice(s); their reports' claims ✓ {} held, ✗ {} contradicted, ? {} not checked by OctoBuddy; {} accepted over OctoBuddy's findings; {rest}", c.slices, c.held, c.contradicted, c.unchecked, c.overrides),
            format!("第 {round} 轮的验收单：{} 个切片；汇报里的说法 ✓ {} 条成立、✗ {} 条与事实不符、? {} 条 OctoBuddy 没有核实；{} 个是外环不顾 OctoBuddy 的检查接受的；{rest}", c.slices, c.held, c.contradicted, c.unchecked, c.overrides)));
        crate::hagency::tell(&project, &format!("sheet:{}:r{round}:{}", input.session, receipts::short(&sheet.digest)), &crate::hagency::sheet_message(round, &sheet.counts, &sheet.open));
    }

    /// The Confirm acceptance button: shown while the session's latest sheet waits.
    pub(crate) fn sync_acceptance(&mut self, cx: &mut Cx, session: &str) {
        if !self.sheets.contains_key(session) {
            self.sheets.insert(session.to_string(), sheets_of(session).pop());
        }
        let waiting = self.sheets.get(session).cloned().flatten().filter(|r| r.status == Status::Prepared).and_then(|r| parse_key(&r.inputs).map(|(_, n)| n));
        let button = self.view.button(cx, ids!(confirm_acceptance));
        button.set_visible(cx, waiting.is_some());
        if let Some(round) = waiting {
            button.set_text(cx, &i18n::pick(format!("Confirm acceptance · round {round}"), format!("确认验收 · 第 {round} 轮")));
        }
    }

    /// The person confirms the selected session's waiting sheet: confirmed
    /// when its HEAD and its facts are still as it has them; else it is
    /// stale and a new one is made for the round as it is now.
    pub(crate) fn confirm_acceptance(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected else { return };
        if self.confirm_sheet(at, "person").is_some() {
            self.save();
            self.sync(cx);
        }
    }

    /// The waiting sheet of `at`'s session confirmed (`by`: "person" at the
    /// button, a coordinator's Matrix id from the hagency room) when its
    /// HEAD and its facts are still as it has them; else it is stale and a
    /// new one is made for the round as it is now. Its round and what
    /// happened, when a sheet waited.
    fn confirm_sheet(&mut self, at: SessionRef, by: &str) -> Option<(u64, ConfirmResult)> {
        let id = self.store.session(at)?.id.clone();
        let mut r = self.sheet_of(&id).filter(|r| r.status == Status::Prepared)?;
        let (_, round) = parse_key(&r.inputs)?;
        let head = self.sheet_head(at);
        let input = self.sheet_input(at, round, head.clone(), false)?;
        let now = Snapshot { head: head.clone(), content: digest_of(&input), version: None };
        let file = path(&self.store.projects[at.0].path, &id, round);
        match decide(&r.snap, &now) {
            Decision::Confirm => {
                let commit = head.as_deref().map(|h| receipts::short(h).to_string()).unwrap_or_else(|| "—".into());
                r = confirmed(r, by, &commit, &now);
                let said = if by == "person" {
                    i18n::pick(format!("You accepted round {round} (sheet {}, {}): the project is still at {commit} and the round's facts are as the sheet has them.", receipts::short(&r.snap.content), file.display()),
                        format!("你验收了第 {round} 轮（验收单 {}，{}）：项目仍在 {commit}，这一轮的事实与验收单一致。", receipts::short(&r.snap.content), file.display()))
                } else {
                    i18n::pick(format!("{by} accepted round {round} from the hagency room (sheet {}, {}): the project is still at {commit} and the round's facts are as the sheet has them.", receipts::short(&r.snap.content), file.display()),
                        format!("{by} 在 hagency 房间验收了第 {round} 轮（验收单 {}，{}）：项目仍在 {commit}，这一轮的事实与验收单一致。", receipts::short(&r.snap.content), file.display()))
                };
                self.system(at, &said);
                let note = if by == "person" {
                    format!("the person accepted round {round} on its acceptance sheet ({})", file.display())
                } else {
                    format!("{by} accepted round {round} from the hagency room ({})", file.display())
                };
                self.rt.notes.entry(id.clone()).or_default().push(note);
                if by == "person" {
                    crate::hagency::tell(&self.store.projects[at.0].path, &format!("accepted:{id}:r{round}"), &crate::hagency::accepted_message(round, by, &commit));
                }
                self.sheets.insert(id, Some(r));
                Some((round, ConfirmResult::Confirmed(commit)))
            }
            Decision::Remake(moved) => {
                let _ = receipts::still(&r, &now);
                self.system(at, &i18n::pick(format!("The project changed since the sheet for round {round} ({}): it is out of date, and here is a new one.", moved.join("; ")),
                    format!("第 {round} 轮的验收单做好之后项目变了（{}）：它已过期，下面是新的一份。", moved.join("；"))));
                self.sheets.remove(&id);
                self.make_sheet(at, round);
                Some((round, ConfirmResult::Remade))
            }
        }
    }

    /// A coordinator accepted round `round` in the project's hagency room:
    /// the same confirm as the button, with the receipt's `by` their Matrix
    /// id. What the room hears back.
    pub(crate) fn accept_from_room(&mut self, project: &str, round: u64, by: &str) -> String {
        let Some(pi) = self.store.projects.iter().position(|p| p.path == project) else {
            return i18n::pick("This project is not open here.".into(), "这个项目没有在这里打开。".into());
        };
        let at = (0..self.store.projects[pi].sessions.len()).rev().map(|si| (pi, si)).find(|at| {
            self.store.session(*at).is_some_and(|s| !s.is_detached() && !s.is_archived()
                && self.sheet_of(&s.id).filter(|r| r.status == Status::Prepared).and_then(|r| parse_key(&r.inputs).map(|(_, n)| n)) == Some(round))
        });
        let Some(at) = at else {
            return i18n::pick(format!("Round {round} has no acceptance sheet waiting (it may be accepted already, or its round has not settled yet)."),
                format!("第 {round} 轮没有在等待确认的验收单（可能已经验收过，或这一轮还没有到齐）。"));
        };
        let result = self.confirm_sheet(at, by);
        self.save();
        match result {
            Some((_, ConfirmResult::Confirmed(commit))) => crate::hagency::accepted_message(round, by, &commit),
            Some((_, ConfirmResult::Remade)) => i18n::pick(
                format!("The project changed since round {round}'s sheet, so it was out of date: a new sheet was made — read it, then accept round {round} again."),
                format!("第 {round} 轮的验收单做好之后项目变了，它已过期：已出新的一份——看过后再回复「验收 第 {round} 轮」。")),
            None => i18n::pick(format!("Round {round} has no acceptance sheet waiting (it may be accepted already, or its round has not settled yet)."),
                format!("第 {round} 轮没有在等待确认的验收单（可能已经验收过，或这一轮还没有到齐）。")),
        }
    }

    /// A session's sheet for OctoSense (`octobuddy.status`): none, or its
    /// round, file, status and the first of what still needs the person.
    pub(crate) fn acceptance_status(&self, session: &str) -> serde_json::Value {
        let (Some(r), Some(at)) = (self.sheet_of(session), self.store.find_session(session)) else { return serde_json::Value::Null };
        let Some((_, round)) = parse_key(&r.inputs) else { return serde_json::Value::Null };
        let open: Vec<String> = self.sheet_input(at, round, r.snap.head.clone(), false).map(|i| sheet(&i).open.into_iter().take(5).collect()).unwrap_or_default();
        serde_json::json!({"round": round, "path": path(&self.store.projects[at.0].path, session, round).display().to_string(), "status": status_word(r.status), "open": open})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::tests::peer as make_peer;
    use crate::model::{Gate, Usage};

    const CARD: &str = "## Goal\nAdd `add(a, b)` to calc.py.\n## Files\n- calc.py\n## Facts\n- ints only\n## Done when\n- add(1, 2) == 3\n- the check passes\n";

    /// A slice accepted on every check: its check passed, its claims held, its commit read back.
    fn clean(slug: &str) -> Peer {
        let mut p = make_peer(Vec::new());
        p.id = format!("w-{slug}");
        p.slug = slug.into();
        p.brief = CARD.into();
        p.check = Some("pytest -q".into());
        p.commits = Some(vec!["abc1234 feat(calc): add".into()]);
        p.verdict = Some("[commit] abc1234 feat(calc): add (calc.py)\n[receipt] ✓ committed abc1234 — read back: files, tree, content\n[tests] `pytest -q` passed (exit 0)\n[claims] ✓ `pytest -q` passes — OctoBuddy's run passed (exit 0)".into());
        p.gate = Some(Gate { check: Some(true), cmd: Some("pytest -q".into()), status: Some("exit 0".into()), at: Some("abc1234".into()), claims: Some(Vec::new()), stray: Some(Vec::new()) });
        p.review = Some("accept: tests pass".into());
        p.rounds_used = Some(12);
        p.estimate = Some(3.0);
        p.usage = Some(Usage { input: 1, output: 1, cost: 0.04, context_window: None });
        p
    }

    fn input(slices: Vec<Peer>) -> SheetInput {
        SheetInput {
            session: "s-1".into(), title: "Calc".into(), project: "calc".into(), round: 2, head: Some("abc1234def".into()), at: 1_791_763_200 + 9 * 3600 + 30 * 60,
            request: Some("Add an add function.\nKeep it small.".into()),
            slices: slices.into_iter().map(|peer| Slice { peer, on: "octos · MiniMax-M3".into(), rework: Vec::new() }).collect(),
        }
    }

    #[test]
    fn a_round_all_held_leaves_nothing_but_what_cannot_be_checked() {
        i18n::set(i18n::Lang::En);
        let s = sheet(&input(vec![clean("calc-add")]));
        assert!(s.open.is_empty(), "{:?}", s.open);
        assert_eq!(s.counts, Counts { slices: 1, held: 1, ..Default::default() });
        let md = &s.markdown;
        for want in ["# Acceptance sheet · round 2", "HEAD `abc123` · 2026-10-12 09:30 UTC", "> Add an add function.\n> Keep it small.",
            "Nothing: everything OctoBuddy could check holds.", "how it looks and feels in use", "### calc-add · developer · octos · MiniMax-M3",
            "- Goal: Add `add(a, b)` to calc.py.", "  - add(1, 2) == 3", "- Check (run by OctoBuddy): `pytest -q` passed (exit 0, on `abc1234`)",
            "  - ✓ `pytest -q` passes", "- Commits: `abc1234` feat(calc): add", "  - ✓ committed abc1234 — read back", "- The outer loop's review: accept: tests pass",
            "- Spent: 12 steps (model calls) · estimated 3 rounds · $0.0400"] {
            assert!(md.contains(want), "missing {want:?} in:\n{md}");
        }
        println!("{md}");
    }

    #[test]
    fn what_octobuddy_did_not_check_or_was_overruled_on_still_needs_the_person() {
        i18n::set(i18n::Lang::En);
        let mut unchecked = clean("ui");
        unchecked.verdict = Some(format!("{}\n[claims] ? the layout looks right — OctoBuddy ran nothing that shows it", unchecked.verdict.unwrap()));
        let mut over = clean("core");
        over.gate = Some(Gate { check: Some(false), cmd: Some("pytest -q".into()), status: Some("exit 1".into()), at: Some("abc1234".into()), claims: Some(vec!["`pytest -q` passes — OctoBuddy's run FAILED (exit 1)".into()]), stray: Some(vec!["README.md".into()]) });
        over.verdict = Some("[claims] ✗ `pytest -q` passes — OctoBuddy's run FAILED (exit 1)".into());
        over.review = Some("accept (override: the failure is a flaky network test): done".into());
        let mut stray = clean("docs");
        stray.gate.as_mut().unwrap().stray = Some(vec!["src/main.rs".into()]);
        let mut closed = clean("old");
        closed.review = None;
        closed.status = "closed".into();
        closed.verdict = Some("[claims] ? it works on Windows — OctoBuddy ran nothing that shows it".into());
        let mut none = clean("notes");
        none.check = None;
        none.gate = None;
        none.verdict = None;
        none.uncommitted = Some(vec!["NOTES.md".into()]);
        let s = sheet(&input(vec![unchecked, over, stray, closed, none]));
        let has = |slug: &str, what: &str| s.open.iter().any(|o| o.starts_with(&format!("{slug}: ")) && o.contains(what));
        assert!(has("ui", "? the layout looks right") && has("ui", "the agent's own word"), "{:#?}", s.open);
        assert!(has("core", "accepted it over OctoBuddy's findings") && has("core", "flaky network test") && has("core", "README.md"), "{:#?}", s.open);
        assert!(!s.open.iter().any(|o| o.starts_with("core: ✗")), "its override names it: {:#?}", s.open);
        assert!(has("docs", "outside its card: src/main.rs"), "{:#?}", s.open);
        assert!(has("old", "closed without an accept") && has("old", "abc1234"), "{:#?}", s.open);
        assert_eq!(s.open.iter().filter(|o| o.starts_with("old: ")).count(), 1, "only that it was closed: {:#?}", s.open);
        assert!(has("notes", "no check ran: nothing was executed") && has("notes", "1 file(s) left uncommitted: NOTES.md"), "{:#?}", s.open);
        assert_eq!(s.counts, Counts { slices: 5, held: 2, contradicted: 1, unchecked: 1, overrides: 1 }, "not the closed slice's claims");
        assert!(s.markdown.contains("- [ ] ui: ? the layout looks right"), "{}", s.markdown);
        assert!(!s.markdown.contains("Nothing: everything"));
    }

    #[test]
    fn a_sheet_speaks_the_persons_language_and_its_digest_does_not_change_with_it() {
        let one = input(vec![clean("calc-add")]);
        i18n::set(i18n::Lang::Zh);
        let zh = sheet(&one);
        assert!(zh.markdown.contains("# 验收单 · 第 2 轮") && zh.markdown.contains("## 仍需你确认/处理") && zh.markdown.contains("没有：OctoBuddy 能检查的都成立。"), "{}", zh.markdown);
        let mut none = clean("notes");
        none.check = None;
        none.gate = None;
        assert!(sheet(&input(vec![none])).open[0].contains("没有跑过任何检查"));
        i18n::set(i18n::Lang::En);
        let en = sheet(&one);
        assert!(en.markdown.contains("## Still needs you"));
        assert_eq!(zh.digest, en.digest, "the same facts, in either language");
        let later = SheetInput { at: one.at + 86_400, head: Some("fff".into()), ..one.clone() };
        assert_eq!(sheet(&later).digest, en.digest, "not its day, nor its HEAD (the snapshot's own)");
        let mut changed = one.clone();
        changed.slices[0].peer.review = Some("stale: b changed calc.py after it was accepted".into());
        assert_ne!(sheet(&changed).digest, en.digest);
    }

    #[test]
    fn a_round_settles_when_each_open_slice_is_accepted() {
        let mut a = clean("a");
        let mut b = clean("b");
        assert_eq!(settled_round(&[a.clone(), b.clone()]), Some(1));
        b.review = Some("fix: more tests".into());
        assert_eq!(settled_round(&[a.clone(), b.clone()]), None);
        b.status = "closed".into();
        assert_eq!(settled_round(&[a.clone(), b.clone()]), Some(1), "a closed slice is settled");
        a.status = "closed".into();
        assert_eq!(settled_round(&[a.clone(), b.clone()]), None, "a round closed whole has nothing accepted");
        let mut next = clean("c");
        next.round = 2;
        next.review = None;
        assert_eq!(settled_round(&[clean("a"), next]), None, "only the latest round");
    }

    #[test]
    fn confirming_a_sheet_whose_project_moved_makes_a_new_one() {
        let was = Snapshot { head: Some("abc1234".into()), content: "d1".into(), version: None };
        assert_eq!(decide(&was, &was.clone()), Decision::Confirm);
        i18n::set(i18n::Lang::En);
        let moved = Snapshot { head: Some("def5678".into()), ..was.clone() };
        assert!(matches!(decide(&was, &moved), Decision::Remake(m) if m[0].contains("abc123 → def567")));
        assert!(matches!(decide(&was, &Snapshot { content: "d2".into(), ..was.clone() }), Decision::Remake(_)), "its facts changed");
        assert_eq!(parse_key(&key("s-1", 3, "d1")), Some(("s-1", 3)));
        assert_eq!(parse_key("s-1 3 d1"), None);
        assert_eq!(status_word(Status::Prepared), "waiting for the person");
    }

    #[test]
    fn the_request_is_the_persons_last_message_before_the_round() {
        let mut s = crate::model::Store::default();
        let dir = std::env::temp_dir();
        s.projects.push(crate::model::Project { id: "p".into(), name: "p".into(), path: dir.to_string_lossy().into_owned(), expanded: true, sessions: Vec::new(), kind: None, pinned: None, archived: None });
        let at = s.add_session(0).unwrap();
        s.push_message(at, Role::User, "", "first");
        s.push_message(at, Role::Lead, "claude", "ok");
        let session = s.session_mut(at).unwrap();
        session.messages[0].at = 100;
        session.messages[1].at = 110;
        assert_eq!(request_of(session, 105).as_deref(), Some("first"));
        assert_eq!(request_of(session, 50), None);
        assert_eq!(when(0), "1970-01-01 00:00");
    }
}
