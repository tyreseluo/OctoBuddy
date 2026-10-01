//! The engine picker under the composer, as Cindy's: first which loop (the
//! outer loop, or the inner loops a session's new slices get), then an
//! agent on the left — Claude Code, octos, Codex, pi — and its models on the
//! right. An agent with no model to pick there (octos for inner loops: it
//! runs on the AI providers' own choice) takes effect when chosen.
use crate::model::{effort_levels, effort_word, OuterPick, SessionRef};
use crate::{i18n, providers, rpc_lead, system_chat, tapped, workspace, OctoBuddyView, CLAUDE_MODELS};
use makepad_widgets::*;

const AGENTS: [(&str, LiveId); 4] = [("claude", live_id!(pa_claude)), ("octos", live_id!(pa_octos)), (rpc_lead::CODEX, live_id!(pa_codex)), (rpc_lead::PI, live_id!(pa_pi))];
const MODELS: [LiveId; 8] = [live_id!(pm0), live_id!(pm1), live_id!(pm2), live_id!(pm3), live_id!(pm4), live_id!(pm5), live_id!(pm6), live_id!(pm7)];
/// The effort chips: the agent's own setting, then its levels.
const EFFORTS: [LiveId; 8] = [live_id!(pe0), live_id!(pe1), live_id!(pe2), live_id!(pe3), live_id!(pe4), live_id!(pe5), live_id!(pe6), live_id!(pe7)];

fn installed(agent: &str) -> bool {
    match agent {
        "claude" | "octos" => true,
        other => workspace::find_bin(other).is_file(),
    }
}

impl OctoBuddyView {
    /// What `agent` offers for the outer loop (`inner`: for new inner loops)
    /// of the session at `at`: each choice's label and pick.
    fn agent_models(&self, at: SessionRef, inner: bool, agent: &str) -> Vec<(String, OuterPick)> {
        let routes = providers::claude_capable();
        let pick = |engine: &str, model: Option<String>| OuterPick { engine: engine.into(), model };
        match (agent, inner) {
            ("claude", false) => CLAUDE_MODELS.iter().map(|m| (m.unwrap_or(i18n::t("default (its settings)", "默认（它的设置）")).to_string(), pick("claude", m.map(String::from))))
                .chain(routes.iter().map(|r| (r.clone(), pick("claude", Some(r.clone()))))).collect(),
            ("claude", true) => std::iter::once((i18n::t("your login (default)", "你的登录（默认）").to_string(), pick("claude", None)))
                .chain(routes.iter().map(|r| (r.clone(), pick("claude", Some(r.clone()))))).collect(),
            ("octos", false) => {
                let plain = self.store.is_plain(at);
                self.octos_choices().into_iter().map(|(label, m)| match (plain, &m) {
                    (true, None) => {
                        let model = self.system_model().or_else(|| self.providers.primary().map(|p| p.label.clone())).unwrap_or_default();
                        (i18n::pick(format!("default · {model} (OctoSense's agent)"), format!("默认 · {model}（OctoSense 自带的 agent）")), pick(system_chat::ENGINE, None))
                    }
                    _ => (label, pick("octos", m)),
                }).collect()
            }
            // octos inner loops run on the AI providers' choice: nothing to pick.
            ("octos", true) => Vec::new(),
            (engine, _) => routes.iter().map(|r| (r.clone(), pick(engine, Some(r.clone())))).collect(),
        }
    }

    /// What the side shown runs on now.
    fn picked(&self, at: SessionRef, inner: bool) -> OuterPick {
        let Some(s) = self.store.session(at) else { return OuterPick { engine: "claude".into(), model: None } };
        if inner {
            OuterPick { engine: s.inner_engine().into(), model: s.inner_model().map(String::from) }
        } else {
            OuterPick { engine: s.engine().into(), model: s.outer_model().map(String::from) }
        }
    }

    fn agent_name(agent: &str) -> &str {
        match agent {
            "claude" => "Claude Code",
            rpc_lead::CODEX => "Codex",
            other => other,
        }
    }

