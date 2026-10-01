//! The project on disk: its git repository, each peer's clone and branch,
//! and where the loops' programs are found.
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn git(dir: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("PATH", search_path()).output()
        .map_err(|err| format!("could not run git: {err}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// Files the agents leave in a working directory that are not the work.
pub const AGENT_FILES: &[&str] = &[".octos-workspace.toml", ".octos/", "__pycache__/", ".pytest_cache/", ".octobuddy/"];

/// Whether a changed path is one of `AGENT_FILES`.
pub fn is_agent_file(path: &str) -> bool {
    AGENT_FILES.iter().any(|rule| {
        let rule = rule.trim_end_matches('/');
        path == rule || path.starts_with(&format!("{rule}/")) || path.contains(&format!("/{rule}/")) || path.ends_with(&format!("/{rule}"))
    })
}

/// A git worktree of the project for one session, on a new branch from the
/// project's current commit. The session's outer and inner loops all work in it.
pub fn add_worktree(project: &str, dir: &Path, branch: &str) -> Result<(), String> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    git(project, &["worktree", "add", "--quiet", "-b", branch, &dir.to_string_lossy()])?;
    exclude_agent_files(project);
    Ok(())
}

/// Files changed and not committed in `dir`, the agents' own left out.
pub fn uncommitted(dir: &str) -> Vec<String> {
    git(dir, &["status", "--porcelain"]).unwrap_or_default().lines()
        .map(|l| l.get(3..).unwrap_or("").trim().to_string())
        .filter(|f| !f.is_empty() && !is_agent_file(f))
        .collect()
}

/// Removes the session's worktree and its branch (its work is dropped).
pub fn remove_worktree(project: &str, dir: &str, branch: &str) -> Result<String, String> {
    if Path::new(dir).exists() {
        git(project, &["worktree", "remove", "--force", dir])?;
    }
    let _ = git(project, &["worktree", "prune"]);
    if git(project, &["rev-parse", "--verify", "-q", &format!("refs/heads/{branch}")]).is_ok() {
        git(project, &["branch", "-D", branch])?;
    }
    Ok(format!("Discarded the worktree and its branch {branch}."))
}

/// octos keeps its own files where it works: out of git's sight, so they
/// are never committed or counted as changes. Worktrees share the project's
/// `info/exclude`.
pub fn exclude_agent_files(project: &str) {
    let Ok(common) = git(project, &["rev-parse", "--path-format=absolute", "--git-common-dir"]) else { return };
    let exclude = Path::new(&common).join("info/exclude");
    let mut rules = std::fs::read_to_string(&exclude).unwrap_or_default();
    for rule in AGENT_FILES {
        if !rules.lines().any(|l| l.trim() == *rule) {
            rules.push_str(&format!("\n{rule}"));
        }
    }
    if let Some(parent) = exclude.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&exclude, format!("{}\n", rules.trim_end()));
}

/// Each file's content hash (git's blob id) as it is now in `dir`; files
/// that are gone are left out.
pub fn file_hashes(dir: &str, files: &[String]) -> Vec<(String, String)> {
    let present: Vec<&String> = files.iter().filter(|f| Path::new(dir).join(f).is_file()).collect();
    if present.is_empty() {
        return Vec::new();
    }
    let mut args = vec!["hash-object", "--"];
    args.extend(present.iter().map(|f| f.as_str()));
    match git(dir, &args) {
        Ok(out) => present.into_iter().cloned().zip(out.lines().map(str::to_string)).collect(),
        Err(_) => Vec::new(),
    }
}

/// What a peer's branch would bring into the project's current branch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BranchDiff {
    /// The project's branch it would merge into.
    pub into: String,
    /// Its commits past the merge base, newest first (`abc1234 subject`).
    pub commits: Vec<String>,
    /// `(added, deleted, path)`; `-` counts for binary files.
    pub files: Vec<(String, String, String)>,
    /// The patch, cut at `PATCH_LINES` lines.
    pub patch: String,
    /// Already in the project's branch (merged, or nothing new).
    pub merged: bool,
}

