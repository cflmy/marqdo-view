# PR-03 浏览器视觉验收证据

- 采集环境：Chromium Headless Shell 131（Playwright 1.49.0），Linux x86_64，
  视口见文件名；真实冻结核心 `marqdo 3.0.0-beta.1`
  （SHA-256 `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f`）。
- 采集脚本：`tests/browser/reader.browser.cjs`，入口 `tools/test-browser.sh`。
- 断言范围（全部 PASS）：单一 CSS/单一 script、无外部网络请求、无 console 错误、
  文档/源码切换、搜索、设置字号与标注显隐、7 个宽度（390/600/900/1280/1664/
  1920/2560）无横向溢出且 Dock 在视口内、无 JS 可读、打印隐藏交互栏、显式运行
  确认后展示真实 Trace 与内联 chip、清理结果。

| 文件 | 说明 |
|---|---|
| `welcome-1664x920.png` | 显式 `view.json` Welcome 页，1664×920（与基准截图同尺寸），无运行 |
| `welcome-390x844.png` | 同一页面 390×844 单栏布局 |
| `control-flow-run-1664x920.png` | `examples/control-flow` 用户确认运行后的真实 Trace v2 运行面板与内联事件 chip |

## 诚实限制

- 未做与 `docs/design/reference-old-view-reader.png` 的像素差分：本环境无法进行
  人工视觉判读，`U-01` 只声明“DOM/几何/字体栈/颜色令牌/无溢出/无外部资源”通过，
  像素级视觉相似度标记为 **UNVERIFIED（人工复核待办）**，不以自动化 PASS 代替。
- 截图是证据，不是新的渲染路径：页面仍由唯一 `ReaderRenderer` 生成。
