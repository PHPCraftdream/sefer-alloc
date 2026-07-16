# Crate-extraction — consciously NOT filed (deferred / skipped / in-place)

Companion note to the 10 filed crate tasks (#171–179 = P1–P9). This records
what the first pass over the 5 reports (`01_data_structures`, `02_concurrency`,
`03_os_platform`, `04_test_infra`, `SUMMARY`) consciously did **not** turn into
a crate task, and why. Re-evaluation is task **#180 (CRATE-P10)** →
`docs/crate_extraction/P10_DEFERRED_VERDICT.md` (blocked by #170 R7-FINAL).

## 1. Deferred — file later, after P1–P9, only if there's appetite

- **carved-mem** (01 §3, SUMMARY row 12) — the `Node` raw-memory membrane
  (atomic-views-at-offset, intrusive freelist word, `atomic_ptr_ref`
  exposed-provenance). **Blocker first:** the `'static` lifetime on the atomic
  views is load-bearing for the `#![forbid(unsafe_code)]` upper world; a general
  crate must return a lifetime-parameterized ref / raw handle, which ripples back
  into sefer. The safety-contract rewrite (every `// SAFETY:` proof → generic
  caller obligation) is the real cost. Only as a deliberate follow-up to
  `aligned-vmem` 0.2 ("second half of the vmem unsafe story").
- **intrusive-once-stack** (02 candidate 5, SUMMARY row 13) —
  `deferred_large/{push,drain,tail}` idempotent-push Treiber stack. **Blocker:**
  production stores raw addresses in `AtomicU64` (exposed provenance) + the link
  word doubles as a lifecycle field → needs `AtomicPtr` + node-trait rework,
  loses the address-reuse trickery. Novelty worth keeping = the loom-proven
  double-insert guard.
- **iai-judge** (04 §C4, SUMMARY row 11) — iai-callgrind WSL bridge + marginal
  Ir/op decomposition. Narrow niche. Note: the in-place manifest fix (derive
  `BENCH_OPS` from bench names) is worth doing even if never extracted.
- **criterion-arms** (04 §C3, SUMMARY row 14) — 3-arm normalized bench table.
  Heavy overlap with `critcmp`/`criterion-table`; novel parts thin. Likely
  "document the discipline, extract on demand only."

## 2. Skip — not a crate (per report verdicts)

- **gen-slot** (02 candidate 6, SUMMARY row 16) — the repo itself
  deprecated/retired this tier; depends on non-miri-clean `crossbeam-epoch`.
  Resurrecting retired code. Skip unless external demand.
- **tcache-magazine** (01 §8, SUMMARY row 19) — deliberately trivial (array +
  len); the interesting parts (double-free oracles, flush interplay) live in
  `HeapCore`, not here. Only as a future "pool-building-blocks" family if
  P5/P7 succeed.
- **Bucket (c) / SUMMARY row 20** (unanimous "not crates") — bitmaps
  (`SegmentBitmap`/`AllocBitmap`/`MagazineBitmap`), `SegmentTable` (backward-shift
  hash), `SegmentDirectory`/`PageMap`/`BinTable`, `Segment` newtype,
  `SegmentHeader`, large-segment cache (**also mid-refactor** — untracked
  `alloc_core_{large,small}*` split), xthread state machines, sanitizer runner
  scripts, the unsafe-seam pattern. Reason in each case: too thin, pure internal
  ABI, or value is ~80% convention.

## 3. In-place hygiene — restructuring, not crates

Not filed as its own task in the first pass; partially folded into **#171**
(retire `examples/malloc_macro.rs` duplicate) and **#176** (step 0 = unify the 3
differential-model copies). The residual, to be scoped by #180:

- **Single-source the sanitizer feature matrices** — one
  `scripts/matrix.{json,toml}` consumed by `loom.mjs`/`miri.mjs`/`tsan.mjs` and
  validated against `.github/workflows/ci.yml`. The "MUST mirror" ×5 drift class
  already went stale once (deleted `alloc` feature, task #204).
- **Bench-emitted MANIFEST lines** — kill hand-mirrored constants in
  `scripts/bench-table.mjs` (`OPS=1024`/`SIZES`/`ARMS`) and `scripts/iai.mjs`
  (`BENCH_OPS`); benches emit, scripts derive.
- **Dead-hook detection** — a consistency test that every `#[doc(hidden)] dbg_*`
  accessor is referenced by ≥1 file under `tests/`.
- **Shared `examples/_shared` RSS/commit reader** — mostly absorbed by #178
  (`proc-memstat`); confirm the residual.

## 4. Document as technique (no code extraction)

A proposed `docs/` chapter, not crates: the two-tier confined-unsafe seam +
self-verifying inventory grep + `#[doc(hidden)]` dbg-forwarders; hardening-harness
conventions (zero-runs-is-red, verdict-by-output-scan, WSL sccache/target-dir
traps); state-machine spec-model methodology (`CROSS_THREAD_STATE_MACHINES.md`);
the platform-shim cfg convention ("how to add a platform" page in vmem).
