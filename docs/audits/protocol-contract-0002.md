# 冻结 CLI 公共合同黑盒审计（RFC-VIEW-0002 §2 / PR-00）

- 状态：已用发行二进制黑盒复现；**CLI 源码只读，不修改、不重编译**
- 日期：2026-11-22
- 发行基线：`marqdo 3.0.0-beta.1`（tag `v3.0.0-beta.1`，
  `2c7c4f31e7a388d3fcfa717a77067e696ed13854`）
- 二进制：`/root/work/marqdo-cli/dist/marqdo-3.0.0-beta.1-x86_64-unknown-linux-gnu/marqdo`
- SHA-256：`062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f`
- 平台：`x86_64-unknown-linux-gnu`；协议：Tool v1 / Analysis v2 / Trace v2 /
  Markdown Profile `marqdo.md.v1`
- 规范来源：`/root/work/marqdo-cli/spec/{tool-v1,analysis-v2,trace-v2,callable-resolution,value-wire}.md`、
  `spec/schemas/*.json`、`crates/cli/src/commands/run.rs`（只读参考）。
  正式发布后若规范更新，以**确切发布版**为准，不以本文代替规范。

> 本文是 marqdo-view 实现与测试的合同摘要。任何字段都以“规范 + 黑盒复现二选一”
> 为依据；未复现项显式标 `UNVERIFIED`，绝不推断。

## 1. 命令面（`--help` 实测）

- `run [FILE]`：运行入口文件 `main`；`--mode` 默认 **`project`**（View 必须显式传
  `--mode pure`）。
- `call <FILE> <FUNCTION>`：调用命名函数；`--arg <NAME=VALUE>` 常量表达式，可重复。
- `check [FILE] [--refs] [--emit PATH] [--json]`：纯静态，不执行。
- 预算/追踪参数（`run`/`call` 共有）：`--fuel`、`--stack`、`--max-output`、
  `--timeout`、`--trace-file`、`--trace-max-events`、`--trace-max-bytes`。
- 权限参数：`--allow-read/--allow-write/--allow-net/--allow-env/--allow-process/
  --allow-random/--allow-foreign/--allow-plugin/--plugin/--provider/--allow-provider/
  --kernel/--trust-native-plugin`。View 首版只使用 `--allow-read/--allow-write`
  （且仅在显式 `project` 请求中），其余一律不暴露。
- 未知顶层命令通过 Tool v1 注册表分发到外部二进制（`spec/tool-v1.md`）。

## 2. `check --emit` 合同

规范：`spec/analysis-v2.md` §1；实测：

| 场景 | 结果 |
|---|---|
| 成功 | exit 0；`--json` stdout 为 `{"command":"check","diagnostics":[],"errors":0,"success":true}`；`--emit` 路径写出 Analysis v2 精确 UTF-8 字节（pretty JSON），原子写入 |
| 失败 | exit 1；stdout 为 `{"command":"check","diagnostics":[...],"errors":N,"success":false}`；**逐条诊断带 `code/severity/message/line/column/module/hints`**（`file` 可能出现绝对路径，View 必须不泄漏/只保留相对信息） |
| stale emit | 失败时 **删除目标路径的旧文件**（实测：先写 `stale.json` 再失败 check，文件不存在）。View 必须同样丢弃自己的旧 Analysis 缓存 |
| `--refs` | 额外链接检查，可能产生额外 `diagnostics`；不得当作语言执行/编译成功证明 |

`AnalysisDigest` = `sha256:` + 精确 emit 文件字节的小写 hex SHA-256；**不是**
反序列化再序列化的哈希。`Trace v2.source.analysis_digest` 必须与它一致。

## 3. Analysis v2 关键字段（View 消费面）

- 顶层：`schema="marqdo.analysis"`、`schema_version=2`、`producer{name,version,
  markdown_profile}`、`project{root_id,identity_kind,lock_digest,stdlib}`、
  `modules[]`、`diagnostics[]`。
- `project.root_id`：`root:<name>@<version>`（有 manifest）或
  `sha256:<workspace snapshot>`；是 Trace `source.root_id` 的唯一比对值。
