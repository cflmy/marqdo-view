# RFC-VIEW-0002 合规矩阵（CONFORMANCE）

> 活文档：每完成一个 PR 更新。状态值：
> `PASS`（本仓库实现 + 可复现证据）、`PARTIAL`（部分实现/证据待补）、
> `PENDING`（未开始）、`N/A`（0002 明确非目标/删除项）。
> 证据列写测试/脚本文档；不得以视觉相似代替测试。

最后更新：2026-11-22（PR-00～PR-06 完成）

## 1. §0 核心不可妥协条件

| # | 条款 | 状态 | 实现位置 / 证据 |
|---|---|---|---|
| M1 | 独立构建/测试/打包/安装；不改/不重编译 CLI；无私有 crate/Git/path 依赖 | PASS（0001 基线） | `Cargo.toml`、`tests/contract/no_coupling.rs`、`scripts/acceptance.sh`（SHA 前后一致） |
| M2 | 语言真相=Analysis v2；运行真相=`run/call --json`+Trace v2；原文=`.mq.md`；Markdown 不判语义 | PASS | `protocol/*`、`execution/controller.rs`、`reader/markdown.rs`；Trace 绑定/错误包络测试 |
| M3 | 打开/刷新/搜索/切换/导出/预览/目录/诊断绝不运行 | PASS（0001 证据，待 0002 复核） | `tests/integration/http_live.rs`、`tests/integration/export.rs`、浏览器证据 |
| M4 | 单一 Read Model / HTML 组件 / CSS / JS；Live=Export | PASS（0001 证据） | `src/render.rs`、`src/assets.rs`、`tests/contract/no_coupling.rs`、`tests/contract/asset_integrity.rs` |
| M5 | 原生执行只经 `MARQDO_BIN run/call` 子进程；无第二运行时 | PASS | `execution/process.rs` 唯一适配器；`no_coupling` 门禁；无第二解释器 |
| M6 | Tool 是普通进程；不得宣称 OS 沙箱 | PASS | README/COMPATIBILITY/UI project 警告明确；无“安全运行”措辞 |
| M7 | 不开发 lint/knowledge/prove；无硬依赖 | N/A（合规） | 无相关目录/依赖 |
| M8 | 可见按钮必须有效；无公开数据/执行能力的面板删除 | PASS | 每个 Dock/对话框动作有效；变量/内存仅一个 UNSUPPORTED 说明，无假按钮 |

## 2. §1.2 功能范围（MVP 强制项）

| 功能 | 状态 | 计划/证据 |
|---|---|---|
| Markdown/GFM 阅读 | PASS | `src/document.rs`、`src/render.rs`、浏览器证据 |
| `@exec`/`@knowledge` 轻量标识（依 Analysis） | PASS | `src/semantic.rs`、`tests/integration/rendering.rs` |
| 源码态 | PASS | `src/render.rs`、`tests/integration/unicode_spans.rs` |
| 函数清单/导航 | PASS | `src/presentation.rs`、`src/render.rs` |
| 单函数参数填写 + 调用 | PASS | `assets/reader.js` 动态表单 + `execution/request.rs` + HTTP/SSE 测试 |
| 文档入口 `main` 执行 | PASS | `POST /api/runs` → SSE；无 main 时引导函数列表 |
| 运行结果（返回/stdout/stderr/错误） | PASS | 安全文本/DOM 渲染、`value_text`、stderr/stdout、错误码；浏览器/HTTP 测试 |
| Trace v2 调用/判断/循环/副作用 | PASS（动态锚点） | Controller 校验摘要 + JS `source_id` chip；stale 不锚定 |
| 执行确认/取消/超时/单任务 | PASS | 两阶段确认、幂等取消、唯一终态、409 busy、墙钟上限 |
| **SSE 状态与终态（禁止轮询）** | PASS | 逐帧 flush、心跳、Last-Event-ID、snapshot、2 连接上限；无轮询路由 |
| 独立 Tool 安装/调用 | PASS（0001） | PR-01 迁移到 0.2.0/scripts |
| 静态 HTML 导出 | PASS | `no_exec_suite`、`export_suite`；导出无运行控件/CSRF/外链 |
| 暖纸/品牌/樱花/侧注/Dock | PASS | Chromium 宽度矩阵/截图 `docs/evidence/PR-04|05/`；像素级人工比对 UNVERIFIED |
| 搜索 + 目录 | PASS | 浏览器证据 |
| 有选择的 Observed 侧注/退化 | PASS | Trace `source_id` 分组 chip；stale/无 Trace/截断降级（ADR-0010） |
| 深色主题/字号/显示设置 | PASS | 单一 token 覆盖层 + localStorage；浏览器断言 |
| project fs read/write grants | PASS（MVP CONTROLLED） | 请求校验 + 二次确认 + `--allow-read/--allow-write` argv 测试 |
| net/process/env/plugin 等授权 | N/A（DEFERRED，必须拒绝并提示 CLI） | PR-03 校验 |
| 变量/内存/Notebook/调试/Session/AI/Kernel | N/A（REMOVED） | 无路由/DOM/状态；仅一个明确 UNSUPPORTED 说明 |

