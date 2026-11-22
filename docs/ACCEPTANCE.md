# 验收状态（RFC-VIEW-0002 §9 / §10）

> 最新 issue #2 复验见 §7。以下为 PR-00～PR-06 的初次验收记录。
> 证据来自真实冻结 CLI（`marqdo 3.0.0-beta.1`）+ 真实进程 HTTP/SSE 测试 +
> 真实 Chromium 131；生成命令为 `MARQDO_BIN=… scripts/acceptance.sh`。
> 机器可读证据：`docs/evidence/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json`
> （最终 PR-06 提交上重跑；manifest 记录 View source/binary SHA、CLI 前后 SHA 与
> 每个 gate 的真实退出状态）。

- 日期：2026-11-22
- View：`0.2.0`（Linux x86_64）
- 冻结 CLI：`marqdo 3.0.0-beta.1`，SHA-256
  `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f`
- CLI 源码：tag `v3.0.0-beta.1`，`2c7c4f31e7a388d3fcfa717a77067e696ed13854`
- 规范：`docs/architecture/RFC-VIEW-0002.md`；矩阵：`docs/CONFORMANCE.md`

## 0. 最终证据身份

| 项 | 值 |
|---|---|
| View 代码提交 | `0fceb40088e728d481411c36a8d311aa3431b057`（本证据对应提交；evidence 提交紧随其后） |
| View 发布二进制 SHA-256 | `eb19f79632457f37ae3573696d7db84d57358627b65e618819ff80f132b0026a` |
| 发布归档 SHA-256 | `a3fb28c8d025f44dee6c06da80c13d3f65b9c62d1f657c04b02ef360ff24fcf5` |
| 冻结 CLI SHA-256（前后相同） | `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f` |
| 生成时间（UTC） | `2026-11-22T06:55:13.151362+00:00` |
| manifest | `docs/evidence/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json` |

## 1. §10 发布门禁（强制）

| # | 门禁 | 结果 | 证据 |
|---|---|---|---|
| 1 | PR-00～06 完成、P0 检验、无伪交付 | **PASS** | 本文件 §2/§3；各 PR `docs/evidence/PR-0x/` |
| 2 | Tool v1 独立安装；CLI 无 View 核心提交；SHA 不变 | **PASS** | acceptance `archive_install`/`tool_list`/`view_help`/`core_sha`；CLI before=after=`062cb56e…` |
| 3 | 打开/刷新/导出零副作用；执行只来自显式 POST；跨站/CSRF 全绿 | **PASS** | `no_exec_suite`（源/锁哈希不变、无 sentinel、导出无执行控件）、`security_suite`、`tests/security/` |
| 4 | SSE 生命周期/心跳/序号/断线重连/有界重放；无轮询 | **PASS** | `tests/integration/http_live.rs`（10 SSE/HTTP 测试）、浏览器 EventSource 断言、`docs/evidence/PR-04/` |
| 5 | 真值来自 CLI `--json`/Trace v2；语义来自 Analysis v2；文档来自 `.mq.md` | **PASS** | `execution/controller.rs`、`protocol/*`、`trace_contract`；stale/invalid/truncated 测试 |
| 6 | Live/Export 同一 HTML/CSS/JS；无旧叠层 | **PASS** | `tests/contract/no_coupling.rs`、`asset_integrity.rs`、`integration/export.rs`、`test-export.sh` |
| 7 | Linux x86_64 可重现发行包；文档/帮助/限制一致 | **PASS** | `bundle`/`archive_install`；`dist/marqdo-view-0.2.0-x86_64-unknown-linux-gnu.tar.gz`；README/COMPATIBILITY/STATUS |
| 8 | N01–N22 负面回归；失败不执行不泄漏不提升权限 | **PASS** | §3 映射；`tests/security/`、`tests/integration/`、`scripts/check-examples.sh`、`scripts/test-no-exec.sh` |

发布阻塞项（无意外执行失败、SSE 跨站读取/执行、过期 Trace 绑定、后台子进程
泄漏、CLI 核心被迫改动、导出行程序、伪造实时结果、双解释器）逐项均无。

## 2. §9.1 正向端到端

