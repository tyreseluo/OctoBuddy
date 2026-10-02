//! A plain chat with OctoSense's own agent: octos on the shell's one kernel,
//! through the peer the shell gave OctoBuddy (`system.rs`; the one way an app
//! reaches it). Each chat is a request context of that peer, a transcript of
//! its own, keyed by the chat's id. Its model is the one OctoSense's AI
//! providers make primary: the host's, not a chat's (a chat that names
//! another model runs on OctoBuddy's own octos instead). Its events are said
//! as an outer loop's (`Lead*`), so the chat view shows it like any other.
use crate::events::{post, LoopEvent};
use crate::model::SessionRef;
use crate::{i18n, OctoBuddyView};
use octosense_app_peers::{ContextEvent, ContextOp, ContextSpec, EventSink, TurnTrigger};
use serde_json::Value;
use std::sync::{Arc, Mutex};

/// The engine name a session's `OuterPick` carries for it.
pub const ENGINE: &str = "system";

impl OctoBuddyView {
    /// Whether OctoSense's agent can take a chat now.
    pub(crate) fn system_ready(&self) -> bool {
        self.link.is_some() && matches!(self.system_link, Some(Ok(())))
    }

    pub(crate) fn system_unready_text(&self) -> String {
        self.system_unready()
    }

    /// Why it cannot, in the person's words.
    fn system_unready(&self) -> String {
        match (&self.link, &self.system_link) {
            (None, _) if !crate::system::hosted() => i18n::t(
                "OctoBuddy runs on its own here, not in OctoSense: there is no OctoSense agent to use.",
                "这里的 OctoBuddy 是单独运行的，不在 OctoSense 里：没有 OctoSense 的 agent 可用。").into(),
            (None, _) => i18n::t(
                "OctoBuddy may not use OctoSense's agent yet: allow it when OctoSense asks (or in OctoSense Settings › Assistant), then reopen OctoBuddy.",
                "OctoBuddy 还没获准使用 OctoSense 的 agent：在 OctoSense 询问时允许（或在 OctoSense 设置 › 助手 中打开），然后重新打开 OctoBuddy。").into(),
            (_, Some(Err(err))) => i18n::pick(format!("OctoSense's agent is not available: {err}"), format!("OctoSense 的 agent 不可用：{err}")),
            _ => i18n::t("OctoSense's agent is still starting; send again in a moment.", "OctoSense 的 agent 还在启动，请稍后再发。").into(),
        }
    }

    /// The model it runs, as the shell reports it.
    pub(crate) fn system_model(&self) -> Option<String> {
        let m = self.link.as_ref()?.service.model()?;
        m.model.or(m.provider)
    }

    /// Runs a turn of a plain chat on OctoSense's agent.
    pub(crate) fn system_chat_send(&mut self, at: SessionRef, text: &str) -> Result<(), String> {
        if !self.system_ready() {
            let why = self.system_unready();
            self.system_reconnect();
            return Err(why);
        }
        let link = self.link.as_ref().unwrap();
        let id = self.store.session(at).map(|s| s.id.clone()).ok_or("no session")?;
        let context = match self.rt.system_chats.get(&id).filter(|c| c.is_open()) {
            Some(c) => c.clone(),
            None => {
                let services = link.service.services();
                let c = link.service.open_context(ContextSpec { account: "device".into(), instance: format!("chat-{id}"), services })?;
                self.rt.system_chats.insert(id.clone(), c.clone());
                c
            }
        };
        // Its model: as the shell reports it, else AI providers' primary (the one it runs).
        if let Some(model) = self.system_model().or_else(|| self.providers.primary().map(|p| p.label.clone())) {
            post(&self.rt.inbox, LoopEvent::LeadInfo { session: id.clone(), model });
        }
        let sink = forward(self.rt.inbox.clone(), id.clone());
        context.call(ContextOp::TurnFrom { text: text.to_string(), trigger: TurnTrigger::Person }, sink)
    }

    /// After a failed link: try again, once at a time.
    pub(crate) fn system_reconnect(&mut self) {
        if let (Some(link), Some(Err(_))) = (self.link.as_ref(), self.system_link.as_ref()) {
            link.retry(&self.rt.inbox);
            self.system_link = None;
        }
    }

    /// Stops a plain chat's running turn on OctoSense's agent.
    pub(crate) fn system_chat_stop(&mut self, session: &str) {
        if let Some(c) = self.rt.system_chats.get(session) {
            let _ = c.call(ContextOp::Interrupt, Arc::new(|_| {}));
        }
    }
}

/// The context's events, said as an outer loop's.
fn forward(inbox: crate::events::Inbox, session: String) -> EventSink {
    // The reply so far: the broker sends the whole text, the view takes deltas.
    let said = Arc::new(Mutex::new(String::new()));
    Arc::new(move |event| match event {
        ContextEvent::Data(data) => {
            let params = &data["params"];
            let s = |k: &str| params[k].as_str().unwrap_or("").to_string();
            match data["method"].as_str().unwrap_or("") {
                "turn/started" => post(&inbox, LoopEvent::LeadStatus { session: session.clone(), status: "thinking".into() }),
                "tool/started" => {
                    let detail = params.get("arguments").or(params.get("input")).map(short_args).unwrap_or_default();
                    post(&inbox, LoopEvent::LeadTool { session: session.clone(), id: s("tool_call_id"), name: s("tool_name"), detail });
                }
                "tool/completed" => {
                    let ok = params["error"].is_null() && params["status"].as_str() != Some("failed");
                    post(&inbox, LoopEvent::LeadToolEnd { session: session.clone(), id: s("tool_call_id"), ok });
                }
                _ => {}
            }
            if let Some(text) = data.get("text").and_then(Value::as_str) {
                let mut said = said.lock().unwrap_or_else(|e| e.into_inner());
                if text.len() > said.len() && text.starts_with(said.as_str()) {
                    post(&inbox, LoopEvent::LeadDelta { session: session.clone(), text: text[said.len()..].to_string() });
                    *said = text.to_string();
                }
            }
        }
        ContextEvent::Complete(result) => {
            let so_far = said.lock().map(|s| s.clone()).unwrap_or_default();
            match result {
                Ok(v) => {
                    let text = v["text"].as_str().filter(|t| !t.trim().is_empty()).map(String::from).unwrap_or(so_far);
                    post(&inbox, LoopEvent::LeadMessage { session: session.clone(), text: text.clone() });
                    post(&inbox, LoopEvent::LeadTurnDone { session: session.clone(), ok: true, error: None, lead_session: Some(format!("chat-{session}")), text, cost: None });
                }
                Err(err) => {
                    if !so_far.is_empty() {
                        post(&inbox, LoopEvent::LeadMessage { session: session.clone(), text: so_far.clone() });
                    }
                    let error = if err.to_ascii_lowercase().contains("interrupt") || err.contains("cancelled") { "interrupted".to_string() } else { err };
                    post(&inbox, LoopEvent::LeadTurnDone { session: session.clone(), ok: false, error: Some(error), lead_session: None, text: so_far, cost: None });
                }
            }
        }
    })
}

/// A tool call's arguments, in a line.
fn short_args(v: &Value) -> String {
    let text = match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    text.chars().take(120).collect()
}
