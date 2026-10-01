//! Codex on a Chat Completions provider (Z.ai's GLM, …). Codex speaks only
//! OpenAI's Responses API (`wire_api = "chat"` is gone); these providers
//! speak Chat Completions. OctoBuddy's proxy translates, as Cindy's bridge
//! does: a Responses request becomes a chat one (`to_chat`), and the chat
//! stream's chunks become the Responses stream's events (`Translator`).
//!
//! What crosses: instructions (a system message), the conversation's
//! messages, function calls and their outputs, function tools, and Codex's
//! free-form tools (`custom`, like `apply_patch`) as a function taking one
//! `input` string, given back as `custom_tool_call`. What does not: hosted
//! tools (web search, a local shell), reasoning items from earlier turns.
use serde_json::{json, Map, Value};

/// A Responses request as a Chat Completions one, and the names of the
/// free-form tools in it (their calls go back as `custom_tool_call`).
pub fn to_chat(req: &Value) -> (Value, Vec<String>) {
    let mut messages: Vec<Value> = Vec::new();
    if let Some(text) = req.get("instructions").and_then(Value::as_str).filter(|t| !t.trim().is_empty()) {
        messages.push(json!({"role": "system", "content": text}));
    }
    let text_of = |content: &Value| -> String {
        match content {
            Value::String(s) => s.clone(),
            Value::Array(parts) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join(""),
            _ => String::new(),
        }
    };
    let input = match req.get("input") {
        Some(Value::String(s)) => vec![json!({"type": "message", "role": "user", "content": s})],
        Some(Value::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    for item in &input {
        let kind = item.get("type").and_then(Value::as_str).unwrap_or("message");
        match kind {
            "message" => {
                let role = match item.get("role").and_then(Value::as_str).unwrap_or("user") {
                    "developer" | "system" => "system",
                    "assistant" => "assistant",
                    _ => "user",
                };
                let text = text_of(item.get("content").unwrap_or(&Value::Null));
                if text.is_empty() {
                    continue;
                }
                messages.push(json!({"role": role, "content": text}));
            }
            "function_call" | "custom_tool_call" => {
                let call = json!({
                    "id": item.get("call_id").cloned().unwrap_or(Value::Null),
                    "type": "function",
                    "function": {
                        "name": item.get("name").cloned().unwrap_or(Value::Null),
                        "arguments": if kind == "custom_tool_call" {
                            json!({"input": item.get("input").and_then(Value::as_str).unwrap_or("")}).to_string()
                        } else {
                            item.get("arguments").and_then(Value::as_str).unwrap_or("{}").to_string()
                        },
                    },
                });
                // Calls of one answer: one assistant message.
                match messages.last_mut() {
                    Some(last) if last["role"] == "assistant" && last.get("tool_calls").is_some() => {
                        last["tool_calls"].as_array_mut().unwrap().push(call);
                    }
                    _ => messages.push(json!({"role": "assistant", "content": Value::Null, "tool_calls": [call]})),
                }
            }
            "function_call_output" | "custom_tool_call_output" => {
                let output = match item.get("output") {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => other.get("content").map(text_of).filter(|t| !t.is_empty()).unwrap_or_else(|| other.to_string()),
                    None => String::new(),
                };
                messages.push(json!({"role": "tool", "tool_call_id": item.get("call_id").cloned().unwrap_or(Value::Null), "content": output}));
            }
            _ => {}
        }
    }
    let mut custom = Vec::new();
    let tools: Vec<Value> = req.get("tools").and_then(Value::as_array).into_iter().flatten().filter_map(|t| {
        match t.get("type").and_then(Value::as_str)? {
            "function" => Some(json!({"type": "function", "function": {
                "name": t.get("name")?, "description": t.get("description").cloned().unwrap_or(json!("")),
                "parameters": t.get("parameters").cloned().unwrap_or(json!({"type": "object", "properties": {}})),
            }})),
            "custom" => {
                let name = t.get("name")?.as_str()?.to_string();
                custom.push(name.clone());
                let mut description = t.get("description").and_then(Value::as_str).unwrap_or("").to_string();
                if let Some(grammar) = t.pointer("/format/definition").and_then(Value::as_str) {
                    description.push_str(&format!("\n\nThe input follows this grammar:\n{grammar}"));
                }
                Some(json!({"type": "function", "function": {"name": name, "description": description,
                    "parameters": {"type": "object", "properties": {"input": {"type": "string", "description": "the tool's whole input"}}, "required": ["input"]}}}))
            }
            _ => None,
        }
    }).collect();
    let mut out = Map::new();
    out.insert("model".into(), req.get("model").cloned().unwrap_or(json!("")));
    out.insert("messages".into(), Value::Array(messages));
    out.insert("stream".into(), json!(true));
    out.insert("stream_options".into(), json!({"include_usage": true}));
    if !tools.is_empty() {
        out.insert("tools".into(), Value::Array(tools));
        out.insert("tool_choice".into(), req.get("tool_choice").filter(|c| c.is_string()).cloned().unwrap_or(json!("auto")));
        if let Some(p) = req.get("parallel_tool_calls") {
            out.insert("parallel_tool_calls".into(), p.clone());
        }
    }
    if let Some(n) = req.get("max_output_tokens") {
        out.insert("max_tokens".into(), n.clone());
    }
    if let Some(t) = req.get("temperature") {
        out.insert("temperature".into(), t.clone());
    }
    (Value::Object(out), custom)
}

/// A tool call being streamed: where it sits in the output, what it is.
struct Call {
    index: usize,
    id: String,
    call_id: String,
    name: String,
    arguments: String,
    custom: bool,
}

/// The chat stream's chunks as the Responses stream's events (each an
/// `event:` name and its `data:` JSON).
pub struct Translator {
    id: String,
    model: String,
    custom: Vec<String>,
    started: bool,
    next: usize,
    /// The assistant message being written: its output index and text.
    text: Option<(usize, String, String)>,
    calls: Vec<(u64, Call)>,
    done: Vec<Value>,
    usage: Value,
}

impl Translator {
    pub fn new(model: &str, custom: Vec<String>) -> Translator {
        let id = format!("resp_{}", crate::model::new_id("o"));
        Translator { id, model: model.into(), custom, started: false, next: 0, text: None, calls: Vec::new(), done: Vec::new(), usage: Value::Null }
    }

    fn response(&self, status: &str, output: Vec<Value>) -> Value {
        let mut r = json!({"id": self.id, "object": "response", "status": status, "model": self.model, "output": output});
        if !self.usage.is_null() {
            r["usage"] = self.usage.clone();
        }
        r
    }

    fn begin(&mut self, out: &mut Vec<(String, Value)>) {
        if !self.started {
            self.started = true;
            out.push(("response.created".into(), json!({"type": "response.created", "response": self.response("in_progress", Vec::new())})));
        }
    }

    fn end_text(&mut self, out: &mut Vec<(String, Value)>) {
        if let Some((index, id, text)) = self.text.take() {
            let item = json!({"type": "message", "id": id, "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": text, "annotations": []}]});
            out.push(("response.output_text.done".into(), json!({"type": "response.output_text.done", "item_id": id, "output_index": index, "content_index": 0, "text": text})));
            out.push(("response.output_item.done".into(), json!({"type": "response.output_item.done", "output_index": index, "item": item})));
            self.done.push(item);
        }
    }

    /// One chunk of the chat stream (its `data:` JSON).
    pub fn feed(&mut self, chunk: &Value) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        self.begin(&mut out);
        if let Some(u) = chunk.get("usage").filter(|u| u.is_object()) {
            let input = u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0);
            let output = u.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0);
            let cached = u.pointer("/prompt_tokens_details/cached_tokens").and_then(Value::as_u64).unwrap_or(0);
            self.usage = json!({"input_tokens": input, "input_tokens_details": {"cached_tokens": cached},
                "output_tokens": output, "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": input + output});
        }
        let Some(choice) = chunk.get("choices").and_then(Value::as_array).and_then(|c| c.first()) else { return out };
        let delta = choice.get("delta").cloned().unwrap_or(Value::Null);
        if let Some(text) = delta.get("content").and_then(Value::as_str).filter(|t| !t.is_empty()) {
            if self.text.is_none() {
                let (index, id) = (self.next, format!("msg_{}", crate::model::new_id("m")));
                self.next += 1;
                out.push(("response.output_item.added".into(), json!({"type": "response.output_item.added", "output_index": index,
                    "item": {"type": "message", "id": id, "role": "assistant", "status": "in_progress", "content": []}})));
                out.push(("response.content_part.added".into(), json!({"type": "response.content_part.added", "item_id": id, "output_index": index, "content_index": 0,
                    "part": {"type": "output_text", "text": "", "annotations": []}})));
                self.text = Some((index, id, String::new()));
            }
            let (index, id, all) = self.text.as_mut().unwrap();
            all.push_str(text);
            out.push(("response.output_text.delta".into(), json!({"type": "response.output_text.delta", "item_id": id, "output_index": *index, "content_index": 0, "delta": text})));
        }
        for tc in delta.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
            let n = tc.get("index").and_then(Value::as_u64).unwrap_or(0);
            if !self.calls.iter().any(|(k, _)| *k == n) {
                // Text before the calls is a message of its own, done.
                self.end_text(&mut out);
                let name = tc.pointer("/function/name").and_then(Value::as_str).unwrap_or("").to_string();
                let call_id = tc.get("id").and_then(Value::as_str).map(String::from).unwrap_or_else(|| format!("call_{}", crate::model::new_id("c")));
                let custom = self.custom.contains(&name);
                let call = Call { index: self.next, id: format!("fc_{}", crate::model::new_id("f")), call_id, name, arguments: String::new(), custom };
                self.next += 1;
                let item = if custom {
                    json!({"type": "custom_tool_call", "id": call.id, "call_id": call.call_id, "name": call.name, "input": "", "status": "in_progress"})
                } else {
                    json!({"type": "function_call", "id": call.id, "call_id": call.call_id, "name": call.name, "arguments": "", "status": "in_progress"})
                };
                out.push(("response.output_item.added".into(), json!({"type": "response.output_item.added", "output_index": call.index, "item": item})));
                self.calls.push((n, call));
            }
            let call = &mut self.calls.iter_mut().find(|(k, _)| *k == n).unwrap().1;
            if let Some(name) = tc.pointer("/function/name").and_then(Value::as_str).filter(|n| !n.is_empty() && call.name.is_empty()) {
                call.name = name.to_string();
            }
            if let Some(args) = tc.pointer("/function/arguments").and_then(Value::as_str).filter(|a| !a.is_empty()) {
                call.arguments.push_str(args);
                if !call.custom {
                    out.push(("response.function_call_arguments.delta".into(), json!({"type": "response.function_call_arguments.delta", "item_id": call.id, "output_index": call.index, "delta": args})));
                }
            }
        }
        out
    }

    /// The stream ended: what is open closes, and the response completes.
    pub fn finish(&mut self) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        self.begin(&mut out);
        self.end_text(&mut out);
        for (_, call) in std::mem::take(&mut self.calls) {
            let item = if call.custom {
                let input = serde_json::from_str::<Value>(&call.arguments).ok()
                    .and_then(|v| v.get("input").and_then(Value::as_str).map(String::from)).unwrap_or(call.arguments.clone());
                json!({"type": "custom_tool_call", "id": call.id, "call_id": call.call_id, "name": call.name, "input": input, "status": "completed"})
            } else {
                let arguments = if call.arguments.trim().is_empty() { "{}".to_string() } else { call.arguments.clone() };
                out.push(("response.function_call_arguments.done".into(), json!({"type": "response.function_call_arguments.done", "item_id": call.id, "output_index": call.index, "arguments": arguments})));
                json!({"type": "function_call", "id": call.id, "call_id": call.call_id, "name": call.name, "arguments": arguments, "status": "completed"})
            };
            out.push(("response.output_item.done".into(), json!({"type": "response.output_item.done", "output_index": call.index, "item": item})));
            self.done.push(item);
        }
        let output = std::mem::take(&mut self.done);
        out.push(("response.completed".into(), json!({"type": "response.completed", "response": self.response("completed", output)})));
        out
    }
}

