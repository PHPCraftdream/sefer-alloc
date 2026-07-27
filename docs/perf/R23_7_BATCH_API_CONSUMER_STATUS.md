# R23-7 — `batch-api` downstream-consumer status: decision record

**Task:** R23-7 (task #376), from an independent read-only review
(`docs/reviews/2026-07-26-r22-readonly-review.md` §4.6): the batch API has a
measured win but no real downstream caller, so for a typical Box/Vec-shaped
single-object workload its effect is exactly zero.
**DECISION-ONLY.** No `src/` change, no `Cargo.toml` change, no new
benchmark, no new measurement. Re-cites existing numbers; does not re-run
them.
**Date:** 2026-07-27. **Base revision:** `main` @ `ff48029` (R22-15, the tip
at the time this task started).

---

## 0. Headline

**No new benchmark is being built.** The realistic-consumer-shaped
benchmark the review's §4.6 asks for **already exists and already ran**:
`docs/perf/R10_7_BATCH_WARM_ARM.md` (task R10-7) built the tcache-aware
`batch_tcache` path — which goes THROUGH the warm magazine, not around it —
and measured it against the real, warm `SeferAlloc::alloc`/`dealloc` scalar
path across a batch-size sweep (N = 8, 16, 32, 64, 1024) at three sizes
(16/64/256 B). That is a more realistic harness than the "call
`alloc_batch(1024)` in a tight loop" shape this task was tasked with looking
past. Building a fourth generation of the same microbench would not add
information; it would be Option A performed for its own sake, which this
project's own house style (and this task's brief) says to avoid.

What the review's §4.6 point actually names — and what remains genuinely
unanswered — is **demand**, not mechanism: nothing in this codebase, its
`crates/` workspace, or any example calls `alloc_batch`/`dealloc_batch` for
its own internal purposes. That is a fact about this repository's callers,
not something a new benchmark could produce evidence for.

---

## 1. What already exists (confirmed by reading, not assumed)

Three prior perf-gate reports form a complete arc on this exact question:

1. **R8-7** (`R8_7_BATCH_CEILING_MEASUREMENT.md`) — measured a **ceiling**
   using internal `AllocCore::refill_class_bump`/`flush_class` primitives
   directly (bypassing `SeferAlloc`/TLS/magazine entirely), batch=1024 only.
   Result: 2.73×/1.71×/1.20× at 16/64/256 B. Explicitly scoped as a ceiling,
   not a caller-realistic number (§4 of that report).
2. **R9-9** (`R9_9_BATCH_BENCH_FOLLOWUP.md`) — added the batch-size sweep (N
   = 8, 16, 32, 64, 1024) AND a third arm using the real, warm
   `SeferAlloc`/`GlobalAlloc` scalar path. Found the ceiling degrades sharply
   at realistic N, and — decisively — the real warm `SeferAlloc` scalar path
   (tcache hit) is **2-30× faster** than the cold `AllocCore` batch
   primitive at every realistic N. Verdict: **CONDITIONAL-NO-GO** for
   realistic callers, based on an *inference* that even a warmed batch
   primitive could not close that gap (§3.2, admittedly not directly
   measured — see its own §5 caveat).
3. **R10-7** (`R10_7_BATCH_WARM_ARM.md`) — built the warm arm R9-9's §5
   flagged as missing, AND implemented the realistic design (`batch_tcache`:
   drain the warm magazine first, batch-refill only the remainder) that
   became today's shipped `HeapCore::alloc_batch`/`SeferAlloc::alloc_batch`.
   Measured `batch_tcache` against the real warm `SeferAlloc` scalar path
   across the same (size, N) grid. **Result: batch_tcache beats the real
   scalar path by 1.1×-1.6× at every measured (size, N), including the
   smallest realistic batch (N=8).** R9-9's no-daylight inference was
   empirically refuted, not confirmed.

