//! An independent reviewer: for a slice that asked for one, a fresh inner
//! loop that shares none of its context reads what it did (its task, its
//! report, its diff) and ranks the problems it finds. It runs read-only,
//! once per report; its findings go with the report to the outer loop, which
//! sees both at once, and it is closed when done.
use crate::dispatch::{Delivery, From, Line};
use crate::events::{self, LoopEvent};
use crate::model::{self, now_secs, Peer, SessionRef};
use crate::{i18n, workspace, OctoBuddyView};

const RUBRIC: &str = "You are an independent reviewer for OctoBuddy. You did not do this work and share no \
context with who did: judge it only by what is below and the files as they are now. Review only: change \
no file, run nothing that writes. Find the real problems, ranked:\n\
- P0: wrong or broken (it fails its task, breaks what worked, loses data, is unsafe)\n\
- P1: a likely bug, a missing case, a breach of its task's boundaries or contract\n\
- P2: quality (clarity, tests that prove little, needless complexity)\n\
For each: file:line, what is wrong, and why. Say plainly when it is sound. Do not restate the work. End \
with one report block holding your findings, P0 first (or \"no findings\").";

/// The outer loop's own reviewer (a Claude Code subagent on a cheaper
/// model, `--agents`): the same rubric, its brief in a file OctoBuddy wrote.
pub(crate) fn reviewer_agent() -> String {
    let prompt = RUBRIC.replace("End with one report block holding your findings", "End with your findings")
        + "\n\nThe outer loop gives you a file (under .octobuddy/context/): it holds the work's task, its report and its diff. \
Read it, then read the files it changed as they are now where you need more.";
    serde_json::json!({
        // For a wide look (many files, a template app, the docs) while it plans:
        // fast, cheap, read-only, several at once.
        "scout": {
            "description": "Fast read-only scout: finds and summarises what the outer loop needs from many files or long docs (it reads, you plan). Give it one precise question; run several in parallel.",
            "prompt": "You are a scout for OctoBuddy's outer loop. Answer its one question from the files, briefly: the facts it needs (file:line for each), quoted where exact wording matters, nothing else. Read only; change nothing.",
            "tools": ["Read", "Glob", "Grep"],
            "model": "haiku",
        },
        "reviewer": {
            "description": "Independent reviewer: reviews an inner loop's slice (its task, report and diff in the file it is given) with fresh eyes, findings ranked P0-P2.",
            "prompt": prompt,
            "tools": ["Read", "Glob", "Grep"],
            "model": "sonnet",
        }
    }).to_string()
}

impl OctoBuddyView {
    /// A peer's report is complete (its checks, if any, have run). A
    /// reviewer's findings join the report they reviewed; work that asked
    /// for an independent review gets its reviewer first.
    pub(crate) fn on_report_ready(&mut self, at: SessionRef, peer: &str) {
        let Some(p) = self.store.session(at).and_then(|s| s.peers().iter().find(|p| p.id == peer)).cloned() else { return };
        if let Some(of) = p.reviews_for.clone() {
            self.reviewer_done(at, &p, &of);
        } else if p.review_wanted == Some(true) {
            // Nothing it changed (it was blocked, say): nothing to review.
            let changed = p.touched.as_ref().is_some_and(|t| !t.is_empty()) || p.commits.as_ref().is_some_and(|c| !c.is_empty());
            // Its checks failed: the outer loop has it fix that first; the
            // fixed work is reviewed when it passes.
            let failed = p.verdict.as_deref().is_some_and(|v| v.lines().any(|l| l.starts_with("[tests]") && l.contains("FAILED")));
            if !changed {
                self.system(at, &i18n::pick(format!("{} changed no file: no independent review of this report.", p.slug),
                    format!("{} 没有改任何文件：这份汇报不做独立审查。", p.slug)));
            } else if failed {
                self.system(at, &i18n::pick(format!("{}'s checks failed: it is reviewed once they pass.", p.slug),
                    format!("{} 的检查没通过：先修好，通过后再做独立审查。", p.slug)));
            } else if std::env::var_os("OCTOBUDDY_INNER_REVIEWER").is_some() {
                self.start_reviewer(at, &p);
            } else {
                self.ask_outer_review(at, &p);
            }
        }
    }

    /// The outer loop reviews it itself (the smartest model here), through a
    /// fresh-context subagent of its own: the report says so, with what to
    /// read.
    fn ask_outer_review(&mut self, at: SessionRef, of: &Peer) {
        let Some(session) = self.store.session(at).map(|s| s.id.clone()) else { return };
        let files = of.touched.clone().unwrap_or_default().join(", ");
        let commits = of.commits.clone().unwrap_or_default().join("; ");
        let base = of.base.clone().unwrap_or_else(|| "HEAD".into());
        // Its task, report and diff, in a file for the reviewer to read: the
        // outer loop only passes the path on.
        let report = self.rt.unreported.get(&session).and_then(|v| v.iter().find(|(id, _, _)| id == &of.id)).map(|(_, r, _)| r.clone()).unwrap_or_default();
        let diff = workspace::peer_diff(&of.dir, &base, &of.touched.clone().unwrap_or_default(), of.commits.as_deref().unwrap_or(&[]))
            .map(|d| d.patch).unwrap_or_else(|e| format!("(no diff: {e}; run `git diff {base}`)"));
        let brief = format!("# Review: {}\n\n## Its task\n\n{}\n\n## Its report\n\n{}\n\n## Its changes\n\n```diff\n{diff}\n```\n", of.slug, of.brief.trim(), report.trim());
        let path = self.write_review_brief(at, &of.slug, &brief);
        let read = path.map(|p| format!(" Its task, report and diff are in `{p}`: give your `reviewer` subagent that path."))
            .unwrap_or_else(|| format!(" Its diff: `git diff {base}` over {files}{}.", if commits.is_empty() { String::new() } else { format!(" (commits: {commits})") }));
        let note = format!("\n\n[independent review wanted] Review this slice before you accept it, with fresh eyes (INDEPENDENT REVIEW \
in your rules).{read}");
        if let Some(entry) = self.rt.unreported.get_mut(&session).and_then(|v| v.iter_mut().find(|(id, _, _)| id == &of.id)) {
            entry.1.push_str(&note);
        }
        self.system(at, &i18n::pick(format!("{} asked for an independent review: the outer loop does it, with a subagent of its own.", of.slug),
            format!("{} 需要独立审查：由 outer 用它自己的子 agent 来审。", of.slug)));
    }

