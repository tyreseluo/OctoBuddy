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

The plugin `card-loop` keeps watch over the apps after they are published. It follows GOSIM 2026's "production-loop agent" bounty: read the app's runtime signals, find a failing card, repair it, publish it again.

**The Live page.** The pulse button beside Settings at the bottom of the sidebar opens it; the button turns red while a watched app is not well. The page lists every OctoSense app project, each with:
- its published version and its health;
- a dot for each of its last 12 runs;
- what is wrong now, and what was done about it;
- the buttons Watch, Run now, Drill, Auto repair, Session, Repair and Publish fix.

It watches the version published to the local App Hub, not the project's working copy.

**A run.** Every 5 minutes, a copy of the published version runs headless on live data, as the app check does. The run reads:
- whether the app started;
- its script errors;
- the widgets on screen;
- a screenshot.

The app's storage is kept between runs (`card-loop/state/`), as on a device. A version that keeps its last data can show it when its API is down.

**Judged.** The first healthy run of a version is its baseline. A later run is:
- **down** if it did not start;
- **broken** if it has script errors;
- **degraded** if:
  - 40% of its named widgets are gone; or
  - it shows text in less than half as many widgets; or
  - it shows a failure its baseline did not, unless its content is kept (at least 80% of the widgets with text). An app that says it is offline and shows its last data is coping.

A new version is judged by its content against the last version's baseline until it has a baseline of its own.

**An incident** runs from the run that finds the app ill to the one that finds it well.
- Its session hears both ends.
- **Repair** hands it to that session's outer loop with a brief: what the run showed, the screenshot, and what a drill means. **Auto repair** does that at once.
- **Publish fix** appears once the project's app differs from the published copy. It publishes the fixed version, and the next run checks it.
- If a drill is on, that next run is made without the drill first, as a device online when it updated. The drilled runs that follow decide.
- When it is well again, the session hears how long each step took.

**Kept** in `<project>/.octobuddy/card-loop/`:
- `watch.json`: on or off, pace, baseline, health, the incident;
- `health.jsonl`: a line per run;
- `shots/`: the last 20 screenshots;
- `state/`: the app's storage.

**Drills.** The published copy itself is never changed.
- **Drill** (`offline`) cuts the app's API off in the watch's runs, on a device that used the app before.
- `"drill": "offline-fresh"` in `watch.json` does it on a new device, with nothing kept.
- `"every"` (seconds, 30 or more) sets the pace.

**Verified** on macOS on 2026-10-02, with the published 汇率看板 (fx-board) 0.1.0:
- The live test `probes_a_published_app_well_and_cut_off` (ignored by default):
  - with its API up: healthy (26 widgets, 5.5 s a run);
  - with its API cut off on a device that used it before: healthy, because it keeps its last rates and says so;
  - with its API cut off on a new device: degraded.
- In OctoSense, on the Live page:
  - Watch took the baseline;
  - Drill stayed healthy;
  - the new-device drill opened an incident, reported in the session;
  - Repair sent the brief to the session's outer loop and opened the session. That turn was stopped there.
- **Not run end to end yet:** an outer loop's repair, Publish fix, and the run that closes the incident. fx-board already copes with a drill, so it has nothing to repair.

**With the OctoSense assistant.** OctoBuddy's agent in OctoSense has three tools. The system agent can call them, and so can the agents of apps granted them:
- `octobuddy.status` (read) also returns `live.apps`: each published app's version, whether it is watched, its health, an open incident (what is wrong, whether it went to an outer loop, the fixed version published) and the last report.
- `octobuddy.report {app, problem, from}` (act, confirmed by the host) passes on a problem someone saw, for example "汇率看板打开是空白" from the person, or another app's agent that saw it fail.
  - OctoBuddy runs the published version at once, turning the watch on if it was off.
  - The call is answered with what that run found, or that it is still checking after 20 s.
  - The app's session hears the report and the result. The Live page lists the report.
  - A report the run confirmed joins the incident and the repair brief.
  - One the run did not confirm can still be handed to the outer loop with Repair, since it may happen only on a real device.
- `octobuddy.send` writes to a session's outer loop, as before.

The person still decides the repair and the publish. A report only makes OctoBuddy look.

**Verified** in OctoSense on 2026-10-02, with fx-board. The shell's kernel was octos `ae230ce`, the revision OctoSense pins; an older kernel cannot open an app agent's conversation (octos UPCR-2026-034 `read_parent`).
- **Asked in Ask OctoBuddy** ("汇率看板整个是空白的，帮我让 OctoBuddy 复查一下"): its agent called `octobuddy.report`. The run found the app healthy, and the agent passed that on with what to try next. No sheet was shown for it.
- **Asked in the system agent's chat** ("朋友新装了 OctoSense，打开汇率看板什么都看不到"), with the new-device drill on:
  - the system agent passed it to OctoBuddy's agent with `peer_send_input`;
  - that agent first asked for `terminal.run` to fetch the API, which was denied there;
  - it then called `octobuddy.report`;
  - the run confirmed the problem, and the report joined the incident.

  `octobuddy.report`'s description now says to call it first.

**Not seen by the watch.** Failures only the real host shows, such as a permission denied in OctoSense. The shell does not share installed cards' runtime errors with other apps yet.

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