- `modules[].{id,package_instance_id,relative_path,source_digest,units,functions,
  imports,references,calls,resources}`。`source_digest` = 精确源文件字节的 SHA-256
  （不含 `sha256:` 前缀）。
- `functions[].{id,unit_id,local_name,namespace,span,heading_span,signature{kind,
  params[]},body,effects}`；`params[].{name,required,type,default,span}`。
  `signature.kind` 为 `explicit|inferred`；`required = default.is_none()`。
- `units[].kind`：`root|namespace|function|knowledge|section`，带 `level`、
  `span`、`function`、`name`、`id`。**Analysis 不公开 H3+ 的最终 EffectiveMode**；
  View 不得自行重建并宣称为编译器状态（RFC §2.7）。
- `diagnostics[]` 标记 UTF-8 字节 `span`（`start/end`）或行列；View 显示原文与
  诊断即可，不从错误推断 IR。
- `effects` 是保守摘要（如 `["unknown"]`），不是纯度证明，不能作为“安全运行”依据。

黑盒样例（`target/probe/function-params.mq.md`，2 个 H1/H2 中文标题）证明：
中文 namespace/local_name、UTF-8 `span`、`heading_span`、`unit_id` 均按字节偏移
给出；`id` 形如
`root:<name>@<ver>::<relative path>::<namespace>.<local_name>`（无 namespace 时
`::<local_name>`）。

## 4. 函数定位与 `call` 的名称解析（关键）

CLI `call FILE FUNCTION` 的查找实现：`Module::function(name)`
（`crates/marqdo-ir/src/lib.rs:404`，只读参考），规则为：

1. 先按 **canonical qualified name**（`function.id.name`，如
   `计算示例.double`）完全匹配；
2. 否则按 `local_name` 匹配，且必须**唯一**；同名跨 namespace 重复时返回
   `None`，CLI 报 `sem.no_entry_function`，没有“文档顺序优先”。

实测：`call function-params.mq.md 计算示例.double --arg n=7 --mode pure` 成功；
`call function-params.mq.md double` 成功（唯一 local name）。因此 View 必须：

- `target.function_id` 从**当前入口模块** Analysis 的 `functions[]` 精确取得；
- 服务端把 `function_id` 解析为该函数的 CLI 名称：若 `local_name` 在入口模块内
  唯一（或与 namespace 无关地唯一）用 `local_name`，否则用
  `<namespace>.<local_name>`；**不能**把客户端字符串直接拼接为参数；
- 只允许入口模块函数；依赖模块函数不是 `call FILE FUNCTION` 的目标
  （RFC §6.3）。

## 5. `run` / `call` 的 JSON 包络（实测差异，必须容忍）

成功（`run` 与 `call` 同形）：

```json
{"command":"run|call","success":true,
 "value":{"type":"<wire tag>", "...": "..."},
 "stdout":"...","stderr":"...","diagnostics":[],
 "trace":[]}
```

`value` 是 Wire Value（`spec/value-wire.md` v1），**不是任意字符串**：

| type | 形状 |
|---|---|
| `unit` | `{"type":"unit"}` |
| `none` | `{"type":"none"}` |
| `bool` | `{"type":"bool","value":true}` |
| `int` | `{"type":"int","decimal":"-123..."}`（十进制字符串，不丢精度） |
| `float` | `{"type":"float","value":1.5}` 或 `{"type":"float","special":"nan|infinity|-infinity"}` |
| `text` | `{"type":"text","value":"..."}` |
| `list` | `{"type":"list","items":[...]}` |
| `map` | `{"type":"map","entries":{"k":<value>}}` |
| `table` | `{"type":"table","header":[...],"rows":[[...]]}` |
| `formula` / `code` | 惰性数据（latex/source 等） |
| `function_ref` | `{"type":"function_ref","module":"...","name":"..."}` |
| `plugin_handle` | `{"type":"plugin_handle",...}` |
| `memory` / `resource` | 仅 `{"type":"memory"}`/`{"type":"resource"}`，**不可解码/不应出现** |

