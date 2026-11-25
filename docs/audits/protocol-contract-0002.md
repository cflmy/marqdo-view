# 冻结 CLI 公共合同黑盒审计（marqdo-view 0.3.0 / rc.4）

- 状态：已用发行二进制黑盒复现；**CLI 源码只读，不修改、不重编译**
- 日期：2026-11-25
- 发行基线：`marqdo 3.0.0-rc.4`（tag `v3.0.0-rc.4`，
  源码提交 `452e9a32bd9e40dba427d4180691c58bab74449e`）
- 归档：`marqdo-3.0.0-rc.4-x86_64-unknown-linux-gnu.tar.gz`，
  SHA-256 `9c3f166b5e5b8840d39d10019074e4e1c42432fc23e26c69670e64fdcf4e15d8`
- 二进制（归档内 `marqdo`）：
  `/root/work/marqdo-cli/dist/marqdo-3.0.0-rc.4-x86_64-unknown-linux-gnu/marqdo`，
  SHA-256 `a7306962689c635445a59043971ea2eb09298e01feeee81f0ba07b1f3e10306e`
- 平台：`x86_64-unknown-linux-gnu`；当前契约：CLI Tool revision 1（`api_revision` 1）/ 
  Analysis revision 3（ADR-0069 公共边界）/ Observation revision 1 /
  Markdown profile `marqdo.md.v1`
- 规范来源：`marqdo-cli@v3.0.0-rc.4` 的 `spec/{analysis,observation,cli-tool,references,
  markdown-profile,value-wire}.md`、`docs/adr/0068-*.md`、`docs/adr/0069-*.md`；
  正式发布后若规范更新，以**确切发布版**为准，不以本文代替规范。

> 本合同摘要依据“规范 + 黑盒复现二选一”。未复现项显式标 `UNVERIFIED`，绝不推断。
> 本文件只描述**当前**契约；0.2.0 时代的旧格式已从当前树与当前文档移除，
> 仅保留在 Git 历史与 CHANGELOG 的 History 段。

## 1. 命令面（`--help` 实测）

```text
marqdo 3.0.0-rc.4
  run      Run the entry function (`main` by default) of a document
  call     Call a named function with typed arguments
  check    Statically check a document without executing it
  inspect  Inspect AST, units, signatures, IR, references or platform support
  ...
  tool     Manage and dispatch generic external CLI tools
```

- `check [FILE] [--refs] [--emit PATH] [--json]`：纯静态，不执行。
  `--json` 只输出 check 诊断摘要；`--emit` 是完整 Analysis 的唯一写出者。
- `run`/`call`：默认 `--mode project`（View 必须显式传 `--mode pure` 或带白名单的
  `--mode project`）。`--output text|json|jsonl` 默认 `text`；`--observation-file PATH`
  原子发布同一执行的快照。`--observe-variable NAME`、`--observe-parameter NAME`、
  `--observe-text-values`、`--observe-max-events`、`--observe-max-bytes`、
  `--observe-max-value-bytes` 控制观测选择与预算。
- 已退休的运行参数在当前 CLI 中直接触发“unexpected argument”并以 exit 2 结束；
  View 不得在失败后换旧参数重试。
- 预算/权限参数：`--fuel`、`--stack`、`--max-output`、`--max-memory`、
  `--timeout`、`--allow-read/--allow-write/...`。View 首版只使用
  `--mode` 与 `--allow-read/--allow-write`（且仅在显式 `project` 请求中）。
- `inspect --ast` 可用于人工调试，但**不得**作为渲染语义的机器来源；渲染语义只来自
  同一次 `check --emit`。

## 2. `check --emit` 合同

实测（`examples/control-flow/index.mq.md`、`examples/function-params.mq.md`）：

| 场景 | 观察 |
|---|---|
| 成功 | exit 0；`--json` stdout 输出 `success:true` 诊断摘要；`--emit` 路径写出 Analysis revision 3 的精确 UTF-8 字节（pretty JSON），owner-only 临时文件 + 原子 rename |
| 失败 | exit 1（静态诊断错误）或 exit 2（参数/入口不可读/输出发布失败）；不发布成功工件，可能按合同删除调用者指定的旧 emit 文件 |
| `--refs` | 额外链接检查，改变 diagnostics，因此改变 artifact 字节；不能替代普通 emit 作为运行绑定 |
| 无绝对路径 | 相同逻辑项目从不同 checkout 目录 emit 字节一致；错误位置只用 `relative_path`/`line`/`column` |

`AnalysisDigest` = `sha256:` + 精确 emit 文件字节的小写 hex SHA-256（含格式）。
Observation `run.started.payload.source.analysis_digest` 必须与它一致。

## 3. Analysis revision 3 关键字段（View 消费面）

实测顶层与模块字段：

