# PR-02 证据：Reader/Analysis 结构与只读面（RFC-VIEW-0002）

- 日期：2026-11-22
- 范围：`src/protocol/*`、`src/reader/*`、`src/config.rs`、只读路由与阅读 UI 收敛
- 冻结 CLI：`marqdo 3.0.0-beta.1`（`062cb56e…`），本 PR 不改/不重编译
- 证据来源：本仓库 `cargo test`、`cargo clippy`、冻结 Analysis/Trace golden、
  真实 Chromium 证据（0001 时期，PR-05 重新跑 0002 宽度矩阵）

## 1. 目标目录结构（§5.3 的 Reader 部分）

```
src/
  main.rs            # 唯一 composition root（解析参数后调用 lib::run）
  lib.rs             # 模块声明 + 用例分派（browse/verify/export），无第二管线
  cli.rs             # View 自有参数解析
  config.rs          # view.json 站点配置（不定义语言语义、不授权）
  core_client.rs     # 唯一 MARQDO_BIN 调用入口（check/negotiate/verify）
  error.rs           # 稳定错误码
  export.rs          # 静态导出（只依赖 Reader，不依赖执行）
  protocol/
    analysis_v2.rs   # 公共 Analysis v2 解析/校验/投影
    trace_v2.rs      # 公共 Trace v2 解析/校验/绑定
    tool_v1.rs       # Tool v1 环境协商与版本轴
  reader/
    source.rs        # 受控项目根、路径 containment、UTF-8 byte span、source digest
    markdown.rs      # 唯一 GFM 渲染器（无语义执行判断）
    model.rs         # PresentationModel / SourceStatus / Navigation / Outline
    assemble.rs      # 静态+运行产物合并、stale 判定、DocumentService
    render.rs        # 唯一 HTML 渲染入口（Live/Export 共用）
    assets.rs        # 构建期内嵌唯一 CSS/JS/图片
  server.rs          # 过渡文件：只读路由；PR-04 拆为 server/{mod,routes,security,sse}
```

`app.rs`/`project.rs`/`semantic.rs`/`trace.rs`/`document.rs`/`presentation.rs`/
`source_map.rs`/`render.rs`/`assets.rs` 旧路径已删除；`git mv` 保留历史。
没有对旧 `marqdo` 或 `marqdo-cli` 的 crate/Git/path 依赖（`tests/contract/no_coupling.rs`）。

## 2. 只读 API 收敛（本 PR 部分）

| 路由 | 状态 |
|---|---|
| `GET /` | 只读页面（不执行） |
| `GET /file?path=...` | 受控单文件页面（不执行） |
| `GET /assets/...` | 固定打包资产 |
| `GET /api/documents` | **新增**：文档列表 + 当前文档 outline/functions/analysis 摘要；只读 |
| `GET /api/document` | **删除**（旧单数路由，返回 404） |
| `GET /api/health`、`POST /api/run`、`/api/clear-run` | 过渡保留，PR-04 替换为 7 条合同路由（SSE） |

`GET /api/documents` 测试：`tests/integration/http_live.rs::documents_api_is_a_read_only_structure_and_old_route_is_gone`。

## 3. 阅读 UI 收敛（§4.2/§4.5）

- 页面只保留 `文档 / 函数 / 运行` 三项；`源码` 仍为“更多/局部切换”，无编辑器。
- “更多”菜单删除“变量信息/内存快照”占位项，改为一个“未提供的能力”说明项
  （变量/内存/调试/证明均明确 UNSUPPORTED，不伪造）。
- 新增简单深色主题开关（`data-theme` + 单一 token 覆盖层 + localStorage 偏好），
  无主题框架/远程皮肤；设置里可切字号/标注/主题/打印。
- `tests/integration/rendering.rs::reader_exposes_simple_theme_toggle_and_no_variable_or_memory_tabs`
  断言页面不再出现 variables/memory 动作，且存在 theme/unsupported 动作。

## 4. 测试执行（实际）

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --offline -- -D warnings
cargo test --locked --offline
```

结果：unit 41 + contract 41 + integration 33 = **115 passed / 0 failed**。
`cargo test` 的输出中每个 `[[test]]` 目标：
`contract`（CLI/Tool/Analysis/Trace/coupling/assets）、
`integration`（Live HTTP/renderer/unicode/export）。

## 5. 未完成/后续

- `server.rs` 仍是单文件过渡实现；PR-04 拆分为 `server/{mod,routes,security,sse}.rs`
  并落实 7 路由、会话 Cookie、SSE、取消。
- `GET /api/health`/`/api/run`/`/api/clear-run` 是 PR-03/PR-04 前的过渡路由，
  最终必须删除；当前不把“同步运行后整页重载”当作 0002 交付。
- `tests/security/` 套件与 `docs/design` 的 0002 宽度截图矩阵在 PR-04/PR-05 补齐。
