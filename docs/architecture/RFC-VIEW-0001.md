# RFC-VIEW-0001：Marqdo View 3.0 独立阅读器架构与开发验收规范


> **历史设计（0.1.0 时代）**：本文中的协议名称与命令面只记录当时设计，不是 0.3.0
> 的当前契约。当前实现只使用 Analysis revision 3（ADR-0069）与 Observation
> revision 1；见 [`observation-v1-and-analysis-v3.md`](observation-v1-and-analysis-v3.md)
> 与 [ADR-0012](../decisions/0012-rc4-current-contracts.md)。

> 状态：**Proposed / 可实施设计规范，不代表功能已经实现**  
> 目标仓库：`marqdo-view`（独立 Git 仓库；首次开发时新建）  
> 文档版本：`1.0-draft`  
> 审查日期：2026-11-22  
> 新内核来源：`https://github.com/cflmy/marqdo-cli`，审查 `master@eeea01d672c07b6be7d6444031d1619309dc88ab`；首个 Beta 发布源码标签 `v3.0.0-beta.1`，标记源码 `2c7c4f31e7a388d3fcfa717a77067e696ed13854`  
> 旧 View 来源：`https://github.com/cflmy/marqdo`，审查 `main@1c55baabd570e18b9da8f9057cef2f848e176690`  
> 视觉基准：本项目随文档保存的 `docs/design/reference-old-view-reader.png`（1664×920 用户提供的旧界面截图）  
> 适用产品：Marqdo CLI `>=3.0.0-beta.1,<4.0.0`，公开 Analysis v2、Trace v2、CLI Tool Protocol v1；若发布新的破坏性协议，必须先验证本 RFC。

## 0. 文档使用方法及优先级

本文是**独立仓库开发所需的完整任务定义**。开发人员不需要知道历史聊天或其他未列出的设计资料。开发时只需要本文件、随包截图和以下两个仓库中的**已提交、已公开**的源文件及规范。除正常构建/测试依赖外，不得假设存在第三个私有仓库、旧版运行服务、私有 API 或手工修改后的 CLI。

1. **可执行语义和协议真相**：`marqdo-cli` 的**发布版**及 `spec/`；在设计文档与实际 ABI 冲突时，冻结发布二进制的黑盒结果、对应 tag 的 `spec/` 优先；将不一致记录为风险，不能自行重新定义语言。
2. **视觉基准**：本文 §2 逐元素说明、`docs/design/reference-old-view-reader.png`；旧仓库的 Reader 资产作为可选择性复用的实现素材，而非运行时依赖。
3. **旧版功能参照**：`marqdo` 的 `src/view/`、`doc/design/view*.md`。**旧版语法和旧 View 私有 API 均不是 3.0 契约。**
4. **本文建议的接口与目录**：新 `marqdo-view` 内部合同。若新仓库实现需要变更这些结构，必须更新本文、测试和 ADR，而不允许悄悄在 UI 中创建第二套逻辑。
5. 任何尚未在新 CLI 公开的能力（逐语句赋值值、调试帧、知识证明等）必须显式标记 **unsupported/deferred**，不得用猜测数据模拟。

本文使用三种状态：`OBSERVED`＝已在旧/新仓库源码中直接发现；`REQUIRED`＝本 RFC 的新开发约束；`PROPOSED`＝可按实际依赖调整的方案；不能把 `PROPOSED` 写成现有功能。正式代码实现状态须以 `STATUS.md` 和实际 CI 证据为准。

---

## 1. 产品目标、非目标及最终成功定义

### 1.1 一句话定位

**Marqdo View 是一个以原始 Markdown 为阅读主体的可执行文档阅读器：将 CLI 的静态语义与可选的真实运行轨迹附着到原文，而不是把文档翻译成另一套语言或页面。**

正确的体验包括三个相互定位的视图：

- **文档态 Document（默认）**：高质量 GFM/CommonMark 阅读；`@knowledge` 的知识内容按普通文档叙述，`@exec` 的函数在原位置出现轻量语义提示；不自动运行。
- **源码态 Source**：严格保留原文字节对应的文本（包括 Markdown 标记、原始标题与换行），用于与文档态逐位置对应。
- **执行态 Execution**：在文档上附加有来源的 Analysis/Trace 信息，清楚标注未运行、运行成功、运行错误、记录过期或截断。执行由用户明确触发，绝不能在打开或导出文档时隐式触发。

### 1.2 强制目标（MUST）

1. **源码仓库彻底解耦**：新的 `marqdo-view` 不得依赖旧 `marqdo` 的 crate、本地 path/git Rust 包、`src/view` 内部模块，也不得依赖 `marqdo-cli` 的私有 crate 或源目录。源码只作为审计/参考。`marqdo-view` 在只有发布版 `marqdo` 二进制的环境下必须能构建、安装并运行。
2. **运行时完全解耦**：使用 CLI Tool Protocol v1 注册为 `marqdo view`，只通过 `MARQDO_BIN`、公开 CLI 命令、Analysis v2/Trace v2 JSON 及用户授权的原始文件交互。**不更改、不重编译、不动态链接 CLI 内核。**
3. **单一语义权威**：只有 `marqdo-cli` 决定什么可执行、函数边界、语句/分支/循环含义。View 只渲染、引用和校验公开编译结果。
4. **单一页面系统**：Live 与 Export 共用 `DocumentModel → PresentationModel → ReaderRenderer`；不允许分别实现两套 HTML/Markdown/语义映射引擎。Debug 不属于首版。
5. **截图外观延续**：暖纸背景、中文衬线正文、居中品牌、樱花装饰、低密度导航、黄金点缀、大留白、圆润浮动操作栏及克制侧注。保留**视觉语言而不是完整旧交互行为**。
6. **能力可扩展而内核稳定**：未来 `marqdo-knowledge`、`marqdo-prove`、`marqdo-lint` 通过版本化可选信息层进入 View；不存在任意一个扩展时 View 仍必须完整阅读普通 `.mq.md`。
7. **安全默认值**：本地静态阅读、禁止自动执行、外部 HTML/URL/本地文件边界明确、自动化对抗性测试。
8. **便于维护**：优先单一 Rust package + 单一前端资产入口。没有充分性能/协议需求，不建立微服务、第二个编译器、第二个数据层、通用插件宿主或多 crate workspace。

### 1.3 非目标（MUST NOT）

- 不实现 `@exec/@knowledge` 的新语法，不改变 `@exec` 的函数规则；不新增核心 `View` 子命令。
- 不支持旧 Marqdo 2.x 的执行语法兼容层；旧样例须重写为 3.0 且在 `marqdo check` 中通过。
- 不内置 Proof/Knowledge/Lint 引擎，不内置数据库、编译器、包管理器、用户身份系统、真实时 token 协议。
- 不从 `inspect --ir`、私有 IR serde、Rust 内存结构或旧版 `ViewExecutionReport` 获取事实。
- 不把浏览、刷新、导出、搜索解释为执行许可；不自动传递 `project` 模式权限或宿主环境密钥。
- 不为了复制旧 View 的“每句变量值”“断点”“Memory 全状态”而伪造事件。

### 1.4 完成判据

发布的同一个、已 SHA-256 固定的 `marqdo` 二进制中，未包含任何新 View 代码；一个全新独立 Git 仓库构建出的 `marqdo-view`，通过 `marqdo tool install ...` 后可以 `marqdo view`，正确阅读/静态导出/按用户显式授权执行，且独立二进制和内核 hash 在测试前后完全相同。HTML 视图可离线打开，读者不必知道内部 Rust 类型。详细门禁见 §18。

---

## 2. 视觉基准：随包截图的客观拆解

### 2.1 截图事实（OBSERVED）

截图尺寸约 **1664×920**，呈现以下空间组织（截图并非整个页面全长，不得从截图推断折叠区后续行为）：

1. **画布**：接近白色的暖纸/米色底，低对比边界，正文区域大面积留白，几乎没有重卡片。
2. **左上装饰**：一束由左上角向中间延伸的淡粉色樱花枝；是装饰，不承担语义、滚动或交互。
3. **顶栏**：左侧三横菜单图标；中间品牌图案、`Marqdo` 衬线字和极小大写 `EXECUTABLE KNOWLEDGE`；右侧搜索图标。顶栏清爽、横向居中而非复杂管理后台。
4. **顶部分段导航**：`文档 / 函数 / 变量 / 内存 / 运行`，水平居中，`文档` 采用柔和金色和下划线。**这五个名字属于截图旧产品的视觉/信息架构参考，不自动意味着 3.0 已提供完整变量/内存快照。**
5. **Welcome 区**：`WELCOME TO` 小字间距较大；巨大金色斜体 `Marqdo` 字样，右侧羽毛图案；下方口号 `代码即文档 · 文档即知识库 · 知识即可执行`。
6. **文件信息**：小尺寸文件名 `00-welcome.mq.md`，一个淡线条叙述折叠块，两个椭圆形导入/引用标签。
7. **主体开始**：`main` 大号黑色衬线标题，左侧细长金色倾斜竖线，旁有说明小字；下方显示 GFM 表，继续下方有英文正文。
8. **悬浮 Dock**：页面底部中央胶囊样式，约五个动作：刷新、设置、金色突出运行、清理、更多。主体可以从 Dock 后面连续滚动，Dock 不是侧栏或固定内容卡。
9. 画面右边有黑色短线/竖线，**仅凭截图不能判定其意图或成因**。禁止把可疑旧线条当成必须复刻的装饰需求；必须通过浏览器 DOM/Computed Style/截图重现确认。

### 2.2 可执行的视觉规范（REQUIRED）

- 基本色令牌为建议起点：`--paper:#fcf9f5`、`--surface:#fffcf8`、`--ink:#29251f`、`--muted:#847565`、`--line:#e7ddce`、`--gold:#b88a45`、`--olive:#52603a`。颜色可精调，但必须做到纸张感与足够对比度。
- 文档正文采用本地系统衬线栈：`Iowan Old Style`、`Palatino Linotype`、`Noto Serif CJK SC`、`Source Han Serif SC`、`Songti SC`、`SimSun`、`Georgia`、`serif`；代码以系统 monospace 栈显示，**不从 CDN 拉字体**。
- 宽屏阅读区建议文字主栏 720～900 CSS px；留出不干扰正文的侧注区域。不能用重边框堆砌页面，不能让函数区变成密集工程控制面板。
- Welcome 大标题只在**显式 Welcome 页面/站点首页配置**时出现；普通 `.mq.md` 不因为文件名包含 `welcome` 就强制出现大面积 Hero。
- 樱花、羽毛、Logo 应从旧仓库中筛选可复用资产（§3），核对权利和哈希后放入新仓库**版本管理**；不允许运行时读旧仓库路径。图片需要 alt 或 `aria-hidden=true` 合理分类。
- 页面基础视图是“连续 Markdown 长文”，不是逐单元卡片；函数及执行标注应可折叠、可隐藏。公式、表格、链接、代码围栏按 GFM/文档语义呈现。
- Dock 的每个按钮必须有可用动作。若某项只有旧版调试内核才能实现（如变量/内存快照），应标明“当前版本不支持”或**不展示该交互入口**，不可显示可点击的空按钮。
- 响应式：≥1440px 宽屏可开启左右侧注；900～1439px 收缩侧注；≤899px 侧注流入正文/抽屉；≤600px 单栏、Dock 不遮住正文末尾，必要时隐藏次级导航文字。打印时隐藏交互栏与装饰，但保留正文和必要注释。
- 动画须尊重 `prefers-reduced-motion`，键盘 focus、屏幕阅读器、无 JS 与 JS 失败时仍可阅读。

### 2.3 截图基准不是实现真相

**禁止“截图像了”就认定功能正确**。要同时通过语义、视觉和运行路径门禁。截图里看见 `main`、参数表、导入标签，不能证明其来自 3.0 的 Analysis；新 View 只能通过当下的 Analysis v2 显示这些语义内容。截图里点击 Run 的实际权限、是否自动执行、内存面板能否观察私有数据也无法推断，必须用新版 CLI 合同重新定义。

---

## 3. 旧版 View 源码取证审计：确实有多层样式和多条渲染路径

### 3.1 必须先阅读的证据（OBSERVED）

以下文件路径均在 `cflmy/marqdo@1c55baa`：

