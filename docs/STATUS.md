# marqdo-view 现状（STATUS）

> 唯一现状来源。设计/规范见 RFC 与 ADR；本文只记录**实际完成、证据与限制**。
> 最后更新：2026-11-22（PR-06 完成）

## 版本与基线

| 项 | 值 |
|---|---|
| View 版本 | `0.1.0` |
| 目标 CLI 产品范围 | `>=3.0.0-beta.1,<4.0.0` |
| 冻结 CLI 二进制 | 发布归档 `marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu.tar.gz` 中的 `marqdo` |
| 冻结 CLI SHA-256 | `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f` |
| CLI 源码 SHA | `2c7c4f31e7a388d3fcfa717a77067e696ed13854`（tag `v3.0.0-beta.1`） |
| 协议 | Tool v1 / Analysis v2 / Trace v2 / markdown profile `marqdo.md.v1` |
| 目标平台 | Linux x86_64 glibc（正式）；其他平台未验证 |
| 视觉基准 | `docs/design/reference-old-view-reader.png`（1664×920） |
| 验收证据 | `docs/evidence/<CLI-SHA>/manifest.json`（由 `tools/acceptance.sh` 真实命令生成） |

## 分阶段进度

| 阶段 | 状态 | 证据 |
|---|---|---|
| PR-00 基线与旧 View 静态取证 | **完成** | `docs/audits/legacy-render-paths.md`、`docs/design/*`、ADR-0001/0002 |
| PR-01 Tool v1 安装闭环 | **完成** | `tool.json`、`tools/package.sh`、`tools/install-smoke.sh`、`tests/core_contract.rs`、`tests/no_coupling.rs` |
| PR-02 DocumentModel + Analysis v2 浏览 | **完成** | `src/document.rs`、`src/semantic.rs`、`src/presentation.rs`、`src/render.rs`、`src/server.rs`、冻结 Analysis golden |
| PR-03 Reader 视觉重建 | **完成（像素级人工比对未执行）** | 单一 `assets/reader.css`/`reader.js`；真实 Chromium 7 宽度/无 JS/打印/运行 PASS；`docs/evidence/PR-03/` |
| PR-04 Trace v2 运行可视化 | **完成** | `src/trace.rs`、CSRF/确认运行 API、真实 Trace golden、`tests/trace_contract.rs`、ADR-0004 |
| PR-05 Live 安全 + 静态导出 | **完成** | `src/export.rs`、`tools/test-export.sh`、`tools/test-security.sh`、`tests/export.rs`、ADR-0005 |
| PR-06 冻结 CLI 端到端验收 | **完成（CI 远端未运行）** | `tools/acceptance.sh` 全部 PASS；`docs/evidence/<CLI-SHA>/manifest.json`；`docs/ACCEPTANCE.md`；ADR-0006 |

## 能力现状

| 能力 | 状态 |
|---|---|
| 原始 Markdown 阅读（GFM 表/任务列表/引用/围栏/数学原文） | 已实现 |
| 文档态 / 源码态（原文逐行，含标记） | 已实现 |
| Analysis v2 静态语义（Unit/Function/Signature/诊断/导入/引用/资源） | 已实现，严格校验 + 无效 span 降级 |
| 单一 `ReaderRenderer`，Live/Export 共用 | 已实现（门禁测试） |
| Live 只读 API（`/`、`/file`、`/assets`、`/api/document`、`/api/health`） | 已实现 |
| 显式确认运行 + Trace v2 可视化（`pure` 模式） | 已实现（CSRF/Origin/单活动/超时） |
| 静态离线导出（纯只读、manifest/哈希、原子发布） | 已实现 |
| 搜索/导航/设置/打印/无 JS/响应式 | 已实现（真实浏览器验收） |
| `view verify`（CLI/Analysis/source 绑定，无执行） | 已实现 |
| 逐语句变量值 / 参数实际值 / 内存快照 | **UNSUPPORTED**（Trace v2 不提供，绝不伪造） |
| 知识“已证明” / Prove / Knowledge / Lint | **UNSUPPORTED**（等待独立协议） |
| 调试器/断点/单步、Notebook、旧 2.x 语法 | **不实现**（非目标） |
| `project`/`strict` 运行与 grants 授权界面 | **未实现**（首版仅 `pure`，拒绝其他模式） |
| SSE 流式输出、多运行历史、取消运行 | **未实现**（运行完成后整页重载） |
| SVG 曲线侧注 | **未实现**（使用内联 keyed chip，U-04 PARTIAL） |
| 像素级视觉比对 | **UNVERIFIED**（人工复核待办，U-01） |
| 远端 CI 实际运行 | **UNVERIFIED**（`.woodpecker.yml` 已提供；本地可复现证据已生成） |

## 验收证据

- 一键：`MARQDO_BIN=/abs/path/marqdo tools/acceptance.sh`
- 全链路结果：`fmt/clippy/test/build/bundle/archive_install/view_help/verify/export/browse/real_run/export_suite/security_suite/browser/core_sha` 全部 PASS。
- 冻结核心 SHA-256 在验收前后完全相同（`062cb56e…`），View 从未重编译/链接 CLI。
- 安装来自 `.tar.gz` 发布归档解包到临时目录；不读取 CLI 源码或旧仓库运行代码。
- 浏览器：Chromium Headless Shell 131（Playwright 1.49），7 个宽度、无外部请求、无 console 错误、无 JS 可读、打印、恶意 fixture、显式运行。
- 详见 `docs/ACCEPTANCE.md` 与 `docs/evidence/`。

## 已知限制与偏差

- 测试按 crate 根级集成测试文件组织，而非 RFC 建议的子目录树；职责与门禁不变（ADR-0006）。
- 运行完成后整页重载；不实现 SSE/取消/历史。
- 静态导出每个文档执行一次 `check`（非执行），无增量导出。
- 首版运行仅 `pure`；`project`/`strict` grants 需要未来独立授权 UI。
- 平台：仅 Linux x86_64 glibc 正式验证；macOS/Windows 未验证。
- 视觉：几何/颜色/响应式/无外部资源自动化 PASS，但像素级人工比对未执行。

## 复现文档

- 一分钟使用：`README.md`
- 规范：`docs/RFC-VIEW-0001.md`
- 兼容性：`COMPATIBILITY.md`
- 变更：`CHANGELOG.md`
- 旧 View 审计：`docs/audits/legacy-render-paths.md`
- 协议一致性：`docs/audits/protocol-conformance.md`
- 验收：`docs/ACCEPTANCE.md`
- 决策：`docs/ADR/0001..0006`
