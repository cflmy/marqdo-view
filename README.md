# marqdo-view

Marqdo View（`0.3.0`）是一个独立的可执行文档阅读器：以**原始 Markdown 为阅读主体**，
把冻结 `marqdo` CLI 的当前公开契约——Analysis revision 3（ADR-0069 公共边界）与
Observation revision 1——附着回原文，而不是把文档翻译成另一套语言。

- 状态（唯一现状来源）：[`docs/STATUS.md`](docs/STATUS.md)
- **当前分层与契约**：[`docs/architecture/observation-v1-and-analysis-v3.md`](docs/architecture/observation-v1-and-analysis-v3.md)
- 当前契约决策：[ADR-0012](docs/decisions/0012-rc4-current-contracts.md)
- 完整阅读与结果导出：[实施 RFC](docs/architecture/RFC-VIEW-0001-complete-reader.md) · [设计图](docs/design/complete-reader.png)
- 验收标准与证据：[`docs/ACCEPTANCE.md`](docs/ACCEPTANCE.md) · [`docs/evidence/`](docs/evidence/README.md)
- 旧 0.2.0 基线（历史）：[`docs/architecture/RFC-VIEW-0002.md`](docs/architecture/RFC-VIEW-0002.md)
- 冻结 CLI 合同审计：[`docs/audits/protocol-contract-0002.md`](docs/audits/protocol-contract-0002.md)
- 兼容性：[`COMPATIBILITY.md`](COMPATIBILITY.md) / 变更：[`CHANGELOG.md`](CHANGELOG.md)

> 0.3.0 只支持当前预发布契约：`marqdo 3.0.0-rc.5`（产品范围
> `>=3.0.0-rc.3,<4.0.0`；验收固定 rc.5）。静态语义只有一个来源
> `check --emit`（Analysis revision 3，含精确 source、Markdown AST、heading 作用域、
> anchors 与状态化 references）；运行事实只有一条 Observation revision 1 事件序列
> （`run`/`call --output jsonl`，原子快照 `--observation-file`）。旧格式显式拒绝，
> 没有兼容分支、迁移器或运行时探测。

## 能力（0.3.0 实现范围）

> 下表的“实现”表示当前代码路径按 rc.5 契约实现；`PASS/FAIL/UNVERIFIED` 的机器
> 验收结论只以 [`docs/evidence/`](docs/evidence/README.md) 中绑定 CLI 源码 SHA 的
> manifest 为准。new manifest 生成前，全部发布门禁在 STATUS 中保持 `UNVERIFIED`。

| 能力 | 状态 |
|---|---|
| Markdown + GFM、源码态、项目图片、跨页锚点、离线数学 | 实现 |
| 单一静态源：`check --emit` Analysis revision 3（source/document/units/anchors/functions/calls/resources/diagnostics） | 实现；不读取 `inspect --ast` 作为机器来源 |
| heading_span（标题 UI/目录/锚点）与完整 span（section 高亮/切片/归属） | 实现 |
| 引用状态 resolved / not_loaded / missing / ambiguous / external / outside_scope / invalid | 实现；`not_loaded` 显示“未核验”，不是断链 |
| 本机 Live 阅读（默认 `127.0.0.1`，只读 API + 显式执行端点） | 实现 |
| 显式确认后单次运行/调用；Observation v1 事件经 View SSE 转发/摘要 | 实现（单任务、取消、断线重放） |
| 只读静态导出（离线 `file://`、媒体/字体/哈希）；普通导出与单次结果 ZIP | 实现 |
| 单函数参数表单 + 受控 `project` fs grants | 参数来自 Analysis 签名；project 需显式 glob 二次确认 |
| 变量/参数值观测 | 协议支持 `--observe-variable`/`--observe-parameter`（Text 需 `--observe-text-values`）；**View 默认不请求任何值**，绝不伪造变量值/内存状态 |
| 知识“已证明” / Prove / Lint / 调试器 | 非目标/等待独立协议 |

不变量：

- `TRUNCATED`、传输未完成（EOF 无终态）、`UNSUPPORTED`、`PARTIAL`、`WARN`
  绝不写成成功/PASS；没有终态的流显示“未完成/无结论”。
- 每一条运行结论都绑定**同一次普通 `check --emit`** 的精确字节摘要；
  带 `--refs` 的检查会改变字节，不能作为运行绑定。
- 协议事件只转发/摘要，View 自有 SSE 信封不重编号、不重排、不合成协议事件。

## 安装

```bash
# 1) 取得官方冻结 CLI 发布包（验收固定 3.0.0-rc.5）
MARQDO_BIN=/absolute/path/to/marqdo-3.0.0-rc.5-x86_64-unknown-linux-gnu/marqdo
"$MARQDO_BIN" --version   # marqdo 3.0.0-rc.5

# 2) 构建并制作 CLI Tool revision 1 bundle
./scripts/package.sh
# → dist/marqdo-view/tool.json + dist/marqdo-view/bin/marqdo-view

# 3) 安装为 `marqdo view`（旧安装请先 tool remove 后重装）
"$MARQDO_BIN" tool install "$(pwd)/dist/marqdo-view/tool.json"
"$MARQDO_BIN" view --help
```

`tool.json` 只声明精确要求：`requires {api_revision: 1, analysis: 3, observation: 1}`，
没有 `compatibility` 字段，也没有版本协商或迁移。

## 使用