const PATCH_LINES: usize = 400;

/// The peer's branch against the project's current branch, read in the
/// project (the branch was fetched back there).
pub fn branch_diff(project: &str, branch: &str) -> Result<BranchDiff, String> {
    git(project, &["rev-parse", "--verify", "-q", &format!("refs/heads/{branch}")])
        .map_err(|_| format!("the project has no branch {branch}"))?;
    let into = git(project, &["symbolic-ref", "--short", "-q", "HEAD"]).unwrap_or_else(|_| "HEAD".into());
    let range = format!("HEAD...{branch}");
    let commits: Vec<String> = git(project, &["log", "--format=%h %s", &format!("HEAD..{branch}")])?
        .lines().map(str::to_string).collect();
    let files = git(project, &["diff", "--numstat", &range])?.lines().filter_map(|l| {
        let mut parts = l.splitn(3, '\t');
        Some((parts.next()?.to_string(), parts.next()?.to_string(), parts.next()?.to_string()))
    }).collect();
    let patch = cut_patch(&git(project, &["diff", "--no-color", &range])?, &format!("git diff {range} in the project"));
    Ok(BranchDiff { into, merged: commits.is_empty(), commits, files, patch })
}

/// Merges the peer's branch into the project's current branch. A merge
/// that conflicts is aborted, leaving the project as it was.
pub fn merge_branch(project: &str, branch: &str) -> Result<String, String> {
    let into = git(project, &["symbolic-ref", "--short", "-q", "HEAD"])
        .map_err(|_| "the project is not on a branch (detached HEAD): check out a branch first".to_string())?;
    let before = git(project, &["rev-parse", "HEAD"])?;
    match git(project, &["merge", "--no-edit", branch]) {
        Ok(_) => {
            let after = git(project, &["rev-parse", "--short", "HEAD"])?;
            let how = if git(project, &["rev-parse", "HEAD^2"]).is_ok() && git(project, &["rev-parse", "HEAD^1"]).ok().as_deref() == Some(before.as_str()) {
                "merge commit"
            } else {
                "fast-forward"
            };
            Ok(format!("Merged {branch} into {into} ({how}, now {after})."))
        }
        Err(err) => {
            if git(project, &["rev-parse", "-q", "--verify", "MERGE_HEAD"]).is_ok() {
                let conflicts = git(project, &["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default().replace('\n', ", ");
                let _ = git(project, &["merge", "--abort"]);
                Err(format!("{branch} conflicts with {into} in {conflicts}: merge aborted, nothing changed. Merge it by hand, or ask the outer loop to have it rebased on {into}."))
            } else {
                Err(format!("git refused to merge {branch} into {into}: {err}"))
            }
        }
    }
}

/// Commits exactly `files` in `dir` with `message`, as the project's own git
/// identity and through its hooks (never bypassed): what one inner loop
/// changed, while others work in the same directory. Says the commit made.
pub fn commit_files(dir: &str, files: &[String], message: &str) -> Result<String, String> {
    if files.is_empty() {
        return Err("no files to commit".into());
    }
    let mut add = vec!["add", "-A", "--"];
    add.extend(files.iter().map(String::as_str));
    git(dir, &add)?;
    let mut commit = vec!["commit", "-q", "-m", message, "--"];
    commit.extend(files.iter().map(String::as_str));
    git(dir, &commit).map_err(|err| format!("not committed: {err}"))?;
    git(dir, &["log", "-1", "--format=%h %s"])
}

/// What one inner loop changed since `base`: the diff of its own files
/// (committed and not), and OctoBuddy's commits of them.
pub fn peer_diff(dir: &str, base: &str, files: &[String], commits: &[String]) -> Result<BranchDiff, String> {
    let mut args = vec!["diff", "--numstat", base, "--"];
    args.extend(files.iter().map(String::as_str));
    let numstat = git(dir, &args)?;
    let files_changed = numstat.lines().filter_map(|l| {
        let mut parts = l.splitn(3, '\t');
        Some((parts.next()?.to_string(), parts.next()?.to_string(), parts.next()?.to_string()))
    }).collect();
    let mut args = vec!["diff", "--no-color", base, "--"];
    args.extend(files.iter().map(String::as_str));
    let patch = cut_patch(&git(dir, &args)?, &format!("git diff {base} -- <its files>"));
    Ok(BranchDiff { into: String::new(), commits: commits.to_vec(), files: files_changed, merged: false, patch })
}

fn cut_patch(patch: &str, whole: &str) -> String {
    let mut lines: Vec<&str> = patch.lines().take(PATCH_LINES + 1).collect();
    if lines.len() > PATCH_LINES {
        lines.truncate(PATCH_LINES);
        format!("{}\n… (cut at {PATCH_LINES} lines: {whole} shows all)", lines.join("\n"))
    } else {
        lines.join("\n")
    }
}

pub fn is_git_repo(dir: &str) -> bool {
    Command::new("git").arg("-C").arg(dir).args(["rev-parse", "--is-inside-work-tree"])
        .env("PATH", search_path()).output()
        .map(|o| o.status.success()).unwrap_or(false)
}

/// Where tools are looked for: the environment's PATH plus the usual
/// install places, since a shell started from the dock gets a short PATH.
pub fn search_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut dirs: Vec<String> = std::env::var("PATH").unwrap_or_default().split(':').map(String::from).collect();
    // OctoBuddy's own commands first (`octobuddy-app-check`), then what
    // the agents it keeps bring along (Codex's ripgrep).
    dirs.insert(0, crate::app::bin_dir().to_string_lossy().into_owned());
    for (i, extra) in crate::agents::extra_path().into_iter().enumerate() {
        dirs.insert(1 + i, extra.to_string_lossy().into_owned());
    }
    // Where people's own installs live when OctoSense was opened from the
    // Dock (no shell profile ran): Node (nvm's newest, Volta) for pi, pnpm's.
    let node = newest_nvm_node(&home).into_iter().chain([format!("{home}/.volta/bin"), format!("{home}/Library/pnpm")]);
    for extra in [format!("{home}/.local/bin"), format!("{home}/.cargo/bin"), "/opt/homebrew/bin".into(), "/usr/local/bin".into(), "/usr/bin".into(), "/bin".into()].into_iter().chain(node) {
        if !dirs.contains(&extra) {
            dirs.push(extra);
        }
    }
    dirs.join(":")
}

/// `~/.nvm/versions/node/<newest>/bin`, if nvm keeps any Node.
fn newest_nvm_node(home: &str) -> Option<String> {
    let version = |name: &str| -> Vec<u32> { name.trim_start_matches('v').split('.').map(|n| n.parse().unwrap_or(0)).collect() };
    let dir = Path::new(home).join(".nvm/versions/node");
    let newest = std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| dir.join(n).join("bin").is_dir()).max_by_key(|n| version(n))?;
    Some(dir.join(newest).join("bin").to_string_lossy().into_owned())
}

