# PR-05 证据：Trace v2 绑定、原文标注与浏览器回归

- 日期：2026-11-22
- 范围：`src/execution/controller.rs`（Trace 摘要/`source_id`/stale）、
  `assets/reader.js`（Observed chip 分组）、`src/reader/render.rs`
  （`data-annotation-source`）、浏览器套件与视觉证据
- 冻结 CLI：`marqdo 3.0.0-beta.1`（`062cb56e…`），浏览器测试前后 SHA 不变
- 规范：RFC-VIEW-0002 §4.4、§7.3、§7.4；ADR-0010

## 1. 绑定与降级（测试）

| 场景 | 断言 | 测试 |
|---|---|---|
| 正常运行 | Trace `status=verified`、事件 `source_id` 可锚定、`stale=false` | `explicit_entry_run_streams_...` |
| 源在运行中被编辑 | `result.stale=true`、`trace.stale=true`、所有 `anchor/source_id=null` | `source_edit_during_a_run_marks_the_result_stale_and_drops_anchor_ids` |
| Trace digest 不符 | 运行仍 `succeeded`，`trace.status=invalid`，无事件/无锚点 | `trace_with_a_mismatched_digest_is_reported_invalid_but_the_run_still_succeeds` |
| Trace 截断 | `trace.truncated.reason=event_budget`，结果保留，不声称完整 | `truncated_trace_is_marked_without_claiming_complete_coverage` |
| Trace 缺失 | `trace=null`、`trace_status=unavailable`，不伪造 | `run_without_a_trace_file_succeeds_but_reports_unavailable_trace` |
| 非入口模块事件 | 不映射到入口正文（`source_id=null`），保留模块 id | Controller Trace 摘要/`trace_contract` 校验 |
| UTF-8/span/越界 | 既有 Analysis/Trace 校验测试保持通过 | `tests/contract/trace_contract.rs`、`integration/unicode_spans.rs` |

## 2. 浏览器回归（真实 Chromium 131）

`browser.log`：三个项目全部 PASS。关键断言：

- `observed chips carry trace provenance`：chip 带
  `data-annotation-source="trace"`；
- `static statement anchors carry analysis provenance`：静态锚点带
  `data-annotation-source="analysis"`；
- `at most one grouped chip per anchor`：同锚点多事件合并为单个 chip（>3 条显示
  `×N` 与真实 `title`），不压过正文；
- run 终态/运行区/Trace 列表/stdout/清理与既有检查；
- 2048×1133、1664×920、1440×900、1024×768、768×1024、390×844：无横向溢出、
  Dock 在视口、无外部请求、无 console error；
- 无 JS 可读、打印保持正文、深色/暖纸主题切换与持久化。

截图：`control-flow-run.png`、`control-flow-run-dark.png`、
`welcome-2048x1133.png`、`control-flow-390x844.png`。

## 3. 测试计数

```
unit 63 + contract 41 + integration 43 + security 11 = 158 passed / 0 failed
```

## 4. 未做/偏差

- 未实现 SVG 弯曲连接线；按 RFC §4.4 允许的降级采用内联分组 chip（ADR-0010），
  不画无来源/错位曲线。
- 逐语句变量值/参数值/内存快照：Trace v2 不提供，保持 UNSUPPORTED。
- 人工像素级比对与完整 screen-reader 走查仍为 `UNVERIFIED`。
