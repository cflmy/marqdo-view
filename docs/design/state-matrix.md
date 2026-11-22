# 运行状态矩阵（PR-00 设计基准）

> 状态：**设计基准**。实现真源：`src/presentation.rs` 的 `RunDisplayState` 与 `assets/reader.css` 的 `[data-run-state]`。
> 来源：RFC-VIEW-0001 §4.4、§12.2–§12.3。

## 1. 静态来源状态（文档头部 `source_status`）

| 值 | 触发条件 | 文档正文 | 语义标注 | 运行按钮 | 文案（中文） |
|---|---|---|---|---|---|
| `CORE_UNAVAILABLE` | 无 `MARQDO_BIN`/核心不可执行 | 完整可读 | 无（“语言语义未验证”） | 禁用 + 说明 | 找不到冻结核心，语义未验证 |
| `STATIC_OK` | `check` 退出 0 且 Analysis v2 校验通过 | 完整可读 | 有效 | 可用（需确认） | 静态检查已通过 · 未运行 |
| `CORE_ERROR` | `check` 退出非 0，但 Markdown 可读 | 完整可读 | 全部撤销，显示 Core diagnostics | 禁用 | Core 检查失败，语义标注不可用 |
| `STALE` | 源码变化或 Analysis digest 不匹配 | 完整可读 | 不绑定旧 Analysis | 可用（重新检查） | 分析已过期，请刷新 |
| `PROTOCOL_INCOMPATIBLE` | 未知 schema major / 产品版本不兼容 | 完整可读 | 无 | 禁用 | 核心协议不兼容（analysis vN） |
| `SOURCE_MISMATCH` | Analysis `source_digest` ≠ 当前字节 | 完整可读 | 不展示 | 禁用 | 源码与 Analysis 不一致 |

## 2. 运行状态（`RunDisplayState`）

| 值 | 触发条件 | 视觉 | 说明 |
|---|---|---|---|
| `NOT_RUN` | 未点击运行 | 中性 outline | “未运行”；不出现任何结果 |
| `STATIC_ONLY` | 只有 `check` 成功 | 中性 | “仅静态检查通过” |
| `RUNNING` | 用户已确认，Core 子进程运行中 | 金色脉冲（reduced-motion 时静态） | 显示入口/模式/预算，可取消（若实现） |
| `SUCCEEDED` | `run` 退出 0 + Trace v2 校验通过 | `--mq-ok` | 显示真实事件与 stdout；不显示返回值具体内容（除非公开 JSON 提供） |
| `FAILED` | `run` 退出非 0/运行错误码 | `--mq-error` | 显示 Core 错误码与 stderr 文本（转义） |
| `STALE` | Trace `source.analysis_digest` 与当前 Analysis 不符 | `--mq-warn` | “结果已过期”，不叠到当前正文 |
| `TRUNCATED` | `trace.truncated != null` | `--mq-warn` | “Trace 被截断，仅显示已记录事件” |
| `UNSUPPORTED` | 需要的字段/能力不在公开协议 | 中性 + 明确说明 | 绝不伪造：无变量值、无逐句赋值、无内存快照、无证明状态 |

## 3. 事件 → UI 映射（保守）

| Trace v2 事件 | 允许显示 | 禁止显示 |
|---|---|---|
| `call.enter` | 函数 id/名称、顺序、来源 span | 参数实际值 |
| `call.exit` | 退出类型/错误码 | 最终变量表 |
| `branch.eval` | 真/假 + 来源 span | “所有分支已覆盖” |
| `loop.iteration` | continue/exit 次数（以事件完整性为界） | 由静态结构推测总迭代数 |
| `return` | 发生返回 + 值类型名 | 具体返回值（除非公开 JSON 明确提供） |
| `effect` | operation、结果摘要、耗时 | “权限安全/已证明” |
| `trace.truncated` | 明确截断原因与上限 | 把缺失事件说成未执行 |
| 未知 kind | 忽略（记入 `ignored_event_kinds`） | 猜测语义 |

## 4. 能力边界（首版硬性）

| 旧 UI 能力 | 新 View 首版 | 理由 |
|---|---|---|
| 每个语句执行后的变量值 | **UNSUPPORTED** | Trace v2 不提供 |
| 参数实际值 / 内存快照 | **UNSUPPORTED** | Trace v2 不提供 |
| 知识“已证明” | **UNSUPPORTED** | 需要独立 Prove 协议与证书 |
| 调试断点/单步 | **不实现** | 首版非目标 |
| 函数大纲/签名 | 支持（来自 Analysis `functions[]`） | 公开静态事实 |
| 静态变量信息（params/赋值目标） | 支持但明确标注“静态” | 来自 Analysis，不冒充运行值 |
| 静态导入/引用标签 | 支持 | 来自 Analysis `imports[]`/`references[]` |
| 运行 Trace 事件侧注 | 支持 | Trace v2 + digest 校验 |
