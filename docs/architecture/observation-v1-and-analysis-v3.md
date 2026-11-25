# Observation v1 与 Analysis revision 3：当前分层

> 适用范围：`marqdo-view 0.3.0`，冻结 `marqdo 3.0.0-rc.4`。
> 本文是当前实现的架构基线；0.2.0 时期的 RFC 只保留为历史记录
> （`docs/architecture/RFC-VIEW-0001*.md`、`docs/architecture/RFC-VIEW-0002.md`）。

## 1. 唯一数据源

| 事实 | 唯一来源 | 只读产物 |
|---|---|---|
| 静态语义（source/document/units/anchors/functions/calls/resources/diagnostics/references） | 一次 `marqdo check ENTRY --emit PATH` | `marqdo.analysis` revision 3 精确 UTF-8 字节 + `sha256:` digest |
| 运行事实（生命周期/调用/分支/循环/返回/效果/输出字节/终态） | 一次 `marqdo run|call --output jsonl [--observation-file PATH]` | `marqdo.observation` revision 1 事件序列 / 原子快照 |
| 源绑定 | Observation `run.started.payload.source` | `analysis_digest` + `root_id`，对照同一入口、同一静态选项的 Analysis 精确字节 |

`inspect --ast`/`inspect --ir` 只用于人工调试，不是渲染/执行语义的机器来源。
不允许第二解释器、第二 Markdown AST、私有 IR 反推或包缓存物理路径依赖。

## 2. 分层与依赖方向

```text
CLI 适配层（src/core_client.rs, src/execution/process.rs）
  唯一知道 argv / 环境变量 / 退出码语义
  ├─ check --emit
  └─ run|call --output jsonl --observation-file
        │
protocol 解码 + 结构校验（src/protocol/{analysis_v3,observation_v1,tool_v1,value_wire_v1}.rs）
  schema/revision/必需字段/closed record/seq/终态/预算/绑定
        │
规范化领域模型（SemanticProjection、ObservationRun、RunBinding）
  无 CLI 细节、无版本分支、无 schema_version 判断
        │
消费层
  ├─ reader/{source,markdown,model,assemble,render}
  ├─ execution/{request,controller,events}
  ├─ server/{routes,security,sse}
  └─ export（同一 ReaderRenderer）
```

强制不变量：

1. 协议模块之外没有 `schema_version` 判断；协议模块内只有当前 revision 一个分支。
2. 依赖单向：`core_client → protocol → model`；`reader/execution/server → model`。
   模型不反向依赖 CLI/协议；reader 不依赖 core_client 细节。
3. JSONL 流与原子快照共用同一解码器与同一领域类型；生产代码只消费一种来源
   （Live 用流），两者一致性只在测试中断言，不做“双读合并”。
4. 每个拒绝都有稳定 `view.*` 错误码与可执行提示；不使用 `unwrap`/`expect`
   处理协议数据。
5. 事件顺序、`run_id` 一致性、连续 `seq`、唯一终态、`truncated` 语义由解码器集中校验；
   渲染/导出/SSE 层不“修正”协议数据。
6. 传输中断（EOF 无终态）显示“未完成/无结论”，不得渲染为成功。

## 3. Analysis revision 3 使用方式

- **一次入口一次 emit**：每个文档的静态事实都来自同一 artifact；不跨进程拼接
  `inspect --ast` 与 Analysis，避免跨快照错配。
- `modules[].source` 是精确 UTF-8 源文本，所有 span 是 half-open UTF-8 字节区间；
  `source_digest` 对应其解码字节。需要磁盘实时性时重新 `check` 并比较 digest，
  不从 `relative_path` 重读文件重建 span。
- 标题 UI/目录/锚点使用 `heading_span`；内容高亮/section 切片/标注归属使用完整
  `span`（到下一个同级或更高层标题或 EOF）。
- `units[].path` 是结构化数组；`parent_id`/`mode`/`mode_origin`/`mode_source_unit_id`
  直接使用；不 split display id。`anchors` 是导航名称，不是身份。
- `calls[]` 的 `state/target_kind/target_owner/reason` 用于执行标记；
  `unresolved(dynamic|unbound)` 是合法静态结果，不显示为编译错误。
- `resources`：`content_digest` 对应 AST literal；公式 `display` 为 bool，代码为 `null`。
- `diagnostics[]` 是唯一文件内诊断来源；不再运行第二套检查生成第二套诊断。
- 旧同 revision（缺 `source/document/anchors/entry_module_id` 等）artifact 显式拒绝：
  `view.protocol_incompatible`，提示重新生成或升级核心。

## 4. 引用状态

| state | 渲染语义 |
|---|---|
| `resolved` | 可跳转；`target_kind` 为 `module` 或 `file` |
| `not_loaded` | **未核验**：当前 catalog 未加载该本地目标；不是断链、不是 missing |
| `missing` | 目标模块存在但无对应 fragment，或显式核验证明文件不存在 → 可行动错误 |
| `ambiguous` | canonical-name 回退命中两个及以上候选；列出 `candidates`，绝不静默选第一个 |
| `external` | 外链；不联网、不是错误 |
| `outside_scope` | 本地目标逃出允许根/绝对路径/UNC → 不读目标；不输出物理路径 |
| `invalid` | URI/percent 编码非法 → 不猜测修复 |

