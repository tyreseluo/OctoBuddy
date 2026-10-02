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
- **插件。**内置的有：OctoSense 应用类型、应用预览与发布到 App Hub、应用数据、应用工厂和生产回路（见下文）、原生 TUI（默认关闭）。每个内置插件是 `src/plugins/` 下的一个文件（框架在 `src/plugins/mod.rs`）。外部插件放在 `<data>/plugins/<id>/plugin.json`，旁边放一个程序，OctoBuddy 每次调用都运行它一次。

## 应用工厂

插件 `app-factory` 负责做 OctoBuddy 之外请求的 OctoSense 应用，对应 GOSIM 2026 的「软件工厂」悬赏：任务进度、Agent 状态、结果验收、协作交接。

- **请求。**OctoSense 系统 agent，或被授权的其他应用的 agent，调用 `octobuddy.request {what, for, acceptance, data, name}`。调用立刻返回请求的 id。
- **等你决定。**请求出现在「应用」页（侧栏底部设置旁边的脉冲按钮；有请求等待时按钮显示为强调色），写明谁请求、为谁做、验收标准和数据来源。点**开始做**之前，什么都不会建，也不花模型的钱。点**不做**则拒绝。
- **建造。**点「开始做」后，OctoBuddy 在 `<data>/apps/` 下从设计流程的模板建项目，读取它指定的数据 API，然后打开一个新会话，把请求的说明放在输入框里。在输入框下方的选择器里选好外环和内环用的模型，再发送；数据结构会随这条消息一起交给外环。
- **发布。**外环停下后，在卡片上点**发布**，把它发布到本地 App Hub。发布后自动开启巡检。
- **跟进。**`octobuddy.status` 的 `requests` 列出每个请求的状态：等你决定、已拒绝、准备中、建造中（以及外环是否在工作）、已发布（版本号）、失败。
- **记录**在 `<data>/requests.json`。
- **已验证：**2026-10-02 在 OctoSense 里用 Open-Meteo 的接口做上海天气卡片。
  - 在系统 agent 的对话里提出请求后，系统 agent 转给 OctoBuddy 的 agent，后者调用了 `octobuddy.request`，并根据 fx-board 那次故障自己补上了验收标准。
  - 请求卡出现在「应用」页，侧栏按钮也有了标记。
  - 点「开始做」后建好了项目，读到了 API 的数据结构，打开了会话，并把说明发给了外环。那一轮随后被停止，所以应用还没真正做出来。


插件 `card-loop` 在应用发布之后继续守着它们，对应 GOSIM 2026 的「生产回路 Agent」悬赏：读取运行指标，发现卡片失败，修复并重新发布。

**巡检页。**在侧栏底部设置图标旁边的「脉冲」按钮打开；有巡检中的应用出问题时，这个按钮变红。页面列出所有 OctoSense 应用项目，每个应用显示：
- 已发布的版本和健康状态；
- 最近 12 次巡检的圆点；
- 现在哪里有问题、已经做了什么；
- 按钮：开启巡检、立即巡检、演练断网、自动修复、打开会话、修复、发布修复版。

巡检的是发布到本地 App Hub 的版本，不是项目里正在改的文件。

**一次巡检。**每 5 分钟用实时数据 headless 运行一次已发布版本的副本（和应用检查的跑法一样），读取：
- 是否启动；
- 脚本报错；
- 屏幕上的控件；
- 截图。

应用的存储在两次巡检之间保留（`card-loop/state/`），和真实设备一样。所以会保存上次数据的版本，在 API 挂掉时能显示出来。

**判断。**一个版本第一次正常运行时记为基线。之后的运行：
- 没能启动是**起不来**；
- 有脚本报错是**出错**；
- 以下任一情况是**异常**：
  - 命名控件少了 40% 以上；
  - 有文字的控件不到一半；
  - 出现了基线里没有的失败提示。但如果内容还在（有文字的控件至少 80%），就不算异常：提示离线、同时显示上次数据的应用是在正常应对。

新版本在拿到自己的基线之前，用上一个版本的基线比较内容。

