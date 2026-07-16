# Crate extraction — consolidated synthesis (SUMMARY)

Synthesis of the four lane reports (2026-07-16): `01_data_structures.md`,
`02_concurrency.md`, `03_os_platform.md`, `04_test_infra.md`. De-duplicated,
ranked, decision-ready.

## Executive summary

The four lanes converge on a clear picture: the two cheapest, highest-value
moves are not extractions at all — **publish what is already extracted**
(`malloc-bench-rs` is publish-ready today; `aligned-vmem` needs a small 0.2 —
real `page_size()`, fallible API, a mock/fault-injection feature — before
publishing; `numa-shim` just needs publication). Among genuinely new crates,
the strongest candidates are the ones the repo itself already reuses ≥3 times
(`racy-ptr-cell` — the CAS-once cell transcribed into 4 loom models; the
Vyukov MPSC ring living twice as `RemoteFreeRing`/`HeapOverflow`) plus two
low-coupling gems (`size-classes`, `tagged-index-stack`) and the
test-infrastructure twin pair (`globalalloc-model` differential harness,
`proc-memstat`/`paired-ab` process probes). The decisive testability argument,
made independently by two lanes: every loom harness today is a **shadow model
transcription** that can silently drift from production code; extraction with
`#[cfg(loom)]` atomic aliasing turns those models into executable proofs of
the real implementation. Equally consistent anti-finding: the drift disease
(3 copies of the differential model, triplicated FFI probes, hand-mirrored
bench constants, dual feature matrices) is mostly cured by in-place
restructuring, not by crates. One blocker to respect: `alloc_core_large*` /
`alloc_core_small*` are an untracked in-flight split of `alloc_core.rs` —
nothing touching that area is a stable extraction target until the refactor
lands.

---

## 1. Master ranked table (all distinct candidates, de-duplicated)

Legend: value/testability H/M/L; effort Low/Med/High. "Lanes" = which reports
proposed it (DS=01, CC=02, OS=03, TI=04).