`source_unit` 用于定位引用所属 section，`span` 用于高亮原文。普通 Analysis
不打开链接目标；`check --refs` 的额外诊断改变 artifact 字节，不能替代普通 emit
作为运行绑定。

## 5. Observation revision 1 使用方式

- 事件种类：`stream.opened`、`run.started`、`call.enter/exit`、`branch.eval`、
  `loop.iteration`、`control.break/continue`、`return`、`effect`、
  `parameter.observed`、`variable.assigned`、`observation.truncated`、
  `stdout.chunk`、`stderr.chunk`、`run.completed/failed/rejected`。
- 唯一 `run_id`、零基连续 `seq`、恰好一个终态且最后。View SSE 只转发/摘要，
  保留自身信封但不重编号、不重排、不合成协议事件。
- 绑定校验：`analysis_digest` 必须等于普通 `check --emit` 精确字节的
  `sha256:` 表达；`root_id` 等于 `project.root_id`；事件 `module_id` 属于
  `modules[]`；span 为该模块 source 的合法 UTF-8 半开区间；模块 `source_digest`
  与 artifact 的 `source` 一致。任一不符 → 不渲染运行结果、不错误高亮。
- `observation.truncated`/`capture.truncated` → 显示 **TRUNCATED**：预算耗尽，
  不代表程序失败，也不得声明完整覆盖。
- 字节输出：`stdout.chunk`/`stderr.chunk` 为 base64 + 解码字节数；渲染为转义文本，
  绝不当 HTML。
- 错误位置只用 `module_id`/`relative_path`/`line`/`column`/`span`；快照可对外分享，
  不出现绝对主机路径。
- EOF 无终态 → **INCOMPLETE/未完成**：不显示成功、不生成“成功”导出。
- 值：默认 `capture.level=structural`，不发布 `variable/parameter` 值；协议支持
  显式 `--observe-variable`/`--observe-parameter`（Text 另需 `--observe-text-values`），
  但 0.3.0 的 View **不请求任何值**。内存/资源/句柄永不发布。
- `strict`：CLI 只支持 text/JSON stdout；Live/文件目的地 exit 2 +
  `strict.unsupported_grant`。View 映射为明确错误，不回退 `project`、不重试旧参数。

## 6. 失败与降级矩阵

| 条件 | 错误/状态 | 用户可见行为 |
|---|---|---|
| 常量/加载错误 | CLI exit 1，无成功 artifact | 显示诊断，语义撤回；不显示“成功” |
| 参数/入口/发布错误 | CLI exit 2 | `view.core_option_rejected` 或等价；不去参数重试 |
| 未知 schema/revision/kind | `view.protocol_incompatible` | 显式拒绝，提示升级/重装/重新生成 |
| 旧同 revision artifact | 协议错误 | 提示重新 `check --emit`；无 fallback 解码 |
| digest/root/module/span 不符 | `view.observation_stale` 或等价 | 不渲染运行结果、不错误高亮、不导出成功 |
| 预算截断 | `TRUNCATED` | 显示截断原因/限额，不声明完整 |
| EOF 无终态 | `INCOMPLETE/未完成` | 不显示成功、不生成成功导出 |
| 值未选择却出现 values | 协议违约 | 拒绝该 artifact/事件，不显示值 |
| 旧 Tool 清单/安装记录 | 安装/分发失败 | 提示 remove + 重装；无迁移器 |

## 7. 文件映射

| 层 | 代码 |
|---|---|
| CLI 适配 | `src/core_client.rs`、`src/execution/process.rs` |
| 解码/校验 | `src/protocol/analysis_v3.rs`、`src/protocol/observation_v1.rs`、`src/protocol/tool_v1.rs`、`src/protocol/value_wire_v1.rs` |
| 领域/绑定 | `src/execution/{request,controller,events}.rs` |
| 阅读/渲染 | `src/reader/*`、`assets/reader.css`、`assets/reader.js` |
| 服务/SSE | `src/server/{mod,routes,security,sse}.rs` |
| 导出 | `src/export*.rs`（同一 ReaderRenderer） |
| 门禁 | `tests/contract/no_coupling.rs`、`scripts/check-no-legacy.sh`、`scripts/check-docs.sh`、`scripts/check-observation.py` |

## 8. 验证入口

```bash
scripts/check-no-legacy.sh                         # G01：无旧契约残留
scripts/check-docs.sh                              # G12：当前文档事实一致
MARQDO_BIN=... scripts/check-examples.sh           # 示例真实 check/call/run
MARQDO_BIN=... python3 scripts/check-observation.py stream-snapshot
MARQDO_BIN=... python3 scripts/check-observation.py binding
MARQDO_BIN=... python3 scripts/check-observation.py fail-closed
MARQDO_BIN=... scripts/acceptance.sh               # G01–G12 + manifest
```

任何未运行、无法判定的门禁记录为 `UNVERIFIED`；不得写成 `PASS`。
