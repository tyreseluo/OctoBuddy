//! The disk-map panel's Makepad widget: a self-drawn treemap canvas.
//!
//! The widget reads a `disk_map::Scan` plus the current crumb chain and
//! draws the children of that directory as colored squares (one per
//! `Category`) using `disktree_core::treemap::layout` for placement and
//! `treemap::hit` for picking. A click picks a tile, a double-click
//! drills into it.
//!
//! This is the panel half of the plugin (`disk_map.rs` is the testable
//! half: scans, snapshots, diff, tool text). All file walking and text
//! formatting happens there; this file only paints and routes clicks.
use crate::plugins::disk_map::{self, Scan};
use disktree_core::{classify::Category, treemap, treemap::Rect as TreemapRect};
use makepad_widgets::*;

/// What the widget hands back when a tile is picked or drilled into.
#[derive(Clone, Debug)]
pub enum DiskAction {
    Picked { crumbs: Vec<usize> },
    Drilled { crumbs: Vec<usize> },
}

fn color_for(cat: Category) -> Vec4f {
    match cat {
        Category::Code => vec4(0.20, 0.55, 0.85, 1.0),
        Category::AgentScratch => vec4(0.55, 0.35, 0.75, 1.0),
        Category::Toolchain => vec4(0.55, 0.55, 0.20, 1.0),
        Category::Synced => vec4(0.20, 0.65, 0.55, 1.0),
        Category::Git => vec4(0.85, 0.45, 0.20, 1.0),
        Category::Media => vec4(0.85, 0.25, 0.55, 1.0),
        Category::Documents => vec4(0.40, 0.40, 0.65, 1.0),
        Category::Cache => vec4(0.50, 0.50, 0.50, 1.0),
        Category::Other => vec4(0.30, 0.30, 0.30, 1.0),
    }
}

#[derive(Clone)]
struct ShownTile {
    rect: TreemapRect,
    /// A directory's title strip, when the layout gave it one.
    header: Option<TreemapRect>,
    /// How deep it sits under the directory shown (0: a direct child).
    depth: u32,
    name: String,
    bytes: u64,
    category: Category,
    crumbs: Vec<usize>,
}

#[derive(Script, ScriptHook, Widget)]
pub struct DiskCanvas {
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
    area: Area,
    #[rust]
    scan: Option<Scan>,
    /// The directory shown (crumbs into the scan). Empty: the scan root.
    #[rust]
    crumbs: Vec<usize>,
    #[rust]
    tiles: Vec<ShownTile>,
    /// The crumbs the user picked last (highlight).
    #[rust]
    selected: Option<Vec<usize>>,
    /// The crumbs the cursor is over.
    #[rust]
    hover: Option<Vec<usize>>,
    /// The drawing area (filled at draw time).
    #[rust]
    area_rect: TreemapRect,
    /// A layout pass is queued (the scan changed): re-layout on next draw.
    #[rust]
    relayout: bool,
    /// The rect the tiles were last laid out in: another size lays them out again.
    #[rust]
    laid_in: Option<TreemapRect>,
}

impl DiskCanvas {
    pub fn set_scan(&mut self, cx: &mut Cx, scan: Option<Scan>) {
        self.scan = scan;
        self.crumbs.clear();
        self.selected = None;
        self.hover = None;
        self.relayout = true;
        self.redraw(cx);
    }

    pub fn drill(&mut self, cx: &mut Cx, crumbs: &[usize]) {
        self.crumbs = crumbs.to_vec();
        self.selected = None;
        self.relayout = true;
        self.redraw(cx);
    }

    pub fn back(&mut self, cx: &mut Cx) {
        if !self.crumbs.is_empty() {
            self.crumbs.pop();
            self.selected = None;
            self.relayout = true;
            self.redraw(cx);
        }
    }

