# 工单 #3 验收

Linux x86_64 使用官方 `marqdo 3.0.0-rc.2` 发行归档及 Chromium
`131.0.6778.33`。View 代码和测试提交、发行包、可执行文件、结果 ZIP、
清单与证据文件的 SHA-256 见 [验收清单](manifest.json)。Core 源码标签
及安装归档校验见 [发行基线](../issue-3-baseline.md)。

| 检查 | 结果 |
|---|---|
| Rust 格式、严格 Clippy、160 项单元与集成回归 | PASS |
| 发行包构建、官方 RC.2 安装独立 Tool、Analysis v3 核对 | PASS |
| 原有 Chromium 四套回归 | PASS |
| 本地图片、跨文档重复标题锚点、离线公式和 390px 布局 | PASS |
| 函数旁显式调用及默认参数、终态前可见 stdout、预览确认与下载 | PASS |
| 打开和导出零运行；两种导出后执行计数仍为 1 | PASS |
| 缺少 CSRF、跨 Origin/会话、过期或活动 Job、篡改、源码/锁/导入/图片变化 | PASS |
| 成功、失败确认、取消、超时、Trace 缺失与纯文本输出 | PASS |
| ZIP 条目与清单哈希、打包失败与客户端断开清理、进程重启 | PASS |
| 关闭服务器后，禁用 JavaScript 和网络，以 `file://` 阅读并打印结果 | PASS |
| 完整外部屏幕阅读器手工走查 | UNVERIFIED |

测试日志见 [Rust 与打包](checks.log)、[HTTP 与 ZIP](http.log)及
[浏览器](browser.log)。浏览器验证用真实发行 CLI。计数包装器审查传给 CLI
的 argv，并在流式验证场景延迟转发终态，以观察真实 stdout 在终态之前出现。

- [函数旁结果](live-result.png)
- [导出预览](export-preview.png)
- [离线正文及媒体](offline-snapshot.png)
- [离线结果](offline-result.png)
- [窄屏阅读](mobile-reader.png)

## 复跑

```bash
export MARQDO_BIN=/absolute/path/to/released/marqdo
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
./scripts/test-complete-reader.py
(cd tests/browser && npm ci && npx playwright install chromium)
node tests/browser/complete-reader.browser.cjs
```

如已有 Chromium，可通过 `CHROMIUM_BIN` 指定可执行文件。
`MQ_SCREENSHOT_DIR` 可收集浏览器截图，`SNAPSHOT_ZIP` 可保存 HTTP 验收下载文件。
部署前可运行 `scripts/package.sh`，用同一官方 CLI 复验 Tool 安装。
