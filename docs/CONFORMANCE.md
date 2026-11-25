# 合规矩阵（CONFORMANCE）— marqdo-view 0.3.0 / rc.4

> 活文档。状态值：
> `PASS`（已由绑定 SHA 的机器证据证明）、`FAIL`、`UNVERIFIED`（尚未证明，
> 不得写成 PASS）、`N/A`（明确非目标）。
> 本矩阵不代替 [`docs/ACCEPTANCE.md`](ACCEPTANCE.md) 与
> `docs/evidence/<CLI-SOURCE-SHA>/manifest.json`。当前新证据尚未生成，
> 因此所有门禁为 `UNVERIFIED`；0.2.0 的旧证据只作历史记录。

最后更新：2026-11-25。

## 1. 当前契约不变量

| # | 条款 | 状态 | 实现位置 / 证据 |
|---|---|---|---|
| C1 | 每个公开边界只有一个当前实现、一个解码器、一个验证器 | 实现，待验收 | `src/protocol/{analysis_v3,observation_v1,tool_v1}.rs`；`scripts/check-no-legacy.sh` |
| C2 | 静态语义只来自一次 `check --emit`（Analysis revision 3，ADR-0069） | 实现，待验收 | `src/core_client.rs`、`src/protocol/analysis_v3.rs` |
| C3 | 运行事实只来自一次执行的 Observation revision 1 序列 | 实现，待验收 | `src/core_client.rs`、`src/protocol/observation_v1.rs`、`src/execution/*` |
| C4 | JSONL 流与原子快照共用同一解码器/领域类型；生产不双读合并 | 实现，待验收 | `src/protocol/observation_v1.rs`；`scripts/check-observation.py stream-snapshot` |
| C5 | 协议模块之外不判断 `schema_version`；领域层无 CLI 细节 | 实现，待验收 | `scripts/check-no-legacy.sh` 对 `src/reader` 的静态检查；`tests/contract/no_coupling.rs` |
| C6 | 未知 schema/revision/kind 显式拒绝，不回退、不重试旧参数 | 实现，待验收 | `src/protocol/*`、`src/error.rs`；协议负向探针 |
| C7 | Tool 清单 revision 1 精确 `requires`，无 `compatibility` | 实现，待验收 | `tool.json`、`scripts/package.sh`、`scripts/install-smoke.sh` |
| C8 | 冻结 CLI 不修改、不重编译、不链接、不读源码 | 实现，待验收 | `scripts/verify-frozen-cli.sh`、acceptance 前后 SHA |

## 2. 静态阅读与引用

| 条款 | 状态 | 实现 / 证据 |
|---|---|---|
| Markdown/GFM 阅读、源码态、项目图片、MathML | 实现，待验收 | `src/reader/*`、浏览器套件 |
| Analysis revision 3 严格校验（source/document/units/anchors/functions/calls/resources/diagnostics） | 实现，待验收 | `src/protocol/analysis_v3.rs` |
| heading_span 用于标题 UI；完整 span 用于 section 高亮/切片/归属 | 实现，待验收 | `src/reader/*`、`assets/reader.js` |
| 引用七态：resolved/not_loaded/missing/ambiguous/external/outside_scope/invalid | 实现，待验收 | `src/protocol/analysis_v3.rs`、`src/reader/render.rs` |
| `not_loaded` = 未核验，不是断链；`ambiguous` 列出 candidates | 实现，待验收 | 同上 |
| 不把 `inspect --ast` 作为机器渲染来源 | 实现，待验收 | `src/core_client.rs`；`scripts/check-no-legacy.sh` |
| source 作为敏感本地数据：不写日志、不上传、不复制进 Observation | 实现，待验收 | `src/protocol/*`、export/live 路由 |

## 3. 执行、SSE 与值

