OctoBuddy @VERSION@ puts the lifecycle under the flow graph, makes the disk map draw and answer the pointer properly, and builds on OctoSense's current main.

## Download

- Macs with Apple silicon (M1 and later): `OctoBuddy-@VERSION@-macos-aarch64.dmg`.
- Intel Macs: `OctoBuddy-@VERSION@-macos-x86_64.dmg` (built on Apple silicon; opened under Rosetta, not yet on an Intel Mac).

Each has its `.sha256` beside it. Open the DMG and drag OctoBuddy to Applications (replace the earlier one).

**The first time you open it.** It is not notarized by Apple yet, so macOS stops it the first time. Open it once, then go to **System Settings › Privacy & Security** and choose **Open Anyway**. Or, in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OctoBuddy.app
```

## Changed

- **The lifecycle panel lives in the Flow view**, under the graph, instead of a view of its own. Drag the line between them for more room (double-click for the default) or fold the panel to its title row. They follow each other: open an inner loop's card and the state machine lights its slice's path; click a slice in the panel and its card lights on the graph.
- **OctoSense** is pinned at its current main (`ffd5e488`); Makepad (`d653bc4`) and App Hub (`3863591`) stay where OctoSense's main pins them.

## Fixed

- **The disk map showed names but no tiles**, and a tile under the pointer was not the one picked when the window was scrolled: the treemap is drawn as plain quads, shaded by depth with each directory's title strip darker, names cut to fit; it is laid out again when its size changes; the pointer is taken in the canvas's own coordinates.
- **Worktrees could not be picked** on the disk map: their folder is found by the project's id (it was looked for by its path and folder name). An inner loop's disk tools scan the project, not its own worktree.
- **The growth line's time** read as a day count since 1970; it says how long ago.
- **The state machine's shapes slid away from their labels** once under the graph; they are drawn as quads too. A pan or zoom is kept when the same slices are passed again, and a click is taken where it shows.
- A test that every widget the code looks up is in the DSL misread names ending in a digit.

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

OctoBuddy @VERSION@ 把生命周期放到了流程图下面，修好了磁盘地图的绘制和指针命中，并跟上 OctoSense 当前的 main。

**下载**：
- Apple 芯片（M1 及以后）的 Mac：`OctoBuddy-@VERSION@-macos-aarch64.dmg`。
- Intel Mac：`OctoBuddy-@VERSION@-macos-x86_64.dmg`（在 Apple 芯片上构建；在 Rosetta 下打开过，还没在 Intel Mac 上跑过）。

打开 DMG，把 OctoBuddy 拖进「应用程序」，替换旧版即可。第一次打开时，到「系统设置 › 隐私与安全性」点「仍要打开」，或者运行上面那条 `xattr` 命令。

**变更**：
- **生命周期面板放进流程图页**：在流程图下方，不再单独占一个视图。拖动两块之间的分隔线可以调整高度（双击恢复默认），也可以把面板收起成一行标题。两边互相联动：打开内环卡片，状态机高亮这个切片走过的路径；在面板里单击切片，流程图里它的卡片会亮起来。
- **OctoSense** 改钉它当前的 main（`ffd5e488`）；makepad（`d653bc4`）和 App Hub（`3863591`）与 OctoSense main 所钉一致，不变。

**修复**：
- **磁盘地图只有名字、没有色块**，窗口滚动后指针下的格子也不是被选中的那个：treemap 改用普通色块绘制，按层级区分明暗、目录标题条更暗，名字按格宽截断；尺寸变化时重新排布；指针按画布自己的坐标换算。
- **磁盘地图点不了「工作树」**：worktree 目录改按项目 id 查找（原来按路径和文件夹名找）。内环调用磁盘工具时扫整个项目，而不是它自己的 worktree。
- **「增长」里的时间**显示成自 1970 年起的天数，改为「几分钟前」。
- **状态机的图形放进流程图页后和文字错开**：同样改用色块绘制。同一组切片再次刷新时保留拖动和缩放，点击按显示位置换算。
- 检查「代码用到的每个控件都在界面定义里」的测试，把名字以数字结尾的控件误判为缺失。

**需要**：
- macOS 11 或更高。
- 拉取应用模板需要 `git`。
- 用 pi 需要 Node.js 和 npm。
- 检查和提交应用，需要编好 App Hub 的 `hub` 和 `card-host`（`OCTOSENSE_APP_HUB`），并装好 GitHub 的 `gh` 且已登录。

**还没有**：
- 不让 inner 去读别的项目的源码：Codex、pi 和 Claude Code 能读整个磁盘。目前 OctoBuddy 只能劝阻，并在它反复读同一段时提醒它。
- OctoBuddy 拿不到事实的声明一律是 ?：它不会去跑 inner 提到的、但不是这个切片检查的命令。
- Linux 和 Windows 的版本；经过公证的版本。
