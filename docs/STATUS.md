# marqdo-view 现状（STATUS）

> 唯一现状来源。规范见 `docs/architecture/RFC-VIEW-0002.md`；本文只记录
> **实际完成、证据与限制**。最后更新：2026-11-22（PR-00～PR-06 完成）

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
| 验收状态 | `docs/ACCEPTANCE.md`（§10 门禁全部 PASS）；机器证据见 `docs/evidence/<CLI-SHA>/manifest.json` |

## RFC-VIEW-0002 里程碑（全部完成）

| 阶段 | 状态 | 证据 |
|---|---|---|
| PR-00 基线、旧 View 取证、冻结 CLI 合同 | **完成** | `docs/audits/legacy-view.md`、`docs/audits/protocol-contract-0002.md`、ADR-0007 |
| PR-01 Tool v1 打包/安装/调度、0.2.0、scripts/、CI | **完成** | `scripts/package.sh`、`scripts/verify-frozen-cli.sh`、`.github/workflows/ci.yml`、`docs/evidence/PR-01/` |
| PR-02 Reader/Analysis 目标目录与只读面 | **完成** | `src/protocol/*`、`src/reader/*`、`src/config.rs`、`/api/documents`、`tests/{contract,integration}`、`docs/evidence/PR-02/` |
| PR-03 执行层：request/process/controller、pure + project grants | **完成** | `src/execution/*`、`--run-timeout`、ADR-0008、`docs/evidence/PR-03/` |
| PR-04 SSE 任务生命周期、取消、7 路由、会话 Cookie/CSRF | **完成** | `src/server/{mod,routes,security,sse}.rs`、`assets/reader.js`、ADR-0009、`docs/evidence/PR-04/`（真实 CLI + Chromium） |
| PR-05 Trace 绑定/原文标注/浏览器回归 | **完成** | Controller Trace 摘要/`source_id`/stale、ADR-0010、`docs/evidence/PR-05/` |
| PR-06 静态导出/发布包/示例/文档闭环 | **完成** | `examples/*.mq.md`、`scripts/check-examples.sh`、`scripts/test-no-exec.sh`、`docs/ACCEPTANCE.md`、最终 evidence manifest |

## 能力现状

| 能力 | 状态 |
|---|---|
| 原始 Markdown/GFM 阅读、源码态、搜索/设置/打印/深色主题 | 已实现（单一 `assets/reader.css` + `assets/reader.js`） |
| Analysis v2 严格校验 + 函数/Unit/签名/诊断/导入/引用/资源投影 | 已实现（无效 span/摘要降级，未知 major 拒绝） |
| 单一 Read Model/HTML 组件/CSS/JS；Live=Export | 已实现（门禁测试） |
| 固定 7 路由 Live 服务；打开/刷新/搜索/切换/导出零执行 | 已实现并测试 |
| 显式单任务执行（run main / call function），纯 + 受控 project fs grants | 已实现（两阶段确认、命令预览、二次确认） |
| SSE 状态/终态、取消、心跳、Last-Event-ID 重放、无轮询 | 已实现（单任务、128 环形缓冲、2 连接上限） |
| 运行结果（Wire Value/stdout/stderr/错误/诊断）安全文本展示 | 已实现（无 innerHTML 服务端数据） |
| Trace v2 调用/分支/循环/副作用 + 原文锚点 + stale 降级 | 已实现（分组 chip、来源标注、截断/无效显式） |
| Tool v1 独立安装/分发、冻结 CLI SHA 前后一致 | 已实现（0.2.0 包，篡改拒绝） |
| 静态离线导出（无执行、无联网、manifest/哈希、不可变发布） | 已实现并重新验收 |
| 固定 examples（welcome/branch-loop/function-params/knowledge-only/unsafe-side-effect） | 已实现，冻结 CLI check/call/run 实测 |
| 逐语句变量值 / 参数实际值 / 内存快照 | **UNSUPPORTED**（Trace v2 不提供，绝不伪造） |
| 知识“已证明” / Prove / Lint | **UNSUPPORTED**（等待独立版本化报告协议） |
| 调试器/断点/单步/调用栈、Notebook/持久内核、Session/journal | **不实现**（RFC 明确删除） |
| AI/Agent/Token 流、插件市场、DB/多租户 | **不实现**（非目标） |
| `strict`、net/process/env/plugin/provider/kernel 授权 UI | **不提供**（拒绝并引导终端 CLI） |
| Windows/macOS 发行 | **不支持/未验证** |

## 明确 UNSUPPORTED / 非目标（不得伪造）

- 变量/内存/逐语句值：Trace v2 不提供；UI 只有一个明确的“未提供的能力”说明。
- 数学/领域正确性与“已证明”：Analysis 成功 ≠ 证明；未来 Prove 独立提供。
- Notebook/Run All/内核重启、断点/单步/时间旅行、第二套 Markdown AST/解释器。
- 网络/进程/环境/插件/Provider/Foreign 授权 UI；高级执行由用户在终端运行 CLI。

## 证据入口

| 内容 | 路径 |
|---|---|
| 一键验收 + 最终 manifest | `scripts/acceptance.sh`；`docs/evidence/2c7c4f31…/manifest.json` |
| 验收门禁报告 | `docs/ACCEPTANCE.md` |
| 0002 合规矩阵 | `docs/CONFORMANCE.md` |
| 冻结 CLI 合同 | `docs/audits/protocol-contract-0002.md` |
| 旧 View 取证 | `docs/audits/legacy-view.md`（静态；运行时 `LEGACY_RUNTIME_UNVERIFIED`） |
| 分 PR 证据 | `docs/evidence/PR-01..PR-06/` |
| 示例 fixture 实测 | `scripts/check-examples.sh`（输出见 `docs/evidence/PR-06/`） |
| 零执行哨兵 | `scripts/test-no-exec.sh` |
| 浏览器截图/日志 | `docs/evidence/PR-04/`、`docs/evidence/PR-05/` |
| 决策记录 | `docs/decisions/0001..0010` |

## 已知限制与偏差（诚实状态）

- 首版运行仅 `pure` 与受控 `project` fs read/write；`project` 不是 OS 沙箱，
  不自动提升 `marqdo.yaml` capability 请求。
- SSE 只推送 View 生命周期与终态数据，不承诺逐字 stdout/Token 流（RFC §6.1）。
- 观测侧注采用内联分组 chip，而非 SVG 曲线；按 RFC §4.4 允许降级（ADR-0010）。
- 像素级人工视觉比对与完整 screen-reader 走查仍为 `UNVERIFIED`；自动化证据充分。
- 远端 CI 配置已提供，但本环境未实际运行远端 CI；证据来自本地真实命令。
- 平台：Linux x86_64 glibc 正式验证；其他平台不声明支持。

## 复现文档

- 使用/安装：`README.md`
- 规范：`docs/architecture/RFC-VIEW-0002.md`（当前）、`RFC-VIEW-0001.md`（历史）
- 兼容性：`COMPATIBILITY.md`
- 变更：`CHANGELOG.md`
- 决策：`docs/decisions/`
