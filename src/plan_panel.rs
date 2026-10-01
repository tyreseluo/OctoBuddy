//! The plan panel: floats over the conversation and the flow canvas (the
//! person drags it by its header) and shows the outer loop's current round —
//! the intent it worked from, the slices it cut, which peer agent took each,
//! how far each has come, and its estimate against the time it really took.
use crate::estimate::{self, Calibration, MIN_PER_ROUND};
use crate::model::{now_secs, Peer, Role, Session};
use crate::{hex_color, i18n, status_word, FlowOpen, OctoBuddyView, Stage};
use makepad_widgets::*;

/// The panel's rows (`pr0`…`pr7`).
const PLAN_ROWS: [LiveId; 8] = [live_id!(pr0), live_id!(pr1), live_id!(pr2), live_id!(pr3), live_id!(pr4), live_id!(pr5), live_id!(pr6), live_id!(pr7)];

/// A slice's name: its agent-spec task name, else its brief's first line.
fn task_name(p: &Peer) -> String {
    let brief = p.brief.trim();
    // A contract's header: `spec: task` then `name: "…"`.
    let named = brief.lines().take(4).find_map(|l| l.trim().strip_prefix("name:")).map(|n| n.trim().trim_matches('"').trim().to_string());
    if let Some(name) = named.filter(|n| !n.is_empty()) {
        return name.chars().take(40).collect();
    }
    let first = brief.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("spec:") && !l.starts_with('#')).unwrap_or(&p.slug);
    first.chars().take(40).collect()
}

/// The round on show: the latest one the outer loop planned.
fn round_peers(session: &Session) -> Option<(u64, Vec<&Peer>)> {
    let planned = |p: &&Peer| p.by_person != Some(true) && p.log().first().is_some_and(|e| e.from == "lead");
    let round = session.peers().iter().filter(planned).map(|p| p.round).max()?;
    let peers: Vec<&Peer> = session.peers().iter().filter(planned).filter(|p| p.round == round).collect();
    Some((round, peers))
}

/// Where a slice stands: its words and its dot's colour.
fn standing(p: &Peer) -> (String, u32) {
    match (p.status.as_str(), p.review.as_deref().map(|r| r.split(':').next().unwrap_or("").trim())) {
        ("running", _) => (status_word("running").into(), 0x1a7f37),
        ("queued", _) => (status_word("queued").into(), 0xd4a72c),
        ("checking", _) => (status_word("checking").into(), 0x0969da),
        ("failed", _) => (status_word("failed").into(), 0xcf222e),
        ("interrupted", _) => (status_word("interrupted").into(), 0x8c959f),
        (_, Some("accept")) => (i18n::t("accepted", "已接受").into(), 0x2f6feb),
        (_, Some("fix")) => (i18n::t("to fix", "待修复").into(), 0xe16f24),
        ("closed", _) => (status_word("closed").into(), 0x8c959f),
        _ => (i18n::t("done, under review", "已完成，待审查").into(), 0x8250df),
    }
}

