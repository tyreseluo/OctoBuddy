# 前后对照：App Hub 准入检查的稳定错误码与修复建议

- 日期：2026-10-12
- 上游仓库：[OctoSense-org/OctoSense-App-Hub](https://github.com/OctoSense-org/OctoSense-App-Hub)（本机检出 `octosense-ws/OctoSense-App-Hub`）
- 基线提交：`386359153ed6377b5f047bd9ed2767a6a2ac2bc7`（main）
- 改动分支：`feat/error-codes-main`，提交 `2f759daf1dbb76bffd4b6a4db46decc3c5bb4822`（`feat(hub): stable error codes and one-line fixes in the gate report`，把旧基线上的 `42a83a4` 重落到当前 main）
- 被测工具：`hub check <bundle> --allow-unsigned`（OctoBuddy 的 `octobuddy-app-check` 经 `tools/octo check` 调用的同一个准入检查）

## 改了什么

准入报告的每条 refusal / warning 附带一个稳定错误码与一行修复建议：文本报告在原有行下方加 `fix [code]: …` 行（原有文字逐字不变），`--json` 的每个 finding 增加 `code`、`hint` 字段（可选字段，旧消费者不受影响）。码表见上游 `docs/ERROR-CODES.md`（53 个现役码，覆盖结构准入、摘要与签名、身份、内容、越界引用、密钥、存储、商店信息、agent 与工具、授权、发布历史，以及新基线新增的应用自带函数、共享组件与组件发布；`hub-host-undeclared` 已退役保留——自新基线起 https 主机只是声明，不再拒绝）。仅供审核者参考的说明性警告（组件能访问什么、由哪些 crate 构建、每次调用都需人批准的工具）不带码，照旧打印。

## 复现步骤

```sh
# 基线与分支各构建一次（仓库内共用 target/）
git -C octosense-ws/OctoSense-App-Hub switch --detach origin/main && cargo build --bin hub   # 存为 hub-before
git -C octosense-ws/OctoSense-App-Hub switch feat/error-codes-main && cargo build --bin hub   # 存为 hub-after
# 五个最小应用包 fixture（docs/evidence/diagnostics/fixtures/<名>/bundle），各触发一类拒绝
for n in digest-mismatch listing-missing listing-no-screenshots host-undeclared grants-invalid; do
  hub-before check fixtures/$n/bundle --allow-unsigned   # 退出码 1
  hub-after  check fixtures/$n/bundle --allow-unsigned   # 退出码 1
done
```

fixture 一览（均在 `fixtures/` 下，打戳后各自制造一处缺陷）：

| fixture | 缺陷 | 触发的码 |
| --- | --- | --- |
| `digest-mismatch` | 打戳后又改了 `main.splash` | `hub-digest-mismatch` |
| `listing-missing` | 没有 `listing.json` | `hub-listing-missing` |
| `listing-no-screenshots` | `listing.json` 的 `screenshots` 为空 | `hub-listing-no-screenshots` |
| `host-undeclared` | 脚本用纯 `http://` 取 `api.example.com` | `hub-asset-external` |
| `grants-invalid` | 清单请求契约没有的能力 `teleport` | `hub-grants-invalid` |

**fixture 调整说明**：`host-undeclared` 原来取 `https://api.example.com`（清单未声明该主机）。新基线起 https 目的地只是面向审核的声明、不再拒绝，该 fixture 在基线上变为通过；按最小改动改为纯 `http://`（唯一剩下的网络类拒绝）并重新打戳，触发的码由已退役的 `hub-host-undeclared` 变为 `hub-asset-external`。fixture 目录名保留，以免打断既有引用。

## 对照输出

### 1. digest-mismatch

之前：

```
dev.evidence.digest 1.0.0 — REFUSED
  [refused] digest: the bundle hashes to e7852470dda5ac8f6050a5704aa4f7d89c94dbb2deaa9d278eef3ec40a5a1b27, the manifest claims 4b46ff70480268719cd7d62691365f2c2b131837ca47bea7c3922d7cd5a21316
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  declarations: capabilities {}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

之后：

```
dev.evidence.digest 1.0.0 — REFUSED
  [refused] digest: the bundle hashes to e7852470dda5ac8f6050a5704aa4f7d89c94dbb2deaa9d278eef3ec40a5a1b27, the manifest claims 4b46ff70480268719cd7d62691365f2c2b131837ca47bea7c3922d7cd5a21316
    fix [hub-digest-mismatch]: run `hub stamp <bundle>` after every edit (tools/octo check stamps first), then check again
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
    fix [hub-unsigned]: sign before submitting: an unsigned release asks the person to trust the hub alone
  declarations: capabilities {}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

### 2. listing-missing

之前：

```
dev.evidence.nolisting 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] listing: no listing.json: a store needs a description, a category, screenshots, a publisher and the platforms it runs on
  declarations: capabilities {}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

之后：

```
dev.evidence.nolisting 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
    fix [hub-unsigned]: sign before submitting: an unsigned release asks the person to trust the hub alone
  [refused] listing: no listing.json: a store needs a description, a category, screenshots, a publisher and the platforms it runs on
    fix [hub-listing-missing]: add listing.json with description, category, screenshots, publisher and platforms (docs/PUBLISHING.md#the-listing)
  declarations: capabilities {}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

### 3. listing-no-screenshots

之前：

```
dev.evidence.noshots 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] listing: no screenshots: at least one PNG in the bundle, named in the listing, is required
  declarations: capabilities {}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

之后：

```
dev.evidence.noshots 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
    fix [hub-unsigned]: sign before submitting: an unsigned release asks the person to trust the hub alone
  [refused] listing: no screenshots: at least one PNG in the bundle, named in the listing, is required
    fix [hub-listing-no-screenshots]: capture a PNG of the running app (tools/octo shot) and name it in listing.json's screenshots
  declarations: capabilities {}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

### 4. host-undeclared

之前：

```
dev.evidence.nohost 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] assets: main.splash contains http://api.example.com/v1"}, fn(r){}) } Label{text: "hi"}  points outside the bundle; ship the asset with the app
  declarations: capabilities {"net"}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

之后：

```
dev.evidence.nohost 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
    fix [hub-unsigned]: sign before submitting: an unsigned release asks the person to trust the hub alone
  [refused] assets: main.splash contains http://api.example.com/v1"}, fn(r){}) } Label{text: "hi"}  points outside the bundle; ship the asset with the app
    fix [hub-asset-external]: ship the asset inside the bundle and reference it bundle-relatively ({{assets}} in a script)
  declarations: capabilities {"net"}, hosts {}, storage 16777216 bytes, agent none
hub: the bundle was refused
```

### 5. grants-invalid

之前：

```
dev.evidence.teleport 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
  [refused] policy: app dev.evidence.teleport requests unknown capability "teleport"
hub: the bundle was refused
```

之后：

```
dev.evidence.teleport 1.0.0 — REFUSED
  [warning] publisher-signature: unsigned: accountability rests on the hub alone
    fix [hub-unsigned]: sign before submitting: an unsigned release asks the person to trust the hub alone
  [refused] policy: app dev.evidence.teleport requests unknown capability "teleport"
    fix [hub-grants-invalid]: the manifest asks for something the contract does not offer: check capabilities, hosts, agent and research against docs/PUBLISHING.md
hub: the bundle was refused
```

## `--json` 同步带码（digest-mismatch 例，节选）

```json
{
 "check": "digest",
 "code": "hub-digest-mismatch",
 "detail": "the bundle hashes to e785…, the manifest claims 4b46…",
 "hint": "run `hub stamp <bundle>` after every edit (tools/octo check stamps first), then check again",
 "severity": "refusal"
}
```

## 结论

- 五个 fixture 的拒绝行逐字不变，退出码不变（均为 1）；新增的只有 `fix [code]: …` 行与 JSON 字段。
- 与上一版对照（基线 `e014fa9`）相比，基线侧自身的两处变化来自上游 275 个提交，与本改动无关：报告末行由 `grants: capabilities {}, hosts {}, storage none, agent none` 变为 `declarations: capabilities {}, hosts {}, storage 16777216 bytes, agent none`；未声明的 https 主机不再拒绝（见上方 fixture 调整说明）。
- 码与修复建议覆盖 OctoBuddy `check_hints` 里准入侧的全部场景（摘要/打戳不符、清单字段与截图缺失、能力或宿主被拒）；运行时 Splash 错误（`widget not found`、`on_render closure failed`）出在 makepad 脚本引擎（另一仓库），不在本改动范围。
- OctoBuddy 侧消费方式：见 README「Checked before review」与本目录 fixtures 的对照。