| 文件/位置 | 静态可确认事实 | 对新架构的影响 |
|---|---|---|
| `src/main.rs` 约 266–284、743–771、1657–1755 | 原生 `View`、`ViewAction::Output/Capture/Document` 与独立 `Debug` 路由；`ViewAction::Document` 使用 Workbench 文档模型并自行拼 HTML。 | 旧版至少有两类文档呈现入口，不能合并推断为一套一致行为。 |
| `src/view/mod.rs` 约 73–125、238–392 | Live `serve`、另一个 `serve_debug`；`/file` 调用 `build_file_view`，`/debug` 走 `build_debug_model/page_debug`，会话/Notebook API 路由很多。 | 新项目只保留主阅读路由与明确授权的运行路由。 |
| `src/view/html.rs` 约 30–1666、1667–1730、2849 以后、4210–4330 | `stylesheet()` 是大块基础 CSS；`notebook_ui()` 再插入 Notebook CSS/控件；`layout()` 在主模板中加载 `reader.css` 和 `reader.js`。 | 存在多层 CSS 级联和主题继承；不能整体复制 `html.rs`。 |
| `src/view/reader.css` 约 1–158 | `body.reader-page`、`body.reader-page.notebook-page` 高专用度覆盖旧皮肤；独立定义暖纸色、Hero、樱花、Dock、侧注、SVG 曲线。 | 截图风格的关键来源；可抽取设计令牌，但必须独立重写单一皮肤。 |
| `src/view/reader.js` 约 200–390、556–595 | 动态侧注几何与 SVG 连接线；按文件名正则生成 Welcome Hero。 | 新版改显式配置，不靠文档名匹配；连接线是**渐进增强**，需要几何测试。 |
| `src/view/render.rs` 与 `src/view/document.rs` | 旧版 AST 映射、叙述/注释保真、标注卡片。 | 可读思想可以保留；旧语法识别/AST 类型不可复用。 |
| `src/view/output.rs` 约 188–321 | 静态导出与 Live 共用 `page_file`，但其抓取/执行准备存在 `build_file_view` 与 `build_file_view_with_exec` 路线；默认写出路径未必天然无副作用。 | 新导出**必须纯渲染**，无 `run/call` 路径。 |
| `src/view/debug_page.rs` | 完整的深色 Debug UI、专用 CSS/JS 和调试 API。 | 与截图的轻量阅读器是另一个产品，首版不迁移。 |
| `src/view/knowledge_reading.rs` / `knowledge_present.rs` | 使用旧 `sema::SemanticProject` 的知识制品。 | 不是 Analysis v2/Proof 3.0；绝不能直接复制成新版知识验证。 |
| `src/view/assets/` | `blossom.*`、`feather.*`、`logo.*`、离线 KaTeX 资源等。 | **资产候选**，可复用时须确定来源/许可/摘要/尺寸；不能复制所有 vendor 依赖。 |

重要：这组证据足以认定**渲染入口和 CSS 主题多层重叠**；尚未对每一个历史 commit 的真实浏览器页面进行构建/抓包，因此**不能断言存在两套完全独立、同时可运行且争夺路由的 View 代码库，也不能声称截图精准对应当前旧仓库 HEAD 的执行结果**。截图是视觉证据，静态源码是结构证据，运行输出须另验。

### 3.2 旧版必须实际执行的取证程序（开发者复核，不作为新项目依赖）

在一个干净且可编译的旧仓库 checkout 上（不覆盖原始工作树）：

```bash
# 仅示意取证命令；若旧项目编译失败，标记 UNVERIFIED，不用模拟页面代替
rg -n 'ViewAction|Commands::View|serve_debug|page_file|page_debug|layout_debug' src/main.rs src/view
rg -n 'stylesheet\(|notebook_ui\(|reader\.css|reader\.js|reader-page|notebook-page' src/view
rg -n 'build_file_view|run_file_capture|write_static|no_exec' src/view
rg -n 'reader-hero|reader-dock|reader-connectors|annot-band' src/view
```

- 对 `marqdo view FILE`、`marqdo view output FILE`、`marqdo view document`、`marqdo debug` 分别记录 **命令→函数→模板→CSS/JS→HTML** 路径（`docs/audits/legacy-render-paths.md`），不能把路径名字相似视为共享组件。
- 如果能够启动，在宽屏/窄屏使用浏览器开发工具保存 DOM、Computed Style、规则命中源文件、Network、截图；确认 `stylesheet → notebook_ui → reader.css` 的实际覆盖关系；记录 `data-theme`、`reader-page`、`notebook-page` 是否同时存在。
- 每一条旧样式规则标记为 `KEEP_VISUAL`、`REWRITE`、`DROP`、`UNKNOWN`；**不迁移旧库 CSS 原文的叠层结构**。只把可用素材加入 `assets/`，避免暗中携带旧语义和调试状态。
- 如果无法得到旧版真实运行输出，保留上述静态审计并明确注明 `LEGACY_RUNTIME_UNVERIFIED`，依靠截图和本 RFC 的目标渲染进行验收，不得凭空补齐缺失结果。

### 3.3 防止再次形成双 View 的建筑门禁

- 新仓库只允许一个 `ReaderRenderer` 公共入口、一个 `build_presentation()`、一个 `DocumentModel` 定义；Live/Export 只能选择传输与链接重写策略，不能选择另一套渲染器。
- 前端只有一个正式 CSS 主题入口 `assets/reader.css`、一个 JS 入口 `assets/reader.js`；如内部细分多个源文件，最终必须合成单一有序资产清单，且主题令牌只有一处定义。
- **禁止** `legacy.css`、`new.css`、`notebook.css` 并行后依赖 `!important` 覆盖；禁止在服务端拼接一套旧控件、前端再隐藏另一套。
- 欢迎页由显式 View 路由或项目元数据选择；不以 `00-welcome` 文件名正则改变语义或默认页面。
- UI 模式只改变呈现（Document/Source/Execution），不加载第二套 AST，也不在 JS 中解析 `@exec`。
- 建立静态代码门禁，见 §18：不允许 `marqdo::`、`marqdo_ir`、`marqdo_frontend` 等依赖，检查唯一的主题文件/渲染入口。

---

## 4. Marqdo CLI 3.0 的权威协议与版本固定

必须读取发布版和仓库中的以下文档，作为**精确语义依据**（本文已摘要可实施要求，引用是方便对照，并非要求开发者寻找额外聊天背景）：

| CLI 文件 | 公开约定/作用 |
|---|---|
| `spec/mapping.md`、`spec/markdown-profile.md`、`spec/expression.ebnf` | 唯一 Markdown → 执行语义、词法与函数规则 |
| `spec/analysis-v2.md`、`spec/schemas/analysis-v2.schema.json` | 静态 Analysis v2 的 JSON 与源位置 |
| `spec/trace-v2.md`、`spec/schemas/trace-v2.schema.json` | 执行 Trace v2 与 Analysis 字节摘要绑定 |
| `spec/tool-v1.md`、`spec/schemas/tool-v1.schema.json` | 独立 CLI Tool 安装/分发协议 |
| `docs/developers/tools.md`、`docs/developers/analysis-trace.md` | 对开发者的真实用法与限制 |
| `docs/developers/compatibility.md`、`docs/developers/release-limits.md` | Beta 平台、版本和安全保证边界 |
| `docs/releases/3.0.0-beta.1.md` | 发布构建、官方 SHA 与发行声明 |

**不同版本轴不得混淆**：产品 `3.0.0-beta.1`、Analysis `2`、Trace `2`、Tool `1`、Markdown profile `marqdo.md.v1`、lock `4` 相互独立。无法协商协议、或 JSON schema 版本未知时必须显式错误，不能回退试猜 `inspect --ir`。

### 4.1 CLI Tool 的安装合同

`marqdo-cli` 已存在通用工具命令（不需要修改核心）：

```bash
marqdo tool install /absolute/path/to/marqdo-view/tool.json
marqdo tool list
marqdo view --help
marqdo view index.mq.md
marqdo tool remove view
```

工具发布 bundle 的根目录内必须有 `tool.json`，以及匹配当前平台的真实可执行 `bin/marqdo-view`：

```json
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "marqdo-view",
  "version": "0.1.0",
  "command": "view",
  "entrypoint": "bin/marqdo-view",
  "compatibility": {
    "marqdo": ">=3.0.0-beta.1,<4.0.0",
    "analysis": [2],
    "trace": [2]
  }
}
```

注：`>=3.0.0,<4.0.0` **不能**匹配 `3.0.0-beta.1`；若 View 首版只消费 Analysis 不消费 Trace，可以在尚无 Trace 功能时省略 `trace` 字段，但此时不可宣称支持运行态。CLI 安装 Tool 时复制整个 bundle 并按文件摘要验证；**修改已安装程序之后必须重新执行 `tool install` 或正确的重装流程**，不要要求用户手工改受控的已安装 bundle。

工具得到的环境（由已发布 CLI 注入）：

```text
MARQDO_BIN                当前发布的 marqdo 可执行程序绝对路径
MARQDO_TOOL_PROTOCOL      1
MARQDO_PROTOCOL_ANALYSIS  2（声明并协商时）
MARQDO_PROTOCOL_TRACE     2（声明并协商时）
```

`marqdo-view` 构造子进程时用命令参数数组，**不得经过 `sh -c`**。与核心交互的所有动作使用 `MARQDO_BIN`，不要猜 PATH 上的版本。单独直启 `marqdo-view` 时可支持显式 `--marqdo-bin PATH` 开发参数，但该行为属于 View 自身，不得改变 Tool Protocol。

### 4.2 唯一允许的 CLI 消费渠道

**静态：**

```bash
"$MARQDO_BIN" check /project/index.mq.md --emit /private/temp/analysis.json
# 可选：要检查文档链接时使用 --refs；它可能使 check 失败
"$MARQDO_BIN" check /project/index.mq.md --refs --emit /private/temp/analysis.json
```

**动态（用户点击运行后才发生）：**

```bash
"$MARQDO_BIN" run /project/index.mq.md --mode pure --json \
  --trace-file /private/temp/trace.json
```

这里的 `--mode pure` 是 View 推荐的**最低权限首选模式**，不意味着适用于任何具有 I/O 的项目；如果程序需要显式 project 能力，应由用户确认模式、路径/网络 grants 及风险，View 显示完整计划和结果，不得暗中降级或提权。执行行为与具体 grant 语法必须遵循 CLI 对应发布版的 `--help` 和 `spec/security.md`，不能自造授权语法。

- `check` 错误时不读取/缓存旧的成功 Analysis；Core 会移除目标，View 自己也必须废弃前一次成功记录。
- `run`/`call` 的 stdout 以 CLI JSON/实际文件为准，不从 stderr 文本拼接“伪 Trace”。
- `inspect --ir --json` 不是已冻结的公共跨仓协议，不可作为 View 的业务依赖。
- 使用 `--refs` 是更严格的引用有效性检查，不能和“基础语法检查”混为一谈；引用失败时仍需允许源码 Markdown 浏览与诊断显示。

### 4.3 Analysis v2 必须理解的字段

顶层为 `{schema:"marqdo.analysis",schema_version:2,producer,project,modules,diagnostics}`；`modules[]` 中含 `id`,`package_instance_id`,`relative_path`,`source_digest`,`units`,`functions`,`imports`,`references`,`calls`,`resources`。核心还提供：

- `units[].id/name/level/kind/span/function`：语义单元与 H1～H6 所属关系，`kind` 可为 `root/namespace/function/knowledge/section`。**当前此 JSON 不保证把“每一个 Section 的 Exec/Knowledge mode”作为独立字段输出；不能把 `kind=section` 直接等同于 Exec 或 Knowledge。**
- `functions[].id/unit_id/local_name/namespace/span/heading_span/signature/body/effects`：函数与语句静态投影；`effects:["unknown"]` 是**保守不确定**，不是“已证明纯净”。
- `functions[].body[].kind` 当前有 `assign`、`expression`、`branch`、`loop`、`return`；含复合表达式、条件/分支臂和 UTF-8 `span`。**标记类型决定不了语句种类**。
- `references[]` 是静态 Markdown 引用图，不是执行调用。`calls[]` 由 Core 唯一绑定表投影，不允许 View 再独立给插件/内建函数重新解析优先级。
- `resources[]` 有 `formula`、`code` 及 span、digest 等；公式和 fenced code 是**惰性资源**，即使位于 Exec 范围内也不能自动执行。
- `diagnostics[]` 的 severity/code/message/span/module/line/column 是编译诊断，允许 View 将其链接到原始源码区间，不得将风格建议伪装成 Core error。
- `project.root_id`、`lock_digest`、可选 `stdlib` 标识用于确定运行文档和版本；不应以绝对缓存路径作身份。

