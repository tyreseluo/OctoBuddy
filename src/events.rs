//! What the loops' background threads tell the view. Every process is read
//! on its own thread; its events go to the view's inbox, and `SignalToUI`
//! wakes the UI to drain it (on `Event::Signal`).
use makepad_widgets::makepad_platform::thread::SignalToUI;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum LoopEvent {
    /// What the lead is doing now ("reading src/main.rs").
    LeadStatus { session: String, status: String },
    /// Time to start octos again (it found its data directory still held).
    ServeRetry,
    /// A data source for a session's app was read (or could not be).
    DataFound { session: String, found: Result<crate::plugins::app_data::Found, String>, commit: Option<String> },
    /// An app was published to the local App Hub (or could not be).
    Published { session: String, result: Result<crate::plugins::app_publish::Published, String> },
    /// A request's context pack, built off the UI thread: the request goes on.
    PackReady { session: String, message: String, pack: Option<String> },
    /// Settings › AI Providers: a test, an add, an import or a removal is done.
    ProvidersDone { job: crate::providers_view::ProvidersJob, result: Result<String, String> },
    /// The app workbench's App Hub tab: a step done (its output, or why not).
    HubDone { project: String, step: crate::plugins::workbench::HubStep, result: Result<String, String> },
    /// Settings › AI Providers: the agents' own sign-ins (agent, what it says).
    LoginsProbed(Vec<(String, String)>),
    /// Settings › Tools: the versions found on this machine.
    ToolsProbed(Vec<crate::tools_info::ToolInfo>),
    /// OctoBuddy's copy of an agent's program is installed (its path), or
    /// why not. `awaited`: something could not start without it.
    AgentInstalled { name: String, result: Result<String, String>, awaited: bool },
    /// A requested app's project made, its data read (`app_factory`), or why not.
    FactoryReady { id: String, result: crate::plugins::app_factory::Made },
    /// The bad-release drill published its version (and the function it broke), or why not.
    CardLoopDrilled { project: String, result: Result<(String, String), String> },
    /// The production loop ran a published app (`card_loop::probe`).
    CardLoopProbed { project: String, result: Result<crate::plugins::card_loop::Probe, String> },
    /// An external plugin's button ran: what it said, for the session.
    PluginSaid { session: String, plugin: String, result: Result<String, String> },
    /// A piece of the lead's reply as it is written.
    LeadDelta { session: String, text: String },
    /// The lead starts a tool (`detail` may come later, with the same id).
    LeadTool { session: String, id: String, name: String, detail: String },
    LeadToolEnd { session: String, id: String, ok: bool },
    /// A tool call made by one of the lead's subagents, inside its Agent call `parent`.
    LeadSubTool { session: String, parent: String, name: String, detail: String },
    /// The model the lead's process runs.
    LeadInfo { session: String, model: String },
    /// One assistant message of the lead is complete (its text may be empty).
    LeadMessage { session: String, text: String },
    /// The lead finished a turn: everything it said in it, for the blocks.
    LeadTurnDone { session: String, ok: bool, error: Option<String>, lead_session: Option<String>, text: String, cost: Option<f64> },
    /// A message steered into the lead's running turn (`Lead::steer`) was
    /// taken up: what it says from now on answers it.
    LeadSteerTaken { session: String },
    /// A steered message the agent turned down (its turn had just ended):
    /// it is sent the usual way instead.
    LeadSteerRefused { session: String, text: String },
    /// The turn ends with steered messages not taken up: they start the
    /// agent's next turn(s), so the loop stays at work.
    LeadSteerCarried { session: String, turns: usize },
    /// The lead process is gone (it exited, or could not start).
    /// `gen`: the process's own number (`lead::next_gen`): a process
    /// replaced by another (another engine picked) ends without touching it.
    LeadExited { session: String, error: Option<String>, gen: u64 },

    /// A peer's clone is ready (or it works in the project directory).
    PeerReady { session: String, peer: String, dir: String, branch: Option<String> },
    /// A peer could not get ready: no clone, no octos.
    PeerFailed { session: String, peer: String, error: String },
    PeerTurnStarted { peer: String },
    PeerActivity { peer: String, line: String },
    /// A piece of the peer's reply as it is written.
    PeerDelta { peer: String, text: String },
    PeerTool { peer: String, id: String, name: String, detail: String },
    PeerToolEnd { peer: String, id: String, ok: bool },
    /// The peer asks the person something (`ask_user_question`); its turn waits.
    PeerQuestion { peer: String, question_id: String, text: String, options: Vec<String> },
    /// A steer was taken into the running turn (`steered`), or started a new one.
    PeerSteered { peer: String, steered: bool, turn: Option<String> },
    /// Steers the turn ended without reading: they go back in line.
    PeerSteerDropped { peer: String, texts: Vec<String> },
    /// A peer turn ended: `completed`, `interrupted` or `failed`, with what it said.
    PeerTurnEnded { peer: String, outcome: String, text: String },
    /// octos refused a turn (for example: one is already running).
    PeerTurnRejected { peer: String, error: String },
    /// What a peer's session runs with: its model (from its costs), its
    /// reasoning effort (from session/open). `None` leaves a value as it is.
    PeerInfo { peer: String, model: Option<String>, effort: Option<String> },
    /// The peer started another round (an iteration of its model loop).
    PeerRound { peer: String },
    /// A file the peer's file tools wrote, as octos resolved it.
    PeerFileChanged { peer: String, path: String },
    /// How many of the peer's subagents run now.
    PeerAgents { peer: String, running: u64 },
    /// One of the peer's subagents changed: started, finished, failed.
    PeerSubagent { peer: String, id: String, role: String, title: String, status: String, summary: String },
    /// What a peer's session has used so far (octos's `token_cost_update`).
    PeerCost { peer: String, input: u64, output: u64, cost: f64, context_window: Option<u64> },
    /// OctoBuddy's checks of a peer's work: agent-spec against its contract,
    /// and its test command (`passed` when one ran).
    /// `commit` is set when OctoBuddy committed the peer's files first (or could not).
    PeerChecked { peer: String, verdict: String, passed: Option<bool>, commit: Option<Result<String, String>> },
    /// An approval was decided, resolved by policy or cancelled elsewhere: no longer the person's.
    PeerApprovalGone { peer: String, approval_id: String },
    /// A peer asks the person to approve a tool call.
    PeerApproval { peer: String, approval_id: String, title: String, body: String },
    /// The system octos (OctoSense's kernel) called one of OctoBuddy's tools;
    /// answer through `reply`, once.
    /// `caller`: the calling app as the shell stamped it (`system`: the
    /// system agent); `client`: its request context's client (a Rinx mini app).
    ToolCall { name: String, args: serde_json::Value, caller: String, client: Option<String>, reply: octosense_app_peers::host_tools::ToolReply },
    /// A tool call of a session's outer loop over MCP; the answer goes back on `reply`.
    McpCall { session: String, name: String, args: serde_json::Value, reply: std::sync::mpsc::Sender<Result<String, String>> },
    /// OctoBuddy's peer on the system octos is ready, or why not.
    SystemLink { result: Result<(), String> },
    /// The octos server is gone: every running peer turn is over.
    ServeExited { error: String },
}

pub type Inbox = Arc<Mutex<Vec<LoopEvent>>>;

pub fn post(inbox: &Inbox, event: LoopEvent) {
    if let Ok(mut events) = inbox.lock() {
        events.push(event);
    }
    SignalToUI::set_ui_signal();
}

pub fn take(inbox: &Inbox) -> Vec<LoopEvent> {
    inbox.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default()
}
