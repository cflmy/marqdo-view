# 旧 View 渲染路径审计（PR-00）


> **历史取证**：本文记录旧渲染路径的静态审计，仅作迁移历史；当前实现与契约见
> [`docs/STATUS.md`](../STATUS.md)。

> 状态：**静态审计完成，运行时验证未执行（`LEGACY_RUNTIME_UNVERIFIED`）**
> 审计对象：`cflmy/marqdo` `main@1c55baabd570e18b9da8f9057cef2f848e176690`（下称“旧仓库”）
> 审计方式：只读源码检查（`git status` 干净；**没有**编译、没有写入、没有移动旧仓库任何文件）
> 审计日期：2026-11-22
> 结论用途：决定新 `marqdo-view` 可复用/必须重写/必须丢弃的视觉与结构素材。旧语义、旧执行模型、旧私有 API **一律不复用**。

## 0. 审计边界与可信度声明

1. 旧仓库为 Apache-2.0（`/root/work/marqdo/LICENSE`），其视觉资产可在保留许可声明的前提下筛选复用（§6）。
2. 旧仓库**没有已编译产物**（无 `target/`），且本任务明令禁止修改旧仓库。因此本次**未启动旧 View 服务、未抓取 DOM/Computed Style/Network/HAR**。所有结论均来自可复核的静态源码行号，属于 RFC §3.1 允许的“结构证据”。
3. 随 RFC 保存的 `docs/design/reference-old-view-reader.png`（1664×920）是**用户提供的视觉基准**，不是本审计运行得到的截图。旧仓库 HEAD 实际渲染结果是否与截图一致**未验证**。
4. 标记规则沿用 RFC：`OBSERVED`＝源码直接可见；`INFERRED`＝由源码组合可推断但未运行验证；`UNVERIFIED`＝无法静态确认。
5. 旧版语法/旧 `ViewExecutionReport`/旧知识制品都**不是** 3.0 契约；本审计只提取“视觉语言”和“曾经犯过的结构错误”，不提取任何运行时语义。

## 1. 命令 → 函数 → 模板 → CSS/JS → HTML 路径

| 用户命令 | 入口 | 渲染函数 | 模板 | 注入 CSS/JS | 产物 |
|---|---|---|---|---|---|
| `marqdo view [PATH]`（无子命令） | `src/main.rs:1657` `Commands::View` → `None` 分支 `src/main.rs:1737` | `view::serve`（`src/view/mod.rs:74`）→ `handle` → `page_file`（`src/view/html.rs:4111`） | `html::layout`（`src/view/html.rs:1667`） | `layout` 内 `<style>{stylesheet}</style>`、`<style>{reader_css}</style>`、`<script>{reader_js}</script>`；`page_file` 的 `main` 中再插入 `notebook_ui()` `<style>`（`src/view/html.rs:2849`、`4261`） | 每请求实时 HTML |
| `marqdo view output PATH -o OUT [--no-exec]` | `src/main.rs:1664` `ViewAction::Output` → `write_static`（`src/view/output.rs:266`） | `write_static` 循环调用 `page_file` | 同上 | 同上 | 静态 HTML 文件树（`index.html`、`pages/*.html`） |
| `marqdo view document` | `src/main.rs:1688` `ViewAction::Document` | `workbench::build_document_view`（私有 workbench 模型） | 不复用 `layout`；直接拼一个内联样式的最小 HTML（`src/main.rs:1712` 左右） | 只有内联 `<style>` | JSON 到 stdout 或可选独立 HTML |
| `marqdo debug` | `src/main.rs:1751` 附近 `Commands::Debug` → `serve_debug`（`src/view/mod.rs:102`） | `handle_debug` → `build_debug_model` → `page_debug`（`src/view/debug_page.rs`） | `page_debug` 自带独立模板/深色 UI | 独立 CSS/JS 与调试 API | 独立调试页面 |

**OBSERVED 结论：**

