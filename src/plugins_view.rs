//! Plugins in the window: the external plugins' buttons over the
//! conversation, Settings › Plugins (each plugin, what it adds, its switch)
//! and Settings › Tools (what OctoBuddy runs on, its versions), and the
//! outer loop's `octobuddy_plugin` calls. The plugins themselves: `plugins.rs`.
use crate::events::{self, LoopEvent};
use crate::{agents, i18n, plugins, tapped, tools_info, OctoBuddyView};
use makepad_widgets::*;
use serde_json::Value;
use std::sync::mpsc;

const SLOTS: [LiveId; 3] = [live_id!(plug_btn0), live_id!(plug_btn1), live_id!(plug_btn2)];
const TOOL_CARDS: [LiveId; 12] = [live_id!(tool0), live_id!(tool1), live_id!(tool2), live_id!(tool3), live_id!(tool4), live_id!(tool5),
    live_id!(tool6), live_id!(tool7), live_id!(tool8), live_id!(tool9), live_id!(tool10), live_id!(tool11)];

impl OctoBuddyView {
    /// The external plugins' buttons for the project shown (three at most).
    pub(crate) fn sync_plugin_buttons(&mut self, cx: &mut Cx, project: Option<&str>) {
        let buttons: Vec<(String, plugins::Button)> = project.map(plugins::external_active).unwrap_or_default().into_iter()
            .flat_map(|p| p.buttons.into_iter().map(move |b| (p.id.clone(), b))).take(SLOTS.len()).collect();
        self.plugin_slots = buttons.iter().map(|(p, b)| (p.clone(), b.id.clone())).collect();
        for (i, slot) in SLOTS.iter().enumerate() {
            let shown = buttons.get(i);
            self.view.view(cx, &[*slot]).set_visible(cx, shown.is_some());
            if let Some((_, b)) = shown {
                self.view.label(cx, &[*slot, live_id!(name)]).set_text(cx, &b.name);
                self.view.label(cx, &[*slot, live_id!(sub)]).set_text(cx, &b.sub);
            }
        }
    }

