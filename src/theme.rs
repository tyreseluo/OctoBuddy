//! OctoBuddy's look: a palette of named colours (tokens) that its views
//! take when their script is registered, so that changing the theme is
//! registering it again and applying it anew (as the OctoSense shell does on
//! a restyle).
//!
//! The person picks one in Settings › Appearance (kept in
//! `<data>/appearance.json`): "follow OctoSense" (the default: OctoBuddy's
//! light or dark one, as the shell's style is), or a theme of its own
//! (GitHub, Atom One, Dracula, Nord, Solarized). Each theme names eleven
//! colours; the rest (hover, tints, selections, the status backgrounds) are
//! mixed from them, so every theme is whole.
use makepad_widgets::*;
use std::sync::Mutex;

/// A theme the person may pick, by its stored id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    /// OctoBuddy's light or dark theme, as OctoSense's style is.
    Follow,
    Theme(&'static Base),
}

/// What a theme says; the other tokens are mixed from it.
#[derive(Debug, PartialEq, Eq)]
pub struct Base {
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    /// The page, the sidebar, panels (inner panel, cards' subtle fill).
    pub bg: u32,
    pub sidebar: u32,
    pub panel: u32,
    /// Text, its quieter shade, and lines.
    pub ink: u32,
    pub muted: u32,
    pub line: u32,
    pub accent: u32,
    pub success: u32,
    pub danger: u32,
    pub warning: u32,
    /// Inner loops, octos, subagents.
    pub purple: u32,
}

/// OctoBuddy's own light theme: GitHub's Primer, as it has always looked.
pub const LIGHT: Base = Base {
    id: "light", name: "Light", dark: false,
    bg: 0xffffff, sidebar: 0xf3f4f6, panel: 0xf6f8fa,
    ink: 0x1f2328, muted: 0x6e7781, line: 0xd8dee4,
    accent: 0x2f6feb, success: 0x1a7f37, danger: 0xcf222e, warning: 0x9a6700, purple: 0x8250df,
};
/// Its dark one: the same, in a quiet dark.
pub const DARK: Base = Base {
    id: "dark", name: "Dark", dark: true,
    bg: 0x1c1d21, sidebar: 0x16171a, panel: 0x222327,
    ink: 0xe3e5e8, muted: 0x9097a0, line: 0x34363c,
    accent: 0x4c8dff, success: 0x3fb950, danger: 0xf26d6d, warning: 0xd8a33a, purple: 0xa98bf5,
};
pub const GITHUB_LIGHT: Base = Base {
    id: "github-light", name: "GitHub Light", dark: false,
    bg: 0xffffff, sidebar: 0xf6f8fa, panel: 0xf6f8fa,
    ink: 0x1f2328, muted: 0x59636e, line: 0xd1d9e0,
    accent: 0x0969da, success: 0x1a7f37, danger: 0xd1242f, warning: 0x9a6700, purple: 0x8250df,
};
pub const GITHUB_DARK: Base = Base {
    id: "github-dark", name: "GitHub Dark", dark: true,
    bg: 0x0d1117, sidebar: 0x010409, panel: 0x151b23,
    ink: 0xf0f6fc, muted: 0x9198a1, line: 0x3d444d,
    accent: 0x4493f8, success: 0x3fb950, danger: 0xf85149, warning: 0xd29922, purple: 0xab7df8,
};
pub const ATOM_ONE_LIGHT: Base = Base {
    id: "atom-one-light", name: "Atom One Light", dark: false,
    bg: 0xfafafa, sidebar: 0xeaeaeb, panel: 0xf0f0f1,
    ink: 0x383a42, muted: 0x80828a, line: 0xdbdbdc,
    accent: 0x4078f2, success: 0x50a14f, danger: 0xe45649, warning: 0xc18401, purple: 0xa626a4,
};
pub const ATOM_ONE_DARK: Base = Base {
    id: "atom-one-dark", name: "Atom One Dark", dark: true,
    bg: 0x282c34, sidebar: 0x21252b, panel: 0x2c313a,
    ink: 0xabb2bf, muted: 0x7f848e, line: 0x3e4451,
    accent: 0x61afef, success: 0x98c379, danger: 0xe06c75, warning: 0xe5c07b, purple: 0xc678dd,
};
pub const DRACULA: Base = Base {
    id: "dracula", name: "Dracula", dark: true,
    bg: 0x282a36, sidebar: 0x21222c, panel: 0x2f3240,
    ink: 0xf8f8f2, muted: 0x8a93c0, line: 0x44475a,
    accent: 0xbd93f9, success: 0x50fa7b, danger: 0xff5555, warning: 0xf1fa8c, purple: 0xff79c6,
};
pub const NORD: Base = Base {
    id: "nord", name: "Nord", dark: true,
    bg: 0x2e3440, sidebar: 0x272c36, panel: 0x343b48,
    ink: 0xeceff4, muted: 0x9aa5b8, line: 0x434c5e,
    accent: 0x88c0d0, success: 0xa3be8c, danger: 0xbf616a, warning: 0xebcb8b, purple: 0xb48ead,
};
pub const SOLARIZED_LIGHT: Base = Base {
    id: "solarized-light", name: "Solarized Light", dark: false,
    bg: 0xfdf6e3, sidebar: 0xeee8d5, panel: 0xf6efdb,
    ink: 0x3b4f57, muted: 0x839496, line: 0xe2dbc6,
    accent: 0x268bd2, success: 0x859900, danger: 0xdc322f, warning: 0xb58900, purple: 0x6c71c4,
};
pub const SOLARIZED_DARK: Base = Base {
    id: "solarized-dark", name: "Solarized Dark", dark: true,
    bg: 0x002b36, sidebar: 0x00222b, panel: 0x073642,
    ink: 0xc6d0d0, muted: 0x839496, line: 0x0f4b59,
    accent: 0x268bd2, success: 0x859900, danger: 0xdc322f, warning: 0xb58900, purple: 0x8d91d6,
};

