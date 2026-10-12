//! The lifecycle view's self-drawing canvas: the slice state machine for
//! the session shown. Nodes are the states, edges the transitions the
//! session crossed. A side panel lists the slices; a click picks one and
//! highlights its path through the graph.
//!
//! The model is what `lifecycle::build_slice_machine` derives from the
//! store. The view does not write back.
use crate::lifecycle::{SliceMachine, SliceState, TransitionKind};
use crate::model::now_secs;
use crate::{i18n, OctoBuddyView, Stage};
use makepad_widgets::*;
use std::path::Path;

/// Open `path` with the OS default handler (text editor for a .md file).
/// macOS: `open <path>`. Linux: `xdg-open`. Windows: `explorer`.
/// Best-effort: a failure here is silent (the person can open it from Finder).
fn open_in_default(path: &Path) {
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let program = if cfg!(target_os = "macos") { "open" } else if cfg!(windows) { "explorer" } else { "xdg-open" };
        let _ = std::process::Command::new(program).arg(&path).spawn();
    });
}

/// Where a node sits on the canvas. The layout is hand-picked: the happy
/// path runs left to right, the SentBack loop curves back to Running, and
/// the side exits (Failed, Stopped, OverBudget, Discarded) drop below.
#[derive(Clone, Copy, Debug)]
struct NodePos {
    state: SliceState,
    x: f64,
    y: f64,
}

const NODE_W: f64 = 96.0;
const NODE_H: f64 = 44.0;

/// All the nodes, in the order the view draws them.
fn nodes() -> Vec<NodePos> {
    let row1: &[SliceState] = &[
        SliceState::Waiting,
        SliceState::Running,
        SliceState::Reported,
        SliceState::Checked,
        SliceState::Accepted,
        SliceState::Landed,
    ];
    let row2: &[SliceState] = &[
        SliceState::Discarded,
        SliceState::Failed,
        SliceState::Stopped,
        SliceState::OverBudget,
    ];
    let mut out = Vec::new();
    let spacing = NODE_W + 38.0;
    for (i, s) in row1.iter().enumerate() {
        out.push(NodePos { state: *s, x: i as f64 * spacing, y: 60.0 });
    }
    for (i, s) in row2.iter().enumerate() {
        out.push(NodePos { state: *s, x: i as f64 * spacing + NODE_W * 0.5, y: 220.0 });
    }
    out
}

/// All the edges the view draws (the session's actual edges are added on
/// top, sharing this scheme so the manual layout lines up).
fn base_edges() -> Vec<(TransitionKind, TransitionKind)> {
    use TransitionKind as K;
    vec![
        (K::Waiting, K::Running),
        (K::Running, K::Reported),
        (K::Reported, K::Checked),
        (K::Checked, K::Accepted),
        (K::Accepted, K::Landed),
        // Closed without an accept (no Accepted transition before it) goes to Discarded.
        (K::Running, K::Discarded),
        // Side exits off Running.
        (K::Running, K::Failed),
        (K::Running, K::Stopped),
        (K::Running, K::OverBudget),
    ]
}

/// What a click on a slice in the side panel means.
#[derive(Clone, Debug, Default)]
pub enum LifeAction {
    #[default]
    None,
    /// A slice was picked (its slug), so the view can locate it in the
    /// doc stream too.
    Picked(String),
}

#[derive(Clone, Debug, Default)]
struct SliceHit {
    /// A slice list row's bounding rect on screen.
    rect: Rect,
    slug: String,
}

#[derive(Script, Widget)]
pub struct LifecycleCanvas {
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
    #[rust]
    slices: SliceMachine,
    /// The slice the person picked, its slug.
    #[rust]
    picked: Option<String>,
    /// The rows' screen rects, kept for hit-testing.
    #[rust]
    hits: Vec<SliceHit>,
    /// Where the running slices' moving dots are along their paths.
    #[rust]
    phase: f64,
    #[rust]
    next_frame: NextFrame,
    #[rust]
    area: Area,
    #[rust]
    rect: Rect,
}

