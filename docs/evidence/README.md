# 验收证据

本目录只保存**由真实命令生成**的机器证据。当前基线：
`marqdo 3.0.0-rc.4`（源码 `452e9a32bd9e40dba427d4180691c58bab74449e`）。
`PASS/FAIL/UNVERIFIED` 一律以 manifest 为准；`UNVERIFIED`、`PARTIAL`、`WARN`
不得写成 `PASS`。缺证据即视为未完成，任一 P0 门禁失败/未证明 → 不发布 0.3.0。

## 目录布局

- `docs/evidence/<CLI-SOURCE-SHA>/manifest.json`：当前契约的机器验收清单
  （当前目录名固定为 `452e9a32bd9e40dba427d4180691c58bab74449e`）；
- `docs/evidence/<CLI-SOURCE-SHA>/README.md`：该次证据的说明/附件索引；
- 历史目录（`2c7c4f31e7a388d3fcfa717a77067e696ed13854/`、`issue-2/`、
  `issue-3/`、`PR-01..PR-06/`、`0001-pr03/`）：0.2.0 旧预发布契约时代的证据，
  **仅作历史记录，不能替代 rc.4 门禁**；
- 分 PR 的人工/截图/日志不是协议证据，只在对应 README 中说明。

## manifest schema：`marqdo.view.acceptance` / `schema_version: 1`

必填字段（按 issue #5 §0）：

```json
{
  "schema": "marqdo.view.acceptance",
  "schema_version": 1,
  "view_commit": "<40 hex>",
  "view_binary_sha256": "sha256:<64 hex>",
  "view_tool_bundle_digest": "sha256:<64 hex>",
  "cli_source_sha": "452e9a32bd9e40dba427d4180691c58bab74449e",
  "cli_archive_sha256": "sha256:9c3f166b5e5b8840d39d10019074e4e1c42432fc23e26c69670e64fdcf4e15d8",
  "cli_binary_sha256": "sha256:a7306962689c635445a59043971ea2eb09298e01feeee81f0ba07b1f3e10306e",
  "platform": "linux-x86_64-glibc2.39",
  "generated_utc": "<UTC ISO-8601>",
  "gates": {
    "G01_no_legacy": {"state": "UNVERIFIED", "command": "...", "exit_code": null, "log": ""}
  },
  "unverified": []
}
```

生成器额外输出（不改变上述语义）：

- `product {name, version, target}`、`cli_version`、`contracts {api_revision:1,
  analysis:3, observation:1}`、`markdown_profile`、`product_range`；
- `result`：`PASS | PARTIAL | FAIL`；
- `gates.G01…G12`：每项 `{state, command, exit_code, log, note?}`，
  `state ∈ {PASS, FAIL, UNVERIFIED}`；
- `unverified`：所有非 PASS 门禁 id 的显式列表；
- `extras`：cargo 测试数、浏览器状态、说明。

`view_tool_bundle_digest` 的计算方式：bundle 内规范化 `SHA256SUMS` 文件字节的
SHA-256（`sha256:` 前缀），字段旁会记录 `view_tool_bundle_digest_method`。

## 门禁状态含义

| 状态 | 含义 |
|---|---|
| `PASS` | 本环境真实命令 exit 0，且产物/日志与声明一致 |
| `FAIL` | 真实命令非 0 或断言失败；阻塞发布 |
| `UNVERIFIED` | 未运行、无法在本环境判定或缺少测试路径；**不得**读作 PASS；阻塞发布 |

G01–G12 的判定命令与通过判据见 [`docs/ACCEPTANCE.md`](../ACCEPTANCE.md)。
manifest 的 gate id 固定为：`G01_no_legacy`、`G02_build`、`G03_install`、
`G04_static`、`G05_run`、`G06_stream_snapshot`、`G07_binding_stale`、
`G08_negative`、`G09_security`、`G10_export`、`G11_evidence_ci`、`G12_docs`。
G11（证据与 CI）只在其实际运行的 CI 环境中可为 PASS；本地 manifest 中保持
`UNVERIFIED`，不能证明远端 CI 通过。

## 生成方式（不要手写/篡改）

```bash
export MARQDO_BIN=/abs/unpacked/marqdo-3.0.0-rc.4-x86_64-unknown-linux-gnu/marqdo
scripts/frozen-beta.sh verify "$MARQDO_BIN"
scripts/acceptance.sh
# → docs/evidence/452e9a32bd9e40dba427d4180691c58bab74449e/manifest.json
# → dist/acceptance/manifest.json（同一内容）
```

`manifest.json` 在验收最后一步写入；任一步 FAIL 时脚本非零退出且不会声称 PASS。
冻结 CLI 的 `--version`/SHA 与发布归档必须一致（`scripts/verify-frozen-cli.sh`）。
`VIEW_EVIDENCE_OUT` 可覆盖主输出路径，但不得用于伪造门禁结果。

## 历史证据声明

0.2.0 时代的 manifest 与 PR 证据使用旧预发布契约与旧 manifest 结构，
其中的 `PASS` 只对其当时的基线有效。它们不得被复制为 0.3.0 证据，
也不得在 README/COMPATIBILITY/CHANGELOG/STATUS/ACCEPTANCE 中作为当前能力证明。

远端 CI 证据只在 `.github/workflows/ci.yml` / `.woodpecker.yml` 真正运行后才可补入；
本仓库当前**未声称**远端 CI 通过。
