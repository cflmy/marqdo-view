# PR-03 证据：执行层（request / process / controller / events）

- 日期：2026-11-22
- 范围：`src/execution/{request,process,controller,events}.rs`、`core_client` 单一进程
  适配器、`--run-timeout`、`DocumentService::static_document`
- 冻结 CLI：`marqdo 3.0.0-beta.1`（`062cb56e…`）；本 PR 只用假核心做确定性进程级
  测试，PR-04/06 再用真实冻结 CLI 做端到端验收
- 说明：本 PR **不切换 HTTP 运行路由**；`POST /api/run`（同步旧路径）暂留到
  PR-04 被 7 路由 + SSE 替换。

## 1. 实现结构

```
src/execution/
  request.rs     # §6.3 请求 schema/大小/路径/mode/grants/参数名校验；
                 # function_id -> CLI 名称（canonical 或唯一 local name）
  process.rs     # 唯一子进程适配器：argv 数组、独立进程组、SIGTERM→2s→SIGKILL、
                 # 有界 stdout/stderr（继续排空避免死锁）、spawn hook、墙钟
  events.rs      # RunState 状态机、run.* 事件、128 环形缓冲、Last-Event-ID 重放/
                 # 快照、SSE 帧、心跳常量
  controller.rs  # 单任务门禁、去重、静态 check+digest 比对、后台执行、
                 # Trace 校验投影、取消、终态唯一性、TTL 清理
```

`core_client` 的 `check`/`--version` 也改为走 `process::run_bounded`，因此
View 只有**一个**子进程实现；`build_run_args`/`build_call_args` 是唯一 argv 构造点。

## 2. 关键测试（`tests/integration/execution.rs`，假核心真实进程）

| 测试 | 断言 |
|---|---|
| `explicit_entry_run_streams_accepted_running_and_succeeded_with_verified_trace` | 事件序号递增；accepted/running/succeeded；Trace `verified`、事件非空、`stale=false`、stdout 来自 CLI JSON |
| `call_target_uses_the_call_subcommand_and_function_name` | argv 为 `call FILE <name>`；FUNCTION 名称由 function_id 精确解析 |
| `same_client_request_id_is_idempotent_and_different_payload_conflicts` | 同 ID 同 body → 同 `run_id`（reused）；不同 body → `view.run_conflict` |
| `second_run_is_busy_while_one_is_active_and_cancel_is_idempotent` | 第二任务 `view.run_busy`；取消幂等；终态唯一 `cancelled`；<10s 结束 |
| `cross_session_observation_is_rejected` | 其他会话订阅/取消 → `view.run_not_found` |
| `stale_analysis_digest_is_rejected_before_any_process_starts` | digest 不符 → `view.run_invalid`，假核心无 run 调用 |
| `project_grants_are_mapped_to_explicit_cli_flags_and_pure_grants_are_rejected` | project 请求 argv 有 `--mode project`/`--allow-read`/`--allow-write`；pure+grants 被拒 |
| `run_without_a_trace_file_succeeds_but_reports_unavailable_trace` | 成功 + `trace=null` + `trace_status=unavailable`，不伪造 |
| `event_frames_use_the_view_run_events_schema_and_numeric_ids` | `marqdo.view-run-events` v1、run_id/seq 一致 |
| 单元测试（events/process/request） | 环形缓冲截断→快照、seq/id 一致、取消终止进程组、glob/参数/包络校验 |

## 3. 实测命令与结果

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --offline -- -D warnings
cargo test --locked --offline
```

结果：unit 58 + contract 41 + integration 42 = **141 passed / 0 failed**。
`--run-timeout` 限制在 1..=120 秒，默认 30 秒（`tests/contract/cli_surface.rs`）。

## 4. 明确未完成

- HTTP 路由/SSE 传输/session Cookie 属于 PR-04；当前 Controller 已提供
  `submit/subscribe/cancel/job_state`，但未暴露公共状态轮询路由。
- 前端函数参数表单与运行区仍在 PR-04 切换；旧同步 `/api/run` 在此期间仍是过渡实现。
- 真实冻结 CLI 的 run/call/Trace 端到端证据在 PR-04/PR-06 记录（本 PR 不冒称）。