**故障**从发现问题的那次巡检开始，到确认恢复的那次巡检结束。
- 开始和结束都会在它的会话里说。
- 点**修复**，就把一份说明交给那个会话的外环：巡检看到了什么、截图，以及演练意味着什么。打开**自动修复**后，出故障时立即自动交给外环。
- 项目里的应用和已发布的副本不同之后，会出现**发布修复版**。点它发布修复版，下一次巡检会检查它。
- 如果演练开着，发布后的下一次巡检先不演练运行一次，相当于设备联网时更新并用了一次；之后的演练运行才算数。
- 恢复正常时，会话里会说明每一步各用了多久。

**记录**在 `<项目>/.octobuddy/card-loop/` 下：
- `watch.json`：开关、间隔、基线、状态、故障；
- `health.jsonl`：每次巡检一行；
- `shots/`：最近 20 张截图；
- `state/`：应用的存储。

**演练。**已发布的副本本身从不改动。
- **演练断网**（`offline`）：巡检运行时断开应用的 API，设备之前用过它。
- 在 `watch.json` 里写 `"drill": "offline-fresh"`：按新设备运行，没有任何缓存。
- **演练：发布坏版本**：故意把应用改坏，作为下一个版本发布，模拟一次没走检查就上线的改动。
  - 在 `main.splash` 里挑被调用最多的函数，只改它定义处的名字，调用处不变。
  - 这处改动在项目里单独作为一个提交（`drill: 坏版本演练…`）。
  - 由此产生的故障会标为这次演练。修复说明会要求外环根据巡检看到的错误找到原因并修好，不要直接回退整个版本。
  - 发布修复版、并且修复版运行正常后，这次演练结束。
- `"every"`（秒，至少 30）设置间隔。

**已验证：**2026-10-02 在 macOS 上用已发布的汇率看板（fx-board）0.1.0 验证。
- 真实测试 `probes_a_published_app_well_and_cut_off`（默认忽略）：
  - API 正常：健康（26 个控件，每次 5.5 秒）；
  - 在用过它的设备上断开 API：仍然健康，因为它保留了上次的汇率并提示了用户；
  - 在新设备上断开 API：异常。
- 在 OctoSense 的巡检页里：
  - 开启巡检后记下了基线；
  - 演练断网时保持健康；
  - 新设备演练打开了一个故障，并在会话里报告；
  - 点「修复」后，说明发给了会话的外环，并打开了会话。那一轮随后被停止。
- **还没有端到端跑过：**外环真正修复、发布修复版、以及确认恢复的那次巡检。fx-board 本身就能应对演练，没什么可修的。

**和 OctoSense 系统 agent 配合。**OctoBuddy 在 OctoSense 里的应用 agent 有三个工具，系统 agent 可以调用，被授权的其他应用的 agent 也可以：
- `octobuddy.status`（只读）现在还会返回 `live.apps`：每个已发布应用的版本、是否在巡检、健康状态、未结束的故障（哪里有问题、是否已交给外环、修复版是否已发布），以及最近一次报告。
- `octobuddy.report {app, problem, from}`（会执行动作，需宿主确认）用来转达别人看到的问题，比如你说「汇率看板打开是空白」，或者别的应用的 agent 发现它出错了。
  - OctoBuddy 立刻运行一次已发布的版本（没开巡检的话会顺便打开）。
  - 调用的返回值就是这次运行看到了什么；超过 20 秒还没跑完，就先返回「还在复查」。
  - 应用的会话里会说明收到的报告和复查结果，巡检页也会列出这条报告。
  - 复查确认的报告会并入故障，也会写进修复说明。
  - 复查没复现的报告，仍然可以点「修复」交给外环，因为它可能只在真实设备上出现。
- `octobuddy.send` 和以前一样，给某个会话的外环发消息。

修不修、发不发布，仍然由你决定。一条报告只会让 OctoBuddy 去看一看。

