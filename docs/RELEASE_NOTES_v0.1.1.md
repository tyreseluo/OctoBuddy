OctoBuddy @VERSION@ fixes what the first release got wrong in a conversation's terminal, its icon and its providers.

## Download

For Macs with Apple silicon (M1 and later): `OctoBuddy-@VERSION@-macos-aarch64.dmg`, with its `.sha256` beside it. Open the DMG and drag OctoBuddy to Applications (replace the earlier one).

**The first time you open it.** It is not notarized by Apple yet, so macOS stops it the first time. Open it once, then go to **System Settings › Privacy & Security** and choose **Open Anyway**. Or, in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OctoBuddy.app
```

## Fixed

- **A blank stage.** After a conversation's terminal had been open, switching to another conversation could leave the messages and the composer hidden. They show again.
- **A blank terminal.** In the packaged app the terminal drew no text: its fonts were not found in the package. They are now.
- **A terminal that outlived its conversation.** The terminal now belongs to the conversation (or inner loop) it was opened on. Picking another one, deleting that one, turning the plugin off or closing OctoBuddy ends it, and the CLI in it.
- **The icon.** The window's caption shows OctoBuddy's icon, and a build run from source sets it in the Dock too, not Makepad's.

## New

- **Claude Code and pi on more providers:** MiniMax (China and international), DeepSeek, Moonshot (Kimi), Zhipu, Alibaba DashScope (Qwen) and OpenRouter, through the Anthropic-compatible endpoint each provider runs beside its own API. With MiniMax China's key, Claude Code answered on MiniMax-M3.
- **Provider marks** beside models in the picker, in Settings › AI Providers and its wizard, and in an inner loop's panel.

---

## 中文

OctoBuddy @VERSION@ 修复了第一个版本在终端、图标和模型接入上的问题。

**下载**：适用于 Apple 芯片（M1 及以后）的 Mac，下载 `OctoBuddy-@VERSION@-macos-aarch64.dmg`。打开 DMG，把 OctoBuddy 拖进「应用程序」，替换旧版即可。第一次打开时，到「系统设置 › 隐私与安全性」点「仍要打开」，或运行上面那条 `xattr` 命令。

**修复**：
- **空白界面**：某个对话开过终端后，切到别的对话，消息区和输入框可能不显示。现在会正常显示。
- **终端白屏**：打包版里终端显示不出文字，因为包里缺了终端用的字体。现在已经带上。
- **旧终端不关**：终端现在只属于打开它的那个对话或内环。切到别的对话、删掉这个对话、关掉插件或退出 OctoBuddy 时，它都会结束，里面的 CLI 也一起结束。
- **图标**：窗口标题栏显示 OctoBuddy 的图标。从源码运行时，Dock 上也是 OctoBuddy 的图标，不再是 Makepad 的。

**新增**：
- **Claude Code 和 pi 支持更多厂商**：MiniMax（国内和国际）、DeepSeek、Moonshot（Kimi）、智谱、阿里云百炼（通义千问）和 OpenRouter。走的是这几家在自家 API 旁边另外提供的 Anthropic 兼容地址。用 MiniMax 国内版的 key 实测过，Claude Code 能在 MiniMax-M3 上正常回答。
- **厂商图标**：模型选择器、「设置 › AI Providers」和添加向导、内环面板里，模型旁边都会显示它所属厂商的图标。
