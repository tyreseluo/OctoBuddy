//! OctoBuddy's own AI providers, for when it runs on the host. Inside
//! OctoSense the shell's AI providers app keeps them and OctoBuddy only
//! reads them (`providers.rs`); on its own it keeps them itself, the way
//! Cindy does, in the same form, so everything that reads a profile reads
//! this one too: `<data>/providers/profiles/_main.json`, written with
//! `octosense-llm-config` (the library AI providers is built on, its
//! catalog of families, models and endpoints).
//!
//! Keys go to the platform's store, never into a child: on macOS the
//! keychain item octos reads (service `octos`), under an account of
//! OctoBuddy's own (`<KEY_ENV>::octobuddy`, so OctoSense's item for the
//! same provider is never overwritten); the profile holds the marker
//! `keychain:<account>`. Elsewhere the key is in the profile, which only its
//! owner reads. The agents still get placeholders: OctoBuddy's proxy adds
//! the key upstream (`claude_proxy.rs`).
//!
//! Where a host OctoBuddy reads providers from is the person's choice
//! (`<data>/providers/source`): its own, or OctoSense's AI providers
//! (read-only). Importing copies OctoSense's into its own, keys included.
use octosense_llm_config::{catalog, profile, Provider, ProviderSet};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The suffix of OctoBuddy's own keychain accounts.
pub const ACCOUNT_SUFFIX: &str = "::octobuddy";

/// Where a host OctoBuddy reads its providers from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Own,
    OctoSense,
}

/// Its own core dir: what octos would call one (`profiles/_main.json`).
pub fn dir() -> PathBuf {
    crate::model::data_dir().join("providers")
}

pub fn profile_path() -> PathBuf {
    profile::profile_path(&dir())
}

/// OctoSense's: where its AI providers app writes. In OctoSense, its
/// kernel's; on the host, under OctoSense's home (`OCTOSENSE_HOME`, else
/// `~/.octosense`, as the shell resolves it): `<home>/octos-home/.octos`.
/// (On the host the kernel library is not configured, and names a default
/// of its own that is not OctoSense's.)
pub fn octosense_dir() -> Option<PathBuf> {
    if crate::system::hosted() {
        return octosense_app_peers::octos_core::core_dir();
    }
    if let Some(dir) = std::env::var_os("OCTOS_APP_CORE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("OCTOSENSE_HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".octosense")))?;
    Some(home.join("octos-home/.octos"))
}

pub fn source() -> Source {
    match std::fs::read_to_string(dir().join("source")).ok().as_deref().map(str::trim) {
        Some("octosense") => Source::OctoSense,
        Some("own") => Source::Own,
        // Not chosen yet: its own once it has any; else OctoSense's, if there are some.
        _ => {
            let theirs = octosense_dir().is_some_and(|d| profile::profile_path(&d).is_file());
            if profile_path().is_file() || !theirs { Source::Own } else { Source::OctoSense }
        }
    }
}

pub fn set_source(source: Source) {
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(dir().join("source"), if source == Source::Own { "own" } else { "octosense" });
}

/// The providers it keeps (primary first).
pub fn set() -> ProviderSet {
    profile::load(&profile_path()).map(|l| l.set).unwrap_or_default()
}

/// A key kept: on macOS in the keychain item octos reads (its marker goes
/// in the profile), elsewhere the profile's value itself.
fn store_key(key_env: &str, key: &str) -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Ok(key.to_string());
    }
    let account = format!("{key_env}{ACCOUNT_SUFFIX}");
    keychain_put(&account, key)?;
    Ok(format!("keychain:{account}"))
}

/// `security -i`, the key on its standard input (never a command line),
/// then read back: it says 0 even when a command failed.
fn keychain_put(account: &str, secret: &str) -> Result<(), String> {
    use std::io::Write;
    if secret.chars().any(|c| c == '"' || c == '\\' || c.is_control()) {
        return Err("this key has characters the keychain tool cannot take".into());
    }
    let mut child = std::process::Command::new("security").arg("-i")
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| format!("could not run security: {e}"))?;
    let line = format!("add-generic-password -U -s \"octos\" -a \"{account}\" -w \"{secret}\"\n");
    child.stdin.take().ok_or("could not talk to security")?.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    let _ = child.wait();
    match crate::providers::resolve_key(&format!("keychain:{account}"), account) {
        Some(stored) if stored == secret => Ok(()),
        _ => Err(format!("could not keep the key in the keychain ({account})")),
    }
}

