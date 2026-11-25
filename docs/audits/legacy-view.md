# 旧 View 静态取证与迁移裁决（RFC-VIEW-0002 §3 / PR-00）


> **历史取证**：本文记录旧 View 的静态审计，仅作迁移历史；当前契约与状态见
> [`docs/STATUS.md`](../STATUS.md) 与
> [`observation-v1-and-analysis-v3.md`](../architecture/observation-v1-and-analysis-v3.md)。

- 状态：静态源码审计完成，**旧运行时浏览器复验未执行**
  （`LEGACY_RUNTIME_UNVERIFIED`）
- 日期：2026-11-22
- 旧产品来源：`cflmy/marqdo`，`main@1c55baabd570e18b9da8f9057cef2f848e176690`
  （本机只读 checkout `/root/work/marqdo`；新仓库**没有**任何 Git/path/Cargo 依赖）
- 新仓库：`marqdo-view`（独立 crate，见 `Cargo.toml` / `tests/contract/no_coupling.rs`）
- 结论适用范围：只作为**视觉与历史取证**。旧 `view` 的运行结果、旧 AST、旧
  `RunCapture` 都**不是** 3.0 语言语义真值；语言真值只来自冻结 CLI 的
  Analysis v2 / Trace v2（见 `docs/audits/protocol-contract-0002.md`）。

## 0. 证据分级与审计限定

| 级别 | 含义 | 本文用法 |
|---|---|---|
| LEGACY-OBSERVED | 在旧源码中直接读到的入口、样式注入、事件监听、执行调用 | 均可给出 `file:line` |
| LEGACY_RUNTIME_UNVERIFIED | 只有源码证据，没有在浏览器逐像素/交互复现 | 所有旧运行时结论均加此标记 |
| KEEP_ASSET | 只把已核验图形的**字节**迁入新仓库，附来源与许可证 | 见 §5 |
| REIMPLEMENT | 保留设计特征，按新架构重写 | 视觉 token、Dock、侧注 |
| DROP | 明确不迁移 | Notebook/Debug/Memory/旧 CSS 栈等 |
| UNKNOWN | 证据不足，不据此实现 | 截图右侧黑线 artifact 等 |

**限定**：§3.1 的旧结构与样式覆盖事实来自源码阅读；旧项目是否仍能编译、旧
README 截图对应哪一版运行 DOM，都**未**在本工作周期复现，不得当作软件事实引用。
参考截图 `docs/design/reference-old-view-reader.png` 已核验为 **2048×1133**
PNG（`file` 输出：`PNG image data, 2048 x 1133, 8-bit/color RGBA`），是视觉验收
基准，不是源码实现证据。

## 1. 旧命令树与页面入口

命令定义在旧 `src/main.rs`（LEGACY-OBSERVED）：

| 旧入口 | 位置 | 构建/执行路径 | 新 View 裁决 |
|---|---|---|---|
| `Commands::View { action: None }` → `serve(ViewOptions)` | `src/main.rs:1741-1750`、`src/view/mod.rs:74` | Live HTTP 页面，`/api/run`、Notebook/调试 API 同进程 | **REIMPLEMENT**：只保留阅读 + 显式单任务运行（RFC §5.2 路由表） |
| `ViewAction::Output` → `write_static(OutputOptions)` | `src/main.rs:1664-1678`、`src/view/output.rs:266` | 静态导出；`no_exec` 开关决定是否在导出时执行文档 | **REIMPLEMENT**：新导出从类型边界就不能执行（`src/export.rs`，RFC §5.4） |
| `ViewAction::Capture` → `write_capture` | `src/main.rs:1679-1687`、`src/view/output.rs:189` | 运行捕获快照（依赖旧 `RunCapture`/session 产物） | **DROP**：Trace v2 不提供逐句变量/参数快照 |
| `ViewAction::Document` → `workbench::build_document_view` | `src/main.rs:1688-1740` | Workbench HTML fragment 输出 | **DROP**：不是阅读器，不建第二套 DOM/工作台 |
| `Commands::Debug` → `serve_debug(DebugOptions)` | `src/main.rs:1751-1760`、`src/view/mod.rs:102` | 独立暗色调试服务器；断点、单步、`page_debug` | **DROP**：RFC §1.2/§3.3/§4.5 明令删除 |
| Live `serve` 内路径分派 | `src/view/mod.rs:74-101`、`240-390` | `page_file`、`page_debug`、`/api/run`、`/api/session/*`、`/api/debug/*` | **REIMPLEMENT**：新服务固定 7 条路由（RFC §6.2） |