| ID | 测试 | 结果 | 证据 |
|---|---|---|---|
| E01 | Tool 安装/卸载/分发 `view` | PASS | acceptance `archive_install`/`tool_list`/`view_help`；`scripts/install-smoke.sh` |
| E02 | 打开含 `@knowledge`+H2 `@exec` 文件不运行 | PASS | `pages_issue_a_session_cookie_and_never_execute_the_document`；`no_exec_suite` |
| E03 | Source/Document 切换、字节保真 | PASS | `tests/integration/rendering.rs`、浏览器 source mode 断言 |
| E04 | 中文/Emoji/表格/公式 span | PASS | `tests/integration/unicode_spans.rs`、冻结 Analysis golden |
| E05 | 显式 `main` 运行（真实 run --json --trace-file） | PASS | acceptance `real_run`（真实冻结 CLI + SSE，285 条 Trace 事件） |
| E06 | H2 Exec `n=5` 调用 | PASS | `scripts/check-examples.sh`（Int 10）；`execution.rs` 的 call argv；前端表单 |
| E07 | 文本/列表常量参数（无 shell） | PASS | `build_call_args` 单测/集成；`--arg name=expression` argv 测试 |
| E08 | 无 main 不运行其他函数 | PASS | `document_without_main_...`（400 `view.no_entry_function`，无 run 调用） |
| E09 | if/while Trace 事件映射、不伪造变量 | PASS | `execution.rs` Trace 摘要；`at most one grouped chip`；无变量字段 |
| E10 | `GET …/events` 为 SSE、accepted/running/terminal、无 polling | PASS | `explicit_run_streams_sse_...`；浏览器请求记录 |
| E11 | Last-Event-ID 重连有序、无重复、过期 snapshot | PASS | `last_event_id_replays_...` |
| E12 | 长任务 SSE 心跳、GET 不重启 | PASS | `sse_heartbeats_keep_a_slow_job_connected`、`at_most_two_sse_observers_per_run` |
| E13 | 取消终止进程树、唯一终态 | PASS | `cancel_is_idempotent...`、`process.rs` 单测 |
| E14 | 静态导出无执行/无联网/相对链接 | PASS | acceptance `export`/`export_suite`；`test-export.sh`（file:// Chromium） |
| E15 | 页面刷新/重开不自动执行 | PASS | `pages_issue...`（10 次刷新无 run）；浏览器 |
| E16 | 运行后源码改变 → 结果可见但 stale、无错误高亮 | PASS | `source_edit_during_a_run...`（stale=true、全部 anchor/source_id=null） |

## 3. §9.2 必测失败与安全回归

| ID | 故障注入 | 结果 | 对应测试/证据 |
|---|---|---|---|
| N01 | 假 Analysis schema v3/缺 modules | PASS | `tests/contract/presentation_states.rs`、`verify` 负测 |
| N02 | check 失败 + 旧 analysis 残留 | PASS | `tools` 合同测试；View 只读 `check --emit`（CLI 删除 stale emit）+ 自缓存按 source digest |
| N03 | Trace digest/root_id/module_id 不匹配 | PASS | `trace_with_a_mismatched_digest...`、`trace_contract` |
| N04 | UTF-8 边界/span 越界 | PASS | `tests/contract/trace_contract.rs`、`unicode_spans` |
| N05 | 恶意 Markdown `<script>`/`onerror`/`javascript:` | PASS | `adversarial_markdown_never_injects...`；浏览器 security-project |
| N06 | `../`、`%2e%2e`、符号链接出根 | PASS | `path_traversal...`、`symlink_escape...` |
| N07 | 无 Cookie / 缺 CSRF / Origin 不符的 POST | PASS | `host_origin_session_csrf...`、`tests/security/` |
| N08 | 跨站 SSE 读别人的 run | PASS | `sse_requires_a_session...`（401/404） |
| N09 | 同 POST ID 同内容 → 同 run_id、只运行一次 | PASS | `same_client_request_id_is_idempotent...`、HTTP 版 |
| N10 | 同 ID 不同目标 → 409 | PASS | `view.run_conflict` 测试 |
| N11 | 运行中第二任务 → 409 busy | PASS | `second_run_is_busy...`、HTTP 版 |
| N12 | 伪造/过期 Last-Event-ID | PASS | `last_event_id_replays_...`（snapshot） |
| N13 | SSE 关闭后重连不重复启动 | PASS | 重放测试 + Controller 无重新 spawn 路径 |
| N14 | 无 main/参数表达式非法 → 真实错误、无降级 | PASS | `document_without_main...`、CLI 错误包络测试 |
| N15 | 输出超限/卡住/崩溃 → 上限与终态 | PASS | `process.rs` 有界输出/墙钟/取消单测；`view.output_limit` |
| N16 | 取消后延迟返回 → 唯一终态 | PASS | `cancel_is_idempotent...`（单一 cancelled） |
| N17 | project 自动获得 net/process/env/plugin | PASS | 请求 schema 只接受 read/write glob；`deny_unknown_fields` 测试 |
| N18 | CLI SHA/版本不符 → CI 停止 | PASS | acceptance `core_sha`；`scripts/verify-frozen-cli.sh` |
| N19 | 安装后修改 bundle 一字节 → 拒绝分发 | PASS | `docs/evidence/PR-01/README.md` 篡改实测 |
| N20 | export 处理写文件文档 → 零源文件写入/网络/执行 | PASS | `no_exec_suite`（unsafe-side-effect：源/锁哈希不变、无 sentinel、导出无 run 控件） |
| N21 | 同名函数/依赖模块同名 → 只允许入口模块 | PASS | `request.rs` function_id 解析与拒绝测试 |
| N22 | 后台/多标签 SSE 竞态与清理 | PASS | 2 连接上限、跨会话、TTL/环形缓冲测试 |