```bash
# 只读浏览（不会自动执行任何文档）
marqdo view examples/welcome --no-open
marqdo view examples/control-flow --port 0 --no-open

# 校验 CLI / Analysis / 源码绑定（无执行）
marqdo view verify examples/mixed-modes/index.mq.md
marqdo view verify examples/mixed-modes --refs --json

# 显式触发（只有页面确认后才 POST /api/runs）
marqdo view examples/function-params.mq.md --no-open
marqdo view examples/unsafe-side-effect.mq.md --no-open

# 纯只读静态导出（永不执行；导出后可用 file:// 离线打开）
marqdo view export examples/welcome --out ./site
marqdo view export examples/mixed-modes --out ./site --no-annotations
```

开发期可直接运行 `marqdo-view`（用 `--marqdo-bin` 指定冻结核心）。运行只有一个入口：
Live 页面点击 Dock “运行”（执行 `main`）或函数标题旁的“调用”（执行 `call`），
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
CLI 适配层（唯一知道 argv / 环境变量 / 退出码语义）
  ├─ check --emit → Analysis revision 3 精确字节
  └─ run|call --output jsonl [--observation-file] → Observation revision 1 流/快照
      │
解码 + 结构校验（protocol 模块：schema/revision/必需字段/seq/终态/预算）
      │
规范化领域模型（Analysis / Observation / RunBinding；无 CLI 细节、无版本分支）
      │
ExecutionController ─ 单任务、取消、SSE 事件（仅 POST /api/runs 可达）
      │
冻结发布版 marqdo CLI（不修改、不重编译、不链接、不读源码）
```

- 单一 Rust package、单一渲染入口、单一主题文件；没有第二个 Markdown→语义管线。
- 协议模块之外没有 `schema_version` 判断；协议模块内只有当前 revision 一个分支。
- JSONL 流与原子快照共用同一解码器与领域类型；生产不“双读合并”，一致性只在测试中断言。
- Live/Export 内容同源；导出绝不执行、绝不覆盖项目源文件或无关目录。
- 门禁：`tests/contract/no_coupling.rs` 拒绝旧 crate/源码依赖、第二套渲染器/主题、shell 执行；
  `tests/contract/asset_integrity.rs` 锁定内嵌资产与唯一主题层；
  `tests/security/` 覆盖 CSRF/Origin/Cookie/路径/symlink/XSS/跨会话 SSE。

## 开发与验收

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
scripts/check-no-legacy.sh   # G01：当前树 0 条旧契约残留（cmd 可单独跑）
scripts/check-docs.sh        # G12：当前文档不把旧格式写成当前能力
```

全链路验收使用 Rust 1.89 或更新版本，以及能运行官方 rc.5 的 Linux x86_64
环境（glibc 2.39 起）。浏览器、显式运行和离线导出通过已安装的 Tool 执行。

```bash
(cd tests/browser && npm ci && PLAYWRIGHT_BROWSERS_PATH="$PWD/../.cache/pw-browsers" npx playwright install chromium)
MARQDO_BIN="$(./scripts/frozen-beta.sh fetch "$(mktemp -d)")"
export MARQDO_BIN
./scripts/acceptance.sh      # G01–G12，写 docs/evidence/452e9a32…/manifest.json
cat docs/evidence/c44b0f943875d563bb7c170f8ccd3fd56a82360f/manifest.json
```

验收会核对 CLI 源码 SHA、归档 SHA-256 与二进制 SHA-256，并在前后两次记录 CLI 摘要。
未实际运行、无法在本环境判定的门禁记为 `UNVERIFIED`，不会写成 `PASS`。

单独套件：

```bash
MARQDO_BIN=... ./scripts/check-examples.sh   # 示例的真实 check/call/run（Observation v1）
MARQDO_BIN=... ./scripts/test-no-exec.sh      # 打开/导出零副作用哨兵
MARQDO_BIN=... ./scripts/test-contract.sh     # code + Tool revision 1 + verify + export
MARQDO_BIN=... ./scripts/test-export.sh       # 纯只读导出 + file:// Chromium
./scripts/test-security.sh                    # HTTP/路径/XSS/导出对抗
MARQDO_BIN=... ./scripts/test-browser.sh      # 真实 Chromium 视觉/交互/响应式/运行
```

协议 golden 由冻结 CLI 生成，见 `tests/golden/README.md` 与 `scripts/gen-golden.sh`
（Analysis revision 3 + Observation revision 1 流/快照）。

## 已知限制（诚实状态）

- 运行仅 `pure`（默认）与受控 `project` fs read/write；`strict` 只支持 text/JSON
  快照，`--output jsonl` 与 `--observation-file` 在启动前被 CLI 拒绝（exit 2，
  `strict.unsupported_grant`），View 不伪装、不回退到 `project`。
- `project` 不是 OS 沙箱；View 不继承 `marqdo.yaml` 的 capability 请求。
- 变量/参数值：协议支持显式选择（`--observe-variable`/`--observe-parameter`，
  Text 需 `--observe-text-values`），但 0.3.0 的 View **默认不请求、不展示**；
  内存/资源/句柄永不发布。
- `not_loaded` 引用只表示当前 catalog 未核验，不是断链；需要 `check --refs`
  才能核验普通本地链接，且 `--refs` 的字节摘要不能用于运行绑定。
- 平台：仅 Linux x86_64 / glibc 2.39 正式验证；Windows/macOS 不支持声明。
- 远端 CI 配置已提供（`.github/workflows/ci.yml`、`.woodpecker.yml`），本环境
  未实际运行远端 CI；未运行的门禁在 evidence 中保持 `UNVERIFIED`。
- 像素级人工视觉比对与完整 screen-reader 走查为 `UNVERIFIED`。
