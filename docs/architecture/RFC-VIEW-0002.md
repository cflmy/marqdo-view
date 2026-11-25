# RFC-VIEW-0002：Marqdo View 3.0 交互式执行闭环与产品精简实施规范


> **历史基线（0.2.0）**：本 RFC 是 0.2.0 的实现基线，其中的协议名称、命令行与
> 门禁描述已不是 0.3.0 的当前契约。当前契约见
> [`observation-v1-and-analysis-v3.md`](observation-v1-and-analysis-v3.md)、
> [ADR-0012](../decisions/0012-rc4-current-contracts.md) 与
> [`docs/ACCEPTANCE.md`](../ACCEPTANCE.md)。

> **状态**：Proposed / 具备直接实施所需的设计、约束、测试和验收定义；并非完成声明。  
> **目标仓库**：独立 `marqdo-view`，建议路径 `docs/architecture/RFC-VIEW-0002.md`。  
> **发布日期/审查基线**：2026-11-22。  
> **CLI 唯一公开核心**：`https://github.com/cflmy/marqdo-cli`，审查 `master@eeea01d672c07b6be7d6444031d1619309dc88ab`；首个公开 Beta 源码为 `v3.0.0-beta.1@2c7c4f31e7a388d3fcfa717a77067e696ed13854`。  
> **旧产品，仅供视觉/历史取证**：`https://github.com/cflmy/marqdo`，参考 `main@1c55baabd570e18b9da8f9057cef2f848e176690`。  
> **原始视觉基准**：`docs/design/reference-old-view-reader.png`（2048×1133；截图本身来自用户提供）；开发包随附同名图。  
> **公开接口**：Marqdo CLI 3.0.0-beta.1，Tool Protocol v1、Analysis v2、Trace v2；首要支持平台 Linux x86_64。  
> **文档性质**：这是**完整且自足的实施 RFC**。若已有 RFC-VIEW-0001，以本文件有关执行、功能范围、安全、简化后的页面及发布门禁的规定为准；**不能要求开发者先阅读 0001 或历史聊天才能实施**。

---

## 0. 如何使用本文、证据分级与强制规则

本文把“已经存在的核心能力”和“要求新 View 开发的能力”明确分开：

- **CORE-EXISTS**：已从指定 `marqdo-cli` 提交中的 `spec/` 或 Rust 源码核实，如 `marqdo tool install`、`check --emit`、`run/call --json --trace-file`。仅意味着接口存在，不代表本项目的调用/包装已经测试通过。
- **LEGACY-OBSERVED**：已从旧 `marqdo` 仓库源码审计发现的入口、CSS/JS、渲染路径。这是迁移风险证据，不等于旧截图与 HEAD 产物已经经过浏览器逐像素复验。
- **VIEW-REQUIRED**：本 RFC 新规定的 `marqdo-view` 功能、类型、路由、测试和性能/安全边界，需由开发者实现。
- **VIEW-DEFERRED / REMOVED**：必须推迟或直接删除的旧功能，不能为了“功能还原”偷偷加入范围。

**决策优先级**：发布 CLI 的真实黑盒行为及同 tag 规范 > 本文所列核心事实摘要 > 本文针对新 View 的设计决定 > 旧 View 源码与截图的视觉线索。若 Beta CLI 的精确语法/参数与本文示例不符，先用其 `--help`、`spec/` 和黑盒测试确认差异；更新本文及 fixture，**绝不修改 CLI、添加第二解释器或伪造一次成功运行**。

**核心不可妥协条件（MUST）**：

1. 新仓库独立构建、测试、打包、安装；**不更改和不重新编译** `marqdo-cli`，也不依赖其私有 Rust crate、Git/path 依赖或内部 IR。
2. 所有语言真相来自官方 `check` 输出的 **Analysis v2**；运行真相来自官方 `run/call` 的 **JSON** 和 **Trace v2**；原始文档内容直接读取 `.mq.md`；Markdown 渲染器只管排版，**不判断可执行语义**。
3. 打开、刷新、搜索、切换文件、静态导出、预览、生成目录和检查诊断**绝不运行程序**。运行必须是用户显式操作，且命令、函数、参数、权限清楚可见。
4. **单一 Read Model、单一 HTML 组件体系、单一 CSS 主题入口、单一 JS 入口**。Live 和 Export 共用渲染代码；只改变链接策略和运行控件启用状态。
5. 原生交互执行只允许通过 `MARQDO_BIN` 的 `run` 或 `call` 子进程；不引入 Jupyter/Notebook/REPL、隐式 session kernel、浏览器端表达式求值器、另一个 Host 或运行时状态机。
6. Tool 本身是普通操作系统进程；CLI `pure`/`project` 不是 OS 沙箱，Worker 也不是 OS 安全沙箱。不得声称浏览器运行天然隔离。
7. 不开发 `marqdo-lint`、`marqdo-knowledge`、`marqdo-prove`；未来只通过版本化可选报告叠加信息，不设硬依赖。
8. 每个可见按钮必须实际有效；暂时不具备公开数据和执行能力的面板**直接删掉**，禁止灰色假按钮占据主界面。

### 0.1 一句话产品定义

> **Marqdo View 是一个“默认只读、显式调用、结果就地呈现”的可执行 Markdown 阅读器**。它不是 IDE，不是 Notebook，不是第二套 Marqdo 编译器，也不是知识/证明/AI 管理平台。

### 0.2 成功的用户任务

1. 用户执行 `marqdo view index.mq.md`，打开温暖、连续、原生 Markdown 阅读页面；**零程序副作用**。
2. 用户在函数标题旁点击“调用”，从 Analysis v2 获得参数名/类型/是否必填，填入值，确认命令与权限，CLI 通过 `call` 执行，View 显示**真实返回值、标准输出、错误、Trace**。
3. 用户点击底部金色“运行”，明确运行入口 `main`，结果同样回到当前阅读页。没有 `main` 则解释原因并建议选择函数；绝不自动找别的函数替代。
4. 用户看到一次已执行的调用、判断和循环事件，能通过准确的源码位置阅读相关内容；不存在逐语句赋值值时不伪造。
5. 用户执行 `marqdo view export . --out site/`，得到离线可读的静态 HTML，**无一次运行、无外部网络请求、无改写源项目**。
6. 完全离线、只安装已发布的 `marqdo` 和新 View 的机器即可运行上述流程，完全不需要历史 Marqdo 工作树。

### 0.3 明确不承诺

不承诺旧版 View 的调用帧参数值/逐句变量值/内存状态；不承诺实时 LLM Token 流、持久内核、断点和单步调试；但交互式执行的状态与结果必须通过 SSE 推送，禁止浏览器轮询；不承诺未知外部扩展安全；不承诺跨平台发行或对任意旧 `.mq.md` 自动升级。所有缺失项需在 UI 中以“未提供/不可用/不支持”表示，**不可当成不存在的运行事实**。

---

## 1. 为什么需要交互执行，为什么必须精简

### 1.1 交互执行是区别于普通 Markdown 阅读器的关键

Marqdo 的价值不是把源码染上颜色，而是使自然语言叙述、可调用函数和计算结果共享同一个文档上下文。单纯显示静态 Analysis 只能解释**可能怎样运行**，无法展示**这次确实做了什么**。一旦读者明确选择函数、填入参数、触发运行、查看返回及事件，就能在科研、教学、算法验证、领域 SKILL 阅读中形成实际闭环。

但必须防止旧版本的错误演进：View 因承载 Notebook、debug、知识编译、证明、事件总线、Agent、内存和会话等职责，产生多套 DOM 模板、重复 CSS/JS 和执行权限模糊。新 View 的价值来自**一个具体闭环的完成度**，不是功能数量。

### 1.2 产品功能范围（此表具有强制验收效力）

| 功能 | 判定 | 原因 / 处理方式 |
|---|---|---|
| 普通 Markdown/GFM 阅读 | **MVP MUST** | View 的主体。保留段落、标题、列表、表格、代码围栏、公式、链接等原文顺序 |
| `@exec` 函数与 `@knowledge` 单元的轻量标识 | **MVP MUST** | 依据 Analysis；不推断未公开的完整 H3 有效模式 |
| 源码态（纯文本，准确映射） | **MVP MUST** | 证明代码即文档；只读，不内置编辑器 |
| 函数清单与函数导航 | **MVP MUST** | 支持发现可调用 H2；不展示复杂的变量图 |
| 单函数参数填写 + 调用 | **MVP MUST** | 直接验证 `marqdo call` 与 Analysis 签名 |
| 文档入口 `main` 执行 | **MVP MUST** | 直接验证 `marqdo run`；须显式操作 |
| 运行结果（返回、stdout、stderr、错误） | **MVP MUST** | 官方 `--json` 真值，用户最需要的反馈 |
| Trace v2 的调用/判断/循环/副作用事件 | **MVP MUST** | 官方的真实运行证据；无数据时安全降级 |
| 执行确认、取消、超时与单任务约束 | **MVP MUST** | 防误执行和资源泄漏，不是增加 Notebook |
| **SSE 实时执行状态与终态结果** | **MVP MUST** | 使用 POST 建立任务、GET SSE 接收状态/结果、POST 取消；**禁止轮询**，不承诺逐指令或 Token 实时流 |
| 独立 Tool 安装/调用 | **MVP MUST** | 验证内核与外围完全解耦 |
| 静态 HTML 导出 | **MVP MUST** | 文档价值可脱离本地服务；无执行副作用 |
| 暖纸色、品牌、樱花、轻量侧注、底部 Dock | **MVP MUST** | 保留给定截图的识别度，删除旧主题叠层 |
| 基础文内搜索和目录 | **MVP MUST** | 文档必要可达性；不做跨仓全文数据库 |
| 有选择地在侧注显示“本次确实发生的事件” | **MVP MUST** | 让结果回到原文，拥挤时退化为行内摘要 |
| 深色主题、字号与显示设置 | **MVP SMALL** | 可提供简单开关；不引入主题框架或在线皮肤市场 |
| 高级项目权限：选定 fs read/write glob | **MVP CONTROLLED** | 只在明确确认后 `--mode project`；按限制逐项允许 |
| 网络、进程、环境访问授权 | **DEFERRED** | 风险和 UI 复杂度较高，首版只能提示改用 CLI；不得静默授权 |
| Plugin/Provider/Foreign 可视化安装、授权、热插拔 | **DEFERRED** | 已有 CLI 可用，但 View 不应成为扩展管理器 |
| 变量面板（入口最终变量全集） | **REMOVED** | Trace v2 不提供；不能以老私有捕获替代 |
| 内存检查器、内存网格、地址寄存器 GUI | **REMOVED** | 是调试工具职责，不是阅读工具 |
| Notebook Cell / Run All / Restart Kernel | **REMOVED** | 需要持久内核，与一次 `run/call` 不同 |
| 断点、单步、调用栈、堆栈帧、时间旅行 | **REMOVED** | 需要独立调试协议，不属于 View 第一阶段 |
| Session / journal / snapshot replay / cell persistence | **REMOVED** | 引入第二个状态管理系统且无法使用现行公开协议实现 |
| AI 对话框、实时 LLM token 流、Agent 控制台 | **REMOVED** | 交给未来独立 Agent/Tool；CLI 暂无对应稳定协议 |
| View 内编译 Knowledge/Prove | **REMOVED** | 对应工具独立仓库，避免再次耦合 |
| 可视化格式修正和完整 Linter | **DEFERRED** | 后续独立 `marqdo-lint`；本版仅呈现 CLI diagnostics |
| 数据库、账号、多租户、工作台、知识后台、插件市场 | **REMOVED** | 不是本产品 |
| 第二套 Markdown AST / 解释器 / `inspect --ir` 解析 | **FORBIDDEN** | 破坏 3.0 语义单一权威 |