| # | Candidate | What / source | Community value | Testability gain (this repo) | Effort / coupling | Risk / notes | Lanes |
|---|-----------|---------------|-----------------|------------------------------|-------------------|--------------|-------|
| 1 | **Publish `malloc-bench-rs`** | larson/mstress MT macro-bench harness, `crates/malloc-bench` (already extracted, publish-ready metadata) | **H** — the pure-Rust `mimalloc-bench` | M — add per-thread pin hook → retire `examples/malloc_macro.rs` duplicate (task #28 drift) | **Near zero** (publish) + days (pin hook) | None; cheapest win in the whole survey | TI |
| 2 | **`aligned-vmem` 0.2 — extend + publish** | `crates/vmem` (~1080 lines, zero deps). 0.2 = real `page_size()` (16 KiB macOS!), `Result<_, VmemError>`, ungate lazy-commit, `MADV_FREE` decommit variant, optional huge pages | **H** — unoccupied niche: aligned anonymous VM + commit control; `memmap2`/`region`/`mmap-rs` don't cover it | H — own CI matrix; macOS/aarch64 legs become meaningful | Small–Med / ~zero coupling | `page_size()`=4096 hardcode is a **correctness bug on macOS** — fix before publishing | OS |
| 3 | **vmem `mock` feature: recording mock + fail-N-th-commit injection** | Merge `crates/numa` `mod mock` pattern + `src/alloc_core/os.rs::COMMIT_FAIL_ARMED` (R7-B2) into `aligned-vmem` | M-H — "deterministically test your OOM path"; no competing VM crate offers it | **Highest in the OS lane** — OOM paths get a supported API; vmem contract tests run under miri | Small / low | Absorbs the R7-B4 plan item once, for everyone | OS |
| 4 | **`racy-ptr-cell`** — lazy CAS-published cell with OOM rollback + re-race | `src/registry/bootstrap.rs` (3 instances: registry, `ensure_chunk`, overflow sidecar) + `src/global/fallback.rs::heap_ptr` | **H** — the `#[global_allocator]`-safe `OnceLock`: no_std, allocation-free, fallible init with rollback (OnceLock poisons; this retries) | H — 4 loom models (`loom_bootstrap_cas`, `loom_chunk_cas`, `loom_overflow_sidecar_cas`, `loom_fallback_init`) collapse into one suite against the **real** type | **Low** (~100 lines) / low — init fn becomes a closure | Must document spin-wait (no parking). Strongest internal-reuse signal (4 copies) | CC |
| 5 | **`ring-mpsc`** — bounded MPSC index ring over owned OR borrowed memory (+ optional `DirtyRouter` module) | Merged DS§4 + CC§1: `src/alloc_core/remote_free_ring.rs` (in-place over raw segment metadata) + `src/registry/heap_overflow.rs` (safe owned-array, two-word pair-publish); DirtyRouter from `heap_slot.rs`/`heap_core_xthread.rs` (CC§4) | **H** — allocation-free, raw-memory-capable (shared-memory IPC, DMA mailboxes, in-arena queues), loom-proof-carrying; crossbeam/heapless/rtrb all own their storage | **H** — 5–7 loom models + 4 counterfactuals ship against the real type (`loom_remote_ring*`, `loom_heap_overflow*`, `loom_overflow_first_retry`, `loom_dirty_*`) | Low–Med (CC view: generalize `RingEntry`) to Med-High (DS view: strip dirty-bit/generation bolt-ons) | DirtyRouter's contract is honest "at-least-once, bounded deferral" — needs precise docs; hardened-mode stamping stays behind | DS+CC |
| 6 | **`size-classes`** — const-built size-class table + O(1) `SIZE2CLASS` + alignment-jump classifier | `src/alloc_core/size_classes.rs` (~500 lines, pure safe `const` arithmetic) | **H** — every slab/pool/arena reinvents this trio, usually with a wrong alignment story; nothing on crates.io | **H** — 3 test files move verbatim; parameterized property-testing becomes possible (in-tree constants are baked) | Low–Med / ~zero (2 threads to cut: `HUGE_THRESHOLD`, `MIN_BLOCK ≥ NODE_SIZE` assert) | Worthwhile extra: const-generic builder (min_block, growth, extras) — that is the "Med" part | DS |
| 7 | **`tagged-index-stack`** — ABA-tagged Treiber free-index list | Merged DS§2 + CC§3: `src/registry/tagged_ptr.rs` (packed index:16 \| tag:48) + `heap_registry.rs` `pop/push_free_slot` (H-2 empty-tag fix, RAD-1 lazy `next_free`) | M-H — canonical slot-recycler for slabs/pools/ECS/id-allocators; the empty-tag-reset bug (H-2) is exactly what people get wrong | **H** — 680-line loom model + untagged counterfactual + 48-bit wrap test run against real code; deletes the `dbg_*` forwarder wart | Low–Med / low (links via trait or owned `[AtomicU32; N]`) | `TaggedPtr` alone is too small — only earns extraction together with the stack protocol | DS+CC |
| 8 | **`globalalloc-model`** — differential op-stream harness for any `GlobalAlloc` | `tests/alloc_core_differential.rs` + `tests/heap_differential.rs` + `fuzz/fuzz_targets/global_alloc_ops.rs` — **three drifted copies** of one model (M1–M4 oracles) | **H** — "proptest + cargo-fuzz your allocator in 10 lines"; correctness twin of malloc-bench; nothing comparable exists | **H** — one oracle to harden; improvements reach proptest, miri, and libFuzzer at once (the realloc-tail fix already had to be hand-mirrored once) | Low–Med (1–2 days) / low — model needs only the `GlobalAlloc` surface | In-place unification of the 3 copies is step 0 regardless of publication | TI |
| 9 | **`proc-memstat` / `proc-probe`** — same-instant RSS + commit-charge + peak self-probe (+ RESULT protocol) | Merged OS§3 + TI C5(a): `examples/first_alloc_process.rs:132-317` — FFI struct **copy-pasted 3×** in one example; readers also hand-rolled in 3 probe binaries | **H** — commit charge is almost never surfaced by existing crates (`sysinfo` is heavy/whole-system); catches what RSS hides | M — probe gets its own tests; examples shrink; triplication dies | Low (a weekend) / zero | Add macOS `task_info` aperture and Linux `VmHWM`; single-snapshot API for apples-to-apples | OS+TI |
| 10 | **`paired-ab`** — process-level A/B/B/A paired judge with t-test + same-vs-same control | `scripts/paired-ab-runner.mjs` + probe examples (TI C5) | H — what every "my allocator is faster" claim should be backed by; criterion structurally can't measure per-process effects | Mostly banked already (it IS the reusable form of prior one-offs) | Med (3–5 days for the family) / low in the runner | Depends on #9 as its measurement half | TI+OS |
| 11 | **`iai-judge`** — iai-callgrind WSL bridge + marginal Ir/op decomposition | `benches/perf_gate_iai.rs` + `scripts/iai.mjs` (452 lines; sccache/target-dir/runner-pin traps) | M-H in a narrow niche (perf-gated crates, Windows devs); the marginal-Ir/op technique deserves a write-up regardless | M — derive `BENCH_OPS` from bench names instead of mirroring | Med (2–3 days) / med | The in-place manifest fix is worth doing even if never extracted | TI |
| 12 | **`carved-mem`** — the `Node` raw-memory membrane (atomic views at offsets, intrusive freelist word, exposed provenance) | `src/alloc_core/node.rs` (~600 lines, tier-1 unsafe seam) | M — narrow audience (allocator/arena/shm authors) but exactly the audience that gets provenance wrong; "second half of the `aligned-vmem` story" | M — each primitive gets direct miri coverage (esp. `atomic_ptr_ref`) | **Med, but contractually substantial** — every SAFETY proof leans on allocator invariants and must be rewritten as generic caller obligations | The `'static` atomic-view lifetime is load-bearing for the `#![forbid(unsafe_code)]` upper world — resolving it ripples back into sefer. Deliberate follow-up only | DS |
| 13 | **`intrusive-once-stack`** — idempotent-push intrusive MPSC Treiber stack (double-free → detected no-op) | `src/alloc_core/deferred_large/{push,drain,tail}.rs` (A1 hardening, two sentinels) | M — intrusive stacks exist (`cordyceps`); the loom-proven double-insert guard is the novelty | M — `loom_deferred_large` ships against real code | Med / med-high — production stores raw addresses in `AtomicU64` (exposed provenance); needs `AtomicPtr` + node-trait rework | Provenance rework loses the address-reuse trickery; the link word doubles as a lifecycle field | CC |
| 14 | `criterion-arms` — 3-arm normalized bench table | `scripts/bench-table.mjs` + `benches/global_alloc.rs` | M-L — overlaps `critcmp`/`criterion-table`; the novel parts (per-op normalization, arm-ratio, completeness gate) are thin | H **in-place**: emit a MANIFEST line from the bench, kill the hand-mirrored constants | Med / med-high | Extract only on demand; the discipline is better told as documentation | TI |
| 15 | `dirty-bits` standalone | `heap_slot.rs` dirty bitmap router | M | H (2 loom models) | Med | Best shipped **inside `ring-mpsc`** (row 5), not standalone | CC |
| 16 | `gen-slot` — generation-CAS seqlock slot | `src/concurrent/hand.rs` + friends | L-M — the repo itself **deprecated/retired** this tier | L | Low (self-contained) | Would be resurrecting retired code; depends on non-miri-clean crossbeam-epoch. Skip unless external demand | CC |
| 17 | `vmem::leak_zeroed_pages` — leaked zero-init VM sidecar | Pattern used 3× (`registry_chunk`, overflow sidecar, `reserve_directory_sidecar`) → one fn in `aligned-vmem` | M — GlobalAlloc/pre-main authors, exact vmem audience | Modest — the miri-zeroing invariant proven once instead of per-site | **XS** | Extract the reservation pattern only; ownership disciplines stay with consumers | OS |
| 18 | First-touch fresh-process bench harness | `scripts/first-alloc-bench.mjs` methodology (OS§5 = TI C5 runner half) | M — under-served, but audience narrow | None beyond today | Med | **Defer** until #9 ships and gets traction | OS+TI |
| 19 | Tcache magazine + byte-budget refill policy | `src/registry/tcache.rs` (D3 policy) | L-M | L | Low but pointless alone | Only as a future "pool-building-blocks" family if #6/#7 succeed | DS |
| 20 | **Not crates** (unanimous): bitmaps (`SegmentBitmap`/`AllocBitmap`), `SegmentTable` (backward-shift hash), `SegmentDirectory`/`PageMap`/`BinTable`, `Segment` newtype, `SegmentHeader`, large-segment cache, xthread state machines, sanitizer runner scripts, unsafe-seam pattern | various | — | — | — | Too thin, pure internal ABI, or value is 80% convention. Large cache additionally **mid-refactor (untracked files)**. See bucket (c) | all |

---

## 2. Cross-cutting themes (multi-lane convergence)

1. **Shadow-model drift → extraction as verification upgrade.** All 18 loom
   harnesses are transcriptions, not the production types (deliberate: the real
   types sit on raw segment memory loom can't host). Both DS and CC lanes
   independently conclude that extraction with `#[cfg(loom)]` atomic aliasing
   makes the shipped loom proofs exercise the **real implementation** — the
   single biggest testability gain available, and simultaneously the
   community pitch ("lock-free crates with executable loom proofs +
   `#[should_panic]` counterfactuals are rare on crates.io").
2. **Extend + publish beats new micro-crates.** The OS lane's headline:
   candidates 1–4 of that lane all land as `aligned-vmem` 0.2 features, not
   separate crates. Same shape in TI: `malloc-bench-rs` is done — just
   publish. `numa-shim` likewise. Avoid fragmenting one clean seam into five
   supply-chain slots.
3. **Internal reuse count is the best generality signal.** The top new-crate
   candidates are precisely the protocols the repo already instantiated ≥3
   times: racy CAS-cell ×4, MPSC ring ×2 (+5 loom models), zeroed-sidecar
   pattern ×3, differential model ×3, RSS/commit FFI ×3(+3).
4. **The drift disease is systemic and mostly cured in-place, not by crates:**
   3 differential-model copies (already drifted once), `malloc_macro.rs`'s
   documented duplicate workload, hand-mirrored constants in
   `bench-table.mjs`/`iai.mjs`, the sanitizer feature matrices duplicated
   between `scripts/*.mjs` and `ci.yml` ("MUST mirror" ×5, already went stale
   once), triplicated probe FFI. Single-source each of these regardless of
   any extraction.
5. **Some of the best material is technique, not code:** the two-tier
   confined-unsafe seam with its self-verifying inventory grep, the
   `#[doc(hidden)] dbg_*` test-hook pattern, zero-runs-is-red /
   verdict-by-output-scan, the WSL sanitizer traps, the state-machine
   spec-model methodology (`CROSS_THREAD_STATE_MACHINES.md`). A docs chapter
   or blog write-up, not crates.
6. **One timing blocker:** `src/alloc_core/alloc_core_{large,small}*.rs` are
   an untracked, in-flight split of `alloc_core.rs`. Anything in that
   neighborhood (large-segment cache; to a lesser degree the ring bolt-on
   disentangling) waits for the refactor to land.

---

## 3. Recommended priority order

1. **Publish `malloc-bench-rs`** (+ per-thread pin hook, retire
   `examples/malloc_macro.rs` duplicate). Effort ≈ days, value high, zero
   risk — it is already extracted.
2. **`aligned-vmem` 0.2 and publish**: real `page_size()` (correctness fix),
   `try_*` Result API, ungate lazy-commit, `MADV_FREE` variant, plus the
   `mock`/fault-injection feature (absorb `COMMIT_FAIL_ARMED`) and
   `leak_zeroed_pages`. One coherent small release; the mock feature is the
   biggest OS-lane testability item. (Publish `numa-shim` in the same batch.)
3. **`racy-ptr-cell`** — lowest effort of the new crates (~100 lines), high
   value, deduplicates 4 in-repo instances, collapses 4 loom models into one
   real-type suite.
4. **`ring-mpsc`** (with `DirtyRouter` as a module) — the strongest external
   pitch (raw-memory MPSC + loom proofs); start from the safe `HeapOverflow`
   shape, add the `over_raw` in-place tier. Slightly gated by the small/large
   refactor around `RemoteFreeRing`'s call sites — the crate can be built
   from the protocol now, the in-tree swap waits for the refactor.
5. **`size-classes`** — near-zero coupling, tests move verbatim; the
   const-generic builder is the only real work.
6. **`globalalloc-model`** — do the in-place unification of the 3 copies
   first (that is step 0 and pays for itself), then publish the harness.
7. **`tagged-index-stack`** — small, safe, upgrades the 680-line loom
   transcription to a real-type proof.
8. **`proc-memstat` → `paired-ab`** — weekend crate first, runner family
   after; defer the first-touch harness until the probe has traction.
9. **In-place hygiene batch** (no publication): single-source the sanitizer
   matrices (`scripts/matrix.json` validated against `ci.yml`); bench
   MANIFEST lines to kill mirrored constants in `bench-table.mjs`/`iai.mjs`;
   dead-hook detection for `dbg_*`; shared RSS readers in `examples/_shared/`.
10. **Later / conditional:** `carved-mem` (only after resolving the
    `'static`-view question; the safety-contract rewrite is the real cost),
    `intrusive-once-stack` (provenance rework), `iai-judge`,
    `criterion-arms` (on demand). **Skip:** `gen-slot` (retired tier),
    tcache magazine, and all of bucket (c) below as crates.

---

## 4. Three buckets

### (a) Extract as a NEW community crate
`racy-ptr-cell`, `ring-mpsc` (+`DirtyRouter` module), `size-classes`,
`tagged-index-stack`, `globalalloc-model`, `proc-memstat`, `paired-ab`.
Conditional/later: `carved-mem`, `intrusive-once-stack`, `iai-judge`,
`criterion-arms`. Skip: `gen-slot`.

### (b) Extend + publish an EXISTING crate
- **`aligned-vmem` 0.2**: page-size honesty, fallible API, ungated
  lazy-commit, `MADV_FREE`/huge-page options, `mock` + fail-N-th-commit
  feature, `leak_zeroed_pages`. Then publish.
- **`malloc-bench-rs`**: add the per-thread pin hook; publish as-is otherwise.
- **`numa-shim`**: nothing to change; publish.

### (c) NOT a crate — restructure in-place or document as technique
- **Restructure:** unify the 3 differential-model copies; single-source the
  sanitizer feature matrices (scripts + ci.yml); bench-emitted manifests
  replacing mirrored constants in `bench-table.mjs`/`iai.mjs`; retire the
  `malloc_macro.rs` duplicate via the pin hook; dead-`dbg_*`-hook consistency
  test; one shared RSS/commit reader module for the probe examples.
- **Document as technique:** two-tier confined-unsafe seam + self-verifying
  inventory grep + `#[doc(hidden)]` dbg-forwarders; hardening-harness
  conventions (zero-runs-is-red, verdict-by-output-scan, WSL
  sccache/target-dir traps); state-machine spec-model methodology; the
  platform-shim cfg convention (a "how to add a platform" page in vmem).
- **Keep in-tree, no action:** bitmaps, `SegmentTable`, `SegmentDirectory`,
  `PageMap`/`BinTable`, `SegmentHeader`, `Segment` newtype, large-segment
  cache (also blocked on the in-flight `alloc_core` split), xthread
  ownership protocols.
