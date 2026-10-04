//! Each provider's mark (lobe-icons, MIT; `resources/ICONS.md`), beside a
//! model wherever OctoBuddy names one: the model picker, Settings › AI
//! Providers and its wizard, an inner loop's panel. So which provider a
//! model runs on shows at a glance.
use makepad_widgets::*;
use std::collections::HashMap;

/// What a model with no known provider shows: a plain chip.
const PLAIN: &str = r##"<svg height="24" viewBox="0 0 24 24" width="24" xmlns="http://www.w3.org/2000/svg"><rect x="3" y="3" width="18" height="18" rx="5" fill="none" stroke="#8b93a1" stroke-width="2"/><circle cx="12" cy="12" r="3" fill="#8b93a1"/></svg>"##;

/// The mark of `family` (a registry id or one of its aliases).
pub fn svg(family: &str) -> &'static str {
    match family {
        "anthropic" => include_str!("../resources/providers/anthropic.svg"),
        "openai" => include_str!("../resources/providers/openai.svg"),
        "gemini" | "google" => include_str!("../resources/providers/gemini.svg"),
        "vertex" | "vertex-ai" | "vertexai" => include_str!("../resources/providers/vertex.svg"),
        "deepseek" => include_str!("../resources/providers/deepseek.svg"),
        "minimax" | "minimax-cn" | "minimaxi" => include_str!("../resources/providers/minimax.svg"),
        "moonshot" | "moonshot-coding" | "kimi" | "kimi-coding" => include_str!("../resources/providers/moonshot.svg"),
        "dashscope" | "qwen" => include_str!("../resources/providers/dashscope.svg"),
        "zhipu" | "glm" => include_str!("../resources/providers/zhipu.svg"),
        "zai" | "zai-coding" | "z.ai" | "z.ai-coding" | "glm-coding" => include_str!("../resources/providers/zai.svg"),
        "openrouter" => include_str!("../resources/providers/openrouter.svg"),
        "groq" => include_str!("../resources/providers/groq.svg"),
        "nvidia" | "nim" => include_str!("../resources/providers/nvidia.svg"),
        "ollama" => include_str!("../resources/providers/ollama.svg"),
        "vllm" => include_str!("../resources/providers/vllm.svg"),
        "local" => include_str!("../resources/providers/local.svg"),
        _ => PLAIN,
    }
}

/// The provider a pick runs on: a `family/model` label names it; an
/// agent's own models are its maker's (Claude Code's Anthropic's, Codex's
/// OpenAI's).
pub fn family_of<'a>(engine: &str, model: Option<&'a str>) -> &'a str {
    match model.and_then(|m| m.split_once('/')) {
        Some((family, _)) => family,
        None => match engine {
            "claude" => "anthropic",
            crate::rpc_lead::CODEX => "openai",
            _ => model.map(family_of_model).unwrap_or(""),
        },
    }
}

/// The provider a bare model id comes from, by its name (`glm-5.3` →
/// Z.ai): for what reports only the model.
pub fn family_of_model(model: &str) -> &'static str {
    let m = model.to_ascii_lowercase();
    let m = m.rsplit('/').next().unwrap_or(&m);
    if m.starts_with("glm") {
        "zai"
    } else if m.starts_with("minimax") {
        "minimax"
    } else if m.starts_with("kimi") || m.starts_with("k2") || m.starts_with("k3") {
        "moonshot"
    } else if m.starts_with("deepseek") {
        "deepseek"
    } else if m.starts_with("qwen") {
        "dashscope"
    } else if m.starts_with("claude") || matches!(m, "opus" | "sonnet" | "haiku") {
        "anthropic"
    } else if m.starts_with("gpt") || m.starts_with("o3") || m.starts_with("o4") || m.starts_with("codex") {
        "openai"
    } else if m.starts_with("gemini") {
        "gemini"
    } else {
        ""
    }
}

/// Shows `svg` in the `Svg` widget `w` (one with no `svg` resource of its
/// own), parsed only when it changes: list rows are drawn every frame.
pub fn show(w: &WidgetRef, svg: &'static str, shown: &mut HashMap<WidgetUid, usize>) {
    let key = svg.as_ptr() as usize;
    let uid = w.widget_uid();
    if shown.get(&uid) == Some(&key) {
        return;
    }
    if let Some(mut icon) = w.borrow_mut::<Svg>() {
        icon.draw_svg.load_from_str(svg);
        shown.insert(uid, key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_shows_its_providers_mark() {
        assert_eq!(family_of("claude", Some("minimax-cn/MiniMax-M3")), "minimax-cn");
        assert_eq!(family_of("claude", Some("opus")), "anthropic");
        assert_eq!(family_of("claude", None), "anthropic");
        assert_eq!(family_of(crate::rpc_lead::CODEX, None), "openai");
        assert_eq!(family_of("octos", Some("glm-5.3")), "zai");
        assert_eq!(family_of_model("MiniMax-M3"), "minimax");
        assert_eq!(family_of_model("kimi-k2.5"), "moonshot");
        assert_eq!(family_of_model("something-else"), "");
        // Every family the catalog offers has its mark (or the plain one on purpose).
        for f in octosense_llm_config::catalog::families() {
            let mark = svg(f.id());
            assert!(mark.starts_with("<svg"), "{}", f.id());
            if !matches!(f.id(), "r9s") {
                assert!(!std::ptr::eq(mark, PLAIN), "{} has no mark", f.id());
            }
        }
    }
}