fn keychain_remove(account: &str) {
    let _ = std::process::Command::new("security").args(["delete-generic-password", "-s", "octos", "-a", account])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
}

/// Its profile, written: only its owner reads it.
fn save(set: &ProviderSet, env: &BTreeMap<String, String>) -> Result<(), String> {
    std::fs::create_dir_all(dir().join("profiles")).map_err(|e| e.to_string())?;
    profile::save_merge(&profile_path(), set, env).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(profile_path(), std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Adds `provider` (the primary while there is none, else after the
/// others; one it has already is replaced), with its key when given. Its
/// label.
pub fn add(provider: Provider, key: Option<&str>) -> Result<String, String> {
    let mut env = BTreeMap::new();
    if let Some(key) = key.map(str::trim).filter(|k| !k.is_empty()) {
        env.insert(provider.key_env.clone(), store_key(&provider.key_env, key)?);
    }
    let mut s = set();
    let label = provider.label();
    if s.primary.as_ref().is_some_and(|p| p.label() == label) {
        s.primary = Some(provider);
    } else if let Some(i) = s.fallbacks.iter().position(|p| p.label() == label) {
        s.fallbacks[i] = provider;
    } else if s.primary.is_none() {
        s.primary = Some(provider);
    } else {
        s.fallbacks.push(provider);
    }
    save(&s, &env)?;
    Ok(label)
}

/// Removes the provider `label`; its key too, when no other provider uses it.
pub fn remove(label: &str) -> Result<(), String> {
    let mut s = set();
    let gone = s.iter().find(|p| p.label() == label).cloned().ok_or_else(|| format!("{label} is not one of them"))?;
    if s.primary.as_ref().is_some_and(|p| p.label() == label) {
        s.primary = (!s.fallbacks.is_empty()).then(|| s.fallbacks.remove(0));
    } else {
        s.fallbacks.retain(|p| p.label() != label);
    }
    save(&s, &BTreeMap::new())?;
    if !s.iter().any(|p| p.key_env == gone.key_env) {
        let _ = profile::remove_env(&profile_path(), std::slice::from_ref(&gone.key_env));
        keychain_remove(&format!("{}{ACCOUNT_SUFFIX}", gone.key_env));
    }
    Ok(())
}

/// Makes `label` the primary (the old one first after it).
pub fn make_primary(label: &str) -> Result<(), String> {
    let mut s = set();
    let i = s.fallbacks.iter().position(|p| p.label() == label).ok_or_else(|| format!("{label} is not a fallback"))?;
    let chosen = s.fallbacks.remove(i);
    if let Some(old) = s.primary.replace(chosen) {
        s.fallbacks.insert(0, old);
    }
    save(&s, &BTreeMap::new())
}

/// OctoSense's AI providers added to its own: the ones it does not have,
/// after its own (the primary when it has none), with their keys (into its
/// own keychain accounts) where it has no key of that name yet. How many.
/// Blocks (the keychain): off the UI thread.
pub fn import_octosense() -> Result<usize, String> {
    let theirs = octosense_dir().map(|d| profile::profile_path(&d)).ok_or("no OctoSense home here")?;
    let loaded = profile::load(&theirs).map_err(|e| format!("{}: {e}", theirs.display()))?;
    if loaded.set.is_empty() {
        return Err(format!("OctoSense's AI providers has none ({})", theirs.display()));
    }
    let mine = profile::load(&profile_path()).unwrap_or_default();
    let mut s = mine.set.clone();
    let mut env = BTreeMap::new();
    let mut count = 0;
    for p in loaded.set.iter() {
        if s.iter().any(|o| o.label() == p.label()) {
            continue;
        }
        if !mine.env_vars.contains_key(&p.key_env) && !env.contains_key(&p.key_env) {
            if let Some(key) = loaded.env_vars.get(&p.key_env).and_then(|v| crate::providers::resolve_key(v, &p.key_env)) {
                env.insert(p.key_env.clone(), store_key(&p.key_env, &key)?);
            }
        }
        if s.primary.is_none() {
            s.primary = Some(p.clone());
        } else {
            s.fallbacks.push(p.clone());
        }
        count += 1;
    }
    if count > 0 {
        save(&s, &env)?;
    }
    Ok(count)
}

/// Whether it keeps a key named `key_env` (a new one need not be entered).
pub fn has_key(key_env: &str) -> bool {
    profile::load(&profile_path()).is_ok_and(|l| l.env_vars.get(key_env).is_some_and(|v| !v.is_empty()))
}

/// Where a family's keys are made (its console), for the ones known.
pub fn key_page(family: &str) -> Option<&'static str> {
    Some(match family {
        "anthropic" => "https://console.anthropic.com/settings/keys",
        "openai" => "https://platform.openai.com/api-keys",
        "gemini" => "https://aistudio.google.com/apikey",
        "openrouter" => "https://openrouter.ai/keys",
        "deepseek" => "https://platform.deepseek.com/api_keys",
        "groq" => "https://console.groq.com/keys",
        "zai" | "zai-coding" => "https://z.ai/manage-apikey/apikey-list",
        "zhipu" => "https://open.bigmodel.cn/usercenter/apikeys",
        _ => return None,
    })
}

/// The families the form offers, in the catalog's order.
pub fn families() -> Vec<&'static catalog::CatalogFamily> {
    catalog::families().iter().filter(|f| !f.models.is_empty()).collect()
}

/// A group of the wizard's first step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Coding plans: a subscription's endpoint for coding agents.
    Coding,
    /// Every other provider with an API key.
    More,
    /// On this computer or one's own server: no key.
    Local,
}

