# marqdo-view 现状（STATUS）

> 最后更新：2026-11-25。当前目标：`marqdo-view 0.3.0`，只支持
> `marqdo 3.0.0-rc.4` 的公开契约（adaptation #4，验收 #5，公共 Analysis 边界 #6）。
> 机器证据只认绑定 CLI 源码 SHA 的 manifest；未生成的证据一律 `UNVERIFIED`。

## 版本与基线

| 项 | 值 |
|---|---|
| View 产品版本 | `0.3.0` |
| 产品范围 | `>=3.0.0-rc.3,<4.0.0`；验收固定 `3.0.0-rc.4` |
| CLI Tool | revision 1：`requires {api_revision: 1, analysis: 3, observation: 1}`，无 `compatibility` |
| 静态语义 | Analysis revision 3（`marqdo.analysis`，ADR-0069 公共边界） |
| 运行事实 | Observation revision 1（`marqdo.observation`，单一事件序列） |
| Markdown profile | `marqdo.md.v1` |
| 冻结 CLI 版本 | `marqdo 3.0.0-rc.4` |
| CLI 源码 SHA | `452e9a32bd9e40dba427d4180691c58bab74449e` |
| CLI 归档 SHA-256 | `9c3f166b5e5b8840d39d10019074e4e1c42432fc23e26c69670e64fdcf4e15d8` |
| CLI 二进制 SHA-256 | `a7306962689c635445a59043971ea2eb09298e01feeee81f0ba07b1f3e10306e` |
| 平台 | Linux x86_64 / glibc 2.39（唯一正式验证） |
| 视觉基准 | `docs/design/reference-old-view-reader.png`（**2048×1133**） |
| 验收 manifest | `docs/evidence/452e9a32bd9e40dba427d4180691c58bab74449e/manifest.json`（**result: PASS，G01–G12 全部 PASS**） |

## 0.3.0 适配交付（实现状态）

| 交付 | 状态 | 位置 |
|---|---|---|
| CLI Tool revision 1 清单、无 `compatibility` | 实现 | `tool.json`、`scripts/package.sh` |
| Analysis revision 3 单一静态源（source/document/anchors/heading_span/references） | 实现（待验收） | `src/protocol/analysis_v3.rs`、`src/reader/*` |
| Observation revision 1 流/快照单一解码器 | 实现（待验收） | `src/protocol/observation_v1.rs`、`src/core_client.rs` |
| 绑定校验（analysis_digest/root_id/module_id/UTF-8 半开 span） | 实现（待验收） | `src/execution/*` |
| 引用状态渲染（`not_loaded` = 未核验） | 实现（待验收） | `src/reader/render.rs`、`assets/reader.js` |
| 值默认不请求、不伪造 | 实现 | `src/execution/*`、`assets/reader.js` |
| G01 无旧契约静态检查 | 通过 | `scripts/check-no-legacy.sh` |
| G12 文档事实一致 | 通过 | `scripts/check-docs.sh` |
| 验收 manifest G01–G12 + `unverified` | 已生成 `result: PASS` | `scripts/acceptance.sh` |
| CI 钉 rc.4 + 独立 no-legacy job | 远端 Woodpecker pipeline #10 对 `340ecb0` 成功 | `.github/workflows/ci.yml`、`.woodpecker.yml` |

## 发布门禁 G01–G12（issue #5）

> 证据规则：`PASS/FAIL/UNVERIFIED` 只能由 `scripts/acceptance.sh` 生成并写入
> `docs/evidence/<CLI-SOURCE-SHA>/manifest.json`。当前 manifest 对 View 源码提交
> `340ecb0b6ea19fb3047a2930f3ef0c3c36ba291f` 的 `result` 为 `PASS`（G11 引用
> Woodpecker #9）；`UNVERIFIED`、`PARTIAL`、`WARN` 一律不得写成 `PASS`。

| # | 门禁 | 当前证据状态 |
|---|---|---|
| G01 | 协议层无旧契约残留（`scripts/check-no-legacy.sh`，0 命中） | `PASS` |
| G02 | fmt + clippy + test + release build 全 exit 0 | `PASS` |
| G03 | rc.4 归档独立安装、`view --help`、CLI SHA 前后不变、篡改拒绝 | `PASS` |
| G04 | 静态契约正向：`check --emit` + `view verify` | `PASS` |
| G05 | 运行契约正向：显式 run/call，终态与 CLI 一致 | `PASS` |
| G06 | 流/快照等价（同一次执行两个目的地事件序列一致） | `PASS` |
| G07 | stale 绑定语义（digest/root/span；失败不显示成功） | `PASS` |
| G08 | N01–N20 负例全部 fail-closed | `PASS` |
| G09 | 安全基线不回退（security + no_exec + browser） | `PASS` |
| G10 | 导出零执行且 Live/Export 同渲染器 | `PASS` |
| G11 | manifest 与 CI 同 SHA；CI 红不发布 | `PASS` |
| G12 | 文档事实一致（`scripts/check-docs.sh`） | `PASS` |

任一 P0 门禁 `FAIL` 或 `UNVERIFIED` → 不发布 0.3.0。

## 能力基线

