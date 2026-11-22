# ADR-0004：显式运行、CSRF/Origin 与运行缓存

- 状态：Accepted
- 日期：2026-11-22
- 关联：RFC-VIEW-0001 §10.3、§11.2、§12；PR-04

## 背景

旧 View 的 `POST /api/run` 只解析 JSON body 就执行文档，没有 CSRF、Origin、Host 或显式确认。新 View 必须在“文档始终可读”和“运行可能产生真实副作用”之间划出可审计的边界，同时保持单一渲染器：运行结果不能由 JS 拼 HTML 或伪造。

## 决策

1. **只允许显式确认**：页面加载、刷新、搜索、导出、`GET /api/document` 永不调用 `run`。只有页面内确认对话框的 `POST /api/run` 可以执行；请求体必须含 `confirm:true`。
2. **CSRF + Origin + Host 三重校验**：随机 256-bit 会话令牌由 `/dev/urandom` 生成，通过 meta 注入页面，POST 必须带 `X-MQ-CSRF`；若请求带 `Origin`，必须是本机允许源；Host 仍使用既有 DNS-rebinding 校验。缺 token/错 token/跨站 Origin 一律 403 且不启动子进程。
3. **权限最小化**：首版只接受 `mode:"pure"`；`project`/`strict` 及 grants 需要未来独立授权 UI。View 不读取 `marqdo.yaml` 的 capabilities 来推导授权。
4. **单活动运行**：进程内 `AtomicBool` 保证同一 View 会话只有一个运行；第二次请求返回 409。子进程使用独立进程组与超时，超时杀整个进程树。
5. **服务端运行缓存而非 JSON 拼 HTML**：运行成功后服务器把 `{相对路径, source_digest, VerifiedRun}` 存入进程内缓存，返回 `redirect=/?run=<id>`；浏览器重载后同一 `ReaderRenderer` 用缓存渲染运行面板与锚点 chip。JS 不解析 Trace、不生成结果 HTML。
6. **重新绑定**：渲染运行页时重新 `check` 并重新校验 Analysis digest；源码或 Analysis 字节变化 ⇒ `STALE`，事件不贴到新正文。运行缓存只在进程内，重启/清理即丢弃。
7. **诚实字段**：只展示 Trace v2 公开字段和 `run --json` 公开的 stdout/stderr/最终值；不展示参数实际值、逐语句赋值、内存快照或证明状态。事件列表有上限并显式标注截断。

## 后果

- 正面：运行路径可被 HTTP 层测试完整覆盖（拒绝路径不产生 `run` 进程；页面加载 10 次为 0 次运行）；Live 与 Export 继续共用唯一的 Reader 渲染字节。
- 负面：运行完成后整页重载（保留阅读位置）；首版无流式输出/取消/多运行历史；这些属于后续 PR，不得伪造。
- 门禁：`tests/http_live.rs` 覆盖 token/Origin/confirm/mode/单活动/清缓存/源码变化 stale；`tests/trace_contract.rs` 覆盖 digest、module、seq、truncated、failed。
