# Protocol goldens

These Analysis v2 files were emitted by the **frozen** `marqdo` CLI release
binary, not hand-written:

| Binary | SHA-256 |
|---|---|
| `marqdo 3.0.0-beta.1` | `062cb56e226bee55c21d238e195920caa985e62bb3877ba7b07abcc04dc0fe3f` |
| CLI source SHA | `2c7c4f31e7a388d3fcfa717a77067e696ed13854` |

Command shape used for every golden:

```bash
"$MARQDO_BIN" check <example> --emit tests/golden/analysis-v3/<name>.json
```

| Golden | Input |
|---|---|
| `mixed-modes.json` | `examples/mixed-modes/index.mq.md` |
| `control-flow.json` | `examples/control-flow/index.mq.md` |
| `tables-math.json` | `examples/tables-math/index.mq.md` |
| `welcome.json` | `examples/welcome/00-welcome.mq.md` |

`tools/gen-golden.sh` regenerates them from a frozen binary. If an example's
bytes change, its `source_digest` changes too and the tests will fail until the
golden is regenerated — that is intentional evidence, not a test bug.

`examples/diagnostics/index.mq.md` is deliberately invalid and has **no**
golden; its failure path is covered with `check --json` output.
