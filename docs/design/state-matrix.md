# 运行状态矩阵（0.3.0 设计基准）

> 状态：**设计基准**。实现真源：`src/execution/events.rs` 的 `RunDisplayState`、
> `src/protocol/observation_v1.rs` 的校验结果与 `assets/reader.css` 的
> `[data-run-state]`。当前契约：Analysis revision 3 + Observation revision 1。
> 旧 RFC 仅供历史参考，不再是当前契约来源。

## 1. 静态来源状态（文档头部 `source_status`）

| 值 | 触发条件 | 文档正文 | 语义标注 | 运行按钮 | 文案（中文） |
|---|---|---|---|---|---|
| `CORE_UNAVAILABLE` | 无 `MARQDO_BIN`/核心不可执行 | 完整可读 | 无（“语言语义未验证”） | 禁用 + 说明 | 找不到冻结核心，语义未验证 |
| `STATIC_OK` | `check --emit` 退出 0 且 Analysis revision 3 校验通过 | 完整可读 | 有效 | 可用（需确认） | 静态检查已通过 · 未运行 |
| `CORE_ERROR` | `check` 退出非 0，但 Markdown 可读 | 完整可读 | 全部撤销，显示 Core diagnostics | 禁用 | Core 检查失败，语义标注不可用 |
| `STALE` | 源码变化或 Analysis digest 不匹配 | 完整可读 | 不绑定旧 Analysis | 可用（重新检查） | 分析已过期，请刷新 |
| `PROTOCOL_INCOMPATIBLE` | 未知 schema/revision / 产品版本不兼容 | 完整可读 | 无 | 禁用 | 核心协议不兼容（需 rc.4） |
| `SOURCE_MISMATCH` | Analysis `source_digest` ≠ 当前字节 | 完整可读 | 不展示 | 禁用 | 源码与 Analysis 不一致 |

## 2. 运行状态（`RunDisplayState`）

| 值 | 触发条件 | 视觉 | 说明 |
|---|---|---|---|
| `NOT_RUN` | 未点击运行 | 中性 outline | “未运行”；不出现任何结果 |
| `STATIC_ONLY` | 只有 `check` 成功 | 中性 | “仅静态检查通过” |
| `RUNNING` | 用户已确认，Core 子进程运行中 | 金色脉冲（reduced-motion 时静态） | 显示入口/模式/预算，可取消 |
| `SUCCEEDED` | `run.completed` + Observation 绑定校验通过 | `--mq-ok` | 显示真实事件与 stdout；值按 Value Wire 文本展示 |
| `FAILED` | `run.failed` / `run.rejected` | `--mq-error` | 显示错误码与 stderr 文本（转义） |
| `STALE` | `run.started.payload.source.analysis_digest` 与当前 Analysis 不符 | `--mq-warn` | “结果已过期”，不叠到当前正文 |
| `TRUNCATED` | `observation.truncated` / `capture.truncated` | `--mq-warn` | “观测被截断，仅显示已记录事件” |
| `INCOMPLETE` | EOF/中断且无终态 | `--mq-warn` | “未完成/无结论”，不显示成功 |
| `UNSUPPORTED` | 需要的字段/能力不在公开协议 | 中性 + 明确说明 | 绝不伪造：默认无变量值、无逐句赋值、无内存快照、无证明状态 |

## 3. 事件 → UI 映射（保守）

| Observation 事件 | 允许显示 | 禁止显示 |
|---|---|---|
| `call.enter` | 函数 id/名称、顺序、来源 span | 参数实际值（未显式选择时） |
| `call.exit` | 退出类型/错误码 | 最终变量表 |
| `branch.eval` | 真/假 + 来源 span | “所有分支已覆盖” |
| `loop.iteration` | continue/exit 次数（以事件完整性为界） | 由静态结构推测总迭代数 |
| `control.break` / `control.continue` | 发生位置与最近循环帧 | 推测循环总次数 |
| `return` | 发生返回 + 值类型名 | 未公开的具体值 |
| `effect` | operation、结果码、来源位置 | “权限安全/已证明” |
| `stdout.chunk` / `stderr.chunk` | base64 解码后的转义文本 | 当 HTML 注入 |
| `parameter.observed` / `variable.assigned` | 仅有显式选择时的名称/类型/位置；Text 需二次同意 | 未选择的默认值；内存/资源/句柄 |
| `observation.truncated` | 明确截断原因与上限 | 把缺失事件说成未执行 |
| 未知 kind | 拒绝或保守忽略（记录） | 猜测语义 |

## 4. 引用状态渲染

| state | 文档内表现 | 是否可跳转 |
|---|---|---|
| `resolved` | 正常链接 | 是（module/file） |
| `not_loaded` | “未核验”提示 | 否（不显示为断链） |
| `missing` | 可行动错误 | 否 |
| `ambiguous` | 列出 candidates 供选择 | 否（不静默选第一个） |
| `external` | 外链图标，不联网 | 否（仅展示 URL） |
| `outside_scope` | 可行动错误 | 否 |
| `invalid` | 可行动错误 | 否 |

## 5. 能力边界（首版硬性）

| 旧 UI 能力 | 当前 View | 理由 |
|---|---|---|
| 每个语句执行后的变量值 | **默认不请求、不展示** | 协议支持显式选择；View 0.3.0 不请求 |
| 参数实际值 / 内存快照 | **默认不请求、不展示** | 同上；内存/句柄永不发布 |
| 知识“已证明” | **UNSUPPORTED** | 需要独立 Prove 协议与证书 |
| 调试断点/单步 | **不实现** | 首版非目标 |
| 函数大纲/签名 | 支持（来自 Analysis `functions[]`） | 公开静态事实 |
| 静态变量信息（params/赋值目标） | 支持但明确标注“静态” | 来自 Analysis，不冒充运行值 |
| 静态导入/引用标签 | 支持（含七态） | 来自 Analysis `imports[]`/`references[]` |
| 运行 Observation 事件侧注 | 支持（含 stale/truncated 降级） | Observation v1 + digest 校验 |