    #[allow(dead_code)]
    pub fn up_path(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(scan) = &self.scan {
            let mut node = &scan.tree;
            out.push(node.name.clone().into_string());
            for c in &self.crumbs {
                node = &node.children[*c];
                out.push(node.name.clone().into_string());
            }
        }
        out
    }

    fn relayout_tiles(&mut self, area: TreemapRect) {
        self.tiles.clear();
        let Some(scan) = self.scan.clone() else { return };
        let raw = disk_map::layout(&scan, &self.crumbs, area);
        for t in raw {
            match t.kind {
                treemap::TileKind::Node { crumbs } => {
                    if let Some(node) = scan.resolve(&crumbs) {
                        self.tiles.push(ShownTile { rect: t.rect, header: t.header, depth: t.depth, name: node.name.clone().into_string(), bytes: node.bytes, category: node.category, crumbs });
                    }
                }
                treemap::TileKind::Others { .. } => {}
            }
        }
    }

    /// The tile under a pointer event. `abs` is where it shows on screen,
    /// `shown` where this canvas shows (scrolled with whatever holds it);
    /// the tiles are in the canvas's own coordinates, so take the scroll off.
    fn tile_at_event(&self, abs: DVec2, shown: Rect) -> Option<&ShownTile> {
        let x = (abs.x - shown.pos.x) as f32 + self.area_rect.x;
        let y = (abs.y - shown.pos.y) as f32 + self.area_rect.y;
        self.tile_at(x, y)
    }

    fn tile_at(&self, x: f32, y: f32) -> Option<&ShownTile> {
        self.tiles.iter().rev().find(|t| t.rect.contains(x, y))
    }
}

