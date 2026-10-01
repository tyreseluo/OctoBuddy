//! The flow canvas: a project's outer loops and their inner loops as cards
//! joined by lines. It is the conversations' state drawn as a graph, not a
//! second copy of it: the view lays the cards out from the store, and what
//! the person does here becomes the same actions the chat view takes.
//!
//! - Drag a card to place it (the view keeps where).
//! - Drop an inner loop's card on another outer loop's card to move it there.
//! - Double-click the canvas to start a new outer loop at that spot.
//! - Click a card to open its conversation over the canvas.
//! - Drag the canvas (or scroll) to pan.
//!
//! Lines: an outer loop and its inner loops (peer agents) talk both ways,
//! an arrow at each end; a subagent is sent one way, one arrow. A line to a
//! loop at work carries moving dots.
//!
//! Waves: a round's later wave starts when its earlier one is accepted. Each
//! such dependency is a gate to the right of the inner loops — edges from
//! every card of the earlier wave into it, and from it to every card of the
//! later one. A slice still waiting for its wave can be dragged onto another
//! wave (its pill, shown while it is dragged, or a card of that wave): it
//! moves there, and starts at once if that wave's turn has come.
use makepad_widgets::*;
use std::collections::HashMap;

/// One card, as the view lays it out.
#[derive(Clone, Debug)]
pub struct Card {
    pub id: LiveId,
    /// What the card stands for (a session id, a peer id).
    pub key: String,
    pub template: LiveId,
    /// Its top-left corner, in canvas points.
    pub pos: DVec2,
    pub size: DVec2,
    /// The card it hangs from: an inner loop's outer loop, a subagent's agent.
    pub parent: Option<LiveId>,
    /// How it is joined to its parent.
    pub link: Link,
    /// At work: its line moves.
    pub active: bool,
    /// An inner loop's card: it may be dropped on another outer loop's.
    pub movable_to_outer: bool,
    /// Drawn in the accent colour (the one open, or running).
    pub lit: bool,
    /// An inner loop's round and wave (the waves of one round run in order).
    pub wave: Option<(u32, u32)>,
    /// It waits for its wave: it may be moved to another one.
    pub rewave: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Link {
    /// A peer agent: both ways.
    #[default]
    Peer,
    /// A subagent: sent one way, gone when done.
    Sub,
}

#[derive(Clone, Debug, Default)]
pub enum FlowAction {
    #[default]
    None,
    /// A card was clicked.
    Open(String),
    /// A card was dragged to a new place (canvas points).
    Placed(String, DVec2),
    /// An inner loop's card was dropped on another outer loop's card.
    Attach { card: String, onto: String },
    /// A double click on empty canvas (canvas points).
    NewAt(DVec2),
    /// A right click: on a card (its key) or on the canvas; where, on the
    /// screen and in canvas points.
    Menu { card: Option<String>, at: DVec2, canvas: DVec2 },
    /// A press on the canvas (a menu open over it closes).
    Pressed,
    /// A slice waiting for its wave was dropped on another wave.
    Rewave { card: String, wave: u32 },
}

/// Where a dragged slice can be dropped to join a wave.
#[derive(Clone, Debug)]
struct WaveSlot {
    rect: Rect,
    wave: u32,
    new: bool,
}

enum Drag {
    Card { index: usize, grab: DVec2, start: DVec2, moved: bool },
    Pan { last: DVec2 },
}

#[derive(Script, Widget)]
pub struct FlowCanvas {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[redraw]
    #[live]
    draw_bg: DrawColor,
    #[live]
    draw_vector: DrawVector,
    #[live]
    draw_text: DrawText,
    #[live]
    line_color: Vec4f,
    #[live]
    wave_color: Vec4f,
    #[live]
    lit_color: Vec4f,
    #[rust]
    templates: HashMap<LiveId, ScriptObjectRef>,
    #[rust]
    widgets: HashMap<LiveId, (LiveId, WidgetRef)>,
    #[rust]
    cards: Vec<Card>,
    #[rust]
    pan: DVec2,
    #[rust]
    origin: DVec2,
    /// The canvas's size when last drawn.
    #[rust]
    seen: DVec2,
    #[rust]
    drag: Option<Drag>,
    /// The outer loop's card an inner loop's card is held over.
    #[rust]
    target: Option<usize>,
    /// The wave a waiting slice is held over, and the drop targets shown.
    #[rust]
    wave_target: Option<u32>,
    #[rust]
    slots: Vec<WaveSlot>,
    /// Where the canvas takes no presses (panels drawn over it).
    #[rust]
    covered: Vec<Rect>,
    #[rust]
    area: Area,
    /// Where the moving dots are, 0..1 along each line.
    #[rust]
    phase: f64,
    #[rust]
    next_frame: NextFrame,
}

impl ScriptHook for FlowCanvas {
    fn on_before_apply(&mut self, _vm: &mut ScriptVm, apply: &Apply, _scope: &mut Scope, _value: ScriptValue) {
        if apply.is_reload() {
            self.templates.clear();
        }
    }

