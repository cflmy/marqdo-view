# PR-04 证据：SSE 执行闭环、7 路由与会话安全

- 日期：2026-11-22
- 范围：`src/server/{mod,routes,security,sse}.rs`、`src/reader/render.rs` 运行表单/
  运行区、`assets/reader.js` SSE 前端、`tests/integration/http_live.rs`、
  `tests/security/`、浏览器套件
- 冻结 CLI：`marqdo 3.0.0-beta.1`，SHA-256
  `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f`
  （真实 run 证据 `real-run.log`，run 前后 SHA 不变）
- 产品规则：SSE 无轮询；运行唯一入口 `POST /api/runs`；单任务；取消唯一终态

## 1. 路由与安全

| 路由 | 验证 |
|---|---|
| `GET /` / `GET /file` | 发放 HttpOnly/SameSite=Strict Cookie + CSRF meta；重复打开 10 次无 `run` 调用（`pages_issue_a_session_cookie_and_never_execute_the_document`） |
| `GET /assets/...` | 唯一 CSS/JS、immutable、CSP/`nosniff`；未知资产 404 |
| `GET /api/documents` | `marqdo.view-documents` v1 只读结构；旧 `/api/document`、`/api/health`、`/api/run` 均 404 |
| `POST /api/runs` | 202 + run_id/events_url/cancel_url；无 Cookie/CSRF/Origin、错 Origin、非 JSON、超大 body 全部拒绝且不启动进程 |
| `GET /api/runs/{id}/events` | `text/event-stream`、逐帧 flush、心跳、Last-Event-ID 重放/伪造游标 snapshot、跨会话 404、无会话 401、>2 连接 429 |
| `POST /api/runs/{id}/cancel` | 幂等、唯一 `cancelled` 终态、跨会话 404、不新增进程 |

浏览器测试还断言：`Origin`/`Sec-Fetch-Site` 跨站拒绝、`HttpOnly`+`SameSite=Strict`、
CSP 无 `unsafe-inline`/`unsafe-eval`、symlink 出根被拒、Markdown 不能注入
`<script>`/事件处理器。

## 2. SSE 生命周期（真实事件序列）

`tests/integration/http_live.rs`：

- `explicit_run_streams_sse_accepted_running_and_terminal_with_verified_trace`：
  accepted → running → succeeded；`id` 严格递增且与 `data.seq` 一致；终态含
  CLI result 与 `trace.status=verified` 事件；
- `last_event_id_replays_only_newer_events_and_old_cursor_gets_snapshot`：重放
  只发更新事件；伪造游标得权威 snapshot；
- `cancel_is_idempotent_and_yields_one_terminal_state`：两次 cancel 均 200，
  SSE 只有一个 `cancelled` 终态且 <8s 结束；
- `sse_heartbeats_keep_a_slow_job_connected`：长任务期间收到 `: heartbeat`，
  任务仍成功；
- `at_most_two_sse_observers_per_run`：第 3 个观察连接 429；
- `duplicate_request_id_is_idempotent_and_busy_or_conflict_never_starts_a_second_process`：
  同 ID 同 body → 同 run_id；busy/conflict → 409，不启第二进程；
- `sse_and_cancel_are_scoped_to_the_owning_session`：跨会话读取/取消 404；
- `document_without_main_is_rejected_without_running_another_function`：400
  `view.no_entry_function` 且无 run 调用。

## 3. 浏览器（真实 Chromium 131，七套视图宽度）

`browser.log` 为 `MARQDO_BIN=062cb56e… scripts/test-browser.sh` 的真实输出，
三个项目全部 PASS，包括：

- run 对话框 → 命令预览 → 确认 → **SSE 终态 succeeded**（无整页重载）；
- 运行区、Trace 事件列表、stdout 文本、原文 `.mq-run-chip` 锚点；
- 请求记录中没有状态轮询 API；
- 清理隐藏运行区且不重载页面；
- 2048×1133 / 1664×920 / 1440×900 / 1024×768 / 768×1024 / 390×844 无横向溢出、
  Dock 在视口内、无外部请求、无 console error、无 JS 可读、打印隐藏交互；
- 深色/暖纸主题切换并持久化，无 unsafe-inline/eval。

截图：`welcome-2048x1133.png`、`control-flow-2048x1133.png`、
`control-flow-390x844.png`、`control-flow-run.png`。

## 4. 真实冻结 CLI 端到端

`real-run.log`：启动 `target/release/marqdo-view examples/control-flow`（使用
冻结 CLI），脚本 `scripts/acceptance-run.py` 完成 GET 页面 → POST /api/runs →
读取 SSE 终态：`succeeded`、`trace.status=verified`、**285 条 Trace 事件**。
截图与日志均来自该实现；CLI SHA 在前后保持不变。

## 5. 测试计数（本 PR 合并时）

```
cargo test --locked --offline
unit 63 + contract 41 + integration 40 + security 11 = 155 passed / 0 failed
cargo clippy --all-targets --offline -- -D warnings   # clean
```

## 6. 已知边界

- 首版不提供 net/process/env/plugin/provider/kernel 授权 UI；请求会被拒绝并提示
  使用终端 CLI（`view.run_denied`/`view.run_invalid`）。
- `project` 模式需要显式 read/write glob 并二次确认；它不是 OS 沙箱。
- SSE 只推送 View 生命周期与终态数据，不推送逐字 stdout/Token 流（RFC §6.1）。