**MVP CONTROLLED** 是首版需提供的安全受限项目运行：只接受相对项目根的精确 fs read/write grants，暂不把 `--allow-net/--allow-process/--allow-env/--allow-foreign/--plugin/--provider/--kernel/--trust-native-plugin` 暴露到浏览器中。完整高级执行由用户在终端运行 CLI。若项目已声明第三方扩展，Core 可能拒绝 `pure`；View 解释错误并引导外部 CLI，而不是猜测缺失的 grants 或自动降级到 `project`。

### 1.3 首版页面删减决策

给定截图旧顶部是 `文档 / 函数 / 变量 / 内存 / 运行`。新版本**只保留 `文档 / 函数 / 运行` 三项**；`源码`作为页面上的切换或“更多”中的明确入口，不占据新的一级后台式菜单。变量、内存两项**不显示**，也不留空壳。

旧 Dock 保留五个视觉元素，但必须严格重定义：**刷新**＝仅重新读取与检查；**设置**＝字体/主题/轨迹显示；**运行**＝执行 `main`，无 main 则打开函数选择；**清理**＝清除当前会话结果与临时工件，**不删除项目文件或 lock**；**更多**＝源码、导出、版本/帮助。除此不增加常驻按钮。界面默认的首屏高度、文档连续性优先于调试信息密度。

### 1.4 明确不同的“执行”概念

- **静态结构**：来自 Analysis v2 的函数、语句、分支、循环，未运行时可展示结构，但不得暗示已经执行。
- **本次运行**：来自一个经确认的 CLI `run/call`；用户能看到运行目标、参数、模式、命令及结果。
- **轨迹证据**：Trace v2 的事件，能说明发生过哪些分支/调用/循环/副作用；不是完整调试器，也不是证明证书。
- **领域正确性/数学证明**：不属于 View 或 `check` 成功的含义；未来 Prove 独立提供并验证。

---

## 2. 已审查的公开 CLI 合同：外部开发者无需猜测

本节是开发必需的公开接口摘要，实施时用指定发布版二进制运行本节黑盒命令。**不要把本节作为发明新核心选项的许可。**

### 2.1 版本及来源

| 版本轴 | 当前实际值 | 使用原则 |
|---|---|---|
| Marqdo 产品 | `3.0.0-beta.1` | Tool 兼容声明使用 `>=3.0.0-beta.1,<4.0.0`，Beta 期间须复测 |
| CLI Tool Protocol | v1 | 外部工具用 `tool.json` 安装，核心分发未知顶层命令 |
| Analysis | v2 | 静态语义、函数签名、源码 byte spans，唯一公开结构化编译结果 |
| Trace | v2 | `run/call --trace-file` 的静态来源关联事件 |
| Markdown Profile | `marqdo.md.v1` | 公共 Markdown 解析档案，不等于旧版本的语句语义 |
| 锁文件 | v4 | 项目身份与依赖；View 不更改锁文件 |
| 旧项目 | Marqdo 2.x | 不兼容，不提供 legacy mapper |

CLI 发布 tag 对应源码记录：`2c7c4f31e7a388d3fcfa717a77067e696ed13854`。当前 `master` 可能在此 tag 之后继续更新；**测试始终以实际安装的发行二进制 `--version`、SHA-256、`RELEASE_INFO.json` 和对应 `spec/` 为准**。若未下载到发行包，明确记 `BASELINE_UNAVAILABLE`，不可伪称跨仓通过。

### 2.2 外挂 Tool 安装：实际支持，不需要改核心

发布 bundle（不是源仓库根的假模板）应具有：

```text
marqdo-view-package/
  tool.json
  bin/
    marqdo-view                # 真实可执行文件；Linux x86_64
  assets/                      # 如果没有全部内嵌，则仅打包已校验文件
    reader.css
    reader.js
    ...
```

`tool.json` 内容：

```json
{
  "schema": "marqdo.cli-tool",
  "schema_version": 1,
  "name": "marqdo-view",
  "version": "0.2.0",
  "command": "view",
  "entrypoint": "bin/marqdo-view",
  "compatibility": {
    "marqdo": ">=3.0.0-beta.1,<4.0.0",
    "analysis": [2],
    "trace": [2]
  }
}
```

**`0.2.0` 是建议的独立 View 产品版本，不是 CLI 产品版本，也不是现有发布的事实**。安装及分发：

```bash
export MARQDO_BIN=/absolute/path/to/released/marqdo
"$MARQDO_BIN" --version
"$MARQDO_BIN" tool install /absolute/path/to/marqdo-view-package/tool.json
"$MARQDO_BIN" tool list
"$MARQDO_BIN" view --help
"$MARQDO_BIN" view index.mq.md
"$MARQDO_BIN" tool remove view
```

核心注册表会将未知顶层 `view` 分发至**外部独立二进制**，不会添加 `Command::View`。工具执行进程继承普通 cwd/stdin/stdout/stderr，并获得：`MARQDO_BIN`、`MARQDO_TOOL_PROTOCOL=1`、`MARQDO_PROTOCOL_ANALYSIS=2`、`MARQDO_PROTOCOL_TRACE=2`（在兼容协商成功时）。工具**不得**用 PATH 猜测另一个 `marqdo`，也不得把这几个环境变量当作用户已授予的执行权限。

当前 Tool 的包安装会检查目录/工件清单并重新校验整体摘要；所有发布文件必须在打包后保持字节稳定。任何插件/资产修改之后都应**重建并重新安装工具包**，不能用“开发时直接替换已安装文件”绕过完整性。

### 2.3 静态检查与 Analysis v2

```bash
"$MARQDO_BIN" check index.mq.md --emit /private/tmp/analysis.json
# --refs 为可选的链接检查：可能产生额外报错，不应被误作语言编译错误
"$MARQDO_BIN" check index.mq.md --refs --emit /private/tmp/analysis.json
```

成功时读取文件中的准确 JSON 字节，`schema="marqdo.analysis"`、`schema_version=2`，再计算 `sha256:...`。`modules[]` 包括 `id`、`relative_path`、`source_digest`、`units`、`functions`、`imports`、`references`、`calls`、`resources`。`functions[].signature.params[]` 包括 `name`、`required`、`type`、`default`；`functions[].id`、`namespace`、`local_name` 提供定位。`units[].kind` 是 `root|namespace|function|knowledge|section`，但 Analysis v2 **没有公开所有 H3+ 的最终 EffectiveMode 字段**。诊断位于顶层 `diagnostics[]`，标记 UTF-8 字节范围。

Core `check` **不执行目标程序**。失败时不要读取曾经的 `analysis.json` 作为成功产物；Core 会清除 stale emit，View 亦必须清除自己的旧缓存。可读取 `check --json` 仅用于展示真实错误，但不能从错误推断尚未公开的 IR。

### 2.4 运行：`run` 与 `call` 不等价

`run` 默认调用当前入口文件的 `main`，不接收命名函数的参数：

```bash
"$MARQDO_BIN" run index.mq.md --mode pure --json \
  --trace-file /private/tmp/trace.json \
  --timeout 30 --fuel 1000000 --max-output 1048576 \
  --trace-max-events 10000 --trace-max-bytes 4194304
```

单函数调用通过 `call FILE FUNCTION --arg name=CONSTANT`：

```bash
"$MARQDO_BIN" call index.mq.md main \
  --arg n=5 \
  --mode pure --json --trace-file /private/tmp/trace.json
```

**开发者必须保留两个按钮的语义差别**：底部 Run 仅表示 `run main`；在“函数”面板选择 `compute` 是 `call compute`。没有 `main` 时底部按钮不能偷偷运行“第一个函数”，应打开函数列表或解释原因。

`--mode` 在核心中支持 `pure|project|strict`，**CLI 自身的默认是 `project`，但 View 必须显式发送 `--mode pure`**，绝不能遗漏后使用核心默认。`strict` 是 Linux probe-gated 的独立模式；本 View 第一版不提供 strict 的保证或按钮。Project 模式并非 OS 沙箱。

Core 还支持一组 grants（`--allow-read`、`--allow-write`、`--allow-net`、`--allow-env`、`--allow-process`、`--allow-random`、`--plugin`、`--allow-plugin`、`--kernel`、`--allow-foreign`、`--provider`、`--allow-provider`、`--trust-native-plugin`）；**这里只陈述 Core 存在这些选项，View 首版仅允许用户显式选择 read/write glob 作为项目权限**，其他全部拒绝并提示通过终端调用 Core。这是刻意缩小安全面，而非误以为 Core 缺少扩展能力。

### 2.5 `--json` 成功与失败包络

在 CLI `crates/cli/src/commands/run.rs` 中，成功结果具有 `command`、`success:true`、`value`（`marqdo_value::to_wire(...)`，不是任意普通字符串）、`stdout`、`stderr`、`diagnostics`、`trace`；失败常见 `success:false`、`error` 或 `diagnostics`，并可能包含 stdout/stderr/trace。**处理器应接受某些失败路径缺少 `value`/`trace`/`stdout`，不要直接 unwrap**。