### 4.4 Trace v2 必须理解的字段及缺失项

顶层 `{schema:"marqdo.trace",schema_version:2,run,source,limits,truncated,events}`；主要事件：`call.enter`、`call.exit`、`branch.eval`、`loop.iteration`、`return`、`effect`、`trace.truncated`。事件含 `seq`、相关 `module_id`、可选 `span` 和类型信息。**Trace v2 不存参数实际值、敏感数据或文件内容，也不提供每次赋值后的完整值、变量历史快照或旧 View 的 ViewExecutionReport。**

必须校验：

```text
sha256(exact bytes of current analysis.json) == trace.source.analysis_digest
trace.source.root_id == analysis.project.root_id
all event.module_id references exist in analysis.modules (when non-null)
```

- 两份数据未匹配：标记 `STALE`，保留原始结果下载入口但禁止关联到“当前执行态”。
- Trace `truncated != null`：标记 `TRUNCATED`；不可声明“已遍历全部分支/循环”。
- 未开始：`NOT_RUN`；仅静态检查成功：`STATIC_OK`；执行失败：`RUN_ERROR`；缺能力：`UNSUPPORTED`。这些状态的视觉文案、颜色及辅助说明由 Presentation 层提供，而不是把所有 `ok` 渲染为“知识已证实”。

---

## 5. 3.0 Markdown 语义：展示必须与编译器分权

### 5.1 必须准确传达的规则（来自 CLI `spec/mapping.md`）

1. 文档根为 Knowledge；H1～H6 标题 `@exec` / `@knowledge` 可以显式声明模式，否则**继承最近祖先标题**的模式。
2. **只有 `@exec` 模式的 H2 是可调用函数**；H1 负责命名空间；H2 Knowledge 是惰性知识单元；H3～H6 是段落/函数内部 section，永远不是新函数。
3. 在 Exec 区，`**strong**`、`*emph*`、行内 `` `code` `` 是**等价的单语句载体**，而 `if/else/while/end/return/params(...)` 等关键字决定操作。列表、引用、段落是文档容器，不是执行/分支/循环指令。
4. `@knowledge` 范围的载荷完全惰性；GFM 表格、代码围栏、公式、HTML 和链接不是可执行语句载体。`**` 不等于“执行”、`*` 不等于“返回”、有序列表不等于“分支”。
5. 不允许通过 `@agent` 等新标题标记注入核心语义。未知标题标记属于 Core 的语法诊断，View 只展示。
6. H3 Exec 仍归属于前面的所属 H2 Exec；分支/循环不能跨标题段落，具体错误以 Core 为准。
7. 语法错误不能静默回退为“这段只是纯文本”；普通 Markdown 浏览可以继续，但语义高亮/可执行状态须标记无效。

### 5.2 建议的 UI 呈现

- 标题的**视觉正文**可以去除合法标题声明前缀 `@exec`/`@knowledge` 并在旁边用轻量文本标识（例如 `EXEC`、`KNOWLEDGE`）；原文标题必须在 Source 态完整呈现。去掉前缀是**View 的显示规则，不修改源文件**。
- 合法的 `Exec H2` 显示函数名、参数摘要、轻金色侧标；普通 Exec 子章节默认阅读流中展示内容，静态控制流可以用侧边次级标注。
- Knowledge 单元显示文档书卷/知识标记，但不承诺“已验证”；没有 Prove 结果时绝不出现“已证明”。
- 用 Analysis body 的 `kind` 判定赋值、返回、条件/循环；仅当源码 `span` 精确命中有效数据才创建语义标注。**不可仅通过 CSS strong/em/code 判断操作。**
- 对 Exec 中作为**装饰性的强调/行内代码**发生歧义时，让 Core `check` 来判定；View 不自行静默忽略执行载荷，也不擅自修复源文件。以后由 `marqdo-lint` 另提供风格建议。

### 5.3 视觉与语义一致性示例（示例素材，开发前用发布 CLI 验证）

```markdown
# @knowledge 计算笔记

这是一份关于简单运算的知识说明。**此处加粗是知识内容，不执行。**

## @knowledge 算法说明

下面描述算法的含义和适用范围；此处的 `n` 是惰性正文。

# @exec 演示

## @exec main

`params(n: Int)`

### @knowledge 用法

调用方提供整数 `n`。这一小节不应该执行任何语句。

### @exec 计算

**if n > 0**

`result = n * 2`

*return result*

**else**

`return 0`

**end**
```

必须至少覆盖：显式/继承模式、标题回切、粗体/斜体/行内代码同义语句、中文 UTF-8、知识中的代码跨度、H3 Exec、GFM 表格与 fenced code。可执行示例最终由 CLI 生成 Analysis golden；如果样例因 Beta 语法演进被 Core 判为不合法，**修改 fixture 并记录原因，绝不能让 View 的预期覆盖 Core 的诊断**。

---
## 6. 总体架构、数据流与代码依赖边界

### 6.1 强制的单向依赖

```text
                  用户浏览器 / 静态 HTML 文件
                            │
                Reader UI（单一 CSS / JS）
                            │
                ReaderRenderer（唯一模板）
                            │
              PresentationModel（唯一模型）
                  ╱          │           ╲
      DocumentModel    SemanticProjection    RunProjection
      原始 Markdown      Analysis v2            Trace v2
                  ╲          │           ╱
                    PublicCoreClient
                            │
             MARQDO_BIN（仅子进程 + 公开 JSON）
                            │
       已发布 marqdo CLI（不可更改／不可重编译）

旁路：可选 OverlayProvider（未来 Knowledge / Prove / Lint）
      只合成显示标注，不影响文档解析与运行语义。
```

**依赖方向：** `CLI/Server → Application → Domain → ProtocolAdapter` 和 `Application → Presentation → Renderer → Assets`。Domain/Pipeline 不允许反向引用 HTTP Request 或浏览器 DOM。Reader JS 只负责交互/视觉布局，不能自行解释 Marqdo 语义。

### 6.2 层级与唯一职责

| 层 | 责任 | 明确禁止 |
|---|---|---|
| Command/CLI | 解析 `view` 自身参数、选择 Browse/Export、配置用户显式运行意图 | 解析 `.mq.md` 为可执行语句 |
| PublicCoreClient | 查找 `MARQDO_BIN`、启动 `check/run`、校验退出状态与公开协议版本 | 引用 CLI Rust crate、私有 IR、旧运行时 |
| ProjectReader | 在用户给定根下列出、读取原始 `.mq.md`，构造源码摘要与稳定路径 | 未授权走出项目根/扫描整个 home |
| SourceIndex | 原文字节偏移、行列、源码片段与文档节点的关联 | 自行决定哪些语句生效 |
| SemanticProjection | 校验 Analysis v2，提取函数、Unit、span、诊断、引用、调用静态事实 | 重新实现 Callable 优先级、重写 AST |
| RunProjection | 校验 Trace v2、来源绑定、状态/事件，提供可视化事实 | 推导实际变量值/假装证明知识 |
| PresentationModel | 将文档、语义、运行信息组织成稳定可渲染结构 | 混入 HTML 字符串、Host 权限、外部进程 |
| ReaderRenderer | 唯一 HTML 模板和安全 Markdown 输出、Live/Export 共用 | 因导出/Debug 生成另一套文档内容 |
| HTTP Server | localhost 静态资源、读取 API、用户确认后执行 API | 自动执行、跨站可发起运行、访问任意文件 |
| Exporter | 写无执行副作用的离线站点、重写相对链接 | 创建运行会话、隐式连接外网 |
| Web Assets | 阅读交互、搜索、侧注、曲线、主题、键盘操作 | 解译控制流/拼造 Analysis 记录 |

### 6.3 所有组件的共享数据契约（建议 Rust 类型/伪码）

开发者需实现等价的带版本模型，字段可按实际 Rust 习惯组织，但**语义必须保持**：

```rust
struct DocumentIdentity {
    project_root_id: String,    // 来自 Analysis.project.root_id
    module_id: String,          // Analysis.modules[].id
    relative_path: String,      // Analysis.modules[].relative_path
    source_sha256: String,      // 与原始 bytes 比较
}

struct SourceSpan { start_byte: u64, end_byte: u64 } // 内部半开区间

struct DocumentModel {
    identity: DocumentIdentity,
    source_bytes: Vec<u8>,      // 只允许有效 UTF-8，渲染用文本引用
    markdown_blocks: Vec<DocBlock>,
    source_index: SourceIndex,
}

struct SemanticProjection {
    schema_version: u32,        // 必须为 2
    analysis_sha256: String,    // exact JSON bytes
    functions: Vec<SemanticFunction>,
    units: Vec<SemanticUnit>,
    statements: Vec<SemanticStatement>,
    diagnostics: Vec<CoreDiagnostic>,
    calls: Vec<StaticCall>,
    imports: Vec<StaticImport>,
    resources: Vec<StaticResource>,
}

enum RunDisplayState {
    NotRun,
    StaticOnly,
    Running,
    Succeeded,
    Failed { core_code: String },
    Stale,
    Truncated,
    Unsupported,
}

struct RunProjection {
    source_analysis_sha256: String,
    run_id: String,
    status: RunDisplayState,
    events: Vec<TrustedTraceEvent>,
    // 不定义 assignment_values / secret_args；Trace v2 不提供这些字段
}

struct PresentationModel {
    version: u32,               // 私有 View 呈现模型版本，初始 1
    document: DocumentModel,
    semantic: Option<SemanticProjection>,
    run: Option<RunProjection>,
    overlays: Vec<DisplayOverlay>,
    source_status: SourceStatus,
}
```

这里 `DocBlock` 是**排版结构**：段落、标题、列表、表格、代码围栏、数学、链接、引用、正文文本；不能是第二套 Marqdo AST/IR。只允许从“已成功校验的 Analysis”注入函数、语句的语义装饰。`PresentationModel` 应输出确定性的 UI 数据（可以作为内部测试 JSON），但不宣称它是 CLI 或 Knowledge/Prove 公共协议。

**单一身份键**：渲染标注以 `module_id + source_span + kind + stable local ordinal` 唯一定位；不得直接使用 HTML 自增索引或某个物理文件缓存路径作为跨运行身份。未来 UI 生成随机短 ID 时必须有稳定映射表，不能用于协议或缓存身份。

### 6.4 纯函数数据流（唯一核心管线）

```text
browse/export(path)
  0. resolve project root + selected file (read boundary)
  1. read raw Markdown UTF-8; compute SHA-256; parse for display only
  2. Core.check emits Analysis v2 into private temporary file
  3. validate schema/protocol, module identity, source digest and spans
  4. build DocumentModel + SemanticProjection; on Core error keep prose+diagnostics
  5. build PresentationModel; RunProjection initially None
  6. ReaderRenderer renders HTML + local hashed CSS/JS assets
  7a. live: serve same HTML at localhost, link to controlled APIs
  7b. export: write same HTML/asset set with safe relative links

user explicitly clicks Run
  8. show mode, command, requested capabilities, program path, risk confirmation
  9. Core.run creates Trace v2 + JSON result via MARQDO_BIN
 10. validate trace/source digest and event module ids; return immutable RunProjection
 11. rebuild or update same PresentationModel; UI maps trusted events to existing source anchors
```

第 1 步“为显示解析 Markdown”是合规的：保留 GFM 表、列表和排版需要 Markdown 解析器；但**禁止**该解析器的结果参与是否执行的结论。Core `check` 不可用时，View 可以退化为**无语义标注**的 Markdown 阅读器，必须标注“语言语义未验证”。

### 6.5 缓存策略

- 静态 Analysis 缓存键至少包含 `(CLI product version, public analysis protocol major, project root identity, entry file, lock digest, sorted source digests, relevant check flags)`；如果不能完整收集依赖变化，保守策略是**每次变更重新 check**，不要漏更新。
- SourceIndex 根据精确原始 bytes 建立；文件改变就立即使所有 span 绑定失效，直到重新校验 Analysis。
- Trace 缓存以 `source.analysis_digest + run.id` 为键；**永不**因为文件路径一致就沿用旧轨迹。Trace 协议现有 `run.id` 不保证用于跨机器稳定排序，UI 不得因此断言可重放。
- 缓存只在用户本地临时目录/明确配置的数据目录。不得偷偷修改源项目或生成未加入 `.gitignore` 的杂乱制品。
- 不在公开 HTML/JSON 中输出绝对机密路径、Token、外部凭据；错误信息尽量规范化为项目相对路径。

