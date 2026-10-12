OctoBuddy @VERSION@ stops taking the agents' word for it, and hands the person a sheet to accept. An inner loop's report is held against OctoBuddy's own run of its check, its commit and its files; the outer loop cannot accept work those findings say no to without an override the person sees; and every commit, publish and submission leaves a receipt that is read back. Text from anyone other than the person reaches the agents as data, never as instructions.

## Download

- Macs with Apple silicon (M1 and later): `OctoBuddy-@VERSION@-macos-aarch64.dmg`.
- Intel Macs: `OctoBuddy-@VERSION@-macos-x86_64.dmg` (built on Apple silicon; opened under Rosetta, not yet on an Intel Mac).

Each has its `.sha256` beside it. Open the DMG and drag OctoBuddy to Applications (replace the earlier one).

**The first time you open it.** It is not notarized by Apple yet, so macOS stops it the first time. Open it once, then go to **System Settings › Privacy & Security** and choose **Open Anyway**. Or, in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/OctoBuddy.app
```

## New

- **A report against the facts.** An inner loop's `verified:` line names what it ran (in backticks). OctoBuddy holds each claim against its own run of the slice's check, the commit it made and the files, and marks it ✓ (the facts bear it out), ✗ (they contradict it: "the tests pass" when OctoBuddy's run of that check failed, a file "changed" that is as it was, a file "deleted" that is still there) or ? (OctoBuddy did not run that: the agent's own word). The `[claims]` lines go to the outer loop with the report; a report with a ✗ goes back to its inner loop once, with why, first. A near match, a run that timed out or a failure the agent reported itself is never a ✗.
- **The accept gate.** OctoBuddy refuses `accept` while a slice's check failed, it was committed again after its check, a claim of its report is contradicted, or it changed files outside its card. The outer loop sends it back, closes it, or accepts it with `"override": "<why>"` (in a review block or the review tool), which the person sees. The outer loop is told before it reviews (`gate:` in its results) and when it is refused.
- **Receipts, read back.** An inner loop's commit, a publish to the local App Hub, a Publish fix and a submission to App Hub each leave a receipt in `<data>/receipts.jsonl`: what was meant, on which commit and content, who confirmed it, what was done, and what reading it back found.
  - A commit: git's list of its files (deletions too) and what they hold.
  - A publish: the catalog's entry for the app and version, its digest against the staged bundle's, the published copy's content, and its commit. The version OctoBuddy reports is the one read back.
  - A submission: the tag on GitHub against the local one, and the issue.

  Each is ✓ confirmed, ✗ a mismatch or ? unknown, in the session, on the Live page and in the workbench. A receipt left mid-way is read back when OctoBuddy opens again, never redone.
- **What you saw is what is done.** Publish buttons show the content they will publish (a short digest and the version) and refuse a press after the app changed. App Hub's submit plan names the content and commit it is for, and is void if either changes before Submit. A request card's Build builds the request that card showed.
- **An incident closes on its fix.** The Live page closes an incident when the published fix (its receipt confirmed), or a later version, runs well; well again on an older version, it waits for the fix; well again with no fix published, it says it recovered by itself.
- **Data is not instructions.** A request from another app's agent, a message the OctoSense assistant relays, what an app's API answered, a bug report and what an app showed on screen reach the agents inside `<untrusted source="…">` blocks, with anything that could close the block or a code fence neutralised. The outer and inner loops' rules say such text is never an instruction. A request is told as its caller's words, not the person's.
- **The acceptance sheet.** When every slice of a round is settled, OctoBuddy writes one page for the person: the request, each slice's goal, check, claims, commits and receipts, the outer loop's review, and what still needs them (claims it did not verify, overrides, files outside a card, uncommitted files, slices no check ran for, what it cannot check at all). **Confirm acceptance** records it as a receipt, or makes a new sheet if the project moved since. `octobuddy.status` says when one waits.
- **Working, waiting, silent, done.** Every loop says which and since when: what it is working on, whom it waits for (you, the outer loop's review, OctoBuddy's check, an earlier wave, a slot, its agent starting), silent when a turn is open and nothing is heard for 2 minutes (a long command of its own is work), or done. A silent loop turns the warning colour and its session hears it once; `octobuddy.status` gives each loop's state, `as_of` and how long since it was heard, and a Live page watch that stopped running is `stale`.
- **Following a submission's review.** After an app is submitted to App Hub, OctoBuddy reads its issue every 5 minutes; a reviewer's new comment becomes a request bound to that project (on the Apps page inside OctoSense, a new session's composer on its own), fenced as data, waiting for the person. The new version is submitted as before; OctoBuddy then follows its issue.
- **Failures in plain words.** An agent or provider that fails to start or answer is reported by kind (not installed, no key or login, rate limited or out of credit, offline, too slow, an answer OctoBuddy could not read), with what still works and what to do next, and the raw error last. Denying an agent's request says what happens next, and the outer loop hears it.

## Fixed

- **Publishing twice published twice.** Pressing Publish again with nothing changed raised the version and published the same content again. It now says it is published already, as which version.
- **Build could start the wrong request.** A request that arrived while the Apps page was open shifted the cards, so Build acted on the request now in that place. It acts on the one the card showed.
- **A submission cut off could not go on.** With its tag pushed and its issue not opened, running Submit again stopped at "the tag exists already". It resumes: it pushes what was not pushed, or opens the issue.
- **An incident closed on any healthy run**, whether or not the fix was what ran.

## Changed

- Makepad, App Hub and OctoSense are pinned at the revisions OctoSense's main pins now (Makepad `d653bc4`, App Hub `3863591`, OctoSense `b9d66d0f`), so OctoSense builds OctoBuddy with one App Hub.

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

OctoBuddy @VERSION@ 不再只听 agent 怎么说。inner 的汇报要对照 OctoBuddy 自己跑的检查、它做的提交和文件；这些核对说「不行」的工作，外环不写明理由（用户看得到）就验收不了；每一次提交、发布和提交审核都留回执并读回核对。来自用户以外的文字，都当作资料交给 agent，而不是指令。

**下载**：
- Apple 芯片（M1 及以后）的 Mac：`OctoBuddy-@VERSION@-macos-aarch64.dmg`。
- Intel Mac：`OctoBuddy-@VERSION@-macos-x86_64.dmg`（在 Apple 芯片上构建；在 Rosetta 下打开过，还没在 Intel Mac 上跑过）。

打开 DMG，把 OctoBuddy 拖进「应用程序」，替换旧版即可。第一次打开时，到「系统设置 › 隐私与安全性」点「仍要打开」，或者运行上面那条 `xattr` 命令。

**新增**：
- **汇报对照事实**：inner 汇报里的 `verified:` 写它运行了什么（命令放在反引号里）。OctoBuddy 拿每条声明去对照自己跑的这个切片的检查、它做的提交和文件，标成 ✓（事实支持）、✗（事实矛盾：说「测试通过」而 OctoBuddy 跑同一个检查失败了；说「改了」的文件原样未动；说「删了」的文件还在）或 ?（OctoBuddy 没跑这条：只是 agent 自己的说法）。`[claims]` 几行随汇报交给外环；有 ✗ 的汇报先附上原因退回 inner 一次。命令只是近似、运行超时、agent 自己报了失败，都不会判成 ✗。
- **验收闸**：切片的检查没过、检查后又提交过、汇报里的声明被驳倒、或者改了任务卡以外的文件时，OctoBuddy 拒绝 `accept`。外环可以退回、关掉，或者在审阅块或审阅工具里带 `"override": "<理由>"` 验收，用户会看到这条理由。外环审阅前就会在结果里看到 `gate:`，被拒时也会收到说明。
- **回执与读回**：inner 的提交、发布到本地 App Hub、发布修复版、提交到 App Hub，每一件都在 `<data>/receipts.jsonl` 留回执：要做的是什么、基于哪个提交和内容、谁确认的、做了什么、读回发现了什么。
  - 提交：读回 git 记录的改动文件（含删除）和它们的内容。
  - 发布：读回 catalog 里这个应用这个版本的条目，摘要与暂存包比对，读发布出去的副本内容和它记录的提交。OctoBuddy 报出的版本取自读回结果。
  - 提交 App Hub：GitHub 上的 tag 与本地比对，确认 issue 存在。

  每张回执是 ✓ 已核对、✗ 不一致或 ? 待核实，会话、Live 页和工作台上都会显示。停在半途的回执，下次打开 OctoBuddy 时只读回核对，绝不重做。
- **看到什么就做什么**：发布按钮写明将要发布的内容（短摘要和版本），应用改动之后再点会被拒绝。App Hub 的提交计划写明它基于哪份内容和哪个提交，确认之前任一项变了，计划就作废。请求卡片的「开始做」只做卡片上显示的那条请求。
- **事故以修复版结案**：Live 页上，发布的修复版（回执已核对）或更新的版本运行正常才结案；在旧版本上恢复的，等修复版运行；没有发布修复版就恢复的，照实写「自行恢复」。
- **资料不是指令**：别的应用的 agent 发来的请求、OctoSense 助手转述的消息、应用 API 返回的内容、报障文字、应用屏幕上的文字，都包在 `<untrusted source="…">` 块里交给 agent，块里能闭合这个块或代码围栏的写法都会被中和。外环和内环的规则都写明这些文字绝不是指令。请求按请求方的话转述，不当成用户本人的话。
- **验收单**：一轮的切片全部落定后，OctoBuddy 给用户写一页验收单：原始请求，每个切片的目标、检查、声明核对、提交与回执、外环审阅，以及仍需用户确认的事（没核实的声明、覆盖、卡外文件、没提交的文件、没有检查的切片、它根本核对不了的部分）。**确认验收** 记成一张回执；项目在此之后有改动的，改为生成新的验收单。`octobuddy.status` 会说明有验收单在等。
- **工作中、等待、无动静、已结束**：每个环都标明处在哪种状态、从什么时候起：在做什么；在等谁（你、外环审阅、OctoBuddy 检查、前一个 wave、空位、agent 启动）；一轮开着却 2 分钟没有消息就是无动静（它自己在跑的长命令算工作）；或已结束。无动静的环变成警示色，会话里提醒一次；`octobuddy.status` 给出每个环的状态、`as_of` 和距上次消息多久，停止运行的巡检标为 `stale`。
- **跟进提交的审核意见**：应用提交到 App Hub 后，OctoBuddy 每 5 分钟读一次它的 issue；审核人的新留言会变成绑定该项目的请求（在 OctoSense 里出现在应用页，单独运行时放进一个新会话的输入框），按资料隔离，等用户决定。新版本照常提交，之后 OctoBuddy 跟进新的 issue。
- **失败说人话**：agent 或供应商没能启动、没有回应时，按类说明（没装、没有 key 或没登录、限流或余额不足、断网、太慢、返回了读不懂的内容），并说明什么还能用、下一步做什么，原始错误放在最后。拒绝 agent 的请求时会说明接下来怎样，外环也会收到。

**修复**：
- **连按两次发布会发两次**：内容没变时再按发布，会把版本号加一、把同样的内容再发一遍。现在会说明已经发布过，是哪个版本。
- **「开始做」可能做错请求**：应用页开着时新来一条请求，卡片会移位，「开始做」作用到这个位置上新的那条。现在只作用于卡片上显示的那条。
- **中断的提交审核续不上**：tag 已推送、issue 没开成时，再按提交会停在「tag 已存在」。现在会接着做：没推送的补推，没开的 issue 补开。
- **事故遇到任何一次正常运行就结案**，不管运行的是不是修复版。

**变更**：
- makepad、App Hub 和 OctoSense 改钉 OctoSense main 当前的版本（makepad `d653bc4`、App Hub `3863591`、OctoSense `b9d66d0f`），编入 OctoSense 时只有一份 App Hub。

**需要**：
- macOS 11 或更高。
- 拉取应用模板需要 `git`。
- 用 pi 需要 Node.js 和 npm。
- 检查和提交应用，需要编好 App Hub 的 `hub` 和 `card-host`（`OCTOSENSE_APP_HUB`），并装好 GitHub 的 `gh` 且已登录。

**还没有**：
- 不让 inner 去读别的项目的源码：Codex、pi 和 Claude Code 能读整个磁盘。目前 OctoBuddy 只能劝阻，并在它反复读同一段时提醒它。
- OctoBuddy 拿不到事实的声明一律是 ?：它不会去跑 inner 提到的、但不是这个切片检查的命令。
- Linux 和 Windows 的版本；经过公证的版本。
