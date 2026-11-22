# ADR-0005：静态导出是纯只读、同渲染器、原子发布

- 状态：Accepted
- 日期：2026-11-22
- 关联：RFC-VIEW-0001 §11.3、§14.3、§14.5；PR-05

## 背景

旧 `marqdo view output` 默认在导出过程中执行文档以抓取输出快照（`static_page_data`），只有显式 `--no-exec`/`--snapshot` 才避免。这使“导出”不再是纯只读操作。新 View 的导出必须无条件纯只读，同时与 Live 共用唯一渲染器。

## 决策

1. **导出路径不存在 `run` 调用**：`export.rs` 只调用 `AppContext::load`（内部仅 `check`），从不构造 `RunOptions`、从不起子进程 `run`。导出 HTML 中不存在运行面板或可点击的运行按钮；`data-live="false"`、`data-run-available="false"`。
2. **同一渲染器**：每个页面由 `ReaderRenderer::render(model, RenderContext::static_export(depth, annotations))` 产生；Live/Export 唯一差异是 `LinkPolicy`、`annotations` 与控件可用性。Exporter 不拼 HTML。
3. **离线资产**：写出的 CSS/JS/图片来自 `AssetBundle` 的同一份字节；CSS 内图片为 data URI，导出站点无外链字体/CDN/远程资源。`index.html` 在站点根，`pages/<relative>.html` 按文档相对路径生成，全部链接相对化。
4. **原子发布**：先写入同父目录的 staging 目录，成功后 `rename` 到目标；失败清理 staging，不污染上一个成功导出。目标已存在时只允许是“空目录”或带 `schema:marqdo.view-export` 清单的旧导出，否则拒绝覆盖。
5. **边界与隐私**：拒绝输出目录位于项目根内（避免递归复制/覆盖源文件）；`manifest.json` 不包含绝对机器路径，只含相对路径、SHA-256、协议/版本与 `executed:false`。
6. **标注可关闭**：`--no-annotations` 仅让渲染器不生成语句锚点并设置 `data-annotations="off"`；正文、目录、链接、表格、源码态保持完整。

## 后果

- 对抗测试 `tests/export.rs` 使用会写 marker 的假核心：导出后 marker 不存在、假核心 argv 中没有 `run`。
- 真实冻结核心导出两个文档页面均为 `static-ok`；`tools/test-export.sh` 校验离线资源、manifest 哈希与核心 SHA 不变。
- 副作用：导出对每个文档执行一次 `check`（非执行），大项目导出耗时与文档数线性相关；首版不做增量导出，保持正确性优先。
