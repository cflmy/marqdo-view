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
| PR-02 Reader/Analysis 只读面与目标目录结构 | 待开始（现有 0001 实现可迁移） | `src/document.rs`、`src/semantic.rs`、`src/render.rs`、`src/server.rs` |
| PR-03 执行层：request/process/controller、pure + project grants | 待开始（现有同步 `pure` 执行可迁移） | `src/core_client.rs`、`src/app.rs` |
| PR-04 SSE 任务生命周期、取消、7 路由、会话 Cookie/CSRF | **未实现（MVP 阻塞项）** | — |
| PR-05 Trace 绑定/原文标注/浏览器回归 | 待开始（现有 Trace 校验可迁移） | `src/trace.rs` |
| PR-06 静态导出/发布包/文档闭环 | 待开始（现有导出可迁移） | `src/export.rs` |

## 继承自 0001 的可用资产（不等于 0002 验收通过）

以下能力已实现并有测试/证据，但需在 0002 架构下迁移、补路由/SSE 与重新验收：

| 能力 | 现状 |
|---|---|
| 原始 Markdown/GFM 阅读、源码态、搜索/设置/打印 | 已实现（单一 `assets/reader.css`/`assets/reader.js`） |
| Analysis v2 严格校验、函数/Unit/签名/诊断/导入/引用/资源投影 | 已实现（无效 span 降级） |
| 单一 `ReaderRenderer`，Live/Export 共用 | 已实现（门禁测试） |
| 显式确认后同步运行 `pure` + Trace v2 校验与展示 | 已实现，但**不是 0002 的异步 SSE 闭环** |
| 静态离线导出（无执行、manifest/哈希、原子发布） | 已实现，待按 0002 重新验收 |
| Tool v1 打包/安装/分发、冻结 CLI SHA 前后一致 | 已实现，待迁移至 `scripts/` 与 0.2.0 |
| HTTP 安全（Host/Origin/CSRF/路径 containment/CSP） | 部分实现；缺**会话 Cookie**模型与 7 路由合同 |
| 真实 Chromium 视觉/交互/无 JS/打印 | 已有证据（0001 时期），待按 0002 宽度矩阵重跑 |

## 与 RFC-VIEW-0002 的已知差距（全部必须关闭）

1. **SSE 执行闭环缺失**：当前是 POST 同步运行后整页重载；0002 要求
   `POST /api/runs` → 202 → `GET /api/runs/{id}/events` SSE（无轮询）→
   accepted/running/终态；断线 `Last-Event-ID` 重放、128 事件环形缓冲、终态 TTL。
2. **路由未收敛**：现为 `/api/run`、`/api/clear-run`、`/api/document`、
   `/api/health`；0002 只允许
   `GET /`、`GET /file`、`GET /assets/...`、`GET /api/documents`、
   `POST /api/runs`、`GET /api/runs/{id}/events`、`POST /api/runs/{id}/cancel`。
3. **无单函数调用表单**：函数目前只有跳转列表；缺基于 Analysis 签名的参数填写、
   命令预览、`--arg` 常量表达式提交。
4. **无 project grants**：0002 要求受控 fs read/write glob（明确二次确认），
   其它 net/process/env/plugin 一律拒绝并引导 CLI。
5. **无运行会话 Cookie**：CSRF 目前是进程级 token；0002 要求
   `HttpOnly; SameSite=Strict` 随机会话 Cookie + 每会话 CSRF，且 SSE 按会话隔离。
6. **无取消/单任务状态机/重复请求去重**：缺 `cancel_requested`、唯一终态、
   `client_request_id` 幂等/409、409 run.busy。
7. **执行预算未按 0002**：默认 30s（最大 120s）、`--fuel 1000000`、
   `--max-output 1048576`、`--trace-max-events 10000`、`--trace-max-bytes 4194304`、
   JSON/Trace/日志 4 MiB 有界读取与 `view.output_limit` 未实现。
8. **目录/测试/脚本/示例未按 0002 §5.3**：缺 `src/protocol`、`src/reader`、
   `src/execution`、`src/server`、`tests/{contract,integration,security,browser,
   fixtures,golden}`、`scripts/`、`examples/{welcome,branch-loop,function-params,
   knowledge-only,unsafe-side-effect}.mq.md`、`.github/workflows/ci.yml`。
9. **文档口径**：README/COMPATIBILITY/ACCEPTANCE 仍含 0001 时期表述，需在
   PR-06 前全部更新为 0002 口径。
10. **视觉回归矩阵**：0002 要求 2048×1133 源参考与 1664×920/1440×900/1024×768/
    768×1024/390×844 宽度；现有截图集合不完整。

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
