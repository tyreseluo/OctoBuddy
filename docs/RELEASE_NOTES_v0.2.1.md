OctoBuddy @VERSION@ makes a new OctoSense app from a template in one dialog, and fixes what v0.2.0 got wrong on this computer: the terminal's font, a blank conversation, a reply's mark.

## Download

For Macs with Apple silicon (M1 and later): `OctoBuddy-@VERSION@-macos-aarch64.dmg`, with its `.sha256` beside it. Open the DMG and drag OctoBuddy to Applications (replace the earlier one).

**The first time you open it.** It is not notarized by Apple yet, so macOS stops it the first time. Open it once, then go to **System Settings › Privacy & Security** and choose **Open Anyway**. Or, in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OctoBuddy.app
```

## New

- **A new OctoSense app, from a template.** The sidebar's **+ › New OctoSense app…** opens a dialog: pick a template, name it (its id follows the name), choose where it goes and tick what it may do, filled in from the template. **Create** writes the name, id and grants into the manifest, stamps it, makes the first commit and opens it in the workbench's Preview.
- **Templates that show what OctoSense gives an app**, in their own repository, [tyreseluo/octosense-app-templates](https://github.com/tyreseluo/octosense-app-templates), fetched each time the dialog opens: **Sandbox starter** (what the sandbox grants and refuses, its own storage, a home tile), **News reader** (allow-listed hosts, pictures from the web, the system web view), **Weather** (one host, the device's location, an offline cache) and **Camera journal** (camera, microphone, photo library). Each ran in App Hub's `card-host` and passes `hub check`. Offline and never fetched, the design flow's own template is offered.
- **A reply says what it ran on.** An agent's reply carries its model's name and its provider's mark: "Claude Code · MiniMax-M3" with MiniMax's mark, the agent's own mark on its own sign-in.

## Fixed

- **The terminal's font.** In the installed app the terminal drew in a proportional font, widely spaced, without emoji: it now uses the bundled JetBrains Mono (bold too), emoji and Nerd Font symbols, with LXGW WenKai for Chinese. OctoBuddy keeps its Makepad state in its own data folder (`.makepad`), not the `~/.makepad` every Makepad app shares.
- **A blank conversation.** Opening a conversation could show nothing until a message was sent: each conversation now keeps where you were in it, and opens at its end otherwise.
- **Settings › AI Providers** is laid out again: the providers, the agents' own sign-ins and the inner loops' model in one list (no gap under one card), **+ Add a provider** beside **Reload**, each card with its model, Primary or Fallback, its provider, endpoint and where its key is. The page speaks the interface's language throughout.
- **Settings › Tools** shares each row's width among its cards: no empty space on the right.
- **Monochrome provider marks** (Kimi, Z.ai, OpenAI…) show on a dark theme.
- **Smaller things:** the send button's corners are squarer; the sidebar's title is OctoBuddy, not "Projects".

## Needs

- A Mac with Apple silicon, macOS 11 or later.
- `git`, to fetch the app templates.
- For pi: Node.js and npm.
- To check and submit an app: App Hub's `hub` and `card-host` built (`OCTOSENSE_APP_HUB`), and GitHub's `gh`, signed in.

## Not yet

- A mail template: no host that runs here serves the mail service to check it with.
- Intel Macs, Linux and Windows; a notarized build.

---

## 中文

OctoBuddy @VERSION@ 可以在一个对话框里从模板新建 OctoSense 应用，并修好了 v0.2.0 在本机上的几个问题：终端字体、对话白屏、回复的图标。

**下载**：适用于 Apple 芯片（M1 及以后）的 Mac，下载 `OctoBuddy-@VERSION@-macos-aarch64.dmg`。打开 DMG，把 OctoBuddy 拖进「应用程序」，替换旧版即可。第一次打开时，到「系统设置 › 隐私与安全性」点「仍要打开」，或者运行上面那条 `xattr` 命令。

**新增**：
- **从模板新建 OctoSense 应用**：侧栏「+ › 新建 OctoSense 应用…」打开对话框，选模板、起名字（应用 ID 跟着名字生成）、选保存位置、勾选权限（按模板预先填好）。点「创建」后写好 manifest 里的名称、ID 和权限，盖 digest，做第一次提交，并在工作台预览里打开。
- **体现 OctoSense 能力的模板**，单独放在 [tyreseluo/octosense-app-templates](https://github.com/tyreseluo/octosense-app-templates)，每次打开对话框时拉取最新的：
  - **沙箱起步**：沙箱给了什么、拒绝什么，自己的存储，主屏卡片；
  - **资讯阅读**：域名白名单、网络图片、系统网页视图；
  - **天气**：单一域名、设备定位、离线缓存；
  - **相机日记**：相机、麦克风、相册。
  
  每个模板都在 App Hub 的 `card-host` 里跑过，并通过了 `hub check`。离线而且从没拉取过时，提供 Design Flow 自带的模板。
- **回复标明用的什么模型**：Agent 的回复带上模型名和供应商图标，比如「Claude Code · MiniMax-M3」配 MiniMax 的图标；用 agent 自己的登录时仍是它自己的图标。

**修复**：
- **终端字体**：安装版的终端原来用的是比例字体，字距很宽，emoji 也不显示。现在用自带的 JetBrains Mono（粗体也是），emoji 和 Nerd Font 符号都能显示，中文用霞鹜文楷。OctoBuddy 的 Makepad 状态放在自己的数据目录（`.makepad`），不再写所有 Makepad 应用共用的 `~/.makepad`。
- **对话白屏**：打开对话时可能一片空白，要发一条消息才显示。现在每个对话记住自己的滚动位置，没记过的直接显示到最新一条。
- **设置 › AI Providers 重新排版**：
  - 供应商、Agent 自己的账号、Inner 用的模型放在同一个列表里，只有一张卡片时下面不再空出一大片；
  - 「+ 添加供应商」放在「重新载入」旁边；
  - 每张卡片写明模型、主模型还是备用、供应商、接入点和 Key 存在哪里；
  - 整页文字都跟随界面语言。
- **设置 › 工具**：卡片按窗口宽度分列，右侧不再留白。
- **单色供应商图标**（Kimi、Z.ai、OpenAI 等）在深色主题下能看清了。
- **小改动**：发送按钮的圆角更小、更方；侧栏左上角显示 OctoBuddy，不再显示「项目」。

**需要**：
- Apple 芯片的 Mac，macOS 11 或更高。
- 拉取应用模板需要 `git`。
- 用 pi 需要 Node.js 和 npm。
- 检查和提交应用，需要编好 App Hub 的 `hub` 和 `card-host`（`OCTOSENSE_APP_HUB`），并装好 GitHub 的 `gh` 且已登录。

**还没有**：
- 邮件模板：本机能跑的宿主都不提供邮件服务，没法验证。
- Intel Mac、Linux 和 Windows 的版本；经过公证的版本。