**已验证：**2026-10-02 在 OctoSense 里用 fx-board 验证。shell 的内核是 OctoSense 钉住的 octos `ae230ce`；更旧的内核打不开应用 agent 的对话（octos UPCR-2026-034 `read_parent`）。
- **在 Ask OctoBuddy 里说**「汇率看板整个是空白的，帮我让 OctoBuddy 复查一下」：它的 agent 调用了 `octobuddy.report`。复查结果是健康，agent 把结果和下一步建议转告了用户。这次调用没有弹出确认表。
- **在系统 agent 的对话里说**「朋友新装了 OctoSense，打开汇率看板什么都看不到」（开着新设备断网演练）：
  - 系统 agent 用 `peer_send_input` 把问题转给 OctoBuddy 的 agent；
  - 那个 agent 先申请用 `terminal.run` 去取 API，在那里被拒绝；
  - 随后它调用了 `octobuddy.report`；
  - 复查确认了问题，这条报告并入了故障。

  `octobuddy.report` 的描述现在写明要先调用它。

**巡检看不到的。**只在真实宿主里才出现的问题，比如在 OctoSense 里权限被拒。shell 目前还不会把已安装卡片的运行时报错开放给其他应用。

## 外观

在设置 › 外观里选择它的样子。

**跟随 OctoSense**（默认）：OctoSense 是深色时用 OctoBuddy 的深色主题，否则用浅色主题，OctoSense 一切换就跟着变。其余选项是 OctoBuddy 自己的主题：浅色、深色、GitHub Light、GitHub Dark、Atom One Light、Atom One Dark、Dracula、Nord、Solarized Light、Solarized Dark。选完立即生效，不用重启。

- **选择保存在哪：**`<data>/appearance.json`。
- **主题怎么定义：**每套主题在 `src/theme.rs` 里只写 11 个颜色（页面、侧栏、面板、文字、次要文字、分隔线、强调色、成功、危险、警告、紫色），其余 token 都由它们混合出来。所以加一套新主题，只要写 11 个颜色。

## 在宿主机上运行

和 Rinx 一样，OctoBuddy 先作为这台电脑上的独立应用运行，再进 OctoSense 里运行。在宿主机上，不用开着 OctoSense，就能用它开发 OctoSense 本身、给 OctoSense 做应用。

- **同一个界面，两种构建。**
  - `standalone` feature（默认）是宿主机上的应用：界面放在它自己的窗口里。
  - OctoSense 用 `--no-default-features --features octosense-module` 构建它，并托管它的模块（`OCTOBUDDY_MODULE`，在 `src/module.rs`）。这时不编译窗口部分。
- **怎么运行。**在 macOS 上 `cargo run` 会以 `OctoBuddy.app` 启动，Dock 里显示它的名字和图标。这个 bundle 生成在二进制旁边，由 `packaging/run-macos.sh` 生成，`.cargo/config.toml` 把它设成 cargo 的 runner。Bundle id 是 `org.octosense.octobuddy`，图标在 `packaging/` 下。
- **单独运行时有什么不同。**
  - 没有系统 agent：关于页会说明，选择器也不提供 OctoSense 的 agent；octos 跑在 OctoBuddy 自己的 octos 上。
  - `octobuddy.status`、`.report`、`.request` 要在 OctoSense 里才能被调用，所以宿主机上的「应用」页不会收到外部请求；巡检照常工作。
  - AI providers：用它自己的，或 OctoSense 的（见下文）。
  - agent 的程序用它自带的或你自己的（见上文）。
- **数据**在 `~/.octobuddy`（用 `OCTOBUDDY_HOME` 可以换目录），宿主机上和 OctoSense 里是同一份。
- **还没做：**
  - 发布包（用 cargo-packager 打签名的 `.dmg`、Linux 和 Windows 安装包、发布 workflow）；
  - 把 octos 打进应用包里。
- **已验证：**2026-10-02 在 macOS（Apple silicon）上验证：`cargo run` 启动了 `target/debug/OctoBuddy.app/Contents/MacOS/OctoBuddy`，窗口标题是 OctoBuddy；关于页和 AI Providers 页都说明它是单独运行的，并写明了读取的 profile。

### 它自己的 AI providers

