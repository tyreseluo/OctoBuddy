//! The lifecycle view's self-drawing canvas: the slice state machine for
//! the session shown. Nodes are the states, edges the transitions the
//! session crossed. A side panel lists the slices; a click picks one and
//! highlights its path through the graph, numbered in the order it walked.
//!
//! The graph is laid out once in a fixed logical coordinate system and
//! shown through a pan-and-zoom `GraphView`: it fits when first shown and
//! whenever the canvas is resized, the person drags or scrolls to pan,
//! Ctrl/Cmd + wheel to zoom (centred on the pointer), and a double click
//! (or the Fit button) fits again. The slice list keeps its own strip on
//! the right; the graph never draws under it.
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

/// `secs` as a day and a time (UTC): `2026-10-12 09:30` (the acceptance
/// sheet's own `when`, kept local so this file owns what it shows).
fn when(secs: u64) -> String {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs % 86_400 / 3600, secs % 3600 / 60)
}

/// A pan-and-zoom window from the graph's logical points to the canvas:
/// `screen = pan + point * scale`, with `screen` measured from the canvas's
/// top left. Pure math, unit-tested.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GraphView {
    pub scale: f64,
    pub pan: DVec2,
}

/// How far the person may zoom with the wheel. Fit is allowed below the
/// floor: a narrow canvas must still show every node.
pub(crate) const ZOOM_MIN: f64 = 0.5;
pub(crate) const ZOOM_MAX: f64 = 2.0;
/// The margin a fit keeps around the graph.
pub(crate) const FIT_PAD: f64 = 20.0;
/// The legend strip at the graph area's bottom.
pub(crate) const LEGEND_H: f64 = 42.0;
/// The gap between the graph area and the slice list.
pub(crate) const LIST_GAP: f64 = 10.0;

impl GraphView {
    /// The view that fits `bounds` (graph points) inside `size` (canvas
    /// points), centred, with `pad` to spare on every side. The scale's
    /// only cap is the zoom ceiling: a narrow canvas zooms out as far as
    /// it takes to show every node.
    pub fn fit(bounds: Rect, size: DVec2, pad: f64) -> Self {
        let w = bounds.size.x.max(1.0);
        let h = bounds.size.y.max(1.0);
        let fit = ((size.x - 2.0 * pad) / w).min((size.y - 2.0 * pad) / h);
        let scale = fit.clamp(0.02, ZOOM_MAX);
        let pan = dvec2((size.x - w * scale) * 0.5 - bounds.pos.x * scale, (size.y - h * scale) * 0.5 - bounds.pos.y * scale);
        Self { scale, pan }
    }

    pub fn to_screen(self, p: DVec2) -> DVec2 {
        self.pan + p * self.scale
    }

    /// Zoom by `factor`, keeping the graph point under `anchor` (canvas
    /// points) exactly where it is. Clamped to the zoom range.
    pub fn zoom_at(&self, anchor: DVec2, factor: f64) -> Self {
        let scale = (self.scale * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        let pan = anchor - (anchor - self.pan) * (scale / self.scale);
        Self { scale, pan }
    }
}

/// The canvas's two areas: the graph on the left, the slice list on the
/// right. The list keeps its width (narrower in a narrow canvas), so the
/// graph never draws under it. Returns `(graph width, list width)`.
pub(crate) fn split_canvas(width: f64) -> (f64, f64) {
    let wide = if width < 560.0 { 180.0 } else { 240.0 };
    let list = wide.min(width * 0.45);
    (width - list - LIST_GAP, list)
}

/// Where a node sits in the graph's logical points. The layout is
/// hand-picked: the happy path runs left to right, the SentBack loop
/// curves back over the top to Running, and the side exits drop below.
#[derive(Clone, Copy, Debug)]
struct NodePos {
    state: SliceState,
    x: f64,
    y: f64,
}

const NODE_W: f64 = 96.0;
const NODE_H: f64 = 44.0;
/// The main row's y, and the side exits' row below it.
const ROW1_Y: f64 = 64.0;
const ROW2_Y: f64 = 224.0;

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
        out.push(NodePos { state: *s, x: i as f64 * spacing, y: ROW1_Y });
    }
    for (i, s) in row2.iter().enumerate() {
        out.push(NodePos { state: *s, x: i as f64 * spacing + NODE_W * 0.5, y: ROW2_Y });
    }
    out
}

/// Everything the graph draws, in logical points: the nodes, the sent-back
/// arc above them, and the numbered badges over the picked path's nodes.
pub(crate) fn graph_bounds() -> Rect {
    let max_x = nodes().iter().map(|n| n.x + NODE_W).fold(0.0, f64::max);
    Rect { pos: dvec2(0.0, -18.0), size: dvec2(max_x, ROW2_Y + NODE_H + 18.0) }
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
    /// A slice was clicked once (its slug): its card on the flow canvas lights.
    Selected(String),
}

#[derive(Clone, Debug, Default)]
struct SliceHit {
    /// A slice list row's bounding rect on screen.
    rect: Rect,
    slug: String,
}

/// A text the canvas draws after its shapes: where, what, how big, in what
/// colour.
struct Text {
    pos: DVec2,
    text: String,
    size: f32,
    color: Vec4f,
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
    /// The shapes of this frame, drawn as quads.
    #[rust]
    pen: Pen,
    /// The pan-and-zoom the graph is shown through.
    #[rust]
    view: GraphView,
    /// A pan in progress: where the pointer was last.
    #[rust]
    drag: Option<DVec2>,
    /// The slice list's scroll offset.
    #[rust]
    list_scroll: f64,
    /// The canvas's size and its graph area's size when last drawn (a
    /// change means the view should fit again).
    #[rust]
    seen: DVec2,
    #[rust]
    graph_size: DVec2,
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

impl Default for GraphView {
    fn default() -> Self {
        Self { scale: 1.0, pan: dvec2(0.0, 0.0) }
    }
}

impl ScriptHook for LifecycleCanvas {}

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

impl LifecycleCanvas {
    /// The path the picked slice walked, in state order. Used to light up
    /// its nodes and edges.
    fn picked_path(&self) -> Option<Vec<SliceState>> {
        let slug = self.picked.as_deref()?;
        let path = self.slices.paths.iter().find(|p| p.slug == slug)?;
        let states: Vec<SliceState> = path.transitions.iter().map(|t| t.to).filter_map(transition_to_state).collect();
        Some(states)
    }

    /// The slices in list order (by slug): their slug, state and rework
    /// count, owned so the drawing loop can borrow the canvas.
    fn sorted_slices(&self) -> Vec<(String, SliceState, usize)> {
        let mut sorted: Vec<(String, SliceState, usize)> = self.slices.paths.iter().map(|p| (p.slug.clone(), p.current, p.rework_count)).collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        sorted
    }

    /// The list area's height that rows may fill (below the title).
    fn list_view_h(&self) -> f64 {
        (self.rect.size.y - 44.0).max(0.0)
    }