/// Map a transition kind back to the slice state it means, when there is one.
/// Round kinds return `None`: the slice state machine only draws slice states.
fn transition_to_state(k: TransitionKind) -> Option<SliceState> {
    match k {
        TransitionKind::Waiting => Some(SliceState::Waiting),
        TransitionKind::Running => Some(SliceState::Running),
        TransitionKind::Reported => Some(SliceState::Reported),
        TransitionKind::Checked => Some(SliceState::Checked),
        TransitionKind::Accepted => Some(SliceState::Accepted),
        TransitionKind::SentBack => Some(SliceState::SentBack),
        TransitionKind::Landed => Some(SliceState::Landed),
        TransitionKind::Discarded => Some(SliceState::Discarded),
        TransitionKind::Failed => Some(SliceState::Failed),
        TransitionKind::Stopped => Some(SliceState::Stopped),
        TransitionKind::OverBudget => Some(SliceState::OverBudget),
        _ => None,
    }
}

impl ScriptHook for LifecycleCanvas {}

impl LifecycleCanvas {
    #[allow(dead_code)]
    fn node_at(&self, s: SliceState) -> Option<NodePos> {
        nodes().into_iter().find(|n| n.state == s)
    }

    /// The path the picked slice walked, in state order. Used to light up
    /// its nodes and edges.
    fn picked_path(&self) -> Option<Vec<SliceState>> {
        let slug = self.picked.as_deref()?;
        let path = self.slices.paths.iter().find(|p| p.slug == slug)?;
        let kinds: Vec<TransitionKind> = path.transitions.iter().map(|t| t.to).collect();
        let states: Vec<SliceState> = kinds.into_iter().filter_map(|k| match k {
            TransitionKind::Waiting => Some(SliceState::Waiting),
            TransitionKind::Running => Some(SliceState::Running),
            TransitionKind::Reported => Some(SliceState::Reported),
            TransitionKind::Checked => Some(SliceState::Checked),
            TransitionKind::Accepted => Some(SliceState::Accepted),
            TransitionKind::SentBack => Some(SliceState::SentBack),
            TransitionKind::Landed => Some(SliceState::Landed),
            TransitionKind::Discarded => Some(SliceState::Discarded),
            TransitionKind::Failed => Some(SliceState::Failed),
            TransitionKind::Stopped => Some(SliceState::Stopped),
            TransitionKind::OverBudget => Some(SliceState::OverBudget),
            // Round kinds never appear in a slice path, but the compiler
            // needs to see them mapped.
            _ => None,
        }).collect();
        Some(states)
    }

    /// Position a node at `pos`, in canvas points.
    fn node_rect(&self, pos: NodePos) -> Rect {
        Rect { pos: dvec2(self.rect.pos.x + pos.x, self.rect.pos.y + pos.y), size: dvec2(NODE_W, NODE_H) }
    }

    fn color_for(&self, state: SliceState) -> u32 {
        match state {
            SliceState::Waiting => crate::theme::hex("muted"),
            SliceState::Running => crate::theme::hex("accent"),
            SliceState::Reported => crate::theme::hex("accent"),
            SliceState::Checked => crate::theme::hex("accent"),
            SliceState::Accepted => crate::theme::hex("success"),
            SliceState::SentBack => crate::theme::hex("warning"),
            SliceState::Landed => crate::theme::hex("success"),
            SliceState::Discarded => crate::theme::hex("faint"),
            SliceState::Failed => crate::theme::hex("danger"),
            SliceState::Stopped => crate::theme::hex("orange"),
            SliceState::OverBudget => crate::theme::hex("warning"),
        }
    }

    #[allow(dead_code)]
    fn label(&mut self, cx: &mut Cx2d, pos: DVec2, text: &str, size: f32, color: Vec4f) {
        self.draw_text.text_style.font_size = size;
        self.draw_text.color = color;
        self.draw_text.draw_abs(cx, pos, text);
    }