/// The themes, in the order Settings offers them (after "follow OctoSense").
pub const THEMES: [&Base; 10] = [&LIGHT, &DARK, &GITHUB_LIGHT, &GITHUB_DARK, &ATOM_ONE_LIGHT, &ATOM_ONE_DARK, &DRACULA, &NORD, &SOLARIZED_LIGHT, &SOLARIZED_DARK];

impl Choice {
    pub fn id(self) -> &'static str {
        match self {
            Choice::Follow => "follow",
            Choice::Theme(b) => b.id,
        }
    }

    pub fn from_id(id: &str) -> Choice {
        THEMES.iter().find(|b| b.id == id).map(|b| Choice::Theme(b)).unwrap_or(Choice::Follow)
    }
}

/// Every token, mixed from a theme. Colours are 0xRRGGBB; `glass` and
/// `overlay` carry their own alpha (0xRRGGBBAA, see `rgba`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub bg: u32,
    pub raised: u32,
    pub sidebar: u32,
    pub panel: u32,
    pub hover: u32,
    pub ink: u32,
    pub ink2: u32,
    pub muted: u32,
    pub muted_strong: u32,
    pub faint: u32,
    pub line: u32,
    pub line_strong: u32,
    pub accent: u32,
    pub accent_hover: u32,
    pub accent_down: u32,
    pub accent_ink: u32,
    pub accent_soft: u32,
    pub accent_selected: u32,
    pub accent_line: u32,
    pub on_accent: u32,
    pub inverse_bg: u32,
    pub inverse_fg: u32,
    pub success: u32,
    pub success_strong: u32,
    pub success_bg: u32,
    pub success_line: u32,
    pub danger: u32,
    pub danger_strong: u32,
    pub danger_bg: u32,
    pub danger_bg_hover: u32,
    pub danger_bg_down: u32,
    pub warning: u32,
    pub warning_bg: u32,
    pub orange: u32,
    pub orange_bg: u32,
    pub purple: u32,
    pub purple_bg: u32,
    pub purple_bg_strong: u32,
    pub purple_line: u32,
}

/// `a` mixed toward `b` by `t` (0: `a`, 1: `b`).
pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xff) as f32;
    let one = |s: u32| (ch(a, s) + (ch(b, s) - ch(a, s)) * t).round().clamp(0.0, 255.0) as u32;
    (one(16) << 16) | (one(8) << 8) | one(0)
}

/// How light a colour reads (0 black, 1 white).
fn luma(c: u32) -> f32 {
    let ch = |s: u32| ((c >> s) & 0xff) as f32 / 255.0;
    0.2126 * ch(16) + 0.7152 * ch(8) + 0.0722 * ch(0)
}