`value` 是 CLI 自己定义的版本相关 Wire Value；首版 View 仅进行**结构化安全展示**：优先呈现已识别的安全标量值，其他保留为折叠的、安全转义 JSON，不推导未公开的具体运行状态。stdout/stderr 完全视为用户数据，**不能赋值给 `innerHTML`**。

### 2.6 运行来源和 Trace v2

```bash
"$MARQDO_BIN" run index.mq.md \
  --mode pure --json --trace-file /private/tmp/trace.json
```

`trace.json` 顶层：`schema:"marqdo.trace"`、`schema_version:2`、`run {id,entry,status}`、`source {root_id,analysis_digest}`、`limits`、`truncated`、`events[]`。事件种类包括 `call.enter`、`call.exit`、`return`、`branch.eval`、`loop.iteration`、`effect`、`trace.truncated`。每条有源位置的事件必须指定真实 `module_id` 和 UTF-8 `span`。未来新增事件类型按协议忽略或保守显示。

**绑定规则（不能简化）**：将 `trace.source.analysis_digest` 与同一源码、同一检查配置所得到的 **Analysis JSON 文件的原始字节**的 SHA-256 比较，而不是反序列化再序列化后做哈希。还须核对 `source.root_id`、相关 `module_id` 和 `span` 的边界。Trace 的 `truncated` 或 `trace.truncated` 代表观测预算耗尽，不代表程序终止。Trace v2 **不记录参数值、逐语句赋值后的变量值、Handle、文件内容和秘密**。

### 2.7 三种载荷外观，不能恢复旧语法绑定

3.0 中 `@exec/@knowledge` 区分语义区域；在有效 Exec 位置，`**语句**`、`*语句*`、行内 `` `语句` `` 均能携带一条语句，实际执行分类由语句里的 `if/else/while/end/return/params`、赋值/表达式决定。H2 Exec 才是函数，H1 仅命名空间，H3+ 仅节。标题若没有模式声明则继承最近祖先模式；文档根是 Knowledge。

**这不授权 View 解析/推断 Exec**：渲染原始 Markdown 是合法的；表示“此处有函数/语句”必须来自 Analysis；`@knowledge` 不是“已数学证明”，`@exec` 不是“这次实际执行”。对于 H3 的模式继承，Analysis 未公开完全有效模式时只能显示准确的语义索引和显式源文本，不得把自行重建的 Mode 算法宣称为编译器真实状态。

### 2.8 开发者对应阅读的权威文件（本 RFC 已给出实施摘要）

位于 `cflmy/marqdo-cli`：`spec/tool-v1.md`、`spec/analysis-v2.md`、`spec/trace-v2.md`、`spec/mapping.md`、`spec/expression.ebnf`、`spec/schemas/{tool-v1,analysis-v2,trace-v2}.schema.json`、`docs/developers/{tools,analysis-trace,compatibility,release-limits}.md`、`crates/cli/src/{main,args,analysis,trace,tool}.rs`、`crates/cli/src/commands/run.rs`。本文不要求开发者另找设计资料或阅读历史聊天，但如正式协议有新提交，以**确切发布版**为最终权威。

---

## 3. 旧 View 的混乱必须有证据地去掉，不做整体代码移植

### 3.1 从旧源码直接确认的结构风险

在旧 `cflmy/marqdo@1c55baa`：

| 路径 | 已观察到的事实 | 新版裁决 |
|---|---|---|
| `src/main.rs` | `View`、`ViewAction::Output/Capture/Document`、`Debug` 是不同 CLI 路径；`ViewAction::Document` 还有 Workbench HTML 输出 | 不复制旧命令树，只保留一套新 View 命令 |
| `src/view/html.rs` | 包含庞大的 `stylesheet()` 和 `notebook_ui()`，同时在 `layout()` 又嵌入 `reader.css` / `reader.js` | **彻底废弃旧 CSS 叠层；保留视觉 token 重新实现** |
| `src/view/reader.css` | 暖纸/樱花/羽毛/衬线/五按钮 Dock/侧注曲线的重要样式来源 | 只抽取设计特征/必要资产，不原样复制整个文件后再覆盖 |
| `src/view/reader.js` | 以 `/welcome/` 文件名匹配注入 Hero；布局侧注 SVG 曲线和交互 | Hero 改明确页面配置；曲线保留为可降级小模块 |
| `src/view/debug_page.rs` | 独立暗色调试页面、调试工具、断点 | **删除**，不是新的 reader 模式 |
| `src/view/mod.rs` | Live page 与 Debug 走不同路由；还存在很多 Session/Notebook/API 路由 | 新服务仅留纯阅读与用户主动运行 API |
| `src/view/output.rs` | Live/Export 的页面模板有重用，但执行抓取路径与 `--no-exec` 控制相互交错 | 导出构造器**从类型边界不能取得**运行能力 |
| `src/view/knowledge_*` | 依赖旧 `SemanticProject`、知识编译结构 | 全部不迁移；3.x 专用 Knowledge/Prove 未来独立仓库 |
| `src/view/render.rs` | 依赖旧 AST 与 Strong/Emph/列表的语义映射 | 不能作为 3.0 执行语义真相 |

**审计限定**：以上说明旧代码存在多条页面生成路径和样式覆盖，并不等于已经在浏览器里证明有“两套完整 View 同时竞争路由”。随附截图是**视觉验收依据**，不是源码实现证据。旧项目能否编译/对应什么样的运行 DOM 仍须单独记录 `LEGACY_RUNTIME_VERIFIED` 或 `LEGACY_RUNTIME_UNVERIFIED`，禁止把未经复现的历史印象当作软件事实。

### 3.2 必须记录的取证文件

新仓库创建 `docs/audits/legacy-view.md`，列出旧命令到入口函数、页面模板、CSS 注入顺序、JS 初始化、执行调用的连线。对每个可用资源标记 `KEEP_ASSET / REIMPLEMENT / DROP / UNKNOWN`；经审计批准后再将必要 Logo、樱花、羽毛放进本仓库并记录许可证、SHA-256 和来源提交。不能对旧 `marqdo` 建立任何 Git/path/Cargo 依赖。

最少调查命令（只读，可在旧源码 checkout 中运行）：

```bash
rg -n 'ViewAction|Commands::View|page_file|page_debug|layout_debug|serve_debug' src/main.rs src/view
rg -n 'stylesheet\(|notebook_ui\(|reader\.css|reader\.js|notebook-page|reader-page' src/view
rg -n 'build_file_view|run_file_capture|write_static|no_exec' src/view
rg -n 'reader-hero|reader-dock|reader-connectors|annot-band' src/view
```

### 3.3 应当真正删除的旧目录与功能概念

新仓库**禁止创建** `src/notebook/`、`src/debug/`、`src/memory_inspector/`、`src/session_kernel/`、`src/knowledge_compiler/`、`src/proof_engine/`、`src/agent/`、`src/legacy_compat/`。也禁止“先复制再隐藏”的做法：没有相应 DOM、路由、状态容器、请求处理器和测试假依赖，才叫删除。代码审查必须拒绝因为 UI 复刻而在 View 中内嵌旧 AST 或旧 `RunCapture`。

---

## 4. 新产品信息架构与视觉标准：删到刚好够用

### 4.1 给定截图的视觉特征

**本次交付的参考图片文件尺寸为 2048×1133**；可确认暖白纸色画布、左上粉色樱花枝、顶部居中 Marqdo 标识及小字、副标题与衬线字体、五项旧导航、金色欢迎大标题和羽毛、连续阅读区、金色函数竖标、轻边框表格以及底部居中椭圆浮动操作栏。右侧的黑色短线和竖线不能据此判定是必要设计元素，**不复刻任何未知划线 artifact**。

新 View 继续保持其**文档阅读的温度**，但删掉旧的 Variables/Memory 标签、满屏 Notebook 工具条、技术字段表、默认巨大执行日志。普通 `.mq.md` 的第一页直接显示作者文档，不自动加一个巨大 Welcome Hero。只在明确的 `welcome` 配置或独立欢迎页面显示装饰性大 Hero。

### 4.2 唯一页面骨架

```text
┌──────────────────────────────────────────────────┐
│  菜单              Marqdo             搜索       │
│             文档   函数   运行                    │
├──────────────────────────────────────────────────┤
│                                                  │
│             文章标题 / 可选 Welcome              │
│       文档段落、图、表格、公式、函数原文           │
│          [函数标题  · 调用]                      │
│          [可选：本次事件侧注]                    │
│                                                  │
│          [独立的运行结果面板，默认折叠]           │
│                                                  │
│         ⌜刷新 设置 ▶运行 清理 更多⌟             │
└──────────────────────────────────────────────────┘
```

- `文档`：主视图，始终原文顺序；默认选中，滚动是连续阅读，不是可执行卡片列表。
- `函数`：列表/抽屉，而不是替代正文的后台页面；点击导航到原文 H2，点击调用打开单函数表单。
- `运行`：仅显示当前会话运行历史（首版最近 1 次）和事件；默认不占据文章前部，不代替文档。
- `源码`：通过“更多”或正文上的局部切换显示纯源码/复制按钮；**没有内建源码编辑器或保存操作**。
- `Dock`：五个按钮有单一行为，执行按钮因没有 main 应引导选择函数，不应变成“运行所有”。
- 用户可选择隐藏所有语义侧注，恢复与普通 Markdown 阅读器基本相同的页面。

### 4.3 风格 token / 资产约束

建议以旧 `reader.css` 的核心色感为初始：`--paper:#fcf9f5`、`--surface:#fffcf8`、`--ink:#29251f`、`--muted:#847565`、`--line:#e7ddce`、`--gold:#b88a45`、`--olive:#52603a`。正式主题只有 `assets/reader.css`，唯一入口 `assets/reader.js`；其他细分 CSS/JS 如果存在，必须在构建中合并成一份显式顺序产物，且没有旧 `.notebook-page` 皮肤覆盖。

主文字使用本机衬线字体族，不从 CDN 下载字体。桌面正文可读宽度约 720–900 CSS px，侧注只有宽屏才靠两侧排布。≤900 px 侧注转正文；≤600 px 单栏且 Dock 不遮挡末尾；打印隐藏装饰/交互控制。响应 `prefers-reduced-motion`、键盘焦点和触屏尺寸。

### 4.4 侧注：保留审美，不恢复虚假的值快照

