# ADR-0012 — rc.4 当前契约取代 rc.3 基线（0.3.0）

- 状态：Accepted（2026-11-25）
- 范围：`marqdo-view 0.3.0` 的公开协议基线、验收证据与文档
- 取代：issue #5 中以 rc.3 为固定 SHA 的验收基线；不取代 issue #5 的门禁编号
  （G01–G12）与 fail-closed 原则

## 背景

`marqdo-view 0.3.0` 的一次性适配最初按 `marqdo 3.0.0-rc.3` 定义（adaptation
issue #4，验收 issue #5）。随后 `marqdo-cli` 的公共静态分析边界工作
（`marqdo-cli` ADR-0069，追踪 issue #10）在 `v3.0.0-rc.4` 原地替换了
`marqdo.analysis` revision 3 的字段集合，仍保持 revision 号为 3：

- `check --emit` 现在直接携带精确 `source`、Markdown `document`、完整标题作用域
  （`heading_span` + 完整 section `span`）、`anchors`、`entry_module_id`、
  `manifest_digest`、`extensions` 与状态化 `references`；
- `inspect --ast` 不再是渲染语义的机器来源，只是人工调试命令；
- 旧 revision-3 字段集合（缺少上述字段）不再被当前 schema/consumer 接受；
- Observation revision 1 仍绑定**普通** `check --emit` 的精确字节摘要；
  `--refs` 检查改变字节，不能替代运行绑定。

因此 issue #5 的 rc.3 SHA 基线不能继续作为 0.3.0 的验收身份。若继续钉 rc.3，
会出现“门禁全绿但不是发布契约”的错误证据。

## 决策

1. 0.3.0 的唯一契约基线升级为 `marqdo 3.0.0-rc.4`：
   - CLI 源码 `452e9a32bd9e40dba427d4180691c58bab74449e`
   - 归档 `marqdo-3.0.0-rc.4-x86_64-unknown-linux-gnu.tar.gz`
     SHA-256 `9c3f166b5e5b8840d39d10019074e4e1c42432fc23e26c69670e64fdcf4e15d8`
   - 归档内二进制 SHA-256
     `a7306962689c635445a59043971ea2eb09298e01feeee81f0ba07b1f3e10306e`
   - 平台 `linux-x86_64-glibc2.39`；产品范围 `>=3.0.0-rc.3,<4.0.0`
2. 验收门禁编号、fail-closed 原则与 manifest schema
   （`marqdo.view.acceptance` revision 1）沿用 issue #5；只把身份字段与
   证据路径改为绑定 rc.4 的源码/归档/二进制 SHA。
3. 静态语义只消费一次 `check --emit`；不运行 `inspect --ast` 作为机器来源。
4. 旧 revision-3 制品（缺少 rc.4 必需字段）显式拒绝并提示重新生成；不保留
   同 revision 的兼容解码、迁移器或双态类型。
5. 旧 rc.3/0.2.0 证据只作历史记录；不得与 rc.4 证据混用或替代。

## 影响

- `scripts/frozen-beta.sh` 钉 rc.4；`scripts/acceptance.sh` 从该脚本读取
  version/source/archive/binary SHA，生成 issue #5 §0 schema 的 manifest。
- `scripts/check-no-legacy.sh`（G01）继续阻止树内出现旧契约、旧 argv、旧 Tool
  协商或双态分支。
- `docs/audits/protocol-contract-0002.md` 的探测基线改为 rc.4，并在无稳定样本的
  项上显式标注 `UNVERIFIED`。
- `docs/STATUS.md`/`docs/ACCEPTANCE.md` 在 rc.4 manifest 生成前不声明 PASS；
  `UNVERIFIED` 不得写成 PASS。
- issue #5 的 G01–G12 定义仍然有效；其中“冻结 CLI 源码/归档 SHA”字段按本 ADR
  的 rc.4 值填写。

## 验证

复现与机器证据见 `docs/ACCEPTANCE.md` 和
`docs/evidence/452e9a32bd9e40dba427d4180691c58bab74449e/manifest.json`（`result: PASS`，G01–G12 全 PASS，Woodpecker #10）：

```bash
scripts/frozen-beta.sh verify /abs/unpacked/marqdo-3.0.0-rc.4-x86_64-unknown-linux-gnu/marqdo
scripts/check-no-legacy.sh
scripts/check-docs.sh
MARQDO_BIN=... scripts/acceptance.sh
```

未运行门禁保持 `UNVERIFIED`；本 ADR 不声称任何尚未执行的验收结果。