impl Palette {
    pub fn of(b: &Base) -> Palette {
        let (white, black) = (0xffffff, 0x000000);
        // Toward the text (a light theme's hover darkens, a dark one's lightens).
        let toward_ink = |t: f32| mix(b.bg, b.ink, t);
        let tint = |c: u32, t: f32| mix(b.bg, c, t);
        let deep = |c: u32| if b.dark { mix(c, white, 0.25) } else { mix(c, black, 0.25) };
        // Text on the accent: white unless the accent itself is light.
        let on_accent = if luma(b.accent) > 0.62 { mix(b.bg, black, 0.85) } else { white };
        let orange = mix(b.warning, b.danger, 0.45);
        Palette {
            dark: b.dark,
            bg: b.bg,
            raised: if b.dark { mix(b.bg, b.ink, 0.04) } else { b.bg },
            sidebar: b.sidebar,
            panel: b.panel,
            hover: toward_ink(if b.dark { 0.10 } else { 0.08 }),
            ink: b.ink,
            ink2: mix(b.ink, b.muted, 0.3),
            muted: b.muted,
            muted_strong: mix(b.muted, b.ink, 0.25),
            faint: mix(b.muted, b.bg, 0.3),
            line: b.line,
            line_strong: mix(b.line, b.muted, 0.35),
            accent: b.accent,
            accent_hover: if b.dark { mix(b.accent, white, 0.1) } else { mix(b.accent, black, 0.1) },
            accent_down: if b.dark { mix(b.accent, white, 0.2) } else { mix(b.accent, black, 0.22) },
            accent_ink: deep(b.accent),
            accent_soft: tint(b.accent, if b.dark { 0.16 } else { 0.08 }),
            accent_selected: tint(b.accent, if b.dark { 0.26 } else { 0.16 }),
            accent_line: tint(b.accent, 0.42),
            on_accent,
            inverse_bg: b.ink,
            inverse_fg: b.bg,
            success: b.success,
            success_strong: deep(b.success),
            success_bg: tint(b.success, if b.dark { 0.18 } else { 0.1 }),
            success_line: tint(b.success, 0.4),
            danger: b.danger,
            danger_strong: deep(b.danger),
            danger_bg: tint(b.danger, if b.dark { 0.18 } else { 0.09 }),
            danger_bg_hover: tint(b.danger, if b.dark { 0.26 } else { 0.16 }),
            danger_bg_down: tint(b.danger, if b.dark { 0.34 } else { 0.24 }),
            warning: b.warning,
            warning_bg: tint(b.warning, if b.dark { 0.18 } else { 0.14 }),
            orange,
            orange_bg: tint(orange, if b.dark { 0.18 } else { 0.1 }),
            purple: b.purple,
            purple_bg: tint(b.purple, if b.dark { 0.14 } else { 0.05 }),
            purple_bg_strong: tint(b.purple, if b.dark { 0.24 } else { 0.16 }),
            purple_line: tint(b.purple, 0.45),
        }
    }

    /// A token by its DSL name (`theme::c`).
    pub fn get(&self, key: &str) -> Option<u32> {
        Some(match key {
            "bg" => self.bg, "raised" => self.raised, "sidebar" => self.sidebar, "panel" => self.panel, "hover" => self.hover,
            "ink" => self.ink, "ink2" => self.ink2, "muted" => self.muted, "muted_strong" => self.muted_strong, "faint" => self.faint,
            "line" => self.line, "line_strong" => self.line_strong,
            "accent" => self.accent, "accent_hover" => self.accent_hover, "accent_down" => self.accent_down, "accent_ink" => self.accent_ink,
            "accent_soft" => self.accent_soft, "accent_selected" => self.accent_selected, "accent_line" => self.accent_line,
            "on_accent" => self.on_accent, "inverse_bg" => self.inverse_bg, "inverse_fg" => self.inverse_fg,
            "success" => self.success, "success_strong" => self.success_strong, "success_bg" => self.success_bg, "success_line" => self.success_line,
            "danger" => self.danger, "danger_strong" => self.danger_strong, "danger_bg" => self.danger_bg,
            "danger_bg_hover" => self.danger_bg_hover, "danger_bg_down" => self.danger_bg_down,
            "warning" => self.warning, "warning_bg" => self.warning_bg, "orange" => self.orange, "orange_bg" => self.orange_bg,
            "purple" => self.purple, "purple_bg" => self.purple_bg, "purple_bg_strong" => self.purple_bg_strong, "purple_line" => self.purple_line,
            _ => return None,
        })
    }
}

