# ADR-0009：SSE 传输实现、会话模型与 7 路由收敛

- 状态：Accepted
- 日期：2026-11-22
- 关联：`docs/architecture/RFC-VIEW-0002.md` §6.2/§6.4/§6.5/§6.8；PR-04

## 1. SSE 传输：直接写 socket 并逐帧 flush

`tiny_http` 的流式 `Response` 使用 `chunked_transfer::Encoder::new`，只有缓冲区
填满或响应结束时才 flush。对 SSE 来说，这意味着已接受的 `accepted`/`running`/
心跳可能被扣留数秒，直到任务结束，**不满足“状态变化立即推送”**。

因此 `server/sse.rs` 在成功鉴权后使用 `Request::into_writer()` 取到底层 socket
writer，自行写入：

```
HTTP/1.1 200 OK
Content-Type: text/event-stream; charset=utf-8
Cache-Control: no-store
X-Accel-Buffering: no
Connection: close

event: ... / id: N / data: {...}
```

要点：

- **每帧/心跳后立即 `flush()`**，浏览器 EventSource 即时收到；
- 不设 `Content-Length` 且 `Connection: close`，body 以 EOF 结束（SSE 合法传输）；
- 终态事件写出后主动关闭流；客户端 JS 在收到终态时关闭 EventSource，不会重连
  重复执行；
- 客户端断开（broken pipe）只关闭观察连接，不取消任务、不产生执行终态；
- 只允许 4 种 `run.*` 事件 + `: heartbeat` 注释；心跳不使用序号；
- 每任务最多 2 个并发 SSE 观察连接（Controller 计数，超出 `429 view.sse_busy`）；
  128 条环形缓冲 + 终态快照 + `Last-Event-ID` 重放或权威 snapshot；
- 测试钩子 `MARQDO_VIEW_SSE_HEARTBEAT_SECS` 仅用于本地/CI 缩短心跳周期，默认
  15 秒（不暴露为产品功能）。

## 2. 会话与鉴权

- 首次 `GET /` 或 `GET /file` 发放 `mq_view_session=<随机32字节hex>` Cookie：
  `HttpOnly; SameSite=Strict; Path=/; Max-Age=43200`；loopback HTTP 不加 `Secure`
  （不伪造 HTTPS）。
- 页面通过 `<meta name="mq-csrf-token" content="...">` 获得**该会话**的 CSRF；
  POST 必须携带 `X-View-CSRF`，与 Cookie 会话匹配（常量时间比较）。
- POST 还要求：合法 `Host`（loopback+端口）、精确同源 `Origin`（缺失即拒绝）、
  `Sec-Fetch-Site` 不得为 `cross-site`；`POST /api/runs` 要求
  `Content-Type: application/json`，body ≤64 KiB。
- `POST /api/runs/{id}/cancel` 是无 body 的控制请求：允许无 JSON Content-Type，
  但仍要求 Cookie + CSRF + Origin/Host。
- SSE GET 只允许**启动该 run 的会话**读取；事件/取消一律按 `session_id` 校验，
  跨会话返回 404（不泄漏任务是否存在）。
- 会话表上限 64、空闲 TTL 12h；Cookie/CSRF 只存内存，不写磁盘、不进 URL。

## 3. 路由表（最终态）

只保留 7 条：`GET /`、`GET /file?path=`、`GET /assets/...`、
`GET /api/documents`、`POST /api/runs`、`GET /api/runs/{id}/events`、
`POST /api/runs/{id}/cancel`。

- 删除 `/api/health`、`/api/document`、`/api/run`、`/api/clear-run`；
- 不提供状态轮询路由、`/api/eval`、Notebook/session 兼容层或通用 shell 端点；
- `GET /api/documents` 是只读结构（文档列表 + 当前 outline/functions/analysis
  摘要），无副作用；页面/刷新/搜索/切换/导出均不形成执行边。

## 4. 前端最小状态

`reader.js` 只做 DOM/表单/SSE：

- Dock 运行：有 main → 打开运行表单；无 main → 打开函数列表并解释，绝不替跑；
- 函数列表的“调用”按钮携带 Analysis 函数 id 与参数签名，动态生成表单；
- 表单有命令预览（文本节点）、pure/project 模式与 fs read/write glob；project
  需要二次 `window.confirm`；无 net/process/env/plugin 等按钮；
- 提交一次 POST 后建立 `EventSource(..., {withCredentials:true})`，**没有
  setInterval/轮询**；`seq` 去重、`run_id` 校验、终态关闭；
- 运行区/结果/stdout/stderr/Trace 全部用 `textContent`/DOM 文本构建，绝不
  innerHTML 服务端数据；stale 结果不绘制原文锚点；
- 浏览器重载不会自动 POST；清理只隐藏本地展示，不删除项目文件/lock，也不取消
  仍在服务端的任务。

## 5. 与 RFC 的关系

实现保留 RFC 的公开合同（事件类型、字段、状态机、`Last-Event-ID`、心跳语义），
仅选择“直接写 socket”作为传输细节——属于 RFC 允许的实现自由，不改变协议。
测试覆盖：HTTP/SSE 集成、真实 Chromium EventSource、跨会话/跨站拒绝、取消唯一
终态、心跳、2 连接上限与 Last-Event-ID 重放（见 `docs/evidence/PR-04/`）。