```text
top-level: schema="marqdo.analysis", schema_version=3,
           producer{name,version,markdown_profile},
           project{root_id, identity_kind, entry_module_id, lock_digest,
                   manifest_digest, stdlib, extensions},
           modules[], diagnostics[]
module:   id, package_instance_id, relative_path, source_digest, source,
          document, units[], anchors[], functions[], imports[], references[],
          calls[], resources[]
unit:     id, path[](结构化), parent_id, kind, name, level, mode, mode_origin,
          mode_source_unit_id, heading_span, span, function
function: id, unit_id, local_name, namespace, span, heading_span, signature, effects, body
reference:source, source_unit, url, span, state, target_kind, target_module,
          target_unit, candidate_package_instance_id, candidate_path, fragment, candidates
```

关键语义：

- `source` 是精确 UTF-8 源文本；所有 `span` 是 half-open UTF-8 字节区间，
  `source_digest` = `sha256(source 解码字节)`。消费者不得从 `relative_path` 重读文件
  重建 span。
- `units` 的 `heading_span` = 标题 AST 节点范围（标题 UI/目录/锚点）；
  完整 `span` = 到下一个同级或更高层标题（或 EOF）的 section 范围（内容高亮/切片/归属）。
  `path` 是结构化数组；不得 split display id。
- `anchors` 是 `[{anchor, unit_id}]`，anchor 是导航名称，不是身份。
- `calls[].state` 为 `function|builtin|plugin|unresolved`；`unresolved(dynamic|unbound)`
  是合法静态结果，不是编译错误。`target_owner` 只携带静态身份，不伪造函数节点。
- `resources` 只投影 `formula`/`code`；`content_digest` 对应 AST literal；
  公式 `display` 为 bool，代码为 `null`。
- `diagnostics[]` 已包含文件内诊断；不得用 `inspect` 或私有 IR 生成第二套。
- `effects` 是保守摘要（如 `["unknown"]`），不是纯度证明，不能作为“安全运行”依据。

## 4. 引用状态（实测 fixture）

用 `examples` 之外的最小 fixture（单文档，未加载其它模块）实测：

| 输入 | state | 观察 |
|---|---|---|
| `#引用状态`（同页标题） | `resolved` | `target_kind: module`，fragment 已解码，target_unit 存在 |
| `./theory.mq.md`（未加载） | `not_loaded` | `target_unit: null`；表示当前 catalog 未核验，**不是** missing |
| `#不存在的标题` | `missing` | target fields null |
| `https://example.com/x` | `external` | 不联网；不是错误 |
| `../../../etc/passwd` | `outside_scope` | 不读目标；不输出物理路径 |
| `%zz` | `invalid` | 不做猜测修复 |

`check --refs` 会构建显式扩展 catalog 并追加诊断；如果该次检查带 error，则 exit 1
且不发布 artifact。View 的引用 UI 必须按 `state` 渲染：
`not_loaded` 显示“未核验”，`ambiguous` 列出 candidates，绝不静默选第一个。

**`UNVERIFIED`**：`ambiguous` 的稳定黑盒样本（需要两个同名 canonical 候选）与
`resolved` 的 `target_kind: file` 分支未在本次探针中复现；实现按规范校验，证据待补。

## 5. Observation revision 1

实测（`call function-params.mq.md double --arg n=5 --mode pure --output jsonl
--observation-file obs.json`）：

- 事件：`stream.opened`、`run.started`、`call.enter`、`parameter.observed`、
  `return`、`call.exit`、`run.completed`；每行 `schema:"marqdo.observation"`、
  `schema_version:1`、同一 `run_id`、连续 `seq`（0 起）。
- 快照：`{schema, schema_version, run{id, source{analysis_digest,root_id}, status},
  capture{level, ...}, events[]}`；同一次执行的 JSONL 行与快照 `events` 逐项相等。
- `run.completed.payload.value` 是 Value Wire（如 `{"type":"int","decimal":"10"}`）。
- `--observe-max-events 2` 实测：`capture.truncated={reason:"event_budget",limit:2,
  written:2}`，事件含 `observation.truncated`，最后仍有真实终态；截断**不代表**
  程序失败，UI 必须显示 TRUNCATED，不得声明完整。
- 事件错误位置只携带 `module_id`/`relative_path`/`line`/`column`/`span`；
  不出现绝对主机路径。
- `stdout.chunk`/`stderr.chunk` 是 base64 字节块；解码后必须与程序输出一致。
- `parameter.observed`/`variable.assigned` 默认不携带值；只有显式
  `--observe-parameter`/`--observe-variable`（Text 另需 `--observe-text-values`）
  才发布所选标量。内存/资源/句柄永不发布。
- `--mode strict` 只支持 text/JSON stdout；`--output jsonl` 与 `--observation-file`
  在启动前 exit 2（`strict.unsupported_grant`）。View 明确拒绝，不回退 `project`。
- EOF/中断且无终态 = 未完成/无结论，不得渲染为成功。

**`UNVERIFIED`**：`run.rejected` 的完整错误码矩阵、`effect.operation` 的具体集合、
多模块事件的跨模块归属由 View 测试覆盖；本审计只复现了 entry 单模块样本。

## 6. Tool revision 1（View 打包/安装依据）

规范 `spec/cli-tool.md`；实测：