失败包络**不统一**（实测两种，至少）：

```json
// A. prepare/semantic 失败（如无 main、函数不存在）：error 是字符串，另有顶层 code
{"command":"run","success":false,"code":"sem.no_entry_function",
 "error":"<file> has no callable `main`; ..."}
// B. 运行期失败（类型/预算等）：error 是对象，含 code/message/line/column/span/...
{"command":"call","success":false,
 "error":{"code":"runtime.type","message":"...","span":{...},"frames":[...]},
 "stdout":"","stderr":"","trace":[]}
```

**实现要求**：任何失败路径都不得 `unwrap` `value`/`trace`/`stdout`/`stderr`；
`success:true` 但 `value` 缺失时按 `unit` 谨慎显示；`success:false` 时保留
`code`（顶层或 `error.code`）和可读消息；stdout/stderr 一律当用户文本。

关于 `trace` 数组：`run --json` 顶层 `trace` 实测可能为 `[]`（即使
`--trace-file` 写了完整 Trace v2）。**结构化运行证据只以 `--trace-file` 文件为准**，
不能把 JSON 里的 `trace` 当 Trace v2。

## 6. Trace v2 合同（`--trace-file`）

规范：`spec/trace-v2.md`；实测 `target/probe/t2.json`：

- 顶层：`schema="marqdo.trace"`、`schema_version=2`、
  `run{id,entry,status}`、`source{root_id,analysis_digest}`、
  `limits{max_events,max_bytes}`、`truncated`、`events[]`。
- 事件：`seq`（全局递增）、`kind`、`function_id`、`module_id`、`span`、
  `detail`；`effect` 另有 `operation`/`duration_micros`。
- kind：`call.enter|call.exit|return|branch.eval|loop.iteration|effect|
  trace.truncated`；未来新增 kind 必须忽略或保守显示，不得猜测。
- 有 span 的事件必须指定真实 `module_id` 与 UTF-8 字节 `span`；
  无 span 的 `call.enter/call.exit`（entry call）可 `span:null`。
- `truncated:null | {"reason":"event_budget|byte_budget","limit":N,"written":M}`；
  截断只代表观测预算耗尽，不代表程序失败；末事件为 `trace.truncated`。
- **不记录**参数值、逐语句变量值、Handle、文件内容、秘密。View 不得伪造这些值。
- 认证绑定（RFC §2.6）：把 `source.analysis_digest` 与**同一源码、同一检查配置**
  的 Analysis 原始字节 SHA-256 比较；再核对 `source.root_id`、`module_id` 属于
  Analysis `modules[]`、span 边界/UTF-8 合法、模块 `source_digest` 等于当前源码
  字节哈希。任一不符 → 不画执行标注。
- 实测 `--trace-max-events 2`：`truncated={reason:event_budget,limit:2,written:2}`，
  events 数 3（含 `trace.truncated`）。View 的 `--trace-max-events 10000`、
  `--trace-max-bytes 4194304` 是上限而非保证。

## 7. Tool v1 安装/分发（View 打包依据）

规范：`spec/tool-v1.md`。要点（源码 + 既有验收证据）：

- `tool.json` 必填：`schema="marqdo.cli-tool"`、`schema_version=1`、`name`、
  `version`（合法 SemVer）、`command`、`entrypoint`（包内受控相对路径）、
  `compatibility{marqdo:"<semver range>", analysis:[2], trace:[2]}`。
- 安装：校验 schema/SemVer/协议交集/entrypoint 不越界/命令冲突 → 收集整包
  规范化文件清单（path/size/sha256，拒绝符号链接/特殊文件）→ staging 复制并
  复算一致 → 原子发布；失败清理 staging。
- 分发：启动前重新计算磁盘整包清单并与记录逐项比较；**修改已安装包任意一个字节
  后分发被拒绝**（tests 已覆盖，见 `tools/install-smoke.sh`）。