---

## 7. 仓库布局：单 Package、单 Reader、清晰边界

**初期只创建一个 Rust crate**；前端默认原生 JS/CSS（以后若有充分复杂度可用构建工具，但**最终只有一份 Reader runtime**）。推荐初始目录：

```text
marqdo-view/
├── README.md                      # 一分钟安装/命令/能力边界
├── LICENSE-MIT / LICENSE-APACHE    # 仿照当前 CLI 项目许可并确认资产许可
├── CHANGELOG.md
├── COMPATIBILITY.md                # View版本/CLI范围/Analysis/Trace/OS
├── Cargo.toml                      # 单 package，不把 CLI crate 作为依赖
├── Cargo.lock                      # 发布固定依赖
├── tool.json                       # 发布 bundle 合同（构建时固定）
├── src/
│   ├── main.rs                     # 入口：只分发 CLI/应用层
│   ├── cli.rs                      # browse/export/verify 参数
│   ├── core_client.rs              # MARQDO_BIN + subprocess + temp artifacts
│   ├── project.rs                  # 路径边界、文件与项目读取
│   ├── source_map.rs               # UTF-8 bytes/span/行列转换
│   ├── document.rs                 # Markdown 读者模型，不是第二编译器
│   ├── semantic.rs                 # Analysis v2 校验与投影
│   ├── trace.rs                    # Trace v2 校验与投影
│   ├── presentation.rs             # 唯一 PresentationModel 构建入口
│   ├── render.rs                   # 唯一 HTML 阅读器入口
│   ├── server.rs                   # 127.0.0.1 read API + authenticated Run POST
│   ├── export.rs                   # 同 ReaderRenderer 的离线导出
│   └── security.rs                 # HTML/safe path/security policy
├── assets/
│   ├── reader.css                  # 唯一 CSS 入口，含 token/theme/layout
│   ├── reader.js                   # 唯一 JS 入口，DOM 交互及侧注布局
│   ├── logo.svg
│   ├── blossom.webp
│   ├── feather.webp
│   └── licenses/                  # 每个第三方素材/库的来源与许可
├── examples/
│   ├── welcome/                    # 显式 Welcome 项目，3.0 语法
│   ├── mixed-modes/                # knowledge/exec/继承/切换
│   ├── branch-loop/                # 分支/循环 Trace 示例
│   ├── tables-math/                # GFM/公式/围栏惰性资源
│   └── diagnostics/               # 无效/冲突源码
├── tests/
│   ├── contracts/                  # 冻结 CLI + JSON 协议
│   ├── semantic/                   # span、unit、语义正反例
│   ├── rendering/                  # HTML结构/安全/静态导出
│   ├── browser/                    # Playwright/Chromium 视觉及交互
│   ├── security/                   # 浏览器 XSS/CSRF/path/exec
│   └── golden/                     # 固定输出/基线元数据
├── docs/
│   ├── RFC-VIEW-0001.md            # 本文
│   ├── STATUS.md                   # 唯一现状，证据、限制、目标SHA
│   ├── ADR/                        # 新的关键设计决策，避免临时散文档
│   ├── design/
│   │   ├── reference-old-view-reader.png
│   │   ├── visual-tokens.md        # 截图拆解与颜色/尺寸
│   │   └── state-matrix.md         # NOT_RUN/STALE/ERROR的视觉状态
│   ├── audits/
│   │   └── legacy-render-paths.md  # §3 的历史取证结论
│   └── evidence/
│       └── <CLI-SHA>/             # 真实CI/冻结内核/视觉验收证据
└── dist/                           # 构建输出，gitignore；不能放源码
    └── marqdo-view-<version>-<target>/
        ├── tool.json
        ├── bin/marqdo-view
        └── assets/...              # 如外置资源需随包完整分发
```

- `src` 中的文件只有独立职责，不为每个概念开 crate，也不在 `lib/`/`core/`/`infra/`/`service/`/`engine/` 建立五层空目录。若单个模块超过维护能力，再基于稳定职责拆文件。
- 文档只有一个状态源 `docs/STATUS.md`；RFC、ADR、Changelog 各有唯一职责。不要把每天的“现状”复制到多个文件使其互相矛盾。
- `assets/` 下只保留**实际进入构建**的资源，禁止把原仓库全部图标/Notebook CSS/调试器 JS/vendor 复制过来。
- 发布 `tool.json` 中的 `entrypoint` 必须确实指向 bundle 内可执行文件；可执行文件可把 CSS/JS/小尺寸图作为编译内嵌资源，或以受控相对路径读取。两种方式择一固定，不能既内嵌又加载另一套旧 CSS。**推荐首版把资产以内嵌常量提供给 Live，并在 Export 时写出同一份静态字节。**
- 构建脚本负责生成 bundle、摘要和安装烟测；`dist/` 不允许入 Git。

### 7.1 受控的前端样式规则

推荐唯一 `reader.css` 中固定顺序：

```css
@layer reset, tokens, layout, typography, semantics, overlays, interactions, responsive;
/* 所有设计令牌只在 tokens 出现一次；不使用旧版 notebook-page 兼容变量。 */
```

使用例如 `.mq-reader` 作为单一组件根命名空间，语义样式 `.mq-reader [data-semantic="exec"]`、`[data-semantic="knowledge"]`、`[data-run-state="stale"]`；禁止散布 `body.reader-page.notebook-page ... !important` 等叠层覆盖技术。

JS 只操作 `[data-action]`、`[data-source-id]`、CSS 变量/少量 `data-*` 状态；代码中不能硬编码大段 HTML 来替代服务端 ReaderRenderer。执行后更新应重新使用同一套 PresentationModel 的增量/整体渲染结果。除 Welcome 配置外，**不允许按文件名正则插入 UI**。

---

## 8. HTML/Markdown 渲染与源码定位实现细则

### 8.1 Markdown 保真

- 使用公开社区 Markdown 解析器（例如与 CommonMark/GFM 行为兼容的 Comrak），**以排版为目的**生成 HTML。遵循 `marqdo-cli/spec/markdown-profile.md` 中明确的可见 Markdown 规则；不要直接依赖核心私有 `marqdo-markdown` Rust crate。
- `@exec`/`@knowledge` 在普通 Markdown 的标题文本中自然存在：源码态绝不改写；文档态可以以视觉图标/副标题代替显式前缀，但显示转换须有来源位置、可逆，并在 hover/复制源码时恢复原文。
- GFM 表格、任务列表、段落、块引用、链接、图片、fenced/inline code、数学必须可阅读；**不得把 HTML 内容作为用户有权限执行的脚本**。
- Markdown 原文中的主题标签、HTML 标签以及脚本应按明确且保守的 HTML 净化策略处理。默认关闭/转义未经信任的 raw HTML；如果要允许安全子集，采用维护中的白名单净化器，禁止 `script`, `iframe`, `on*` 属性、`javascript:`、任意 `style`、未知 SVG 动态内容。保留格式化语义但不破坏安全。
- 任何 Markdown 渲染结果都不能自动转发 `file://`、任意本地 HTTP、外部链接探测；外部网络资源默认阻止，显式用户点击才导航。图像默认只展示已确认在根目录内的本地资源或安全 data URL，远程加载可作为用户显式许可选项。

### 8.2 SourceIndex：唯一位置算法

Core Analysis/Trace 的源位置是 UTF-8 **字节偏移**；浏览器字符串与普通 JS `string.length` 使用 UTF-16 code units。这是中文、emoji 与数学文档中最容易引起锚点错位的地方。必须：

1. 按**确切 UTF-8 原始字节**计算 `source_digest` 与 span 边界。不要先做 Unicode NFC 或 CRLF 规范化再比对 Core Digest。
2. 读取后构造 `byte_offset → (line,column)` 与 `byte_offset → JS UTF-16 offset` 两张可验证映射，范围检查、边界落在合法 UTF-8 码点边界才接受；Core span 是否为半开区间按发布版 schema/fixtures 校验，然后内部统一为 `[start,end)`。
3. DOM 位置基于**被渲染的源码节点与字节区间交集**建立锚点；不要在渲染后的整段 HTML 上按字符串查找 `return`/`if` 等文本来反向定位。
4. 当 CommonMark 文档 AST 的 sourcepos 不能精确映射到 Core 的语句 span 时，跳过该单条语义装饰并显示一条“无法精确定位”诊断；**保留完整原文**，不可让注释错指向别的语句。
5. 源文件被改动、Raw Markdown 无效 UTF-8、span 越界、某块节点被 sanitizer 移除时，语义坐标即为不可用；不得沿用旧字节索引。

测试必须包括中文、emoji（多字节）、组合符、CRLF/LF、Tab、同一行三个载荷、重复的 `return` 文本、代码围栏中的 `@exec`、HTML 转义前后的位置、长表格与嵌套列表。

### 8.3 HTML 锚点与元素标识

建议使用可确定的标注标识：

```text
source-id = sha256(module_id + "\0" + start_byte + ":" + end_byte + ":" + type)[0..16]
```

该 ID 只是**本次文档内的 DOM 身份**；绝不能表示可跨源码编辑不变的函数或知识证明标识。稳定、可寻址的函数/单元必须使用 Analysis 公开 `id` 与当前 `root_id` 组合。

- 锚点与侧注关系用 `data-source-id` / `aria-describedby`，不要依赖页面行号（随更改漂移）。
- 连线布局在 `requestAnimationFrame` 或 ResizeObserver 调度，避免每个字符变动重排；同一来源多个侧注共享锚点需按 `id` 去重。
- 连接线只能显示**确有来源的**静态语义或真实事件；不要通过装饰性线条暗示一个正文陈述已通过证明。
- 函数和 Knowledge 标题支持键盘锚点跳转及复制固定链接，避免生成相同 ID 导致跳错。

---

## 9. 前端信息架构：以文档为主，不再复制后台/Notebook

### 9.1 页面骨架（响应式）

```text
┌───────────────────────────────────────────────────────────────────────┐
│ ☰                        Marqdo  · EXECUTABLE KNOWLEDGE           搜索│
│              文档       函数       [可选结果]         运行           │
├───────────────────────────────────────────────────────────────────────┤
│   左侧樱花（仅装饰）                                                  │
│                                                                       │
│          [Welcome 时可有的 WELCOME TO / Marqdo 大标题]                │
│                                                                       │
│          source-file.mq.md    [状态：静态检查已通过/未运行]            │
│                                                                       │
│        ︱ 知识说明 / 函数标题            右侧轻量说明 / 本文目录         │
│          Markdown 正文                                                │
│          行内语义锚点──曲线（仅真实来源）──简短侧注                    │
│          GFM 表、公式、代码围栏                                        │
│                                                                       │
│                   [源码/文档/执行 交互层]                              │
│                                                                       │
│                      [刷新] [设置] [运行] [更多]                       │
└───────────────────────────────────────────────────────────────────────┘
```

截图的“函数/变量/内存/运行”可保持布局气质，但新版本必须按真实能力分类：

| 导航/动作 | 首版行为 | 进阶行为 |
|---|---|---|
| 文档 | 默认展示正文 | 点击语义源位置跳到关联代码 |
| 函数 | Analysis functions 大纲/签名，跳转 | 与真实调用轨迹定位 |
| 变量 | **不展示完整运行变量值**；只能展示合法声明的参数/静态赋值目标，明确称“静态变量信息” | 等将来独立、受权限审计的调试/观测协议 |
| 内存 | 首版**不可提供真实内存快照**；可以不显示 Tab，或只显示“尚不支持”的明确说明 | 新的公开可选协议发布后再启用 |
| 运行 | 本次执行状态、真实 Trace 事件和输出，不自动运行 | 更多历史运行记录，需来源与隐私控制 |
| 搜索 | 在当前源文件与已加载文档文字内本地搜索 | 多文档可控搜索（不自动遍历隐私目录） |
| 悬浮运行按钮 | 必须先确认本次运行意图，执行 Core CLI | 支持已授权的、明确显示的 grants 配置 |
| 悬浮清理 | 仅清空 View 当前展示的运行结果和缓存 | 不擅自删除用户项目文件/源代码 |

