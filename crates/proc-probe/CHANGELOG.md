# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - 2026-09-09

First release. Everything below is new in this version; nothing has shipped
before it.

### Added

- **The `RESULT key=value` stdout protocol — this crate's whole reason to
  exist.** A *probe* is a tiny binary that a *runner* launches N times as fresh
  OS processes, parsing one machine-readable line per metric out of each run's
  stdout. The line shape is deliberately trivial so a runner can `grep`/parse
  it robustly regardless of surrounding log noise:
  `RESULT <key>=<value>`.
  `<key>` is `[a-z0-9_]+` (ASCII only). `<value>` is one or more characters the
  reference runner's ECMAScript parser treats as non-whitespace — i.e. outside
  ECMAScript `\s` (its WhiteSpace + LineTerminator sets plus the
  Space_Separator category), *not* outside Rust's `char::is_whitespace`; the
  two whitespace models disagree at the boundary (U+FEFF is `\s`-whitespace but
  not Rust-whitespace, U+0085 the reverse), so values involving those code
  points parse differently between a Rust-side model and the real runner.
  Probes should keep keys and values ASCII, where the two models agree exactly
  (the divergence is pinned and node-verified — see Testing).
- **`RESULT_PREFIX`** — the line prefix every emitted metric carries, exposed
  as a constant rather than hard-coded per call site: a runner keys off
  exactly this token. Always available, even under `no_std`.
- **The `emit` family** (requires the default `std` feature) — one function per
  value shape, so a probe never hand-rolls the `println!("RESULT ...")` string
  and never drifts the format the runner parses against:
  **`emit(key, value)`** — the string-valued primitive, emitting one
  `RESULT <key>=<value>` line to stdout; **`emit_u64`**,
  **`emit_i64`** (a signed delta may be negative), **`emit_f64`**
  (the default `f64` `Display`, which never emits whitespace, so the value
  stays a single parseable token), and **`emit_ns`** — a plain-integer
  nanosecond metric for the common `Instant::elapsed().as_nanos()` (`u128`)
  shape, printed at full `u128` width with no narrowing step and no
  hand-rolled `RESULT` formatting at the call site. The family is unchecked:
  the key/value shape is a precondition, and violating it is a caller logic
  error — nothing is validated, sanitized, or truncated. A stdout write
  failure panics, exactly as `println!` does.
- **A re-export of `proc-memstat`'s `snapshot()` and `MemStat` — and of the
  fallible `try_snapshot()` / `SnapshotError` pair** (requires the default
  `std` feature; `proc-memstat` is itself an std-only crate) — a probe almost
  always wants to *measure* memory and then *report* it, and this crate
  re-exports both forms so a probe binary depends on **one** crate for the
  whole "measure + report" pair. The fallible form exists for the paired-A/B
  case where a judge must distinguish a failed reading from a genuinely
  near-zero one: best-effort `snapshot()` returns an all-zero `MemStat` on
  failure, which a bare `RESULT rss_kib=0` cannot tell apart from "memory was
  freed".
- **A genuine `no_std` core with ZERO dependencies.** The `std` feature is
  what pulls in the (optional) `proc-memstat` dependency; with
  `default-features = false` the crate builds with zero dependencies and
  exposes only `RESULT_PREFIX` — a downstream `no_std` probe that supplies its
  own sink can still use the protocol constant and format. Confirmed by
  cross-compiling to `thumbv7em-none-eabi`.
- **`#![forbid(unsafe_code)]`** — the crate holds no `unsafe` of its own; all
  the OS FFI stays confined to `proc-memstat`.

### Testing

`tests/protocol.rs` checks the contract at THREE independent layers, exactly as
its module doc states:

- **Parser-side model** (`line()` / `parse_result()` + the round-trip tests): a
  zero-dependency model of the runner's
  `line.trim()` + `/^RESULT\s+([a-z0-9_]+)=(\S+)$/` contract, exercised for
  self-consistent shape checks (key/value round-trips, malformed-line
  rejection). `line()` rebuilds the expected line from the library's own
  `RESULT_PREFIX`, which is fine here — these tests only pin the model, not the
  library's real bytes.
- **Real emitted bytes** (`real_stdout_emit_family_exact_bytes`): the REAL
  `emit*` functions run in a freshly re-exec'd child process and their stdout
  bytes are compared against independently hardcoded expectations (the literal
  word `RESULT`, not `RESULT_PREFIX`). This closes the round-1 review finding
  (P2-2): the previous suite never inspected real `emit*` output — emptying
  every `emit*` body or renaming `RESULT_PREFIX` to `"RESULTX"` passed every
  test.
- **Framing-proof real-stdout child** (round-2 review P2-1): the re-exec'd
  child now runs with `--format terse`, so libtest's own harness framing can
  never share a physical line with the emitted bytes — the serial-child case
  (an inherited `RUST_TEST_THREADS=1`, or `available_parallelism() == 1`)
  makes the default pretty formatter print `test <name> ... ` without a
  trailing newline before the test body, which silently swallowed the first
  `RESULT` line out of the strict line-filter assertion while the emit bytes
  themselves were exactly correct. The new
  `real_stdout_emit_family_exact_bytes_serial_child` test forces
  `RUST_TEST_THREADS=1` on the CHILD via `.env(...)` and pins exactly that
  condition; the byte assertions themselves are unchanged.
- **Model vs the real ECMAScript parser**
  (`parser_contract_matches_node_ecmascript_regex`): the Rust parser model is
  compared line-by-line against the ACTUAL regex from
  `scripts/paired-ab-runner.mjs:253`, executed by real Node.js. The six
  documented U+FEFF/U+0085 divergences are explicitly documented, pinned, and
  node-verified rather than claimed away as "identical" (round-1 review
  finding P3-2). When the runner source or `node` is genuinely absent
  (`ErrorKind::NotFound` — a published-crate checkout has no `scripts/`) the
  test SKIPs with a stderr notice, never fails; any other spawn error
  (e.g. PermissionDenied: found but not executable) hard-fails instead of
  being misclassified as an absent optional tool (round-2 review P3-2a).
  Skip notices — and the final node-verification success diagnostic — are
  direct `std::io::stderr()` writes, so they remain visible in ordinary CI
  logs without `--nocapture` despite libtest capture (round-2 review P3-2b).
- **Active-contract drift guard** (round-2 review P3-1): the guard no longer
  substring-checks a hand-copied regex body — it extracts the runner's
  ACTIVE contract (regex literal body AND flags, plus whether `.trim()`
  precedes `.exec()`) and compares named fields, failing loudly on drift;
  the Node check builds `new RegExp(body, flags)` and applies trim exactly
  as the runner does. Two negative controls pin the previously-invisible
  drifts (added `i` flag, removed `.trim()`).
- **Visible, correctly-classified skips** (round-2 review P3-2): the
  permissive skip path is narrowed to `NotFound`, other spawn errors
  hard-fail, and both skip notices plus the success diagnostic bypass
  libtest capture via direct stderr writes; the new
  `skip_notice_survives_libtest_capture` test pins the capture-visibility
  counterfactually, and the CI `proc-probe-gates` job asserts node and the
  runner script up front so the skip path is structurally impossible there.

`try_snapshot_re_export_reachable` additionally proves the fallible
`try_snapshot()`/`SnapshotError` re-export is usable through `proc-probe`
alone (round-1 review finding P3-4).

The crate ships no doctests (repo convention); runnable examples live in
`tests/`.
