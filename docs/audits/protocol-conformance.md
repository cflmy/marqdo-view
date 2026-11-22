# 协议一致性审计（P-03）

> 结论：View 只消费冻结 CLI 的公开 Tool v1 / Analysis v2 / Trace v2 协议；
> 未实现或未提供的能力显式标注 `UNSUPPORTED`。本文把规范条款映射到实现与测试，
> 便于审阅者不依赖对话历史复核。

## 1. 权威来源

- `marqdo-cli` 发布二进制 `3.0.0-beta.1`，SHA-256
  `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f`
  （`docs/STATUS.md`、`tests/golden/README.md`、`docs/evidence/<SHA>/manifest.json`）。
- 规范阅读：`spec/tool-v1.md`、`spec/analysis-v2.md`、`spec/trace-v2.md`、
  `spec/mapping.md`、`spec/markdown-profile.md`、`spec/security.md`。
- 旧仓库只作为视觉/历史证据（`docs/audits/legacy-render-paths.md`），不是运行时依赖。

## 2. Tool Protocol v1

| 条款 | 实现 | 测试/证据 |
|---|---|---|
| `tool.json` schema/字段/SemVer/command/entrypoint | `tool.json`、`scripts/package.sh` | `tests/no_coupling.rs`；`scripts/install-smoke.sh` |
| 冻结核心安装、整包摘要、启动校验 | `scripts/install-smoke.sh`、`scripts/acceptance.sh` | acceptance `archive_install` PASS；手工篡改字节验证 README（PR-01 证据） |
| 环境变量 `MARQDO_BIN`/协议协商 | `src/core_client.rs`、`src/version.rs` | `tests/core_contract.rs`、`tests/http_live.rs` |
| argv 数组、无 shell、保留退出码/stdio | `src/core_client.rs` | `tests/core_contract.rs`（argv、timeout、fake core） |
| 不链接/不重编译/不读 CLI 源码 | 依赖图 | `tests/no_coupling.rs`、`docs/ADR/0001` |
| 篡改/缺文件/协议不兼容拒绝 | CLI 自身 + View 侧协商 | `tests/core_contract.rs`、`scripts/install-smoke.sh` |

## 3. Analysis v2

| 条款 | 实现 |
|---|---|
| `check --emit` 仅成功时写、失败删除旧产物 | `src/core_client.rs`（退出码/文件存在性双检） |
| `schema/schema_version=2`、未知 major 拒绝 | `src/semantic.rs`、`src/app.rs`、`verify` |
| `producer.markdown_profile=marqdo.md.v1` | `src/semantic.rs` |
| `project.root_id/identity_kind/lock_digest/stdlib` | `SemanticProjection` 字段 + `verify --json` |
| `source_digest` 精确字节校验 | `src/semantic.rs`、`tests/unicode_spans.rs`（CRLF） |
| modules/units/functions/signature/body kind/span | `src/semantic.rs`（typed projection） |
| `resources[]` formula/code + `display`/language | `StaticResource`，渲染为惰性资源 |
| `imports[]`/`references[]`/`calls[]` | 标签与引用状态；不重新解析优先级 |
| `diagnostics[]`（span/line/column/severity/code/hints） | 页面诊断区；`check --json` 失败路径 |
| `effects:["unknown"]` 不当作纯净证明 | UI 不显示“已证明”；`docs/design/state-matrix.md` |

## 4. Trace v2

| 条款 | 实现 |
|---|---|
| `schema/schema_version=2` | `src/trace.rs` |
| `sha256(analysis bytes)==source.analysis_digest` | `verify_trace`；`tests/trace_contract.rs` |
| `source.root_id==project.root_id` | `verify_trace` |
| 事件 `module_id` ∈ Analysis modules | `verify_trace` |
| `seq` 严格递增 | `verify_trace` |
| 未知 event kind 忽略（不解释） | `TraceKind::Other`，不产生语义结论 |
| `truncated != null` → TRUNCATED | `RunDisplayState::Truncated`；golden truncated |
| 无参数值/秘密/内存/赋值历史 | 数据模型不存在这些字段；UI 标 `UNSUPPORTED` |
| `run.status` / `run --json` 错误码 | `Failed` + `error_code`；golden arity 错误 |
| 公开最终返回值 | 仅 `run --json` 的 `value`，标“Core 本次返回值” |

## 5. Markdown profile / mapping

| 条款 | 实现 |
|---|---|
| Comrak `=1.0.0` 冻结选项集 | `src/document.rs::markdown_options` |
| 原始 Markdown 是阅读主体 | `DocumentModel` + `ReaderRenderer` |
| 不重新决定可执行性 | 只有 `semantic.rs` 的 Analysis 事实创建锚点 |
| `@exec/@knowledge` 显示前缀仅显示层剥离 | `docs/ADR/0003` |
| 知识区强调/行内代码不执行 | `tests/unicode_spans.rs::knowledge_emphasis_never_becomes_an_execution_anchor` |
| GFM 表/任务列表/围栏/数学可读且惰性 | `tests/rendering.rs`、`tests/export.rs` |
| 原始 HTML 默认忽略、URL scheme 净化 | `src/render.rs`、浏览器安全 fixture |

## 6. Security 边界（View 自身）

| 条款 | 实现 | 测试 |
|---|---|---|
| loopback-only + Host 校验 | `src/server.rs` | `tests/http_live.rs` |
| CSRF/Origin/确认 | `POST /api/run` | `tests/http_live.rs` |
| 路径边界/单次解码/无遍历 | `SafeRelativePath`、`percent_decode` | `tests/http_live.rs`、`tests/project` 单测 |
| 导出纯只读、无执行 | `src/export.rs` | `tests/export.rs`（marker 假核心） |
| 离线文件:// | 内嵌资产 + 相对链接 | `scripts/test-export.sh` Chromium file:// |
| 输出/错误不泄露绝对路径 | `ViewError::public_message`、manifest 相对路径 | `tests/http_live.rs`、`tests/export.rs` |
| 无第二渲染器/第二主题 | 代码门禁 | `tests/no_coupling.rs`、`tests/asset_integrity.rs` |

## 7. 明确 UNSUPPORTED / 非目标

- 逐语句赋值实际值、参数实际值、变量历史、Memory 全状态：Trace v2 不提供，UI 只显示静态分析目标并明确说明。
- 知识“已证明”/Prove/Lint/Knowledge 引擎：不实现，等独立版本化协议。
- 调试器/断点/单步、Notebook/旧 2.x 语法兼容、SSE 流式运行、多运行历史：非首版目标。
- `inspect --ir` / 私有 IR / 旧 `ViewExecutionReport`：没有作为依赖；`tests/no_coupling.rs` 门禁。
- 严格模式/项目 grants：首版运行仅 `pure`；`project`/`strict` 需要未来独立授权界面。

## 8. 复核命令

```bash
export MARQDO_BIN=/abs/path/to/frozen/marqdo
scripts/acceptance.sh          # §18.1 全链路 + evidence manifest
scripts/test-contract.sh       # code + Tool v1 + verify + export
scripts/test-export.sh         # 纯只读导出 + file:// Chromium
scripts/test-security.sh       # HTTP/路径/XSS/导出对抗
scripts/test-browser.sh        # 真实 Chromium 视觉/交互/响应式/运行
```
