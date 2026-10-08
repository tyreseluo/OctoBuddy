# OctoBuddy — Privacy

OctoBuddy runs on your Mac (or Linux) and inside OctoSense. It does not call home to a service of its own. Every byte it sends goes to a destination you chose — a model provider, the npm registry, GitHub, App Hub — and every thing it keeps on disk is in a folder you can delete.

## What OctoBuddy keeps on this machine

OctoBuddy has one data directory, called `<data>` below.

- **Default:** `~/.octobuddy` — `src/model.rs:790-796` (`fn data_dir`).
- **Override:** set `OCTOBUDDY_HOME` to any path.

Everything below lives under `<data>`, except where noted.

### The app's own state

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/state.json` | Projects, sessions, every message (user, lead, peer, system), the queue waiting for a restart | `src/model.rs:798-800`, `src/persist.rs:32-64` |
| `<data>/state.json.corrupt-<secs>` | A copy of `state.json` that did not parse; the file is renamed and a fresh empty store is started so nothing is overwritten | `src/model.rs:741-756` |
| `<data>/appearance.json` | The theme choice (Light, Dark, GitHub Light, …, "Follow OctoSense") | `src/theme.rs:265-266` |

`state.json` is written through a `.json.tmp` sibling and renamed, so a crash never leaves half a file (`src/model.rs:758-766`).

### Live loop logs

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/live/s-<session>.log` | One per outer loop: every turn, tool call, status, in plain text (ANSI colors) | `src/live.rs:30-46`, `src/live.rs:198-273` |
| `<data>/live/p-<peer>.log` | One per inner loop (same shape) | `src/live.rs:43-46` |
| Truncation | A log over 4 MiB keeps its last quarter | `src/live.rs:18-19`, `src/live.rs:96-104` |

No key or token reaches a loop's events (`src/live.rs:11-12`).

### The agents' copies

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/agents/<name>/<version>/` | Claude Code, Codex, pi, octos at the version OctoBuddy was tested with | `src/agents.rs:123-125` |
| `<data>/agents.json` | Per agent: OctoBuddy's copy (default) or yours (the first one on `PATH`) | `src/agents.rs:153-177` |

Downloads come from the **npm registry** for Claude Code, Codex and pi (`src/agents.rs:46-72`) and from **GitHub releases** for octos (`src/agents.rs:87-94`). Every tarball is checked against its pinned `sha512-` (npm) or `sha256:` (GitHub) digest before its folder appears (`src/agents.rs:209-224`, `src/agents.rs:245-275`). pi's `npm ci` runs with `--ignore-scripts` (`src/agents.rs:265-266`); no other install script runs.

### AI providers (host build only)

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/providers/profiles/_main.json` | The profile OctoBuddy's Settings › AI Providers writes; same format OctoSense uses | `src/own_providers.rs:5-7`, `src/own_providers.rs:27-33` |
| `<data>/providers/profiles/_main.json` permissions | Written `0600` | `src/own_providers.rs:79-83` |

Inside OctoSense the page reads the shell's own profile, not this one (`src/providers.rs:222-232`, `src/providers.rs:506-507`).

### Plugins

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/plugins/<id>/plugin.json` | An external plugin's manifest | `src/plugins/mod.rs:19-23`, `src/plugins/mod.rs:187-189`, `src/plugins/mod.rs:239-251` |
| `<data>/plugins/<id>/<command>` | The program OctoBuddy runs once per call (`tool <name>` / `action <id>`) | `src/plugins/mod.rs:316-358` |

Built-in plugins (OctoSense apps, app data, native TUI, app factory, production loop) live in `src/plugins/` and have nothing on disk (`src/plugins/mod.rs:11-18`, `src/plugins/mod.rs:133-184`).

### Templates and the design flow

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/templates/octosense-app-templates/` | A git clone of the templates repo, brought up to `HEAD` each time the new-app dialog opens | `src/plugins/new_app.rs:20`, `src/plugins/new_app.rs:55-89` |
| `<data>/design-flow/<commit>/` | OctoScript App Design Flow (docs, `tools/octo`, script-app template), written out once from `resources/design-flow/` so a machine with OctoBuddy alone can still plan an app | `src/plugins/design_flow.rs:9-57` |

`OCTOBUDDY_TEMPLATES_REPO` names another repository for the templates fetch (`src/plugins/new_app.rs:55-57`).

### Lessons

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/lessons/splash.md` | Splash / app-runtime rules the outer loop learned from a run, capped at 80 lines | `src/lessons.rs:11-68` |
| `<data>/lessons/orchestration.md` | Same for planning, splitting and picking agents | `src/lessons.rs:11-68` |

A lesson is a short rule, the evidence (what showed it) and the day and project wing it came from; the outer loop keeps up to 3 new ones after a round (`src/lessons.rs:14`, `src/lessons.rs:37-69`).

### App factory (OctoSense only)

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/requests.json` | The requests waiting for you, "what" and "for whom", with their status (`working → declined → preparing → building → published → failed`) | `src/plugins/app_factory.rs:9-15`, `src/plugins/app_factory.rs:119-129` |

