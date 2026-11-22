# ADR-0002：资产内嵌与 Tool Protocol v1 分发

- 状态：Accepted
- 日期：2026-11-22
- 关联：RFC-VIEW-0001 §4.1、§7、§11.3、§14.5、§17.3

## 背景

Tool Protocol v1 安装时复制整个 bundle，并对每个文件做 SHA-256、启动前重算整包清单。View 需要同一份 CSS/JS/图片同时服务 Live 与离线 Export。若 Live 读源码目录、Export 读另一份文件，就会出现 RFC §3.3 禁止的“两套资产来源”。

## 决策

1. 首版把 `assets/reader.css`、`assets/reader.js`、`assets/logo.svg`、`assets/blossom.webp`、`assets/feather.webp` 通过 `include_bytes!` **编译内嵌**为 `src/assets.rs::AssetBundle`；每个资产带 SHA-256，Live 路由 `/assets/<sha-prefix>.<ext>` 与 Export 写盘使用同一份字节。
2. 发布 bundle 只包含运行必需文件：

   ```text
   dist/marqdo-view-<ver>-<target>/
     tool.json
     bin/marqdo-view
     README.md
     COMPATIBILITY.md
     LICENSE-MIT
     LICENSE-APACHE
     SOURCE_SHA
     RELEASE_INFO.json
   ```

   CSS/JS/图片不额外放在 bundle 根（已内嵌），避免“内嵌 + 外部”双来源。
3. `tool.json` 由源码模板 `tool.json` 固定，`compatibility.marqdo = ">=3.0.0-beta.1,<4.0.0"`、`analysis: [2]`、`trace: [2]`；bundle 目录内 `bin/marqdo-view` 是 release 构建产物。
4. 开发时允许 `marqdo-view --marqdo-bin PATH` 直启；安装态必须以 `MARQDO_BIN` 为准，且不向核心传自定义参数格式。
5. `tools/package.sh` 负责：`cargo build --release --locked` → 校验工具协议/SHA → 组装 dist → 写 `SHA256SUMS`/`SOURCE_SHA`/`RELEASE_INFO.json` → 用冻结 CLI 做一次 `tool install` + `view --help` 烟测。

## 后果

- 正面：离线 Export 与 Live 字节一致；Tool 协议能校验唯一二进制；无运行时资产丢失风险。
- 负面：修改 CSS/JS 必须重新编译二进制（可接受，前端资产规模小；由 `build.rs` 在资产变化时触发重编）。
- 门禁：`tests/rendering/asset_tests.rs` 断言 `AssetBundle` 摘要与磁盘源文件一致；`tests/contract` 断言 bundle 内无未声明文件、`entrypoint` 指向可执行文件。