/// `OCTOBUDDY_<NAME>_BIN`, else OctoBuddy's own copy of an agent's program
/// (`agents::chosen`), else the first `name` on the search path, else the
/// bare name (so the spawn error names it).
pub fn find_bin(name: &str) -> PathBuf {
    if let Some(path) = std::env::var_os(format!("OCTOBUDDY_{}_BIN", name.to_uppercase())) {
        return PathBuf::from(path);
    }
    if let Some(path) = crate::agents::chosen(name) {
        return path;
    }
    search_path().split(':').map(|d| Path::new(d).join(name)).find(|p| p.is_file()).unwrap_or_else(|| PathBuf::from(name))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_changed_file_has_another_hash() {
        let dir = std::env::temp_dir().join(format!("octobuddy-hash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_string_lossy().into_owned();
        git(&d, &["init", "-q"]).unwrap();
        std::fs::write(dir.join("a.py"), "x = 1\n").unwrap();
        let files = vec!["a.py".to_string(), "gone.py".to_string()];
        let before = file_hashes(&d, &files);
        assert_eq!(before.len(), 1, "a missing file is left out");
        std::fs::write(dir.join("a.py"), "x = 2\n").unwrap();
        assert_ne!(before, file_hashes(&d, &files));
        let _ = std::fs::remove_dir_all(&dir);
    }

    use super::*;

    fn repo(root: &Path) -> String {
        let _ = std::fs::remove_dir_all(root);
        std::fs::create_dir_all(root).unwrap();
        let p = root.to_string_lossy().into_owned();
        for args in [&["init", "-q", "-b", "main"][..], &["config", "user.email", "t@example.invalid"], &["config", "user.name", "t"]] {
            git(&p, args).unwrap();
        }
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git(&p, &["add", "-A"]).unwrap();
        git(&p, &["commit", "-qm", "init"]).unwrap();
        p
    }

    #[test]
    fn each_loop_commits_only_its_own_files() {
        let root = std::env::temp_dir().join(format!("octobuddy-commit-{}", std::process::id()));
        let p = repo(&root);
        let base = git(&p, &["rev-parse", "HEAD"]).unwrap();
        exclude_agent_files(&p);
        std::fs::write(root.join(".octos-workspace.toml"), "x").unwrap();
        // Two loops at once: one wrote b.txt, the other is half-way through c.txt.
        std::fs::write(root.join("b.txt"), "b\n").unwrap();
        std::fs::write(root.join("c.txt"), "half\n").unwrap();
        assert_eq!(uncommitted(&p), vec!["b.txt".to_string(), "c.txt".to_string()], "agent files are not changes");
        let made = commit_files(&p, &["b.txt".into()], "feat: add b").unwrap();
        assert!(made.ends_with(" feat: add b"));
        assert_eq!(uncommitted(&p), vec!["c.txt".to_string()], "the other loop's file is left alone");
        let d = peer_diff(&p, &base, &["b.txt".into()], &[made]).unwrap();
        assert_eq!(d.files, vec![("1".into(), "0".into(), "b.txt".into())]);
        assert!(d.patch.contains("+b") && !d.patch.contains("half"));
        assert!(commit_files(&p, &[], "x").is_err());
        assert!(is_agent_file(".octos/x") && is_agent_file("pkg/__pycache__/a.pyc") && !is_agent_file("src/octos.rs"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_session_worktree_is_merged_once_or_discarded() {
        let root = std::env::temp_dir().join(format!("octobuddy-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let p = repo(&root.join("project"));
        let wt = root.join("wt");
        add_worktree(&p, &wt, "octobuddy/s1").unwrap();
        let w = wt.to_string_lossy().into_owned();
        std::fs::write(wt.join("b.txt"), "b\n").unwrap();
        commit_files(&w, &["b.txt".into()], "add b").unwrap();
        let d = branch_diff(&p, "octobuddy/s1").unwrap();
        assert_eq!((d.into.as_str(), d.commits.len(), d.merged), ("main", 1, false));
        assert!(merge_branch(&p, "octobuddy/s1").unwrap().contains("fast-forward"));
        assert!(branch_diff(&p, "octobuddy/s1").unwrap().merged);
        // Work goes on in the worktree after a merge while main moves too: a conflict is aborted.
        std::fs::write(wt.join("b.txt"), "worktree\n").unwrap();
        commit_files(&w, &["b.txt".into()], "change b").unwrap();
        std::fs::write(root.join("project/b.txt"), "main\n").unwrap();
        git(&p, &["commit", "-qam", "main b"]).unwrap();
        let err = merge_branch(&p, "octobuddy/s1").unwrap_err();
        assert!(err.contains("conflicts") && err.contains("b.txt"), "{err}");
        assert_eq!(git(&p, &["status", "--porcelain"]).unwrap(), "", "an aborted merge leaves nothing behind");
        assert!(remove_worktree(&p, &w, "octobuddy/s1").unwrap().starts_with("Discarded"));
        assert!(!wt.exists() && git(&p, &["rev-parse", "--verify", "-q", "refs/heads/octobuddy/s1"]).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
