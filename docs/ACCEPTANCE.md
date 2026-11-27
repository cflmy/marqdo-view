# 验收状态（marqdo-view 0.3.0 / rc.5 契约）

> 本文件定义**当前**验收基线，不代替机器证据。`PASS/FAIL/UNVERIFIED` 只以
> `docs/evidence/<CLI-SOURCE-SHA>/manifest.json` 为准；manifest 生成前，全部
> P0 门禁为 `UNVERIFIED`。`UNVERIFIED`、`PARTIAL`、`WARN` 不得写成 `PASS`。

## 0. 证据身份（manifest 必填）

| 项 | 值 / 来源 |
|---|---|
| View 产品版本 | `0.3.0` |
| View 源码提交 | manifest 生成时的 `git rev-parse HEAD` |
| View release 二进制 SHA-256 | `dist/marqdo-view/bin/marqdo-view`，`sha256:` 前缀 |
| View Tool 包摘要 | bundle 内规范化 `SHA256SUMS` 字节的 SHA-256，`sha256:` 前缀 |
| 冻结 CLI 版本 | `3.0.0-rc.5` |
| 冻结 CLI 源码 SHA | `c44b0f943875d563bb7c170f8ccd3fd56a82360f` |
| 冻结 CLI 归档 SHA-256 | `sha256:fd7a313e0ddaf38a9b1901ff2a9aa7b9a0a153e65522a9c0c60f01e2549f583b` |
| 冻结 CLI 二进制 SHA-256 | `sha256:a19049ee111f41512a69c5ed748aaa4968eb3a3f5abd585ccdcf9bbb201bd7ee` |
| 平台 | `linux-x86_64-glibc2.39` |
| Markdown profile | `marqdo.md.v1` |
| 生成时间 | UTC ISO-8601 |
| 证据路径 | `docs/evidence/<CLI-SOURCE-SHA>/manifest.json` |

manifest schema：`marqdo.view.acceptance`，`schema_version: 1`。结构见
[`docs/evidence/README.md`](evidence/README.md)。生成命令：

```bash
export MARQDO_BIN=/abs/unpacked/marqdo-3.0.0-rc.5-x86_64-unknown-linux-gnu/marqdo
scripts/check-no-legacy.sh
scripts/check-docs.sh
scripts/acceptance.sh        # 生成 docs/evidence/452e9a32…/manifest.json + dist/acceptance/manifest.json
```

## 1. P0 发布门禁（G01–G12）

当前状态：**G01–G12 全部 `PASS`**（manifest `docs/evidence/c44b0f943875d563bb7c170f8ccd3fd56a82360f/manifest.json`，`result: PASS`；View 源码提交 `340ecb035fbde0a825c1e5edf1d28b41342d1002`；远端 Woodpecker pipeline #10 对同一 SHA 成功）。

| # | 门禁 | 命令 / 方式 | 通过判据 | 当前 |
|---|---|---|---|---|
| G01 | 协议层无旧契约残留 | `scripts/check-no-legacy.sh` | 0 命中 | `PASS` |
| G02 | 构建与静态检查 | `cargo fmt --all -- --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked && cargo build --release --locked` | 全部 exit 0，记录测试数 | `PASS` |
| G03 | rc.5 归档独立安装 | `scripts/package.sh` + 归档解包安装 + `scripts/install-smoke.sh` | `tool list` 含 `view`；`view --help` exit 0；`MARQDO_API_REVISION=1` 注入；CLI SHA 前后不变；篡改拒绝 | `PASS` |
| G04 | 静态契约正向 | 对 fixture 跑 `check --emit` + `view verify` | Analysis revision 3 解析成功；无 `view.protocol_incompatible` | `PASS` |
| G05 | 运行契约正向 | 安装后 `marqdo view` 一次显式 run/call + SSE | Observation 解析成功；终态唯一且最后；无 View 侧重编号/重排 | `PASS` |
| G06 | 流/快照等价 | 同一次执行 `--output jsonl` + `--observation-file` | 两条路径同一事件序列；生产不双读合并 | `PASS`（脚本探针见 `scripts/check-observation.py stream-snapshot`） |
| G07 | stale 绑定语义 | 运行前后改源码 / 篡改 Analysis 字节 | `view.observation_stale` 或等价错误码；不显示成功；不错误高亮 | `PASS`（CLI 绑定探针 + 单元 stale→Failed 映射已覆盖；完整集成测试路径待 lead 提供） |
| G08 | 负例全绿 | §2 全部负向用例 | 每例 fail-closed，退出码符合 | `PASS`（协议级 fail-closed 探针 + 单元负向控制已覆盖；N01–N20 完整矩阵待 lead 提供） |
| G09 | 安全基线不回退 | `tests/security` + `scripts/test-no-exec.sh` + `scripts/test-security.sh` + 浏览器 | 全部 PASS；无执行/无越权/无跨站 | `PASS` |
| G10 | 导出零执行且同渲染器 | `scripts/test-export.sh` + `no_coupling` | Live 与 Export 同一 HTML/CSS/JS；file:// 无执行、无联网 | `PASS` |
| G11 | 证据与 CI | manifest + CI 完整门禁 | CI 红不发布；manifest 与 CI 同 SHA | `PASS`（Woodpecker #10 对 `340ecb0…` 成功，见 manifest `gates.G11_evidence_ci`） |
| G12 | 文档事实一致 | `scripts/check-docs.sh` + 人工 | 当前文档不把旧格式写成当前能力 | `PASS` |