/// The theme picked, and the palette in use (resolved when the script is
/// registered: "follow" needs the shell's style, which only a VM knows).
struct State {
    choice: Choice,
    palette: Palette,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn appearance_file() -> std::path::PathBuf {
    crate::model::data_dir().join("appearance.json")
}

/// The theme the person picked, as kept.
pub fn stored() -> Choice {
    let text = std::fs::read_to_string(appearance_file()).unwrap_or_default();
    let id = serde_json::from_str::<serde_json::Value>(&text).ok()
        .and_then(|v| v["theme"].as_str().map(String::from)).unwrap_or_default();
    Choice::from_id(&id)
}

/// Keeps `choice` (it takes effect when the script is registered again).
pub fn pick(choice: Choice) {
    let _ = std::fs::write(appearance_file(), serde_json::json!({"theme": choice.id()}).to_string());
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = state.as_mut() {
        s.choice = choice;
    }
}

/// The theme picked now.
pub fn choice() -> Choice {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.choice).unwrap_or_else(stored)
}

/// Whether what OctoBuddy follows is dark: inside OctoSense its style
/// (`<style>-dark`); on the host the system's appearance.
fn shell_is_dark(vm: &mut ScriptVm) -> bool {
    if crate::system::hosted() {
        return makepad_widgets::desktop_style::current_name(vm).is_some_and(|name| name.ends_with("-dark"));
    }
    system_is_dark()
}

/// macOS in Dark Mode (`AppleInterfaceStyle` is `Dark`; unset when light).
fn system_is_dark() -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    std::process::Command::new("defaults").args(["read", "-g", "AppleInterfaceStyle"]).output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim().eq_ignore_ascii_case("dark"))
}

/// The palette for `choice`, the shell's style considered.
pub fn palette_for(choice: Choice, shell_dark: bool) -> Palette {
    match choice {
        Choice::Follow if shell_dark => Palette::of(&DARK),
        Choice::Follow => Palette::of(&LIGHT),
        Choice::Theme(b) => Palette::of(b),
    }
}

/// Resolves the palette for this registration (the script's first line).
pub fn resolve(vm: &mut ScriptVm) -> ScriptValue {
    let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let choice = state.as_ref().map(|s| s.choice).unwrap_or_else(stored);
    let palette = palette_for(choice, shell_is_dark(vm));
    *state = Some(State { choice, palette });
    ScriptValue::NIL
}

/// The palette in use (what Rust drawing reads).
pub fn current() -> Palette {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.palette).unwrap_or_else(|| Palette::of(&LIGHT))
}

/// A token as the script takes it (opaque).
pub fn c(key: &str) -> ScriptValue {
    let rgb = current().get(key).unwrap_or_else(|| panic!("no theme token {key}"));
    ScriptValue::from_color((rgb << 8) | 0xff)
}

/// A token with its own alpha (`glass` surfaces, the dialog backdrop).
pub fn ca(key: &str, alpha: u8) -> ScriptValue {
    let rgb = current().get(key).unwrap_or_else(|| panic!("no theme token {key}"));
    ScriptValue::from_color((rgb << 8) | alpha as u32)
}

/// What dims the window under a dialog: the text's colour, faint, on a
/// light theme; black, deeper, on a dark one.
pub fn scrim() -> ScriptValue {
    let p = current();
    ScriptValue::from_color(if p.dark { 0x0000008c } else { (p.ink << 8) | 0x33 })
}

/// A token as Rust drawing takes it.
pub fn vec4(key: &str) -> Vec4f {
    crate::hex_color(current().get(key).unwrap_or(0xff00ff))
}

/// A token as `set_color_hex` takes it.
pub fn hex(key: &str) -> u32 {
    current().get(key).unwrap_or(0xff00ff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_has_every_token_and_reads() {
        for b in THEMES {
            let p = Palette::of(b);
            assert_eq!(luma(p.bg) < 0.5, b.dark, "{}: its page is as dark as it says", b.id);
            // Text stands out from its page; the accent's text from the accent.
            assert!((luma(p.ink) - luma(p.bg)).abs() > 0.45, "{}: text on page", b.id);
            assert!((luma(p.on_accent) - luma(p.accent)).abs() > 0.3, "{}: text on accent", b.id);
            for key in ["bg", "hover", "accent_soft", "danger_bg", "purple_line", "inverse_fg"] {
                assert!(p.get(key).is_some(), "{}: {key}", b.id);
            }
        }
        assert_eq!(Choice::from_id("nord").id(), "nord");
        assert_eq!(Choice::from_id("nonsense"), Choice::Follow);
    }

    #[test]
    fn the_light_theme_is_how_it_always_looked() {
        let p = Palette::of(&LIGHT);
        assert_eq!((p.bg, p.ink, p.muted, p.line, p.accent, p.sidebar), (0xffffff, 0x1f2328, 0x6e7781, 0xd8dee4, 0x2f6feb, 0xf3f4f6));
        assert_eq!(mix(0x000000, 0xffffff, 0.5), 0x808080);
    }
}
