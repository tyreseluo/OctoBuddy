# Loops 架构与不变量

> 外环（Lead）与内环（inner loop）的运行契约。改 `src/orchestrate.rs`、`src/plan.rs`、
> `src/handoff.rs`、`src/dispatch.rs`、`src/mcp.rs`、`src/persist.rs` 前先读本文；
> 与代码冲突时以代码为准，但必须同步修正本文。

## 通道与协议

外环驱动内环有两条等价通道：MCP 工具（`octobuddy_plan` / `octobuddy_send` /
`octobuddy_review`）与 fenced 块（无 MCP 的 agent 用）。两者读同一份解析代码。

**不变量 1：派发要有确认信号。** MCP 路径必须返回逐项结果：
- `octobuddy_plan` → `{ok, started, held}`：哪些切片现在开工、哪些等第几 wave；
- `octobuddy_send` → 每条消息的 `mode` 与 `queued_as`（队列 id），直接进入运行中
  turn 的消息标 `delivered:"now"`。
外环只能依据这些信号判断"派成了"；解析失败/未知 slug 是工具错误，当轮修复。

**不变量 2：终态按同一通道验收。** fenced 块与 MCP 工具产生完全相同的副作用
（`apply_reply` 是唯一入口）；外环不能用 subagent 的结果冒充内环完成。

## 任务卡与交接（`src/handoff.rs`）

- 外环写任务卡（Goal / Files / Facts / Done when），不写 Given/When/Then（Rust 的
  agent-spec 合同除外）。
- 宿主补上下文：Files 的定义行号表、`read` 交接的已读片段（≤6k，按开工时文件内容）、
  项目记忆（≤1k，去掉估算校准）。
- **不变量 3：`read` 只读项目内。** 绝对路径、含 `..`、canonicalize 后不在项目目录
  内的条目一律拒绝，不回退。
- **不变量 4：Files 即边界。** 越界改动的文件以 `[files]` 行报告外环；裸文件名
  （README、Makefile）也是合法边界。卡片没写 Files 才不检查。
- 汇报四行（status / verified / decide / notes），超 1,200 字截断并指向全文路径；
  没汇报时转发回复末尾 ≤2,500 字，标 forwarded。

## 队列与消息（`src/dispatch.rs`）

每个内环一条 `Line`：一个运行中的 turn + 一个等待队列。三种模式：
- `queue`：排在队尾，消息带 id（`q7`），消费前可 cancel/replace/merge；
- `steer`：并入运行中的 turn（下一步读取）；未被读取的回到队列；
- `interrupt`：终止当前 turn（octos 丢弃含输入在内的整个 turn），新消息排到队首，
  是下一条被读到的。

**不变量 5：忙不丢消息。** 目标在跑时消息入队并返回 id；agent 死亡时未完成的
delivery 回队（`refused` + `inflight`）。

## 报告节奏（`maybe_review`）

- 就绪报告在 `BATCH_SECS`（120s）内合批；外环忙则进外环队列占一个 Reports 槽。
- **不变量 6：阻塞即醒。** 首行为 `status: question` / `status: blocked` 的汇报
  跳过合批窗口立即送达；forwarded 尾巴不算阻塞。
  `status: question` 的回合还跳过检查与自修（它没有可检查的成果；点名要提交的
  commit 照做）——问题不排在检查后面。
- wave 门：后面的 wave 等前面全部 accept/close；无结论时 `verdict_nudge` 提醒一次。

## 持久化（`src/persist.rs`）

**不变量 7：等待的状态必须活过重启。** 外环队列、未读报告（unreported）、notes、
held wave、每条 Line 的队列（含 id）、inflight、pending、person_tasks 都随 store
保存；重启后切断的 turn 从其消息重跑，队列 id 不重号（`seen_id`）。

## 项目地图（`.octobuddy/knowledge/MAP.md`）

每次请求前与每次验收后刷新；首条请求同步写（外环马上要读）。总量封顶
`MAP_TOTAL`（12k）：地图是省上下文的，不是新的上下文负担。
