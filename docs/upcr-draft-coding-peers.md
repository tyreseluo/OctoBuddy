# Draft: coding peers — an app's inner loops on the system octos

- **Date:** 2026-10-01
- **Status:** Draft for discussion (OctoSense + octos). Nothing here is decided.
- **Relates to:** [ADR 0004](../../../docs/adr/0004-native-apps-hosting-and-peers.md) (native apps, app agents, approvals); octos UPCR-2026-034 (host-owned app peers) and UPCR-2026-035 (host tools); OctoBuddy's `src/system.rs`.

## Where OctoBuddy stands

OctoBuddy is a two-loop coding client: an outer loop (Claude Code) plans and reviews, and inner loops (octos) write code in parallel. Each inner loop is an octos session whose `cwd` is a folder the person chose. Every inner loop uses the same folder. They run their tests with bash, and they are steered and approved from OctoBuddy's own panel.

**Linked today** (this change):
- OctoBuddy declares the four assistant services and two tools in `native-apps.json`: `octobuddy.status` (read) and `octobuddy.send` (act, confirmed by the host).
- With the person's consent, the shell gives it one peer on the system octos, owned by the system agent.
- The system agent sees OctoBuddy in `peer_list`, talks to it with `peer_send_input`, and its model calls the two tools. OctoBuddy answers them from its state.

**Not yet on the system octos:** the inner loops. They still run on OctoBuddy's own `octos serve --stdio`, with the same provider profile (a link to the kernel's `profiles/_main.json`). The system octos cannot host them yet, for the reasons below.

## What an app peer cannot do yet (checked at octos e045c727)

1. **Work in a folder the person picked.**
   - A peer's `cwd` is the app's storage jail (`crates/shell/src/host_tools/mod.rs`, `agent_workspace`). `peer/prepare` takes a `cwd`, but no app-facing API passes one.
   - A context is fenced to `<peer cwd>/contexts/<id>`; anything else is refused with `peer_context_workspace_escape`.
   - Peer workspaces may not nest or overlap. So parallel inner loops can share one repository neither as contexts nor as separate peers.
2. **Run commands.**
   - The kernel's `_main` profile denies `group:runtime` to every session (`crates/kernel/src/system_tools.rs`).
   - `native_apps.py` refuses octos's shell in `agent.generic_tools` (ADR 0004 §12).
   - An inner loop that cannot run its tests or git works blind. OctoBuddy runs checks itself afterwards, but not during the work.
3. **Be steered, or answered.**
   - `ContextOp` has no steer; octos's `turn/steer` exists.
   - `ContextOp::Approval` has no `approval_scope` (octos accepts `request | turn | session | tool`).
   - The shell takes every approval and question (`approval/handled_by_host`), so the app's own panel cannot answer them.
4. **Run long.**
   - A context turn is interrupted after 180 s (`BrokerConfig::turn_timeout`). A coding turn takes minutes.

## Proposal

### octos (a UPCR)

- **`peer/prepare` with a granted folder:** the host passes a `cwd` the person granted, unchanged, even for an app peer.
- **Shared-workspace contexts:** `peer/context/open { …, workspace: "shared" }` opens a context in the peer's own `cwd`. It is not fenced to a subfolder, and several contexts may use it at once. Without the flag, the current behaviour stays.
- **A coding runtime for one peer:** the host can register `bash`/`exec_command` for a peer whose grant allows it. They run in the kernel's sandbox, fenced to that peer's `cwd`, with approvals as today. The `_main` profile's policy stays as it is.
- **Per-context `turn_timeout`:** the host sets it within a kernel maximum.

### app-peers

- **`ContextSpec.workspace: Option<WorkspaceGrant>`:** a token the host minted for a folder grant, never a raw path from the app.
- **`ContextOp::Steer { text }`.**
- **`ContextOp::Approval { id, approve, scope }`.**
- **`ContextOp::Answer { id, answers }`.**
- **Coding approvals go to the app:** the host routes a coding context's approvals and questions to the app that opened it, as ADR 0004 says ("approvals by the person, in that app"), instead of taking them.

### Shell

- **A folder-grant sheet:** "Let OctoBuddy's agent work in `<folder>`?" It mints a `WorkspaceGrant`, which the person can revoke in Settings.
- **A manifest flag:** `agent.coding: true` lets an app ask for grants. `native_apps.py` checks it.
- **Policy:** a coding grant carries the runtime tools and a longer turn timeout.

### OctoBuddy, once this lands

- One peer per project folder (one grant each). Each inner loop is a shared-workspace context of that peer: steerable, with approvals in OctoBuddy's panel.
- OctoBuddy's own `octos serve` goes away, as does its provider-profile link.
- The system agent sees the inner loops' work through the peer.

## Open questions

- **Several projects at once:** one peer per (app, account) today. Would a coding grant add peers per folder, or contexts across grants?
- **Worktrees:** a session worktree lies outside the project folder, and a git worktree's git data lies outside the worktree. Should a grant cover both paths?
- **Budgets:** a coding peer's token budget and rate limits, against the system agent's.