- 至少存在 **三类文档呈现入口**（`serve` 的 Reader、`ViewAction::Document` 的 Workbench 文档、`debug` 的调试页）与 **两套页面模板体系**（`layout`+`page_file` vs `page_debug`）。它们不是同一套组件，字体、颜色、状态语义都不同。
- `page_file` 与 `page_index` 共享 `layout`，因此 Live（`serve`）与静态导出（`write_static`）确实共用同一套 Reader 模板与资产注入 —— 这一点值得保留为新架构的硬约束（RFC §6.1）。
- `ViewAction::Document` 使用私有 `marqdo::workbench` 模型并现拼 HTML，**不能**作为新 View 的语义来源。

## 2. CSS 层叠报告（OBSERVED）

最终读者页面（`serve` / `write_static` 产物）的 `<head>`/`<body>` 中样式顺序固定为：

| 顺序 | 来源 | 规模（源码行） | 作用 | 是否实际生效 |
|---|---|---|---|---|
| 1 | `html::stylesheet()`（`src/view/html.rs:30`–`1662`） | ~1633 行 | 基础站点壳：Apple-HIG 风格浅灰 token、`.shell/.nav/.panel/.structure` 等全部组件结构 | **生效** |
| 2 | `html::notebook_ui()`（`src/view/html.rs:2849`–`4017`，样式块约 1168 行） | ~1168 行 | VS Code / Jupyter 深色 Notebook skin（`body.notebook-page ...`）及 `body.notebook-page[data-theme="light"]` 浅色覆盖，含 notebook 控件样式 | **未生效（休眠）**：全仓库没有把 `notebook-page` class 加到 `<body>`，也没有设置 `data-theme`。`layout` 写死 `<body class="reader-page">`（`src/view/html.rs:1680`），`reader.js` 只切换 `reader-tools/reader-large/reader-hide-notes/reader-welcome`（`src/view/reader.js:142,143,152,570`）。 |
| 3 | `src/view/reader.css`（268 行） | 268 行 | 暖纸 Reader 主题：`--paper/--gold/--olive` 令牌、`body.reader-page...` 高特异性覆盖、Hero、樱花、Dock、侧注、SVG 曲线样式 | **生效**，且大量选择器写成 `body.reader-page.notebook-page...`（第 2 行、18 行等），依赖一个从不出现的 class，属于**针对休眠状态的防御性覆盖** |
| 4 | `layout` 内联追加 `<style>{reader_css}` 后再追加 hero 羽毛/樱花 `url(...)` 变量（`src/view/html.rs:1700` 左右） | 少量 | 把 `feather.uri`/`blossom.uri` 数据 URI 注入 Hero 与装饰 | 生效 |
| 5 | `reader.js`（594 行，`src/view/html.rs:2730` 左右 `<script>`） | 594 行 | 导航、对话框、搜索、Run 按钮、侧注几何与 SVG 曲线、Welcome Hero | 生效 |

**OBSERVED 结论：**

- 截图中的暖纸外观来自**最后一个**样式层 `reader.css`，其选择器专门压制 `stylesheet()` 的基础浅灰壳。这是典型的“后加载皮肤 + `!important`/高特异性覆盖”结构，**正是新架构 §3.3 / §7.1 明令禁止的形态**。
- `notebook_ui()` 是约 1168 行的深色 Notebook 样式，被塞进每个页面的 `<main>` 内，但 body class 从未切换 ⇒ **每页都携带不生效的深色主题字节**。不能把这种“死层”搬进新仓库。
- 顶层 HTML 结构里没有任何 `<link>` 外链 CSS/字体；KaTeX 的 CSS/JS 通过 `include_str!` 内嵌（`src/view/html.rs:2717` 左右），符合“离线可读”原则，可以借鉴其**内嵌资产**策略，但不能复用其 HTML 拼接方式。

## 3. Reader 模板与交互清单（OBSERVED）

`layout()`（`src/view/html.rs:1667`–`1730`）固定输出：