侧注可分为 `Static`（参数、函数、源链接）和 `Observed`（本次 Trace 的调用/分支/循环/副作用）；只有后者允许标记“已发生”。旁注最多保持合理密度（建议同一段默认 ≤3 个），重叠时折叠为有编号的就地注释，不能让线穿越正文。连接线可用简洁 SVG 弯曲路径；失败时**降级为行内关联**。它们是增强，不是布局生存条件；静态导出与禁 JS 阅读都不能丢失核心说明。

### 4.5 不允许假装拥有的旧工作台视图

不得提供“变量”“内存”空标签，不得用 `value` 的最终返回冒充所有赋值结果，不得显示未由 Trace 提供的所谓实时函数栈。网页也没有实际生效的 `Run All`、`Restart Kernel`、`Step`、`Breakpoint` 和 `Stream Token` 控制。若未来有正式公开调试协议，再另开 RFC，**不能先设计空 UI 等接口**。

---
## 5. 唯一架构：Reader + Controller + CLI，禁止第二运行框架

### 5.1 模块职责及不可逆依赖方向

```text
                         用户浏览器（只读首屏）
            ┌──────────────────────────────────────┐
            │ Reader UI / Document / Function / Run│
            │ reader.js：仅 DOM、表单、SSE 状态     │
            └─────┬───────────────────┬────────────┘
                  │ GET 静态页        │ POST /api/runs（用户主动）
                  │                   │ GET /events（SSE）
                  ▼                   ▼
       ┌────────────────┐    ┌────────────────────────┐
       │ DocumentService│    │ ExecutionController    │
       │ Markdown renderer│   │ 单任务、取消、超时、缓存│
       └──────┬─────────┘    └───────────┬────────────┘
              │                          │
       ┌──────▼─────────┐        ┌───────▼────────────┐
       │ AnalysisClient │        │ CliProcessAdapter  │
       │ marqdo check   │        │ marqdo run/call    │
       └──────┬─────────┘        └────────┬───────────┘
              │                           │
              └───────────┬───────────────┘
                          ▼
                 MARQDO_BIN：冻结发行版
                    Analysis v2 / Trace v2

                   ↓ 合并到唯一数据模型 ↓
                    PresentationModel
                           ↓
                 Renderer（Live/Export 共用）
```

绝对禁止任何 `DocumentService → ExecutionController` 隐式调用；`GET /`、`GET /file`、`GET /api/documents`、目录扫描、前端刷新、静态导出不能形成执行边。运行命令仅从 `POST /api/runs` 的服务端处理器显式产生。**浏览器端任何代码都不能直接组装系统命令或执行语言表达式。**

### 5.2 层间数据结构（供实现与单测固定）

推荐 Rust 单 crate：

```rust
// 类型草图；字段语义是合同，具体序列化通过新仓库 serde 定义。
struct SourceDocument {
    relative_path: String,           // 相对于受控项目根目录
    raw_utf8: Vec<u8>,               // 精确保留换行和 BOM 的原始字节
    source_sha256: String,
}
struct AnalysisArtifact {
    bytes: Vec<u8>,                   // 原始 check --emit 文件字节
    digest: String,                   // "sha256:" + hex(SHA256(bytes))
    schema_version: u32,             // 必须 == 2
    root_id: String,
}
struct SemanticAnchor {
    module_id: String,
    start: usize,
    end: usize,
    kind: String,                     // 仅来自公开 Analysis 节点
}
struct ExecutionRecord {
    run_id: String,
    state: RunState,
    source_analysis_digest: String,
    target: String,
    mode: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    result: Option<SafeCliResult>,
    trace: Option<VerifiedTrace>,
    stale: bool,
}
struct PresentationModel {
    source: SourceDocument,
    analysis: Option<AnalysisArtifact>,
    diagnostics: Vec<SafeDiagnostic>,
    anchors: Vec<SemanticAnchor>,
    execution: Option<ExecutionRecord>,
}
```

`SafeCliResult` 不保存环境变量、原始权限令牌、密钥，也不假定 `value` 一定存在。字段类型在实现中采用显式 DTO，禁止把内部 JSON 对象直接送入 HTML 模板作为信任值。View 的动态数据模型只表示用户**发起的一次外部调用及其完成记录**，不代表解释器的持久状态。

### 5.3 仓库结构：仅一个应用、一个主题、一个渲染入口

```text
marqdo-view/
├── Cargo.toml                        # 一个主 binary crate；禁止复制 marqdo workspace
├── Cargo.lock
├── tool.json                         # 随发行包输出，command=view
├── README.md                         # 安装、使用、限制、截图和开发入口
├── LICENSE-MIT / LICENSE-APACHE      # 与实际使用的源码/资产许可证对应
├── docs/
│   ├── architecture/
│   │   ├── RFC-VIEW-0001.md          # 历史基础设计（可选归档）
│   │   └── RFC-VIEW-0002.md          # 本文件（最终执行规范）
│   ├── audits/legacy-view.md
│   ├── decisions/                    # 仅记录确实发生的设计权衡
│   └── design/reference-old-view-reader.png
├── src/
│   ├── main.rs                       # 唯一 composition root
│   ├── cli.rs                        # 参数解析：view/export/version
│   ├── config.rs                     # 本工具配置；不定义语言语义
│   ├── core_client.rs                # 唯一 MARQDO_BIN 运行入口
│   ├── protocol/
│   │   ├── analysis_v2.rs            # 公共 JSON schema / 校验
│   │   ├── trace_v2.rs               # 公共 JSON schema / 校验
│   │   └── tool_v1.rs
│   ├── reader/
│   │   ├── source.rs                 # 受控文件读取与 byte span
│   │   ├── markdown.rs               # 单一 GFM 渲染器；无语义执行
│   │   ├── model.rs                  # PresentationModel
│   │   ├── assemble.rs               # 静态/运行产物合并与过期判定
│   │   └── render.rs                 # 唯一 HTML 渲染入口
│   ├── execution/
│   │   ├── request.rs                # 目标、参数、权限验证
│   │   ├── controller.rs             # Job 生命周期与并发
│   │   ├── process.rs                # 可取消 CLI 子进程与输出限制
│   │   └── events.rs                 # SSE 帧、重放缓冲、状态机
│   ├── server/
│   │   ├── mod.rs                    # 本地单用户 HTTP
│   │   ├── routes.rs                 # 固定最小路由清单
│   │   ├── security.rs               # Origin/Host/CSRF/Cookie
│   │   └── sse.rs                    # 单向长连接，不含业务引擎
│   └── export.rs                     # 静态导出：仅 Reader，不含执行模块
├── assets/
│   ├── reader.css                    # 唯一最终 CSS 入口
│   ├── reader.js                     # 唯一最终浏览器 JS 入口
│   ├── logo.svg / blossom.webp / feather.webp
│   └── LICENSES.md
├── examples/                          # 全部 3.0 新语法，不能复制旧代码
│   ├── welcome.mq.md
│   ├── branch-loop.mq.md
│   ├── function-params.mq.md
│   ├── knowledge-only.mq.md
│   └── unsafe-side-effect.mq.md
├── tests/
│   ├── contract/                     # 冻结 CLI 发布二进制黑盒
│   ├── integration/                  # 独立 Tool install/dispatch + SSE
│   ├── security/                     # 无意外执行、恶意路径/HTML/授权
│   ├── browser/                      # screenshot、键盘、SSE reconnect
│   ├── fixtures/                     # Analysis/Trace 固定有效/无效样例
│   └── golden/                       # HTML / 屏幕宽度视觉基线
├── scripts/
│   ├── package.sh                    # 生成 tool.json+bin+assets 的不可变包
│   └── verify-frozen-cli.sh          # 核心 SHA 前后必须相同
└── .github/workflows/ci.yml
```

不允许以“可扩展”为理由先拆 5～10 个独立 Rust crate；如未来存在真实复用需求，先给出衡量结果和 ADR。`protocol/` 表示 View 消费的公共格式，不是重新实现公共协议的发布源。`assets/reader.css` 与 `reader.js` 各只有一个发布入口；构建期模块化可以存在，但必须明确、可追踪地合并，不能通过多个历史 stylesheet 的偶然顺序覆盖。

### 5.4 唯一渲染流水线

```text
SourceDocument
    ├─> Markdown renderer ──────────────────────┐
    ├─> AnalysisClient.check ─> AnalysisArtifact ├─> assemble ─> PresentationModel
    └─> optional Verified ExecutionRecord ──────┘                  │
                                                                 ▼
                                            render(model, RenderMode::Live | Export)
```

`RenderMode::Export` 类型中**没有** `ExecutionController`、`CliProcessAdapter` 或 HTTP client：从编译依赖结构就不能执行用户代码。Live 页面首次装载使用同一个 renderer，但可以接收后续执行记录；Renderer 仅更新运行区与关联锚点，不重新创建整个语义树。源文档变更导致模型旧状态失效，应清理观察性标注，不能把上一轮运行的关联硬套到新源码。

---

## 6. 交互式执行：SSE 是必需传输协议，但不是 Token/VM 事件系统

### 6.1 采用 SSE 而非轮询的原因及严格边界

- 浏览器需要及时接收任务开始、运行中、完成、失败、超时、取消和已验证 Trace 就绪状态；以固定频率 GET 轮询会制造空请求、延迟与脆弱的竞争状态。因此本产品**禁止任何周期性状态轮询 API**。
- SSE 是浏览器内置的单向长连接，能自动重连并携带 `Last-Event-ID`；比此场景下双向 WebSocket 更简单，且不需要复制旧 EventBus。
- **SSE 只保证“View 一旦知道状态变化，就立即推送”。** 3.0.0-beta.1 的 `run/call --json` 在结束时才产生结构化结果，Trace v2 也在运行后写出。View 可以实时知道**进程已启动、仍运行、收到取消、已退出**；不可承诺每条 stdout token、内部函数调用、每次赋值立刻推送。
- 第一版不实现增量 `stdout.chunk`：为得到可靠的 `value`/`error`，CLI 使用 `--json`，完整 JSON 要在退出时解析。不要为了边跑边展示 stdout 改用文本模式，却丢弃结构化返回值，更不可偷偷双重执行获取两份结果。
- 将来 CLI 有新公开流协议时，只在 `CoreClient` adapter 增加可信事件来源和版本协商，不改变 Viewer 的事件模型或通过私有 crate 绕过协议。

### 6.2 最小 HTTP API（MVP 强制；其他路由需 ADR）

