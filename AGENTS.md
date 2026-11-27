<!-- marqdo:repo-conventions v1 -->
# AGENTS.md — view

全局规范唯一来源：[marqdo 仓库开发规范](https://git.cflmy.cn/cflmy/marqdo/src/branch/main/docs/REPOSITORY-CONVENTIONS.md)。本文件只写本仓边界，不复制全局规范或合同正文。

## 1. 本仓定位与唯一权威

- catalog ID：`view`；父级：根；角色：tool；状态：active。
- 职责/本仓拥有：独立文档阅读器与静态导出；消费公开 Analysis 3/Observation 1 的展示层
- 本仓不拥有：Markdown/语言 parser、设计解释、CLI 版本选择、其它 Tool 的业务
- 权威输入/合同：CLI 公开 Analysis 3/Observation 1；`MARQDO_BIN` 绝对路径与制品身份

## 2. 必做 / 禁止

| 必做 | 禁止 |
| --- | --- |
| 只在本仓职责内修改，先开本仓工单 | 实现其它仓库功能，复制 parser/实现/合同 |
| 用真实 CLI/制品与独立期望测试 | mock、字符串检查或原型冒充真实验收/PASS |
| 依赖使用固定 SHA/摘要与显式输入 | 依赖父仓 checkout、兄弟仓相对路径、浮动分支 |
| 未运行写 `UNVERIFIED` | 伪造 entry/版本/CI 徽章/gitlink/发布资产 |
| 记录 SHA、制品摘要、平台、命令、退出码 | 提交密钥、凭证、构建产物或本地绝对路径 |
| 跨仓变更走 C2 → B2 → A2 晋升 | 自动 tag/release/合并，或直接改父仓/兄弟仓 |

- 不新增第二 catalog/manifest/lock/版本表/发布流程；不为单功能拆仓或建空壳仓。
- 不在本仓登记其它仓库后代路径；治理树最多两级 gitlink。

## 3. 构建、测试与证据

- CI/验收入口：`scripts/acceptance.sh`（非交互、失败非零、不打印秘密、只写声明输出；构建与发布权限分离）。
- 输入与依赖：CLI 公开 Analysis 3/Observation 1；`MARQDO_BIN` 绝对路径与制品身份
- 交付：Tool bundle + `tool.json` 与安装/运行/卸载证据
- 证据：候选 SHA、依赖/制品 SHA-256、平台/工具链、命令与退出码、日志位置、测试范围；FAIL/UNVERIFIED 不得写成 PASS。

## 4. 工单路由

- 公共协议/字段缺失 → `marqdo-cli`；阅读/导出行为 → 本仓。
- 不复制 CLI 内部 crate 或私有输出；不新增 CLI 语法。