## 3. §5 架构与目录

| 条款 | 状态 | 说明 |
|---|---|---|
| 唯一 Reader+Controller+CLI；无 DocumentService→Execution 隐式调用 | PASS | 只有 `POST /api/runs` 处理函数持有 Controller；页面/导出/GET 无执行边 |
| 层间数据结构（SourceDocument/AnalysisArtifact/SemanticAnchor/ExecutionRecord/PresentationModel） | PASS | `reader::{source,model,assemble}`、`StaticDocument`、`ExecutionRecord`-equivalent job/event DTO；serde 显式 DTO |
| §5.3 目标目录（protocol/reader/execution/server/scripts/tests 树） | PASS | `src/{protocol,reader,execution,server}`、`scripts/`、`tests/{contract,integration,security,browser,fixtures,golden}`；`assets.rs` 归 reader |
| §5.4 单一渲染流水线；Export 类型边界不可执行 | PASS（0001） | `src/export.rs` 不依赖执行层；PR-06 复核 |
| 无第二 Markdown AST/解释器/主题/JS 入口 | PASS | `tests/contract/no_coupling.rs`、`tests/contract/asset_integrity.rs` |

## 4. §6 HTTP / SSE / 执行合同

| 条款 | 状态 | 计划 |
|---|---|---|
| 7 条最小路由 | PASS | `server/routes.rs`；旧路由 404 测试 |
| POST /api/runs 两阶段确认请求体（document/target/parameters/analysis_digest/mode/grants/client_request_id） | PASS | `execution/request.rs` + `POST /api/runs`；unknown field/size/mode/grant 测试 |
| 校验顺序（会话→CSRF/Origin/Host→大小/schema→containment→check→digest→函数/参数→模式/grants→并发→202） | PASS | `server/security.rs` + Controller；拒绝路径不启动进程测试 |
| 同 `client_request_id` 幂等/不同 payload 409 | PASS | Controller + HTTP 集成测试 |
| SSE 帧合同（事件类型/seq/id/schema/心跳） | PASS | `server/sse.rs`、`tests/integration/http_live.rs` |
| Last-Event-ID 重放、快照、128 缓冲、TTL、≤2 连接/任务 | PASS | 重放/伪造游标/2 连接/终态快照测试 |
| 单活动任务、409 run.busy、后台执行不阻塞 SSE | PASS | Controller + HTTP 409/并发测试 |
| 进程组取消（TERM→2s→KILL）、墙钟上限、有界输出 | PASS（模块） | `src/execution/process.rs`、`tests/integration/execution.rs` |
| 显式预算参数 | PASS（模块） | `src/execution/controller.rs`、`--run-timeout` 1..=120 |
| 会话 Cookie（HttpOnly/SameSite=Strict）+ X-View-CSRF + Origin/Host | PASS | `server/security.rs`、`tests/security/`、浏览器测试 |
| loopback only、请求体≤64KiB、CSP、路径 containment | PASS | `server/security.rs`；security 套件（Cookie/CSP/Host/path/symlink） |
| SSE 按会话隔离、跨站读取拒绝 | PASS | 跨会话 404/401、Sec-Fetch-Site 拒绝 |

## 5. §8 里程碑

