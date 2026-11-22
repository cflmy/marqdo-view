# 视觉令牌与阅读布局（PR-00 设计基准）

> 状态：**设计基准，实现以 `assets/reader.css` 的 `tokens` 层为唯一真源。**
> 来源：RFC-VIEW-0001 §2、`docs/design/reference-old-view-reader.png`、`docs/audits/legacy-render-paths.md` §6–§7。
> 本文只描述“视觉语言”，不复制旧 CSS 原文；实现时任何数值以 CSS 为准，本文件与 CSS 冲突时以 CSS 为唯一实现真源并更新本文。

## 1. 色彩令牌

| Token | 值 | 用途 |
|---|---|---|
| `--mq-paper` | `#fcf9f5` | 页面底色（暖纸） |
| `--mq-surface` | `#fffcf8` | 浮起表面（Dock、代码围栏、弹层） |
| `--mq-surface-2` | `#f7f0e5` | 次级填充（标签、行内 code） |
| `--mq-ink` | `#29251f` | 正文墨色 |
| `--mq-strong` | `#1f1b16` | 标题/强调墨色 |
| `--mq-muted` | `#847565` | 次要文字、元信息 |
| `--mq-line` | `#e7ddce` | 轻分隔线 |
| `--mq-line-strong` | `#c9b79c` | 需要可见的边界 |
| `--mq-gold` | `#b88a45` | 黄金点缀：品牌、激活态、运行强调 |
| `--mq-gold-soft` | `#e8c78f` | 金色渐变高光 |
| `--mq-olive` | `#52603a` | 运行/知识侧注、次级语义色 |
| `--mq-ok` | `#52603a` | 运行成功 |
| `--mq-warn` | `#996b28` | 截断/待确认 |
| `--mq-error` | `#a63e30` | 运行失败/Core 错误 |
| `--mq-focus` | `#8a622e` | 键盘焦点环、激活 Tab 文字 |

对比度要求：正文 `--mq-ink` on `--mq-paper` ≥ 10:1；`--mq-muted` 仅用于辅助信息且 ≥ 4.5:1 于 `--mq-paper`。状态色不单独承担语义，必须同时有文字标签（色盲可读）。

## 2. 字体令牌

```css
--mq-serif: "Iowan Old Style", "Palatino Linotype", "Noto Serif CJK SC",
            "Source Han Serif SC", "Songti SC", SimSun, Georgia, serif;
--mq-mono: ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace;
--mq-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans CJK SC", sans-serif;
```

- 只使用**本地系统栈**，禁止 CDN / `@font-face` 远程加载。
- 正文 serif；UI 控件与状态标签 sans；代码/源码 mono。
- 基准正文字号 18–19px，行高 1.72；`prefers-reduced-motion` 时禁用位移/缩放动画。

## 3. 布局令牌

| 维度 | 值/规则 |
|---|---|
| 阅读主栏 | `min(100% - 2*var(--mq-gutter), 820px)`；720–900px 区间内 |
| 页面外边距 | `--mq-gutter: clamp(20px, 5vw, 64px)` |
| 顶栏高度 | ~72px，品牌居中，左右各一个 icon 按钮 |
| 分段导航 | 居中、gap `clamp(20px, 5vw, 64px)`、金色 2px 下划线激活 |
| Hero（仅显式 Welcome） | 金色斜体 `Marqdo`，`clamp(56px, 9vw, 96px)`；羽毛右侧装饰 |
| 函数标题 | 34px/1.35 衬线 + 左侧 5px 金色斜竖线 |
| 侧注 | ≥1440px 可左右侧注（宽 170px）；900–1439px 单侧或内联；≤899px 内联；≤600px 单栏 |
| Dock | `position: sticky` 底部居中胶囊；安全区 `env(safe-area-inset-bottom)`；不遮正文末段 |
| 内容节奏 | 段落间距 18px；标题上 32px/下 14px；不用重卡片堆砌 |

## 4. 组件外观

- **顶栏**：透明纸底；品牌 `Marqdo` 32px 衬线 + 9px `EXECUTABLE KNOWLEDGE` 字距 3px；左右 icon 按钮 40px 圆形 hover 填充 `--mq-surface-2`。
- **Dock**：胶囊（radius 999px）、`--mq-surface` 底、细 `--mq-line` 边、轻阴影；5 个按钮图标 + 小字标签；运行按钮金色实心。禁用按钮有 `aria-disabled` 与说明，不显示可点击空按钮。
- **函数/知识标题**：Exec 标题左金斜线；Knowledge 标题前书卷图标（`aria-hidden`）；标题旁 `EXEC`/`KNOWLEDGE` 小号大写标签；原文前缀在 Source 态完整保留。
- **导入/引用标签**：椭圆 chip，`◇`/`⟶` 前缀，mono 12px。
- **GFM 表**：横向可滚动容器；表头 `--mq-surface-2`；1px `--mq-line` 分隔。
- **代码围栏**：`--mq-surface` 底、1px 边、圆角 10px；横向滚动；无行号（源码态除外）。
- **公式**：首版以等宽/衬线文本呈现原文（`$...$` 与 `$$...$$`），标注“未渲染数学”；不引入 vendor。
- **侧注**：正文右侧 170px；金色/橄榄 1px 贝塞尔曲线，起点为真实语义锚点；无来源不画线；拥挤时退化为内联 keyed 注释。
- **装饰**：樱花 `blossom.webp` 左上、羽毛 `feather.webp` 仅 Welcome Hero；`aria-hidden="true"`；打印隐藏。

## 5. 状态视觉

见 `docs/design/state-matrix.md`。所有状态必须同时具备：文字标签 + 颜色 + `data-run-state` 属性；不得只靠颜色。

## 6. 响应式与无障碍

| 宽度 | 行为 |
|---|---|
| ≥1440px | 左右侧注区域可用，Dock 居中 |
| 900–1439px | 侧注收缩为单侧或内联 |
| ≤899px | 侧注全部内联于锚点下方 |
| ≤600px | 单栏；隐藏次级导航文字只留图标；Dock 不覆盖末尾（正文底部预留 Dock 高度） |
| 打印 | 隐藏顶栏/Dock/装饰/曲线，保留正文与内联注释；强制黑白友好对比 |

- 键盘：Tab 顺序 = 顶栏 → 导航 → 正文链接/折叠 → Dock；Esc 关闭 dialog；焦点环 2px `--mq-gold` + 2px offset。
- 无 JS：文档正文、标题、表格、链接、内联注释、源码态全部可读；执行控件退化为说明文字。
- 屏幕阅读器：装饰 `aria-hidden`；侧注与锚点用 `aria-describedby`；状态 `role="status"` 播报。
- `prefers-reduced-motion: reduce` 关闭侧注动画与 smooth scroll。
