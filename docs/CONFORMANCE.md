# RFC-VIEW-0002 合规矩阵（CONFORMANCE）

> 活文档：每完成一个 PR 更新。状态值：
> `PASS`（本仓库实现 + 可复现证据）、`PARTIAL`（部分实现/证据待补）、
> `PENDING`（未开始）、`N/A`（0002 明确非目标/删除项）。
> 证据列写测试/脚本文档；不得以视觉相似代替测试。

最后更新：2026-11-22（PR-00）

## 1. §0 核心不可妥协条件

| # | 条款 | 状态 | 实现位置 / 证据 |
|---|---|---|---|
| M1 | 独立构建/测试/打包/安装；不改/不重编译 CLI；无私有 crate/Git/path 依赖 | PASS（0001 基线） | `Cargo.toml`、`tests/contract/no_coupling.rs`、`scripts/acceptance.sh`（SHA 前后一致） |
| M2 | 语言真相=Analysis v2；运行真相=`run/call --json`+Trace v2；原文=`.mq.md`；Markdown 不判语义 | PARTIAL | 已实现 Analysis 严格校验与 Trace 绑定；`call` 入口与请求合同待 PR-03/04 |
| M3 | 打开/刷新/搜索/切换/导出/预览/目录/诊断绝不运行 | PASS（0001 证据，待 0002 复核） | `tests/integration/http_live.rs`、`tests/integration/export.rs`、浏览器证据 |
| M4 | 单一 Read Model / HTML 组件 / CSS / JS；Live=Export | PASS（0001 证据） | `src/render.rs`、`src/assets.rs`、`tests/contract/no_coupling.rs`、`tests/contract/asset_integrity.rs` |
| M5 | 原生执行只经 `MARQDO_BIN run/call` 子进程；无第二运行时 | PARTIAL | 子进程已有；`call` 与取消/状态机待 PR-03/04 |
| M6 | Tool 是普通进程；不得宣称 OS 沙箱 | PARTIAL | 文档已声明；UI 文案待 PR-04/06 复核 |
| M7 | 不开发 lint/knowledge/prove；无硬依赖 | N/A（合规） | 无相关目录/依赖 |
| M8 | 可见按钮必须有效；无公开数据/执行能力的面板删除 | PARTIAL | 现 UI 仍有“变量/内存不支持”入口（解释性）；PR-04 复核 |

## 2. §1.2 功能范围（MVP 强制项）

| 功能 | 状态 | 计划/证据 |
|---|---|---|
| Markdown/GFM 阅读 | PASS | `src/document.rs`、`src/render.rs`、浏览器证据 |
| `@exec`/`@knowledge` 轻量标识（依 Analysis） | PASS | `src/semantic.rs`、`tests/integration/rendering.rs` |
| 源码态 | PASS | `src/render.rs`、`tests/integration/unicode_spans.rs` |
| 函数清单/导航 | PASS | `src/presentation.rs`、`src/render.rs` |
| 单函数参数填写 + 调用 | **PENDING** | PR-03 请求合同 + PR-04 前端表单 |
| 文档入口 `main` 执行 | PARTIAL | 同步 pure 已存在；需改为 SSE 异步任务（PR-04） |
| 运行结果（返回/stdout/stderr/错误） | PARTIAL | 0001 已有；需 0002 JSON→UI 映射与安全文本展示复核（PR-04/05） |
| Trace v2 调用/判断/循环/副作用 | PARTIAL | `src/trace.rs` 已有；需 SSE 后动态标注与 stale 规则（PR-04/05） |
| 执行确认/取消/超时/单任务 | **PENDING** | PR-03/04 |
| **SSE 状态与终态（禁止轮询）** | **PENDING（阻塞）** | PR-04 |
| 独立 Tool 安装/调用 | PASS（0001） | PR-01 迁移到 0.2.0/scripts |
| 静态 HTML 导出 | PARTIAL | 已实现；PR-06 复核零执行/离线 |
| 暖纸/品牌/樱花/侧注/Dock | PARTIAL | 已有；PR-05 视觉矩阵 |
| 搜索 + 目录 | PASS | 浏览器证据 |
| 有选择的 Observed 侧注/退化 | PARTIAL | 内联 chip 已有；SSE 后按本次运行更新（PR-04/05） |
| 深色主题/字号/显示设置 | PARTIAL | 已有字号/标注开关/print；深色待 PR-05 |
| project fs read/write grants | **PENDING** | PR-03/04（MVP CONTROLLED） |
| net/process/env/plugin 等授权 | N/A（DEFERRED，必须拒绝并提示 CLI） | PR-03 校验 |
| 变量/内存/Notebook/调试/Session/AI/Kernel | N/A（REMOVED） | 禁止出现 DOM/路由/状态 |

## 3. §5 架构与目录

| 条款 | 状态 | 说明 |
|---|---|---|
| 唯一 Reader+Controller+CLI；无 DocumentService→Execution 隐式调用 | PARTIAL | 现同步架构无隐式执行；SSE 后复核 |
| 层间数据结构（SourceDocument/AnalysisArtifact/SemanticAnchor/ExecutionRecord/PresentationModel） | PARTIAL | 现有类型可映射；PR-02/03 重整 |
| §5.3 目标目录（protocol/reader/execution/server/scripts/tests 树） | PARTIAL | protocol/reader/config/scripts/tests(contract,integration) 已迁移；execution/server 拆分与 security 套件待 PR-03/04 |
| §5.4 单一渲染流水线；Export 类型边界不可执行 | PASS（0001） | `src/export.rs` 不依赖执行层；PR-06 复核 |
| 无第二 Markdown AST/解释器/主题/JS 入口 | PASS | `tests/contract/no_coupling.rs`、`tests/contract/asset_integrity.rs` |

