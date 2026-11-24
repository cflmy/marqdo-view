# ADR-0011 显式发行一次终态结果

- 状态：Accepted，实施与验证状态见 [STATUS](../STATUS.md)。
- 规范：[RFC-VIEW-0001 完整阅读与导出](../architecture/RFC-VIEW-0001-complete-reader.md)
- 本决策更新 ADR-0005 的可导出内容。普通导出及 Live 结果导出共用文档模型、渲染器和打包核心。

唯一执行入口仍为 `POST /api/runs`。用户可在当前会话的终态 Job 有效期内选择一次结果，明确确认后由 `POST /api/exports` 返回静态 ZIP。该路由验证当前来源、会话和字段选择，将有界不可变的数据交给渲染器。结果随 Job 过期，下载完成的文件由用户保管。