    /// Keep the list's scroll where the last row is still just in view.
    fn clamp_list_scroll(&mut self) {
        let rows_h = self.slices.paths.len() as f64 * 32.0;
        self.list_scroll = self.list_scroll.clamp(0.0, (rows_h - self.list_view_h()).max(0.0));
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

    fn text_w(&mut self, cx: &mut Cx2d, text: &str, size: f32) -> f64 {
        self.draw_text.text_style.font_size = size;
        self.draw_text.layout(cx, 0.0, 0.0, None, false, Align::default(), text).size_in_lpxs.width as f64
    }

    /// `text` cut with an ellipsis to fit `max_w` at `size`.
    fn fit_text(&mut self, cx: &mut Cx2d, text: &str, size: f32, max_w: f64) -> String {
        let full = self.text_w(cx, text, size);
        if full <= max_w || text.is_empty() {
            return text.to_string();
        }
        let per_char = full / text.chars().count() as f64;
        let mut n = ((max_w / per_char) as usize).saturating_sub(2).max(1);
        let mut cut: String = text.chars().take(n).collect();
        cut.push('…');
        while n > 1 && self.text_w(cx, &cut, size) > max_w {
            n -= 1;
            cut = text.chars().take(n).collect();
            cut.push('…');
        }
        cut
    }

    /// One edge: a cubic bezier from `a` to `b` (canvas points) with an
    /// arrowhead at `b`.
    fn draw_edge(&mut self, a: DVec2, c1: DVec2, c2: DVec2, b: DVec2, color: Vec4f, width: f32) {
        self.pen.set_color(color.x, color.y, color.z, color.w);
        self.pen.move_to(a.x as f32, a.y as f32);
        self.pen.bezier_to(c1.x as f32, c1.y as f32, c2.x as f32, c2.y as f32, b.x as f32, b.y as f32);
        self.pen.stroke(width);
        // The arrowhead points the way the curve arrives.
        let dir = (b - c2).normalize();
        let n = dvec2(-dir.y, dir.x) * 3.6;
        let back = b - dir * 7.0;
        self.pen.set_color(color.x, color.y, color.z, color.w);
        self.pen.move_to(b.x as f32, b.y as f32);
        self.pen.line_to((back.x + n.x) as f32, (back.y + n.y) as f32);
        self.pen.line_to((back.x - n.x) as f32, (back.y - n.y) as f32);
        self.pen.close();
        self.pen.fill();
    }

    /// The `×N` badge on an edge: a small rounded label at `at` (canvas
    /// points) that carries its own background, so it never eats the nodes.
    fn draw_badge(&mut self, cx: &mut Cx2d, texts: &mut Vec<Text>, at: DVec2, count: usize, scale: f64, color: Vec4f) {
        let label = format!("×{count}");
        let size = (7.5 * scale).max(3.0) as f32;
        let w = self.text_w(cx, &label, size) + 8.0;
        let h = size as f64 + 5.0;
        self.pen.set_color_hex(crate::theme::hex("raised"), 1.0);
        self.pen.rounded_rect((at.x - w * 0.5) as f32, (at.y - h * 0.5) as f32, w as f32, h as f32, 4.0);
        self.pen.fill();
        self.pen.set_color(color.x, color.y, color.z, 1.0);
        self.pen.rounded_rect((at.x - w * 0.5) as f32, (at.y - h * 0.5) as f32, w as f32, h as f32, 4.0);
        self.pen.stroke(1.0);
        texts.push(Text { pos: dvec2(at.x - w * 0.5 + 4.0, at.y - h * 0.5 + 2.5), text: label, size, color: crate::hex_color(crate::theme::hex("ink")) });
    }

    /// The point at `t` along the cubic bezier `a, c1, c2, b`.
    fn bezier_at(a: DVec2, c1: DVec2, c2: DVec2, b: DVec2, t: f64) -> DVec2 {
        let u = 1.0 - t;
        a * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + b * (t * t * t)
    }

    /// The legend: one colour chip per group of states (plus the edge
    /// badge), drawn in the strip under the graph, wrapping to as many
    /// rows as the width takes.
    fn draw_legend(&mut self, cx: &mut Cx2d, texts: &mut Vec<Text>, left: f64, right: f64, top: f64) {
        let mut entries: Vec<(Option<u32>, String)> = vec![
            (Some(crate::theme::hex("muted")), SliceState::Waiting.display().to_string()),
            (Some(crate::theme::hex("accent")), [SliceState::Running, SliceState::Reported, SliceState::Checked].iter().map(|s| s.display()).collect::<Vec<_>>().join("·")),
            (Some(crate::theme::hex("success")), [SliceState::Accepted, SliceState::Landed].iter().map(|s| s.display()).collect::<Vec<_>>().join("·")),
            (Some(crate::theme::hex("warning")), [SliceState::SentBack, SliceState::OverBudget].iter().map(|s| s.display()).collect::<Vec<_>>().join("·")),
            (Some(crate::theme::hex("danger")), SliceState::Failed.display().to_string()),
            (Some(crate::theme::hex("orange")), SliceState::Stopped.display().to_string()),
            (Some(crate::theme::hex("faint")), SliceState::Discarded.display().to_string()),
            (None, i18n::t("edge crossings", "边经过次数").to_string()),
        ];
        let size = 8.0f32;
        let mut x = left;
        let mut y = top;
        let line_h = 17.0;
        for (hex, label) in entries.drain(..) {
            let swatch_w = hex.map(|_| 16.0).unwrap_or(24.0);
            let tw = self.text_w(cx, &label, size);
            if x + swatch_w + tw + 6.0 > right && x > left {
                x = left;
                y += line_h;
            }
            match hex {
                Some(h) => {
                    self.pen.set_color_hex(h, 1.0);
                    self.pen.rounded_rect(x as f32, (y + 3.0) as f32, 8.0, 8.0, 2.0);
                    self.pen.fill();
                    texts.push(Text { pos: dvec2(x + 12.0, y), text: label, size, color: crate::hex_color(crate::theme::hex("muted_strong")) });
                }
                None => {
                    // The ×N badge the edges wear.
                    let w = self.text_w(cx, "×N", size) + 6.0;
                    self.pen.set_color_hex(crate::theme::hex("raised"), 1.0);
                    self.pen.rounded_rect(x as f32, (y + 1.0) as f32, w as f32, 12.0, 4.0);
                    self.pen.fill();
                    self.pen.set_color_hex(crate::theme::hex("muted_strong"), 1.0);
                    self.pen.rounded_rect(x as f32, (y + 1.0) as f32, w as f32, 12.0, 4.0);
                    self.pen.stroke(1.0);
                    texts.push(Text { pos: dvec2(x + 3.0, y + 2.5), text: "×N".to_string(), size, color: crate::hex_color(crate::theme::hex("muted_strong")) });
                    texts.push(Text { pos: dvec2(x + w + 6.0, y), text: label, size, color: crate::hex_color(crate::theme::hex("muted_strong")) });
                }
            }
            x += swatch_w + tw + 16.0;
        }
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
        // The areas as last drawn: the list takes the right strip.
        let (_, list_w) = split_canvas(self.rect.size.x);
        let list_left = self.rect.pos.x + self.rect.size.x - list_w;
        let in_list = |p: DVec2| p.x >= list_left;
        match event.hits(cx, self.area) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                // Where it shows (scrolled with whatever holds the canvas)
                // to the canvas's own coordinates, which the hits are in.
                let at = fe.abs - fe.rect.pos + self.rect.pos;
                if in_list(at) {
                    // A slice row: one click highlights its path through
                    // the graph; a double click also opens its documents.
                    if let Some(hit) = self.hits.iter().find(|h| h.rect.contains(at)).cloned() {
                        self.picked = Some(hit.slug.clone());
                        if fe.tap_count >= 2 {
                            cx.widget_action(uid, LifeAction::Picked(hit.slug));
                        } else {
                            cx.widget_action(uid, LifeAction::Selected(hit.slug));
                        }
                        self.redraw(cx);
                    }
                    self.drag = None;
                } else {
                    // A double click on the graph: fit it again.
                    if fe.tap_count >= 2 {
                        self.view = GraphView::fit(graph_bounds(), self.graph_size, FIT_PAD);
                        self.redraw(cx);
                    }
                    self.drag = Some(fe.abs);
                }
            }
            Hit::FingerMove(fe) => {
                if let Some(last) = self.drag {
                    self.view.pan += fe.abs - last;
                    self.drag = Some(fe.abs);
                    self.redraw(cx);
                }
            }
            Hit::FingerUp(_) => {
                self.drag = None;
            }
            Hit::FingerScroll(fe) => {
                if fe.modifiers.is_primary() || fe.modifiers.control {
                    // Ctrl/Cmd + wheel (a trackpad pinch arrives the same
                    // way): zoom centred on the pointer, as far as it turned.
                    let factor = (fe.scroll.y * 0.005).exp();
                    if factor != 1.0 {
                        self.view = self.view.zoom_at(fe.abs - fe.rect.pos, factor);
                        self.redraw(cx);
                    }
                } else if in_list(fe.abs - fe.rect.pos + self.rect.pos) {
                    // The list scrolls alone.
                    self.list_scroll += fe.scroll.y;
                    self.clamp_list_scroll();
                    self.redraw(cx);
                } else {
                    self.view.pan -= fe.scroll;
                    self.redraw(cx);
                }
            }
            Hit::FingerHoverOver(fe) => {
                let at = fe.abs - fe.rect.pos + self.rect.pos;
                let hand = in_list(at) && self.hits.iter().any(|h| h.rect.contains(at));
                cx.set_cursor(if hand { MouseCursor::Hand } else { MouseCursor::Default });
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
        // The two areas, never overlapping: the graph on the left, the
        // slice list on the right, the legend strip under the graph.
        let (graph_w, list_w) = split_canvas(rect.size.x);
        let graph_size = dvec2(graph_w, (rect.size.y - LEGEND_H).max(80.0));
        self.graph_size = graph_size;
        let list_pos = dvec2(rect.pos.x + rect.size.x - list_w, rect.pos.y);
        // Fit when the canvas is new or was resized (the person's zoom is
        // kept between redraws of the same size).
        if (self.seen - rect.size).length() > 0.5 {
            self.view = GraphView::fit(graph_bounds(), graph_size, FIT_PAD);
            self.clamp_list_scroll();
        }
        self.seen = rect.size;
        let v = self.view;
        let sp = |p: DVec2| rect.pos + v.to_screen(p);
        let fs = |pt: f64| (pt * v.scale).max(2.5) as f32;
        let ink = crate::hex_color(crate::theme::hex("ink"));
        let muted_strong = crate::hex_color(crate::theme::hex("muted_strong"));

        // Counts at each node: how many slices are currently there.
        let mut at: std::collections::BTreeMap<SliceState, usize> = std::collections::BTreeMap::new();
        for p in &self.slices.paths {
            *at.entry(p.current).or_default() += 1;
        }
        // Edge counts from the machine's own aggregation.
        let edge_counts: std::collections::BTreeMap<(TransitionKind, TransitionKind), usize> = self.slices.edges.iter().map(|e| ((e.from, e.to), e.count)).collect();
        // The picked slice's states: light up its nodes and edges, numbered
        // in the order it walked them.
        let picked: Vec<SliceState> = self.picked_path().unwrap_or_default();
        let picked_edges: std::collections::HashSet<(SliceState, SliceState)> = picked.windows(2).map(|w| (w[0], w[1])).collect();
        let mut numbers: std::collections::BTreeMap<SliceState, Vec<usize>> = std::collections::BTreeMap::new();
        for (i, s) in picked.iter().enumerate() {
            numbers.entry(*s).or_default().push(i + 1);
        }

        let pos_map: Vec<NodePos> = nodes();
        let pos_of = |s: SliceState| -> Option<NodePos> { pos_map.iter().copied().find(|n| n.state == s) };
        let mut texts: Vec<Text> = Vec::new();
        self.pen.begin();

        // Edges first (under the nodes).
        let accent = crate::hex_color(crate::theme::hex("accent"));
        let edge_color = crate::hex_color(crate::theme::hex("muted"));
        for (from, to) in &base_edges() {
            let Some(from_s) = transition_to_state(*from) else { continue };
            let Some(to_s) = transition_to_state(*to) else { continue };
            let Some((a, b)) = pos_of(from_s).zip(pos_of(to_s)) else { continue };
            let lit = picked_edges.contains(&(from_s, to_s));
            let count = edge_counts.get(&(*from, *to)).copied().unwrap_or(0);
            let color = if lit { accent } else { edge_color };
            let (width, alpha) = if lit { (1.6, 1.0) } else { (1.0, 0.45) };
            let mut c = color;
            c.w *= alpha as f32;
            // A chain edge leaves `a`'s right side for `b`'s left; a side
            // exit leaves Running's bottom for the node's top.
            let (p1, c1, c2, p2) = if b.y > a.y {
                let p1 = sp(dvec2(a.x + NODE_W * 0.5, a.y + NODE_H));
                let p2 = sp(dvec2(b.x + NODE_W * 0.5, b.y));
                (p1, dvec2(p1.x, p1.y + 34.0 * v.scale), dvec2(p2.x, p2.y - 34.0 * v.scale), p2)
            } else {
                let p1 = sp(dvec2(a.x + NODE_W, a.y + NODE_H * 0.5));
                let p2 = sp(dvec2(b.x, b.y + NODE_H * 0.5));
                (p1, dvec2((p1.x + p2.x) * 0.5, p1.y), dvec2((p1.x + p2.x) * 0.5, p2.y), p2)
            };
            self.draw_edge(p1, c1, c2, p2, c, width * v.scale as f32);
            if count > 0 {
                let mid = Self::bezier_at(p1, c1, c2, p2, 0.5);
                self.draw_badge(cx, &mut texts, mid, count, v.scale, color);
            }
        }

        // The SentBack loop: a curve over the top from Checked back to
        // Running (a fix review without a gate leaves from Reported; the
        // same loop carries it).
        if let (Some(a), Some(b)) = (pos_of(SliceState::Checked), pos_of(SliceState::Running)) {
            let warning = crate::theme::hex("warning");
            let lit = picked.contains(&SliceState::SentBack);
            let count = edge_counts.get(&(TransitionKind::Reported, TransitionKind::SentBack)).copied().unwrap_or(0)
                + edge_counts.get(&(TransitionKind::Checked, TransitionKind::SentBack)).copied().unwrap_or(0);
            let p1 = sp(dvec2(a.x + NODE_W * 0.5, a.y));
            let p2 = sp(dvec2(b.x + NODE_W * 0.5, b.y));
            let peak = ROW1_Y - 62.0;
            let c1 = dvec2(p1.x - 24.0 * v.scale, sp(dvec2(0.0, peak)).y);
            let c2 = dvec2(p2.x + 24.0 * v.scale, sp(dvec2(0.0, peak)).y);
            let mut color = crate::hex_color(warning);
            color.w = if lit { 1.0 } else { 0.7 };
            self.draw_edge(p1, c1, c2, p2, color, 1.2 * v.scale as f32);
            let top = Self::bezier_at(p1, c1, c2, p2, 0.5);
            let label = SliceState::SentBack.display().to_string();
            let size = fs(7.5);
            let tw = self.text_w(cx, &label, size);
            texts.push(Text { pos: dvec2(top.x - tw * 0.5, top.y - size as f64 - 2.0), text: label, size, color: crate::hex_color(warning) });
            if count > 0 {
                self.draw_badge(cx, &mut texts, dvec2(top.x, top.y + 3.0), count, v.scale, crate::hex_color(warning));
            }
        }

        // Nodes on top.
        let on_picked = |s: SliceState| picked.contains(&s);
        for n in &pos_map {
            let p = sp(dvec2(n.x, n.y));
            let size = dvec2(NODE_W, NODE_H) * v.scale;
            let color = self.color_for(n.state);
            let count = at.get(&n.state).copied().unwrap_or(0);
            let lit = on_picked(n.state);
            // Background.
            self.pen.set_color_hex(crate::theme::hex("raised"), 1.0);
            self.pen.rounded_rect(p.x as f32, p.y as f32, size.x as f32, size.y as f32, 6.0 * v.scale as f32);
            self.pen.fill();
            // Border, lit when the picked slice visited here, or always on Running.
            let border_alpha = if lit || n.state == SliceState::Running && count > 0 { 1.0 } else { 0.4 };
            self.pen.set_color_hex(color, border_alpha);
            self.pen.rounded_rect(p.x as f32, p.y as f32, size.x as f32, size.y as f32, 6.0 * v.scale as f32);
            self.pen.stroke((if lit { 1.6 } else { 1.0 }) * v.scale as f32);
            // The state's name, and under it the count: two lines that
            // cannot run into each other.
            let label = n.state.display().to_string();
            let lsize = fs(8.5);
            let lw = self.text_w(cx, &label, lsize);
            texts.push(Text { pos: dvec2(p.x + (size.x - lw) * 0.5, p.y + 6.0 * v.scale), text: label, size: lsize, color: ink });
            if count > 0 {
                let count_label = count.to_string();
                let csize = fs(12.0);
                let cw = self.text_w(cx, &count_label, csize);
                texts.push(Text { pos: dvec2(p.x + (size.x - cw) * 0.5, p.y + 18.0 * v.scale), text: count_label, size: csize, color: ink });
            }
            // The picked path's order: numbered dots over the node's top.
            if let Some(nums) = numbers.get(&n.state) {
                let r = 7.0 * v.scale;
                let mut cxp = p.x + size.x - r - 2.0;
                for num in nums.iter().rev() {
                    self.pen.set_color_hex(crate::theme::hex("accent"), 1.0);
                    self.pen.circle(cxp as f32, (p.y - r * 0.4) as f32, r as f32);
                    self.pen.fill();
                    let label = num.to_string();
                    let ns = fs(7.0);
                    let nw = self.text_w(cx, &label, ns);
                    texts.push(Text { pos: dvec2(cxp - nw * 0.5, p.y - r * 0.4 - ns as f64 * 0.6), text: label, size: ns, color: crate::hex_color(crate::theme::hex("on_accent")) });
                    cxp -= r * 2.0 + 2.0;
                }
            }
            // An active dot when something runs there.
            if n.state == SliceState::Running && count > 0 {
                let dot = dvec2(p.x + size.x * 0.5, p.y + size.y - 4.0 * v.scale);
                self.pen.set_color_hex(crate::theme::hex("accent"), (0.6 + 0.4 * self.phase) as f32);
                self.pen.circle(dot.x as f32, dot.y as f32, ((2.0 + self.phase * 3.0) * v.scale) as f32);
                self.pen.fill();
            }
        }

        // The slice list, in its own panel on the right: never under the
        // graph, scrolled alone by the wheel over it.
        let inner = Rect { pos: list_pos + dvec2(4.0, 4.0), size: dvec2(list_w - 8.0, rect.size.y - 8.0) };
        self.pen.set_color_hex(crate::theme::hex("panel"), 1.0);
        self.pen.rounded_rect(inner.pos.x as f32, inner.pos.y as f32, inner.size.x as f32, inner.size.y as f32, 6.0);
        self.pen.fill();
        // The separator between the graph and the list.
        self.pen.set_color_hex(crate::theme::hex("line"), 1.0);
        self.pen.move_to((list_pos.x - LIST_GAP * 0.5) as f32, (rect.pos.y + 8.0) as f32);
        self.pen.line_to((list_pos.x - LIST_GAP * 0.5) as f32, (rect.pos.y + rect.size.y - 8.0) as f32);
        self.pen.stroke(1.0);
        let title = i18n::pick(format!("Slices · {}", self.slices.paths.len()), format!("切片 · {}", self.slices.paths.len()));
        texts.push(Text { pos: dvec2(inner.pos.x + 6.0, inner.pos.y + 6.0), text: title, size: 9.0, color: muted_strong });
        self.clamp_list_scroll();
        let row_h = 30.0;
        let rows_top = inner.pos.y + 26.0;
        let sorted = self.sorted_slices();
        for (i, (slug, current, rework)) in sorted.iter().enumerate() {
            let y = rows_top + i as f64 * (row_h + 2.0) - self.list_scroll;
            if y + row_h < inner.pos.y || y > inner.pos.y + inner.size.y {
                continue;
            }
            let picked_row = self.picked.as_deref() == Some(slug.as_str());
            let row = Rect { pos: dvec2(inner.pos.x + 4.0, y), size: dvec2(inner.size.x - 8.0, row_h) };
            self.hits.push(SliceHit { rect: row, slug: slug.clone() });
            if picked_row {
                self.pen.set_color_hex(crate::theme::hex("accent_soft"), 1.0);
                self.pen.rounded_rect(row.pos.x as f32, row.pos.y as f32, row.size.x as f32, row.size.y as f32, 5.0);
                self.pen.fill();
            }
            // A dot in the state's colour, the slug beside it, the state
            // (and any rework) under it.
            self.pen.set_color_hex(self.color_for(*current), 1.0);
            self.pen.circle((row.pos.x + 8.0) as f32, (row.pos.y + 8.0) as f32, 3.5);
            self.pen.fill();
            let slug = self.fit_text(cx, slug, 9.0, row.size.x - 20.0);
            texts.push(Text { pos: dvec2(row.pos.x + 15.0, row.pos.y + 2.0), text: slug, size: 9.0, color: ink });
            let mut line2 = current.display().to_string();
            if *rework > 0 {
                line2.push_str(&format!(" · ×{} {}", rework, i18n::t("rework", "返工")));
            }
            let line2 = self.fit_text(cx, &line2, 8.0, row.size.x - 18.0);
            texts.push(Text { pos: dvec2(row.pos.x + 15.0, row.pos.y + 16.0), text: line2, size: 8.0, color: muted_strong });
        }

        // The legend, in the strip under the graph.
        self.draw_legend(cx, &mut texts, rect.pos.x + 6.0, rect.pos.x + graph_w - 6.0, rect.pos.y + graph_size.y + 8.0);
        // The shapes, as quads in the order they were laid.
        let back = self.draw_bg.color;
        for (r, color) in std::mem::take(&mut self.pen.quads) {
            self.draw_bg.color = color;
            self.draw_bg.draw_abs(cx, r);
        }
        self.draw_bg.color = back;

        // Texts in their own call, after the shapes.
        self.draw_text.new_draw_call(cx);
        for t in texts {
            self.draw_text.text_style.font_size = t.size;
            self.draw_text.color = t.color;
            self.draw_text.draw_abs(cx, t.pos, &t.text);
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
            // Another set of slices: fit the graph again. The same ones
            // (every sync passes them) keep the person's pan and zoom.
            let key = |m: &SliceMachine| m.paths.iter().map(|p| (p.peer_id.clone(), p.current, p.transitions.len())).collect::<Vec<_>>();
            if key(&c.slices) != key(&slices) {
                c.seen = dvec2(0.0, 0.0);
            }
            c.slices = slices;
            c.redraw(cx);
        }
    }

    /// Light the slice `slug` (a card picked on the flow canvas), or none.
    pub fn set_picked(&self, cx: &mut Cx, slug: Option<String>) {
        if let Some(mut c) = self.borrow_mut() {
            if c.picked != slug {
                c.picked = slug;
                c.redraw(cx);
            }
        }
    }

    /// Fit the graph into the canvas again.
    pub fn fit(&self, cx: &mut Cx) {
        if let Some(mut c) = self.borrow_mut() {
            c.view = GraphView::fit(graph_bounds(), c.graph_size, FIT_PAD);
            c.redraw(cx);
        }
    }

    pub fn actions(&self, actions: &Actions) -> Vec<LifeAction> {
        let uid = self.widget_uid();
        actions.iter().filter_map(|a| a.as_widget_action()).filter(|a| a.widget_uid == uid)
            .map(|a| a.cast::<LifeAction>()).filter(|a| !matches!(a, LifeAction::None)).collect()
    }
}

/// One row of the doc stream's list: a round separator or a document.
#[derive(Clone, Copy, Debug)]
enum LifeRow {
    Round(Option<u64>),
    Doc(usize),
}

impl OctoBuddyView {
    /// Build the shown session slice machine and hand it to the canvas.
    pub(crate) fn sync_life(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        let session = self.store.session(at).unwrap();
        let machine = crate::lifecycle::build_slice_machine(session);
        let rounds = crate::lifecycle::build_round_machine(session);
        // The doc stream starts with what the store gives us (per-session),
        // then folds in any repo docs and acceptance sheets the background
        // worker has produced for this project.
        let mut stream = crate::lifecycle::build_doc_stream(&[session], &Default::default());
        let project = self.store.projects[at.0].path.clone();
        let (repo_docs, acceptance) = self.lookup_life_cache(&project);
        let cached_repo: std::collections::BTreeMap<String, Vec<crate::lifecycle::RepoDocVersion>> = repo_docs.into_iter().collect();
        let cached_acc = acceptance;
        crate::lifecycle::extend_with_repo_docs(&mut stream, &cached_repo);
        crate::lifecycle::extend_with_acceptance(&mut stream, &project, &cached_acc);
        self.life_docs = stream.items.clone();
        if self.life_doc_picked.is_some_and(|i| i >= self.life_docs.len()) {
            self.life_doc_picked = None;
        }
        // The state machine canvas gets the slice machine.
        let canvas = self.view.widget(cx, ids!(life_sm)).as_lifecycle_canvas();
        canvas.set_slices(cx, machine.clone());
        // A card opened on the graph above: its slice picked here too.
        let open = match &self.flow_open { Some(crate::FlowOpen::Inner(id)) => Some(id.clone()), _ => None };
        if open != self.life_flow_seen {
            self.life_flow_seen = open.clone();
            if let Some(id) = open {
                let slug = machine.paths.iter().find(|p| p.peer_id == id).map(|p| p.slug.clone());
                if slug.is_some() {
                    canvas.set_picked(cx, slug);
                }
            }
        }
        // Folded to its title row: the graph above takes the room.
        self.view.label(cx, ids!(life_title)).set_text(cx, i18n::t("Lifecycle", "生命周期"));
        self.view.button(cx, ids!(life_fold)).set_text(cx, if self.life_folded { i18n::t("Show", "展开") } else { i18n::t("Hide", "收起") });
        // The two sub-tabs, one at a time: the one in use marked (an SegOn
        // beside its SegOff, like the view switcher's pairs).
        let show_sm = self.life_tab == crate::LifeTab::StateMachine;
        self.view.view(cx, ids!(life_divider)).set_visible(cx, !self.life_folded);
        self.apply_life_pane(cx);
        self.view.view(cx, ids!(life_sm_panel)).set_visible(cx, show_sm && !self.life_folded);
        self.view.view(cx, ids!(life_docs_panel)).set_visible(cx, !show_sm && !self.life_folded);
        for (off, on, here) in [
            (ids!(life_tab_sm), ids!(life_tab_sm_on), show_sm),
            (ids!(life_tab_docs), ids!(life_tab_docs_on), !show_sm),
        ] {
            self.view.button(cx, off).set_visible(cx, !here);
            self.view.button(cx, on).set_visible(cx, here);
        }
        self.view.button(cx, ids!(life_tab_sm)).set_text(cx, i18n::t("State machine", "状态机"));
        self.view.button(cx, ids!(life_tab_sm_on)).set_text(cx, i18n::t("State machine", "状态机"));
        self.view.button(cx, ids!(life_tab_docs)).set_text(cx, i18n::t("Docs", "文档流"));
        self.view.button(cx, ids!(life_tab_docs_on)).set_text(cx, i18n::t("Docs", "文档流"));
        self.view.button(cx, ids!(life_fit)).set_visible(cx, show_sm);
        self.view.button(cx, ids!(life_fit)).set_text(cx, i18n::t("Fit", "适应"));
        self.view.button(cx, ids!(life_open_living)).set_text(cx, i18n::t("Open LIVING.md", "打开 LIVING.md"));
        if !show_sm {
            self.render_life_docs(cx);
        }
        // Kick off (or skip) the background worker for this project. The
        // worker is cheap when nothing changed (it still calls git for the
        // same commit list — but the cache in `life_docs_cache` means the
        // UI re-render only happens when the result differs).
        self.maybe_refresh_life_docs(at);
        // A summary of where each round is: 「第 1 轮 · 全部接受」.
        let bits: Vec<String> = rounds.rounds.iter().map(|r| {
            i18n::pick(format!("Round {} · {}", r.round, r.current.display()), format!("第 {} 轮 · {}", r.round, r.current.display()))
        }).collect();
        let summary = if bits.is_empty() { i18n::t("No rounds yet.", "还没有轮次。").to_string() } else { bits.join(" · ") };
        self.view.label(cx, ids!(life_round)).set_text(cx, &summary);
        let _ = now_secs();
    }

