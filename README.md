# OctoBuddy

English | [简体中文](README.zh-CN.md)

OctoBuddy is OctoSense's native coding app with two loops. An **outer loop** turns a request into a plan of slices and reviews what comes back. **Inner loops** work on the slices in parallel, in the project's folder (or its git worktree). A plain **chat** talks to one agent, with no loops.

It runs inside OctoSense as a native app, and on its own in a window of its own. OctoSense registers it in its `native-apps.json` as the app `octobuddy`, pinned to a revision of this repository, the way it takes Rinx. Desktop shells build it by default; phone shells leave it out. That registration is ready on the branch [`feat/octobuddy-app`](https://github.com/tyreseluo/OctoSense/tree/feat/octobuddy-app) of a fork of OctoSense (it pins a recent revision of `main`). **Pending:** its pull request to OctoSense is not made yet.

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

## Its own agents

Like Cindy, OctoBuddy keeps its own copy of each agent's program, at the version it was tested with. The protocols it drives change between releases, so the same OctoBuddy behaves the same on every machine.

| Program | Version | Where it comes from | Checked against |
| --- | --- | --- | --- |
| Claude Code | 2.1.286 | npm, `@anthropic-ai/claude-code-<platform>` | npm's sha512 integrity |
| Codex | 0.152.0 | npm, `@openai/codex@0.152.0-<platform>` | npm's sha512 integrity |
| pi | 0.99.2 | npm, `npm ci` from `resources/agents/pi/package-lock.json` | npm's integrity of every package |
| octos | 2.0.3-rc.12 | the bundle of octos's GitHub release | GitHub's sha256 digest |

- **When.** A copy is installed the first time it is needed. If a loop or a chat cannot start because the program is not on this machine, OctoBuddy starts installing it and says so in the conversation; send again once it is ready. Settings › Tools can also install one ahead of time.
- **Where.** In `<data>/agents/<name>/<version>/`. A version's folder appears only after its download matched the pinned digest and was unpacked. No install script runs (pi installs with `--ignore-scripts`).
- **Yours instead.** Each agent's card in Settings › Tools chooses **OctoBuddy's** (the default) or **Yours** (the first one on `PATH`). The choice is kept in `<data>/agents.json`. Until OctoBuddy's copy is installed, yours runs. A change applies to the agents started after it.
- **Logins.** They stay yours: OctoBuddy's Claude Code still reads `~/.claude`. It runs with `DISABLE_AUTOUPDATER=1`, so it stays at its version.
- **Not covered.**
  - pi needs Node.js and npm.
  - That octos release has no Intel Mac bundle, so on an Intel Mac yours runs.
  - There are no copies for Windows.
- **Size.** About 900 MB for all four on Apple silicon.
- **Moving a version.** Change its pin in `src/agents.rs` (for pi, also `resources/agents/pi/`).
- **Verified** on macOS (Apple silicon) on 2026-10-02. `installs_every_agent_this_machine_has_a_copy_of` (ignored by default) downloads all four into a scratch `OCTOBUDDY_HOME`, installs each one and checks its `--version`. In OctoSense, a chat on pi, on a machine with no pi, installed it at its first message and answered at the next; chats on Codex and Claude Code ran on OctoBuddy's copies (Claude Code on the person's own login). The inner loops on OctoBuddy's octos were not run yet. **Unverified** on Linux.

## In a conversation

- **Stop.** The Stop button, or Esc, interrupts the turn. If it has not stopped after 5 seconds, its process group is ended. What it wrote so far is kept.
- **Go on.** After a stop, the queue waits. **Go on** sends what is queued, or asks the agent to continue from where it was cut off.
- **Steer.** ⌘Enter (Ctrl+Enter elsewhere) puts a message into the running turn without cutting it off. A queued message has a **Steer in** button that does the same.
  - The agent takes it up at its next step. If it arrives just after the turn ends, it goes first in the queue instead.
  - Below the message, a receipt says whether it was taken up.