    fn text_w(&mut self, cx: &mut Cx2d, text: &str, size: f32) -> f64 {
        self.draw_text.text_style.font_size = size;
        self.draw_text.layout(cx, 0.0, 0.0, None, false, Align::default(), text).size_in_lpxs.width as f64
    }
}

impl Widget for LifecycleCanvas {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        if let Some(ev) = self.next_frame.is_event(event) {
            // A running slice: a slow dot on its node.
            self.phase = (ev.time * 0.5).fract();
            if self.slices.paths.iter().any(|p| p.current == SliceState::Running) {
                self.redraw(cx);
            }
        }
        match event.hits(cx, self.area) {
            crate::makepad_widgets::Hit::FingerDown(fe) if fe.is_primary_hit() => {
                // A slice list row first.
                let hit = self.hits.iter().find(|h| h.rect.contains(fe.abs)).cloned();
                if let Some(h) = hit {
                    self.picked = Some(h.slug.clone());
                    cx.widget_action(uid, LifeAction::Picked(h.slug));
                    self.redraw(cx);
                }
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.walk_turtle(walk);
        self.rect = rect;
        self.draw_bg.draw_abs(cx, rect);
        cx.begin_turtle(
            Walk { abs_pos: Some(rect.pos), width: Size::Fixed(rect.size.x), height: Size::Fixed(rect.size.y), ..Default::default() },
            Layout { clip_x: true, clip_y: true, ..Layout::default() },
        );
        self.hits.clear();

        let pos_map: Vec<NodePos> = nodes();
        let pos_of = |s: SliceState| -> Option<NodePos> { pos_map.iter().copied().find(|n| n.state == s) };
        // Counts at each node: how many slices are currently there.
        let mut at: std::collections::BTreeMap<SliceState, usize> = std::collections::BTreeMap::new();
        for p in &self.slices.paths {
            *at.entry(p.current).or_default() += 1;
        }
        // Edge counts from the machine's own aggregation.
        let edge_counts: std::collections::BTreeMap<(TransitionKind, TransitionKind), usize> = self.slices.edges.iter().map(|e| ((e.from, e.to), e.count)).collect();
        // The picked slice's states: light up its nodes and edges.
        let picked: Vec<SliceState> = self.picked_path().unwrap_or_default();
        let picked_edges: std::collections::HashSet<(SliceState, SliceState)> = picked.windows(2).map(|w| (w[0], w[1])).collect();

        // Side panel: the slices, their state, transition count.
        let panel_x = rect.pos.x + rect.size.x - 220.0;
        self.draw_vector.begin();
        // Panel background.
        self.draw_vector.set_color_hex(crate::theme::hex("panel"), 1.0);
        self.draw_vector.rect((panel_x - 8.0) as f32, (rect.pos.y + 16.0) as f32, 220.0, (rect.size.y - 32.0) as f32);
        self.draw_vector.fill();
        // Panel title.
        let title = i18n::pick("Slices".to_string(), "切片".to_string());
        let mut texts: Vec<(DVec2, String, f32, Vec4f)> = Vec::new();
        texts.push((dvec2(panel_x + 4.0, rect.pos.y + 24.0), title, 9.0, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }));

        // Edges first (under the nodes).
        let base = base_edges();
        for (from, to) in &base {
            let Some(from_s) = transition_to_state(*from) else { continue };
            let Some(to_s) = transition_to_state(*to) else { continue };
            let Some((a, b)) = pos_of(from_s).zip(pos_of(to_s)) else { continue };
            let lit = picked_edges.contains(&(from_s, to_s));
            let count = edge_counts.get(&(*from, *to)).copied().unwrap_or(0);
            let color = if lit { crate::theme::hex("accent") } else { crate::theme::hex("muted") };
            // An S-curve, looping back from Accepted to Reported for SentBack, etc.
            let ax = self.rect.pos.x + a.x + NODE_W;
            let ay = self.rect.pos.y + a.y + NODE_H * 0.5;
            let bx = self.rect.pos.x + b.x;
            let by = self.rect.pos.y + b.y + NODE_H * 0.5;
            let is_back = b.x < a.x;
            let mid = if is_back { (ax - 80.0, ay) } else { ((ax + bx) * 0.5, (ay + by) * 0.5) };
            let pts: Vec<(f64, f64)> = (0..=14).map(|i| {
                let t = i as f64 / 14.0;
                let u = 1.0 - t;
                let px = u * u * u * ax + 3.0 * u * u * t * mid.0 + 3.0 * u * t * t * mid.0 + t * t * t * bx;
                let py = u * u * u * ay + 3.0 * u * u * t * mid.1 + 3.0 * u * t * t * mid.1 + t * t * t * by;
                (px, py)
            }).collect();
            self.draw_vector.set_color_hex(color, if lit { 1.0 } else { 0.45 });
            for w in pts.windows(2) {
                self.draw_vector.move_to(w[0].0 as f32, w[0].1 as f32);
                self.draw_vector.line_to(w[1].0 as f32, w[1].1 as f32);
                self.draw_vector.stroke(if lit { 1.6 } else { 1.0 });
            }
            // Arrowhead at the destination.
            let dir = (bx - pts[pts.len() - 2].0, by - pts[pts.len() - 2].1);
            let _ = dir;
            self.draw_vector.set_color_hex(color, if lit { 1.0 } else { 0.45 });
            self.draw_vector.move_to(bx as f32, by as f32);
            self.draw_vector.line_to((bx - 6.0) as f32, (by - 4.0) as f32);
            self.draw_vector.line_to((bx - 6.0) as f32, (by + 4.0) as f32);
            self.draw_vector.close();
            self.draw_vector.fill();
            // Count badge, when the edge carried any.
            if count > 0 {
                let label = format!("×{count}");
                let mid_pt = ((ax + bx) * 0.5, (ay + by) * 0.5);
                texts.push((dvec2(mid_pt.0 - 8.0, mid_pt.1 - 14.0), label, 8.0, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: if lit { 1.0 } else { 0.55 } }));
            }
        }
        // The SentBack loop: a curve above the row from Accepted back to Running.
        if let (Some(a), Some(b)) = (pos_of(SliceState::Accepted), pos_of(SliceState::Running)) {
            let color = crate::theme::hex("warning");
            let ax = self.rect.pos.x + a.x + NODE_W * 0.5;
            let ay = self.rect.pos.y + a.y;
            let bx = self.rect.pos.x + b.x + NODE_W * 0.5;
            let by = self.rect.pos.y + b.y;
            let mid = (ax, ay - 60.0);
            let pts: Vec<(f64, f64)> = (0..=18).map(|i| {
                let t = i as f64 / 18.0;
                let u = 1.0 - t;
                let px = u * u * u * ax + 3.0 * u * u * t * mid.0 + 3.0 * u * t * t * mid.0 + t * t * t * bx;
                let py = u * u * u * ay + 3.0 * u * u * t * mid.1 + 3.0 * u * t * t * mid.1 + t * t * t * by;
                (px, py)
            }).collect();
            self.draw_vector.set_color_hex(color, 0.7);
            for w in pts.windows(2) {
                self.draw_vector.move_to(w[0].0 as f32, w[0].1 as f32);
                self.draw_vector.line_to(w[1].0 as f32, w[1].1 as f32);
                self.draw_vector.stroke(1.2);
            }
            // A small label.
            texts.push((dvec2(mid.0 - 30.0, mid.1 - 8.0), i18n::t("Sent back", "退回重做").to_string(), 7.5, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.7 }));
        }

        // Nodes on top.
        let on_picked = |s: SliceState| picked.contains(&s);
        for n in &pos_map {
            let r = self.node_rect(*n);
            let color = self.color_for(n.state);
            let count = at.get(&n.state).copied().unwrap_or(0);
            let lit = on_picked(n.state);
            // Background.
            self.draw_vector.set_color_hex(crate::theme::hex("raised"), 1.0);
            self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, r.size.x as f32, r.size.y as f32, 6.0);
            self.draw_vector.fill();
            // Border, lit when the picked slice visited here, or always on Running.
            let border_alpha = if lit { 1.0 } else { if n.state == SliceState::Running && count > 0 { 1.0 } else { 0.4 } };
            self.draw_vector.set_color_hex(color, border_alpha);
            self.draw_vector.rounded_rect(r.pos.x as f32, r.pos.y as f32, r.size.x as f32, r.size.y as f32, 6.0);
            self.draw_vector.stroke(if lit { 1.6 } else { 1.0 });
            // The state label.
            let label = n.state.label();
            let w = self.text_w(cx, label, 8.5);
            let ink = if lit || n.state == SliceState::Running && count > 0 { crate::theme::hex("ink") } else { crate::theme::hex("muted_strong") };
            texts.push((dvec2(r.pos.x + (NODE_W - w) * 0.5, r.pos.y + 8.0), label.to_string(), 8.5, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }));
            // The count.
            if count > 0 {
                let count_label = format!("{count}");
                let cw = self.text_w(cx, &count_label, 12.0);
                texts.push((dvec2(r.pos.x + (NODE_W - cw) * 0.5, r.pos.y + 22.0), count_label, 12.0, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }));
            }
            // An active dot when something runs there.
            if n.state == SliceState::Running && count > 0 {
                let cx = r.pos.x + r.size.x * 0.5;
                let cy = r.pos.y + r.size.y - 4.0;
                self.draw_vector.set_color_hex(crate::theme::hex("accent"), (0.6 + 0.4 * self.phase) as f32);
                self.draw_vector.circle(cx as f32, cy as f32, (2.0 + self.phase * 3.0) as f32);
                self.draw_vector.fill();
            }
            let _ = ink;
        }

        // The side panel: each slice, click to pick.
        let mut row_y = rect.pos.y + 44.0;
        let mut sorted: Vec<&crate::lifecycle::SlicePath> = self.slices.paths.iter().collect();
        sorted.sort_by(|a, b| a.slug.cmp(&b.slug));
        for p in &sorted {
            let picked = self.picked.as_deref() == Some(p.slug.as_str());
            let row_h = 28.0;
            let row = Rect { pos: dvec2(panel_x, row_y), size: dvec2(204.0, row_h) };
            self.hits.push(SliceHit { rect: row, slug: p.slug.clone() });
            if picked {
                self.draw_vector.set_color_hex(crate::theme::hex("accent_soft"), 1.0);
                self.draw_vector.rect(row.pos.x as f32, row.pos.y as f32, row.size.x as f32, row.size.y as f32);
                self.draw_vector.fill();
            }
            let line = format!("{} · {}", p.slug, p.current.label());
            texts.push((dvec2(row.pos.x + 6.0, row.pos.y + 6.0), line, 8.0, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 1.0 }));
            if p.rework_count > 0 {
                texts.push((dvec2(row.pos.x + 6.0, row.pos.y + 16.0), format!("×{} rework", p.rework_count), 7.0, Vec4f { x: 0.0, y: 0.0, z: 0.0, w: 0.65 }));
            }
            row_y += row_h + 2.0;
            if row_y > rect.pos.y + rect.size.y - 8.0 { break; }
        }
        self.draw_vector.end(cx);

        // Texts in their own call, after the shapes.
        self.draw_text.new_draw_call(cx);
        self.draw_text.draw_depth = self.draw_vector.draw_depth + self.draw_vector.cur_zbias;
        for (pos, text, size, color) in texts {
            self.draw_text.text_style.font_size = size;
            self.draw_text.color = color;
            self.draw_text.draw_abs(cx, pos, &text);
        }

        if self.slices.paths.iter().any(|p| p.current == SliceState::Running) {
            self.next_frame = cx.new_next_frame();
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

impl LifecycleCanvasRef {
    pub fn set_slices(&self, cx: &mut Cx, slices: SliceMachine) {
        if let Some(mut c) = self.borrow_mut() {
            c.slices = slices;
            c.redraw(cx);
        }
    }

    #[allow(dead_code)]
    pub fn picked(&self) -> Option<String> {
        self.borrow().and_then(|c| c.picked.clone())
    }

    pub fn actions(&self, actions: &Actions) -> Vec<LifeAction> {
        let uid = self.widget_uid();
        actions.iter().filter_map(|a| a.as_widget_action()).filter(|a| a.widget_uid == uid)
            .map(|a| a.cast::<LifeAction>()).filter(|a| !matches!(a, LifeAction::None)).collect()
    }
}

impl OctoBuddyView {
    /// Build the shown session slice machine and hand it to the canvas.
    pub(crate) fn sync_life(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        let session = self.store.session(at).unwrap();
        let machine = crate::lifecycle::build_slice_machine(session);
        let rounds = crate::lifecycle::build_round_machine(session);
        let stream = crate::lifecycle::build_doc_stream(&[session], &Default::default());
        self.life_docs = stream.items.clone();
        if self.life_doc_picked.is_some_and(|i| i >= self.life_docs.len()) {
            self.life_doc_picked = None;
        }
        // The state machine canvas gets the slice machine.
        let canvas = self.view.widget(cx, ids!(life_sm)).as_lifecycle_canvas();
        canvas.set_slices(cx, machine.clone());
        // Sub-tabs: which one is shown.
        let show_sm = self.life_tab == crate::LifeTab::StateMachine;
        self.view.view(cx, ids!(life_sm)).set_visible(cx, show_sm);
        self.view.view(cx, ids!(life_open_living)).set_visible(cx, true);
        if show_sm {
            self.set_life_docs_visible(cx, false);
            self.view.button(cx, ids!(life_tab_sm)).set_text(cx, i18n::t("State machine", "状态机"));
            self.view.button(cx, ids!(life_tab_docs)).set_text(cx, i18n::t("Docs", "文档流"));
        } else {
            self.set_life_docs_visible(cx, true);
            self.view.button(cx, ids!(life_tab_sm)).set_text(cx, i18n::t("State machine", "状态机"));
            self.view.button(cx, ids!(life_tab_docs)).set_text(cx, i18n::t("Docs", "文档流"));
            self.render_life_docs(cx);
            // Opening the docs stream: regenerate LIVING.md (sync, but only when content changed).
            let sessions: Vec<&_> = self.store.projects.iter().flat_map(|p| p.sessions.iter()).collect();
            let stream = crate::lifecycle::build_doc_stream(&sessions, &Default::default());
            let content = crate::lifecycle::build_living(&self.store.projects[at.0].name, &machine, &rounds, &stream);
            let project = self.store.projects[at.0].path.clone();
            let outcome = crate::lifecycle::write_living_md(&project, &content);
            match outcome {
                crate::lifecycle::LivingWrite::Wrote(path) => {
                    self.system(at, &i18n::pick(format!("LIVING.md updated: {}", path.display()), format!("已更新 LIVING.md: {}", path.display())));
                }
                crate::lifecycle::LivingWrite::Skipped(why) => {
                    self.system(at, &i18n::pick(format!("Could not write LIVING.md: {why}"), format!("无法写 LIVING.md：{why}")));
                }
                crate::lifecycle::LivingWrite::Unchanged => {}
            }
        }
        // A summary of where each round is, for the legend.
        let mut bits: Vec<String> = Vec::new();
        for r in &rounds.rounds {
            bits.push(i18n::pick(format!("round {} - {}", r.round, r.current.label()), format!("第 {} 轮 - {}", r.round, r.current.label())));
        }
        let summary = if bits.is_empty() { i18n::t("No rounds yet.", "还没有轮次。").to_string() } else { bits.join(" - ") };
        self.view.label(cx, ids!(life_round)).set_text(cx, &summary);
        self.view.label(cx, ids!(life_legend)).set_text(cx, i18n::t(
            "Green: success - red: failed - orange: stopped - yellow: over budget - light: discarded. Back-curve: sent back. Click a slice to highlight its path. Counts: how many of each edge the session crossed.",
            "Green: success - red: failed - orange: stopped - yellow: over budget - light: discarded. Back-curve: sent back. Click a slice to highlight its path. Counts: how many of each edge the session crossed."
        ));
        let _ = now_secs();
    }

    /// Toggle the docs panel by id (its DSL handle).
    fn set_life_docs_visible(&mut self, cx: &mut Cx, on: bool) {
        self.view.view(cx, ids!(life_docs_panel)).set_visible(cx, on);
    }

    /// Render the doc stream as two labels: the list on the left, the picked
    /// item's body on the right.
    fn render_life_docs(&mut self, cx: &mut Cx) {
        let mut lines: Vec<String> = Vec::new();
        for (i, d) in self.life_docs.iter().enumerate() {
            let marker = if Some(i) == self.life_doc_picked { ">" } else { " " };
            let ver = d.version.map(|v| format!(" v{v}")).unwrap_or_default();
            let round = d.round.map(|r| format!("R{r} ")).unwrap_or_default();
            lines.push(format!("{marker} [{}{ver}] {} - {}", round, d.kind.label(), d.title));
        }
        let list_text = if lines.is_empty() { i18n::t("Nothing yet.", "还没有。").to_string() } else { lines.join("\n") };
        self.view.label(cx, ids!(life_docs_list)).set_text(cx, &list_text);
        let body = self.life_doc_picked.and_then(|i| self.life_docs.get(i))
            .map(|d| {
                let head = format!("[{}] {}\nby {}\n", d.kind.label(), d.title, d.author);
                let mut text = head;
                if let Some(path) = &d.path { text.push_str(&format!("path: {path}\n")); }
                if let Some(commit) = &d.commit { text.push_str(&format!("commit: {commit}\n")); }
                if let Some(peer) = &d.peer { text.push_str(&format!("peer: {peer}\n")); }
                if let Some(round) = d.round { text.push_str(&format!("round: {round}\n")); }
                text.push_str("---\n");
                if let Some(body) = &d.body {
                    text.push_str(body);
                } else if d.path.is_some() {
                    text.push_str(i18n::t("(use Open LIVING.md or pick a request/plan/report to read here)", "（使用「打开 LIVING.md」或选一条需求/计划/报告查看）"));
                }
                text
            })
            .unwrap_or_else(|| i18n::t("Pick a doc on the left to read it here.", "左侧选一条文档来读。").to_string());
        self.view.label(cx, ids!(life_docs_body)).set_text(cx, &body);
    }

    /// What the lifecycle view does on clicks (slice picked, docs tab).
    pub(crate) fn handle_life_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.stage != Stage::Life { return; }
        if self.view.button(cx, ids!(life_tab_sm)).clicked(actions) {
            self.life_tab = crate::LifeTab::StateMachine;
            self.relayout(cx);
            return;
        }
        if self.view.button(cx, ids!(life_tab_docs)).clicked(actions) {
            self.life_tab = crate::LifeTab::Docs;
            self.relayout(cx);
            return;
        }
        if self.view.button(cx, ids!(life_open_living)).clicked(actions) {
            if let Some(at) = self.selected {
                let project = self.store.projects[at.0].path.clone();
                let path = crate::lifecycle::living_path(&project);
                open_in_default(&path);
            }
            return;
        }
        let canvas = self.view.widget(cx, ids!(life_sm)).as_lifecycle_canvas();
        for a in canvas.actions(actions) {
            match a {
                LifeAction::Picked(slug) => {
                    self.life_tab = crate::LifeTab::Docs;
                    // Find the first doc item that came from this slice's slug.
                    self.life_doc_picked = self.life_docs.iter().position(|d| {
                        d.peer.as_deref() == Some(&slug)
                            || d.author == slug
                            || d.title.starts_with(&slug)
                    });
                    self.relayout(cx);
                }
                LifeAction::None => {}
            }
        }
    }
}