    /// Find the cached repo docs + acceptance for a project. The cache is
    /// filled by `LoopEvent::LifeDocs` from the background worker.
    fn lookup_life_cache(&self, project: &str) -> (Vec<(String, Vec<crate::lifecycle::RepoDocVersion>)>, Vec<crate::lifecycle::AcceptanceItem>) {
        let repo_docs = self.life_repo_cache.iter().find(|c| c.project == project).map(|c| c.paths.clone()).unwrap_or_default();
        let acceptance = self.life_acceptance_cache.iter().find(|(p, _)| p == project).map(|(_, v)| v.clone()).unwrap_or_default();
        (repo_docs, acceptance)
    }

    /// Spawn the background worker for this project's sessions. A bump in
    /// the lifecycle token (anything that changed for this project) means
    /// the worker should run again.
    fn maybe_refresh_life_docs(&mut self, at: crate::model::SessionRef) {
        let project = self.store.projects[at.0].path.clone();
        let project_name = self.store.projects[at.0].name.clone();
        // What the docs and LIVING.md are built from: each slice's commits,
        // state and review, each session's messages, and its sheet.
        let token: u64 = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            for s in &self.store.projects[at.0].sessions {
                (s.id.as_str(), s.messages.len(), self.sheets.get(&s.id).and_then(Option::as_ref).map(|r| r.status.key())).hash(&mut h);
                for p in s.peers() {
                    (p.id.as_str(), p.commits.as_ref().map(Vec::len), p.status.as_str(), p.review.as_deref(), p.landed.as_deref()).hash(&mut h);
                }
            }
            h.finish()
        };
        if token == self.life_token && self.life_repo_cache.iter().any(|c| c.project == project) {
            return;
        }
        self.life_token = token;
        let sessions: Vec<SessionCommits> = self.store.projects[at.0].sessions.iter().map(|s| {
            let peers = s.peers().iter().map(|p| PeerCommits {
                slug: p.slug.clone(),
                round: p.round,
                commits: p.commits.clone().unwrap_or_default(),
            }).collect();
            SessionCommits { session: s.clone(), peers }
        }).collect();
        // Each session's latest acceptance sheet, as its receipt says.
        let receipts: Vec<crate::lifecycle::ReceiptStatus> = self.store.projects[at.0].sessions.iter().filter_map(|s| {
            let r = self.sheet_of(&s.id)?;
            let (_, round) = crate::acceptance::parse_key(&r.inputs)?;
            Some(crate::lifecycle::ReceiptStatus { kind: "Acceptance".into(), session: s.id.clone(), round: Some(round), confirmed: r.status == crate::receipts::Status::Confirmed })
        }).collect();
        let inbox = self.rt.inbox.clone();
        refresh_life_docs(project, project_name, sessions, receipts, inbox);
    }