```json
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "marqdo-view",
  "version": "0.3.0",
  "command": "view",
  "entrypoint": "bin/marqdo-view",
  "requires": {"api_revision": 1, "analysis": 3, "observation": 1}
}
```

- 必填精确值：`requires.api_revision = 1`；声明 `analysis`/`observation` 时必须是
  单个精确整数。没有 `compatibility` 字段、没有版本数组协商。
- 实测 `tool install` 成功 → `tool list` 输出 `demo-view 0.3.0 (marqdo-view)` →
  分发时注入 `MARQDO_BIN` 与 `MARQDO_API_REVISION=1`，不注入旧协商变量。
- 实测旧清单（带 `compatibility`）安装失败：`invalid tool.json: unknown field
  compatibility ... Build the tool with current exact protocol requirements`，
  无迁移器。
- 安装：校验 schema/SemVer/revision/entrypoint 不越界/命令冲突 → 整包规范化文件
  清单（path/size/sha256，拒绝符号链接/特殊文件）→ staging 复制复算 → 原子发布；
  分发前重算磁盘整包清单，任一字节被改即拒绝。
- 这些环境变量只表示 API revision，**不是**执行授权；权限仍由 `--mode`/grants 决定。

## 7. 对 View 实现的硬性含义

1. 一次入口一次 `check --emit`：保存精确 Analysis 字节 + digest；`inspect --ast`
   不再作为机器来源。
2. 运行只解析 Observation v1：JSONL 流是生产来源，原子快照用于测试等价断言；
   View 不双读合并、不重编号、不重排、不合成。
3. 运行 argv 由 `core_client`/`process` 以数组构造：`run|call --output jsonl
   --observation-file <tmp>`；显式 `--mode pure` 或白名单 grants；永不经过 shell。
4. Trace 绑定改为 Observation 绑定：`source.analysis_digest` + `root_id` +
   `module_id` + 模块 `source_digest` + UTF-8 半开 span；任一不符 →
   `view.observation_stale` 或等价错误，不显示成功、不错误高亮。
5. `--refs` 改变 artifact 字节，不能用于运行绑定；需要磁盘实时性时重新 check 并比较 digest。
6. 未知字段/旧同 revision 制品显式拒绝并给出可执行提示（重新生成/升级核心/重装 Tool）。
7. 值默认不请求、不发布；不伪造变量/内存状态。
8. 失败/截断/未完成必须诚实呈现；`UNSUPPORTED`/`PARTIAL`/`WARN` 不得写成 PASS。

## 8. 黑盒证据表（2026-11-25，rc.4 二进制）

| # | 命令（临时目录内） | 观察 |
|---|---|---|
| 1 | `marqdo --version` | `marqdo 3.0.0-rc.4`；SHA 见头部 |
| 2 | `marqdo check --help` / `run --help` / `call --help` | `--emit/--refs/--json`；`--output text|json|jsonl`、`--observation-file`、`--observe-*`、strict 支持范围 |
| 3 | `check control-flow/index.mq.md --emit cf.json --json` | exit 0；schema `marqdo.analysis` revision 3；模块含 source/document/units/anchors/functions/calls/resources/diagnostics |
| 4 | `call function-params.mq.md double --arg n=5 --mode pure --output jsonl --observation-file obs.json` | exit 0；7 个事件；JSONL 与快照 events 完全一致；value Int 10 |
| 5 | `run branch-loop.mq.md --mode pure --output jsonl --observation-file runobs.json` | exit 0；事件含 variable.assigned/loop.iteration/return；14 条；快照一致；value Int 0 |
| 6 | `run branch-loop.mq.md --observe-max-events 2 ...` | exit 0；`capture.truncated={event_budget,2,2}`；`observation.truncated` 后仍有终态 |
| 7 | 引用 fixture（resolved/not_loaded/missing/external/outside_scope/invalid） | 六态逐一复现，见 §4 |
| 8 | 传入已退休的旧运行参数（Git 历史保留，不在当前文档重复） | exit 2，unexpected argument；无回退重试 |
| 9 | `run … --mode strict --output jsonl` | exit 2，`strict.unsupported_grant` |
| 10 | 新 `tool.json` install/list/dispatch | 安装成功；`MARQDO_API_REVISION=1`；无旧协商变量 |
| 11 | 带 `compatibility` 的旧 `tool.json` install | exit 1，unknown field + 重写提示 |

## 9. 未复现项（UNVERIFIED）

- `ambiguous` 与 `resolved(target_kind:file)` 的稳定黑盒样本（见 §4）。
- 插件/Provider/Foreign/Kernel 的完整运行时行为（View 首版不暴露；不属 MVP）。
- `--fuel`/`--max-output`/`--max-memory`/`--stack` 触顶时的精确错误码与包络
  （由 CLI 负责；View 只显示真实错误）。
- 多模块事件的跨模块归属、`run.rejected` 全错误码矩阵、`effect.operation` 完整集合。
- Windows/macOS 行为（发行范围仅 Linux x86_64）。
- 远端 CI 实际执行（本环境未运行；对应门禁保持 `UNVERIFIED`）。
