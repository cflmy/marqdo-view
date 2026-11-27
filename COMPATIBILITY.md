# Compatibility

## 版本轴（不得混用）

| 轴 | 本 View 值 | 说明 |
|---|---|---|
| View 产品版本 | `0.3.0` | 独立 SemVer，与 CLI 不共用 |
| `marqdo` 产品范围 | `>=3.0.0-rc.3,<4.0.0` | 预发布契约只在 rc.3 起有意义；验收固定 `3.0.0-rc.5` |
| CLI Tool revision | `1` | `tool.json`：`requires.api_revision = 1`，无 `compatibility` 字段 |
| Analysis revision | `3` | `marqdo.analysis`，ADR-0069 公共边界（含 source/document/anchors/references/heading_span） |
| Observation revision | `1` | `marqdo.observation`，单一事件序列（JSONL 流 / 原子快照） |
| Markdown profile | `marqdo.md.v1` | 与冻结 CLI 相同选项集 |
| 平台 | `linux-x86_64-glibc2.39` | 唯一正式验证平台 |

当前契约：**Analysis 3**、**Observation 1**、CLI Tool **api_revision 1**。

`3.0.0-rc.3,<4.0.0` 是当前预发布范围；`>=3.0.0,<4.0.0` 不匹配任何 rc 预发布版。
验收与证据只认绑定 CLI 源码 SHA / 归档 SHA / 二进制 SHA 的结果：
`marqdo 3.0.0-rc.5`，源码 `c44b0f943875d563bb7c170f8ccd3fd56a82360f`，
归档 `fd7a313e0ddaf38a9b1901ff2a9aa7b9a0a153e65522a9c0c60f01e2549f583b`，
二进制 `a19049ee111f41512a69c5ed748aaa4968eb3a3f5abd585ccdcf9bbb201bd7ee`。

## 契约替换（0.3.0 一次性完成）

| 边界 | 当前契约 | 消费方式 | 不符时行为 |
|---|---|---|---|
| 静态语义 | `check --emit` → Analysis revision 3 | 精确 UTF-8 字节 + `sha256:` 摘要；一次入口一次 emit | `view.protocol_incompatible`，显式提示重新生成/升级核心 |
| 运行事实 | Observation revision 1 | `run`/`call --output jsonl` 流；`--observation-file` 原子快照 | 拒绝；不重排、不重编号、不合成 |
| 源绑定 | `run.started.payload.source.{analysis_digest,root_id}` | 与同一次普通 `check --emit` 的精确字节摘要 + module/source_digest/UTF-8 半开 span 校验 | `view.observation_stale`；不显示成功、不做错误高亮 |
| Tool 清单 | `marqdo.cli-tool` / revision 1 | 精确 `requires {api_revision:1, analysis:3, observation:1}` | 安装失败；提示移除旧注册后重装 |
| 分发环境 | 只注入 `MARQDO_BIN` 与 `MARQDO_API_REVISION=1` | 仅静态期望校验，不据此协商 | 不读取、不阻塞；旧变量不存在 |

删除而非分支：树内没有第二解码器、双态类型、运行时版本探测、迁移器或“先兼容再清理”。
旧格式（含同 revision 的旧字段集合）只在 Git 历史与 CHANGELOG 的 History 段中保留。

## 平台

| 平台 | 状态 | 说明 |
|---|---|---|
| Linux x86_64 (glibc 2.39+) | **Supported** | 完整阅读、显式运行与导出使用官方 rc.5 验收；媒体需要 Linux 5.6+ |
| Linux aarch64 | 未构建 | 未验证 |
| macOS / Windows | 未验证 | 不声明支持 |

## 能力边界（诚实状态）

| 能力 | 状态 |
|---|---|
| 原始 Markdown 只读浏览（GFM 表/列表/引用/代码围栏/MathML/本地图片） | 已实现 |
| Analysis revision 3 严格校验 + 函数/Unit/签名/诊断/引用/资源投影 | 已实现（拒绝旧同 revision 制品，无降级解码） |
| heading_span 用于标题 UI；完整 span 用于 section 高亮/切片/归属 | 已实现 |
| 引用状态 resolved/not_loaded/missing/ambiguous/external/outside_scope/invalid | 已实现；`not_loaded` 显示“未核验”，不误报断链 |
| 文档态 / 源码态切换 | 已实现 |
| 用户显式触发运行 + Observation v1 事件可视化 + SSE + 取消 + 断线重放 | 已实现（`pure`/受控 `project`，显式确认 + 会话 CSRF） |
| Live 本机安全 API（含媒体和 ZIP） | 已实现（Cookie/CSRF/Origin/CSP/单任务/SSE） |
| 静态离线导出 | 普通导出与显式单次结果 ZIP 共用渲染器 |
| 变量/参数值观测 | 协议支持；**View 默认不请求、不展示**（Text 需二次同意）；内存/句柄永不发布 |
| 知识“已证明”状态 | **UNSUPPORTED**（需要独立 Prove 协议与证书） |
| `project` 运行与 fs grants | **已实现（受控）**：仅相对项目根 read/write glob，显式二次确认；其它权限拒绝 |
| `strict` 运行 | 仅终端 text/JSON 快照；SSE 场景明确拒绝（`strict.unsupported_grant`），不回退 |
| SVG 曲线侧注 | **未实现**（事件使用内联 keyed chip） |
| 调试器/断点/单步、Knowledge/Prove/Lint 引擎 | **不实现**（非目标） |

实际完成状态与机器证据以 [`docs/STATUS.md`](docs/STATUS.md) 与
[`docs/ACCEPTANCE.md`](docs/ACCEPTANCE.md) 为准。发布门禁 G01–G12 的
`PASS/FAIL/UNVERIFIED` 只来自 `docs/evidence/<CLI-SOURCE-SHA>/manifest.json`；
当前 0.3.0 证据生成前不得写成 PASS。

## 消费者契约

- 静态语义只来自一次 `check --emit`；`inspect --ast`/`inspect --ir` 仅人工调试，不是机器来源。
- `sha256(exact analysis.json bytes)` 必须等于 Observation `run.started.payload.source.analysis_digest`；
  `--refs` 检查改变字节，不能替代运行绑定。
- 事件 `module_id` 必须属于 Analysis `modules[]`；span 必须是该模块 source 的 UTF-8 半开字节区间。
- `capture.truncated`/`observation.truncated` 表示观测预算耗尽，不代表程序失败，也不得声明完整覆盖；
  EOF 无终态 = 未完成/无结论，不得当成功。
- 运行只使用 `MARQDO_BIN` 子进程与公开契约；不链接 Core crate、不读 Core 源码、不依赖私有 IR
  或包缓存物理路径。
- 未知 `schema`/revision/kind → 显式拒绝；不猜测、不回退、不在失败后换旧参数重试。
