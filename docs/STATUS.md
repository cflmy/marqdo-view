# marqdo-view 现状（STATUS）

> 唯一现状来源。其他文档描述设计；本文件只记录**实际完成、证据与限制**。
> 最后更新：2026-11-22（PR-00 完成）

## 版本与基线

| 项 | 值 |
|---|---|
| View 版本 | `0.1.0`（开发中） |
| 目标 CLI 产品范围 | `>=3.0.0-beta.1,<4.0.0` |
| 冻结 CLI 二进制 | `/root/work/marqdo-cli/dist/marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu/marqdo` |
| 冻结 CLI SHA-256 | `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f` |
| CLI 源码 SHA | `2c7c4f31e7a388d3fcfa717a77067e696ed13854`（tag `v3.0.0-beta.1`） |
| 协议 | Tool v1 / Analysis v2 / Trace v2 / markdown profile `marqdo.md.v1` |
| 目标平台 | Linux x86_64 glibc（正式）；其他平台未验证 |
| 视觉基准 | `docs/design/reference-old-view-reader.png`（1664×920） |

## 分阶段进度

| 阶段 | 状态 | 证据 |
|---|---|---|
| PR-00 基线与旧 View 取证 | **完成** | `docs/audits/legacy-render-paths.md`、`docs/design/visual-tokens.md`、`docs/design/state-matrix.md`、ADR-0001/0002 |
| PR-01 Tool v1 安装闭环 | **完成** | `tool.json`、`tools/package.sh`、`tools/install-smoke.sh`、`tests/core_contract.rs`、`tests/no_coupling.rs` |
| PR-02 DocumentModel + Analysis v2 浏览 | **完成** | `src/document.rs`、`src/semantic.rs`、`src/presentation.rs`、`src/render.rs`、`src/server.rs`、`tests/rendering.rs`、`tests/http_live.rs`、`tests/golden/analysis-v2/`（冻结 CLI 生成） |
| PR-03 Reader 视觉重建 | 进行中（基础主题已就绪） | `assets/reader.css`/`assets/reader.js` 单一入口，`tests/asset_integrity.rs`；完整欢迎页/响应式截图待补 |
| PR-04 Trace v2 运行可视化 | **完成** | `src/trace.rs`、`POST /api/run`（CSRF/Origin/确认/并发）、`tests/trace_contract.rs`、`tests/http_live.rs`、`tests/golden/trace-v2/`（冻结 CLI 生成）、ADR-0004 |
| PR-05 Live 安全 + 静态导出 | 未开始 | — |
| PR-06 冻结 CLI 端到端验收 | 未开始 | — |

## 证据与限制

- 旧 View 运行时状态：`LEGACY_RUNTIME_UNVERIFIED`（旧仓库无预编译产物，且任务禁止修改/编译旧仓库；只做只读静态审计）。
- PR-01 起 `marqdo view --help/--version/verify` 由本程序提供；PR-02 起 `marqdo view [PATH]` 提供本机只读浏览。
- `export` 仍返回 `view.unsupported`（PR-05 落地）；运行入口在 PR-04 起可用，但必须经过页面内显式确认 + CSRF 校验，且只允许 `pure` 模式。
- 冻结 CLI 安装烟测通过前不得声称 Tool v1 闭环可用；证据由 `tools/install-smoke.sh` 生成。
- PR-02 的 Analysis golden 全部由冻结 CLI `check --emit` 生成（见 `tests/golden/README.md`）；`examples/diagnostics` 故意 check 失败且没有 golden。
- 已知限制：逐语句变量值/参数值/内存快照/证明状态仍不可用且标为 UNSUPPORTED；`project`/`strict` 运行授权界面、SSE 流式输出、调试器未实现；`export` 未实现；浏览器截图验收在 PR-03/PR-06。
- 运行安全：首版只提供 `--mode pure`；POST `/api/run` 要求同源 Origin（若提供）、CSRF token、`confirm:true`、单活动运行；运行缓存仅保存在进程内，重启即清空。
- 已核对旧视觉资产许可（旧仓库 Apache-2.0）：logo/blossom/feather 可复用，KaTeX 不引入。
- 已知非目标：变量运行值、内存快照、调试器、Prove/Knowledge/Lint 引擎（均标 `UNSUPPORTED`）。