R10-7's `batch_tcache` arm — going through the warm magazine, measured
against the real `SeferAlloc` scalar entry point, swept across realistic
batch sizes — **is** the "batch-oriented operation exercised the way a real
caller might" shape this task's Option A describes. It is not a
"loop-calling-the-raw-FFI-shaped-batch-fn" microbench; it is the shipped
implementation measured against the shipped scalar alternative. There is
nothing left to cheaply build that would be a materially different,
more-realistic harness than what R10-7 already ran. A hypothetical fourth
benchmark could only vary consumer *shape* (e.g. "bulk-deserialize a Vec of
records") — but without a concrete real consumer to model that shape after,
any such benchmark would itself be an invented synthetic caller, exactly the
circularity R8-7 §1 already flagged and this task's own brief warns against
manufacturing.

**Conclusion: Option A's prerequisite ("find or cheaply build something
genuinely MORE realistic than what already exists") is not satisfiable.**
What exists (R10-7) already clears that bar; nothing cheaper or more
realistic remains to build. Proceeding to Option B.

---

## 2. Why no real downstream consumer exists today

Confirmed directly, not assumed:

- `alloc_batch`/`dealloc_batch` are `pub unsafe fn` on `SeferAlloc`
  (`src/global/sefer_alloc.rs:482,522`), gated behind the `batch-api`
  feature.
- `batch-api = ["experimental", "alloc-core"]` (`Cargo.toml:214`) — nested
  under the crate's `experimental`/no-semver-guarantees umbrella since
  R12-12 (`a7db75a`), specifically because two independent reviews (R10,
  R12) found `#[doc(hidden)]` alone was not a strong enough "this is
  unstable" signal.
- `batch-api` is **not** part of `production` (`Cargo.toml:399`;
  `production = ["alloc-global", "alloc-xthread", "alloc-decommit",
  "fastbin", "alloc-segment-directory", "primordial-lazy-commit",
  "class-aware-dirty"]` — no `batch-api`).
- Grepped the whole crate for actual call sites of `.alloc_batch(`/
  `.dealloc_batch(` outside the functions' own definitions
  (`grep -rn "\.alloc_batch(\|\.dealloc_batch(" src/` excluding the `fn
  alloc_batch`/`fn dealloc_batch` declaration lines themselves): the ONLY
  call sites are `src/global/sefer_alloc.rs:485,490,527,533` —
  `SeferAlloc::alloc_batch`/`dealloc_batch` calling straight through to
  `HeapCore::alloc_batch`/`dealloc_batch` (the `#[doc(hidden)]` registry
  layer) and the `fallback::with_heap` fallback path. **No other `src/`
  file, no `crates/` workspace member, no `examples/` file calls into this
  API for its own internal work.** This is a materially different situation
  from the internal `refill_class_bump`/`flush_class` primitives R8-7
  measured, which genuinely ARE used in production today (magazine-miss
  refill in `src/registry/heap_core_alloc.rs:437`; magazine-overflow flush
  in `src/registry/heap_core_free.rs:376` and thread-heap teardown in
  `src/registry/heap_core_tcache.rs:101`) — those are load-bearing
  production mechanisms; `alloc_batch`/`dealloc_batch` are a leaf API with
  no caller of their own.
- The only other references to `alloc_batch`/`dealloc_batch` in the tree are
  test files exercising the API's own correctness
  (`tests/batch_tcache.rs`, `tests/r10_7_alloc_batch_xthread_double_free.rs`,
  `tests/r11_4_dealloc_batch_*.rs`, `tests/r11_2_overflow_drain_*.rs`) and
  the bench file measuring it (`benches/global_alloc.rs`'s
  `bench_batch_ceiling_followup` group) — none of these are "downstream
  consumers" in the sense the review means; they are this feature's own
  test/measurement harness, testing itself.

This confirms the review's factual premise exactly: the batch API is
opt-in, explicitly marked unstable, absent from `production`, and unused by
any of this crate's own production code paths. For a typical `Box`/`Vec`
allocation workload (which never opts into `batch-api` and never calls
`alloc_batch` directly), this feature's effect is exactly zero — not
because the mechanism doesn't work (R10-7 measured that it does), but
because nothing invokes it.

---

## 3. What the measured numbers mean, and don't mean

**R10-7's 1.1×-1.6× figure (and R8-7/R9-9's earlier, less-realistic
batch=1024 ceilings) remain true AS MEASURED.** This is not a retraction of
any prior report. The `batch_tcache` mechanism genuinely beats the real
warm `SeferAlloc` scalar path at every measured (size, N), including small,
realistic batch sizes — that is a real, reproducible, honestly-measured
result (7 correctness tests green, isolated-run methodology documented in
R10-7 §2.3 to avoid a harness ordering confound).