impl Widget for DiskCanvas {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        match event.hits(cx, self.area) {
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                if let Some(tile) = self.tile_at_event(fe.abs, fe.rect) {
                    let crumbs = tile.crumbs.clone();
                    self.selected = Some(crumbs.clone());
                    let is_dir = self.scan.as_ref().and_then(|s| s.resolve(&crumbs)).is_some_and(|n| n.is_dir());
                    if fe.tap_count >= 2 && is_dir {
                        cx.widget_action(uid, DiskAction::Drilled { crumbs: crumbs.clone() });
                        self.drill(cx, &crumbs);
                    } else {
                        cx.widget_action(uid, DiskAction::Picked { crumbs });
                    }
                    self.redraw(cx);
                }
            }
            Hit::FingerHoverOver(fe) => {
                let over = self.tile_at_event(fe.abs, fe.rect).map(|t| t.crumbs.clone());
                if over.is_some() {
                    cx.set_cursor(MouseCursor::Hand);
                }
                if over != self.hover {
                    self.hover = over;
                    self.redraw(cx);
                }
            }
            Hit::FingerHoverOut(_) if self.hover.take().is_some() => {
                self.redraw(cx);
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.walk_turtle(walk);
        self.draw_bg.draw_abs(cx, rect);
        cx.begin_turtle(
            Walk { abs_pos: Some(rect.pos), width: Size::Fixed(rect.size.x), height: Size::Fixed(rect.size.y), ..Default::default() },
            Layout { clip_x: true, clip_y: true, ..Layout::default() },
        );
        self.area_rect = TreemapRect::new(rect.pos.x as f32, rect.pos.y as f32, rect.size.x as f32, rect.size.y as f32);
        let resized = self.laid_in.is_none_or(|l| (l.x, l.y, l.w, l.h) != (self.area_rect.x, self.area_rect.y, self.area_rect.w, self.area_rect.h));
        if self.relayout || resized {
            self.relayout_tiles(self.area_rect);
            self.relayout = false;
            self.laid_in = Some(self.area_rect);
        }
        // Each tile a plain quad (parents first, so a child sits on top),
        // inset a pixel so the edges between them show.
        let back = self.draw_bg.color;
        for tile in &self.tiles {
            let r = tile.rect;
            let color = color_for(tile.category);
            let selected = self.selected.as_ref().is_some_and(|s| s == &tile.crumbs);
            let hovered = self.hover.as_ref().is_some_and(|h| h == &tile.crumbs);
            // Deeper is lighter, so a child stands out on its parent; the
            // picked and the hovered tile lighter still.
            let lift = tile.depth as f32 * 0.07 + if selected { 0.22 } else if hovered { 0.10 } else { 0.0 };
            let shade = |c: f32, d: f32| (c + d).clamp(0.0, 1.0);
            self.draw_bg.color = vec4(shade(color.x, lift), shade(color.y, lift), shade(color.z, lift), 1.0);
            let inset = 1.0;
            self.draw_bg.draw_abs(cx, Rect {
                pos: dvec2(r.x as f64 + inset, r.y as f64 + inset),
                size: dvec2((r.w as f64 - 2.0 * inset).max(0.0), (r.h as f64 - 2.0 * inset).max(0.0)),
            });
            // A directory's title strip a shade darker than its body.
            if let Some(h) = tile.header {
                self.draw_bg.color = vec4(shade(color.x, lift - 0.08), shade(color.y, lift - 0.08), shade(color.z, lift - 0.08), 1.0);
                self.draw_bg.draw_abs(cx, Rect {
                    pos: dvec2(h.x as f64 + inset, h.y as f64 + inset),
                    size: dvec2((h.w as f64 - 2.0 * inset).max(0.0), (h.h as f64 - inset).max(0.0)),
                });
            }
        }
        self.draw_bg.color = back;
        // The names in a draw call of their own, in front of every tile: a
        // directory's in its title strip, a leaf's (with its size) inside it.
        self.draw_text.new_draw_call(cx);
        for tile in &self.tiles {
            let r = tile.rect;
            match tile.header {
                Some(h) if h.w > 40.0 && h.h >= 10.0 => {
                    self.draw_text.text_style.font_size = 8.5;
                    self.draw_text.color = vec4(1.0, 1.0, 1.0, 0.95);
                    let label = fit(&format!("{}  {}", tile.name, size_short(tile.bytes)), h.w as f64 - 8.0, 8.5);
                    self.draw_text.draw_abs(cx, dvec2(h.x as f64 + 4.0, h.y as f64 + 1.0), &label);
                }
                Some(_) => {}
                None if r.w > 60.0 && r.h > 30.0 => {
                    self.draw_text.text_style.font_size = 9.0;
                    self.draw_text.color = vec4(1.0, 1.0, 1.0, 0.95);
                    self.draw_text.draw_abs(cx, dvec2(r.x as f64 + 5.0, r.y as f64 + 4.0), &fit(&tile.name, r.w as f64 - 10.0, 9.0));
                    self.draw_text.text_style.font_size = 8.0;
                    self.draw_text.color = vec4(1.0, 1.0, 1.0, 0.8);
                    self.draw_text.draw_abs(cx, dvec2(r.x as f64 + 5.0, r.y as f64 + 17.0), &size_short(tile.bytes));
                }
                None => {}
            }
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

/// `text` cut with an ellipsis to about `width` points at `size` (CJK
/// characters count double), so a name never runs into its neighbour.
fn fit(text: &str, width: f64, size: f64) -> String {
    let unit = size * 0.6;
    let cost = |c: char| if c.is_ascii() { unit } else { unit * 1.8 };
    if text.chars().map(cost).sum::<f64>() <= width {
        return text.to_string();
    }
    let mut used = unit;
    let mut out = String::new();
    for c in text.chars() {
        used += cost(c);
        if used > width {
            break;
        }
        out.push(c);
    }
    if out.is_empty() { String::new() } else { format!("{out}…") }
}

pub fn size_short(b: u64) -> String {
    disktree_core::size::human_bytes_short(b)
}

#[allow(dead_code)]
pub fn category_color(c: Category) -> Vec4f {
    color_for(c)
}
