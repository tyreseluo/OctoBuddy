# OctoBuddy

[English](README.md) | 简体中文

OctoBuddy 是 OctoSense 的原生编码应用，分内外两层循环：
- **外环**把需求拆成若干切片的计划，并审查做回来的结果；
- **内环**在项目目录（或它的 git worktree）里并行做这些切片；
- **对话**只和一个 agent 交谈，不跑循环。

它既能作为原生应用跑在 OctoSense 里，也能单独开一个自己的窗口运行。OctoSense 会像引入 Rinx 那样，在自己的 `native-apps.json` 里把它登记为应用 `octobuddy`，钉在本仓库的某个 revision 上。桌面 shell 默认编进去，手机 shell 不带。**待完成：**这项登记要向 OctoSense 提 PR，目前还没提。

2026-10-02 之前它叫 OctoLoop。

## Agent 与模型

| Agent | 外环 / 对话 | 内环 | OctoBuddy 怎么驱动它 |
| --- | --- | --- | --- |
| Claude Code | 支持 | 支持 | `claude -p`，输入输出都是 stream-json |
| Codex | 支持 | 支持 | `codex app-server`（JSON-RPC） |
| pi | 支持 | 支持 | `pi --mode rpc` |
| octos | 支持 | 支持（默认） | OctoBuddy 自己的 `octos serve`；对话里也可以用 OctoSense 自带的 agent |

- **模型。**Claude Code 可以用你自己的登录；每个 agent 也都可以通过 OctoBuddy 的本机代理，使用 **AI providers** 里启用的 provider。
- **Codex。**这些 provider 没有 `/responses` 接口，所以 Codex 的请求先经过一层 Responses → Chat Completions 的转换。
- **Key。**代理从 AI providers 的 profile 里读出 provider 的 key，加到它转发的请求上。子进程只拿到占位 key，它的环境变量和命令行参数里都没有 key。
- **选择。**输入框下方的选择器左边是 agent，右边是它的模型，模型下面是思考强度（effort）。
  - 同一个 agent 换模型会保留对话（Codex 恢复原线程，pi 恢复原会话文件）。
  - 换 agent 则从新对话开始。
  - 改思考强度会让 agent 接着原对话重启；如果这一轮还在运行，等它结束后再重启。

## 对话中

- **停止。**点停止按钮或按 Esc 会中断这一轮。5 秒后还没停下，就结束它的整个进程组。已经写出的内容保留。
- **继续。**停止后队列先暂停。点「继续」会发出排队的内容；没有排队内容时，让 agent 从中断处接着做。
- **插话。**⌘Enter（其他平台 Ctrl+Enter）把消息插进正在运行的这一轮，不打断它。排队中的消息上也有「插话」按钮，作用相同。
  - agent 在下一步采纳它；如果刚好在这一轮结束后才到，就改成排在队列最前面。
  - 消息下方有一行回执，显示它是否已送达。
- **视图。**对话；外环和内环的流程图；回放整个会话的时间轴；开启原生 TUI 插件后，还有 agent 自己的终端界面。
- **插件。**内置的有：OctoSense 应用类型、应用预览与发布到 App Hub、应用数据、原生 TUI（默认关闭）。外部插件放在 `<data>/plugins/<id>/plugin.json`，旁边放一个程序，OctoBuddy 每次调用都运行它一次。

## 构建与测试

在本仓库里：

```sh
cargo test --locked
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo run            # 在 OctoSense 之外，单独开一个 OctoBuddy 窗口
```

真实测试默认忽略。运行它们需要：各 agent 的 CLI、AI providers 里配好对应的 provider、shell 的 octos 目录。

```sh
OCTOS_APP_CORE_DIR=<OctoSense home>/octos-home/.octos OCTOBUDDY_PI_BIN=<pi> \
  cargo test --lib -- --ignored --nocapture <测试名>
```

| 测试 | 验证什么 |
| --- | --- |
| `codex_switches_model`、`pi_switches_model` | 同一个对话里先用 GLM 再换 MiniMax：GLM 记下的暗号，MiniMax 能答出来 |
| `codex_and_pi_steer_a_turn`、`claude_steers_a_turn` | 命令执行中插话，同一轮里就会采纳 |
| `codex_and_pi_steer_a_reply` | agent 在写不调工具的回复时插话，同样能被听到 |
| `codex_lead_on_glm`、`pi_lead_on_glm`、`codex_chat_reaches_the_network` | 各 agent 在 GLM 上都能回答；Codex 在对话里能访问网络 |

以上测试都在 macOS（Apple 芯片）上运行过，时间是 2026-10-01 和 2026-10-02。当时代码还在 OctoSense 仓库里，也还没改名。在 Linux 和 Windows 上**未验证**。

### 钉住的版本

`Cargo.toml` 里各依赖的版本：
- OctoSense 和 App Hub：和 OctoSense main 钉的版本相同；
- makepad：用 App Hub 指定的版本 `1f3b1de`，比 OctoSense 的 `e29a0eaa` 早 9 个提交。

Cargo 不能用同一个仓库的另一个 revision 去 patch 一个 git 来源，所以这里只能统一成 App Hub 那一份。在 OctoSense 里编译时，它的 `[patch]` 会把这些依赖全部换成 shell 自己的检出。OctoSense 升级这些版本时，这里也要跟着改。

单独运行时，侧栏旁边的分隔线会显示成一条深色粗条；在 OctoSense 里则是正常的细线。**还没查原因。**

## 环境变量

| 变量 | 用途 |
| --- | --- |
| `OCTOBUDDY_HOME` | 数据目录，放项目、对话和插件（默认 `~/.octobuddy`） |
| `OCTOBUDDY_PI_BIN` | pi 不在 `PATH` 上时，它所在的位置 |
| `OCTOBUDDY_DEBUG_EVENTS=1` | 把每个循环事件打印到 stderr |
| `OCTOBUDDY_BRIDGE_LOG=<file>` | 追加记录 Codex 转换层每次失败的调用（状态、请求和响应，从不含 key） |

## 尚未完成

- 内环还不能跑在系统 octos 上：app peer 缺少工作目录、bash、插话和审批范围，见 [docs/upcr-draft-coding-peers.md](docs/upcr-draft-coding-peers.md)。
- octos 的 macOS 沙箱不给 `iokit-open`，UI 应用在里面无法渲染。这需要 octos 加一个 GPU 开关，正在向 octos 提出。
- **未验证：**手机、Linux、Windows。
