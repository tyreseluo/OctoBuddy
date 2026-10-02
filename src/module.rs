//! OctoBuddy as OctoSense hosts it (feature `octosense-module`): the module
//! the shell creates an instance of, in a pane of its own. On the host it
//! is the same view in a window of its own (`main.rs`, feature `standalone`).
use crate::{script_mod, system};
use makepad_app_module::{
    makepad_ai_services::wire::{ServiceCall, ServiceManifest, ToolResult},
    AppModule, ExecOutcome, InstanceHandles, InstanceParts, OpenSchema, ServiceExecutor, ValidatedOpen,
};
use makepad_widgets::*;

pub struct OctoBuddyModule;
pub static OCTOBUDDY_MODULE: OctoBuddyModule = OctoBuddyModule;


impl AppModule for OctoBuddyModule {
    fn id(&self) -> &'static str { "octobuddy" }
    fn label(&self) -> &'static str { "OctoBuddy" }
    fn register(&self, vm: &mut ScriptVm) {
        // The terminal the native TUI plugin shows an agent's CLI in.
        makepad_terminal::widget::script_mod(vm);
        script_mod(vm);
    }
    fn open_schema(&self) -> OpenSchema { OpenSchema::new(1) }
    // The assistant services: OctoBuddy's peer on the system octos.
    fn capabilities(&self) -> &'static [&'static str] { &octosense_app_peers::OCTOS_SERVICES }
    fn create(&self, vm: &mut ScriptVm, _open: ValidatedOpen, handles: InstanceHandles) -> InstanceParts {
        system::set_hosted();
        system::keep(octosense_app_peers::injection::claim(system::APP_ID, &handles.scope.to_string()));
        let value = script_eval!(vm, {
            use mod.widgets.*
            OctoBuddyView {}
        });
        InstanceParts {
            root: WidgetRef::script_from_value(vm, value),
            executor: Box::new(OctoBuddyExecutor),
            shutdown: Box::new(|_| {}),
        }
    }
}

struct OctoBuddyExecutor;
impl ServiceExecutor for OctoBuddyExecutor {
    fn manifest(&self) -> ServiceManifest {
        ServiceManifest::new("octobuddy", "OctoBuddy", "Projects and sessions for the OctoBuddy two-loop workflow.")
    }
    fn execute(&mut self, _cx: &mut Cx, call: &ServiceCall) -> ExecOutcome {
        ExecOutcome::Done(ToolResult::unavailable(&call.call_id, "OctoBuddy has no tools yet"))
    }
}
