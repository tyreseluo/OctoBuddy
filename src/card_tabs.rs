//! The open card's tabs on the flow canvas: its messages, its budget, and
//! its specs — every task file an outer loop's slices were given (agent-spec
//! contracts and plain briefs), which one is in force, and how far they are
//! applied. A spec opens to its file as written.
use crate::model::{Peer, SpecRef};
use crate::{estimate, i18n, FlowOpen, OctoBuddyView};
use makepad_widgets::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum CardTab {
    #[default]
    Messages,
    Budget,
    Specs,
}

/// A spec as its tab lists it.
#[derive(Clone, Debug, Default)]
pub(crate) struct SpecRow {
    pub path: String,
    pub name: String,
    pub meta: String,
    pub state: String,
    /// The last its inner loop was given: the one in force.
    pub in_force: bool,
    /// Its inner loop works on it now.
    pub working: bool,
    /// Done and accepted by the outer loop.
    pub applied: bool,
}

/// A peer's specs, oldest first (a contract from before the history: one).
fn specs_of(p: &Peer) -> Vec<SpecRef> {
    match (&p.specs, &p.contract) {
        (Some(v), _) if !v.is_empty() => v.clone(),
        (_, Some(path)) => vec![SpecRef { path: path.clone(), at: p.log().first().map(|e| e.at).unwrap_or(0), round: None, contract: true }],
        _ => Vec::new(),
    }
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

impl OctoBuddyView {
    /// What a peer's spec is at: in force or not, worked on, applied.
    fn spec_state(&self, p: &Peer, last: bool) -> (String, bool, bool) {
        let t = |en: &'static str, zh: &'static str| i18n::t(en, zh).to_string();
        if !last {
            return (t("earlier task", "之前的任务"), false, false);
        }
        let accepted = p.review.as_deref().is_some_and(|r| r.trim_start().starts_with("accept"));
        let failed = p.verdict.as_deref().is_some_and(|v| v.lines().any(|l| l.starts_with("[tests]") && l.contains("FAILED")));
        if self.rt.held.contains_key(&p.id) {
            (t("waiting for its wave", "等待所在 wave"), false, false)
        } else if matches!(p.status.as_str(), "running" | "queued") {
            (t("in progress", "进行中"), true, false)
        } else if p.status == "checking" {
            (t("being checked", "检查中"), true, false)
        } else if accepted {
            (t("applied ✓", "已应用 ✓"), false, true)
        } else if failed {
            (t("its check failed", "检查未通过"), false, false)
        } else if p.status == "closed" {
            (t("closed", "已关闭"), false, false)
        } else {
            (t("done, awaiting review", "已完成，待审查"), false, false)
        }
    }

    /// The open card's specs, and the line above them.
    pub(crate) fn card_spec_rows(&self) -> (String, Vec<SpecRow>) {
        let peers: Vec<Peer> = match &self.flow_open {
            Some(FlowOpen::Outer(id)) => self.store.projects.iter().flat_map(|p| &p.sessions).find(|s| &s.id == id)
                .map(|s| s.peers().iter().filter(|p| p.reviews_for.is_none()).cloned().collect()).unwrap_or_default(),
            Some(FlowOpen::Inner(id)) => self.store.find_peer(id).and_then(|at| self.store.session(at))
                .and_then(|s| s.peers().iter().find(|p| &p.id == id).cloned()).into_iter().collect(),
            _ => Vec::new(),
        };
        let outer = matches!(self.flow_open, Some(FlowOpen::Outer(_)));
        let mut rows: Vec<(u64, SpecRow)> = Vec::new();
        for p in &peers {
            let specs = specs_of(p);
            let n = specs.len();
            for (i, s) in specs.iter().enumerate() {
                let (state, working, applied) = self.spec_state(p, i + 1 == n);
                let kind = if s.contract { i18n::t("contract (agent-spec)", "契约（agent-spec）") } else { i18n::t("brief", "任务说明") };
                let round = s.round.map(|r| i18n::pick(format!(" · round {r}"), format!(" · 第 {r} 轮"))).unwrap_or_else(|| i18n::t(" · sent later", " · 后续派发").to_string());
                let who = if outer { format!("{} · ", p.slug) } else { String::new() };
                rows.push((s.at, SpecRow {
                    path: s.path.clone(),
                    name: file_name(&s.path),
                    meta: format!("{who}{kind}{round}"),
                    state,
                    in_force: i + 1 == n,
                    working,
                    applied,
                }));
            }
        }
        rows.sort_by_key(|(at, _)| *at);
        // Numbered in the order they were handed out.
        let rows: Vec<SpecRow> = rows.into_iter().enumerate().map(|(i, (_, mut r))| {
            r.meta = format!("#{} · {}", i + 1, r.meta);
            r
        }).collect();
        let head = if rows.is_empty() {
            i18n::t("No specs yet: each task an outer loop hands out is kept here as a file.", "还没有 spec：外环每派出一个任务，它的说明都会作为文件保存在这里。").to_string()
        } else if outer {
            let in_force: Vec<&SpecRow> = rows.iter().filter(|r| r.in_force).collect();
            let applied = in_force.iter().filter(|r| r.applied).count();
            let working: Vec<String> = in_force.iter().filter(|r| r.working).map(|r| r.name.trim_end_matches(".spec.md").trim_end_matches(".brief.md").to_string()).collect();
            let now = if working.is_empty() { i18n::t("nothing in progress", "没有进行中的").to_string() } else { i18n::pick(format!("in progress: {}", working.join(", ")), format!("进行中：{}", working.join("、"))) };
            i18n::pick(format!("Applied {applied} of {} spec(s) · {now}", in_force.len()), format!("已应用 {applied} / {} 份 spec · {now}", in_force.len()))
        } else {
            let last = rows.iter().rev().find(|r| r.in_force).cloned().unwrap_or_default();
            i18n::pick(format!("In force: {} — {} · {} spec(s) in all", last.name, last.state, rows.len()),
                format!("当前生效：{}（{}）· 共 {} 份", last.name, last.state, rows.len()))
        };
        (head, rows)
    }

    /// The open card's budget, in words.
    fn card_budget_text(&self) -> (String, String) {
        let now = crate::model::now_secs();
        match &self.flow_open {
            Some(FlowOpen::Inner(id)) => {
                let Some(p) = self.store.find_peer(id).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| &p.id == id)).cloned() else { return Default::default() };
                let used = p.rounds_used.unwrap_or(0);
                let cost = p.usage.as_ref().map(|u| u.cost).unwrap_or(0.0);
                let b = p.budget.unwrap_or_default();
                let limit = |v: Option<String>| v.unwrap_or_else(|| i18n::t("no limit", "不限").into());
                let steps = i18n::pick(format!("Steps: {used} / {}", limit(b.steps.map(|s| s.to_string()))), format!("步数：{used} / {}", limit(b.steps.map(|s| s.to_string()))));
                let dollars = i18n::pick(format!("Cost: ${cost:.3} / {}", limit(b.cost.map(|c| format!("${c:.2}")))), format!("费用：${cost:.3} / {}", limit(b.cost.map(|c| format!("${c:.2}")))));
                let share = [b.steps.filter(|s| *s > 0).map(|s| used as f64 / s as f64), b.cost.filter(|c| *c > 0.0).map(|c| cost / c)]
                    .into_iter().flatten().fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.max(v))));
                let share = match share {
                    Some(f) => {
                        let word = match crate::budget_tier(&p) { 3 => i18n::t("used up", "已用完"), 2 => i18n::t("near its limit", "接近上限"), 1 => i18n::t("past half", "已过半"), _ => i18n::t("fine", "正常") };
                        i18n::pick(format!("\nUsed: {:.0}% ({word})", f * 100.0), format!("\n用量：{:.0}%（{word}）", f * 100.0))
                    }
                    None => String::new(),
                };
                let est = p.estimate.map(|e| {
                    let cal = estimate::calibrate(self.store.projects.iter().flat_map(|p| &p.sessions)).map(|c| e * c.min_per_round);
                    let cal = cal.map(|m| i18n::pick(format!(", calibrated {}", estimate::minutes_text(m)), format!("，校准后 {}", estimate::minutes_text(m)))).unwrap_or_default();
                    i18n::pick(format!("\nEstimate: {e:.0} round(s) ≈ {}{cal}", estimate::minutes_text(e * estimate::MIN_PER_ROUND)),
                        format!("\n估算：{e:.0} 轮 ≈ {}{cal}", estimate::minutes_text(e * estimate::MIN_PER_ROUND)))
                }).unwrap_or_default();
                let actual = estimate::actual_minutes(&p, now).map(|m| i18n::pick(format!("\nActual: {} over {} turn(s)", estimate::minutes_text(m), p.log().len()),
                    format!("\n实际：{}，共 {} 轮对话", estimate::minutes_text(m), p.log().len()))).unwrap_or_default();
                (format!("{steps}\n{dollars}{share}{est}{actual}"),
                    i18n::t("Steps are model calls over all its turns. Past its budget, OctoBuddy interrupts it and holds it.", "步数是它所有轮次的模型调用次数。超出预算时 OctoBuddy 会打断并挂起它。").into())
            }
            Some(FlowOpen::Outer(id)) => {
                let Some(s) = self.store.projects.iter().flat_map(|p| &p.sessions).find(|s| &s.id == id) else { return Default::default() };
                let left = self.rounds_left.get(id).copied().unwrap_or(0);
                let times = self.rt.extended.get(id).copied().unwrap_or(0);
                let review = i18n::pick(format!("Review budget: {left} review(s) left (extended by itself {times}/{} times this request)", crate::MAX_EXTENSIONS),
                    format!("审查预算：还剩 {left} 次（本次请求已自动续 {times}/{} 次）", crate::MAX_EXTENSIONS));
                let inner: f64 = s.peers().iter().map(|p| p.usage.as_ref().map(|u| u.cost).unwrap_or(0.0)).sum();
                let cost = i18n::pick(format!("Outer loop: ${:.2} · its inner loops: ${inner:.2}", s.lead_cost.unwrap_or(0.0)),
                    format!("外环费用：${:.2} · inner 合计：${inner:.2}", s.lead_cost.unwrap_or(0.0)));
                let (active, all) = (s.peers().iter().filter(|p| p.is_active()).count(), s.peers().iter().filter(|p| p.reviews_for.is_none()).count());
                let peers = i18n::pick(format!("Inner loops: {active} working of {all}"), format!("inner：{active} 个在工作 / 共 {all} 个"));
                let est = s.estimate.as_ref().map(|e| i18n::pick(format!("\nEstimate: {e}"), format!("\n估算：{e}"))).unwrap_or_default();
                let first = s.messages.iter().find(|m| m.role() == crate::model::Role::User).map(|m| m.at);
                let last = s.messages.iter().map(|m| m.at + m.took.unwrap_or(0)).max();
                let took = first.zip(last).map(|(a, b)| i18n::pick(format!("\nSo far: {}", estimate::minutes_text(b.saturating_sub(a) as f64 / 60.0)),
                    format!("\n至今用时：{}", estimate::minutes_text(b.saturating_sub(a) as f64 / 60.0)))).unwrap_or_default();
                (format!("{review}\n{cost}\n{peers}{est}{took}"),
                    i18n::t("Each report the outer loop reviews takes one; while checks keep passing it is extended by itself, up to 3 times. Your next message refills it.",
                        "外环每审一次汇报用掉一次；检查持续通过时会自动续（最多 3 次）。你发下一条消息时重新补满。").into())
            }
            _ => Default::default(),
        }
    }

    /// Shows the open card's tab (`creating`: the new-peer form instead).
    pub(crate) fn sync_card_tabs(&mut self, cx: &mut Cx, creating: bool) {
        let tab = self.card_tab;
        self.view.view(cx, ids!(flow_tabs)).set_visible(cx, !creating);
        for (on, off, this) in [(ids!(ft_msgs_on), ids!(ft_msgs), CardTab::Messages), (ids!(ft_budget_on), ids!(ft_budget), CardTab::Budget), (ids!(ft_specs_on), ids!(ft_specs), CardTab::Specs)] {
            self.view.button(cx, on).set_visible(cx, tab == this);
            self.view.button(cx, off).set_visible(cx, tab != this);
        }
        let t = |en: &'static str, zh: &'static str| i18n::t(en, zh);
        for (id, text) in [(ids!(ft_msgs_on), t("Messages", "消息")), (ids!(ft_msgs), t("Messages", "消息")), (ids!(ft_budget_on), t("Budget", "预算")),
            (ids!(ft_budget), t("Budget", "预算")), (ids!(ft_specs_on), t("Specs", "Spec")), (ids!(ft_specs), t("Specs", "Spec"))] {
            self.view.button(cx, id).set_text(cx, text);
        }
        // The conversation only under Messages: Budget and Specs fill the space.
        let messages = !creating && tab == CardTab::Messages;
        self.view.view(cx, ids!(flow_chat_pane)).set_visible(cx, messages);
        self.view.view(cx, ids!(flow_compose)).set_visible(cx, messages);
        self.view.view(cx, ids!(flow_budget_pane)).set_visible(cx, !creating && tab == CardTab::Budget);
        self.view.view(cx, ids!(flow_specs_pane)).set_visible(cx, !creating && tab == CardTab::Specs);
        let (head, rows) = self.card_spec_rows();
        let in_force = rows.iter().filter(|r| r.in_force).count();
        self.view.label(cx, ids!(ft_note)).set_text(cx, &if rows.is_empty() { String::new() } else {
            i18n::pick(format!("{} spec(s)", rows.len()), format!("{} 份 spec", rows.len()))
        });
        let _ = in_force;
        if tab == CardTab::Budget {
            let (text, note) = self.card_budget_text();
            self.view.label(cx, ids!(flow_budget_text)).set_text(cx, &text);
            self.view.label(cx, ids!(flow_budget_note)).set_text(cx, &note);
        }
        if tab == CardTab::Specs {
            self.view.label(cx, ids!(flow_specs_summary)).set_text(cx, &head);
            self.spec_rows = rows;
            self.view.portal_list(cx, ids!(flow_spec_list)).redraw(cx);
        }
    }

    /// The spec opened from the list: its file in a dialog over everything.
    pub(crate) fn sync_spec_dialog(&mut self, cx: &mut Cx) {
        let row = self.spec_shown.as_ref().and_then(|p| self.spec_rows.iter().find(|r| &r.path == p)).cloned();
        self.view.view(cx, ids!(spec_layer)).set_visible(cx, row.is_some());
        self.view.button(cx, ids!(spec_close)).set_text(cx, i18n::t("Close", "关闭"));
        let Some(row) = row else { return };
        let text = std::fs::read_to_string(&row.path).unwrap_or_else(|e| i18n::pick(format!("(could not read {}: {e})", row.path), format!("（无法读取 {}：{e}）", row.path)));
        let force = if row.in_force { i18n::t(" · in force", " · 当前生效") } else { "" };
        self.view.label(cx, ids!(spec_title)).set_text(cx, &format!("{} · {}{force}", row.name, row.state));
        self.view.label(cx, ids!(spec_where)).set_text(cx, &format!("{} · {}", row.meta, row.path));
        self.view.label(cx, ids!(spec_body)).set_text(cx, &text);
    }

    pub(crate) fn draw_spec_list(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        if self.spec_rows.is_empty() {
            list.set_item_range(cx, 0, 0);
            while list.next_visible_item(cx).is_some() {}
            return;
        }
        list.set_item_range(cx, 0, self.spec_rows.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(row) = self.spec_rows.get(index).cloned() else { continue };
            let item = list.item(cx, index, id!(SpecRow));
            let mark = if row.applied { "✓ " } else if row.working { "▶ " } else { "" };
            item.label(cx, ids!(spec_name)).set_text(cx, &format!("{mark}{}", row.name));
            let state = if row.in_force && !row.applied && !row.working { format!("{} · {}", row.state, i18n::t("in force", "当前生效")) } else { row.state.clone() };
            item.label(cx, ids!(spec_state)).set_text(cx, &state);
            item.label(cx, ids!(spec_meta)).set_text(cx, &row.meta);
            // In progress: blue; applied: green; earlier tasks: faint.
            let (border, size, bg) = if row.working { (0x2f6feb, 1.5, 0xf3f8ff) } else if row.applied { (0x1a7f37, 1.0, 0xffffff) } else if !row.in_force { (0xe5e7eb, 1.0, 0xfafbfc) } else { (0xd8dee4, 1.0, 0xffffff) };
            let (border, bg) = (crate::hex_color(border), crate::hex_color(bg));
            let mut w = item.view(cx, ids!(row));
            script_apply_eval!(cx, w, { draw_bg +: {color: #(bg) border_color: #(border) border_size: #(size)} });
            item.draw_all_unscoped(cx);
        }
    }

    /// The tabs, a spec opened, back to the list.
    pub(crate) fn card_tabs_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let mut changed = false;
        for (id, tab) in [(ids!(ft_msgs), CardTab::Messages), (ids!(ft_budget), CardTab::Budget), (ids!(ft_specs), CardTab::Specs)] {
            if self.view.button(cx, id).clicked(actions) {
                self.card_tab = tab;
                self.spec_shown = None;
                changed = true;
            }
        }
        if self.view.button(cx, ids!(spec_close)).clicked(actions) || (self.spec_shown.is_some() && crate::tapped(&self.view.view(cx, ids!(spec_backdrop)), actions)) {
            self.spec_shown = None;
            changed = true;
        }
        if self.card_tab == CardTab::Specs && self.spec_shown.is_none() {
            let list = self.view.portal_list(cx, ids!(flow_spec_list));
            let hit = list.items_with_actions(actions).into_iter()
                .find(|(_, item)| crate::tapped(&item.view(cx, ids!(row)), actions)).map(|(index, _)| index);
            if let Some(row) = hit.and_then(|i| self.spec_rows.get(i)) {
                self.spec_shown = Some(row.path.clone());
                changed = true;
            }
        }
        if changed {
            self.relayout(cx);
        }
    }
}
