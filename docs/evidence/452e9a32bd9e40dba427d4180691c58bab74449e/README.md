# rc.4 验收证据（已生成）

本目录绑定冻结 CLI 源码 SHA `452e9a32bd9e40dba427d4180691c58bab74449e`
（`marqdo 3.0.0-rc.4`）。

- 归档 SHA-256：`sha256:9c3f166b5e5b8840d39d10019074e4e1c42432fc23e26c69670e64fdcf4e15d8`
- 二进制 SHA-256：`sha256:a7306962689c635445a59043971ea2eb09298e01feeee81f0ba07b1f3e10306e`
- 平台：`linux-x86_64-glibc2.39`
- 期望产物：`manifest.json`（schema `marqdo.view.acceptance`，`schema_version: 1`）
- 门禁：G01–G12（见 [`docs/ACCEPTANCE.md`](../../ACCEPTANCE.md)，
  schema 与状态含义见 [`docs/evidence/README.md`](../README.md)）

## manifest 契约（`marqdo.view.acceptance` / `schema_version: 1`）

必填身份字段：`view_commit`、`view_binary_sha256`、`view_tool_bundle_digest`、
`cli_source_sha`、`cli_archive_sha256`、`cli_binary_sha256`、`platform`、
`generated_utc`、`gates`、`unverified`（完整示例见
[`docs/evidence/README.md`](../README.md)）。

门禁 id 与状态：`G01_no_legacy`、`G02_build`、`G03_install`、`G04_static`、
`G05_run`、`G06_stream_snapshot`、`G07_binding_stale`、`G08_negative`、
`G09_security`、`G10_export`、`G11_evidence_ci`、`G12_docs`；
每项 `state ∈ {PASS, FAIL, UNVERIFIED}`，并带 `command`/`exit_code`/`log`。
`unverified` 列出所有非 PASS 门禁；`UNVERIFIED`/`PARTIAL`/`WARN` 不得读作或写成 PASS。

**当前状态：`manifest.json` 已由真实命令生成，`result: PASS`（G01–G12 全部 PASS，含远端 Woodpecker #10 与同 SHA 校验；`extras.browser: PASS`）。** manifest 只能由真实命令重新生成，禁止手写、复制历史 manifest 或从别处伪造：

```bash
export MARQDO_BIN=/abs/unpacked/marqdo-3.0.0-rc.4-x86_64-unknown-linux-gnu/marqdo
scripts/frozen-beta.sh verify "$MARQDO_BIN"
scripts/acceptance.sh
```

`scripts/acceptance.sh` 会把 manifest 写入本目录，并把同一内容写到
`dist/acceptance/manifest.json`。本目录 `logs/` 保存该次 manifest 引用的真实日志。
任一 P0 门禁 `FAIL` 或 `UNVERIFIED` → 0.3.0 不发布；`UNVERIFIED` 不得写成 `PASS`。

历史 0.2.0 证据位于其他目录（`../2c7c4f31e7a388d3fcfa717a77067e696ed13854/`、
`../issue-2/`、`../issue-3/`、`../PR-01..PR-06/`），只作历史记录，
不能替代本目录的 rc.4 门禁。
