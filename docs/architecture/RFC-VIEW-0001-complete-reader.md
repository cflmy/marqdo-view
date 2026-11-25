# RFC-VIEW-0001（定稿建议）：完整 Markdown 阅读、统一函数调用与显式结果静态导出


> **契约提示**：本文描述完整阅读与结果导出的产品/交互设计，设计仍参考有效；
> 但其中任何协议字段、命令参数或事件名称以
> [`observation-v1-and-analysis-v3.md`](observation-v1-and-analysis-v3.md) 与
> [ADR-0012](../decisions/0012-rc4-current-contracts.md) 的当前契约为准。
> 0.2.0 时代的协议细节是历史记录，不是当前能力。

> **状态：Proposed / 开发规范，不表示已经实现或通过验收。**
>
> **目标仓库：** [`cflmy/marqdo-view`](https://github.com/cflmy/marqdo-view)，本次审阅基线 `master@dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5`（`0.2.0`）。
>
> **核心仓库：** [`cflmy/marqdo-cli`](https://github.com/cflmy/marqdo-cli)，审阅基线 `master@652ea763603517ba4d13325a405e3124844ded31`（已包含 3.0.0-rc.2 候选控制流、Analysis v3 与 Execution Events v1）。这些是**源代码审阅事实**，不是新特性已在发行二进制上验证的声明。
>
> **适用对象：** 不熟悉本项目历史的 View/CLI/前端开发者、审查者与测试维护者。
>
> **文档优先级：** 这是一份自足的完整实施 RFC，**整体替代**此前同名 RFC-VIEW-0003 的“历史结果持久化版”和“绝不在导出中包含结果版”；已实现的 RFC-VIEW-0002 能力仍是可复用基线。无须阅读任何聊天或前置 RFC 才能实施。
>
> **一句话产品定义：** **View 是一篇可调用的 Markdown 阅读器；执行结果只在当前交互中暂存，用户主动导出时，才被固化为一份独立、只读、可离线阅读的文档快照。**

---

## 0. 决议摘要：不建结果数据库，只允许显式静态导出

本规范区分三类对象。它们不可互换。

| 对象 | 权威来源 | 生命周期 | 允许操作 |
|---|---|---|---|
| `.mq.md` 源文档、依赖、知识 | 用户项目和版本控制 | 与 View 无关 | 阅读、静态分析、明确调用；**永不被导出结果改写** |
| 当前执行观察：参数、stdout/stderr、返回值、事件 | 真实 CLI `run/call` | View 进程内的有限任务生命周期 | 当前页面显示、取消、短时断线重连、在过期前选择导出 |
| **静态结果快照**：文档、已选定的一次结果、媒体、来源信息 | 用户明确点击“导出（含本次结果）”生成的文件 | 由用户下载/分发后独立于 View | 离线阅读、分享、审查；**不会恢复为可执行会话** |

### 0.1 必须满足的不变量

1. **打开、刷新、跳转、搜索、切换文档、普通静态导出都绝不执行程序。** 显式 `POST /api/runs` 是唯一启动 CLI `run/call` 的入口。
2. **导出结果只复制已完成的一次真实执行观察**，不能因导出而重跑、补跑、自动重试、自动申请权限或执行文档中的代码。
3. **默认普通导出**：`marqdo view export PROJECT --out SITE` 保持只读，内容为完整 Markdown 与安全本地媒体；没有结果数据。
4. **含结果导出**：从已运行的 Live 页面，由用户选定**一条仍在临时内存中**且终态明确的运行记录，确认内容与敏感性，然后导出只读快照。
5. **不建立 RunStore、SQLite、结果目录、浏览器 IndexedDB/localStorage 运行历史、持久 Kernel、跨重启恢复、自动导出。** 导出的文件由用户拥有；View 不维护文件索引与生命周期。
6. **静态 HTML 不得具有运行能力。** 无任何可生效的调用 API；无 `POST /api/runs`、无服务端依赖、无自动联网、无执行按钮假象。即使 HTML 中保存了结果，它也不是可恢复的会话。
7. **不得将运行输出伪装为原始 Markdown。** 可见标签须注明“本次执行快照”、运行目标、终态及来源摘要；只有通过验证的事件才能附着到源码位置。
8. **不为 View 修改语言内核或新增 `save_result` HostOp。** 所有真实计算仍使用公开 CLI、Analysis、Trace、Execution Events 和 Tool 协议。
9. **View 的“无保存”应精确定义为无自动、隐式、持久的运行存储；允许用户显式创建静态文档制品。** 这两者不矛盾。

### 0.2 用户路径（产品级完成标准）

1. 用户打开 `marqdo view examples/report/index.mq.md`，看到排版完整的文档、图片、公式、链接；不会自动运行。
2. 用户在 `## @exec ...` 函数标题旁点击“调用”，输入参数、查看权限和预算，确认一次 `marqdo call`。
3. View 显示实时 stdout、分支/循环事件、最终返回值，结果位于相关函数附近，但与源码视觉区分。
4. 用户点击结果旁的 **“导出文档（含本次结果）”**；预览会包含的结果、参数/隐私选项、失败或截断标记。
5. 用户确认后下载一份静态网站包（ZIP；内部 `index.html` + 媒体/字体/样式 + manifest）。解压后可用 `file://` 离线浏览；查看结果无需 CLI、View Server 或网络。
6. 页面再次刷新或 View 服务重启，不保证还存在这次运行；**已经导出的 ZIP 不受影响**。用户没有选择导出，就不会留下 View 管理的持久记录。

---

## 1. 完整职责边界：什么是 Markdown、执行观察和导出制品

### 1.1 Markdown 仍是页面主体

Marqdo 使用固定的 `marqdo.md.v1` Markdown AST；`@exec` 中的普通无标记叙述不执行，显式 `Strong` / `Emph` / inline Code 载体进入语句映射。`@knowledge` 是惰性知识区域。视图不能从视觉标记、Markdown 列表符号推断控制流；语义只依赖公开 Analysis。

UI 不采用 Jupyter 的 Cell/Kernel 模型。用户阅读的是一篇连续的、可执行知识文档。函数周围只有克制的调用入口和**可收起的实际结果**；列表、表格、图片、公式优先于状态卡片。

### 1.2 执行与结果的所有权

- **CLI**：检查、解析、调用、运行、权限、安全预算、计算真值。
- **View ExecutionController**：检查调用请求、显式确认、单活动任务、有界 stdout/事件与终态、取消、短时 SSE 重连；不是持久存储系统。
- **View Reader**：渲染 Markdown 与来源已校验的本次计算观察；不解释 Marqdo 表达式。
- **StaticExporter**：从读取模型及用户明确选中的**已验证终态观察**构建不可执行的离线制品；只做转换/打包，不调用 CLI。
- **用户项目**：如果需要 CSV、JSON、PNG 或其他正式业务数据，仍应由受授权的 Marqdo 程序或其他通用工具明确写出。View 的网页快照不等于领域数据存储。

### 1.3 不做的功能

不做历史列表、保存每次运行、最近实验恢复、Notebook Run All、自动重跑、后台刷新计算、不可见状态会话、结果数据库、实验对比平台、运行数据云同步、自动把运行输出写回 `.mq.md`、对 Math/Agent/Quantum 添加特殊处理。

用户在同一 View 页面上可再次调用函数；当前结果更新即可。为了控制复杂度，**第一阶段一份快照只包含用户明确选定的一次运行**；多次调用并排比较和合并多份结果属于后续独立需求。

---

## 2. 审阅基线与当前真实缺口（固定 SHA）

### 2.1 View 侧

| 分类 | 文件/实现 | 已确认事实 | 实施要求 |
|---|---|---|---|
| EXISTS | `src/reader/markdown.rs` | `comrak` 构造文档树，含图片、链接、表格、列表、数学、UTF-8 source span | 复用既有解析器，不复制第二套 Markdown→语义管线 |
| DEFECT | `src/reader/render.rs::Inline::Image` | 任何图片都显示“阻止自动加载”占位；合法本地图也不可见 | 增加同源安全本地图片解析、呈现与静态打包 |
| DEFECT | `src/reader/render.rs::resolve_link` | `.mq.md#fragment` 重写时可能丢 fragment；标题 ID 按字节位置生成，与常见 Markdown `#slug` 不一致 | 标准标题锚点、重复标题、跨文件锚点均需可导航 |
| DEFECT | `src/reader/render.rs::MathBlock / Inline::Math` | 公式主要显示 LaTeX 原文，不是真正数学排版 | 实现离线、安全数学渲染 |
| EXISTS | `src/reader/render.rs::render_functions` | 已有函数列表、调用按钮和签名信息 | 改善函数标题附近的调用与结果关联，不重写调用机制 |
| EXISTS | `src/execution/request.rs` | `RunRequest`、entry/function、参数校验、Analysis digest 与权限 | 继续保留唯一运行入口和确认流程 |
| EXISTS | `src/execution/controller.rs` | 单活动任务、去重、取消、内存任务、默认终态 TTL=15 分钟、上限 32 任务 | 在**现有 Job 终态内**保留导出需要的有界可信投影，不创建第二套历史库 |
| EXISTS | `src/execution/events.rs` | View 自有 SSE 生命周期事件协议 | 新 CLI 实时事件通过一个 EventBridge 映射；不创建第二个 Controller |
| DEFECT | `src/protocol/analysis_v2.rs` / `tool.json` | View 仅接受 Analysis 2；新 Core 对部分控制流发 Analysis 3 | 支持 Analysis v2/v3，基于 schema_version 协商 |
| DEFECT | `src/core_client.rs` | 现有执行主要为 `--json` + Trace，运行末尾读取结果 | 消费新 `--event-stream jsonl`，不能与 `--json` 同时使用 |
| EXISTS | `src/export.rs` | 静态导出包含 HTML 和 View 内置资源、manifest；还未集成用户 Markdown 图片 | 扩展已有 StaticExporter；允许**可选** `ExportResultOverlay`，不得重写导出器 |
| EXISTS | `src/server/routes.rs` | 有 7 条固定路由，执行入口是 `POST /api/runs` | 增加一个受保护的结果**导出** POST，而非运行/保存/历史路由 |

固定来源：[`View 源码`](https://github.com/cflmy/marqdo-view/tree/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5)。

### 2.2 Core 侧公开协议（不可通过猜测添加字段）

- [`spec/markdown-profile.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/markdown-profile.md)：`marqdo.md.v1`，CommonMark/GFM/数学/front matter 扩展的闭合选项。
- [`spec/analysis-v2.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/analysis-v2.md)、[`v3`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/analysis-v3.md)：多臂 `else if` 两版支持；含 `break/continue` 的内容可能选择 v3。View 不能假设永远是 v2。
- [`spec/trace-v2.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/trace-v2.md)：Trace 绑定精确 Analysis JSON 字节 SHA-256，带 source span 的真实事件，可截断、不可伪造。
- [`spec/execution-events-v1.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/execution-events-v1.md)：`run/call --event-stream jsonl`，实时 stdout/stderr chunk、结构事件、可选选择性值观察与终态；`--event-stream` **与 `--json` 互斥**；Trace v2 可并用。
- [`spec/tool-v1.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/tool-v1.md)：独立 Tool 的调用/安装/协商，不为 View 添加 CLI 领域分支。

**实现事实不等于发行承诺。** 所有用到 Analysis 3、Events v1 的发布门禁都必须用同一个真实已发布 Core 二进制验证。

---

## 3. 最小模块架构：不是持久化系统

```text
原始项目 (.mq.md + 本地图片/媒体)
     │
     ▼
DocumentService ──► MarkdownDocument + SourceIndex
     │                    │
     ├── CoreAdapter ─────┤  Core `check --emit` → Analysis v2/v3
     │                    │
     ▼                    ▼
PresentationModel ◄── SemanticProjection
     │
     ├── ReaderRenderer ──► Live HTML  ← 当前调用结果(可选瞬态附着)
     │        ▲
     │        └── LinkResolver / MediaResolver / MathRenderer
     │
浏览器（显式调用）
     │ POST /api/runs（唯一执行入口）
     ▼
ExecutionController ─► CoreAdapter ─► 真实 MARQDO_BIN run/call
     │                     │
     │                     ├── Execution Events v1 → EventBridge → SSE
     │                     └── Trace v2 → 精确摘要绑定
     ▼
当前 Job 的有界终态投影（仅内存；TTL 到期删除）
     │
     │ 用户点击“导出文档（含本次结果）”
     ▼
POST /api/exports ─► ExportSelectionValidator
     │                     │
     │                     ├─确认 session / run_id / 终态 / 摘要 / 隐私
     │                     └─构造只读 ExportResultOverlay
     ▼
StaticExporter（与普通导出同一套 Renderer）
     │
     ▼
ZIP: index.html + pages/ + media/ + assets/ + manifest.json
     │
     └── user download → 文件独立存在；View 不保存索引/历史
```

### 3.1 模块不可违背的依赖方向

- `protocol/` 只验证公开协议，不依赖 `reader/`、DOM、HTTP。
- `execution/` 只持有 CLI 执行与有界任务状态，不存 HTML；Job 可暴露经鉴权的**只读** `TerminalExportCandidate`。
- `reader/` 可以接受不可变的 `ExportResultOverlay` 并安全渲染，但不得反向调用 `ExecutionController`。
- `export.rs` 只接受静态文档模型、媒体清单和可选的不可变导出叠加层；**不得有 `run()`、`call()`、命令执行或未验证事件解析逻辑**。
- `server/` 鉴权并从当前 Job 获取候选结果，然后传入导出器；这是唯一把瞬态执行观察连接到静态发行物的边界。
- Live、纯文档导出、含结果导出共用 **同一个** `DocumentModel`、`ReaderRenderer`、Link/Media Resolver 和主题。禁止第二套“结果版 HTML 模板”。

### 3.2 推荐目录变更（文件名仅为建议，不强求完全一致）

```text
src/
  core_client.rs                 # MARQDO_BIN 唯一公开调用适配层
  protocol/
    analysis.rs                  # Analysis v2/v3 版本分发 + 验证
    execution_events_v1.rs      # Core NDJSON 事件验证
    trace_v2.rs                  # Trace 绑定与可选观察
  reader/
    markdown.rs                  # 原有 comrak 文字/结构模型
    model.rs                     # 原有 PresentationModel
    render.rs                    # 单一 HTML 渲染器
    links.rs                     # 文档路径、标题锚点
    media.rs                     # 项目内媒体只读解析/安全检查
  execution/
    request.rs                   # 统一参数与权限请求
    process.rs                   # 唯一进程执行实现
    controller.rs                # 单活动任务；瞬态 Job；无结果数据库
    events.rs                    # View SSE
    export_candidate.rs          # 从已完成 Job 只读投影可导出字段
  export.rs                      # 共用静态导出核心
  export_result.rs               # 选择、审查、脱敏、生成 Overlay
  server/
    routes.rs                    # 一个新 POST /api/exports
    security.rs                  # session/Origin/CSRF
assets/
  reader.js                      # 含结果导出确认、文件下载
  reader.css                     # 只读结果排版
```

**若仅需几十行代码，不必为了名字建立一层空模块。** 核心约束是责任单一、依赖单向、统一协议和单渲染管线。

---

## 4. Markdown 全面可视呈现：优先于结果导出

### 4.1 正常可显示的内容

| 类型 | 最小通过要求 |
|---|---|
| H1–H6/Setext | 连续文档流；保持层级；`@exec/@knowledge` 标注不吞标题内容 |
| 段落/行内强调/代码 | CommonMark 源样式；载体高亮不得改变内核语义 |
| 有序/无序/嵌套列表 | 真正 HTML 列表层级；`1.` 分支风格与 `-` 循环风格正确显示；**列表本身不执行** |
| GFM 表格、任务列表、删除线 | 表格对齐、checkbox、删除线真实可见 |
| 代码围栏/引用/分隔线 | 正确显示并保持惰性资源属性 |
| 本地 PNG/JPEG/WebP/GIF/SVG | 有权限控制的真实图片渲染；替代文本、尺寸限制；本地恶意 SVG 不得获得脚本能力 |
| 同页、跨 `.mq.md`、外部链接 | fragment 不丢失；重复标题确定性锚点；外部链接只导航不自动下载 |
| 行内与块级数学 | 离线 KaTeX 或等效安全渲染；无 CDN；不可执行 LaTeX 宏 |
| 源码态 | 无损精确 UTF-8 原文；中文/BOM/CRLF span 可对应 |
| Raw HTML | 默认**不直接执行** HTML/JS；安全降级为明确提示或受限 allowlist，而非声称支持运行任意网页代码 |

注意：Marqdo 的 Markdown profile **不是所有非标准 Markdown 扩展的全集**。例如脚注、自动 URL 链接、wiki-link 等若未启用，不得假称内核支持。View 可展示原文，但不得偷偷让解析选项改变程序语义。

### 4.2 图片与媒体安全

- 从当前文档目录解析相对 URL，允许 `../assets/x.png` **仅在规范化并解析真实路径后仍位于项目根之内**；阻止绝对路径、NUL、控制字符、 symlink 逃逸与 TOCTOU。
- Live 增加受保护的只读媒体读取路由；验证扩展名不足，必须校验 MIME/魔数及尺寸/字节预算。
- Export 将引用的安全媒体复制到 `media/`，计算 SHA-256 和 manifest，脱离原工作目录仍可 `file://` 阅读。
- 默认不自动拉取远程图片、CDN 脚本、字体或执行输出引用；远程资源降级时保留 alt/地址与原因。
- 原始 HTML、SVG 中的脚本、`javascript:` URL、`data:text/html` 等受限；普通外链可单击导航，但不自动发送页面内容。

### 4.3 链接与锚点

`resolve_link()` 必须统一处理：`#slug`、`doc.mq.md#slug`、`../docs/doc.mq.md#slug`、百分号编码与 Unicode、重复同名标题、文档重命名及不存在的目标。Live 和 Export 应产生同义结果，实际点击必须到达目标标题；不能只看生成了 `<a>` 标签。

源字节 span ID 可以继续用于 Analysis 精确绑定，**但需要另外提供符合 Markdown 阅读习惯的稳定标题 slug**；两者分别服务语义锚定与读者跳转，不可混用。

### 4.4 数学公式

数学排版是 View 责任，不是 Math HostOp。资源应离线、固定版本、禁止不可信 HTML/宏，Live 和 Export 共用同一公式处理与字体资产；导出的计算快照即使不能执行，也必须保留公式排版。

---

## 5. 统一单函数执行：View 只有一条运行数据通道

### 5.1 静态能力与目标

- `check --emit` 输出 Analysis v2 或 v3（按 JSON 实际 `schema_version` 验证，绝不猜）。
- H2 `@exec` 函数从 Analysis 确定可调用目标、签名、参数名、默认值；`@knowledge` 不调用。
- `main` 显式 `run` 与所选函数 `call` 使用**同一** `ExecutionController`、权限确认、预算、终态和结果组件；不得扩展为分函数单独后台线程池。
- 对 `else if` 使用已有 `branch.arms`；对 `break/continue` 投影 Analysis v3 对应节点。不存在这些语句时 v2 继续正常支持。
- 跨文档函数调用遵循 CLI `call` 已公开可调用目标限制；不能把依赖模块的任何可见标题都当成可直接调用的独立入口。

### 5.2 实时事件

运行必须在真实已发布二进制上验证：

```bash
marqdo check index.mq.md --emit ./analysis.json
marqdo call index.mq.md 计算均值 --arg 'a=3' --arg 'b=5' --event-stream jsonl --trace-file ./trace.json
```

> 示例仅说明调用与协议形状；参数名称、函数及作用域须与项目真实声明匹配。`--event-stream jsonl` 与 `--json` 互斥，必须做模式选择，不能将两者拼在一起。

`EventBridge` 负责完整解析 `stream.opened`、`run.started`、`stdout.chunk`、`stderr.chunk`、`call.*`、`branch.eval`、`loop.iteration`、`control.break/continue`、`effect`、`run.completed/failed/rejected`。严格校验 seq、schema/version、run_id、终态唯一性、base64 长度和源摘要；EOF 无终态就是 `incomplete`，不是成功。

SSE 仍为 View 与浏览器之间的现有通道。**Core NDJSON 和 View SSE 是不同协议**；允许 Versioned EventBridge 映射，不得把未经验证的 JSON 直接抛给前端执行或视为可信 HTML。

### 5.3 权限和预算

沿用现有 session Cookie、Origin、CSRF、单活动任务、显式授权与 `pure/project` 限制；不暗中开放 net/process/foreign/plugin。Live 页面调用与静态导出都是独立行为；**静态导出 API 不具备运行 Core 的权限或调用路径**。

当前的有界内存 Job 已足够提供“最近一次结果导出”，无需结果历史服务。若执行输出过大，终态保留安全截断标记；**不要为了导出提高 Core 的执行权限或无限内存预算**。

---

## 6. 核心新功能：显式导出包含一次运行结果的静态文档

### 6.1 两种导出，单一实现

**A. 纯文档导出（默认、既有模式）**

```bash
marqdo view export ./project --out ./site
```

含原始 Markdown 的静态阅读视图、引用本地安全媒体、公式资产、manifest。**无需执行、不读取 Controller、不含运行结果**。该命令在全新进程中运行，不存在“刚才浏览器里跑过一次所以能找到结果”的假设。

**B. 含本次执行结果的导出（新增、仅 Live 当前会话）**

浏览器内先显式运行一次，再在终态区域选择“导出文档（含本次结果）”，勾选敏感字段和输出范围，确认后调用 `POST /api/exports`。后台从**同一进程、同一 session 的现有 Job 内存态**读取这次终态，验证后传给同一个 `StaticExporter`；最后直接返回 ZIP 文件供用户下载。**绝不重新运行 CLI**。

本阶段不提供 `marqdo view export --include-last-run` 等会误导人的选项：一个新启动的命令行进程无权也无法访问浏览器会话中的内存任务。未来如需纯 CLI 的“明确运行 → 导出”一体化工作流，必须另立接口规范，要求可验证的同一次执行和显式输出，不能偷偷变成持久 RunStore。

### 6.2 唯一新增 HTTP 路由

`POST /api/exports`，**不是** `/api/runs/save`、`/api/history`、`/api/records`；其职责仅为把一次当前终态转为即刻下载的静态制品。

提议请求格式（本协议为**View 自有新增 v1**，需要在 View 仓库落地 schema 与负例）：

```json
{
  "schema": "marqdo.view-static-export-request",
  "schema_version": 1,
  "run_id": "<current-session-terminal-job-id>",
  "document": "index.mq.md",
  "analysis_digest": "sha256:<exact-current-analysis-bytes>",
  "scope": "document",
  "include": {
    "result_value": true,
    "stdout": true,
    "stderr": false,
    "trace_summary": true,
    "arguments": false
  },
  "confirm_sensitive_output": false,
  "client_export_request_id": "<opaque-id>"
}
```

- body 大小、字段集合、枚举、长度均有严格上限；拒绝未知字段、未知 major。
- 必须是当前 Cookie session 的 Job；`run_id` **不是**授权令牌，不得仅凭 ID 访问其他会话结果。
- 服务器重新验证 Origin + CSRF 及当前文档、真实当前 Analysis 字节摘要、项目身份、Job 终态与来源。
- `run_id` 处于 `running/accepted`、已过 TTL、不存在或不属于当前 session：明确拒绝；**绝不启动新运行**。
- `scope=document` 第一阶段只导出承载该次函数的文档和必要媒体；如果继续保留现有项目级导出布局，则其他页面为**纯文档**，不得把一次结果贴到所有页面。
- `include.arguments=false` 默认。不要默认导出输入参数的实际表达式、授权 glob、环境变量、文本观察值或原始扩展细节。任何含敏感数据选择必须展示最终预览并二次确认；`confirm_sensitive_output` 不替代用户真实 UI 操作。
- 可以导出失败、取消、超时的**终态观察**，但必须醒目标记 `failed/cancelled/timed_out` 和是否有部分数据，不得当成成功结果；第一阶段可为它们提供明确的单独复选确认。
- HTTP 输出为直接下载的 `application/zip`，设置 `Content-Disposition: attachment`、`Cache-Control: no-store`、`X-Content-Type-Options: nosniff` 与严格 CSP。ZIP 构建在有界、私有、短生命周期临时目录内，发送完立刻清理；不增加后台缓存下载接口。
- 浏览器使用经过认证的 `fetch POST` 获取 ZIP `Blob` 后触发用户下载，及时释放 Blob URL；**不能用 localStorage/IndexedDB 缓存 ZIP 或计算结果**。

### 6.3 一个额外的只读导出投影，而不是新数据系统

不新增 `RunStore`。在原有 `Job` 终态中保留足够导出的、**已经完成验证且有字节上限**的 `TerminalExportCandidate`；只在现有 TTL 内存在。Controller 暴露：

```text
get_export_candidate(session_id, run_id)
  -> Result<ImmutableTerminalExportCandidate, ViewError>
```

这个 API **只读、不能执行、不分配长期 ID、不创建记录文件**。内部包含：

- View `run_id` 与 CLI 真实 `core_run_id`（如果协议提供，来源明确）、函数 `function_id`、源文档 `module_id`/相对路径、终态和时间。
- **精确** `analysis_digest`、`root_id`、相关源码 SHA-256、可用的 lock digest、已选择的媒体哈希。为避免泄漏，绝对路径、用户名、环境变量、秘钥不放入导出。
- Core 版本与协议版本、运行模式和预算的**安全摘要**（不包含授权秘密和具体敏感路径）。
- 通过解析器验证后的 Wire Result、stdout/stderr 文本/字节及其完整性状态、可选经核验的 Trace 结构摘要。
- `truncated`、`trace_invalid`、`source_changed`、`output_incomplete` 等独立事实状态；不能为了拥有一个“成功”徽章而合并这些区别。

**注意隐私：** 不记录省略参数值的未经加盐普通哈希作为“隐私替代”，低熵参数可能被猜测；默认只标记 `parameters: omitted`。需要复现实验的用户可以明确选择导出必要的可见参数并接受隐私提醒。

### 6.4 快照来源与失效规则

- 导出开始前获取并核对**当前** `check --emit` 的真实 Analysis 字节摘要；必须与候选结果的 `analysis_digest` 相同，来源 `root_id` 与选定函数 ID 相符。`run.started.source` 的摘要也需要一致。
- 用户运行之后源码、导入内容、锁文件或相关模块变更：第一阶段**拒绝“将旧结果贴到新正文”**，UI 提示先重新运行；不得仅凭函数名相同就附着。
- 运行时源验证通过，但 Trace 文件无效或缺失：导出可保留真实终态、返回值和 stdout，但必须清楚标记 `trace_status=invalid/unavailable`，**不附着任何未经核验的源码侧注**。
- Trace 截断、输出截断、SSE 未收到终态：区分处理。无真实终态的运行不可作为成功快照；可以提供“错误报告”但必须单独实现与审查，不在 P0 混入。
- 静态快照在导出时被冻结；以后源代码变化**不会改变已下载的 HTML**。manifest 提供当时内容的来源摘要，但**不声称当前代码仍与历史结果匹配**。
- 来源身份不是数学正确性证明或第三方签名。除非独立 Prove 工具验证过证书，不展示 `proved`。

### 6.5 导出的内容结构

最低交付为一套 HTML+资产的 ZIP，不要求单文件 HTML；优先延续当前 `StaticExporter`：

```text
marqdo-report-2026-11-24.zip
  index.html
  pages/
    index.mq.md.html
  media/
    diagram-<digest>.png
  assets/
    reader-<digest>.css
    math-<digest>.css
    ...
  manifest.json
```

`index.html` 中的结果应插入**所选函数阅读区域下方**，并在页面顶部有简洁的“本文件包含一次执行快照”提示。静态导出页面的调用按钮应被完全移除；结果使用 `<details>` 或可打印的常规 HTML，禁用 JS 也能阅读。源码段落和结果块的 DOM 类名/aria 标签不同。

**建议 manifest v2 示例**（是 View 自有导出契约，不是 Core 改动）：

```json
{
  "schema": "marqdo.view-export",
  "schema_version": 2,
  "view": {"name": "marqdo-view", "version": "<view-version>"},
  "source": {
    "project_root_id": "root:demo@0.1.0",
    "document": "index.mq.md",
    "source_sha256": "<64hex>",
    "analysis_digest": "sha256:<64hex>",
    "lock_digest": "<lock-digest-or-null>"
  },
  "execution": {
    "included": true,
    "target": "root:demo@0.1.0::index.mq.md::计算均值",
    "status": "succeeded",
    "source_verified": true,
    "trace_status": "verified",
    "result_complete": true,
    "parameters": "omitted",
    "core_protocol": {"analysis": 2, "trace": 2, "events": 1}
  },
  "assets": [
    {"path": "assets/reader-<digest>.css", "sha256": "<64hex>", "size": 1234}
  ],
  "files": [
    {"path": "index.html", "sha256": "<64hex>", "size": 5678}
  ],
  "executed_during_export": false
}
```

这些 `<...>` 字段是**规范示例占位符**，不是可运行的发行文件。真实 manifest 必须通过自有 JSON Schema 校验。`execution.included` 与 `executed_during_export` 是两个不同概念：**包含过去一次明确执行的观察，不代表导出动作执行了任何代码**。不要沿用只有一个易混淆 `executed` 布尔值的旧表示。

是否在 HTML 中另存一份完整机器可读 EventStream，需单独考虑隐私、文件膨胀和浏览器安全；**P0 只保存选择的结果与经过验证的有界摘要**，并在 manifest 中声明完整性，不把所有 token、每个变量、原始秘密偷偷打包。

### 6.6 导出样例的实际 HTML 阅读效果

源 Markdown（继续保持普通、可直接渲染）：

````markdown
# @knowledge 一个小型统计实验

这份文档说明两数平均值的计算依据，并允许读者调用函数。

![算法说明图](./media/overview.png)

## @exec 计算均值

对两个整数求平均值；结果可能有小数部分。

`params(a: Int, b: Int)`

**结果 = (a + b) / 2**

`return 结果`
````

用户手动调用 `计算均值(a=3, b=5)`，假设真实返回 `4`。View 中显示：

> **本次执行** · 函数：计算均值 · 成功
>
> 参数：a=3, b=5（仅在本地 UI 中显示；是否导出由用户决定）
>
> 返回值：`4` · 来源：已验证的 CLI 终态
>
> [导出文档（含本次结果）]

用户确认导出时，静态页面中对应函数下方出现：

> **执行快照（用户于导出时明确附加；不是源文档）**
>
> 执行目标：计算均值；状态：成功；返回值：`4`
>
> 来源：项目/源码/Analysis 摘要见 manifest；未在导出期间重新运行
>
> 参数：未包含（默认隐私设置）

这里的 Markdown 行为只是产品说明，并不意味着执行结果会注入项目 `.mq.md` 文件。样例需要在真实 CLI 与 browser E2E 中验证；如数值类型的 Wire 表达与示意不同，应以真实协议编码为准。

---

## 7. 导出安全与隐私：这是文件发行，不是本地历史

### 7.1 显式披露与选择

输出可能包含令牌、路径、个人信息、数据库记录或远端服务响应。点击导出后至少展示：目标函数、执行终态、所选 stdout/stderr 大小、结果字节数、是否包含参数值、是否含观察值、是否有失败/截断/敏感性风险。用户必须明确确认；不得把浏览器执行结果默认为可发布数据。

推荐默认：包含返回值和有界 stdout、经过验证的结构事件**摘要**；不包含 stderr、原始参数表达式、环境变量、绝对路径、观察到的原始 Text 值、资源句柄或完整二进制内存。用户如需增加内容必须手动勾选，并通过大小及隐私门禁。

### 7.2 严格隔离和输出转义

- 同源鉴权 Cookie + Origin + CSRF + session-job 所有权。禁止跨浏览器会话导出同一个 `run_id`。
- 编码与 HTML 转义统一在 `ReaderRenderer`；stdout、stderr、result/Trace 字符串必须以**文本**渲染，不允许 `innerHTML`、远端模板、运行输出中的 `<img onerror>` 生效。
- ZIP 安全：固定内容路径与规范化目录名、绝不写 `../` 归档条目、不包含系统绝对路径、不将软链接解引用到项目外；控制总文件数、总字节数、单媒体上限、压缩比/解压后大小。
- CSP：静态快照默认 `default-src 'none'`，只开放打包的 CSS、同源相对媒体和必要的离线公式字体；不要包含自动远程字体、远程 JavaScript、iframe 等。
- HTTP `no-store`、下载后立即删除服务端临时文件；浏览器下载动作结束撤销 Blob URL。
- manifest 内容摘要不等于签名；可选独立签名属于未来工具，不得虚构真实性认证。

### 7.3 无保存架构门禁

仓库测试/静态检查应拒绝以下新增行为：

- `.marqdo-view/runs/`、`run_history.db`、`RunStore`、`RunRecord`、浏览器 IndexedDB 持久输出与 `POST /api/runs/save`。
- View 进程启动后主动扫描历史输出或恢复终态。
- `marqdo view export` 在新进程中猜测/查找“最近一次执行结果”。
- 含结果导出未提供有效 `run_id` 却直接执行 Core 以填充结果。
- 为导出特意保留永久临时目录，或导出后在服务器暴露永久下载 URL。

**允许：** 终态 Job 的原有 TTL 内存、只在响应期间存在的临时 ZIP、用户已下载的 ZIP、静态 HTML 所引用的必要本地媒体与样式。

---

## 8. 浏览器与 HTTP 交互设计

现有 API 不破坏，新增路由只做快照导出。

| 路由 | 作用 | 会执行 Core？ | 结果生命周期 |
|---|---|---:|---|
| `GET /`、`GET /file?...` | 阅读当前文档 | 否 | 无 |
| `GET /assets/...` | View 内置 CSS/JS | 否 | 发行资源 |
| `GET /api/documents` | 文档列表 | 否 | 无 |
| `GET /media/...`（提议） | 验证后的项目媒体 | 否 | 只读 |
| `POST /api/runs` | **唯一显式运行入口** | **是** | 现有单任务、临时 Job |
| `GET /api/runs/{id}/events` | SSE 重连/观察 | 否 | TTL |
| `POST /api/runs/{id}/cancel` | 取消已有运行 | 不启动新运行 | TTL |
| **`POST /api/exports`（提议）** | 当前终态 → 静态 ZIP 直接下载 | **绝不** | 响应完成后删除服务端临时文件 |

浏览器 UI 流程：

```text
未运行文档
  │ 点击“调用”→ 填参 → 确认
  ▼
执行中：EventBridge → SSE（取消可用；“含结果导出”不可用）
  │ 唯一终态
  ▼
结果面板：返回 / stdout / 事件 / 状态 / 来源
  ├── 清除当前结果（仅内存 UI）
  └── “导出文档（含本次结果）”
        │
        ▼
     结果范围 + 隐私预览 + 明确确认
        │ POST /api/exports（无 Core 调用）
        ▼
     浏览器下载 .zip，独立离线可阅读
```

若用户选择“导出纯文档”，仍走现有 StaticExporter，只传 `ExportResultOverlay=None`。不应把“已经在 UI 中看到了结果”理解成自动将它放入任何普通导出包。

---

## 9. 数据边界：实现者需要的最小类型与失效状态

提议数据模型（代码为设计伪类型，不表示当前仓库已有这些定义）：

```text
ImmutableTerminalExportCandidate {
  view_run_id, session_owner, terminal_status,
  function_id, module_id, document_relative_path,
  core_version, analysis_version, analysis_digest,
  project_root_id, source_sha256, lock_digest?,
  result_value?, stdout?, stderr?,
  trace_status, verified_trace_summary?,
  output_complete, observation_truncated,
  created_at, terminal_at
}

ExportSelection {
  run_id, document, analysis_digest,
  include_result_value, include_stdout, include_stderr,
  include_trace_summary, include_arguments,
  confirmed_sensitive_output
}

ExportResultOverlay {
  source: VerifiedSourceBinding,
  target: VerifiedFunctionIdentity,
  state: TerminalState,
  value: SafeDisplayValue?,
  stdout: EscapedText?, stderr: EscapedText?,
  trace: VerifiedTraceSummary?,
  completeness: ExplicitFlags
}
```

**禁止 `ExportResultOverlay` 保存 `Arc<Job>`/锁/进程句柄**；它是从执行端复制出的受上限约束、不可变的数据。将其传给 `StaticExporter` 时，不允许导出器自行重查 Controller，也不允许重新解释 Value Wire。

状态显示必须区分：`not_run / running / succeeded / failed / cancelled / timed_out / incomplete / source_changed / trace_unavailable / trace_invalid / trace_truncated / output_truncated`。其中失败/取消/截断是不同维度；不能把 `trace_invalid` 当成运行失败，也不能把运行成功当成完整 Trace 或证明成功。

---

## 10. 真实测试：先构造无法蒙混的 fixture

### 10.1 必需的示例项目

```text
examples/complete-reader/
  marqdo.yaml
  index.mq.md              # @knowledge、@exec、文本、列表、表格、公式、函数
  docs/explanation.mq.md   # 带跨文档与 fragment 链接、重复标题
  media/flow.png           # 真实 PNG，非占位字符串
  media/result.webp        # 真实 WebP，需解码校验
  media/diagram.svg        # 经过安全审核的 SVG
  media/README.md          # 资源来源及使用许可
```

必须有显式函数：`main`、有参数纯函数、含 `else if` 的函数、含 `break/continue` 的循环、失败函数和有界 stdout 函数。真实 Core 使用固定 SHA 的 Linux 发行包测试；View UI 使用真实 Chromium，不仅是 HTML contains 断言。

### 10.2 主要验收矩阵

| ID | 测试情景 | 必须观察到的事实 |
|---|---|---|
| MD-01 | 本地 PNG/WebP/GIF | 实际 `naturalWidth>0`，alt 有效，响应 MIME 与哈希正确 |
| MD-02 | 图像引用 `../media/x.png` | 根内允许；逃逸根、symlink 到外面拒绝 |
| MD-03 | 图片静态导出后离线打开 | 没有任何网络请求，图片仍显示 |
| MD-04 | 外链/相对链接/同页 anchor/跨文件 anchor | 实际点击到达目标；重复标题确定性编号 |
| MD-05 | 行内/块数学和 GFM 表格、列表 | Live、Export 同样渲染，无远端字体 |
| MD-06 | Raw HTML/恶意 SVG/JS URL | 不产生脚本执行或远端加载 |
| CORE-01 | 纯函数 Analysis v2 | 函数表单可用、结果正确 |
| CORE-02 | 带 `break/continue` 的项目 Analysis v3 | View 成功协商、显示正确语义，不误拒绝 |
| CORE-03 | `else if` 多臂 | View 不重新推断分支；结构与来源正确 |
| CORE-04 | `--event-stream jsonl` | `stream.opened` 到唯一终态按 seq 显示；不与 `--json` 混用 |
| CORE-05 | stdout 中间输出 | 执行完成前可见；最终字节与真实输出一致 |
| CORE-06 | EOF 无终态 / 非法 NDJSON / 源摘要错 | 标 `incomplete`/`invalid`，不伪造成功 |
| CORE-07 | 未确认权限/无 main/错误参数 | 不能执行或提示正确，不自动降级权限 |
| EXPORT-01 | 未执行时普通导出 | 没有结果，Core `run/call` 计数 0 |
| EXPORT-02 | 已完成一次函数调用后普通导出 | **仍不包含结果**，执行次数不增加 |
| EXPORT-03 | 已完成一次函数调用后含结果导出 | ZIP 解压可见选中的真实结果，Core 调用数**完全不增加** |
| EXPORT-04 | 同一函数另一次调用 | 只能选定明确的 run_id，不能混合结果 |
| EXPORT-05 | 导出前源码或锁变更 | 拒绝绑定旧结果与新文档；没有隐式重跑 |
| EXPORT-06 | `run_id` 属于其他 session、过期、仍运行 | 403/404/409 中明确稳定错误；没有内容泄漏 |
| EXPORT-07 | 参数/文本观察值涉及秘密 | 默认不含参数、stderr、敏感原始观察；显示导出预览与确认 |
| EXPORT-08 | stdout 包含 `<script>`、`<img onerror>` | 导出页面显示为纯文本，Chromium 无脚本事件 |
| EXPORT-09 | 失败/取消/超时、Trace 缺失/截断 | 显示真实状态；仅合规字段被导出；不标“已验证成功” |
| EXPORT-10 | 导出下载 ZIP 后关闭 View/删除临时目录 | `file://` 完全离线可读、无外部请求；没有依赖 localhost |
| EXPORT-11 | View 重启、超过终态 TTL | 不能恢复任何运行；先前已下载的 ZIP 正常 |
| EXPORT-12 | POST 导出未带 CSRF 或跨 Origin | 拒绝；不进入生成器、不访问其他 session 的 Job |
| EXPORT-13 | 多个恶意路径、ZIP 条目、超大结果 | 不发生目录穿越、内存失控、Zip Slip |
| EXPORT-14 | snapshot manifest / CSS / 媒体篡改 | 哈希验证失败；结果来源与资产身份可核对 |
| EXPORT-15 | 打包中途失败、客户端断开 | 无半成品持久文件；临时文件被清理 |
| ARCH-01 | 查找 View 源码的历史库、持久缓存 | 无 RunStore/历史 DB/跨重启恢复/浏览器结果存储 |
| ARCH-02 | 解析导出函数调用关系 | `StaticExporter` 无 `run/call` 依赖；只有 POST `/api/runs` 执行 |
| ARCH-03 | 单一渲染器 | Live、纯文档导出、含结果导出复用同一个 HTML 构造与安全转义 |

### 10.3 三条最高优先级黑盒门禁

1. 打开 View → 不执行；运行一次 → `run/call` 次数恰为 1；点击“导出含结果” → 次数**仍为 1**。
2. 将 `.mq.md`、图片、依赖摘要在运行之后改变 → 含结果导出**拒绝把旧结果贴到新正文**；不尝试重跑。
3. 下载 ZIP，关闭 View Server，断网，清空浏览器缓存后，从 `file://` 阅读文档、图片、公式及结果；没有脚本执行、远程资源请求或运行功能。

### 10.4 证据要求

须记录用于验收的 CLI 发行二进制哈希和版本、View 源码 SHA、Linux 平台、`cargo fmt/clippy/test --locked`、受保护的 HTTP 与 ZIP 负例、Chromium 截图/浏览器日志、运行计数哨兵和 manifest 摘要。未执行的验证必须写 `UNVERIFIED`，不能用旧版 RFC 的 PASS 代替。

---

## 11. 开发实施顺序：少量 PR，单一职责

| PR | 内容 | 完成标准 |
|---|---|---|
| PR-00 | 固定 CLI/View 二进制与源码基线；更新 ADR，撤销旧 RunRecord/不允许结果导出的相反决策；新增失败 fixture | 开发者只读本 RFC 与仓库即可复现真实缺口 |
| PR-01 | Analysis v2/v3 消费者；新控制流；Execution Events v1 EventBridge | 真实 Core 的 check/call/run 通过，SSE 不重造语义 |
| PR-02 | Markdown 完整性、图片只读媒体、链接/slug、公式排版 | 真正图片与跨页链接在 Chromium 可用，恶意资源拒绝 |
| PR-03 | 将媒体与字体纳入既有 StaticExporter；保持纯文档导出零结果/零运行 | 离线 `file://` 阅读 PASS，manifest 完整 |
| PR-04 | 函数旁就地调用、终态投影稳定、结果源验证；不建历史数据库 | 同函数调用一次、取消、失败、截断均正确显示 |
| **PR-05** | `POST /api/exports`、ExportSelectionValidator、不可变 Overlay、共用 Renderer 的“含结果” ZIP | 显式单次结果快照；**导出绝不执行**；源变更/跨会话拒绝 |
| PR-06 | 浏览器隐私预览、下载流程、a11y/打印/离线、全链路安全/性能/验收证据 | 真实 CLI + Chromium + ZIP/哈希验证通过 |

实施时 PR 不宜一边修协议一边创造新产品数据模型。P0 从**一条已完成且来源匹配的当前会话结果**生成静态 ZIP 即可，不做多结果合并、分享服务器与结果搜索。

---

## 12. 独立开发的 DoD（验收标准）

- [ ] View 仍是独立 Tool v1；不修改 `marqdo-cli`、不链接私有 Rust crate、不添加 `save_*` HostOp。
- [ ] 纯 Markdown 阅读、图片、跨文档链接、锚点、公式、代码围栏、GFM 表格都符合 Profile，真实浏览器完成验证。
- [ ] Analysis v2/v3 与 Events v1/Trace v2 通过公开 Schema 和固定来源摘要验证，旧 CLI 能力不足时**明确拒绝**而不是伪造。
- [ ] `run(main)` 与 `call(function)` 统一任务流程，只有 `POST /api/runs` 触发执行、且必须用户显式确认。
- [ ] 普通 `view export` 零运行、零结果；含结果导出要求当前 session 中明确选中的**终态**，绝不启动第二次执行。
- [ ] 所有导出使用唯一 `DocumentModel` / `ReaderRenderer` / `StaticExporter`，仅通过不可变 `ExportResultOverlay` 添加已验证结果。
- [ ] 用户下载 ZIP 中的 HTML/资源离线可读，结果与原文可视觉区分，源码和源项目不被改动。
- [ ] 运行后源、Analysis、项目身份变更时阻止错误附着；Trace 缺失与运行终态分离，不捏造证据。
- [ ] 默认输出不含未确认的参数值、stderr、秘密、绝对路径或原始高风险观察；HTML/ZIP/XSS/CSRF/媒体路径门禁 PASS。
- [ ] 进程重启后无运行历史可恢复；用户已下载文件可以永久独立存在；没有 RunStore、数据库、`localStorage` 结果缓存、后台自动保存。
- [ ] 全套测试在同一 SHA 的真实发行 Core、真实 Chromium 和离线环境通过；未测试项标 UNVERIFIED。

---

## 13. FAQ：开发者不能自行改写这些设计决定

**Q1：为什么允许用户下载一份包含结果的 ZIP，却不允许保存结果？**

因为两者有本质区别：View 不会在每次运行后主动创建可查询的持久记录；只有用户明确选择“发行一份静态报告”时，才产生一个由用户持有的不可执行文件。**这是文档导出，不是 View 的运行历史系统。**

**Q2：为什么不直接把结果追加到 `.mq.md`？**

`.mq.md` 是源文档与可执行知识，修改它会改变 Git diff、摘要和语义引用，还可能把本来是输出的内容误作为 `@exec` 载体。导出把输出放在独立 HTML 附着层；原文完全不变。

**Q3：为什么不采用 `.ipynb` 的 Cell Outputs / Kernel？**

我们只借鉴“结果与文档共同可阅读、可以离线带走”的优点。Marqdo 以完整 Markdown 文档为中心、函数显式调用并独立执行；Persistent Kernel 会引入隐藏共享变量、历史顺序依赖与新的执行语义，超出 View 职责。

**Q4：用户刷新浏览器，能不能还看到当前结果？**

UI 可以在相同 session、Job 未过 TTL 且安全验证的条件下恢复该次当前任务的 SSE 状态，但**不承诺跨刷新、跨浏览器或跨进程恢复**。想永久保留，须在结果仍可选时主动下载静态快照。

**Q5：为什么不能直接在新启动的 `marqdo view export` 命令中附带“上次结果”？**

因为前一次运行在另一个 View Server 进程的有限内存中；为得到“上次结果”而扫描目录或查询数据库就会重新引入历史存储。另起 CLI 无权猜测这次运行内容。第一阶段只允许 Live 页面引用**同一进程当前 Job**。

**Q6：用户需要真正的科研数据、图像或 Agent 制品怎么办？**

使用 Marqdo 的一般文件/资源能力，由程序在用户授权下明确输出普通文件或外部资源。View 的 ZIP 是一次阅读与执行说明的静态报告；它不代替业务数据文件。必要的项目本地图片可按安全规则一并打包。

**Q7：失败或者 Trace 不完整的运行可以导出吗？**

可以考虑导出**事实清楚标记的失败/部分结果报告**，但绝不能作为成功计算、完整轨迹或证明证书。第一阶段必须至少支持成功终态；其他终态需要显式 UI 确认和对应负例测试。

**Q8：导出文件还能再次执行函数吗？**

不可以。导出是不可执行静态报告。若要重新计算，回到原 `.mq.md` 和真实 Marqdo CLI，再经用户授权调用。

---

## 14. 审计来源与维护要求

### View 仓库源码

- [`src/reader/markdown.rs`](https://github.com/cflmy/marqdo-view/blob/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5/src/reader/markdown.rs)
- [`src/reader/render.rs`](https://github.com/cflmy/marqdo-view/blob/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5/src/reader/render.rs)
- [`src/reader/source.rs`](https://github.com/cflmy/marqdo-view/blob/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5/src/reader/source.rs)
- [`src/core_client.rs`](https://github.com/cflmy/marqdo-view/blob/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5/src/core_client.rs)
- [`src/execution/{request,controller,events}.rs`](https://github.com/cflmy/marqdo-view/tree/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5/src/execution)
- [`src/export.rs`](https://github.com/cflmy/marqdo-view/blob/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5/src/export.rs)
- [`src/server/routes.rs`](https://github.com/cflmy/marqdo-view/blob/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5/src/server/routes.rs)
- [`tool.json`](https://github.com/cflmy/marqdo-view/blob/dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5/tool.json)

### CLI 公开协议

- [`spec/markdown-profile.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/markdown-profile.md)
- [`spec/analysis-v2.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/analysis-v2.md) / [`analysis-v3.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/analysis-v3.md)
- [`spec/trace-v2.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/trace-v2.md)
- [`spec/execution-events-v1.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/execution-events-v1.md)
- [`spec/tool-v1.md`](https://github.com/cflmy/marqdo-cli/blob/652ea763603517ba4d13325a405e3124844ded31/spec/tool-v1.md)

### 交付前必须做的事情

1. 重新取得最新 View HEAD 与已发布 Core tag/binary SHA，核对所有“当前事实”是否仍成立。
2. 对本 RFC 的**新增**请求、manifest 和 Overlay 写入 View 自有版本化 Schema、负例、ADR 与兼容文档，禁止直接把提议示例冒充既有公开协议。
3. 每个 PR 记录测试证据；源码阅读、示例设计与假 Core 测试不能替代真实冻结 CLI/浏览器/离线验收。
4. 更新 `docs/STATUS.md` 与 README，写清“目前支持”和“设计待实现”，禁止把下载 ZIP、Events v1、数学排版等拟议能力提前标为已完成。

**最终设计判据：** 用户只需阅读一篇 Markdown → 明确调用一个函数 → 看懂真实结果 → 在需要时主动把**当前这一次真实结果**连同原文导出为静态报告；这一过程无需 View 保存历史，也不迫使精简 `marqdo-cli` 增加任何 View 专用接口。