impl OctoBuddyView {
    /// Fills the plan panel, or hides it.
    pub(crate) fn sync_plan(&mut self, cx: &mut Cx) {
        let panel = self.view.view(cx, ids!(plan_panel));
        let at = self.selected.filter(|at| self.store.session(*at).is_some());
        let shown = at.filter(|_| matches!(self.stage, Stage::Chat | Stage::Flow) && self.page == crate::Page::Chat);
        let Some(at) = shown else {
            panel.set_visible(cx, false);
            return;
        };
        let session = self.store.session(at).unwrap().clone();
        let Some((round, peers)) = round_peers(&session) else {
            panel.set_visible(cx, false);
            return;
        };
        if self.plan_hidden.as_ref() == Some(&(session.id.clone(), round)) {
            panel.set_visible(cx, false);
            return;
        }
        panel.set_visible(cx, true);
        let now = now_secs();
        let calibration = estimate::calibrate(self.store.projects[at.0].sessions.iter());
        // The intent: what the person asked for, last before the round began.
        let began = peers.iter().filter_map(|p| p.log().first().map(|e| e.at)).min().unwrap_or(now);
        let intent = session.messages.iter().rev().find(|m| m.role() == Role::User && m.at <= began)
            .map(|m| m.text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(140).collect::<String>())
            .unwrap_or_default();
        let done = peers.iter().filter(|p| !p.is_active() && p.log().iter().any(|e| e.outcome.is_some())).count();
        let ended = peers.iter().all(|p| !p.is_active());
        let last = peers.iter().flat_map(|p| p.log().iter().map(|e| e.at + e.took.unwrap_or(0))).max().unwrap_or(now);
        let elapsed = (if ended { last } else { now }).saturating_sub(began) as f64 / 60.0;
        let est = session.estimate.clone().unwrap_or_default();
        self.view.label(cx, ids!(plan_title)).set_text(cx, &i18n::pick(format!("Plan · round {round}"), format!("任务切分 · 第 {round} 轮")));
        let progress = i18n::pick(format!("{done}/{} done · {}", peers.len(), estimate::minutes_text(elapsed)),
            format!("{done}/{} 完成 · 用时 {}", peers.len(), estimate::minutes_text(elapsed)));
        self.view.label(cx, ids!(plan_progress)).set_text(cx, &progress);
        let mut about = if intent.is_empty() { String::new() } else { i18n::pick(format!("Intent: {intent}"), format!("意图：{intent}")) };
        if !est.is_empty() {
            about.push_str(&i18n::pick(format!("\nEstimate (agent-estimation): {est}"), format!("\n估算（agent-estimation）：{est}")));
        }
        if let Some(c) = calibration {
            about.push_str(&i18n::pick(
                format!("\nCalibrated by {} finished slice(s): a round takes {} here ({:.1}×)", c.n, estimate::minutes_text(c.min_per_round), c.factor()),
                format!("\n已用 {} 个完成的切片校准：这里一轮实际约 {}（{:.1}×）", c.n, estimate::minutes_text(c.min_per_round), c.factor())));
        }
        self.view.label(cx, ids!(plan_intent)).set_text(cx, &about);
        self.view.view(cx, ids!(plan_body)).set_visible(cx, !self.plan_folded);
        self.view.button(cx, ids!(plan_fold)).set_text(cx, if self.plan_folded { "+" } else { "–" });
        self.plan_peers = peers.iter().map(|p| p.id.clone()).collect();
        for (i, row) in PLAN_ROWS.iter().enumerate() {
            let view = self.view.view(cx, &[live_id!(plan_panel), *row]);
            let Some(p) = peers.get(i) else {
                view.set_visible(cx, false);
                continue;
            };
            view.set_visible(cx, true);
            let (words, color) = standing(p);
            self.view.label(cx, &[live_id!(plan_panel), *row, live_id!(name)]).set_text(cx, &task_name(p));
            let wave = p.wave.map(|w| i18n::pick(format!(" · wave {w}"), format!(" · 第 {w} 波"))).unwrap_or_default();
            self.view.label(cx, &[live_id!(plan_panel), *row, live_id!(who)]).set_text(cx, &format!("{}{wave}", p.slug));
            self.view.label(cx, &[live_id!(plan_panel), *row, live_id!(meta)]).set_text(cx, &slice_line(p, &words, calibration, now));
            let mut dot = self.view.widget(cx, &[live_id!(plan_panel), *row, live_id!(dot)]);
            let c = hex_color(color);
            script_apply_eval!(cx, dot, { draw_bg +: {color: #(c)} });
        }
        self.view.label(cx, ids!(plan_more)).set_text(cx, &if peers.len() > PLAN_ROWS.len() {
            i18n::pick(format!("and {} more", peers.len() - PLAN_ROWS.len()), format!("还有 {} 个", peers.len() - PLAN_ROWS.len()))
        } else {
            String::new()
        });
        // Where the person put it, kept inside the stage; at first, top right.
        let stage = self.view.view(cx, ids!(stage)).area().rect(cx);
        if stage.size.x > 0.0 {
            let w = 340.0;
            let pos = self.plan_pos.unwrap_or(dvec2(stage.pos.x + stage.size.x - w - 14.0, stage.pos.y + 80.0));
            // Whole, inside the stage (its height as last drawn).
            let h = self.view.view(cx, ids!(plan_panel)).area().rect(cx).size.y.max(40.0);
            let pos = dvec2(pos.x.clamp(stage.pos.x, (stage.pos.x + stage.size.x - w).max(stage.pos.x)),
                pos.y.clamp(stage.pos.y, (stage.pos.y + stage.size.y - h).max(stage.pos.y)));
            let mut w = self.view.widget(cx, ids!(plan_panel));
            script_apply_eval!(cx, w, { abs_pos: #(pos) });
        }
    }

    /// Its header dragged, folded or closed; a row opens its peer.
    pub(crate) fn handle_plan_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let head = self.view.view(cx, ids!(plan_head));
        if let Some(e) = head.finger_down(actions) {
            let at = self.view.view(cx, ids!(plan_panel)).area().rect(cx).pos;
            self.plan_drag = Some(e.abs - at);
        }
        if let (Some(e), Some(grab)) = (head.finger_move(actions), self.plan_drag) {
            self.plan_pos = Some(e.abs - grab);
            self.sync_plan(cx);
            cx.redraw_all();
        }
        if head.finger_up(actions).is_some() {
            self.plan_drag = None;
        }
        if self.view.button(cx, ids!(plan_fold)).clicked(actions) {
            self.plan_folded = !self.plan_folded;
            self.sync_plan(cx);
            cx.redraw_all();
        }
        if self.view.button(cx, ids!(plan_close)).clicked(actions) {
            let shown = self.selected.and_then(|at| self.store.session(at)).and_then(|s| round_peers(s).map(|(r, _)| (s.id.clone(), r)));
            self.plan_hidden = shown;
            self.sync_plan(cx);
            cx.redraw_all();
        }
        let picked = PLAN_ROWS.iter().position(|row| crate::tapped(&self.view.view(cx, &[live_id!(plan_panel), *row]), actions));
        if let Some(id) = picked.and_then(|i| self.plan_peers.get(i)).cloned() {
            if self.stage == Stage::Flow {
                self.flow_open = Some(FlowOpen::Inner(id));
            } else {
                self.selected_peer = Some(id);
                self.show_inner = true;
                self.show_preview = false;
            }
            self.relayout(cx);
        }
    }
}

/// A slice's line: where it stands, its time against the estimate, what it does.
fn slice_line(p: &Peer, words: &str, calibration: Option<Calibration>, now: u64) -> String {
    let mut parts = vec![words.to_string()];
    let actual = estimate::actual_minutes(p, now);
    if let Some(est) = p.estimate {
        let mut e = i18n::pick(format!("est. {est} round(s) ≈ {}", estimate::minutes_text(est * MIN_PER_ROUND)),
            format!("估 {est} 轮≈{}", estimate::minutes_text(est * MIN_PER_ROUND)));
        if let Some(c) = calibration {
            e.push_str(&i18n::pick(format!(" (calibrated ≈ {})", estimate::minutes_text(est * c.min_per_round)),
                format!("（校准≈{}）", estimate::minutes_text(est * c.min_per_round))));
        }
        parts.push(e);
    }
    if let Some(m) = actual.filter(|m| *m > 0.0) {
        parts.push(i18n::pick(format!("actual {}", estimate::minutes_text(m)), format!("实际 {}", estimate::minutes_text(m))));
    }
    if let Some(used) = p.rounds_used {
        parts.push(i18n::pick(format!("{used} calls"), format!("{used} 次调用")));
    }
    let activity = p.activity.as_deref().filter(|_| p.is_active()).unwrap_or("").lines().next().unwrap_or("").chars().take(48).collect::<String>();
    if !activity.is_empty() {
        parts.push(activity);
    }
    parts.join(" · ")
}