| 条款 | 状态 | 实现 / 证据 |
|---|---|---|
| 显式确认后才 `POST /api/runs` 启动一次 run/call | 实现，待验收 | `src/server/*`、`assets/reader.js` |
| Observation v1 序列经 View SSE 转发/摘要，不重编号/重排/合成 | 实现，待验收 | `src/execution/*`、`src/server/sse.rs` |
| 源绑定：analysis_digest + root_id + module_id + UTF-8 半开 span + source_digest | 实现，待验收 | `src/execution/controller.rs`；`scripts/check-observation.py binding` |
| stale 不显示成功、不错误高亮 | 实现，待验收 | `src/execution/*`；View 回归路径待测试负责人声明 |
| `observation.truncated`/capture.truncated → TRUNCATED；EOF 无终态 → 未完成 | 实现，待验收 | `src/protocol/observation_v1.rs`、`src/execution/events.rs` |
| 值默认不请求、不展示；协议支持显式 `--observe-*`（Text 需二次同意） | 实现，待验收 | `src/execution/*` |
| 单任务、取消、心跳、Last-Event-ID 重放、无轮询 | 实现，待验收 | `src/server/sse.rs`、`tests/integration/*` |
| `strict` + Live/文件目的地：明确拒绝，不回退 | 实现，待验收 | `src/core_client.rs`、`src/error.rs`；协议探针 |

## 4. 导出与安全

| 条款 | 状态 | 实现 / 证据 |
|---|---|---|
| 普通导出零执行、零联网、离线 file:// 可读 | 实现，待验收 | `scripts/test-export.sh` |
| 显式单次结果导出：来源匹配终态、隐私预览、只读 ZIP | 实现，待验收 | `src/server/*`、`scripts/test-complete-reader.py` |
| Live/Export 同一 HTML/CSS/JS 与渲染器 | 实现，待验收 | `tests/contract/no_coupling.rs`、`asset_integrity.rs` |
| 打开/刷新/搜索/切换/导出零副作用 | 实现，待验收 | `scripts/test-no-exec.sh` |
| CSRF/Origin/Cookie/Host/路径/symlink/XSS/跨会话 SSE | 实现，待验收 | `tests/security/`、`scripts/test-security.sh` |
| 快照/错误无绝对主机路径；错误只用 relative_path | 实现，待验收 | `src/protocol/observation_v1.rs`、renderer |
| Tool 安装/分发/卸载/篡改拒绝；CLI SHA 前后一致 | 实现，待验收 | `scripts/install-smoke.sh`、acceptance G03 |

## 5. 门禁与证据

| 门禁 | 状态 | 证据 |
|---|---|---|
| G01 无旧契约残留 | `UNVERIFIED` | `scripts/check-no-legacy.sh`（待验收记录） |
| G02 构建与静态检查 | `UNVERIFIED` | `scripts/acceptance.sh` |
| G03 rc.4 独立安装 | `UNVERIFIED` | `scripts/install-smoke.sh` + acceptance |
| G04 静态契约正向 | `UNVERIFIED` | `view verify` + `scripts/check-examples.sh` |
| G05 运行契约正向 | `UNVERIFIED` | `scripts/acceptance-run.py` |
| G06 流/快照等价 | `UNVERIFIED` | `scripts/check-observation.py` |
| G07 stale 绑定 | `UNVERIFIED` | `scripts/check-observation.py binding` + View 回归 |
| G08 负例 | `UNVERIFIED` | `scripts/check-observation.py fail-closed` + 测试套件 |
| G09 安全基线 | `UNVERIFIED` | `scripts/test-security.sh`、`test-no-exec.sh`、浏览器 |
| G10 导出 | `UNVERIFIED` | `scripts/test-export.sh` |
| G11 证据与 CI | `UNVERIFIED` | 远端 CI 未在本环境运行 |
| G12 文档一致 | `UNVERIFIED` | `scripts/check-docs.sh` |

## 6. 明确非目标（N/A）

| 项 | 状态 |
|---|---|
| Lint/Knowledge/Prove 引擎、调试器/断点/单步、Notebook/持久内核 | N/A（独立协议或明确删除） |
| 变量/内存/句柄默认可视化 | N/A（协议支持显式选择；View 0.3.0 默认不请求） |
| net/process/env/plugin/provider/kernel 授权 UI | N/A（拒绝并引导终端 CLI） |
| Windows/macOS 发行 | N/A（不声明） |
| 像素级人工视觉比对、完整 screen-reader 走查 | `UNVERIFIED`（人工项） |
