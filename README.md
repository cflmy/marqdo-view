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

> **当前状态**：RFC-VIEW-0002 PR-00～PR-06 已完成，按 §10 发布门禁验收
> **PASS**（真实冻结 CLI + 真实 Chromium；见
> [`docs/ACCEPTANCE.md`](docs/ACCEPTANCE.md) 与
> `docs/evidence/<CLI-SHA>/manifest.json`）。像素级人工比对与完整
> screen-reader 走查仍为 `UNVERIFIED`，见 `docs/STATUS.md`。

## 能力

| 能力 | 状态 |
|---|---|
| 原始 Markdown + GFM 阅读、源码态、文档态 | ✅ |
| Analysis v2 静态语义（函数/Unit/签名/诊断/导入/引用/资源） | ✅ 严格校验，span 无效则降级 |
| 本机 Live 阅读（默认 `127.0.0.1`，只读 API + 显式执行端点） | ✅ |
| 显式确认后运行 + SSE 状态/取消 + 真实 Trace v2 | ✅ 单任务、无轮询、断线重放 |
| 纯只读静态导出（离线 `file://`、manifest/哈希、原子发布） | ✅ |
| 单函数参数表单 + 受控 `project` fs grants | ✅ 参数来自 Analysis 签名；project 需显式 glob 二次确认 |
| SSE 状态推送 / 取消 / 断线重放（禁止轮询） | ✅ `POST /api/runs` + `GET …/events` + `POST …/cancel` |
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

# RFC §13.3 示例（可检查/调用/运行；无隐式执行）
marqdo view examples/function-params.mq.md --no-open
marqdo view examples/unsafe-side-effect.mq.md --no-open

# 纯只读静态导出（永不执行；导出后可用 file:// 离线打开）
marqdo view export examples/welcome --out ./site
marqdo view export examples/mixed-modes --out ./site --no-annotations
```

开发期可直接运行 `marqdo-view`（用 `--marqdo-bin` 指定冻结核心）。运行只有一个入口：
Live 页面点击 Dock “运行”（执行 `main`）或函数面板的“调用”（执行 `call`），
阅读命令预览与权限说明后点击“确认执行”，服务端才 `POST /api/runs` 启动一个
CLI 子进程；状态与结果通过 SSE 推送，可取消，断线可重放。页面刷新/打开/搜索/
切换/导出都不会运行，也没有状态轮询。

## 架构（强制约束）

```text
Browser / static HTML
      │
Reader UI（唯一 assets/reader.css + assets/reader.js）
      │
ReaderRenderer（唯一模板，Live/Export 共用）
      │
PresentationModel（reader::assemble / reader::model）
      │
CoreClient ─ MARQDO_BIN 子进程 + 公开 JSON（check --emit / run/call --trace-file）
      │
ExecutionController ─ 单任务、取消、SSE 事件（仅 POST /api/runs 可达）
      │
冻结发布版 marqdo CLI（不修改、不重编译、不链接、不读源码）
```

- 单一 Rust package、单一渲染入口、单一主题文件；没有第二个 Markdown→语义管线。
- 服务端只有 7 条固定路由；SSE 只推送 View 自有 `run.*` 生命周期事件。
- Analysis v2 是唯一语义权威：函数/语句/分支/循环/诊断均来自 `check --emit`，
  源码字节 span 只用于绑定视觉位置。
- Trace v2 与 Analysis 字节摘要严格绑定；`STALE/TRUNCATED/FAILED/UNSUPPORTED`
  状态显式呈现，不猜测未公开值。
- Live/Export 内容同源；导出绝不执行、绝不覆盖项目源文件或无关目录。
- 门禁：`tests/contract/no_coupling.rs` 拒绝旧 crate/源码依赖、第二套渲染器/主题、shell 执行；
  `tests/contract/asset_integrity.rs` 锁定内嵌资产与唯一主题层；
  `tests/security/` 覆盖 CSRF/Origin/Cookie/路径/symlink/XSS/跨会话 SSE。

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
MARQDO_BIN=... ./scripts/check-examples.sh  # RFC §13.3 示例真实 check/call/run
MARQDO_BIN=... ./scripts/test-no-exec.sh     # 打开/导出零副作用哨兵
MARQDO_BIN=... ./scripts/test-contract.sh   # code + Tool v1 + verify + export
MARQDO_BIN=... ./scripts/test-export.sh     # 纯只读导出 + file:// Chromium
./scripts/test-security.sh                  # HTTP/路径/XSS/导出对抗
(cd tests/browser && npm install && PLAYWRIGHT_BROWSERS_PATH="$PWD/../.cache/pw-browsers" npx playwright install chromium)
MARQDO_BIN=... ./scripts/test-browser.sh    # 真实 Chromium 视觉/交互/响应式/运行
```

协议 golden 由冻结 CLI 生成，见 `tests/golden/README.md` 与 `scripts/gen-golden.sh`。

## 已知限制（诚实状态）

- 运行仅 `pure`（默认）与受控 `project` fs read/write；`strict`、网络/进程/环境/
  plugin/provider/kernel 授权不在首版 UI，必须使用终端 CLI。
- `project` 不是 OS 沙箱；View 不继承 `marqdo.yaml` 的 capability 请求。
- SSE 只推送 View 生命周期与终态数据，不承诺逐字 stdout/Token 流；Trace v2 不含
  参数实际值/逐语句变量/内存快照。
- 平台：仅 Linux x86_64 正式验证；Windows/macOS 不支持声明。
- 远端 CI 配置已提供（`.github/workflows/ci.yml`），本环境未实际运行远端 CI；
  验收证据来自本地真实命令。
- 像素级人工视觉比对与完整 screen-reader 走查为 `UNVERIFIED`（自动化几何/响应式/
  无外部资源/无 JS/打印证据见 `docs/evidence/PR-04/`）。
