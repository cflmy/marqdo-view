# ADR-0010：静态/观测标注策略、分组与 stale 降级

- 状态：Accepted
- 日期：2026-11-22
- 关联：`docs/architecture/RFC-VIEW-0002.md` §4.4/§7.3/§7.4；PR-05

## 1. 两类标注，来源显式

- **Static（analysis）**：`render` 依据 Analysis v2 的 statement/function/unit
  span 输出 `data-annotation-source="analysis"` 的锚点。未运行时只展示静态结构，
  不出现“已发生”措辞。
- **Observed（trace）**：运行终态事件里每个带 `source_id` 的事件由前端以
  `.mq-run-chip[data-annotation-source="trace"]` 就地附加，携带
  `data-run-seq`/`data-run-kind`；只有经 `analysis_digest`/`root_id`/`module_id`/
  span 校验后的 Trace v2 才会产生 `source_id`。

## 2. 曲线 vs 内联分组 chip

RFC §4.4 允许“先准确关联，再追求曲线审美”，并允许在空间不足/打印/禁 JS/来源
匹配失败时退化为正文内编号注释。首版选择**内联 keyed chip**作为唯一实现：

- 不绘制 SVG 曲线，也不把截图右侧未知黑线当品牌元素；
- 同一锚点的事件按 `source_id` 分组，最多显示 1 个 chip；>3 条时文本加 `×N`，
  `title` 给出真实条数与事件种类——避免事件多时压过正文（近似 §4.4 的 ≤3 密度
  建议）；
- chip 只放在 `data-source-id` 精确匹配的语句锚点上；不做“找最近文本”的猜测；
- 无 JS/静态导出没有 Observed chip；静态信息仍完整可读；
- 打印模式隐藏交互控件但不丢正文/静态标注。

## 3. Stale 与降级（硬规则）

- Trace 的 `source.analysis_digest` 必须等于**运行前**由 `check --emit` 得到并进入
  `POST /api/runs` 的 Analysis 原始字节 SHA-256；`root_id`/`module_id`/span 再逐项
  校验；任何不符 → `trace_status=invalid`，**不产生 source_id**、不画 chip，但
  运行 `result` 仍可见（不把 Trace 无效误判为程序失败，见 ADR-0008）。
- 运行结束后重新读取源文件：字节变化 → `stale=true`。服务端 Trace 摘要仍保留
  `status=verified` 供参考，但所有 `anchor`/`source_id` 置空；前端显示“结果过期，
  不锚定当前正文”，绝不把上一轮事件压到新源码上。
- `truncated`（`event_budget`/`byte_budget`）显式提示“Trace 被截断，不代表程序
  终止”，不声称观察到全部路径。
- 无 Trace（`unavailable`）时只显示 CLI 返回/输出，不伪造空 Trace 或“完整执行”。

## 4. 测试与证据

- `tests/integration/execution.rs`：stale 源编辑 → `stale=true` 且所有 anchor/null；
  Trace digest 不符 → `invalid` 且无事件；截断 Golden → `truncated` 保留；
- `tests/contract/trace_contract.rs`：Analysis 字节摘要、module/span/seq 校验；
- 浏览器套件：`observed chips carry trace provenance`、`static statement anchors
  carry analysis provenance`、`at most one grouped chip per anchor`，并采集
  `control-flow-run.png`、`control-flow-run-dark.png`、2048×1133/390×844 截图
  （`docs/evidence/PR-05/`）。
