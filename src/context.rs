//! What an inner loop may read when its task needs more than its brief:
//! the outer loop's decisions and its siblings' reports, as files in the
//! directory they share (`.octobuddy/context/<session>/`, kept out of git).
//! OctoBuddy rewrites them when a round starts, a report comes in and the
//! outer loop answers; the inner loops read them only when they need to.
use crate::model::{Role, SessionRef};
use crate::OctoBuddyView;
use std::path::PathBuf;

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