### Publisher key (host build, App Hub submission)

| Path | What it is | Code |
| --- | --- | --- |
| `<data>/publisher/<id>.key` | The App Hub publisher key, made once the first time you tick **Sign it** for a submission. It stays in `<data>`, never in the project | `src/plugins/app_hub.rs:188-190`, `README.md:96` |

## What OctoBuddy keeps in each project (`.octobuddy/`)

| Path | What it is | Code |
| --- | --- | --- |
| `<project>/.octobuddy/knowledge/MAP.md` | The project's file map (what is defined where, by line) and the work accepted so far, refreshed as the outer loop runs | `src/handoff.rs:40`, `src/handoff.rs:304-316` |
| `<project>/.octobuddy/docs/` | The two digests (`SPLASH-COOKBOOK.md`, `DESIGN-FLOW.md`) and the design flow's own docs | `src/pack.rs:89` |
| `<project>/.octobuddy/context/<session>/` | Per-session pack and review files for inner loops | `src/pack.rs:1-9`, `src/review.rs:24` |
| `<project>/.octobuddy/memory/<stamp>-<room>.md` | Per-project memory notes, ingested by `mempal` | `src/memory.rs:43-58` |
| `<project>/.octobuddy/card-loop/` | The production loop's watch (`watch.json`), per-run health (`health.jsonl`), screenshots and the app's storage | `src/plugins/card_loop.rs:13`, `src/plugins/card_loop.rs:444`, `README.md:174-179` |

The `.octobuddy/` folder is excluded from the project's `git add` (`src/plugins/new_app.rs:289`, `src/plugins/new_app.rs:563`).

## Keys

### macOS: keychain

OctoBuddy keeps AI-provider keys in the macOS keychain, not in its profile:

