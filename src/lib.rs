//! OctoBuddy: the OctoBuddy two-loop workflow (an outer-loop lead driving
//! inner-loop octos peers) as an OctoSense app, in a standalone window or an
//! embedded app instance.
//!
//! Projects are listed on the left, each with its sessions; a session is one
//! campaign. Every message the person sends goes to the session's lead
//! (Claude). When the lead ends a reply with a plan, OctoBuddy starts one
//! inner-loop peer (octos) per slice, and when a round is over it hands the
//! results back to the lead for review.
pub use makepad_widgets;
use makepad_widgets::*;
use makepad_widgets::makepad_platform::file_dialogs::{FileDialog, FileDialogAction};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub mod agents;
/// The module OctoSense hosts: its pane in the shell (`OCTOBUDDY_MODULE`).
#[cfg(feature = "octosense-module")]
mod module;
#[cfg(feature = "octosense-module")]
pub use module::{OctoBuddyModule, OCTOBUDDY_MODULE};
pub use plugins::native_tui::own_terminal_settings;
mod chat;
mod context;
pub mod flow;
pub mod i18n;
pub mod contract;
pub mod dispatch;
pub mod events;
pub mod inner;
pub mod lead;
pub mod memory;
mod mcp;
pub mod model;
mod octos_outer;
mod orchestrate;
pub mod plan;
mod persist;
pub mod providers;
pub mod own_providers;
pub mod live;
pub mod lessons;
mod providers_view;
mod review;
pub mod stream;
pub mod system;
pub mod timeline;
mod time_view;
pub mod estimate;
mod plan_panel;
mod claude_proxy;
mod claude_inner;
mod card_tabs;
mod picker;
mod provider_icons;
use plugins::workbench::Workbench;
mod appearance;
pub mod plugins;
mod tools_info;
mod reveal;
mod resize;
pub mod theme;
mod layout;
mod pack;
mod responses_bridge;
mod rpc_lead;
use card_tabs::{CardTab, SpecRow};
mod sidebar;
mod system_chat;
pub mod verify;
pub mod workspace;
use flow::FlowCanvasWidgetRefExt;
use timeline::Timeline;
use model::{now_secs, Peer, SessionRef, Store};
use orchestrate::Runtime;
use plugins::card_loop::Loops as CardLoops;
use plugins::app_factory::Factory as AppFactory;
use providers::Providers;

/// The outer-loop CLIs the picker offers, each with its selected and
/// unselected button. Only `claude` runs yet.
/// Claude Code's models the picker offers (`None`: its own setting).
const CLAUDE_MODELS: [Option<&str>; 4] = [None, Some("opus"), Some("sonnet"), Some("haiku")];

/// The rows of a queue panel (`q0`…`q5`).
const QUEUE_ROWS: [LiveId; 6] = [live_id!(q0), live_id!(q1), live_id!(q2), live_id!(q3), live_id!(q4), live_id!(q5)];

/// Plan-and-review rounds one request may start before OctoBuddy stops and asks.
const MAX_ROUNDS: u32 = 3;
/// Reviews added when a request's budget is used up while its checks still
/// pass, and how many times per request (the person's next message resets it).
const EXTENSION: u32 = 2;
const MAX_EXTENSIONS: u32 = 3;

/// The workspace (projects and sessions), or the settings page instead of it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Page {
    #[default]
    Chat,
    Settings,
    /// The production loop's page: the published apps, watched.
    Live,
}

