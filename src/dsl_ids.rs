//! A guard for the panels' wiring: every widget the `ids!` macro reaches
//! for must be a named instance — a `:=` definition — some DSL defines.
//! The disk panel once shipped with its whole interface referenced from
//! Rust and none of it in the DSL: a blank panel nothing could be done
//! about. This test catches that class of mistake at `cargo test`, before
//! the app runs.
//!
//! The DSL lives inside `script_mod! { … }` blocks across `src/`, so both
//! sides are read from the same sources: the names the `ids!` macro asks
//! for (each dotted segment of each argument) against every named-instance
//! definition those sources contain.

/// Names that look like `ids!` names but are made another way (a template
/// the DSL writes at runtime, a widget whose id comes from a variable).
/// Each entry says how the name is really defined; keep this short.
const DYNAMIC: &[&str] = &[
    // None yet: every ids! name today is a named instance in some DSL.
];

/// Every `.rs` file under `dir`, in a stable order (the failure message
/// lists what is missing, and a stable order keeps it readable).
fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).map(|it| it.flatten().collect()).unwrap_or_default();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            rust_files(&path, out);
        } else if entry.file_name().to_string_lossy().ends_with(".rs") {
            out.push(path);
        }
    }
}

/// The names the `ids!` macro uses: each dotted segment of each
/// comma-separated argument (a dotted path asks for every segment of it).
/// The macro takes no nested parentheses, so reading to the next closing
/// one is enough.
fn ids_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = 0;
    while let Some(open) = source[at..].find("ids!(").map(|i| i + at + 5) {
        let Some(close) = source[open..].find(')').map(|i| i + open) else { break };
        for part in source[open..close].split(',') {
            for segment in part.split('.') {
                let name = segment.trim();
                if !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                    names.push(name.to_string());
                }
            }
        }
        at = close + 1;
    }
    names
}

/// Every name the DSL defines as a named instance: an identifier right
/// before a `:=` (spaces allowed between the two).
fn defined_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut at = 0;
    while let Some(hit) = source[at..].find(":=").map(|i| i + at) {
        let tail = source[..hit].trim_end();
        let name: String = tail.chars().rev().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty()
            && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
            // The character before the name must not continue one (an id
            // never grows onto `.` or another letter's tail).
            && tail[..tail.len() - name.len()].chars().next().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        {
            names.push(name.chars().rev().collect());
        }
        at = hit + 2;
    }
    names
}

/// Every `ids!` name must be a named instance some DSL defines (or sit on
/// the `DYNAMIC` list, each with its reason).
#[test]
fn every_ids_name_is_defined_in_some_dsl() {
    let mut files = Vec::new();
    rust_files(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(), &mut files);
    assert!(!files.is_empty(), "the sources were found");

    let mut sources = Vec::new();
    for path in &files {
        sources.push((path.display().to_string(), std::fs::read_to_string(path).expect("a source file reads")));
    }
    let defined: std::collections::HashSet<String> = sources.iter().flat_map(|(_, text)| defined_names(text)).collect();
    let used: std::collections::BTreeSet<String> = sources.iter().flat_map(|(_, text)| ids_names(text)).collect();
    assert!(!defined.is_empty(), "the DSL named instances were found");
    assert!(used.len() > 400, "the ids! names were found (got {})", used.len());

    let missing: Vec<String> = used.into_iter().filter(|name| !DYNAMIC.contains(&name.as_str()) && !defined.contains(name.as_str())).collect();
    assert!(missing.is_empty(), "ids! names with no `name :=` in any DSL (add the widget, or say why on the DYNAMIC list):\n  {}", missing.join("\n  "));
}
