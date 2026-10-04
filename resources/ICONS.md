# Icons

- `claude.svg`, `zai.svg`, `codex.svg`: from [lobe-icons](https://github.com/lobehub/lobe-icons)
  (`@lobehub/icons-static-svg` 1.95.1, MIT), sized to 24 and given explicit colors. The marks
  belong to their owners (Anthropic, Z.ai, OpenAI); they only name the agent or model in use.
- `providers/*.svg`: each AI provider's mark, named by its family id, from the same package
  (1.95.1, MIT): `anthropic` (claude-color), `openai`, `gemini` (gemini-color), `vertex`
  (vertexai-color), `deepseek` (deepseek-color), `minimax` (minimax-color), `moonshot` (kimi),
  `dashscope` (qwen-color), `zhipu` (zhipu-color), `zai`, `openrouter`, `groq`, `nvidia`
  (nvidia-color), `ollama`, `vllm` (vllm-color), `local` (lmstudio). Sized to 24; the
  monochrome ones given the explicit `#1f2328`. The marks belong to their owners; they only
  name the provider a model runs on (`src/provider_icons.rs`).
- `octos.svg`: OctoSense's own octopus (`crates/shell/resources/icons/octopus.svg`), colored.