| 方法和路径 | 唯一职责 | 允许执行？ |
|---|---|---|
| `GET /` | 主页/默认文档 | 否 |
| `GET /file?path=...` | 受控路径的单文件阅读 | 否 |
| `GET /assets/...` | 固定打包 CSS/JS/图片，无任意路径映射 | 否 |
| `GET /api/documents` | 搜索/目录/文档结构（只读） | 否 |
| `POST /api/runs` | 验证用户明确提交的执行请求并建立任务 | **是，唯一启动入口** |
| `GET /api/runs/{run_id}/events` | SSE 推送此任务生命周期与终态数据；断线重放 | 否（仅观察） |
| `POST /api/runs/{run_id}/cancel` | 幂等地请求取消；不启动其他进程 | 否（控制已启动任务） |

不建立 `GET /api/run?command=...`、`POST /api/eval`、Notebook REST 兼容层、`/api/session/*`、通用 shell 命令执行端点，也不因未来引入 Prove/Knowledge 增加直通 CLI 私有接口。内部 job 查询可以由 Controller 对象直接提供，不代表必须公开 HTTP status-poll 路由。

### 6.3 请求定义：两阶段显式确认，不允许隐式运行

浏览器先由 Analysis 得到候选 H2 Exec 函数签名，打开表单并生成**命令预览**。用户点击“确认执行”才 POST：

```json
{
  "document": "index.mq.md",
  "target": {"kind": "function", "function_id": "<Analysis-v2-中实际完整-function.id>"},
  "parameters": [{"name": "n", "expression": "5"}],
  "analysis_digest": "sha256:<本次 check --emit 字节摘要>",
  "mode": "pure",
  "grants": {"read": [], "write": []},
  "client_request_id": "<browser-generated-opaque-id>"
}
```

`target.kind` 只能为 `entry`（运行 `main`）或 `function`（运行 `call`）；`function` 必须从**当前选中文件的入口模块** Analysis 函数表中选取，不能直接把依赖模块的函数 ID 当作 CLI `call FILE FUNCTION` 可运行目标。`function_id` 必须服务端按 Analysis 精确解析为 CLI 所接受的函数名，不能把客户端字符串原样拼接成任意参数。

`parameters[].expression` 是核心支持的常量表达式源文本，如 `5`、`"小明"`、`[1,2]`；服务器应做长度/字符/参数名/重复检查，**最后由 CLI 自身进行语法和类型验证**。无隐式裸文本转字符串。若复杂类型不具备被证实的可靠转换，首版以“高级常量表达式”原样输入、显示真实 CLI 错误，不做 JSON→Marqdo 文本的猜测转换。

响应 `202 Accepted`：

```json
{
  "run_id": "<server-issued-unguessable-id>",
  "state": "accepted",
  "events_url": "/api/runs/<id>/events",
  "cancel_url": "/api/runs/<id>/cancel"
}
```

如果同一 `client_request_id` 被重复提交且请求体摘要相同，返回同一个 `run_id`；同 ID 不同 payload 一律 `409 Conflict`。这避免双击按钮、浏览器重试造成二次执行。对已过期 ID 返回明确错误，不能默认再执行。

**在接受前的服务端检查顺序**：会话 Cookie/CSRF/Origin/Host → 大小与 JSON schema → 文档真实路径 containment → 重新静态 `check`（只读）→ Analysis SHA 比对 → 精确函数/参数解析 → 允许的模式及 grants → 同时运行数预算 → 创建任务并入队 → 202。任一失败不得启动用户程序，也不得为错误请求生成“成功执行”的 UI。

### 6.4 SSE 帧合同（本仓库自有协议 `marqdo.view-run-events` v1）

`GET /api/runs/{id}/events` 使用 `Content-Type: text/event-stream; charset=utf-8`、`Cache-Control: no-store`、`X-Accel-Buffering: no`，不启用跨域共享，也不允许缓存或压缩缓冲导致延迟。每帧：

```text
event: run.state
id: 2
data: {"schema":"marqdo.view-run-events","schema_version":1,"run_id":"<id>","seq":2,"state":"running","at":"2026-11-22T00:00:00Z"}

```

`id` 是**任务内严格递增整数**，与 `data.seq` 一致；`run_id` 是全局随机 ID。事件为 `run.state`、`run.complete`、`run.error`、`run.snapshot` 四种（所有数据按下表定义），另外心跳为 SSE 注释 `: heartbeat`，不占用事件序号：

| SSE `event` | data 必需内容 | 触发条件 |
|---|---|---|
| `run.state` | `schema/schema_version/run_id/seq/state/at` | `accepted`、`running`、`cancel_requested` |
| `run.complete` | 通用字段 + `state:"succeeded"` + `result` + `trace` 验证结果 | 子进程成功退出且 JSON 合法；Trace 仅在校验后附上引用或安全摘要 |
| `run.error` | 通用字段 + `state` + `error {code,message}` + 可选安全 `result` | `failed`、`cancelled`、`timed_out`、`unavailable` 等终态 |
| `run.snapshot` | 通用字段 + 当前权威 `state` + 可用的安全结果 | SSE 重连请求的历史事件已被截断时进行状态同步 |

对于 `succeeded`，若 Trace 缺失/无效但 `--json` 成功，**不得伪装 Trace 存在**。可标记结果成功、Trace `unavailable` 或 `invalid`（根据当前协议要求是否应将缺失 Trace 视作整体任务失败，在测试中固定）；源摘要不匹配不得在原文上画执行高亮。`run.complete` 的 `result` 包括已解析的 JSON 返回值与裁剪后的 stdout/stderr；**前端只按文本节点展示**，绝不执行数据里的 HTML/JS。

建议每 15 秒写 `: heartbeat\n\n` 保持中间连接存活；心跳**不是轮询**，不查询 Core 运行状态、不发起新命令。SSE 实时推送是服务端对已有进程状态的响应，前端 `EventSource` 不调用定时器反复请求状态。服务端状态转移必须先写 Job 状态和环形缓冲，再通知 SSE 订阅者；即使客户端暂时没有连接，执行完成事件仍可重放。

### 6.5 SSE 断线重连、缓存和背压

- 内存中每个 job 保留最近 **128 个状态事件**（预计正常任务远少于该数量），并保留独立的最终结果快照；终态保留建议 15 分钟，超过后安全删除。
- 允许 `Last-Event-ID`：仅在同一 job 下按 id/seq 重放 `> last_id` 的事件；若客户端游标太旧，直接发送最新 `run.snapshot`，不得发送不连续的伪历史。新的连接不提供游标时先发送当前状态快照，再订阅后续事件，要求“读取快照+注册订阅”在同一 Controller 锁中原子化，避免竞态漏事件。
- 每任务**最多 2 个并发 SSE 连接**；超过限制返回 `429`。单个发送队列有界，慢客户端被断开而不阻塞 CLI 子进程；重连后可获取最终状态。
- 客户端对 `seq` 去重、拒绝任务 ID 不匹配、忽略旧序号；终态后主动关闭 EventSource，不再自动重连。页面导航/关闭仅断开观察通道，**不自动取消正在运行的外部进程**，除非显式用户操作。
- 禁止无限历史、没有 TTL 的 Job map、向浏览器公开所有工作区运行历史，也禁止 SSE 未鉴权读取终态结果。

### 6.6 任务状态机：所有终态一次且仅一次

```text
 accepted ────────> running ────────────────> succeeded
    │                  │  └───────────────> failed
    │                  ├──cancel_requested─> cancelled
    │                  └───────────────────> timed_out
    └─(无法启动)─────────────────────────────> unavailable
```

`cancel_requested` 不是终态；竞态中若进程已成功退出，允许最终落到 `succeeded`，但不能同时发出 `cancelled` 和 `succeeded` 两个终态。任何 terminal 事件必须有唯一 `run_id`/`seq`，并由单一 `ExecutionController` 负责终态转移。

第一版**每个 View 服务进程最多同时运行 1 个用户 CLI 任务**，可拒绝第二任务 `409 run.busy`，而非暗中无限排队。上限是刻意简化，并非语言执行引擎的能力限制。接受后须在后台执行 CLI 子进程，HTTP handler 不得同步阻塞 SSE 服务线程。

### 6.7 可取消的 CLI 子进程与执行预算

由 `CoreClient` 按参数数组调用，不经过 shell：

```text
MARQDO_BIN run <canonical_entry_path>
    --mode pure --json --trace-file <private_trace_path>
    --timeout 30 --fuel 1000000 --max-output 1048576
    --trace-max-events 10000 --trace-max-bytes 4194304
```

单函数：`MARQDO_BIN call <canonical_entry_path> <function_name> --arg n=5 ...`。运行时 cwd 应设置为已确认的项目根目录，**不允许客户端传入 cwd、任意二进制路径、额外 CLI flags 或 shell 文本**。每次执行都产生独立、权限最小的系统临时目录；Analysis、Trace、结果日志和任何抽取文件均在此目录内，结束后按 TTL 删除；不得在项目文件夹中留下 `.view-*` 运行产物。

- `--timeout` 是 Core 内的运行限制；View 另设**独立墙钟上限**，不依赖 Core 自愿终止。默认建议 30 秒，最大 120 秒，且两者配置须明确一致或 View 更严格。
- 最大 JSON 响应读取 4 MiB，最多 10000 条 Trace 事件、Trace 文件 4 MiB；超限停止接收并报 `view.output_limit`，不得无界缓冲或 `stdout` 管道死锁。`stderr` 同样有界，不能因小管道无人读而阻塞子进程。
- Linux 优先用新进程组执行和取消：先发温和终止信号，约 2 秒后仍存活则强制结束进程组；父进程 wait/reap 并回收句柄。不能只 kill shell（因为**不使用 shell**）或仅移除 UI Job。Windows/macOS 不在当前 Beta 发布支持声明中，可对应平台独立实现或明确 fail-closed。
- 取消本身要鉴权，与启动路由相同的会话作用域；重复取消幂等。Server 进程异常退出时应有子进程清理策略（Linux 进程组/watchdog；需要黑盒验证），不得遗留孤儿长任务。
- **纯模式默认**。用户要在首版 `project` 中使用 read/write，必须再次确认每个相对项目根的 glob，并执行白名单验证后映射为明确 `--allow-read`/`--allow-write`；禁止从 `marqdo.yaml` 自动把“请求”提升成“授权”。`strict`、Foreign/Plugin/Provider、网络、进程、环境授权和 Native 信任按钮全部不在首版 UI 中。

