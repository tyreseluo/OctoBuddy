//! Settings › Appearance: follow OctoSense's light or dark style, or one of
//! OctoBuddy's own themes (`theme.rs`). A theme taken is OctoBuddy's script
//! registered again with its tokens and the view applied anew, as the shell
//! does on a restyle.
use crate::theme::{self, Choice, Palette, THEMES};
use crate::{i18n, tapped, OctoBuddyView};
use makepad_widgets::*;

const ROWS: [LiveId; 11] = [
    live_id!(ap0), live_id!(ap1), live_id!(ap2), live_id!(ap3), live_id!(ap4), live_id!(ap5),
    live_id!(ap6), live_id!(ap7), live_id!(ap8), live_id!(ap9), live_id!(ap10),
];

/// The choices, in the rows' order: following OctoSense first.
fn choices() -> Vec<Choice> {
    std::iter::once(Choice::Follow).chain(THEMES.iter().map(|b| Choice::Theme(b))).collect()
}

impl OctoBuddyView {
    pub(crate) fn sync_appearance(&mut self, cx: &mut Cx) {
        self.view.label(cx, ids!(appearance_title)).set_text(cx, i18n::t("Appearance", "外观"));
        self.view.label(cx, ids!(appearance_hint)).set_text(cx, i18n::t(
            "Follow OctoSense: OctoBuddy is light or dark as OctoSense is. Or pick a theme of its own.",
            "跟随 OctoSense：OctoSense 是浅色或深色，OctoBuddy 就跟着是浅色或深色。也可以选一套 OctoBuddy 自己的主题。"));
        let now = theme::choice();
        let in_use = theme::current();
        for (id, choice) in ROWS.iter().zip(choices()) {
            let row = self.view.view(cx, &[*id]);
            let (name, sub, palette) = match choice {
                Choice::Follow => (
                    i18n::t("Follow OctoSense", "跟随 OctoSense").to_string(),
                    i18n::t("Light or dark, as OctoSense is", "浅色或深色，跟着 OctoSense").to_string(),
                    if now == Choice::Follow { in_use } else { theme::palette_for(Choice::Follow, false) },
                ),
                Choice::Theme(b) => (b.name.to_string(), if b.dark { i18n::t("Dark", "深色") } else { i18n::t("Light", "浅色") }.to_string(), Palette::of(b)),
            };
            row.label(cx, ids!(name)).set_text(cx, &name);
            row.label(cx, ids!(sub)).set_text(cx, &sub);
            row.label(cx, ids!(mark)).set_text(cx, if choice == now { "✓" } else { "" });
            // The swatch in the theme's own colours (not the one in use).
            let c = crate::hex_color;
            for (part, color) in [(ids!(swatch), palette.bg), (ids!(sw_ink), palette.ink), (ids!(sw_accent), palette.accent), (ids!(sw_muted), palette.muted)] {
                let mut w = row.view(cx, part);
                let color = c(color);
                script_apply_eval!(cx, w, { draw_bg +: {color: #(color)} });
            }
            let mut sw = row.view(cx, ids!(swatch));
            let edge = c(palette.line);
            script_apply_eval!(cx, sw, { draw_bg +: {border_color: #(edge)} });
            // The one in use: an accent edge.
            let (edge, size) = if choice == now { (c(in_use.accent), 1.5) } else { (c(in_use.line), 1.0) };
            let mut w = row.clone();
            script_apply_eval!(cx, w, { draw_bg +: {border_color: #(edge) border_size: #(size)} });
        }
    }

    pub(crate) fn appearance_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        for (id, choice) in ROWS.iter().zip(choices()) {
            if tapped(&self.view.view(cx, &[*id]), actions) && choice != theme::choice() {
                theme::pick(choice);
                self.retheme(cx);
                return;
            }
        }
    }

    /// OctoBuddy's look anew: its script registered again (its tokens taken
    /// from the theme now) and this view applied from it.
    pub(crate) fn retheme(&mut self, cx: &mut Cx) {
        cx.with_vm(|vm| {
            vm.with_reload(crate::script_mod);
            let source = vm.bx.heap.type_default_for_id(std::any::TypeId::of::<OctoBuddyView>());
            if let Some(source) = source {
                self.script_apply(vm, &Apply::ScriptReapply, &mut Scope::empty(), source.into());
            }
        });
        self.relayout(cx);
    }
}
