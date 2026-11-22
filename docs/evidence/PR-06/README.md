# PR-06 证据：示例、静态导出、发布门禁与文档闭环

- 日期：2026-11-22
- 范围：`examples/*.mq.md`、`scripts/{check-examples,test-no-exec}.sh`、
  `scripts/acceptance.sh`、静态导出控件收敛、最终 ACCEPTANCE/STATUS/README
- 冻结 CLI：`marqdo 3.0.0-beta.1`，SHA-256
  `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f`
- 机器证据（最终）：`docs/evidence/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json`
  （在 PR-06 提交 `0fceb40` 上重跑 `scripts/acceptance.sh` 生成；
  View binary `eb19f796…`、archive `a3fb28c8…`、CLI before/after
  `062cb56e…`；`manifest.pre-commit.json` 为文档定稿前同一代码的预跑结果，
  保留对照）

## 1. §13.3 示例（真实 CLI）

`check-examples.log`：

- 5 个示例全部 `check --emit` 成功；
- `call function-params.mq.md double --arg n=5` → `Int(10)`；
- `run branch-loop.mq.md` → `Int(0)`，Trace 含 `loop.iteration`/`return`；
- `run knowledge-only.mq.md` → exit 1 `sem.no_entry_function`（无隐式替代）；
- `run unsafe-side-effect.mq.md --mode pure` → `permission.denied`，无文件；
- `--mode project --allow-write out --allow-write 'out/**'` → 仅此时写出
  `out/view-sentinel.txt`（内容 `explicit run only\n`）；
- 全程冻结 CLI SHA 不变。

## 2. §13.4 零执行哨兵（真实 HTTP + 导出）

`no-exec.log`：对 unsafe-side-effect 执行
`GET /`、`GET /file`、`GET /api/documents`、重复 reload、`view export` 后：

- `examples/**/*.mq.md`、`marqdo.yaml`、`view.json`、`marqdo.lock` 哈希不变；
- 无 `examples/out/view-sentinel.txt`；
- 导出的 HTML **不含** `data-action="run"`、`mq-csrf-token` 或外链；
- 冻结 CLI SHA 不变。

## 3. 一键验收（§9.4）

`acceptance.log`（真实命令输出）包含：

```
fmt / clippy / test / build / bundle / archive_install / tool_list / view_help /
verify / export / examples_check / browse / real_run / export_suite /
no_exec_suite / security_suite / browser / core_sha   -> 全部 PASS
```

其中：

- `test`：unit 63 + contract 41 + integration 43 + security 11 = 158 PASS；
- `browser`：welcome/control-flow/security-project，真实 Chromium 131，
  EventSource 终态、宽矩阵、主题、无外部请求、无 console error；
- `real_run`：真实冻结 CLI + SSE，`succeeded` + `trace.status=verified`；
- `core_sha`：CLI before == after == `062cb56e…`。

## 4. 发布包

- 版本：`marqdo-view 0.2.0`；
- 包：`dist/marqdo-view-0.2.0-x86_64-unknown-linux-gnu.tar.gz`
  （SHA-256 记录在最终 manifest 的 `archive_sha256`）；
- 从 `.tar.gz` 解包到独立临时目录后经冻结 CLI `tool install` 安装、`view --help`
  分发与真实运行；安装态与本地源码目录解耦。

## 5. 仍未验证（诚实项）

- 人工像素级比对（相对 2048×1133 参考图）与完整 screen-reader 走查：`UNVERIFIED`；
- 远端 CI 未实际运行（配置已提供）；
- Windows/macOS 未构建/未验证。