## 4. §6 HTTP / SSE / 执行合同

| 条款 | 状态 | 计划 |
|---|---|---|
| 7 条最小路由 | **PENDING** | PR-04 |
| POST /api/runs 两阶段确认请求体（document/target/parameters/analysis_digest/mode/grants/client_request_id） | **PENDING** | PR-03/04 |
| 校验顺序（会话→CSRF/Origin/Host→大小/schema→containment→check→digest→函数/参数→模式/grants→并发→202） | **PENDING** | PR-03/04 |
| 同 `client_request_id` 幂等/不同 payload 409 | **PENDING** | PR-04 |
| SSE 帧合同（事件类型/seq/id/schema/心跳） | **PENDING** | PR-04 |
| Last-Event-ID 重放、快照、128 缓冲、TTL、≤2 连接/任务 | **PENDING** | PR-04 |
| 单活动任务、409 run.busy、后台执行不阻塞 SSE | **PENDING** | PR-03/04 |
| 进程组取消（TERM→2s→KILL）、墙钟上限、有界输出 | **PENDING** | PR-03 |
| 显式预算参数 | **PENDING** | PR-03 |
| 会话 Cookie（HttpOnly/SameSite=Strict）+ X-View-CSRF + Origin/Host | **PENDING** | PR-04 |
| loopback only、请求体≤64KiB、CSP、路径 containment | PARTIAL | 大部分已有；会话 Cookie 待 PR-04 |
| SSE 按会话隔离、跨站读取拒绝 | **PENDING** | PR-04 |

## 5. §8 里程碑

| PR | 状态 | 提交 |
|---|---|---|
| PR-00 取证/基线 | **完成** | 待提交（本提交） |
| PR-01 Tool/打包/scripts/CI/0.2.0 | **完成** | `scripts/package.sh`、`scripts/install-smoke.sh`、`scripts/verify-frozen-cli.sh`、`.github/workflows/ci.yml` |
| PR-02 Reader/Analysis 结构与只读路由 | **完成** | `src/protocol/*`、`src/reader/*`、`src/config.rs`、`/api/documents`、`tests/{contract,integration}`；`docs/evidence/PR-02/` |
| PR-03 执行层/请求/进程/控制器 | PENDING | — |
| PR-04 SSE/路由/会话/前端闭环 | PENDING | — |
| PR-05 Trace 绑定/标注/浏览器回归 | PENDING | — |
| PR-06 导出/发布/文档闭环 | PENDING | — |

## 6. §9 验收测试矩阵（E01–E16 / N01–N22）

| 组 | 状态 | 说明 |
|---|---|---|
| E01 Tool 安装/卸载/分发 | PASS（0001 证据；PR-01 重跑） | `scripts/acceptance.sh` |
| E02 打开 knowledge+exec 文件不运行 | PASS | `tests/integration/http_live.rs` |
| E03 Source/Document 切换 | PASS | 浏览器证据 |
| E04 中文/Emoji/表格/公式 span | PASS | `tests/integration/unicode_spans.rs`、golden |
| E05 main 运行 run --json --trace-file | PARTIAL | 同步路径有；SSE 终态待 PR-04 |
| E06 H2 Exec n=5 call | **PENDING** | PR-03/04 |
| E07 文本/列表常量参数 | **PENDING** | PR-03/04 |
| E08 无 main 不自动运行别的函数 | PARTIAL | 现有 `can_offer_run` 逻辑；PR-04 复核 |
| E09 Trace 事件映射不伪造变量 | PARTIAL | `src/trace.rs`；SSE 后复核 |
| E10 SSE text/event-stream 无 polling | **PENDING** | PR-04 |
| E11 Last-Event-ID 重连 | **PENDING** | PR-04 |
| E12 长任务心跳/GET 不重启 | **PENDING** | PR-04 |
| E13 取消终止进程树 | **PENDING** | PR-03/04 |
| E14 静态导出零联网/相对链接 | PARTIAL | 已实现；PR-06 复核 |
| E15 刷新/重开不自动执行 | PASS | `tests/integration/http_live.rs` |
| E16 运行后改源码 → stale 无高亮 | PARTIAL | 现有 stale；PR-05 复核 |
| N01–N22 | PARTIAL | 0001 已覆盖部分（路径/XSS/跨站/digest/超时/打包完整性）；SSE/去重/取消/会话项待 PR-04 补齐，逐项记录在 PR-05/06 证据 |

## 7. §10 发布门禁

| 门禁 | 状态 |
|---|---|
| 1. PR-00～06 + P0 检验，无伪交付 | PENDING |
| 2. Tool 独立安装；CLI 无 View 核心提交；SHA 不变 | PARTIAL（0001 已证 SHA 不变，需 0.2.0 重跑） |
| 3. 打开/刷新/导出零副作用；执行仅来自显式 POST；跨站/CSRF 全绿 | PARTIAL（同步模型已覆盖，SSE 模型待补） |
| 4. SSE 生命周期/心跳/序号/重连/有界重放；无轮询 | **PENDING** |
| 5. 真值来自 CLI/Analysis/Trace；无假变量/内存/证明 | PASS（设计） |
| 6. Live/Export 同一 HTML/CSS/JS；无旧叠层 | PASS（0001 证据） |
| 7. Linux x86_64 可重现包；README/帮助/限制一致 | PENDING（PR-06） |
| 8. N01–N22 至少覆盖；失败不执行不泄漏不提升权限 | PENDING |
