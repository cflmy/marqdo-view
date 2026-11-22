# PR-01 证据：Tool v1 独立打包、安装与分发（RFC-VIEW-0002）

- 日期：2026-11-22
- View 版本：`0.2.0`（`Cargo.toml`、`tool.json`、`marqdo-view --version` 一致）
- 冻结 CLI：`marqdo 3.0.0-beta.1`，SHA-256
  `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f`
- 平台：`x86_64-unknown-linux-gnu`
- 关键脚本：`scripts/package.sh`、`scripts/install-smoke.sh`、
  `scripts/verify-frozen-cli.sh`
- 包：`dist/marqdo-view-0.2.0-x86_64-unknown-linux-gnu.tar.gz`
  SHA-256 `c3504879c875bdf647f1d3269d5a3c422ca4de7f7c61982a383afadf33be0910`
  （本次本地构建；该值随源码版本变化，最终发布由 PR-06 记录）

## 复现命令（均为实际执行）

```bash
export MARQDO_BIN=/abs/path/to/frozen/marqdo-3.0.0-beta.1
scripts/verify-frozen-cli.sh "$MARQDO_BIN" -- "$MARQDO_BIN" --version

cargo fmt --all -- --check
cargo clippy --all-targets --offline -- -D warnings
cargo test --locked --offline
MARQDO_BIN="$MARQDO_BIN" scripts/package.sh
```

## 黑盒注册/分发结果

在隔离的 `MARQDO_TOOL_HOME` 中解包 `dist/marqdo-view-0.2.0-...tar.gz` 后：

| 步骤 | 命令 | 结果 |
|---|---|---|
| 安装 | `marqdo tool install .../tool.json` | `installed tool 'view' version 0.2.0 (marqdo-view)` |
| 列表 | `marqdo tool list` | `view 0.2.0 (marqdo-view)` |
| 分发帮助 | `marqdo view --help` | 输出由 View 提供（含 `export` / `verify`），CLI 未新增 `Command::View` |
| 分发版本 | `marqdo view --version` | `marqdo-view 0.2.0` |
| 整包篡改 | 向已安装 `view/bin/marqdo-view` 追加 1 字节后 `marqdo view --version` | exit 1，`error: tool 'view' bundle hash mismatch; reinstall it`（拒绝分发） |
| 卸载 | `marqdo tool remove view` | `removed tool 'view'`；`tool list` → `no tools installed` |
| 冻结 CLI SHA | 全流程前后 | 均为 `062cb56e…`，**未变化**；未构建/修改 CLI |

`cargo build --release --locked` 的依赖图不含 `marqdo-cli`/旧 `marqdo` 私有 crate
（`tests/contract/no_coupling.rs` 门禁）。

## 备注

- 包只含 `bin/marqdo-view`、`tool.json`、README/兼容性/许可证与
  `RELEASE_INFO.json`/`SHA256SUMS`；CSS/JS/图片在构建期嵌入二进制（ADR-0002）。
- 安装后若发生篡改必须先 `tool remove view` 再重装（CLI 拒绝覆盖已安装命令），
  这是 Core 的整包校验行为，View 不绕过。
- 远端 CI 配置为 `.github/workflows/ci.yml`；本次证据来自本地可复现命令，
  未声称远端 CI 已运行。