    fn on_after_apply(&mut self, vm: &mut ScriptVm, apply: &Apply, scope: &mut Scope, value: ScriptValue) {
        // The card templates, as FlatList keeps its items'.
        if !apply.is_eval() {
            if let Some(obj) = value.as_object() {
                vm.vec_with(obj, |vm, vec| {
                    for kv in vec {
                        if let (Some(id), Some(template)) = (kv.key.as_id(), kv.value.as_object()) {
                            self.templates.insert(id, vm.bx.heap.new_object_ref(template));
                        }
                    }
                });
            }
        }
        if apply.is_reload() {
            for (template, widget) in self.widgets.values_mut() {
                if let Some(t) = self.templates.get(template) {
                    widget.script_apply(vm, apply, scope, t.as_object().into());
                }
            }
        }
    }
}

impl FlowCanvas {
    /// The cards to draw, and the widget of each (made from its template on
    /// first use, for the view to fill).
    pub fn set_cards(&mut self, cx: &mut Cx, cards: Vec<Card>) -> Vec<WidgetRef> {
        let mut out = Vec::new();
        for card in &cards {
            let fresh = self.widgets.get(&card.id).is_none_or(|(t, _)| *t != card.template);
            if fresh {
                if let Some(t) = self.templates.get(&card.template) {
                    let value: ScriptValue = t.as_object().into();
                    let widget = cx.with_vm(|vm| WidgetRef::script_from_value(vm, value));
                    cx.widget_tree_insert_child(self.widget_uid(), card.id, widget.clone());
                    self.widgets.insert(card.id, (card.template, widget));
                }
            }
            out.push(self.widgets.get(&card.id).map(|(_, w)| w.clone()).unwrap_or_default());
        }
        self.widgets.retain(|id, _| cards.iter().any(|c| c.id == *id));
        // A card being dragged keeps where the person holds it.
        if let Some(Drag::Card { index, .. }) = &self.drag {
            if let (Some(old), Some(new)) = (self.cards.get(*index), cards.get(*index)) {
                if old.id != new.id {
                    self.drag = None;
                }
            }
        }
        let held = match &self.drag {
            Some(Drag::Card { index, moved: true, .. }) => self.cards.get(*index).map(|c| (c.id, c.pos)),
            _ => None,
        };
        self.cards = cards;
        if let Some((id, pos)) = held {
            if let Some(c) = self.cards.iter_mut().find(|c| c.id == id) {
                c.pos = pos;
            }
        }
        out
    }

    /// Pans so the card `id` sits at the top left (the one the person
    /// works with, when the view opens on it).
    /// Brings a card into view, unless it is already in view (below the
    /// buttons floating over the canvas's top): the canvas stays where the
    /// person put it.
    pub fn focus(&mut self, cx: &mut Cx, id: LiveId) {
        if let Some(card) = self.cards.iter().find(|c| c.id == id) {
            let at = self.pan + card.pos;
            let end = at + card.size;
            if self.seen.x > 0.0 && at.x >= 0.0 && at.y >= 76.0 && end.x <= self.seen.x && end.y <= self.seen.y {
                return;
            }
            self.pan = dvec2(30.0, 84.0) - card.pos;
            self.redraw(cx);
        }
    }

    /// Where the canvas takes no presses: the panels over it.
    pub fn set_covered(&mut self, rects: Vec<Rect>) {
        self.covered = rects;
    }

    fn is_covered(&self, p: DVec2) -> bool {
        self.covered.iter().any(|r| r.contains(p))
    }

    fn screen(&self, pos: DVec2) -> DVec2 {
        self.origin + self.pan + pos
    }