    /// The doc stream's list rows: a round separator, then its docs in
    /// time order (the docs without a round come first, under their own
    /// separator).
    fn life_rows(&self) -> Vec<LifeRow> {
        let mut rows: Vec<LifeRow> = Vec::new();
        let mut last: Option<Option<u64>> = None;
        for (i, d) in self.life_docs.iter().enumerate() {
            if last != Some(d.round) {
                rows.push(LifeRow::Round(d.round));
                last = Some(d.round);
            }
            rows.push(LifeRow::Doc(i));
        }
        rows
    }

    /// The theme colour a doc kind's dot wears.
    fn kind_color(k: crate::lifecycle::DocKind) -> u32 {
        use crate::lifecycle::DocKind;
        match k {
            DocKind::Request => crate::theme::hex("accent"),
            DocKind::Plan => crate::theme::hex("purple"),
            DocKind::Spec => crate::theme::hex("orange"),
            DocKind::Report => crate::theme::hex("muted_strong"),
            DocKind::Gate => crate::theme::hex("warning"),
            DocKind::Review => crate::theme::hex("success"),
            DocKind::RepoDoc => crate::theme::hex("muted"),
            DocKind::Acceptance => crate::theme::hex("orange"),
            DocKind::Receipt => crate::theme::hex("success"),
        }
    }

