//! The sidebar's right-click menu and what it does to projects and
//! sessions: rename, pin, archive (the session's gist saved in the project's
//! memory, mempal's room "archive", so an outer loop can still find what was
//! decided there), remove, and move a session between the chats and a
//! project (Cindy's "Move to project"): its Claude transcript moves with it,
//! else what it said is handed to its agent with the next message.
use crate::model::{self, Role, Session, SessionRef};
use crate::{i18n, OctoBuddyView, Page, Row};
use makepad_widgets::*;

/// The menu's "move to" rows (`s_mv0`…`s_mv5`).
const MOVE_ROWS: [LiveId; 6] = [live_id!(s_mv0), live_id!(s_mv1), live_id!(s_mv2), live_id!(s_mv3), live_id!(s_mv4), live_id!(s_mv5)];

/// Claude Code keeps a folder's transcripts under `~/.claude/projects/<the
/// folder's path, every non-alphanumeric character a dash>`.
fn claude_dir(cwd: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME")?;
    let name: String = cwd.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    Some(std::path::PathBuf::from(home).join(".claude/projects").join(name))
}

/// Copies a Claude session's transcript to where it now works, so `--resume`
/// finds it there. Whether it could.
fn move_transcript(from: &str, to: &str, session: &str) -> bool {
    let (Some(a), Some(b)) = (claude_dir(from), claude_dir(to)) else { return false };
    let file = format!("{session}.jsonl");
    let source = a.join(&file);
    source.is_file() && std::fs::create_dir_all(&b).is_ok() && std::fs::copy(&source, b.join(&file)).is_ok()
}

/// What a session said, short, for an agent that starts afresh.
fn gist(s: &Session) -> String {
    let lines: Vec<String> = s.messages.iter().filter(|m| matches!(m.role(), Role::User | Role::Lead) && !m.text.trim().is_empty())
        .rev().take(12).collect::<Vec<_>>().into_iter().rev()
        .map(|m| {
            let who = if m.role() == Role::User { "The person" } else { "You" };
            let text: String = m.text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(500).collect();
            format!("{who}: {text}")
        }).collect();
    lines.join("\n")
}

/// A session's note for the project's memory when it is archived.
fn archive_note(project: &str, s: &Session) -> String {
    let asked = s.messages.iter().find(|m| m.role() == Role::User).map(|m| m.text.chars().take(400).collect::<String>()).unwrap_or_default();
    let answer = s.messages.iter().rev().find(|m| m.role() == Role::Lead && !m.text.trim().is_empty())
        .map(|m| m.text.chars().take(800).collect::<String>()).unwrap_or_default();
    let mut note = format!("Archived session \"{}\" of {project}.\nAsked: {asked}\nLast answer: {answer}", s.title);
    let peers: Vec<String> = s.peers().iter().map(|p| {
        let review = p.review.as_deref().map(|r| r.split(':').next().unwrap_or("").trim().to_string()).unwrap_or_default();
        let commits = p.commits.as_ref().map(|c| c.join("; ")).unwrap_or_default();
        format!("- {} ({}): {}{}{}", p.slug, p.role(), p.status, if review.is_empty() { String::new() } else { format!(", {review}") },
            if commits.is_empty() { String::new() } else { format!(", commits: {commits}") })
    }).collect();
    if !peers.is_empty() {
        note.push_str(&format!("\nInner loops:\n{}", peers.join("\n")));
    }
    note
}

