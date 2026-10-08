# OctoBuddy — 隐私说明

OctoBuddy 既能跑在 Mac（或 Linux）上，也能跑在 OctoSense 里。它自己不发请求回自家服务器。每发出一个字节，都是去你选的地址——模型服务、npm 仓库、GitHub、App Hub；每保存一段，都在你能删的文件夹里。

## OctoBuddy 在本机保留了哪些东西

OctoBuddy 只有一个数据目录，下文记作 `<data>`。

- **默认：** `~/.octobuddy` — `src/model.rs:790-796`（`fn data_dir`）。
- **覆盖：** 把环境变量 `OCTOBUDDY_HOME` 指向别的路径。

下文都在 `<data>` 之下，除非另注。

### 应用自身

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/state.json` | 项目、会话、每条消息（用户、lead、peer、系统）、重启后排队的待办 | `src/model.rs:798-800`，`src/persist.rs:32-64` |
| `<data>/state.json.corrupt-<secs>` | 解析不通过的 `state.json` 副本：文件先被改名，再从空 store 启动，避免覆盖原文件 | `src/model.rs:741-756` |
| `<data>/appearance.json` | 主题选择（Light、Dark、GitHub Light、…，以及「跟随 OctoSense」） | `src/theme.rs:265-266` |

`state.json` 先写到 `.json.tmp` 再改名，崩溃时不会留下半个文件（`src/model.rs:758-766`）。

### 循环的实时日志

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/live/s-<session>.log` | 每个外环一个：每一轮的工具调用、状态、文本（带 ANSI 颜色） | `src/live.rs:30-46`，`src/live.rs:198-273` |
| `<data>/live/p-<peer>.log` | 每个内环一个（结构同上） | `src/live.rs:43-46` |
| 截断 | 日志超过 4 MiB 只保留最后四分之一 | `src/live.rs:18-19`，`src/live.rs:96-104` |

循环的事件里不会出现 key 或 token（`src/live.rs:11-12`）。

### agent 的自带副本

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/agents/<name>/<version>/` | Claude Code、Codex、pi、octos 测试时的版本 | `src/agents.rs:123-125` |
| `<data>/agents.json` | 每个 agent：用 OctoBuddy 自带的（默认），还是用你自己的 | `src/agents.rs:153-177` |

下载来源：Claude Code、Codex、pi 来自 **npm 仓库**（`src/agents.rs:46-72`），octos 来自 **GitHub releases**（`src/agents.rs:87-94`）。每个 tarball 都要先对一遍固定的摘要：`sha512-`（npm）或 `sha256:`（GitHub），对得上才会出现这个版本的目录（`src/agents.rs:209-224`，`src/agents.rs:245-275`）。pi 用 `npm ci` 并加 `--ignore-scripts`（`src/agents.rs:265-266`）；其他不跑安装脚本。

### AI providers（仅在宿主版）

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/providers/profiles/_main.json` | OctoBuddy 的「设置 › AI Providers」写入的 profile；和 OctoSense 用的是同一种 | `src/own_providers.rs:5-7`，`src/own_providers.rs:27-33` |
| 文件权限 | 写入 `0600` | `src/own_providers.rs:79-83` |

在 OctoSense 里时，此页读的是 shell 自己的 profile，不是这份（`src/providers.rs:222-232`，`src/providers.rs:506-507`）。

### 插件

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/plugins/<id>/plugin.json` | 外部插件的清单 | `src/plugins/mod.rs:19-23`，`src/plugins/mod.rs:187-189`，`src/plugins/mod.rs:239-251` |
| `<data>/plugins/<id>/<command>` | OctoBuddy 每次调用都会跑一次的程序（`tool <name>` / `action <id>`） | `src/plugins/mod.rs:316-358` |

内置插件（OctoSense 应用、应用数据、原生 TUI、应用工厂、生产回路）写在 `src/plugins/`，不在磁盘上留东西（`src/plugins/mod.rs:11-18`，`src/plugins/mod.rs:133-184`）。

### 模板和 design flow

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/templates/octosense-app-templates/` | 模板仓库的 git 克隆；每次打开「新建应用」对话框都拉到 `HEAD` | `src/plugins/new_app.rs:20`，`src/plugins/new_app.rs:55-89` |
| `<data>/design-flow/<commit>/` | OctoScript App Design Flow（文档、`tools/octo`、脚本应用模板），第一次需要时从 `resources/design-flow/` 写出，让只装了 OctoBuddy 的机器也能规划应用 | `src/plugins/design_flow.rs:9-57` |

