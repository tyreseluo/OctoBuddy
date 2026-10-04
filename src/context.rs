//! What an inner loop may read when its task needs more than its brief:
//! the outer loop's decisions and its siblings' reports, as files in the
//! directory they share (`.octobuddy/context/<session>/`, kept out of git).
//! OctoBuddy rewrites them when a round starts, a report comes in and the
//! outer loop answers; the inner loops read them only when they need to.
use crate::handoff;
use crate::model::{Peer, Role, SessionRef};
use crate::OctoBuddyView;
use std::path::{Path, PathBuf};

/// The context directory of a session, relative to the directory the loops share.
pub fn relative(session: &str) -> String {
    format!(".octobuddy/context/{session}")
}

impl OctoBuddyView {
    fn context_dir(&self, at: SessionRef) -> Option<PathBuf> {
        let session = self.store.session(at)?;
        let base = session.work_dir.clone().unwrap_or_else(|| self.store.projects[at.0].path.clone());
        Some(PathBuf::from(base).join(relative(&session.id)))
    }

    /// What an inner loop's first message carries beyond its card, so it
    /// does not go and read it: its own files' map, the excerpts its lead
    /// already read (as they are now), the memory that bears on the request.
    pub(crate) fn handoff_extra(&self, p: &Peer, context: &str) -> String {
        let dir = Path::new(&p.dir);
        let mut out = String::new();
        let map = handoff::file_map(dir, &handoff::card_files(&p.brief));
        if !map.is_empty() {
            out.push_str(&format!("\nYOUR FILES (what is defined where: read only the ranges you need)\n{map}"));
        }
        if let Some(reads) = p.reads.as_ref().filter(|r| !r.is_empty()) {
            let excerpts = handoff::excerpts(dir, reads);
            if !excerpts.is_empty() {
                out.push_str(&format!("\nALREADY READ BY THE LEAD (the lines as they are now: do not read them again){excerpts}"));
            }
        }
        let pack = std::fs::read_to_string(dir.join(context).join("pack.md")).unwrap_or_default();
        let memory = handoff::inner_memory(&pack);
        if !memory.is_empty() {
            out.push_str(&format!("\nMEMORY (from this project's earlier work)\n{memory}"));
        }
        out
    }

    /// The project's map (`handoff::KNOWLEDGE`), written again off the UI
    /// thread: its files with their definitions, and the work accepted.
    pub(crate) fn write_project_map(&self, at: SessionRef) {
        let Some(session) = self.store.session(at) else { return };
        let dir = PathBuf::from(session.work_dir.clone().unwrap_or_else(|| self.store.projects[at.0].path.clone()));
        let mut accepted: Vec<String> = Vec::new();
        for s in &self.store.projects[at.0].sessions {
            for p in s.peers().iter().filter(|p| p.review.as_deref().is_some_and(|r| r.starts_with("accept"))) {
                let goal = handoff_goal(&p.brief);
                let files = p.touched.as_ref().filter(|t| !t.is_empty()).map(|t| format!(" ({})", t.join(", "))).unwrap_or_default();
                accepted.push(format!("{} [{}]{files}: {goal}", p.slug, p.role()));
            }
        }
        std::thread::spawn(move || {
            let path = dir.join(handoff::KNOWLEDGE);
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, handoff::project_map(&dir, &accepted));
        });
    }

    /// The latest context pack (`pack.md`: memory hits, the docs index), for
    /// the inner loops to read before their task.
    pub(crate) fn write_pack(&self, at: SessionRef, pack: &str) {
        let Some(dir) = self.context_dir(at) else { return };
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("pack.md"), pack);
    }

    /// A review's brief (`reviews/<slug>.md`), for the outer loop's reviewer:
    /// its path relative to the directory the loops share.
    pub(crate) fn write_review_brief(&self, at: SessionRef, slug: &str, brief: &str) -> Option<String> {
        let dir = self.context_dir(at)?.join("reviews");
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::write(dir.join(format!("{slug}.md")), brief).ok()?;
        Some(format!("{}/reviews/{slug}.md", relative(&self.store.session(at)?.id)))
    }

    /// Writes the session's context files (quietly: they are a convenience).
    pub(crate) fn write_context(&self, at: SessionRef) {
        let (Some(dir), Some(session)) = (self.context_dir(at), self.store.session(at)) else { return };
        if session.peers().is_empty() {
            return;
        }
        let _ = std::fs::create_dir_all(dir.join("reports"));
        let asked = session.messages.iter().find(|m| m.role() == Role::User).map(|m| m.text.as_str()).unwrap_or("");
        let mut outer = format!("# The outer loop's decisions: session \"{}\"\n\n(OctoBuddy keeps this file; read it when your task needs the bigger picture.)\n\n## What the person asked\n\n{}\n\n## The plan\n\n", session.title, asked.trim());
        for p in session.peers() {
            let brief = p.brief.lines().find(|l| !l.trim().is_empty() && !l.starts_with("spec:") && !l.starts_with("---")).unwrap_or("").trim();
            let review = p.review.as_deref().map(|r| format!("; the outer loop: {r}")).unwrap_or_default();
            let wave = p.wave.map(|w| format!(", wave {w}")).unwrap_or_default();
            outer.push_str(&format!("- {} ({}{wave}, {}{review}): {brief}\n", p.slug, p.role(), p.status));
        }
        if let Some(last) = session.messages.iter().rev().find(|m| m.role() == Role::Lead && !m.text.trim().is_empty()) {
            let said: String = last.text.chars().take(4000).collect();
            outer.push_str(&format!("\n## What the outer loop said last\n\n{said}\n"));
        }
        let _ = std::fs::write(dir.join("outer.md"), outer);
        for p in session.peers() {
            let Some(report) = p.result.as_deref().filter(|r| !r.trim().is_empty()) else { continue };
            let text = format!("# {} ({}): {}\n\n{}\n", p.slug, p.role(), p.status, report.trim());
            let _ = std::fs::write(dir.join("reports").join(format!("{}.md", p.slug)), text);
        }
    }
}

/// A brief's goal in one line: its card's Goal (or a contract's Intent), else its first line.
fn handoff_goal(brief: &str) -> String {
    let mut lines = brief.lines().map(str::trim).skip_while(|l| {
        let t = l.trim_start_matches('#').trim().to_ascii_lowercase();
        !(l.starts_with('#') && (t == "goal" || t == "intent"))
    });
    let line = lines.nth(1).filter(|l| !l.is_empty() && !l.starts_with('#'))
        .or_else(|| brief.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("spec:") && !l.starts_with("---") && !l.starts_with('#')))
        .unwrap_or("");
    line.chars().take(120).collect()
}