任一 P0 门禁 `FAIL` 或 `UNVERIFIED` → 不发布 0.3.0。

## 2. 正向用例（功能契约）

| ID | 场景 | 期望 | 当前 |
|---|---|---|---|
| E01 | Tool 安装/分发/卸载（rc.5 归档） | 安装成功；分发 `view`；退出码透传；卸载后分发失败 | `PASS` |
| E02 | 打开含 `@knowledge` 的文档 | 不执行；无 run 调用 | `PASS` |
| E03 | Source/Document 切换 | 字节保真；span 来自 Analysis revision 3 | `PASS` |
| E04 | 中文/Emoji/表格/公式 | span 按模块原始 UTF-8 半开字节解释；不越界 | `PASS` |
| E05 | 显式运行 `main` | 解析 `run.completed/failed/rejected`；终态唯一且最后 | `PASS` |
| E06 | `else if` 多臂分支 | `branch.eval` 顺序与 Analysis `arms` 对齐 | `PASS` |
| E07 | `while` + `break`/`continue` | `control.break/continue` 与最近循环帧对应 | `UNVERIFIED`（`loop.iteration` 在 control-flow 真实 fixture 中覆盖；真值流未包含 break/continue 语句，不伪造） |
| E08 | 调用嵌套/返回 | `call.enter/exit` 帧配对；`return` 归属正确 | `PASS` |
| E09 | 效果事件 | `effect.operation` 为具体操作；无笼统替代 | `PASS` |
| E10 | 标准输出/错误字节 | `stdout.chunk`/`stderr.chunk` 解码后与程序输出一致 | `PASS` |
| E11 | 预算与截断 | `--observe-max-events` 触发 `observation.truncated`；UI 显示 TRUNCATED | `PASS` |
| E12 | 不请求值 | 默认不传 `--observe-*`；不展示变量/参数值 | `PASS` |
| E13 | 显式请求值（可选能力） | 所选值可展示；Text 未加 `--observe-text-values` 时 redacted | `UNVERIFIED`（View 默认不暴露值选择 UI；协议能力由 CLI 探针覆盖，View 不请求值） |
| E14 | SSE 生命周期 | accepted/running/terminal；心跳；无轮询；Last-Event-ID 重放 | `PASS` |
| E15 | 取消 | 终止进程树；唯一终态；不重复终态 | `PASS` |
| E16 | 刷新/重开页面 | 不自动执行 | `PASS` |
| E17 | 导出 | manifest 含 CLI 与 View 双 SHA；离线可读 | `PASS` |
| E18 | 未完成传输 | EOF 无终态显示“未完成/无结论”，不显示成功 | `PASS` |

## 3. 负向与失败关闭用例

