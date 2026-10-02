//! The LLM providers OctoSense's AI providers app has enabled, and the octos
//! profile the inner loop runs with because of them.
//!
//! AI providers writes the kernel's profile (`<core dir>/profiles/_main.json`,
//! keys in the platform keychain). OctoBuddy only reads it: the settings page
//! lists what is enabled (never a key), and when there is a primary provider
//! the peers run on it. They run in OctoBuddy's own octos server, whose data
//! directory is not the kernel's and whose main profile is a link to that
//! file: peers never write into the kernel's home and always see the
//! person's current choice.
use octosense_llm_config::profile;
use std::path::PathBuf;

/// The octos profile peers use when OctoSense has no provider enabled
/// (`~/.octos/profiles/<name>.json`, as octoscode sets it up).
pub fn fallback_profile() -> String {
    std::env::var("OCTOBUDDY_OCTOS_PROFILE").unwrap_or_else(|_| "octos".to_string())
}

/// What Claude Code needs to run on one of the person's providers instead
/// of Anthropic: an Anthropic-compatible endpoint, the model, and the key.
/// The key stays in OctoBuddy: Claude Code talks to OctoBuddy's loopback proxy
/// (`claude_proxy.rs`), which adds it upstream; no child sees it.
#[derive(Clone)]
pub struct ClaudeRoute {
    pub label: String,
    pub base_url: String,
    pub model: String,
    pub(crate) key: String,
    /// Its OpenAI-compatible Chat Completions endpoint, when it has one
    /// (Codex reaches it through OctoBuddy's Responses bridge).
    pub chat_url: Option<String>,
}

/// A family's Chat Completions endpoint: known ones by name, else the
/// Anthropic endpoint's sibling (`…/anthropic` → `…/v1`).
fn chat_url(family: &str, anthropic: &str) -> Option<String> {
    let known = match family {
        "zai-coding" => Some("https://api.z.ai/api/coding/paas/v4"),
        "zai" => Some("https://api.z.ai/api/paas/v4"),
        "zhipu" => Some("https://open.bigmodel.cn/api/paas/v4"),
        "deepseek" => Some("https://api.deepseek.com/v1"),
        "moonshot" | "kimi" => Some("https://api.moonshot.cn/v1"),
        _ => None,
    };
    known.map(String::from).or_else(|| anthropic.strip_suffix("/anthropic").map(|b| format!("{b}/v1")))
}

/// Whether `url` speaks Anthropic's Messages API (what Claude Code calls).
fn anthropic_compatible(url: &str) -> bool {
    url.contains("/anthropic") || url.contains("api.anthropic.com") || url.contains("api.kimi.com/coding")
}

/// A key the profile holds: itself, or a keychain marker resolved the way
/// octos does (`security`, service `octos`, the marker's account).
fn resolve_key(value: &str, env_name: &str) -> Option<String> {
    let Some(rest) = value.strip_prefix("keychain:") else {
        return Some(value.to_string()).filter(|v| !v.is_empty());
    };
    let account = rest.strip_prefix("octos/").unwrap_or(rest);
    let account = if account.is_empty() { env_name } else { account };
    let out = std::process::Command::new("security").args(["find-generic-password", "-s", "octos", "-a", account, "-w"]).output().ok()?;
    let key = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !key.is_empty()).then_some(key)
}

