//! An app project's app, running in OctoBuddy's window: OctoSense's runtime
//! for script apps (a Splash isolate under the app's manifest, as the shell's
//! card runner and App Hub's card-host run one), from the project's files as
//! they are, unsigned. Its storage is OctoBuddy's (`<data dir>/preview/<id>`),
//! never an installed app's.
//!
//! The source is checked in a throwaway isolate first: a live Splash keeps
//! its old view when a new body fails, so the errors are shown instead.
use makepad_widgets::*;
use octosense_app_policy::{admit_and_resolve_dir, AssetServer, HostLimits, RefuseAllSignatures};
use std::hash::{Hash, Hasher};
use std::path::Path;

/// The app as it runs.
pub struct Running {
    pub app_id: String,
    pub name: String,
    pub version: String,
    pub capabilities: Vec<String>,
    // Serves `{{assets}}` for as long as the app runs.
    _assets: AssetServer,
}

/// Runs the project's app in `splash`, replacing what ran there. `Err`: why
/// it cannot run (a refusal, or the script's errors), one line each.
pub fn start(cx: &mut Cx, splash: &SplashRef, project: &str) -> Result<Running, Vec<String>> {
    let one = |e: String| vec![e];
    let bundle = crate::app::bundle(project);
    let manifest = std::fs::read_to_string(bundle.join(octosense_app_policy::MANIFEST_FILE)).map_err(|e| one(format!("manifest.json: {e}")))?;
    let digest = octosense_app_policy::digest_dir(&bundle).map_err(one)?;
    // Edited while it runs, and unsigned: its digest is stamped in memory,
    // as card-host does for a system app in development.
    let mut value: serde_json::Value = serde_json::from_str(&manifest).map_err(|e| one(format!("manifest.json: {e}")))?;
    value["integrity"]["bundle_blake3"] = serde_json::Value::String(digest.clone());
    let limits = HostLimits::default().with_require_signature(false);
    let policy = admit_and_resolve_dir(&value.to_string(), &digest, &limits, &RefuseAllSignatures).map_err(one)?;
    let mut settings = policy.isolate_settings(&crate::model::data_dir().join("preview"));
    std::fs::create_dir_all(&settings.jail_root).map_err(|e| one(format!("the app's storage: {e}")))?;
    let assets = AssetServer::start_with_static(&bundle, &[]).map_err(one)?;
    settings.hosts.push(assets.allowlist_entry());
    let source = octosense_app_policy::script_source(&bundle, assets.origin())
        .ok_or_else(|| one("bundle/main.splash is missing".into()))?
        .map_err(one)?;
    let errors = validate_splash_body(cx, &source, settings.allow_net);
    if !errors.is_empty() {
        return Err(errors);
    }
    splash.set_text(cx, "");
    octosense_app_policy::splash_adapter::apply(splash, cx, &settings);
    splash.set_text(cx, &source);
    Ok(Running {
        app_id: policy.app_id.clone(),
        name: policy.display_name.clone(),
        version: policy.version.clone(),
        capabilities: policy.capabilities.iter().cloned().collect(),
        _assets: assets,
    })
}

/// Stops the app running in `splash`.
pub fn stop(cx: &mut Cx, splash: &SplashRef) {
    splash.set_text(cx, "");
}

/// What the bundle's files are now (their paths, sizes and times): a change
/// means a reload.
pub fn stamp(project: &str) -> u64 {
    fn walk(dir: &Path, h: &mut std::collections::hash_map::DefaultHasher) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            path.hash(h);
            if meta.is_dir() {
                walk(&path, h);
            } else {
                meta.len().hash(h);
                meta.modified().ok().hash(h);
            }
        }
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    walk(&crate::app::bundle(project), &mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changed_file_changes_the_stamp() {
        let dir = std::env::temp_dir().join(format!("octobuddy-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bundle")).unwrap();
        std::fs::write(dir.join("bundle/main.splash"), "View{}").unwrap();
        let project = dir.to_string_lossy().into_owned();
        let before = stamp(&project);
        assert_eq!(before, stamp(&project));
        std::fs::write(dir.join("bundle/main.splash"), "View{} // more").unwrap();
        assert_ne!(before, stamp(&project));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
