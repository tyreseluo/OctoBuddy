//! What OctoBuddy learned from its own runs, kept across projects: the
//! outer loop saves a lesson (`octobuddy_learn`) when a round it ran did
//! work again (see `save_rework`), and every later run starts from it — the
//! Splash ones in each app's cookbook copy, the orchestration ones in the
//! outer loop's rules. So a mistake made once is not made again.
//!
//! One file per topic under `<data>/lessons/`, a line per lesson: the rule,
//! what showed it, when and where.
use std::path::PathBuf;

/// The topics a lesson can be about.
pub const TOPICS: [&str; 2] = ["splash", "orchestration"];

/// A lesson file keeps its newest lessons, this many.
const KEEP: usize = 80;

fn root() -> PathBuf {
    crate::model::data_dir().join("lessons")
}

pub fn path(topic: &str) -> PathBuf {
    root().join(format!("{topic}.md"))
}

/// The lessons of `topic`, a line each (none: empty).
pub fn read(topic: &str) -> Vec<String> {
    read_in(&root(), topic)
}

fn read_in(root: &std::path::Path, topic: &str) -> Vec<String> {
    std::fs::read_to_string(root.join(format!("{topic}.md"))).unwrap_or_default().lines()
        .filter(|l| l.starts_with("- ")).map(String::from).collect()
}

/// Saves `lesson` under `topic` (`evidence`: what showed it; `project`: where).
/// Says what it did; one already kept (the same rule) is not kept twice.
pub fn add(topic: &str, lesson: &str, evidence: &str, project: &str) -> Result<String, String> {
    add_in(&root(), topic, lesson, evidence, project)
}

fn add_in(root: &std::path::Path, topic: &str, lesson: &str, evidence: &str, project: &str) -> Result<String, String> {
    if !TOPICS.contains(&topic) {
        return Err(format!("topic is one of: {}", TOPICS.join(", ")));
    }
    let lesson = lesson.split_whitespace().collect::<Vec<_>>().join(" ");
    if lesson.len() < 12 {
        return Err("say the lesson as a rule an agent can follow (a sentence)".into());
    }
    let mut lines = read_in(root, topic);
    let key = |l: &str| l.trim_start_matches("- ").split(" — ").next().unwrap_or("").to_lowercase();
    if lines.iter().any(|l| key(l) == lesson.to_lowercase()) {
        return Ok(format!("already kept ({topic})"));
    }
    let date = crate::model::now_secs() / 86_400;
    let wing = crate::memory::wing(project);
    let evidence = evidence.split_whitespace().collect::<Vec<_>>().join(" ");
    lines.push(format!("- {lesson} — {evidence} (day {date}, {wing})"));
    if lines.len() > KEEP {
        lines.drain(..lines.len() - KEEP);
    }
    let file = root.join(format!("{topic}.md"));
    std::fs::create_dir_all(file.parent().unwrap_or(&file)).map_err(|e| e.to_string())?;
    let head = match topic {
        "splash" => "# What OctoBuddy learned about Splash and the app runtime (verified with its probe)",
        _ => "# What OctoBuddy learned about planning, splitting and picking agents",
    };
    std::fs::write(&file, format!("{head}\n\n{}\n", lines.join("\n"))).map_err(|e| e.to_string())?;
    Ok(format!("kept ({topic}, {} in all)", lines.len()))
}

/// The lessons of `topic` as a section to add to a reference, or nothing.
pub fn section(topic: &str, title: &str) -> String {
    let lines = read(topic);
    if lines.is_empty() {
        return String::new();
    }
    format!("\n\n{title}\n\n{}\n", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lesson_is_kept_once_and_read_back() {
        let root = std::env::temp_dir().join(format!("octobuddy-lessons-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let rule = "An on_render container is empty until its first .render(): render it in boot()";
        assert!(add_in(&root, "splash", rule, "probe: ui.x not found before, found after", "/p/app").unwrap().starts_with("kept"));
        assert_eq!(add_in(&root, "splash", &rule.to_uppercase(), "again", "/p/app").unwrap(), "already kept (splash)");
        assert!(add_in(&root, "nope", rule, "", "/p/app").is_err(), "a known topic");
        assert!(add_in(&root, "splash", "short", "", "/p/app").is_err(), "a rule, not a word");
        let lines = read_in(&root, "splash");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("render it in boot()") && lines[0].contains("app"));
        let _ = std::fs::remove_dir_all(root);
    }
}