/// An event as the Responses stream sends it.
pub fn sse(name: &str, data: &Value) -> String {
    format!("event: {name}\ndata: {data}\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_responses_request_becomes_a_chat_one() {
        let req = json!({
            "model": "glm-5.3", "instructions": "be brief", "stream": true,
            "input": [
                {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "rules"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "list files"}]},
                {"type": "function_call", "call_id": "c1", "name": "shell", "arguments": "{\"command\":[\"ls\"]}"},
                {"type": "function_call", "call_id": "c2", "name": "shell", "arguments": "{\"command\":[\"pwd\"]}"},
                {"type": "function_call_output", "call_id": "c1", "output": "a.rs"},
                {"type": "custom_tool_call", "call_id": "c3", "name": "apply_patch", "input": "*** Begin Patch"},
                {"type": "reasoning", "summary": []}
            ],
            "tools": [
                {"type": "function", "name": "shell", "description": "run", "parameters": {"type": "object"}},
                {"type": "custom", "name": "apply_patch", "description": "patch", "format": {"type": "grammar", "definition": "start: x"}},
                {"type": "web_search"}
            ]
        });
        let (chat, custom) = to_chat(&req);
        let m = chat["messages"].as_array().unwrap();
        assert_eq!(m[0], json!({"role": "system", "content": "be brief"}));
        assert_eq!(m[1]["role"], "system");
        assert_eq!(m[2], json!({"role": "user", "content": "list files"}));
        // Two calls of one answer: one assistant message.
        assert_eq!(m[3]["tool_calls"].as_array().unwrap().len(), 2);
        assert_eq!(m[4], json!({"role": "tool", "tool_call_id": "c1", "content": "a.rs"}));
        assert_eq!(m[5]["tool_calls"][0]["function"]["arguments"], json!({"input": "*** Begin Patch"}).to_string());
        assert_eq!(m.len(), 6, "a reasoning item does not cross");
        assert_eq!(chat["tools"].as_array().unwrap().len(), 2, "hosted tools do not cross");
        assert_eq!(custom, ["apply_patch"]);
        assert_eq!(chat["stream"], true);
    }

    #[test]
    fn a_chat_stream_becomes_responses_events() {
        let mut t = Translator::new("glm-5.3", vec!["apply_patch".into()]);
        let mut names = Vec::new();
        let chunks = [
            json!({"choices": [{"delta": {"role": "assistant", "reasoning_content": "hmm"}}]}),
            json!({"choices": [{"delta": {"content": "Hel"}}]}),
            json!({"choices": [{"delta": {"content": "lo"}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "id": "call_a", "function": {"name": "shell", "arguments": "{\"comm"}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"arguments": "and\":[\"ls\"]}"}}]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [{"index": 1, "id": "call_b", "function": {"name": "apply_patch", "arguments": "{\"input\":\"*** Begin Patch\"}"}}]}}]}),
            json!({"choices": [{"delta": {}, "finish_reason": "tool_calls"}], "usage": {"prompt_tokens": 10, "completion_tokens": 5}}),
        ];
        let mut all = Vec::new();
        for c in &chunks {
            all.extend(t.feed(c));
        }
        all.extend(t.finish());
        for (n, _) in &all {
            names.push(n.as_str());
        }
        assert_eq!(names.first(), Some(&"response.created"));
        assert_eq!(names.last(), Some(&"response.completed"));
        let text: String = all.iter().filter(|(n, _)| n == "response.output_text.delta").map(|(_, d)| d["delta"].as_str().unwrap()).collect();
        assert_eq!(text, "Hello");
        let done = &all.last().unwrap().1["response"];
        let output = done["output"].as_array().unwrap();
        assert_eq!(output[0]["content"][0]["text"], "Hello");
        assert_eq!(output[1]["type"], "function_call");
        assert_eq!(output[1]["arguments"], "{\"command\":[\"ls\"]}");
        assert_eq!(output[1]["call_id"], "call_a");
        assert_eq!(output[2]["type"], "custom_tool_call");
        assert_eq!(output[2]["input"], "*** Begin Patch");
        assert_eq!(done["usage"]["total_tokens"], 15);
        assert!(sse("response.created", &json!({"a": 1})).starts_with("event: response.created\ndata: {"));
    }
}
