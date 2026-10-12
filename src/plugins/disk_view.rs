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
                        if node.is_dir() {
                            self.tiles.push(ShownTile { rect: t.rect, name: node.name.clone().into_string(), bytes: node.bytes, category: node.category, crumbs });
                        }
                    }
                }
                treemap::TileKind::Others { .. } => {}
            }
        }
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
                if let Some(tile) = self.tile_at(fe.abs.x as f32, fe.abs.y as f32) {
                    let crumbs = tile.crumbs.clone();
                    self.selected = Some(crumbs.clone());
                    if fe.tap_count >= 2 {
                        cx.widget_action(uid, DiskAction::Drilled { crumbs: crumbs.clone() });
                        self.drill(cx, &crumbs);
                    } else {
                        cx.widget_action(uid, DiskAction::Picked { crumbs });
                    }
                    self.redraw(cx);
                }
            }
            Hit::FingerHoverOver(fe) => {
                let over = self.tile_at(fe.abs.x as f32, fe.abs.y as f32).map(|t| t.crumbs.clone());
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
        cx.begin_turtle(
            Walk { abs_pos: Some(rect.pos), width: Size::Fixed(rect.size.x), height: Size::Fixed(rect.size.y), ..Default::default() },
            Layout { clip_x: true, clip_y: true, ..Layout::default() },
        );
        self.area_rect = TreemapRect::new(rect.pos.x as f32, rect.pos.y as f32, rect.size.x as f32, rect.size.y as f32);
        self.draw_bg.draw_abs(cx, rect);
        if self.relayout {
            self.relayout_tiles(self.area_rect);
            self.relayout = false;
        }
        self.draw_vector.begin();
        for tile in &self.tiles {
            let r = tile.rect;
            let color = color_for(tile.category);
            let selected = self.selected.as_ref().is_some_and(|s| s == &tile.crumbs);
            let fill = if selected { vec4(color.x * 1.2 + 0.05, color.y * 1.2 + 0.05, color.z * 1.2 + 0.05, 1.0) } else { color };
            self.draw_vector.set_color(fill.x as f32, fill.y as f32, fill.z as f32, fill.w as f32);
            self.draw_vector.rect(r.x, r.y, r.w, r.h);
            self.draw_vector.fill();
        }
        self.draw_vector.end(cx);
        for tile in &self.tiles {
            let r = tile.rect;
            if r.w > 60.0 && r.h > 22.0 {
                self.draw_text.text_style.font_size = 10.0;
                self.draw_text.color = vec4(1.0, 1.0, 1.0, 0.95);
                self.draw_text.draw_abs(cx, dvec2(r.x as f64 + 4.0, r.y as f64 + 2.0), &tile.name);
                self.draw_text.text_style.font_size = 9.0;
                self.draw_text.color = vec4(1.0, 1.0, 1.0, 0.85);
                self.draw_text.draw_abs(cx, dvec2(r.x as f64 + 4.0, r.y as f64 + 14.0), &size_short(tile.bytes));
            }
        }
        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

pub fn size_short(b: u64) -> String {
    disktree_core::size::human_bytes_short(b)
}

#[allow(dead_code)]
pub fn category_color(c: Category) -> Vec4f {
    color_for(c)
}