/// Which group `family` is shown in.
pub fn group_of(family: &catalog::CatalogFamily) -> Group {
    let id = family.id();
    if !family.family.key_required {
        Group::Local
    } else if id.contains("coding") || id.starts_with("minimax") {
        Group::Coding
    } else {
        Group::More
    }
}

/// The agents that can run on `p`, by its endpoint's protocol: Claude Code
/// and pi on an Anthropic-compatible one; Codex where there is a Chat
/// Completions endpoint (OctoBuddy's bridge turns its Responses calls into
/// those); octos on any.
pub fn agents_for(p: &Provider) -> Vec<&'static str> {
    let family = octosense_llm_config::registry::lookup(&p.family);
    let base = p.base_url.clone().or_else(|| family.and_then(|f| f.default_base_url).map(String::from)).unwrap_or_default();
    let mut out = Vec::new();
    if crate::providers::anthropic_compatible(&base) {
        out.extend(["Claude Code", "pi"]);
    }
    if !base.is_empty() && (crate::providers::chat_url(&p.family, &base).is_some() || !crate::providers::anthropic_compatible(&base)) {
        out.push("Codex");
    }
    out.push("octos");
    out
}

/// Whether OctoSense's AI providers has any here, and which (labels).
pub fn octosense_has() -> Vec<String> {
    octosense_dir().map(|d| profile::profile_path(&d))
        .and_then(|p| profile::load(&p).ok())
        .map(|l| l.set.iter().map(Provider::label).collect()).unwrap_or_default()
}

/// A provider as the form makes it: a family, one of its models, one of
/// that model's endpoints.
pub fn provider(family: &catalog::CatalogFamily, model: &catalog::Model, route: &catalog::Route) -> Provider {
    let mut p = Provider::new(family.id(), Some(model.id.clone()));
    if !route.is_official() {
        p.route_id = Some(route.id.clone());
        p.route_label = Some(route.label.clone());
        p.base_url = route.base_url.clone();
        if let Some(env) = &route.api_key_env {
            p.key_env = env.clone();
        }
    }
    p
}

