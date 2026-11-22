# marqdo-view

Marqdo View 是一个独立的可执行文档阅读器：以**原始 Markdown 为阅读主体**，把冻结 `marqdo` CLI 的静态语义（Analysis v2）与用户显式触发的真实运行轨迹（Trace v2）附着回原文，而不是把文档翻译成另一套语言。

- 设计总规范：[`docs/RFC-VIEW-0001.md`](docs/RFC-VIEW-0001.md)
- 实际实施状态（唯一现状来源）：[`docs/STATUS.md`](docs/STATUS.md)
- 旧 View 取证审计：[`docs/audits/legacy-render-paths.md`](docs/audits/legacy-render-paths.md)
- 关键决策：[ADR-0001](docs/ADR/0001-single-package-single-pipeline.md) / [ADR-0002](docs/ADR/0002-embedded-assets-tool-protocol.md)
- 视觉基准：`docs/design/reference-old-view-reader.png` 与 [`docs/design/visual-tokens.md`](docs/design/visual-tokens.md)

## 当前状态

**开发中。** PR-00（基线/取证/设计基准）、PR-01（Tool Protocol v1 + verify）、PR-02（DocumentModel/Analysis v2 只读浏览 + 单一渲染器 + 本机 Live 阅读）与 PR-04（显式确认后运行 + Trace v2 可视化）已完成。`marqdo view [PATH]` 提供本机只读浏览；在 Live 页面点击“运行”并确认后，才会调用冻结核心执行 `pure` 模式并展示真实 Trace；`export` 按 PR-05 交付。任何未完成能力都在 `docs/STATUS.md` 与 CHANGELOG 明确标注，不以占位功能冒充已实现。

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

静态导出（纯只读、离线可打开，永不执行文档）：

```bash
marqdo view export examples/welcome --out ./site
# 或：marqdo-view export examples/welcome --out ./site --marqdo-bin /path/to/marqdo
```

本机浏览（默认只绑定 127.0.0.1，不自动执行）：

```bash
MARQDO_BIN=/absolute/path/to/marqdo cargo run -- examples/welcome --no-open
# 或用已安装 bundle：marqdo view examples/welcome --no-open
```

安装到冻结 CLI（开发期）：

```bash
MARQDO_BIN=/absolute/path/to/marqdo
./tools/package.sh
"$MARQDO_BIN" tool install "$(pwd)/dist/marqdo-view/tool.json"
"$MARQDO_BIN" view --help
```

浏览器验收（需要一次性的 Playwright/Chromium 下载）：

```bash
(cd tests/browser && npm install && PLAYWRIGHT_BROWSERS_PATH="$PWD/../.cache/pw-browsers" npx playwright install chromium)
MARQDO_BIN=/absolute/path/to/marqdo ./tools/test-browser.sh
MARQDO_BIN=/absolute/path/to/marqdo ./tools/test-export.sh
./tools/test-security.sh
```

完整安装/验收步骤见 RFC §18，脚本位于 `tools/`。

## 许可

代码按 MIT / Apache-2.0 双许可发布（见 `LICENSE-MIT`、`LICENSE-APACHE`）。复用的旧 View 视觉资产授权与来源见 `assets/licenses/`。