在宿主机上，设置 › AI Providers 是 OctoBuddy 自己的，做法参照 Cindy。在 OctoSense 里，这一页和以前一样只读地显示 shell 的 AI Providers。代码在 `src/own_providers.rs`（数据）和 `src/providers_view.rs`（页面）。

- **同一种格式。**它存在 `<data>/providers/profiles/_main.json`，用 `octosense-llm-config` 写。OctoSense 的 AI Providers 应用就是基于这个库做的，供应商、模型和接入点都来自它的目录。所以本机代理、各 agent 和 inner 的 octos 读这份配置，和读 OctoSense 的一样。
- **来源由你选**，就在页面标题下面：
  - **OctoBuddy 自己的**（有了之后默认用它）；
  - **OctoSense 的**：它的 AI Providers 应用写的那份，只读。

  OctoSense 的里有 OctoBuddy 自己没有的 provider 时，会出现横幅，提供 **导入**。导入会把它们加在 OctoBuddy 自己的后面，并复制它们的 key。它不会替换 OctoBuddy 已有的 provider 或 key，也不改动 OctoSense 的。`OCTOBUDDY_PROVIDERS=<octos 目录>` 仍可指定另一份配置，只读。
- **添加**分三步：
  1. **选择供应商。**分三组：Coding Plan 在前，然后是更多供应商，最后是本机和自托管。每一项都写明要不要 key、有几个模型、哪些 agent 能跑在它上面。
  2. **连接。**选接入点，填 key（输入时显示为圆点）。只有接入点是你自己的服务时才要填接入地址。对已知控制台的供应商，**获取 API Key…** 会打开它的控制台。**测试连接**只发一个 1 token 的请求，并显示返回结果。Key 从不显示，报错里也没有。
  3. **选择模型。**列出这个接入点上的模型。推荐的那个默认勾选；多勾几个，就作为后备一起加上。
- **列表**主模型在前。每一行写明哪些 agent 能跑在它上面，按接入点的协议推导：
  - Anthropic 兼容的接入点上能跑 Claude Code 和 pi；
  - 有 Chat Completions 的地方能跑 Codex（经转换层）；
  - octos 哪里都能跑。

  按钮有 **设为主模型**、**测试**、**删除**。删除要点两次；如果没有别的 provider 用同一个 key，删除时也会删掉这个 key。
- **Key。**
  - 在 macOS 上，key 存进 octos 读取的钥匙串条目（服务 `octos`），账户是 OctoBuddy 自己的 `<KEY_ENV>::octobuddy`，不会碰到 OctoSense 给同一个供应商存的条目。配置文件里只写标记 `keychain:<账户>`。
  - 在其他系统上，key 直接写在配置文件里。
  - 配置文件权限是 `0600`。
  - Key 通过标准输入交给钥匙串（`security -i`）和测试（`curl -K -`），从不出现在命令行上。
  - Agent 拿到的仍然是占位符，由 OctoBuddy 的代理在上游加上 key。相比之下，Cindy 会把 key 本身交给 pi。
- **Agent 自己的登录。**页面会显示 Claude Code（`claude auth status`）和 Codex（`codex login status`）有没有登录，但不显示登录的是谁。没选 provider 时，agent 就用它自己的登录。
- **还没做：**在页面上登录（目前只提示要运行的命令）；目录里没有的供应商的自定义接入点；从供应商拉取模型列表。
- **已验证：**2026-10-02 在 macOS 上，用隐藏窗口、单独的数据目录和一份假的 OctoSense 配置（`OCTOS_APP_CORE_DIR`）验证：
  - 页面读出了 OctoSense 的配置，并提示可以导入；
  - 选「OctoBuddy 自己的」后，向导添加了 `zai-coding/glm-5.3` 和 `glm-5.3-flash`。配置文件权限是 `0600`，里面只有钥匙串标记，key 进了钥匙串；
  - 用假 key 测试，显示了供应商返回的 `401`，key 被遮住；
  - 「设为主模型」调整了顺序，「导入」补上了缺的那个；
  - 删除全部行后，钥匙串条目也被删掉，OctoSense 的配置没有变化。

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