impl OctoBuddyView {
    /// Fills the sidebar's menu for its row, at the pointer, or hides it.
    pub(crate) fn sync_side_menu(&mut self, cx: &mut Cx) {
        let menu = self.view.view(cx, ids!(side_menu));
        let Some((row, at)) = self.side_menu else {
            menu.set_visible(cx, false);
            return;
        };
        let t = i18n::t;
        let mut moves: Vec<String> = Vec::new();
        let (title, pin, archive, to_chats, remove) = match row {
            Row::Project(pi) | Row::ArchivedProject(pi) => {
                let p = &self.store.projects[pi];
                (p.name.clone(), (!p.is_archived()).then(|| if p.is_pinned() { t("Unpin", "取消置顶") } else { t("Pin to the top", "置顶") }),
                    if p.is_archived() { t("Restore from the archive", "取消归档") } else { t("Archive", "归档") }, false,
                    t("Remove from the list…", "从列表移除…"))
            }
            Row::Session(s) | Row::ArchivedSession(s) => {
                let Some(session) = self.store.session(s) else { self.side_menu = None; menu.set_visible(cx, false); return };
                let plain = self.store.is_plain(s);
                // A chat goes to a project; a project's session without inner loops, to the chats.
                if plain {
                    moves = self.store.projects.iter().filter(|p| !p.is_chats() && !p.is_archived()).take(MOVE_ROWS.len()).map(|p| p.name.clone()).collect();
                }
                let movable = !plain && session.peers().iter().all(|p| p.status == "closed");
                (session.title.clone(), (!session.is_archived()).then(|| if session.is_pinned() { t("Unpin", "取消置顶") } else { t("Pin to the top", "置顶") }),
                    if session.is_archived() { t("Restore from the archive", "取消归档") } else { t("Archive", "归档") }, movable,
                    if plain { t("Delete this chat…", "删除这个对话…") } else { t("Delete this session…", "删除这个会话…") })
            }
            _ => {
                self.side_menu = None;
                menu.set_visible(cx, false);
                return;
            }
        };
        self.view.label(cx, ids!(side_title)).set_text(cx, &title.chars().take(30).collect::<String>());
        let set = |view: &mut Self, cx: &mut Cx, id: &[LiveId], text: Option<&str>| {
            let b = view.view.button(cx, id);
            b.set_visible(cx, text.is_some());
            if let Some(text) = text {
                b.set_text(cx, text);
            }
        };
        set(self, cx, ids!(s_rename), Some(t("Rename…", "重命名…")));
        set(self, cx, ids!(s_pin), pin);
        set(self, cx, ids!(s_archive), Some(archive));
        set(self, cx, ids!(s_to_chats), to_chats.then(|| t("Move to Chats", "移到对话")));
        set(self, cx, ids!(s_remove), Some(remove));
        self.view.label(cx, ids!(s_move_title)).set_visible(cx, !moves.is_empty());
        self.view.label(cx, ids!(s_move_title)).set_text(cx, t("Move to a project:", "移到项目："));
        for (i, id) in MOVE_ROWS.iter().enumerate() {
            set(self, cx, &[*id], moves.get(i).map(String::as_str));
        }
        self.side_moves = moves;
        menu.set_visible(cx, true);
        // At the pointer, inside the window.
        let window = self.view.area().rect(cx);
        let rows = 5.0 + self.side_moves.len() as f64 + if self.side_moves.is_empty() { 0.0 } else { 1.0 };
        // Its rows, the title, the move heading and its padding; corrected
        // once it is drawn (see `place`).
        let h = 40.0 + rows * 28.0;
        let pos = crate::place(at, dvec2(220.0, h), window);
        let mut w = self.view.widget(cx, ids!(side_menu));
        script_apply_eval!(cx, w, { abs_pos: #(pos) });
    }

    /// A row of the menu, the rename dialog's buttons.
    pub(crate) fn handle_side_menu(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.renaming.is_some() {
            let input = self.view.text_input(cx, ids!(rename_input));
            if self.view.button(cx, ids!(rename_ok)).clicked(actions) || input.returned(actions).is_some() {
                let name = input.text().trim().to_string();
                if let (Some(key), false) = (self.renaming.take(), name.is_empty()) {
                    self.rename(&key, name);
                }
                self.renaming = None;
                self.save();
                self.relayout(cx);
            }
            if self.view.button(cx, ids!(rename_cancel)).clicked(actions) {
                self.renaming = None;
                self.relayout(cx);
            }
        }
        let Some((row, _)) = self.side_menu else { return };
        let picked = [ids!(s_rename), ids!(s_pin), ids!(s_archive), ids!(s_to_chats), ids!(s_remove)].into_iter()
            .position(|id| self.view.button(cx, id).clicked(actions));
        let moved = MOVE_ROWS.iter().position(|id| self.view.button(cx, &[*id]).clicked(actions));
        if picked.is_none() && moved.is_none() {
            return;
        }
        self.side_menu = None;
        let key = match row {
            Row::Project(pi) | Row::ArchivedProject(pi) => format!("p:{}", self.store.projects[pi].id),
            Row::Session(at) | Row::ArchivedSession(at) => format!("s:{}", self.store.session(at).map(|s| s.id.clone()).unwrap_or_default()),
            _ => return,
        };
        match (picked, moved) {
            (Some(0), _) => {
                let current = match row {
                    Row::Project(pi) | Row::ArchivedProject(pi) => self.store.projects[pi].name.clone(),
                    Row::Session(at) | Row::ArchivedSession(at) => self.store.session(at).map(|s| s.title.clone()).unwrap_or_default(),
                    _ => String::new(),
                };
                let what = if key.starts_with("p:") { i18n::t("Rename the project", "重命名项目") } else { i18n::t("Rename", "重命名") };
                self.view.label(cx, ids!(rename_title)).set_text(cx, what);
                self.view.button(cx, ids!(rename_ok)).set_text(cx, i18n::t("Rename", "确定"));
                self.view.button(cx, ids!(rename_cancel)).set_text(cx, i18n::t("Cancel", "取消"));
                let input = self.view.text_input(cx, ids!(rename_input));
                input.set_text(cx, &current);
                self.renaming = Some(key);
                self.relayout(cx);
                self.view.text_input(cx, ids!(rename_input)).set_key_focus(cx);
                return;
            }
            (Some(1), _) => self.toggle_pin(&key),
            (Some(2), _) => self.toggle_archive(&key),
            (Some(3), _) => {
                let chats = self.store.chats();
                if let Row::Session(at) | Row::ArchivedSession(at) = row {
                    self.move_session_to(cx, at, chats);
                }
            }
            (Some(_), _) => {
                // Removing a project, deleting a session: asked first.
                self.confirm_delete = Some(if let Some(id) = key.strip_prefix("p:") { format!("proj:{id}") } else { key });
            }
            (None, Some(i)) => {
                let name = self.side_moves.get(i).cloned().unwrap_or_default();
                let to = self.store.projects.iter().position(|p| !p.is_chats() && p.name == name);
                if let (Some(to), Row::Session(at) | Row::ArchivedSession(at)) = (to, row) {
                    self.move_session_to(cx, at, to);
                }
            }
            _ => {}
        }
        self.save();
        self.relayout(cx);
    }

    fn rename(&mut self, key: &str, name: String) {
        if let Some(id) = key.strip_prefix("p:") {
            if let Some(p) = self.store.projects.iter_mut().find(|p| p.id == id) {
                p.name = name;
            }
        } else if let Some(id) = key.strip_prefix("s:") {
            if let Some(s) = self.store.projects.iter_mut().flat_map(|p| p.sessions.iter_mut()).find(|s| s.id == id) {
                s.title = name;
            }
        }
    }

    fn toggle_pin(&mut self, key: &str) {
        if let Some(id) = key.strip_prefix("p:") {
            if let Some(p) = self.store.projects.iter_mut().find(|p| p.id == id) {
                p.pinned = (!p.is_pinned()).then_some(true);
            }
        } else if let Some(id) = key.strip_prefix("s:") {
            if let Some(s) = self.store.projects.iter_mut().flat_map(|p| p.sessions.iter_mut()).find(|s| s.id == id) {
                s.pinned = (!s.is_pinned()).then_some(true);
            }
        }
    }

    /// Archives (or restores) a project or a session. Archived, a session's
    /// gist goes to its project's memory (mempal, room "archive"), where the
    /// outer loop of a later session finds it.
    fn toggle_archive(&mut self, key: &str) {
        if let Some(id) = key.strip_prefix("p:") {
            let Some(pi) = self.store.projects.iter().position(|p| p.id == id) else { return };
            let archive = !self.store.projects[pi].is_archived();
            let project = self.store.projects[pi].clone();
            if archive {
                for s in project.sessions.iter().filter(|s| !s.is_detached() && !s.is_archived() && !s.messages.is_empty()) {
                    crate::memory::remember(&project.path, "archive", archive_note(&project.name, s));
                }
            }
            self.store.projects[pi].archived = archive.then_some(true);
            self.store.projects[pi].pinned = None;
        } else if let Some(id) = key.strip_prefix("s:") {
            let Some(at) = self.store.find_session(id) else { return };
            let project = self.store.projects[at.0].clone();
            let s = &project.sessions[at.1];
            let archive = !s.is_archived();
            if archive && !s.messages.is_empty() {
                let name = if project.is_chats() { "the chats".to_string() } else { project.name.clone() };
                crate::memory::remember(&project.path, "archive", archive_note(&name, s));
            }
            let s = self.store.session_mut(at).unwrap();
            s.archived = archive.then_some(true);
            s.pinned = None;
        }
    }

    /// Removes a project from the list (its folder stays): its sessions go,
    /// their loops stopped.
    pub(crate) fn remove_project(&mut self, id: &str) {
        let Some(pi) = self.store.projects.iter().position(|p| p.id == id && !p.is_chats()) else { return };
        for si in (0..self.store.projects[pi].sessions.len()).rev() {
            self.delete_session((pi, si));
        }
        self.store.projects.remove(pi);
        self.selected = None;
        self.flow_open = None;
        self.page = Page::Chat;
    }

    /// Moves a session to another project: a chat into a project, or a
    /// project's session (without inner loops) to the chats. It works there
    /// from now on; its conversation goes with it.
    pub(crate) fn move_session_to(&mut self, cx: &mut Cx, at: SessionRef, to: usize) {
        let Some(session) = self.store.session(at).cloned() else { return };
        if self.session_busy(at) {
            self.say(cx, i18n::t("It is at work: move it when it has finished (or stop it).", "它还在工作：等它做完（或先停下）再移动。"));
            return;
        }
        let from_plain = self.store.is_plain(at);
        let to_plain = self.store.projects.get(to).is_some_and(|p| p.is_chats());
        let old_cwd = session.work_dir.clone();
        // Its processes belong to where it was.
        self.rt.leads.remove(&session.id);
        self.octos_outer_stop(&session.id);
        self.rt.octos_outers.remove(&session.id);
        self.rt.system_chats.remove(&session.id);
        let Some(new_at) = self.store.move_session(at, to) else { return };
        if let Some(s) = self.store.session_mut(new_at) {
            s.work_dir = None;
            s.work_branch = None;
            s.worktree = None;
            // OctoSense's agent works in no project: a project's session talks to Claude Code.
            if !to_plain && s.engine() == crate::system_chat::ENGINE {
                s.outer = Some(model::OuterPick { engine: "claude".into(), model: None });
                s.lead_session = None;
            }
        }
        self.selected = Some(new_at);
        let new_cwd = self.work_dir(new_at).ok();
        // Claude resumes where it now works; anything else starts afresh, told what was said.
        let resumed = match (session.engine(), session.lead_session.as_deref(), old_cwd.as_deref(), new_cwd.as_deref()) {
            ("claude", Some(id), Some(from), Some(to)) => move_transcript(from, to, id),
            _ => false,
        };
        if let Some(s) = self.store.session_mut(new_at) {
            if !resumed {
                s.lead_session = None;
                let said = gist(&session);
                s.carried = (!said.is_empty()).then_some(said);
            }
        }
        let place = if to_plain { i18n::t("the chats", "对话").to_string() } else { self.store.projects[to].name.clone() };
        let how = match (resumed, from_plain) {
            (true, _) => i18n::t("its agent resumes its conversation there", "agent 在那里接着原来的对话"),
            (false, _) => i18n::t("its agent starts afresh there, told what was said so far", "agent 在那里重新开始，会先得知之前的对话"),
        };
        self.system(new_at, &i18n::pick(format!("Moved to {place}: it works there now; {how}."), format!("已移到「{place}」：之后在那里工作；{how}。")));
    }
}