    fn start_reviewer(&mut self, at: SessionRef, of: &Peer) {
        let Some(session) = self.store.session(at).cloned() else { return };
        let report = self.rt.unreported.get(&session.id)
            .and_then(|v| v.iter().find(|(id, _, _)| id == &of.id)).map(|(_, r, _)| r.clone()).unwrap_or_default();
        let base = of.base.clone().unwrap_or_else(|| "HEAD".into());
        let files = of.touched.clone().unwrap_or_default();
        let diff = workspace::peer_diff(&of.dir, &base, &files, of.commits.as_deref().unwrap_or(&[]))
            .map(|d| d.patch).unwrap_or_else(|e| format!("(no diff: {e})"));
        let brief = format!("{RUBRIC}\n\n## Its task\n\n{}\n\n## Its report\n\n{}\n\n## Its changes\n\n```diff\n{diff}\n```\n",
            of.brief.trim(), report.trim());
        let taken: Vec<String> = session.peers().iter().map(|p| p.slug.clone()).collect();
        let mut slug = format!("{}-review", of.slug);
        let mut n = 2;
        while taken.contains(&slug) {
            slug = format!("{}-review-{n}", of.slug);
            n += 1;
        }
        let id = model::new_id("w");
        let mut reviewer = of.clone();
        reviewer.id = id.clone();
        reviewer.slug = slug.clone();
        reviewer.role = Some("reviewer".into());
        reviewer.brief = brief.clone();
        reviewer.status = "queued".into();
        reviewer.started_at = now_secs();
        reviewer.finished_at = None;
        reviewer.activity = Some(i18n::t("preparing", "准备中").into());
        reviewer.result = None;
        reviewer.session_key = Some(format!("local:octobuddy:{id}"));
        reviewer.log = None;
        reviewer.contract = None;
        reviewer.usage = None;
        reviewer.check = None;
        reviewer.verdict = None;
        reviewer.review = None;
        reviewer.touched = None;
        reviewer.commits = None;
        reviewer.subagents = None;
        reviewer.estimate = Some(1.0);
        reviewer.rounds_used = None;
        reviewer.budget = None;
        reviewer.over_budget = None;
        reviewer.flow = None;
        reviewer.joined_from = None;
        reviewer.accepted = None;
        reviewer.review_wanted = None;
        reviewer.reviews_for = Some(of.id.clone());
        reviewer.claude_session = None;
        reviewer.specs = None;
        if let Some(s) = self.store.session_mut(at) {
            s.peers_mut().push(reviewer);
        }
        // Its report waits for the review.
        self.rt.checking.insert(of.id.clone());
        let mut line = Line::default();
        line.queue.push_back(Delivery::new(From::Lead, brief));
        self.rt.lines.insert(id.clone(), line);
        self.system(at, &i18n::pick(format!("{slug} reviews {}'s work before the outer loop sees it.", of.slug),
            format!("{slug} 先独立审查 {} 的工作，再交给 outer。", of.slug)));
        events::post(&self.rt.inbox, LoopEvent::PeerReady { session: session.id.clone(), peer: id, dir: of.dir.clone(), branch: None });
    }

    fn reviewer_done(&mut self, at: SessionRef, reviewer: &Peer, of: &str) {
        let Some(session_id) = self.store.session(at).map(|s| s.id.clone()) else { return };
        let entries = self.rt.unreported.entry(session_id).or_default();
        let findings = entries.iter().position(|(id, _, _)| id == &reviewer.id).map(|i| entries.remove(i).1).unwrap_or_default();
        let findings = format!("\n\n[independent review by {}]\n{}", reviewer.slug, findings.trim());
        match entries.iter_mut().find(|(id, _, _)| id == of) {
            Some((_, report, _)) => report.push_str(&findings),
            None => entries.push((of.to_string(), findings, true)),
        }
        self.rt.checking.remove(of);
        // Used once: it takes no more work.
        self.close_peer(&reviewer.id, "reviewed");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_outer_reviewer_is_cheap_and_read_only() {
        let v: serde_json::Value = serde_json::from_str(&super::reviewer_agent()).unwrap();
        let r = &v["reviewer"];
        assert_eq!(r["model"], "sonnet");
        assert_eq!(r["tools"], serde_json::json!(["Read", "Glob", "Grep"]));
        assert!(r["prompt"].as_str().unwrap().contains("P0"));
        assert!(!r["prompt"].as_str().unwrap().contains("report block"));
    }
}
