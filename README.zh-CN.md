# OctoBuddy

[English](README.md) | 简体中文

OctoBuddy 是 OctoSense 的原生编码应用，分内外两层循环：
- **外环**把需求拆成若干切片的计划，并审查做回来的结果；
- **内环**在项目目录（或它的 git worktree）里并行做这些切片；
- **对话**只和一个 agent 交谈，不跑循环。

它既能作为原生应用跑在 OctoSense 里，也能单独开一个自己的窗口运行。OctoSense 会像引入 Rinx 那样，在自己的 `native-apps.json` 里把它登记为应用 `octobuddy`，钉在本仓库的某个 revision 上。桌面 shell 默认编进去，手机 shell 不带。这项登记已经在 OctoSense 的一个 fork 上的分支 [`feat/octobuddy-app`](https://github.com/tyreseluo/OctoSense/tree/feat/octobuddy-app) 里准备好了（钉的是 `main` 上较新的一个 revision）。**待完成：**还没有向 OctoSense 提 PR。

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

## 自带的 agent

和 Cindy 一样，OctoBuddy 为每个 agent 的程序自带一份，版本是测试过的那个。OctoBuddy 驱动它们用的协议会随版本变化，所以自带一份，同一个 OctoBuddy 在每台机器上的行为就一样。

| 程序 | 版本 | 来源 | 校验依据 |
| --- | --- | --- | --- |
| Claude Code | 2.1.286 | npm，`@anthropic-ai/claude-code-<platform>` | npm 的 sha512 integrity |
| Codex | 0.152.0 | npm，`@openai/codex@0.152.0-<platform>` | npm 的 sha512 integrity |
| pi | 0.99.2 | npm，按 `resources/agents/pi/package-lock.json` 执行 `npm ci` | npm 对每个包的 integrity |
| octos | 2.0.3-rc.12 | octos 在 GitHub release 里发布的 bundle | GitHub 的 sha256 摘要 |

- **什么时候装。**第一次用到时安装。如果外环、内环或对话因为本机没有这个程序而起不来，OctoBuddy 会开始安装，并在对话里说明；装好后再发一次即可。也可以在 设置 › 工具 里提前安装。
- **装在哪。**`<data>/agents/<name>/<version>/`。只有下载内容与固定的摘要一致、并且解压完成之后，这个版本的目录才会出现。不运行任何安装脚本（pi 用 `--ignore-scripts` 安装）。
- **改用你自己的。**设置 › 工具 里每个 agent 的卡片可以选**自带的**（默认）或**你自己的**（`PATH` 上找到的第一个）。选择保存在 `<data>/agents.json`。自带的还没装好时，运行你自己的。切换只影响之后启动的 agent。
- **登录。**登录仍然是你自己的：自带的 Claude Code 照样读 `~/.claude`。它以 `DISABLE_AUTOUPDATER=1` 运行，所以版本不会自己变。
- **不覆盖的情况。**
  - pi 需要 Node.js 和 npm。
  - 这个 octos 版本没有 Intel Mac 的 bundle，所以 Intel Mac 上运行你自己的。
  - Windows 上没有自带的版本。
- **大小。**Apple silicon 上四个一共约 900 MB。
- **升级版本。**改 `src/agents.rs` 里对应的固定项（pi 还要改 `resources/agents/pi/`）。
- **已验证：**2026-10-02 在 macOS（Apple silicon）上验证。`installs_every_agent_this_machine_has_a_copy_of`（默认忽略）会把四个都下载到一个临时的 `OCTOBUDDY_HOME`，逐个安装并检查 `--version`。在 OctoSense 里：本机没有 pi 时，在 pi 的对话里发第一条消息就开始安装，再发一次即得到回复；Codex 和 Claude Code 的对话都跑在自带的版本上（Claude Code 用的是你自己的登录）。跑在自带 octos 上的内环还没有实测。Linux **未验证**。

## 对话中

- **停止。**点停止按钮或按 Esc 会中断这一轮。5 秒后还没停下，就结束它的整个进程组。已经写出的内容保留。
- **继续。**停止后队列先暂停。点「继续」会发出排队的内容；没有排队内容时，让 agent 从中断处接着做。
- **插话。**⌘Enter（其他平台 Ctrl+Enter）把消息插进正在运行的这一轮，不打断它。排队中的消息上也有「插话」按钮，作用相同。
  - agent 在下一步采纳它；如果刚好在这一轮结束后才到，就改成排在队列最前面。
  - 消息下方有一行回执，显示它是否已送达。
- **视图。**对话；外环和内环的流程图；回放整个会话的时间轴；开启原生 TUI 插件后，还有 agent 自己的终端界面。
- **插件。**内置的有：OctoSense 应用类型、应用预览与发布到 App Hub、应用数据、生产回路（见下文）、原生 TUI（默认关闭）。每个内置插件是 `src/plugins/` 下的一个文件（框架在 `src/plugins/mod.rs`）。外部插件放在 `<data>/plugins/<id>/plugin.json`，旁边放一个程序，OctoBuddy 每次调用都运行它一次。

## 生产回路

插件 `card-loop` 在应用发布之后继续守着它，对应 GOSIM 2026 的「生产回路 Agent」悬赏（读取运行指标，发现卡片失败，修复并重新发布）。现在实现了巡检这一段，修复和重新发布是下一步。

- **开启。**在应用项目里点「巡检」按钮。应用要先发布到本地 App Hub，因为巡检的是发布出去的那个版本，不是项目里正在改的文件。
- **一次巡检。**每 5 分钟用实时数据 headless 运行一次已发布版本的副本（和应用检查的跑法一样），读取：是否启动、脚本报错、屏幕上的控件、截图。
- **判断。**一个版本第一次正常运行时记为基线。之后的运行：
  - 没能启动是**起不来**；
  - 有脚本报错是**出错**；
  - 命名控件少了 40% 以上，或者有文字的控件不到一半，或者出现了基线里没有的失败提示（如「无法获取汇率…」），是**异常**。
- **通知。**健康状态一变，就在开启巡检的那个会话里发一条消息，附原因和截图。按钮会显示当前状态（绿、红）和上次巡检的时间。
- **记录。**在 `<项目>/.octobuddy/card-loop/` 下：`watch.json`（开关、间隔、基线、状态）、`health.jsonl`（每次巡检一行）、`shots/`（保留最近 20 张）。
- **演练。**在 `watch.json` 里写 `"drill": "offline"`，巡检时会把副本能访问的网络主机换成一个永远不响应的地址，模拟它的 API 挂了。`"every"`（秒，至少 30）设置间隔。两者都从下一次巡检起生效。已发布的副本本身从不改动。
- **已验证：**2026-10-02 在 macOS 上用已发布的汇率看板（fx-board）0.1.0 验证。真实测试 `probes_a_published_app_well_and_cut_off`（默认忽略）的结果是：正常时判为健康（25 个控件，每次 5.7 秒），断开 API 后判为异常。在 OctoSense 里：开启巡检并记下基线；断网演练 29 秒后报故障、按钮变红；恢复后 30 秒报恢复；之后关闭巡检。
- **巡检看不到的。**只在真实宿主里才出现的问题，比如在 OctoSense 里权限被拒。shell 目前还不会把已安装卡片的运行时报错开放给其他应用。

## 外观

在设置 › 外观里选择它的样子。

**跟随 OctoSense**（默认）：OctoSense 是深色时用 OctoBuddy 的深色主题，否则用浅色主题，OctoSense 一切换就跟着变。其余选项是 OctoBuddy 自己的主题：浅色、深色、GitHub Light、GitHub Dark、Atom One Light、Atom One Dark、Dracula、Nord、Solarized Light、Solarized Dark。选完立即生效，不用重启。

- **选择保存在哪：**`<data>/appearance.json`。
- **主题怎么定义：**每套主题在 `src/theme.rs` 里只写 11 个颜色（页面、侧栏、面板、文字、次要文字、分隔线、强调色、成功、危险、警告、紫色），其余 token 都由它们混合出来。所以加一套新主题，只要写 11 个颜色。

## 构建与测试

在本仓库里：

```sh
cargo test --locked
cargo clippy --locked --all-targets --no-deps -- -D warnings
cargo run            # 在 OctoSense 之外，单独开一个 OctoBuddy 窗口
```

真实测试默认忽略。运行它们需要：各 agent 的程序（自带的或你自己的）、AI providers 里配好对应的 provider、shell 的 octos 目录。

```sh
OCTOS_APP_CORE_DIR=<OctoSense home>/octos-home/.octos \
  cargo test --lib -- --ignored --nocapture <测试名>
```

| 测试 | 验证什么 |
| --- | --- |
| `codex_switches_model`、`pi_switches_model` | 同一个对话里先用 GLM 再换 MiniMax：GLM 记下的暗号，MiniMax 能答出来 |
| `codex_and_pi_steer_a_turn`、`claude_steers_a_turn` | 命令执行中插话，同一轮里就会采纳 |
| `codex_and_pi_steer_a_reply` | agent 在写不调工具的回复时插话，同样能被听到 |
| `codex_lead_on_glm`、`pi_lead_on_glm`、`codex_chat_reaches_the_network` | 各 agent 在 GLM 上都能回答；Codex 在对话里能访问网络 |

以上测试都在 macOS（Apple 芯片）上运行过，时间是 2026-10-01 和 2026-10-02。当时代码还在 OctoSense 仓库里，也还没改名。在 Linux 和 Windows 上**未验证**。

### 在 OctoSense 里运行

在那个 PR 合入之前，可以用上面这个分支在 OctoSense 的 shell 里运行 OctoBuddy。在 OctoSense 的检出里切到该分支后：

```sh
python3 tools/setup.py --update
cargo build --locked --release -p octosense
MAKEPAD_WM_TEST_APP=octobuddy ./target/release/octosense
```

2026-10-02 就是这样跑起来的（macOS，Rust 1.97.1；shell 依赖的 matrix-sdk 需要 1.95 或更新）。Cargo 会按钉住的 revision 拉取 OctoBuddy；要用更新的版本，需要改 OctoSense `native-apps.json` 里的钉，再运行 `python3 tools/native_apps.py`。

第一次在 OctoSense 里启动时，OctoSense 会询问是否允许 OctoBuddy 的 agent 启动（即它的 octos 服务）。不管选哪个，OctoBuddy 自己的引擎都能照常运行。

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
| `OCTOBUDDY_<NAME>_BIN` | 指定 `claude`、`codex`、`pi`、`octos`（以及其他工具）运行哪个程序，优先于自带的和你自己的 |
| `OCTOBUDDY_DEBUG_EVENTS=1` | 把每个循环事件打印到 stderr |
| `OCTOBUDDY_BRIDGE_LOG=<file>` | 追加记录 Codex 转换层每次失败的调用（状态、请求和响应，从不含 key） |

## 尚未完成

- 内环还不能跑在系统 octos 上：app peer 缺少工作目录、bash、插话和审批范围，见 [docs/upcr-draft-coding-peers.md](docs/upcr-draft-coding-peers.md)。
- octos 的 macOS 沙箱不给 `iokit-open`，UI 应用在里面无法渲染。这需要 octos 加一个 GPU 开关，正在向 octos 提出。
- **未验证：**手机、Linux、Windows。
