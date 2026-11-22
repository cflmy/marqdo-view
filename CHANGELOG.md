# Changelog

All notable changes to `marqdo-view` are documented here. The format follows
Keep a Changelog and the project uses SemVer for the View product version.
Protocol axes (Tool v1, Analysis v2, Trace v2) are versioned separately.

## [Unreleased]

### Added

- PR-00: legacy View static audit (`docs/audits/legacy-render-paths.md`), visual
  tokens, run-state matrix, ADR-0001/0002, MIT/Apache-2.0 licensing, STATUS.
- PR-01: single Rust package with a testable library boundary; `marqdo-view`
  CLI (`browse`, `export`, `verify`) owned entirely by View; Tool Protocol v1
  manifest (`tool.json`) and packaging/install smoke scripts.
- PR-01: `PublicCoreClient` over `MARQDO_BIN` with explicit argv arrays, private
  0700 scratch artifacts, wall-clock timeout that kills the whole process group,
  product-range and protocol negotiation, and stable `view.*` error codes.
- PR-01: `verify` command/use case that checks CLI version, Analysis v2 schema,
  Markdown profile, project identity and exact source digest without executing.
- PR-01: contract tests with a real fake-core executable (argv, exit codes,
  stderr preservation, missing emit, timeout, invalid JSON) and architectural
  coupling gates.

### Known limitations

- `browse` and `export` currently return `view.unsupported`; they land in
  PR-02/PR-03 (reader) and PR-05 (export).
- The single `assets/reader.css` + `assets/reader.js` theme is not present yet.
- `verify` uses a structural binding check; the typed `SemanticProjection`
  replaces it in PR-02 with the same external result.
