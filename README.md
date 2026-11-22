# marqdo-view 开发规范交付包（不是已实现的源码仓库）

此目录包含实施新独立 `marqdo-view` 所需的完整架构规范和用户提供的旧界面截图。两者须一起提交到未来的 `marqdo-view` 独立仓库，并在正式实现时逐步补齐 `docs/STATUS.md`、ADR、测试、独立 bundle。

- `docs/RFC-VIEW-0001.md`：总体架构、历史取证、语义/协议、安全、目录、PR、验收。
- `docs/design/reference-old-view-reader.png`：1664×920 的旧 View 视觉参考。并非已验证的旧仓库 HEAD 页面截图。

新项目应从空仓库开发，禁止直接把旧 `marqdo/src/view/` 复制为运行代码，禁止修改或编译 `marqdo-cli` 核心。实施证据由实际开发者生成，规范文件本身不代表已有代码已实现。