## 4. §9.3 浏览器视觉与可达性

真实 Chromium 131（Playwright 1.49）三个项目（welcome/control-flow/
security-project）全部 PASS：

- 宽度矩阵：2048×1133（源参考）、1664×920、1440×900、1024×768、768×1024、
  390×844：无横向溢出、Dock 在视口、无外部请求、无 console error；
- 顶部仅 `文档/函数/运行`；Dock 刷新/设置/运行/清理/更多都有真实行为；
- Welcome 只在显式 `view.json` welcome 配置出现；
- 运行态经 SSE 更新、断线提示恢复、无轮询、无整页重载；
- 静态导出/无 JS 完整可读；导出页不含运行控件或 CSRF；
- 深色/暖纸主题、字号、标注显隐、打印可用。

**UNVERIFIED（人工项）**：与旧参考截图的像素级人工比对、完整 screen-reader
走查。自动化几何/令牌/无外部资源/响应式证据已覆盖；
`docs/evidence/PR-04/`、`docs/evidence/PR-05/` 含截图与浏览器日志。

## 5. 复现

```bash
export MARQDO_BIN=/abs/path/to/frozen/marqdo-3.0.0-beta.1
scripts/verify-frozen-cli.sh "$MARQDO_BIN" -- "$MARQDO_BIN" --version
scripts/acceptance.sh
cat docs/evidence/2c7c4f31e7a388d3fcfa717a77067e696ed13854/manifest.json
```

单项：

```bash
cargo test --locked --offline            # unit + contract + integration + security
MARQDO_BIN=... scripts/check-examples.sh # RFC §13.3 fixtures
MARQDO_BIN=... scripts/test-no-exec.sh   # §13.4 零副作用
MARQDO_BIN=... scripts/test-export.sh    # 离线导出
MARQDO_BIN=... scripts/test-browser.sh   # 真实 Chromium
```

## 6. 诚实限制

- 运行仅 `pure`（默认）与受控 `project` fs read/write；`strict`、网络/进程/环境/
  plugin/provider/kernel 不暴露到浏览器，需终端 CLI。
- `project` 不是 OS 沙箱；View 不把项目 capability 声明自动提升为授权。
- SSE 不承诺逐字 stdout/Token 流；Trace v2 不含参数实际值/逐语句变量/内存快照。
- 平台：Linux x86_64 正式；Windows/macOS 不支持声明。
- `.github/workflows/ci.yml` 已提供，但**远端 CI 未在本环境实际运行**；上述证据
  均来自本地可复现命令与真实进程/浏览器。


## 7. Issue #2 复验（2026-11-22）

实现提交 `34de5a34d9cd8488ae4d753aff7d27b9c95379f3` 使用 Gitea 官方
`v3.0.0-beta.1` Linux x86_64 归档进行验收。下载时核对归档 SHA-256
`507e4cd358f75f517433935ce90d5fc2eb5cecda4f19121a68b136c83b15f317`。
二进制 SHA-256 在验收前后一致。版本、源码与工件身份见
[机器验收清单](evidence/issue-2/manifest.json)。

- Rust 1.100.0 的格式、Clippy、158 项测试与 release 构建均通过。
  锁定的 Comrak 依赖要求 Rust 1.89 起，CI 使用 Rust 1.89.0。
- Chromium 131 通过四个项目的交互和响应式检查。浏览器消费已安装工具
  的真实 SSE，断线后按原 `Last-Event-ID` 重连，全程只提交一次运行。
  无 `main` 进入函数清单，必填项缺失不提交，`double(5)` 得到
  `Int(10)`，文本和列表表单得到真实的 Unicode 返回值。
- 会话、取消、限额、Trace 绑定和过期降级通过现有回归。安装包篡改
  和错误 CLI 摘要的拒绝路径也纳入一键验收。
- 冷装环境发现官方归档没有 `stdlib/`。验收示例使用发布二进制的 stdout、Memory
  与文件资源原语。重新从禁网容器采集 Analysis/Trace 金样，控制流生成
  27 条真实事件，stdout 为三行 `0`、`0`、`2`，返回 `Int(3)`。
- 在仅挂载两个归档和样例的禁网 Ubuntu 24.04 容器中验证 glibc 2.39、
  Tool 安装与卸载、静态校验、离线导出和真实调用。容器内无 Cargo。
- 对照旧参考图复看 2048×1133 的欢迎页，保留暖纸、樱花、品牌和 Dock，
  导航为文档、函数、运行。辅助技术覆盖为浏览器键盘和控件可访问名称测试。

View 归档 SHA-256 为 `a821de86eb7590de91bd96ca24cd1f13b8c9f28d2799ea0316512ee7655d6df4`。
CI 使用 `scripts/frozen-beta.sh` 获取发行资产并调用相同的验收入口。