- **Views.** Chat, the flow graph of the outer and inner loops, a timeline to replay the session, and (with the native TUI plugin) the agent's own terminal UI.
- **Plugins.** Built in: the OctoSense app type, app preview and publishing to App Hub, app data, the production loop (below), and the native TUI (off by default). Each built-in one is a file in `src/plugins/` (the framework is `src/plugins/mod.rs`). External plugins live in `<data>/plugins/<id>/plugin.json`, next to a program that OctoBuddy runs once per call.

## Production loop

The plugin `card-loop` keeps watch over an app after it is published: GOSIM 2026's "production-loop agent" bounty (read its runtime signals, find a failing card, repair it, publish again). The watch part runs now; repairing and publishing again come next.

- **On.** The **Live** pill of an app project turns it on. It needs the app on the local App Hub, because it watches the version published there, not the project's working copy.
- **A run.** Every 5 minutes it runs a copy of the published version headless, on live data, as the app check does. It reads whether the app started, its script errors, the widgets on screen and a screenshot.
- **Judged.** The first healthy run of a version is its baseline. A later run is:
  - **down** if it did not start;
  - **broken** if it has script errors;
  - **degraded** if 40% of its named widgets are gone, it shows text in less than half as many widgets, or it shows a failure it did not show in its baseline ("无法获取汇率…").
- **Said.** A change of health is a message in the session the watch was turned on from, with the reasons and the screenshot. The pill says how it is (green, red) and when it last ran.
- **Kept.** In `<project>/.octobuddy/card-loop/`: `watch.json` (on, pace, baseline, health), `health.jsonl` (one line per run), `shots/` (the last 20).
- **Drill.** `"drill": "offline"` in `watch.json` runs the copy with its network hosts swapped for one that never answers, as if its API were down. `"every"` (seconds, 30 or more) sets the pace. Both apply from the next run. The published copy itself is never changed.
- **Verified** on macOS on 2026-10-02 with the published 汇率看板 (fx-board) 0.1.0. The live test `probes_a_published_app_well_and_cut_off` (ignored by default) found it healthy (25 widgets, 5.7 s a run) and degraded with its API cut off. In OctoSense, the watch was turned on and took its baseline. The offline drill was reported 29 s later with the pill red, and the recovery 30 s after that. The watch was then turned off.
- **Not seen by it.** Failures only the real host shows, such as a permission denied in OctoSense. The shell does not share installed cards' runtime errors with other apps yet.

## Appearance

Settings › Appearance picks its look. **Follow OctoSense** (the default) is OctoBuddy's light theme, or its dark one while OctoSense's style is dark; it changes as soon as OctoSense does. The other choices are themes of its own: Light, Dark, GitHub Light, GitHub Dark, Atom One Light, Atom One Dark, Dracula, Nord, Solarized Light and Solarized Dark. A theme takes effect at once, without a restart.

- **Where it is kept:** the choice is in `<data>/appearance.json`.
- **How a theme is made:** each one names eleven colours in `src/theme.rs` (page, sidebar, panel, text, its quieter shade, lines, accent, success, danger, warning, purple). The other tokens are mixed from them, so adding a theme is adding eleven colours.

## Build and test

In this repository:

```sh
cargo test --locked
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo run            # OctoBuddy in a window of its own, outside OctoSense
```

The live tests are ignored by default. They need the agents' programs (OctoBuddy's copies or yours), their providers in AI providers, and the shell's octos home:

```sh
OCTOS_APP_CORE_DIR=<OctoSense home>/octos-home/.octos \
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
| `OCTOBUDDY_<NAME>_BIN` | The program to run for `claude`, `codex`, `pi`, `octos` (and the other tools), over OctoBuddy's copy and yours |
| `OCTOBUDDY_DEBUG_EVENTS=1` | Prints every loop event on stderr |
| `OCTOBUDDY_BRIDGE_LOG=<file>` | Appends each Codex bridge call that failed (its status, request and answer; never the key) |

## Not done yet

- The inner loops cannot run on the system octos yet. App peers lack a working folder, bash, steering and approval scopes; see [docs/upcr-draft-coding-peers.md](docs/upcr-draft-coding-peers.md).
- A UI app cannot render inside octos's macOS sandbox, which grants no `iokit-open`. This needs a GPU switch in octos, which is being proposed there.
- **Unverified:** phones, Linux, Windows.