**多页面生成路径事实**：Live 主路径 `page_file`（`src/view/html.rs:4111`）、调试
路径 `page_debug`（`src/view/debug_page.rs:50`）、Workbench fragment、静态导出
`write_static` 各自组装 HTML。这证明旧代码存在多条页面/模板路径；它**不**证明
浏览器里同时加载了两套 View（那需要运行时复验，`LEGACY_RUNTIME_UNVERIFIED`）。

## 2. CSS/JS 注入顺序与覆盖（必须整体废弃的叠层）

旧 `layout()`/`page_file` 在 `src/view/html.rs:1680` 起组装 `<body class="reader-page">`，
同文件 `2660-2700` 一处同时注入：

- `stylesheet()`（`src/view/html.rs:30`）：包含 Notebook 工作台皮肤和
  `body.notebook-page ...` 数百条规则（同文件 `699-704`、`2849` 之后的
  `notebook_ui()` 大段 CSS）；
- `notebook_ui()`（`src/view/html.rs:2849` 起）：`body.notebook-page` 作用域样式；
- KaTeX CSS/JS 通过 `include_str!("assets/vendor/katex*.{css,js}")` 内联；
- 最后追加 `reader.css`（`include_str!("reader.css")`）与 `reader.js`
  （`include_str!("reader.js")`，`src/view/html.rs:2689-2690`）。

`reader.css` 自身又以更高优先级反向覆盖 Notebook 皮肤，例如
`body.reader-page, body.reader-page.notebook-page, ...`（`src/view/reader.css:2`）、
`body.reader-page.notebook-page[data-theme="dark"]`（同文件 `18`）、
`body.reader-page #notebook-bar ...`（同文件 `102-105`）。也就是说旧页面的最终外观
依赖“两套皮肤 + 后置覆盖”的偶然顺序。

`reader.js` 在运行时再 `document.body.classList.add("notebook-page")`
（`src/view/reader.js:3561` 附近逻辑），把 Notebook 皮肤激活到阅读页上。

**裁决：DROP 整个旧 CSS/JS 叠层。** 新仓库只有 `assets/reader.css` 与
`assets/reader.js` 两个发布入口（见 `docs/decisions/0001`、`0002`），没有
`.notebook-page` 皮肤，不内联第三方 CSS/JS（KaTeX 不引入；公式按原文呈现）。

## 3. JS 初始化与执行调用连线

| 旧行为 | 证据 | 裁决 |
|---|---|---|
| Welcome Hero 由**文件名正则**注入：`/(^|\/)(00[-_])?(welcome|欢迎)/i.test(path)` 后 `hero.innerHTML = ...` 并 `prepend` | `src/view/reader.js:569-575` | **REIMPLEMENT**：只允许显式 `view.json` welcome 配置；禁止路径猜测 |
| 侧注曲线：创建 `svg.reader-connectors`、`g.reader-connector`，按 DOM 矩形计算路径 | `src/view/reader.js:221`、`341`、`366`、`555-559` | **REIMPLEMENT**（可降级）：新 View 首版用内联 keyed chip，不画无来源线；曲线是增强而非布局条件 |
| 旧注解焦点/搜索/对话框键盘事件与 `MutationObserver` 重排 | `src/view/reader.js:555-600` | REIMPLEMENT（仅保留阅读所需搜索/设置/焦点） |
| 页面内执行：`POST /api/run`，服务端 `api_run` 调 `run_file_capture(abs, opts)`，再把 stdout/bindings 注入 HTML | `src/view/mod.rs:245-246`、`805`；`src/view/mod.rs:430-520`；`src/view/output.rs:189` | **DROP**：执行必须由用户显式提交、经 SSE 观察、结果只按文本/DOM 展示，不得由页面加载/导出触发 |
| 运行后重读源码、重算 annots 并原地替换 surface | `src/view/mod.rs:500-520` | REIMPLEMENT：新 View 以 Analysis/Trace span 绑定，源码变化即 stale |
| 会话/Notebook/调试 REST：`/api/session/*` 20+ 路由、`/api/debug/*`、`/api/events` | `src/view/mod.rs:197-246`、`300-338` | **DROP**：新服务不允许 shell 执行端点、Notebook REST 兼容层、EventBus |

## 4. 新仓库必须拒绝“复制再隐藏”

RFC §3.3 的禁建目录（新仓库确实都不存在）：`src/notebook/`、`src/debug/`、
`src/memory_inspector/`、`src/session_kernel/`、`src/knowledge_compiler/`、
`src/proof_engine/`、`src/agent/`、`src/legacy_compat/`。
新仓库保留旧代码特征的地方只有**视觉 token 与三张已核验图片**；没有旧 AST、
没有旧 `RunCapture`、没有旧事件总线、没有旧页面模板。