```text
<input #nav-toggle> + <label .backdrop>
<header .reader-header>
  button.reader-menu（三横线，data-reader-action="files"）
  a.reader-brand（logo.uri + "Marqdo" + "EXECUTABLE KNOWLEDGE"）
  button.reader-search（data-reader-action="search"）
  nav.reader-tabs：文档 / 函数 / 变量 / 内存 / 运行
<div .reader-ornament.reader-blossom-top>  （+ bottom）
<div .shell>：<aside .nav> + <main .panel>
<footer .reader-footer>（MARQDO / 代码即文档 · 文档即知识库 · 知识即可执行）
<div .reader-dock data-reader-action="reload|settings|run|clear|more">
<div .reader-toast>
<dialog #reader-dialog>
<style>{reader_css + 装饰 url}</style>
<script>{reader_js}</script>
<dialog .var-dialog>（旧变量快照 UI）
<script>{katex_js}</script>
```

对应旧行为（`reader.js:88` 的 `action(name)`）：

- `run` → `POST /api/run`（`src/view/mod.rs:805`），执行并流式返回 `bindings_html`/`structure_html`/`trace_json`；
- `variables`/`memory` → 展示旧 `ViewExecutionReport` 的变量/内存快照（Trace v2 **不提供**这些数据，新 View 不得复刻）；
- `functions` → 把旧结构面板移入 dialog；
- `settings` → 浅/深色、正文字号、标注显隐（深色依赖 `#nb-theme`，即休眠的 notebook 皮肤）；
- `clear` → 隐藏旧输出；
- `search` → 在当前页面文本内搜索，并混入文件导航（`reader.js:69`–`82`）。

**主阅读区（`page_file`，`src/view/html.rs:4111`–`4330`）：** `h1.page` + `.status-pill`、`.meta` 路径、`notebook_ui` 样式、文档 `structure_block`、可选知识块、执行块（stdin 表单/流式面板/结果 `<pre>`/plot）、`<details>` 完整源码。**它不是“连续 Markdown 长文”，而是“文档 + 运行结果 + 完整源码”的多段拼接页**，与 RFC §2.2 的目标形态不同。

## 4. Welcome Hero 的生成方式（OBSERVED，必须重写）

`src/view/reader.js:568`–`575`：

```js
var path = window.MARQDO_FILE_PATH || '';
if (/(^|\/)(00[-_])?(welcome|欢迎)/i.test(path)) {
  document.body.classList.add('reader-welcome');
  var hero = document.createElement('section'); hero.className = 'reader-hero'; ...
  hero.innerHTML = '<p class="eyebrow">WELCOME TO</p><h1>Marqdo</h1><p class="reader-motto">代码即文档 · 文档即知识库 · 知识即可执行</p>';
  document.querySelector('main.panel').prepend(hero);
}
```

**结论：旧版按文件名正则（`00-welcome` / `welcome` / `欢迎`）在客户端插入 Hero，且用 `innerHTML` 拼字符串。** 新 View 要求由**显式 `view.json` 配置**选择 Welcome 页面，由服务端 `ReaderRenderer` 输出，JS 不得按文件名插入 UI（RFC §9.4、§7.1）。

## 5. 静态导出与执行副作用（OBSERVED，新实现必须纠正）

`write_static`（`src/view/output.rs:266`–`338`）：

1. 默认（未传 `--no-exec`）为每个文件调用 `build_file_view_with_exec(..., !opts.no_exec)`；
2. 未提供 `--snapshot` 时调用 `static_page_data(&root, &abs, &vm.preset_stdin)`（`src/view/output.rs:67`），即**导出过程中执行文档**以获得输出快照；
3. 再把输出注入 HTML（`inject_cell_outputs` / `inject_static_session`，`src/view/output.rs:147,172`）。

**结论：旧 `view output` 的默认路径存在“导出时执行”的语义，只有显式 `--no-exec`/`--snapshot` 才能避免。** 新 View 的 `export` **无条件纯只读**（RFC §11.3），并且该差异必须写成对抗测试（`tests/security`），而不是仅靠文档说明。

另外旧 `/api/run`（`src/view/mod.rs:805`–`880`）只做 body JSON 解析与 `resolve_rel`，**没有** CSRF token、`Origin`/`Host` 校验或一次性会话确认；`/api/foreign-run` 同样存在。新 View 不得继承此模型（RFC §11.2）。

## 6. 旧视觉资产清单与复用决策（OBSERVED + 许可核对）