/// Tries `provider` with one tiny request (its key, or the one it keeps):
/// what came back. Blocks: off the UI thread. The key goes to curl on its
/// standard input, never on a command line.
pub fn test(provider: &Provider, key: Option<&str>) -> Result<String, String> {
    use std::io::Write;
    let key = match key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => k.to_string(),
        None => {
            let loaded = profile::load(&profile_path()).map_err(|e| e.to_string())?;
            let value = loaded.env_vars.get(&provider.key_env).ok_or("no key for it: enter one")?;
            crate::providers::resolve_key(value, &provider.key_env).ok_or("its key could not be read")?
        }
    };
    let family = octosense_llm_config::registry::lookup(&provider.family);
    let base = provider.base_url.clone().or_else(|| family.and_then(|f| f.default_base_url).map(String::from))
        .ok_or("no endpoint known for it")?;
    let base = base.trim_end_matches('/');
    let model = provider.model.clone().or_else(|| family.and_then(|f| f.default_model).map(String::from)).unwrap_or_default();
    let anthropic = base.contains("/anthropic") || base.contains("api.anthropic.com") || base.contains("api.kimi.com/coding");
    let (url, auth) = if anthropic {
        let url = if base.ends_with("/v1") { format!("{base}/messages") } else { format!("{base}/v1/messages") };
        (url, format!("header = \"x-api-key: {key}\"\nheader = \"anthropic-version: 2023-06-01\"\n"))
    } else {
        (format!("{base}/chat/completions"), format!("header = \"Authorization: Bearer {key}\"\n"))
    };
    let body = serde_json::json!({"model": model, "max_tokens": 1, "messages": [{"role": "user", "content": "ping"}]}).to_string();
    let started = std::time::Instant::now();
    let mut child = std::process::Command::new("curl").args(["-sS", "-m", "30", "-K", "-", "-H", "content-type: application/json", "-w", "\n%{http_code}", "--data-binary", &body, &url])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| format!("could not run curl: {e}"))?;
    child.stdin.take().ok_or("curl")?.write_all(auth.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", text.as_ref()));
    let code: u16 = code.trim().parse().unwrap_or(0);
    let ms = started.elapsed().as_millis();
    if (200..300).contains(&code) {
        return Ok(format!("HTTP {code} · {ms} ms · {model}"));
    }
    if code == 0 {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    // What the provider said (its error's message when it says one),
    // without anything of the key.
    let message = serde_json::from_str::<serde_json::Value>(body).ok()
        .and_then(|v| v["error"]["message"].as_str().or(v["message"].as_str()).map(String::from));
    let said: String = message.as_deref().unwrap_or(body).replace(&key, "•••").chars().take(200).collect();
    Err(format!("HTTP {code}: {said}"))
}

/// Whether `path` is its own profile (the settings page's rows can change it).
pub fn is_own(path: &Path) -> bool {
    path == profile_path()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_choice_is_a_provider_on_its_route() {
        let fam = catalog::family("zai-coding").unwrap();
        let model = fam.models.iter().find(|m| m.id == "glm-5.3").unwrap();
        let routes = model.routes();
        let p = provider(fam, model, &routes[0]);
        assert_eq!(p.label(), "zai-coding/glm-5.3");
        assert!(!families().is_empty());
    }

    #[test]
    fn what_runs_on_a_provider_is_its_protocols() {
        let on = |family: &str| agents_for(&Provider::new(family, None));
        // An Anthropic-compatible coding endpoint: every agent.
        assert_eq!(on("zai-coding"), ["Claude Code", "pi", "Codex", "octos"]);
        // Chat Completions only: Codex (through the bridge) and octos.
        assert_eq!(on("deepseek"), ["Codex", "octos"]);
    }

    #[test]
    fn the_wizard_groups_coding_plans_apart_from_local_servers() {
        let group = |id: &str| group_of(catalog::family(id).unwrap());
        assert_eq!(group("zai-coding"), Group::Coding);
        assert_eq!(group("openai"), Group::More);
        assert_eq!(group("ollama"), Group::Local);
    }
}
