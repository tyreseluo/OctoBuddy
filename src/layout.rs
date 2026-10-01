//! The window's panes and how the person sizes them: each resizable pane is
//! a row of `PANES` (its view, its handle, which way dragging widens it, its
//! limits and default), its width kept in the store. The sidebar also folds
//! away. A pane added later needs a `ResizeHandle` beside it and a row here.
use crate::resize::{ResizeAction, ResizeHandleWidgetRefExt};
use crate::{model, tapped, OctoBuddyView};
use makepad_widgets::*;

pub(crate) struct Pane {
    pub key: &'static str,
    pub pane: LiveId,
    pub handle: LiveId,
    /// Its handle is on its right (dragging right widens it), else its left.
    pub grows_right: bool,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

/// The least the conversation keeps beside the panes.
const MIN_CHAT: f64 = 360.0;

pub(crate) const PANES: [Pane; 3] = [
    Pane { key: "sidebar", pane: live_id!(sidebar), handle: live_id!(sidebar_handle), grows_right: true, min: 180.0, max: 460.0, default: 230.0 },
    Pane { key: "preview", pane: live_id!(preview_panel), handle: live_id!(preview_handle), grows_right: false, min: 280.0, max: 1000.0, default: 400.0 },
    Pane { key: "inner", pane: live_id!(inner_panel), handle: live_id!(inner_handle), grows_right: false, min: 300.0, max: 900.0, default: 360.0 },
];

/// A width dragged by `dx` from `start`, inside the pane's limits.
pub(crate) fn dragged(pane: &Pane, start: f64, dx: f64) -> f64 {
    (start + if pane.grows_right { dx } else { -dx }).clamp(pane.min, pane.max)
}

impl OctoBuddyView {
    pub(crate) fn pane_width(&self, key: &str) -> f64 {
        let default = PANES.iter().find(|p| p.key == key).map(|p| p.default).unwrap_or(300.0);
        self.store.panes.iter().flatten().find(|p| p.id == key).map(|p| p.width).unwrap_or(default)
    }

    fn set_pane_width(&mut self, key: &str, width: f64) {
        let panes = self.store.panes.get_or_insert_with(Vec::new);
        match panes.iter_mut().find(|p| p.id == key) {
            Some(p) => p.width = width,
            None => panes.push(model::PaneWidth { id: key.to_string(), width }),
        }
    }

    fn apply_pane(&mut self, cx: &mut Cx, pane: &Pane) {
        // Never so wide that the conversation is squeezed out: it keeps
        // `MIN_CHAT`, beside the other panes shown (the width kept is not
        // changed: a wider window gives it back).
        let window = self.view.area().rect(cx).size.x;
        let others: f64 = PANES.iter().filter(|p| p.key != pane.key && self.view.view(cx, &[p.pane]).visible()).map(|p| self.pane_width(p.key)).sum();
        let room = if window > 0.0 && pane.key != "sidebar" { (window - others - MIN_CHAT).max(pane.min) } else { pane.max };
        let width = self.pane_width(pane.key).min(room);
        let mut w = self.view.widget(cx, &[pane.pane]);
        script_apply_eval!(cx, w, { width: #(width) });
    }

    /// The widths as kept, and the sidebar open or folded away.
    pub(crate) fn apply_panes(&mut self, cx: &mut Cx) {
        for pane in &PANES {
            self.apply_pane(cx, pane);
        }
        let closed = self.store.sidebar_closed == Some(true);
        self.view.view(cx, ids!(sidebar)).set_visible(cx, !closed);
        self.view.view(cx, ids!(sidebar_divider)).set_visible(cx, !closed);
        self.view.view(cx, ids!(sidebar_open)).set_visible(cx, closed);
    }

    /// A handle dragged or double-clicked; the sidebar folded or opened.
    pub(crate) fn resize_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        for pane in &PANES {
            for action in self.view.widget(cx, &[pane.handle]).as_resize_handle().actions(actions) {
                match action {
                    ResizeAction::Start => {
                        let shown = self.view.view(cx, &[pane.pane]).area().rect(cx).size.x;
                        let start = if shown > 0.0 { shown } else { self.pane_width(pane.key) };
                        self.resizing = Some((pane.key, start));
                    }
                    ResizeAction::Drag(dx) => {
                        let Some((key, start)) = self.resizing.filter(|(k, _)| *k == pane.key) else { continue };
                        self.set_pane_width(key, dragged(pane, start, dx));
                        self.apply_pane(cx, pane);
                        // The lists in it too (a list is not redrawn with its parent).
                        cx.redraw_all();
                    }
                    ResizeAction::End => {
                        self.resizing = None;
                        self.save();
                        self.relayout(cx);
                    }
                    ResizeAction::Reset => {
                        self.set_pane_width(pane.key, pane.default);
                        self.apply_pane(cx, pane);
                        self.save();
                        self.relayout(cx);
                    }
                    ResizeAction::None => {}
                }
            }
        }
        let close = tapped(&self.view.view(cx, ids!(sidebar_close)), actions);
        let open = tapped(&self.view.view(cx, ids!(sidebar_open)), actions);
        if close || open {
            self.store.sidebar_closed = close.then_some(true);
            self.apply_panes(cx);
            self.save();
            self.relayout(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pane_widens_the_way_its_handle_is_dragged() {
        let (sidebar, inner) = (&PANES[0], &PANES[2]);
        assert_eq!(dragged(sidebar, 230.0, 40.0), 270.0, "the sidebar's handle is on its right");
        assert_eq!(dragged(inner, 360.0, -40.0), 400.0, "the inner panel's is on its left");
        assert_eq!(dragged(sidebar, 230.0, -500.0), sidebar.min, "never narrower than its minimum");
        assert_eq!(dragged(inner, 360.0, -5000.0), inner.max);
    }
}
