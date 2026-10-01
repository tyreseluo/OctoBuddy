//! The native TUI plugin: an agent's own terminal UI in place of OctoBuddy's
//! messages — Claude Code's, Codex's, pi's — in Makepad's terminal (the one
//! OctoSense's Terminal app is), on the same conversation: the outer loop in
//! the stage (Terminal, at the top left), an inner loop in its panel (TUI).
//!
//! One process owns a conversation at a time: OctoBuddy ends its own for that
//! agent, the CLI resumes the conversation (`claude --resume`, `codex
//! resume`, `pi --continue`), and while it is open what OctoBuddy has for
//! that agent waits; it goes on when the person comes back (or the CLI
//! exits), with the conversation the TUI added to. Keys stay in OctoBuddy's
//! proxy: the CLI gets the same placeholders its process here would.
use crate::lead::Mode;
use crate::{i18n, plugins, providers, rpc_lead, OctoBuddyView, Stage};
use makepad_terminal::widget::{MpTerm, MpTermAction};
use makepad_widgets::*;

/// A word for a POSIX shell, single-quoted.
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The CLI that runs `engine` (an argv as one shell line, `exec`'d).
#[derive(Default)]
pub(crate) struct Cli {
    pub env: Vec<(String, String)>,
    pub unset: Vec<String>,
    pub argv: Vec<String>,
}

impl Cli {
    pub fn line(&self) -> String {
        let mut words = vec!["exec".to_string(), "env".to_string()];
        for name in &self.unset {
            words.push("-u".into());
            words.push(name.clone());
        }
        for (k, v) in &self.env {
            words.push(format!("{k}={}", quote(v)));
        }
        words.extend(self.argv.iter().map(|a| quote(a)));
        words.join(" ")
    }
}

impl OctoBuddyView {
    /// The CLI for an agent on `engine`, resuming `resume`, on `label`
    /// (`family/model` through the proxy, or a Claude model name), with
    /// OctoBuddy's `rules` and its MCP servers.
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    fn cli(&mut self, engine: &str, session: &str, resume: Option<&str>, label: Option<&str>, effort: Option<&str>, rules: &str, mcp: &[(String, String)], mode: Mode) -> Result<Cli, String> {
        let provider = label.filter(|l| l.contains('/'));
        let route = match provider {
            Some(l) => Some(providers::claude_route(l)?),
            None => None,
        };
        if route.is_some() && self.rt.claude_proxy.is_none() {
            self.rt.claude_proxy = crate::claude_proxy::Proxy::start();
        }
        let proxy = self.rt.claude_proxy.as_ref();
        let mut cli = Cli::default();
        match engine {
            "claude" => {
                if let (Some(r), Some(p)) = (&route, proxy) {
                    cli.env = p.env_for(r);
                    cli.unset = crate::claude_proxy::SCRUB.iter().map(|s| s.to_string()).collect();
                }
                cli.argv.push("claude".into());
                if let Some(id) = resume {
                    cli.argv.extend(["--resume".into(), id.into()]);
                }
                if let Some(model) = route.as_ref().map(|r| r.model.clone()).or(label.filter(|l| !l.contains('/')).map(String::from)) {
                    cli.argv.extend(["--model".into(), model]);
                }
                if let Some(effort) = effort {
                    cli.argv.extend(["--effort".into(), effort.into()]);
                }
                if !rules.is_empty() {
                    cli.argv.extend(["--append-system-prompt".into(), rules.into()]);
                }
                for (name, config) in mcp {
                    cli.argv.extend(["--mcp-config".into(), config.clone(), "--allowedTools".into(), format!("mcp__{name}")]);
                }
                if mode == Mode::Inner {
                    cli.argv.extend(["--permission-mode".into(), "acceptEdits".into(), "--settings".into(),
                        r#"{"sandbox":{"enabled":true,"autoAllowBashIfSandboxed":true,"allowUnsandboxedCommands":false}}"#.into()]);
                }
            }
            rpc_lead::CODEX => {
                cli.unset = vec!["OPENAI_API_KEY".into()];
                cli.argv.push("codex".into());
                if let Some(id) = resume {
                    cli.argv.extend(["resume".into(), id.into()]);
                }
                if let (Some(r), Some(p)) = (&route, proxy) {
                    cli.env.push(("OCTOBUDDY_CODEX_KEY".into(), crate::claude_proxy::PLACEHOLDER_KEY.into()));
                    let base = p.responses_base(r)?;
                    for c in ["model_provider=\"octobuddy\"".to_string(), "model_providers.octobuddy.name=\"OctoBuddy\"".into(),
                        format!("model_providers.octobuddy.base_url=\"{base}\""), "model_providers.octobuddy.wire_api=\"responses\"".into(),
                        "model_providers.octobuddy.env_key=\"OCTOBUDDY_CODEX_KEY\"".into(), "model_providers.octobuddy.supports_websockets=false".into()] {
                        cli.argv.extend(["-c".into(), c]);
                    }
                    cli.argv.extend(["-m".into(), r.model.clone()]);
                }
                if let Some(effort) = effort {
                    cli.argv.extend(["-c".into(), format!("model_reasoning_effort=\"{effort}\"")]);
                }
                for (name, config) in mcp {
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(config) else { continue };
                    if let Some(url) = v["mcpServers"][name.as_str()]["url"].as_str() {
                        cli.argv.extend(["-c".into(), format!("mcp_servers.{name}.url=\"{url}\"")]);
                    }
                }
                let sandbox = if mode == Mode::Outer { "read-only" } else { "workspace-write" };
                cli.argv.extend(["-s".into(), sandbox.into(), "-a".into(), "never".into()]);
            }
            rpc_lead::PI => {
                let r = route.as_ref().ok_or("pi runs on one of your AI providers")?;
                let p = proxy.ok_or("OctoBuddy's proxy could not start")?;
                // The same agent directory as its process here (its models.json, its sessions).
                let safe: String = session.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
                let home = crate::model::data_dir().join("pi").join(&safe);
                std::fs::create_dir_all(home.join("sessions")).map_err(|e| e.to_string())?;
                let models = serde_json::json!({"providers": {"octobuddy": {
                    "baseUrl": p.anthropic_base(r), "api": "anthropic-messages", "apiKey": crate::claude_proxy::PLACEHOLDER_KEY,
                    "models": [{"id": r.model, "name": r.model, "reasoning": true, "input": ["text"], "contextWindow": 200000, "maxTokens": 32000,
                        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0}}]}}});
                std::fs::write(home.join("models.json"), models.to_string()).map_err(|e| e.to_string())?;
                cli.env = vec![("PI_CODING_AGENT_DIR".into(), home.display().to_string()), ("PI_OFFLINE".into(), "1".into())];
                cli.argv = vec![crate::workspace::find_bin("pi").display().to_string(), "--session-dir".into(), home.join("sessions").display().to_string(),
                    "--provider".into(), "octobuddy".into(), "--model".into(), r.model.clone()];
                if resume.is_some() {
                    cli.argv.push("--continue".into());
                }
                if let Some(effort) = effort {
                    cli.argv.extend(["--thinking".into(), effort.into()]);
                }
                if !rules.is_empty() {
                    cli.argv.extend(["--append-system-prompt".into(), rules.into()]);
                }
                if mode == Mode::Outer {
                    cli.argv.extend(["--tools".into(), "read,grep,find,ls".into()]);
                }
            }
            other => return Err(i18n::pick(format!("{other} has no terminal UI here."), format!("{other} 在这里没有终端界面。"))),
        }
        Ok(cli)
    }

    /// Whether the TUI holds the outer loop of `session` (what OctoBuddy has
    /// for it waits).
    pub(crate) fn tui_holds_outer(&self, session: &str) -> bool {
        self.tui.as_deref() == Some(&format!("s:{session}"))
    }

    pub(crate) fn tui_holds_peer(&self, peer: &str) -> bool {
        self.tui.as_deref() == Some(&format!("p:{peer}"))
    }

    fn start_term(&mut self, cx: &mut Cx, term: &[LiveId], cwd: &str, line: String) {
        if let Some(mut t) = self.view.widget(cx, term).borrow_mut::<MpTerm>() {
            t.restart_with(cx, Some(std::path::PathBuf::from(cwd)), Some(line));
            t.focus(cx);
        }
    }

    /// The outer loop of the session shown, in its own CLI.
    pub(crate) fn open_outer_tui(&mut self, cx: &mut Cx) -> Result<(), String> {
        let at = self.selected.ok_or("no session")?;
        let session = self.store.session(at).cloned().ok_or("no session")?;
        if self.lead_busy(&session.id) {
            return Err(i18n::t("The outer loop is at work: open its terminal when this turn is done.", "外环正在工作：等这一轮结束再打开它的终端。").into());
        }
        let engine = session.engine().to_string();
        let plain = self.store.is_plain(at);
        let cwd = session.work_dir.clone().unwrap_or_else(|| self.store.projects[at.0].path.clone());
        let rules = if plain { crate::lead::PLAIN_CHAT.to_string() } else { crate::lead::lead_prompt() };
        let mcp: Vec<(String, String)> = self.mcp.as_ref().filter(|_| !plain).map(|m| vec![("octobuddy".to_string(), m.config(&session.id))]).unwrap_or_default();
        let cli = self.cli(&engine, &session.id, session.lead_session.as_deref(), session.outer_model(), session.outer_effort(), &rules, &mcp, if plain { Mode::Plain } else { Mode::Outer })?;
        // Its process here ends: the CLI owns the conversation now.
        self.rt.leads.remove(&session.id);
        self.tui = Some(format!("s:{}", session.id));
        self.stage = Stage::Tui;
        self.start_term(cx, ids!(cli_term), &cwd, cli.line());
        Ok(())
    }

    /// An inner loop in its own CLI, in its panel.
    pub(crate) fn open_inner_tui(&mut self, cx: &mut Cx, peer: &str) -> Result<(), String> {
        let at = self.store.find_peer(peer).ok_or("no such inner loop")?;
        let session = self.store.session(at).cloned().ok_or("no session")?;
        let p = session.peers().iter().find(|p| p.id == peer).cloned().ok_or("no such inner loop")?;
        if p.is_active() {
            return Err(i18n::t("It is at work: open its terminal when this turn is done.", "它正在工作：等这一轮结束再打开它的终端。").into());
        }
        if !self.on_claude(peer) {
            return Err(i18n::t("An inner loop on octos has no terminal UI here.", "跑在 octos 上的 inner 在这里没有终端界面。").into());
        }
        let engine = p.agent().to_string();
        let label = p.model_pick.clone().or(session.inner_model().map(String::from));
        let mcp: Vec<(String, String)> = self.mcp.as_ref().map(|m| vec![("octobuddy".to_string(), m.inner_config(peer))]).unwrap_or_default();
        let effort = session.inner_effort.clone().filter(|e| crate::model::effort_levels(&engine).contains(&e.as_str()));
        let cli = self.cli(&engine, &format!("inner:{peer}"), p.claude_session.as_deref(), label.as_deref(), effort.as_deref(), crate::lead::INNER_RULES, &mcp, Mode::Inner)?;
        self.rt.claude_inners.remove(peer);
        self.tui = Some(format!("p:{peer}"));
        self.start_term(cx, ids!(inner_term), &p.dir, cli.line());
        Ok(())
    }

    /// Back to OctoBuddy's view: the CLI ends, what waited goes on.
    pub(crate) fn close_tui(&mut self, cx: &mut Cx) {
        if self.release_tui(cx) {
            self.relayout(cx);
        }
    }

    /// The same, without laying out again (from `sync_tui`); whether one was open.
    fn release_tui(&mut self, cx: &mut Cx) -> bool {
        let Some(key) = self.tui.take() else { return false };
        for term in [ids!(cli_term), ids!(inner_term)] {
            if let Some(mut t) = self.view.widget(cx, term).borrow_mut::<MpTerm>() {
                t.unload(cx);
            }
        }
        if self.stage == Stage::Tui {
            self.stage = Stage::Chat;
        }
        if let Some(sid) = key.strip_prefix("s:") {
            if let Some(at) = self.store.find_session(sid) {
                self.system(at, i18n::t("Back from the terminal: the outer loop goes on here, with what you did there.", "已从终端回来：外环在这里接着做，包括你在终端里做的。"));
                self.drain_outer(at);
            }
        } else if let Some(peer) = key.strip_prefix("p:") {
            let next = self.rt.lines.get_mut(peer).and_then(|l| l.opened());
            if let Some(d) = next {
                self.start_peer_turn_now(peer, d);
            }
        }
        true
    }

    /// The switch, the inner panel's toggle, a CLI that exited.
    pub(crate) fn tui_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let on = plugins::enabled(plugins::NATIVE_TUI);
        // Its button again, lit: back to the conversation.
        if crate::tapped(&self.view.view(cx, ids!(view_tui_on)), actions) {
            self.close_tui(cx);
            return;
        }
        if on && crate::tapped(&self.view.view(cx, ids!(view_tui)), actions) {
            if let Err(err) = self.open_outer_tui(cx) {
                self.say(cx, &err);
            }
            self.relayout(cx);
        }
        if on && self.view.button(cx, ids!(inner_tui)).clicked(actions) {
            let peer = self.selected.and_then(|at| self.shown_peer(at)).map(|p| p.id.clone());
            if let Some(peer) = peer {
                if let Err(err) = self.open_inner_tui(cx, &peer) {
                    self.say(cx, &err);
                }
                self.relayout(cx);
            }
        }
        if self.view.button(cx, ids!(inner_tui_on)).clicked(actions) {
            self.close_tui(cx);
        }
        if self.view.button(cx, ids!(tui_restart)).clicked(actions) && self.tui.is_some() {
            let key = self.tui.clone().unwrap_or_default();
            self.close_tui(cx);
            let reopened = if key.starts_with("s:") { self.open_outer_tui(cx) } else { self.open_inner_tui(cx, key.trim_start_matches("p:")) };
            if let Err(err) = reopened {
                self.say(cx, &err);
            }
            self.relayout(cx);
        }
        // The CLI exited (/exit, Ctrl-D): back to OctoBuddy's view.
        let uids = [self.view.widget(cx, ids!(cli_term)).widget_uid(), self.view.widget(cx, ids!(inner_term)).widget_uid()];
        let exited = actions.iter().filter_map(|a| a.as_widget_action()).any(|a| uids.contains(&a.widget_uid) && matches!(a.cast::<MpTermAction>(), MpTermAction::Exited));
        if exited && self.tui.is_some() {
            self.close_tui(cx);
        }
    }

    /// What shows: the switch (the plugin on, a session shown), the
    /// terminal or the messages, the inner panel's toggle.
    pub(crate) fn sync_tui(&mut self, cx: &mut Cx) {
        let on = plugins::enabled(plugins::NATIVE_TUI);
        // The plugin off, or another view picked at the top left: back to OctoBuddy's.
        let left = self.stage != Stage::Tui && self.tui.as_deref().is_some_and(|k| k.starts_with("s:"));
        if (!on && self.tui.is_some()) || left {
            self.release_tui(cx);
        }
        let shown = self.selected.is_some_and(|at| self.store.session(at).is_some());
        let outer = self.stage == Stage::Tui && self.tui.as_deref().is_some_and(|k| k.starts_with("s:"));
        self.view.view(cx, ids!(view_tui)).set_visible(cx, on && shown && !outer);
        self.view.view(cx, ids!(view_tui_on)).set_visible(cx, on && shown && outer);
        for id in [ids!(view_tui), ids!(view_tui_on)] {
            self.view.label(cx, &[id[0], live_id!(name)]).set_text(cx, i18n::t("Terminal", "终端"));
            self.view.label(cx, &[id[0], live_id!(sub)]).set_text(cx, "");
        }
        self.view.view(cx, ids!(tui_view)).set_visible(cx, outer);
        if outer {
            let engine = self.selected.and_then(|at| self.store.session(at)).map(|s| s.engine().to_string()).unwrap_or_default();
            let name = match engine.as_str() { "claude" => "Claude Code", crate::rpc_lead::CODEX => "Codex", other => other }.to_string();
            let plain = self.selected.is_some_and(|at| self.store.is_plain(at));
            let (title, note) = if plain {
                (i18n::pick(format!("{name}'s own terminal UI, on this conversation"), format!("{name} 自己的终端界面，接着这个对话")),
                    i18n::t("The terminal button at the top left again (or exiting the CLI) brings back OctoBuddy's view of the conversation.",
                        "再点一次左上角的终端按钮（或退出 CLI），回到 OctoBuddy 的对话视图。").to_string())
            } else {
                (i18n::pick(format!("The outer loop's own CLI ({name}), on this conversation"), format!("外环自己的 CLI（{name}），接着这个会话")),
                    i18n::t("What waits for the outer loop goes on when you come back (the terminal button at the top left again, or exit the CLI). Its octobuddy tools work here too.",
                        "再点一次左上角的终端按钮或退出 CLI 后，外环在 OctoBuddy 里继续，排队的内容那时再发给它。它的 octobuddy 工具在这里也能用。").to_string())
            };
            self.view.label(cx, ids!(tui_title)).set_text(cx, &title);
            self.view.label(cx, ids!(tui_note)).set_text(cx, &note);
            self.view.button(cx, ids!(tui_restart)).set_text(cx, i18n::t("Reconnect", "重新连接"));
        }
        // The inner panel: its CLI or its messages.
        let peer = self.selected.and_then(|at| self.shown_peer(at)).map(|p| p.id.clone());
        let inner = peer.as_deref().is_some_and(|p| self.tui_holds_peer(p));
        let lead_backed = peer.as_deref().is_some_and(|p| self.on_claude(p));
        self.view.button(cx, ids!(inner_tui)).set_visible(cx, on && lead_backed && !inner);
        self.view.button(cx, ids!(inner_tui_on)).set_visible(cx, on && inner);
        self.view.view(cx, ids!(inner_term_pane)).set_visible(cx, inner);
        self.view.view(cx, ids!(peer_messages_pane)).set_visible(cx, !inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cli_is_one_quoted_shell_line() {
        let cli = Cli { env: vec![("K".into(), "a b".into())], unset: vec!["X".into()], argv: vec!["claude".into(), "--resume".into(), "it's".into()] };
        assert_eq!(cli.line(), "exec env -u X K='a b' 'claude' '--resume' 'it'\\''s'");
    }
}
