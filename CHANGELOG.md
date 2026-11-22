# Changelog

All notable changes to `marqdo-view` are documented here. The format follows
Keep a Changelog and the project uses SemVer for the View product version.
Protocol axes (Tool v1, Analysis v2, Trace v2) are versioned separately.

## [Unreleased]

### Added (RFC-VIEW-0002 PR-05)

- Trace v2 evidence binding hardened end to end: the SSE trace summary carries
  per-event `source_id` only for verified entry-module spans, observed chips are
  marked `data-annotation-source="trace"`, static anchors
  `data-annotation-source="analysis"`, and grouped chips collapse >3 events per
  anchor with a real count in `title`.
- Stale/invalid/truncated Trace semantics covered by tests: editing source during
  a run yields `stale=true` with all anchors/source ids null; a mismatched Trace
  digest yields `trace.status=invalid` while the run still succeeds; truncated
  traces are reported without claiming complete coverage.
- ADR-0010 records the annotation/staleness strategy (inline keyed chips instead
  of SVG curves, per RFC §4.4's allowed degradation).
- Browser regression at the RFC width matrix incl. 2048x1133, provenance/grouping
  assertions, dark-theme run screenshot; evidence in `docs/evidence/PR-05/`.

### Added (RFC-VIEW-0002 PR-04)

- `src/server/{mod,routes,security,sse}.rs`: the fixed 7-route Live API with one
  explicit execution entry. Removed `/api/health`, `/api/document`,
  `/api/run` and `/api/clear-run`.
- SSE transport writes the `text/event-stream` response directly to the socket
  and flushes every frame/heartbeat (tiny_http's chunked writer would delay
  frames); heartbeats, `Last-Event-ID` replay, authoritative snapshots, 128-event
  ring, terminal close and a max of two observers per run.
- Per-session `HttpOnly; SameSite=Strict` cookie + per-session CSRF token
  (`X-View-CSRF`), exact same-origin `Origin`, loopback `Host` validation,
  `Sec-Fetch-Site` cross-site rejection, JSON-only bounded run POSTs (64 KiB).
- Reader front-end now runs through one POST + EventSource: target form built
  from Analysis signatures, text-only command preview, pure/project mode with
  explicit fs read/write globs and second confirmation, cancel button, live
  result/stdout/stderr/Trace panel, keyed source anchors (skipped when stale),
  and no status polling. Run shell is hidden until an explicit run.
- New `tests/security/` suite (cookies/CSRF/CORS/symlink/Markdown/SSE session
  isolation) and rewritten HTTP/SSE integration tests; browser suite updated for
  EventSource, dark theme and the RFC width matrix (incl. 2048x1133).
- ADR-0009 records the SSE transport, session model and route decisions; evidence
  in `docs/evidence/PR-04/` includes real Chromium logs/screenshots and a real
  frozen-CLI run (285 Trace v2 events).

### Added (RFC-VIEW-0002 PR-03)

- Execution layer: `src/execution/{request,process,controller,events}.rs`.
  - request validation per §6.3 (size/path/target/parameter/mode/fs grants) and
    exact `function_id` -> CLI name resolution from Analysis v2;
  - one cancellable process adapter (explicit argv, process group,
    SIGTERM→2s→SIGKILL, bounded stdout/stderr, spawn hook, wall clock); the
    CoreClient now uses the same adapter for `check`/`--version`;
  - single active job, `client_request_id` + payload digest idempotency/409,
    unique terminal states, cancellation, 128-event ring buffer with
    `Last-Event-ID` replay/snapshot, and `marqdo.view-run-events` v1 frames;
  - Trace v2 verification against the exact pre-run Analysis bytes with safe
    event summaries; missing/invalid trace is reported without faking evidence.
- Explicit budgets on every run/call (`--fuel`, `--max-output`, `--timeout`,
  trace limits) plus 4 MiB JSON/Trace bounded reads and `view.output_limit`.
- New `--run-timeout` option clamped to 1..=120 seconds (default 30).
- ADR-0008 records lifecycle/cancel-race/Trace-missing semantics.
- Tests: `tests/integration/execution.rs` with a real fake-Core executable
  (accepted/running/terminal SSE-ready events, call argv, dedupe, busy, cancel,
  session isolation, stale digest, grants, unavailable trace).

### Changed (RFC-VIEW-0002 PR-02)

- Restructured the crate into the RFC-VIEW-0002 §5.3 layout: `src/protocol/
  {analysis_v2,trace_v2,tool_v1}.rs`, `src/reader/{source,markdown,model,
  assemble,render,assets}.rs`, `src/config.rs`; `lib.rs` is the composition
  root and `app.rs`/`project.rs`/`semantic.rs`/`trace.rs`/`document.rs`/
  `presentation.rs`/`source_map.rs`/`render.rs`/`assets.rs` are gone.
- Replaced `GET /api/document` with the read-only `GET /api/documents`
  (documents list + current outline/function/analysis summary).
- Reading UI: removed variable/memory placeholder menu entries (one explicit
  "unsupported capability" explanation remains), added a simple dark theme
  toggle with a single token override layer and persisted preference.
- Reorganized tests into Cargo `[[test]]` suites: `tests/contract/` (CLI/Tool/
  Analysis/Trace/coupling/assets) and `tests/integration/` (Live HTTP/renderer/
  Unicode/export); `tests/fixtures`, `tests/golden`, `tests/browser` unchanged.

### Changed (RFC-VIEW-0002 PR-01)

- View product version is now `0.2.0` (`Cargo.toml`, `tool.json`, binary
  `--version`), still independent from the CLI product version.
- Scripts moved from `tools/` to `scripts/` per RFC-VIEW-0002 §5.3; added
  `scripts/verify-frozen-cli.sh` (records version/SHA and fails if the frozen
  CLI changes across a command sequence).
- Added `.github/workflows/ci.yml`: quality lane (fmt/clippy/test/build) plus a
  frozen-CLI acceptance lane that verifies the release archive SHA, runs the
  one-command acceptance and uploads evidence. `.woodpecker.yml` updated to the
  new script paths.
- Tool v1 packaging now emits `0.2.0`; install/dispatch/tamper behaviour is
  re-verified against the frozen `3.0.0-beta.1` CLI.

### Changed (RFC-VIEW-0002 PR-00)

- Adopted `docs/architecture/RFC-VIEW-0002.md` as the sole implementation
  baseline for execution, scope, SSE, UI routes and release gates; RFC-0001 is
  archived as historical design (ADR-0007).
- Moved specs to `docs/architecture/` and decisions to `docs/decisions/`.
- Added `docs/audits/legacy-view.md` (static legacy evidence with
  `KEEP_ASSET/REIMPLEMENT/DROP/UNKNOWN` decisions and `LEGACY_RUNTIME_UNVERIFIED`
  declaration), `docs/audits/protocol-contract-0002.md` (frozen CLI black-box
  contract), `docs/CONFORMANCE.md` (living RFC-VIEW-0002 matrix) and refreshed
  `docs/STATUS.md` with the remaining 0002 gaps.
- No source behaviour changed in this step; the tracked 0001 implementation is
  unchanged and still passes its tests while PR-02..PR-06 migration proceeds.

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
  Trace run page in `docs/evidence/0001-pr03/`.
- Meta CSP no longer carries the ignored `frame-ancestors` directive (the HTTP
  header still enforces framing).

### Added (PR-05)

- Static export (`marqdo view export PATH --out DIR`) through the same
  `ReaderRenderer`: `index.html` + `pages/<relative>.html` + hash-named assets +
  `manifest.json` (relative paths, per-file SHA-256, protocols, `executed:false`).
- Pure read-only guarantee: export never calls Core `run`, never writes into the
  project root, never overwrites unrelated non-empty directories; staging +
  atomic rename, previous successful export replaced safely.
- Offline by construction: same embedded asset bytes, data-URI images, no remote
  fonts/CDN; relative internal links; `--no-annotations` hides semantic anchors
  without changing prose.
- HTTP hardening: bounded concurrent requests (503 beyond 32), detail-free
  public error JSON/pages (no absolute paths or subprocess text), 30s page check
  budget.
- `tools/test-export.sh`, `tools/test-security.sh`, adversarial browser fixture
  (`tests/fixtures/security-project`) proving no script execution and no
  external requests; `tests/export.rs` export contracts; ADR-0005.

### Added (PR-06)

- `tools/acceptance.sh`: one-command RFC §18.1 acceptance running the frozen CLI
  from the release archive in an isolated tool home (fmt/clippy/test/build,
  bundle, archive install, view help/verify/export, Live browse, explicit
  CSRF-confirmed real run, export/security/browser suites, CLI SHA check) and
  generating `docs/evidence/<CLI-SHA>/manifest.json` from real exit statuses.
- `tools/test-contract.sh`, `.woodpecker.yml` (code/contract/export/browser/
  release-bundle lanes; remote CI execution not verified).
- `docs/ACCEPTANCE.md` gate table, `docs/audits/protocol-conformance.md`,
  ADR-0006 (documented RFC deviations), final README/STATUS/COMPATIBILITY.
- Unicode/CRLF/duplicate-text goldens and `tests/unicode_spans.rs` (S-04);
  `tests/presentation_states.rs` degradation matrix (S-05..S-07); hostile
  stdout escaping test (X-05); keyboard accessibility browser checks (U-03).

### Known limitations

- First Beta: run is `pure`-only; project/strict grants UI, SSE streaming,
  cancellation and multi-run history are not implemented.
- Variables/memory/proof states remain `UNSUPPORTED`; debugger/Notebook are
  non-goals.
- No incremental export or multi-project site; one project per invocation.
- Pixel-level comparison with the reference screenshot remains manual
  UNVERIFIED; SVG curve side notes are not implemented (inline keyed chips).
- Remote CI is not executed in this environment; `.woodpecker.yml` is provided
  and local acceptance evidence is complete.
- `project`/`strict` grant UI, streaming output, cancellation and debugger are
  not implemented; variables/memory/proof states remain `UNSUPPORTED`.
- Pixel-level comparison with the reference screenshot remains a manual
  UNVERIFIED item; automated geometry/DOM/network/print/no-JS checks pass.