环境变量 `OCTOBUDDY_TEMPLATES_REPO` 可以换成别的模板仓库（`src/plugins/new_app.rs:55-57`）。

### 经验（lessons）

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/lessons/splash.md` | 外环从自己的运行里学到的 Splash / 应用运行时规则，最多 80 行 | `src/lessons.rs:11-68` |
| `<data>/lessons/orchestration.md` | 同上，针对规划、拆分和选 agent | `src/lessons.rs:11-68` |

一条经验是一句简短的规则，加上触发它的现象和日期与项目 wing；外环在一轮结束后最多保留 3 条（`src/lessons.rs:14`，`src/lessons.rs:37-69`）。

### 应用工厂（仅在 OctoSense）

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/requests.json` | 等你处理的请求（what、for whom），以及它们的状态（`working → declined → preparing → building → published → failed`） | `src/plugins/app_factory.rs:9-15`，`src/plugins/app_factory.rs:119-129` |

### Publisher key（宿主版，提交 App Hub 时）

| Path | 内容 | Code |
| --- | --- | --- |
| `<data>/publisher/<id>.key` | App Hub 的发布者密钥；首次勾选「签名」时生成一次。留在 `<data>`，不进项目 | `src/plugins/app_hub.rs:188-190`，`README.md:96` |

## 每个项目里的 `.octobuddy/`

| Path | 内容 | Code |
| --- | --- | --- |
| `<project>/.octobuddy/knowledge/MAP.md` | 这个项目的文件地图（每行定义在哪里）和已经接受的内容；运行中持续刷新 | `src/handoff.rs:40`，`src/handoff.rs:304-316` |
| `<project>/.octobuddy/docs/` | 两份摘要（`SPLASH-COOKBOOK.md`、`DESIGN-FLOW.md`）和 design flow 自己的文档 | `src/pack.rs:89` |
| `<project>/.octobuddy/context/<session>/` | 每个会话的 pack 和 review 文件，给内环用 | `src/pack.rs:1-9`，`src/review.rs:24` |
| `<project>/.octobuddy/memory/<stamp>-<room>.md` | 项目内的记忆笔记，由 `mempal` 摄入 | `src/memory.rs:43-58` |
| `<project>/.octobuddy/card-loop/` | 生产回路的 watch（`watch.json`）、每次运行的健康记录（`health.jsonl`）、截图，以及 app 自身的存储 | `src/plugins/card_loop.rs:13`，`src/plugins/card_loop.rs:444`，`README.md:174-179` |

`.octobuddy/` 目录会被排除在项目的 `git add` 之外（`src/plugins/new_app.rs:289`，`src/plugins/new_app.rs:563`）。

## 密钥

### macOS：钥匙串

OctoBuddy 把 AI provider 的 key 放进 macOS 钥匙串，不放进 profile：

- service `octos`，account `<KEY_ENV>::octobuddy`（这样 OctoSense 的同名条目不会被覆盖） — `src/own_providers.rs:9-13`，`src/own_providers.rs:23-24`。
- profile 里只放 `keychain:<account>` 标记（`src/own_providers.rs:42-48`，`src/providers.rs:107-117`）。
- key 通过 `security -i` 走标准输入写进钥匙串，绝不出现在命令行参数里（`src/own_providers.rs:51-67`）。
- 删掉一个 `KEY_ENV` 的最后一个 row 时，钥匙串里对应的条目也跟着删（`src/own_providers.rs:70-73`，`src/own_providers.rs:118-122`）。

### 其他平台：放进 profile

非 macOS 上 key 直接放在 profile 里（`src/own_providers.rs:42-45`）；profile 写入时是 `0600`（`src/own_providers.rs:79-83`）。

