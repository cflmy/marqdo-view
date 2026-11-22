# marqdo-view 现状（STATUS）

> 唯一现状来源。规范见 `docs/architecture/RFC-VIEW-0002.md`；本文只记录
> **实际完成、证据与限制**。最后更新：2026-11-22（RFC-VIEW-0002 PR-00 完成）

## 版本与基线

| 项 | 值 |
|---|---|
| View 产品版本 | `0.2.0` |
| 实施规范 | `docs/architecture/RFC-VIEW-0002.md`（2026-11-22） |
| 目标 CLI 产品范围 | `>=3.0.0-beta.1,<4.0.0` |
| 冻结 CLI 二进制 SHA-256 | `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f` |
| CLI 源码 SHA | `2c7c4f31e7a388d3fcfa717a77067e696ed13854`（tag `v3.0.0-beta.1`） |
| 协议 | Tool v1 / Analysis v2 / Trace v2 / `marqdo.md.v1` |
| 目标平台 | Linux x86_64 glibc（正式）；其他平台不支持/未验证 |
| 视觉基准 | `docs/design/reference-old-view-reader.png`（**2048×1133**） |

## RFC-VIEW-0002 里程碑

| 阶段 | 状态 | 证据 |
|---|---|---|
| PR-00 基线、旧 View 取证、冻结 CLI 合同 | **完成** | `docs/audits/legacy-view.md`、`docs/audits/protocol-contract-0002.md`、`docs/decisions/0007` |
| PR-01 Tool v1 打包/安装/调度、0.2.0、scripts/、CI | **完成** | `scripts/package.sh`、`scripts/install-smoke.sh`、`scripts/verify-frozen-cli.sh`、`.github/workflows/ci.yml`；冻结 CLI `tool install`/`view --help`/篡改拒绝实测 |
| PR-02 Reader/Analysis 只读面与目标目录结构 | **完成** | `src/protocol/*`、`src/reader/*`、`src/config.rs`、`tests/{contract,integration}/`、`/api/documents`；证据 `docs/evidence/PR-02/` |
| PR-03 执行层：request/process/controller、pure + project grants | **完成** | `src/execution/{request,process,controller,events}.rs`、`--run-timeout`、ADR-0008、`tests/integration/execution.rs`；证据 `docs/evidence/PR-03/` |
| PR-04 SSE 任务生命周期、取消、7 路由、会话 Cookie/CSRF | **完成** | `src/server/{mod,routes,security,sse}.rs`、`assets/reader.js` SSE/表单、`tests/integration/http_live.rs`、`tests/security/`、真实 Chromium + 真实冻结 CLI SSE 证据 `docs/evidence/PR-04/` |
| PR-05 Trace 绑定/原文标注/浏览器回归 | 待开始（SSE 后动态标注/宽度矩阵已基本具备） | `src/execution/controller.rs` Trace 摘要、浏览器证据 |
| PR-06 静态导出/发布包/文档闭环 | 待开始 | `src/export.rs`、`scripts/acceptance.sh`、`docs/ACCEPTANCE.md` |

## 继承自 0001 的可用资产（不等于 0002 验收通过）

以下能力已实现并有测试/证据，但需在 0002 架构下迁移、补路由/SSE 与重新验收：

| 能力 | 现状 |
|---|---|
| 原始 Markdown/GFM 阅读、源码态、搜索/设置/打印 | 已实现（单一 `assets/reader.css`/`assets/reader.js`） |
| Analysis v2 严格校验、函数/Unit/签名/诊断/导入/引用/资源投影 | 已实现（无效 span 降级） |
| 单一 `ReaderRenderer`，Live/Export 共用 | 已实现（`src/reader/render.rs`，门禁测试） |
| 显式确认 + SSE 异步执行 + Trace v2 | 已实现（7 路由、单任务、取消、断线重放） |
| 静态离线导出（无执行、manifest/哈希、原子发布） | 已实现，待按 0002 重新验收 |
| Tool v1 打包/安装/分发、冻结 CLI SHA 前后一致 | 已实现，待迁移至 `scripts/` 与 0.2.0 |
| HTTP 安全（Host/Origin/CSRF/路径 containment/CSP） | 部分实现；缺**会话 Cookie**模型与 7 路由合同 |
| 真实 Chromium 视觉/交互/无 JS/打印 | 已有证据（0001 时期），待按 0002 宽度矩阵重跑 |

## 与 RFC-VIEW-0002 的已知差距（剩余项）

PR-00～PR-04 已关闭 SSE、7 路由、单函数调用表单、受控 project grants、
会话 Cookie/CSRF、取消/去重/单任务、执行预算与有界输出、目标目录/测试树/
`scripts/`/CI/浏览器宽度矩阵。**剩余**：

1. **PR-05**：Trace 事件到原文的锚点/侧注在 SSE 后已由前端按 `source_id`
   渲染 keyed chip；仍需固化 stale/截断/无 Trace 的浏览器回归与最终视觉矩阵
   证据（含深色/打印/断网/无 JS）。
2. **PR-06**：`examples/{welcome,branch-loop,function-params,knowledge-only,
   unsafe-side-effect}.mq.md` 需按 RFC §13.3 创建并用冻结 CLI 实际 check/call
   记录；静态导出需按 0002 重新跑离线/零执行验收；更新 ACCEPTANCE/README/
   COMPATIBILITY/CHANGELOG 为 0002 口径并生成最终 evidence manifest。
3. 0002 §9.2 N 系列须逐项在最终验收报告中登记（当前 HTTP/SSE/安全套件已覆盖
   大部分：N01/N02/N03/N04/N05/N06/N07/N09/N10/N11/N12/N13/N14/N15/N16/N17/
   N19/N21/N22 有对应测试；N18/N20 在 PR-06 冻结 CLI/导出套件中登记）。
4. 视觉/可达性的人工像素级比对与完整 screen-reader 走查仍标 `UNVERIFIED`，
   自动化几何/颜色/无外部资源/响应式证据已存在（`docs/evidence/PR-04/`）。

## 明确 UNSUPPORTED / 非目标（不得伪造）

- 逐语句变量值、参数实际值、内存快照/地址寄存器：Trace v2 不提供，永远显示
  “不支持”，不得用 `value` 代替。
- 知识“已证明”/Prove、Lint、Knowledge 编译：独立仓库/未来版本化报告。
- Notebook/Run All/持久内核、断点/单步/调用栈/时间旅行、Session/journal/snapshot。
- AI/Agent/LLM Token 流、插件市场/热插拔、数据库/账号/多租户。
- 第二套 Markdown AST/解释器/`inspect --ir` 解析：FORBIDDEN。
- Windows/macOS 发行、`strict` 模式按钮、net/process/env/foreign 授权 UI：首版不做。

## 复现与证据

- 冻结 CLI 合同与实测：`docs/audits/protocol-contract-0002.md`
- 旧 View 取证：`docs/audits/legacy-view.md`（静态；运行时 `LEGACY_RUNTIME_UNVERIFIED`）
- 0002 条款映射：`docs/CONFORMANCE.md`
- 验收（0001 时期，历史）：`docs/ACCEPTANCE.md`、`docs/evidence/`
- 0001 时期一键验收脚本：`scripts/acceptance.sh`（PR-01 后迁移为 `scripts/` 口径）

## 复现文档

- 使用/安装：`README.md`
- 规范：`docs/architecture/RFC-VIEW-0002.md`（当前）、`RFC-VIEW-0001.md`（历史）
- 兼容性：`COMPATIBILITY.md`
- 变更：`CHANGELOG.md`
- 决策：`docs/decisions/0001..0007`
