//! OctoScript App Design Flow, bundled (`resources/design-flow`, its
//! `SOURCE.md`): the docs an OctoSense script app is planned with, its
//! `tools/octo` and its script-app template, written out under OctoBuddy's
//! data the first time they are needed. Used when no checkout of the flow
//! is found, so an app can be made, checked and submitted on a machine with
//! OctoBuddy alone (`tools/octo` still runs App Hub's `hub` and `card-host`).
use std::path::PathBuf;

/// The flow's commit these files are from.
pub const COMMIT: &str = "63d3dbda";

/// Each file: its path in the flow, its bytes.
const FILES: &[(&str, &[u8])] = &[
    ("AGENTS.md", include_bytes!("../../resources/design-flow/AGENTS.md")),
    ("LICENSE", include_bytes!("../../resources/design-flow/LICENSE")),
    ("NOTICE", include_bytes!("../../resources/design-flow/NOTICE")),
    ("SOURCE.md", include_bytes!("../../resources/design-flow/SOURCE.md")),
    ("docs/AI-SERVICES.md", include_bytes!("../../resources/design-flow/docs/AI-SERVICES.md")),
    ("docs/CAPABILITIES.md", include_bytes!("../../resources/design-flow/docs/CAPABILITIES.md")),
    ("docs/GLOSSARY.md", include_bytes!("../../resources/design-flow/docs/GLOSSARY.md")),
    ("docs/HOST-SERVICES.md", include_bytes!("../../resources/design-flow/docs/HOST-SERVICES.md")),
    ("docs/PUBLISHING.md", include_bytes!("../../resources/design-flow/docs/PUBLISHING.md")),
    ("docs/QUICKSTART.md", include_bytes!("../../resources/design-flow/docs/QUICKSTART.md")),
    ("docs/SCRIPT-API.md", include_bytes!("../../resources/design-flow/docs/SCRIPT-API.md")),
    ("flows/script-app/FLOW.md", include_bytes!("../../resources/design-flow/flows/script-app/FLOW.md")),
    ("templates/script-app/.gitignore", include_bytes!("../../resources/design-flow/templates/script-app/.gitignore")),
    ("templates/script-app/AGENTS.md", include_bytes!("../../resources/design-flow/templates/script-app/AGENTS.md")),
    ("templates/script-app/CLAUDE.md", include_bytes!("../../resources/design-flow/templates/script-app/CLAUDE.md")),
    ("templates/script-app/GEMINI.md", include_bytes!("../../resources/design-flow/templates/script-app/GEMINI.md")),
    ("templates/script-app/README.md", include_bytes!("../../resources/design-flow/templates/script-app/README.md")),
    ("templates/script-app/README.zh-CN.md", include_bytes!("../../resources/design-flow/templates/script-app/README.zh-CN.md")),
    ("templates/script-app/bundle/assets/icon.svg", include_bytes!("../../resources/design-flow/templates/script-app/bundle/assets/icon.svg")),
    ("templates/script-app/bundle/listing.json", include_bytes!("../../resources/design-flow/templates/script-app/bundle/listing.json")),
    ("templates/script-app/bundle/main.splash", include_bytes!("../../resources/design-flow/templates/script-app/bundle/main.splash")),
    ("templates/script-app/bundle/manifest.json", include_bytes!("../../resources/design-flow/templates/script-app/bundle/manifest.json")),
    ("tools/octo", include_bytes!("../../resources/design-flow/tools/octo")),
];

/// The bundled flow, written out (once per commit): its folder.
pub fn dir() -> Option<PathBuf> {
    let dir = crate::model::data_dir().join("design-flow").join(COMMIT);
    let done = dir.join(".complete");
    if !done.is_file() {
        for (path, bytes) in FILES {
            let to = dir.join(path);
            std::fs::create_dir_all(to.parent()?).ok()?;
            std::fs::write(&to, bytes).ok()?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir.join("tools/octo"), std::fs::Permissions::from_mode(0o755));
        }
        std::fs::write(&done, COMMIT).ok()?;
    }
    Some(dir)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_bundled_flow_has_what_octobuddy_uses() {
        let has = |p: &str| super::FILES.iter().any(|(path, _)| *path == p);
        for p in ["tools/octo", "templates/script-app/bundle/manifest.json", "templates/script-app/bundle/main.splash",
            "AGENTS.md", "flows/script-app/FLOW.md", "docs/SCRIPT-API.md", "docs/CAPABILITIES.md", "docs/PUBLISHING.md", "LICENSE", "NOTICE"] {
            assert!(has(p), "{p}");
        }
    }
}
