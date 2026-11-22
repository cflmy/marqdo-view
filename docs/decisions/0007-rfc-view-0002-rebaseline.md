# ADR-0007：以 RFC-VIEW-0002 为唯一实施基线

- 状态：Accepted
- 日期：2026-11-22
- 关联：`docs/architecture/RFC-VIEW-0002.md`（本 ADR 的规范来源）；
  `docs/architecture/RFC-VIEW-0001.md`（历史基础设计）；
  既有 ADR-0001～0006（0001 时期决策，除与 0002 冲突的部分外仍有效）

## 背景

仓库此前按 `RFC-VIEW-0001` 完成了 PR-00～PR-06，具备只读 Reader、Analysis v2
校验、显式确认后同步执行 `pure`、Trace v2 绑定、静态导出和冻结 CLI 验收。
随后 `docs/RFC-VIEW-0002.md` 发布，明确：

1. 执行闭环必须使用 **SSE**（POST 建任务、GET SSE 观察、POST 取消），**禁止轮询**；
2. 路由收敛为 7 条固定最小 API，运行只能从 `POST /api/runs` 发起；
3. 首版必须提供单函数 `call` 表单与受控 `project` fs grants（MVP CONTROLLED）；
4. 页面收敛为 `文档 / 函数 / 运行`，删除变量/内存等旧工作台概念；
5. 仓库结构、测试树、脚本、示例和发布门禁按 0002 §5.3/§8/§9 落地；
6. 0002 在执行控制、权限、默认只读、产品裁剪、UI 路由与交互流程上**优先于
   0001**；不得单方面更改 Tool/Analysis/Trace 版本、默认不执行、显式授权、
   单渲染链、SSE 无轮询和 CLI 冻结验收要求。

## 决策

1. **0002 是当前唯一实施基线**。0001 仅作历史视觉/阅读原则参考；凡冲突以 0002
   为准。`docs/RFC-VIEW-0001.md` 移入 `docs/architecture/` 归档，不再作为验收
   依据。
2. **目标结构**按 0002 §5.3 迁移：`src/{main,cli,config,core_client,export}.rs` +
   `src/protocol/{analysis_v2,trace_v2,tool_v1}.rs` +
   `src/reader/{source,markdown,model,assemble,render,assets}.rs` +
   `src/execution/{request,controller,process,events}.rs` +
   `src/server/{mod,routes,security,sse}.rs`。
   - `lib.rs`、`error.rs`、各 `mod.rs` 是 Rust 单 crate 的必要支撑模块；
   - `assets.rs` 归入 `reader/`（构建期资产嵌入，仍只有 `assets/reader.css` 与
     `assets/reader.js` 两个发布入口）；
   - 不拆分多个 Rust crate；不引入第二个 Markdown/语义/执行管线。
3. **只重构 View**：冻结 CLI 不改、不重编译、不链接其私有 crate；黑盒证据以
   `062cb56e…`（3.0.0-beta.1）为准，验收前后 SHA 必须相同。
4. **PR 顺序即提交顺序**：PR-00 取证/基线 → PR-01 Tool/打包 → PR-02 Reader +
   Analysis 路由 → PR-03 执行层（request/process/controller）→ PR-04 SSE 服务与
   前端闭环 → PR-05 Trace 绑定/标注与浏览器回归 → PR-06 导出/发布/文档闭环。
   每个提交必须可构建、测试绿、无隐式执行；中间态功能不完整时在 STATUS 明示。
5. **版本**：View 产品版本从 `0.1.0` 升为 `0.2.0`（0002 §2.2 建议的独立 View
   版本；不是 CLI 版本），同步 `Cargo.toml`、`tool.json`、文档与验收证据。
6. **测试组织**：按 0002 §5.3 建立 `tests/{contract,integration,security,browser,
   fixtures,golden}`；通过 `[[test]]` 显式声明测试入口（Cargo 只自动发现
   `tests/*.rs`，目录需显式声明），职责与门禁不变。
7. **文档一致性**：`docs/STATUS.md` 是唯一现状来源；`docs/CONFORMANCE.md` 维护
   0002 条款 → 实现 → 测试/证据映射；任何未实现项必须标 `UNSUPPORTED/PENDING/
   UNVERIFIED`，不得以视觉相似代替证据。
8. **允许的实现自由**：HTTP 细节、内部类型命名、并发原语、CSS 细节可按黑盒测试
   调整；不得改变 0002 的公开协议、安全边界与验收门禁。

## 与 0001 时期偏差记录的关系

`docs/decisions/0006-rfc-deviations.md` 记录的是 0001 时期的目录/实现偏差，仍作为
历史有效。本 ADR 生效后：

- 0002 明确要求或重新定义的（SSE、路由、call 表单、project grants、页面三项、
  tests 树、scripts/、examples/、0.2.0 版本）不再属于“可保留偏差”，必须实现；
- 0001 时期已实现且与 0002 不冲突的部分（精确 Analysis 校验、Trace 绑定、
  单一渲染器、只读导出、安全头/路径 containment、冻结 CLI 验收）保留并迁移；
- 任何将来仍无法在首版完成的事项，必须新开 ADR 并在 STATUS/README 标
  `UNSUPPORTED`，不得留下灰色按钮或占位路由。

## 后果

- 正面：执行闭环可观测、可取消、可重放；路由/权限面最小且可审计；结构符合
  0002 审查口径；旧叠层与工作台概念彻底消失。
- 代价：需要重写 `server`/执行交互并迁移测试与文档；中间提交可能短暂出现
  “旧同步运行 + 新执行层并存”，只能作为过渡并立即由 PR-04 替换，最终不允许
  双运行框架或轮询 API。