    pub(crate) fn sync_picker(&mut self, cx: &mut Cx, at: SessionRef) {
        self.view.view(cx, ids!(outer_picker)).set_visible(cx, self.show_picker);
        if !self.show_picker {
            return;
        }
        // Over the conversation, above its button (corrected once drawn).
        let (button, window) = (self.view.view(cx, ids!(outer_pick)).area().rect(cx), self.view.area().rect(cx));
        let drawn = self.view.view(cx, ids!(outer_picker)).area().rect(cx).size;
        let size = if drawn.y > 0.0 { drawn } else { dvec2(640.0, 380.0) };
        let pos = crate::above(button, size, window);
        let mut w = self.view.widget(cx, ids!(outer_picker));
        script_apply_eval!(cx, w, { abs_pos: #(pos) });
        // A plain chat has no inner loops.
        let plain = self.store.is_plain(at);
        let inner = self.pick_inner && !plain;
        for (on, off, this) in [(ids!(pk_outer_on), ids!(pk_outer), false), (ids!(pk_inner_on), ids!(pk_inner), true)] {
            self.view.button(cx, on).set_visible(cx, inner == this && !(this && plain));
            self.view.button(cx, off).set_visible(cx, inner != this && !(this && plain));
        }
        let t = |en: &'static str, zh: &'static str| i18n::t(en, zh);
        for (id, text) in [(ids!(pk_outer_on), t("Outer loop", "外环")), (ids!(pk_outer), t("Outer loop", "外环")),
            (ids!(pk_inner_on), t("Inner loops (new ones)", "内环（之后新开的 inner）")), (ids!(pk_inner), t("Inner loops (new ones)", "内环（之后新开的 inner）"))] {
            self.view.button(cx, id).set_text(cx, text);
        }
        let now = self.picked(at, inner);
        // The system agent is octos too.
        let now_agent = if now.engine == system_chat::ENGINE { "octos".to_string() } else { now.engine.clone() };
        let shown = self.pick_agent.clone().unwrap_or_else(|| now_agent.clone());
        let now_label = self.agent_models(at, inner, &now_agent).into_iter().find(|(_, p)| *p == now).map(|(l, _)| l)
            .or(now.model.clone()).unwrap_or_else(|| t("default", "默认").into());
        self.view.label(cx, ids!(pk_current)).set_text(cx, &i18n::pick(format!("now: {} · {now_label}", Self::agent_name(&now_agent)), format!("当前：{} · {now_label}", Self::agent_name(&now_agent))));
        for (agent, id) in AGENTS {
            let row = self.view.view(cx, &[id]);
            let models = self.agent_models(at, inner, agent);
            // Codex and pi run on a provider: none there, no row.
            let offered = agent == "claude" || agent == "octos" || !models.is_empty();
            row.set_visible(cx, offered);
            let sub = if !installed(agent) {
                t("not installed", "未安装").to_string()
            } else if agent == now_agent {
                i18n::pick(format!("in use · {now_label}"), format!("使用中 · {now_label}"))
            } else if models.is_empty() {
                t("the AI providers' choice", "用 AI providers 的选择").to_string()
            } else {
                i18n::pick(format!("{} model(s)", models.len()), format!("{} 个模型", models.len()))
            };
            row.label(cx, ids!(name)).set_text(cx, Self::agent_name(agent));
            row.label(cx, ids!(sub)).set_text(cx, &sub);
            row.label(cx, ids!(arrow)).set_text(cx, if models.is_empty() { "" } else { "›" });
            let bg = if agent == shown { crate::theme::vec4("accent_selected") } else { crate::theme::vec4("raised") };
            let mut w = row.clone();
            script_apply_eval!(cx, w, { draw_bg +: {color: #(bg)} });
        }
        // The models of the agent shown.
        let models = if installed(&shown) { self.agent_models(at, inner, &shown) } else { Vec::new() };
        self.view.label(cx, ids!(pm_head)).set_text(cx, &i18n::pick(format!("{} · models", Self::agent_name(&shown)), format!("{} · 模型", Self::agent_name(&shown))));
        for (i, id) in MODELS.iter().enumerate() {
            let row = self.view.view(cx, &[*id]);
            let m = models.get(i);
            row.set_visible(cx, m.is_some());
            if let Some((label, p)) = m {
                let on = *p == now;
                row.label(cx, ids!(label)).set_text(cx, label);
                row.label(cx, ids!(mark)).set_text(cx, if on { "✓" } else { "" });
                let bg = if on { crate::theme::vec4("accent_soft") } else { crate::theme::vec4("raised") };
                let mut w = row.clone();
                script_apply_eval!(cx, w, { draw_bg +: {color: #(bg)} });
            }
        }
        let none = if !installed(&shown) {
            i18n::pick(format!("{} is not installed: Settings › Tools says where it comes from.", Self::agent_name(&shown)), format!("{} 未安装：来源见 设置 › 工具。", Self::agent_name(&shown)))
        } else if models.is_empty() {
            t("Nothing to pick: it runs on the model AI providers make primary. Choosing octos on the left applies it.", "无需选择模型：它用 AI providers 的主模型。在左侧点 octos 即生效。").to_string()
        } else {
            String::new()
        };
        self.view.label(cx, ids!(pm_none)).set_text(cx, &none);
        self.sync_efforts(cx, at, inner, &now, &now_agent, &shown);
        self.view.label(cx, ids!(pick_soon)).set_text(cx, "");
        let note = if inner {
            t("New inner loops of this session run on it; the ones already at work keep their engine.", "这个会话之后新开的 inner 用它；已经开过的 inner 保持原来的引擎。").to_string()
        } else if plain {
            let ready = if self.system_ready() { String::new() } else { format!(" {}", self.system_unready_text()) };
            i18n::pick(format!("octos's default runs on OctoSense's own agent; another model runs on OctoBuddy's octos.{ready}"),
                format!("octos 的默认项用 OctoSense 自带的 agent；选其他模型则在 OctoBuddy 自己的 octos 上运行。{ready}"))
        } else {
            t("Another model of the same agent keeps the conversation; another agent starts the outer loop anew.", "同一 agent 换模型会保留对话；换 agent 会让外环从头开始。").to_string()
        };
        self.view.label(cx, ids!(pick_note)).set_text(cx, &note);
    }

    /// The effort chips under the models: the levels of the agent in use
    /// (Cindy's 思考强度); another agent's, once one of its models is picked.
    fn sync_efforts(&mut self, cx: &mut Cx, at: SessionRef, inner: bool, now: &OuterPick, now_agent: &str, shown: &str) {
        let levels = effort_levels(&now.engine);
        let other = shown != now_agent && !effort_levels(shown).is_empty();
        self.view.view(cx, ids!(pe_row)).set_visible(cx, (shown == now_agent && !levels.is_empty()) || other);
        let current = self.store.session(at).and_then(|s| if inner { s.inner_effort() } else { s.outer_effort() }).map(String::from);
        let head = if other {
            i18n::pick(format!("Effort: pick a model of {} first", Self::agent_name(shown)), format!("思考强度：先选一个 {} 的模型", Self::agent_name(shown)))
        } else {
            i18n::t("Effort (how hard it thinks; some providers may ignore it)", "思考强度（effort；部分 provider 可能不支持）").to_string()
        };
        self.view.label(cx, ids!(pe_head)).set_text(cx, &head);
        let choices: Vec<Option<&str>> = if other { Vec::new() } else { std::iter::once(None).chain(levels.iter().map(|l| Some(*l))).collect() };
        for (i, id) in EFFORTS.iter().enumerate() {
            let chip = self.view.view(cx, &[*id]);
            let choice = choices.get(i);
            chip.set_visible(cx, choice.is_some());
            if let Some(level) = choice {
                let on = level.map(String::from) == current;
                chip.label(cx, ids!(label)).set_text(cx, effort_word(level.unwrap_or("")));
                let (bg, edge) = if on { (crate::theme::vec4("accent_selected"), crate::theme::vec4("accent_line")) } else { (crate::theme::vec4("raised"), crate::theme::vec4("line")) };
                let mut w = chip.clone();
                script_apply_eval!(cx, w, { draw_bg +: {color: #(bg) border_color: #(edge)} });
            }
        }
    }

    /// The effort for the outer loop (`inner`: for new inner loops) of the
    /// session at `at`. Its process takes it when it starts: started again
    /// at the next message (now if idle, else once its turn ends), the
    /// conversation kept.
    fn pick_effort(&mut self, cx: &mut Cx, at: SessionRef, inner: bool, effort: Option<String>) {
        let word = effort_word(effort.as_deref().unwrap_or(""));
        let Some(s) = self.store.session_mut(at) else { return };
        let id = s.id.clone();
        if inner {
            s.inner_effort = effort;
            self.system(at, &i18n::pick(format!("New inner loops of this session think at effort: {word}."), format!("这个会话之后新开的 inner 用思考强度：{word}。")));
        } else {
            s.outer_effort = effort;
            let busy = self.lead_busy(&id);
            if busy {
                self.respawn.insert(id);
            } else {
                self.rt.leads.remove(&id);
            }
            let when = if busy { i18n::t("once this turn is done", "这一轮结束后") } else { i18n::t("from your next message", "从下一条消息起") };
            self.system(at, &i18n::pick(format!("Effort: {word}, {when} (the conversation is kept)."), format!("思考强度改为 {word}，{when}生效（保留原对话）。")));
        }
        self.save();
        self.relayout(cx);
    }

    /// New inner loops of the session at `at` run on `pick`.
    fn pick_inner_engine(&mut self, cx: &mut Cx, at: SessionRef, pick: OuterPick) {
        let label = format!("{} · {}", Self::agent_name(&pick.engine), pick.model.as_deref().unwrap_or(i18n::t("default", "默认")));
        let label = if pick.engine == "octos" { "octos".to_string() } else { label };
        if let Some(s) = self.store.session_mut(at) {
            s.inner = (pick.engine != "octos").then_some(pick);
        }
        self.show_picker = false;
        self.system(at, &i18n::pick(format!("New inner loops of this session run on {label}; the ones already at work keep their engine."),
            format!("这个会话之后新开的 inner 用 {label}；已经开过的 inner 保持原来的引擎。")));
        self.save();
        self.relayout(cx);
    }

    pub(crate) fn picker_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        if !self.show_picker {
            return;
        }
        for (id, inner) in [(ids!(pk_outer), false), (ids!(pk_inner), true)] {
            if self.view.button(cx, id).clicked(actions) {
                self.pick_inner = inner;
                self.pick_agent = None;
                self.relayout(cx);
                return;
            }
        }
        let inner = self.pick_inner && !self.store.is_plain(at);
        for (agent, id) in AGENTS {
            if !tapped(&self.view.view(cx, &[id]), actions) {
                continue;
            }
            let models = if installed(agent) { self.agent_models(at, inner, agent) } else { Vec::new() };
            if models.is_empty() && installed(agent) && agent == "octos" && inner {
                // Nothing to pick: chosen.
                self.pick_inner_engine(cx, at, OuterPick { engine: "octos".into(), model: None });
            } else {
                self.pick_agent = Some(agent.to_string());
                self.relayout(cx);
            }
            return;
        }
        // An effort chip: the agent's own setting, or one of its levels.
        let now = self.picked(at, inner);
        let levels = effort_levels(&now.engine);
        for (i, id) in EFFORTS.iter().enumerate() {
            if tapped(&self.view.view(cx, &[*id]), actions) {
                let effort = if i == 0 { None } else { levels.get(i - 1).map(|l| l.to_string()) };
                self.pick_effort(cx, at, inner, effort);
                return;
            }
        }
        let shown = self.pick_agent.clone().unwrap_or_else(|| {
            let now = self.picked(at, inner);
            if now.engine == system_chat::ENGINE { "octos".into() } else { now.engine }
        });
        let models = self.agent_models(at, inner, &shown);
        for (i, id) in MODELS.iter().enumerate() {
            if !tapped(&self.view.view(cx, &[*id]), actions) {
                continue;
            }
            let Some((_, pick)) = models.get(i).cloned() else { return };
            self.pick_agent = None;
            if inner {
                self.pick_inner_engine(cx, at, pick);
            } else {
                self.pick_outer(cx, at, pick);
            }
            return;
        }
    }
}
