# 工单 #3 基线

- View 基线：`dcb9daeaf4e5eefd8c35aa0c843dd8a0eaf7fef5`。
- [Linux x86_64 RC.2 发行包](https://git.cflmy.cn/cflmy/marqdo-cli/releases/tag/v3.0.0-rc.2)与发行方 SHA-256 清单核对通过。
- 发行 tag：`9149ee40a48fcda692c5c112f66c465487d13dfb`。
- 包 SHA-256：`e32e6426caf023fc813ebdbf5a171ec164ee2ffb843ae5594b8e7b9780f496ea`。
- 二进制 SHA-256：`5caa90b8f6a282aef5bb301443ede6cd3bdcf5f64c88969e6d71cb324b03dc5b`。

发行 CLI 完成示例的 `check --emit`（Analysis v3）和 `call`（Events v1、Trace v2），返回 Float 4.0。View 基线只接收 Analysis v2，无法显示这个项目的执行语义。本文件记录 CLI 可观察的基线，View 的端到端门禁另行验证。