| 资产 | 大小 | SHA-256 | 来源/许可 | 决策 |
|---|---|---|---|---|
| `src/view/assets/logo.svg` | 4 KiB | `52b2338468c62a7da86178ecec86cbb7ea44a996cd1bc4275ebb56f16f2244fe` | 旧仓库自绘 SVG，Apache-2.0 | **KEEP_VISUAL，复制为 `assets/logo.svg`**（保留新旧 IR 差异说明与许可） |
| `src/view/assets/blossom.webp` | 84 KiB | `901fd6b37ebb20d1dfc3bf4d265a063308605350fd317e41914fa35b06f46e00` | 旧仓库 `assets/README.md`：imagegen 生成的原始透明插画；Apache-2.0 | **KEEP_VISUAL，复制为 `assets/blossom.webp`** |
| `src/view/assets/feather.webp` | 12 KiB | `fbd787174be1e8027ae4661b99f7f2cc01b824d8fc875a1d2d39cbd0c052e5de` | 同上 | **KEEP_VISUAL，复制为 `assets/feather.webp`** |
| `src/view/assets/blossom.png` / `feather.png` | 950 KiB / 1.7 MiB | `2189f61b...` / `1b933940...` | 旧仓库设计原图 | **DROP**（新仓库不携带未进入构建的大图） |
| `assets/vendor/katex.min.*` | ~1 MiB+ | 见 `vendor/KATEX-LICENSE` | KaTeX 0.16.22，MIT | **不复制**：首版公式以保守可读文本 + 可选本地渲染，不引入 1 MiB vendor 与自动渲染脚本 |
| `reader.css` | 268 行 | — | 旧仓库自写，Apache-2.0 | **REWRITE**（只提炼令牌与视觉关系，不复制选择器/覆盖链） |
| `reader.js` | 594 行 | — | 同上 | **REWRITE**（只保留交互职责边界；Welcome 正则、旧 trace/变量面板、`innerHTML` 一律丢弃） |
| `html.rs` 的 `stylesheet()` / `notebook_ui()` | ~2800 行 | — | 同上 | **DROP**（新仓库从零写单一 `reader.css`） |
| `debug_page.rs` + 调试 API | 973 行 + API | — | 同上 | **DROP**（首版无 debug，见 RFC §1.3/§3.1） |
| 旧 `knowledge_*` 制品 | 477 行 | — | 旧 `sema::SemanticProject` | **DROP**（不是 Analysis v2 / Proof 3.0） |

资产权利说明：旧仓库根 `LICENSE` 为 Apache-2.0；KaTeX 为 MIT（未复制）。复制进新仓库的三个文件在 `assets/licenses/` 记录来源 commit、SHA-256 与 Apache-2.0 归属。

## 7. 截图元素清单与实现归属

由 `docs/design/reference-old-view-reader.png`（1664×920）与 RFC §2.1 拆解，逐元素指定新实现归属：

| 截图元素 | 归属 | 新实现 |
|---|---|---|
| 暖纸/米色画布、低对比边界 | KEEP_VISUAL | `assets/reader.css` `tokens` 层唯一令牌 |
| 左上樱花枝 | KEEP_VISUAL | `blossom.webp`，`aria-hidden="true"`，仅装饰 |
| 顶栏三横线 / 品牌 / `EXECUTABLE KNOWLEDGE` / 搜索 | KEEP_VISUAL | 服务端模板 + `[data-action]` |
| 顶部分段导航 `文档/函数/变量/内存/运行` | REWRITE | 首版 `文档/函数/运行`；`变量` 只显示静态参数/赋值目标；`内存` 明确 UNSUPPORTED 并从主 Tab 移除 |
| Welcome：`WELCOME TO` + 金色斜体 `Marqdo` + 羽毛 + 口号 | KEEP_VISUAL | 仅 `view.json` 显式 welcome 页面，服务端渲染 |
| 文件名 + 叙述折叠块 + 导入/引用椭圆标签 | REWRITE | 文件身份条 + Analysis imports/references 标签（无旧语法） |
| `main` 大标题 + 左侧细金斜竖线 + 说明小字 | KEEP_VISUAL | `[data-semantic="exec"]` 标题样式，来源 Analysis `functions[]` |
| GFM 表、英文正文 | KEEP_VISUAL | Comrak 渲染 + 净化 |
| 底部胶囊 Dock（刷新/设置/金色运行/清理/更多） | KEEP_VISUAL | 每个按钮必须有真实动作；Export 中运行按钮变说明文字 |
| 右侧黑色短线/竖线 | DROP（原因不明，UNKNOWN） | 不复刻；新页面不产生无来源装饰线 |