/// The profile's lanes (primary, then fallbacks) and its env vars.
fn lanes() -> Result<(Vec<serde_json::Value>, serde_json::Value), String> {
    let core = core_dir().ok_or("no home directory")?;
    let text = std::fs::read_to_string(profile::profile_path(&core)).map_err(|e| format!("AI providers profile: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("AI providers profile: {e}"))?;
    let llm = &v["config"]["llm"];
    let mut lanes = vec![llm["primary"].clone()];
    lanes.extend(llm["fallbacks"].as_array().cloned().unwrap_or_default());
    Ok((lanes, v["config"]["env_vars"].clone()))
}

fn lane_label(lane: &serde_json::Value) -> String {
    format!("{}/{}", lane["family_id"].as_str().unwrap_or(""), lane["model_id"].as_str().unwrap_or(""))
}

/// A lane's Anthropic-compatible endpoint: its route's, else its family's.
fn lane_url(lane: &serde_json::Value) -> Option<String> {
    let family = octosense_llm_config::registry::lookup(lane["family_id"].as_str().unwrap_or(""));
    lane["route"]["base_url"].as_str().map(String::from)
        .or_else(|| family.and_then(|f| f.default_base_url).map(String::from))
        .filter(|u| anthropic_compatible(u))
}

/// The route for the provider `label` (`family/model`, as the picker lists
/// them), when Claude Code can talk to it. Reads the key.
pub fn claude_route(label: &str) -> Result<ClaudeRoute, String> {
    let (lanes, vars) = lanes()?;
    let lane = lanes.into_iter().find(|l| lane_label(l) == label).ok_or_else(|| format!("{label} is not one of the AI providers"))?;
    let base_url = lane_url(&lane).ok_or_else(|| format!("{label} has no Anthropic-compatible endpoint, so Claude Code cannot use it"))?;
    let family = octosense_llm_config::registry::lookup(lane["family_id"].as_str().unwrap_or(""));
    let env_name = lane["route"]["api_key_env"].as_str().map(String::from)
        .or_else(|| family.and_then(|f| f.key_env).map(String::from)).unwrap_or_default();
    let names: Vec<String> = std::iter::once(env_name).chain(family.into_iter().flat_map(|f| f.key_env_aliases.iter().map(|a| a.to_string()))).collect();
    let key = names.iter().find_map(|n| vars[n.as_str()].as_str().and_then(|value| resolve_key(value, n)))
        .ok_or_else(|| format!("no key for {label} (set it in AI providers)"))?;
    let model = lane["model_id"].as_str().filter(|m| !m.is_empty()).map(String::from)
        .or_else(|| family.and_then(|f| f.default_model).map(String::from)).unwrap_or_default();
    let chat_url = chat_url(lane["family_id"].as_str().unwrap_or(""), &base_url);
    Ok(ClaudeRoute { label: label.to_string(), base_url, model, key, chat_url })
}

/// The AI providers Claude Code can run on (labels, the profile's order).
/// Reads no key.
pub fn claude_capable() -> Vec<String> {
    lanes().map(|(lanes, _)| lanes.iter().filter(|l| lane_url(l).is_some()).map(lane_label).collect()).unwrap_or_default()
}

const LINKED_NAME: &str = "octosense";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyState {
    /// The profile names a keychain entry.
    Keychain,
    /// The key is in the profile itself.
    InProfile,
    /// No key for the provider's env var (it may still come from the environment).
    Missing,
}

impl KeyState {
    pub fn label(self) -> &'static str {
        match self {
            KeyState::Keychain => "key in keychain",
            KeyState::InProfile => "key set",
            KeyState::Missing => "no key",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProviderRow {
    /// `family/model`.
    pub label: String,
    /// `primary` or `fallback N`.
    pub role: String,
    pub route: String,
    pub key: KeyState,
}

#[derive(Clone, Debug, Default)]
pub struct Providers {
    pub profile_path: PathBuf,
    pub rows: Vec<ProviderRow>,
    pub error: Option<String>,
}

impl Providers {
    pub fn primary(&self) -> Option<&ProviderRow> {
        self.rows.iter().find(|r| r.role == "primary")
    }
}

/// Where AI providers writes: the core dir of this process's kernel, which
/// the shell derives from its own state directory
/// (`<OCTOSENSE_HOME>/octos-home/.octos` on a desktop); outside a shell, the
/// same default the kernel would use.
fn core_dir() -> Option<PathBuf> {
    // On the host, another profile when the person names one.
    let named = std::env::var_os("OCTOBUDDY_PROVIDERS").map(PathBuf::from).filter(|_| !crate::system::hosted());
    named.or_else(octosense_app_peers::octos_core::core_dir).or_else(profile::default_core_dir)
}

/// Reads what AI providers has enabled. Never returns a key.
pub fn read() -> Providers {
    let Some(core) = core_dir() else {
        return Providers { error: Some("no home directory".into()), ..Default::default() };
    };
    let path = profile::profile_path(&core);
    match profile::load(&path) {
        Ok(loaded) => {
            let key = |env: &str| match loaded.env_vars.get(env) {
                Some(v) if profile::is_keychain_marker(v) => KeyState::Keychain,
                Some(v) if !v.is_empty() => KeyState::InProfile,
                _ => KeyState::Missing,
            };
            let rows = loaded.set.iter().enumerate().map(|(i, p)| ProviderRow {
                label: p.label(),
                role: if i == 0 && loaded.set.primary.is_some() { "primary".into() } else { format!("fallback {}", if loaded.set.primary.is_some() { i } else { i + 1 }) },
                route: p.route_label.clone().or_else(|| p.route_id.clone()).unwrap_or_else(|| "official".into()),
                key: key(&p.key_env),
            }).collect();
            Providers { profile_path: path, rows, error: None }
        }
        Err(err) => Providers { profile_path: path, rows: Vec::new(), error: Some(err.to_string()) },
    }
}

/// The profile peers run with: the file each peer's data directory links
/// to, and the name octos knows it by there.
#[derive(Clone, Debug, PartialEq)]
pub struct PeerProfile {
    /// `None` when there is no profile file at all (octos then reports it).
    pub file: Option<PathBuf>,
    pub name: String,
    /// For the person: where the model comes from.
    pub source: String,
}

/// OctoSense's AI providers when they name a primary provider, else the
/// person's own octos profile.
pub fn peer_profile(providers: &Providers) -> PeerProfile {
    if let Some(primary) = providers.primary() {
        return PeerProfile {
            file: Some(providers.profile_path.clone()),
            name: LINKED_NAME.to_string(),
            source: format!("OctoSense AI providers · {}", primary.label),
        };
    }
    let name = fallback_profile();
    let file = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".octos/profiles").join(format!("{name}.json")))
        .filter(|f| f.is_file());
    PeerProfile { file, source: format!("octos profile “{name}” (~/.octos/profiles/{name}.json)"), name }
}

/// Prepares OctoBuddy's octos server directory: its `profiles/_main.json`
/// (the profile every session runs with) links to the profile file.
pub fn serve_data_dir(dir: &std::path::Path, profile: &PeerProfile) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    match &profile.file {
        Some(file) => link_profile(dir, "_main", file),
        None => Ok(()),
    }
}

/// The profile a loop runs under on octos on the model `label`
/// (`family/model`): an outer loop the person set to it, an inner loop whose
/// slice names it. See [`model_profiles`].
pub fn model_profile_id(label: &str) -> String {
    let slug: String = label.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    format!("model-{}", slug.trim_matches('-'))
}

/// One profile per model of `profile` other than its primary, each with
/// that model first (its other models behind it): what a loop runs under
/// on octos on that model. octos has no model per session,
/// only per profile, and reads profiles when it starts: they are written
/// beside `_main` before it does.
pub fn model_profiles(data_dir: &std::path::Path, profile: &PeerProfile) -> std::io::Result<()> {
    let Some(file) = &profile.file else { return Ok(()) };
    let base: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file)?).map_err(std::io::Error::other)?;
    let label = |m: &serde_json::Value| format!("{}/{}", m["family_id"].as_str().unwrap_or(""), m["model_id"].as_str().unwrap_or(""));
    let Some(llm) = base.pointer("/config/llm") else { return Ok(()) };
    let primary = llm["primary"].clone();
    let fallbacks = llm["fallbacks"].as_array().cloned().unwrap_or_default();
    let dir = data_dir.join("profiles");
    std::fs::create_dir_all(&dir)?;
    for (i, chosen) in fallbacks.iter().enumerate() {
        let id = model_profile_id(&label(chosen));
        let mut rest = fallbacks.clone();
        rest.remove(i);
        if !primary.is_null() {
            rest.insert(0, primary.clone());
        }
        let mut v = base.clone();
        v["id"] = serde_json::Value::String(id.clone());
        v["config"]["llm"]["primary"] = chosen.clone();
        without_shell_policy(&mut v);
        v["config"]["llm"]["fallbacks"] = serde_json::Value::Array(rest);
        write_private(&dir.join(format!("{id}.json")), &serde_json::to_string_pretty(&v).map_err(std::io::Error::other)?)?;
    }
    Ok(())
}

