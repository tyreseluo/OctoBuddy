//! The lead (outer loop): one long-lived `claude -p` per session, fed with
//! stream-json on stdin. A message written while the lead is busy waits in
//! Claude Code's own queue; an interrupt is a `control_request` that ends
//! the running turn, after which the same conversation carries on.
use crate::events::{post, Inbox, LoopEvent};
use crate::workspace::{find_bin, search_path};
use makepad_widgets::makepad_micro_serde::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

/// What the lead is told about its job, appended to Claude Code's own
/// system prompt.
pub const LEAD_RULES: &str = "You are the lead (outer loop) of OctoBuddy, working in the project in your \
current directory. You never edit files and cannot run commands: you plan, delegate, steer and review. \
OctoBuddy runs the peers and machine-checks their work for you.

PLANS. When the person asks for work, split it into 1-8 slices that inner-loop coding \
agents (octos peers, a cheaper model) run in parallel, all in your directory at the same time: give each \
slice files no other slice changes (its Boundaries), so they never edit the same file. \
Send ALL slices of a request in one plan: never one now and one later. Slices that need another's work \
go in a later wave: OctoBuddy starts a wave only when you have accepted every slice of the waves before \
it, so write the interfaces a later wave relies on into the earlier briefs (as Decisions). \
Give each a role, one word: \
developer, tester, reviewer or writer. A peer sees ONLY its brief, so write every brief as an \
agent-spec Task Contract:
spec: task
name: \"<slice title>\"
---
## Intent
<what to do and why>
## Decisions
- <choices already made>
## Boundaries
### Allowed Changes
- <paths it may change, like src/calc.py or tests/** (write paths with a / or a file extension)>
### Forbidden
- <what must not change>
## Completion Criteria
Scenario: <behavior>
  Test: <the test that proves it>
  Given <...>
  When <...>
  Then <...>
Peers do not commit: OctoBuddy commits each peer's own files, with the message the peer names, as the \
person. OctoBuddy lints each contract when the slice starts and, when the peer finishes, checks the files \
that peer changed against it (its boundaries) and gives you the verdict. \
TESTS. Give every slice that changes code a `check`: ONE shell command, run from the directory's root, that runs \
the tests proving its Completion Criteria (like python3 -m pytest -q, cargo test, npm test). OctoBuddy runs it \
itself when the peer is done and gives you the exit code and the end of the output: your machine evidence, \
where the peer's own words are only a claim. \
ESTIMATE every plan first with the agent-estimation method given at the end, in wave mode: show its \
table in your reply, and put the numbers in the plan: per slice its effective `rounds`, its `wave` (1 for \
no dependencies) and the `reviews` you expect before you accept it (1: it should pass as sent); for the \
plan its `estimate` (total rounds, waves, minutes). OctoBuddy shows them on the task cards, counts the \
rounds each peer really uses, and sizes the review budget from your `reviews`. \
TOOLS. You have OctoBuddy's tools: octobuddy_plan (start slices), octobuddy_send (message inner loops, close them, \
change what you queued), octobuddy_review (your verdicts), octobuddy_status. Use them instead of the fenced blocks \
below: a tool checks what you give it at once and tells you what is wrong, so you fix it in the same turn. Their \
fields are the blocks' fields. (Without the tools, the blocks below do the same.)

To start slices, end your reply with exactly one fenced block, the closing fence on its own line \
(inside JSON the contract is one string, newlines as \\n):
```octobuddy-plan
{\"estimate\":{\"rounds\":18,\"waves\":2,\"minutes\":54},\"slices\":[{\"slug\":\"short-kebab-name\",\"role\":\"developer\",\"brief\":\"spec: task\\n...\",\"check\":\"python3 -m pytest -q\",\"rounds\":4,\"wave\":1,\"reviews\":1,\"model\":\"family/model\"}]}
```

SCOUTS. For a wide look while you plan (a template app, many files, a long doc) send your `scout` subagents \
(subagent_type \"scout\": fast and read-only), several at once with one precise question each, instead of \
reading it all yourself.

SHARED AND SHORT. What every slice needs alike — data structures, storage keys, names, which file each slice \
owns — goes once in the plan's `shared` (a string): OctoBuddy gives it to every slice with its brief. Keep each \
contract short (about 30 lines): its own Intent, Decisions, Boundaries and 2-4 scenarios; point at doc sections \
(file and heading) instead of restating them. Writing long contracts is the slowest part of planning.

MODELS. A slice may name the model it runs on (`model`, one of the MODELS STATUS lists; leave it out for \
the default): a cheaper one for routine work (copy, docs, simple tests), a stronger one for hard code.

AGENTS. A slice may also name the agent it runs on (`agent`, one of the AGENTS STATUS lists: octos, codex, pi, \
claude), with a `model` from that agent's list; leave it out for octos. Name them when the person asks for \
particular agents, or when one suits a slice better. Each slice still owns its files.

WHEN AN AGENT FAILS. A slice whose agent or model failed before doing any work (OctoBuddy says so in its \
report, with the error) is not a failure of the work. Find out why if the error is not plain, then run it again \
on another of the AGENTS and MODELS with octobuddy-send's `rerun` ({\"to\":\"slug\",\"agent\":\"…\",\"model\":\"…\"}): \
it starts afresh from its brief. In your reply tell the person what failed, why, and what you switched it to.

INDEPENDENT REVIEW. For risky slices set `\"independent_review\": true`: when the slice reports with its \
checks passed, it is reviewed with fresh eyes before you accept it. OctoBuddy writes its task, its report and its \
diff to a file and names it; hand that path to your `reviewer` subagent (the Agent tool, subagent_type \
\"reviewer\": a cheaper model, read-only, sharing none of the inner loop's reasoning), which ranks the real \
problems P0 (wrong or broken: fails its task, breaks what worked, loses data, unsafe), P1 (a likely bug, a \
missing case, a breach of its boundaries or contract), P2 (quality), each with file:line, what and why. Do not \
read the diff yourself first. Only for the slice the whole request rests on, or one that touches security or \
data, use a general subagent on your own model instead. Then accept it or send the fixes. Skip it for routine \
work. Several reports may come in one message: answer them all in that turn.

MESSAGES TO PEERS (the inner loops). A peer stays available after its turn: send it more work, context \
or questions by slug, with mode interrupt, steer or queue in octobuddy-send.

## 向 inner 发消息前，先判断「打断」还是「排队」

给 inner（尤其是一次发给多个 inner）投递消息前，先回答一个问题：
这条消息会不会推翻该 inner 正在执行的任务？

会推翻 → 用 mode \"interrupt\" 立即叫停。典型场景：
- 正在实现的设计/方案被修改或推翻
- 需求变更，当前做法已不成立
- 发现正在做的工作方向错误，继续做只会返工
- 用户明确要求立即停止
interrupt 会终止当前 turn，并把新指令原子地预留为该 inner 的下一条输入，
确保它第一时间停下并读到新指令（被打断的 turn 连同它的任务一起丢弃，所以新指令要写全它需要的一切）。
此时不要用 queue——inner 忙时消息只会排队，要等它跑完当前 turn 才被消费，它会在错误方向上继续消耗。

不会推翻 → 用 mode \"queue\" 正常派发。追加任务、补充上下文、追问进度、
派相互独立的新活都属于此类；inner 忙时自动入队、按序消费，不要打断它。
（只补充一个它当前这一步就该读到的细节时，可用 mode \"steer\"：并入正在运行的 turn 的下一步。）

一句话标准：消息晚点到会造成错误或浪费 → interrupt；否则 → queue。
interrupt 会终结未完成的 turn、丢弃进行中的工作，只用于「替换当前任务」，
绝不用于「追加信息」。向多个 inner 广播时逐个套用此判断：
谁的当前任务会被推翻就打断谁，其余照常排队。

STATUS lists what waits in each peer's line with an id; you may change YOUR queued messages before they \
start (cancel, replace, or merge several into one), never the person's. For follow-up work on a peer's \
files, message that peer instead of starting a new slice. A peer stays open when it finishes and after you \
accept its work: the person may ask it for changes in its own tab, and you may send it follow-ups. Never \
close a peer because it finished or was accepted. Close it (`close`) only when the person asks you to, or \
when it must take no more work (its slice was dropped, or given to another peer); a closed peer takes no \
more work. Send with one fenced block, the \
closing fence on its own line (any list may be left out):
```octobuddy-send
{\"close\":[\"slug\"],\"messages\":[{\"to\":\"slug\",\"mode\":\"queue\",\"message\":\"...\"}],\"queue_ops\":[{\"op\":\"cancel\",\"to\":\"slug\",\"id\":\"q3\"},{\"op\":\"replace\",\"to\":\"slug\",\"id\":\"q4\",\"message\":\"...\"},{\"op\":\"merge\",\"to\":\"slug\",\"ids\":[\"q5\",\"q6\"],\"message\":\"...\"}]}
```

WHAT YOU RECEIVE. OctoBuddy's messages start with a tag: \
INNER RESULTS: the reports peers sent when they finished or got blocked, as each one finishes (others may \
still be working: review what came, do not wait for them), one per task, with the agent-spec \
verdict when there was a contract; a report marked as forwarded means the peer did not report and \
OctoBuddy sent the end of its reply. Review each (read the files it changed, listed with it) with a verdict: \
verified, partially verified or unverified, and the evidence, naming which peer did which part. \
A [tests] line is OctoBuddy's own run of the slice's check; a [commit] line is the commit it made of the \
peer's files (or why it made none). Then end with one block giving each peer you reviewed your call: accept \
(verified, done: send it nothing more) or fix (not yet: send that peer exactly what to fix, and its next \
report comes back to you). Keep reviewing and fixing until the person's request is met. Its commits stay \
either way; to undo work, send a peer to revert it.
```octobuddy-review
{\"branches\":[{\"slug\":\"slug\",\"verdict\":\"accept\",\"why\":\"tests pass, within boundaries\"}]}
```

STATUS: the peers' state and queues, and whether the person merged or discarded the session's worktree; \
it comes with the person's messages and with results. The person may also talk to a peer directly: that \
stays between them, and you do not hear of it (a peer reports to you on your tasks, and on the first task of \
a peer the person made for you). \
SUBAGENTS. You may hand reading to subagents with the Agent tool (they read, like you: they cannot edit \
files or run commands, so a test or a script is an inner loop's to run, its result in its report): \
for a broad search or a survey of unfamiliar code before you plan, or to check several peers' work at once \
when you review. Never use them for the work itself: that is what the inner loops are for. \
A subagent you start finishes within the turn you start it in: wait for its result before you end that turn, \
never leave one running in the background (OctoBuddy may start your process anew between turns, and what \
runs in it then is lost). \
Do not wait or poll for peers: end your turn; their results come to you. Keep replies short.";

/// The agent-estimation skill (github.com/tyreseluo/agent-estimation, MIT),
/// given to the lead whole: the method and its calibration examples.
const ESTIMATION: &str = include_str!("../resources/agent-estimation/SKILL.md");
const CALIBRATION: &str = include_str!("../resources/agent-estimation/calibration-examples.md");

/// What runs a `claude` process.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Outer,
    Plain,
    Inner,
}

/// An inner loop on Claude Code: its standing note (its task, with
/// OctoBuddy's rules for inner loops, comes as its first message).
pub const INNER_RULES: &str = "You are an inner loop of OctoBuddy: one slice of a larger plan, in a folder you share \
with other inner loops and the outer loop (the lead). Do your slice only; change only the files it is about. Do not \
run `git commit` (OctoBuddy commits your files from your `Commit:` line). Follow the rules in your first message.";

/// A plain chat's system note: no project, no loops.
pub const PLAIN_CHAT: &str = "You are chatting with the person in OctoBuddy, an app on OctoSense. This chat belongs to \
no project: your working folder is a scratch folder of its own, empty at first. Answer the person directly, in \
their language. You may read files and look things up on the web; you cannot change files or run commands here \
(for work on a codebase, the person opens a project in OctoBuddy).";

/// What the lead is told: its rules, then the estimation method.
pub fn lead_prompt() -> String {
    // The skill's front matter is for skill loaders, not for the model.
    let method = ESTIMATION.splitn(3, "---").nth(2).unwrap_or(ESTIMATION).trim();
    format!("{LEAD_RULES}\n\nAGENT-ESTIMATION (the method to estimate every plan with):\n\n{method}\n\n{}", CALIBRATION.trim())
}

/// Claude Code's own model and effort settings for a project: the user's
/// settings, then the project's, then its local ones (the later wins).
/// The lead runs with them; its process reports the model it resolved.
pub fn claude_settings(project: &str) -> (Option<String>, Option<String>) {
    let home = std::env::var("HOME").unwrap_or_default();
    let (mut model, mut effort) = (None, None);
    for file in [format!("{home}/.claude/settings.json"), format!("{project}/.claude/settings.json"), format!("{project}/.claude/settings.local.json")] {
        let Some(v) = std::fs::read_to_string(&file).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { continue };
        if let Some(m) = v.get("model").and_then(|m| m.as_str()) {
            model = Some(m.to_string());
        }
        if let Some(e) = v.get("effortLevel").and_then(|e| e.as_str()) {
            effort = Some(e.to_string());
        }
    }
    (model, effort)
}

/// A running lead. Dropping it stops the process.
pub struct Lead {
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    child: Arc<Mutex<Option<Child>>>,
    /// Codex or pi, speaking their own protocol (`rpc_lead.rs`); none: Claude Code.
    rpc: Option<Arc<crate::rpc_lead::Rpc>>,
    /// Set when OctoBuddy ends it (another engine picked, the loop closed):
    /// its exit is then no failure.
    pub(crate) stopped: Arc<std::sync::atomic::AtomicBool>,
    /// Its number: its `LeadExited` names it, so a late exit of a process
    /// already replaced leaves the new one alone.
    pub(crate) gen: u64,
    /// Claude Code: what was written to it and not yet taken up, in order
    /// (true: steered into a running turn). It echoes each message as it
    /// takes it up (`--replay-user-messages`).
    sent: Arc<Mutex<std::collections::VecDeque<bool>>>,
}

/// The next agent process's number.
pub(crate) fn next_gen() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

/// Its own process group, so that ending it ends what it started too
/// (Codex's launcher runs the real program as a child of its own).
pub(crate) fn own_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    let _ = cmd;
}

/// Ends `child` and its process group (`own_group`).
pub(crate) fn end(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill").arg("-TERM").arg(format!("-{}", child.id())).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    let _ = child.kill();
}

impl Drop for Lead {
    fn drop(&mut self) {
        self.stopped.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock() {
            if let Some(child) = child.as_mut() {
                end(child);
            }
        }
    }
}

impl Lead {
    /// Starts `claude` for `session` in `cwd` (where the inner loops work
    /// too), resuming `resume` when given.
    /// `extra` follows OctoBuddy's rules (what kind of project this is).
    /// `model`: the one the person picked (none: Claude Code's own setting).
    /// `mcp`: MCP servers it may use besides (`--mcp-config`), allowed by name.
    /// `mode`: the outer loop, a plain chat (no OctoBuddy rules; tools to read
    /// and to look things up on the web), or an inner loop (it edits and runs
    /// commands, in Claude Code's sandbox). `env`: what the child runs with
    /// besides (OctoBuddy's proxy to another provider, say).
    /// `effort`: the reasoning effort picked (none: its own setting).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(inbox: &Inbox, session: &str, cwd: &str, resume: Option<&str>, extra: &str, model: Option<&str>, effort: Option<&str>, mcp: &[(String, String)], mode: Mode, env: &[(String, String)]) -> Result<Lead, String> {
        crate::agents::ready(inbox, "claude")?;
        let bin = find_bin("claude");
        let mut cmd = Command::new(&bin);
        // OctoBuddy's copy stays the version it keeps: no self-update.
        if crate::agents::is_kept(&bin) {
            cmd.env("DISABLE_AUTOUPDATER", "1");
        }
        let (tools, prompt) = match mode {
            Mode::Plain => ("Read,Glob,Grep,WebSearch,WebFetch", format!("{PLAIN_CHAT}{extra}")),
            // Agent: the lead may hand reading and research to subagents
            // (they get the same read-only tools).
            Mode::Outer => ("Read,Glob,Grep,Agent", format!("{}{extra}", lead_prompt())),
            Mode::Inner => ("Read,Edit,Write,MultiEdit,Glob,Grep,Bash,TodoWrite,Agent", format!("{INNER_RULES}{extra}")),
        };
        if mode == Mode::Inner {
            // Edits without asking, commands in Claude Code's sandbox (never
            // outside it): what octos's sandbox gives an inner loop.
            cmd.args(["--permission-mode", "acceptEdits", "--allowedTools", "Read,Edit,Write,MultiEdit,Glob,Grep,Bash,TodoWrite,Agent"])
                .arg("--settings").arg(r#"{"sandbox":{"enabled":true,"autoAllowBashIfSandboxed":true,"allowUnsandboxedCommands":false}}"#);
        }
        if mode == Mode::Outer {
            // Its reviewer, on a cheaper model (INDEPENDENT REVIEW).
            cmd.arg("--agents").arg(crate::review::reviewer_agent());
        }
        if !env.is_empty() {
            for name in crate::claude_proxy::SCRUB {
                cmd.env_remove(name);
            }
            for (k, v) in env {
                cmd.env(k, v);
            }
        }
        // Each message echoed as it is taken up: how a steered one is known
        // to have reached the turn it was meant for.
        cmd.args(["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--replay-user-messages"])
            // Token-level deltas, to show the reply as it is written.
            .arg("--include-partial-messages")
            .args(["--tools", tools])
            // The lead needs no MCP server; without this it also hears about
            // the account's unauthorized connectors and repeats that to the person.
            .arg("--strict-mcp-config")
            .arg("--append-system-prompt").arg(prompt);
        if let Some(id) = resume {
            cmd.arg("--resume").arg(id);
        }
        if let Some(model) = model {
            cmd.arg("--model").arg(model);
        }
        if let Some(effort) = effort {
            cmd.arg("--effort").arg(effort);
        }
        for (name, config) in mcp {
            cmd.arg("--mcp-config").arg(config).arg("--allowedTools").arg(format!("mcp__{name}"));
        }
        cmd.current_dir(cwd).env("PATH", search_path())
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        own_group(&mut cmd);
        let mut child = cmd.spawn().map_err(|err| format!("could not start claude: {err}"))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take();
        let child = Arc::new(Mutex::new(Some(child)));

        let (inbox, session) = (inbox.clone(), session.to_string());
        let reaper = child.clone();
        let stopped: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let ended = stopped.clone();
        let gen = next_gen();
        let sent: Arc<Mutex<std::collections::VecDeque<bool>>> = Arc::default();
        let taken = sent.clone();
        std::thread::spawn(move || {
            let stderr_text = std::thread::spawn(move || {
                let mut text = String::new();
                if let Some(mut pipe) = stderr {
                    let _ = pipe.read_to_string(&mut text);
                }
                text
            });
            let mut texts: Vec<String> = Vec::new();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                // A message of ours echoed as it is taken up: a steered one
                // now reaches its turn.
                if line.contains("\"isReplay\"") {
                    let v: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
                    if v["type"] == "user" && v["isReplay"] == true {
                        let steered = taken.lock().ok().and_then(|mut q| q.pop_front()).unwrap_or(false);
                        if steered {
                            post(&inbox, LoopEvent::LeadSteerTaken { session: session.clone() });
                        }
                        continue;
                    }
                }
                let Ok(ev) = StreamEvent::deserialize_json_lenient(&line) else { continue };
                // A subagent's own messages carry the Agent call they belong
                // to: they are steps of that call, never the lead's text.
                if let Some(parent) = ev.parent_tool_use_id.clone().filter(|p| !p.is_empty()) {
                    if ev.kind == "assistant" {
                        for part in ev.message.and_then(|m| m.content).unwrap_or_default() {
                            if part.kind == "tool_use" {
                                post(&inbox, LoopEvent::LeadSubTool { session: session.clone(), parent: parent.clone(), name: part.name.clone().unwrap_or_default(), detail: tool_detail(&part) });
                            }
                        }
                    }
                    continue;
                }
                match ev.kind.as_str() {
                    "system" if ev.subtype.as_deref() == Some("init") => {
                        post(&inbox, LoopEvent::LeadStatus { session: session.clone(), status: "thinking".into() });
                        if let Some(model) = ev.model.clone() {
                            post(&inbox, LoopEvent::LeadInfo { session: session.clone(), model });
                        }
                    }
                    "stream_event" => {
                        let Some(e) = ev.event else { continue };
                        match e.kind.as_str() {
                            "content_block_delta" => {
                                if let Some(text) = e.delta.and_then(|d| (d.kind == "text_delta").then_some(d.text).flatten()) {
                                    post(&inbox, LoopEvent::LeadDelta { session: session.clone(), text });
                                }
                            }
                            "content_block_start" => {
                                if let Some(b) = e.content_block.filter(|b| b.kind == "tool_use") {
                                    let name = b.name.unwrap_or_default();
                                    post(&inbox, LoopEvent::LeadStatus { session: session.clone(), status: crate::stream::tool_label(&name).to_string() });
                                    post(&inbox, LoopEvent::LeadTool { session: session.clone(), id: b.id.unwrap_or_default(), name, detail: String::new() });
                                }
                            }
                            _ => {}
                        }
                    }
                    "assistant" => {
                        let mut message = Vec::new();
                        for part in ev.message.and_then(|m| m.content).unwrap_or_default() {
                            match part.kind.as_str() {
                                "text" => if let Some(text) = part.text.filter(|t| !t.trim().is_empty()) {
                                    message.push(text);
                                },
                                "tool_use" => {
                                    let (status, detail) = (describe_tool(&part), tool_detail(&part));
                                    post(&inbox, LoopEvent::LeadStatus { session: session.clone(), status });
                                    post(&inbox, LoopEvent::LeadTool {
                                        session: session.clone(), id: part.id.clone().unwrap_or_default(),
                                        name: part.name.clone().unwrap_or_default(), detail,
                                    });
                                }
                                _ => {}
                            }
                        }
                        let text = message.join("\n\n");
                        if !text.is_empty() {
                            texts.push(text.clone());
                        }
                        post(&inbox, LoopEvent::LeadMessage { session: session.clone(), text });
                    }
                    "user" => {
                        // Tool results come back as the next user message.
                        for part in ev.message.and_then(|m| m.content).unwrap_or_default() {
                            if part.kind == "tool_result" {
                                post(&inbox, LoopEvent::LeadToolEnd {
                                    session: session.clone(), id: part.tool_use_id.clone().unwrap_or_default(),
                                    ok: !part.is_error.unwrap_or(false),
                                });
                            }
                        }
                    }
                    "result" => {
                        let ok = !ev.is_error.unwrap_or(false) && ev.subtype.as_deref() == Some("success");
                        let result = ev.result.unwrap_or_default();
                        let text = if texts.is_empty() { result.clone() } else { texts.join("\n\n") };
                        let error = (!ok).then(|| match ev.subtype.as_deref() {
                            Some("error_during_execution") => "interrupted".to_string(),
                            _ if !result.is_empty() => result.clone(),
                            _ => "the lead reported an error".to_string(),
                        });
                        texts.clear();
                        // Steered too late for this turn: they make the next.
                        let turns = taken.lock().map(|q| q.iter().filter(|s| **s).count()).unwrap_or(0);
                        if turns > 0 {
                            post(&inbox, LoopEvent::LeadSteerCarried { session: session.clone(), turns });
                        }
                        post(&inbox, LoopEvent::LeadTurnDone { session: session.clone(), ok, error, lead_session: ev.session_id, text, cost: ev.total_cost_usd });
                    }
                    _ => {}
                }
            }
            let status = reaper.lock().ok().and_then(|mut c| c.take()).and_then(|mut c| c.wait().ok());
            let stderr = stderr_text.join().unwrap_or_default();
            let error = match status {
                Some(s) if s.success() => None,
                // Ended by OctoBuddy (another engine picked, the loop closed).
                _ if ended.load(std::sync::atomic::Ordering::SeqCst) => None,
                _ => Some(last_lines(&stderr, 4).unwrap_or_else(|| "claude exited".into())),
            };
            post(&inbox, LoopEvent::LeadExited { session, error, gen });
        });
        Ok(Lead { stdin: Arc::new(Mutex::new(stdin)), child, rpc: None, stopped, gen, sent })
    }

    /// A Codex or pi agent, its process kept to end it with the Lead.
    pub(crate) fn rpc(rpc: Arc<crate::rpc_lead::Rpc>, child: Arc<Mutex<Option<Child>>>, stopped: Arc<std::sync::atomic::AtomicBool>, gen: u64) -> Lead {
        Lead { stdin: Arc::new(Mutex::new(None)), child, rpc: Some(rpc), stopped, gen, sent: Arc::default() }
    }

    /// Queues a message: the lead reads it now, or after its current turn.
    pub fn send(&self, text: &str) -> Result<(), String> {
        if let Some(rpc) = &self.rpc {
            return rpc.send(text);
        }
        self.write_user(text, false)
    }

    /// A message into the turn running now, not after it (as Cindy's 插话):
    /// the agent takes it up at its next step (Claude Code and pi after the
    /// tool calls under way, Codex at its next tool boundary), its work not
    /// cut off. An error: no turn to steer (send it the usual way).
    pub fn steer(&self, text: &str) -> Result<(), String> {
        if let Some(rpc) = &self.rpc {
            return rpc.steer(text);
        }
        self.write_user(text, true)
    }

    fn write_user(&self, text: &str, steered: bool) -> Result<(), String> {
        let line = format!("{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":{}}}}}\n", text.serialize_json());
        // In the order written: the echoes come back in it.
        let mut sent = self.sent.lock().map_err(|_| "lead lock poisoned".to_string())?;
        self.write(&line)?;
        sent.push_back(steered);
        Ok(())
    }

    /// Messages written whose echo never came: none is waited for any more.
    pub fn forget_unechoed(&self) {
        if let Ok(mut sent) = self.sent.lock() {
            sent.clear();
        }
    }

    /// Ends the lead's running turn; queued messages stay queued.
    pub fn interrupt(&self) -> Result<(), String> {
        if let Some(rpc) = &self.rpc {
            return rpc.interrupt();
        }
        self.write("{\"type\":\"control_request\",\"request_id\":\"octobuddy-interrupt\",\"request\":{\"subtype\":\"interrupt\"}}\n")
    }

    fn write(&self, line: &str) -> Result<(), String> {
        let mut stdin = self.stdin.lock().map_err(|_| "lead input lock poisoned".to_string())?;
        let pipe = stdin.as_mut().ok_or("the lead has exited")?;
        pipe.write_all(line.as_bytes()).and_then(|_| pipe.flush()).map_err(|err| format!("could not write to the lead: {err}"))
    }
}

#[derive(DeJson)]
struct StreamEvent {
    #[rename(type)]
    kind: String,
    subtype: Option<String>,
    message: Option<StreamMessage>,
    /// For `stream_event`: the API's own streaming event.
    event: Option<PartialEvent>,
    result: Option<String>,
    session_id: Option<String>,
    is_error: Option<bool>,
    /// On `result`: what the lead has cost so far (cumulative in a process).
    total_cost_usd: Option<f64>,
    /// Set on a subagent's messages: the lead's Agent call that runs it.
    parent_tool_use_id: Option<String>,
    /// On `system` `init`: the model this session runs.
    model: Option<String>,
}

#[derive(DeJson)]
struct PartialEvent {
    #[rename(type)]
    kind: String,
    delta: Option<PartialDelta>,
    content_block: Option<PartialBlock>,
}

#[derive(DeJson)]
struct PartialDelta {
    #[rename(type)]
    kind: String,
    text: Option<String>,
}

#[derive(DeJson)]
struct PartialBlock {
    #[rename(type)]
    kind: String,
    id: Option<String>,
    name: Option<String>,
}

#[derive(DeJson)]
struct StreamMessage {
    content: Option<Vec<StreamPart>>,
}

#[derive(DeJson)]
struct StreamPart {
    #[rename(type)]
    kind: String,
    text: Option<String>,
    id: Option<String>,
    name: Option<String>,
    input: Option<ToolInput>,
    tool_use_id: Option<String>,
    is_error: Option<bool>,
}

#[derive(DeJson)]
struct ToolInput {
    file_path: Option<String>,
    pattern: Option<String>,
    path: Option<String>,
    /// An Agent call: which subagent, and what for.
    subagent_type: Option<String>,
    description: Option<String>,
}

/// What a tool call works on, for the step line: its path or pattern, or
/// for a subagent its kind and task.
fn tool_detail(part: &StreamPart) -> String {
    let input = part.input.as_ref();
    if let Some(what) = input.and_then(|i| i.description.clone()) {
        let kind = input.and_then(|i| i.subagent_type.clone()).unwrap_or_else(|| "general-purpose".into());
        return format!("{kind} · {what}");
    }
    input.and_then(|i| i.file_path.clone().or(i.pattern.clone()).or(i.path.clone())).unwrap_or_default()
}

fn describe_tool(part: &StreamPart) -> String {
    let name = crate::stream::tool_label(part.name.as_deref().unwrap_or("a tool"));
    let input = part.input.as_ref();
    let target = input.and_then(|i| i.file_path.as_deref().or(i.pattern.as_deref()).or(i.path.as_deref()));
    let target = target.map(|t| Path::new(t).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| t.to_string()));
    match (name, target) {
        ("Read", Some(t)) => format!("reading {t}"),
        ("Grep", Some(t)) => format!("searching {t}"),
        ("Glob", Some(t)) => format!("listing {t}"),
        ("Agent" | "Task", _) => format!("running a subagent: {}", input.and_then(|i| i.description.as_deref()).unwrap_or("")),
        (name, Some(t)) => format!("{name} {t}"),
        (name, None) => name.to_string(),
    }
}

fn last_lines(text: &str, n: usize) -> Option<String> {
    let lines: Vec<_> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    (!lines.is_empty()).then(|| lines[lines.len().saturating_sub(n)..].join("\n"))
}

/// Live checks' shared part: `lead` is asked to run a slow command; once
/// it runs, it is steered; the turn must take the steer up (one turn, its
/// answer as steered). Returns the answer.
#[cfg(test)]
pub(crate) fn steered_turn(lead: &Lead, inbox: &Inbox, label: &str) -> String {
    lead.send("Run the shell command `sleep 6; echo done-sleeping`, then tell me its output in one sentence.").unwrap();
    let started = std::time::Instant::now();
    let (mut steered, mut taken, mut done, mut seen) = (false, false, None, Vec::new());
    while done.is_none() {
        assert!(started.elapsed().as_secs() < 150, "{label}: no end in time; {seen:?}");
        for e in crate::events::take(inbox) {
            seen.push(format!("{e:?}").chars().take(70).collect::<String>());
            match e {
                LoopEvent::LeadTool { .. } if !steered => {
                    lead.steer("Also: end your final reply with the word BANANA.").unwrap();
                    steered = true;
                }
                LoopEvent::LeadSteerTaken { .. } => taken = true,
                LoopEvent::LeadSteerRefused { .. } => panic!("{label}: steer refused; {seen:?}"),
                LoopEvent::LeadSteerCarried { .. } => panic!("{label}: steer left for the next turn; {seen:?}"),
                LoopEvent::LeadTurnDone { ok, text, .. } => {
                    assert!(ok, "{label}: {text}");
                    done = Some(text);
                }
                _ => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let text = done.unwrap();
    eprintln!("{label}: {:?} in {:?}", crate::stream::strip_think(&text).chars().take(90).collect::<String>(), started.elapsed());
    assert!(steered && taken, "{label}: steered {steered}, taken {taken}; {seen:?}");
    assert!(text.contains("BANANA"), "{label}: the steer was heard: {text}");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live: Claude Code (your login) steered while its command runs.
    #[test]
    #[ignore]
    fn claude_steers_a_turn() {
        let dir = std::env::temp_dir().join(format!("octobuddy-claude-steer-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let inbox = Inbox::default();
        let lead = Lead::spawn(&inbox, "claude-steer", &dir.to_string_lossy(), None, "", Some("haiku"), None, &[], Mode::Inner, &[]).unwrap();
        steered_turn(&lead, &inbox, "claude");
    }

    #[test]
    fn stream_events_parse() {
        let ev = StreamEvent::deserialize_json_lenient(r#"{"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"t","name":"Read","input":{"file_path":"/a/b/main.rs","limit":5}},{"type":"text","text":"hi"}]},"session_id":"s"}"#).unwrap();
        let parts = ev.message.unwrap().content.unwrap();
        assert_eq!(describe_tool(&parts[0]), "reading main.rs");
        assert_eq!(parts[1].text.as_deref(), Some("hi"));
        assert_eq!(tool_detail(&parts[0]), "/a/b/main.rs");
        let delta = StreamEvent::deserialize_json_lenient(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}},"session_id":"s"}"#).unwrap();
        let d = delta.event.unwrap().delta.unwrap();
        assert_eq!((d.kind.as_str(), d.text.as_deref()), ("text_delta", Some("Hel")));
        let start = StreamEvent::deserialize_json_lenient(r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_1","name":"Read","input":{}}}}"#).unwrap();
        let b = start.event.unwrap().content_block.unwrap();
        assert_eq!((b.kind.as_str(), b.id.as_deref(), b.name.as_deref()), ("tool_use", Some("toolu_1"), Some("Read")));
        let result = StreamEvent::deserialize_json_lenient(r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"x","is_error":false}]}}"#).unwrap();
        let r = &result.message.unwrap().content.unwrap()[0];
        assert_eq!((r.tool_use_id.as_deref(), r.is_error), (Some("toolu_1"), Some(false)));
        let done = StreamEvent::deserialize_json_lenient(r#"{"type":"result","subtype":"success","is_error":false,"result":"OK","session_id":"abc","usage":{"input_tokens":3}}"#).unwrap();
        assert_eq!((done.kind.as_str(), done.result.as_deref(), done.session_id.as_deref()), ("result", Some("OK"), Some("abc")));
        let cost = StreamEvent::deserialize_json_lenient(r#"{"type":"result","subtype":"success","total_cost_usd":0.0203,"result":"OK"}"#).unwrap();
        assert_eq!(cost.total_cost_usd, Some(0.0203));
    }

    #[test]
    fn subagents_and_the_model() {
        let init = StreamEvent::deserialize_json_lenient(r#"{"type":"system","subtype":"init","model":"claude-opus-5-5","tools":["Read"],"session_id":"s"}"#).unwrap();
        assert_eq!(init.model.as_deref(), Some("claude-opus-5-5"));
        let call = StreamEvent::deserialize_json_lenient(r#"{"type":"assistant","parent_tool_use_id":null,"message":{"content":[{"type":"tool_use","id":"a1","name":"Agent","input":{"description":"find the tests","prompt":"...","subagent_type":"Explore"}}]}}"#).unwrap();
        assert_eq!(call.parent_tool_use_id, None);
        let parts = call.message.unwrap().content.unwrap();
        assert_eq!(tool_detail(&parts[0]), "Explore · find the tests");
        assert_eq!(describe_tool(&parts[0]), "running a subagent: find the tests");
        let inside = StreamEvent::deserialize_json_lenient(r#"{"type":"assistant","parent_tool_use_id":"a1","message":{"content":[{"type":"tool_use","id":"t2","name":"Grep","input":{"pattern":"def test_"}}]}}"#).unwrap();
        assert_eq!(inside.parent_tool_use_id.as_deref(), Some("a1"));
    }

    #[test]
    fn the_lead_is_given_the_estimation_method() {
        let prompt = lead_prompt();
        assert!(prompt.starts_with("You are the lead"));
        assert!(prompt.contains("## Estimation Procedure") && prompt.contains("# Calibration Examples"));
        assert!(!prompt.contains("name: agent-estimation"), "front matter is dropped");
    }

    #[test]
    fn user_lines_are_json_escaped() {
        let text = "line \"one\"\nline two";
        let line = format!("{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":{}}}}}", text.serialize_json());
        assert!(line.contains(r#""content":"line \"one\"\nline two""#), "{line}");
    }
}
