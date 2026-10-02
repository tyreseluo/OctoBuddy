//! Project memory, kept in mempal (the person's memory tool: one SQLite
//! store, a "wing" per project, local embeddings). The outer loop on Claude
//! Code gets mempal's tools and is told its project's wing; OctoBuddy itself
//! records how each accepted slice went against its estimate, so the next
//! estimate starts from this project's own history.
//!
//! Without mempal on the PATH nothing of this runs.
use crate::workspace::{find_bin, search_path};
use std::path::{Path, PathBuf};
use std::process::Command;

/// mempal, when it is installed.
pub fn mempal() -> Option<PathBuf> {
    let bin = find_bin("mempal");
    Path::new(&bin).is_file().then_some(bin)
}

/// The wing of a project: its folder's name, as mempal names projects.
pub fn wing(project: &str) -> String {
    Path::new(project).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "octobuddy".into())
}

/// `--mcp-config` for Claude Code: mempal's server over stdio.
pub fn mcp_config(bin: &Path) -> String {
    serde_json::json!({"mcpServers": {"mempal": {"command": bin, "args": ["serve", "--mcp"]}}}).to_string()
}

/// What the outer loop is told about its project's memory.
pub fn outer_rules(project: &str) -> String {
    let wing = wing(project);
    format!("\n\nPROJECT MEMORY. This project's memory is mempal's wing \"{wing}\" (tools mempal_search, mempal_ingest; \
always pass wing \"{wing}\"). OctoBuddy searches it for you with each request's words (MEMORY in the CONTEXT \
ahead of the request: decisions, conventions, pitfalls, room \"calibration\", how accepted slices went \
against their estimates, and room \"rework\", what earlier rounds did again and why: plan around it); search it \
yourself only for what that lacks. When you settle a decision or a convention, or learn a pitfall, save it in one \
or two sentences with its reason (room \"decisions\", \"conventions\" or \"pitfalls\"). Do not save what the code \
or git history already says. Sessions the person archived are in room \"archive\": what was asked, answered and done \
there.")
}

/// Saves `text` in the project's wing, in `room`, in the background (a
/// file under `.octobuddy/memory`, which mempal ingests).
pub fn remember(project: &str, room: &str, text: String) {
    let Some(bin) = mempal() else { return };
    let (project, room) = (project.to_string(), room.to_string());
    std::thread::spawn(move || {
        let dir = Path::new(&project).join(".octobuddy/memory");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let file = dir.join(format!("{stamp}-{room}.md"));
        if std::fs::write(&file, text).is_err() {
            return;
        }
        let _ = Command::new(bin).arg("ingest").arg(&file).args(["--wing", &wing(&project), "--room", &room])
            .env("PATH", search_path()).current_dir(&project).output();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_is_the_wing_of_its_folder() {
        assert_eq!(wing("/home/me/tip-calc"), "tip-calc");
        let rules = outer_rules("/home/me/tip-calc");
        assert!(rules.contains("wing \"tip-calc\"") && rules.contains("calibration"));
        let config: serde_json::Value = serde_json::from_str(&mcp_config(Path::new("/bin/mempal"))).unwrap();
        assert_eq!(config["mcpServers"]["mempal"]["args"][1], "--mcp");
    }
}
