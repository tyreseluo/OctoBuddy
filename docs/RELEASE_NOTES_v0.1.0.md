OctoBuddy @VERSION@ is the first release of OctoSense's coding app with two loops, on its own, outside OctoSense. An **outer loop** turns a request into a plan of slices and reviews what comes back; **inner loops** build the slices in parallel. A plain **chat** talks to one agent.

## Download

For Macs with Apple silicon (M1 and later): `OctoBuddy-@VERSION@-macos-aarch64.dmg`, with its `.sha256` beside it. Open the DMG and drag OctoBuddy to Applications. Intel Macs are not supported.

**The first time you open it.** This release is not notarized by Apple yet (there is no Developer ID for OctoBuddy), so macOS stops it the first time. Open it once, then go to **System Settings › Privacy & Security** and choose **Open Anyway**. Or, in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OctoBuddy.app
```

## What it does

- **Agents.** Claude Code, Codex, pi and octos, for the outer loop, the inner loops and chats. OctoBuddy installs its own copy of each, at the version it was tested with, the first time one is needed. Claude Code can use your own login; every agent can use the providers you set up in **Settings › AI Providers** (keys stay in the macOS Keychain, behind OctoBuddy's loopback proxy).
- **In a conversation.** Stop, go on, steer a running turn (⌘Enter), or interrupt it and send (⇧⌘Enter). Views: chat, the flow graph of the loops, a timeline that replays the session (each wait says why; rework is marked with its cost), and a read-only live view of any loop.
- **Each slice on its own agent and model**, chosen by the outer loop; when one fails before doing any work, the outer loop can run it again elsewhere and tells you why.
- **OctoSense apps.** A project can be an OctoSense app. Every agent gets OctoBuddy's tools to look at, drive, probe and check the running app, and two digests to read first: a Splash cookbook and OctoScript App Design Flow. Preview it in OctoBuddy, then publish it to a local App Hub.
- **Learning from rework.** After rework, the outer loop keeps lessons that every later run starts from.

## Needs

- A Mac with Apple silicon, macOS 11 or later.
- For pi: Node.js and npm.
- For OctoSense apps: a checkout of [OctoScript App Design Flow](https://github.com/OctoSense-org/OctoScript-App-Design-Flow) beside OctoSense, or `OCTOBUDDY_DESIGN_FLOW` set to it.

## Not yet

- Intel Macs, Linux and Windows.
- A notarized build.

---

## 中文

OctoBuddy @VERSION@ 是 OctoSense 双环编码应用的第一个独立版本，可以脱离 OctoSense 单独运行。**外环**把需求拆成切片并审查结果，**内环**并行完成各个切片；**对话**模式直接和一个 agent 交流。

**下载**：适用于 Apple 芯片（M1 及以后）的 Mac，下载 `OctoBuddy-@VERSION@-macos-aarch64.dmg`。打开 DMG，把 OctoBuddy 拖进「应用程序」。暂不支持 Intel Mac。

**第一次打开**：这个版本还没有经过 Apple 公证，macOS 会拦一次。先打开一次，再到「系统设置 › 隐私与安全性」点「仍要打开」；也可以在终端运行上面那条 `xattr` 命令。

**主要功能**：
- 外环、内环和对话都可以用 Claude Code、Codex、pi、octos。
- 每个切片可以用不同的 agent 和模型；某个切片失败了，外环会换一个 agent 或模型重做。
- 时间轴写明每段等待的原因，并标出返工和它浪费的时间。
- 外环、内环都有只读的实时视图。
- 可以做 OctoSense 应用：预览、检查，再发布到本地 App Hub。
- 能从返工中总结经验，之后的每次运行都会用上。

**需要**：
- Apple 芯片的 Mac，macOS 11 或更高版本。
- 用 pi 需要 Node.js 和 npm。

**还没有**：Intel Mac、Linux 和 Windows 的版本，以及经过公证的版本。