/// The settings page's sections.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum SettingsTab {
    #[default]
    Providers,
    Plugins,
    Tools,
    Language,
    Appearance,
    About,
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // The theme (theme.rs): its tokens, taken when this script is registered.
    let th_resolved = #(crate::theme::resolve(vm))
    let th_accent = #(crate::theme::c("accent"))
    let th_accent_down = #(crate::theme::c("accent_down"))
    let th_accent_hover = #(crate::theme::c("accent_hover"))
    let th_accent_ink = #(crate::theme::c("accent_ink"))
    let th_accent_line = #(crate::theme::c("accent_line"))
    let th_accent_selected = #(crate::theme::c("accent_selected"))
    let th_accent_soft = #(crate::theme::c("accent_soft"))
    let th_bg = #(crate::theme::c("bg"))
    let th_danger = #(crate::theme::c("danger"))
    let th_danger_bg = #(crate::theme::c("danger_bg"))
    let th_danger_bg_down = #(crate::theme::c("danger_bg_down"))
    let th_danger_bg_hover = #(crate::theme::c("danger_bg_hover"))
    let th_danger_strong = #(crate::theme::c("danger_strong"))
    let th_faint = #(crate::theme::c("faint"))
    let th_hover = #(crate::theme::c("hover"))
    let th_ink = #(crate::theme::c("ink"))
    let th_ink2 = #(crate::theme::c("ink2"))
    let th_inverse_bg = #(crate::theme::c("inverse_bg"))
    let th_inverse_fg = #(crate::theme::c("inverse_fg"))
    let th_line = #(crate::theme::c("line"))
    let th_line_strong = #(crate::theme::c("line_strong"))
    let th_muted = #(crate::theme::c("muted"))
    let th_muted_strong = #(crate::theme::c("muted_strong"))
    let th_on_accent = #(crate::theme::c("on_accent"))
    let th_panel = #(crate::theme::c("panel"))
    let th_purple = #(crate::theme::c("purple"))
    let th_purple_bg = #(crate::theme::c("purple_bg"))
    let th_purple_bg_strong = #(crate::theme::c("purple_bg_strong"))
    let th_purple_line = #(crate::theme::c("purple_line"))
    let th_raised = #(crate::theme::c("raised"))
    let th_sidebar = #(crate::theme::c("sidebar"))
    let th_success = #(crate::theme::c("success"))
    let th_success_bg = #(crate::theme::c("success_bg"))
    let th_success_line = #(crate::theme::c("success_line"))
    let th_success_strong = #(crate::theme::c("success_strong"))
    let th_warning = #(crate::theme::c("warning"))
    let th_warning_bg = #(crate::theme::c("warning_bg"))
    let th_scrim = #(crate::theme::scrim())
    let th_glass = #(crate::theme::ca("bg", 0xfa))
    let th_glass_raised = #(crate::theme::ca("raised", 0xee))
    let th_glass_panel = #(crate::theme::ca("panel", 0xe6))
    let th_accent_glow = #(crate::theme::ca("accent", 0x33))
    let th_select_on_accent = #(crate::theme::ca("on_accent", 0x55))
    // The scroll bars in OctoBuddy's colours (Makepad's take the stock
    // theme's when it defines them): every ScrollBar in this VM.
    mod.widgets.ScrollBar = set_type_default() do mod.widgets.ScrollBar{
        draw_bg +: {color: th_line_strong color_hover: th_faint color_drag: th_muted}
    }
    let ink = th_ink
    let muted = th_muted
    let line = th_line
    let accent = th_accent
    let sidebar_bg = th_sidebar
    let pane_bg = th_bg

    let ChevronRight = Vector{width: 12 height: 12 viewbox: vec4(0 0 24 24)
        Path{d: "M9 6l6 6-6 6" fill: false stroke: th_muted stroke_width: 2.2 stroke_linecap: "round" stroke_linejoin: "round"}
    }
    let ChevronDown = Vector{width: 12 height: 12 viewbox: vec4(0 0 24 24)
        Path{d: "M6 9l6 6 6-6" fill: false stroke: th_muted stroke_width: 2.2 stroke_linecap: "round" stroke_linejoin: "round"}
    }

    let SmallButton = ButtonFlatter{
        width: 24 height: 24
        draw_bg +: {color_hover: th_hover border_color_hover: th_hover}
        draw_text +: {color: th_muted color_hover: th_ink color_down: th_ink color_focus: th_muted text_style +: {font_size: 13}}
    }

    let InputStyle = TextInput{
        draw_bg +: {
            color: th_raised color_hover: th_raised color_focus: th_raised color_empty: th_raised
            border_color: th_line border_color_hover: th_line_strong border_color_focus: th_accent border_color_empty: th_line
            border_radius: 4.0
        }
        padding: Inset{left: 10 right: 10 top: 8 bottom: 8}
        draw_text +: {
            color: th_ink color_hover: th_ink color_focus: th_ink color_down: th_ink
            color_empty: th_faint color_empty_hover: th_faint color_empty_focus: th_faint
        }
        draw_cursor +: {color: th_ink}
    }

    let ProjectRow = View{
        width: Fill height: Fit
        row := View{
            width: Fill height: Fit
            padding: Inset{left: 10 right: 6 top: 6 bottom: 6}
            flow: Right spacing: 6 align: Align{y: 0.5}
            cursor: MouseCursor.Hand grab_key_focus: false
            open := View{width: Fit height: Fit ChevronDown{}}
            closed := View{width: Fit height: Fit ChevronRight{}}
            name := Label{width: Fill text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
            pin := Label{text: "" padding: 0 draw_text.color: th_faint draw_text.text_style.font_size: 8}
            add_session := SmallButton{text: "+"}
        }
    }
    // Archived, under the list's "Archived": greyed.
    let ProjectRowArchived = ProjectRow{row +: {name +: {draw_text.color: th_faint}}}

    let SessionRow = View{
        width: Fill height: Fit
        padding: Inset{left: 6 right: 6}
        row := RoundedView{
            width: Fill height: Fit new_batch: true
            padding: Inset{left: 26 right: 8 top: 6 bottom: 6}
            cursor: MouseCursor.Hand grab_key_focus: false
            draw_bg.color: #x00000000 draw_bg.border_radius: 6.0
            flow: Right spacing: 6 align: Align{y: 0.5}
            title := Label{width: Fill text: "" max_lines: 1 draw_text.color: ink draw_text.text_style.font_size: 10.5}
            pin := Label{text: "" padding: 0 draw_text.color: th_faint draw_text.text_style.font_size: 8}
        }
    }

    let SessionRowActive = SessionRow{
        row +: {
            draw_bg.color: th_accent_selected
            title +: {draw_text.color: th_accent_ink}
        }
    }
    let SessionRowArchived = SessionRow{row +: {title +: {draw_text.color: th_faint}}}

    let SidebarEmpty = View{
        width: Fill height: Fit padding: 16
        empty_text := Label{width: Fill text: "No projects yet. Add a project directory above." draw_text.color: muted draw_text.text_style.font_size: 10}
    }

    // Two sheets: copy.
    let CopyIcon = Vector{width: 13 height: 13 viewbox: vec4(0 0 24 24)
        Path{d: "M9 9h11v11H9z" fill: false stroke: th_faint stroke_width: 2 stroke_linejoin: "round"}
        Path{d: "M5 15H4V4h11v1" fill: false stroke: th_faint stroke_width: 2 stroke_linecap: "round" stroke_linejoin: "round"}
    }
    // Under an agent's reply: copy it, and how long the turn took.
    let MsgFooter = View{
        width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
        copy := View{
            width: Fit height: Fit padding: 3
            cursor: MouseCursor.Hand grab_key_focus: false
            CopyIcon{}
        }
        copied := Label{text: "" padding: 0 draw_text.color: th_success draw_text.text_style.font_size: 8.5}
        took := Label{text: "" padding: 0 draw_text.color: th_faint draw_text.text_style.font_size: 8.5}
    }

    // What the agents write is Markdown.
    let MsgMarkdown = Markdown{
        width: Fill height: Fit
        padding: 0
        // Drag to select part of it; Cmd+C copies the selection, Cmd+A all.
        // (The theme's selection colour is near white: invisible on white.)
        selectable: true
        draw_selection +: {color: th_accent_glow}
        font_size: 10.5
        font_color: th_ink
        paragraph_spacing: 8
        pre_code_spacing: 6
        heading_base_scale: 2.2
        draw_text +: {color: th_ink}
        text_style_normal: theme.font_regular{font_size: 10.5}
        text_style_italic: theme.font_italic{font_size: 10.5}
        text_style_bold: theme.font_bold{font_size: 10.5}
        text_style_bold_italic: theme.font_bold_italic{font_size: 10.5}
        text_style_fixed: theme.font_code{font_size: 9.5}
        draw_block +: {
            line_color: th_ink sep_color: th_line
            quote_bg_color: th_panel quote_fg_color: th_muted_strong
            code_color: th_sidebar
            table_header_bg_color: th_panel table_border_color: th_line
        }
    }
    // The person's words, white on the bubble, selectable like the rest.
    let UserMarkdown = MsgMarkdown{
        font_color: th_on_accent
        draw_text +: {color: th_on_accent}
        draw_selection +: {color: th_select_on_accent}
    }
    let CardMarkdown = MsgMarkdown{
        font_size: 9.5
        font_color: th_ink2
        draw_text +: {color: th_ink2}
        text_style_normal: theme.font_regular{font_size: 9.5}
        text_style_italic: theme.font_italic{font_size: 9.5}
        text_style_bold: theme.font_bold{font_size: 9.5}
        text_style_bold_italic: theme.font_bold_italic{font_size: 9.5}
        text_style_fixed: theme.font_code{font_size: 8.5}
    }

    // The person, on the right. A Fit label does not wrap, so a long
    // message takes the wide one (see `text_width`).
    let UserMsg = View{
        width: Fill height: Fit flow: Right
        padding: Inset{left: 48 right: 0 top: 8 bottom: 8}
        Filler{}
        bubble := RoundedView{
            width: Fit height: Fit new_batch: true
            padding: Inset{left: 12 right: 12 top: 8 bottom: 8}
            draw_bg.color: th_accent draw_bg.border_radius: 12.0
            body := UserMarkdown{width: Fit}
        }
    }
    let UserMsgWide = View{
        width: Fill height: Fit flow: Right
        padding: Inset{left: 48 right: 0 top: 8 bottom: 8}
        bubble := RoundedView{
            width: Fill height: Fit new_batch: true
            padding: Inset{left: 12 right: 12 top: 8 bottom: 8}
            draw_bg.color: th_accent draw_bg.border_radius: 12.0
            body := UserMarkdown{}
        }
    }
    // A message steered into a running turn: the same, with a receipt under
    // it (steered in, taken up). A template of its own: a list item's child
    // is not hidden by code. (Drawn after the bubble: a label drawn ahead of
    // a new batch in a list item left the bubble undrawn.)
    let SteerTag = Label{text: "" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
    let UserSteerMsg = View{
        width: Fill height: Fit flow: Down spacing: 3
        padding: Inset{left: 48 right: 0 top: 8 bottom: 8}
        View{
            width: Fill height: Fit flow: Right
            Filler{}
            bubble := RoundedView{
                width: Fit height: Fit new_batch: true
                padding: Inset{left: 12 right: 12 top: 8 bottom: 8}
                draw_bg.color: th_accent draw_bg.border_radius: 12.0
                body := UserMarkdown{width: Fit}
            }
        }
        View{width: Fill height: Fit flow: Right new_batch: true Filler{} tag := SteerTag{}}
    }
    let UserSteerMsgWide = View{
        width: Fill height: Fit flow: Down spacing: 3
        padding: Inset{left: 48 right: 0 top: 8 bottom: 8}
        bubble := RoundedView{
            width: Fill height: Fit new_batch: true
            padding: Inset{left: 12 right: 12 top: 8 bottom: 8}
            draw_bg.color: th_accent draw_bg.border_radius: 12.0
            body := UserMarkdown{}
        }
        View{width: Fill height: Fit flow: Right new_batch: true Filler{} tag := SteerTag{}}
    }

    // An agent, on the left: its mark, who it is, then what it says. The
    // mark is set as it is drawn: its model's provider, or the agent's own.
    let AgentMsg = View{
        width: Fill height: Fit flow: Right spacing: 10
        padding: Inset{top: 10 bottom: 6 right: 24}
        icon := Svg{width: 22 height: 22 animating: false}
        View{
            width: Fill height: Fit flow: Down spacing: 5
            author := Label{text: "" padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
            body := MsgMarkdown{}
            footer := MsgFooter{}
        }
    }
    // The same, with the tools it used above its text.
    let AgentMsgSteps = View{
        width: Fill height: Fit flow: Right spacing: 10
        padding: Inset{top: 10 bottom: 6 right: 24}
        icon := Svg{width: 22 height: 22 animating: false}
        View{
            width: Fill height: Fit flow: Down spacing: 6
            author := Label{text: "" padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
            // Its tool calls, folded to one line (how many, what it does now)
            // until that line is clicked.
            RoundedView{
                width: Fill height: Fit new_batch: true flow: Down spacing: 4
                padding: Inset{left: 10 right: 10 top: 6 bottom: 6}
                draw_bg.color: th_panel draw_bg.border_radius: 6.0
                steps_head := View{
                    width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                    cursor: MouseCursor.Hand grab_key_focus: false
                    steps_sum := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_muted_strong draw_text.text_style: theme.font_code{font_size: 8}}
                    steps_fold := Label{text: "" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8}
                }
                steps_body := View{
                    width: Fill height: Fit visible: false
                    steps := Label{width: Fill text: "" draw_text.color: th_muted draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 8}}
                }
            }
            body := MsgMarkdown{}
            footer := MsgFooter{}
        }
    }
    let OuterMsg = AgentMsg{}
    let OuterMsgSteps = AgentMsgSteps{}
    let InnerMsg = AgentMsg{}
    let InnerMsgSteps = AgentMsgSteps{}
    let CodexMsg = AgentMsg{}
    let CodexMsgSteps = AgentMsgSteps{}
    let PiMsg = AgentMsg{}
    let PiMsgSteps = AgentMsgSteps{}

    // A message between the loops: folded to a few lines until clicked.
    let MsgCard = View{
        width: Fill height: Fit padding: Inset{left: 32 top: 5 bottom: 5 right: 24}
        card := RoundedView{
            width: Fill height: Fit new_batch: true flow: Down spacing: 4
            padding: Inset{left: 12 right: 12 top: 8 bottom: 8}
            cursor: MouseCursor.Hand grab_key_focus: false
            draw_bg.color: th_purple_bg draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_purple_bg_strong
            View{
                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                header := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_purple draw_text.text_style: theme.font_bold{font_size: 9}}
                fold := Label{text: "show all" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
            }
            // Folded: its header only; the whole message when opened.
        }
    }
    let MsgCardOpen = View{
        width: Fill height: Fit padding: Inset{left: 32 top: 5 bottom: 5 right: 24}
        card := RoundedView{
            width: Fill height: Fit new_batch: true flow: Down spacing: 4
            padding: Inset{left: 12 right: 12 top: 8 bottom: 8}
            draw_bg.color: th_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_purple_line
            card_head := View{
                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                cursor: MouseCursor.Hand grab_key_focus: false
                header := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_purple draw_text.text_style: theme.font_bold{font_size: 9}}
                fold := Label{text: "collapse" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
            }
            body := CardMarkdown{}
        }
    }

    let SystemMsg = View{
        width: Fill height: Fit padding: Inset{left: 32 right: 24 top: 4 bottom: 4}
        body := Label{width: Fill text: "" draw_text.color: th_faint draw_text.wrap: Words draw_text.text_style.font_size: 9}
    }

    let ChatEmpty = View{
        width: Fill height: 240 flow: Down spacing: 8 align: Align{x: 0.5 y: 0.5}
        empty_title := Label{text: "Start the campaign" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 14}}
        empty_text := Label{text: "Describe the task. The outer loop plans it, splits it and starts the inner loops." draw_text.color: muted draw_text.text_style.font_size: 10.5}
    }

    // One conversation view, for the session and for each inner loop.
    let ChatList = PortalList{
        width: Fill height: Fill
        auto_tail: true
        // Drag across messages to select text; Cmd+C copies it, Cmd+A all.
        // (A mouse drag selects: the wheel and the scroll bar scroll.)
        selectable: true
        drag_scrolling: false
        scroll_bar: ScrollBar{}
        User := UserMsg{}
        UserWide := UserMsgWide{}
        UserSteer := UserSteerMsg{}
        UserSteerWide := UserSteerMsgWide{}
        Outer := OuterMsg{}
        OuterSteps := OuterMsgSteps{}
        Inner := InnerMsg{}
        InnerSteps := InnerMsgSteps{}
        Codex := CodexMsg{}
        CodexSteps := CodexMsgSteps{}
        Pi := PiMsg{}
        PiSteps := PiMsgSteps{}
        Card := MsgCard{}
        CardOpen := MsgCardOpen{}
        System := SystemMsg{}
        Empty := ChatEmpty{}
    }

    // What waits, above an input: in the order it runs; the person moves or drops it.
    let QueueRow = RoundedView{
        width: Fill height: Fit visible: false new_batch: true
        flow: Right spacing: 6 align: Align{y: 0.5}
        padding: Inset{left: 8 right: 2 top: 2 bottom: 2}
        draw_bg.color: th_panel draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_hover
        num := Label{text: "" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
        who := Label{text: "" padding: 0 draw_text.color: th_purple draw_text.text_style: theme.font_bold{font_size: 8.5}}
        text := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: ink draw_text.text_style.font_size: 9}
        // The person's message, into the turn running now (the outer queue).
        steer := SmallButton{text: "Steer in" width: Fit height: 22 visible: false padding: Inset{left: 8 right: 8}}
        up := SmallButton{text: "↑" width: 22 height: 22}
        down := SmallButton{text: "↓" width: 22 height: 22}
        del := SmallButton{text: "×" width: 22 height: 22}
    }
    let QueuePanel = View{
        width: Fill height: Fit flow: Down spacing: 3 visible: false
        head := Label{text: "" padding: 0 draw_text.color: muted draw_text.text_style: theme.font_bold{font_size: 8.5}}
        q0 := QueueRow{}
        q1 := QueueRow{}
        q2 := QueueRow{}
        q3 := QueueRow{}
        q4 := QueueRow{}
        q5 := QueueRow{}
        more := Label{text: "" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
    }

    let SendButton = ButtonFlat{
        text: "Send" width: Fit height: 30 padding: Inset{left: 16 right: 16}
        draw_bg +: {color: th_accent color_hover: th_accent_hover color_down: th_accent_down color_focus: th_accent border_size: 0.0 border_radius: 3.0}
        draw_text +: {color: th_on_accent color_hover: th_on_accent color_down: th_on_accent color_focus: th_on_accent}
    }

    // What a loop runs, under its input: the agent's mark and name, the model's.
    let ModelIcon = Svg{width: 14 height: 14 animating: false draw_svg +: {svg: crate_resource("self:resources/claude.svg")}}
    // A provider's mark beside a model (`provider_icons.rs` sets which; no
    // resource of its own, or that would replace the one set).
    let ProviderIcon = Svg{width: 14 height: 14 animating: false}
    let ModelText = Label{text: "" padding: 0 draw_text.color: th_muted_strong draw_text.text_style.font_size: 9}
    // The engine picker's rows: an agent (its icon, its name, its current
    // model under it), a model of the agent chosen.
    // A rounded row that tints under the pointer (a soft blue wash over
    // whatever colour it has, chosen or not), as Cindy's picker rows.
    let HoverRow = RoundedView{
        draw_bg +: {
            hover: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(self.border_size, self.border_size, self.rect_size.x - self.border_size * 2.0, self.rect_size.y - self.border_size * 2.0, max(1.0, self.border_radius))
                let tint = vec4(0.18, 0.43, 0.92, 1.0)
                let base = self.color
                // A clear row (an icon button): the tint itself, faintly.
                let rgb = if base.a < 0.01 tint.rgb else mix(base.rgb, tint.rgb, 0.10 * self.hover)
                let wash = vec4(rgb, max(base.a, 0.10 * self.hover))
                sdf.fill_keep(wash)
                if self.border_size > 0.0 {
                    sdf.stroke(self.border_color, self.border_size)
                }
                return sdf.result
            }
        }
        animator: Animator{
            hover: {
                default: @off
                off: AnimatorState{from: {all: Forward {duration: 0.12}} apply: {draw_bg: {hover: 0.0}}}
                on: AnimatorState{from: {all: Forward {duration: 0.08}} apply: {draw_bg: {hover: 1.0}}}
            }
        }
    }
    let AgentRow = HoverRow{
        width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5} new_batch: true
        padding: Inset{left: 8 right: 8 top: 6 bottom: 6}
        cursor: MouseCursor.Hand grab_key_focus: false
        draw_bg.color: #x00000000 draw_bg.border_radius: 6.0
        icon := ModelIcon{}
        View{
            width: Fill height: Fit flow: Down spacing: 1
            name := Label{width: Fill text: "" padding: 0 max_lines: 1 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
            sub := Label{width: Fill text: "" padding: 0 max_lines: 1 draw_text.color: muted draw_text.text_style.font_size: 8}
        }
        arrow := Label{text: "›" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 11}
    }
    let ModelRow = HoverRow{
        width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5} new_batch: true visible: false
        padding: Inset{left: 8 right: 8 top: 6 bottom: 6}
        cursor: MouseCursor.Hand grab_key_focus: false
        draw_bg.color: #x00000000 draw_bg.border_radius: 6.0
        picon := ProviderIcon{}
        label := Label{width: Fill text: "" padding: 0 max_lines: 1 draw_text.color: ink draw_text.text_style.font_size: 9.5}
        mark := Label{text: "" padding: 0 draw_text.color: th_success draw_text.text_style: theme.font_bold{font_size: 10}}
    }
    // An agent's own terminal UI. Its fonts named by crate, not by a path
    // out of the terminal's crate (`self:../../widgets/…`): a packaged
    // OctoBuddy finds a resource only by its crate's name, so the terminal's
    // own fonts drew nothing there (a blank terminal). The same members, in
    // the same order. Bold text has its own family. (The terminal keeps them
    // only while its settings name no fonts: `native_tui::own_terminal_settings`.)
    let AgentTerm = MpTerm{
        draw_bg +: {corner_radius: 6.0 frame_width: 1.0 frame_color: th_line}
        draw_text +: {
            text_style +: {
                font_family +: {
                    latin := FontMember{res: crate_resource("makepad_widgets:resources/jetbrains_mono_variable.ttf") asc: 0.0 desc: 0.0 weight: 400.0}
                    nerd := FontMember{res: crate_resource("makepad_terminal:resources/SymbolsNerdFontMono-Regular.ttf") asc: 0.0 desc: 0.0}
                    icons := FontMember{res: crate_resource("makepad_widgets:resources/fa-solid-900.ttf") asc: 0.0 desc: 0.0}
                    emoji := FontMember{res: crate_resource("makepad_widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                    symbols := FontMember{res: crate_resource("makepad_widgets:resources/Inter.ttf") asc: 0.0 desc: 0.0}
                    chinese := FontMember{res: crate_resource("makepad_widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0}
                }
            }
        }
        bold_text_style +: {
            font_family +: {
                latin := FontMember{res: crate_resource("makepad_widgets:resources/jetbrains_mono_variable.ttf") asc: 0.0 desc: 0.0 weight: 800.0}
                nerd := FontMember{res: crate_resource("makepad_terminal:resources/SymbolsNerdFontMono-Regular.ttf") asc: 0.0 desc: 0.0}
                icons := FontMember{res: crate_resource("makepad_widgets:resources/fa-solid-900.ttf") asc: 0.0 desc: 0.0}
                emoji := FontMember{res: crate_resource("makepad_widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
                symbols := FontMember{res: crate_resource("makepad_widgets:resources/Inter.ttf") asc: 0.0 desc: 0.0}
                chinese := FontMember{res: crate_resource("makepad_widgets:resources/LXGWWenKaiBold.ttf") asc: 0.0 desc: 0.0}
            }
        }
    }
    // The app workbench's Files: a row of the project's tree.
    let FileItem = View{
        width: Fill height: Fit
        row := HoverRow{
            width: Fill height: Fit padding: Inset{left: 8 right: 8 top: 3 bottom: 3}
            cursor: MouseCursor.Hand grab_key_focus: false new_batch: true
            draw_bg.color: #x00000000 draw_bg.border_radius: 4.0
            name := Label{width: Fill text: "" padding: 0 max_lines: 1 draw_text.color: ink draw_text.text_style: theme.font_code{font_size: 9}}
        }
    }
    // The app workbench's App Hub form: a section's title, a field's name, a tick.
    let HubSection = Label{width: Fill text: "" margin: Inset{top: 12} draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
    let HubLabel = Label{width: Fill text: "" margin: Inset{top: 4} draw_text.color: muted draw_text.text_style.font_size: 9}
    let HubDrop = DropDown{
        height: 34 padding: Inset{left: 10 right: 24 top: 8 bottom: 8}
        draw_text +: {color: th_ink color_hover: th_ink color_focus: th_ink color_down: th_ink text_style: theme.font_regular{font_size: 10}}
        draw_bg +: {
            color: th_raised color_hover: th_raised color_focus: th_raised color_down: th_raised
            border_color: th_line border_color_hover: th_line_strong border_color_focus: th_accent border_color_down: th_accent
            border_color_2: th_line border_color_2_hover: th_line_strong border_color_2_focus: th_accent border_color_2_down: th_accent
        }
    }
    let HubCheck = CheckBox{
        text: ""
        draw_text +: {
            color: th_ink color_hover: th_ink color_down: th_ink color_focus: th_ink color_active: th_ink
            text_style: theme.font_regular{font_size: 9.5}
        }
    }
    // The app workbench's Permissions: a capability, what the store says it does.
    let CapRow = View{
        width: Fill height: Fit flow: Down spacing: 2 padding: Inset{top: 4 bottom: 4}
        check := CheckBox{
            text: ""
            draw_text +: {
                color: th_ink color_hover: th_ink color_down: th_ink color_focus: th_ink color_active: th_ink
                text_style: theme.font_bold{font_size: 9.5}
            }
        }
        note := Label{width: Fill text: "" margin: Inset{left: 26} draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
    }
    // A theme in Settings › Appearance: a swatch in its own colours (set by
    // the code), its name, whether it is light or dark, a mark when in use.
    let ThemeRow = HoverRow{
        width: Fill height: Fit flow: Right spacing: 12 align: Align{y: 0.5} new_batch: true
        padding: Inset{left: 10 right: 14 top: 8 bottom: 8}
        cursor: MouseCursor.Hand grab_key_focus: false
        draw_bg.color: th_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
        swatch := RoundedView{
            width: 60 height: 36 flow: Down spacing: 5 new_batch: true padding: Inset{left: 7 right: 7 top: 7 bottom: 7}
            draw_bg.color: th_bg draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
            sw_ink := RoundedView{width: 34 height: 5 draw_bg.color: th_ink draw_bg.border_radius: 2.5}
            View{
                width: Fill height: Fit flow: Right spacing: 4 align: Align{y: 0.5}
                sw_accent := RoundedView{width: 16 height: 9 draw_bg.color: th_accent draw_bg.border_radius: 3.0}
                sw_muted := RoundedView{width: 20 height: 4 draw_bg.color: th_muted draw_bg.border_radius: 2.0}
            }
        }
        View{
            width: Fill height: Fit flow: Down spacing: 2
            name := Label{text: "" padding: 0 draw_text.color: th_ink draw_text.text_style: theme.font_bold{font_size: 10.5}}
            sub := Label{text: "" padding: 0 draw_text.color: th_muted draw_text.text_style.font_size: 9}
        }
        mark := Label{text: "" padding: 0 draw_text.color: th_accent draw_text.text_style: theme.font_bold{font_size: 13}}
    }
    // An effort level in the picker (the one in use, tinted by the code).
    let EffortChip = HoverRow{
        width: Fit height: 26 flow: Right align: Align{x: 0.5 y: 0.5} new_batch: true visible: false
        padding: Inset{left: 10 right: 10}
        cursor: MouseCursor.Hand grab_key_focus: false
        draw_bg.color: th_raised draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
        label := Label{text: "" padding: 0 draw_text.color: ink draw_text.text_style.font_size: 9}
    }

    let Chip = RoundedView{
        width: Fit height: Fit new_batch: true
        padding: Inset{left: 8 right: 8 top: 3 bottom: 3}
        draw_bg.color: th_sidebar draw_bg.border_radius: 9.0
        label := Label{text: "" draw_text.color: muted draw_text.text_style.font_size: 9}
    }

    let SegOff = ButtonFlat{
        width: Fit height: 24 padding: Inset{left: 10 right: 10}
        draw_bg +: {color: th_sidebar color_hover: th_hover color_down: th_line color_focus: th_sidebar border_size: 0.0 border_radius: 6.0}
        draw_text +: {color: th_muted_strong color_hover: th_ink color_down: th_ink color_focus: th_muted_strong text_style +: {font_size: 9.5}}
    }
    let SegOn = SegOff{
        draw_bg +: {color: th_inverse_bg color_hover: th_inverse_bg color_down: th_inverse_bg color_focus: th_inverse_bg}
        draw_text +: {color: th_inverse_fg color_hover: th_inverse_fg color_down: th_inverse_fg color_focus: th_inverse_fg}
    }

    let NavOff = ButtonFlat{
        width: Fill height: 32 align: Align{x: 0.0 y: 0.5} padding: Inset{left: 12 right: 12}
        draw_bg +: {color: #x00000000 color_hover: th_hover color_down: th_line color_focus: #x00000000 border_size: 0.0 border_radius: 6.0}
        draw_text +: {color: th_ink2 color_hover: th_ink color_down: th_ink color_focus: th_ink2 text_style +: {font_size: 10.5}}
    }
    // One slice in the plan panel: where it stands, who took it, its time.
    let PlanRow = View{
        width: Fill height: Fit flow: Down spacing: 2 visible: false
        cursor: MouseCursor.Hand grab_key_focus: false
        View{
            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
            dot := RoundedView{width: 8 height: 8 new_batch: true draw_bg.color: th_faint draw_bg.border_radius: 4.0}
            name := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 8.5}}
            who := Label{text: "" padding: 0 draw_text.color: th_purple draw_text.text_style.font_size: 8}
        }
        meta := Label{width: Fill text: "" padding: Inset{left: 14} draw_text.color: th_muted draw_text.wrap: Words draw_text.text_style.font_size: 8}
    }
    // What does what cannot be undone.
    let DangerButton = SegOff{
        draw_bg +: {color: th_danger color_hover: th_danger_strong color_down: th_danger_strong color_focus: th_danger}
        draw_text +: {color: th_on_accent color_hover: th_on_accent color_down: th_on_accent color_focus: th_on_accent}
    }
    // A right-click menu's row.
    // A tappable icon (its `icon` child): no text, a hand cursor.
    let IconButton = HoverRow{
        width: Fit height: Fit padding: 7 new_batch: true
        cursor: MouseCursor.Hand grab_key_focus: false
        draw_bg.color: #x00000000 draw_bg.border_radius: 6.0
    }
    let MenuItem = NavOff{height: 28 padding: Inset{left: 8 right: 8} draw_text +: {text_style +: {font_size: 9.5}}}
    let NavOn = NavOff{
        draw_bg +: {color: th_accent_selected color_hover: th_accent_selected color_down: th_accent_selected color_focus: th_accent_selected}
        draw_text +: {color: th_accent_ink color_hover: th_accent_ink color_down: th_accent_ink color_focus: th_accent_ink}
    }

    let SectionTitle = Label{text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 12.5}}
    let Body = Label{width: Fill text: "" draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 10.5}

    let PeerTab = View{
        width: 132 height: Fill padding: Inset{left: 3 right: 3}
        tab := RoundedView{
            width: Fill height: Fill new_batch: true flow: Down spacing: 4
            padding: Inset{left: 10 right: 10 top: 8 bottom: 8}
            cursor: MouseCursor.Hand grab_key_focus: false
            draw_bg.color: th_raised draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
            head := View{
                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                Svg{width: 14 height: 14 animating: false draw_svg +: {svg: crate_resource("self:resources/octos.svg")}}
                role := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 10}}
                dot := RoundedView{width: 7 height: 7 draw_bg.color: th_faint draw_bg.border_radius: 3.5}
            }
            state := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
        }
    }
    let TabRunning = PeerTab{tab +: {head +: {dot +: {draw_bg.color: th_accent}}}}
    let TabDone = PeerTab{tab +: {head +: {dot +: {draw_bg.color: th_success}}}}
    let TabFailed = PeerTab{tab +: {head +: {dot +: {draw_bg.color: th_danger}}}}
    let TabHalted = PeerTab{tab +: {head +: {dot +: {draw_bg.color: th_warning}}}}
    // The selected tab: the same, with an accent border and tint.
    let TabQueuedSel = PeerTab{tab +: {draw_bg.color: th_accent_soft draw_bg.border_color: th_accent draw_bg.border_size: 1.5}}
    let TabRunningSel = TabRunning{tab +: {draw_bg.color: th_accent_soft draw_bg.border_color: th_accent draw_bg.border_size: 1.5}}
    let TabDoneSel = TabDone{tab +: {draw_bg.color: th_accent_soft draw_bg.border_color: th_accent draw_bg.border_size: 1.5}}
    let TabFailedSel = TabFailed{tab +: {draw_bg.color: th_accent_soft draw_bg.border_color: th_accent draw_bg.border_size: 1.5}}
    let TabHaltedSel = TabHalted{tab +: {draw_bg.color: th_accent_soft draw_bg.border_color: th_accent draw_bg.border_size: 1.5}}

    let Badge = RoundedView{
        width: Fit height: Fit new_batch: true visible: false
        padding: Inset{left: 8 right: 8 top: 2 bottom: 2}
        draw_bg.color: th_panel draw_bg.border_radius: 8.0
        label := Label{text: "" draw_text.color: th_muted_strong draw_text.text_style: theme.font_bold{font_size: 8.5}}
    }

    // A spec in the open card's Spec tab: its file, its state, where it came from.
    let SpecRowView = View{
        width: Fill height: Fit padding: Inset{top: 2 bottom: 2}
        row := RoundedView{
            width: Fill height: Fit new_batch: true flow: Down spacing: 2
            cursor: MouseCursor.Hand grab_key_focus: false
            padding: Inset{left: 10 right: 10 top: 7 bottom: 7}
            draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
            View{
                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                spec_name := Label{width: Fill text: "" max_lines: 1 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
                spec_state := Label{text: "" draw_text.color: muted draw_text.text_style.font_size: 8.5}
            }
            spec_meta := Label{width: Fill text: "" max_lines: 1 draw_text.color: muted draw_text.text_style.font_size: 8.5}
        }
    }

    // A plugin in Settings › Plugins: what it adds, and its switch.
    let PluginRowView = View{
        width: Fill height: Fit padding: Inset{top: 4 bottom: 4}
        card := RoundedView{
            width: Fill height: Fit new_batch: true flow: Right spacing: 10 align: Align{y: 0.5}
            padding: Inset{left: 14 right: 14 top: 10 bottom: 10}
            draw_bg.color: th_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
            View{
                width: Fill height: Fit flow: Down spacing: 3
                name := Label{width: Fill text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
                detail := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                adds := Label{width: Fill text: "" draw_text.color: th_muted_strong draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
            }
            on := SegOn{text: "On" height: 26}
            off := SegOff{text: "Off" height: 26}
        }
    }
    // A tool in Settings › Tools, a card of the grid: what it is, its
    // version, where it comes from, where it is.
    let ToolCard = RoundedView{
        width: 300 height: Fit new_batch: true flow: Down spacing: 4 visible: false
        margin: Inset{bottom: 12}
        padding: Inset{left: 14 right: 14 top: 12 bottom: 12}
        draw_bg.color: th_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
        name := Label{width: Fill text: "" max_lines: 1 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
        version := Label{width: Fill text: "" max_lines: 1 draw_text.color: th_success draw_text.text_style: theme.font_code{font_size: 9}}
        // An agent's program OctoBuddy keeps: which copy runs, and the choice.
        source := Label{width: Fill text: "" draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9}
        agent_row := View{
            width: Fill height: Fit flow: Right spacing: 4 align: Align{y: 0.5} visible: false
            margin: Inset{top: 2 bottom: 2}
            use_kept_on := SegOn{text: "OctoBuddy's"} use_kept := SegOff{text: "OctoBuddy's"}
            use_own_on := SegOn{text: "Yours"} use_own := SegOff{text: "Yours"}
            Filler{}
            install := SegOff{text: "Install"}
        }
        what := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
        repo := Label{width: Fill text: "" draw_text.color: th_accent draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 8}}
        path := Label{width: Fill text: "" draw_text.color: th_muted draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 7.5}}
    }

    // The Live page: an app as people run it, its last runs, what to do.
    let LiveRun = RoundedView{width: 8 height: 8 visible: false draw_bg.color: th_line_strong draw_bg.border_radius: 4.0}
    let LiveCard = RoundedView{
        width: Fill height: Fit new_batch: true flow: Down spacing: 7 visible: false
        margin: Inset{bottom: 10}
        padding: Inset{left: 16 right: 16 top: 12 bottom: 12}
        draw_bg.color: th_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
        View{
            width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
            dot := RoundedView{width: 10 height: 10 draw_bg.color: th_line_strong draw_bg.border_radius: 5.0}
            name := Label{text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 12}}
            version := Label{text: "" draw_text.color: muted draw_text.text_style: theme.font_code{font_size: 9}}
            Filler{}
            state := Label{text: "" draw_text.color: th_ink2 draw_text.text_style.font_size: 9.5}
        }
        runs := View{
            width: Fill height: Fit flow: Right spacing: 4 align: Align{y: 0.5}
            h0 := LiveRun{} h1 := LiveRun{} h2 := LiveRun{} h3 := LiveRun{} h4 := LiveRun{} h5 := LiveRun{}
            h6 := LiveRun{} h7 := LiveRun{} h8 := LiveRun{} h9 := LiveRun{} h10 := LiveRun{} h11 := LiveRun{}
        }
        why := Label{width: Fill text: "" draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
        View{
            width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 6 align: Align{y: 0.5}
            watch_on := SegOn{text: "Watching"} watch_off := SegOff{text: "Watch"}
            run_now := SegOff{text: "Run now"}
            drill_on := SegOn{text: "Drill"} drill_off := SegOff{text: "Drill"}
            auto_on := SegOn{text: "Auto repair"} auto_off := SegOff{text: "Auto repair"}
            drill_bad := SegOff{text: "Drill: bad release"}
            open := SegOff{text: "Session"}
            repair := SegOn{text: "Repair"}
            publish_fix := SegOn{text: "Publish fix"}
        }
    }

    // The Apps page: an app asked for from outside, waiting for the person.
    let RequestCard = RoundedView{
        width: Fill height: Fit new_batch: true flow: Down spacing: 6 visible: false
        margin: Inset{bottom: 10}
        padding: Inset{left: 16 right: 16 top: 12 bottom: 12}
        draw_bg.color: th_accent_soft draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_accent_line
        who := Label{width: Fill text: "" draw_text.color: th_accent_ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
        what := Label{width: Fill text: "" draw_text.color: ink draw_text.wrap: Words draw_text.text_style.font_size: 11}
        details := Label{width: Fill text: "" draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
        View{
            width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 6 align: Align{y: 0.5}
            build := SegOn{text: "Build"}
            decline := SegOff{text: "Decline"}
            open := SegOff{text: "Session"}
            publish := SegOn{text: "Publish"}
            dismiss := SegOff{text: "Dismiss"}
        }
    }

    let ProviderRowView = View{
        width: Fill height: Fit padding: Inset{top: 4 bottom: 4}
        card := RoundedView{
            width: Fill height: Fit new_batch: true flow: Right spacing: 10 align: Align{y: 0.5}
            padding: Inset{left: 14 right: 14 top: 10 bottom: 10}
            draw_bg.color: th_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
            picon := ProviderIcon{width: 22 height: 22}
            View{
                width: Fill height: Fit flow: Down spacing: 3
                name := Label{width: Fill text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
                detail := Label{width: Fill text: "" draw_text.color: muted draw_text.text_style.font_size: 9.5}
                agents := Label{width: Fill text: "" draw_text.color: th_success draw_text.text_style.font_size: 9}
            }
            row_primary := SegOff{text: "Make primary" visible: false}
            row_test := SegOff{text: "Test" visible: false}
            row_remove := SegOff{text: "Remove" visible: false}
            role_badge := RoundedView{
                width: Fit height: Fit new_batch: true
                padding: Inset{left: 8 right: 8 top: 2 bottom: 2}
                draw_bg.color: th_accent_soft draw_bg.border_radius: 8.0
                role := Label{text: "" draw_text.color: th_accent_ink draw_text.text_style: theme.font_bold{font_size: 8.5}}
            }
        }
    }

    // OctoBuddy's own (on the host): its rows can be changed. (Visibility
    // set on a list item's child does not take: a template each.)
    let OwnPrimaryRow = ProviderRowView{card +: {row_test +: {visible: true} row_remove +: {visible: true}}}
    let OwnFallbackRow = ProviderRowView{card +: {row_primary +: {visible: true} row_test +: {visible: true} row_remove +: {visible: true}}}

    // Settings › AI Providers on the host: the add wizard's rows.
    let WizardHeader = View{
        width: Fill height: Fit padding: Inset{top: 10 bottom: 4}
        label := Label{text: "" draw_text.color: muted draw_text.text_style: theme.font_bold{font_size: 9.5}}
    }
    let WizardFamily = View{
        width: Fill height: Fit padding: Inset{top: 3 bottom: 3}
        card := HoverRow{
            width: Fill height: Fit new_batch: true flow: Right spacing: 10 align: Align{y: 0.5}
            padding: Inset{left: 14 right: 14 top: 10 bottom: 10}
            cursor: MouseCursor.Hand grab_key_focus: false
            draw_bg.color: th_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
            picon := ProviderIcon{width: 22 height: 22}
            View{
                width: Fill height: Fit flow: Down spacing: 3
                name := Label{width: Fill text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
                meta := Label{width: Fill text: "" draw_text.color: muted draw_text.text_style.font_size: 9.5}
            }
            tag := Label{text: "" draw_text.color: th_accent draw_text.text_style.font_size: 9}
        }
    }
    let WizardModel = View{
        width: Fill height: Fit padding: Inset{top: 2 bottom: 2}
        card := HoverRow{
            width: Fill height: Fit new_batch: true flow: Right spacing: 10 align: Align{y: 0.5}
            padding: Inset{left: 14 right: 14 top: 8 bottom: 8}
            cursor: MouseCursor.Hand grab_key_focus: false
            draw_bg.color: th_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
            mark := Label{width: 16 text: "" draw_text.color: th_accent draw_text.text_style: theme.font_bold{font_size: 11}}
            View{
                width: Fill height: Fit flow: Down spacing: 2
                name := Label{width: Fill text: "" draw_text.color: ink draw_text.text_style.font_size: 10.5}
                meta := Label{width: Fill text: "" draw_text.color: muted draw_text.text_style.font_size: 9}
            }
            tag := Label{text: "" draw_text.color: th_accent draw_text.text_style.font_size: 9}
        }
    }

    let ProvidersEmpty = View{
        width: Fill height: Fit padding: Inset{top: 6 bottom: 6}
        empty_text := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 10}
    }

    // The floating buttons over the conversation (on: the one in use): light
    // pills, its mark, its name and what it does now on one line.
    let FloatSquare = RoundedView{
        width: Fit height: 30 new_batch: true flow: Right spacing: 5 align: Align{y: 0.5}
        padding: Inset{left: 10 right: 11 top: 0 bottom: 0}
        cursor: MouseCursor.Hand grab_key_focus: false
        draw_bg.color: th_glass_raised draw_bg.border_radius: 8.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
    }
    let FloatName = Label{text: "" padding: 0 draw_text.color: ink draw_text.text_style.font_size: 9}
    let FloatSub = Label{text: "" padding: 0 max_lines: 1 draw_text.color: muted draw_text.text_style.font_size: 8}
    let FloatOuter = FloatSquare{
        icon := Svg{width: 15 height: 15 animating: false draw_svg +: {svg: crate_resource("self:resources/claude.svg")}}
        name := FloatName{}
        sub := FloatSub{}
    }
    let FloatInner = FloatSquare{
        icon := Svg{width: 15 height: 15 animating: false draw_svg +: {svg: crate_resource("self:resources/octos.svg")}}
        name := FloatName{}
        sub := FloatSub{}
    }
    let FlowIcon = Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
        Path{d: "M3 4h6v5H3zM15 15h6v5h-6z" fill: false stroke: th_muted_strong stroke_width: 1.8 stroke_linejoin: "round"}
        Path{d: "M9 6.5c5 0 1 11 6 11" fill: false stroke: th_muted_strong stroke_width: 1.8 stroke_linecap: "round"}
    }
    let ChatIcon = Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
        Path{d: "M4 5h16v11H10l-5 4v-4H4z" fill: false stroke: th_muted_strong stroke_width: 1.8 stroke_linejoin: "round"}
    }
    let PlayIcon = Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
        Path{d: "M8 5l11 7-11 7z" fill: false stroke: th_success stroke_width: 1.8 stroke_linejoin: "round"}
    }
    let TimeIcon = Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
        Path{d: "M3 7h9M3 12h14M3 17h6" fill: false stroke: th_muted_strong stroke_width: 1.8 stroke_linecap: "round"}
        Path{d: "M20 4v16" fill: false stroke: th_danger stroke_width: 1.6 stroke_linecap: "round"}
    }
    let FloatFlow = FloatSquare{icon := FlowIcon{width: 15 height: 15} name := FloatName{} sub := FloatSub{}}
    let FloatChat = FloatSquare{icon := ChatIcon{width: 15 height: 15} name := FloatName{} sub := FloatSub{}}
    let FloatTime = FloatSquare{icon := TimeIcon{width: 15 height: 15} name := FloatName{} sub := FloatSub{}}
    let TermIcon = Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
        Path{d: "M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM7 10l3 2-3 2M12 15h5" fill: false stroke: th_ink stroke_width: 1.6 stroke_linecap: "round" stroke_linejoin: "round"}
    }
    let FloatTui = FloatSquare{icon := TermIcon{width: 15 height: 15} name := FloatName{} sub := FloatSub{}}
    let FloatApp = FloatSquare{icon := PlayIcon{width: 15 height: 15} name := FloatName{} sub := FloatSub{}}
    let DataIcon = Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
        Path{d: "M4 6c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6M4 12c0 1.7 3.6 3 8 3s8-1.3 8-3" fill: false stroke: th_purple stroke_width: 1.6 stroke_linejoin: "round"}
    }
    let FloatData = FloatSquare{icon := DataIcon{width: 15 height: 15} name := FloatName{} sub := FloatSub{}}
    let PublishIcon = Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
        Path{d: "M12 15V4M7 9l5-5 5 5M5 15v4h14v-4" fill: false stroke: th_accent stroke_width: 1.8 stroke_linecap: "round" stroke_linejoin: "round"}
    }
    let FloatPublish = FloatSquare{icon := PublishIcon{width: 15 height: 15} name := FloatName{} sub := FloatSub{}}
    // An external plugin's button (Settings › Plugins).
    let PluginIcon = Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
        Path{d: "M9 3v4M15 3v4M7 7h10v5a5 5 0 0 1-10 0zM12 17v4" fill: false stroke: th_muted_strong stroke_width: 1.7 stroke_linecap: "round" stroke_linejoin: "round"}
    }
    let FloatPlugin = FloatSquare{icon := PluginIcon{width: 15 height: 15} name := FloatName{} sub := FloatSub{}}

    // The flow canvas's cards: an outer loop, an inner loop (lit: the one
    // shown, or at work).
    // What kind of agent a card is, at its top right.
    let FlowTag = RoundedView{
        width: Fit height: Fit new_batch: true padding: Inset{left: 5 right: 5 top: 1 bottom: 1}
        draw_bg.color: th_sidebar draw_bg.border_radius: 3.0
        tag := Label{text: "" padding: 0 draw_text.color: th_muted_strong draw_text.text_style.font_size: 7.5}
    }
    let FlowOuterCard = RoundedView{
        width: Fill height: Fill new_batch: true flow: Down spacing: 4
        padding: Inset{left: 12 right: 12 top: 10 bottom: 10}
        draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
        View{
            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
            Svg{width: 18 height: 18 animating: false draw_svg +: {svg: crate_resource("self:resources/claude.svg")}}
            title := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 10}}
            kind := FlowTag{}
        }
        status := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 9}
        // What it runs on: engine · model · effort.
        model := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_faint draw_text.text_style: theme.font_code{font_size: 7.5}}
        detail := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_muted_strong draw_text.text_style.font_size: 8.5}
        // What waits for it, under a line.
        queue_box := View{
            width: Fill height: Fit flow: Down spacing: 2 visible: false
            SolidView{width: Fill height: 1 margin: Inset{top: 3 bottom: 2} draw_bg.color: th_hover}
            queue := Label{width: Fill text: "" max_lines: 4 padding: 0 draw_text.color: th_muted draw_text.text_style.font_size: 8}
        }
    }
    let FlowOuterCardLit = FlowOuterCard{draw_bg.border_color: th_accent draw_bg.border_size: 2.0 draw_bg.color: th_accent_soft}
    let FlowInnerCard = RoundedView{
        width: Fill height: Fill new_batch: true flow: Down spacing: 3
        padding: Inset{left: 10 right: 10 top: 8 bottom: 8}
        draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
        View{
            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
            Svg{width: 15 height: 15 animating: false draw_svg +: {svg: crate_resource("self:resources/octos.svg")}}
            title := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
            kind := FlowTag{}
        }
        status := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
        model := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_faint draw_text.text_style: theme.font_code{font_size: 7.5}}
        detail := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_muted_strong draw_text.text_style.font_size: 8.5}
        // What waits for it, under a line.
        queue_box := View{
            width: Fill height: Fit flow: Down spacing: 2 visible: false
            SolidView{width: Fill height: 1 margin: Inset{top: 3 bottom: 2} draw_bg.color: th_hover}
            queue := Label{width: Fill text: "" max_lines: 4 padding: 0 draw_text.color: th_muted draw_text.text_style.font_size: 8}
        }
    }
    // At work: green. Chosen (its conversation open): blue, like an outer loop's.
    let FlowInnerCardLit = FlowInnerCard{draw_bg.border_color: th_success draw_bg.border_size: 2.0 draw_bg.color: th_success_bg}
    let FlowInnerCardSel = FlowInnerCard{draw_bg.border_color: th_accent draw_bg.border_size: 2.0 draw_bg.color: th_accent_soft}
    let FlowInnerCardSelLit = FlowInnerCard{draw_bg.border_color: th_accent draw_bg.border_size: 2.0 draw_bg.color: th_success_bg}
    // A subagent: sent for one job, shown while it runs.
    let FlowSubCard = RoundedView{
        width: 190 height: 56 new_batch: true flow: Down spacing: 3
        padding: Inset{left: 10 right: 10 top: 7 bottom: 7}
        draw_bg.color: th_purple_bg draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_purple_line
        View{
            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
            title := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_purple draw_text.text_style: theme.font_bold{font_size: 9}}
            kind := FlowTag{draw_bg.color: th_purple_bg tag +: {draw_text.color: th_purple}}
        }
        status := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
        detail := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8}
    }

    mod.widgets.ResizeHandleBase = #(resize::ResizeHandle::register_widget(vm))
    mod.widgets.ResizeHandle = set_type_default() do mod.widgets.ResizeHandleBase{
        width: 7 height: Fill
        draw_bg +: {color: #x00000000}
        line_color: th_line
        active_color: th_accent
    }
    let ResizeHandle = mod.widgets.ResizeHandle
    mod.widgets.FlowCanvasBase = #(flow::FlowCanvas::register_widget(vm))
    mod.widgets.FlowCanvas = set_type_default() do mod.widgets.FlowCanvasBase{
        width: Fill height: Fill
        draw_bg +: {color: th_panel}
        line_color: th_line_strong
        lit_color: th_accent
        wave_color: th_purple
        draw_text +: {text_style: theme.font_bold{font_size: 8}}
    }
    // Registered in this module, after its `use mod.widgets.*`: bound by name here.
    let FlowCanvas = mod.widgets.FlowCanvas
    mod.widgets.TimelineCanvasBase = #(timeline::TimelineCanvas::register_widget(vm))
    mod.widgets.TimelineCanvas = set_type_default() do mod.widgets.TimelineCanvasBase{
        width: Fill height: Fill
        draw_bg +: {color: th_raised}
        draw_text +: {text_style: theme.font_regular{font_size: 8}}
    }
    let TimelineCanvas = mod.widgets.TimelineCanvas

    mod.widgets.OctoBuddyViewBase = #(OctoBuddyView::register_widget(vm))
    mod.widgets.OctoBuddyView = set_type_default() do mod.widgets.OctoBuddyViewBase{
        width: Fill height: Fill flow: Overlay
      // The window: the workspace (or settings); over it, menus and dialogs.
      View{
        width: Fill height: Fill flow: Down

        workspace := View{
            width: Fill height: Fill flow: Right

            sidebar := SolidView{
                width: 230 height: Fill flow: Down new_batch: true
                draw_bg.color: sidebar_bg

                View{
                    width: Fill height: Fit padding: Inset{left: 14 right: 10 top: 14 bottom: 10}
                    flow: Right spacing: 8 align: Align{y: 0.5}
                    Svg{width: 20 height: 20 animating: false draw_svg +: {svg: crate_resource("self:resources/octos.svg")}}
                    projects_title := Label{text: "Projects" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 13}}
                    Filler{}
                    // A project, or a new OctoSense app (its menu).
                    add_project := SmallButton{text: "+" width: 28 height: 28}
                    // The sidebar folds away (the button at the stage's top left opens it).
                    sidebar_close := IconButton{icon := Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
                            Path{d: "M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM9 5v14" fill: false stroke: th_muted_strong stroke_width: 1.6 stroke_linejoin: "round"}
                        }}
                }
                SolidView{width: Fill height: 1 draw_bg.color: line}
                tree := PortalList{
                    width: Fill height: Fill
                    padding: Inset{top: 6 bottom: 6}
                    scroll_bar: ScrollBar{}
                    Project := ProjectRow{}
                    ProjectArchived := ProjectRowArchived{}
                    Session := SessionRow{}
                    SessionActive := SessionRowActive{}
                    SessionArchived := SessionRowArchived{}
                    Empty := SidebarEmpty{}
                }
                SolidView{width: Fill height: 1 draw_bg.color: line}
                View{
                    width: Fill height: Fit flow: Right align: Align{y: 0.5} padding: Inset{left: 8 right: 8 top: 6 bottom: 8}
                    // Settings: a gear.
                    settings_button := IconButton{
                        icon := Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
                            Path{d: "M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7z" fill: false stroke: th_muted_strong stroke_width: 1.7}
                            Path{d: "M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" fill: false stroke: th_muted_strong stroke_width: 1.5 stroke_linejoin: "round"}
                        }
                    }
                    // Live: the published apps, watched (red while one is not well).
                    live_button := IconButton{
                        icon := Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
                            Path{d: "M3 12h4l2.5-6 5 12 2.5-6h4" fill: false stroke: th_muted_strong stroke_width: 1.7 stroke_linecap: "round" stroke_linejoin: "round"}
                        }
                    }
                    live_button_alert := IconButton{
                        visible: false
                        icon := Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
                            Path{d: "M3 12h4l2.5-6 5 12 2.5-6h4" fill: false stroke: th_danger stroke_width: 2.0 stroke_linecap: "round" stroke_linejoin: "round"}
                        }
                    }
                    // An app asked for, waiting for the person.
                    live_button_request := IconButton{
                        visible: false
                        icon := Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
                            Path{d: "M3 12h4l2.5-6 5 12 2.5-6h4" fill: false stroke: th_accent stroke_width: 2.0 stroke_linecap: "round" stroke_linejoin: "round"}
                        }
                    }
                }
            }

            // Drag to widen the sidebar; double-click for its default width.
            sidebar_divider := View{width: Fit height: Fill sidebar_handle := ResizeHandle{}}

            main := SolidView{
                width: Fill height: Fill flow: Down new_batch: true
                draw_bg.color: pane_bg

                placeholder := View{
                    width: Fill height: Fill flow: Down spacing: 10 align: Align{x: 0.5 y: 0.5}
                    Svg{width: 56 height: 56 animating: false draw_svg +: {svg: crate_resource("self:resources/octos.svg")}}
                    Label{text: "OctoBuddy" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 20}}
                    placeholder_hint := Label{text: "" draw_text.color: muted draw_text.text_style.font_size: 11}
                }

                chat := View{
                    width: Fill height: Fill flow: Right visible: false

                    conversation := View{
                        width: Fill height: Fill flow: Down
                        // The session's worktree: what it holds, merged back once.
                        worktree_bar := View{
                          width: Fill height: Fit flow: Down visible: false
                          SolidView{
                            width: Fill height: Fit flow: Down spacing: 6 new_batch: true
                            padding: Inset{left: 20 right: 20 top: 8 bottom: 8}
                            draw_bg.color: th_panel
                            worktree_info := Label{width: Fill text: "" draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                            View{
                                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                merge_worktree := ButtonFlat{
                                    text: "Merge" width: Fit height: 26 padding: Inset{left: 12 right: 12}
                                    draw_bg +: {color: th_success color_hover: th_success_strong color_down: th_success_strong color_focus: th_success border_size: 0.0 border_radius: 6.0}
                                    draw_text +: {color: th_on_accent color_hover: th_on_accent color_down: th_on_accent color_focus: th_on_accent}
                                }
                                discard_worktree := SegOff{text: "Discard worktree" height: 26}
                            }
                          }
                          SolidView{width: Fill height: 1 draw_bg.color: line}
                        }
                      // The conversation (or the graph), with the floating buttons over it.
                      stage := View{
                        width: Fill height: Fill flow: Overlay
                        // A list is no View: hidden through this one (in the flow view it
                        // would otherwise still take presses under the canvas).
                        chat_pane := View{
                            width: Fill height: Fill
                            messages := ChatList{padding: Inset{left: 20 right: 20 top: 48 bottom: 8}}
                        }
                        // The project's loops as a graph; a card's conversation opens over it.
                        flow_view := View{
                            width: Fill height: Fill flow: Overlay visible: false
                            flow := FlowCanvas{
                                Outer := FlowOuterCard{}
                                OuterLit := FlowOuterCardLit{}
                                Inner := FlowInnerCard{}
                                InnerLit := FlowInnerCardLit{}
                                InnerSel := FlowInnerCardSel{}
                                InnerSelLit := FlowInnerCardSelLit{}
                                Sub := FlowSubCard{}
                            }
                            // Bottom left, on the canvas's own colour: never over a card's text.
                            View{
                                width: Fill height: Fill align: Align{y: 1.0} padding: Inset{left: 10 bottom: 10}
                                RoundedView{
                                    width: Fit height: Fit new_batch: true padding: Inset{left: 6 right: 6 top: 3 bottom: 3}
                                    draw_bg.color: th_glass_panel draw_bg.border_radius: 4.0
                                    flow_hint := Label{text: "" draw_text.color: muted draw_text.text_style.font_size: 9}
                                }
                            }
                            View{
                                width: Fill height: Fill flow: Right padding: Inset{left: 12 right: 12 top: 50 bottom: 12}
                                Filler{}
                                flow_popup := RoundedView{
                                    width: 400 height: Fill flow: Down new_batch: true visible: false
                                    draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line_strong
                                    View{
                                        width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                        padding: Inset{left: 14 right: 8 top: 10 bottom: 2}
                                        flow_popup_title := Label{width: Fill text: "" max_lines: 1 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 12}}
                                        flow_popup_chat := SegOff{text: "Open in chat" height: 24}
                                        flow_popup_delete := SegOff{text: "Delete" height: 24}
                                        flow_popup_close := SmallButton{text: "x"}
                                    }
                                    View{
                                        width: Fill height: Fit padding: Inset{left: 14 right: 14 bottom: 6}
                                        flow_popup_info := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                                    }
                                    // Its tabs: messages, budget, specs.
                                    flow_tabs := View{
                                        width: Fill height: Fit flow: Right spacing: 4 align: Align{y: 0.5}
                                        padding: Inset{left: 12 right: 12 bottom: 8}
                                        ft_msgs_on := SegOn{text: "Messages" height: 24}
                                        ft_msgs := SegOff{text: "Messages" height: 24 visible: false}
                                        ft_budget_on := SegOn{text: "Budget" height: 24 visible: false}
                                        ft_budget := SegOff{text: "Budget" height: 24}
                                        ft_specs_on := SegOn{text: "Specs" height: 24 visible: false}
                                        ft_specs := SegOff{text: "Specs" height: 24}
                                        Filler{}
                                        ft_note := Label{text: "" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
                                    }
                                    flow_budget_pane := View{
                                        width: Fill height: Fill flow: Down spacing: 12 visible: false
                                        SolidView{width: Fill height: 1 draw_bg.color: line}
                                        View{
                                            width: Fill height: Fit padding: Inset{left: 14 right: 14 top: 2}
                                            flow_budget_text := Label{width: Fill text: "" draw_text.color: ink draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                                        }
                                        // An inner loop's limit: its steps (model calls) and cost.
                                        flow_budget := View{
                                            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5} visible: false
                                            padding: Inset{left: 14 right: 14}
                                            flow_budget_label := Label{text: "Budget" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 9}
                                            flow_budget_steps := InputStyle{width: 80 height: 28 empty_text: "steps"}
                                            flow_budget_cost := InputStyle{width: 80 height: 28 empty_text: "$"}
                                            flow_budget_set := SegOff{text: "Set" height: 26}
                                            flow_budget_clear := SegOff{text: "No limit" height: 26}
                                        }
                                        View{
                                            width: Fill height: Fit padding: Inset{left: 14 right: 14}
                                            flow_budget_note := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                        }
                                    }
                                    // Every task file its slices were given; one opens to its text.
                                    flow_specs_pane := View{
                                        width: Fill height: Fill flow: Down visible: false
                                        SolidView{width: Fill height: 1 draw_bg.color: line}
                                        View{
                                            width: Fill height: Fit padding: Inset{left: 14 right: 14 top: 8 bottom: 6}
                                            flow_specs_summary := Label{width: Fill text: "" draw_text.color: ink draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                                        }
                                        flow_spec_list := PortalList{
                                            width: Fill height: Fill padding: Inset{left: 12 right: 12 bottom: 10}
                                            SpecRow := SpecRowView{}
                                        }
                                    }
                                    // A new peer agent: its name, role, model, outer loop, first task.
                                    flow_create := View{
                                        width: Fill height: Fit flow: Down spacing: 8 visible: false
                                        padding: Inset{left: 14 right: 14 top: 4 bottom: 8}
                                        View{
                                            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                            new_name_label := Label{text: "Name" width: 64 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 9}
                                            new_name := InputStyle{width: Fill height: 28 empty_text: "short-kebab-name"}
                                        }
                                        View{
                                            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                            new_role_label := Label{text: "Role" width: 64 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 9}
                                            new_role := InputStyle{width: Fill height: 28 empty_text: "developer"}
                                        }
                                        new_model_label := Label{text: "Model" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 9}
                                        View{
                                            width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 6
                                            nm0 := SegOff{text: "" height: 24}
                                            nm1 := SegOff{text: "" height: 24 visible: false}
                                            nm2 := SegOff{text: "" height: 24 visible: false}
                                            nm3 := SegOff{text: "" height: 24 visible: false}
                                        }
                                        new_outer_label := Label{text: "Outer loop" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 9}
                                        View{
                                            width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 6
                                            no0 := SegOff{text: "" height: 24 visible: false}
                                            no1 := SegOff{text: "" height: 24 visible: false}
                                            no2 := SegOff{text: "" height: 24 visible: false}
                                            no3 := SegOff{text: "" height: 24 visible: false}
                                            no_none := SegOff{text: "None" height: 24}
                                        }
                                        new_task := InputStyle{width: Fill height: 72 is_multiline: true empty_text: "Its first task (optional: you can message it later)"}
                                        View{
                                            width: Fill height: Fit flow: Right align: Align{y: 0.5}
                                            new_note := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                            new_create := SendButton{text: "Create"}
                                        }
                                    }
                                    flow_chat_rule := SolidView{width: Fill height: 1 draw_bg.color: line}
                                    // In a View of its own: a PortalList hidden directly still takes presses.
                                    flow_chat_pane := View{
                                        width: Fill height: Fill
                                        flow_chat := ChatList{padding: Inset{left: 12 right: 12 top: 4 bottom: 4}}
                                    }
                                    flow_compose := View{
                                        width: Fill height: Fit flow: Down spacing: 8
                                        padding: Inset{left: 12 right: 12 top: 6 bottom: 12}
                                        flow_queue := QueuePanel{}
                                        flow_input := InputStyle{width: Fill height: 60 is_multiline: true submit_on_enter: true}
                                        View{
                                            width: Fill height: Fit flow: Right align: Align{y: 0.5}
                                            flow_popup_note := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                            flow_send := SendButton{}
                                        }
                                    }
                                }
                            }
                        }
                        // The outer loop's own terminal UI (the native TUI plugin).
                        tui_view := View{
                            width: Fill height: Fill flow: Down spacing: 6 visible: false
                            padding: Inset{left: 14 right: 14 top: 50 bottom: 12}
                            View{
                                width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                                tui_title := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 10}}
                                tui_restart := SegOff{text: "Reconnect" height: 26}
                            }
                            // Its own surface (the desktop's terminal colours), framed lightly.
                            cli_term := AgentTerm{}
                            tui_note := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                        }
                        // The session's run on a time axis, to replay.
                        time_view := View{
                            width: Fill height: Fill flow: Down spacing: 8 visible: false
                            padding: Inset{left: 16 right: 16 top: 50 bottom: 14}
                            View{
                                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                time_play := SegOn{text: "Play"}
                                time_rewind := SegOff{text: "Start"}
                                time_end := SegOff{text: "Now"}
                                time_speed := SegOff{text: "20×"}
                                time_squeeze := SegOff{text: "Squeeze idle"}
                                Filler{}
                                time_clock := Label{text: "" padding: 0 draw_text.color: th_danger draw_text.text_style: theme.font_code{font_size: 9}}
                            }
                            time_summary := Label{width: Fill text: "" max_lines: 2 padding: 0 draw_text.color: th_muted_strong draw_text.text_style.font_size: 9}
                            RoundedView{
                                width: Fill height: Fill new_batch: true padding: 1
                                draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
                                timeline := TimelineCanvas{}
                            }
                            time_legend := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8}
                            View{
                                width: Fill height: 180 flow: Right spacing: 10
                                RoundedView{
                                    width: Fill height: Fill flow: Down spacing: 5 new_batch: true padding: 10
                                    draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
                                    time_now_title := Label{text: "" padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9}}
                                    time_now := Label{width: Fill text: "" padding: 0 draw_text.color: th_muted_strong draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                }
                                RoundedView{
                                    width: Fill height: Fill flow: Down spacing: 5 new_batch: true padding: 10
                                    draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
                                    time_events_title := Label{text: "" padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9}}
                                    time_events := Label{width: Fill text: "" padding: 0 draw_text.color: th_muted_strong draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                }
                                // What was done again, and why: for finding where the loops lose time.
                                RoundedView{
                                    width: Fill height: Fill flow: Down spacing: 5 new_batch: true padding: 10
                                    draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
                                    View{
                                        width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                        time_rework_title := Label{width: Fill text: "" padding: 0 draw_text.color: th_warning draw_text.text_style: theme.font_bold{font_size: 9}}
                                        time_evolve := SegOff{text: "Learn from it" height: 22 visible: false}
                                    }
                                    ScrollYView{
                                        width: Fill height: Fill
                                        time_rework := Label{width: Fill text: "" padding: 0 draw_text.color: th_muted_strong draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                    }
                                }
                                RoundedView{
                                    width: Fill height: Fill flow: Down spacing: 5 new_batch: true padding: 10
                                    draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
                                    View{
                                        width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                        time_sel_title := Label{width: Fill text: "" padding: 0 draw_text.color: ink draw_text.wrap: Words draw_text.text_style: theme.font_bold{font_size: 9}}
                                        time_open := SegOff{text: "Open" height: 22 visible: false}
                                    }
                                    time_sel := Label{width: Fill text: "" padding: 0 draw_text.color: th_muted_strong draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                }
                            }
                        }
                        // The views at the left (they change what the stage shows);
                        // at the right, the outer and inner loops, the app.
                        SolidView{
                            width: Fill height: Fit flow: Right new_batch: true padding: Inset{top: 6 left: 14 right: 14 bottom: 6}
                            draw_bg.color: th_glass
                            View{
                                width: Fit height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                // The sidebar, folded away: opened again here.
                                sidebar_open := IconButton{visible: false icon := Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
                            Path{d: "M4 5h16a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1zM9 5v14" fill: false stroke: th_muted_strong stroke_width: 1.6 stroke_linejoin: "round"}
                        }}
                                view_chat := FloatChat{}
                                view_chat_on := FloatChat{visible: false draw_bg.color: th_accent_soft draw_bg.border_color: th_accent_line draw_bg.border_size: 1.0}
                                view_flow := FloatFlow{}
                                view_flow_on := FloatFlow{visible: false draw_bg.color: th_accent_soft draw_bg.border_color: th_accent_line draw_bg.border_size: 1.0}
                                view_time := FloatTime{}
                                view_time_on := FloatTime{visible: false draw_bg.color: th_accent_soft draw_bg.border_color: th_accent_line draw_bg.border_size: 1.0}
                                // The native TUI plugin: the outer loop's own CLI.
                                view_tui := FloatTui{visible: false}
                                view_tui_on := FloatTui{visible: false draw_bg.color: th_accent_soft draw_bg.border_color: th_accent_line draw_bg.border_size: 1.0}
                            }
                            Filler{}
                            View{
                                width: Fit height: Fit flow: Right spacing: 6
                                outer_btn := FloatOuter{}
                                outer_btn_on := FloatOuter{visible: false draw_bg.color: th_accent_soft draw_bg.border_color: th_accent_line draw_bg.border_size: 1.0}
                                inner_btn := FloatInner{}
                                inner_btn_on := FloatInner{visible: false draw_bg.color: th_accent_soft draw_bg.border_color: th_accent_line draw_bg.border_size: 1.0}
                                new_peer_btn := FloatInner{visible: false}
                                data_btn := FloatData{visible: false}
                                preview_btn := FloatApp{visible: false}
                                preview_btn_on := FloatApp{visible: false draw_bg.color: th_success_bg draw_bg.border_color: th_success_line draw_bg.border_size: 1.0}
                                publish_btn := FloatPublish{visible: false}
                                plug_btn0 := FloatPlugin{visible: false}
                                plug_btn1 := FloatPlugin{visible: false}
                                plug_btn2 := FloatPlugin{visible: false}
                            }
                        }
                        // Under the floating buttons, by the Data one: what the app is built from.
                        View{
                            width: Fill height: Fit flow: Right padding: Inset{top: 46 right: 14}
                            Filler{}
                            // Data the app is built from: a CSV file, or a JSON API.
                            data_panel := RoundedView{
                                width: 440 height: Fit flow: Down spacing: 8 visible: false new_batch: true
                                padding: Inset{left: 12 right: 12 top: 10 bottom: 10}
                                draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
                                View{
                                    width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                    DataIcon{}
                                    data_title := Label{text: "Data for the app" padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9}}
                                }
                                // What the app already has: one line per source.
                                data_known := Label{width: Fill text: "" padding: 0 draw_text.color: th_muted_strong draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                View{
                                    width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                                    data_csv := SegOff{text: "Choose a CSV file…" height: 28}
                                    data_url := InputStyle{width: Fill height: 30 empty_text: "https://… (a JSON API)"}
                                    data_add_api := SegOff{text: "Add API" height: 28}
                                }
                                data_note := Label{width: Fill text: "" visible: false draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                            }
                        }
                        // The outer loop's round, floating over the conversation and the
                        // graph: the person drags it by its header.
                        plan_panel := RoundedView{
                            width: 340 height: Fit flow: Down visible: false new_batch: true
                            draw_bg.color: th_glass draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line_strong
                            plan_head := View{
                                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                padding: Inset{left: 10 right: 6 top: 6 bottom: 6}
                                cursor: MouseCursor.Move grab_key_focus: false
                                plan_title := Label{text: "" padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9}}
                                plan_progress := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8}
                                plan_fold := SmallButton{text: "–"}
                                plan_close := SmallButton{text: "x"}
                            }
                            plan_body := View{
                                width: Fill height: Fit flow: Down spacing: 7 padding: Inset{left: 10 right: 10 bottom: 10}
                                plan_intent := Label{width: Fill text: "" padding: 0 draw_text.color: th_muted_strong draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                SolidView{width: Fill height: 1 draw_bg.color: th_hover}
                                pr0 := PlanRow{}
                                pr1 := PlanRow{}
                                pr2 := PlanRow{}
                                pr3 := PlanRow{}
                                pr4 := PlanRow{}
                                pr5 := PlanRow{}
                                pr6 := PlanRow{}
                                pr7 := PlanRow{}
                                plan_more := Label{text: "" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8}
                            }
                        }
                        // A flow card's (or the canvas's) right-click menu, at the
                        // pointer: last, over the floating buttons too.
                        flow_menu := RoundedView{
                            width: 236 height: Fit flow: Down visible: false new_batch: true
                            padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
                            draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line_strong
                            menu_title := Label{width: Fill text: "" max_lines: 1 padding: Inset{left: 8 right: 8 top: 4 bottom: 4} draw_text.color: muted draw_text.text_style.font_size: 8}
                            m_new_peer := MenuItem{}
                            m_open := MenuItem{}
                            m_chat := MenuItem{}
                            m_budget := MenuItem{}
                            m_new_outer := MenuItem{}
                            m_new_free := MenuItem{}
                            m_delete := MenuItem{draw_text +: {color: th_danger color_hover: th_danger_strong color_focus: th_danger}}
                        }
                      }
                        composer_area := View{
                            width: Fill height: Fit flow: Down spacing: 8
                            padding: Inset{left: 16 right: 16 top: 8 bottom: 12}
                            outer_queue := QueuePanel{}
                            composer := InputStyle{
                                width: Fill height: 76
                                is_multiline: true submit_on_enter: true
                                empty_text: "Message the outer loop  (Enter to send, Shift+Enter for a new line)"
                            }
                            View{
                              width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                              View{
                                width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
                                // What runs the outer loop: a click lists the engines and models.
                                outer_pick := RoundedView{
                                    width: Fit height: 28 new_batch: true flow: Right spacing: 6 align: Align{y: 0.5}
                                    padding: Inset{left: 8 right: 8}
                                    cursor: MouseCursor.Hand grab_key_focus: false
                                    draw_bg.color: th_sidebar draw_bg.border_radius: 4.0
                                    outer_icon := View{width: Fit height: Fit ModelIcon{}}
                                    outer_icon_octos := View{width: Fit height: Fit visible: false ModelIcon{draw_svg +: {svg: crate_resource("self:resources/octos.svg")}}}
                                    outer_icon_codex := View{width: Fit height: Fit visible: false ModelIcon{draw_svg +: {svg: crate_resource("self:resources/codex.svg")}}}
                                    outer_icon_pi := View{width: Fit height: Fit visible: false ModelIcon{draw_svg +: {svg: crate_resource("self:resources/pi.svg")}}}
                                    outer_model := ModelText{}
                                    Vector{width: 10 height: 10 viewbox: vec4(0 0 24 24)
                                        Path{d: "M6 9l6 6 6-6" fill: false stroke: th_muted stroke_width: 2.5 stroke_linecap: "round" stroke_linejoin: "round"}
                                    }
                                }
                                use_worktree := CheckBox{
                                    text: "Use a git worktree"
                                    draw_text +: {
                                        color: th_muted_strong color_hover: th_ink color_down: th_ink
                                        color_focus: th_muted_strong color_active: th_ink
                                        text_style: theme.font_regular{font_size: 9}
                                    }
                                }
                              }
                                stop := ButtonFlat{
                                    text: "Stop" width: Fit height: 30 visible: false
                                    padding: Inset{left: 12 right: 12}
                                    draw_bg +: {
                                        color: th_danger_bg color_hover: th_danger_bg_hover color_down: th_danger_bg_down color_focus: th_danger_bg
                                        border_size: 0.0 border_radius: 3.0
                                    }
                                    draw_text +: {color: th_danger color_hover: th_danger color_down: th_danger color_focus: th_danger}
                                }
                                // At work, something typed: its turn (and its subagents) cut off,
                                // the message its next turn; the inner loops go on.
                                interrupt_send := ButtonFlat{
                                    text: "Interrupt & send" width: Fit height: 30 visible: false
                                    padding: Inset{left: 12 right: 12}
                                    draw_bg +: {
                                        color: th_warning_bg color_hover: th_hover color_down: th_line color_focus: th_warning_bg
                                        border_size: 0.0 border_radius: 3.0
                                    }
                                    draw_text +: {color: th_warning color_hover: th_warning color_down: th_warning color_focus: th_warning}
                                }
                                // After a stop: what waited, or the turn cut off, goes on.
                                go_on := ButtonFlat{
                                    text: "Go on" width: Fit height: 30 visible: false
                                    padding: Inset{left: 12 right: 12}
                                    draw_bg +: {
                                        color: th_accent_soft color_hover: th_accent_selected color_down: th_accent_selected color_focus: th_accent_soft
                                        border_size: 0.0 border_radius: 3.0
                                    }
                                    draw_text +: {color: th_accent_ink color_hover: th_accent_ink color_down: th_accent_ink color_focus: th_accent_ink}
                                }
                                send := SendButton{}
                            }
                        }
                    }

                    // An OctoSense app, running from the project's files.
                    preview_divider := View{width: Fit height: Fill visible: false preview_handle := ResizeHandle{}}
                    preview_panel := SolidView{
                        width: 420 height: Fill flow: Down new_batch: true visible: false
                        draw_bg.color: th_panel
                        View{
                            width: Fill height: Fit flow: Right spacing: 4 align: Align{y: 0.5}
                            padding: Inset{left: 16 right: 10 top: 14 bottom: 4}
                            preview_title := Label{text: "Preview" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 13}}
                            Filler{}
                            reload_preview := SegOff{text: "Reload" height: 24}
                            close_preview := SmallButton{text: "x"}
                        }
                        // The workbench's tabs: the app running, its files, its permissions, App Hub.
                        View{
                            width: Fill height: Fit flow: Right spacing: 4 padding: Inset{left: 16 right: 16 bottom: 8}
                            wb_t0 := SegOff{text: "Preview"} wb_t0_on := SegOn{text: "Preview" visible: false}
                            wb_t1 := SegOff{text: "Files"} wb_t1_on := SegOn{text: "Files" visible: false}
                            wb_t2 := SegOff{text: "Permissions"} wb_t2_on := SegOn{text: "Permissions" visible: false}
                            wb_t3 := SegOff{text: "App Hub"} wb_t3_on := SegOn{text: "App Hub" visible: false}
                        }
                        wb_preview := View{
                        width: Fill height: Fill flow: Down
                        View{
                            width: Fill height: Fit padding: Inset{left: 16 right: 16 bottom: 8}
                            preview_info := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                        }
                        preview_problem := View{
                            width: Fill height: Fit flow: Down spacing: 6 visible: false
                            padding: Inset{left: 16 right: 16 bottom: 8}
                            RoundedView{
                                width: Fill height: Fit flow: Down spacing: 6 new_batch: true
                                padding: Inset{left: 10 right: 10 top: 8 bottom: 8}
                                draw_bg.color: th_danger_bg draw_bg.border_radius: 6.0
                                preview_problem_title := Label{width: Fill text: "" draw_text.color: th_danger draw_text.wrap: Words draw_text.text_style: theme.font_bold{font_size: 9}}
                                ScrollYView{
                                    width: Fill height: 110 flow: Down
                                    preview_errors := Label{width: Fill text: "" draw_text.color: th_danger_strong draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 8.5}}
                                }
                                preview_fix := SegOff{text: "Ask the outer loop to fix it" height: 24}
                            }
                        }
                        SolidView{width: Fill height: 1 draw_bg.color: line}
                        SolidView{
                            width: Fill height: Fill new_batch: true
                            draw_bg.color: th_raised
                            preview := Splash{width: Fill height: Fill}
                        }
                        }
                        // Its files: the project's tree, then the file picked.
                        wb_files := View{
                            width: Fill height: Fill flow: Down visible: false
                            View{
                                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5} padding: Inset{left: 16 right: 16 bottom: 6}
                                files_root := Label{width: Fill text: "" max_lines: 1 draw_text.color: muted draw_text.text_style: theme.font_code{font_size: 8.5}}
                                files_reveal := SegOff{text: "Finder"}
                            }
                            View{
                                width: Fill height: 230 padding: Inset{left: 8 right: 8}
                                files_list := PortalList{
                                    width: Fill height: Fill
                                    File := FileItem{}
                                }
                            }
                            SolidView{width: Fill height: 1 draw_bg.color: line}
                            View{
                                width: Fill height: Fit padding: Inset{left: 16 right: 16 top: 8 bottom: 6}
                                source_path := Label{width: Fill text: "" max_lines: 1 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
                            }
                            SolidView{
                                width: Fill height: Fill new_batch: true draw_bg.color: th_raised
                                ScrollXYView{
                                    width: Fill height: Fill flow: Down padding: Inset{left: 12 right: 12 top: 8 bottom: 8}
                                    source_text := Label{width: Fit text: "" draw_text.color: th_ink2 draw_text.text_style: theme.font_code{font_size: 8.5}}
                                }
                            }
                        }
                        // Its permissions: what its manifest asks the person for.
                        wb_perms := ScrollYView{
                            width: Fill height: Fill flow: Down spacing: 4 visible: false
                            padding: Inset{left: 16 right: 16 bottom: 16}
                            perms_hint := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                            cap_storage := CapRow{} cap_net := CapRow{} cap_images := CapRow{} cap_web := CapRow{}
                            cap_camera := CapRow{} cap_microphone := CapRow{} cap_library := CapRow{} cap_location := CapRow{} cap_mail := CapRow{}
                            perms_other := Label{width: Fill text: "" draw_text.color: th_warning draw_text.wrap: Words draw_text.text_style.font_size: 9}
                            perms_hosts_label := Label{width: Fill text: "" margin: Inset{top: 8} draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
                            perms_hosts := InputStyle{width: Fill height: Fit empty_text: "api.example.org, cdn.example.org"}
                            perms_storage_label := Label{width: Fill text: "" margin: Inset{top: 8} draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 9.5}}
                            perms_storage := InputStyle{width: 160 height: Fit empty_text: "16"}
                            View{
                                width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5} margin: Inset{top: 10}
                                perms_save := SegOn{text: "Save"}
                                perms_status := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                            }
                        }
                        // App Hub: its listing and the submission (on the host), or the local publish (in OctoSense).
                        wb_hub := ScrollYView{
                            width: Fill height: Fill flow: Down spacing: 4 visible: false
                            padding: Inset{left: 16 right: 16 bottom: 24}
                            hub_hint := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                            // Inside OctoSense: to this device's App Hub, to install.
                            hub_local := View{
                                width: Fill height: Fit flow: Down spacing: 8 visible: false
                                hub_publish := SegOn{text: "Publish to this device's App Hub"}
                            }
                            // On the host: what the store shows, who publishes it, the submission.
                            hub_form := View{
                                width: Fill height: Fit flow: Down spacing: 4
                                hub_sec_app := HubSection{}
                                hub_name_l := HubLabel{} hub_name := InputStyle{width: Fill height: Fit}
                                View{
                                    width: Fill height: Fit flow: Right spacing: 10
                                    View{width: 110 height: Fit flow: Down spacing: 4 hub_version_l := HubLabel{} hub_version := InputStyle{width: Fill height: Fit empty_text: "1.0.0"}}
                                    View{width: Fill height: Fit flow: Down spacing: 4 hub_category_l := HubLabel{} hub_category := HubDrop{width: Fill labels: ["productivity" "utilities" "photo-video" "news" "weather" "travel" "finance" "health" "education" "entertainment" "games" "social" "shopping" "lifestyle" "developer"]}}
                                }
                                hub_subtitle_l := HubLabel{} hub_subtitle := InputStyle{width: Fill height: Fit}
                                hub_description_l := HubLabel{} hub_description := InputStyle{width: Fill height: 96 is_multiline: true}
                                hub_keywords_l := HubLabel{} hub_keywords := InputStyle{width: Fill height: Fit}
                                View{
                                    width: Fill height: Fit flow: Right spacing: 10
                                    View{width: 110 height: Fit flow: Down spacing: 4 hub_age_l := HubLabel{} hub_age := HubDrop{width: Fill labels: ["all" "12+" "16+" "18+"]}}
                                    View{width: Fill height: Fit flow: Down spacing: 4 hub_license_l := HubLabel{} hub_license := InputStyle{width: Fill height: Fit empty_text: "Apache-2.0"}}
                                }
                                hub_notes_l := HubLabel{} hub_notes := InputStyle{width: Fill height: Fit}
                                hub_platforms_l := HubLabel{}
                                View{
                                    width: Fill height: Fit flow: Right spacing: 10
                                    plat_macos := HubCheck{text: "macOS"} plat_ios := HubCheck{text: "iOS"} plat_android := HubCheck{text: "Android"} plat_windows := HubCheck{text: "Windows"}
                                }
                                View{
                                    width: Fill height: Fit flow: Right spacing: 10
                                    plat_linux := HubCheck{text: "Linux"} plat_openharmony := HubCheck{text: "OpenHarmony"} plat_web := HubCheck{text: "Web"}
                                }
                                hub_sec_pub := HubSection{}
                                hub_pub_name_l := HubLabel{} hub_pub_name := InputStyle{width: Fill height: Fit}
                                hub_support_l := HubLabel{} hub_support := InputStyle{width: Fill height: Fit empty_text: "mailto:… or https://…"}
                                hub_privacy_l := HubLabel{} hub_privacy := InputStyle{width: Fill height: Fit empty_text: "https://…"}
                                hub_sign := HubCheck{text: "Sign it"}
                                hub_pub_id_l := HubLabel{} hub_pub_id := InputStyle{width: Fill height: Fit empty_text: "your-publisher-id"}
                                View{
                                    width: Fill height: Fit flow: Right spacing: 6 margin: Inset{top: 8}
                                    hub_save := SegOff{text: "Save"}
                                    hub_prepare := SegOn{text: "Screenshots & check"}
                                }
                                hub_check_out := Label{width: Fill text: "" draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 8}}
                                hub_sec_review := HubSection{}
                                hub_questions := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                                hub_answers := InputStyle{width: Fill height: 150 is_multiline: true empty_text: "Answer each review question truthfully, citing the source"}
                                View{
                                    width: Fill height: Fit flow: Right spacing: 6
                                    hub_draft := SegOff{text: "Ask the outer loop to draft them"}
                                    hub_answers_save := SegOff{text: "Save the answers"}
                                }
                                hub_sec_submit := HubSection{}
                                hub_submit := SegOn{text: "Submit for review…"}
                                hub_plan := Label{width: Fill text: "" draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 8}}
                                hub_confirm_row := View{
                                    width: Fill height: Fit flow: Right spacing: 6 visible: false
                                    hub_confirm := SegOn{text: "Submit"}
                                    hub_cancel := SegOff{text: "Cancel"}
                                }
                            }
                            hub_status := Label{width: Fill text: "" margin: Inset{top: 6} draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                        }
                    }

                    inner_divider := View{width: Fit height: Fill visible: false inner_handle := ResizeHandle{}}
                    inner_panel := SolidView{
                        width: 360 height: Fill flow: Down new_batch: true visible: false
                        draw_bg.color: th_panel
                        View{
                            width: Fill height: Fit flow: Right align: Align{y: 0.5}
                            padding: Inset{left: 16 right: 10 top: 14 bottom: 4}
                            inner_title := Label{text: "Inner" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 13}}
                            Filler{}
                            // The native TUI plugin: this inner loop's own CLI.
                            inner_tui := SegOff{text: "TUI" height: 24 visible: false}
                            inner_tui_on := SegOn{text: "TUI" height: 24 visible: false}
                            close_inner := SmallButton{text: "x"}
                        }
                        View{
                            width: Fill height: Fit padding: Inset{left: 16 right: 16 bottom: 8}
                            inner_summary := Label{width: Fill text: "" draw_text.color: muted draw_text.text_style.font_size: 9.5}
                        }
                        View{
                            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                            padding: Inset{left: 16 right: 16 bottom: 8}
                            approvals_label := Label{text: "Approvals" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 9}
                            approvals_ask_on := SegOn{text: "Ask" height: 22}
                            approvals_ask := SegOff{text: "Ask" height: 22}
                            approvals_auto_on := SegOn{text: "Auto" height: 22}
                            approvals_auto := SegOff{text: "Auto" height: 22}
                            approvals_hint := Label{width: Fill text: "" padding: 0 max_lines: 1 draw_text.color: muted draw_text.text_style.font_size: 8.5}
                        }
                        tabs := PortalList{
                            width: Fill height: 62 flow: Right
                            padding: Inset{left: 10 right: 10 top: 0 bottom: 8}
                            Queued := PeerTab{}
                            Running := TabRunning{}
                            Done := TabDone{}
                            Failed := TabFailed{}
                            Halted := TabHalted{}
                            QueuedSel := TabQueuedSel{}
                            RunningSel := TabRunningSel{}
                            DoneSel := TabDoneSel{}
                            FailedSel := TabFailedSel{}
                            HaltedSel := TabHaltedSel{}
                        }
                        SolidView{width: Fill height: 1 draw_bg.color: line}
                        peers_empty := View{
                            width: Fill height: Fit padding: 16
                            peers_empty_text := Label{width: Fill text: "No inner loops yet. They start when the outer loop sends a plan." draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 10}
                        }
                        peer_detail := View{
                            width: Fill height: Fill flow: Down
                            View{
                                width: Fill height: Fit flow: Down spacing: 4
                                padding: Inset{left: 16 right: 16 top: 10 bottom: 8}
                                View{
                                    width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                                    d_title := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11.5}}
                                    close_peer := SegOff{text: "Close" height: 22 padding: Inset{left: 8 right: 8}}
                                    b_queued := Badge{label.text: "queued"}
                                    b_running := Badge{draw_bg.color: th_accent_soft label +: {text: "running" draw_text.color: th_accent}}
                                    b_done := Badge{draw_bg.color: th_success_bg label +: {text: "done" draw_text.color: th_success}}
                                    b_failed := Badge{draw_bg.color: th_danger_bg label +: {text: "failed" draw_text.color: th_danger}}
                                    b_halted := Badge{draw_bg.color: th_warning_bg label +: {draw_text.color: th_warning}}
                                }
                                d_meta := Label{width: Fill text: "" padding: 0 draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
                                // What the flow canvas does to a card, here too: its budget, and handing it over.
                                View{
                                    width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5} margin: Inset{top: 2}
                                    d_budget_label := Label{text: "Budget" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
                                    d_budget_steps := InputStyle{width: 64 height: 26 empty_text: "steps"}
                                    d_budget_cost := InputStyle{width: 64 height: 26 empty_text: "$"}
                                    d_budget_set := SegOff{text: "Set" height: 24 padding: Inset{left: 8 right: 8}}
                                    Filler{}
                                    d_move := SegOff{text: "Hand over" height: 24 padding: Inset{left: 8 right: 8}}
                                }
                                d_move_row := View{
                                    width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 6 align: Align{y: 0.5} visible: false
                                    d_move_label := Label{text: "To outer loop:" padding: 0 draw_text.color: muted draw_text.text_style.font_size: 8.5}
                                    move_t0 := SegOff{text: "" height: 24 visible: false}
                                    move_t1 := SegOff{text: "" height: 24 visible: false}
                                    move_t2 := SegOff{text: "" height: 24 visible: false}
                                    move_t3 := SegOff{text: "" height: 24 visible: false}
                                    move_new := SegOff{text: "New outer loop" height: 24}
                                }
                                d_activity_row := View{
                                    width: Fill height: Fit
                                    d_activity := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: accent draw_text.text_style.font_size: 9}
                                }
                                d_changes_row := RoundedView{
                                    width: Fill height: Fit flow: Down spacing: 6 visible: false new_batch: true
                                    margin: Inset{top: 4}
                                    padding: Inset{left: 10 right: 10 top: 8 bottom: 8}
                                    draw_bg.color: th_raised draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_hover
                                    View{
                                        width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                        d_summary := Label{width: Fill text: "" padding: 0 draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9}
                                        toggle_details := SegOff{text: "Details" height: 22 padding: Inset{left: 8 right: 8}}
                                    }
                                    d_details := View{
                                        width: Fill height: Fit flow: Down spacing: 6 visible: false
                                        d_verdict := Label{width: Fill text: "" padding: 0 draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9}
                                        d_changes := Label{width: Fill text: "" padding: 0 draw_text.color: ink draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 8}}
                                        d_actions := View{
                                            width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 6 align: Align{y: 0.5}
                                            recheck := SegOff{text: "Run checks" height: 24}
                                            toggle_diff := SegOff{text: "Show diff" height: 24}
                                        }
                                        d_patch_row := ScrollYView{
                                            width: Fill height: 220 visible: false
                                            d_patch := Label{width: Fill text: "" draw_text.color: th_ink draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 8}}
                                        }
                                    }
                                }
                            }
                            SolidView{width: Fill height: 1 draw_bg.color: th_hover}
                            peer_messages_pane := View{
                                width: Fill height: Fill
                                peer_messages := ChatList{padding: Inset{left: 12 right: 12 top: 4 bottom: 8}}
                            }
                            inner_term_pane := View{
                                width: Fill height: Fill visible: false margin: Inset{left: 8 right: 8 bottom: 8} flow: Down spacing: 6
                                // The live view's: what it is, and taking it over when its turn is done.
                                inner_live_bar := View{
                                    width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5} visible: false
                                    inner_live_title := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_ink2 draw_text.text_style.font_size: 9}
                                    inner_take := SegOff{text: "Take over" height: 24}
                                }
                                inner_term := AgentTerm{}
                            }
                        }
                        d_question_row := SolidView{
                            width: Fill height: Fit flow: Down visible: false new_batch: true
                            padding: Inset{left: 16 right: 16 top: 10 bottom: 10}
                            draw_bg.color: th_accent_soft
                            d_question := Label{width: Fill text: "" draw_text.color: th_accent_ink draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                        }
                        d_approval_row := SolidView{
                            width: Fill height: Fit flow: Down spacing: 6 visible: false new_batch: true
                            padding: Inset{left: 12 right: 12 top: 8 bottom: 8}
                            draw_bg.color: th_warning_bg
                            d_approval_title := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: th_ink2 draw_text.text_style: theme.font_bold{font_size: 9.5}}
                            View{
                                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                approve := ButtonFlat{
                                    text: "Approve" width: Fit height: 24 padding: Inset{left: 10 right: 10}
                                    draw_bg +: {color: th_success color_hover: th_success_strong color_down: th_success_strong color_focus: th_success border_size: 0.0 border_radius: 6.0}
                                    draw_text +: {color: th_on_accent color_hover: th_on_accent color_down: th_on_accent color_focus: th_on_accent}
                                }
                                approve_session := SegOff{text: "Always in this session" height: 24}
                                deny := SegOff{text: "Deny" height: 24}
                            }
                            // The command can be long: it scrolls, the buttons stay in sight.
                            ScrollYView{
                                width: Fill height: 96
                                d_approval := Label{width: Fill text: "" padding: 0 draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 8}}
                            }
                        }
                        peer_composer := View{
                            width: Fill height: Fit flow: Down spacing: 8 visible: false
                            padding: Inset{left: 12 right: 12 top: 8 bottom: 12}
                            inner_queue := QueuePanel{}
                            peer_input := InputStyle{
                                width: Fill height: 64
                                is_multiline: true submit_on_enter: true
                                empty_text: "Message this inner loop (queued if it is busy)"
                            }
                            View{
                              width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                              View{
                                width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 6 align: Align{y: 0.5}
                                View{
                                    width: Fit height: Fit flow: Right spacing: 5 align: Align{y: 0.5}
                                    ModelIcon{draw_svg +: {svg: crate_resource("self:resources/octos.svg")}}
                                    inner_agent := ModelText{}
                                }
                                View{
                                    width: Fit height: Fit flow: Right spacing: 5 align: Align{y: 0.5}
                                    inner_model_icon := ProviderIcon{}
                                    inner_model := ModelText{}
                                }
                              }
                                peer_interrupt := SegOff{text: "Interrupt & send" height: 30}
                                peer_send := SendButton{}
                            }
                        }
                    }
                }
            }
        }

        live_page := SolidView{
            width: Fill height: Fill flow: Down new_batch: true visible: false
            draw_bg.color: pane_bg
            View{
                width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
                padding: Inset{left: 10 right: 16 top: 10 bottom: 10}
                live_back := IconButton{icon := Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
                    Path{d: "M15 5l-7 7 7 7" fill: false stroke: th_muted_strong stroke_width: 1.6 stroke_linecap: "round" stroke_linejoin: "round"}
                }}
                live_title := Label{text: "Live" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 14}}
            }
            SolidView{width: Fill height: 1 draw_bg.color: line}
            ScrollYView{
                width: Fill height: Fill flow: Down
                padding: Inset{left: 28 right: 28 top: 18 bottom: 22}
                live_hint := Label{width: Fill text: "" margin: Inset{bottom: 14} draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                live_requests_title := Label{text: "" visible: false margin: Inset{bottom: 8} draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
                rq0 := RequestCard{} rq1 := RequestCard{} rq2 := RequestCard{} rq3 := RequestCard{}
                live_apps_title := Label{text: "" margin: Inset{top: 4 bottom: 8} draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
                live_empty := Label{width: Fill text: "" visible: false draw_text.color: muted draw_text.text_style.font_size: 10}
                lc0 := LiveCard{} lc1 := LiveCard{} lc2 := LiveCard{} lc3 := LiveCard{}
                lc4 := LiveCard{} lc5 := LiveCard{} lc6 := LiveCard{} lc7 := LiveCard{}
            }
        }

        settings_page := SolidView{
            width: Fill height: Fill flow: Down new_batch: true visible: false
            draw_bg.color: pane_bg
            View{
                width: Fill height: Fit flow: Right spacing: 10 align: Align{y: 0.5}
                padding: Inset{left: 10 right: 16 top: 10 bottom: 10}
                // Back to the conversations: its mark only, as the other icon buttons.
                back := IconButton{icon := Vector{width: 18 height: 18 viewbox: vec4(0 0 24 24)
                    Path{d: "M15 5l-7 7 7 7" fill: false stroke: th_muted_strong stroke_width: 1.6 stroke_linecap: "round" stroke_linejoin: "round"}
                }}
                settings_title := Label{text: "Settings" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 14}}
            }
            SolidView{width: Fill height: 1 draw_bg.color: line}
            View{
                width: Fill height: Fill flow: Right
                SolidView{
                    width: 200 height: Fill flow: Down spacing: 2 new_batch: true
                    padding: Inset{left: 8 right: 8 top: 10 bottom: 10}
                    draw_bg.color: sidebar_bg
                    nav_providers_on := NavOn{text: "AI Providers"}
                    nav_providers := NavOff{text: "AI Providers"}
                    nav_plugins_on := NavOn{text: "Plugins" visible: false}
                    nav_plugins := NavOff{text: "Plugins"}
                    nav_tools_on := NavOn{text: "Tools" visible: false}
                    nav_tools := NavOff{text: "Tools"}
                    nav_language_on := NavOn{text: "Language"}
                    nav_language := NavOff{text: "Language"}
                    nav_appearance_on := NavOn{text: "Appearance" visible: false}
                    nav_appearance := NavOff{text: "Appearance"}
                    nav_about_on := NavOn{text: "About"}
                    nav_about := NavOff{text: "About"}
                }
                SolidView{width: 1 height: Fill draw_bg.color: line}
                View{
                    width: Fill height: Fill
                    providers_section := View{
                        width: Fill height: Fill flow: Down spacing: 10
                        padding: Inset{left: 28 right: 28 top: 22 bottom: 22}
                        View{
                            width: Fill height: Fit flow: Right align: Align{y: 0.5}
                            providers_title := Label{text: "AI Providers" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 17}}
                            Filler{}
                            reload_providers := ButtonFlat{
                                text: "Reload" width: Fit height: 28 padding: Inset{left: 12 right: 12}
                                draw_bg +: {color: th_sidebar color_hover: th_hover color_down: th_line color_focus: th_sidebar border_size: 0.0 border_radius: 6.0}
                                draw_text +: {color: th_ink color_hover: th_ink color_down: th_ink color_focus: th_ink}
                            }
                        }
                        providers_hint := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                        // On the host: OctoSense's AI providers found here, and where they come from.
                        own_bar := View{
                            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5} visible: false
                            Filler{}
                            add_provider := SegOn{text: "+ Add a provider"}
                        }
                        // (A list's own visibility does not take: its box's does.)
                        provider_box := View{
                            width: Fill height: Fill
                            provider_list := PortalList{
                                width: Fill height: Fill
                                Provider := ProviderRowView{}
                                OwnPrimary := OwnPrimaryRow{}
                                OwnFallback := OwnFallbackRow{}
                                Empty := ProvidersEmpty{}
                            }
                        }
                        own_status := Label{width: Fill text: "" visible: false draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                        // The add wizard: pick a provider, connect it, pick its models (as Cindy's).
                        wizard := RoundedView{
                            width: Fill height: Fill new_batch: true flow: Down spacing: 8 visible: false
                            padding: Inset{left: 16 right: 16 top: 12 bottom: 12}
                            draw_bg.color: th_panel draw_bg.border_radius: 10.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
                            View{
                                width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                                wz_title := Label{text: "" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 13}}
                                wz_steps := Label{text: "" draw_text.color: muted draw_text.text_style.font_size: 9.5}
                                Filler{}
                                wz_cancel := SegOff{text: "Cancel"}
                            }
                            wz_list := PortalList{
                                width: Fill height: Fill
                                Header := WizardHeader{}
                                Family := WizardFamily{}
                                Model := WizardModel{}
                            }
                            // Step 2: its endpoint and key.
                            wz_connect := View{
                                width: Fill height: Fit flow: Down spacing: 8 visible: false
                                wz_base_box := View{
                                    width: Fill height: Fit flow: Down spacing: 8
                                    wz_base_label := Label{text: "" draw_text.color: muted draw_text.text_style.font_size: 9.5}
                                    wz_base := InputStyle{width: Fill height: 32 empty_text: "https://…/v1"}
                                }
                                wz_key_label := Label{text: "" draw_text.color: muted draw_text.text_style.font_size: 9.5}
                                wz_key := InputStyle{width: Fill height: 32 is_password: true empty_text: "API key"}
                                wz_key_link := SegOff{text: "Get an API key…"}
                                wz_note := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                            }
                            View{
                                width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                                wz_back := SegOff{text: "Back"}
                                wz_test := SegOff{text: "Test connection"}
                                wz_status := Label{width: Fill text: "" draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                                wz_next := SegOn{text: "Next"}
                                wz_finish := SegOn{text: "Add"}
                            }
                        }
                        // The agents' own logins (a Claude or ChatGPT subscription), used when no provider is picked.
                        logins_title := SectionTitle{text: "" visible: false}
                        logins := Label{width: Fill text: "" visible: false draw_text.color: th_ink2 draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                        inner_box := View{
                            width: Fill height: Fit flow: Down spacing: 10
                            inner_model_title := SectionTitle{text: "Inner loop model"}
                            inner_source := Body{}
                        }
                    }
                    plugins_section := View{
                        width: Fill height: Fill flow: Down spacing: 10 visible: false
                        padding: Inset{left: 28 right: 28 top: 22 bottom: 22}
                        View{
                            width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                            plugins_title := Label{text: "Plugins" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 17}}
                            Filler{}
                            open_plugins := SegOff{text: "Open folder" height: 28}
                            reload_plugins := SegOff{text: "Rescan" height: 28}
                        }
                        plugins_hint := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                        plugin_list := PortalList{
                            width: Fill height: Fill
                            Plugin := PluginRowView{}
                        }
                    }
                    tools_section := View{
                        width: Fill height: Fill flow: Down spacing: 10 visible: false
                        padding: Inset{left: 28 right: 28 top: 22 bottom: 22}
                        View{
                            width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                            tools_title := Label{text: "Tools" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 17}}
                            Filler{}
                            reload_tools := SegOff{text: "Check again" height: 28}
                        }
                        tools_hint := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                        // A grid: as many cards a row as fit.
                        ScrollYView{
                            width: Fill height: Fill
                            tool_grid := View{
                                width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 12
                                tool0 := ToolCard{} tool1 := ToolCard{} tool2 := ToolCard{} tool3 := ToolCard{}
                                tool4 := ToolCard{} tool5 := ToolCard{} tool6 := ToolCard{} tool7 := ToolCard{}
                                tool8 := ToolCard{} tool9 := ToolCard{} tool10 := ToolCard{} tool11 := ToolCard{}
                            }
                        }
                    }
                    language_section := View{
                        width: Fill height: Fill flow: Down spacing: 12 visible: false
                        padding: Inset{left: 28 right: 28 top: 22 bottom: 22}
                        language_title := Label{text: "Language" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 17}}
                        language_hint := Body{text: ""}
                        View{
                            width: Fill height: Fit flow: Right spacing: 6 align: Align{y: 0.5}
                            lang_en_on := SegOn{text: "English" height: 28}
                            lang_en := SegOff{text: "English" height: 28}
                            lang_zh_on := SegOn{text: "中文" height: 28}
                            lang_zh := SegOff{text: "中文" height: 28}
                        }
                    }
                    // The theme: OctoSense's light or dark, or one of OctoBuddy's own.
                    appearance_section := ScrollYView{
                        width: Fill height: Fill flow: Down spacing: 8 visible: false
                        padding: Inset{left: 28 right: 28 top: 22 bottom: 22}
                        appearance_title := Label{text: "Appearance" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 17}}
                        appearance_hint := Body{text: ""}
                        ap0 := ThemeRow{} ap1 := ThemeRow{} ap2 := ThemeRow{} ap3 := ThemeRow{}
                        ap4 := ThemeRow{} ap5 := ThemeRow{} ap6 := ThemeRow{} ap7 := ThemeRow{}
                        ap8 := ThemeRow{} ap9 := ThemeRow{} ap10 := ThemeRow{}
                    }
                    about_section := ScrollYView{
                        width: Fill height: Fill flow: Down spacing: 10 visible: false
                        padding: Inset{left: 28 right: 28 top: 22 bottom: 22}
                        Label{text: "OctoBuddy" draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 20}}
                        about_version := Label{text: "" draw_text.color: muted draw_text.text_style.font_size: 10}
                        about_body := Body{text: "The OctoBuddy two-loop workflow as an OctoSense app. You talk to the outer loop, which plans the work, splits it into slices and reviews the results; inner loops do the slices. All of them work in the same directory: the project, or the session's git worktree. OctoBuddy commits each inner loop's own files, as you."}
                        loops_title := SectionTitle{text: "Loops"}
                        about_loops := Body{}
                        data_title := SectionTitle{text: "Data on this device"}
                        about_data := Body{}
                    }
                }
            }
        }
      }
        // What the loops run on, as Cindy picks it: the outer loop or the
        // inner loops; an agent on the left, its models on the right (an
        // agent with no model to pick takes effect when chosen).
        // Floating above the composer's engine button (`sync_picker` places it).
        outer_picker := RoundedView{
            width: 640 height: Fit flow: Down spacing: 8 visible: false new_batch: true
            padding: Inset{left: 12 right: 12 top: 10 bottom: 10}
            draw_bg.color: th_raised draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line
            View{
                width: Fill height: Fit flow: Right spacing: 4 align: Align{y: 0.5}
                pk_outer_on := SegOn{text: "Outer loop" height: 26}
                pk_outer := SegOff{text: "Outer loop" height: 26 visible: false}
                pk_inner_on := SegOn{text: "Inner loops" height: 26 visible: false}
                pk_inner := SegOff{text: "Inner loops" height: 26}
                Filler{}
                pk_current := Label{text: "" padding: 0 max_lines: 1 draw_text.color: muted draw_text.text_style.font_size: 8.5}
            }
            SolidView{width: Fill height: 1 draw_bg.color: line}
            View{
                width: Fill height: Fit flow: Right spacing: 10
                View{
                    width: 190 height: Fit flow: Down spacing: 2
                    pa_claude := AgentRow{}
                    pa_octos := AgentRow{icon := ModelIcon{draw_svg +: {svg: crate_resource("self:resources/octos.svg")}}}
                    pa_codex := AgentRow{icon := ModelIcon{draw_svg +: {svg: crate_resource("self:resources/codex.svg")}}}
                    pa_pi := AgentRow{icon := ModelIcon{draw_svg +: {svg: crate_resource("self:resources/pi.svg")}}}
                }
                SolidView{width: 1 height: 168 draw_bg.color: line}
                View{
                    width: Fill height: Fit flow: Down spacing: 2
                    pm_head := Label{text: "" padding: Inset{left: 8 bottom: 2} draw_text.color: muted draw_text.text_style.font_size: 8.5}
                    pm0 := ModelRow{} pm1 := ModelRow{} pm2 := ModelRow{} pm3 := ModelRow{}
                    pm4 := ModelRow{} pm5 := ModelRow{} pm6 := ModelRow{} pm7 := ModelRow{}
                    pm_none := Label{width: Fill text: "" padding: Inset{left: 8 top: 6} draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 9}
                    // How hard it thinks: its engine's levels (none for octos).
                    pe_row := View{
                        width: Fill height: Fit flow: Down spacing: 6 margin: Inset{top: 6} visible: false
                        SolidView{width: Fill height: 1 draw_bg.color: line}
                        pe_head := Label{text: "" padding: Inset{left: 8} draw_text.color: muted draw_text.text_style.font_size: 8.5}
                        View{
                            width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 4 padding: Inset{left: 6}
                            pe0 := EffortChip{} pe1 := EffortChip{} pe2 := EffortChip{} pe3 := EffortChip{}
                            pe4 := EffortChip{} pe5 := EffortChip{} pe6 := EffortChip{} pe7 := EffortChip{}
                        }
                    }
                }
            }
            pick_soon := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
            pick_note := Label{width: Fill text: "" draw_text.color: muted draw_text.wrap: Words draw_text.text_style.font_size: 8.5}
        }
        // The sidebar's "+": a project folder, or a new OctoSense app.
        add_menu := RoundedView{
            width: 230 height: Fit flow: Down visible: false new_batch: true
            padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
            draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line_strong
            a_project := MenuItem{}
            a_app := MenuItem{}
        }
        // A sidebar row's right-click menu, at the pointer.
        side_menu := RoundedView{
            width: 220 height: Fit flow: Down visible: false new_batch: true
            padding: Inset{left: 4 right: 4 top: 4 bottom: 4}
            draw_bg.color: th_raised draw_bg.border_radius: 4.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line_strong
            side_title := Label{width: Fill text: "" max_lines: 1 padding: Inset{left: 8 right: 8 top: 4 bottom: 4} draw_text.color: muted draw_text.text_style.font_size: 8}
            s_rename := MenuItem{}
            s_pin := MenuItem{}
            s_archive := MenuItem{}
            s_to_chats := MenuItem{}
            s_move_title := Label{width: Fill text: "" padding: Inset{left: 8 right: 8 top: 6 bottom: 2} draw_text.color: muted draw_text.text_style.font_size: 8}
            s_mv0 := MenuItem{}
            s_mv1 := MenuItem{}
            s_mv2 := MenuItem{}
            s_mv3 := MenuItem{}
            s_mv4 := MenuItem{}
            s_mv5 := MenuItem{}
            s_remove := MenuItem{draw_text +: {color: th_danger color_hover: th_danger_strong color_focus: th_danger}}
        }
        // Renaming a project or a session.
        rename_layer := View{
            width: Fill height: Fill flow: Overlay visible: false align: Align{x: 0.5 y: 0.35}
            SolidView{width: Fill height: Fill cursor: MouseCursor.Default draw_bg.color: th_scrim}
            RoundedView{
                width: 380 height: Fit flow: Down spacing: 12 new_batch: true
                padding: Inset{left: 18 right: 18 top: 16 bottom: 14}
                draw_bg.color: th_raised draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line_strong
                rename_title := Label{width: Fill text: "" padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
                rename_input := InputStyle{width: Fill height: 32}
                View{
                    width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                    Filler{}
                    rename_cancel := SegOff{text: "Cancel" height: 28}
                    rename_ok := SegOn{text: "Rename" height: 28}
                }
            }
        }
        // A spec's file as written, opened from a card's Specs tab, over all.
        spec_layer := View{
            width: Fill height: Fill flow: Overlay visible: false align: Align{x: 0.5 y: 0.5}
            spec_backdrop := SolidView{width: Fill height: Fill cursor: MouseCursor.Default grab_key_focus: false draw_bg.color: th_scrim}
            RoundedView{
                width: 720 height: 560 flow: Down new_batch: true
                draw_bg.color: th_raised draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line_strong
                View{
                    width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                    padding: Inset{left: 18 right: 12 top: 14 bottom: 10}
                    View{
                        width: Fill height: Fit flow: Down spacing: 3
                        spec_title := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: ink draw_text.text_style: theme.font_bold{font_size: 11}}
                        spec_where := Label{width: Fill text: "" max_lines: 1 padding: 0 draw_text.color: muted draw_text.text_style: theme.font_code{font_size: 8}}
                    }
                    spec_close := SegOff{text: "Close" height: 28}
                }
                SolidView{width: Fill height: 1 draw_bg.color: line}
                ScrollYView{
                    width: Fill height: Fill padding: Inset{left: 18 right: 18 top: 12 bottom: 16}
                    spec_body := Label{width: Fill text: "" draw_text.color: ink draw_text.wrap: Words draw_text.text_style: theme.font_code{font_size: 9}}
                }
            }
        }
        // Deleting an outer loop, closing an inner one: asked first, over all.
        confirm_layer := View{
            width: Fill height: Fill flow: Overlay visible: false align: Align{x: 0.5 y: 0.4}
            SolidView{width: Fill height: Fill cursor: MouseCursor.Default draw_bg.color: th_scrim}
            RoundedView{
                width: 380 height: Fit flow: Down spacing: 12 new_batch: true
                padding: Inset{left: 18 right: 18 top: 16 bottom: 14}
                draw_bg.color: th_raised draw_bg.border_radius: 6.0 draw_bg.border_size: 1.0 draw_bg.border_color: th_line_strong
                confirm_title := Label{width: Fill text: "" padding: 0 draw_text.color: ink draw_text.wrap: Words draw_text.text_style: theme.font_bold{font_size: 11}}
                confirm_text := Label{width: Fill text: "" padding: 0 draw_text.color: th_muted_strong draw_text.wrap: Words draw_text.text_style.font_size: 9.5}
                View{
                    width: Fill height: Fit flow: Right spacing: 8 align: Align{y: 0.5}
                    Filler{}
                    confirm_cancel := SegOff{text: "Cancel" height: 28}
                    confirm_ok := DangerButton{text: "Delete" height: 28}
                }
            }
        }
    }
}