- Service `octos`, account `<KEY_ENV>::octobuddy` (so OctoSense's item for the same provider is never touched) — `src/own_providers.rs:9-13`, `src/own_providers.rs:23-24`.
- The profile holds only `keychain:<account>` as a marker (`src/own_providers.rs:42-48`, `src/providers.rs:107-117`).
- The key reaches the keychain through `security -i` on its standard input, never on a command line (`src/own_providers.rs:51-67`).
- Removing the last provider for a `KEY_ENV` deletes the keychain item too (`src/own_providers.rs:70-73`, `src/own_providers.rs:118-122`).

### Elsewhere: the profile

Outside macOS the key lives in the profile itself (`src/own_providers.rs:42-45`); the profile is written `0600` (`src/own_providers.rs:79-83`).

### Tests with a key

The wizard's "Test connection" sends a one-token request through `curl -K -` with the key on stdin (`src/own_providers.rs:222-251`). On any error response the message shown has the key replaced by `•••` (`src/own_providers.rs:262-267`).

### Children never see a key

A child (`claude`, `codex`, `pi`, `octos`) talks to OctoBuddy's **own** loopback proxy on `127.0.0.1:<port>` (`src/claude_proxy.rs:29-43`). The proxy:

- Sets `ANTHROPIC_API_KEY` to a placeholder (`octobuddy-proxy`) and points `ANTHROPIC_BASE_URL` at itself (`src/claude_proxy.rs:20`, `src/claude_proxy.rs:71-91`).
- Sets `DISABLE_TELEMETRY=1`, `DISABLE_ERROR_REPORTING=1`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` on the child (`src/claude_proxy.rs:87-89`).
- **Scrubs** the parent's `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CONFIG_DIR`, `ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY` before launch (`src/claude_proxy.rs:97-99`).
- Forwards each request with `curl -K -`; the real key, the body file (`0600`) and the headers go on stdin, never in argv (`src/claude_proxy.rs:160-185`, `src/claude_proxy.rs:150-159`).
- For Codex on a Chat Completions provider, translates Responses → Chat Completions and back (`src/claude_proxy.rs:268-357`, `src/responses_bridge.rs`).

`OCTOBUDDY_BRIDGE_LOG=<file>` appends what was sent and what came back, with the key never included (`src/claude_proxy.rs:325-328`).

A test asserts that the real key never appears in the child's stdout or stderr (`src/claude_proxy.rs:393-418`).

## Where OctoBuddy makes network calls

Only destinations the person picked. There is no OctoBuddy-owned endpoint.

| Host | When | Code |
| --- | --- | --- |
| A model provider you enabled (Anthropic, OpenAI, MiniMax, DeepSeek, Moonshot, Z.ai / GLM, Zhipu, Alibaba DashScope / Qwen, or a self-host) | Every chat, every outer or inner loop turn | `src/providers.rs:65-104`, `src/own_providers.rs:18-19`, `src/claude_proxy.rs:160-170` |
| `registry.npmjs.org` | The first time Claude Code, Codex or pi is needed, to fetch the pinned tarball | `src/agents.rs:46-72`, `src/agents.rs:245-256` |
| `github.com/octos-org/octos/releases/download/...` | The first time octos is needed (Apple silicon and Linux) | `src/agents.rs:87-94` |
| `<data>/templates/octosense-app-templates.git` (`github.com/tyreseluo/octosense-app-templates` by default, or `$OCTOBUDDY_TEMPLATES_REPO`) | Each time the new-app dialog opens | `src/plugins/new_app.rs:20`, `src/plugins/new_app.rs:55-89` |
| `github.com/<you>/<app>` (the project's public repo) and `github.com/OctoSense-org/OctoSense-App-Hub` (`$OCTOBUDDY_APP_HUB_REPO` to override) | App Hub submission: `git push` of `bundle/` at tag `v<version>`, plus the `Submit <id> <version>` issue | `src/plugins/app_hub.rs:23`, `src/plugins/workbench.rs:342`, `README.md:96-98` |
| The hosts the new app's manifest lists (each app's own network calls) | Once the app runs | The app's `bundle/manifest.json` |

App Hub's `hub` and `card-host` are external tools OctoBuddy calls (`OCTOSENSE_APP_HUB`), never its own servers (`src/plugins/app_hub.rs:195-198`, `src/plugins/octosense_app.rs:391-404`).

## Telemetry, analytics, crash reports

OctoBuddy sends **none**. There is no Sentry, Bugsnag, Datadog, OpenTelemetry, Mixpanel or first-party usage endpoint in the codebase (`grep -rn "telemetry\|analytics\|crash\|sentry\|bugsnag\|datadog\|opentelemetry" src/` returns two unrelated hits: a comment that OctoBuddy asks its children not to send telemetry (`src/lead.rs:324`), and a "crash" comment about how `state.json` is saved atomically (`src/model.rs:758`)).

`DISABLE_TELEMETRY`, `DISABLE_ERROR_REPORTING` and `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` are set on Claude Code children (`src/claude_proxy.rs:87-89`). Claude Code runs with `DISABLE_AUTOUPDATER=1` (`README.md:52`).

The agents themselves are other vendors' programs. OctoBuddy sets the variables above for Claude Code (`src/lead.rs:324-326`), and on a provider of yours their model traffic goes through its proxy; beyond that, what Claude Code (on your own login), Codex, pi and octos send is governed by their own settings and policies, not by OctoBuddy.

## OctoSense (the `octosense-module` build)

When OctoBuddy runs inside OctoSense:

- It is a registered OctoSense module — `src/module.rs:11-49`.
- Its data directory is the same `~/.octobuddy` (or `$OCTOBUDDY_HOME`) — `README.md:248`.
- It reads the shell's AI providers, not its own (`src/providers.rs:222-232`).
- On OctoSense, AI providers' profile is in the shell's kernel core dir (`<OCTOSENSE_HOME>/octos-home/.octos` on a desktop; `src/providers.rs:217-224`). Its keys are keychain markers whose accounts are the shell's (`<ENV>::<profile id>`, octosense-llm-config's `profile.rs`), not OctoBuddy's `<KEY_ENV>::octobuddy` (`src/own_providers.rs:11-13`).
- The shell's system agent, the app factory (`octobuddy.request`), and the production loop (`octobuddy.report`) are only registered inside OctoSense (`src/plugins/mod.rs:72-78`, `src/plugins/app_factory.rs:9-15`, `src/plugins/card_loop.rs:1-23`).
- The shell's own system-agent tool policy stays in the shell's profile; OctoBuddy copies the profile for `octos serve` and strips that block — `src/providers.rs:412-421`.

The shell's own AI providers app is the source of truth on OctoSense; OctoBuddy's own providers page is read-only there and the keys are never imported (`src/providers.rs:1-13`, `src/providers_view.rs:252-254`).

## Deleting OctoBuddy's data

- **Everything OctoBuddy keeps for itself:** delete the `<data>` directory (default `~/.octobuddy`, or what `$OCTOBUDDY_HOME` points to).
- **Project-side data:** delete each project's `.octobuddy/` folder.
- **macOS keychain items** OctoBuddy wrote (`<KEY_ENV>::octobuddy`): open Settings › AI Providers and **Remove** the last row that uses a `KEY_ENV` (it deletes the keychain item too — `src/own_providers.rs:70-73`, `src/own_providers.rs:118-122`); or remove them by hand with `security delete-generic-password -s octos -a <account>`.
- **Inside OctoSense:** the keychain items are the shell's, not OctoBuddy's, and stay until the shell pulls them.
- **App Hub publisher key** (host, App Hub only): delete `<data>/publisher/<id>.key`.
- **Uninstall:** `rm -rf /Applications/OctoBuddy.app`, then delete `<data>` and project `.octobuddy/` folders as above.