    fn canvas(&self, abs: DVec2) -> DVec2 {
        abs - self.origin - self.pan
    }

    /// The top card under a point (the last drawn is on top).
    fn card_at(&self, abs: DVec2, except: Option<usize>) -> Option<usize> {
        self.cards.iter().enumerate().rev().find(|(i, c)| {
            Some(*i) != except && Rect { pos: self.screen(c.pos), size: c.size }.contains(abs)
        }).map(|(i, _)| i)
    }

    /// A line from `from` (a parent's right edge) to `to` (a child's left
    /// edge): arrowheads at the child, and at the parent too for a peer;
    /// dots running along it while the child works.
    /// `dir` is the way it leaves the parent: right, or down (a card below).
    #[allow(clippy::too_many_arguments)]
    fn draw_line(&mut self, from: DVec2, to: DVec2, dir: DVec2, color: Vec4f, width: f32, link: Link, active: bool) {
        let along = (to - from).x * dir.x + (to - from).y * dir.y;
        let bend = (along.abs() * 0.5).max(30.0);
        let (c1, c2) = (from + dir * bend, to - dir * bend);
        // The stroke stops short of the heads, so their tips stay sharp.
        let head = 7.0;
        let start = if link == Link::Peer { from + dir * head } else { from };
        let end = to - dir * head;
        self.draw_vector.set_color(color.x, color.y, color.z, color.w);
        self.draw_vector.move_to(start.x as f32, start.y as f32);
        self.draw_vector.bezier_to(c1.x as f32, c1.y as f32, c2.x as f32, c2.y as f32, end.x as f32, end.y as f32);
        self.draw_vector.stroke(width);
        self.arrow(to, dir, head, color);
        if link == Link::Peer {
            self.arrow(from, -dir, head, color);
        }
        if active {
            for k in 0..3 {
                let t = (self.phase + k as f64 / 3.0).fract();
                let u = 1.0 - t;
                let p = from * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + to * (t * t * t);
                self.draw_vector.set_color(color.x, color.y, color.z, color.w);
                self.draw_vector.circle(p.x as f32, p.y as f32, 3.0);
                self.draw_vector.fill();
            }
        }
    }

    /// An outer loop's inner loops by round and wave, in order: (round,
    /// wave, card indices). Cards without a wave are left out.
    fn wave_groups(&self, outer: LiveId) -> Vec<(u32, u32, Vec<usize>)> {
        let mut groups: Vec<(u32, u32, Vec<usize>)> = Vec::new();
        for (i, c) in self.cards.iter().enumerate() {
            let (Some((round, wave)), Some(parent)) = (c.wave, c.parent) else { continue };
            if parent != outer || c.link != Link::Peer {
                continue;
            }
            match groups.iter_mut().find(|g| g.0 == round && g.1 == wave) {
                Some(g) => g.2.push(i),
                None => groups.push((round, wave, vec![i])),
            }
        }
        groups.sort_by_key(|g| (g.0, g.1));
        groups
    }

    /// The right edge of an outer loop's inner loops (screen points).
    fn inner_right(&self, outer: LiveId) -> f64 {
        self.cards.iter().filter(|c| c.parent == Some(outer) && c.link == Link::Peer)
            .map(|c| self.screen(c.pos).x + c.size.x).fold(f64::MIN, f64::max)
    }

    /// Where the slice `index` may be dropped: a pill per wave of its round,
    /// and one for a new wave after the last.
    fn wave_slots(&self, index: usize) -> Vec<WaveSlot> {
        let card = &self.cards[index];
        let (Some((round, _)), Some(outer), true) = (card.wave, card.parent, card.rewave) else { return Vec::new() };
        let groups: Vec<(u32, u32, Vec<usize>)> = self.wave_groups(outer).into_iter().filter(|g| g.0 == round).collect();
        let x = self.inner_right(outer) + 120.0;
        let mut out = Vec::new();
        let mut bottom = f64::MIN;
        for (_, wave, cards) in &groups {
            let (top, end) = cards.iter().map(|&i| (self.screen(self.cards[i].pos).y, self.screen(self.cards[i].pos).y + self.cards[i].size.y))
                .fold((f64::MAX, f64::MIN), |(a, b), (t, e)| (a.min(t), b.max(e)));
            bottom = bottom.max(end);
            out.push(WaveSlot { rect: Rect { pos: dvec2(x, (top + end) * 0.5 - 14.0), size: dvec2(74.0, 28.0) }, wave: *wave, new: false });
        }
        let last = groups.iter().map(|g| g.1).max().unwrap_or(1);
        out.push(WaveSlot { rect: Rect { pos: dvec2(x, bottom + 14.0), size: dvec2(74.0, 28.0) }, wave: last + 1, new: true });
        out
    }