### 6.8 SSE 安全模型：本地服务也必须保护执行端点

SSE `EventSource` 默认不能附加自定义认证头，因此推荐以本地会话 Cookie 进行 SSE GET 授权：`HttpOnly; SameSite=Strict; Path=/`，且绑定随机会话 ID；当服务基于 HTTPS 时加 `Secure`，本地 loopback HTTP 不允许假冒 HTTPS。初次打开页面时服务端发放随机 session 与防 CSRF 值；POST 请求要求自定义 `X-View-CSRF` 与同源 `Origin`、合法 `Host`。`GET /events` 仅允许已认证的会话读取**本会话**启动的 `run_id`。拒绝跨站 CORS、拒绝 query token 泄漏、禁止将密钥写入 SSE URL。

- 默认监听 **`127.0.0.1`，随机可用端口或用户指定本机端口**；首版不支持 `0.0.0.0` 公网监听，也不提供无认证远程执行模式。
- HTTP 读取与 POST 设置严格体积上限（建议 64 KiB 以内），只接受 `application/json`，拒绝浏览器自动表单造成的跨站请求；`Origin` 不匹配直接拒绝，`Sec-Fetch-Site` 可作为额外信号而非唯一保障。
- Web 页面上不加载 CDN、不启用任意前端脚本、Markdown HTML 必须经过安全策略处理。服务端 `Content-Security-Policy` 建议 `default-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'`；如 JS/CSS 内联，须使用 nonce/hash 而非退回全局 `unsafe-inline`。
- `file?path` 与 `document` 必须规范化并检查真实路径在受控根之下；拒绝 `..`、绝对路径、符号链接越界、URL 编码双重解码逃逸；Markdown 的链接只能用于正常导航，不能成为任意本地文件读取 API。
- **Tool 自身是用户启动的普通程序**，`project`/`pure`/Worker 不是对不可信源代码的 OS 级沙箱。任何 UI 字样均不得把显式运行描述为“安全运行”。

---

## 7. 运行结果如何返回同一文档，而非变成 Notebook

### 7.1 前端最小状态

```text
ReaderState:
  selected_document
  view_mode = Document | Source
  active_tab = Document | Functions | Run
  display_settings = Theme + FontSize + ShowAnnotations
  current_run_id?          # 仅用户发起的当前任务
  current_run_state?       # SSE 驱动，不做周期轮询
  current_result?          # 安全的终态视图
```

刷新页面先做只读分析。先前任务如果尚有服务端记录，只能在本地会话明确选择“恢复查看结果”后连接 SSE；**绝不因为重新加载页面自动发起 POST**。浏览器 localStorage 只能保存阅读偏好，不持久化密钥或危险授权。普通运行结果默认内存保留，首版不实现数据库或跨浏览器持久执行历史。

### 7.2 `--json` 结果到 UI 的可验证映射

| CLI 数据 | 页面展示 | 禁止行为 |
|---|---|---|
| `success: true` | 成功状态与耗时/返回类型（可知时） | 将成功等同于数学证明 |
| `value` | 按 Wire Value 安全展示；未知类型折叠为 JSON | 用 `value` 伪造所有变量值 |
| `stdout` | 转义后的普通文本输出 | 当 HTML 执行 |
| `stderr` | 错误/信息区，裁剪后可复制 | 当作可信 HTML / 自动脚本 |
| `error` / `diagnostics` | 代码、错误信息、源范围，帮助定位 | 从缺少字段猜测成功 |
| `Trace v2` | 经过 digest 关联的调用、分支、循环、effect | 展示不存在的每步变量/参数快照 |
| `trace.truncated` | “本次轨迹不完整”提示 | 宣称全部路径已观察 |
| `analysis_digest` 不符 | “结果与当前源码不一致” | 把事件侧注映射到当前源码 |

成功与失败的终态应在原文附近的独立轻量执行面板展现，不能为了日志把读者踢到另一套 IDE 页面。默认可见最重要的一行状态与返回值；点击展开stdout、stderr、轨迹。静态导出不包含执行控制按钮；若未来允许快照导出，应另立契约并清楚标“历史快照、来源及时间”，**不属于 0002 必需范围**。

### 7.3 精确源位置映射

View 渲染原始 Markdown，但只能用 Analysis/Trace 产生语义标识。对有 span 的事件：

1. 检查 `module_id` 是该 Analysis 的 `modules[]` 成员，且所用 source 是此模块的真实源码；
2. 检查 `start <= end <= raw_utf8.len()`、UTF-8 边界合法；
3. 验证模块 `source_digest` 是当前真实 source bytes 的 SHA-256；
4. 验证 Trace 整体 `source.analysis_digest`/`root_id`；
5. 根据字节 span 关联 Markdown 渲染节点，**不要用行号猜匹配**；
6. 无法精确定位时退化为结构化事件列表，绝不随意在最相近的文本画连接线。

如果读取不到依赖包的真实源码，仍可展示模块 ID 和事件列表，但不能把依赖包事件画到入口文件的正文里。样例覆盖中文、Emoji、组合 Unicode、CRLF、不同换行、源文件热修改和同名函数的模块歧义。

### 7.4 侧注规则与渐进降级

先渲染完整阅读文档，再叠加来源明确的最小侧注：`StaticFunction`（静态函数声明）、`ObservedCall`、`ObservedBranch`、`ObservedLoop`、`ObservedEffect`。每类标注带 `source=analysis|trace`，实际未运行时只能展示静态信息。显示 CSS/SVG 弯曲引出线时以锚点和注释实际矩形计算，避开文字；若容纳空间不足、打印、禁 JS 或来源匹配失败，改成正文下有编号的短注释。**先实现准确关联，再追求曲线审美**；禁止复活旧 `reader.js` 里整套 Notebook 状态监听器。

---

## 8. 具体开发里程碑：逐 PR、文件、测试和退出条件

### PR-00：冻结来源、证据与旧 View 清理表

**输入**：本 RFC、旧截图、两个指定仓库源码、官方 `v3.0.0-beta.1` release archive。

**工作**：创建 `docs/audits/legacy-view.md`；记录真实 HTML 主入口、reader CSS 最终覆盖、旧 notebook/debug CSS 冲突、运行副作用；给每个图片/样式片段写迁移裁决。记录 CLI 二进制版本、SHA、平台、实际 `--help` 和协议。创建独立仓库骨架，**不得引用旧源码为 Git 依赖**。

**通过**：证据文件列出 `src/view/html.rs`、`reader.css`、`reader.js`、`debug_page.rs` 的具体入口和 CSS 优先级；未经浏览器复现的旧行为标记 UNVERIFIED；截图独立保留。没有任何 UI 实现前即可执行 `cargo metadata`，并确认依赖图不含核心私有 crate。

### PR-01：Tool v1 的独立打包、安装和调度

**工作**：Rust binary、CLI 参数和 `tool.json`；`scripts/package.sh` 创建包含真实执行文件的发布目录；支持 `marqdo view --help`、`marqdo view FILE`、`marqdo view export PATH --out DIR`（后者可先建立拒绝的占位命令，但不得向用户声称已可用）。写一个无需改 Core 的黑盒注册测试。

**通过**：冻结 `MARQDO_BIN` 的 SHA 前后相同；`marqdo tool install ./dist/.../tool.json` 成功；运行 `marqdo view --help` 分发到外部程序；篡改 assets 或 executable 后 Core 拒绝分发；卸载成功。仓库不添加 `Command::View`、不编译 Core。

### PR-02：只读文档面与准确 Analysis v2

**工作**：`reader/source.rs` 的路径/UTF-8/源摘要安全、`protocol/analysis_v2.rs`、`core_client.check`、`reader/markdown.rs`、`reader/assemble.rs`、`reader/render.rs`；通过只读页面显示 Markdown/GFM、公式、H2 Exec 函数、Knowledge、Analysis diagnostics 和源代码视图。创建一套唯一 `reader.css`、`reader.js`；旧截图作为视觉参考但不能成为第二 DOM 模板。

**通过**：开页 10 次没有任意被标记的程序副作用；脚本无法执行来自 Markdown 的 HTML；语义错误能读原文与诊断；同 SHA Analysis 稳定、不同源码 stale 标记正确；中文跨度无错位；浅/深色和移动端可读；不出现变量/内存/调试占位 tab。

### PR-03：交互式调用表单与执行策略

**工作**：`execution/request.rs`、`core_client` 的 `run/call` argv 构造、`execution/controller.rs`；参数来自 Analysis，带命令预览和显式权限确认。实现纯模式和受控 project fs grants。每任务专用临时目录、并发上限、独立超时、完整 `--json` 解析。

**通过**：能调用单函数、带 Int/Text/List 常量值，并得到真正 Wire Value；无 main 不启动意外函数；没有授权无法访问读取/写入效果；已声明第三方 extension 的 pure 请求明确拒绝，不自动升级模式；点击其他 UI 控件不会运行；恶意参数不能进行 shell 注入。

### PR-04：**SSE 任务生命周期、取消和终态结果（MVP 不可省略）**

**工作**：`server/routes.rs` 最小 7 个路由；`execution/events.rs` 的有界 Job 事件重放；`server/sse.rs` 以 EventSource 支持即时状态与终态推送；`execution/process.rs` 进程组取消、超时、JSON 大小预算。实施同源 Cookie + Origin/CSRF 并锁定 loopback。

**通过**：POST 返回 202 与 run_id；SSE 不靠定时轮询就接收到 accepted→running→terminal；不管执行耗时多久都能收到心跳；浏览器断线后 `Last-Event-ID` 重放、事件去重、终态唯一；取消后无残留子进程；恶意跨站请求不能启动执行；没有 Token 流，不引入旧 EventBus。

### PR-05：Trace v2 证据绑定、原文标注与浏览器回归

**工作**：`protocol/trace_v2.rs`、`reader/assemble.rs` 把真实 run 和 trace 联系起来；实现函数/分支/循环/副作用侧注和降级。运行表单/结果面板/时间线清楚区分静态与动态，完整简洁的截图视觉迭代。测试窄屏/打印/无 JS/断网。

**通过**：源摘要改变或 Analysis/Trace 不匹配不会画本次执行连接线；Truncated 正确显示；不显示未公开的参数值或逐语句变量值；截图与参考的视觉结构接近，但在已批准的“文档/函数/运行”简化导航下不要求完全像素相同；CSS 唯一入口和无远程 CDN 均满足。