/// A copy of a profile holds what it holds (a key, maybe): only its owner reads it.
fn write_private(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// The shell's own tool policy for its system agent (its kernel writes it
/// into the AI providers profile: no commands, `owner: octosense`) is not
/// OctoBuddy's: OctoBuddy's inner loops run their project's commands, on
/// their own `octos serve`. OctoBuddy takes the profile's models and keys.
fn without_shell_policy(value: &mut serde_json::Value) {
    let shells = value.pointer("/config/tool_policy/owner").and_then(|o| o.as_str()) == Some("octosense");
    if let (true, Some(config)) = (shells, value.get_mut("config").and_then(|c| c.as_object_mut())) {
        config.remove("tool_policy");
    }
}

/// `name` in OctoBuddy's serve dir: its own copy of the profile at `target`,
/// under that id, refreshed at every start. A copy, never a link: a
/// profile keeps its data under its id (one named otherwise would have
/// octos open that store twice), and the shell's policy stays the shell's.
fn link_profile(data_dir: &std::path::Path, name: &str, target: &std::path::Path) -> std::io::Result<()> {
    let dir = data_dir.join("profiles");
    std::fs::create_dir_all(&dir)?;
    let link = dir.join(format!("{name}.json"));
    let json = std::fs::read_to_string(target)?;
    let mut value: serde_json::Value = serde_json::from_str(&json).map_err(std::io::Error::other)?;
    value["id"] = serde_json::Value::String(name.to_string());
    without_shell_policy(&mut value);
    let text = serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?;
    let is_link = std::fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink());
    if !is_link && std::fs::read_to_string(&link).ok().as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    // A link (as earlier versions made) goes first: writing through it
    // would change the shell's own profile.
    let _ = std::fs::remove_file(&link);
    write_private(&link, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_providers_without_keys_and_links_the_profile() {
        let root = std::env::temp_dir().join(format!("octobuddy-providers-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let core = root.join("core");
        std::fs::create_dir_all(core.join("profiles")).unwrap();
        std::fs::write(core.join("profiles/_main.json"), r#"{"config":{"llm":{
            "primary":{"family_id":"zai-coding","model_id":"glm-5.3","route":{"route_id":"zai-coding","label":"GLM Coding Plan","api_key_env":"ZAI_API_KEY"}},
            "fallbacks":[{"family_id":"deepseek","model_id":"deepseek-chat"}]},
            "env_vars":{"ZAI_API_KEY":"keychain:octos/ZAI_API_KEY","DEEPSEEK_API_KEY":"sk-secret"}}}"#).unwrap();
        // SAFETY of the env var: this test is the only one reading it.
        std::env::set_var("OCTOS_APP_CORE_DIR", &core);
        let providers = read();
        std::env::remove_var("OCTOS_APP_CORE_DIR");
        assert_eq!(providers.error, None);
        let rows: Vec<_> = providers.rows.iter().map(|r| (r.label.as_str(), r.role.as_str(), r.key)).collect();
        assert_eq!(rows, [("zai-coding/glm-5.3", "primary", KeyState::Keychain), ("deepseek/deepseek-chat", "fallback 1", KeyState::InProfile)]);
        assert!(!format!("{providers:?}").contains("sk-secret"), "no key leaves the profile");

        let peer = peer_profile(&providers);
        assert_eq!(peer.name, "octosense");
        assert!(peer.source.contains("zai-coding/glm-5.3"));
        let serve = root.join("octobuddy/octos/serve");
        serve_data_dir(&serve, &peer).unwrap();
        let main = serve.join("profiles/_main.json");
        assert!(std::fs::read_link(&main).is_err(), "OctoBuddy's own copy, not a link to the shell's file");
        let copied: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&main).unwrap()).unwrap();
        assert_eq!(copied["config"]["llm"]["primary"]["model_id"], "glm-5.3");
        serve_data_dir(&serve, &peer).unwrap();  // again: already right

        // The shell's kernel writes its system agent's policy into that
        // profile; OctoBuddy's copy leaves it out, the shell's file keeps it.
        let shell_policy = r#"{"id":"_main","config":{"llm":{"primary":{"family_id":"zai-coding","model_id":"glm-5.3"}},"tool_policy":{"allow":[],"deny":["group:runtime"],"owner":"octosense"}}}"#;
        std::fs::write(core.join("profiles/_main.json"), shell_policy).unwrap();
        serve_data_dir(&serve, &peer).unwrap();
        let copied: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&main).unwrap()).unwrap();
        assert!(copied["config"].get("tool_policy").is_none(), "{copied}");
        assert_eq!(std::fs::read_to_string(core.join("profiles/_main.json")).unwrap(), shell_policy, "the shell's own profile is untouched");

        let none = peer_profile(&Providers::default());
        assert_eq!(none.name, "octos");

        // An octos profile of another id is copied as `_main`, not linked.
        let own = root.join("octos.json");
        std::fs::write(&own, r#"{"id":"octos","config":{}}"#).unwrap();
        serve_data_dir(&serve, &PeerProfile { file: Some(own), name: "octos".into(), source: String::new() }).unwrap();
        let main = serve.join("profiles/_main.json");
        assert!(std::fs::read_link(&main).is_err(), "a copy, not a link");
        let copied: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&main).unwrap()).unwrap();
        assert_eq!(copied["id"], "_main");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&main).unwrap().permissions().mode() & 0o777, 0o600, "a copy only its owner reads");
        }

        // An outer loop's profile per other model, that model first.
        let two = root.join("two.json");
        std::fs::write(&two, r#"{"config":{"llm":{"primary":{"family_id":"zai","model_id":"glm-5.3"},"fallbacks":[{"family_id":"deepseek","model_id":"deepseek-chat"}]}}}"#).unwrap();
        model_profiles(&serve, &PeerProfile { file: Some(two), name: "x".into(), source: String::new() }).unwrap();
        let id = model_profile_id("deepseek/deepseek-chat");
        assert_eq!(id, "model-deepseek-deepseek-chat");
        let outer: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(serve.join(format!("profiles/{id}.json"))).unwrap()).unwrap();
        assert_eq!(outer["id"], id.as_str());
        assert_eq!(outer["config"]["llm"]["primary"]["model_id"], "deepseek-chat");
        assert_eq!(outer["config"]["llm"]["fallbacks"][0]["model_id"], "glm-5.3");
        let _ = std::fs::remove_dir_all(root);
    }
}