| PR | 状态 | 提交 |
|---|---|---|
| PR-00 取证/基线 | **完成** | 待提交（本提交） |
| PR-01 Tool/打包/scripts/CI/0.2.0 | **完成** | `scripts/package.sh`、`scripts/install-smoke.sh`、`scripts/verify-frozen-cli.sh`、`.github/workflows/ci.yml` |
| PR-02 Reader/Analysis 结构与只读路由 | **完成** | `src/protocol/*`、`src/reader/*`、`src/config.rs`、`/api/documents`、`tests/{contract,integration}`；`docs/evidence/PR-02/` |
| PR-03 执行层/请求/进程/控制器 | **完成** | `src/execution/*`、ADR-0008、`docs/evidence/PR-03/` |
| PR-04 SSE/路由/会话/前端闭环 | **完成** | `docs/evidence/PR-04/`（含真实 Chromium + 真实冻结 CLI） |
| PR-05 Trace 绑定/标注/浏览器回归 | **完成** | ADR-0010、`docs/evidence/PR-05/` |
| PR-06 导出/发布/文档闭环 | **完成** | `examples/`、`scripts/{check-examples,test-no-exec}.sh`、`docs/ACCEPTANCE.md`、最终 evidence manifest |

## 6. §9 验收测试矩阵（E01–E16 / N01–N22）

| 组 | 状态 | 说明 |
|---|---|---|
| E01 Tool 安装/卸载/分发 | PASS（0001 证据；PR-01 重跑） | `scripts/acceptance.sh` |
| E02 打开 knowledge+exec 文件不运行 | PASS | `tests/integration/http_live.rs` |
| E03 Source/Document 切换 | PASS | 浏览器证据 |
| E04 中文/Emoji/表格/公式 span | PASS | `tests/integration/unicode_spans.rs`、golden |
| E05 main 运行 run --json --trace-file | PASS | 真实冻结 CLI SSE 证据 `docs/evidence/PR-04/real-run.log` |
| E06 H2 Exec n=5 call | PASS | call argv 测试 + 前端函数表单；真实 CLI 在 PR-06 验收 |
| E07 文本/列表常量参数 | PASS | `--arg` 原样表达式 + 前端输入；CLI 最终校验 |
| E08 无 main 不自动运行别的函数 | PASS | 400 `view.no_entry_function` + 无 run 调用测试 |
| E09 Trace 事件映射不伪造变量 | PASS | Controller Trace 摘要 + JS 分组 chip；无变量值 |
| E10 SSE text/event-stream 无 polling | PASS | 集成 + 浏览器请求记录 |
| E11 Last-Event-ID 重连 | PASS | 重放/伪造游标 snapshot 测试 |
| E12 长任务心跳/GET 不重启 | PASS | 心跳 + 2 连接 + GET 观察不启动任务 |
| E13 取消终止进程树 | PASS | Controller/process 单测 + HTTP 取消唯一终态 + 浏览器取消路径 |
| E14 静态导出零联网/相对链接 | PASS | `export_suite`（file:// Chromium）、`no_exec_suite` |
| E15 刷新/重开不自动执行 | PASS | `tests/integration/http_live.rs` |
| E16 运行后改源码 → stale 无高亮 | PASS | `source_edit_during_a_run...`（stale + 全部 anchor=null） |
| N01–N22 | PASS | 全部映射到测试/验收命令，见 `docs/ACCEPTANCE.md` §3 |

## 7. §10 发布门禁

| 门禁 | 状态 |
|---|---|
| 1. PR-00～06 + P0 检验，无伪交付 | PASS | `docs/ACCEPTANCE.md` §1/§2 |
| 2. Tool 独立安装；CLI 无 View 核心提交；SHA 不变 | PASS | acceptance `archive_install`/`core_sha`（0.2.0 重跑） |
| 3. 打开/刷新/导出零副作用；执行仅来自显式 POST；跨站/CSRF 全绿 | PASS | `no_exec_suite`、`security_suite`、`tests/security/` |
| 4. SSE 生命周期/心跳/序号/重连/有界重放；无轮询 | PASS | HTTP/SSE 集成 + 浏览器请求记录 |
| 5. 真值来自 CLI/Analysis/Trace；无假变量/内存/证明 | PASS（设计） |
| 6. Live/Export 同一 HTML/CSS/JS；无旧叠层 | PASS（0001 证据） |
| 7. Linux x86_64 可重现包；README/帮助/限制一致 | PASS | `bundle`/`archive_install`；0.2.0 archive；文档一致性 |
| 8. N01–N22 至少覆盖；失败不执行不泄漏不提升权限 | PASS | `docs/ACCEPTANCE.md` §3 |