/// One line of the sidebar.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Row {
    Project(usize),
    Session(SessionRef),
    /// "Archived (N)": opens the archived projects and sessions below it.
    ArchiveHead(usize),
    ArchivedProject(usize),
    ArchivedSession(SessionRef),
}

/// Where an app's data comes from.
enum DataFrom {
    Csv(PathBuf),
    Api(String),
}

/// What the stage shows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Stage {
    #[default]
    Chat,
    Flow,
    Time,
    /// The outer loop's own terminal UI (the native TUI plugin).
    Tui,
}

/// Playback speeds, axis seconds a second.
const TIME_SPEEDS: [f64; 4] = [20.0, 60.0, 5.0, 1.0];

/// A card opened on the flow canvas: an outer loop (its session id) or an
/// inner loop (its peer id).
#[derive(Clone, Debug, PartialEq)]
enum FlowOpen {
    Outer(String),
    Inner(String),
    /// A new peer agent being configured.
    Create,
}

#[derive(Script, ScriptHook, Widget)]
pub struct OctoBuddyView {
    #[deref]
    view: View,
    /// Which mark each provider icon shows (`provider_icons::show`).
    #[rust]
    icons_shown: HashMap<WidgetUid, usize>,
    /// The width Settings › Tools' cards have now (`fit_tool_cards`).
    #[rust]
    tool_card_width: f64,
    #[rust]
    tool_fit_frame: NextFrame,
    /// The conversation each chat list shows (`draw_chat`).
    #[rust]
    chat_shown: HashMap<WidgetUid, String>,
    /// Where the person left each conversation: its first item shown, that
    /// item's offset, and whether it was at the end (`draw_chat`).
    #[rust]
    chat_places: HashMap<String, (usize, f64, bool)>,
    /// The app workbench beside the conversation (`plugins/workbench.rs`).
    #[rust]
    wb: Workbench,
    #[rust]
    store: Store,
    #[rust]
    store_path: Option<PathBuf>,
    #[rust]
    rows: Vec<Row>,
    #[rust]
    selected: Option<SessionRef>,
    #[rust]
    started: bool,
    #[rust]
    rt: Runtime,
    /// What each session's lead is doing, by session id, while it runs.
    #[rust]
    lead_status: HashMap<String, String>,
    /// Rounds each session's current request may still start.
    #[rust]
    rounds_left: HashMap<String, u32>,
    /// Sessions the person stopped: their round is not handed to the lead.
    #[rust]
    halted: HashSet<String>,
    /// Outer loops asked to stop, and when: one that has not stopped within
    /// `STOP_GRACE` is ended (its conversation resumes at the next message).
    #[rust]
    stopping: HashMap<String, std::time::Instant>,
    /// Sessions whose turn the person stopped, with nothing sent since: they
    /// may go on (`继续`), with what waited for them first.
    #[rust]
    resumable: HashSet<String>,
    /// Outer loops to start again once their turn ends (their effort was
    /// changed while they worked; the conversation is kept).
    #[rust]
    respawn: HashSet<String>,
    /// Sessions whose model was changed with their conversation kept: what
    /// their agent is told with the next message (the replies above it came
    /// from another model, which it would otherwise go on claiming to be).
    #[rust]
    model_note: HashMap<String, String>,
    #[rust]
    show_inner: bool,
    /// Ticks once a second, to keep elapsed times current.
    #[rust]
    clock: Timer,
    #[rust]
    page: Page,
    // What AI providers has enabled, as last read.
    #[rust]
    providers: Providers,
    // Settings › AI Providers on the host (`providers_view.rs`): the add
    // wizard while open, what was last done, the row asked to be removed,
    // the agents' own sign-ins.
    #[rust]
    wizard: Option<providers_view::Wizard>,
    #[rust]
    wizard_clear: bool,
    #[rust]
    own_note: Option<Result<String, String>>,
    #[rust]
    confirm_remove: Option<String>,
    #[rust]
    logins: Option<Vec<(String, String)>>,
    #[rust]
    logins_probing: bool,
    // Where the view was last drawn: a change means a relayout, see draw_walk.
    #[rust]
    last_rect: Rect,
    #[rust]
    settings_tab: SettingsTab,
    // Set while our folder picker is open: its answer comes as a platform
    // action every module sees, and only ours should act on it.
    #[rust]
    picking_folder: bool,
    // That folder is for a new OctoSense app.
    #[rust]
    picking_app: bool,
    // What the stage shows: the conversation, the graph of the project's
    // loops, or the session's timeline.
    #[rust]
    stage: Stage,
    // The timeline last drawn, for its panels.
    #[rust]
    time_tl: Timeline,
    // Its playback speed (axis seconds a second) and whether idle is squeezed.
    #[rust]
    time_speed: usize,
    #[rust]
    time_unsqueezed: bool,
    // A sidebar row's right-click menu: the row, where it opened.
    #[rust]
    side_menu: Option<(Row, DVec2)>,
    // The archived projects and sessions are shown, under "Archived".
    #[rust]
    show_archived: bool,
    // What the rename dialog renames: `p:<project id>` or `s:<session id>`.
    #[rust]
    renaming: Option<String>,
    // The session whose app is being published.
    #[rust]
    publishing: Option<String>,
    // The production loop's watches (`plugins::card_loop`).
    #[rust]
    card_loop: CardLoops,
    // Apps asked for from outside (`plugins::app_factory`).
    #[rust]
    factory: AppFactory,
    // The projects the side menu's "move to" rows stand for, by name.
    #[rust]
    side_moves: Vec<String>,
    // A right-click menu over the graph: the card it is for (none: the
    // canvas), where it opened on the screen and on the canvas.
    #[rust]
    flow_menu: Option<(Option<String>, DVec2, DVec2)>,
    // The plan panel: where the person put it, the grab of a drag, folded,
    // closed for a session's round, and the peers its rows stand for.
    #[rust]
    plan_pos: Option<DVec2>,
    #[rust]
    plan_drag: Option<DVec2>,
    #[rust]
    plan_folded: bool,
    #[rust]
    plan_hidden: Option<(String, u64)>,
    #[rust]
    plan_peers: Vec<String>,
    // A press began on a panel floating over the messages: the list under it
    // takes no text selection from it.
    #[rust]
    pressing_overlay: bool,
    // The card whose conversation is open over the graph.
    #[rust]
    flow_open: Option<FlowOpen>,
    // Settings › Plugins and Tools: what they list (tools probed off the UI thread).
    #[rust]
    plugin_rows: Vec<plugins::Plugin>,
    #[rust]
    tool_rows: Option<Vec<tools_info::ToolInfo>>,
    // The external plugins' buttons shown now: (plugin, button) per slot.
    #[rust]
    plugin_slots: Vec<(String, String)>,
    // The engine picker: the inner loops' side shown (else the outer
    // loop's), and the agent whose models it lists.
    #[rust]
    pick_inner: bool,
    #[rust]
    pick_agent: Option<String>,
    // The native TUI plugin: whose CLI is open (`s:<session>`, `p:<peer>`).
    #[rust]
    tui: Option<String>,
    // A pane being resized: which, and its width when the drag began.
    #[rust]
    resizing: Option<(&'static str, f64)>,
    // The sidebar's "+" menu, open: where.
    #[rust]
    add_menu: Option<DVec2>,
    // Menus drawn outside the window, moved in next frame (`place`).
    #[rust]
    menu_fix: Vec<(LiveId, DVec2)>,
    #[rust]
    menu_frame: NextFrame,
    // Live replies shown at an even pace: (conversation, row) → how far.
    #[rust]
    reveal: HashMap<(String, usize), reveal::Reveal>,
    #[rust]
    reveal_frame: NextFrame,
    #[rust]
    dots: usize,
    // Its tab, the spec opened in it, and the specs listed there.
    #[rust]
    card_tab: CardTab,
    #[rust]
    spec_shown: Option<String>,
    #[rust]
    spec_rows: Vec<SpecRow>,
    // The session the canvas was last brought to.
    #[rust]
    flow_focus: Option<String>,
    // The open card's queue: its item ids, and whether they are an outer loop's.
    #[rust]
    flow_queue_ids: Vec<String>,
    #[rust]
    flow_queue_outer: bool,
    // The card whose Delete was pressed once: the next press deletes it.
    #[rust]
    confirm_delete: Option<String>,
    // The new peer agent's model (an index of the octos choices) and outer
    // loop (a session id; none: on its own).
    #[rust]
    new_model: usize,
    #[rust]
    new_outer: Option<String>,
    // The peer whose budget fills the panel's fields (refilled only when another is shown).
    #[rust]
    budget_of: Option<String>,
    // OctoBuddy's tools for the outer loop (MCP over loopback HTTP).
    #[rust]
    mcp: Option<mcp::Server>,
    // The outer loop's engine and model list is open.
    #[rust]
    show_picker: bool,
    // The app's data panel is open; a CSV file is being picked for it.
    #[rust]
    show_data: bool,
    #[rust]
    picking_data: bool,
    // A data source was just added in this session: fill the input for it.
    #[rust]
    data_added: Option<(String, Result<(), String>)>,
    // The panel's hand-over row is open; the sessions its buttons stand for.
    #[rust]
    show_move: bool,
    #[rust]
    move_targets: Vec<String>,
    // The app project's app, in the preview panel.
    #[rust]
    show_preview: bool,
    #[rust]
    preview: Option<plugins::app_preview::Running>,
    // The project it runs, and its files' stamp then.
    #[rust]
    preview_of: Option<(String, u64)>,
    // Why the files as they are now do not run (the last good version may).
    #[rust]
    preview_errors: Vec<String>,
    // The inner-loop tab shown in the panel, by peer id.
    #[rust]
    selected_peer: Option<String>,
    // Cards opened in the conversation: (session id, message index).
    #[rust]
    expanded: HashSet<(String, usize)>,
    // The messages whose tool calls the person unfolded: (conversation, index).
    #[rust]
    steps_open: HashSet<(String, usize)>,
    // Peers whose diff is open in the panel.
    #[rust]
    show_diff: HashSet<String>,
    // The peer whose Close was pressed while it worked: the next press closes it.
    #[rust]
    confirm_close: Option<String>,
    // OctoBuddy's peer on the system octos (none: not granted here).
    #[rust]
    link: Option<system::Link>,
    // Whether that peer is ready, or why not.
    #[rust]
    system_link: Option<Result<(), String>>,
    // The reply last copied: (conversation key, row).
    #[rust]
    copied: Option<(String, usize)>,
    // The item ids the queue panels show, row by row.
    #[rust]
    outer_queue_ids: Vec<String>,
    #[rust]
    inner_queue_ids: Vec<String>,
    // Peers whose Result details are open.
    #[rust]
    show_details: HashSet<String>,
    // The session whose Discard worktree was pressed once.
    #[rust]
    confirm_worktree: Option<String>,
}

impl OctoBuddyView {
    fn ensure_started(&mut self, cx: &mut Cx) {
        if self.started {
            return;
        }
        self.started = true;
        let path = model::default_store_path();
        self.store = Store::load(&path);
        plugins::set_disabled(self.store.disabled_plugins.as_deref().unwrap_or(&[]));
        plugins::set_enabled(self.store.enabled_plugins.as_deref().unwrap_or(&[]));
        self.apply_panes(cx);
        // Plain chats, at the top: a conversation needs no project.
        self.store.chats();
        // What waited when it last saved comes back first (it needs to see
        // which turns were running), then those turns are marked cut off.
        self.restore();
        self.store.mark_interrupted();
        let lang = self.store.lang.as_deref().and_then(i18n::Lang::parse).unwrap_or_else(i18n::Lang::from_env);
        i18n::set(lang);
        self.apply_language(cx);
        self.store_path = Some(path);
        self.selected = self.store.projects.iter().enumerate()
            .find_map(|(pi, p)| p.sessions.iter().rposition(|s| !s.is_detached()).map(|si| (pi, si)));
        self.clock = cx.start_interval(1.0);
        self.link = system::Link::start(&self.rt.inbox);
        self.mcp = mcp::Server::start(&self.rt.inbox);
        self.providers = providers::read();
        self.sync(cx);
        // And what waited goes on.
        self.resume();
        self.save();
        self.sync(cx);
    }

