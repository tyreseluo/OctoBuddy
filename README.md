# OctoBuddy

English | [简体中文](README.zh-CN.md)

OctoBuddy is OctoSense's native coding app with two loops. An **outer loop** turns a request into a plan of slices and reviews what comes back. **Inner loops** work on the slices in parallel, in the project's folder (or its git worktree). A plain **chat** talks to one agent, with no loops.

It runs inside OctoSense as a native app, and on its own in a window of its own. OctoSense registers it in its `native-apps.json` as the app `octobuddy`, pinned to a revision of this repository, the way it takes Rinx. Desktop shells build it by default; phone shells leave it out. That registration is ready on the branch [`feat/octobuddy-app`](https://github.com/tyreseluo/OctoSense/tree/feat/octobuddy-app) of a fork of OctoSense (it pins `fad8480`). **Pending:** its pull request to OctoSense is not made yet.

It was called OctoLoop until 2026-10-02.

## Agents and models

| Agent | Outer loop / chat | Inner loops | How OctoBuddy drives it |
| --- | --- | --- | --- |
| Claude Code | yes | yes | `claude -p`, stream-json in and out |
| Codex | yes | yes | `codex app-server` (JSON-RPC) |
| pi | yes | yes | `pi --mode rpc` |
| octos | yes | yes (the default) | OctoBuddy's own `octos serve`, or OctoSense's agent in a chat |

- **Models.** Claude Code can use your own login. Every agent can also use the providers enabled in **AI providers**, through OctoBuddy's loopback proxy.
- **Codex.** Its requests go through a Responses → Chat Completions bridge, because those providers have no `/responses`.
- **Keys.** OctoBuddy's proxy reads the provider's key from the AI providers profile and adds it to the requests it forwards. A child process gets only a placeholder key, so no key is in its environment or argv.
- **Choosing.** The picker under the composer shows agents on the left and their models on the right, with the reasoning effort under the models.
  - Switching to another model of the same agent keeps the conversation (Codex resumes its thread, pi its session file).
  - Switching to another agent starts a new conversation.
  - Changing the effort restarts the agent on the same conversation. If a turn is under way, that happens when it ends.

## In a conversation

- **Stop.** The Stop button, or Esc, interrupts the turn. If it has not stopped after 5 seconds, its process group is ended. What it wrote so far is kept.
- **Go on.** After a stop, the queue waits. **Go on** sends what is queued, or asks the agent to continue from where it was cut off.
- **Steer.** ⌘Enter (Ctrl+Enter elsewhere) puts a message into the running turn without cutting it off. A queued message has a **Steer in** button that does the same.
  - The agent takes it up at its next step. If it arrives just after the turn ends, it goes first in the queue instead.
  - Below the message, a receipt says whether it was taken up.
- **Views.** Chat, the flow graph of the outer and inner loops, a timeline to replay the session, and (with the native TUI plugin) the agent's own terminal UI.
- **Plugins.** Built in: the OctoSense app type, app preview and publishing to App Hub, app data, and the native TUI (off by default). External plugins live in `<data>/plugins/<id>/plugin.json`, next to a program that OctoBuddy runs once per call.

## Build and test

In this repository:

```sh
cargo test --locked
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo run            # OctoBuddy in a window of its own, outside OctoSense
```

The live tests are ignored by default. They need the agents' CLIs, their providers in AI providers, and the shell's octos home:

```sh
OCTOS_APP_CORE_DIR=<OctoSense home>/octos-home/.octos OCTOBUDDY_PI_BIN=<pi> \
  cargo test --lib -- --ignored --nocapture <test>
```

| Test | What it shows |
| --- | --- |
| `codex_switches_model`, `pi_switches_model` | GLM, then MiniMax, in one conversation: a code word told to GLM is answered by MiniMax |
| `codex_and_pi_steer_a_turn`, `claude_steers_a_turn` | A turn steered while its command runs takes the steer up in the same turn |
| `codex_and_pi_steer_a_reply` | A steer sent while the agent writes a reply with no tool call is still heard |
| `codex_lead_on_glm`, `pi_lead_on_glm`, `codex_chat_reaches_the_network` | Each agent answers on GLM; Codex in a chat reaches the network |

All of these were run on macOS (Apple silicon) on 2026-10-01 and 2026-10-02, as part of OctoSense, before the move to this repository and the rename. They are **unverified** on Linux and Windows.

### In OctoSense

Until that pull request is merged, the branch above is how to run OctoBuddy in an OctoSense shell. From a checkout of OctoSense on that branch:

```sh
python3 tools/setup.py --update
cargo build --locked --release -p octosense
MAKEPAD_WM_TEST_APP=octobuddy ./target/release/octosense
```

That is how it was run on 2026-10-02 (macOS, Rust 1.97.1; the shell's matrix-sdk needs 1.95 or newer). Cargo fetches OctoBuddy at the pinned revision; a newer one means moving the pin in OctoSense's `native-apps.json` and running `python3 tools/native_apps.py`.

The first time it starts there, OctoSense asks whether OctoBuddy's agent may start (its octos services). OctoBuddy's own engines run either way.

### Its pinned revisions

`Cargo.toml` takes OctoSense and App Hub at the revisions OctoSense's main pins, and Makepad at the revision that App Hub names (`1f3b1de`, nine commits before OctoSense's `e29a0eaa`). Cargo cannot patch a git source with another revision of the same repository, so one Makepad here means App Hub's. Inside OctoSense, its `[patch]` sections resolve all of them to the shell's own checkouts. Move them when OctoSense moves its pins.

Run on its own, the divider beside the sidebar draws as a dark bar; inside OctoSense it is the thin line it should be. **Not yet looked into.**

## Environment

| Variable | Use |
| --- | --- |
| `OCTOBUDDY_HOME` | Its data directory: projects, chats, plugins (default `~/.octobuddy`) |
| `OCTOBUDDY_PI_BIN` | Where pi is, when it is not on `PATH` |
| `OCTOBUDDY_DEBUG_EVENTS=1` | Prints every loop event on stderr |
| `OCTOBUDDY_BRIDGE_LOG=<file>` | Appends each Codex bridge call that failed (its status, request and answer; never the key) |

## Not done yet

- The inner loops cannot run on the system octos yet. App peers lack a working folder, bash, steering and approval scopes; see [docs/upcr-draft-coding-peers.md](docs/upcr-draft-coding-peers.md).
- A UI app cannot render inside octos's macOS sandbox, which grants no `iokit-open`. This needs a GPU switch in octos, which is being proposed there.
- **Unverified:** phones, Linux, Windows.