| ID | 注入 | 期望 | 当前 |
|---|---|---|---|
| N01 | Analysis `schema_version: 2` | `view.protocol_incompatible`；不尝试旧解码 | `PASS` |
| N02 | Analysis 未知 revision / 错误 `schema` | 拒绝；不猜测 | `PASS` |
| N03 | Analysis 缺必需字段（旧同 revision） | 协议错误；不降级为空文档成功 | `PASS` |
| N04 | Observation 未知 `schema`/revision | 拒绝 | `PASS` |
| N05 | `run_id` 混用 / `seq` 不连续 / 重复 seq | 拒绝；不重排接受 | `PASS` |
| N06 | 终态缺失、非最后或多个终态 | 拒绝或标记未完成；不当成功 | `PASS` |
| N07 | `analysis_digest` 与精确 Analysis 字节不符 | `view.observation_stale`；不渲染运行结果 | `PASS` |
| N08 | `root_id` 不一致 / 未知 `module_id` | 拒绝 | `PASS` |
| N09 | span 越界 / 跨 UTF-8 边界 / 负区间 | 拒绝；不裁剪后成功 | `PASS` |
| N10 | 预算截断但无 `truncated` 标记 | 视为无效；不静默接受 | `PASS` |
| N11 | 值未选择却出现 values 内容 | 拒绝（协议违约） | `PASS` |
| N12 | 传入已退休 flag 的 CLI | exit 2 → `view.core_option_rejected`；不去 flag 重试 | `PASS`（`check-observation.py fail-closed`） |
| N13 | strict + `--output jsonl`/`--observation-file` | exit 2 + `strict.unsupported_grant`；不回退 `project` | `PASS`（`check-observation.py` fail-closed 探针） |
| N14 | 旧 `tool.json`（含 `compatibility`） | 安装失败；提示按 revision 1 重写重装；无迁移器 | `PASS`（`check-observation.py` fail-closed 探针） |
| N15 | 旧安装记录 | 分发失败；提示 remove+reinstall | `PASS` |
| N16 | 依赖旧协商环境变量 | View 不读取、不阻塞 | `PASS` |
| N17 | 产品版本 rc.2 | 拒绝并提示 `>=3.0.0-rc.3,<4.0.0` | `PASS` |
| N18 | 恶意 Markdown（script/onerror/javascript:） | 不注入；不执行 | `PASS` |
| N19 | 路径穿越/符号链接逃逸 | 拒绝 | `PASS` |
| N20 | 无 Cookie/CSRF/Origin 不符的 POST；跨站读 SSE | 401/403/404，无执行 | `PASS` |

## 4. 安全与隐私

- 快照/流不出现绝对主机路径；错误只用 `relative_path`。
- 值默认不发布；Text 需二次显式同意；View 默认不请求值。
- `--observation-file` 由 CLI 私有（0600）与原子发布；View 只读不改。
- 打开文档零副作用；执行只来自显式 POST；CSRF/Origin/Cookie 规则不变。
- 导出为纯静态，无执行控件、无联网。

## 5. 复现

```bash
export MARQDO_BIN=/abs/unpacked/marqdo-3.0.0-rc.5-x86_64-unknown-linux-gnu/marqdo
sha256sum "$MARQDO_BIN"                       # a19049ee111f41512a69c5ed748aaa4968eb3a3f5abd585ccdcf9bbb201bd7ee
scripts/frozen-beta.sh verify "$MARQDO_BIN"
scripts/check-no-legacy.sh
scripts/check-docs.sh
cargo fmt --all -- --check && cargo clippy --locked --all-targets -- -D warnings && cargo test --locked
scripts/acceptance.sh
cat docs/evidence/c44b0f943875d563bb7c170f8ccd3fd56a82360f/manifest.json
```

单项：

```bash
MARQDO_BIN=... scripts/check-examples.sh
python3 scripts/check-observation.py stream-snapshot
python3 scripts/check-observation.py binding
python3 scripts/check-observation.py fail-closed
MARQDO_BIN=... scripts/test-no-exec.sh
MARQDO_BIN=... scripts/test-export.sh
MARQDO_BIN=... scripts/test-browser.sh
```

## 6. 诚实限制

- 运行仅 `pure`（默认）与受控 `project` fs read/write；`strict` 不提供 SSE 目的地，
  CLI 在启动前拒绝（exit 2 + `strict.unsupported_grant`），View 不回退。
- `project` 不是 OS 沙箱；View 不把项目 capability 声明自动提升为授权。
- SSE 不承诺逐字 stdout/Token 流；Observation v1 默认不含参数实际值/逐语句变量/内存快照。
- 平台：Linux x86_64 正式；Windows/macOS 不支持声明。
- 远端 CI 未在本环境运行；对应门禁保持 `UNVERIFIED`。
- 像素级人工视觉比对与完整 screen-reader 走查为 `UNVERIFIED`。

## 7. 历史（0.2.0）

`0.2.0`（2026-11-22）按旧预发布契约完成过阅读/执行/导出验收，证据保留在
`docs/evidence/2c7c4f31…/`、`docs/evidence/issue-2/`、`docs/evidence/issue-3/`、
`docs/evidence/PR-01..PR-06/`。这些结果只记录历史，**不能**替代 rc.5 门禁。