    /// `sync`, for a change that moves things around (a page, the inner
    /// panel, another session). Views with `new_batch` keep their own draw
    /// list and replay it at its old position unless they are redrawn
    /// themselves, which a redraw of this view does not do: redraw them all.
    fn relayout(&mut self, cx: &mut Cx) {
        self.sync(cx);
        cx.redraw_all();
    }

    fn save(&self) {
        if let Some(path) = &self.store_path {
            // With what waits in memory, so a restart goes on from it.
            if let Err(err) = self.snapshot().save(path) {
                log!("octobuddy: could not save {}: {err}", path.display());
            }
        }
    }

    fn rebuild_rows(&mut self) {
        self.rows.clear();
        // The plain chats first, then the pinned projects, then the rest;
        // the archived ones under "Archived", at the end.
        let mut order: Vec<usize> = (0..self.store.projects.len())
            .filter(|&pi| self.store.projects[pi].is_chats() || !self.store.projects[pi].is_archived()).collect();
        order.sort_by_key(|&pi| (!self.store.projects[pi].is_chats(), !self.store.projects[pi].is_pinned()));
        let archived_projects: Vec<usize> = (0..self.store.projects.len())
            .filter(|&pi| !self.store.projects[pi].is_chats() && self.store.projects[pi].is_archived()).collect();
        let archived_sessions: Vec<SessionRef> = order.iter().flat_map(|&pi| {
            let p = &self.store.projects[pi];
            (0..p.sessions.len()).filter(move |&si| p.sessions[si].is_archived() && !p.sessions[si].is_detached()).map(move |si| (pi, si))
        }).collect();
        for pi in order {
            let project = &self.store.projects[pi];
            self.rows.push(Row::Project(pi));
            if project.expanded {
                // The holder of peers without an outer loop is no session to list.
                let mut shown: Vec<usize> = (0..project.sessions.len())
                    .filter(|si| !project.sessions[*si].is_detached() && !project.sessions[*si].is_archived()).collect();
                shown.sort_by_key(|si| !project.sessions[*si].is_pinned());
                for si in shown {
                    self.rows.push(Row::Session((pi, si)));
                }
            }
        }
        let archived = archived_projects.len() + archived_sessions.len();
        if archived > 0 {
            self.rows.push(Row::ArchiveHead(archived));
            if self.show_archived {
                self.rows.extend(archived_projects.into_iter().map(Row::ArchivedProject));
                self.rows.extend(archived_sessions.into_iter().map(Row::ArchivedSession));
            }
        }
    }

    fn session_busy(&self, at: SessionRef) -> bool {
        let Some(session) = self.store.session(at) else { return false };
        self.lead_status.contains_key(&session.id) || session.peers().iter().any(Peer::is_active)
    }

    /// Projects the state onto the parts that are not lists.
    fn sync(&mut self, cx: &mut Cx) {
        self.rebuild_rows();
        let selected = self.selected.filter(|at| self.store.session(*at).is_some());
        let chat = self.page == Page::Chat;
        self.view.view(cx, ids!(workspace)).set_visible(cx, chat);
        self.view.view(cx, ids!(settings_page)).set_visible(cx, self.page == Page::Settings);
        self.view.view(cx, ids!(live_page)).set_visible(cx, self.page == Page::Live);
        self.sync_live_button(cx);
        if self.page == Page::Live {
            self.sync_live_page(cx);
        }
        self.view.view(cx, ids!(chat)).set_visible(cx, chat && selected.is_some());
        self.view.view(cx, ids!(placeholder)).set_visible(cx, chat && selected.is_none());
        if self.page == Page::Settings {
            self.sync_settings(cx);
        }
        let hint = if self.store.projects.iter().all(|p| p.is_chats()) {
            i18n::t("Press + beside Chats to talk to an agent now, or + beside Projects to work on a project folder.", "点「对话」旁的 + 直接和 agent 对话，或点「项目」旁的 + 选择项目文件夹。")
        } else {
            i18n::t("Choose a session, or press + beside Chats or a project to start one.", "选择一个会话，或点「对话」或项目旁的 + 新建。")
        };
        self.view.label(cx, ids!(placeholder_hint)).set_text(cx, hint);
        if let Some(at) = selected {
            let busy = self.session_busy(at);
            let project = &self.store.projects[at.0];
            let session = &project.sessions[at.1];
            let lead = self.lead_status.get(&session.id).cloned().unwrap_or_else(|| "idle".into());
            let lead = status_word(&lead).to_string();
            let (_, model) = self.rt.outer_label(session, &project.path, &lead);
            let cost = session.lead_cost.map(|c| format!(" · ${c:.2}")).unwrap_or_default();
            let effort = session.outer_effort().map(|e| format!(" · {}", model::effort_word(e))).unwrap_or_default();
            self.view.label(cx, ids!(outer_model)).set_text(cx, &format!("{model}{effort}{cost}"));
            let counts = PeerCounts::of(session.peers());
            let waiting = session.peers().iter().filter(|p| self.rt.approvals.contains_key(&p.id)).count();
            // Outer and Inner float over the conversation once it has inner loops.
            let has_inner = !session.peers().is_empty();
            let working = session.peers().iter().filter(|p| p.is_active()).count();
            let open = session.peers().iter().filter(|p| p.status != "closed").count();
            let inner = match (waiting, working) {
                (0, 0) => i18n::pick(format!("{open} open"), format!("{open} 个")),
                (0, n) => i18n::pick(format!("{n} working"), format!("{n} 个在做")),
                (n, _) => i18n::pick(format!("{n} to approve"), format!("{n} 个待批")),
            };
            let panel = self.show_inner;
            for (id, show) in [(ids!(outer_btn), has_inner && panel), (ids!(outer_btn_on), has_inner && !panel), (ids!(inner_btn), has_inner && !panel), (ids!(inner_btn_on), has_inner && panel)] {
                self.view.view(cx, id).set_visible(cx, show);
            }
            for id in [ids!(outer_btn), ids!(outer_btn_on)] {
                self.view.label(cx, &[id[0], live_id!(name)]).set_text(cx, "Outer");
                self.view.label(cx, &[id[0], live_id!(sub)]).set_text(cx, &lead);
            }
            for id in [ids!(inner_btn), ids!(inner_btn_on)] {
                self.view.label(cx, &[id[0], live_id!(name)]).set_text(cx, "Inner");
                self.view.label(cx, &[id[0], live_id!(sub)]).set_text(cx, &inner);
            }
            let peer_cost: f64 = session.peers().iter().filter_map(|p| p.usage.as_ref()).map(|u| u.cost).sum();
            let spent = if peer_cost > 0.0 { i18n::pick(format!(" · ${peer_cost:.3} on peers"), format!(" · inner 花费 ${peer_cost:.3}")) } else { String::new() };
            let auto = session.auto();
            for (id, show) in [(ids!(approvals_ask_on), !auto), (ids!(approvals_ask), auto), (ids!(approvals_auto_on), auto), (ids!(approvals_auto), !auto)] {
                self.view.button(cx, id).set_visible(cx, show);
            }
            self.view.label(cx, ids!(approvals_hint)).set_text(cx, if auto { i18n::t("OctoBuddy approves · sandbox on", "OctoBuddy 代为批准 · 沙箱开启") } else { i18n::t("you approve risky commands", "有风险的命令由你批准") });
            let estimate = session.estimate.as_deref().map(|e| i18n::pick(format!("\nEstimate: {e}"), format!("\n估算：{e}"))).unwrap_or_default();
            self.view.label(cx, ids!(inner_summary)).set_text(cx, &format!("{}{spent}{estimate}", counts.long(session.round())));

            let engine = session.engine().to_string();
            // The mark of the agent it runs on.
            self.view.view(cx, ids!(outer_icon)).set_visible(cx, engine == "claude");
            self.view.view(cx, ids!(outer_icon_codex)).set_visible(cx, engine == rpc_lead::CODEX);
            self.view.view(cx, ids!(outer_icon_pi)).set_visible(cx, engine == rpc_lead::PI);
            self.view.view(cx, ids!(outer_icon_octos)).set_visible(cx, !matches!(engine.as_str(), "claude" | rpc_lead::CODEX | rpc_lead::PI));
            // At work: Stop, in Send's place while nothing is typed (typed,
            // Send queues it and Stop stands beside it, as Cindy's).
            let typed = !self.view.text_input(cx, ids!(composer)).text().trim().is_empty();
            self.view.button(cx, ids!(stop)).set_visible(cx, busy);
            self.view.button(cx, ids!(send)).set_visible(cx, !busy || typed);
            let interruptible = busy && typed && self.rt.leads.contains_key(&session.id) && !self.tui_holds_outer(&session.id);
            let interrupt = self.view.button(cx, ids!(interrupt_send));
            interrupt.set_visible(cx, interruptible);
            interrupt.set_text(cx, i18n::t("Interrupt & send", "打断并发送"));
            self.view.button(cx, ids!(go_on)).set_visible(cx, !busy && self.resumable.contains(&session.id));
            let worktree = self.view.check_box(cx, ids!(use_worktree));
            worktree.set_active(cx, session.worktree(), Animate::No);
            // Where the loops work is fixed once the session has started; a
            // plain chat has no repository to branch.
            worktree.set_visible(cx, session.work_dir.is_none() && !project.is_chats());
            self.sync_worktree_bar(cx, at);
            let session_id = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
            let waiting = self.outer_waiting(&session_id);
            self.outer_queue_ids = self.sync_queue(cx, live_id!(outer_queue), &waiting);
            // The person's own messages may go into the turn running now.
            let steerable = self.steerable(&session_id).is_ok();
            for (i, row) in QUEUE_ROWS.iter().enumerate() {
                let mine = waiting.get(i).is_some_and(|(_, who, _)| who == "you");
                let button = self.view.button(cx, &[live_id!(outer_queue), *row, live_id!(steer)]);
                button.set_visible(cx, steerable && mine);
                button.set_text(cx, i18n::t("Steer in", "插话"));
            }
            self.sync_peer_detail(cx, at);
        }
        // A plain chat has no inner loops: no panel for them (it comes back
        // with a session that has).
        let plain = selected.is_some_and(|at| self.store.is_plain(at));
        let show_inner = self.show_inner && selected.is_some() && chat && !plain;
        self.view.view(cx, ids!(inner_panel)).set_visible(cx, show_inner);
        self.view.view(cx, ids!(inner_divider)).set_visible(cx, show_inner);
        // An OctoSense app's session: its app beside it, and a brief to ask
        // for — each a plugin's slot, there while its plugin is on.
        let project_path = selected.map(|at| self.store.projects[at.0].path.clone()).unwrap_or_default();
        let is_app = selected.is_some() && plugins::active(plugins::OCTOSENSE_APP, &project_path);
        let has_data = selected.is_some() && plugins::active(plugins::APP_DATA, &project_path);
        let has_preview = selected.is_some() && plugins::active(plugins::OCTOSENSE_APP, &project_path);
        self.sync_plugin_buttons(cx, selected.map(|_| project_path.as_str()));
        self.view.view(cx, ids!(data_btn)).set_visible(cx, has_data);
        self.view.label(cx, ids!(data_btn.name)).set_text(cx, i18n::t("Data", "数据"));
        let sources = selected.and_then(|at| self.store.session(at)).and_then(|s| s.data.as_ref()).map(|d| d.len()).unwrap_or(0);
        let sub = if sources == 0 { i18n::t("add", "添加").to_string() } else { i18n::pick(format!("{sources} source(s)"), format!("{sources} 个")) };
        self.view.label(cx, ids!(data_btn.sub)).set_text(cx, &sub);
        let known: Vec<String> = selected.and_then(|at| self.store.session(at)).and_then(|s| s.data.as_ref()).into_iter().flatten()
            .map(|d| {
                // "CSV <file> → …" or "API <url>: …": where it comes from.
                let end = [": ", " →"].iter().filter_map(|m| d.summary.find(m)).min().unwrap_or(d.summary.len());
                let first = d.summary[..end].trim();
                let first: String = first.chars().take(80).collect();
                let told = if d.told { "" } else { i18n::t(" (outer not told yet)", "（还没告诉 outer）") };
                format!("· {} — {first}{told}", d.name)
            }).collect();
        let data_known = self.view.label(cx, ids!(data_known));
        data_known.set_visible(cx, !known.is_empty());
        data_known.set_text(cx, &known.join("\n"));
        self.view.view(cx, ids!(data_panel)).set_visible(cx, has_data && self.show_data);
        // The data panel lies over the messages, and the list sees presses
        // its overlays took: one on the panel's fields must not start a text
        // selection there (and take the keyboard from the field).
        if let Some(mut list) = self.view.portal_list(cx, ids!(messages)).borrow_mut() {
            // Nor while a dialog or the sidebar's menu is open over it.
            let modal = self.renaming.is_some() || self.confirm_delete.is_some() || self.side_menu.is_some();
            list.selectable = !(has_data && self.show_data) && !self.pressing_overlay && !modal;
        }
        // The app workbench: preview, files, permissions, App Hub (publishing is its last tab).
        self.view.view(cx, ids!(preview_btn)).set_visible(cx, has_preview && !self.show_preview);
        self.view.view(cx, ids!(preview_btn_on)).set_visible(cx, has_preview && self.show_preview);
        self.view.view(cx, ids!(publish_btn)).set_visible(cx, false);
        for id in [ids!(preview_btn), ids!(preview_btn_on)] {
            self.view.label(cx, &[id[0], live_id!(name)]).set_text(cx, i18n::t("App", "应用"));
            self.view.label(cx, &[id[0], live_id!(sub)]).set_text(cx, i18n::t("workbench", "工作台"));
        }
        let show_preview = self.show_preview && has_preview && chat;
        self.view.view(cx, ids!(preview_panel)).set_visible(cx, show_preview);
        self.sync_workbench(cx, show_preview.then_some(project_path.as_str()));
        self.view.view(cx, ids!(preview_divider)).set_visible(cx, show_preview);
        let plain = selected.is_some_and(|at| self.store.is_plain(at));
        // At work on an engine that can be steered: how to steer it.
        let steerable = selected.and_then(|at| self.store.session(at)).is_some_and(|s| self.steerable(&s.id).is_ok());
        let hint = if steerable {
            if cfg!(target_os = "macos") {
                i18n::t("At work: Enter sends after this turn, ⌘Enter steers it (taken up at its next step), ⇧⌘Enter interrupts it and sends", "运行中：Enter 排在这一轮之后 · ⌘Enter 插话（它下一步才读到）· ⇧⌘Enter 打断并发送")
            } else {
                i18n::t("At work: Enter sends after this turn, Ctrl+Enter steers it (taken up at its next step), ⇧Ctrl+Enter interrupts it and sends", "运行中：Enter 排在这一轮之后 · Ctrl+Enter 插话（它下一步才读到）· ⇧Ctrl+Enter 打断并发送")
            }
        } else if is_app {
            i18n::t("Describe the app you want: what it is for, its screens, its data  (Enter to send)", "描述你想要的应用：做什么用、有哪些界面、用什么数据（Enter 发送）")
        } else if plain {
            i18n::t("Message  (Enter to send, Shift+Enter for a new line)", "发消息（Enter 发送，Shift+Enter 换行）")
        } else {
            i18n::t("Message the outer loop  (Enter to send, Shift+Enter for a new line)", "给 outer 发消息（Enter 发送，Shift+Enter 换行）")
        };
        self.view.text_input(cx, ids!(composer)).set_empty_text(cx, hint.into());
        if let Some(at) = selected {
            self.sync_picker(cx, at);
        }
        self.sync_preview(cx);
        // The terminal first: one that no longer belongs to what is shown
        // ends here and the stage goes back to the conversation, which the
        // views then show (the other way round left them hidden: a blank stage).
        self.sync_tui(cx);
        self.sync_flow(cx);
        self.sync_plan(cx);
        self.sync_confirm(cx);
        self.sync_side_menu(cx);
        self.sync_add_menu(cx);
        self.view.view(cx, ids!(rename_layer)).set_visible(cx, self.renaming.is_some());
        self.view.portal_list(cx, ids!(tree)).redraw(cx);
        self.view.portal_list(cx, ids!(messages)).redraw(cx);
        self.view.portal_list(cx, ids!(tabs)).redraw(cx);
        self.fit_pills(cx);
        // The panes shown now, fitted to the window.
        self.apply_panes(cx);
        self.view.redraw(cx);
    }

    /// The floating bar's pills fitted to the room: a narrow conversation
    /// keeps only the views' marks; a pill without a state shows none.
    fn fit_pills(&mut self, cx: &mut Cx) {
        let width = self.view.view(cx, ids!(chat)).area().rect(cx).size.x;
        let compact = width > 0.0 && width < 860.0;
        let views = [live_id!(view_chat), live_id!(view_chat_on), live_id!(view_flow), live_id!(view_flow_on), live_id!(view_time), live_id!(view_time_on), live_id!(view_tui), live_id!(view_tui_on)];
        for id in views {
            self.view.label(cx, &[id, live_id!(name)]).set_visible(cx, !compact);
        }
        let all = views.into_iter().chain([live_id!(outer_btn), live_id!(outer_btn_on), live_id!(inner_btn), live_id!(inner_btn_on), live_id!(new_peer_btn), live_id!(data_btn),
            live_id!(preview_btn), live_id!(preview_btn_on), live_id!(publish_btn), live_id!(plug_btn0), live_id!(plug_btn1), live_id!(plug_btn2)]);
        for id in all {
            let sub = self.view.label(cx, &[id, live_id!(sub)]);
            sub.set_visible(cx, !sub.text().is_empty());
        }
    }

    fn sync_settings(&mut self, cx: &mut Cx) {
        let tab = self.settings_tab;
        if tab == SettingsTab::Appearance {
            self.sync_appearance(cx);
        }
        for (on, off, section, this) in [
            (ids!(nav_providers_on), ids!(nav_providers), ids!(providers_section), SettingsTab::Providers),
            (ids!(nav_plugins_on), ids!(nav_plugins), ids!(plugins_section), SettingsTab::Plugins),
            (ids!(nav_tools_on), ids!(nav_tools), ids!(tools_section), SettingsTab::Tools),
            (ids!(nav_language_on), ids!(nav_language), ids!(language_section), SettingsTab::Language),
            (ids!(nav_appearance_on), ids!(nav_appearance), ids!(appearance_section), SettingsTab::Appearance),
            (ids!(nav_about_on), ids!(nav_about), ids!(about_section), SettingsTab::About),
        ] {
            self.view.button(cx, on).set_visible(cx, tab == this);
            self.view.button(cx, off).set_visible(cx, tab != this);
            self.view.view(cx, section).set_visible(cx, tab == this);
        }
        self.sync_plugins_page(cx);
        let zh = i18n::zh();
        for (id, show) in [(ids!(lang_en_on), !zh), (ids!(lang_en), zh), (ids!(lang_zh_on), zh), (ids!(lang_zh), !zh)] {
            self.view.button(cx, id).set_visible(cx, show);
        }

        self.sync_providers_page(cx);
        let peer = providers::peer_profile(&self.providers);
        self.view.label(cx, ids!(inner_source)).set_text(cx, &i18n::pick(format!("Inner loops run with {}.", peer.source), format!("Inner 使用 {}。", peer.source)));

        self.view.label(cx, ids!(about_version)).set_text(cx, &i18n::pick(format!("Version {}", env!("CARGO_PKG_VERSION")), format!("版本 {}", env!("CARGO_PKG_VERSION"))));
        let (claude, octos) = (workspace::find_bin("claude"), workspace::find_bin("octos"));
        let loops = i18n::pick(format!("Outer and inner loops run on Claude Code ({}), Codex or pi (on your AI providers, through OctoBuddy's proxy), or octos ({}, with {}). Versions: Settings › Tools.",
            claude.display(), octos.display(), peer.source), format!("外环和 inner 可以跑在 Claude Code（{}）、Codex 或 pi（用你的 AI providers，经 OctoBuddy 本机代理）、或 octos（{}，使用 {}）上。版本见 设置 › 工具。",
            claude.display(), octos.display(), peer.source));
        let link = match (&self.link, &self.system_link) {
            // On the host there is no system agent to link: nothing to say.
            (None, _) if !system::hosted() => String::new(),
            (None, _) => i18n::t("System octos: not linked (OctoSense has not granted OctoBuddy an assistant here).", "系统 octos：未接入（OctoSense 在这里没有给 OctoBuddy 授权助手）。").to_string(),
            (Some(_), None) => i18n::t("System octos: linking OctoBuddy's peer…", "系统 octos：正在接入 OctoBuddy 的 peer…").to_string(),
            (Some(_), Some(Ok(()))) => i18n::t("System octos: linked. The system agent sees OctoBuddy as a peer and can call octobuddy.status and octobuddy.send.", "系统 octos：已接入。系统 agent 能看到 OctoBuddy 这个 peer，并可调用 octobuddy.status 和 octobuddy.send。").to_string(),
            (Some(_), Some(Err(err))) => i18n::pick(format!("System octos: not linked: {err}"), format!("系统 octos：接入失败：{err}")),
        };
        self.view.label(cx, ids!(about_loops)).set_text(cx, &if link.is_empty() { loops } else { format!("{loops}\n{link}") });
        let data = model::data_dir();
        let (store, worktrees, octos_data) = (model::default_store_path(), data.join("worktrees"), data.join("octos/serve"));
        let text = i18n::pick(format!("Projects and sessions: {}\nSession worktrees: {}\nInner loops' octos data: {}",
            store.display(), worktrees.display(), octos_data.display()), format!("项目与会话：{}\n会话 worktree：{}\nInner 的 octos 数据：{}",
            store.display(), worktrees.display(), octos_data.display()));
        self.view.label(cx, ids!(about_data)).set_text(cx, &text);
    }