### PR-06：静态导出、发布包、全项目回归和文档闭环

**工作**：`export.rs` 调用纯 Reader 模型，生成离线相对链接、单页入口及各文档页，默认绝不执行；提供 README、使用指南、受支持版本/平台、安全限制。CI 使用冻结发布 CLI 进行正向/负向测试，输出双端 SHA 和平台证明。

**通过**：导出前后程序副作用标记不存在；离线打开所有 HTML 不请求外网；从不同目录执行发行工具无需 Core checkout；取消/超时/冲突/断流恢复/恶意路径安全测试绿；文档提到的命令全可黑盒运行；没有“旧版兼容”或伪造功能。

### 8.1 必须固定的开发原则

- 每一个 PR 只能引入本阶段必要层；不允许提前创建未来 Proof/Knowledge/Lint/Notebook 目录；不存在“先占位、以后实现”的按钮。
- `PR-04` 的 SSE 是**首版必须通过的产品合同**，不是 UI 优化可选项；若当前 CLI 尚不支持内部实时 event，仍能依靠子进程生命周期正确完成 SSE。
- 首个发布版本不以旧 `marqdo` 的 `view` 运行结果作为语言语义金标准；它只能用于视觉取证。所有计算金标准来自冻结 3.0 CLI。

---

## 9. 验收测试：可直接转为 CI 测试矩阵

### 9.1 正向端到端测试

| ID | 测试动作 | 预期可断言结果 |
|---|---|---|
| E01 | Tool v1 安装/卸载/分发 `view` | 从独立包加载，不需要 Core 源码 |
| E02 | 打开含 `@knowledge` 与 H2 `@exec` 的文件 | 原文、函数、知识单元正确显示；没有启动 run/call |
| E03 | Source/Document 切换 | Source 字节保真；Document Markdown 完整，语义只依 Analysis |
| E04 | 含中文/Emoji 的表格、段落、公式 | 断行和源码 byte-span 对应正确 |
| E05 | 显式 `main` 运行 | 真正 CLI `run --json --trace-file`；Run 区收到返回和 stdout |
| E06 | H2 Exec 的 `n=5` 调用 | CLI `call` 接受 `--arg n=5`，显示真实结果 |
| E07 | 文本/列表常量参数 | 正确封装引号/数组；不插值进 shell |
| E08 | 无 main 的普通库文档 | Doc 仍可读；底部 Run 引导函数选择，不运行第一函数 |
| E09 | 执行 if/while 源码 | Trace v2 事件在有来源时能映射，过程图不假装有变量值 |
| E10 | `GET /api/runs/<id>/events` | `text/event-stream`，accepted/running/terminal；没有前端 polling |
| E11 | SSE 断开后携带 Last-Event-ID 重连 | 有序重放、无重复展示；过期缓冲 snapshot 正确 |
| E12 | 一个长任务运行期间 SSE 长连 | 心跳正常、浏览器保持连接；任务不可被 GET 重启 |
| E13 | 取消当前运行 | 进程树终止、唯一终态、资源回收 |
| E14 | 导出静态 HTML | 无执行/无联网；页面间相对链接正确 |
| E15 | 页面刷新/重新打开 | 完全不自动执行，用户无确认不会发生副作用 |
| E16 | 源码在运行后被编辑 | 结果可见但标记 stale，原文上绝无错误高亮 |

### 9.2 必测失败与安全回归

| ID | 输入/故障注入 | 强制响应 |
|---|---|---|
| N01 | 假 Analysis schema v3 / 缺失 modules | 明确 unsupported；不伪造语义或执行成功 |
| N02 | `check --emit` 失败但旧 analysis 文件残留 | 删除/忽略旧文件；文档阅读可安全降级 |
| N03 | Trace digest/root_id/module_id 不匹配 | 不绑定执行侧注，解释 stale/error |
| N04 | UTF-8 边界错误、span 越界 | 安全丢弃标注并报告协议诊断 |
| N05 | 恶意 Markdown `<script>` / `onerror` / `javascript:` | 不执行、不创建恶意事件处理器 |
| N06 | HTTP path `../`、%2e%2e、符号链接出根 | 拒绝，不读取外部文件 |
| N07 | 无 Cookie / 缺 CSRF / Origin 不符的 POST | 403，用户程序绝不启动 |
| N08 | 试图跨站开启 SSE 读取别人的 run | 401/403，无事件或结果泄漏 |
| N09 | 同一个 POST 客户端请求 ID 重复且内容相同 | 同一 run_id；运行仅一次 |
| N10 | 同请求 ID 携带不同执行目标 | 409；不能偷换既有任务 |
| N11 | 正在运行时发起第二任务 | 409 run.busy；不创建第二进程 |
| N12 | SSE 客户端发送过期/伪造 Last-Event-ID | 明确 snapshot 或拒绝，状态一致 |
| N13 | 浏览器 SSE 关闭后又连接 | 不重复启动 Core；结果能重放 |
| N14 | Core 没有 `main`/参数表达式非法 | 真实错误回 UI；无隐式降级 |
| N15 | Core 输出超限/卡住/子进程崩溃 | 限制生效、任务终态明确、pipe 与进程回收 |
| N16 | 被取消的 Core 进程再延迟返回 | 最终态唯一，不出现成功和取消双终态 |
| N17 | project 试图自动获得 net/process/env/plugin grant | 拒绝；必须外部 CLI 明确操作 |
| N18 | Core Binary SHA 或版本与记录不符 | CI 停止，禁止“开发中重编译”隐藏依赖 |
| N19 | 已安装 Tool bundle 被改一个字节 | Core 验证失败，要求重新安装 |
| N20 | `marqdo view export` 处理含写文件语句的文档 | 零源文件写入/网络/命令执行 |
| N21 | 两个文件同名函数、依赖模块有同名函数 | 仅允许实际 entry module 函数，绑定来源精确 |
| N22 | 浏览器后台/多标签 SSE 竞态与清理 | 有界连接数、无内存泄漏或任务重复执行 |

### 9.3 浏览器视觉与可达性门禁

根据参考截图建立视觉金样例：`2048×1133`（源参考）、`1664×920`、`1440×900`、`1024×768`、`768×1024`、`390×844`；同时测长中文、英文、表格、公式、函数过多、无函数、断线事件页和执行失败页。手工或 Playwright 自动断言：

1. 正文保持连续 Markdown；截屏不出现旧 Notebook 深色栈、复杂工具条及虚假的变量/内存面板。
2. 顶部只有 `文档/函数/运行`，主 Dock 五个按钮都可真实触发对应动作；在窄屏下不遮盖正文/返回按钮。
3. Welcome 仅在显式欢迎页出现，不能依据名字前缀凭空注入大型封面。
4. 执行态动态更新经 SSE，断线后状态不会“卡死”；用户能在一次明确请求的上下文查看结果。
5. 标注曲线不压过正文；线条错位时降级为行内编号，不把截图右侧不明黑线当作品牌元素。
6. JS 关闭/导出 HTML 后仍能完整读文档；辅助信息可见，交互控件不伪装可运行。
7. 页载脚本、字体、KaTeX/图片均为受控本地资源；禁外网条件下可用。

### 9.4 冻结 CLI 核心验收脚本（示意，需要新仓库 CI 落地）

```bash
set -euo pipefail
CORE="${MARQDO_BIN:?Set MARQDO_BIN to the RELEASE binary}"
before="$(sha256sum "$CORE" | awk '{print $1}')"
"$CORE" --version
cargo build --release --locked                 # 仅构建 marqdo-view
./scripts/package.sh
"$CORE" tool install ./dist/marqdo-view-package/tool.json
"$CORE" view --help
"$CORE" tool list
cargo test --locked
# browser and server integration tests must run against the installed tool
"$CORE" tool remove view
after="$(sha256sum "$CORE" | awk '{print $1}')"
test "$before" = "$after"
```

`cargo build` 的 Cargo.lock、`Cargo.toml` 依赖图必须不出现 `marqdo-cli` 私有 workspace crate 或从 `cflmy/marqdo` 拷来的解释器。以单独机器只装 release archive、View package 做同样测试；若靠源码 checkout、私有 git path 或环境缓存才能成功，即视为**未解耦**。

---

## 10. 发布门禁：不因 UI 漂亮掩盖架构失败

**BETA-CANDIDATE 的强制条件**：

1. 完成 PR-00～06、所有 P0 检验；没有只实现视觉而缺少执行控制或 SSE 的伪交付。
2. `marqdo view` 通过 Tool v1 的独立安装；`marqdo-cli` 在这个工作周期中**没有任何针对 View 的核心提交**，测试 Core hash 前后保持一致。
3. 打开/刷新/导出零副作用；每次执行均来自用户主动提交的 POST；Cross-origin/CSRF 测试必须全部通过。
4. SSE 支持即时生命周期、心跳、序号、断线重连及有界重放；**不得以轮询代替**，不得声称内部逐指令实时观测。
5. 真值来自 CLI `--json`/Trace v2、语义来自 Analysis v2、文档来自原始 `.mq.md`。不允许假造变量、内存、证明或 Agent 状态。
6. 同一套 HTML/CSS/JS 处理 Live/Export；旧 CSS 叠层和多套 View 入口不存在。
7. Linux x86_64 上具备可重现的发行包；其它平台可明确不支持。 README、命令帮助、协议范围、安全限制和实际行为一致。
8. 负面测试至少覆盖 9.2 的 N01～N22，失败时不执行、不泄漏、不自动提升权限。

**发布阻塞**：无意外执行保证失败、SSE 跨站读取/执行、错把过期 Trace 绑定当前源码、后台子进程泄漏、CLI 核心不得不为 View 改动、静态导出运行程序、伪造实时结果、双解释器，任一成立均不得发布。

**允许后续版本完善**：细节动画、完整深色模式、复杂大规模文档搜索、多个并发任务、持久历史、跨平台安装器、更高级的资源侧注和高性能渲染。不能以这些未完成事项阻塞首个支持范围准确、功能闭环完整的 View Beta。

---

## 11. 对外扩展策略：信息层可插拔，但没有隐式依赖

