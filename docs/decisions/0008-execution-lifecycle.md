# ADR-0008：单任务执行生命周期、取消竞态与 Trace 缺失语义

- 状态：Accepted
- 日期：2026-11-22
- 关联：`docs/architecture/RFC-VIEW-0002.md` §6.3–§6.7；PR-03/PR-04

本 ADR 固定 RFC 留给实现选择的几个边界，避免后续 UI/测试出现两套解释。

## 1. 单活动任务与接受顺序

- 每个 View 服务进程**最多 1 个活动 CLI 任务**；第二个请求返回 `409 run.busy`
  （`view.run_busy`），不排队、不并发执行。
- `POST /api/runs` 的接受顺序：schema/大小/路径 → `client_request_id` 去重 →
  单任务门禁 → **只读 `check --emit` + Analysis 字节摘要比对** → 目标/参数解析 →
  创建 `accepted` 任务 → 后台线程 → `202`。
- 去重键是 `client_request_id` + **原始请求体字节的 SHA-256**：
  - 相同 ID + 相同字节 → 返回同一 `run_id`（`reused=true`），**不启动第二次进程**；
  - 相同 ID + 不同字节 → `409 Conflict`（`view.run_conflict`）；
  - ID 对应任务已过 TTL → 明确错误（`view.run_invalid`），不默认重跑。
- 去重/活动任务/任务表由同一个 Controller 锁保护；静态 `check` 在锁内完成，
  保证两个并发请求不会同时越过单任务门禁。`check` 不执行目标程序。

## 2. 状态机与终态唯一性

- 状态：`accepted → running → {succeeded, failed, cancelled, timed_out}`，
  `cancel_requested` 不是终态，`unavailable` 仅表示进程无法启动/环境不可用。
- `running` 只在内核进程**成功 spawn 之后**发布（进程适配器提供 spawn hook）。
- 终态由 Controller 在线程内、持有 job 锁时发布；发布前检查 `log.terminal()`，
  所以任何任务最多一个终态事件。取消/超时/自然退出竞态都走同一路径。

## 3. 取消与退出竞态

- 取消 = 幂等 POST 设置取消标志并发布 `cancel_requested`；执行线程负责终止
  进程组（先 SIGTERM，约 2s 后 SIGKILL）与唯一终态。
- 若进程在取消请求后自然退出：
  - 退出码为 0 且 `--json.success=true` → 允许 `succeeded`（RFC §6.6 竞态条款）；
  - 其他情况（被信号终止、非零退出、JSON 缺失/失败）→ `cancelled`。
- 这一判断依赖“取消请求”标志而非仅依赖适配器是否发出了 SIGKILL，因为
  SIGTERM 可能在 2s 宽限期内就让进程退出。
- 页面关闭/SSE 断开**不取消**任务；取消只能显式 POST。

## 4. Trace 缺失/无效与运行成功的关系

- 运行成功与否以 CLI `--json.success` + 进程退出状态为准；`trace_status` 独立：
  - `verified`：Trace v2 通过 `analysis_digest`/`root_id`/`module_id`/span 校验；
  - `unavailable`：CLI 未写 trace（例如失败路径或未请求）；
  - `invalid`：Trace 存在但 schema/摘要/事件绑定不合法。
- **Trace 缺失或 invalid 不把成功运行改判为失败**，但 UI 必须明确显示
  “Trace 不可用/无效”，不得绘制执行侧注、不得伪造事件，也不得宣称完整观测。
- 运行后源码字节变化 → 结果 `stale=true`，Trace 即使曾校验通过也不得锚定当前
  正文；前端仍可显示结果，但标注失效。

## 5. 预算与有界性

- 入口固定预算：`--timeout`=View 墙钟（默认 30s，最大 120s，`--run-timeout`
  可配置且与 Core 参数一致）、`--fuel 1000000`、`--max-output 1048576`、
  `--trace-max-events 10000`、`--trace-max-bytes 4194304`。
- View 侧再限制：stdout/JSON ≤4 MiB、stderr 保留 ≤64 KiB（继续排空管道避免
  子进程阻塞）、Trace 文件 ≤4 MiB；超限以 `view.output_limit` 失败终态报告。
- 每次执行使用独立 0700 临时目录，Trace 读出后随临时目录删除；项目目录中不写
  `.view-*` 运行产物。

## 6. 事件日志与 SSE

- 每任务事件日志保留最近 128 条 + 终态快照；序号严格递增，SSE `id` 与
  `data.seq` 一致；心跳是注释不占序号。
- 快照使用**新的序号**（不重复旧 id），保证客户端去重逻辑不会误丢弃快照。
- 这些决定在 PR-04 的 HTTP/SSE 层消费；Controller 本身不实现 HTTP 轮询。