**What it does NOT establish:** that this win translates into any actual
end-to-end benefit for a real program. A benchmark can only measure "if you
call this API this way, it is this much faster than the alternative" — it
cannot manufacture evidence that any real workload *would* call it that way.
R10-7 §3 itself says this explicitly ("Ceiling, not a shippable-API
forecast... a shipped API pays extra argument-validation overhead the raw
internal call does not"), and R8-7 §1 named the underlying circularity
concern before any of these three reports were written. This decision
record does not change that scope — it confirms the scope was already
correctly stated, and formalizes that the missing piece (a real caller) is
still missing three rounds later.

---

## 4. Falsifiability clause — what would make this actionable again

This record closes the "should we build another batch-API benchmark, or
chase a consumer speculatively" question for future rounds absent one of
the following concrete triggers:

1. **A real internal consumer emerges.** A future round identifies a
   concrete in-tree use case that would naturally batch same-class
   allocations — e.g. a bulk-deserialize path, a batch node-construction
   step in a future data-structure feature, or a `Vec::with_capacity`-style
   bulk-reservation helper built on top of `SeferAlloc` — and that use case
   is either implemented or seriously scoped for implementation. At that
   point, wiring it to `alloc_batch`/`dealloc_batch` and measuring the
   actual end-to-end (not per-op-in-isolation) effect becomes a real,
   non-circular measurement task.
2. **A specific downstream project adopts or requests batch-shaped
   allocation.** A user of this crate (via an issue, PR, or a concrete
   reported workload) demonstrates they would call `alloc_batch` with a
   realistic batch-size distribution for their own application. That
   distribution then becomes the sweep this task's Option A would have
   invented from nothing — but grounded in a real requirement instead of a
   guess.
3. **`dealloc_batch` gets batch-optimized.** R10-7 §2.4 names an open gap:
   `dealloc_batch` currently loops the per-block `dealloc` path (amortizing
   only TLS lookup + classification, not the magazine push/overflow
   bookkeeping `flush_class` batches on the `AllocCore`-direct ceiling arm).
   If a future round closes that gap (accumulate + one batched
   `flush_class`-style push, without re-implementing the M2 double-free
   oracles unsafely), the free-side ceiling moves closer to R8-7/R10-7's
   `(d)` arm — worth re-measuring end-to-end at that point, not before.

Absent one of these three, the correct action for a future round that
re-encounters this question is to **cite this record** (and the R8-7/R9-9/
R10-7 trail it summarizes), not to build a fifth generation of the same
microbench. `batch-api` stays exactly where R12-12 already placed it:
explicitly experimental, opt-in, no semver guarantees, real measured
mechanism win, zero known consumers.

---

## 5. Files this record is grounded in

- `docs/reviews/2026-07-26-r22-readonly-review.md` §4.6 (the flagging
  review), read in full for this task.
- `docs/perf/R8_7_BATCH_CEILING_MEASUREMENT.md` — read in full.
- `docs/perf/R9_9_BATCH_BENCH_FOLLOWUP.md` — read in full.
- `docs/perf/R10_7_BATCH_WARM_ARM.md` — read in full.
- `Cargo.toml` — `batch-api`/`production`/`experimental` feature
  definitions (lines 188-214, 399), confirmed `batch-api` is not part of
  `production`.
- `src/global/sefer_alloc.rs` — `alloc_batch`/`dealloc_batch` definitions
  and their only call sites (lines 482-533).
- `src/registry/heap_core_alloc.rs`, `src/registry/heap_core_free.rs`,
  `src/registry/heap_core_tcache.rs` — confirmed `refill_class_bump`/
  `flush_class` (the DIFFERENT, genuinely-production-used primitives) have
  real production call sites, unlike `alloc_batch`/`dealloc_batch`.
- `git log --oneline` for the batch-API commit trail (`de4c4ae`, `5e467ec`,
  `9611a56`, `33581bd`, `ff4a1af`, `a7db75a`) — confirmed the chronology
  R8-7 → R9-9 → R10-7 → R11-1/R11-4 (implementation hardening) → R12-12
  (experimental marking).
- `docs/perf/OPEN_ITEMS.md` item 10 (`[D]` tier) — the R9-9 warm-batch-arm
  ask this record confirms was actually resolved by R10-7 (see this task's
  `OPEN_ITEMS.md` edit).