## 8. 旧版结构失误 → 新架构硬约束

| 旧版事实 | 新架构约束 |
|---|---|
| `stylesheet()` + `notebook_ui()` + `reader.css` 三层叠加、`body.reader-page.notebook-page` 高特异性覆盖 | 单一 `assets/reader.css`，`@layer reset,tokens,layout,typography,semantics,overlays,interactions,responsive`，禁止遗留皮肤文件与 `!important` 覆盖链 |
| `serve`/`write_static` 共用模板（好）但 `ViewAction::Document` 另有一套 HTML | 只允许一个 `ReaderRenderer::render()`；Live/Export 仅 `LinkPolicy` 与控件可用性不同 |
| Welcome Hero 由 JS 文件名正则 + `innerHTML` 插入 | 显式 `view.json` + 服务端渲染，无文件名正则 |
| 静态导出默认执行文档以抓快照 | `export` 永不执行；`--no-annotations` 只影响标注 |
| `/api/run` 无 CSRF/Origin/Host 校验 | 一次性 CSRF token + 严格同源/Host 校验 + 显式确认 |
| 旧 `ViewExecutionReport` 提供逐句变量值/内存快照 | Trace v2 不提供；新 UI 状态矩阵显式 `UNSUPPORTED`，绝不伪造 |
| `notebook_ui()` 深色 skin 进入每页字节却不生效 | 资产按实际进入构建；CSS/JS 单一入口与 SHA-256 记录 |

## 9. 运行时验证状态与复现步骤

- 状态：**`LEGACY_RUNTIME_UNVERIFIED`**。
- 原因：旧仓库无预编译产物；本任务禁止修改/编译旧仓库；在本机从头构建旧 2.9.1 工程不符合“新项目只依赖发布版 CLI”的验收前提，且会污染只读审计对象。
- 复现（在**旧仓库副本**中、不覆盖原始工作树时）应执行：

```bash
cp -a /path/to/marqdo /tmp/legacy-marqdo-audit && cd /tmp/legacy-marqdo-audit
cargo build --release --features cli,view
./target/release/marqdo view --no-open examples/  # 记录 URL
# 用 Chromium DevTools 保存 DOM、Computed Style、style rule 命中源、Network、截图
rg -n 'reader-hero|reader-dock|reader-connectors|annot-band|notebook-page' src/view
```

- 若将来完成上述步骤，应把 DOM/Computed Style/HAR 证据补进 `docs/evidence/legacy-<commit>/`，并把本文 §2 的“休眠”结论更新为实际观测。**在补齐前，不得声称旧 HEAD 页面与随包截图一致。**

## 10. 审计结论摘要

1. 旧 Reader 的**视觉语言**（暖纸、衬线、金色点缀、樱花、羽毛、Dock、侧注曲线）值得延续；实现细节必须重写为单一主题。
2. 旧 Reader 的**结构**是“基础壳 + 深色 Notebook 死层 + 后加载 Reader 皮肤”，并混有 Workbench/Debug 第二套模板 —— 全部禁止移植。
3. 旧版**运行/导出安全模型**不满足 3.0 要求（导出可执行、Run API 无 CSRF），必须按 RFC §11 重新设计。
4. 旧版**执行数据模型**（逐句值、内存快照、旧知识制品）与 Trace v2 不兼容，必须显式 `UNSUPPORTED`。
5. 三个视觉资产（logo/blossom/feather）许可与哈希已核对，可复制进新仓库的 `assets/` 并由 `AssetBundle` 统一内嵌。
