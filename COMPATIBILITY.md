# Compatibility

## 版本轴（不得混用）

| 轴 | 本 View 值 | 说明 |
|---|---|---|
| View 产品版本 | `0.1.0` | 独立 SemVer，与 CLI 不共用 |
| `marqdo` 产品范围 | `>=3.0.0-beta.1,<4.0.0` | 安装与分发时由 CLI 强制校验 |
| CLI Tool Protocol | `1` | `tool.json` 清单/整包摘要/分发 |
| Analysis | `2` | 未知 major 直接拒绝，不回退 |
| Trace | `2` | 与 Analysis 精确字节摘要绑定 |
| Markdown profile | `marqdo.md.v1` | 与冻结 CLI 相同选项集 |

Beta 版 CLI 必须使用 prerelease 范围（`>=3.0.0-beta.1,<4.0.0`）；`>=3.0.0,<4.0.0` **不匹配** `3.0.0-beta.1`。

## 平台

| 平台 | 状态 | 说明 |
|---|---|---|
| Linux x86_64 (glibc) | **Supported** | 与 CLI Beta 唯一发布目标一致 |
| Linux aarch64 | 未构建 | 未验证 |
| macOS / Windows | 未验证 | 不声明支持；路径/进程组逻辑留有平台分支，但未做实测 |

## 能力边界（诚实状态）

| 能力 | 状态 |
|---|---|
| 原始 Markdown 只读浏览（GFM 表/列表/引用/代码围栏/数学文本） | **已实现**（PR-02） |
| Analysis v2 静态语义（函数/Unit/诊断/引用/资源） | **已实现**（PR-02） |
| 文档态 / 源码态切换 | **已实现**（PR-02） |
| 暖纸 Reader 视觉（单一 CSS/JS） | 计划 PR-03 |
| 用户显式触发运行 + Trace v2 事件可视化 | **已实现**（PR-04，仅 `pure` 模式，需页面内确认 + CSRF） |
| Live 本机安全 API | 部分实现（PR-02 只读 + PR-04 run；PR-05 强化/导出） |
| 静态离线导出（纯只读） | 计划 PR-05 |
| 逐语句变量值 / 参数实际值 / 内存快照 | **UNSUPPORTED**（Trace v2 不提供，绝不伪造） |
| 知识“已证明”状态 | **UNSUPPORTED**（需要独立 Prove 协议与证书） |
| 调试器/断点/单步 | **不实现**（首版非目标） |
| Knowledge/Prove/Lint 引擎 | **不实现**（独立仓库/未来可选信息层） |

实际完成状态以 [`docs/STATUS.md`](docs/STATUS.md) 与 CI 证据为准，本文不代替现状。

## 消费者契约

- 未知 `schema_version` → `view.protocol_incompatible`，不回退猜测。
- `sha256(exact analysis.json bytes) == trace.source.analysis_digest`；不匹配 → `view.trace_stale`。
- `trace.source.root_id == analysis.project.root_id`；事件 `module_id` 必须存在于 Analysis。
- `trace.truncated != null` → `TRUNCATED`，不声明全部分支/循环已遍历。
- 运行只使用 `MARQDO_BIN` 子进程与公开 JSON；不链接 Core crate、不读 Core 源码、不依赖私有 IR。
