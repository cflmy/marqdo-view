# ADR-0003：显示层标题前缀剥离与文档内锚点身份

- 状态：Accepted
- 日期：2026-11-22
- 关联：RFC-VIEW-0001 §5.2、§8.2、§8.3；PR-02

## 背景

`@exec` / `@knowledge` 是标题语义声明，但 Markdown 标题的**显示文本**不应把声明前缀当作正文主标题重复展示。与此同时，语义标注必须能稳定指向源码位置，而且不能在标题重名时产生重复 DOM id。Analysis v2 公开字段中：

- `units[]` 没有单独的“本标题的 mode”字段（RFC §4.3 已警告不得把 `kind=section` 当成 Exec/Knowledge）；
- `functions[].heading_span` 与 `units[].span` 对 ATX 标题精确对应，可作为锚点来源；
- 继承模式无法从公开 JSON 直接读出。

## 决策

1. **标题前缀只在显示层剥离**：仅当标题文本以精确的 `@exec ` 或 `@knowledge ` 开头，且剥离后的空白折叠结果等于 Analysis `units[].name` 时，才视为合法声明并剥离；同时输出 `mq-mode-badge`。`@Exec`、拼写变体、`**@exec**` 等一律原样显示（Core 会给出诊断）。Source 态永远显示原始字节，因此剥离是可逆的显示规则，不是改写源文件。
2. **不猜测继承模式**：没有显式声明前缀的 H3–H6 不渲染 EXEC/KNOWLEDGE 徽标，`data-semantic="document"`；H2 function 由 `functions[]` 精确映射为 `exec`，H2 knowledge 由 `units[].kind=knowledge` 映射为 `knowledge`。
3. **锚点 id 来自源码 span**：`id="mq-h-<start-hex>-<end-hex>"`，由标题自身的 UTF-8 半开区间派生；函数大纲使用同一条 `heading_span`，保证“函数列表跳转”与正文标题命中同一个 id。禁止用标题文本、序号或随机数生成 id（重名会跳错）。
4. **语义装饰锚点 id** 使用 RFC §8.3 的 `sha256(module_id + "\0" + start + ":" + end + ":" + kind)[0..16]`；它只在本页 DOM 内有效，不跨编辑/跨运行复用。
5. 任何无法通过 `SourceIndex` 验证的 span（越界、非 UTF-8 边界）直接丢弃该条装饰并产生 `view.source_span_invalid` 警告，绝不把它吸附到相邻文字。

## 后果

- 同页多个 H2/H3 永远得到不同 id；`tests/rendering.rs::semantic_annotations_come_from_analysis_and_are_unique` 断言唯一性。
- Setext 标题没有合法前缀声明，会被原样显示并沿用自身 span 锚点；如果未来需要更精确的继承模式标注，必须通过新的公开协议字段实现，不能在 JS/CSS 里猜。
- 标题显示与源码态自动保持一致；无需额外“复制原文”逻辑。