    /// The panel's tabs, newest round first, and which one is shown.
    fn tab_order(&self, at: SessionRef) -> Vec<usize> {
        let Some(session) = self.store.session(at) else { return Vec::new() };
        let mut order: Vec<usize> = (0..session.peers().len()).filter(|&i| session.peers()[i].status != "closed").collect();
        order.sort_by_key(|&i| (std::cmp::Reverse(session.peers()[i].round), i));
        order
    }

    fn shown_peer(&self, at: SessionRef) -> Option<&Peer> {
        let session = self.store.session(at)?;
        let order = self.tab_order(at);
        let chosen = self.selected_peer.as_deref()
            .and_then(|id| session.peers().iter().find(|p| p.id == id && p.status != "closed"));
        chosen.or_else(|| order.first().map(|&i| &session.peers()[i]))
    }

    fn sync_peer_detail(&mut self, cx: &mut Cx, at: SessionRef) {
        let peer = self.shown_peer(at).cloned();
        self.view.view(cx, ids!(peers_empty)).set_visible(cx, peer.is_none());
        self.view.view(cx, ids!(peer_detail)).set_visible(cx, peer.is_some());
        let Some(p) = peer else {
            for id in [ids!(peer_composer), ids!(d_question_row), ids!(d_approval_row)] {
                self.view.view(cx, id).set_visible(cx, false);
            }
            return;
        };
        self.view.label(cx, ids!(d_title)).set_text(cx, &format!("{} · {}", p.role(), p.slug));
        // Its budget, filled when it is first shown (not while the person types).
        if self.budget_of.as_deref() != Some(p.id.as_str()) {
            self.budget_of = Some(p.id.clone());
            self.show_move = false;
            let budget = p.budget.unwrap_or_default();
            self.view.text_input(cx, ids!(d_budget_steps)).set_text(cx, &budget.steps.map(|s| s.to_string()).unwrap_or_default());
            self.view.text_input(cx, ids!(d_budget_cost)).set_text(cx, &budget.cost.map(|c| format!("{c:.2}")).unwrap_or_default());
        }
        // The other outer loops of its project it can be handed to.
        self.move_targets = self.store.projects[at.0].sessions.iter().filter(|s| !s.peers().iter().any(|q| q.id == p.id))
            .take(4).map(|s| s.id.clone()).collect();
        self.view.view(cx, ids!(d_move_row)).set_visible(cx, self.show_move);
        for (i, id) in [live_id!(move_t0), live_id!(move_t1), live_id!(move_t2), live_id!(move_t3)].into_iter().enumerate() {
            let title = self.move_targets.get(i).and_then(|sid| self.store.projects[at.0].sessions.iter().find(|s| &s.id == sid)).map(|s| {
                let t: String = s.title.chars().take(16).collect();
                if s.title.chars().count() > 16 { format!("{t}…") } else { t }
            });
            self.view.button(cx, &[id]).set_visible(cx, title.is_some());
            if let Some(title) = title {
                self.view.button(cx, &[id]).set_text(cx, &title);
            }
        }
        let confirm = self.confirm_close.as_deref() == Some(p.id.as_str());
        self.view.button(cx, ids!(close_peer)).set_text(cx, if confirm { i18n::t("Stop it and close", "停止并关闭") } else { i18n::t("Close", "关闭") });
        let done = matches!(p.status.as_str(), "done" | "idle" | "merged");
        let halted = !matches!(p.status.as_str(), "queued" | "running" | "checking" | "done" | "idle" | "merged" | "failed");
        for (status, id) in [("queued", ids!(b_queued)), ("failed", ids!(b_failed))] {
            self.view.view(cx, id).set_visible(cx, p.status == status);
        }
        self.view.view(cx, ids!(b_running)).set_visible(cx, matches!(p.status.as_str(), "running" | "checking"));
        self.view.label(cx, ids!(b_running.label)).set_text(cx, status_word(&p.status));
        self.view.view(cx, ids!(b_done)).set_visible(cx, done);
        self.view.label(cx, ids!(b_done.label)).set_text(cx, status_word(&p.status));
        self.view.view(cx, ids!(b_halted)).set_visible(cx, halted);
        self.view.label(cx, ids!(b_halted.label)).set_text(cx, status_word(&p.status));
        let elapsed = elapsed(p.finished_at.unwrap_or_else(now_secs).saturating_sub(p.started_at));
        let place = p.dir.clone();
        let used = match &p.usage {
            Some(u) => {
                let ctx = u.context_window.map(|w| format!(" of {} context", tokens(w))).unwrap_or_default();
                format!(" · {} in / {} out{ctx} · ${:.4}", tokens(u.input), tokens(u.output), u.cost)
            }
            None => String::new(),
        };
        let running = self.rt.running_agents.get(&p.id).copied().unwrap_or(0);
        let subagents = if running > 0 { i18n::pick(format!(" · {running} subagent(s) running"), format!(" · {running} 个子 agent 运行中")) } else { String::new() };
        let _ = place;
        let wave = p.wave.map(|w| format!(" · wave {w}")).unwrap_or_default();
        // An estimated round (agent-estimation) is a write-run-verify cycle
        // of ~3 minutes; octos counts steps (model calls), several a round.
        // Against it: the time its turns really took, and what this
        // project's finished slices say a round takes (calibration).
        let actual = estimate::actual_minutes(&p, now_secs()).filter(|m| *m > 0.0)
            .map(|m| i18n::pick(format!(", took {}", estimate::minutes_text(m)), format!("，实际用时 {}", estimate::minutes_text(m)))).unwrap_or_default();
        let took = p.rounds_used.map(|u| i18n::pick(format!("{actual}, {u} steps"), format!("{actual}，{u} 步"))).unwrap_or(actual);
        let calibrated = estimate::calibrate(self.store.projects[at.0].sessions.iter());
        let rounds = match p.estimate {
            Some(est) => {
                let cal = calibrated.map(|c| i18n::pick(format!(", calibrated ~{}", estimate::minutes_text(est * c.min_per_round)), format!("，校准后约 {}", estimate::minutes_text(est * c.min_per_round)))).unwrap_or_default();
                i18n::pick(format!(" · est. {est} rounds (~{:.0} min{cal}){took}", est * 3.0), format!(" · 估算 {est} 轮（约 {:.0} 分钟{cal}）{took}", est * 3.0))
            }
            None => took.replacen(", ", " · ", 1).replacen("，", " · ", 1),
        };
        let meta = i18n::pick(format!("round {}{wave}{rounds} · {elapsed}{used}{subagents}", p.round), format!("第 {} 轮{wave}{rounds} · {elapsed}{used}{subagents}", p.round));
        self.view.label(cx, ids!(d_meta)).set_text(cx, &meta);
        let activity = p.activity.as_deref().filter(|_| p.is_active()).unwrap_or("");
        self.view.view(cx, ids!(d_activity_row)).set_visible(cx, !activity.is_empty());
        self.view.label(cx, ids!(d_activity)).set_text(cx, activity);
        self.sync_result(cx, at, &p);
        // What it runs, under its input: octos, and the model (Z.ai's mark for GLM).
        self.view.label(cx, ids!(inner_agent)).set_text(cx, p.agent());
        let model = p.model.clone().unwrap_or_else(|| i18n::t("model not reported yet", "模型尚未上报").into());
        let effort = p.effort.as_deref().map(|e| i18n::pick(format!("{e} effort"), format!("{e} effort"))).unwrap_or_else(|| i18n::t("default effort", "默认 effort").into());
        self.view.label(cx, ids!(inner_model)).set_text(cx, &format!("{model} · {effort}"));
        // The provider's mark: the one it was put on, else the model's maker.
        let family = p.model_pick.as_deref().and_then(|m| m.split_once('/')).map(|(f, _)| f)
            .unwrap_or_else(|| provider_icons::family_of_model(&model));
        let icon = self.view.widget(cx, ids!(inner_model_icon));
        icon.set_visible(cx, !family.is_empty());
        provider_icons::show(&icon, provider_icons::svg(family), &mut self.icons_shown);
        self.view.portal_list(cx, ids!(peer_messages)).redraw(cx);
        let waiting: Vec<(String, String, String)> = self.rt.waiting(&p.id).into_iter().map(|(id, who, text)| (id, who.to_string(), text)).collect();
        self.inner_queue_ids = self.sync_queue(cx, live_id!(inner_queue), &waiting);
        let question = self.rt.questions.get(&p.id).cloned();
        self.view.view(cx, ids!(d_question_row)).set_visible(cx, question.is_some());
        if let Some((_, text, options)) = &question {
            let options = if options.is_empty() { String::new() } else { format!("\nOptions: {}", options.join(" · ")) };
            self.view.label(cx, ids!(d_question)).set_text(cx, &format!("{} asks you:\n{text}{options}\nType your answer below and press Send.", p.slug));
        }
        let approval = self.rt.approvals.get(&p.id).cloned();
        self.view.view(cx, ids!(d_approval_row)).set_visible(cx, approval.is_some());
        if let Some((_, title, body)) = approval {
            self.view.label(cx, ids!(d_approval_title)).set_text(cx, &format!("{} needs your approval: {title}", p.slug));
            self.view.label(cx, ids!(d_approval)).set_text(cx, &body);
        }
        self.view.view(cx, ids!(peer_composer)).set_visible(cx, true);
        self.view.button(cx, ids!(peer_interrupt)).set_visible(cx, p.status == "running");
    }

    /// The session's worktree: what its branch holds against the project's,
    /// and Merge / Discard.
    fn sync_worktree_bar(&mut self, cx: &mut Cx, at: SessionRef) {
        let project = self.store.projects[at.0].path.clone();
        let Some(session) = self.store.session(at).cloned() else { return };
        let (Some(dir), Some(branch)) = (session.work_dir.clone(), session.work_branch.clone()) else {
            self.view.view(cx, ids!(worktree_bar)).set_visible(cx, false);
            return;
        };
        self.view.view(cx, ids!(worktree_bar)).set_visible(cx, true);
        let busy = self.session_busy(at);
        let state = self.rt.worktrees.entry(session.id.clone()).or_insert_with(|| {
            let diff = workspace::branch_diff(&project, &branch);
            let loose = workspace::uncommitted(&dir);
            (diff, loose)
        }).clone();
        let (text, ready) = match state {
            (_, _) if !std::path::Path::new(&dir).is_dir() => (i18n::pick(format!("Worktree {branch} was removed. The next message starts a new one from the project's current commit."), format!("Worktree {branch} 已删除。下一条消息会从项目当前提交新建一个。")), false),
            (Err(err), _) => (err, false),
            (Ok(d), loose) => {
                let loose = if loose.is_empty() { String::new() } else { i18n::pick(format!(" · {} uncommitted file(s): {}", loose.len(), loose.join(", ")), format!(" · {} 个未提交文件：{}", loose.len(), loose.join("、"))) };
                if d.merged {
                    (i18n::pick(format!("Worktree on {branch}: nothing to merge into {} yet{loose}.", d.into), format!("Worktree（{branch}）：暂无可合并到 {} 的内容{loose}。", d.into)), false)
                } else {
                    let files: Vec<String> = d.files.iter().map(|(a, r, p)| format!("{p} +{a} -{r}")).collect();
                    (i18n::pick(format!("Worktree on {branch}: {} commit(s) to merge into {} ({}){loose}.", d.commits.len(), d.into, files.join(", ")),
                        format!("Worktree（{branch}）：{} 个提交可合并到 {}（{}）{loose}。", d.commits.len(), d.into, files.join("、"))), true)
                }
            }
        };
        self.view.label(cx, ids!(worktree_info)).set_text(cx, &text);
        let merge = self.view.button(cx, ids!(merge_worktree));
        merge.set_visible(cx, ready && !busy);
        merge.set_text(cx, i18n::t("Merge into the project", "合并到项目"));
        let discard = self.view.button(cx, ids!(discard_worktree));
        discard.set_visible(cx, !busy && std::path::Path::new(&dir).is_dir());
        discard.set_text(cx, if self.confirm_worktree.as_deref() == Some(session.id.as_str()) { i18n::t("Confirm: discard the worktree", "确认：丢弃 worktree") } else { i18n::t("Discard worktree", "丢弃 worktree") });
    }

    /// The interface's own words in the current language (what the DSL
    /// says is the English).
    fn apply_language(&mut self, cx: &mut Cx) {
        use i18n::t;
        let labels: [(&[LiveId], &str); 12] = [
            (ids!(projects_title), t("Projects", "项目")),
            (ids!(inner_title), t("Inner", "Inner")),
            (ids!(approvals_label), t("Approvals", "审批")),
            (ids!(peers_empty_text), t("No inner loops yet. They start when the outer loop sends a plan.", "还没有 inner。outer 发出计划后会自动启动。")),
            (ids!(settings_title), t("Settings", "设置")),
            (ids!(providers_title), t("AI Providers", "AI Providers")),
            (ids!(inner_model_title), t("Inner loop model", "Inner 使用的模型")),
            // What it is: OctoSense's app inside OctoSense, an app of its own on the host.
            (ids!(about_body), if system::hosted() {
                t("The OctoBuddy two-loop workflow as an OctoSense app. You talk to the outer loop, which plans the work, splits it into slices and reviews the results; inner loops do the slices. All of them work in the same directory: the project, or the session's git worktree. OctoBuddy commits each inner loop's own files, as you.",
                    "OctoBuddy 双环工作流的 OctoSense 应用。你和 outer 对话，它规划工作、拆成切片并审查结果；inner 完成各个切片。它们都在同一个目录里工作：项目目录，或会话的 git worktree。OctoBuddy 以你的身份提交每个 inner 自己改过的文件。")
            } else {
                t("OctoBuddy, the two-loop coding app, on this computer. You talk to the outer loop, which plans the work, splits it into slices and reviews the results; inner loops do the slices. All of them work in the same directory: the project, or the session's git worktree. OctoBuddy commits each inner loop's own files, as you.",
                    "OctoBuddy 双环编码应用，运行在这台电脑上。你和 outer 对话，它规划工作、拆成切片并审查结果；inner 完成各个切片。它们都在同一个目录里工作：项目目录，或会话的 git worktree。OctoBuddy 以你的身份提交每个 inner 自己改过的文件。")
            }),
            (ids!(loops_title), t("Loops", "双环")),
            (ids!(data_title), t("Data on this device", "本机数据")),
            (ids!(language_title), t("Language", "语言")),
            (ids!(language_hint), t("OctoBuddy's own words. What the agents write stays in their language.", "只影响 OctoBuddy 自己的界面文字；agent 写的内容保持原样。")),
        ];
        for (id, text) in labels {
            self.view.label(cx, id).set_text(cx, text);
        }
        let buttons: [(&[LiveId], &str); 28] = [
            (ids!(stop), t("Stop", "停止")),
            (ids!(go_on), t("Go on", "继续")),
            (ids!(send), t("Send", "发送")),
            (ids!(peer_send), t("Send", "发送")),
            (ids!(peer_interrupt), t("Interrupt & send", "打断并发送")),
            (ids!(approvals_ask_on), t("Ask", "询问")),
            (ids!(approvals_ask), t("Ask", "询问")),
            (ids!(approvals_auto_on), t("Auto", "自动")),
            (ids!(approvals_auto), t("Auto", "自动")),
            (ids!(recheck), t("Run checks", "重新检查")),
            (ids!(approve), t("Approve", "批准")),
            (ids!(approve_session), t("Always in this session", "本会话内都允许")),
            (ids!(deny), t("Deny", "拒绝")),
            (ids!(reload_providers), t("Reload", "重新载入")),
            (ids!(add_provider), t("+ Add a provider", "+ 添加供应商")),
            (ids!(nav_plugins_on), t("Plugins", "插件")),
            (ids!(nav_plugins), t("Plugins", "插件")),
            (ids!(nav_tools_on), t("Tools", "工具")),
            (ids!(nav_tools), t("Tools", "工具")),
            (ids!(open_plugins), t("Open folder", "打开文件夹")),
            (ids!(reload_plugins), t("Rescan", "重新扫描")),
            (ids!(reload_tools), t("Check again", "重新检测")),
            (ids!(nav_language_on), t("Language", "语言")),
            (ids!(nav_appearance_on), t("Appearance", "外观")),
            (ids!(nav_appearance), t("Appearance", "外观")),
            (ids!(nav_language), t("Language", "语言")),
            (ids!(nav_about_on), t("About", "关于")),
            (ids!(nav_about), t("About", "关于")),
        ];
        for (id, text) in buttons {
            self.view.button(cx, id).set_text(cx, text);
        }
        self.view.widget(cx, ids!(use_worktree)).set_text(cx, t("Use a git worktree", "使用 git worktree"));
        self.view.label(cx, ids!(b_queued.label)).set_text(cx, status_word("queued"));
        self.view.label(cx, ids!(b_failed.label)).set_text(cx, status_word("failed"));
        self.view.text_input(cx, ids!(composer)).set_empty_text(cx, t("Message the outer loop  (Enter to send, Shift+Enter for a new line)", "给 outer 发消息（Enter 发送，Shift+Enter 换行）").into());
        self.view.label(cx, ids!(preview_title)).set_text(cx, t("App workbench", "应用工作台"));
        self.view.button(cx, ids!(reload_preview)).set_text(cx, t("Reload", "重新加载"));
        self.view.button(cx, ids!(preview_fix)).set_text(cx, t("Ask the outer loop to fix it", "让 outer 修复"));
        self.view.button(cx, ids!(a_project)).set_text(cx, t("Open a project folder…", "打开项目文件夹…"));
        self.view.button(cx, ids!(a_app)).set_text(cx, t("New OctoSense app…", "新建 OctoSense 应用…"));
        self.view.label(cx, ids!(new_name_label)).set_text(cx, t("Name", "名字"));
        self.view.label(cx, ids!(new_role_label)).set_text(cx, t("Role", "角色"));
        self.view.label(cx, ids!(new_model_label)).set_text(cx, t("Model", "模型"));
        self.view.label(cx, ids!(new_outer_label)).set_text(cx, t("Outer loop it works for", "属于哪个外环"));
        self.view.text_input(cx, ids!(new_task)).set_empty_text(cx, t("Its first task (optional: you can message it later)", "它的第一个任务（可留空，之后再给它发消息）").into());
        self.view.button(cx, ids!(new_create)).set_text(cx, t("Create", "创建"));
        let soon = if workspace::find_bin("pi").is_file() || agents::installable("pi") { "" } else { t("pi is not installed (Settings › Tools says where it comes from).", "pi 未安装（来源见 设置 › 工具）。") };
        self.view.label(cx, ids!(pick_soon)).set_text(cx, soon);
        self.view.label(cx, ids!(data_title)).set_text(cx, t("Data for the app: the outer loop builds the app around it", "应用的数据：outer 会围绕它来做应用"));
        self.view.button(cx, ids!(data_csv)).set_text(cx, t("Choose a CSV file…", "选择 CSV 文件…"));
        self.view.text_input(cx, ids!(data_url)).set_empty_text(cx, t("https://… (a JSON API)", "https://…（返回 JSON 的 API）").into());
        self.view.button(cx, ids!(data_add_api)).set_text(cx, t("Add API", "添加 API"));
        self.view.button(cx, ids!(flow_popup_chat)).set_text(cx, t("Open in chat", "在对话中打开"));
        self.view.label(cx, ids!(d_budget_label)).set_text(cx, t("Budget", "预算"));
        self.view.text_input(cx, ids!(d_budget_steps)).set_empty_text(cx, t("steps", "步数").into());
        self.view.button(cx, ids!(d_budget_set)).set_text(cx, t("Set", "设置"));
        self.view.button(cx, ids!(d_move)).set_text(cx, t("Hand over", "移交"));
        self.view.label(cx, ids!(d_move_label)).set_text(cx, t("To outer loop:", "移交给外环："));
        self.view.button(cx, ids!(move_new)).set_text(cx, t("New outer loop", "新外环"));
        self.view.button(cx, ids!(flow_send)).set_text(cx, t("Send", "发送"));
        self.view.label(cx, ids!(flow_budget_label)).set_text(cx, t("Budget", "预算"));
        self.view.text_input(cx, ids!(flow_budget_steps)).set_empty_text(cx, t("steps", "步数").into());
        self.view.button(cx, ids!(flow_budget_set)).set_text(cx, t("Set", "设置"));
        self.view.button(cx, ids!(flow_budget_clear)).set_text(cx, t("No limit", "不限"));
        self.view.text_input(cx, ids!(peer_input)).set_empty_text(cx, t("Message this inner loop (queued if it is busy)", "给这个 inner 发消息（它忙时会排队）").into());
    }

    /// The person picks the interface's language; kept with the projects.
    fn set_language(&mut self, cx: &mut Cx, lang: i18n::Lang) {
        i18n::set(lang);
        self.store.lang = Some(lang.code().to_string());
        self.save();
        self.apply_language(cx);
        self.relayout(cx);
    }

    /// A queue panel: its rows in running order; returns the item ids shown.
    fn sync_queue(&mut self, cx: &mut Cx, panel: LiveId, items: &[(String, String, String)]) -> Vec<String> {
        self.view.view(cx, &[panel]).set_visible(cx, !items.is_empty());
        let head = i18n::pick(format!("Up next ({}) — runs top to bottom", items.len()), format!("待执行（{}）— 从上到下依次执行", items.len()));
        self.view.label(cx, &[panel, live_id!(head)]).set_text(cx, &head);
        for (i, row) in QUEUE_ROWS.iter().enumerate() {
            let item = items.get(i);
            self.view.view(cx, &[panel, *row]).set_visible(cx, item.is_some());
            if let Some((_, who, text)) = item {
                self.view.label(cx, &[panel, *row, live_id!(num)]).set_text(cx, &format!("{}.", i + 1));
                self.view.label(cx, &[panel, *row, live_id!(who)]).set_text(cx, who_word(who));
                self.view.label(cx, &[panel, *row, live_id!(text)]).set_text(cx, &text.replace('\n', " "));
            }
        }
        let more = items.len().saturating_sub(QUEUE_ROWS.len());
        self.view.label(cx, &[panel, live_id!(more)]).set_text(cx, &if more > 0 { i18n::pick(format!("and {more} more"), format!("还有 {more} 条")) } else { String::new() });
        items.iter().take(QUEUE_ROWS.len()).map(|(id, _, _)| id.clone()).collect()
    }

    /// What the peer's work came to: the checks' verdict, the outer loop's
    /// call, OctoBuddy's commits of its files, its subagents; and Run checks /
    /// Show diff (its own files, since it started).
    fn sync_result(&mut self, cx: &mut Cx, _at: SessionRef, p: &Peer) {
        let settled = !p.is_active();
        let touched = p.touched.clone().unwrap_or_default();
        let diff = match (&p.base, touched.is_empty()) {
            (Some(base), false) if settled => Some(self.rt.diffs.entry(p.id.clone())
                .or_insert_with(|| workspace::peer_diff(&p.dir, base, &touched, p.commits.as_deref().unwrap_or(&[]))).clone()),
            _ => None,
        };
        let mut verdict: Vec<String> = Vec::new();
        if let Some(review) = &p.review {
            verdict.push(i18n::pick(format!("Outer: {review}"), format!("Outer 结论：{review}")));
        }
        if let Some(v) = &p.verdict {
            verdict.extend(v.lines().filter(|l| l.starts_with('[')).map(str::to_string));
        }
        if let Some(cmd) = p.check.as_deref().filter(|_| p.verdict.is_none()) {
            verdict.push(i18n::pick(format!("Tests: OctoBuddy runs `{cmd}` when it is done"), format!("测试：完成后 OctoBuddy 会运行 `{cmd}`")));
        }
        let loose = self.rt.uncommitted(&p.id);
        if !loose.is_empty() {
            verdict.push(i18n::pick(format!("Not committed yet: {}", loose.join(", ")), format!("尚未提交：{}", loose.join("、"))));
        }
        for a in p.subagents.as_deref().unwrap_or(&[]) {
            let summary = if a.summary.is_empty() { String::new() } else { format!(": {}", a.summary.lines().next().unwrap_or("")) };
            let role = if a.role.is_empty() { String::new() } else { format!("{} · ", a.role) };
            verdict.push(i18n::pick(format!("Subagent {role}{} — {}{summary}", a.title, a.status), format!("子 agent {role}{} — {}{summary}", a.title, a.status)));
        }
        let changes = match &diff {
            None => String::new(),
            Some(Err(err)) => err.clone(),
            Some(Ok(d)) => {
                let mut out = String::new();
                if !d.commits.is_empty() {
                    out.push_str(&i18n::pick(format!("{} commit(s) of its work:\n", d.commits.len()), format!("它的 {} 个提交：\n", d.commits.len())));
                    for c in d.commits.iter().rev().take(8) {
                        out.push_str(&format!("  {c}\n"));
                    }
                    out.push('\n');
                }
                for (add, del, path) in &d.files {
                    out.push_str(&format!("  +{add:<4} -{del:<4} {path}\n"));
                }
                if d.files.is_empty() {
                    let base = p.base.as_deref().map(|b| &b[..b.len().min(7)]).unwrap_or("");
                    out.push_str(&i18n::pick(format!("Its files are as they were at {base}: {}", touched.join(", ")), format!("它的文件与 {base} 时相同：{}", touched.join("、"))));
                }
                out.trim_end().to_string()
            }
        };
        let patch = match &diff { Some(Ok(d)) => d.patch.clone(), _ => String::new() };
        let can_check = settled && (p.contract.is_some() || p.check.is_some());
        self.view.label(cx, ids!(d_summary)).set_text(cx, &result_summary(p, loose.len()));
        let details = self.show_details.contains(&p.id);
        self.view.view(cx, ids!(d_details)).set_visible(cx, details);
        self.view.button(cx, ids!(toggle_details)).set_text(cx, if details { i18n::t("Hide", "收起") } else { i18n::t("Details", "详情") });
        let open = diff.is_some() || !verdict.is_empty();
        self.view.view(cx, ids!(d_changes_row)).set_visible(cx, open);
        self.view.label(cx, ids!(d_verdict)).set_text(cx, &verdict.join("\n"));
        self.view.label(cx, ids!(d_changes)).set_text(cx, &changes);
        self.view.view(cx, ids!(d_actions)).set_visible(cx, can_check || !patch.is_empty());
        self.view.button(cx, ids!(recheck)).set_visible(cx, can_check);
        let toggle = self.view.button(cx, ids!(toggle_diff));
        toggle.set_visible(cx, !patch.is_empty());
        let shown = self.show_diff.contains(&p.id) && !patch.is_empty();
        toggle.set_text(cx, if shown { i18n::t("Hide diff", "收起 diff") } else { i18n::t("Show diff", "查看 diff") });
        self.view.view(cx, ids!(d_patch_row)).set_visible(cx, shown);
        self.view.label(cx, ids!(d_patch)).set_text(cx, if shown { &patch } else { "" });
    }

    /// Opens the platform's folder picker; the answer comes back as a
    /// `FileDialogAction` (see `handle_actions`).
    fn pick_project_folder(&mut self, cx: &mut Cx) {
        self.picking_folder = true;
        let title = if self.picking_app {
            i18n::t("Choose an empty folder for the new OctoSense app", "为新的 OctoSense 应用选择一个空文件夹")
        } else {
            i18n::t("Choose a project folder", "选择项目文件夹")
        };
        let mut dialog = FileDialog::new().set_title(title.into());
        if let Some(home) = std::env::var_os("HOME") {
            dialog = dialog.set_location(PathBuf::from(home));
        }
        cx.open_select_folder_dialog(dialog);
    }

    fn add_project(&mut self, cx: &mut Cx, path: &str) {
        if !model::is_project_dir(path) {
            // No session to say it in yet: say it where the hint is.
            self.view.label(cx, ids!(placeholder_hint)).set_text(cx,
                &format!("{path} cannot be a project: choose a project folder, not your home folder or the disk root."));
            return;
        }
        if let Some(pi) = self.store.add_project(path) {
            // A new project opens on its first session; a known one on its latest.
            let sessions = self.store.projects[pi].sessions.len();
            self.selected = if sessions == 0 { self.store.add_session(pi) } else { Some((pi, sessions - 1)) };
            self.page = Page::Chat;
            self.save();
            self.relayout(cx);
        }
    }

    /// A new OctoSense app in `dir` (empty or new), or an app's own folder.
    fn add_app(&mut self, cx: &mut Cx, dir: &std::path::Path) {
        let path = dir.to_string_lossy().into_owned();
        if !plugins::octosense_app::is_app(&path) {
            if let Err(err) = plugins::octosense_app::create(dir) {
                self.say(cx, &i18n::pick(format!("Could not make an OctoSense app there: {err}"), format!("无法在那里新建 OctoSense 应用：{err}")));
                return;
            }
        }
        self.add_project(cx, &path);
        self.show_preview = true;
        self.show_inner = false;
        self.relayout(cx);
        self.view.text_input(cx, ids!(composer)).set_key_focus(cx);
    }

    /// Publishes the shown app project to the local App Hub, off the UI thread.
    fn publish_app(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        if self.publishing.is_some() {
            return;
        }
        let project = self.store.projects[at.0].path.clone();
        let session = self.store.session(at).unwrap().id.clone();
        self.publishing = Some(session.clone());
        self.system(at, i18n::t("Publishing it to the local App Hub: a signed copy of the bundle, its screenshots taken, App Hub's gate run…",
            "正在发布到本地 App Hub：签名一份 bundle 副本、补截图、跑 App Hub 的检查……"));
        let inbox = self.rt.inbox.clone();
        std::thread::spawn(move || {
            let result = plugins::app_publish::publish(&project);
            events::post(&inbox, events::LoopEvent::Published { session, result });
        });
        self.relayout(cx);
    }

    /// Reads a data source for the shown app project, off the UI thread (a
    /// big file, a slow API); `DataFound` brings it back.
    fn add_data(&mut self, cx: &mut Cx, from: DataFrom) {
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        let project = self.store.projects[at.0].path.clone();
        let session = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
        let note = self.view.label(cx, ids!(data_note));
        note.set_text(cx, i18n::t("Reading it…", "正在读取…"));
        note.set_visible(cx, true);
        let inbox = self.rt.inbox.clone();
        std::thread::spawn(move || {
            let (found, commit) = match from {
                DataFrom::Csv(file) => {
                    let found = plugins::app_data::add_csv(&project, &file);
                    // Its data line is OctoBuddy's change: committed as such.
                    let commit = found.as_ref().ok().and_then(|f| {
                        workspace::commit_files(&project, &["bundle/main.splash".to_string()], &format!("chore(data): 加入数据 {}", f.name)).ok()
                    });
                    (found, commit)
                }
                DataFrom::Api(url) => (plugins::app_data::add_api(&project, &url), None),
            };
            events::post(&inbox, events::LoopEvent::DataFound { session, found, commit });
        });
    }

    /// A line for the person: in the session shown, else under the placeholder.
    fn say(&mut self, cx: &mut Cx, text: &str) {
        match self.selected.filter(|at| self.store.session(*at).is_some()) {
            Some(at) => {
                self.system(at, text);
                self.save();
                self.relayout(cx);
            }
            None => self.view.label(cx, ids!(placeholder_hint)).set_text(cx, text),
        }
    }

    /// Runs the shown app project's app in the preview panel when it is open
    /// and its files changed since it last started; stops it when the panel
    /// closes or the project is not an app.
    fn sync_preview(&mut self, cx: &mut Cx) {
        let project = self.selected.filter(|at| self.store.session(*at).is_some())
            .map(|at| self.store.projects[at.0].path.clone())
            .filter(|path| plugins::active(plugins::OCTOSENSE_APP, path));
        let splash = self.view.splash(cx, ids!(preview));
        let Some(project) = project.filter(|_| self.show_preview && self.page == Page::Chat) else {
            if self.preview.take().is_some() {
                plugins::app_preview::stop(cx, &splash);
            }
            self.preview_of = None;
            self.preview_errors.clear();
            return;
        };
        let stamp = plugins::app_preview::stamp(&project);
        if self.preview_of.as_ref() == Some(&(project.clone(), stamp)) {
            return;
        }
        let same_app = self.preview_of.as_ref().is_some_and(|(p, _)| *p == project);
        if !same_app && self.preview.take().is_some() {
            plugins::app_preview::stop(cx, &splash);
        }
        self.preview_of = Some((project.clone(), stamp));
        match plugins::app_preview::start(cx, &splash, &project) {
            Ok(running) => {
                self.preview = Some(running);
                self.preview_errors.clear();
            }
            Err(errors) => self.preview_errors = errors,
        }
        self.show_preview_state(cx);
    }

    fn show_preview_state(&mut self, cx: &mut Cx) {
        let info = match &self.preview {
            Some(r) => {
                let caps = if r.capabilities.is_empty() { i18n::t("no capabilities", "无权限").to_string() } else { r.capabilities.join(", ") };
                i18n::pick(format!("{} {} · {caps} · runs from the project's files, reloaded when they change", r.name, r.version),
                    format!("{} {} · {caps} · 直接运行项目里的文件，文件一变就重新加载", r.name, r.version))
            }
            None => i18n::t("Not running.", "未运行。").to_string(),
        };
        self.view.label(cx, ids!(preview_info)).set_text(cx, &info);
        let problem = !self.preview_errors.is_empty();
        self.view.view(cx, ids!(preview_problem)).set_visible(cx, problem);
        if problem {
            let title = if self.preview.is_some() {
                i18n::t("The files as they are now do not run; the last version that did still runs:", "当前文件无法运行；仍在运行上一个能跑的版本：")
            } else {
                i18n::t("The app does not run:", "应用无法运行：")
            };
            self.view.label(cx, ids!(preview_problem_title)).set_text(cx, title);
            self.view.label(cx, ids!(preview_errors)).set_text(cx, &self.preview_errors.join("\n"));
        }
        self.view.redraw(cx);
    }

    /// The preview's errors go to the session's outer loop, as the person's message.
    fn ask_to_fix_preview(&mut self, cx: &mut Cx) {
        let Some(at) = self.selected.filter(|at| self.store.session(*at).is_some()) else { return };
        if self.preview_errors.is_empty() {
            return;
        }
        let text = format!("The app does not run in OctoBuddy's preview. Fix it:\n```\n{}\n```", self.preview_errors.join("\n"));
        self.send_text(cx, at, text);
    }