    /// A plugin's button pressed: its program runs (off the UI thread), and
    /// what it says comes to the session.
    pub(crate) fn plugin_button_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        for (i, slot) in SLOTS.iter().enumerate() {
            if !tapped(&self.view.view(cx, &[*slot]), actions) {
                continue;
            }
            let Some((plugin, button)) = self.plugin_slots.get(i).cloned() else { continue };
            let Some(p) = plugins::all().into_iter().find(|p| p.id == plugin) else { continue };
            let project = self.store.projects[at.0].path.clone();
            let session = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
            self.system(at, &i18n::pick(format!("Plugin {}: {}…", p.name, button), format!("插件 {}：{}…", p.name, button)));
            let inbox = self.rt.inbox.clone();
            std::thread::spawn(move || {
                let input = serde_json::json!({"project": project, "session": session});
                let result = plugins::run(&p, "action", &button, &input, &project);
                events::post(&inbox, LoopEvent::PluginSaid { session, plugin: p.name, result });
            });
            self.relayout(cx);
        }
    }

    /// The outer loop's `octobuddy_plugin {plugin, tool, args}`: run off the
    /// UI thread, its answer to `reply`.
    pub(crate) fn plugin_mcp_call(&mut self, session: &str, args: &Value, reply: mpsc::Sender<Result<String, String>>) {
        let Some(at) = self.store.find_session(session) else {
            let _ = reply.send(Err("no such session".into()));
            return;
        };
        let project = self.store.projects[at.0].path.clone();
        let (id, tool) = (args.get("plugin").and_then(Value::as_str).unwrap_or("").to_string(), args.get("tool").and_then(Value::as_str).unwrap_or("").to_string());
        let input = args.get("args").cloned().unwrap_or(Value::Object(Default::default()));
        let found = plugins::external_active(&project).into_iter().find(|p| p.id == id);
        let Some(p) = found else {
            let _ = reply.send(Err(format!("no plugin {id} at work for this project (see PLUGINS in your rules)")));
            return;
        };
        if !p.tools.iter().any(|t| t.name == tool) {
            let names: Vec<&str> = p.tools.iter().map(|t| t.name.as_str()).collect();
            let _ = reply.send(Err(format!("{id} has no tool {tool}; it has: {}", names.join(", "))));
            return;
        }
        std::thread::spawn(move || {
            let _ = reply.send(plugins::run(&p, "tool", &tool, &input, &project));
        });
    }

    pub(crate) fn plugin_said(&mut self, session: &str, plugin: &str, result: Result<String, String>) {
        let Some(at) = self.store.find_session(session) else { return };
        let text = match result {
            Ok(out) if out.is_empty() => i18n::pick(format!("Plugin {plugin}: done."), format!("插件 {plugin}：完成。")),
            Ok(out) => i18n::pick(format!("Plugin {plugin}:\n\n{out}"), format!("插件 {plugin}：\n\n{out}")),
            Err(err) => i18n::pick(format!("Plugin {plugin} failed: {err}"), format!("插件 {plugin} 出错：{err}")),
        };
        self.system(at, &text);
    }

    /// Settings › Plugins and Tools: their hints and lists.
    pub(crate) fn sync_plugins_page(&mut self, cx: &mut Cx) {
        self.plugin_rows = plugins::all();
        self.view.label(cx, ids!(plugins_title)).set_text(cx, i18n::t("Plugins", "插件"));
        self.view.label(cx, ids!(tools_title)).set_text(cx, i18n::t("Tools", "工具"));
        let dir = plugins::dir();
        self.view.label(cx, ids!(plugins_hint)).set_text(cx, &i18n::pick(
            format!("What a kind of project adds to OctoBuddy: buttons over the conversation, tools for the agents, rules for the outer loop. A plugin is at work where it applies (and the plugins it needs are on). Built-in ones come with OctoBuddy; others are a folder with a plugin.json in {}.", dir.display()),
            format!("某类项目给 OctoBuddy 增加的东西：对话上方的按钮、给 agent 的工具、外环的规则。插件只在适用的项目里生效（它依赖的插件也要开着）。内置插件随 OctoBuddy 提供；外部插件是 {} 下带 plugin.json 的文件夹。", dir.display())));
        self.view.label(cx, ids!(tools_hint)).set_text(cx, i18n::t(
            "What OctoBuddy runs on, the versions found on this machine, and where each comes from. Claude Code, Codex, pi and octos: OctoBuddy keeps its own copy of each, at the version it was tested with (installed the first time it is needed, checked against its publisher's digest), or runs yours. A change applies to the agents started after it.",
            "OctoBuddy 依赖的工具：本机找到的版本，以及各自的来源。Claude Code、Codex、pi 和 octos 可以用 OctoBuddy 自带的版本（测试过的版本，第一次用到时安装，并按发布方的摘要校验），也可以用你自己装的。切换只影响之后启动的 agent。"));
        self.view.portal_list(cx, ids!(plugin_list)).redraw(cx);
        self.sync_tool_grid(cx);
    }

    pub(crate) fn probe_tools(&mut self) {
        let (inbox, lang) = (self.rt.inbox.clone(), i18n::lang());
        std::thread::spawn(move || {
            i18n::set(lang);
            events::post(&inbox, LoopEvent::ToolsProbed(tools_info::probe()));
        });
    }

    pub(crate) fn tools_probed(&mut self, tools: Vec<tools_info::ToolInfo>) {
        self.tool_rows = Some(tools);
    }

    /// OctoBuddy's copy of an agent's program is in (or could not be): the
    /// open session hears it when it was waiting for it, and Tools shows it.
    pub(crate) fn agent_installed(&mut self, name: &str, result: Result<String, String>, awaited: bool) {
        let pin = agents::pin(name);
        let (label, version) = (pin.map(|p| p.label).unwrap_or(name), pin.map(|p| p.version).unwrap_or(""));
        match &result {
            Ok(path) => log!("octobuddy: installed {label} {version}: {path}"),
            Err(err) => log!("octobuddy: could not install {label} {version}: {err}"),
        }
        if let Some(at) = self.selected.filter(|at| awaited && self.store.session(*at).is_some()) {
            let text = match &result {
                Ok(_) => i18n::pick(format!("{label} {version} is installed (OctoBuddy's copy): send again to start it."),
                    format!("{label} {version} 已安装（OctoBuddy 自带的）：再发一次就会启动。")),
                Err(err) => i18n::pick(format!("{label} could not be installed: {err}. Settings › Tools can try again, or install it yourself."),
                    format!("{label} 安装失败：{err}。可以在 设置 › 工具 里重试，或者自己安装。")),
            };
            self.system(at, &text);
        }
        self.probe_tools();
    }

    pub(crate) fn draw_plugin_list(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        list.set_item_range(cx, 0, self.plugin_rows.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(p) = self.plugin_rows.get(index).cloned() else { continue };
            let item = list.item(cx, index, id!(Plugin));
            let kind = if p.builtin() { i18n::t("built in", "内置") } else { i18n::t("installed", "外部") };
            let version = if p.version.is_empty() { String::new() } else { format!(" {}", p.version) };
            item.label(cx, ids!(name)).set_text(cx, &format!("{}{version} · {kind}", p.name));
            let detail = match &p.broken {
                Some(why) => i18n::pick(format!("Not run: {why}"), format!("无法运行：{why}")),
                None => p.description.clone(),
            };
            item.label(cx, ids!(detail)).set_text(cx, &detail);
            let mut adds = Vec::new();
            if !p.applies_to.is_empty() {
                adds.push(i18n::pick(format!("for projects with {}", p.applies_to.join(" or ")), format!("适用于含 {} 的项目", p.applies_to.join(" 或 "))));
            }
            if !p.buttons.is_empty() {
                let names: Vec<String> = p.buttons.iter().map(|b| b.name.clone()).collect();
                adds.push(i18n::pick(format!("button: {}", names.join(", ")), format!("按钮：{}", names.join("、"))));
            }
            if !p.tools.is_empty() {
                let names: Vec<&str> = p.tools.iter().map(|t| t.name.as_str()).collect();
                adds.push(i18n::pick(format!("tools: {}", names.join(", ")), format!("工具：{}", names.join("、"))));
            }
            if !p.requires.is_empty() {
                adds.push(i18n::pick(format!("needs {}", p.requires.join(", ")), format!("依赖 {}", p.requires.join("、"))));
            }
            item.label(cx, ids!(adds)).set_text(cx, &adds.join(" · "));
            let on = plugins::enabled(&p.id);
            item.button(cx, ids!(on)).set_visible(cx, on);
            item.button(cx, ids!(off)).set_visible(cx, !on);
            item.button(cx, ids!(on)).set_text(cx, i18n::t("On", "已启用"));
            item.button(cx, ids!(off)).set_text(cx, i18n::t("Off", "已停用"));
            item.draw_all_unscoped(cx);
        }
    }

    /// Settings › Tools: a card per tool, as many a row as fit.
    fn sync_tool_grid(&mut self, cx: &mut Cx) {
        let rows = self.tool_rows.clone().unwrap_or_default();
        for (i, slot) in TOOL_CARDS.iter().enumerate() {
            let card = self.view.view(cx, &[*slot]);
            card.set_visible(cx, rows.get(i).is_some());
            let Some(t) = rows.get(i) else { continue };
            card.label(cx, ids!(name)).set_text(cx, &t.name);
            card.label(cx, ids!(version)).set_text(cx, &t.version);
            card.label(cx, ids!(source)).set_visible(cx, !t.source.is_empty());
            card.label(cx, ids!(source)).set_text(cx, &t.source);
            card.view(cx, ids!(agent_row)).set_visible(cx, t.agent.as_deref().is_some_and(agents::installable));
            if let Some(name) = t.agent.as_deref() {
                let own = agents::prefers_own(name);
                for (id, show) in [(ids!(use_kept_on), !own), (ids!(use_kept), own), (ids!(use_own_on), own), (ids!(use_own), !own)] {
                    card.button(cx, id).set_visible(cx, show);
                }
                for id in [ids!(use_kept_on), ids!(use_kept)] {
                    card.button(cx, id).set_text(cx, i18n::t("OctoBuddy's", "自带的"));
                }
                for id in [ids!(use_own_on), ids!(use_own)] {
                    card.button(cx, id).set_text(cx, i18n::t("Yours", "你自己的"));
                }
                let can_install = !own && agents::kept(name).is_none() && agents::job(name) != Some(agents::Job::Installing);
                let retry = matches!(agents::job(name), Some(agents::Job::Failed(_)));
                card.button(cx, ids!(install)).set_visible(cx, can_install);
                card.button(cx, ids!(install)).set_text(cx, if retry { i18n::t("Try again", "重试") } else { i18n::t("Install", "安装") });
            }
            card.label(cx, ids!(what)).set_text(cx, &t.what);
            card.label(cx, ids!(repo)).set_text(cx, &t.repo);
            card.label(cx, ids!(path)).set_text(cx, &t.path);
        }
    }

    /// A plugin switched on or off; the folder opened; a rescan.
    pub(crate) fn plugins_page_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let list = self.view.portal_list(cx, ids!(plugin_list));
        let hit = list.items_with_actions(actions).into_iter()
            .find(|(_, item)| item.button(cx, ids!(on)).clicked(actions) || item.button(cx, ids!(off)).clicked(actions))
            .map(|(index, _)| index);
        if let Some(p) = hit.and_then(|i| self.plugin_rows.get(i)).cloned() {
            if plugins::off_by_default(&p.id) {
                // One that starts off: on is the person's choice.
                let mut on = self.store.enabled_plugins.clone().unwrap_or_default();
                if on.contains(&p.id) {
                    on.retain(|id| id != &p.id);
                } else {
                    on.push(p.id.clone());
                }
                plugins::set_enabled(&on);
                self.store.enabled_plugins = (!on.is_empty()).then_some(on);
                if !plugins::enabled(plugins::NATIVE_TUI) {
                    self.close_tui(cx);
                }
            } else {
                let mut off = self.store.disabled_plugins.clone().unwrap_or_default();
                if off.contains(&p.id) {
                    off.retain(|id| id != &p.id);
                } else {
                    off.push(p.id.clone());
                }
                plugins::set_disabled(&off);
                self.store.disabled_plugins = (!off.is_empty()).then_some(off);
            }
            // A slot whose plugin went off closes.
            if !plugins::enabled(plugins::APP_PREVIEW) {
                self.show_preview = false;
            }
            if !plugins::enabled(plugins::APP_DATA) {
                self.show_data = false;
            }
            self.save();
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(open_plugins)).clicked(actions) {
            let dir = plugins::dir();
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::process::Command::new("open").arg(&dir).spawn();
        }
        if self.view.button(cx, ids!(reload_plugins)).clicked(actions) {
            self.relayout(cx);
        }
        // An agent's card: which copy runs, OctoBuddy's installed.
        let rows = self.tool_rows.clone().unwrap_or_default();
        for (slot, t) in TOOL_CARDS.iter().zip(&rows) {
            let Some(name) = t.agent.as_deref() else { continue };
            let card = self.view.view(cx, &[*slot]);
            let own = if card.button(cx, ids!(use_own)).clicked(actions) {
                Some(true)
            } else if card.button(cx, ids!(use_kept)).clicked(actions) {
                Some(false)
            } else {
                None
            };
            if let Some(own) = own {
                agents::set_prefers_own(name, own);
            }
            let install = card.button(cx, ids!(install)).clicked(actions) || (own == Some(false) && agents::kept(name).is_none());
            if install {
                agents::install_in_background(&self.rt.inbox, name, false);
            }
            if own.is_some() || install {
                self.probe_tools();
                self.relayout(cx);
            }
        }
        if self.view.button(cx, ids!(reload_tools)).clicked(actions) {
            self.tool_rows = None;
            self.probe_tools();
            self.relayout(cx);
        }
    }
}
