//! The layout's resize handles: a thin strip between two panes that the
//! person drags to make one wider (and double-clicks to set back). The strip
//! draws the divider line itself (accent while hovered or dragged); the view
//! owns the widths (`OctoBuddyView::resize_actions`), so any pane added later
//! gets a handle and a row in `PANES`, nothing more.
use makepad_widgets::*;

#[derive(Clone, Debug, Default)]
pub enum ResizeAction {
    #[default]
    None,
    /// A drag began.
    Start,
    /// The pointer moved this far (points) since the drag began.
    Drag(f64),
    /// The drag ended.
    End,
    /// A double click: back to its pane's default width.
    Reset,
}

#[derive(Script, ScriptHook, Widget)]
pub struct ResizeHandle {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    /// The whole strip, clear: where it takes the pointer.
    #[redraw]
    #[live]
    draw_bg: DrawColor,
    #[live]
    draw_line: DrawColor,
    #[live]
    line_color: Vec4f,
    #[live]
    active_color: Vec4f,
    #[rust]
    start: Option<f64>,
    #[rust]
    hover: bool,
    #[rust]
    area: Area,
}

impl Widget for ResizeHandle {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        let uid = self.widget_uid();
        match event.hits(cx, self.area) {
            Hit::FingerHoverIn(_) => {
                cx.set_cursor(MouseCursor::ColResize);
                self.hover = true;
                self.redraw(cx);
            }
            Hit::FingerHoverOver(_) => cx.set_cursor(MouseCursor::ColResize),
            Hit::FingerHoverOut(_) => {
                self.hover = false;
                self.redraw(cx);
            }
            Hit::FingerDown(fe) if fe.is_primary_hit() => {
                if fe.tap_count >= 2 {
                    self.start = None;
                    cx.widget_action(uid, ResizeAction::Reset);
                } else {
                    self.start = Some(fe.abs.x);
                    cx.widget_action(uid, ResizeAction::Start);
                }
                self.redraw(cx);
            }
            Hit::FingerMove(fe) => {
                if let Some(x) = self.start {
                    cx.set_cursor(MouseCursor::ColResize);
                    cx.widget_action(uid, ResizeAction::Drag(fe.abs.x - x));
                }
            }
            Hit::FingerUp(_) if self.start.take().is_some() => {
                cx.widget_action(uid, ResizeAction::End);
                self.redraw(cx);
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        let rect = cx.walk_turtle(walk);
        self.draw_bg.draw_abs(cx, rect);
        self.area = self.draw_bg.area();
        // The divider, in the strip's middle: wider and in the accent colour
        // while it is taken or about to be.
        let active = self.hover || self.start.is_some();
        let w = if active { 2.0 } else { 1.0 };
        self.draw_line.color = if active { self.active_color } else { self.line_color };
        self.draw_line.draw_abs(cx, Rect { pos: dvec2(rect.pos.x + (rect.size.x - w) * 0.5, rect.pos.y), size: dvec2(w, rect.size.y) });
        DrawStep::done()
    }
}

impl ResizeHandleRef {
    /// What the person did with this handle this frame.
    pub fn actions(&self, actions: &Actions) -> Vec<ResizeAction> {
        let uid = self.widget_uid();
        actions.iter().filter_map(|a| a.as_widget_action()).filter(|a| a.widget_uid == uid)
            .map(|a| a.cast::<ResizeAction>()).filter(|a| !matches!(a, ResizeAction::None)).collect()
    }
}