    /// Fill the doc stream's list (a PortalList): a round separator, then
    /// its docs — one clickable row each, the picked one marked.
    pub(crate) fn draw_life_docs(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let rows = self.life_rows();
        if rows.is_empty() {
            // A PortalList keeps offering indices until an item draws
            // nothing: only the first shows the "nothing here" row.
            list.set_item_range(cx, 0, 1);
            while let Some(index) = list.next_visible_item(cx) {
                if index == 0 {
                    let item = list.item(cx, index, id!(Empty));
                    item.label(cx, ids!(text)).set_text(cx, i18n::t("Nothing yet.", "还没有。"));
                    item.draw_all_unscoped(cx);
                }
            }
            return;
        }
        list.set_item_range(cx, 0, rows.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(row) = rows.get(index) else { continue };
            match *row {
                LifeRow::Round(round) => {
                    let item = list.item(cx, index, id!(Round));
                    let text = match round {
                        Some(r) => i18n::pick(format!("Round {r}"), format!("第 {r} 轮")),
                        None => i18n::t("Project", "项目").to_string(),
                    };
                    item.label(cx, ids!(round)).set_text(cx, &text);
                    item.draw_all_unscoped(cx);
                }
                LifeRow::Doc(i) => {
                    let Some(d) = self.life_docs.get(i) else { continue };
                    let picked = self.life_doc_picked == Some(i);
                    let item = list.item(cx, index, if picked { id!(DocSel) } else { id!(Doc) });
                    let round = d.round.map(|r| i18n::pick(format!("R{r} · "), format!("第{r}轮 · "))).unwrap_or_default();
                    item.label(cx, ids!(title)).set_text(cx, &format!("{round}{} · {}", d.kind.display(), d.title));
                    let mut meta = d.author.clone();
                    if d.at > 0 {
                        meta.push_str(&format!(" · {}", when(d.at)));
                    }
                    if let Some(v) = d.version {
                        meta.push_str(&format!(" · v{v}"));
                    }
                    item.label(cx, ids!(meta)).set_text(cx, &meta);
                    let c = crate::hex_color(Self::kind_color(d.kind));
                    let mut dot = item.widget(cx, ids!(dot));
                    script_apply_eval!(cx, dot, { draw_bg +: {color: #(c)} });
                    item.draw_all_unscoped(cx);
                }
            }
        }
    }

    /// The doc stream's reader: the picked item's meta line, its versions
    /// (a repo doc that came back changed), and its body as Markdown.
    fn render_life_docs(&mut self, cx: &mut Cx) {
        let picked = self.life_doc_picked.and_then(|i| self.life_docs.get(i));
        let Some(d) = picked else {
            self.view.label(cx, ids!(life_doc_meta)).set_text(cx, i18n::t("Pick a doc on the left to read it here.", "左侧选一条文档来读。"));
            self.view.widget(cx, ids!(life_docs_body)).set_text(cx, "");
            self.view.view(cx, ids!(life_vers)).set_visible(cx, false);
            self.life_ver_map.clear();
            return;
        };
        // The meta line: kind, who, when, where, which version.
        let mut meta = format!("{} · {}", d.kind.display(), d.author);
        if d.at > 0 {
            meta.push_str(&format!(" · {}", when(d.at)));
        }
        if let Some(path) = &d.path {
            meta.push_str(&format!(" · {path}"));
        }
        if let Some(v) = d.version {
            meta.push_str(&format!(" · v{v}"));
        }
        self.view.label(cx, ids!(life_doc_meta)).set_text(cx, &meta);
        // A repo doc that came back changed: a button per version of the
        // same path, the one shown marked.
        let mut versions: Vec<(u32, usize)> = Vec::new();
        if let Some(path) = &d.path {
            for (i, other) in self.life_docs.iter().enumerate() {
                if other.path.as_deref() == Some(path.as_str()) {
                    if let Some(v) = other.version {
                        if !versions.iter().any(|(w, _)| *w == v) {
                            versions.push((v, i));
                        }
                    }
                }
            }
            versions.sort_by_key(|(v, _)| *v);
        }
        let buttons = [ids!(life_v0), ids!(life_v1), ids!(life_v2), ids!(life_v3), ids!(life_v4), ids!(life_v5)];
        self.life_ver_map.clear();
        let many = versions.len() > 1;
        self.view.view(cx, ids!(life_vers)).set_visible(cx, many);
        for (k, id) in buttons.iter().enumerate() {
            let button = self.view.button(cx, *id);
            match versions.get(k).filter(|_| many) {
                Some((v, _)) => {
                    self.life_ver_map.push(*v);
                    button.set_visible(cx, true);
                    button.set_text(cx, &format!("v{v}"));
                    let active = d.version == Some(*v);
                    let color = crate::hex_color(if active { crate::theme::hex("accent_soft") } else { crate::theme::hex("sidebar") });
                    let text = crate::hex_color(if active { crate::theme::hex("accent_ink") } else { crate::theme::hex("muted_strong") });
                    let mut w = self.view.widget(cx, *id);
                    script_apply_eval!(cx, w, {
                        draw_bg +: {color: #(color)}
                        draw_text +: {color: #(text)}
                    });
                }
                None => button.set_visible(cx, false),
            }
        }
        // The body: what is on hand, or the file's content (a repo doc at
        // a commit, or one on disk).
        let mut body = String::new();
        if let Some(text) = &d.body {
            body = text.clone();
        } else if d.kind == crate::lifecycle::DocKind::RepoDoc {
            if let (Some(commit), Some(path), Some(at)) = (&d.commit, &d.path, self.selected) {
                let project = self.store.projects[at.0].path.clone();
                match crate::lifecycle::git_show_file(&project, commit, path) {
                    Ok(content) => body = content,
                    Err(e) => body = format!("(git show failed: {e})"),
                }
            }
        } else if let Some(path) = &d.path {
            match std::fs::read_to_string(path) {
                Ok(content) => body = content,
                Err(e) => body = format!("({e})"),
            }
        }
        if body.is_empty() {
            body = i18n::t("(nothing to show)", "（没有内容）").to_string();
        }
        self.view.widget(cx, ids!(life_docs_body)).set_text(cx, &body);
    }

    /// What the lifecycle view does on clicks (tabs, the fit button, slice
    /// picks, doc rows, version switches).
    pub(crate) fn handle_life_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        if self.stage != Stage::Flow { return; }
        if self.view.button(cx, ids!(life_fold)).clicked(actions) {
            self.life_folded = !self.life_folded;
            self.relayout(cx);
            return;
        }
        if self.view.button(cx, ids!(life_tab_sm)).clicked(actions) || self.view.button(cx, ids!(life_tab_sm_on)).clicked(actions) {
            if self.life_tab != crate::LifeTab::StateMachine {
                self.life_tab = crate::LifeTab::StateMachine;
                self.relayout(cx);
            }
            return;
        }
        if self.view.button(cx, ids!(life_tab_docs)).clicked(actions) || self.view.button(cx, ids!(life_tab_docs_on)).clicked(actions) {
            if self.life_tab != crate::LifeTab::Docs {
                self.life_tab = crate::LifeTab::Docs;
                self.relayout(cx);
            }
            return;
        }
        if self.view.button(cx, ids!(life_fit)).clicked(actions) {
            self.view.widget(cx, ids!(life_sm)).as_lifecycle_canvas().fit(cx);
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
        // A version button: switch to that version of the same path.
        for (k, id) in [ids!(life_v0), ids!(life_v1), ids!(life_v2), ids!(life_v3), ids!(life_v4), ids!(life_v5)].iter().enumerate() {
            if self.view.button(cx, *id).clicked(actions) {
                let Some(want) = self.life_ver_map.get(k).copied() else { continue };
                let path = self.life_doc_picked.and_then(|i| self.life_docs.get(i)).and_then(|d| d.path.clone());
                if let Some(index) = path.and_then(|path| self.life_docs.iter().position(|d| d.path.as_deref() == Some(path.as_str()) && d.version == Some(want))) {
                    self.life_doc_picked = Some(index);
                    self.render_life_docs(cx);
                    cx.redraw_all();
                }
                return;
            }
        }
        // A doc row: pick it and show it.
        let list = self.view.portal_list(cx, ids!(life_docs_list));
        let rows = self.life_rows();
        let mut picked = None;
        for (index, item) in list.items_with_actions(actions) {
            if crate::tapped(&item.view(cx, ids!(row)), actions) {
                if let Some(LifeRow::Doc(i)) = rows.get(index) {
                    picked = Some(*i);
                }
            }
        }
        if let Some(i) = picked {
            self.life_doc_picked = Some(i);
            self.render_life_docs(cx);
            cx.redraw_all();
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
                LifeAction::Selected(slug) => {
                    // Its card on the flow canvas, lit.
                    let peer = self.selected.and_then(|at| self.store.session(at))
                        .and_then(|s| s.peers().iter().rev().find(|p| p.slug == slug).map(|p| p.id.clone()));
                    if peer.is_some() && self.life_flow_lit != peer {
                        self.life_flow_lit = peer;
                        self.relayout(cx);
                    }
                }
                LifeAction::None => {}
            }
        }
    }
}

/// Compute the repo docs + acceptance sheets for one project, off the UI
/// thread. Posts the result as `LoopEvent::LifeDocs`. The UI thread takes
/// the cache and folds the repo docs + acceptance into the doc stream.
///
/// `sessions` is the slice of the project's sessions and their peers'
/// commit lists, as the UI thread saw them. Anything the worker finds
/// (new commits, new files) gets folded into the cached map.
pub fn refresh_life_docs(
    project: String,
    project_name: String,
    sessions: Vec<SessionCommits>,
    receipts: Vec<crate::lifecycle::ReceiptStatus>,
    inbox: crate::events::Inbox,
) {
    std::thread::spawn(move || {
        let mut peer_commits: Vec<crate::lifecycle::PeerCommit> = Vec::new();
        for s in &sessions {
            for peer in &s.peers {
                for c in peer.commits.iter() {
                    let hash = match c.split_whitespace().next() {
                        Some(h) => h.to_string(),
                        None => continue,
                    };
                    let files = crate::claims::committed(&project, &hash);
                    let ct = crate::workspace::git(&project, &["show", "-s", "--format=%ct", &hash])
                        .ok()
                        .and_then(|s| s.trim().parse::<u64>().ok())
                        .unwrap_or(0);
                    let files = files.into_iter().collect();
                    peer_commits.push(crate::lifecycle::PeerCommit {
                        slug: peer.slug.clone(),
                        round: peer.round,
                        hash,
                        at: ct,
                        files,
                    });
                }
            }
        }
        let repo_docs = crate::lifecycle::group_repo_docs(&peer_commits);
        let now = crate::model::now_secs();
        let owned: Vec<crate::model::Session> = sessions.iter().map(|s| s.session.clone()).collect();
        let session_refs: Vec<&_> = owned.iter().collect();
        let acceptance = crate::lifecycle::acceptance_items(&project, &session_refs, &receipts, now);
        // Build LIVING.md off the UI thread too: the worker already has
        // everything it needs. The project's sessions all fold into one
        // document, so we combine their slice/round machines and doc
        // streams first.
        let machines: Vec<crate::lifecycle::SliceMachine> = owned.iter().map(crate::lifecycle::build_slice_machine).collect();
        let round_machines: Vec<crate::lifecycle::RoundMachine> = owned.iter().map(crate::lifecycle::build_round_machine).collect();
        let mut combined_stream = crate::lifecycle::DocStream::default();
        for s in &owned {
            let mut st = crate::lifecycle::build_doc_stream(&[s], &Default::default());
            crate::lifecycle::extend_with_repo_docs(&mut st, &repo_docs);
            crate::lifecycle::extend_with_acceptance(&mut st, &project, &acceptance);
            combined_stream.items.extend(st.items);
        }
        let mut combined_machine = crate::lifecycle::SliceMachine::default();
        for m in machines { combined_machine.paths.extend(m.paths); combined_machine.edges.extend(m.edges); }
        let mut combined_rounds = crate::lifecycle::RoundMachine::default();
        for m in round_machines { combined_rounds.rounds.extend(m.rounds); combined_rounds.edges.extend(m.edges); }
        let content = crate::lifecycle::build_living(&project_name, &combined_machine, &combined_rounds, &combined_stream, &repo_docs, &acceptance);
        let outcome = crate::lifecycle::write_living_md(&project, &content);
        crate::events::post(&inbox, crate::events::LoopEvent::LifeDocs { project: project.clone(), repo_docs, acceptance });
        crate::events::post(&inbox, crate::events::LoopEvent::LivingWrite { project, outcome });
    });
}

/// One session's commit list, shaped for the background worker. The UI
/// thread clones what it needs before sending so the worker can outlive
/// the borrow of the store.
#[derive(Clone)]
pub struct SessionCommits {
    pub session: crate::model::Session,
    pub peers: Vec<PeerCommits>,
}

#[derive(Clone)]
pub struct PeerCommits {
    pub slug: String,
    pub round: u64,
    pub commits: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fit puts every corner of the graph's bounds inside the viewport
    /// (with the pad to spare), whatever the viewport's shape.
    #[test]
    fn fit_shows_every_node() {
        let bounds = graph_bounds();
        for size in [dvec2(360.0, 260.0), dvec2(600.0, 400.0), dvec2(1200.0, 700.0), dvec2(120.0, 90.0)] {
            let v = GraphView::fit(bounds, size, FIT_PAD);
            for corner in [
                dvec2(bounds.pos.x, bounds.pos.y),
                dvec2(bounds.pos.x + bounds.size.x, bounds.pos.y),
                dvec2(bounds.pos.x, bounds.pos.y + bounds.size.y),
                dvec2(bounds.pos.x + bounds.size.x, bounds.pos.y + bounds.size.y),
            ] {
                let p = v.to_screen(corner);
                assert!((-0.5..=size.x + 0.5).contains(&p.x), "{size:?}: {corner:?} -> {p:?}");
                assert!((-0.5..=size.y + 0.5).contains(&p.y), "{size:?}: {corner:?} -> {p:?}");
            }
            // Every node, not just the bounds' corners.
            for n in nodes() {
                let p = v.to_screen(dvec2(n.x, n.y));
                let q = v.to_screen(dvec2(n.x + NODE_W, n.y + NODE_H));
                assert!((-0.5..=size.x + 0.5).contains(&q.x) && (-0.5..=size.y + 0.5).contains(&q.y) && p.x >= -0.5 && p.y >= -0.5, "{size:?}: {} out", n.state.display());
            }
        }
    }

    /// Zooming keeps the graph point under the pointer exactly where it
    /// was, and stays inside the zoom range.
    #[test]
    fn zoom_keeps_the_point_under_the_pointer() {
        let v = GraphView::fit(graph_bounds(), dvec2(600.0, 400.0), FIT_PAD);
        let anchor = dvec2(123.0, 77.0);
        let under = (anchor - v.pan) / v.scale;
        for factor in [1.1, 1.25, 0.8, 0.5, 3.0] {
            let z = v.zoom_at(anchor, factor);
            let p = z.to_screen(under);
            assert!((p.x - anchor.x).abs() < 1e-9 && (p.y - anchor.y).abs() < 1e-9, "factor {factor}: {p:?}");
            assert!(z.scale >= ZOOM_MIN && z.scale <= ZOOM_MAX);
        }
        // The clamp holds at both ends.
        assert_eq!(v.zoom_at(anchor, 100.0).scale, ZOOM_MAX);
        assert_eq!(v.zoom_at(anchor, 0.001).scale, ZOOM_MIN);
    }

    /// The graph area and the slice list never overlap, at any width (the
    /// list yields its width when the canvas gets narrow).
    #[test]
    fn graph_and_list_never_overlap() {
        for w in [120.0, 200.0, 360.0, 559.0, 560.0, 600.0, 900.0, 1400.0] {
            let (graph, list) = split_canvas(w);
            assert!(graph + list + LIST_GAP <= w + 1e-9, "w {w}: {graph} + {list}");
            assert!((0.0..=240.0).contains(&list), "w {w}: list {list}");
            assert!(graph >= 0.0, "w {w}: graph {graph}");
        }
        // The two widths the narrow/wide switch actually produces.
        assert_eq!(split_canvas(900.0).1, 240.0);
        assert_eq!(split_canvas(400.0).1, 180.0);
    }
}

/// The canvas's shapes as plain quads (`DrawColor`), the way its texts and
/// background are drawn: `DrawVector` meshes landed shifted from them once
/// the canvas sat under the flow graph. Same calls as `DrawVector`; a fill
/// is scanned into rows, a stroke laid as small squares along its path.
#[derive(Default)]
struct Pen {
    color: Vec4f,
    /// The path being built: its subpaths, and whether each is closed.
    path: Vec<(Vec<DVec2>, bool)>,
    /// What is to be drawn, in order: (rect, colour).
    quads: Vec<(Rect, Vec4f)>,
}

impl Pen {
    fn begin(&mut self) {
        self.path.clear();
        self.quads.clear();
    }
    fn set_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.color = vec4(r, g, b, a);
    }
    fn set_color_hex(&mut self, hex: u32, alpha: f32) {
        let c = |s: u32| ((hex >> s) & 0xff) as f32 / 255.0;
        self.color = vec4(c(16), c(8), c(0), alpha);
    }
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.push((vec![dvec2(x as f64, y as f64)], false));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        match self.path.last_mut() {
            Some((points, _)) => points.push(dvec2(x as f64, y as f64)),
            None => self.move_to(x, y),
        }
    }
    fn bezier_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let Some(a) = self.path.last().and_then(|(p, _)| p.last().copied()) else { return };
        let (c1, c2, b) = (dvec2(x1 as f64, y1 as f64), dvec2(x2 as f64, y2 as f64), dvec2(x as f64, y as f64));
        for i in 1..=24 {
            let t = i as f64 / 24.0;
            let u = 1.0 - t;
            let p = a * (u * u * u) + c1 * (3.0 * u * u * t) + c2 * (3.0 * u * t * t) + b * (t * t * t);
            self.line_to(p.x as f32, p.y as f32);
        }
    }
    fn close(&mut self) {
        if let Some((_, closed)) = self.path.last_mut() {
            *closed = true;
        }
    }
    fn rounded_rect(&mut self, x: f32, y: f32, w: f32, h: f32, _r: f32) {
        let (x, y, w, h) = (x as f64, y as f64, w as f64, h as f64);
        self.path.push((vec![dvec2(x, y), dvec2(x + w, y), dvec2(x + w, y + h), dvec2(x, y + h)], true));
    }
    fn circle(&mut self, cx: f32, cy: f32, r: f32) {
        let (cx, cy, r) = (cx as f64, cy as f64, r as f64);
        let points = (0..20).map(|i| {
            let a = i as f64 / 20.0 * std::f64::consts::TAU;
            dvec2(cx + r * a.cos(), cy + r * a.sin())
        }).collect();
        self.path.push((points, true));
    }
    /// Each subpath filled (a convex outline: a row's span is its extremes).
    fn fill(&mut self) {
        for (points, _) in std::mem::take(&mut self.path) {
            if points.len() < 3 {
                continue;
            }
            let top = points.iter().map(|p| p.y).fold(f64::MAX, f64::min);
            let bottom = points.iter().map(|p| p.y).fold(f64::MIN, f64::max);
            let (left, right) = (points.iter().map(|p| p.x).fold(f64::MAX, f64::min), points.iter().map(|p| p.x).fold(f64::MIN, f64::max));
            // An upright rectangle: one quad.
            if points.len() == 4 && points.iter().all(|p| (p.x - left).abs() < 0.01 || (p.x - right).abs() < 0.01) {
                self.quads.push((Rect { pos: dvec2(left, top), size: dvec2(right - left, bottom - top) }, self.color));
                continue;
            }
            let mut y = top;
            while y < bottom {
                let row = y + 0.5;
                let mut span: Option<(f64, f64)> = None;
                for i in 0..points.len() {
                    let (a, b) = (points[i], points[(i + 1) % points.len()]);
                    if (a.y <= row && b.y > row) || (b.y <= row && a.y > row) {
                        let x = a.x + (row - a.y) / (b.y - a.y) * (b.x - a.x);
                        span = Some(span.map_or((x, x), |(l, r)| (l.min(x), r.max(x))));
                    }
                }
                if let Some((l, r)) = span {
                    self.quads.push((Rect { pos: dvec2(l, y), size: dvec2(r - l, 1.0) }, self.color));
                }
                y += 1.0;
            }
        }
    }
    /// Each subpath's segments laid as squares `width` across.
    fn stroke(&mut self, width: f32) {
        let w = (width as f64).max(1.0);
        for (mut points, closed) in std::mem::take(&mut self.path) {
            if closed && points.len() > 2 {
                points.push(points[0]);
            }
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                // Upright and level lines: one quad each.
                if (a.x - b.x).abs() < 0.01 || (a.y - b.y).abs() < 0.01 {
                    let (l, t) = (a.x.min(b.x) - w * 0.5, a.y.min(b.y) - w * 0.5);
                    self.quads.push((Rect { pos: dvec2(l, t), size: dvec2((a.x - b.x).abs() + w, (a.y - b.y).abs() + w) }, self.color));
                    continue;
                }
                let steps = ((b - a).length() / (w * 0.6).max(0.8)).ceil().max(1.0) as usize;
                for i in 0..=steps {
                    let p = a + (b - a) * (i as f64 / steps as f64);
                    self.quads.push((Rect { pos: dvec2(p.x - w * 0.5, p.y - w * 0.5), size: dvec2(w, w) }, self.color));
                }
            }
        }
    }
}
