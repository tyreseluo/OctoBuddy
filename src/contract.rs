//! Briefs as agent-spec Task Contracts. When the lead writes a slice as a
//! contract (`spec: task` … Intent / Decisions / Boundaries / Completion
//! Criteria), OctoBuddy keeps it as a `.spec.md` outside the project, lints
//! it when the slice starts, and after the peer's work runs `agent-spec
//! lifecycle` on the files that peer changed: a machine verdict the lead
//! (who can only read) gets with the report.
//!
//! What agent-spec 1.4 checks here: its boundary layer compares the changed
//! files (relative paths, run in the work directory) with the contract's paths, only
//! with the default layers (naming `--layers` drops it); its test layer runs
//! `cargo test` selectors, so for other languages scenarios come back `skip`,
//! which is said as such.
use crate::workspace::{find_bin, search_path};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn is_contract(brief: &str) -> bool {
    brief.trim_start().starts_with("spec:")
}

/// Writes the contract for a slice; returns its path.
pub fn write(dir: &Path, slug: &str, brief: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{slug}.spec.md"));
    std::fs::write(&path, brief)?;
    Ok(path)
}

/// Keeps a slice's plain brief (not a contract) beside the contracts.
pub fn write_brief(dir: &Path, slug: &str, brief: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("{slug}.brief.md"));
    std::fs::write(&path, brief)?;
    Ok(path)
}

fn agent_spec(args: &[&str], cwd: Option<&Path>) -> Result<Value, String> {
    let mut cmd = Command::new(find_bin("agent-spec"));
    cmd.args(args).env("PATH", search_path());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let out = cmd.output().map_err(|err| format!("could not run agent-spec: {err}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(text.trim()).map_err(|_| {
        let err = String::from_utf8_lossy(&out.stderr);
        format!("agent-spec gave no report: {}", err.lines().next().unwrap_or("").trim())
    })
}

/// The contract's quality, in one line: score and its warnings.
pub fn lint(spec: &Path) -> Result<String, String> {
    let report = agent_spec(&["lint", &spec.to_string_lossy(), "--format", "json"], None)?;
    let score = report.pointer("/quality_score/overall").and_then(Value::as_f64).unwrap_or(0.0);
    let issues: Vec<String> = report.get("diagnostics").and_then(Value::as_array).cloned().unwrap_or_default().iter()
        .filter_map(|d| d.get("message").and_then(Value::as_str).map(str::to_string)).collect();
    Ok(if issues.is_empty() {
        format!("quality {score:.2}, no warnings")
    } else {
        format!("quality {score:.2}, {} warning(s): {}", issues.len(), issues.join("; "))
    })
}

/// Runs the contract against the files one peer changed in `dir` (others
/// work there too, so it is its files, not the directory's changes).
pub fn check(spec: &Path, dir: &str, changes: &[String]) -> Result<String, String> {
    let spec_arg = spec.to_string_lossy().into_owned();
    let mut args: Vec<String> = vec!["lifecycle".into(), spec_arg, "--code".into(), dir.into(), "--format".into(), "json".into()];
    // Relative to the directory, which is the working directory: an
    // absolute path is not mapped back into the project and reads as outside
    // every boundary (checked with agent-spec 1.4).
    for f in changes {
        args.push("--change".into());
        args.push(f.clone());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let report = agent_spec(&refs, Some(Path::new(dir)))?;
    Ok(summarize(&report, changes.len()))
}

fn summarize(report: &Value, changed: usize) -> String {
    let results = report.pointer("/verification/results").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut lines = Vec::new();
    let (mut passed, mut failed, mut skipped) = (0, 0, 0);
    for r in &results {
        let name = r.get("scenario_name").and_then(Value::as_str).unwrap_or("");
        let verdict = r.get("verdict").and_then(Value::as_str).unwrap_or("");
        if name.starts_with("[boundaries]") {
            let bad: Vec<String> = r.get("step_results").and_then(Value::as_array).cloned().unwrap_or_default().iter()
                .filter(|s| s.get("verdict").and_then(Value::as_str) == Some("fail"))
                .map(|s| format!("{} ({})", s.get("step_text").and_then(Value::as_str).unwrap_or(""), s.get("reason").and_then(Value::as_str).unwrap_or("")))
                .collect();
            lines.push(if verdict == "pass" { "boundaries: pass".to_string() } else { format!("boundaries: FAIL: {}", bad.join("; ")) });
            continue;
        }
        if name.starts_with('[') {
            continue;
        }
        match verdict {
            "pass" => passed += 1,
            "fail" => failed += 1,
            _ => skipped += 1,
        }
    }
    if !lines.iter().any(|l| l.starts_with("boundaries")) {
        lines.push(format!("boundaries: not checked ({changed} changed file(s), no path boundaries matched)"));
    }
    let tests = if passed + failed == 0 && skipped > 0 {
        format!("scenarios: {skipped} not machine-verified (agent-spec binds tests through cargo test)")
    } else {
        format!("scenarios: {passed} passed, {failed} failed, {skipped} skipped")
    };
    lines.push(tests);
    format!("[agent-spec] {}", lines.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contracts_are_recognized() {
        assert!(is_contract("spec: task\nname: x\n---"));
        assert!(is_contract("\n  spec: task"));
        assert!(!is_contract("Add remainder to calc.py"));
    }

    #[test]
    fn verdicts_read_in_one_line() {
        let report: Value = serde_json::from_str(r#"{"verification":{"results":[
            {"scenario_name":"[boundaries] explicit change set respects declared paths","verdict":"fail","step_results":[
                {"step_text":"calc.py","verdict":"pass","reason":"matches allowed boundary `calc.py`"},
                {"step_text":"README.md","verdict":"fail","reason":"outside the allowed paths"}]},
            {"scenario_name":"remainder of positive numbers","verdict":"skip","step_results":[]}]}}"#).unwrap();
        assert_eq!(summarize(&report, 2), "[agent-spec] boundaries: FAIL: README.md (outside the allowed paths) · scenarios: 1 not machine-verified (agent-spec binds tests through cargo test)");
        let ok: Value = serde_json::from_str(r#"{"verification":{"results":[{"scenario_name":"a","verdict":"pass"},{"scenario_name":"b","verdict":"fail"}]}}"#).unwrap();
        assert_eq!(summarize(&ok, 0), "[agent-spec] boundaries: not checked (0 changed file(s), no path boundaries matched) · scenarios: 1 passed, 1 failed, 0 skipped");
    }
}