### 测试连接

向导的「测试连接」会用 `curl -K -` 发一个 token 的请求，key 也走 stdin（`src/own_providers.rs:222-251`）。任何报错信息里出现的 key 都会被替换成 `•••`（`src/own_providers.rs:262-267`）。

### 子进程拿不到 key

子进程（`claude`、`codex`、`pi`、`octos`）连的是 OctoBuddy **自己**的本机代理，地址是 `127.0.0.1:<port>`（`src/claude_proxy.rs:29-43`）。代理做的事：

- 把 `ANTHROPIC_API_KEY` 设为占位符（`octobuddy-proxy`），把 `ANTHROPIC_BASE_URL` 指向自己（`src/claude_proxy.rs:20`，`src/claude_proxy.rs:71-91`）。
- 给子进程设置 `DISABLE_TELEMETRY=1`、`DISABLE_ERROR_REPORTING=1`、`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`（`src/claude_proxy.rs:87-89`）。
- 启动前 **清掉** 父进程传入的 `ANTHROPIC_AUTH_TOKEN`、`CLAUDE_CODE_OAUTH_TOKEN`、`CLAUDE_CONFIG_DIR`、`ANTHROPIC_BASE_URL`、`ANTHROPIC_API_KEY`（`src/claude_proxy.rs:97-99`）。
- 转发每个请求用的是 `curl -K -`：真实的 key、body 文件（`0600`）和请求头都走 stdin，不进 argv（`src/claude_proxy.rs:160-185`，`src/claude_proxy.rs:150-159`）。
- Codex 用 Chat Completions 的 provider 时，会做 Responses ↔ Chat Completions 的翻译（`src/claude_proxy.rs:268-357`，`src/responses_bridge.rs`）。

环境变量 `OCTOBUDDY_BRIDGE_LOG=<file>` 会把「发出什么 / 收到什么」追加到文件，**不含 key**（`src/claude_proxy.rs:325-328`）。

测试里也明确断言：真实 key 不会出现在子进程的 stdout 或 stderr 里（`src/claude_proxy.rs:393-418`）。

## OctoBuddy 都会访问哪些网络

只有你自己选的目标，没有 OctoBuddy 自家的服务器。

| 主机 | 何时访问 | Code |
| --- | --- | --- |
| 你启用的模型服务（Anthropic、OpenAI、MiniMax、DeepSeek、Moonshot、Z.ai / GLM、智谱、阿里云百炼 / 通义千问，或自部署的服务） | 每次对话、每个外环或内环 turn | `src/providers.rs:65-104`，`src/own_providers.rs:18-19`，`src/claude_proxy.rs:160-170` |
| `registry.npmjs.org` | Claude Code / Codex / pi 第一次需要时，按固定版本下载 tarball | `src/agents.rs:46-72`，`src/agents.rs:245-256` |
| `github.com/octos-org/octos/releases/download/...` | octos 第一次需要时（Apple silicon 和 Linux） | `src/agents.rs:87-94` |
| 模板仓库（默认 `github.com/tyreseluo/octosense-app-templates`，可用 `OCTOBUDDY_TEMPLATES_REPO` 替换） | 每次打开「新建应用」对话框 | `src/plugins/new_app.rs:20`，`src/plugins/new_app.rs:55-89` |
| `github.com/<你>/<app>`（项目的公开仓库）和 `github.com/OctoSense-org/OctoSense-App-Hub`（用 `OCTOBUDDY_APP_HUB_REPO` 替换） | 提交 App Hub：`git push` 推送 `bundle/` 的 `v<version>` tag，并开 `Submit <id> <version>` issue | `src/plugins/app_hub.rs:23`，`src/plugins/workbench.rs:342`，`README.md:96-98` |
| 新 app 的 manifest 列出的 host（每个 app 自己访问） | app 启动运行时 | 各 app 的 `bundle/manifest.json` |

App Hub 的 `hub` 和 `card-host` 是外部工具（由 `OCTOSENSE_APP_HUB` 指向），不是 OctoBuddy 的服务器（`src/plugins/app_hub.rs:195-198`，`src/plugins/octosense_app.rs:391-404`）。

