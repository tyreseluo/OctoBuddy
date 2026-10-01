//! OctoBuddy on OctoSense's own octos: the shell's one kernel, through
//! `octosense-app-peers` (the one way an app reaches it). With the
//! assistant services granted, the shell gives OctoBuddy ONE peer owned by
//! the system agent: the system agent sees it (`peer_list`), talks to it
//! (`peer_send_input`), and its model reaches OctoBuddy through two tools
//! (`native-apps.json`): `octobuddy.status` (what runs now) and
//! `octobuddy.send` (a message to a session's outer loop, approved by the
//! host). OctoBuddy answers them on the UI thread, where its state lives.
//!
//! The inner loops that write code do not run on this kernel yet: an app's
//! peer cannot work in a directory the person picks, share it with other
//! inner loops, run commands or be steered (see
//! `docs/upcr-draft-coding-peers.md`). They keep OctoBuddy's own `octos serve`,
//! with the same octos and the same provider profile.
use crate::events::{post, Inbox, LoopEvent};
use octosense_app_peers::host_tools::{HostToolCall, ToolExecutor, ToolReply};
use octosense_app_peers::OctosAppService;
use std::cell::RefCell;
use std::sync::Arc;

/// The module id the shell offers the service under.
pub const APP_ID: &str = "octobuddy";

thread_local! {
    // Claimed in `create`, taken by the view when it starts (same thread).
    static CLAIMED: RefCell<Option<Arc<dyn OctosAppService>>> = const { RefCell::new(None) };
}

/// Inside `AppModule::create`: keep what the shell offered this instance.
pub fn keep(service: Option<Arc<dyn OctosAppService>>) {
    CLAIMED.with(|c| *c.borrow_mut() = service);
}

/// The view's link, once it starts: `None` when the shell granted no
/// assistant (standalone, or not allowed yet).
pub struct Link {
    pub service: Arc<dyn OctosAppService>,
}

impl Link {
    /// Takes the claimed service, binds the device's one account, installs
    /// OctoBuddy's tools and prepares its peer (off the UI thread; the
    /// result comes back as `SystemLink`).
    pub fn start(inbox: &Inbox) -> Option<Link> {
        let service = CLAIMED.with(|c| c.borrow_mut().take())?;
        // OctoBuddy has no accounts: one peer for the device.
        service.set_account(Some("device"));
        service.set_tool_executor(Some(Arc::new(Tools { inbox: inbox.clone() })));
        let (s, inbox) = (service.clone(), inbox.clone());
        std::thread::spawn(move || {
            let result = s.prepare();
            post(&inbox, LoopEvent::SystemLink { result });
        });
        Some(Link { service })
    }
}

impl Link {
    /// Prepares the peer again (a cold kernel can take longer than a
    /// prepare waits); the result comes back as `SystemLink`.
    pub fn retry(&self, inbox: &Inbox) {
        let (s, inbox) = (self.service.clone(), inbox.clone());
        std::thread::spawn(move || {
            let result = s.prepare();
            post(&inbox, LoopEvent::SystemLink { result });
        });
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.service.set_tool_executor(None);
        self.service.release();
    }
}

/// Hands every call of OctoBuddy's tools to the view.
struct Tools {
    inbox: Inbox,
}

impl ToolExecutor for Tools {
    fn execute(&self, call: HostToolCall, reply: ToolReply) {
        post(&self.inbox, LoopEvent::ToolCall { name: call.name, args: call.args, reply });
    }
}
