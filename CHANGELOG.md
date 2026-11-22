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

### Added (PR-02)

- Reader-oriented `DocumentModel` over the frozen `marqdo.md.v1` Comrak profile:
  headings, GFM tables/task lists, blockquotes, code fences, math literals,
  exact UTF-8 byte spans, source line index.
- Typed `SemanticProjection` with full Analysis v2 validation: schema/product/
  profile checks, project identity, exact `source_digest`, unit/function/
  statement/resource/import/reference/call/diagnostic projection and
  per-span UTF-8 boundary validation with explicit warnings.
- One `PresentationModel` + one `ReaderRenderer`; Live and Export share the same
  bytes and differ only by `LinkPolicy` and control availability.
- Single `assets/reader.css` + `assets/reader.js` theme (layered tokens, warm
  paper, serif prose, gold/olive semantics, source pane, dialogs, responsive,
  print, reduced motion) and `src/assets.rs` SHA-256 asset bundle.
- Local Live reader: loopback Host validation, strict request limits,
  single-pass percent decoding, safe project paths, CSP/security headers,
  read-only `/`, `/file`, `/assets/*`, `/api/document`, `/api/health`.
- `AppContext` degradation: missing core → prose-only with `CORE_UNAVAILABLE`;
  failed check → diagnostics + revoked semantics; protocol mismatch → hard error.
- Examples (`welcome`, `mixed-modes`, `control-flow`, `tables-math`,
  `diagnostics`) and real Analysis v2 goldens generated from the frozen CLI.
- Tests: rendering contracts, XSS/URL sanitization, Live/Export link parity,
  raw HTTP server security tests, asset integrity, architectural gates.

### Added (PR-04)

- `trace.rs`: Trace v2 validation (schema/product, `analysis_digest`,
  `root_id`, module references, strictly increasing seq, span bounds) and
  projection with explicit `NOT_RUN/STATIC_ONLY/RUNNING/SUCCEEDED/FAILED/
  STALE/TRUNCATED/UNSUPPORTED` states.
- Explicit-run flow: in-page confirmation, CSRF token + strict Origin check,
  JSON body limits, one active run per session, `pure`-only mode, process-group
  timeout, run cache keyed by validated Analysis digest.
- Server-rendered run panel (stdout/stderr escaped, Core error code, public
  final value only, event list capped with explicit truncation) and inline
  run chips anchored to real statement spans; never per-assignment values.
- Real Trace v2 goldens from the frozen CLI (success, truncated, failed arity)
  and contract/HTTP tests proving refresh never runs and rejected requests
  never spawn a process.
- ADR-0004 for the run/CSRF/cache design.

### Added (PR-03)

- Browser acceptance harness (`tests/browser/reader.browser.cjs`,
  `tools/test-browser.sh`) against real Chromium + the frozen core: single
  CSS/script, no external requests, no console errors, document/source switch,
  search, settings, 7 viewport widths without horizontal overflow or dock
  clipping, no-JS reading, print mode, explicit run + clear.
- Screenshot evidence for the Welcome page (1664×920, 390×844) and a real
  Trace run page in `docs/evidence/PR-03/`.
- Meta CSP no longer carries the ignored `frame-ancestors` directive (the HTTP
  header still enforces framing).

### Known limitations

- `export` still returns `view.unsupported` (PR-05).
- `project`/`strict` grant UI, streaming output, cancellation and debugger are
  not implemented; variables/memory/proof states remain `UNSUPPORTED`.
- Pixel-level comparison with the reference screenshot remains a manual
  UNVERIFIED item; automated geometry/DOM/network/print/no-JS checks pass.
