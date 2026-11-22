# ADR-0001：单 Package、单管线、单一 ReaderRenderer

- 状态：Accepted
- 日期：2026-11-22
- 关联：RFC-VIEW-0001 §6、§7、§14；`docs/audits/legacy-render-paths.md`

## 背景

旧 View 至少存在三类文档呈现入口（Reader `serve`、Workbench `ViewAction::Document`、Debug `serve_debug`）与三层叠加 CSS（`stylesheet()` → `notebook_ui()` → `reader.css`）。其中深色 Notebook 层通过 `body.reader-page.notebook-page` 覆盖但 body class 从未被设置，属于携带而无效的死层。静态导出路径默认还会执行文档以抓取输出快照。

新 `marqdo-view` 必须在只有发布版 `marqdo` 二进制、没有旧仓库源码的机器上独立构建运行，并且不允许出现第二套渲染/语义管线。

## 决策

1. **单一 Rust package**（`Cargo.toml` 只有一个 `[package]`，无 workspace、无内部 crate）。
2. **库边界**：`src/lib.rs` 暴露模块以便集成测试与未来 server 复用，`src/main.rs` 只是解析参数、调用 `run()`、打印诊断并退出的薄分派器；这不是第二个渲染/解析入口。
3. **唯一依赖方向**：
   `CLI/Server → Application → Domain → ProtocolAdapter`，以及
   `Application → Presentation → Renderer → Assets`。
   `Domain` 不得引用 HTTP 请求；`Renderer` 不得启动子进程；`reader.js` 不得解释 Marqdo 语义。
4. **唯一渲染入口**：`render.rs::ReaderRenderer::render(&PresentationModel, LinkPolicy)`。Live 与 Export 只差 `LinkPolicy` 与控件可用性；`Command` 层不得自己拼 HTML。
5. **唯一呈现模型**：`presentation.rs::PresentationModel { document, semantic, run, overlays, source_status }`，由 `PresentationBuilder` 从**已验证**类型构建；未验证 JSON 不进入渲染端。
6. **唯一前端主题**：`assets/reader.css`（`@layer reset,tokens,layout,typography,semantics,overlays,interactions,responsive`）+ `assets/reader.js`（只操作 `[data-action]`/`[data-source-id]`/`data-*`）。禁止 `legacy.css`/`new.css` 并行、禁止 `!important` 覆盖链。
7. **唯一用户可见命令层**：`cli.rs` 用 clap 解析 `view` 自身的 `browse/export/verify`；`main.rs` 只做分发。
8. **唯一核心交互**：`core_client.rs::PublicCoreClient` 通过 `MARQDO_BIN` 子进程与公开 JSON 交互；`check --emit` 生成 Analysis v2，`run --trace-file` 生成 Trace v2。禁止链接 CLI crate、读取 CLI 源码、`inspect --ir`。

## 后果

- 正面：Live/Export 内容天然一致（同一字节源）；协议校验集中在 `semantic.rs`/`trace.rs`；可对 `PublicCoreClient` 注入假核心做负向测试。
- 负面：首版无法提供旧版的 Notebook/调试面板；这些能力必须以 `UNSUPPORTED` 显式降级，而不是塞第二套 UI。
- 门禁：`tests/contract/` 静态扫描 `Cargo.toml`/`Cargo.lock`/`src/`，拒绝 `marqdo`/`marqdo-cli`/`marqdo_ir` 等依赖与 `../marqdo` 路径读取；拒绝第二个 `render`/`stylesheet` 入口。