- 运行时向工具注入 `MARQDO_TOOL_PROTOCOL=1`、
  `MARQDO_PROTOCOL_ANALYSIS=2`、`MARQDO_PROTOCOL_TRACE=2`（兼容协商成功时）；
  原样保留 cwd/stdin/stdout/stderr 与可保留环境，用
  `Command::new(entrypoint).args(args)` 直接启动，不经过 shell，退出码原样传播。
- 这些环境变量只表示协议版本，**不是**执行授权；权限仍由 `--mode`/grants 决定。

## 8. 对 View 实现的硬性含义

1. 运行请求必须由服务端重新 `check --emit` 并保存**精确 Analysis 字节 + digest**；
   POST 校验顺序中的 Analysis SHA 比对以它为唯一参照，运行后 Trace 也绑定它。
2. `run` 与 `call` 的 argv 由 `core_client`/`process` 以数组构造，显式
   `--mode pure` 或带白名单 grants 的 `project`；永不经过 shell。
3. 显式预算：`--timeout 30`（View 另设墙钟上限，默认 30s、最大 120s）、
   `--fuel 1000000`、`--max-output 1048576`、`--trace-max-events 10000`、
   `--trace-max-bytes 4194304`；接收侧再设 4 MiB JSON / 4 MiB trace 上限。
4. Value 展示按 §5 白名单类型做结构化文本；未知类型折叠为转义 JSON；不得
   把 `value` 当变量全集、不得当 HTML。
5. `call` 的目标名按 §4 由 function_id 精确解析；无 main 时 `run` 会返回
   `sem.no_entry_function`（实测），View 应提前提示并引导选择函数，绝不替换成别的函数。
6. 运行失败时 `trace` 文件可能不存在（实测 no-main 无 trace 文件）；视为
   “Trace 不可用”，不是伪造空 Trace。

## 9. 黑盒证据表

| # | 命令（在 `target/probe` 内） | 观察 |
|---|---|---|
| 1 | `marqdo --version` | `marqdo 3.0.0-beta.1`；SHA 见头部 |
| 2 | `check function-params.mq.md --emit a1.json --json` | exit 0，success:true，写出 Analysis v2 |
| 3 | `check broken.mq.md --emit stale.json --json` | exit 1，failure JSON，旧 `stale.json` 被删除 |
| 4 | `call function-params.mq.md double --arg n=5 --mode pure --json --trace-file t1.json` | exit 0，`value={type:int,decimal:10}`，Trace v2 写入 |
| 5 | `call function-params.mq.md 计算示例.double --arg n=7 --mode pure` | exit 0，`decimal:14`（canonical 名称可用） |
| 6 | `call function-params.mq.md double --arg 'n="x"' --mode pure --json` | exit 1，`error.code=runtime.type`（对象包络） |
| 7 | `call function-params.mq.md double`（缺 required 参数） | exit 1，`error.code=call.arity` |
| 8 | `call function-params.mq.md nope --mode pure` | exit 1，顶层 `code=sem.no_entry_function`，`error` 为字符串 |
| 9 | `run branch-loop.mq.md --mode pure --json --trace-file t2.json` | exit 0，value int 0；Trace 事件含 loop.iteration/return/call.enter/call.exit |
| 10 | `run branch-loop.mq.md --trace-max-events 2 --trace-file lim.json` | exit 0，`truncated={event_budget,limit:2,written:2}` |
| 11 | `run nomain.mq.md --mode pure --json --trace-file nomain-trace.json` | exit 1，`sem.no_entry_function`，**无 trace 文件** |
| 12 | `run branch-loop.mq.md --json`（未传 mode） | 成功，但 `--help` 证明默认值是 `project`（View 不能依赖此默认） |

## 10. 未复现项（UNVERIFIED）

- 插件/Provider/Foreign/Kernel 的完整运行时行为（View 首版不暴露；不属 MVP）。
- `--max-output`/`--fuel`/`--stack` 触顶时的精确错误码与包络（由 View 外部的
  Core 负责；View 只显示真实错误）。
- 多模块/依赖包事件在入口正文的跨模块绘制（必须按 module_id 拒绝；无有效样本）。
- Windows/macOS 行为（发行范围仅 Linux x86_64）。