    /// The shown project's loops as cards: each outer loop (a session), and
    /// its open inner loops beside it. Unplaced cards go in a column each.
    fn flow_cards(&mut self, pi: usize) -> Vec<(flow::Card, [String; 6])> {
        let mut out = Vec::new();
        let project = self.store.projects[pi].clone();
        let shown = self.selected.and_then(|at| self.store.session(at)).map(|s| s.id.clone());
        let open_peer = match &self.flow_open { Some(FlowOpen::Inner(id)) => Some(id.clone()), _ => None };
        let (outer_tag, peer_tag, sub_tag) = ("outer".to_string(), "peer agent".to_string(), "subagent".to_string());
        // What waits for a loop, a line each (at most three), under its card.
        let queue_text = |items: &[(String, String, String)]| -> String {
            let mut lines: Vec<String> = items.iter().take(3).enumerate()
                .map(|(i, (_, who, text))| format!("{}. {}: {}", i + 1, who_word(who), text.lines().next().unwrap_or("").chars().take(44).collect::<String>()))
                .collect();
            if items.len() > 3 {
                lines.push(i18n::pick(format!("and {} more", items.len() - 3), format!("还有 {} 条", items.len() - 3)));
            }
            lines.join("\n")
        };
        let grow = |n: usize| if n == 0 { 0.0 } else { 8.0 + 14.0 * n.min(4) as f64 };
        // Below the hint and the floating buttons.
        let mut y = 84.0;
        for session in &project.sessions {
            let mut peers: Vec<&Peer> = session.peers().iter().filter(|p| p.status != "closed").collect();
            // A round's waves together and in order, as their gates join them.
            peers.sort_by_key(|p| (p.round, p.wave.unwrap_or(1)));
            let detached = session.is_detached();
            // Its subagents, while they run (a subagent is used once and gone).
            let subs: Vec<model::Step> = session.messages.iter().rev().take(6).filter_map(|m| m.steps.as_ref()).flatten()
                .filter(|st| matches!(st.name.as_str(), "Agent" | "Task") && st.status == "running").cloned().collect();
            let pos = session.flow.map(|f| dvec2(f.x, f.y)).unwrap_or(dvec2(30.0, y));
            let waiting = if detached { Vec::new() } else { self.outer_waiting(&session.id) };
            let outer_h = 106.0 + grow(waiting.len().min(3) + usize::from(waiting.len() > 3));
            let tall = (peers.len() as f64 * 110.0).max(outer_h + subs.len() as f64 * 66.0 + if subs.is_empty() { 0.0 } else { 20.0 });
            y = y.max(pos.y) + tall + 36.0;
            let outer = LiveId::from_str(&format!("s:{}", session.id));
            if !detached {
                let lead = self.lead_status.get(&session.id).cloned().unwrap_or_else(|| "idle".into());
                let (chip, engine) = self.rt.outer_label(session, &project.path, status_word(&lead));
                let counts = if peers.is_empty() { i18n::t("Inner · none yet", "Inner · 暂无").to_string() } else { format!("Inner · {}", PeerCounts::of(session.peers()).short()) };
                let lit = shown.as_deref() == Some(session.id.as_str());
                out.push((flow::Card {
                    id: outer, key: format!("s:{}", session.id), template: if lit { live_id!(OuterLit) } else { live_id!(Outer) },
                    pos, size: dvec2(240.0, outer_h), parent: None, link: flow::Link::Peer, active: false, movable_to_outer: false, lit, wave: None, rewave: false,
                }, [session.title.clone(), chip, counts, outer_tag.clone(), queue_text(&waiting), outer_short(&engine)]));
                for (k, st) in subs.iter().enumerate() {
                    let what: String = st.detail.chars().take(60).collect();
                    out.push((flow::Card {
                        id: LiveId::from_str(&format!("a:{}:{}", session.id, st.id)), key: format!("s:{}", session.id), template: live_id!(Sub),
                        pos: pos + dvec2(24.0, outer_h + 20.0 + k as f64 * 66.0), size: dvec2(190.0, 56.0), parent: Some(outer),
                        link: flow::Link::Sub, active: true, movable_to_outer: false, lit: false, wave: None, rewave: false,
                    }, [i18n::t("Outer's subagent", "outer 的子 agent").to_string(), status_word("running").to_string(), what, sub_tag.clone(), String::new(), String::new()]));
                }
            }
            for (j, p) in peers.iter().enumerate() {
                let waiting: Vec<(String, String, String)> = self.rt.waiting(&p.id).into_iter().map(|(id, who, text)| (id, who.to_string(), text)).collect();
                let h = 90.0 + grow(waiting.len().min(3) + usize::from(waiting.len() > 3));
                // On its own: a column of its own, at the left.
                let at = p.flow.map(|f| dvec2(f.x, f.y)).unwrap_or(if detached { dvec2(30.0, pos.y + j as f64 * 110.0) } else { pos + dvec2(300.0, j as f64 * 110.0) });
                let running = p.is_active();
                let chosen = open_peer.as_deref() == Some(p.id.as_str());
                let template = match (chosen, running) {
                    (true, true) => live_id!(InnerSelLit),
                    (true, false) => live_id!(InnerSel),
                    (false, true) => live_id!(InnerLit),
                    (false, false) => live_id!(Inner),
                };
                let activity = p.activity.as_deref().unwrap_or("").lines().next().unwrap_or("").chars().take(40).collect::<String>();
                let status = if activity.is_empty() { format!("{} · {}", p.role(), status_word(&p.status)) } else { format!("{} · {} · {activity}", p.role(), status_word(&p.status)) };
                let id = LiveId::from_str(&format!("p:{}", p.id));
                let tag = if detached { i18n::t("peer agent · on its own", "peer agent · 未挂外环").to_string() } else { peer_tag.clone() };
                out.push((flow::Card {
                    id, key: format!("p:{}", p.id), template,
                    pos: at, size: dvec2(220.0, h), parent: (!detached).then_some(outer), link: flow::Link::Peer, active: running, movable_to_outer: true, lit: running || chosen,
                    wave: (!detached).then(|| (p.round as u32, p.wave.unwrap_or(1))), rewave: self.rt.held.contains_key(&p.id),
                }, [p.slug.clone(), status, budget_line(p), tag, queue_text(&waiting), inner_short(p)]));
                // Its subagents that still run, to its right: the ones octos
                // reports, and a spawn it waits on inside a tool call.
                let mut live: Vec<model::Subagent> = p.subagents.iter().flatten()
                    .filter(|a| !matches!(a.status.as_str(), "done" | "completed" | "complete" | "failed" | "error" | "cancelled" | "closed"))
                    .cloned().collect();
                if let Some(open) = p.log().last().filter(|e| e.outcome.is_none()) {
                    for st in open.steps.iter().flatten().filter(|st| st.status == "running" && matches!(st.name.as_str(), "spawn" | "spawn_agent" | "delegate" | "Agent" | "Task")) {
                        let task = st.detail.split("task:").nth(1).unwrap_or(&st.detail).trim().trim_matches('"').to_string();
                        live.push(model::Subagent { id: st.id.clone(), role: st.name.clone(), title: st.name.clone(), status: "running".into(), summary: task });
                    }
                }
                for (k, a) in live.iter().enumerate() {
                    let title = if a.title.is_empty() { a.role.clone() } else { a.title.clone() };
                    out.push((flow::Card {
                        id: LiveId::from_str(&format!("a:{}:{}", p.id, a.id)), key: format!("p:{}", p.id), template: live_id!(Sub),
                        pos: at + dvec2(260.0, k as f64 * 64.0), size: dvec2(190.0, 56.0), parent: Some(id),
                        link: flow::Link::Sub, active: true, movable_to_outer: false, lit: false, wave: None, rewave: false,
                    }, [title.chars().take(40).collect(), a.status.clone(), a.summary.chars().take(60).collect(), sub_tag.clone(), String::new(), String::new()]));
                }
            }
        }
        out
    }

    /// The flow view: its cards, and the open card's panel.
    fn sync_flow(&mut self, cx: &mut Cx) {
        let at = self.selected.filter(|at| self.store.session(*at).is_some());
        // A plain chat is a conversation only: no graph, no timeline.
        let plain = at.is_some_and(|at| self.store.is_plain(at));
        let shown = at.is_some() && self.page == Page::Chat && !plain;
        let flow = self.stage == Stage::Flow && shown;
        let time = self.stage == Stage::Time && shown;
        // The terminal stands in for the conversation, in a plain chat too.
        let tui = self.stage == Stage::Tui && at.is_some() && self.page == Page::Chat && self.tui.is_some();
        self.view.view(cx, ids!(chat_pane)).set_visible(cx, !flow && !time && !tui);
        self.view.view(cx, ids!(composer_area)).set_visible(cx, !flow && !time && !tui);
        self.view.view(cx, ids!(flow_view)).set_visible(cx, flow);
        self.view.view(cx, ids!(time_view)).set_visible(cx, time);
        self.view.view(cx, ids!(worktree_bar)).set_visible(cx, !flow && !time && !tui && self.view.view(cx, ids!(worktree_bar)).visible());
        // The view switcher: the one in use, marked.
        let current = if plain { Stage::Chat } else { self.stage };
        for (stage, off, on) in [(Stage::Chat, ids!(view_chat), ids!(view_chat_on)), (Stage::Flow, ids!(view_flow), ids!(view_flow_on)), (Stage::Time, ids!(view_time), ids!(view_time_on))] {
            self.view.view(cx, off).set_visible(cx, !plain && current != stage);
            self.view.view(cx, on).set_visible(cx, !plain && current == stage);
        }
        // The views need no line of their own: their names say it.
        for (id, name, sub) in [
            (ids!(view_time), i18n::t("Timeline", "时间轴"), ""),
            (ids!(view_time_on), i18n::t("Timeline", "时间轴"), ""),
            (ids!(view_flow_on), i18n::t("Flow", "流程图"), ""),
            (ids!(view_chat_on), i18n::t("Chat", "对话"), ""),
        ] {
            self.view.label(cx, &[id[0], live_id!(name)]).set_text(cx, name);
            self.view.label(cx, &[id[0], live_id!(sub)]).set_text(cx, sub);
        }
        if time {
            self.sync_time(cx);
        }
        self.sync_flow_menu(cx, flow);
        self.view.view(cx, ids!(new_peer_btn)).set_visible(cx, flow);
        self.view.label(cx, ids!(new_peer_btn.name)).set_text(cx, i18n::t("+ Peer", "+ Peer"));
        self.view.label(cx, ids!(new_peer_btn.sub)).set_text(cx, "");
        self.view.label(cx, ids!(view_flow.name)).set_text(cx, i18n::t("Flow", "流程图"));
        self.view.label(cx, ids!(view_flow.sub)).set_text(cx, "");
        self.view.label(cx, ids!(view_chat.name)).set_text(cx, i18n::t("Chat", "对话"));
        self.view.label(cx, ids!(view_chat.sub)).set_text(cx, "");
        let Some(at) = at.filter(|_| flow) else { return };
        let cards = self.flow_cards(at.0);
        let canvas = self.view.widget(cx, ids!(flow)).as_flow_canvas();
        let widgets = canvas.set_cards(cx, cards.iter().map(|(c, _)| c.clone()).collect());
        // Opened on a session (or another one chosen): its card comes into view.
        let shown = self.store.session(at).map(|s| s.id.clone());
        if self.flow_focus != shown {
            self.flow_focus = shown.clone();
            if let Some(id) = shown {
                canvas.focus(cx, LiveId::from_str(&format!("s:{id}")));
            }
        }
        for (widget, (card, [title, status, detail, kind, queue, engine])) in widgets.iter().zip(&cards) {
            widget.label(cx, ids!(title)).set_text(cx, title);
            widget.label(cx, ids!(status)).set_text(cx, status);
            widget.label(cx, ids!(detail)).set_text(cx, detail);
            widget.label(cx, ids!(kind.tag)).set_text(cx, kind);
            widget.view(cx, ids!(queue_box)).set_visible(cx, !queue.is_empty());
            widget.label(cx, ids!(queue)).set_text(cx, queue);
            widget.label(cx, ids!(model)).set_text(cx, engine);
            // An inner loop's card takes the colour of its budget's use.
            if let Some(p) = card.key.strip_prefix("p:").filter(|_| card.link == flow::Link::Peer).and_then(|id| self.store.find_peer(id).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| p.id == id))) {
                let (chosen, lit) = (matches!(card.template, t if t == live_id!(InnerSel) || t == live_id!(InnerSelLit)), card.active);
                let tier = budget_tier(p);
                let bg = match (tier, lit, chosen) {
                    (3, _, _) => crate::theme::hex("danger_bg"),
                    (2, _, _) => crate::theme::hex("orange_bg"),
                    (1, _, _) => crate::theme::hex("warning_bg"),
                    (_, true, _) => crate::theme::hex("success_bg"),
                    (_, _, true) => crate::theme::hex("accent_soft"),
                    _ => crate::theme::hex("raised"),
                };
                let border = match (chosen, tier, lit) {
                    (true, _, _) => crate::theme::hex("accent"),
                    (_, 3, _) => crate::theme::hex("danger"),
                    (_, 2, _) => crate::theme::hex("orange"),
                    (_, 1, _) => crate::theme::hex("warning"),
                    (_, _, true) => crate::theme::hex("success"),
                    _ => crate::theme::hex("line"),
                };
                let size = if chosen || lit || tier > 0 { 2.0 } else { 1.0 };
                let (bg, border) = (hex_color(bg), hex_color(border));
                let mut w = widget.clone();
                script_apply_eval!(cx, w, { draw_bg +: {color: #(bg) border_color: #(border) border_size: #(size)} });
            }
        }
        self.view.label(cx, ids!(flow_hint)).set_text(cx, i18n::t(
            "Click a card to talk to it · drag an inner loop onto another outer loop to hand it over · double-click to start an outer loop",
            "点击卡片与它对话 · 把 inner 拖到另一个外环上即可移交 · 双击空白处新建外环"));
        // The open card, if it still exists.
        let creating = matches!(self.flow_open, Some(FlowOpen::Create));
        let open = match &self.flow_open {
            Some(FlowOpen::Outer(id)) => self.store.projects[at.0].sessions.iter().find(|s| &s.id == id).map(|s| {
                let lead = self.lead_status.get(&s.id).cloned().unwrap_or_else(|| "idle".into());
                let n = s.messages.len();
                (s.title.clone(), i18n::pick(format!("{} · {n} messages", status_word(&lead)), format!("{} · {n} 条消息", status_word(&lead))), false)
            }),
            Some(FlowOpen::Inner(id)) => self.store.find_peer(id).and_then(|pat| {
                let session = self.store.session(pat)?;
                let owner = if session.is_detached() { i18n::t("no outer loop", "未挂外环").to_string() } else { session.title.clone() };
                let p = session.peers().iter().find(|p| &p.id == id)?;
                let from = p.joined_from.as_ref().map(|f| i18n::pick(format!(" · joined from “{f}”"), format!(" · 从“{f}”加入"))).unwrap_or_default();
                Some((format!("{} · {}", p.slug, p.role()),
                    i18n::pick(format!("{} · works for “{owner}”{from} · {}", status_word(&p.status), budget_line(p)),
                        format!("{} · 属于“{owner}”{from} · {}", status_word(&p.status), budget_line(p))), true))
            }),
            Some(FlowOpen::Create) => Some((i18n::t("New peer agent", "新建 peer agent").to_string(),
                i18n::t("An inner loop you make yourself: pick its model and the outer loop it works for (or none).", "自己创建的 inner：选模型，选它挂在哪个外环（也可不挂）").to_string(), false)),
            None => None,
        };
        if open.is_none() {
            self.flow_open = None;
        }
        self.view.view(cx, ids!(flow_popup)).set_visible(cx, open.is_some());
        self.view.view(cx, ids!(flow_create)).set_visible(cx, creating);
        self.view.view(cx, ids!(flow_chat)).set_visible(cx, !creating);
        self.view.view(cx, ids!(flow_compose)).set_visible(cx, !creating);
        self.view.button(cx, ids!(flow_popup_chat)).set_visible(cx, !creating);
        self.view.button(cx, ids!(flow_popup_delete)).set_visible(cx, !creating);
        self.view.view(cx, ids!(flow_chat_rule)).set_visible(cx, !creating && self.card_tab == CardTab::Messages);
        self.sync_spec_dialog(cx);
        if open.is_some() {
            self.sync_card_tabs(cx, creating);
        }
        if let Some((title, info, inner)) = open {
            self.view.label(cx, ids!(flow_popup_title)).set_text(cx, &title);
            self.view.label(cx, ids!(flow_popup_info)).set_text(cx, &info);
            self.view.view(cx, ids!(flow_budget)).set_visible(cx, inner);
            let (hint, note) = if inner {
                (i18n::t("Message this inner loop (queued if it is busy)", "给这个 inner 发消息（它忙时会排队）"),
                 i18n::t("Budget: steps are model calls, over all its turns.", "预算：步数是模型调用次数，累计所有 turn。"))
            } else {
                (i18n::t("Message this outer loop", "给这个外环发消息"), "")
            };
            self.view.text_input(cx, ids!(flow_input)).set_empty_text(cx, hint.into());
            self.view.label(cx, ids!(flow_popup_note)).set_text(cx, note);
            // Delete an outer loop, close an inner one: a dialog asks first.
            let label = if inner { i18n::t("Close", "关闭") } else { i18n::t("Delete", "删除") };
            self.view.button(cx, ids!(flow_popup_delete)).set_text(cx, label);
            // What waits for it, to reorder or drop.
            let waiting: Vec<(String, String, String)> = match &self.flow_open {
                Some(FlowOpen::Outer(id)) => self.outer_waiting(id),
                Some(FlowOpen::Inner(id)) => self.rt.waiting(id).into_iter().map(|(i, who, text)| (i, who.to_string(), text)).collect(),
                _ => Vec::new(),
            };
            self.flow_queue_outer = !inner;
            self.flow_queue_ids = self.sync_queue(cx, live_id!(flow_queue), &waiting);
        }
        if creating {
            self.sync_create(cx, at);
        }
        self.view.portal_list(cx, ids!(flow_chat)).redraw(cx);
        self.view.widget(cx, ids!(flow)).redraw(cx);
    }

    /// The open card's key (`s:…`, `p:…`), for the delete confirmation.
    fn flow_open_key(&self) -> Option<String> {
        match &self.flow_open {
            Some(FlowOpen::Outer(id)) => Some(format!("s:{id}")),
            Some(FlowOpen::Inner(id)) => Some(format!("p:{id}")),
            _ => None,
        }
    }

    /// The new peer agent's form: the models and outer loops to pick from.
    fn sync_create(&mut self, cx: &mut Cx, at: SessionRef) {
        let tick = |on: bool, text: &str| if on { format!("✓ {text}") } else { text.to_string() };
        let models = self.octos_choices();
        for (i, id) in [live_id!(nm0), live_id!(nm1), live_id!(nm2), live_id!(nm3)].into_iter().enumerate() {
            let choice = models.get(i);
            self.view.button(cx, &[id]).set_visible(cx, choice.is_some());
            if let Some((label, _)) = choice {
                self.view.button(cx, &[id]).set_text(cx, &tick(self.new_model == i, label));
            }
        }
        let outers: Vec<(String, String)> = self.store.projects[at.0].sessions.iter().filter(|s| !s.is_detached())
            .rev().take(4).map(|s| (s.id.clone(), s.title.chars().take(18).collect())).collect();
        for (i, id) in [live_id!(no0), live_id!(no1), live_id!(no2), live_id!(no3)].into_iter().enumerate() {
            let choice = outers.get(i);
            self.view.button(cx, &[id]).set_visible(cx, choice.is_some());
            if let Some((sid, title)) = choice {
                self.view.button(cx, &[id]).set_text(cx, &tick(self.new_outer.as_deref() == Some(sid.as_str()), title));
            }
        }
        self.view.button(cx, ids!(no_none)).set_text(cx, &tick(self.new_outer.is_none(), i18n::t("None (on its own)", "不挂外环")));
        let note = if self.new_outer.is_some() {
            i18n::t("Its outer loop is told, and gets its reports.", "它的外环会收到通知，也会收到它的汇报。")
        } else {
            i18n::t("On its own: it works for you alone; drag it onto an outer loop later to hand it over.", "未挂外环：只为你工作；以后可以把它拖到某个外环上移交。")
        };
        self.view.label(cx, ids!(new_note)).set_text(cx, note);
    }

    /// The outer loops the new peer agent may join, as the form lists them.
    fn create_outers(&self, at: SessionRef) -> Vec<String> {
        self.store.projects[at.0].sessions.iter().filter(|s| !s.is_detached()).rev().take(4).map(|s| s.id.clone()).collect()
    }

    fn draw_flow_chat(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let rows_key = match &self.flow_open {
            Some(FlowOpen::Outer(id)) => self.store.projects.iter().flat_map(|p| &p.sessions).find(|s| &s.id == id)
                .map(|s| (chat::session_rows(s, self.rt.streaming_lead(&s.id)), s.id.clone())),
            Some(FlowOpen::Inner(id)) => self.store.find_peer(id).and_then(|at| self.store.session(at))
                .and_then(|s| s.peers().iter().find(|p| &p.id == id)).map(|p| (chat::peer_rows(p), p.id.clone())),
            _ => None,
        };
        match rows_key {
            Some((rows, key)) => self.draw_chat(cx, list, &rows, &key),
            None => {
                list.set_item_range(cx, 0, 0);
                while list.next_visible_item(cx).is_some() {}
            }
        }
    }

    /// Opens a card: its session becomes the one shown, its conversation opens over the graph.
    /// Deletes an outer loop (`s:<session>`) or closes an inner one (`p:<peer>`).
    fn delete_flow_card(&mut self, key: &str) {
        if let Some(id) = key.strip_prefix("s:") {
            let at = self.store.projects.iter().enumerate()
                .find_map(|(pi, p)| p.sessions.iter().position(|s| s.id == id).map(|si| (pi, si)));
            if let Some(at) = at {
                self.delete_session(at);
                // Still on the flow canvas: another of the project's sessions is shown.
                self.selected = self.store.projects[at.0].sessions.iter().rposition(|s| !s.is_detached()).map(|si| (at.0, si));
            }
            if self.flow_open_key().as_deref() == Some(key) {
                self.flow_open = None;
            }
        } else if let Some(id) = key.strip_prefix("p:") {
            self.close_peer(id, "by you");
            if self.flow_open_key().as_deref() == Some(key) {
                self.flow_open = None;
            }
        }
    }

    /// The delete or close dialog: what it is about, in words.
    fn sync_confirm(&mut self, cx: &mut Cx) {
        let key = self.confirm_delete.clone();
        self.view.view(cx, ids!(confirm_layer)).set_visible(cx, key.is_some());
        let Some(key) = key else { return };
        let (title, text, ok) = if let Some(id) = key.strip_prefix("proj:") {
            let name = self.store.projects.iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default();
            (i18n::pick(format!("Remove the project “{name}” from the list?"), format!("从列表移除项目「{name}」？")),
                i18n::pick("Its folder and files stay as they are; its sessions leave OctoBuddy, their loops stopped. To keep them out of the way instead, archive it.".to_string(),
                    "项目文件夹和文件不受影响；它的会话会从 OctoBuddy 移除，正在运行的会先停下。只想收起来的话，用「归档」。".to_string()),
                i18n::t("Remove", "确认移除"))
        } else if let Some(at) = key.strip_prefix("s:").and_then(|id| self.store.find_session(id)).filter(|at| self.store.is_plain(*at)) {
            let name = self.store.session(at).map(|s| s.title.clone()).unwrap_or_default();
            (i18n::pick(format!("Delete the chat “{name}”?"), format!("删除对话「{name}」？")),
                i18n::pick("Its messages go. To keep them out of the way instead, archive it.".to_string(), "它的消息会被删除。只想收起来的话，用「归档」。".to_string()),
                i18n::t("Delete", "确认删除"))
        } else if let Some(id) = key.strip_prefix("s:") {
            let session = self.store.projects.iter().flat_map(|p| &p.sessions).find(|s| s.id == id);
            let name = session.map(|s| s.title.clone()).unwrap_or_default();
            let n = session.map(|s| s.peers().iter().filter(|p| p.status != "closed").count()).unwrap_or(0);
            (i18n::pick(format!("Delete the outer loop “{name}”?"), format!("删除外环「{name}」？")),
                i18n::pick(format!("It and its {n} inner loop(s) go, with their conversations; any still at work stop. The project's files stay as they are."),
                    format!("它和它的 {n} 个 inner 会被删除，对话一并删除；还在工作的会先停下。项目文件不受影响。")),
                i18n::t("Delete", "确认删除"))
        } else {
            let id = key.trim_start_matches("p:");
            let slug = self.store.find_peer(id).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| p.id == id)).map(|p| p.slug.clone()).unwrap_or_default();
            (i18n::pick(format!("Close the inner loop “{slug}”?"), format!("关闭 inner「{slug}」？")),
                i18n::pick("It stops (if it works) and takes no more messages; its conversation stays.".to_string(), "它会停下（如果还在工作），之后不能再给它发消息；对话记录保留。".to_string()),
                i18n::t("Close", "确认关闭"))
        };
        self.view.label(cx, ids!(confirm_title)).set_text(cx, &title);
        self.view.label(cx, ids!(confirm_text)).set_text(cx, &text);
        self.view.button(cx, ids!(confirm_ok)).set_text(cx, ok);
        self.view.button(cx, ids!(confirm_cancel)).set_text(cx, i18n::t("Cancel", "取消"));
    }

    /// The new peer agent's form, for an outer loop (its session id) or none.
    fn open_create(&mut self, cx: &mut Cx, outer: Option<String>) {
        self.flow_open = Some(FlowOpen::Create);
        self.flow_menu = None;
        self.new_model = 0;
        self.new_outer = outer;
        for id in [ids!(new_name), ids!(new_role), ids!(new_task)] {
            self.view.text_input(cx, id).set_text(cx, "");
        }
        self.relayout(cx);
        self.view.text_input(cx, ids!(new_name)).set_key_focus(cx);
    }

    /// A new outer loop at a point of the canvas, its conversation open.
    fn new_outer_at(&mut self, cx: &mut Cx, pos: DVec2) {
        let Some(pi) = self.selected.map(|at| at.0) else { return };
        if let Some(at) = self.store.add_session(pi) {
            if let Some(s) = self.store.session_mut(at) {
                s.flow = Some(model::FlowPos { x: pos.x, y: pos.y });
            }
            let id = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
            self.selected = Some(at);
            self.flow_open = Some(FlowOpen::Outer(id));
            self.save();
            self.relayout(cx);
            self.view.text_input(cx, ids!(flow_input)).set_key_focus(cx);
        }
    }

