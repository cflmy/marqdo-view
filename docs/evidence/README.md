# 验收证据

本目录保存**由真实命令生成**的验收证据。主证据命名 `<CLI-SOURCE-SHA>/manifest.json`：

- `schema`/`schema_version`：`marqdo.view-conformance` v1（View 自有，非 CLI 协议）；
- `view_source_sha`：生成证据时的 View Git SHA；
- `view_binary_sha256`：发布 bundle 内 `bin/marqdo-view` 摘要；
- `cli_source_sha`：CLI 发布归档记录的源码 SHA；
- `cli_binary_sha256_before/after`：冻结 CLI 在验收前后的 SHA-256，必须相同；
- `tool_protocol`/`analysis_protocol`/`trace_protocol`、`target`、`archive_sha256`；
- `gates`：`scripts/acceptance.sh` 每一步的真实退出状态
  （fmt/clippy/test/build/bundle/archive_install/tool_list/view_help/verify/export/
  examples_check/browse/real_run/export_suite/no_exec_suite/security_suite/browser/
  core_sha）；
- `tests`：contract/examples/export/no_exec/security/browser 的 PASS/FAIL/SKIP。

生成方式（不要手写/篡改）：

```bash
MARQDO_BIN=/abs/path/to/frozen/marqdo scripts/acceptance.sh
```

`manifest.json` 在验收最后一步写入；任一步失败时脚本非零退出且不会声称 PASS。
冻结 CLI 的 `--version`/SHA 与发布归档必须一致（`scripts/verify-frozen-cli.sh`），
CI 证据只在 `.github/workflows/ci.yml` 真正运行后才可补入；本仓库当前**未声称**
远端 CI 通过。

分 PR 证据目录（人工/截图/日志，非协议证据）：

- `PR-01/`：Tool v1 安装/分发/篡改拒绝实测；
- `PR-02/`：Reader 结构与 `/api/documents` 只读面；
- `PR-03/`：执行层 request/process/controller/events 测试；
- `PR-04/`：SSE 闭环、7 路由、会话安全、真实 Chromium 与真实冻结 CLI run；
- `PR-05/`：Trace 绑定/标注/stale 与浏览器回归截图；
- `PR-06/`：示例 check/call/run、零执行哨兵、一键验收日志与发布包身份；
- `0001-pr03/`：RFC-VIEW-0001 时期的浏览器截图（历史归档）。
