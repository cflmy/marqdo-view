# marqdo-view

Marqdo View（`0.2.0`）是一个独立的可执行文档阅读器：以**原始 Markdown 为阅读主体**，把冻结
`marqdo` CLI 的静态语义（Analysis v2）与用户显式触发的真实运行轨迹（Trace v2）
附着回原文，而不是把文档翻译成另一套语言。

- 状态（唯一现状来源）：[`docs/STATUS.md`](docs/STATUS.md)
- **实施规范（当前）**：[`docs/architecture/RFC-VIEW-0002.md`](docs/architecture/RFC-VIEW-0002.md)
- 0002 合规矩阵：[`docs/CONFORMANCE.md`](docs/CONFORMANCE.md)
- 历史基础设计：[`docs/architecture/RFC-VIEW-0001.md`](docs/architecture/RFC-VIEW-0001.md)
- 旧 View 静态审计：[`docs/audits/legacy-view.md`](docs/audits/legacy-view.md)
- 冻结 CLI 合同审计：[`docs/audits/protocol-contract-0002.md`](docs/audits/protocol-contract-0002.md)
- 兼容性：[`COMPATIBILITY.md`](COMPATIBILITY.md) / 变更：[`CHANGELOG.md`](CHANGELOG.md)
- 决策记录：[`docs/decisions/`](docs/decisions/)

> **当前开发状态**：仓库正按 RFC-VIEW-0002 迭代（PR-00～PR-06）。下方能力表与
> 限制按实际实现更新；未完成项以 `docs/STATUS.md` 为准，不宣称 0002 已完成。

## 能力

| 能力 | 状态 |
|---|---|
| 原始 Markdown + GFM 阅读、源码态、文档态 | ✅ |
| Analysis v2 静态语义（函数/Unit/签名/诊断/导入/引用/资源） | ✅ 严格校验，span 无效则降级 |
| 本机 Live 阅读（默认 `127.0.0.1`，只读 API） | ✅ |
| 用户确认后运行 + 真实 Trace v2 可视化（`pure` 模式） | ✅（0001 同步模型；0002 的 SSE 异步闭环开发中） |
| 纯只读静态导出（离线 `file://`、manifest/哈希、原子发布） | ✅ |
| 单函数参数表单 + 受控 `project` fs grants | 🚧 RFC-VIEW-0002 PR-03/04 |
| SSE 状态推送 / 取消 / 断线重放（禁止轮询） | 🚧 RFC-VIEW-0002 PR-04（MVP 阻塞项） |
| 逐语句变量值 / 参数值 / 内存快照 | ❌ UNSUPPORTED（协议不提供，绝不伪造） |
| 知识“已证明” / Prove / Lint / 调试器 | ❌ 非目标/等待独立协议 |

## 安装

```bash
# 1) 取得官方冻结 CLI 发布包（示例：本地已解包的 3.0.0-beta.1）
MARQDO_BIN=/absolute/path/to/marqdo
"$MARQDO_BIN" --version

# 2) 构建并制作 Tool v1 bundle
./scripts/package.sh
# → dist/marqdo-view/tool.json + dist/marqdo-view/bin/marqdo-view

# 3) 安装为 `marqdo view`
"$MARQDO_BIN" tool install "$(pwd)/dist/marqdo-view/tool.json"
"$MARQDO_BIN" view --help
```

## 使用

```bash
# 只读浏览（不会自动执行任何文档）
marqdo view examples/welcome --no-open
marqdo view examples/control-flow --port 0 --no-open

# 校验 CLI / Analysis / 源码绑定（无执行）
marqdo view verify examples/mixed-modes/index.mq.md
marqdo view verify examples/mixed-modes --refs --json

# 纯只读静态导出（永不执行；导出后可用 file:// 离线打开）
marqdo view export examples/welcome --out ./site
marqdo view export examples/mixed-modes --out ./site --no-annotations
```

开发期可直接运行 `marqdo-view`（用 `--marqdo-bin` 指定冻结核心）。运行入口在 Live
页面中：点击“运行”→ 阅读风险说明 → 点击“确认运行”，才会调用冻结核心执行
`--mode pure`；页面刷新/打开/搜索/导出都不会运行。

## 架构（强制约束）

```text
Browser / static HTML
      │
Reader UI（唯一 assets/reader.css + assets/reader.js）
      │
ReaderRenderer（唯一模板，Live/Export 共用）
      │
PresentationModel（DocumentModel / SemanticProjection / RunProjection）
      │
PublicCoreClient ─ MARQDO_BIN 子进程 + 公开 JSON（check --emit / run --trace-file）
      │
冻结发布版 marqdo CLI（不修改、不重编译、不链接、不读源码）
```

- 单一 Rust package、单一渲染入口、单一主题文件；没有第二个 Markdown→语义管线。
- Analysis v2 是唯一语义权威：函数/语句/分支/循环/诊断均来自 `check --emit`，
  源码字节 span 只用于绑定视觉位置。
- Trace v2 与 Analysis 字节摘要严格绑定；`STALE/TRUNCATED/FAILED/UNSUPPORTED`
  状态显式呈现，不猜测未公开值。
- Live/Export 内容同源；导出绝不执行、绝不覆盖项目源文件或无关目录。
- 门禁：`tests/contract/no_coupling.rs` 拒绝旧 crate/源码依赖、第二套渲染器/主题、shell 执行；
  `tests/contract/asset_integrity.rs` 锁定内嵌资产与唯一主题层。

## 开发与验收

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

全链路（Tool 安装、浏览、显式运行、导出、浏览器、安全、冻结内核 SHA 不变）：

```bash
export MARQDO_BIN=/absolute/path/to/frozen/marqdo
./scripts/acceptance.sh
```

单独套件：

```bash
MARQDO_BIN=... ./scripts/test-contract.sh   # code + Tool v1 + verify + export
MARQDO_BIN=... ./scripts/test-export.sh     # 纯只读导出 + file:// Chromium
./scripts/test-security.sh                  # HTTP/路径/XSS/导出对抗
(cd tests/browser && npm install && PLAYWRIGHT_BROWSERS_PATH="$PWD/../.cache/pw-browsers" npx playwright install chromium)
MARQDO_BIN=... ./scripts/test-browser.sh    # 真实 Chromium 视觉/交互/响应式/运行
```

协议 golden 由冻结 CLI 生成，见 `tests/golden/README.md` 与 `scripts/gen-golden.sh`。

## 已知限制（诚实状态）

- 首版运行只支持 `--mode pure`；`project`/`strict` 及 grants 授权界面未实现。
- 运行完成后整页重载并保留阅读位置；无 SSE 流式输出/取消/多运行历史。
- 变量实际值、参数值、内存快照、知识证明状态不在 Trace v2 中，UI 一律标 UNSUPPORTED。
- 平台仅 Linux x86_64 glibc 正式验证；macOS/Windows 未验证。
- 视觉自动化门禁 PASS，但参考截图的像素级人工比对仍未执行（`U-01 UNVERIFIED`）；
  SVG 曲线侧注未实现，运行事件以可访问的内联 chip 呈现（`U-04 PARTIAL`）。
- 远端正 CI 未实际运行；`.woodpecker.yml` 已提供，本地验收证据完整。

## 许可

代码按 MIT / Apache-2.0 双许可发布（`LICENSE-MIT`、`LICENSE-APACHE`）。复用的旧
View 视觉资产（Apache-2.0）来源与哈希见 `assets/licenses/ASSETS.md`。
