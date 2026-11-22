# 验收证据

本目录保存**由真实命令生成**的验收证据，命名 `<CLI-SOURCE-SHA>/manifest.json`：

- `view_source_sha`：生成证据时的 View Git SHA。
- `view_binary_sha256`：发布 bundle 内 `bin/marqdo-view` 摘要。
- `cli_binary_sha256_before/after`：冻结 CLI 在验收前后的 SHA-256，必须相同。
- `cli_source_sha`：CLI 发布归档 `SOURCE_SHA`。
- `tool/analysis/trace_protocol`、`target`、`archive_sha256`。
- `gates`：`tools/acceptance.sh` 每一步的真实退出状态。
- `tests`：contract/browser/export/security 的 PASS/FAIL/SKIP。

生成方式（不要手写/篡改）：

```bash
MARQDO_BIN=/abs/path/to/frozen/marqdo tools/acceptance.sh
```

`manifest.json` 在验收的最后一步写入；若某一步失败，脚本以非零退出且不会声称 PASS。
CI URL 只有在 `.woodpecker.yml` 真正运行后才可补入；本仓库当前未声称远端 CI 通过。

`PR-03/` 子目录保存浏览器截图证据（非协议证据），说明见其 `README.md`。