- `marqdo-lint`：未来通过版本化文件报告提供风格诊断，View 只负责显示、不实现 lint rule engine。
- `marqdo-knowledge`：未来通过版本化事实/链接/证据报告提供知识关系，**不能把 Knowledge 加载当作阅读成功条件**。
- `marqdo-prove`：未来通过独立可重放证明证书和来源身份报告可验证状态；View 不用颜色猜测“已证明”。
- `marqdo` 扩展库：可通过普通项目依赖、外部 CLI 的公开协议开展开发；View 不维护扩展市场、插件 ABI 适配和专有运行时。
- 新报告要求 `schema/schema_version`、明确 `source_digest`、`project.root_id`、`module_id/span`、producer/version；来源不匹配或未安装对应工具时退化为普通文档。不允许为扩展覆盖 Reader 主 DOM、修改 CSS 根 token 或注入任意客户端 JS。

本条是兼容性方向，不意味着目前已存在可读的 Lint/Knowledge/Prove 正式 JSON 协议。相关独立仓库应各自发布契约，并在 View 消费端明确协商；未经发布不得“先写死字段”。

---

## 12. 开发者可以从这里直接开始的待办与定义完成

**从 0 开始**：创建独立 `marqdo-view`；复制本 RFC 和用户提供的参考截图；下载 `marqdo-cli v3.0.0-beta.1` 的 Linux x86_64 发行包并记录 SHA；按 PR-00 建立审计；按 PR-01 打通 Tool 注册；按 PR-02 做出能独立阅读的界面；按 PR-03/04 把 `run/call` 和 SSE 闭环接入；按 PR-05 合并 Trace；按 PR-06 完成导出与发布。

**每次提交必须回答四个问题**：本次改动消费的是哪个已发布的 Core 协议？是否可能触发一次隐式执行？是否引入第二页面/语义/解释器实现？是否有通过冻结 CLI 的黑盒证据？任何一个问题不能回答，停止扩大功能并补证据。

**完成定义（DoD）**：普通用户只安装官方 `marqdo` 和独立 `marqdo-view`，输入 `marqdo view` 即可安全阅读；选定函数、填入参数、明确确认后由 CLI 执行，View 通过 SSE 无轮询地更新开始/完成/失败/取消状态，显示正确且来源可验证的结果与 Trace；静态导出零执行；视觉延续参考图的阅读气质而删除了旧 Notebook/Debugger/Memory/Knowledge/Proof 混合工作台；新仓库没有对两代 Marqdo 源码的编译链接依赖。**只要这些条件有一项没有通过，就不能称 0002 已完成。**


## 13. 可落地的事件客户端、命令与首批 fixture

本章将前述协议落为最少可执行的实现骨架。片段展示**本 RFC 要求新仓库实现的代码**，不是声称 `marqdo-cli` 已内置 `/api/runs` 或 SSE；真实实现需按所选 Rust HTTP 框架补齐错误映射、会话校验与测试。

### 13.1 浏览器端：只提交一次、订阅 SSE，不轮询

```javascript
// assets/reader.js 内的运行交互骨架（唯一前端脚本的一部分）
async function startRun(payload, csrfToken) {
  const res = await fetch('/api/runs', {
    method: 'POST',
    credentials: 'same-origin',
    headers: {'Content-Type': 'application/json', 'X-View-CSRF': csrfToken},
    body: JSON.stringify(payload)
  });
  if (!res.ok) throw new Error(`run rejected (${res.status})`);
  const job = await res.json();
  let lastSequence = -1;
  const events = new EventSource(job.events_url, {withCredentials: true});
  const apply = (ev) => {
    const data = JSON.parse(ev.data);
    if (data.schema !== 'marqdo.view-run-events' || data.schema_version !== 1 ||
        data.run_id !== job.run_id || !Number.isSafeInteger(data.seq) ||
        data.seq <= lastSequence) return;
    lastSequence = data.seq;
    // updateRunView 必须使用 textContent / DOM 文本节点；不可 innerHTML(data.result)。
    updateRunView(data);
    if (['succeeded', 'failed', 'cancelled', 'timed_out', 'unavailable'].includes(data.state)) {
      events.close();
    }
  };
  ['run.state', 'run.complete', 'run.error', 'run.snapshot'].forEach(
    kind => events.addEventListener(kind, apply)
  );
  // 网络断开时 EventSource 自动重连并发送 Last-Event-ID。
  // 此处绝不 setInterval(fetch(.../status))，也不自动重新 POST。
  return {runId: job.run_id, closeObserver: () => events.close()};
}

async function cancelRun(job, csrfToken) {
  const res = await fetch(job.cancel_url, {
    method: 'POST', credentials: 'same-origin',
    headers: {'X-View-CSRF': csrfToken}
  });
  if (!res.ok) throw new Error(`cancel rejected (${res.status})`);
  // SSE 将告知 cancel_requested 和最终 cancelled/其他终态。
}
```

**客户端实现强制说明**：`updateRunView` 必须先通过验证状态枚举、长度上限和字段类型，不能将服务端 JSON 直接当作可执行 DOM。`EventSource.onerror` 表示网络或协议故障，应提示“正在恢复连接/连接不可用”，不能将连接故障直接误判为 Core 执行失败。浏览器重载、SSE 自动重连和手动发起新运行是三个不同动作。取消是显式 POST，和观察连接关闭无关。开发者须在真实浏览器测试 `Last-Event-ID` 的发送行为；不能把“重新建立 SSE”设计成“重新执行任务”。

### 13.2 服务端 SSE 推送器伪代码：锁内订阅，锁外写网络

```text
POST /api/runs:
  validate_session_csrf_origin()
  validate_request_and_static_analysis()
  match deduplicate(client_request_id, exact_request_digest):
    SAME: return 202(existing_job)
    CONFLICT: return 409
    NEW:
      create_job(accepted)
      publish(job, state=accepted)
      spawn_background_executor(job)
      return 202(job)

GET /api/runs/{id}/events:
  authenticate_same_session()
  lock(job):
    cursor = validate_last_event_id()
    replay = events_after(cursor) OR snapshot()
    listener = subscribe_bounded_channel()
  unlock(job)
  for e in replay: sse_send(e)
  while open:
    select:
      channel_event => sse_send(event)
      keepalive => sse_send_comment("heartbeat")
      terminal_state_and_queue_drained => close
      slow_client/error => close_stream_only()

background_executor(job):
  publish(running) only after child process successfully spawned
  read both stdout/stderr concurrently with bounded limits
  on cancel/timeout: terminate_process_group_and_reap()
  parse_json_and_trace_only_after_child_exit()
  compare_original_analysis_digest_and_current_source()
  choose_exactly_one_terminal_state()
  store_terminal_snapshot()
  publish(terminal_event)
```

`GET /events` 的订阅不直接访问 CLI 进程，也不改变它的生命周期。Job 环形缓冲内事件与订阅建立必须原子化，网络写不能持锁；否则高延迟的浏览器连接会堵塞其他任务和执行清理。只有已经鉴权的会话能拿到状态与结果，且 GET 请求从不改变 `accepted→running` 之外的任何状态。

### 13.3 两个可直接用于 3.0 的最小语法 fixture

`examples/function-params.mq.md`：

```markdown
# @knowledge 计算示例

这是一个可读取、可独立调用的函数。未点击运行时不能发生计算副作用。

## @exec double

`params(n: Int)`

`return n * 2`
```

可作为对照的 Core 命令：

```bash
"$MARQDO_BIN" check examples/function-params.mq.md --emit /tmp/view-analysis.json
"$MARQDO_BIN" call examples/function-params.mq.md double \
  --arg n=5 --mode pure --json --trace-file /tmp/view-trace.json
```

`examples/branch-loop.mq.md`：

```markdown
# @knowledge 分支与循环

## @exec main

`n = 3`

**while n > 0**

`n = n - 1`

**end**

*return n*
```

本文件只使用 3.0 的 `while` 与 `end` 关键字，不以 `-` 列表作为循环语法；`*return n*` 的执行身份来自 `return` 文本而不是斜体本身。**开发者必须先用实际冻结 CLI check 这两个 fixture，记录编译及运行结果，才能把它们当作验收金样例。** 它们展示规范目标，不构成未经运行的通过声明。

### 13.4 只读与写入负例 fixture

`examples/unsafe-side-effect.mq.md` 应使用**目标 CLI 真正支持的写资源 API**或一个由测试 harness 明确授权的可信插件，写入测试项目之外的哨兵文件；具体语法必须以已冻结 CLI 的公开标准库为准，不能从 2.x 复制旧语法。测试脚本在以下操作前后验证哨兵、项目文件和锁文件摘要一致：

```text
marqdo view unsafe-side-effect.mq.md --no-open
browser GET /
browser GET /file?path=unsafe-side-effect.mq.md
browser GET /api/documents
marqdo view export . --out ./site
browser reload / document switch / search / print
```

在没有用户显式 POST 之前，任何文件系统写入或相关被测试目标的外部副作用都是严重故障；测试 fixture 如因 Core 版本无相应资源 API 而无法构造，应在测试报告中清楚记 `UNVERIFIED`，不能假装通过。

### 13.5 如何确保一个“简单 SSE”不会再膨胀成框架

该设计**不提供** Server-Sent Token、`execution.step`、状态订阅主题总线、任意多端协同编辑、调用帧变量快照、工作流调度、持久队列、SSE→WebSocket 兼容代理、插件自定义事件频道。SSE 服务与 Job Controller 仅处理一次用户确认的 CLI 子进程和四种 View 自有事件类型。若开发者需要增加一种事件，必须先说明：它来自哪个公开的 CLI 事实；是否存在可复现的测试；是否会泄露用户数据；是否能直接通过已有事件表达。没有具体需求则保持原合同不变。

---

## 14. 本 RFC 的优先级与变更约束

- 本文的 **SSE 必需条件** 替代早期讨论中的轮询设计；文档里的“禁止 Token 流”不等于“禁止 SSE”，两者是不同层级。
- 本文在执行控制、权限、默认只读、产品裁剪、UI 路由和交互流程方面优先于 `RFC-VIEW-0001`；0001 的视觉和源阅读原则只在不冲突时继续适用。
- 实现可依据黑盒测试调整具体 Rust 框架、内部结构与 CSS 细节，但**不得单方面更改**公开 Tool/Analysis/Trace 版本、默认不执行、强制显式授权、单渲染链、SSE 无轮询和 CLI 冻结验收要求。
- 任何需要修改 `marqdo-cli` 内核才能完成的功能，应**从本 View 版本范围移除**并在缺口记录中明确说明，而不是直接修改 Core；未来跨仓协议扩展需独立 RFC 和兼容性门禁。