建议默认顶部导航在 Beta 第一版做 `文档 / 函数 / 运行`，若希望视觉上尽量靠近截图，可以把 `变量 / 内存` 以**明确禁用且解释限制**的形式置于“更多”，而不是制造假功能。

### 9.2 三态的可读性规则

- **Document**：原文字段是主角，导入依赖用小标签而不是两栏表单；函数、知识、普通章节可阅读性应一致。控制流标注属于次级视觉。
- **Source**：显示源文件字节忠实文本与行号；复制/下载按钮不改写文档；可从当前 Document 锚点切过去。
- **Execution**：仍保留同一篇 Document；追加调用/分支/循环的可靠事件与本次运行摘要。不以“另一个白屏报告”替换当前阅读位置。
- **Knowledge 不能误当 Verified**：在 UI 中 `@knowledge` 是声明**惰性知识区域**，不代表事实被证明。证明结果未来必须有独立证书与对应源码/语义 hash 后才能显示。
- **Exec 也不能误当 Executed**：`@exec` 表示可执行语义区域，不代表用户已运行。用 `Executable / Not run / Ran / Failed` 等精确词描述。

### 9.3 侧注与曲线（必须克制）

用户提供截图主要展示了顶部/正文，并没有展示完整的侧注曲线交互。旧仓库 `reader.css`/`reader.js` 中确实有侧注和 SVG 连线能力，可以提炼为新的 `AnnotationLayout`，但不要盲目移植旧数据结构。

- 推荐桌面端只有**与当前阅读位置有关**的注释；正文段落右侧优先，宽屏允许左右错落排布以减少交叉。
- 曲线使用连续 Bezier，避免较长直线或方折线；起点连到真实语义锚点底部，终点连到注释标题；不覆盖源文本。
- 对每个锚点有明确 `min clearance`，如果要越过正文或无法避让，必须退化成**正文内联 keyed 注释**，不能画一条错指向的线。
- 每段常显注释建议 ≤3 条，拥挤时合并为 `+N`；无 JS、移动端、打印时始终是可访问的顺序内联注释。
- 点击/hover 同步高亮**相同 source-id**；不能跨 Analysis digest/模块错误关联。侧注移动不能影响 Markdown 原文 DOM 顺序。
- 不应要求所有公式、列表和代码段都出现连线；如果没有可信事实就没有连线。

### 9.4 Welcome 与普通文章区别

Welcome 是**站点级配置**，不能以 `00-welcome.mq.md` 文件名猜测。建议新增只属于 View 的**非 Marqdo 语言语义**配置文件（可选），例如 `view.json`：

```json
{
  "schema": "marqdo.view-config",
  "schema_version": 1,
  "welcome": "./docs/00-welcome.mq.md",
  "site_title": "Marqdo",
  "theme": "botanical"
}
```

这只是新工具的建议配置格式，**不是当前 CLI 的既有协议**。默认无配置时，以明确传入文件展示正文；传入目录时选择索引页或经稳定排序后首个 `.mq.md`，文档在 README 中写清楚。配置不能授予执行权限、不能改变执行语义、不能创建虚构的 `@knowledge` 单元。配置路径同样受项目根目录约束。

---
## 10. CLI 用户界面、命令协议和错误码

### 10.1 CLI 命令范围（本 RFC 新设计，不是核心已提供的命令）

建议初版：

```bash
# 通过 Tool v1 注册后的用户命令
marqdo view [PATH] [--port 7429] [--host 127.0.0.1] [--no-open]
marqdo view export [PATH] --out ./site [--no-annotations]
marqdo view verify [PATH]          # 只核对 CLI/Analysis/Source；无执行
marqdo view --help

# 开发/调试：直接运行独立 View 程序
marqdo-view [PATH] --marqdo-bin /opt/marqdo/bin/marqdo --no-open
```

这里 `view` 是 Tool Protocol 注册的**顶层命令**，Core 不知道后面的 `export`、`verify`、`--no-open`，这些选项全部由 View 的一个 `clap` parser 负责。View 不需要以 `view` 参数再次调用自己。为了避免目录真的叫 `export` 的歧义，建议：发现 `export/verify` 关键字时按明确子命令解释；要浏览同名目录则用 `./export`。默认 `PATH=.`，默认 host 必须为 `127.0.0.1`。

- `--no-annotations`：静态导出可禁用标注但不影响正文；不是禁用执行开关，因为导出本来就永不执行。
- `--host 0.0.0.0`：首版明确拒绝或要求高级标志 `--unsafe-expose`，且即便允许也不自动开放执行 API。默认没有公网服务能力。
- `--no-open`：不启动浏览器，但仍能从终端得到完整 local URL。对于用户计算机上的桌面/无界面环境需要均可使用。
- 不引入 `view debug`、`view prove`、`view knowledge`、`view lint` 顶层嵌入，未来同类功能作为可选信息层及其各自命令独立实现。

### 10.2 View 自身的稳定错误种类（建议）

- `view.core_missing`：`MARQDO_BIN` 未设置或指定核心不存在；普通直接运行提供显式配置指引。
- `view.protocol_incompatible`：Tool/Analysis/Trace 产品范围或 major 不匹配。
- `view.analysis_failed`：Core check 失败，页面保留 Markdown 和 Core 诊断；**不应使页面 HTTP 渲染为虚假成功的语义态**。
- `view.source_mismatch`：Analysis source digest 与当前源码不一致；不展示对应语义标注。
- `view.source_span_invalid`：某条 byte span 越界/非码点边界；不将错误标注定位到相邻文字。
- `view.trace_stale`：Trace 与 Analysis digest/identity 不符；显示过期结果但不在当前文档标注。
- `view.run_denied`：用户未确认、模式/授权不允许运行；不能回退 project。
- `view.path_forbidden`：文件超出允许根或符号链接跳出。
- `view.asset_integrity`：导出资源摘要/打包资产与发布记录不一致。
- `view.export_failed`：写入目录不安全、写入失败或产物不完整；不能留下半成品声称成功。

统一退出码建议：`0` 成功、`1` 运行或 I/O 错误、`2` 参数/协议/权限类错误。这是 **View 自身契约**，不能覆盖 Core 返回的详细 `code`。JSON 诊断必须包含 `source:"core" | "view"` 防止混淆。

### 10.3 进程调用与退出清理

- 子进程使用绝对 `MARQDO_BIN`，参数数组，明确 cwd、环境控制策略；**禁止** `Command::new("sh")`/`exec("...")`/shell 插值。
- Core check/run 的 stdout、stderr、exit code 分别处理，绝不把 stderr JSON 当作成功 Analysis。
- 临时目录设置当前用户独占权限（Unix 0700），记录 PID、run ID、输出文件 hash；进程正常结束或错误/超时时清理。Windows 非首个支持平台，保留平台专属路径/权限模块接口，不为了兼容用 insecure 临时路径。
- 同一文档重复 Run 必须有显式并发策略：首版可选 `one active run per View session`，第二次运行前先停止/拒绝旧任务。UI 不可无限启动子进程；超时/预算与 CLI 能力一致，不编造硬性的“所有运行都在沙箱”说法。

---

## 11. HTTP、本地安全与静态导出

### 11.1 HTTP 路由：少而确定

Live 默认 `127.0.0.1:7429`；端口已占用时提示更换端口或允许显式 `--port 0` 随机绑定并输出真实端口。首版建议只有以下 API：

```text
GET  /                   站点或选中文档 HTML（默认静态态）
GET  /file?path=<safe>    已授权根内文件 HTML
GET  /assets/<hash>.css   唯一 CSS 资产
GET  /assets/<hash>.js    唯一 JS 资产
GET  /api/document?path=  当前 PresentationModel 只读 JSON，默认无历史机密输出
GET  /api/health          本地只读健康状态
POST /api/run             显式用户交互 + CSRF/session token + 校验的运行参数
POST /api/cancel          取消本 Session 的运行（若实现）
```

不允许开放通用 `/api/exec`, `/api/shell`, `/api/read?path=/`, `/api/proxy`。不支持以 GET 触发执行，不能靠 `onclick="..."` 或 query 参数解锁运行。

### 11.2 浏览器攻击面和必须实施的保护

1. **Loopback-only**：默认只绑定 `127.0.0.1`；不要公开 LAN。若支持非 loopback，必须明确增加独立、可审计的认证和 origin 机制；首版允许直接拒绝。
2. **CSRF/Origin**：写操作要求 `POST`, `Content-Type: application/json`、严格同源检查、随机会话 CSRF token；不允许通配符 CORS、没有 token 的运行、任意 Origin 通过表单触发。
3. **Host 校验**：避免 DNS rebinding 通过攻击者域名打到本地服务；只接受 `localhost/127.0.0.1` + 实际端口的受控 Host 组合，默认不接受域名解析到 loopback 的任意 Host。
4. **源文件路径**：URL decode 一次且严格拒绝畸形编码、NUL、`..`、根路径、盘符根、Windows UNC、链接逃逸；打开文件前 canonicalize 并检查真实路径仍位于允许的根目录内。禁止遍历 `.git/`、`.marqdo/` 缓存、密钥文件及二进制目录，除非明确作为特定文档依赖访问且通过另一受控 API。
5. **HTML/XSS**：原始 Markdown 作为不可信输入；默认禁用危险 HTML 和 inline 脚本；净化 URL scheme。页面使用严格 CSP，所有真实交互 JS 由本地打包、可验证摘要的 asset 提供。无外部 CDN、`eval`、动态脚本注入。
6. **本地文件与网络泄露**：图片链接、srcset、CSS `url(...)`、导入的 raw HTML 不得隐式读取任意本地文件、上传内容或向第三方发起请求；默认离线，远程资源作为显式用户 opt-in。
7. **运行安全**：Tool 是独立 OS 进程，**不自动享受** `.mq.md` 的 pure/project/strict 隔离；执行时仍应调用 Core 的公开模式/权限，但这不约束 View 自身。UI 必须正确提示风险。
8. **资源边界**：文件大小、总项目扫描数、最大 HTML/JSON、请求体、并发请求、执行超时、trace 事件数设置明确配置上限。极端大文件可报告 `view.resource_limit` 并提供源码直接下载，不可 OOM/crash。
9. **运行结果**：stdout/stderr 只作为文本处理并正确 HTML escape，不把 JSON 串当可执行 `<script>` 插入 HTML。Trace 没有敏感值不等于 stdout 没有秘密，输出保存默认只在当前会话及用户明确的本地路径。
10. **静态站**：打开导出 HTML 无网络连接也能阅读；禁用运行 API、存储型服务依赖。所有内部链接相对化；外部 URL 明确标识。

### 11.3 静态导出是纯只读操作

**`marqdo view export` 无条件遵循：不执行，不启动 foreign/plugin/provider，不触发 shell，不通过网络下载 UI 字体/组件，不修改源文件。**

输出形态（建议）：

```text
site/
  index.html
  pages/
    path/to/index.mq.md.html
    path/to/reference.mq.md.html
  assets/
    reader.<sha256-prefix>.css
    reader.<sha256-prefix>.js
    logo.<digest>.svg
    blossom.<digest>.webp
    feather.<digest>.webp
  manifest.json           # View自有导出清单：版本/输入摘要/协议（不含绝对秘密路径）
```

所有 HTML 由**相同 ReaderRenderer**产生，Live 与 Export 唯一差别是 URL/link 策略与是否展示运行可用按钮。导出内一切执行控件完全禁用或转为“请在本地 `marqdo view` 打开后运行”的文本，不允许有假按钮。`--out` 不得指向输入项目内覆盖源文件的危险路径；若输出位于项目根内，必须跳过输出目录，防止下轮扫描递归复制自身。采用 staging 目录原子发布，写入失败不污染上一个成功导出。

**强制对抗测试：**在 `.mq.md` 中放入可写临时标记的语句，运行 `view export`，确认文件未产生；在无网络环境导出再打开 `file://`，确认正文、表格、目录和本地资产有效。这用于避免旧 View 静态导出曾存在的执行路径歧义。

---

## 12. RunProjection：行为可视化但不越权

### 12.1 运行交互完整流程