    /// The wave the dragged slice `index` is over: a slot, or a card of
    /// another wave of the same round and outer loop.
    fn wave_at(&self, abs: DVec2, index: usize) -> Option<u32> {
        let card = &self.cards[index];
        if !card.rewave {
            return None;
        }
        if let Some(slot) = self.slots.iter().find(|s| s.rect.contains(abs)) {
            return Some(slot.wave);
        }
        let (round, wave) = card.wave?;
        self.card_at(abs, Some(index)).map(|t| &self.cards[t])
            .filter(|t| t.parent == card.parent && t.link == Link::Peer)
            .and_then(|t| t.wave).filter(|(r, w)| *r == round && *w != wave).map(|(_, w)| w)
    }

    fn label(&mut self, cx: &mut Cx2d, pos: DVec2, text: &str, size: f32, color: Vec4f) {
        self.draw_text.text_style.font_size = size;
        self.draw_text.color = color;
        self.draw_text.draw_abs(cx, pos, text);
    }

    /// A filled arrowhead with its tip at `tip`, pointing along `dir`.
    fn arrow(&mut self, tip: DVec2, dir: DVec2, size: f64, color: Vec4f) {
        let n = dvec2(-dir.y, dir.x) * (size * 0.55);
        let back = tip - dir * size;
        self.draw_vector.set_color(color.x, color.y, color.z, color.w);
        self.draw_vector.move_to(tip.x as f32, tip.y as f32);
        self.draw_vector.line_to((back.x + n.x) as f32, (back.y + n.y) as f32);
        self.draw_vector.line_to((back.x - n.x) as f32, (back.y - n.y) as f32);
        self.draw_vector.close();
        self.draw_vector.fill();
    }
}

impl Widget for FlowCanvas {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        // The dots move while any loop works.
        if let Some(ev) = self.next_frame.is_event(event) {
            self.phase = (ev.time * 0.6).fract();
            if self.cards.iter().any(|c| c.active) {
                self.redraw(cx);
            }
        }
        match event.hits(cx, self.area) {
            Hit::FingerDown(fe) if fe.device.mouse_button().is_some_and(|b| b.contains(MouseButton::SECONDARY)) => {
                if self.is_covered(fe.abs) {
                    return;
                }
                self.drag = None;
                let card = self.card_at(fe.abs, None).map(|i| self.cards[i].key.clone());
                cx.widget_action(uid, FlowAction::Menu { card, at: fe.abs, canvas: self.canvas(fe.abs) });
            }
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                if self.is_covered(fe.abs) {
                    return;
                }
                cx.widget_action(uid, FlowAction::Pressed);
                match self.card_at(fe.abs, None) {
                    Some(index) => {
                        let grab = fe.abs - self.screen(self.cards[index].pos);
                        self.drag = Some(Drag::Card { index, grab, start: fe.abs, moved: false });
                        // Where it could go, if it is dragged (shown then).
                        self.slots = self.wave_slots(index);
                    }
                    None if fe.tap_count >= 2 => {
                        self.drag = None;
                        cx.widget_action(uid, FlowAction::NewAt(self.canvas(fe.abs)));
                    }
                    None => self.drag = Some(Drag::Pan { last: fe.abs }),
                }
            }
            Hit::FingerMove(fe) => match &mut self.drag {
                Some(Drag::Card { index, grab, start, moved }) => {
                    let index = *index;
                    // A subagent's card follows its agent: a click, not a drag.
                    if self.cards[index].link == Link::Sub {
                        return;
                    }
                    if !*moved && (fe.abs - *start).length() > 4.0 {
                        *moved = true;
                    }
                    if *moved {
                        let pos = fe.abs - *grab - self.origin - self.pan;
                        self.cards[index].pos = pos;
                        self.target = self.card_at(fe.abs, Some(index))
                            .filter(|t| self.cards[index].movable_to_outer && self.cards[*t].parent.is_none())
                            .filter(|t| self.cards[index].parent != Some(self.cards[*t].id));
                        self.wave_target = if self.target.is_some() { None } else { self.wave_at(fe.abs, index) };
                        self.redraw(cx);
                    }
                }
                Some(Drag::Pan { last }) => {
                    self.pan += fe.abs - *last;
                    *last = fe.abs;
                    self.redraw(cx);
                }
                None => {}
            },
            Hit::FingerUp(fe) if fe.is_primary_hit() => {
                let target = self.target.take();
                let wave = self.wave_target.take();
                self.slots.clear();
                match self.drag.take() {
                    Some(Drag::Card { index, moved: false, .. }) => {
                        cx.widget_action(uid, FlowAction::Open(self.cards[index].key.clone()));
                    }
                    Some(Drag::Card { index, .. }) => {
                        let card = self.cards[index].key.clone();
                        match (target, wave) {
                            (Some(t), _) => cx.widget_action(uid, FlowAction::Attach { card, onto: self.cards[t].key.clone() }),
                            (None, Some(wave)) => cx.widget_action(uid, FlowAction::Rewave { card, wave }),
                            (None, None) => cx.widget_action(uid, FlowAction::Placed(card, self.cards[index].pos)),
                        }
                        self.redraw(cx);
                    }
                    _ => {}
                }
            }
            // Scrolling the open card's conversation must not move the canvas.
            Hit::FingerScroll(fe) if !self.is_covered(fe.abs) => {
                self.pan -= fe.scroll;
                self.redraw(cx);
            }
            Hit::FingerHoverOver(fe) => {
                let over_card = !self.is_covered(fe.abs) && self.card_at(fe.abs, None).is_some();
                cx.set_cursor(if over_card { MouseCursor::Hand } else { MouseCursor::Default });
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.walk_turtle(walk);
        self.origin = rect.pos;
        self.seen = rect.size;
        self.draw_bg.draw_abs(cx, rect);
        cx.begin_turtle(
            Walk { abs_pos: Some(rect.pos), width: Size::Fixed(rect.size.x), height: Size::Fixed(rect.size.y), ..Default::default() },
            Layout { clip_x: true, clip_y: true, ..Layout::default() },
        );
        // The lines first, under the cards: from an outer loop's right edge
        // to its inner loop's left edge; to the card it is held over, lit.
        self.draw_vector.begin();
        let dragged = match &self.drag {
            Some(Drag::Card { index, moved: true, .. }) => Some(*index),
            _ => None,
        };
        for i in 0..self.cards.len() {
            let card = self.cards[i].clone();
            let parent = match (dragged == Some(i), self.target) {
                (true, Some(t)) => Some((t, true)),
                _ => card.parent.and_then(|p| self.cards.iter().position(|c| c.id == p)).map(|p| (p, false)),
            };
            let Some((p, held)) = parent else { continue };
            let from = self.cards[p].clone();
            // A subagent card below its agent's hangs from its bottom edge.
            let below = card.link == Link::Sub && card.pos.y >= from.pos.y + from.size.y - 1.0 && !held;
            let (a, b, dir) = if below {
                (self.screen(from.pos) + dvec2(from.size.x * 0.3, from.size.y), self.screen(card.pos) + dvec2(card.size.x * 0.5, 0.0), dvec2(0.0, 1.0))
            } else {
                (self.screen(from.pos) + dvec2(from.size.x, from.size.y * 0.5), self.screen(card.pos) + dvec2(0.0, card.size.y * 0.5), dvec2(1.0, 0.0))
            };
            let (color, width) = if held || card.lit || card.active { (self.lit_color, 2.0) } else { (self.line_color, 1.5) };
            self.draw_line(a, b, dir, color, width, card.link, card.active && !held);
        }
        // The waves: a gate between each earlier and later wave of a round.
        let mut labels: Vec<(DVec2, String, f32, Vec4f)> = Vec::new();
        let outers: Vec<LiveId> = self.cards.iter().filter(|c| c.parent.is_none()).map(|c| c.id).collect();
        let wave = self.wave_color;
        for outer in outers {
            let groups = self.wave_groups(outer);
            let right = self.inner_right(outer);
            for pair in groups.windows(2) {
                let ((ra, wa, a), (rb, wb, b)) = (&pair[0], &pair[1]);
                if ra != rb {
                    continue;
                }
                let mid = |c: &Card| self.origin + self.pan + c.pos + dvec2(c.size.x, c.size.y * 0.5);
                let froms: Vec<(usize, DVec2)> = a.iter().map(|&i| (i, mid(&self.cards[i]))).collect();
                let tos: Vec<(usize, DVec2, bool)> = b.iter().map(|&i| (i, mid(&self.cards[i]), self.cards[i].active)).collect();
                let a_low = froms.iter().map(|(_, p)| p.y).fold(f64::MIN, f64::max);
                let b_high = tos.iter().map(|(_, p, _)| p.y).fold(f64::MAX, f64::min);
                let gate = dvec2(right + 44.0, (a_low + b_high) * 0.5);
                for (i, from) in froms {
                    if dragged != Some(i) {
                        self.draw_line(from, gate - dvec2(16.0, 0.0), dvec2(1.0, 0.0), wave, 1.3, Link::Sub, false);
                    }
                }
                for (i, to, active) in tos {
                    if dragged != Some(i) {
                        self.draw_line(gate - dvec2(16.0, 0.0), to + dvec2(2.0, 0.0), dvec2(-1.0, 0.0), wave, 1.3, Link::Sub, active);
                    }
                }
                // The gate: a pill naming the wave that waits.
                self.draw_vector.set_color(1.0, 1.0, 1.0, 1.0);
                self.draw_vector.rounded_rect((gate.x - 18.0) as f32, (gate.y - 11.0) as f32, 64.0, 22.0, 11.0);
                self.draw_vector.fill();
                self.draw_vector.set_color(wave.x, wave.y, wave.z, wave.w);
                self.draw_vector.rounded_rect((gate.x - 18.0) as f32, (gate.y - 11.0) as f32, 64.0, 22.0, 11.0);
                self.draw_vector.stroke(1.3);
                labels.push((gate + dvec2(-10.0, -6.0), format!("W{wb} ← W{wa}"), 8.0, wave));
            }
        }
        // While a waiting slice is dragged: the waves it can join.
        let slots = if dragged.is_some() { self.slots.clone() } else { Vec::new() };
        for slot in slots {
            let on = self.wave_target == Some(slot.wave);
            let (r, c) = (slot.rect, wave);
            self.draw_vector.set_color(c.x, c.y, c.z, if on { 1.0 } else { 0.12 });
            self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, r.size.x as f32, r.size.y as f32, 14.0);
            self.draw_vector.fill();
            self.draw_vector.set_color(c.x, c.y, c.z, 1.0);
            self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, r.size.x as f32, r.size.y as f32, 14.0);
            self.draw_vector.stroke(1.3);
            let ink = if on { vec4(1.0, 1.0, 1.0, 1.0) } else { c };
            let text = if slot.new { format!("+ W{}", slot.wave) } else { format!("W{}", slot.wave) };
            labels.push((r.pos + dvec2(if slot.new { 20.0 } else { 26.0 }, 8.0), text, 9.0, ink));
        }
        self.draw_vector.end(cx);
        for (pos, text, size, color) in labels {
            self.label(cx, pos, &text, size, color);
        }
        if self.cards.iter().any(|c| c.active) {
            self.next_frame = cx.new_next_frame();
        }
        for card in self.cards.clone() {
            let Some((_, widget)) = self.widgets.get(&card.id).cloned() else { continue };
            let walk = Walk::fixed(card.size.x, card.size.y).with_abs_pos(self.screen(card.pos));
            widget.draw_walk_all(cx, scope, walk);
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

impl FlowCanvasRef {
    pub fn focus(&self, cx: &mut Cx, id: LiveId) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.focus(cx, id);
        }
    }

    pub fn set_cards(&self, cx: &mut Cx, cards: Vec<Card>) -> Vec<WidgetRef> {
        self.borrow_mut().map(|mut inner| inner.set_cards(cx, cards)).unwrap_or_default()
    }

    pub fn set_covered(&self, rects: Vec<Rect>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_covered(rects);
        }
    }

    /// What happened on the canvas this frame, in order.
    pub fn actions(&self, actions: &Actions) -> Vec<FlowAction> {
        let uid = self.widget_uid();
        actions.iter().filter_map(|a| a.as_widget_action()).filter(|a| a.widget_uid == uid)
            .map(|a| a.cast::<FlowAction>()).filter(|a| !matches!(a, FlowAction::None)).collect()
    }
}