    /// The right-click menu: what it offers for its card, at the pointer.
    fn sync_flow_menu(&mut self, cx: &mut Cx, flow: bool) {
        let menu = self.view.view(cx, ids!(flow_menu));
        let Some((card, at, _)) = self.flow_menu.clone().filter(|_| flow) else {
            menu.set_visible(cx, false);
            return;
        };
        let outer = card.as_deref().and_then(|k| k.strip_prefix("s:")).map(String::from);
        let inner = card.as_deref().and_then(|k| k.strip_prefix("p:")).map(String::from);
        let title = match (&outer, &inner) {
            (Some(id), _) => self.store.projects.iter().flat_map(|p| &p.sessions).find(|s| &s.id == id).map(|s| s.title.clone()).unwrap_or_default(),
            (_, Some(id)) => self.store.find_peer(id).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| &p.id == id))
                .map(|p| format!("{} · {}", p.slug, p.role())).unwrap_or_default(),
            _ => i18n::t("The canvas", "画布").to_string(),
        };
        let t = i18n::t;
        let items = [
            (ids!(m_new_peer), outer.is_some(), t("New peer agent for this outer loop", "新建 peer（挂在这个外环）")),
            (ids!(m_open), card.is_some(), t("Open its conversation", "打开对话")),
            (ids!(m_chat), card.is_some(), t("Open in the chat view", "在对话视图中打开")),
            (ids!(m_budget), inner.is_some(), t("Set its budget…", "设置预算…")),
            (ids!(m_new_outer), card.is_none(), t("New outer loop here", "在这里新建外环")),
            (ids!(m_new_free), card.is_none(), t("New peer agent on its own", "新建 peer（不挂外环）")),
            (ids!(m_delete), card.is_some(), if inner.is_some() { t("Close this inner loop…", "关闭这个 inner…") } else { t("Delete this outer loop…", "删除这个外环…") }),
        ];
        let shown = items.iter().filter(|i| i.1).count() as f64;
        for (id, show, text) in items {
            let button = self.view.button(cx, id);
            button.set_visible(cx, show);
            button.set_text(cx, text);
        }
        self.view.label(cx, ids!(menu_title)).set_text(cx, &title);
        menu.set_visible(cx, true);
        // At the pointer, kept inside the canvas.
        let area = self.view.area().rect(cx);
        let h = 40.0 + shown * 28.0;
        let pos = place(at, dvec2(236.0, h), area);
        let mut w = self.view.widget(cx, ids!(flow_menu));
        script_apply_eval!(cx, w, { abs_pos: #(pos) });
    }

    /// A row of the right-click menu was picked.
    fn handle_flow_menu(&mut self, cx: &mut Cx, actions: &Actions) {
        let Some((card, _, pos)) = self.flow_menu.clone() else { return };
        let picked = [ids!(m_new_peer), ids!(m_open), ids!(m_chat), ids!(m_budget), ids!(m_new_outer), ids!(m_new_free), ids!(m_delete)]
            .into_iter().position(|id| self.view.button(cx, id).clicked(actions));
        let Some(picked) = picked else { return };
        self.flow_menu = None;
        let key = card.clone().unwrap_or_default();
        let outer = key.strip_prefix("s:").map(String::from);
        let inner = key.strip_prefix("p:").map(String::from);
        match picked {
            0 => self.open_create(cx, outer),
            1 => self.open_flow_card(cx, &key),
            2 => {
                if let Some(id) = outer {
                    let at = self.store.projects.iter().enumerate()
                        .find_map(|(pi, p)| p.sessions.iter().position(|s| s.id == id).map(|si| (pi, si)));
                    if at.is_some() {
                        self.selected = at;
                        self.stage = Stage::Chat;
                        self.flow_open = None;
                    }
                } else if let Some(id) = inner {
                    // On its own it has no conversation of an outer loop's to open in.
                    match self.store.find_peer(&id).filter(|at| self.store.session(*at).is_some_and(|s| !s.is_detached())) {
                        Some(at) => {
                            self.selected = Some(at);
                            self.selected_peer = Some(id);
                            self.show_inner = true;
                            self.show_preview = false;
                            self.stage = Stage::Chat;
                            self.flow_open = None;
                        }
                        None => self.open_flow_card(cx, &key),
                    }
                }
                self.relayout(cx);
            }
            3 => {
                self.open_flow_card(cx, &key);
                self.view.text_input(cx, ids!(flow_budget_steps)).set_key_focus(cx);
            }
            4 => self.new_outer_at(cx, pos),
            5 => self.open_create(cx, None),
            _ => {
                // Asked first, in a dialog.
                self.confirm_delete = Some(key);
                self.relayout(cx);
            }
        }
    }

    fn open_flow_card(&mut self, cx: &mut Cx, key: &str) {
        self.confirm_delete = None;
        // Another card: its spec list, not the last card's open spec.
        self.spec_shown = None;
        if let Some(id) = key.strip_prefix("s:") {
            let at = self.store.projects.iter().enumerate()
                .find_map(|(pi, p)| p.sessions.iter().position(|s| s.id == id).map(|si| (pi, si)));
            if let Some(at) = at {
                self.selected = Some(at);
                self.flow_open = Some(FlowOpen::Outer(id.to_string()));
            }
        } else if let Some(id) = key.strip_prefix("p:") {
            if let Some(at) = self.store.find_peer(id) {
                self.selected = Some(at);
                self.selected_peer = Some(id.to_string());
                self.flow_open = Some(FlowOpen::Inner(id.to_string()));
                let budget = self.store.session(at).and_then(|s| s.peers().iter().find(|p| p.id == id)).and_then(|p| p.budget).unwrap_or_default();
                self.view.text_input(cx, ids!(flow_budget_steps)).set_text(cx, &budget.steps.map(|s| s.to_string()).unwrap_or_default());
                self.view.text_input(cx, ids!(flow_budget_cost)).set_text(cx, &budget.cost.map(|c| format!("{c:.2}")).unwrap_or_default());
            }
        }
        self.relayout(cx);
        self.view.text_input(cx, ids!(flow_input)).set_key_focus(cx);
    }

    /// A message's tool-call line was clicked: its calls unfold, or fold again.
    fn toggle_steps(&mut self, cx: &mut Cx, list: &PortalListRef, key: &str, actions: &Actions) {
        let hit = list.items_with_actions(actions).into_iter()
            .find(|(_, item)| tapped(&item.view(cx, ids!(steps_head)), actions)).map(|(index, _)| index);
        if let Some(index) = hit {
            let k = (key.to_string(), index);
            let opened = !self.steps_open.remove(&k) && self.steps_open.insert(k);
            // The last message, opened: it grows past the end, brought into view.
            if opened && list.borrow().is_some_and(|l| index + 1 == l.range_end()) {
                list.smooth_scroll_to_end(cx, 30.0, None);
            }
            self.relayout(cx);
        }
    }

    /// Opens or folds a card. The last one, opened, is scrolled into view:
    /// it grows past the list's end, where it would not be seen.
    fn toggle_card(&mut self, cx: &mut Cx, list: &PortalListRef, key: (String, usize)) {
        let index = key.1;
        let opened = !self.expanded.remove(&key) && self.expanded.insert(key);
        if opened && list.borrow().is_some_and(|l| index + 1 == l.range_end()) {
            list.smooth_scroll_to_end(cx, 30.0, None);
        }
        self.relayout(cx);
    }

    fn handle_flow_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let canvas = self.view.widget(cx, ids!(flow)).as_flow_canvas();
        for action in canvas.actions(actions) {
            match action {
                flow::FlowAction::Open(key) => self.open_flow_card(cx, &key),
                flow::FlowAction::Placed(key, pos) => {
                    let pos = Some(model::FlowPos { x: pos.x, y: pos.y });
                    if let Some(id) = key.strip_prefix("p:") {
                        if let Some(p) = self.store.peer_mut(id) {
                            p.flow = pos;
                        }
                    } else if let Some(id) = key.strip_prefix("s:") {
                        if let Some(s) = self.store.projects.iter_mut().flat_map(|p| p.sessions.iter_mut()).find(|s| s.id == id) {
                            s.flow = pos;
                        }
                    }
                    self.save();
                }
                flow::FlowAction::Attach { card, onto } => {
                    let (Some(peer), Some(sid)) = (card.strip_prefix("p:"), onto.strip_prefix("s:")) else { continue };
                    let to = self.store.projects.iter().enumerate()
                        .find_map(|(pi, p)| p.sessions.iter().position(|s| s.id == sid).map(|si| (pi, si)));
                    let result = to.ok_or_else(|| "no such outer loop".to_string()).and_then(|to| self.move_peer(peer, to));
                    if let Err(err) = result {
                        self.say(cx, &i18n::pick(format!("Could not move it: {err}"), format!("无法移交：{err}")));
                    }
                    self.save();
                    self.relayout(cx);
                }
                flow::FlowAction::NewAt(pos) => self.new_outer_at(cx, pos),
                flow::FlowAction::Rewave { card, wave } => {
                    let Some(peer) = card.strip_prefix("p:") else { continue };
                    if let Err(err) = self.set_wave(peer, wave) {
                        self.say(cx, &err);
                    }
                    self.save();
                    self.relayout(cx);
                }
                flow::FlowAction::Menu { card, at, canvas } => {
                    self.flow_menu = Some((card, at, canvas));
                    self.relayout(cx);
                }
                flow::FlowAction::Pressed => {
                    if self.flow_menu.take().is_some() {
                        self.relayout(cx);
                    }
                }
                flow::FlowAction::None => {}
            }
        }
        self.handle_flow_menu(cx, actions);
        self.card_tabs_actions(cx, actions);
        if self.view.button(cx, ids!(flow_popup_close)).clicked(actions) {
            self.flow_open = None;
            self.confirm_delete = None;
            self.relayout(cx);
        }
        // A new peer agent: the form, then Create.
        if tapped(&self.view.view(cx, ids!(new_peer_btn)), actions) {
            let outer = self.selected.and_then(|at| self.store.session(at)).filter(|s| !s.is_detached()).map(|s| s.id.clone());
            self.open_create(cx, outer);
        }
        if let (Some(FlowOpen::Create), Some(at)) = (self.flow_open.clone(), self.selected) {
            if let Some(i) = [live_id!(nm0), live_id!(nm1), live_id!(nm2), live_id!(nm3)].into_iter().position(|id| self.view.button(cx, &[id]).clicked(actions)) {
                self.new_model = i;
                self.relayout(cx);
            }
            let outers = self.create_outers(at);
            if let Some(i) = [live_id!(no0), live_id!(no1), live_id!(no2), live_id!(no3)].into_iter().position(|id| self.view.button(cx, &[id]).clicked(actions)) {
                self.new_outer = outers.get(i).cloned();
                self.relayout(cx);
            }
            if self.view.button(cx, ids!(no_none)).clicked(actions) {
                self.new_outer = None;
                self.relayout(cx);
            }
            if self.view.button(cx, ids!(new_create)).clicked(actions) {
                let name = self.view.text_input(cx, ids!(new_name)).text();
                let role = self.view.text_input(cx, ids!(new_role)).text();
                let task = Some(self.view.text_input(cx, ids!(new_task)).text().trim().to_string()).filter(|t| !t.is_empty());
                let model = self.octos_choices().get(self.new_model).and_then(|(_, m)| m.clone());
                let attach = self.new_outer.as_ref().and_then(|sid| self.store.projects[at.0].sessions.iter().position(|s| &s.id == sid).map(|si| (at.0, si)));
                match self.create_peer(at.0, &name, &role, model, attach, task) {
                    Ok(id) => {
                        if let Some(pat) = self.store.find_peer(&id) {
                            self.selected = Some(pat);
                        }
                        self.selected_peer = Some(id.clone());
                        self.flow_open = Some(FlowOpen::Inner(id));
                    }
                    Err(err) => self.view.label(cx, ids!(new_note)).set_text(cx, &err),
                }
                self.save();
                self.relayout(cx);
            }
        }
        // Delete an outer loop (its session) or close an inner one: twice.
        // Asked first, in a dialog over everything.
        if self.view.button(cx, ids!(flow_popup_delete)).clicked(actions) {
            self.confirm_delete = self.flow_open_key();
            self.relayout(cx);
        }
        // The open card's queue: up, down, drop.
        let shown = self.flow_queue_ids.clone();
        for (i, row) in QUEUE_ROWS.iter().enumerate() {
            let Some(item) = shown.get(i) else { break };
            let pressed = |name: LiveId| self.view.button(cx, &[live_id!(flow_queue), *row, name]).clicked(actions);
            let (up, down, del) = (pressed(live_id!(up)), pressed(live_id!(down)), pressed(live_id!(del)));
            match (self.flow_queue_outer, up, down, del) {
                (true, true, _, _) => self.shift_outer(cx, item, -1),
                (true, _, true, _) => self.shift_outer(cx, item, 1),
                (true, _, _, true) => self.drop_outer(cx, item),
                (false, true, _, _) => self.shift_peer_queue(cx, item, -1),
                (false, _, true, _) => self.shift_peer_queue(cx, item, 1),
                (false, _, _, true) => self.drop_peer_queue(cx, item),
                _ => {}
            }
        }
        // Its conversation: a card opens or folds, a reply is copied.
        if let Some(key) = match &self.flow_open { Some(FlowOpen::Outer(id)) | Some(FlowOpen::Inner(id)) => Some(id.clone()), _ => None } {
            let list = self.view.portal_list(cx, ids!(flow_chat));
            let toggled = list.items_with_actions(actions).into_iter()
                .find(|(index, item)| tapped(&item.view(cx, ids!(card_head)), actions)
                    || (!self.expanded.contains(&(key.clone(), *index)) && tapped(&item.view(cx, ids!(card)), actions)))
                .map(|(index, _)| index);
            self.toggle_steps(cx, &list, &key, actions);
            if let Some(index) = toggled {
                self.toggle_card(cx, &list, (key.clone(), index));
            }
            let copy = list.items_with_actions(actions).into_iter()
                .find(|(_, item)| tapped(&item.view(cx, ids!(copy)), actions)).map(|(index, _)| index);
            if let Some(index) = copy {
                let rows = match &self.flow_open {
                    Some(FlowOpen::Outer(id)) => self.store.projects.iter().flat_map(|p| &p.sessions).find(|s| &s.id == id).map(|s| chat::session_rows(s, None)),
                    Some(FlowOpen::Inner(id)) => self.store.find_peer(id).and_then(|at| self.store.session(at)).and_then(|s| s.peers().iter().find(|p| &p.id == id)).map(chat::peer_rows),
                    _ => None,
                };
                if let Some(row) = rows.and_then(|r| r.get(index).cloned()) {
                    cx.copy_to_clipboard(&row.body);
                    self.copied = Some((key, index));
                    self.relayout(cx);
                }
            }
        }
        if self.view.button(cx, ids!(flow_popup_chat)).clicked(actions) {
            self.stage = Stage::Chat;
            if matches!(self.flow_open, Some(FlowOpen::Inner(_))) {
                self.show_inner = true;
                self.show_preview = false;
            }
            self.flow_open = None;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(flow_send)).clicked(actions)
            || self.view.text_input(cx, ids!(flow_input)).returned(actions).is_some()
        {
            let input = self.view.text_input(cx, ids!(flow_input));
            let text = input.text().trim().to_string();
            match self.flow_open.clone() {
                _ if text.is_empty() => {}
                Some(FlowOpen::Outer(id)) => {
                    let at = self.store.projects.iter().enumerate()
                        .find_map(|(pi, p)| p.sessions.iter().position(|s| s.id == id).map(|si| (pi, si)));
                    if let Some(at) = at {
                        if self.send_text(cx, at, text) {
                            input.set_text(cx, "");
                        }
                    }
                }
                Some(FlowOpen::Inner(id)) => {
                    input.set_text(cx, "");
                    self.send_text_to_peer(cx, &id, text, dispatch::Mode::Queue);
                }
                _ => {}
            }
            input.set_key_focus(cx);
        }
        let set = self.view.button(cx, ids!(flow_budget_set)).clicked(actions);
        let clear = self.view.button(cx, ids!(flow_budget_clear)).clicked(actions);
        if let (true, Some(FlowOpen::Inner(id))) = (set || clear, self.flow_open.clone()) {
            let budget = if clear {
                self.view.text_input(cx, ids!(flow_budget_steps)).set_text(cx, "");
                self.view.text_input(cx, ids!(flow_budget_cost)).set_text(cx, "");
                None
            } else {
                let steps = self.view.text_input(cx, ids!(flow_budget_steps)).text().trim().parse::<u64>().ok();
                let cost = self.view.text_input(cx, ids!(flow_budget_cost)).text().trim().trim_start_matches('$').parse::<f64>().ok();
                Some(model::Budget { steps, cost })
            };
            self.set_budget(&id, budget);
            self.save();
            self.relayout(cx);
        }
    }

    /// The engines and models the outer loop can run on, the chosen one ticked.
    /// The octos models the outer loop can run on: the one the inner loops
    /// use, then each of the OctoSense AI providers' (`None`: the default).
    fn octos_choices(&self) -> Vec<(String, Option<String>)> {
        let default = match self.providers.primary() {
            Some(p) => i18n::pick(format!("default · {} (as the inner loops)", p.label), format!("默认 · {}（与 inner 相同）", p.label)),
            None => i18n::t("default (as the inner loops)", "默认（与 inner 相同）").to_string(),
        };
        let mut out = vec![(default, None)];
        out.extend(self.providers.rows.iter().filter(|r| r.role != "primary").take(3).map(|r| (r.label.clone(), Some(r.label.clone()))));
        out
    }

    /// The person picked what runs a session's outer loop. It applies to its
    /// next message: the running outer loop stops (when it is not at work).
    fn pick_outer(&mut self, cx: &mut Cx, at: SessionRef, pick: model::OuterPick) {
        let Some(session) = self.store.session(at) else { return };
        let id = session.id.clone();
        // At work: the turn is cut short for the pick (its context packing
        // or its terminal still hold it).
        if self.rt.packing.contains(&id) || self.tui_holds_outer(&id) {
            self.say(cx, i18n::t("The outer loop is getting ready: pick again in a moment.", "外环正在准备：稍后再切换。"));
            return;
        }
        let cut = self.lead_busy(&id);
        if cut {
            self.cut_outer(&id);
            self.system(at, i18n::t("Cut the outer loop's turn short for the switch.", "为了切换，已打断外环当前这一轮。"));
        }
        let Some(session) = self.store.session(at) else { return };
        let new_engine = session.engine() != pick.engine;
        let label = format!("{} · {}", pick.engine, pick.model.as_deref().unwrap_or(i18n::t("default", "默认")));
        let system = pick.engine == system_chat::ENGINE;
        let model = pick.model.clone();
        let s = self.store.session_mut(at).unwrap();
        let kept = !new_engine && s.lead_session.is_some() && s.outer_model() != model.as_deref();
        s.outer = Some(pick);
        s.lead_model = None;
        if new_engine {
            // Another engine cannot resume this one's conversation.
            s.lead_session = None;
        }
        match model.filter(|_| kept) {
            Some(m) => {
                self.model_note.insert(id.clone(), format!("[OctoBuddy] From this message on you run on the model {m}. Replies earlier in this conversation came from another model: \
                    when asked which model you are, answer for {m}, not for that one."));
            }
            None => {
                self.model_note.remove(&id);
            }
        }
        self.rt.leads.remove(&id);
        self.show_picker = false;
        let label = if system { i18n::t("OctoSense's agent", "OctoSense 自带的 agent").to_string() } else { label };
        let text = match (self.store.is_plain(at), new_engine) {
            (true, true) => i18n::pick(format!("Now talking to {label}; it starts afresh from your next message."), format!("改用 {label}；从你的下一条消息开始新的对话。")),
            (true, false) => i18n::pick(format!("Now talking to {label}, from your next message."), format!("改用 {label}，从你的下一条消息起生效。")),
            (false, true) => i18n::pick(format!("The outer loop now runs on {label}; it starts a new conversation at your next message."), format!("外环改用 {label}；从你的下一条消息开始新的对话。")),
            (false, false) => i18n::pick(format!("The outer loop now runs on {label}, from your next message (its conversation is kept)."), format!("外环改用 {label}，从你的下一条消息起生效（保留原对话）。")),
        };
        self.system(at, &text);
        self.save();
        self.relayout(cx);
    }

    /// The sidebar's "+" menu: under its button, inside the window.
    fn sync_add_menu(&mut self, cx: &mut Cx) {
        self.view.view(cx, ids!(add_menu)).set_visible(cx, self.add_menu.is_some());
        let Some(at) = self.add_menu else { return };
        self.view.button(cx, ids!(a_app)).set_visible(cx, plugins::enabled(plugins::OCTOSENSE_APP));
        let window = self.view.area().rect(cx);
        let pos = place(at, dvec2(230.0, 72.0), window);
        let mut w = self.view.widget(cx, ids!(add_menu));
        script_apply_eval!(cx, w, { abs_pos: #(pos) });
    }

    fn add_session(&mut self, cx: &mut Cx, project: usize) {
        if let Some(at) = self.store.add_session(project) {
            // A plain chat starts on OctoSense's own agent when OctoBuddy may use it.
            if self.store.is_plain(at) && self.system_ready() {
                if let Some(s) = self.store.session_mut(at) {
                    s.outer = Some(model::OuterPick { engine: system_chat::ENGINE.into(), model: None });
                }
            }
            self.selected = Some(at);
            self.page = Page::Chat;
            self.save();
            self.relayout(cx);
            self.view.text_input(cx, ids!(composer)).set_key_focus(cx);
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        // "+": a project folder; with the OctoSense apps plugin on, a menu
        // that also makes a new app.
        if self.view.button(cx, ids!(add_project)).clicked(actions) {
            if plugins::enabled(plugins::OCTOSENSE_APP) {
                let b = self.view.button(cx, ids!(add_project)).area().rect(cx);
                let at = dvec2(b.pos.x, b.pos.y + b.size.y + 4.0);
                self.add_menu = (!self.add_menu.is_some()).then_some(at);
                self.relayout(cx);
            } else {
                self.picking_app = false;
                self.pick_project_folder(cx);
            }
        }
        for (id, app) in [(ids!(a_project), false), (ids!(a_app), true)] {
            if self.add_menu.is_some() && self.view.button(cx, id).clicked(actions) {
                self.add_menu = None;
                self.picking_app = app;
                self.pick_project_folder(cx);
                self.relayout(cx);
            }
        }
        for action in actions {
            let Some(answer) = action.downcast_ref::<FileDialogAction>() else { continue };
            match answer {
                FileDialogAction::FolderSelected(path) if self.picking_folder => {
                    self.picking_folder = false;
                    if std::mem::take(&mut self.picking_app) {
                        self.add_app(cx, path);
                    } else {
                        self.add_project(cx, &path.to_string_lossy());
                    }
                }
                FileDialogAction::FolderCancelled => {
                    self.picking_folder = false;
                    self.picking_app = false;
                }
                FileDialogAction::FileSelected { paths, .. } if self.picking_data => {
                    self.picking_data = false;
                    if let Some(file) = paths.first().cloned() {
                        self.add_data(cx, DataFrom::Csv(file));
                    }
                }
                FileDialogAction::FileCancelled { .. } => self.picking_data = false,
                _ => {}
            }
        }
        // Enter drops the input's focus (Makepad's TextInput does on
        // submit); a conversation keeps it, so the next line can follow.
        // ⌘Enter (Ctrl+Enter elsewhere) while it works: steered into its turn.
        let returned = self.view.text_input(cx, ids!(composer)).returned(actions);
        // ⇧⌘Enter (⇧Ctrl+Enter elsewhere), or its button: the turn cut off, the message next.
        if self.view.button(cx, ids!(interrupt_send)).clicked(actions) || returned.as_ref().is_some_and(|(_, m)| (m.logo || m.control) && m.shift) {
            self.interrupt_and_send(cx);
            self.view.text_input(cx, ids!(composer)).set_key_focus(cx);
        } else if returned.as_ref().is_some_and(|(_, m)| m.logo || m.control) {
            self.steer(cx);
            self.view.text_input(cx, ids!(composer)).set_key_focus(cx);
        } else if self.view.button(cx, ids!(send)).clicked(actions) || returned.is_some() {
            self.send(cx);
            self.view.text_input(cx, ids!(composer)).set_key_focus(cx);
        }
        if self.view.button(cx, ids!(stop)).clicked(actions) {
            self.stop(cx);
        }
        if self.view.button(cx, ids!(go_on)).clicked(actions) {
            self.go_on(cx);
        }
        if self.view.text_input(cx, ids!(composer)).changed(actions).is_some() && self.selected.is_some_and(|at| self.session_busy(at)) {
            self.sync(cx);
        }
        if self.view.button(cx, ids!(peer_send)).clicked(actions)
            || self.view.text_input(cx, ids!(peer_input)).returned(actions).is_some()
        {
            self.send_to_peer(cx, dispatch::Mode::Queue);
            self.view.text_input(cx, ids!(peer_input)).set_key_focus(cx);
        }
        if self.view.button(cx, ids!(peer_interrupt)).clicked(actions) {
            self.send_to_peer(cx, dispatch::Mode::Interrupt);
        }
        if self.view.button(cx, ids!(merge_worktree)).clicked(actions) {
            self.merge_worktree(cx);
        }
        if self.view.button(cx, ids!(discard_worktree)).clicked(actions) {
            let session = self.selected.and_then(|at| self.store.session(at)).map(|s| s.id.clone());
            if session.is_some() && self.confirm_worktree == session {
                self.confirm_worktree = None;
                self.discard_worktree(cx);
            } else {
                self.confirm_worktree = session;
                self.relayout(cx);
            }
        }
        if self.view.button(cx, ids!(recheck)).clicked(actions) {
            self.recheck_peer(cx);
        }
        for (panel, outer) in [(live_id!(outer_queue), true), (live_id!(inner_queue), false)] {
            let shown = if outer { self.outer_queue_ids.clone() } else { self.inner_queue_ids.clone() };
            for (i, row) in QUEUE_ROWS.iter().enumerate() {
                let Some(item) = shown.get(i) else { break };
                let pressed = |name: LiveId| self.view.button(cx, &[panel, *row, name]).clicked(actions);
                let (up, down, del) = (pressed(live_id!(up)), pressed(live_id!(down)), pressed(live_id!(del)));
                if outer && pressed(live_id!(steer)) {
                    let item = item.clone();
                    self.steer_queued(cx, &item);
                    break;
                }
                match (outer, up, down, del) {
                    (true, true, _, _) => self.shift_outer(cx, item, -1),
                    (true, _, true, _) => self.shift_outer(cx, item, 1),
                    (true, _, _, true) => self.drop_outer(cx, item),
                    (false, true, _, _) => self.shift_peer_queue(cx, item, -1),
                    (false, _, true, _) => self.shift_peer_queue(cx, item, 1),
                    (false, _, _, true) => self.drop_peer_queue(cx, item),
                    _ => {}
                }
            }
        }
        // The shown inner loop's budget, and handing it to another outer loop.
        let shown = self.selected.and_then(|at| self.shown_peer(at)).map(|p| p.id.clone());
        if let Some(peer) = shown {
            if self.view.button(cx, ids!(d_budget_set)).clicked(actions) {
                let steps = self.view.text_input(cx, ids!(d_budget_steps)).text().trim().parse::<u64>().ok();
                let cost = self.view.text_input(cx, ids!(d_budget_cost)).text().trim().trim_start_matches('$').parse::<f64>().ok();
                self.set_budget(&peer, Some(model::Budget { steps, cost }));
                self.save();
                self.relayout(cx);
            }
            if self.view.button(cx, ids!(d_move)).clicked(actions) {
                self.show_move = !self.show_move;
                self.relayout(cx);
            }
            let picked = [live_id!(move_t0), live_id!(move_t1), live_id!(move_t2), live_id!(move_t3)].into_iter()
                .position(|id| self.view.button(cx, &[id]).clicked(actions));
            let target = match picked {
                Some(i) => self.move_targets.get(i).and_then(|sid| {
                    self.store.projects.iter().enumerate().find_map(|(pi, p)| p.sessions.iter().position(|s| &s.id == sid).map(|si| (pi, si)))
                }),
                None if self.view.button(cx, ids!(move_new)).clicked(actions) => self.selected.and_then(|at| self.store.add_session(at.0)),
                None => None,
            };
            if let Some(to) = target {
                match self.move_peer(&peer, to) {
                    Ok(()) => {
                        self.selected = Some(to);
                        self.selected_peer = Some(peer);
                        self.show_move = false;
                    }
                    Err(err) => self.say(cx, &i18n::pick(format!("Could not hand it over: {err}"), format!("无法移交：{err}"))),
                }
                self.save();
                self.relayout(cx);
            }
        }
        if self.view.button(cx, ids!(close_peer)).clicked(actions) {
            let shown = self.selected.and_then(|at| self.shown_peer(at)).map(|p| (p.id.clone(), p.is_active()));
            match shown {
                // Working: a second press closes it (its turn is cancelled).
                Some((id, true)) if self.confirm_close.as_deref() != Some(id.as_str()) => {
                    self.confirm_close = Some(id);
                    self.relayout(cx);
                }
                Some(_) => {
                    self.confirm_close = None;
                    self.close_shown_peer(cx);
                }
                None => {}
            }
        }
        if self.view.button(cx, ids!(toggle_details)).clicked(actions) {
            if let Some(id) = self.selected.and_then(|at| self.shown_peer(at)).map(|p| p.id.clone()) {
                if !self.show_details.remove(&id) {
                    self.show_details.insert(id);
                }
                self.relayout(cx);
            }
        }
        if self.view.button(cx, ids!(toggle_diff)).clicked(actions) {
            if let Some(id) = self.selected.and_then(|at| self.shown_peer(at)).map(|p| p.id.clone()) {
                if !self.show_diff.remove(&id) {
                    self.show_diff.insert(id);
                }
                self.relayout(cx);
            }
        }
        if self.view.button(cx, ids!(approve)).clicked(actions) {
            self.answer_approval(cx, true, None);
        }
        if self.view.button(cx, ids!(approve_session)).clicked(actions) {
            self.answer_approval(cx, true, Some("session"));
        }
        if self.view.button(cx, ids!(deny)).clicked(actions) {
            self.answer_approval(cx, false, None);
        }
        if self.view.button(cx, ids!(approvals_auto)).clicked(actions) {
            self.set_auto(cx, true);
        }
        if self.view.button(cx, ids!(approvals_ask)).clicked(actions) {
            self.set_auto(cx, false);
        }
        if tapped(&self.view.view(cx, ids!(inner_btn)), actions) || tapped(&self.view.view(cx, ids!(inner_btn_on)), actions) {
            self.show_inner = !self.show_inner;
            // One side panel at a time: the conversation keeps its room.
            self.show_preview &= !self.show_inner;
            self.relayout(cx);
        }
        if tapped(&self.view.view(cx, ids!(outer_btn)), actions) {
            self.show_inner = false;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(close_inner)).clicked(actions) {
            self.show_inner = false;
            self.relayout(cx);
        }
        if tapped(&self.view.view(cx, ids!(view_flow)), actions) {
            self.stage = Stage::Flow;
            self.relayout(cx);
        }
        if tapped(&self.view.view(cx, ids!(view_chat)), actions) {
            self.stage = Stage::Chat;
            self.flow_focus = None;
            self.relayout(cx);
        }
        if tapped(&self.view.view(cx, ids!(view_time)), actions) {
            self.stage = Stage::Time;
            self.relayout(cx);
        }
        self.handle_time_actions(cx, actions);
        self.handle_plan_actions(cx, actions);
        self.handle_side_menu(cx, actions);
        if self.view.button(cx, ids!(confirm_cancel)).clicked(actions) {
            self.confirm_delete = None;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(confirm_ok)).clicked(actions) {
            if let Some(key) = self.confirm_delete.take() {
                match key.strip_prefix("proj:") {
                    Some(id) => self.remove_project(id),
                    None => self.delete_flow_card(&key),
                }
                self.save();
            }
            self.relayout(cx);
        }
        self.handle_flow_actions(cx, actions);
        self.plugin_button_actions(cx, actions);
        self.resize_actions(cx, actions);
        self.tui_actions(cx, actions);
        self.workbench_actions(cx, actions);
        self.live_page_actions(cx, actions);
        if tapped(&self.view.view(cx, ids!(data_btn)), actions) {
            self.show_data = !self.show_data;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(data_csv)).clicked(actions) {
            self.picking_data = true;
            let mut dialog = FileDialog::new().set_title(i18n::t("Choose a CSV file for the app", "为应用选择一个 CSV 文件").into())
                .add_filter("CSV".into(), vec!["csv".into()]);
            if let Some(home) = std::env::var_os("HOME") {
                dialog = dialog.set_location(PathBuf::from(home));
            }
            cx.open_select_file_dialog(dialog);
        }
        if self.view.button(cx, ids!(data_add_api)).clicked(actions) || self.view.text_input(cx, ids!(data_url)).returned(actions).is_some() {
            let url = self.view.text_input(cx, ids!(data_url)).text().trim().to_string();
            if !url.is_empty() {
                self.add_data(cx, DataFrom::Api(url));
            }
        }
        if tapped(&self.view.view(cx, ids!(preview_btn)), actions) || tapped(&self.view.view(cx, ids!(preview_btn_on)), actions) {
            self.show_preview = !self.show_preview;
            self.show_inner &= !self.show_preview;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(close_preview)).clicked(actions) {
            self.show_preview = false;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(reload_preview)).clicked(actions) {
            self.preview_of = None;
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(preview_fix)).clicked(actions) {
            self.ask_to_fix_preview(cx);
        }
        if tapped(&self.view.view(cx, ids!(settings_button)), actions) {
            self.page = Page::Settings;
            self.providers = providers::read();
            self.relayout(cx);
        }
        if self.view.button(cx, ids!(reload_providers)).clicked(actions) {
            self.providers = providers::read();
            self.logins = None;
            self.own_note = None;
            self.relayout(cx);
        }
        self.providers_page_actions(cx, actions);
        if let Some(on) = self.view.check_box(cx, ids!(use_worktree)).changed(actions) {
            if let Some(session) = self.selected.and_then(|at| self.store.session_mut(at)).filter(|s| s.work_dir.is_none()) {
                session.worktree = Some(on);
                self.save();
                self.sync(cx);
            }
        }
        if tapped(&self.view.view(cx, ids!(outer_pick)), actions) {
            self.show_picker = !self.show_picker;
            self.pick_inner = false;
            self.pick_agent = None;
            if self.show_picker {
                self.providers = providers::read();
                // Its octos default is OctoSense's agent: linked by the time it is picked.
                self.system_reconnect();
            }
            self.relayout(cx);
        }
        self.picker_actions(cx, actions);
        if tapped(&self.view.view(cx, ids!(back)), actions) {
            self.page = Page::Chat;
            self.relayout(cx);
        }
        let nav: [(&[LiveId], SettingsTab); 6] = [(ids!(nav_providers), SettingsTab::Providers), (ids!(nav_plugins), SettingsTab::Plugins), (ids!(nav_tools), SettingsTab::Tools), (ids!(nav_language), SettingsTab::Language), (ids!(nav_appearance), SettingsTab::Appearance), (ids!(nav_about), SettingsTab::About)];
        self.appearance_actions(cx, actions);
        self.plugins_page_actions(cx, actions);
        for (id, lang) in [(ids!(lang_en), i18n::Lang::En), (ids!(lang_zh), i18n::Lang::Zh)] {
            if self.view.button(cx, id).clicked(actions) {
                self.set_language(cx, lang);
            }
        }
        if let Some((_, tab)) = nav.into_iter().find(|(id, _)| self.view.button(cx, id).clicked(actions)) {
            self.settings_tab = tab;
            if tab == SettingsTab::Providers {
                self.providers = providers::read();
            }
            if tab == SettingsTab::Tools {
                self.probe_tools();
            }
            self.relayout(cx);
        }
        if let Some(at) = self.selected {
            let session_id = self.store.session(at).map(|s| s.id.clone()).unwrap_or_default();
            let messages = self.view.portal_list(cx, ids!(messages));
            let copy = messages.items_with_actions(actions).into_iter()
                .find(|(_, item)| tapped(&item.view(cx, ids!(copy)), actions))
                .map(|(index, _)| index);
            if let (Some(index), Some(session)) = (copy, self.store.session(at)) {
                let rows = if self.store.is_plain(at) { chat::chat_rows(session, None) } else { chat::session_rows(session, None) };
                if let Some(row) = rows.get(index) {
                    cx.copy_to_clipboard(&row.body);
                    self.copied = Some((session_id.clone(), index));
                    self.relayout(cx);
                }
            }
            // A folded card opens on a click anywhere; an open one closes on
            // its header only, so its text can be selected.
            let toggled = messages.items_with_actions(actions).into_iter()
                .find(|(index, item)| tapped(&item.view(cx, ids!(card_head)), actions)
                    || (!self.expanded.contains(&(session_id.clone(), *index)) && tapped(&item.view(cx, ids!(card)), actions)))
                .map(|(index, _)| index);
            self.toggle_steps(cx, &messages, &session_id, actions);
            if let Some(index) = toggled {
                self.toggle_card(cx, &messages, (session_id, index));
            }
            if let Some(p) = self.shown_peer(at).cloned() {
                let list = self.view.portal_list(cx, ids!(peer_messages));
                let copy = list.items_with_actions(actions).into_iter()
                    .find(|(_, item)| tapped(&item.view(cx, ids!(copy)), actions))
                    .map(|(index, _)| index);
                if let Some(row) = copy.and_then(|i| chat::peer_rows(&p).get(i).cloned().map(|r| (i, r))) {
                    cx.copy_to_clipboard(&row.1.body);
                    self.copied = Some((p.id.clone(), row.0));
                    self.relayout(cx);
                }
            }
            if let Some(peer) = self.shown_peer(at).map(|p| p.id.clone()) {
                let list = self.view.portal_list(cx, ids!(peer_messages));
                let toggled = list.items_with_actions(actions).into_iter()
                    .find(|(index, item)| tapped(&item.view(cx, ids!(card_head)), actions)
                        || (!self.expanded.contains(&(peer.clone(), *index)) && tapped(&item.view(cx, ids!(card)), actions)))
                    .map(|(index, _)| index);
                self.toggle_steps(cx, &list, &peer, actions);
                if let Some(index) = toggled {
                    self.toggle_card(cx, &list, (peer, index));
                }
            }
            let order = self.tab_order(at);
            let tabs = self.view.portal_list(cx, ids!(tabs));
            let picked = tabs.items_with_actions(actions).into_iter()
                .find(|(_, item)| tapped(&item.view(cx, ids!(tab)), actions))
                .and_then(|(index, _)| order.get(index).copied());
            if let Some(i) = picked {
                self.selected_peer = self.store.session(at).map(|s| s.peers()[i].id.clone());
                self.relayout(cx);
            }
        }

        let tree = self.view.portal_list(cx, ids!(tree));
        let mut picked = None;
        for (index, item) in tree.items_with_actions(actions) {
            let Some(row) = self.rows.get(index).copied() else { continue };
            match row {
                Row::Project(_) => {
                    if item.button(cx, ids!(add_session)).clicked(actions) {
                        picked = Some((row, true));
                    } else if tapped(&item.view(cx, ids!(row)), actions) {
                        picked = Some((row, false));
                    }
                }
                _ => {
                    if tapped(&item.view(cx, ids!(row)), actions) {
                        picked = Some((row, false));
                    }
                }
            }
            // A right click: its menu (the chats' own row has none).
            let menu_at = item.view(cx, ids!(row)).finger_down(actions)
                .filter(|e| e.device.mouse_button().is_some_and(|b| b.contains(MouseButton::SECONDARY)))
                .map(|e| e.abs);
            let chats_row = matches!(row, Row::Project(pi) if self.store.projects[pi].is_chats());
            if let (Some(pos), false, false) = (menu_at, chats_row, matches!(row, Row::ArchiveHead(_))) {
                self.side_menu = Some((row, pos));
                self.relayout(cx);
            }
        }
        match picked {
            Some((Row::Project(pi), true)) => self.add_session(cx, pi),
            Some((Row::Project(pi), false)) => {
                let project = &mut self.store.projects[pi];
                project.expanded = !project.expanded;
                self.save();
                self.relayout(cx);
            }
            Some((Row::Session(at) | Row::ArchivedSession(at), _)) => {
                self.selected = Some(at);
                self.selected_peer = None;
                self.page = Page::Chat;
                self.relayout(cx);
            }
            Some((Row::ArchiveHead(_), _)) => {
                self.show_archived = !self.show_archived;
                self.relayout(cx);
            }
            Some((Row::ArchivedProject(_), _)) | None => {}
        }
    }

    fn draw_tree(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        if self.rows.is_empty() {
            draw_empty(cx, list, true);
            return;
        }
        list.set_item_range(cx, 0, self.rows.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(row) = self.rows.get(index).copied() else { continue };
            match row {
                Row::ArchiveHead(n) => {
                    let item = list.item(cx, index, id!(ProjectArchived));
                    item.label(cx, ids!(name)).set_text(cx, &i18n::pick(format!("Archived ({n})"), format!("已归档（{n}）")));
                    item.label(cx, ids!(pin)).set_text(cx, "");
                    item.view(cx, ids!(open)).set_visible(cx, self.show_archived);
                    item.view(cx, ids!(closed)).set_visible(cx, !self.show_archived);
                    item.button(cx, ids!(add_session)).set_visible(cx, false);
                    item.draw_all_unscoped(cx);
                }
                Row::ArchivedProject(pi) => {
                    let item = list.item(cx, index, id!(ProjectArchived));
                    item.label(cx, ids!(name)).set_text(cx, &self.store.projects[pi].name);
                    item.label(cx, ids!(pin)).set_text(cx, i18n::t("project", "项目"));
                    item.view(cx, ids!(open)).set_visible(cx, false);
                    item.view(cx, ids!(closed)).set_visible(cx, false);
                    item.button(cx, ids!(add_session)).set_visible(cx, false);
                    item.draw_all_unscoped(cx);
                }
                Row::ArchivedSession(at) => {
                    let template = if self.selected == Some(at) { id!(SessionActive) } else { id!(SessionArchived) };
                    let item = list.item(cx, index, template);
                    let title = self.store.session(at).map(|s| s.title.as_str()).unwrap_or("");
                    let project = &self.store.projects[at.0];
                    let from = if project.is_chats() { i18n::t("chat", "对话").to_string() } else { project.name.clone() };
                    item.label(cx, ids!(title)).set_text(cx, title);
                    item.label(cx, ids!(pin)).set_text(cx, &from);
                    item.draw_all_unscoped(cx);
                }
                Row::Project(pi) => {
                    let project = &self.store.projects[pi];
                    let item = list.item(cx, index, id!(Project));
                    let name = if project.is_chats() { i18n::t("Chats", "对话").to_string() } else { project.name.clone() };
                    item.label(cx, ids!(name)).set_text(cx, &name);
                    item.label(cx, ids!(pin)).set_text(cx, if project.is_pinned() { i18n::t("pinned", "置顶") } else { "" });
                    item.button(cx, ids!(add_session)).set_visible(cx, true);
                    item.view(cx, ids!(open)).set_visible(cx, project.expanded);
                    item.view(cx, ids!(closed)).set_visible(cx, !project.expanded);
                    item.draw_all_unscoped(cx);
                }
                Row::Session(at) => {
                    let template = if self.selected == Some(at) { id!(SessionActive) } else { id!(Session) };
                    let item = list.item(cx, index, template);
                    let title = self.store.session(at).map(|s| s.title.as_str()).unwrap_or("");
                    let pinned = self.store.session(at).is_some_and(|s| s.is_pinned());
                    item.label(cx, ids!(title)).set_text(cx, title);
                    item.label(cx, ids!(pin)).set_text(cx, if pinned { i18n::t("pinned", "置顶") } else { "" });
                    item.draw_all_unscoped(cx);
                }
            }
        }
    }

    /// The session's conversation.
    fn draw_messages(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let Some(session) = self.selected.and_then(|at| self.store.session(at)) else {
            list.set_item_range(cx, 0, 0);
            while list.next_visible_item(cx).is_some() {}
            return;
        };
        let plain = self.selected.is_some_and(|at| self.store.is_plain(at));
        if session.messages.is_empty() {
            draw_empty_as(cx, list, false, plain);
            return;
        }
        let streaming = self.rt.streaming_lead(&session.id);
        let mut rows = if plain { chat::chat_rows(session, streaming) } else { chat::session_rows(session, streaming) };
        // Sent, its first word not out yet: a row that says so, at once.
        let waiting = self.rt.lead_pending.get(&session.id).copied().unwrap_or(0) > 0 || self.rt.packing.contains(&session.id);
        if waiting && streaming.is_none() {
            let since = session.messages.iter().rev().find(|m| m.role() == model::Role::User).map(|m| m.at).unwrap_or_else(model::now_secs);
            let status = waiting_status(self.lead_status.get(&session.id).map(String::as_str).unwrap_or(""), session.engine());
            rows.push(chat::waiting_row(session, plain, &status, since));
        }
        let key = session.id.clone();
        self.draw_chat(cx, list, &rows, &key);
    }

    /// The shown inner loop's conversation, in the same view.
    fn draw_peer_messages(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let Some(p) = self.selected.and_then(|at| self.shown_peer(at)).cloned() else {
            list.set_item_range(cx, 0, 0);
            while list.next_visible_item(cx).is_some() {}
            return;
        };
        let rows = chat::peer_rows(&p);
        self.draw_chat(cx, list, &rows, &p.id);
    }

    /// Draws `rows` into a ChatList; `key` names the conversation for the
    /// cards the person opened.
    fn draw_chat(&mut self, cx: &mut Cx2d, list: &mut PortalList, rows: &[chat::Row], key: &str) {
        // Another conversation in this list, as Robrix switches rooms: the
        // one left keeps where the person was, and this one goes back to
        // where they left it, or to its end. (Left alone, the list kept the
        // last one's place, which can lie past this one's end when they had
        // scrolled up there: it drew nothing, a blank stage, until a message
        // sent scrolled it.)
        let uid = list.widget_uid();
        if self.chat_shown.get(&uid).map(String::as_str) != Some(key) {
            if let Some(left) = self.chat_shown.insert(uid, key.to_string()) {
                self.chat_places.insert(left, (list.first_id(), list.first_scroll(), list.is_at_end()));
            }
            match self.chat_places.get(key).copied().filter(|(first, _, at_end)| !at_end && *first < rows.len()) {
                Some((first, scroll, _)) => {
                    list.set_tail_range(false);
                    list.set_first_id_and_scroll(first, scroll);
                }
                None => {
                    list.set_tail_range(true);
                    list.set_first_id_and_scroll(rows.len().saturating_sub(1), 0.0);
                }
            }
        }
        list.set_item_range(cx, 0, rows.len());
        // Room for a user bubble: the list's width last frame, less its padding.
        let width = list.area().rect(cx).size.x;
        let room = if width > 0.0 { width - 40.0 - 48.0 - 24.0 } else { 300.0 };
        while let Some(index) = list.next_visible_item(cx) {
            let Some(row) = rows.get(index) else { continue };
            let open = self.expanded.contains(&(key.to_string(), index));
            let template = match row.kind {
                chat::Kind::User if !row.title.is_empty() && text_width(&row.body, 10.5) > room => id!(UserSteerWide),
                chat::Kind::User if !row.title.is_empty() => id!(UserSteer),
                chat::Kind::User if text_width(&row.body, 10.5) > room => id!(UserWide),
                chat::Kind::User => id!(User),
                // An agent's reply wears its engine's mark (who wrote it,
                // from its row's title): Claude Code, octos, Codex, pi.
                chat::Kind::Outer | chat::Kind::Inner => {
                    let steps = !row.steps.is_empty();
                    match (row_engine(&row.title, row.kind == chat::Kind::Inner), steps) {
                        ("codex", false) => id!(Codex),
                        ("codex", true) => id!(CodexSteps),
                        ("pi", false) => id!(Pi),
                        ("pi", true) => id!(PiSteps),
                        ("octos", false) => id!(Inner),
                        ("octos", true) => id!(InnerSteps),
                        (_, false) => id!(Outer),
                        (_, true) => id!(OuterSteps),
                    }
                }
                chat::Kind::Card if open => id!(CardOpen),
                chat::Kind::Card => id!(Card),
                chat::Kind::System => id!(System),
            };
            let item = list.item(cx, index, template);
            if row.kind == chat::Kind::User && !row.title.is_empty() {
                item.label(cx, ids!(tag)).set_text(cx, &row.title);
            }
            // A Label or a Markdown, by template: both take text. A reply
            // being written rolls out at an even pace, a caret at its end.
            let k = (key.to_string(), index);
            if row.live && matches!(row.kind, chat::Kind::Outer | chat::Kind::Inner) {
                let text = reveal::without_placeholder(&row.body);
                let total = text.chars().count();
                let r = self.reveal.entry(k).or_insert_with(|| reveal::Reveal::start(total));
                r.total = total;
                r.shown = r.shown.min(total as f64);
                let shown = if total == 0 { reveal::thinking(self.dots).to_string() } else { reveal::visible(text, r.shown as usize) };
                item.widget(cx, ids!(body)).set_text(cx, &shown);
                self.reveal_frame = cx.new_next_frame();
            } else {
                self.reveal.remove(&k);
                item.widget(cx, ids!(body)).set_text(cx, &row.body);
            }
            match row.kind {
                chat::Kind::Outer | chat::Kind::Inner => {
                    // Who wrote it, and on what: the agent, then its model;
                    // its mark the model's provider (or the agent's own).
                    let engine = row_engine(&row.title, row.kind == chat::Kind::Inner);
                    let author = match row.model.as_deref() {
                        Some(m) => format!("{} · {}", row.title, provider_icons::model_name(m)),
                        None => row.title.clone(),
                    };
                    item.label(cx, ids!(author)).set_text(cx, &author);
                    provider_icons::show(&item.widget(cx, ids!(icon)), provider_icons::reply_svg(engine, row.model.as_deref()), &mut self.icons_shown);
                }
                chat::Kind::Card => {
                    item.label(cx, ids!(header)).set_text(cx, &row.title);
                    item.label(cx, ids!(fold)).set_text(cx, if open { i18n::t("collapse", "收起") } else { i18n::t("show all", "展开") });
                }
                _ => {}
            }
            if !row.steps.is_empty() {
                let shown = self.steps_open.contains(&(key.to_string(), index));
                item.label(cx, ids!(steps_sum)).set_text(cx, &steps_summary(&row.steps));
                item.label(cx, ids!(steps_fold)).set_text(cx, if shown { i18n::t("collapse", "收起") } else { i18n::t("show", "展开") });
                item.view(cx, ids!(steps_body)).set_visible(cx, shown);
                if shown {
                    item.label(cx, ids!(steps)).set_text(cx, &row.steps);
                }
            }
            if matches!(row.kind, chat::Kind::Outer | chat::Kind::Inner) {
                let copied = self.copied.as_ref() == Some(&(key.to_string(), index));
                item.label(cx, ids!(copied)).set_text(cx, if copied { i18n::t("Copied", "已复制") } else { "" });
                // Live: how long it has worked so far, counting up (and, before
                // its first word, what it is doing).
                let took = match (row.live, row.since, row.took) {
                    (true, Some(since), _) => {
                        let t = model::now_secs().saturating_sub(since);
                        match &row.status {
                            Some(status) => format!("{status} · {}", elapsed(t)),
                            None => i18n::pick(format!("working · {}", elapsed(t)), format!("工作中 · {}", elapsed(t))),
                        }
                    }
                    (_, _, Some(t)) => i18n::pick(format!("took {}", elapsed(t)), format!("用时 {}", elapsed(t))),
                    _ => String::new(),
                };
                item.label(cx, ids!(took)).set_text(cx, &took);
            }
            item.draw_all_unscoped(cx);
        }
    }

    /// One tab per peer of the selected session, newest round first.
    fn draw_tabs(&mut self, cx: &mut Cx2d, list: &mut PortalList) {
        let Some(at) = self.selected else {
            list.set_item_range(cx, 0, 0);
            while list.next_visible_item(cx).is_some() {}
            return;
        };
        let order = self.tab_order(at);
        let shown = self.shown_peer(at).map(|p| p.id.clone());
        let Some(session) = self.store.session(at) else { return };
        // All of them fit: from the start. (A list made wider keeps its old
        // scroll otherwise, and the tabs sit at the far end.)
        if order.len() as f64 * 132.0 + 20.0 <= self.pane_width("inner") {
            list.set_first_id_and_scroll(0, 0.0);
        }
        list.set_item_range(cx, 0, order.len());
        while let Some(index) = list.next_visible_item(cx) {
            let Some(p) = order.get(index).map(|&i| &session.peers()[i]) else { continue };
            // Selection is a template, not a visibility flag: set_visible on a
            // child of a list item did not take effect.
            let selected = shown.as_deref() == Some(p.id.as_str());
            let template = match (p.status.as_str(), selected) {
                ("running" | "checking", false) => id!(Running),
                ("running" | "checking", true) => id!(RunningSel),
                ("done" | "idle" | "merged", false) => id!(Done),
                ("done" | "idle" | "merged", true) => id!(DoneSel),
                ("failed", false) => id!(Failed),
                ("failed", true) => id!(FailedSel),
                ("queued", false) => id!(Queued),
                ("queued", true) => id!(QueuedSel),
                (_, false) => id!(Halted),
                (_, true) => id!(HaltedSel),
            };
            let item = list.item(cx, index, template);
            item.label(cx, ids!(role)).set_text(cx, p.role());
            let used = p.usage.as_ref().map(|u| format!(" · {}", tokens(u.input + u.output))).unwrap_or_default();
            let queued = self.rt.queued(&p.id);
            let queued = if queued > 0 { i18n::pick(format!(" · {queued} queued"), format!(" · {queued} 条排队")) } else { String::new() };
            let running = self.rt.running_agents.get(&p.id).copied().unwrap_or(0);
            let subagents = if running > 0 { i18n::pick(format!(" · {running} sub"), format!(" · {running} 子")) } else { String::new() };
            let steps = |u: u64| i18n::pick(format!("{u} steps"), format!("{u} 步"));
            let rounds = match (p.estimate, p.rounds_used) {
                (Some(est), Some(used)) => i18n::pick(format!(" · est {est}r · {}", steps(used)), format!(" · 估 {est} 轮 · {}", steps(used))),
                (Some(est), None) => i18n::pick(format!(" · est {est}r"), format!(" · 估 {est} 轮")),
                (None, Some(used)) => format!(" · {}", steps(used)),
                (None, None) => String::new(),
            };
            let state = format!("{}{queued}{subagents}{rounds}{used}", status_word(&p.status));
            item.label(cx, ids!(state)).set_text(cx, &state);
            item.draw_all_unscoped(cx);
        }
    }
}

/// How many of a session's peers are in each state.
#[derive(Default)]
struct PeerCounts {
    active: usize,
    done: usize,
    failed: usize,
    halted: usize,
    merged: usize,
    discarded: usize,
    closed: usize,
}

impl PeerCounts {
    fn of(peers: &[Peer]) -> PeerCounts {
        let mut c = PeerCounts::default();
        for p in peers {
            match p.status.as_str() {
                "queued" | "running" | "checking" => c.active += 1,
                "done" | "idle" => c.done += 1,
                "failed" => c.failed += 1,
                "merged" => c.merged += 1,
                "closed" => c.closed += 1,
                "discarded" => c.discarded += 1,
                _ => c.halted += 1,
            }
        }
        c
    }

    fn short(&self) -> String {
        let mut parts = Vec::new();
        let mut add = |n: usize, en: &str, zh: &str| if n > 0 { parts.push(i18n::pick(format!("{n} {en}"), format!("{n} 个{zh}"))); };
        add(self.active, "running", "运行中");
        add(self.done, "done", "完成");
        add(self.failed, "failed", "失败");
        add(self.halted, "stopped", "已停止");
        add(self.merged, "merged", "已合并");
        add(self.discarded, "discarded", "已丢弃");
        add(self.closed, "closed", "已关闭");
        parts.join(" · ")
    }

    fn long(&self, rounds: u64) -> String {
        let total = self.active + self.done + self.failed + self.halted + self.merged + self.discarded + self.closed;
        if total == 0 {
            return i18n::t("None created yet.", "还没有创建 inner。").to_string();
        }
        i18n::pick(format!("{total} created over {rounds} round(s) · {}", self.short()), format!("{rounds} 轮共创建 {total} 个 · {}", self.short()))
    }
}

/// A peer's result in one line: the outer loop's call, OctoBuddy's commit,
/// the checks; the rest is behind Details.
fn result_summary(p: &Peer, uncommitted: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    match p.review.as_deref().map(|r| r.split(':').next().unwrap_or("").trim()) {
        Some("accept") => parts.push(i18n::t("✓ accepted", "✓ 已接受").into()),
        Some("fix") => parts.push(i18n::t("✗ to fix", "✗ 待修复").into()),
        _ => {}
    }
    if let Some(c) = p.commits.as_deref().and_then(|c| c.last()) {
        parts.push(format!("commit {}", c.split(' ').next().unwrap_or("")));
    }
    let verdict = p.verdict.as_deref().unwrap_or("");
    for line in verdict.lines() {
        if line.starts_with("[tests]") {
            parts.push(if line.contains(" passed (") { i18n::t("tests passed", "测试通过").into() } else { i18n::t("tests FAILED", "测试失败").into() });
        }
        if line.starts_with("[agent-spec]") {
            parts.push(if line.contains("boundaries: pass") { i18n::t("in bounds", "未越界").into() } else if line.contains("FAIL") { i18n::t("out of bounds", "越界").into() } else { i18n::t("contract checked", "契约已检查").into() });
        }
    }
    let subagents = p.subagents.as_ref().map(Vec::len).unwrap_or(0);
    if subagents > 0 {
        parts.push(i18n::pick(format!("{subagents} subagent(s)"), format!("{subagents} 个子 agent")));
    }
    if uncommitted > 0 {
        parts.push(i18n::pick(format!("{uncommitted} not committed"), format!("{uncommitted} 个未提交")));
    }
    if parts.is_empty() { i18n::t("No result yet", "还没有结果").into() } else { parts.join(" · ") }
}

/// A peer's or the outer loop's state, in the interface's language.
/// The outer loop's engine and model, short: `claude · opus-5-5 · high`,
/// `octos · glm-5.3` (from the label under the input).
pub(crate) fn outer_short(label: &str) -> String {
    label.replacen("Claude Code", "claude", 1).replace(" effort", "")
}

/// An inner loop's engine and model, short: `octos · glm-5.3 · high`.
pub(crate) fn inner_short(p: &Peer) -> String {
    let model = p.model.clone().or_else(|| p.model_pick.clone()).unwrap_or_else(|| i18n::t("default model", "默认模型").into());
    let model = model.rsplit('/').next().unwrap_or(&model).to_string();
    match p.effort.as_deref() {
        Some(e) => format!("{} · {model} · {e}", p.agent()),
        None => format!("{} · {model}", p.agent()),
    }
}

/// What a reply not begun says it waits on, from the outer loop's status.
fn waiting_status(status: &str, engine: &str) -> String {
    let name = match engine {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "system" => "OctoSense",
        other => other,
    };
    let status = status.trim();
    if status.is_empty() || status == "starting" || status.ends_with("queued") {
        i18n::pick(format!("starting {name}…"), format!("正在启动 {name}…"))
    } else if status == "thinking" {
        i18n::t("thinking…", "思考中…").to_string()
    } else {
        status.to_string()
    }
}

/// The engine a reply's row title names (`Outer · codex`, `developer · pi`,
/// `Codex`): an inner loop's row is octos's unless it names another.
fn row_engine(title: &str, inner: bool) -> &'static str {
    let last = title.rsplit(" · ").next().unwrap_or(title).trim().to_lowercase();
    match last.as_str() {
        "codex" => "codex",
        "pi" => "pi",
        "claude" | "claude code" => "claude",
        _ if title.contains("octos") => "octos",
        _ if inner => "octos",
        _ => "claude",
    }
}

/// Where a panel of `size` opening from the button `anchor` goes inside
/// `window`: above it, its left edges lined up; below it when there is no
/// room above.
pub(crate) fn above(anchor: Rect, size: DVec2, window: Rect) -> DVec2 {
    let (left, top) = (window.pos.x + 8.0, window.pos.y + 8.0);
    let (right, bottom) = (window.pos.x + window.size.x - 8.0, window.pos.y + window.size.y - 8.0);
    let x = anchor.pos.x.min(right - size.x).max(left);
    let up = anchor.pos.y - size.y - 6.0;
    let y = if up >= top { up } else { (anchor.pos.y + anchor.size.y + 6.0).min(bottom - size.y).max(top) };
    dvec2(x, y)
}

/// Where a menu of `size` opened at the pointer `at` goes inside `window`:
/// to the right and below the pointer, or flipped to its left or above it
/// where it would not fit, and kept inside (its top first, when it is
/// taller than the window).
pub(crate) fn place(at: DVec2, size: DVec2, window: Rect) -> DVec2 {
    let (left, top) = (window.pos.x + 4.0, window.pos.y + 4.0);
    let (right, bottom) = (window.pos.x + window.size.x - 4.0, window.pos.y + window.size.y - 4.0);
    let x = if at.x + size.x <= right { at.x } else { at.x - size.x };
    let y = if at.y + size.y <= bottom { at.y } else { at.y - size.y };
    dvec2(x.min(right - size.x).max(left), y.min(bottom - size.y).max(top))
}

/// How much of its budget an inner loop used: 0 (under half, or none set),
/// 1 (half), 2 (four fifths), 3 (all of it).
fn budget_tier(p: &Peer) -> u8 {
    if p.over_budget.is_some() {
        return 3;
    }
    let Some(budget) = p.budget else { return 0 };
    let steps = budget.steps.filter(|s| *s > 0).map(|s| p.rounds_used.unwrap_or(0) as f64 / s as f64).unwrap_or(0.0);
    let cost = budget.cost.filter(|c| *c > 0.0).map(|c| p.usage.as_ref().map(|u| u.cost).unwrap_or(0.0) / c).unwrap_or(0.0);
    match steps.max(cost) {
        f if f >= 1.0 => 3,
        f if f >= 0.8 => 2,
        f if f >= 0.5 => 1,
        _ => 0,
    }
}

fn hex_color(hex: u32) -> Vec4f {
    Vec4f { x: ((hex >> 16) & 0xff) as f32 / 255.0, y: ((hex >> 8) & 0xff) as f32 / 255.0, z: (hex & 0xff) as f32 / 255.0, w: 1.0 }
}

/// An inner loop's steps and cost against its budget, in one line.
fn budget_line(p: &Peer) -> String {
    let used = p.rounds_used.unwrap_or(0);
    let cost = p.usage.as_ref().map(|u| u.cost).unwrap_or(0.0);
    let budget = p.budget.unwrap_or_default();
    let steps = match budget.steps { Some(s) => format!("{used}/{s}"), None => used.to_string() };
    let dollars = match budget.cost { Some(c) => format!("${cost:.2}/${c:.2}"), None => format!("${cost:.2}") };
    let over = if p.over_budget.is_some() { i18n::t(" · budget used", " · 预算已用完") } else { "" };
    match (budget.steps, budget.cost) {
        (None, None) => i18n::pick(format!("{steps} steps · {dollars} · no budget"), format!("{steps} 步 · {dollars} · 未设预算")),
        _ => i18n::pick(format!("{steps} steps · {dollars}{over}"), format!("{steps} 步 · {dollars}{over}")),
    }
}

/// A message's tool calls in one line: how many, how many failed, and the
/// one it runs now. `steps` is `stream::steps_text`: a call a line, marked
/// ✓ (done), ✗ (failed) or … (running).
fn steps_summary(steps: &str) -> String {
    let lines: Vec<&str> = steps.lines().filter(|l| !l.trim().is_empty()).collect();
    let failed = lines.iter().filter(|l| l.starts_with('✗')).count();
    let now = lines.iter().rev().find(|l| l.starts_with('…'))
        .map(|l| l.trim_start_matches('…').trim().chars().take(60).collect::<String>());
    // What it did, by kind, as Cindy's work groups count it ("Read 4 · Searched 2").
    let mut kinds: Vec<(&str, usize)> = Vec::new();
    for l in &lines {
        let name = l.get(l.char_indices().nth(2).map(|(i, _)| i).unwrap_or(0)..).unwrap_or("").split(" · ").next().unwrap_or("").trim();
        let kind = match name {
            "Read" | "read_file" | "NotebookRead" | "view_image" => i18n::t("read", "读取"),
            "Grep" | "Glob" | "LS" | "grep" | "glob" | "list_dir" | "search_files" | "find" => i18n::t("searched", "搜索"),
            "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "edit_file" | "write_file" | "apply_patch" => i18n::t("edited", "编辑"),
            "Bash" | "bash" | "shell" | "exec" | "exec_command" => i18n::t("ran", "命令"),
            "WebFetch" | "WebSearch" | "web_fetch" | "web_search" => i18n::t("web", "网络"),
            "Subagent" => i18n::t("subagents", "子 agent"),
            "TodoWrite" | "todo" => i18n::t("planned", "规划"),
            _ => i18n::t("tools", "工具"),
        };
        match kinds.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, n)) => *n += 1,
            None => kinds.push((kind, 1)),
        }
    }
    let head = if now.is_some() {
        i18n::pick(format!("Working · {} step(s)", lines.len()), format!("工作中 · {} 步", lines.len()))
    } else {
        i18n::pick(format!("{} tool call(s)", lines.len()), format!("{} 次工具调用", lines.len()))
    };
    let mut out = head;
    for (kind, n) in &kinds {
        out.push_str(&format!(" · {kind} {n}"));
    }
    // A subagent's own calls are inside its one line ("… · 42 tool call(s), last …").
    let inside: Vec<u64> = lines.iter().filter(|l| l.contains("Subagent")).filter_map(|l| {
        let at = l.find(" tool call(s)")?;
        l[..at].rsplit(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
    }).collect();
    let calls: u64 = inside.iter().sum();
    if calls > 0 {
        out.push_str(&match inside.len() {
            1 => i18n::pick(format!(" · {calls} more inside its subagent"), format!(" · 子 agent 内部 {calls} 次")),
            n => i18n::pick(format!(" · {calls} more inside its {n} subagents"), format!(" · {n} 个子 agent 内部共 {calls} 次")),
        });
    }
    if failed > 0 {
        out.push_str(&i18n::pick(format!(" · {failed} failed"), format!(" · {failed} 个失败")));
    }
    if let Some(now) = now {
        out.push_str(&i18n::pick(format!(" · now {now}"), format!(" · 正在 {now}")));
    }
    out
}