## 遥测 / 分析 / 崩溃上报

OctoBuddy **没有这些**。代码里没有 Sentry、Bugsnag、Datadog、OpenTelemetry、Mixpanel，也没有 OctoBuddy 自家的使用统计入口（`grep -rn "telemetry\|analytics\|crash\|sentry\|bugsnag\|datadog\|opentelemetry" src/` 只命中两条无关内容：`src/lead.rs:324` 一句注释告诉子 agent 不要发遥测，`src/model.rs:758` 注释解释 `state.json` 的原子写入）。

Claude Code 子进程会带上 `DISABLE_TELEMETRY`、`DISABLE_ERROR_REPORTING`、`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC`（`src/claude_proxy.rs:87-89`）。Claude Code 还会带上 `DISABLE_AUTOUPDATER=1`（`README.md:52`）。

这些 agent 本身是其他厂商的程序。OctoBuddy 只为 Claude Code 设置了上面这些变量（`src/lead.rs:324-326`）；用你自己的 provider 时，它们的模型流量经过 OctoBuddy 的代理。除此之外，Claude Code（用你自己的登录时）、Codex、pi 和 octos 会发送什么，由它们自己的设置和隐私政策决定，不由 OctoBuddy 控制。

## OctoSense（`octosense-module` 构建）

OctoBuddy 跑在 OctoSense 里时：

- 它是注册好的 OctoSense 模块 — `src/module.rs:11-49`。
- 数据目录仍然是同一个 `~/.octobuddy`（或 `$OCTOBUDDY_HOME`）— `README.md:248`。
- 读的是 shell 的 AI providers，不是自己的（`src/providers.rs:222-232`）。
- 在 OctoSense 里，AI providers 的 profile 在 shell 内核的 core 目录（桌面上是 `<OCTOSENSE_HOME>/octos-home/.octos`；`src/providers.rs:217-224`）。其中的 key 是钥匙串标记，account 属于 shell（`<ENV>::<profile id>`，见 octosense-llm-config 的 `profile.rs`），不是 OctoBuddy 的 `<KEY_ENV>::octobuddy`（`src/own_providers.rs:11-13`）。
- shell 的系统 agent、应用工厂（`octobuddy.request`）、生产回路（`octobuddy.report`）都只在 OctoSense 里注册（`src/plugins/mod.rs:72-78`，`src/plugins/app_factory.rs:9-15`，`src/plugins/card_loop.rs:1-23`）。
- shell 自己系统 agent 的工具策略留在 shell 的 profile 里；OctoBuddy 拷一份给 `octos serve` 用，并删掉那段内容 — `src/providers.rs:412-421`。

OctoSense 里 shell 自己的 AI providers app 是唯一来源；OctoBuddy 的 provider 页在那里是只读的，key 不会被导入（`src/providers.rs:1-13`，`src/providers_view.rs:252-254`）。

## 怎么删 OctoBuddy 的数据

- **OctoBuddy 自己的全部数据：** 删掉 `<data>` 目录（默认 `~/.octobuddy`，或者 `$OCTOBUDDY_HOME` 指向的目录）。
- **项目侧的数据：** 删掉每个项目的 `.octobuddy/`。
- **OctoBuddy 写入 macOS 钥匙串的条目**（`<KEY_ENV>::octobuddy`）：到「设置 › AI Providers」页面里 **Remove** 那个 key 对应的最后一行（它会顺手把钥匙串条目也删掉 — `src/own_providers.rs:70-73`，`src/own_providers.rs:118-122`）；或者手动用 `security delete-generic-password -s octos -a <account>` 删。
- **在 OctoSense 里：** 钥匙串条目归 shell 所有，不归 OctoBuddy，要 shell 自己去删。
- **App Hub 发布者密钥**（仅宿主版，App Hub 用）：删 `<data>/publisher/<id>.key`。
- **卸载：** `rm -rf /Applications/OctoBuddy.app`，再删 `<data>` 和项目的 `.octobuddy/` 文件夹。