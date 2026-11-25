#!/usr/bin/env bash
# Contract-focused gate: formatting, lints, all tests, CLI Tool revision 1
# install smoke, static verify and static export against the frozen core.
# No browser. Current contracts: Analysis revision 3, Observation revision 1.
#
# Usage: MARQDO_BIN=/abs/path/marqdo scripts/test-contract.sh
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

: "${MARQDO_BIN:?set MARQDO_BIN to the frozen marqdo binary}"
export PATH="${HOME}/.cargo/bin:${PATH}"
export CARGO_HOME="${CARGO_HOME:-$repo_root/.cargo-home}"

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked

MARQDO_BIN="$MARQDO_BIN" ./scripts/package.sh

export MARQDO_TOOL_HOME="$repo_root/dist/contract-tool-home"
rm -rf "$MARQDO_TOOL_HOME"
"$MARQDO_BIN" tool install "$repo_root/dist/marqdo-view/tool.json" >/dev/null
"$MARQDO_BIN" view --help >/dev/null
"$MARQDO_BIN" view verify examples/mixed-modes/index.mq.md
"$MARQDO_BIN" view export examples/tables-math --out "$repo_root/dist/contract-site"
test -f "$repo_root/dist/contract-site/manifest.json"

echo "contract: PASS"
