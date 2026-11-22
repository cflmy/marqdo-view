# 验收状态（RFC-VIEW-0001 §18）

> **历史证据说明（2026-11-22）**：本文件的 PASS 是 **RFC-VIEW-0001 时期**
> 的验收结论，用作已实现能力的证据来源，**不是 RFC-VIEW-0002 的发布门禁**。
> 0002 要求 SSE、7 路由、单函数调用、project grants、0.2.0 发行包与新的浏览器
> 矩阵；这些将在 PR-04～PR-06 重新验收并更新本文件（见 `docs/CONFORMANCE.md`）。


> 结论：首个 Beta 的 **P0 门禁全部 PASS**（由真实冻结 CLI + 真实 Chromium 生成，
> 见 `docs/evidence/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json`）。
> P1 视觉/交互门禁大部分 PASS，**U-01 像素级人工比对与 U-04 动态曲线侧注未完成/未实现**，
> 已在 README/CHANGELOG/STATUS 明确标注，不以自动化结果替代人工判断。

## P0 门禁

| ID | 结果 | 证据 |
|---|---|---|
| A-01 无旧 crate 依赖 | PASS | `tests/contract/no_coupling.rs` 依赖/Cargo.lock/源码扫描 |
| A-02 独立新仓库 clean build | PASS | acceptance `build`；无 `marqdo`/`marqdo-cli` 源码依赖 |
| A-03 Tool v1 安装/分发 | PASS | acceptance `archive_install`、`tool_list`、`view_help`；`scripts/install-smoke.sh` |
| A-04 CLI SHA-256 前后相同 | PASS | acceptance `core_sha`；manifest 中 before==after=`062cb56e…` |
| A-05 旧审计与截图基准 | PASS（静态） | `docs/audits/legacy-render-paths.md`（`LEGACY_RUNTIME_UNVERIFIED` 已声明） |
| A-06 单一 Renderer/CSS 入口 | PASS | `tests/contract/no_coupling.rs`、`tests/contract/asset_integrity.rs`、ADR-0001/0002 |
| S-01 Knowledge/Exec/继承 | PASS | `tests/integration/rendering.rs`、`tests/integration/unicode_spans.rs`、冻结 Analysis golden |
| S-02 Emph/Strong/Code 载体 | PASS | `tests/integration/rendering.rs`（4 个 Exec 锚点）、`tests/integration/unicode_spans.rs` |
| S-03 Knowledge 惰性 | PASS | `knowledge_emphasis_never_becomes_an_execution_anchor` |
| S-04 中文/emoji/CRLF/重复语句 | PASS | `examples/unicode-spans` + 冻结 golden + `tests/integration/unicode_spans.rs` |
| S-05 check 失败可读 | PASS | `tests/contract/presentation_states.rs`、浏览器 diagnostics 区 |
| S-06 未知 Analysis major 拒绝 | PASS | `tests/semantic` 单测、`tests/contract/presentation_states.rs`、`verify` 负测 |
| S-07 digest/span 篡改 | PASS | `tests/contract/trace_contract.rs`、`tests/contract/presentation_states.rs` |
| R-01 刷新 10 次不运行 | PASS | `tests/integration/http_live.rs::opening_and_refreshing_never_runs_the_document` |
| R-02 导出纯静态 | PASS | `tests/integration/export.rs`（marker 假核心 + argv 无 run） |
| R-03 用户确认后真实运行 | PASS | acceptance `real_run`（安装态、CSRF、真实 Trace） |
| R-04 trace 不匹配 → STALE | PASS | `tests/contract/trace_contract.rs`、`tests/integration/http_live.rs` |
| R-05 截断/运行错误 | PASS | 真实 truncated/arity golden + 页面状态 |
| R-06 不猜未公开值 | PASS | Trace 模型无这些字段；UI `UNSUPPORTED`（`docs/design/state-matrix.md`） |
| X-01 恶意 Markdown | PASS | `tests/integration/rendering.rs`、`tests/fixtures/security-project` 浏览器 |
| X-02 路径穿越/symlink | PASS | `tests/integration/http_live.rs`、`SafeRelativePath` 单测 |
| X-03 跨站 POST run | PASS | `tests/integration/http_live.rs`（token/Origin/confirm/mode） |
| X-04 无 JS/禁网/file:// | PASS | 浏览器 no-JS/外部请求检查；`scripts/test-export.sh` file:// Chromium |
| X-05 stdout 注入转义 | PASS | `tests/integration/http_live.rs::program_stdout_is_escaped...` |
| X-06 超时/子进程清理 | PASS | `tests/contract/core_contract.rs::timeout_kills_the_process_group...` |
| P-01 文档可复现 | PASS | README / RFC / STATUS / COMPATIBILITY / ADR / 审计 / 证据 |
| P-02 archive clean install | PASS | acceptance 从 `.tar.gz` 解包到临时目录安装并运行 |
| P-03 协议审计 | PASS | `docs/audits/protocol-conformance.md` |

## P1 门禁

| ID | 结果 | 说明 |
|---|---|---|
| U-01 1664×920 视觉 | **PARTIAL** | 截图已采集（`docs/evidence/PR-03/`），几何/颜色令牌/无溢出/无外部资源 PASS；与参考截图的**像素级人工比对未执行（UNVERIFIED）** |
| U-02 多宽度 | PASS | 390/600/900/1280/1664/1920/2560 真实 Chromium：无横向溢出、Dock 在视口内 |
| U-03 键盘/焦点/屏幕阅读器 | PASS（基础） | Tab 可达命名控件、Esc 关闭、Ctrl/Cmd+K 搜索；完整 screen-reader 手工走查未做 |
| U-04 动态曲线侧注 | **PARTIAL** | 运行事件使用**内联 keyed chip**（无 JS/移动/打印均可读）；SVG 曲线侧注未实现，不画错线 |
| U-05 Welcome 仅显式配置 | PASS | `view.json` 驱动；`tests/integration/rendering.rs`、浏览器 |
| U-06 Live/Export 内容对照 | PASS | `tests/integration/rendering.rs` 同模型对照；导出复用同一 renderer |

## 复现

```bash
export MARQDO_BIN=/abs/path/to/frozen/marqdo-3.0.0-beta.1
scripts/acceptance.sh
cat docs/evidence/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json
```

CI 配置见 `.woodpecker.yml`；本环境未实际运行远端 CI，因此 `CI URL` 仍为
**UNVERIFIED**（证据来自本地可复现命令，不冒称 CI 通过）。
