# OctoBuddy

English | [简体中文](README.zh-CN.md)

OctoBuddy is OctoSense's native coding app with two loops. An **outer loop** turns a request into a plan of slices and reviews what comes back. **Inner loops** work on the slices in parallel, in the project's folder (or its git worktree). A plain **chat** talks to one agent, with no loops.

It runs inside OctoSense as a native app, and on its own in a window of its own. OctoSense registers it in its `native-apps.json` as the app `octobuddy`, pinned to a revision of this repository, the way it takes Rinx. Desktop shells build it by default; phone shells leave it out. That registration is ready on the branch [`feat/octobuddy-app`](https://github.com/tyreseluo/OctoSense/tree/feat/octobuddy-app) of a fork of OctoSense (it pins a recent revision of `main`). **Pending:** its pull request to OctoSense is not made yet.

It was called OctoLoop until 2026-10-02.

## Download

[Releases](https://github.com/tyreseluo/OctoBuddy/releases) have a DMG for Macs with Apple silicon (`macos-aarch64`), with its `.sha256`; Intel Macs are not supported. It is signed ad hoc, not notarized by Apple yet, so macOS stops OctoBuddy the first time: open it once, then choose **Open Anyway** in System Settings › Privacy & Security, or run `xattr -dr com.apple.quarantine /Applications/OctoBuddy.app`.

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
- **Each slice on its own agent.** Unless you picked the session's inner agent yourself, the outer loop may name each slice's agent (`agent`: octos, codex, pi or claude) and model in its plan.
  - STATUS lists the AGENTS and, for each one, the models it can run on. These follow each endpoint's protocol, as Settings › AI Providers shows them.
  - A slice keeps its agent whatever the session's inner agent becomes.
- **When an agent fails.** A slice whose agent or model fails before doing any work (no file changed) is reported as that. You see a note in the conversation with the error and what else the slice can run on. The outer loop gets the same note and can run the slice again from its brief on another agent or model (`rerun` in octobuddy-send), and it tells you what it switched and why.

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
- **Interrupt & send.** ⇧⌘Enter (⇧Ctrl+Enter elsewhere), or the **Interrupt & send** button shown while a turn is at work and something is typed. The turn and its subagents are cut off, and the message starts the next turn. The inner loops go on. An inner loop's panel has the same button.
- **Views.** Chat, the flow graph of the outer and inner loops, a timeline to replay the session, and (with the native TUI plugin) the agent's own terminal UI.
  - **Live view.** A read-only view of what the outer loop or an inner loop does now: what it writes, each tool call, its subagents. It works as usual; nothing is taken over. Kept in `<data>/live/`. Once its turn is done, **Take over** opens the agent's own terminal on the same conversation.
  - **The timeline.** Each line between the loops joins the turn that sent it to the turn that took it up. Each wait is a dotted line that says why: the agent starting, the previous wave's acceptance, a queue, OctoBuddy's check, you, a restart. The summary adds them up by why, counting time once when several lanes wait together. Work that did not stand is marked: a lane replaced by a later one is hatched as history, a turn done again later is outlined, a ring marks each rework with its likely cause, and the panel under it lists them with the time they cost. **Learn from it** asks the outer loop to keep lessons from them (below).
- **Plugins.** Built in: the OctoSense app type, app preview and publishing to App Hub, app data, the app factory and the production loop (below), and the native TUI (off by default). Each built-in one is a file in `src/plugins/` (the framework is `src/plugins/mod.rs`). External plugins live in `<data>/plugins/<id>/plugin.json`, next to a program that OctoBuddy runs once per call.

## Building an OctoSense app

OctoBuddy gives the agents that build an app what they would otherwise look up or work out again on every run.

- **Two digests**, in every app project's `.octobuddy/docs/`, read first by the outer loop and every inner loop:
  - `SPLASH-COOKBOOK.md`: verified Splash patterns (pages, lists, forms, storage, dates, money, bars), the gotchas and the errors they give, then what OctoBuddy's own runs learned since.
  - `DESIGN-FLOW.md`: OctoScript App Design Flow in one place. It covers the flow's steps and what passes each, where a person must decide (publisher details, keys and signing, a real submission), the design checklist (empty, error and restart states; what each action means), the manifest and listing rules, what each capability gives on today's devices, and the common mistakes. Each fact cites its line in the flow's repo (distilled from `63d3dbda`).
  - The flow's own docs are copied beside them (AGENTS.md, FLOW.md, SCRIPT-API.md, CAPABILITIES.md, HOST-SERVICES.md, PUBLISHING.md), to open at a line a digest cites.
- **A skill.** The same as the skill `octosense-app`, in a plugin OctoBuddy writes to `<data>/agent-plugin/octobuddy/`. Claude Code loads it with `--plugin-dir`, pi with `--skill`.
- **Tools for every agent.** An inner loop's sandbox cannot run the app, so OctoBuddy runs it outside for the agent:
  - `octobuddy_app_look`: script errors, a screenshot, the widgets on screen;
  - `octobuddy_app_drive`: click by id or text, type, press keys, wait, look, so a flow is verified by doing it;
  - `octobuddy_app_probe`: a few lines of Splash run headless in seconds, to try something instead of reading the runtime's source;
  - `octobuddy_check`: the slice's own check.

  Claude Code and Codex reach them over MCP. octos reaches them through a small stdio shim in the inner loop's profile, because octos refuses an MCP server on loopback HTTP. pi, which has no MCP, runs the command `octobuddy-app look | drive <steps.json> | probe <file> | check`. The outer loop has probe and drive too.
- **Parts.** A new project's `bundle/main.splash` is made from `app/parts/*.splash` (shared state, one part per page, the root) before every check, preview and publish. Slices can then build the pages at the same time. A check's errors name the part and its line.
- **Checked before review.** When a slice's check fails, OctoBuddy sends the output back to its inner loop, with hints for errors it knows, up to 2 times before the outer loop reviews it. A turn that ends on a promise ("Now let me…") is nudged once to finish.
- **Learning from rework.** After a round is accepted with time lost to rework, or on **Learn from it**, the outer loop keeps up to 3 new lessons with `octobuddy_learn`. A `splash` lesson goes into every project's cookbook and the skill; an `orchestration` lesson goes into the outer loop's rules. They are kept in `<data>/lessons/`.
- **Verified** on 2026-10-02 with 小账本 (pocket-ledger), built with Claude Code · Opus as the outer loop and codex, octos, pi and Claude Code on MiniMax and GLM as inner loops. It was published to the local App Hub (0.1.1). That run took about 2 hours, before parts, the tools for every agent and the digests; a timed run with them has not been made yet.

## App factory

The plugin `app-factory` builds OctoSense apps asked for from outside OctoBuddy. It follows GOSIM 2026's "software factory" bounty: task progress, agent status, acceptance, handover.

- **Asked.** Through `octobuddy.request {what, for, acceptance, data, name}`, called by the OctoSense system agent or another app's agent granted it. The call is answered at once with the request's id.
- **Waits for you.** The request appears on the Apps page (the pulse button beside Settings, in the accent colour while a request waits), with who asked, what for, when it is done and its data. Nothing is made, and no model is paid for, until you press **Build**. **Decline** turns it down.
- **Built.** Build makes the app project from the design flow's template under `<data>/apps/`, and reads the data API it names. It then opens a new session with the request's brief in its composer. Pick the models for its outer and inner loops in the picker under it, and send; the data's shape goes with the message.
- **Published.** **Publish** on its card publishes it to the local App Hub once its outer loop has stopped. Its live watch then starts.
- **Followed.** `octobuddy.status` lists the requests under `requests`, each with its status: waiting for the person, declined, preparing, building (and whether an outer loop works on it), published (the version), or failed.
- **Kept** in `<data>/requests.json`.
- **Verified** in OctoSense on 2026-10-02 with a Shanghai weather card on Open-Meteo's API:
  - Asked in the system agent's chat, the system agent passed it to OctoBuddy's agent, which called `octobuddy.request` with the acceptance criteria it drew from the fx-board incident.
  - The card showed on the Apps page, with the sidebar button marked.
  - Build made the project, read the API's shape, opened a session and sent the brief to its outer loop. That turn was stopped there, so the app was not built.


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
- **Drill: bad release** breaks the app on purpose and publishes that as its next version, as a change that shipped without its check.
  - The function `main.splash` calls most has its definition renamed, and its calls are left as they were.
  - The change is a commit of its own in the project (`drill: 坏版本演练…`).
  - The incident it causes is marked as this drill. Its brief asks the outer loop to find the cause from what the run showed and fix it, not to revert the whole version.
  - Publish fix ends it once the fixed version runs well.
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

## On the host

As Rinx does, OctoBuddy runs first as an app of its own on this computer, and then inside OctoSense. On the host you can use it to work on OctoSense itself and to make apps for it, with no OctoSense running.

- **Two builds of one view.**
  - The `standalone` feature, the default, is the app on the host: its view in a window of its own.
  - OctoSense builds it with `--no-default-features --features octosense-module` and hosts its module (`OCTOBUDDY_MODULE`, in `src/module.rs`). The window is not compiled in there.
- **Running it.** `cargo run` on macOS starts it as `OctoBuddy.app`, with its name and icon in the Dock. It is a bundle next to the binary, made by `packaging/run-macos.sh`, which `.cargo/config.toml` sets as cargo's runner. Bundle id `org.octosense.octobuddy`; the icons are in `packaging/`.
- **What changes on its own.**
  - There is no system agent. The About page says so, and the picker does not offer OctoSense's agent; octos runs on OctoBuddy's own octos.
  - `octobuddy.status`, `.report` and `.request` are OctoSense's to call, so on the host the Apps page has no requests from outside. Its live watch works as in OctoSense.
  - AI providers: its own, or OctoSense's (below).
  - The agents' programs are its own copies (above), or yours.
- **Its data** is in `~/.octobuddy` (`OCTOBUDDY_HOME` names another folder), the same on the host and in OctoSense.
- **Not done yet:**
  - a release package (a signed `.dmg` with cargo-packager, Linux and Windows installers, a release workflow);
  - octos shipped inside the app.
- **Verified** on macOS (Apple silicon) on 2026-10-02: `cargo run` started `target/debug/OctoBuddy.app/Contents/MacOS/OctoBuddy`, in a window titled OctoBuddy, and its About and AI Providers pages said it runs on its own and named the profile it reads.

### Its AI providers

On the host, Settings › AI Providers is OctoBuddy's own, set up the way Cindy does it. Inside OctoSense the page shows the shell's AI providers, read-only, as before. The code is in `src/own_providers.rs` (the data) and `src/providers_view.rs` (the page).

- **One format.** It keeps them in `<data>/providers/profiles/_main.json`, written with `octosense-llm-config`, the library OctoSense's AI providers app is built on. Its catalog gives the families, models and endpoints. So the proxy, the agents and the inner loops' octos read this profile the same way they read OctoSense's.
- **Where they come from** is yours to pick, under the page's title:
  - **OctoBuddy's own** (the default once it has any);
  - **OctoSense's**: the profile its AI providers app writes, read-only.

  When OctoSense's has providers OctoBuddy's own does not, a banner offers **Import**. It adds them after OctoBuddy's own and copies their keys. It never replaces a provider or a key OctoBuddy already has, and it leaves OctoSense's as they are. `OCTOBUDDY_PROVIDERS=<an octos folder>` still names another profile, read-only.
- **Adding one** is a three-step wizard:
  1. **Provider.** The families in three groups: coding plans first, then more providers, then local servers. Each family says whether it needs a key, how many models it has, and which agents can run on it.
  2. **Connect.** Pick its endpoint and enter its key (masked). A base URL is asked for only when the endpoint is your own. **Get an API key…** opens the provider's console, for the providers whose console is known. **Test connection** sends one request of a single token and shows what came back. The key is never shown, even in an error.
  3. **Models.** The models served on that endpoint. The recommended one is checked; check more to add them as fallbacks.
- **Its rows** are primary first. Each one says which agents run on it, from its endpoint's protocol:
  - Claude Code and pi on an Anthropic-compatible endpoint;
  - Codex where there is Chat Completions, through the bridge;
  - octos on any.

  The buttons are **Make primary**, **Test** and **Remove**. Remove takes a second click, and it deletes the key when no other provider uses it.
- **Keys.**
  - On macOS a key goes to the keychain item octos reads (service `octos`), under OctoBuddy's own account `<KEY_ENV>::octobuddy`, so OctoSense's item for the same provider is never touched. The profile holds only the marker `keychain:<account>`.
  - Elsewhere the key is in the profile itself.
  - The profile is written `0600`.
  - The key reaches the keychain (`security -i`) and the test (`curl -K -`) on their standard input, never on a command line.
  - Agents still get placeholders: OctoBuddy's proxy adds the key upstream. Cindy, by comparison, gives pi the key itself.
- **The agents' own sign-in.** The page shows whether Claude Code (`claude auth status`) and Codex (`codex login status`) are signed in, without saying who. An agent with no provider picked runs on that.
- **Not done yet:** signing in from the page (it says which command to run), a custom endpoint for a family the catalog does not list, and fetching a provider's model list.
- **Verified** on macOS on 2026-10-02 in a hidden window with a data folder of its own and a fake OctoSense profile (`OCTOS_APP_CORE_DIR`):
  - the page read OctoSense's and offered to import it;
  - with OctoBuddy's own selected, the wizard added `zai-coding/glm-5.3` and `glm-5.3-flash`. The profile was `0600` and held only the keychain marker, and the key went to the keychain;
  - a fake key's test showed the provider's `401` with the key masked;
  - Make primary reordered the rows, and Import added the one it lacked;
  - removing every row deleted the keychain items, and OctoSense's profile was unchanged.

## Build and test

In this repository:

```sh
cargo test --locked
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo run            # OctoBuddy in a window of its own, outside OctoSense
```

### Packaging

It is packaged as Robrix is, with cargo-packager and robius-packaging-commands (0.4 or later):

```sh
cargo install cargo-packager robius-packaging-commands --locked
cargo packager --release --formats dmg    # dist/: OctoBuddy.app, then the DMG
```

robius-packaging-commands builds it with Makepad's `apple_bundle` and `MAKEPAD_PACKAGE_DIR` set, so the app reads its fonts and icons from the package, never from the source tree. The package's settings are `[package.metadata.packager]` in `Cargo.toml`.

**A release.** Set the version in `Cargo.toml`, write `docs/RELEASE_NOTES_v<version>.md`, then push the tag `v<version>`. The Release workflow (`.github/workflows/release.yml`) checks that the tag matches the version, makes a draft release, builds the DMG on `macos-15` (Apple silicon) and attaches it with its sum, then publishes the release. No one sees it before its DMG is in it; if a step fails, it stays a draft. Run by hand, the workflow only builds the DMG, as the run's artifact.

### Live tests

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