#[cfg(test)]
mod place_tests {
    use super::*;

    #[test]
    fn a_menu_opens_where_it_fits() {
        let window = Rect { pos: dvec2(0.0, 0.0), size: dvec2(800.0, 600.0) };
        let size = dvec2(220.0, 300.0);
        assert_eq!(place(dvec2(100.0, 100.0), size, window), dvec2(100.0, 100.0), "below and right");
        assert_eq!(place(dvec2(100.0, 500.0), size, window), dvec2(100.0, 200.0), "near the bottom: above the pointer");
        assert_eq!(place(dvec2(700.0, 100.0), size, window), dvec2(480.0, 100.0), "near the right: to its left");
        // Taller than the window: from its top.
        assert_eq!(place(dvec2(100.0, 50.0), dvec2(220.0, 900.0), window).y, 4.0);
    }

    #[test]
    fn a_reply_wears_its_engines_mark() {
        assert_eq!(row_engine("Outer · codex", false), "codex");
        assert_eq!(row_engine("Codex", false), "codex");
        assert_eq!(row_engine("pi", false), "pi");
        assert_eq!(row_engine("developer · pi", true), "pi");
        assert_eq!(row_engine("developer · octos", true), "octos");
        assert_eq!(row_engine("developer · claude", true), "claude");
        assert_eq!(row_engine("Outer · claude", false), "claude");
        assert_eq!(row_engine("octos · OctoSense", false), "octos");
    }

    #[test]
    fn the_picker_floats_above_its_button() {
        let window = Rect { pos: dvec2(0.0, 0.0), size: dvec2(800.0, 600.0) };
        let button = Rect { pos: dvec2(300.0, 540.0), size: dvec2(280.0, 28.0) };
        assert_eq!(above(button, dvec2(640.0, 380.0), window), dvec2(152.0, 154.0), "above it, kept inside on the right");
        // No room above: below it.
        let top = Rect { pos: dvec2(20.0, 40.0), size: dvec2(280.0, 28.0) };
        assert_eq!(above(top, dvec2(300.0, 200.0), window).y, 74.0);
    }
}

#[cfg(test)]
mod steps_tests {
    #[test]
    fn a_subagents_calls_are_counted_beside_its_agents() {
        let steps = "✓ Subagent · Explore · Survey splash idioms · 42 tool call(s), last Grep tab|pick\\(\n✓ Grep · categor\n✓ Read · docs/QUICKSTART.md";
        let line = super::steps_summary(steps);
        assert!(line.starts_with("3 tool call(s)") || line.starts_with("3 次工具调用"), "{line}");
        assert!(line.contains("42"), "{line}");
        // By kind: one subagent, one search, one read.
        assert!(line.contains("read 1") || line.contains("读取 1"), "{line}");
        assert!(line.contains("searched 1") || line.contains("搜索 1"), "{line}");
        let running = super::steps_summary("✓ Read · a.rs\n… Bash · cargo test");
        assert!(running.starts_with("Working") || running.starts_with("工作中"), "{running}");
    }
}

fn status_word(status: &str) -> &str {
    if !i18n::zh() {
        return status;
    }
    match status {
        "idle" => "空闲",
        "queued" => "排队中",
        "running" => "运行中",
        "checking" => "检查中",
        "done" => "完成",
        "failed" => "失败",
        "interrupted" => "已中断",
        "stopped" => "已停止",
        "closed" => "已关闭",
        "merged" => "已合并",
        "discarded" => "已丢弃",
        "thinking" => "思考中",
        "starting" => "启动中",
        other => other,
    }
}

/// Who a queued message is from, in the interface's language.
fn who_word(who: &str) -> &str {
    if !i18n::zh() {
        return who;
    }
    match who {
        "you" => "你",
        "outer" => "outer",
        "reports" => "汇报",
        other => other,
    }
}

/// About how wide `text`'s longest line is at `size` points: half an em a
/// Latin character, a full em for CJK and other wide ones.
fn text_width(text: &str, size: f64) -> f64 {
    text.lines().map(|line| line.chars().map(|c| if c.is_ascii() { 0.56 } else { 1.0 }).sum::<f64>() * size).fold(0.0, f64::max)
}

/// Token counts, short: 870, 12.3k, 1.2M.
fn tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", n as f64 / 1e3),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

fn elapsed(secs: u64) -> String {
    if secs < 60 { format!("{secs}s") } else if secs < 3600 { format!("{}m {:02}s", secs / 60, secs % 60) } else { format!("{}h {:02}m", secs / 3600, secs / 60 % 60) }
}


/// Draws a list's `Empty` template once. A PortalList keeps offering indices
/// until an item draws nothing, so every index past the first stays empty.
/// A list's one "nothing here" row: the sidebar's, or an empty conversation's.
fn draw_empty(cx: &mut Cx2d, list: &mut PortalList, sidebar: bool) {
    draw_empty_as(cx, list, sidebar, false)
}

/// The same, for a plain chat (`plain`): a conversation, no campaign.
fn draw_empty_as(cx: &mut Cx2d, list: &mut PortalList, sidebar: bool, plain: bool) {
    list.set_item_range(cx, 0, 1);
    while let Some(index) = list.next_visible_item(cx) {
        if index == 0 {
            let item = list.item(cx, index, id!(Empty));
            if sidebar {
                item.label(cx, ids!(empty_text)).set_text(cx, i18n::t("No projects yet. Add a project directory above.", "还没有项目。点上方的 + 添加一个项目目录。"));
            } else if plain {
                item.label(cx, ids!(empty_title)).set_text(cx, i18n::t("Start a chat", "开始对话"));
                item.label(cx, ids!(empty_text)).set_text(cx, i18n::t("Ask anything. This chat belongs to no project: nothing is planned or split, the agent answers you.", "随便问。这个对话不属于任何项目：不规划、不拆分任务，由 agent 直接回答你。"));
            } else {
                item.label(cx, ids!(empty_title)).set_text(cx, i18n::t("Start the campaign", "开始这场战役"));
                item.label(cx, ids!(empty_text)).set_text(cx, i18n::t("Describe the task. The outer loop plans it, splits it and starts the inner loops.", "描述任务。outer 会规划、拆分，并启动 inner。"));
            }
            item.draw_all_unscoped(cx);
        }
    }
}

/// A click (or a tap) on `view`, with the primary button: a right click
/// opens a menu, it is no click.
fn tapped(view: &ViewRef, actions: &Actions) -> bool {
    view.finger_up(actions).is_some_and(|e| e.is_over && e.was_tap() && e.device.is_primary_hit())
}

impl Widget for OctoBuddyView {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.ensure_started(cx);
        if self.page == Page::Settings && self.settings_tab == SettingsTab::Tools {
            self.fit_tool_cards(cx);
        }
        let tree_uid = self.view.portal_list(cx, ids!(tree)).widget_uid();
        let tabs_uid = self.view.portal_list(cx, ids!(tabs)).widget_uid();
        let providers_uid = self.view.portal_list(cx, ids!(provider_list)).widget_uid();
        let wizard_uid = self.view.portal_list(cx, ids!(wz_list)).widget_uid();
        let peer_messages_uid = self.view.portal_list(cx, ids!(peer_messages)).widget_uid();
        let flow_chat_uid = self.view.portal_list(cx, ids!(flow_chat)).widget_uid();
        let spec_list_uid = self.view.portal_list(cx, ids!(flow_spec_list)).widget_uid();
        let plugin_list_uid = self.view.portal_list(cx, ids!(plugin_list)).widget_uid();
        let files_uid = self.view.portal_list(cx, ids!(files_list)).widget_uid();

        while let Some(step) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = step.as_portal_list().borrow_mut() {
                // The step's own uid is not the list's; ask the list.
                let uid = list.widget_uid();
                if uid == tree_uid {
                    self.draw_tree(cx, &mut list);
                } else if uid == tabs_uid {
                    self.draw_tabs(cx, &mut list);
                } else if uid == providers_uid {
                    self.draw_providers(cx, &mut list);
                } else if uid == wizard_uid {
                    self.draw_wizard(cx, &mut list);
                } else if uid == peer_messages_uid {
                    self.draw_peer_messages(cx, &mut list);
                } else if uid == flow_chat_uid {
                    self.draw_flow_chat(cx, &mut list);
                } else if uid == spec_list_uid {
                    self.draw_spec_list(cx, &mut list);
                } else if uid == plugin_list_uid {
                    self.draw_plugin_list(cx, &mut list);
                } else if uid == files_uid {
                    self.draw_files(cx, &mut list);

                } else {
                    self.draw_messages(cx, &mut list);
                }
            }
        }
        // A menu as drawn (its real size): moved back inside the window
        // next frame if it is not (`place`).
        let window = self.view.area().rect(cx);
        // The engine picker: above its button, by its real height.
        if self.show_picker {
            let (button, r) = (self.view.view(cx, ids!(outer_pick)).area().rect(cx), self.view.view(cx, ids!(outer_picker)).area().rect(cx));
            if r.size.y > 0.0 {
                let pos = above(button, r.size, window);
                if (pos - r.pos).length() > 0.5 {
                    self.menu_fix.retain(|(m, _)| *m != live_id!(outer_picker));
                    self.menu_fix.push((live_id!(outer_picker), pos));
                    self.menu_frame = cx.new_next_frame();
                }
            }
        }
        let anchors = [(live_id!(side_menu), self.side_menu.as_ref().map(|(_, at)| *at)), (live_id!(flow_menu), self.flow_menu.as_ref().map(|(_, at, _)| *at)), (live_id!(add_menu), self.add_menu)];
        for (id, at) in anchors {
            let Some(at) = at else { continue };
            let r = self.view.view(cx, &[id]).area().rect(cx);
            if r.size.y <= 0.0 {
                continue;
            }
            let pos = place(at, r.size, window);
            if (pos - r.pos).length() > 0.5 {
                self.menu_fix.retain(|(m, _)| *m != id);
                self.menu_fix.push((id, pos));
                self.menu_frame = cx.new_next_frame();
            }
        }
        // Resized or moved (the person drags the window): the cached
        // new_batch views would stay where they were drawn. See `relayout`.
        let rect = self.view.area().rect(cx);
        if rect != self.last_rect {
            let first = self.last_rect.size.x == 0.0;
            self.last_rect = rect;
            if !first {
                cx.redraw_all();
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Closing: a terminal's CLI ends with OctoBuddy.
        if let Event::Shutdown = event {
            self.end_tui(cx);
        }
        self.ensure_started(cx);
        // A press outside the engine picker (and its button) closes it; and
        // the "+" menu.
        if let Event::MouseDown(e) = event {
            if self.add_menu.is_some() {
                let (menu, button) = (self.view.view(cx, ids!(add_menu)).area().rect(cx), self.view.button(cx, ids!(add_project)).area().rect(cx));
                if !menu.contains(e.abs) && !button.contains(e.abs) {
                    self.add_menu = None;
                    self.relayout(cx);
                }
            }
            if self.show_picker {
                let (panel, button) = (self.view.view(cx, ids!(outer_picker)).area().rect(cx), self.view.view(cx, ids!(outer_pick)).area().rect(cx));
                if !panel.contains(e.abs) && !button.contains(e.abs) {
                    self.show_picker = false;
                    self.relayout(cx);
                }
            }
        }
        if self.menu_frame.is_event(event).is_some() {
            for (id, pos) in std::mem::take(&mut self.menu_fix) {
                let mut w = self.view.widget(cx, &[id]);
                script_apply_eval!(cx, w, { abs_pos: #(pos) });
            }
            self.view.redraw(cx);
        }
        if self.tool_fit_frame.is_event(event).is_some() {
            self.view.redraw(cx);
        }
        // Live replies: a frame on (the next draw asks for the next frame
        // while any is live), the dots of one not begun moving along.
        if let Some(ev) = self.reveal_frame.is_event(event) {
            // The dots of a reply not begun: a step every third of a second.
            let dots = (ev.time * 3.0) as usize % 3;
            let mut moved = dots != self.dots;
            self.dots = dots;
            for r in self.reveal.values_mut() {
                moved |= r.step();
            }
            if moved {
                for list in [ids!(messages), ids!(peer_messages), ids!(flow_chat)] {
                    self.view.portal_list(cx, list).redraw(cx);
                }
            } else if !self.reveal.is_empty() {
                self.reveal_frame = cx.new_next_frame();
            }
        }
        if let Event::Signal = event {
            let events = events::take(&self.rt.inbox);
            if !events.is_empty() {
                let panel_was_open = self.show_inner;
                for e in events {
                    self.apply_event(e);
                }
                self.card_loop_pending(cx);
                self.factory_pending(cx);
                if std::mem::take(&mut self.wizard_clear) {
                    // The wizard's key and URL, used: not kept in the field.
                    self.view.text_input(cx, ids!(wz_key)).set_text(cx, "");
                    self.view.text_input(cx, ids!(wz_base)).set_text(cx, "");
                }
                if let Some((_, Err(err))) = &self.data_added {
                    let note = self.view.label(cx, ids!(data_note));
                    note.set_text(cx, &i18n::pick(format!("Could not add it: {err}"), format!("没能添加：{err}")));
                    note.set_visible(cx, true);
                    self.data_added = None;
                }
                if let Some((session, _)) = self.data_added.take() {
                    self.show_data = false;
                    let composer = self.view.text_input(cx, ids!(composer));
                    if self.selected.and_then(|at| self.store.session(at)).is_some_and(|s| s.id == session) && composer.text().trim().is_empty() {
                        composer.set_text(cx, i18n::t("Make an app for this data: ", "用这份数据做一个应用："));
                    }
                    self.view.label(cx, ids!(data_note)).set_visible(cx, false);
                }
                self.save();
                // Streams grow messages and move the ones after them: redraw
                // the cached (new_batch) views too, see `relayout`.
                let _ = panel_was_open;
                self.relayout(cx);
            }
        }
        // The production loop's runs that are due (and its page kept fresh).
        if self.clock.is_event(event).is_some() {
            self.card_loop_tick();
            if self.page == Page::Live && now_secs().is_multiple_of(5) {
                self.relayout(cx);
            }
        }
        // An inner loop that stopped on the outer loop's task, unreported.
        if self.clock.is_event(event).is_some() && !self.rt.pending.is_empty() {
            self.check_stalls();
        }
        // Reports held for the others', their wait over.
        if self.clock.is_event(event).is_some() && !self.rt.batch_since.is_empty() {
            self.flush_batches();
        }
        // A steer counted as the next turn whose turn never came.
        if self.clock.is_event(event).is_some() && !self.rt.carried.is_empty() {
            self.carried_overdue();
            self.relayout(cx);
        }
        // An outer loop asked to stop that has not: ended.
        if self.clock.is_event(event).is_some() && !self.stopping.is_empty() {
            self.stop_overdue(cx);
        }
        if self.clock.is_event(event).is_some() && self.selected.is_some_and(|at| self.session_busy(at)) {
            self.sync(cx);
        }
        // An inner loop (or the person) changed the app: run it again.
        if self.clock.is_event(event).is_some() && self.show_preview {
            self.sync_preview(cx);
        }
        // A CSV file dropped on an app project's window: data for its app.
        let is_app = self.selected.filter(|at| self.store.session(*at).is_some()).is_some_and(|at| plugins::active(plugins::OCTOSENSE_APP, &self.store.projects[at.0].path));
        if is_app {
            let csv = |items: &[DragItem]| items.iter().find_map(|i| match i {
                DragItem::FilePath { path, .. } if path.to_ascii_lowercase().ends_with(".csv") => Some(PathBuf::from(path)),
                _ => None,
            });
            match event.drag_hits(cx, self.view.area()) {
                DragHit::Drag(e) if csv(&e.items).is_some() => {
                    if let Ok(mut r) = e.response.lock() {
                        *r = DragResponse::Copy;
                    }
                }
                DragHit::Drop(e) => {
                    if let Some(file) = csv(&e.items) {
                        self.add_data(cx, DataFrom::Csv(file));
                    }
                }
                _ => {}
            }
        }
        // The open card's panel and the menu lie over the canvas: presses there are theirs.
        let mut covered = Vec::new();
        if self.stage == Stage::Flow && self.flow_open.is_some() {
            covered.push(self.view.view(cx, ids!(flow_popup)).area().rect(cx));
        }
        if self.stage == Stage::Flow && self.flow_menu.is_some() {
            covered.push(self.view.view(cx, ids!(flow_menu)).area().rect(cx));
        }
        let plan = self.view.view(cx, ids!(plan_panel));
        if plan.visible() {
            covered.push(plan.area().rect(cx));
        }
        // A dialog over it (a spec's text, a delete to confirm): none of the
        // graph's, scrolling included; nor what floats over it.
        if self.confirm_delete.is_some() || self.spec_shown.is_some() {
            covered.push(self.view.view(cx, ids!(stage)).area().rect(cx));
        }
        if self.show_picker {
            covered.push(self.view.view(cx, ids!(outer_picker)).area().rect(cx));
        }
        if self.add_menu.is_some() {
            covered.push(self.view.view(cx, ids!(add_menu)).area().rect(cx));
        }
        // A press on a panel over the messages is the panel's, not a text
        // selection in the list (which takes the presses its overlays took).
        match event {
            Event::MouseDown(e) if covered.iter().any(|r| r.contains(e.abs)) || self.view.view(cx, ids!(data_panel)).area().rect(cx).contains(e.abs) => {
                self.pressing_overlay = true;
                if let Some(mut list) = self.view.portal_list(cx, ids!(messages)).borrow_mut() {
                    list.selectable = false;
                }
            }
            Event::MouseUp(_) if self.pressing_overlay => {
                self.pressing_overlay = false;
                let open = self.show_data || self.renaming.is_some() || self.confirm_delete.is_some() || self.side_menu.is_some();
                if let Some(mut list) = self.view.portal_list(cx, ids!(messages)).borrow_mut() {
                    list.selectable = !open;
                }
            }
            _ => {}
        }
        if let Event::WindowGeomChange(_) = event {
            self.relayout(cx);
        }
        if let (Event::KeyDown(k), true) = (event, self.flow_menu.is_some() || self.side_menu.is_some() || self.renaming.is_some() || self.show_picker || self.add_menu.is_some() || self.spec_shown.is_some()) {
            if k.key_code == KeyCode::Escape {
                self.flow_menu = None;
                self.side_menu = None;
                self.renaming = None;
                self.show_picker = false;
                self.add_menu = None;
                self.spec_shown = None;
                self.relayout(cx);
            }
        } else if let Event::KeyDown(k) = event {
            // Nothing open over it: Esc stops what works (as Cindy's), not
            // while its terminal has the keys.
            let busy = self.selected.is_some_and(|at| self.session_busy(at));
            if k.key_code == KeyCode::Escape && busy && self.page == Page::Chat && self.tui.is_none() && self.confirm_delete.is_none() {
                self.stop(cx);
            }
        }
        // A press elsewhere closes the sidebar's menu.
        if let (Event::MouseDown(e), Some(_)) = (event, self.side_menu) {
            if !self.view.view(cx, ids!(side_menu)).area().rect(cx).contains(e.abs) && e.button.contains(MouseButton::PRIMARY) {
                self.side_menu = None;
                self.relayout(cx);
            }
        }
        self.view.widget(cx, ids!(flow)).as_flow_canvas().set_covered(covered);
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            self.handle_actions(cx, actions);
        }
    }
}

/// Its launcher art (64×64 SVG): the shell's dock and home draw it as they
/// draw App Hub's (`octosense_app_hub_app::APP_ICON_SVG`).
pub const APP_ICON_SVG: &str = include_str!("../resources/app-icon.svg");

#[cfg(test)]
mod tests {
    use super::*;

    /// The view's script evaluates without errors, so a DSL mistake fails
    /// here instead of at runtime.
    #[test]
    fn view_script_evaluates() {
        // No OS layer (no window server, no GPU): what a CI runner has, and
        // all the script needs. OctoSense's own script tests do the same.
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.with_vm(|vm| {
            makepad_widgets::script_mod(vm);
            makepad_terminal::widget::script_mod(vm);
            crate::script_mod(vm);
            let value = script_eval!(vm, {use mod.widgets.* OctoBuddyView{}});
            let root = WidgetRef::script_from_value(vm, value);
            let errors = vm.take_errors();
            assert!(errors.is_empty(), "{errors:?}");
            assert!(root.borrow::<OctoBuddyView>().is_some());
        });
    }

    #[test]
    fn counts_and_times_read_well() {
        let peer = |status: &str| Peer {
            id: String::new(), slug: String::new(), role: None, agent: None, brief: String::new(), status: status.into(), dir: String::new(),
            branch: None, round: 1, started_at: 0, finished_at: None, activity: None, result: None,
            session_key: None, log: None, contract: None, usage: None, check: None, verdict: None, review: None, landed: None, model: None, effort: None, touched: None, base: None, commits: None, subagents: None, estimate: None, wave: None, rounds_used: None, budget: None, over_budget: None, flow: None, joined_from: None, queued: None, inflight: None, uncommitted: None, model_pick: None, agent_named: None, accepted: None, review_wanted: None, reviews_for: None, by_person: None, claude_session: None, specs: None,
        };
        let c = PeerCounts::of(&[peer("running"), peer("queued"), peer("idle"), peer("failed"), peer("interrupted")]);
        assert_eq!(c.short(), "2 running · 1 done · 1 failed · 1 stopped");
        assert_eq!(PeerCounts::of(&[peer("merged"), peer("discarded")]).short(), "1 merged · 1 discarded");
        assert_eq!(c.long(2), "5 created over 2 round(s) · 2 running · 1 done · 1 failed · 1 stopped");
        assert_eq!((elapsed(42), elapsed(83), elapsed(3725)), ("42s".into(), "1m 23s".into(), "1h 02m".into()));
        assert_eq!((tokens(870), tokens(12_345), tokens(1_000_000)), ("870".into(), "12.3k".into(), "1.0M".into()));
    }
}