1. 用户在 View 中点击“运行”→ UI 弹出本次文档、入口函数、核心版本、模式、预算、是否包含资源请求的风险说明；**未确认时不启动**。
2. 默认 `--mode pure`（最小权限）；项目若需要文件、网络、插件等权限，必须由用户显式选择允许的 CLI 功能与 grants，View 只组装参数，不自动继承项目 `capabilities` 请求。
3. 后端创建有时间/输出上限的任务，得到 Trace v2 及 CLI JSON。运行中状态显示 `RUNNING`；不可因 UI 关闭而泄漏永久子进程。
4. 退出后读取新生成的 Analysis（或对照已验证缓存）和 Trace。先验证 `analysis_digest`、`root_id`、事件 module span 再生成 RunProjection。
5. UI 将调用事件、实际分支真假、循环继续/退出、返回类型、host effect 摘要按实际 span 在原文侧边呈现；不可臆造 return value、具体参数或 Memory 快照。
6. 若用户点击其他文件/修改源文件，已显示结果进入 `STALE` 状态，必须退出当前文档的实时标注绑定。
7. 点击“清理”：清除该页面的客户端展示及服务端本会话临时 Trace 缓存，不能清空用户项目目录或核心 lock/cache。

### 12.2 公开事件到 UI 的保守映射

| 事件 | 可以显示 | 不可以显示 |
|---|---|---|
| `call.enter` | 某函数被进入，函数 ID、顺序、来源 | 未被 Trace 提供的实际参数值 |
| `call.exit` | 某函数退出及退出类型 | 无来源的最终变量表 |
| `branch.eval` | 条件真/假及对应源码 | 声称所有分支都覆盖 |
| `loop.iteration` | `continue`/`exit` 的实际事件及次数（以事件完整性为界） | 从静态循环结构推测本次总迭代 |
| `return` | 发生返回、返回值类型 | 推断/重构返回具体值 |
| `effect` | operation、耗时/结果摘要（以实际字段为准） | 断言权限安全、操作合法性已数学证明 |
| `trace.truncated` | 记录被截断、保守显示已有事件 | 将缺失事件等同于未执行 |

若需要用户能在 Document 中看到 `main` 返回的明确值，只有在**已发布的公开 CLI JSON**真实提供且权限/信息泄露边界清楚时才能显示为“Core 本次返回值”；这不构成逐语句结果协议，也不得回填到任意赋值行。

### 12.3 检查失败与降级

| 情况 | 文档内容 | 语义标注 | 执行按钮 | 说明 |
|---|---|---|---|---|
| Core 可用且 check PASS | 全部可读 | 有效 | 用户确认后可用 | `STATIC_OK` |
| Core 不可用 | 全部可读 | 无 | 禁用 | `CORE_UNAVAILABLE` |
| Core check error | 全部可读 | 不可信部分撤销，显示诊断 | 默认禁用 | `INVALID_SOURCE` |
| Trace 来源不匹配 | 全部可读 | 静态仍有效 | 可再次显式运行 | `STALE` 不能贴旧轨迹 |
| Trace 被截断 | 全部可读 | 静态仍有效 | 新运行可用 | 当前事件不完整 |
| 用户未授权 | 全部可读 | 静态有效 | 当前请求拒绝 | `RUN_DENIED` |
| 导出 HTML | 全部可读 | 只读静态有效（如check PASS） | 无 | `STATIC_EXPORT` |

---

## 13. 后续扩展：为 Knowledge / Prove / Lint 留接口，但不实现它们

### 13.1 为什么 View 不能成为扩展宿主

若 View 开始自己计算知识依赖、证明公式、修复样式、编译 Marqdo，就会重新变成旧仓库里的“大一统”框架。正确方向是：**外部工具独立生产有版本和来源的报告，View 只消费报告并映射到相同的 `module_id + span + source_digest`。**

可选提供者的未来产品边界：

| 生产者 | 自己的源代码/进程 | 给 View 的可选事实 | 首版 View 对其依赖 |
|---|---|---|---|
| `marqdo-knowledge` | 独立仓库 | 关系、引用、来源及知识制品 | 无 |
| `marqdo-prove` | 独立仓库 | 证明目标、proved/disproved/unknown/unsupported、可验证证书 | 无 |
| `marqdo-lint` | 独立仓库（未来） | 格式/可读性规则 ID、建议和可安全修复性 | 无 |
| 领域扩展库 | `.mq.md` package / Plugin / Provider | 独立函数/资源，由 Core Analysis 表示 | 只能通过公开 CLI |

### 13.2 最小的可扩展展示插槽（新仓库内部提案）

```text
DisplayOverlay {
  provider: "core-analysis" | "core-trace" | "future-namespaced-id",
  provider_version: "...",
  module_id: "...",
  source_digest: "...",
  span: {start_byte, end_byte},
  category: "function" | "run" | "knowledge" | "proof" | "style" | ...,
  title: "...",
  body_plain_text: "...",
  evidence_link: optional,
  trust_status: "core" | "externally-reported" | "unverified" ...
}
```

这是 **View 内部建议类型，不是 CLI 已定义的 JSON 协议**。只有待外部仓库的正式协议与安全边界确定后，才允许为其编写外部 adapter。未知 `provider`/`category` 时 UI 可以只展示安全纯文本/忽略；不能在 HTML 中直接插入第三方提供的任意脚本、CSS 或完整 HTML。

### 13.3 Prove 与 Knowledge 最重要的状态边界

`@knowledge` 是文档语义属性；`proved` 必须来自证明系统可核查的**结论/证书及精确源版本**，并具有无法证明、超时、假设不足、外部模型不可信等状态。View **绝不能**因 Knowledge 单元没有运行、代码通过 check、测试通过或 Trace 有 `ok` 就显示“已证明”。这是未来两个仓库之间的硬性分界。

### 13.4 Lint 与 UI 分离（此次不开发）

不把 `marqdo-lint` 作为 `marqdo-view` 的 crate、默认必需工具或本 RFC 的 PR；**本次只显示 Core diagnostics**。将来 Style diagnostics 需有版本化报告协议、规则 ID、严重度与精确 span，View 可选显示，不能阻止 `marqdo check` 已通过的源码被阅读，也不能默默自动改写。

---

## 14. UI 和代码级关键接口（必须可单测、可替换）

### 14.1 `PublicCoreClient`

```rust
trait PublicCoreClient {
    fn version(&self) -> Result<CoreVersion, CoreClientError>;
    fn check(&self, entry: &Path, opts: CheckOptions)
        -> Result<RawAnalysisV2, CoreClientError>;
    fn run(&self, entry: &Path, opts: ExplicitRunOptions)
        -> Result<RawRunResultAndTraceV2, CoreClientError>;
}
```

生产实现只使用真实 `MARQDO_BIN` 子进程；测试实现通过假核心可编程地给出错误、超时、不匹配 JSON。避免将 `Command` 创建散落在 HTTP、Exporter、Renderer 和 JS 中。`check` 在无 `MARQDO_BIN` 时返回明确错误，不用本地私有 Rust parser 替代。

### 14.2 `PresentationBuilder`

```rust
trait PresentationBuilder {
    fn build(&self,
        document: DocumentModel,
        analysis: Option<VerifiedAnalysis>,
        run: Option<VerifiedTrace>,
        overlays: &[VerifiedOverlay]
    ) -> PresentationModel;
}
```

Builder 只接收**已经验证**的 Analysis/Trace 类型，不接受任意未解析 JSON，以防把来源验证散落到渲染端。它可以按需要退化为纯文档模式，并在 View 状态中解释缺失原因。

### 14.3 `ReaderRenderer`

```rust
trait ReaderRenderer {
    fn render(&self, model: &PresentationModel, links: LinkPolicy)
        -> Result<RenderedPage, RenderError>;
}

enum LinkPolicy {
    LocalServer { base_path: String },
    StaticRelative { page_depth: usize },
}
```

**严格只有一个 `render()`**：与旧仓库的多套模板不同，Static/Live 仅差别为链接策略、访问模式和控件可用性。`RenderedPage` 同时包含安全 HTML 与使用的单一资产摘要；生产、测试与 Export 使用同一字节源。

### 14.4 `ProjectReader` 与 `SourceIndex`

```rust
trait ProjectReader {
    fn list_documents(&self, root: &Path) -> Result<Vec<SafeRelativePath>, ReadError>;
    fn read_document(&self, root: &Path, relative: &SafeRelativePath)
        -> Result<Vec<u8>, ReadError>;
}
```

读取根目录、文件扫描、静态导出和 HTTP 文件服务必须复用**同一个**安全路径函数；不能 Live 拒绝 `../` 但 Export 又允许。`SourceIndex` 直接以原始字节构建，使用 Core Digest 对比。`SafeRelativePath` 不接受系统绝对路径或已解码后包含父级目录、控制字节的输入。

### 14.5 `AssetBundle`

一个由 build script 或程序内常量定义的 `AssetBundle` 保存 `reader.css`、`reader.js` 和已批准视觉素材的相同字节及 SHA-256。Live Serve 与 Static Export 均从这份 bundle 获取。浏览器 Network 面板必须能确认无第二套 CSS、无外链字体、无未知 CDN。

---

## 15. 分阶段实施计划：按依赖顺序 PR，拒绝“大爆炸”迁移

> 每个 PR 均交付**代码 + 测试 + 文档 + CI + 真实证据**。尚未完成的功能保持明确 feature status，不编写伪成功脚本，也不因为 UI 不存在就偷偷加入 Core。

### PR-00：固定基线与旧 View 取证（P0）

**输入**：已发布 CLI Beta、旧仓库固定 SHA、本文截图。  
**工作**：建立独立空仓库和许可证；生成 `docs/audits/legacy-render-paths.md`；审计 `src/main.rs`、`view/{mod,html,reader.css,reader.js,output,debug_page}.rs/js/css`（实际文件按路径）；补上原图作为版本化参考；记录成功/失败的历史运行复核。  
**禁止**：复制旧 `src/view` 全目录作为起点、改核心、假设旧 `README` 代表真实效果。  
**验收**：有可读的数据流图、CSS 层叠报告、截图元素清单和 `LEGACY_RUNTIME_VERIFIED/UNVERIFIED` 标记；新仓库无旧 crate 依赖。

### PR-01：独立 Tool Protocol 安装闭环（P0）