| 能力 | 状态 |
|---|---|
| 原始 Markdown/GFM 阅读、源码态、搜索/设置/打印/深色主题 | 实现（单一 `assets/reader.css` + `assets/reader.js`） |
| Analysis revision 3 严格校验 + 函数/Unit/签名/诊断/导入/引用/资源投影 | 实现（无效 span/摘要降级，旧同 revision 拒绝） |
| heading_span 标题 UI / 完整 span section 高亮与归属 | 实现 |
| 引用状态七态渲染；`not_loaded` 未核验、`ambiguous` 列出候选 | 实现 |
| 单一 Read Model/HTML 组件/CSS/JS；Live=Export | 实现（门禁测试） |
| 固定 Live 路由；打开/刷新/搜索/切换/导出零执行 | 实现并测试 |
| 显式单任务执行（run main / call function），纯 + 受控 project fs grants | 实现（两阶段确认、命令预览、二次确认） |
| Observation v1 经 View SSE 转发/摘要；状态/终态、取消、心跳、Last-Event-ID 重放、无轮询 | 实现（单任务、有界摘要、2 连接上限） |
| 运行结果（Wire Value/stdout/stderr/错误/诊断）安全文本展示 | 实现（无 innerHTML 服务端数据） |
| 引用/调用/分支/循环/副作用标注 + stale 降级 | 实现（分组 chip、来源标注、截断/无效显式） |
| Tool revision 1 独立安装/分发、冻结 CLI SHA 前后一致 | 实现（0.3.0 包，篡改拒绝） |
| 静态离线导出（无执行、无联网、manifest/哈希、不可变发布） | 实现 |
| 固定 examples（welcome/branch-loop/function-params/knowledge-only/unsafe-side-effect） | 实现，rc.4 check/call/run 实测脚本 |
| 逐语句变量值 / 参数实际值 / 内存快照 | **默认不请求、不展示**（协议支持显式选择；View 0.3.0 不请求，绝不伪造） |
| 知识“已证明” / Prove / Lint | **UNSUPPORTED**（等待独立版本化报告协议） |
| 调试器/断点/单步/调用栈、Notebook/持久内核、Session/journal | **不实现**（明确非目标） |
| `strict`、net/process/env/plugin/provider/kernel 授权 UI | **不提供**（拒绝并引导终端 CLI） |
| Windows/macOS 发行 | **不支持/未验证** |

## 明确 UNSUPPORTED / 非目标（不得伪造）

- 变量/内存/逐语句值：Observation v1 默认不发布值；View 不请求任何
  `--observe-*` 选择，UI 只有一个明确的“未提供/未请求”说明。
- 数学/领域正确性与“已证明”：Analysis 成功 ≠ 证明；未来 Prove 独立提供。
- Notebook/Run All/内核重启、断点/单步/时间旅行、第二套 Markdown AST/解释器。
- 网络/进程/环境/插件/Provider/Foreign 授权 UI；高级执行由用户在终端运行 CLI。
- `strict` 下的 Live/文件目的地：CLI 在启动前拒绝；View 不伪装、不回退。

## 证据入口

| 内容 | 路径 |
|---|---|
| 一键验收 + manifest（G01–G12） | `scripts/acceptance.sh`；`docs/evidence/452e9a32…/manifest.json`（**待生成**） |
| 证据目录说明与 manifest schema | `docs/evidence/README.md` |
| 验收标准 | `docs/ACCEPTANCE.md` |
| 合规矩阵 | `docs/CONFORMANCE.md` |
| 冻结 CLI 合同审计 | `docs/audits/protocol-contract-0002.md` |
| 当前分层与契约 | `docs/architecture/observation-v1-and-analysis-v3.md` |
| 契约替换决策 | `docs/decisions/0012-rc4-current-contracts.md` |
| 示例 fixture 实测 | `scripts/check-examples.sh` |
| 观测契约探针 | `scripts/check-observation.py` |
| 零执行哨兵 | `scripts/test-no-exec.sh` |
| 旧 View 取证 | `docs/audits/legacy-view.md`（静态；运行时 `LEGACY_RUNTIME_UNVERIFIED`） |
| 历史 0.2.0 证据 | `docs/evidence/2c7c4f31…/`、`docs/evidence/issue-2/`、`docs/evidence/issue-3/`、`docs/evidence/PR-01..PR-06/` |

## 已知限制与偏差（诚实状态）

- 首版运行仅 `pure` 与受控 `project` fs read/write；`project` 不是 OS 沙箱，
  不自动提升 `marqdo.yaml` capability 请求。
- SSE 只推送 View 生命周期与 Observation 摘要，不承诺逐字 stdout/Token 流。
- 观测侧注采用内联分组 chip，而非 SVG 曲线。
- 像素级人工视觉比对与完整 screen-reader 走查仍为 `UNVERIFIED`。
- 远端 CI 配置已提供但本环境未执行；未运行门禁保持 `UNVERIFIED`。
- 平台：Linux x86_64 glibc 正式验证；其他平台不声明支持。

## 历史

`0.2.0`（2026-11-22）是旧预发布契约时代的完整阅读/执行/导出基线，相关证据
仅作历史参考，不能替代 0.3.0 的 rc.4 门禁：`docs/evidence/2c7c4f31…/`、
`docs/evidence/issue-2/`、`docs/evidence/issue-3/`、`docs/evidence/PR-01..PR-06/`。
旧实现规范与决策保留在 `docs/architecture/RFC-VIEW-0001*.md`、
`docs/architecture/RFC-VIEW-0002.md`、`docs/decisions/0001..0011`（历史记录）。

## 复现文档

- 使用/安装：`README.md`
- 当前契约与分层：`docs/architecture/observation-v1-and-analysis-v3.md`
- 兼容性：`COMPATIBILITY.md`
- 变更：`CHANGELOG.md`
- 决策：`docs/decisions/`
