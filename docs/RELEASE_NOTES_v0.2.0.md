OctoBuddy @VERSION@ brings the app workbench: everything about an OctoSense app in one place beside the conversation, and a way to submit it to App Hub from this computer. It also makes the host edition its own: nothing that only OctoSense has shows here.

## Download

For Macs with Apple silicon (M1 and later): `OctoBuddy-@VERSION@-macos-aarch64.dmg`, with its `.sha256` beside it. Open the DMG and drag OctoBuddy to Applications (replace the earlier one).

**The first time you open it.** It is not notarized by Apple yet, so macOS stops it the first time. Open it once, then go to **System Settings › Privacy & Security** and choose **Open Anyway**. Or, in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OctoBuddy.app
```

## New

- **The app workbench.** In an OctoSense app project, **App · workbench** opens it beside the conversation, with four tabs:
  - **Preview**: the app running from its files, reloaded as they change.
  - **Files**: the project's folder as a tree, and the source of the file picked.
  - **Permissions**: the capabilities a store app can use today, each with the line the store shows; the hosts it reaches; its storage limit. Saved into the manifest.
  - **App Hub**: what the store shows and who publishes it; **Screenshots & check** (a real headless run, App Hub's gate, the review packet); the review questions, answered by you or drafted by the outer loop; **Submit for review…**, which first lists every step and the issue it opens, then, when you confirm, signs it if asked, commits and tags it, pushes it to its repository and opens the `Submit <id> <version>` issue in OctoSense-org/OctoSense-App-Hub.
- **One plugin for apps.** Preview and publishing are part of the **OctoSense apps** plugin now.
- **OctoScript App Design Flow comes with OctoBuddy:** its docs, `tools/octo` and its template, so a machine with OctoBuddy alone can make and plan an app.

## Changed

- **The host edition shows only what it has.** AI providers are OctoBuddy's own (set up as in Cindy): no OctoSense source, no import. The app factory and the production loop, which serve OctoSense, are not listed. Appearance's first choice follows macOS's light or dark.

## Needs

- A Mac with Apple silicon, macOS 11 or later.
- For pi: Node.js and npm.
- To check and submit an app: App Hub's `hub` and `card-host` built (`OCTOSENSE_APP_HUB`), and GitHub's `gh`, signed in.

## Not yet

- A real App Hub submission made with it (checked up to the list of steps, on a copy).
- Intel Macs, Linux and Windows; a notarized build.

---

## 中文

OctoBuddy @VERSION@ 带来了**应用工作台**：在对话旁边集中处理一个 OctoSense 应用的所有事情，并且可以在这台电脑上直接提交到 App Hub。宿主机版也独立出来了：只有 OctoSense 才有的东西不再显示。

**下载**：适用于 Apple 芯片（M1 及以后）的 Mac，下载 `OctoBuddy-@VERSION@-macos-aarch64.dmg`。打开 DMG，把 OctoBuddy 拖进「应用程序」，替换旧版即可。第一次打开时，到「系统设置 › 隐私与安全性」点「仍要打开」，或者运行上面那条 `xattr` 命令。

**新增**：
- **应用工作台**：在 OctoSense 应用项目里点「应用 · 工作台」，在对话旁边打开，有四个标签：
  - **预览**：从项目文件运行应用，文件一改就重新加载。
  - **文件**：项目文件夹的目录树，选中文件后显示源码。
  - **权限**：列出商店应用目前能用的能力，每项附商店里显示的说明；还可以填可访问的域名和存储上限，保存进 manifest。
  - **上架**：
    - 填写商店展示的信息和发布者；
    - 「截图并检查」：真实运行截图、跑 App Hub 的检查、生成审查包；
    - 回答审查问题，可以自己写，也可以让外环起草；
    - 「提交审批…」：先列出每一步和要开的 issue，你确认后才执行：按需签名、提交并打 tag、推送仓库，并在 OctoSense-org/OctoSense-App-Hub 开 `Submit <id> <版本>` issue。
- **应用相关功能合成一个插件**：预览和发布都并入「OctoSense 应用」插件。
- **自带 OctoScript App Design Flow**：文档、`tools/octo` 和应用模板都随 OctoBuddy 提供，只装了 OctoBuddy 也能新建和规划应用。

**变化**：
- 宿主机版只显示它自己有的东西：
  - AI providers 只用 OctoBuddy 自己的（Cindy 那种配置），不再有「OctoSense 的」来源和导入；
  - 应用工厂、生产回路这两个为 OctoSense 服务的插件不再列出；
  - 外观的第一项改为跟随 macOS 的浅色或深色。

**需要**：
- Apple 芯片的 Mac，macOS 11 或更高。
- 用 pi 需要 Node.js 和 npm。
- 检查和提交应用，需要编好 App Hub 的 `hub` 和 `card-host`（`OCTOSENSE_APP_HUB`），并装好 GitHub 的 `gh` 且已登录。

**还没有**：
- 还没有用它真正提交过一次 App Hub：只在副本上验证到「列出提交步骤」这一步。
- Intel Mac、Linux 和 Windows 的版本；经过公证的版本。