**工作**：建立单 package、`tool.json`、`main/cli/core_client`；接入 `MARQDO_BIN`、版本协商、错误消息；打包 `bin/marqdo-view`，用冻结 CLI 安装和执行。  
**验收命令**：

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
# 将二进制拷到 dist 根内 bin/，tool.json 拷到 dist 根
"$MARQDO_BIN" tool install /absolute/path/to/dist/marqdo-view/tool.json
"$MARQDO_BIN" tool list
"$MARQDO_BIN" view --help
```

验收要求 `view --help` 来自独立程序而非 Core；功能错误传播正确；安装后 bundle 任意字节被篡改则 Tool Protocol 的启动校验拒绝；Core SHA-256 不变。

### PR-02：DocumentModel + Analysis v2 + 无执行浏览（P0）

**工作**：`ProjectReader`/`SourceIndex`、GFM Markdown 只读渲染、Analysis v2 schema 校验、标题/函数/知识区域映射、Core diagnostics、源码/文档切换；默认独立打开 index 或指定文件。  
**验收**：所有 §5 fixtures 经 Core `check`；知识里的 Emph/Code 不生成实际运行标注；H3 模式继承正确展示/谨慎降级；恶意 HTML 不执行；文档访问不产生运行子进程。

### PR-03：Reader 视觉重建与资产精简（P1）

**工作**：只选择旧 `reader.css`/assets 中有价值的视觉元素，重写单一 CSS；完成顶部品牌、选项卡、Welcome、樱花、羽毛、正文/函数标题、表格、Dock；所有按钮必须可用或隐藏。  
**验收**：在 1664×920 与 §18 所列多宽度截图比较；没有旧 Notebook 黑色皮肤覆盖，没有额外加载样式或 JS，正文语义不因样式改变。

### PR-04：注释与执行轨迹（P1）

**工作**：Trace v2 校验、来源 SHA/digest、运行确认/预算、显示真实 call/branch/loop/effect，侧注/曲线，失败及截断降级。  
**验收**：只有点击明确 Run 才产生 Core run 子进程；trace mismatch 变 STALE；无 Trace 时显示 Not Run；不出现赋值后的虚构实际值；返回/运行/认证错误通过真实结果传播。

### PR-05：Live API 安全与静态站导出（P0）

**工作**：本机服务安全、CSRF/Origin/Host、路径边界、SSE/轮询可暂缓，Exporter 共用 renderer，离线资产、相对链接、静态无执行。  
**验收**：无 JS、禁网、file://、本地 localhost、CSRF 负测、文件路径逃逸负测、恶意 Markdown XSS 负测，满足 §11。

### PR-06：冻结 CLI 端到端验收及 Beta 包（P0）

**工作**：在干净 Linux x86_64 的**CLI 发布包**上执行 Tool 安装、静态渲染、显式运行、静态导出全链路；建立固定截图 goldens、协议消费者测试、证据 Manifest；撰写 README/COMPATIBILITY/CHANGELOG/STATUS。  
**验收**：§18 所有强制门禁 PASS；完整安装包可复制至另一临时目录重新安装，不读核心源码或 `../marqdo` 工作树。

### PR-07（未来，明确非首版门禁）：外部信息层

待独立 Knowledge/Prove/Lint 协议正式发布后再开发；本阶段**禁止**作为上述 PR 的隐式依赖。新增 adapter 必须使用单独版本号、源 digest 校验与 `DisplayOverlay`，不让 View Core 依赖外部工具存在。

---
## 16. 测试数据集：开发者可直接建立的最小 fixtures

本节给出需要在新仓库**实际创建**的测试组。可使用 §5.3 示例文件作为起点，但每个 golden 必须从冻结发布版 CLI `check --emit` 或 `run --trace-file` 真实生成，不允许自己手填语义结论。建议把所有临时运行工件放在 `tests/.cache/`（gitignore），把稳定、人工审阅的协议 golden 放在 `tests/golden/`。

| 组名 | fixture 核心内容 | 必须断言 |
|---|---|---|
| `knowledge-root` | 无任何 `@exec` 的普通 Markdown | 可读，不生成可调用函数和执行标注 |
| `heading-explicit` | H1 Knowledge/H2 Exec、H2 Knowledge | Unit/Function 关联来自 Core |
| `heading-inherit` | H1 Exec→H2 Exec（继承）→H3 Knowledge→H3 Exec | 祖先继承/语义回切；无 H3 独立函数 |
| `mode-invalid` | `@Exec`、空标题、冲突模式标记 | Core 诊断可见、标注不伪造 |
| `exec-carriers` | `**...**`、`*...*`、行内 `` `...` `` 写相同指令 | 语句类型取 Analysis，不取样式节点 |
| `knowledge-prose` | Knowledge 段落含斜体、粗体、行内代码 | 仅正常 Markdown，不执行 |
| `control-flow` | `if/else/end`, `while/end`, 复合表达式 | 静态边界及真实 Trace events 按来源显示 |
| `inert-content` | 链接、GFM 表格、fenced code、math | 这些不能被当作 Exec 指令 |
| `unicode-spans` | 中文、emoji、组合字符、全半角 | 精确 byte span，不错位、不裂字符 |
| `line-endings` | 同等文字的 LF/CRLF 多版本 | Source sha 与 Core 真实输出一致，不偷换字节 |
| `duplicate-text` | 多处完全相同的 `return`、相同表格单元 | 注释绑定到实际 span，不按文本搜索误定位 |
| `syntax-error` | Exec 区错误载荷 | 页面仍可阅读、诊断可见、禁运行 |
| `link-error` | 存在失效 Markdown link | `--refs` 特定报错，不误判核心语法失效 |
| `multi-module` | 主文件导入 pinned Git 或合法 path dep | 项目身份正确；不可访问模块显示限制而不猜路径 |
| `trace-missing` | 静态合法但未 Run | Not Run；不出现任何运行结果 |
| `trace-stale` | 运行后修改一个源码字节 | 展示 Stale，不将旧事件贴到新源位置 |
| `trace-truncated` | 设置很小的 event budget | 只展示已有事件并明确截断 |
| `stdout-untrusted` | 程序输出 `<script>...` 等 | 文本安全展示，无脚本执行 |
| `export-no-exec` | 存在写文件/进程副作用的程序 | 导出过程**不创建副作用标记**；不运行该程序 |
| `large-doc` | 大表、长代码块、海量标题及注释 | 在限定资源内完成或明确报错；无浏览器冻结 |
| `malicious-path` | `../secret`, `%2e%2e`, symlink、绝对路径 | 所有入口拒绝跨根访问 |
| `malicious-html` | inline HTML, SVG/onload, `javascript:` link | 无 XSS、无外联泄露 |

### 16.1 不能只靠静态 JSON golden 的测试

- 必须使用**真实 CLI release binary**检验独立安装、Core 版本协商、`check/run` 的 stdout/stderr/exit code 和 Trace 摘要。
- 必须使用**真实浏览器**检验 Screenshot 几何、Dock、键盘、打印、移动端、无网络与动态侧注；DOM 文本存在不等于布局正确。
- 必须用**可被观测的操作系统副作用**检验“打开/导出不执行”，而不是只 mock `PublicCoreClient`。例如在只允许读取的临时项目内包含一条会产生 marker 的合法 Core 程序，刷新页面/导出后断言 marker 不存在；另用假 Core 记录被启动的命令，断言没有 `run/call`。
- 必须对真实恶意 Markdown 实施浏览器测试：含可执行 HTML、inline SVG、恶意链接、图片外联的文件，页面不能执行、不能私自发网络请求。

---

## 17. CI、构建发布与独立仓库的证据链

### 17.1 新仓库 CI 最低矩阵

首个 Beta 支持平台建议与 CLI Beta 相同：**Linux x86_64 glibc**。其他平台只做 best-effort 编译/浏览器测试，未实际执行前应标 `UNVERIFIED`；不因 Linux PASS 就声明 macOS/Windows 支持。

```text
job: code
  cargo fmt --all -- --check
  cargo clippy --all-targets -- -D warnings
  cargo test --locked
  cargo build --release --locked

job: contract-linux
  下载/使用已批准的官方 marqdo-3.0.0-beta.1 Linux 归档
  校验发布归档 SHA-256 与 SOURCE_SHA / RELEASE_INFO
  解包到只读位置；确保 marqdo 和 provider-worker 相邻
  构建 View bundle；使用冻结二进制 tool install
  运行所有真实 Core/Analysis/Trace/View 契约测试
  比对 marqdo SHA256（开始、结束必须完全相同）

job: browser-linux
  使用固定版本 Chromium/Playwright（测试环境允许下载依赖）
  运行 DOM/视觉/键盘/打印/无网/XSS/响应式回归
  保存失败 screenshot/DOM/console/network HAR artifacts

job: export-linux
  使用 mock 核心 + 真实核心双路径（不能只有其中一个）
  离线 HTML 内容及资源完整性、无执行、无外网依赖

job: release-bundle
  用干净目录制作 bundle、验证文件摘要
  在新临时 Tool home install，再运行 marqdo view --help
  打包 .tar.gz、SHA256、SOURCE_SHA、VERSION/COMPATIBILITY
```

`tool install` 保存的是受控 bundle 副本，CI 必须在**真实安装后的路径**运行，而不是直接跑源码目录中刚编译的 `target/release/marqdo-view` 就宣称集成成功。

### 17.2 防耦合代码扫描门禁

在新仓库中加入测试扫描 `Cargo.toml`、`Cargo.lock`、`src/` 和工作流：

```text
DENY external path/git crate dependency on:
  marqdo, marqdo-cli, marqdo-ir, marqdo-runtime,
  marqdo-frontend, marqdo-host, marqdo-markdown
DENY source imports / module references of:
  marqdo::view, marqdo::sema, marqdo_ir, marqdo_runtime,
  marqdo_frontend, inspect --ir (as required protocol)
DENY source tree accesses into ../marqdo or ../marqdo-cli
DENY generated compatibility parser for old pre-RFC Marqdo syntax
DENY a second independent HTML/Markdown/semantic rendering pipeline
```

注意：不是禁止文档中出现上述名字或源代码**引用链接**；扫描只针对实际编译依赖、读取和运行路径。允许使用独立、官方 crates.io 上的通用 Markdown/HTML/JSON/HTTP 库。运行协议一律以发布二进制/JSON 合同交互。

### 17.3 发布制品必须包含

```text
marqdo-view-0.1.0-x86_64-unknown-linux-gnu.tar.gz
  tool.json
  bin/marqdo-view
  assets/（如果未内嵌）
  README.md
  COMPATIBILITY.md
  LICENSE*
  SOURCE_SHA
  RELEASE_INFO.json（tool版本、CLI版本范围、协议、target、文件hash）