## 5. 资产来源与许可证（已迁入 assets/）

| 文件 | 旧路径 | SHA-256（新仓库实测） | 许可证 |
|---|---|---|---|
| `assets/logo.svg` | `src/view/assets/logo.svg` | `52b2338468c62a7da86178ecec86cbb7ea44a996cd1bc4275ebb56f16f2244fe` | Apache-2.0（旧 marqdo） |
| `assets/blossom.webp` | `src/view/assets/blossom.webp` | `901fd6b37ebb20d1dfc3bf4d265a063308605350fd317e41914fa35b06f46e00` | Apache-2.0（旧 marqdo） |
| `assets/feather.webp` | `src/view/assets/feather.webp` | `fbd787174be1e8027ae4661b99f7f2cc01b824d8fc875a1d2d39cbd0c052e5de` | Apache-2.0（旧 marqdo） |

旧 `assets/README.md` 说明 blossom/feather 是为阅读器改版生成的原生透明素材，logo
为项目自绘几何 M/书签图形；旧仓库整体 Apache-2.0。详细许可说明见
`assets/licenses/ASSETS.md`；三个文件由 `src/assets.rs`（构建期嵌入）以 SHA-256
断言，Live 与 Export 提供完全相同的字节（ADR-0002）。

**UNKNOWN**：参考截图右侧的黑色短线/竖线不是品牌元素，不复刻；没有来源证据说明
它是 UI 组成。

## 6. 逐资源迁移裁决表

| 旧资源/行为 | 裁决 | 理由 |
|---|---|---|
| 暖纸色/金/橄榄/衬线正文视觉 token（旧 `reader.css` 色感） | REIMPLEMENT | 重新定义为 `assets/reader.css` 的唯一 token 集（RFC §4.3） |
| 左上樱花、品牌 logo、羽毛 | KEEP_ASSET | 已核验来源/许可/HASH；仅装饰 |
| 顶栏菜单 / 品牌 / 搜索、三项导航 `文档/函数/运行` | REIMPLEMENT | 新 IA，删除变量/内存 |
| 五按钮 Dock（刷新/设置/运行/清理/更多） | REIMPLEMENT | 每个按钮单一真实行为（RFC §4.2） |
| 金色函数竖标、连续阅读区、轻边框表格、底部浮动椭圆栏 | REIMPLEMENT | 设计特征，不复制 DOM/CSS |
| 侧注连接曲线 | REIMPLEMENT（首版降级为行内编号 chip） | 先准确关联，再追求曲线；失败可降级 |
| 旧 Welcome 文件名注入 | DROP / REIMPLEMENT 为显式配置 | RFC §9.3.3 禁止按名字前缀注入 |
| `stylesheet()` + `notebook_ui()` + `reader.css` 叠层 | DROP | 唯一主题入口；禁止旧 `.notebook-page` 覆盖 |
| 旧 `reader.js` Notebook/调试/连接线状态监听器 | DROP | 不复活 Notebook 状态机；新 JS 只做 DOM/SSE |
| KaTeX 内联渲染 | DROP（公式按原文显示） | 禁 CDN/第三方运行时；数学不属 View 语义 |
| `debug_page.rs`、`debug_api.rs`、断点/单步/调用栈 | DROP | RFC §1.2 REMOVED |
| `write_capture` / `RunCapture` / 变量面板 / 内存网格 | DROP | Trace v2 不提供逐句值；不得伪造 |
| `/api/session/*`、`/api/debug/*`、`/api/events`、Workbench fragment | DROP | 单一阅读 + 单任务执行；固定最小路由 |
| 旧 `serve` 的多页面入口与执行抓取交错 | DROP | 新架构 `Reader → assemble → Renderer` 单一流水线 |

## 7. 风险结论

1. 静态可证：旧 View 存在多页面生成路径、CSS/JS 叠层、页面加载/导出可触发执行的
   代码路径、以及大量 Notebook/调试 REST 入口。
2. 未证：旧页面在浏览器中的实际最终 DOM、样式优先级与截图对应关系；
   一律 `LEGACY_RUNTIME_UNVERIFIED`，不得引用为通过证据。
3. 新 View 的任何“视觉相似”都不构成语言/执行正确性证据；语言事实以冻结
   CLI `check --emit` + `run/call --json` + Trace v2 的黑盒结果为准。
4. 旧的代码/资产只能按 §5 的字节级来源迁入；不得对旧仓库建立任何构建依赖。
