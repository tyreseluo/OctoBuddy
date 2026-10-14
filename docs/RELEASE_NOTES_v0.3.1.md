OctoBuddy @VERSION@ shows where the work stands and what it left behind, and sees the disk it fills. A lifecycle view draws each slice's way through its states and the project's documents from the first request on; a disk map shows what fills a project and its agents' worktrees, round after round. Agents get tools for both, and OctoBuddy can report to a hagency room.

## Download

- Macs with Apple silicon (M1 and later): `OctoBuddy-@VERSION@-macos-aarch64.dmg`.
- Intel Macs: `OctoBuddy-@VERSION@-macos-x86_64.dmg` (built on Apple silicon; opened under Rosetta, not yet on an Intel Mac).

Each has its `.sha256` beside it. Open the DMG and drag OctoBuddy to Applications (replace the earlier one).

**The first time you open it.** It is not notarized by Apple yet, so macOS stops it the first time. Open it once, then go to **System Settings › Privacy & Security** and choose **Open Anyway**. Or, in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OctoBuddy.app
```

## New

- **The lifecycle view: state machine and doc stream.**
  - The state machine draws every slice's path: waiting for its wave, running, reported, checked, accepted, landed, and the ways out (sent back, failed, stopped, over budget, discarded). Each node shows how many slices are there now, each edge how often it was taken: rework is the count on *sent back*.
  - The doc stream lines up, round by round, the request, the plan, each slice's brief, its report, OctoBuddy's check, the outer loop's review, the acceptance sheet and the receipts, and the Markdown the agents wrote or changed in the repository, each version of a document as v1, v2… to read side by side.
  - OctoBuddy keeps `<project>/.octobuddy/living/LIVING.md`, the project's story from its first round, up to date in the background.
  - A new agent tool, `octobuddy_docs`, lets the outer and inner loops look up earlier audits, designs and decisions instead of working them out again.
- **The disk map plugin**, on [tobi/disktree](https://github.com/tobi/disktree)'s `disktree-core` (MIT): a treemap of the project (or of the worktrees OctoBuddy made for its inner loops) by kind of data, what can be had back (build output, agent scratch) and what is worth a look. A snapshot is taken after OctoBuddy commits a slice's files (at most every 5 minutes), so it shows how the disk grew round by round. **Ask the outer loop** puts a question about the picked tile in the composer. Three read-only agent tools, `octobuddy_disk_map`, `octobuddy_disk_growth` and `octobuddy_disk_reclaim`: they suggest how to clear space, never delete.
- **Reporting to a hagency room.** A project can be linked to a Matrix room: a sheet made or accepted, a silent loop, a Live incident, a review request and a publish or submission read back are posted once each. A coordinator on the list can reply `accept round N` (`验收 第 N 轮`) to confirm a sheet, the same as the button (checked against the project first). Room text never reaches an agent; the token comes from the keychain or `OCTOBUDDY_MATRIX_TOKEN` only.
- **Error codes from App Hub's gate.** When `hub check` names a stable code and a one-line fix (proposed upstream in [OctoSense-App-Hub#219](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/219)), OctoBuddy hands the agent that fix; older tools fall back to its own hints.
- **A real-agent benchmark** (`bench_real_inner_loops`, ignored by default): real inner loops on three small tasks, then OctoBuddy's own commit, check, claims and gate. Results in `docs/evidence/bench/`.

## Fixed

- A file with unstaged changes listed first by `git status` lost its first letter (`calc.py` read as `alc.py`), so a slice could be accused of changing files outside its card.

## Needs

- macOS 11 or later.
- `git`, to fetch the app templates.
- For pi: Node.js and npm.
- To check and submit an app: App Hub's `hub` and `card-host` built (`OCTOSENSE_APP_HUB`), and GitHub's `gh`, signed in.

## Not yet

- Keeping an inner loop out of other projects' sources: Codex, pi and Claude Code read the whole disk; OctoBuddy asks them not to, and tells them when they go round in circles.
- A claim OctoBuddy has no fact for stays a ?: it does not run commands an inner loop names that are not the slice's check.
- Linux and Windows; a notarized build.

---

## 中文

OctoBuddy @VERSION@ 让工作走到了哪、留下了什么都看得见，也看得见它把磁盘用成了什么样。生命周期视图画出每个切片走过的状态，以及项目从第一条需求起的全部文档；磁盘地图显示项目和 agent 的 worktree 被什么占满、每一轮之后长了多少。两者都给了 agent 工具；OctoBuddy 还能向 hagency 房间汇报。

**下载**：
- Apple 芯片（M1 及以后）的 Mac：`OctoBuddy-@VERSION@-macos-aarch64.dmg`。
- Intel Mac：`OctoBuddy-@VERSION@-macos-x86_64.dmg`（在 Apple 芯片上构建；在 Rosetta 下打开过，还没在 Intel Mac 上跑过）。

打开 DMG，把 OctoBuddy 拖进「应用程序」，替换旧版即可。第一次打开时，到「系统设置 › 隐私与安全性」点「仍要打开」，或者运行上面那条 `xattr` 命令。

**新增**：
- **生命周期视图：状态机与文档流。**
  - 状态机画出每个切片的路径：等前一波、进行中、已报告、已检查、已接受、已合入，以及各种出口（打回修改、失败、停止、超预算、丢弃）。节点上是现在处在这里的切片数，边上是走过的次数：「打回修改」上的数字就是返工次数。
  - 文档流按轮次排出需求、计划、每个切片的任务书、报告、OctoBuddy 的检查、外环审核、验收单和回执，以及 agent 在仓库里写或改的 Markdown；同一份文档的每个版本记为 v1、v2…，可以切换对照。
  - OctoBuddy 在后台维护 `<项目>/.octobuddy/living/LIVING.md`：项目从第一轮到现在的故事。
  - 新的 agent 工具 `octobuddy_docs`：外环和内环先查已有的审计、设计和决定，不用重新推一遍。
- **磁盘地图插件**（基于 [tobi/disktree](https://github.com/tobi/disktree) 的 `disktree-core`，MIT）：把项目（或 OctoBuddy 为内环开的 worktree）画成 treemap，按数据种类着色，标出可回收的空间（构建产物、agent 临时目录）和值得一看的大目录。OctoBuddy 每提交一次切片的改动就拍一次快照（至少间隔 5 分钟），看得到每一轮之后磁盘怎么增长。「问外环」把选中格子的问题填进输入框。三个只读 agent 工具 `octobuddy_disk_map`、`octobuddy_disk_growth`、`octobuddy_disk_reclaim`：只建议怎么清理，从不替人删除。
- **向 hagency 房间汇报**：项目可以关联一个 Matrix 房间。验收单做好或被验收、环无动静、Live 事故、审核请求、发布 / 提交的读回结果各发一次。名单里的协调人回复「验收 第 N 轮」（`accept round N`）即可确认验收单，和按钮一样（确认前先核对项目没变）。房间里的文字不会交给 agent；令牌只从钥匙串或 `OCTOBUDDY_MATRIX_TOKEN` 读取。
- **App Hub 准入检查的错误码**：`hub check` 给出稳定错误码和一行修法时（已向上游提 [OctoSense-App-Hub#219](https://github.com/OctoSense-org/OctoSense-App-Hub/pull/219)），OctoBuddy 直接把修法交给 agent；旧版工具退回原有提示。
- **真实 agent 基准**（`bench_real_inner_loops`，默认不运行）：真实内环做三道小题，再走 OctoBuddy 自己的提交、检查、声明核对和验收闸。结果在 `docs/evidence/bench/`。

**修复**：
- `git status` 里排第一个的未暂存文件会少掉首字母（`calc.py` 读成 `alc.py`），切片可能被误判改了任务卡以外的文件。

**需要**：
- macOS 11 或更高。
- 拉取应用模板需要 `git`。
- 用 pi 需要 Node.js 和 npm。
- 检查和提交应用，需要编好 App Hub 的 `hub` 和 `card-host`（`OCTOSENSE_APP_HUB`），并装好 GitHub 的 `gh` 且已登录。

**还没有**：
- 不让 inner 去读别的项目的源码：Codex、pi 和 Claude Code 能读整个磁盘。目前 OctoBuddy 只能劝阻，并在它反复读同一段时提醒它。
- OctoBuddy 拿不到事实的声明一律是 ?：它不会去跑 inner 提到的、但不是这个切片检查的命令。
- Linux 和 Windows 的版本；经过公证的版本。
