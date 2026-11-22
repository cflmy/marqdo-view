# marqdo-view

Marqdo View 是一个独立的可执行文档阅读器：以**原始 Markdown 为阅读主体**，把冻结 `marqdo` CLI 的静态语义（Analysis v2）与用户显式触发的真实运行轨迹（Trace v2）附着回原文，而不是把文档翻译成另一套语言。

- 设计总规范：[`docs/RFC-VIEW-0001.md`](docs/RFC-VIEW-0001.md)
- 实际实施状态（唯一现状来源）：[`docs/STATUS.md`](docs/STATUS.md)
- 旧 View 取证审计：[`docs/audits/legacy-render-paths.md`](docs/audits/legacy-render-paths.md)
- 关键决策：[ADR-0001](docs/ADR/0001-single-package-single-pipeline.md) / [ADR-0002](docs/ADR/0002-embedded-assets-tool-protocol.md)
- 视觉基准：`docs/design/reference-old-view-reader.png` 与 [`docs/design/visual-tokens.md`](docs/design/visual-tokens.md)

## 当前状态

**开发中，尚未可运行。** PR-00（基线、旧 View 静态取证、视觉/状态设计基准）已完成；后续按 PR-01→PR-06 交付 Tool Protocol v1 安装闭环、只读浏览、Reader 视觉、Trace 可视化、Live/Export 安全与端到端验收。任何未完成能力都必须在 `docs/STATUS.md` 与 CHANGELOG 明确标注，不以占位功能冒充已实现。

## 架构约束（摘要）

```text
Browser / static HTML
      │
Reader UI（唯一 reader.css + reader.js）
      │
ReaderRenderer（唯一模板）
      │
PresentationModel（唯一模型） ── DocumentModel / SemanticProjection / RunProjection
      │
PublicCoreClient ── MARQDO_BIN 子进程 + 公开 JSON（check --emit / run --trace-file）
      │
冻结发布版 marqdo CLI（不修改、不重编译、不链接）
```

- 单一 Rust package、单一渲染入口、单一主题文件；Live 与 Export 共用同一份渲染字节。
- 不依赖旧 `marqdo` 仓库或 `marqdo-cli` 的私有 crate/源码；仅通过 `MARQDO_BIN` 与公开协议交互。
- 默认纯只读、不自动执行；执行必须由用户在 UI 中显式确认，最低权限首选 `--mode pure`。
- Trace v2 不提供逐语句变量值/内存快照；新 View 一律标 `UNSUPPORTED`，绝不伪造。

## 开发

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

安装到冻结 CLI（开发期）：

```bash
MARQDO_BIN=/absolute/path/to/marqdo
./tools/package.sh
"$MARQDO_BIN" tool install "$(pwd)/dist/marqdo-view/tool.json"
"$MARQDO_BIN" view --help
```

完整安装/验收步骤见 RFC §18，脚本位于 `tools/`。

## 许可

代码按 MIT / Apache-2.0 双许可发布（见 `LICENSE-MIT`、`LICENSE-APACHE`）。复用的旧 View 视觉资产授权与来源见 `assets/licenses/`。
