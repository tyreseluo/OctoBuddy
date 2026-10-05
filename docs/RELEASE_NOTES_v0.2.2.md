OctoBuddy @VERSION@ makes the hand-off between the outer loop and its inner loops short: a task card out, a few lines back, and what an inner loop would otherwise go and read given to it. It also fixes a later wave that never started, and adds a blank app to the new-app dialog.

## Download

For Macs with Apple silicon (M1 and later): `OctoBuddy-@VERSION@-macos-aarch64.dmg`, with its `.sha256` beside it. Open the DMG and drag OctoBuddy to Applications (replace the earlier one).

**The first time you open it.** It is not notarized by Apple yet, so macOS stops it the first time. Open it once, then go to **System Settings › Privacy & Security** and choose **Open Anyway**. Or, in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OctoBuddy.app
```

## New

- **A short hand-off between the loops.**
  - The outer loop writes each slice as a task card: Goal, Files, Facts, Done when (about 15 lines; an agent-spec contract is still taken in a Rust project). It no longer writes out its estimate's table.
  - With its card an inner loop gets a map of its files (what is defined where, by line), the lines the outer loop already read and lists in the slice's `read`, and the project memory that bears on it. Who owns which files OctoBuddy adds from the cards; the fixed rules are about half as long.
  - An inner loop reports in four lines (status, what it ran, what the outer loop must decide, notes). A report over 1,200 characters is cut, with the path of the whole; OctoBuddy adds the files it changed, its check and its commit, and a `[files]` line when it changed files outside its card.
  - A question or a block wakes the outer loop at once, before any check. The plan and send tools answer with what started, what waits for its wave and what was queued.
  - **The project map** (`.octobuddy/knowledge/MAP.md`): the project's files with what is defined where, and the work accepted so far, kept current before each request and after each accept. Both loops start from it instead of searching the project.
  - An inner loop that reads the same lines three times in a turn is told to go on with what it read.
- **A blank app** comes first in the new-app dialog: an empty page and the skeleton every app shares, asking for no permission. It is OctoBuddy's own, so it is there offline too.

## Fixed

- **A wave that never started.** A later wave building on an earlier one's files marked the earlier one's accept as out of date, and the next wave waited for good ("waiting for wave 2 to be accepted"). A later wave of the same plan, and an app's generated `main.splash`, no longer make an accept stale, a stale accept no longer holds the waves after it, and after a restart OctoBuddy looks again at the waves that wait.
- **An inner loop's replies show the model they ran on**: its provider's mark and the model's name ("developer · claude · MiniMax-M3"), as the outer loop's do.
- An octos inner loop with no model named could not open its session.
- Inner loops are **ended**, not "closed"; the counts say queued apart from running.
- A new app's first commit no longer holds OctoBuddy's own `.octobuddy/` files.

## Needs

- A Mac with Apple silicon, macOS 11 or later.
- `git`, to fetch the app templates.
- For pi: Node.js and npm.
- To check and submit an app: App Hub's `hub` and `card-host` built (`OCTOSENSE_APP_HUB`), and GitHub's `gh`, signed in.

## Not yet

- Keeping an inner loop out of other projects' sources: Codex, pi and Claude Code read the whole disk; OctoBuddy asks them not to, and tells them when they go round in circles.
- Intel Macs, Linux and Windows; a notarized build.

---

## 中文

OctoBuddy @VERSION@ 让外环和 inner 之间的交接变短：派出去的是一张任务卡，收回来的是几行汇报，inner 本来要自己去读的东西直接交给它。同时修好了后面的波次一直不开始的问题，新建应用对话框也多了空白应用。

**下载**：适用于 Apple 芯片（M1 及以后）的 Mac，下载 `OctoBuddy-@VERSION@-macos-aarch64.dmg`。打开 DMG，把 OctoBuddy 拖进「应用程序」，替换旧版即可。第一次打开时，到「系统设置 › 隐私与安全性」点「仍要打开」，或者运行上面那条 `xattr` 命令。

**新增**：
- **内外环交接变短**：
  - 外环把每个切片写成任务卡：目标、负责的文件、已知事实、完成条件，大约 15 行（Rust 项目仍可写 agent-spec 合同）。回复里不再贴估算表。
  - inner 拿到任务卡的同时，还会拿到三样东西：它负责的文件里定义了什么、在哪一行；外环在切片的 `read` 里交接的已读片段；和它有关的项目记忆。谁负责哪些文件由 OctoBuddy 从任务卡生成，固定规则也缩短了一半左右。
  - inner 的汇报改成四行：状态、跑了什么、需要外环决定什么、备注。超过 1,200 字会截断，并附上全文路径。改了哪些文件、检查结果和提交由 OctoBuddy 自己附上；改到任务卡以外的文件时，会多附一行 `[files]`。
  - inner 提问或被卡住时立刻叫醒外环，不再等检查。派发工具会明确告诉外环：哪些已经开始、哪些在等自己的波次、哪些消息进了队列。
  - **项目地图**（`.octobuddy/knowledge/MAP.md`）：列出项目里的文件、每个文件定义了什么、在哪一行，以及已验收的工作。每次请求发给外环之前、每次验收之后都会更新，外环和 inner 都先读它，不用再去翻整个项目。
  - inner 一轮里把同一段读了 3 次时，OctoBuddy 会提醒它用已经读到的内容接着做。
- **新建应用可以选空白应用**：它排在第一个，是一个空页面，加上每个应用都有的骨架，不申请任何权限。它由 OctoBuddy 内置生成，离线也能建。

**修复**：
- **后面的波次一直不开始**：后一波按设计会在前一波的文件上接着做，但这会把前一波的验收标成「已过期」，下一波就一直显示「等待 wave 2 被接受」。现在：
  - 同一个计划里后面的波次改动前面波次的文件，不再算过期；应用由 parts 生成的 `main.splash` 变化也不算；
  - 过期的验收不再挡住后面的波次；
  - OctoBuddy 重启后，会重新检查还在等待的波次。
- **inner 的回复显示实际用的模型**：和外环一样，显示供应商图标和模型名，比如「developer · claude · MiniMax-M3」。
- 修复了没指定模型的 octos inner 打不开会话的问题。
- inner 的「关闭」改叫「结束」；计数里「排队中」和「运行中」分开显示。
- 新应用的第一次提交不再带上 OctoBuddy 自己的 `.octobuddy/` 文件。

**需要**：
- Apple 芯片的 Mac，macOS 11 或更高。
- 拉取应用模板需要 `git`。
- 用 pi 需要 Node.js 和 npm。
- 检查和提交应用，需要编好 App Hub 的 `hub` 和 `card-host`（`OCTOSENSE_APP_HUB`），并装好 GitHub 的 `gh` 且已登录。

**还没有**：
- 不让 inner 去读别的项目的源码：Codex、pi 和 Claude Code 能读整个磁盘。目前 OctoBuddy 只能劝阻，并在它反复读同一段时提醒它。
- Intel Mac、Linux 和 Windows 的版本；经过公证的版本。
