# ADR-0006：相对 RFC 建议目录/形态的实现偏差

- 状态：Accepted（记录偏差，避免后来者误以为遗漏）
- 日期：2026-11-22
- 关联：RFC-VIEW-0001 §7、§9.3、§15；PR-06

RFC 的目录/接口是**建议起点**，允许在更新文档与 ADR 后调整。本仓库的实际偏差如下，
均不引入第二套渲染/解析/执行路径。

| 偏差 | 实际实现 | 理由 / 影响 |
|---|---|---|
| RFC 目录未列 `src/lib.rs` | 增加 `lib.rs`，`main.rs` 仅分派 | 集成测试与未来 server 复用；仍单 package、单渲染入口（ADR-0001） |
| 测试按 crate 根级集成文件组织（`tests/*.rs`），非 `tests/contracts/...` 子目录树 | 12 个根级测试文件按职责命名 | Cargo 集成测试惯例；职责与 RFC 列表一致，门禁更易运行 |
| 动态 SVG 曲线侧注 | 运行事件改为**内联 keyed chip**；不做曲线 | 首版优先正确性/无 JS 可读；U-04 标 PARTIAL，不画无来源线 |
| 运行输出流式（SSE） | 运行完成后整页重载（保留阅读位置） | 复用同一 renderer 的服务端渲染；避免 JS 拼 HTML |
| `Debug`/Notebook/Workbench | 不实现 | RFC 非目标；旧 `src/view` 不复制 |
| 导出使用 `pages/<relative>.html` + `manifest.json`（`marqdo.view-export` 私有 schema） | 与 RFC §11.3 建议一致，schema 名由 View 自定 | 非 CLI 协议，不冒称官方 schema |
| `view.json` 配置 | 按 RFC §9.4 的建议格式实现 `schema:marqdo.view-config` v1 | 只识别 welcome/site_title/theme；不授予权限、不改变语义 |
| 首版运行只支持 `pure` | `POST /api/run` 拒绝 project/strict | `project/strict` 需要未来独立授权 UI（ADR-0004） |
| README 状态 | 所有未完成/未验证项在 README/CHANGELOG/STATUS/ACCEPTANCE 标注 | 不以视觉相似或自动化 PASS 替代人工/CI 证据 |