```

不要把截图基准、旧仓库源码、测试缓存和未用第三方 JS vendor 全部塞进**用户运行包**。测试基准属于 Git 仓库的 `docs/design`；用户运行包只发运行所必需资产与许可文件。`tool.json` 不能指向 `/root/work/...` 等开发机器绝对路径。

### 17.4 SHA 绑定证据

每次 release candidate 在 `docs/evidence/<CLI-SHA>/manifest.json` 记录：

```json
{
  "schema": "marqdo.view-conformance",
  "schema_version": 1,
  "view_source_sha": "<40-hex-git-sha>",
  "view_binary_sha256": "<64-hex>",
  "cli_source_sha": "<40-hex-git-sha>",
  "cli_binary_sha256_before": "<64-hex>",
  "cli_binary_sha256_after": "<identical-64-hex>",
  "tool_protocol": 1,
  "analysis_protocol": 2,
  "trace_protocol": 2,
  "target": "x86_64-unknown-linux-gnu",
  "tests": { "contract": "PASS", "browser": "PASS", "export": "PASS", "security": "PASS" }
}
```

这是**拟定的 View 本地证据格式**，不冒称 CLI 的发行证据 schema。生成器必须从实际命令退出状态/测试产物生成；不得手工声称 PASS。每条结果需要可复现脚本、runner 环境和 CI URL。若主仓 CLI 以后发布新 Beta/RC，复跑相同的冻结二进制契约并把兼容性状态写入 `COMPATIBILITY.md`。

---

## 18. 验收门禁表：开发者与审阅者可以直接照表 PASS/FAIL

下表中所有 `P0` 项必须 PASS 才能宣布首个 `marqdo-view` Beta；`P1` 属于本文计划的首个完整 Reader 版本，未完成时必须在 CHANGELOG/README 明确标注。`EXEMPT` 必须写明理由，不允许以“待开发”替代 PASS。

| ID | 优先级 | 验收动作 | PASS 标准 / 否则 FAIL |
|---|---|---|---|
| A-01 | P0 | 查看源码依赖图与 Cargo metadata | 无旧 `marqdo`、CLI 私有 crate、旧 View 路径依赖 |
| A-02 | P0 | 从独立新 Git 仓库 clean build | 不需要 clone/编译旧仓库或 CLI 源码 |
| A-03 | P0 | Tool 协议安装/分发 | `marqdo tool install` 后 `marqdo view --help` 正确，核心不变 |
| A-04 | P0 | 构建前后比较 CLI SHA-256 | 严格字节相同，所有 Core 运行来自相同冻结二进制 |
| A-05 | P0 | 查看旧审计报告与截图基准 | 有路径、CSS 加载/覆盖、可复核来源；无无证据推断 |
| A-06 | P0 | 查看 `ReaderRenderer` 与 CSS 入口 | Live/Export 共享唯一模板和主题，不靠旧覆盖链 |
| S-01 | P0 | Knowledge/Exec/继承 fixture | 有效语义来自 Core；H2 function/H3 section 分清 |
| S-02 | P0 | Emph/Strong/Code 与关键词 fixture | 不把 `*` 当 return，列表不当分支/循环 |
| S-03 | P0 | `@knowledge` 子树/代码围栏/GFM 表 | 无伪执行语句、无自动求值 |
| S-04 | P0 | 中文/emoji/CRLF/相同语句多次 | 源 span 映射严格正确，不错位不猜文本 |
| S-05 | P0 | 故意让 `check` 失败 | 正文可读、诊断可见、旧 Analysis 丢弃、语义结果明确不可用 |
| S-06 | P0 | 替换 Analysis schema_version | 不兼容时拒绝、无 `inspect --ir` 回退 |
| S-07 | P0 | 替换 `source_digest`、篡改 span | 不展示错误位置的注释；错误明确 |
| R-01 | P0 | 打开项目、刷新页面 10 次 | 无 Core run/call、无进程/写文件副作用 |
| R-02 | P0 | `view export`，包括非法/有副作用源 | 仅静态文件被写出，源文档不执行 |
| R-03 | P0 | 用户确认后触发 Run | 真正从冻结 Core 获得结果，参数/模式如实传递 |
| R-04 | P0 | trace SHA/root_id 与当前 Analysis 不匹配 | STALE，不叠到当前正文；不显示旧成功作为当前结果 |
| R-05 | P0 | 制造 Trace 截断/运行错误 | 状态明确，不能呈现为全部步骤成功 |
| R-06 | P0 | 查看 Trace 中未公开的值 | 不猜参数/逐赋值值/完整 Memory/证明状态 |
| X-01 | P0 | 含恶意 inline HTML/SVG/event handler | 无脚本执行、无数据泄露、无外部请求 |
| X-02 | P0 | 目录穿越、symlink escape、URL 双重编码 | 服务与静态导出拒绝越界 |
| X-03 | P0 | 从外站发起 POST Run | CSRF/Origin/Host 检查拒绝，无 Core 执行 |
| X-04 | P0 | 无 JS、禁网、file:// 打开导出 | Markdown/表格/标题/链接/资源仍可阅读 |
| X-05 | P0 | 输出含 `<script>`, secret-looking 值 | 文本转义、浏览器不执行、默认不公开持久保留 |
| X-06 | P0 | 端口/错误/子进程超时/失联 | 无无限重试与子进程泄漏，错误明确 |
| U-01 | P1 | 1664×920 视觉截图 | 页面主要空间布局、颜色、装饰、Dock 与参考风格一致 |
| U-02 | P1 | 390/600/900/1280/1664/1920/2560 宽度 | 无横向溢出、Dock 不挡阅读、侧注可降级 |
| U-03 | P1 | 键盘导航/焦点/屏幕阅读器 | 所有真正可用按钮有名称、Tab/Enter/Esc 正常 |
| U-04 | P1 | 动态侧注布局 | 不与原文重叠；拥挤时禁长线并退化为内联注释 |
| U-05 | P1 | 页面 Welcome/普通文档 | Welcome 由显式配置选择，普通文档不意外出现 Hero |
| U-06 | P1 | 同一文件 Live 与 Export 内容对照 | 语义/正文一致，只有链接与允许动作不同 |
| P-01 | P0 | 文档 README/本 RFC/STATUS/兼容性 | 后来者不用聊天、旧仓库私有构建和外部私有服务就能复现 |
| P-02 | P0 | CI archive clean install | 可从发行包移动安装，用户无需 Rust toolchain 运行 |
| P-03 | P0 | 浏览源码审计 `spec/*` + 协议 golden | 任何不支持的能力明确标注；不冒称实时调试和 Prove |

### 18.1 一键验收流程（在新 `marqdo-view` 根目录执行，按实际脚本落地）

```bash
# 0. 放入/解包已经校验过 SHA256 的官方 CLI Beta 发行包
export MARQDO_BIN="/absolute/path/to/marqdo"
"$MARQDO_BIN" --version
sha256sum "$MARQDO_BIN" > /tmp/marqdo-before.sha256

# 1. 检查代码质量、构建与单元测试
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked

# 2. 用项目自己的发布脚本制作受控 Tool bundle（该脚本属于 PR-01）
./tools/package.sh
"$MARQDO_BIN" tool install "$(pwd)/dist/marqdo-view/tool.json"
"$MARQDO_BIN" tool list
"$MARQDO_BIN" view --help

# 3. 真实端到端：独立工具调用冻结 CLI
"$MARQDO_BIN" view verify examples/mixed-modes/index.mq.md
"$MARQDO_BIN" view export examples/mixed-modes --out /tmp/marqdo-view-site
"$MARQDO_BIN" view examples/welcome --no-open

# 4. 浏览器/安全/冻结内核套件，实际脚本由 PR-06 创建
./tools/test-contract.sh
./tools/test-browser.sh
./tools/test-security.sh

# 5. 确保整个过程没有重新编译或更换 CLI
sha256sum -c /tmp/marqdo-before.sha256
```

上述 `tools/*.sh` 是新仓库 **必须编写** 的脚本文件名建议，不代表 CLI 或旧仓库已经拥有这些脚本。不能仅靠文档中的命令成功回显替代实际 CI 运行。`marqdo view ...` 命令全部需要先在新仓库安装有效的 Tool bundle。

### 18.2 浏览器截图验收尺度

测试至少在 Chromium 的 1664×920、1280×800、900×768、600×800、390×844、1920×1080、2560×1440 下运行。设计对齐同时看：

- **客观几何**：顶栏、导航、Hero、正文起点、导入标签、函数金色侧标、Dock 坐标的合理相对位置；宽窄屏均不遮挡交互。
- **视觉气质**：暖纸色、衬线字、金色强调、樱花、轻量边界，禁止旧 Notebook 黑色主题残留。
- **阅读任务**：普通文章段落在首屏优先，Welcome 例外；脚注/导入标签不压过作者文字。
- **行为**：非 Run 操作绝不执行；高亮来源正确、没有无意义黑色线条/浮动断线；连续滚动顺畅。

**不要只做像素完全一致**：字体栈/不同系统抗锯齿会带来正常差异。用截图差分定位变动，再依据文本保真、布局阈值、重叠检测和设计基准人工复核。无权擅自丢掉失败截图或调高允许差异掩盖严重错位。

---

## 19. 项目维护与变更控制规范

### 19.1 文档与代码必须同步

每个 PR 强制更新：`CHANGELOG.md`、受影响的 `docs/ADR/*`、`docs/STATUS.md`、`COMPATIBILITY.md`（如协议或平台改变）、对应 fixture/golden、CI 证据。禁止留多个“最终架构.md”“新版2.md”“最新版.md”；本 RFC 是架构总规范，ADR 只记录局部选择，STATUS 记录实际实施状态。

### 19.2 源码组织审查

- 新功能必须先回答：它是 Reader 的**视觉**、**数据投影**、**运行时执行**，还是**独立领域工具**？不属于前三者的禁止进入 View。
- 禁止在 HTTP 路由里嵌入 HTML 生成，在 Markdown parser 中注入 Trace，在 `reader.js` 里重新解析 `@exec`，在 CSS 文件里按路径写业务功能差异。
- 超过一个 UI 渲染入口或第二套 skin 时，必须拒绝 PR 并重构为统一组件；不能靠后加载 CSS 或隐藏多余按钮“看似修复”。
- 除真正的可选测试/性能特性外，不引入默认禁用但仍占据源码的大型子系统。
- 依赖更新必须审视许可证、体积、跨平台、执行权限、网络依赖、Bundle 完整性。

### 19.3 当核心协议确实缺能力时

可向 `marqdo-cli` 提出**独立的、可选的、版本化公开协议 RFC**，例如未来有实际需求的 `Debug Observation v1`，但不得作为 View 首版的前提，亦不得让 View 编译/链接 CLI 源码。流程是：

1. 在 View 的 Issue 中记录精确用户场景、需要的字段、现有 Analysis/Trace 无法表达的理由。
2. 说明源位置/权限/隐私/性能的安全约束；提供不改核心的降级 UI。
3. 等 CLI 维护者正式发布独立协议版本、schema、真实 golden 和公开二进制。
4. View 新建协议 adapter，并通过版本协商启用；旧版本自动保留基础阅读能力。

这样即便两个项目有**公开协议层面的必要耦合**，也不会出现私有源码耦合、内存布局耦合或同步发布要求。所谓“完全解耦”应以**独立构建、独立部署、独立发布和稳定公开协议**定义，绝不是互相没有任何规范联系——后者会使语义正确性无法验证。

### 19.4 版本与发布边界

- View 初始独立版本 `0.1.0`；与 CLI `3.0.0-beta.1`、Tool Protocol v1、Analysis v2、Trace v2 **不共用一个版本数字**。
- Beta 阶段允许 View 接口逐步完善，但破坏面必须记录；已公开的 Tool Protocol v1、Analysis v2、Trace v2 的现有字段不能在 View 端重新解释。
- Linux x86_64 为正式首发目标；macOS/Windows 先做 best-effort，不承诺从旧仓库复制的资源/线程模型能跨平台。
- 仅依赖最小权限 Core 的纯阅读功能，应该能在没有 Provider helper、外语环境和任何额外 Marqdo 拓展库的机器上正常使用。

---

## 20. 给独立编码开发者的不可歧义指令（可复制 Issue/任务）

> 创建新的 `marqdo-view` 独立 Git 仓库，完整实现 `docs/RFC-VIEW-0001.md`。以 `cflmy/marqdo-cli` 的正式 `v3.0.0-beta.1` 发布二进制、`spec/mapping.md`、Analysis v2、Trace v2、Tool Protocol v1 为唯一语义与通讯权威；以 `cflmy/marqdo@1c55baa` 及随包 `reference-old-view-reader.png` 为旧视觉审计参考。不能依赖这两个仓库的私有 Rust crate、内部函数或运行时源码，不能修改或重新编译 CLI，不能移植旧 2.x 的 AST/解释器/Notebook/Debug CSS。首先审计旧 `src/view/html.rs` 的 `stylesheet/notebook_ui/reader.css` 层叠及多条 View/Document/Debug 路径，保留审计报告；然后按 PR-00→06 建立单 Rust package、Tool v1 安装、Core 公共 JSON 消费、单一 DocumentModel/PresentationModel/ReaderRenderer、暖纸/樱花/衬线旧视觉、默认无执行的安全 Live/Export、真实 Trace 可视化以及安全/浏览器/冻结 CLI 测试。`@exec/@knowledge` 的继承和 H2 函数、等价 Markdown 单语句载体均以 Core `check` 结果为准；仅用源码 byte span 绑定视觉锚点。严禁虚构赋值实际值、参数值、内存快照和证明状态。所有 PR 带测试、文档、真实证据，未完成项明确标 `UNSUPPORTED`。最终必须在完全独立的源码目录，通过已经 hash 固定、未经修改的 CLI 发布二进制完成 `marqdo tool install` 和 `marqdo view` 浏览/导出/用户显式运行的全部 P0 门禁。**不开始开发 Lint/Knowledge/Prove 或修改核心。**

## 21. 来源索引（无需任何对话背景）

### 新内核（唯一语义真相）

- [marqdo-cli 仓库](https://github.com/cflmy/marqdo-cli)
- [正式 Beta 发布记录](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/docs/releases/3.0.0-beta.1.md)
- [Markdown 执行语义](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/spec/mapping.md)
- [Analysis v2](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/spec/analysis-v2.md)
- [Trace v2](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/spec/trace-v2.md)
- [Tool v1](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/spec/tool-v1.md)
- [第三方 Tool 开发手册](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/docs/developers/tools.md)
- [Analysis/Trace 消费手册](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/docs/developers/analysis-trace.md)
- [版本支持与限制](https://github.com/cflmy/marqdo-cli/blob/eeea01d672c07b6be7d6444031d1619309dc88ab/docs/developers/compatibility.md)

### 旧 View（只用于视觉和历史取证，不能作为执行协议）

- [旧 marqdo 仓库](https://github.com/cflmy/marqdo)
- [旧 View 实现目录](https://github.com/cflmy/marqdo/tree/1c55baabd570e18b9da8f9057cef2f848e176690/src/view)
- [旧 HTML 模板及多层 CSS](https://github.com/cflmy/marqdo/blob/1c55baabd570e18b9da8f9057cef2f848e176690/src/view/html.rs)
- [旧 Reader 暖纸样式](https://github.com/cflmy/marqdo/blob/1c55baabd570e18b9da8f9057cef2f848e176690/src/view/reader.css)
- [旧 Reader JS 与 Welcome Hero](https://github.com/cflmy/marqdo/blob/1c55baabd570e18b9da8f9057cef2f848e176690/src/view/reader.js)
- [旧 View Live/Debug 路由](https://github.com/cflmy/marqdo/blob/1c55baabd570e18b9da8f9057cef2f848e176690/src/view/mod.rs)
- [旧 View Static 导出](https://github.com/cflmy/marqdo/blob/1c55baabd570e18b9da8f9057cef2f848e176690/src/view/output.rs)
- [旧 View 页面可读性重设计](https://github.com/cflmy/marqdo/blob/1c55baabd570e18b9da8f9057cef2f848e176690/doc/design/view-knowledge-reading-experience-2026-10-24.md)
- [旧 Reader botanical 主题设计](https://github.com/cflmy/marqdo/blob/1c55baabd570e18b9da8f9057cef2f848e176690/doc/design/view-reader-botanical.md)

**最终设计原则：让文档始终是文档，让内核始终是内核，让 View 成为可以独立替换的阅读方式。**
