//! The timeline view: the selected session's run for the canvas, and the
//! panels under it (what each loop does at the playhead, what happened last,
//! the turn picked), kept up as the playhead moves.
use crate::model::now_secs;
use crate::timeline::{self, clock, span_len, Kind, TimeAction, TimelineCanvasWidgetRefExt};
use crate::{i18n, inner_short, outer_short, OctoBuddyView, Stage, TIME_SPEEDS};
use makepad_widgets::*;

impl OctoBuddyView {
    /// Builds the shown session's timeline and hands it to the canvas.
    pub(crate) fn sync_time(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        let project = self.store.projects[at.0].path.clone();
        let session = self.store.session(at).unwrap().clone();
        let (_, label) = self.rt.outer_label(&session, &project, "");
        let outer = outer_short(&label);
        let busy = self.lead_status.contains_key(&session.id);
        let tl = timeline::build(&session, now_secs(), busy, &|p| p.map(inner_short).unwrap_or_else(|| outer.clone()));

        let cost = session.lead_cost.unwrap_or(0.0) + session.peers().iter().filter_map(|p| p.usage.as_ref()).map(|u| u.cost).sum::<f64>();
        let inner_turns = tl.turns(Kind::Inner) + tl.turns(Kind::Review);
        let agents = tl.lanes.iter().filter(|l| l.kind != Kind::Outer).count();
        let summary = if tl.spans.is_empty() {
            i18n::t("Nothing has run in this session yet.", "这个会话还没有运行过。").to_string()
        } else {
            i18n::pick(
                format!("{} in all · {} working · outer {} turn(s) · inner {inner_turns} turn(s) ({agents} agent(s)) · ${cost:.2}",
                    span_len(tl.end), span_len(tl.busy()), tl.turns(Kind::Outer)),
                format!("总时长 {} · 实际在跑 {} · outer {} 轮 · inner {inner_turns} 轮（{agents} 个 agent）· ${cost:.2}",
                    span_len(tl.end), span_len(tl.busy()), tl.turns(Kind::Outer)))
        };
        // Where the waiting went, by why (the outer loop's lane, and the inner ones').
        // Each why's time once: waits on several lanes at once (a wave holding
        // three slices) are one stretch of the session, not three.
        let mut by: Vec<(&'static str, f64)> = Vec::new();
        let mut kinds: Vec<&'static str> = tl.waits.iter().map(|w| w.kind).collect();
        kinds.sort();
        kinds.dedup();
        for kind in kinds {
            let mut iv: Vec<(f64, f64)> = tl.waits.iter().filter(|w| w.kind == kind).map(|w| (w.from, w.to)).collect();
            iv.sort_by(|a, b| a.0.total_cmp(&b.0));
            let (mut total, mut until) = (0.0, f64::MIN);
            for (a, b) in iv {
                total += (b - a.max(until)).max(0.0);
                until = until.max(b);
            }
            by.push((kind, total));
        }
        by.sort_by(|a, b| b.1.total_cmp(&a.1));
        let word = |k: &str| match k {
            "start" => i18n::t("starting", "启动"),
            "wave" => i18n::t("waves", "等 wave"),
            "queue" => i18n::t("queued", "排队"),
            "check" => i18n::t("checks", "检查"),
            "person" => i18n::t("you", "等你"),
            _ => i18n::t("restarts", "重启"),
        };
        let waited = by.iter().filter(|(_, t)| *t >= 1.0).map(|(k, t)| format!("{} {}", word(k), span_len(*t))).collect::<Vec<_>>().join(" · ");
        let summary = if waited.is_empty() { summary } else { i18n::pick(format!("{summary} · waits: {waited}"), format!("{summary} · 等待：{waited}")) };
        let lost: f64 = tl.rework.iter().map(|r| r.lost).sum();
        let summary = if tl.rework.is_empty() { summary } else {
            i18n::pick(format!("{summary} · rework: {} ({} lost)", tl.rework.len(), span_len(lost)), format!("{summary} · 返工 {} 处（浪费 {}）", tl.rework.len(), span_len(lost)))
        };
        self.view.label(cx, ids!(time_summary)).set_text(cx, &summary);
        self.view.label(cx, ids!(time_legend)).set_text(cx, i18n::t(
            "Blue: outer · green: inner · purple: reviewer · red: failed · line down: outer sends work (from the turn that sent it to the turn that took it up) · line up: a report (from the turn that wrote it to the turn that read it; a curve says how long it waited) · yellow dot: you · green diamond: commit · orange ring: rework · dotted line: a wait (why beside it; orange: waits for you) · outlined faint bar: done again later · hatched lane: history · grey band: idle, squeezed · wheel: zoom · double-click a turn: open it",
            "蓝：outer · 绿：inner · 紫：审查者 · 红：失败 · 向下的线：outer 派任务（从发出它的那一轮到 inner 开始做的那一轮）· 向上的线：汇报（从写它的那一轮到读它的那一轮，曲线上标着等了多久） · 黄点：你的消息 · 绿菱形：提交 · 橙色圆环：返工 · 点线：等待（旁边写着原因，橙色是等你）· 描边的淡色段：后来重做了 · 斜纹 lane：历史 · 灰带：压缩的空闲 · 滚轮缩放 · 双击一段在对话中打开"));
        self.view.label(cx, ids!(time_rework_title)).set_text(cx, &i18n::pick(format!("Rework and repeats ({})", tl.rework.len()), format!("返工与重复（{}）", tl.rework.len())));
        let lines = tl.rework_lines();
        let evolve = self.view.button(cx, ids!(time_evolve));
        evolve.set_visible(cx, !lines.is_empty());
        evolve.set_text(cx, i18n::t("Learn from it", "沉淀经验"));
        self.view.label(cx, ids!(time_rework)).set_text(cx, &if lines.is_empty() {
            i18n::t("None: nothing was done twice.", "没有：没有重复做的工作。").to_string()
        } else {
            lines.join("\n")
        });
        let canvas = self.view.widget(cx, ids!(timeline)).as_timeline_canvas();
        canvas.set_speed(TIME_SPEEDS[self.time_speed % TIME_SPEEDS.len()]);
        canvas.set_timeline(cx, tl.clone());
        self.time_tl = tl;
        self.time_controls(cx);
        self.time_panels(cx);
    }

    fn time_controls(&mut self, cx: &mut Cx) {
        let canvas = self.view.widget(cx, ids!(timeline)).as_timeline_canvas();
        let play = if canvas.playing() { i18n::t("Pause", "暂停") } else { i18n::t("Play", "回放") };
        self.view.button(cx, ids!(time_play)).set_text(cx, play);
        self.view.button(cx, ids!(time_rewind)).set_text(cx, i18n::t("To the start", "回到开头"));
        self.view.button(cx, ids!(time_end)).set_text(cx, i18n::t("To now", "到现在"));
        let speed = TIME_SPEEDS[self.time_speed % TIME_SPEEDS.len()];
        self.view.button(cx, ids!(time_speed)).set_text(cx, &i18n::pick(format!("Speed {speed:.0}×"), format!("速度 {speed:.0}×")));
        let squeeze = if self.time_unsqueezed { i18n::t("Squeeze idle: off", "压缩空闲：关") } else { i18n::t("Squeeze idle: on", "压缩空闲：开") };
        self.view.button(cx, ids!(time_squeeze)).set_text(cx, squeeze);
    }

    /// The panels under the canvas, for where the playhead is.
    fn time_panels(&mut self, cx: &mut Cx) {
        let canvas = self.view.widget(cx, ids!(timeline)).as_timeline_canvas();
        let t = canvas.now();
        let tl = &self.time_tl;
        self.view.label(cx, ids!(time_clock)).set_text(cx, &format!("{} / {}", clock(t), clock(tl.end)));
        self.view.label(cx, ids!(time_now_title)).set_text(cx, &i18n::pick(format!("At {}", clock(t)), format!("此刻 {}", clock(t))));
        let mut lines = tl.state_at(t);
        if lines.len() > 7 {
            let more = lines.len() - 6;
            lines.truncate(6);
            lines.push(i18n::pick(format!("and {more} more"), format!("还有 {more} 个")));
        }
        self.view.label(cx, ids!(time_now)).set_text(cx, &lines.join("\n"));
        self.view.label(cx, ids!(time_events_title)).set_text(cx, i18n::t("What happened last", "最近发生"));
        let events: Vec<String> = tl.events_until(t, 7).into_iter()
            .map(|(at, line)| format!("{} {}", clock(at), line.chars().take(70).collect::<String>())).collect();
        let events = if events.is_empty() { i18n::t("Nothing yet.", "还没有。").to_string() } else { events.join("\n") };
        self.view.label(cx, ids!(time_events)).set_text(cx, &events);
        let picked = canvas.selected().and_then(|i| tl.spans.get(i));
        let (title, text) = match picked {
            Some(s) => {
                let lane = &tl.lanes[s.lane];
                let from = timeline::who(&s.from);
                let title = if from.is_empty() {
                    i18n::pick(format!("{} · turn {}", lane.name, s.turn), format!("{} · 第 {} 轮", lane.name, s.turn))
                } else {
                    i18n::pick(format!("{} · turn {} · for {from}", lane.name, s.turn), format!("{} · 第 {} 轮 · 来自 {from}", lane.name, s.turn))
                };
                let end = if s.open { i18n::t("now", "现在").to_string() } else { clock(s.end) };
                let mut facts = format!("{} → {end} · {}", clock(s.start), span_len(s.end - s.start));
                if s.steps > 0 {
                    facts.push_str(&i18n::pick(format!(" · {} tool call(s)", s.steps), format!(" · {} 次工具调用", s.steps)));
                    if s.failed > 0 {
                        facts.push_str(&i18n::pick(format!(" ({} failed)", s.failed), format!("（{} 个失败）", s.failed)));
                    }
                }
                if let Some(c) = s.cost {
                    facts.push_str(&format!(" · ≈${c:.3}"));
                }
                match s.outcome.as_str() {
                    "failed" => facts.push_str(i18n::t(" · failed", " · 失败")),
                    "interrupted" => facts.push_str(i18n::t(" · interrupted", " · 被中断")),
                    _ => {}
                }
                let cut = |text: &str, n: usize| -> String {
                    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    let short: String = flat.chars().take(n).collect();
                    if flat.chars().count() > n { format!("{short}…") } else { short }
                };
                let mut text = facts;
                if !s.input.trim().is_empty() {
                    text.push_str(&i18n::pick(format!("\nAsked: {}", cut(&s.input, 90)), format!("\n输入：{}", cut(&s.input, 90))));
                }
                if !s.reply.trim().is_empty() {
                    text.push_str(&i18n::pick(format!("\nAnswered: {}", cut(&s.reply, 110)), format!("\n回复：{}", cut(&s.reply, 110))));
                }
                (title, text)
            }
            None => (i18n::t("The turn you pick", "选中的轮次").to_string(),
                i18n::t("Click a turn on the timeline to see it here; double-click it to open it in the conversation.",
                    "点时间轴上的一段，在这里查看它；双击在对话中打开。").to_string()),
        };
        self.view.label(cx, ids!(time_sel_title)).set_text(cx, &title);
        self.view.label(cx, ids!(time_sel)).set_text(cx, &text);
        let open = self.view.button(cx, ids!(time_open));
        open.set_visible(cx, picked.is_some());
        open.set_text(cx, i18n::t("Open in chat", "在对话中查看"));
    }

    pub(crate) fn handle_time_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.stage != Stage::Time {
            return;
        }
        let canvas = self.view.widget(cx, ids!(timeline)).as_timeline_canvas();
        for action in canvas.actions(actions) {
            match action {
                TimeAction::Moved(_) | TimeAction::Selected(_) => {
                    self.time_panels(cx);
                    // Played to the end: the button says Play again.
                    self.time_controls(cx);
                }
                TimeAction::Open(i) => self.open_turn(cx, i),
                TimeAction::None => {}
            }
        }
        if self.view.button(cx, ids!(time_play)).clicked(actions) {
            canvas.toggle_play(cx);
            self.time_controls(cx);
        }
        if self.view.button(cx, ids!(time_rewind)).clicked(actions) {
            canvas.jump(cx, false);
            self.time_controls(cx);
        }
        if self.view.button(cx, ids!(time_end)).clicked(actions) {
            canvas.jump(cx, true);
            self.time_controls(cx);
        }
        if self.view.button(cx, ids!(time_speed)).clicked(actions) {
            self.time_speed = (self.time_speed + 1) % TIME_SPEEDS.len();
            canvas.set_speed(TIME_SPEEDS[self.time_speed]);
            self.time_controls(cx);
        }
        if self.view.button(cx, ids!(time_squeeze)).clicked(actions) {
            self.time_unsqueezed = !self.time_unsqueezed;
            canvas.set_squeeze(cx, !self.time_unsqueezed);
            self.time_controls(cx);
            self.time_panels(cx);
        }
        // The rework, learned from: the outer loop keeps lessons for later runs.
        if self.view.button(cx, ids!(time_evolve)).clicked(actions) {
            if let Some(at) = self.selected {
                let tl = self.time_tl.clone();
                self.evolve(at, &tl);
                self.save();
                self.relayout(cx);
            }
        }
        if self.view.button(cx, ids!(time_open)).clicked(actions) {
            if let Some(i) = canvas.selected() {
                self.open_turn(cx, i);
            }
        }
    }

    /// A turn, in its conversation: the outer loop's in the session's, an
    /// inner loop's in its panel.
    fn open_turn(&mut self, cx: &mut Cx, i: usize) {
        let Some(span) = self.time_tl.spans.get(i).cloned() else { return };
        let key = self.time_tl.lanes[span.lane].key.clone();
        self.stage = Stage::Chat;
        if let Some(peer) = key.strip_prefix("p:") {
            self.selected_peer = Some(peer.to_string());
            self.show_inner = true;
            self.show_preview = false;
        }
        self.relayout(cx);
        if let (Some(message), Some(at)) = (span.message, self.selected) {
            if let Some(row) = self.store.session(at).and_then(|s| crate::chat::row_of_message(s, message)) {
                self.view.portal_list(cx, ids!(messages)).set_first_id_and_scroll(row, 0.0);
                self.view.portal_list(cx, ids!(messages)).redraw(cx);
            }
        }
    }
}
