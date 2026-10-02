//! OctoBuddy on the host (feature `standalone`, the default): its view in a
//! window of its own. Built for OctoSense (`octosense-module` alone), there
//! is nothing to run here: the shell hosts the module.
#[cfg(feature = "standalone")]
pub use makepad_widgets;
#[cfg(feature = "standalone")]
use makepad_widgets::*;

#[cfg(feature = "standalone")]
app_main!(App);

#[cfg(not(feature = "standalone"))]
fn main() {}

#[cfg(feature = "standalone")]
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    startup() do #(App::script_component(vm)) {
        ui: Root {
            main_window := Window {
                window.title: "OctoBuddy"
                window.inner_size: vec2(1100, 720)
                body := OctoBuddyView {}
            }
        }
    }
}

#[cfg(feature = "standalone")]
#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
}

#[cfg(feature = "standalone")]
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        makepad_terminal::widget::script_mod(vm);
        octosense_octobuddy::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}
