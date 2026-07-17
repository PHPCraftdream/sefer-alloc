# P10 — Deferred / skipped extraction candidates: file-or-drop verdict

**Task:** CRATE-P10 (#180). Read-only research + decision. Companion to
`DEFERRED_AND_SKIPPED.md` and `SUMMARY.md`. Re-evaluates every candidate the
first extraction pass consciously did **not** file, now that P1–P9 (7 new crates
+ vmem 0.2 + malloc-bench publish-prep) have shipped and are committed.

**Grounding note.** Verdicts below were checked against the *actual* shipped
crates, not just the reports:
- `crates/ring-mpsc/src/lib.rs` already ships `MpscRing::over_raw`
  (`unsafe fn`, raw in-place tier), the `U32Entry` / `UsizeU32Entry` two-word
  pair-publish entries, `drain() -> usize` (stop-position contract), the
  `tail_relaxed()` guard idiom, AND `DirtyRouter<const WORDS>`. The MPSC-ring
  protocol is therefore **fully extracted and generalized already** — this is
  decisive for the two deferred concurrency candidates below.
- `crates/vmem` (0.2) already ships `try_reserve_aligned*` (Result API),
  `mod mock` with `fail_next_commit`, and `leak_zeroed_pages`. The "second half
  of the vmem unsafe story" framing for `carved-mem` is now testable against a
  real 0.2 baseline.
- `src/alloc_core/node.rs` atomic views are `pub(crate) fn atomic_uN_at(..) ->
  &'static AtomicUN` — the `'static` is documented in-source as a SEAM
  convenience ("NOT mapped-forever"), load-bearing for the safe upper world.

---

## 1. Ranked file-or-drop table

| Candidate | Group | Verdict | Effort | Reason / numbers |
|-----------|-------|---------|--------|------------------|
| **iai-judge** — in-place `BENCH_OPS` fix | (1) deferred / hygiene | **FILE (hygiene only)** | XS (½ day) | The extraction stays dropped; but `scripts/iai.mjs:121` still hand-mirrors a `BENCH_OPS` map + `bench-table.mjs:40` `OPS=1024`. Cheap, safe, kills a live drift class. Do the manifest fix, not the crate. |
| **carved-mem** — Node raw-memory membrane | (1) deferred | **DROP (for now)** | Med code / **High contract** | ~600 lines mechanically trivial, but the `'static` atomic-view lifetime is load-bearing for `#![forbid(unsafe_code)]`. A general crate must return lifetime-parameterized handles → every `// SAFETY:` becomes a generic caller obligation → ripples back into sefer. vmem 0.2 does **not** reduce this cost (it's a *sibling*, not a prerequisite). Value MED, audience narrow. Bar not cleared. Revisit only on external demand. |
| **intrusive-once-stack** — deferred_large idempotent Treiber | (1) deferred | **DROP** | Med / Med-High | ring-mpsc (P4) already banked the reusable MPSC-ring value. What's left unique here (idempotent double-push guard via `compare_exchange(ABANDONED_TAIL→next)`) is 1 loom model over a protocol whose production form stores **raw addresses in AtomicU64** + link word doubles as a lifecycle field. Extraction forces AtomicPtr + node-trait rework that *loses the address-reuse trick* — i.e. you'd ship a different, less interesting artifact. `cordyceps`/`heapless` already cover intrusive stacks. Drop; keep the double-push guard as a documented technique. |
| **criterion-arms** — 3-arm normalized bench table | (1) deferred | **DROP (document only)** | Med / Med-High | Heavy overlap with `critcmp` + `criterion-table`. Novel parts (per-op normalization, arm-ratio, completeness gate) are a thin layer. Document the *discipline*; the only in-place win (bench-emitted MANIFEST killing `bench-table.mjs:40` `OPS=1024`/`SIZES`/`ARMS`) folds into hygiene. |
| gen-slot | (2) skip | **CONFIRM SKIP** | — | Repo retired/`#[deprecated]` this tier; depends on non-miri-clean crossbeam-epoch 0.9.18. Resurrecting retired code. |
| tcache-magazine | (2) skip | **CONFIRM SKIP** | — | Deliberately trivial (array + len); the interesting oracles live in `HeapCore`, not here. |
| bucket (c) / row 20 | (2) skip | **CONFIRM SKIP** | — | Bitmaps / SegmentTable / directory / PageMap / BinTable / Segment newtype / SegmentHeader / large-cache (still mid-refactor: `alloc_core_{large,small}*` untracked) / xthread SMs / sanitizer scripts / unsafe-seam pattern. Too thin, internal ABI, or ~80% convention. |

**Hygiene sub-tasks proposed** (details + effort/risk in §4): H1 single-source
sanitizer matrix; H2 bench-emitted MANIFEST; H3 dead-`dbg_*`-hook detection;
H4 fold `rss_probe.rs` onto proc-memstat (the last residual). **Recommended
priority: H1 > H2 > H3 > H4.**

**Net:** of the 4 deferred crate candidates, **0 file as crates**; 2 fold into
hygiene (iai `BENCH_OPS`, criterion-arms MANIFEST), 2 drop outright (carved-mem,
intrusive-once-stack). All 3 skip groups confirmed. The phase's crate surface is
effectively complete.

---

## 2. Deferred candidates — per-candidate reasoning

### 2.1 carved-mem (SUMMARY row 12, 01 §3) — **DROP for now**

The Node membrane is `atomic_uN_at` views at offsets, the intrusive freelist
word, and `atomic_ptr_ref` (exposed-provenance shared AtomicPtr, the #142
Tree-Borrows fix). Verified in source: the views return `&'static AtomicUN`
(`node.rs:377,409,494`), and the source comment is explicit that the `'static`
is a **seam convenience** so safe modules (`registry::heap_core`) can hold the
reference — "NOT mapped-forever". That is precisely the blocker the report
called out, confirmed at the code level.

**Does vmem 0.2 change the calculus? No.** The "second half of the vmem unsafe
story" is a *marketing adjacency* ("vmem gives you the span, carved-mem gives you
sound access into it"), not a technical dependency. vmem 0.2 shipping `try_*`,
`mock`, and `leak_zeroed_pages` does nothing to resolve the `'static`-view
lifetime question — that is carved-mem's own internal design problem. If
anything, vmem 0.2 *raised* the bar: it demonstrated that a clean extraction
returns a `Reservation` handle with an honest lifetime, and carved-mem would have
to do the analogous thing (return `&'a AtomicUN` or a raw handle) — which is
exactly the ripple back into sefer's `#![forbid(unsafe_code)]` world that makes
this expensive.

**Cost/value:** ~600 lines, mechanically a copy; but the whole cost is
rewriting every `// SAFETY:` proof (single-writer segment discipline, "block
bytes untouched" remote-free rule, per-path segment-liveness for the `'static`
views) as **generic caller obligations**. Value MED, audience = allocator/arena/
shm authors (narrow). Testability gain MED (direct miri on `atomic_ptr_ref`).
The value/effort ratio does not clear the bar that `size-classes` / `ring-mpsc`
cleared. **Drop; revisit only on concrete external demand**, and only after a
design spike answers the lifetime question in the abstract.

### 2.2 intrusive-once-stack (SUMMARY row 13, 02 candidate 5) — **DROP**

`deferred_large/{push,drain,tail}.rs`: a Treiber stack with two sentinels
(`ABANDONED_TAIL`, `DEFERRED_LARGE_TAIL`) and the A1 idempotent push — push
first claims the link word via `compare_exchange(ABANDONED_TAIL→next)` before
contesting head, so a racing double-push is a detected no-op (double-free →
no-op instead of UAF/double-unmap). Novelty = the loom-proven double-insert
guard (`tests/loom_deferred_large.rs`).

**Does ring-mpsc (P4) change this? Yes — decisively toward DROP.** The generic
MPSC-queue value that would have justified a concurrency crate here is *already
shipped* in ring-mpsc (owned + `over_raw` tiers, drain-stop contract, loom
proofs). What remains genuinely unique to this candidate is only the
idempotent-push guard — and that is bolted to two allocator-specific facts
verified in source: production stores **raw addresses in AtomicU64** (exposed
provenance) and the link word **doubles as a lifecycle field**. A community crate
must switch to `AtomicPtr` + a node trait, which *loses the address-reuse
trickery* — you ship a materially different, less interesting artifact than the
one that earned its loom proof. Intrusive stacks are already covered
(`cordyceps`, `heapless`). **Drop the crate; preserve the double-push-guard
idea in the technique-docs chapter** (§5) as a lock-free idempotency pattern.

### 2.3 iai-judge (SUMMARY row 11, 04 §C4) — **DROP crate; FILE the in-place fix (H2)**

iai-callgrind WSL bridge (`scripts/iai.mjs`, 452 lines) + marginal-Ir/op
decomposition. Narrow niche (perf-gated crates with Windows devs); the WSL
sccache/target-dir/runner-pin traps and the marginal-Ir/op technique are real,
but they are better told as **documentation** (see §5) than shipped as a tool
with substantial overlap on the "run iai from WSL" mechanics.

The one concrete, worth-doing-anyway item is confirmed live in source:
`scripts/iai.mjs:121` still declares a hand-mirrored `const BENCH_OPS = {...}`
map consumed at line 351. That is the same drift-by-mirror class as
`bench-table.mjs`. **Fold into hygiene H2** (derive `BENCH_OPS` from bench names
/ a bench-emitted MANIFEST). Crate stays dropped.

### 2.4 criterion-arms (SUMMARY row 14, 04 §C3) — **DROP crate; document the discipline**

3-arm normalized bench table (`scripts/bench-table.mjs` + `benches/
global_alloc.rs`). `critcmp` and `criterion-table` already own baseline
comparison. The novel bits (per-group op-count normalization to one honest unit,
cross-arm ratio column, expected-ids completeness gate) are a thin layer worth
extracting only on demand. The incident that birthed it (µs/batch vs ns/op unit
mixup masquerading as a 2× regression) is best captured as **written discipline**
(fixed unit, fixed shape, completeness gate). The in-place win is the same
MANIFEST fix: `bench-table.mjs:40` holds `const OPS = 1024` (+ `SIZES`, `ARMS`)
hand-mirrored from the bench. **Fold into H2.**

---

## 3. Skip confirmations (one line each — skip still holds)

- **gen-slot** (row 16) — CONFIRM SKIP. Repo `#[deprecated]`/retired this tier;
  crossbeam-epoch 0.9.18 not miri-clean upstream. Resurrecting retired code.
- **tcache-magazine** (row 19) — CONFIRM SKIP. Deliberately trivial (array +
  len); double-free oracles + flush interplay live in `HeapCore`, not here.
- **SegmentBitmap / AllocBitmap / MagazineBitmap** (row 20) — CONFIRM SKIP. ~40
  lines of bit arithmetic; value is the domain semantics, which don't generalize.
  `bitvec`/`fixedbitset` own the niche.
- **SegmentTable backward-shift hash** (row 20) — CONFIRM SKIP. Inseparable from
  self-hosting in the primordial segment; `heapless::IndexMap` covers generic.
- **SegmentDirectory / PageMap / BinTable** (row 20) — CONFIRM SKIP. Thin views
  whose geometry *is* the segment layout — exporting internal ABI, not a primitive.
- **Segment newtype / SegmentHeader** (row 20) — CONFIRM SKIP. Pure internal ABI.
- **large-segment cache** (row 20) — CONFIRM SKIP + still blocked: the
  `alloc_core_{large,small}*.rs` split remains untracked in the working tree
  (confirmed: 4 untracked `alloc_core_*` files present). Policy, not a data
  structure. Not a stable extraction target.
- **xthread state machines** (row 20) — CONFIRM SKIP. Spec model of the
  allocator's ownership discipline; transfers as *methodology*, not a crate.
- **sanitizer runner scripts** (row 20) — CONFIRM SKIP as a crate; the value is
  80% convention. (But the matrix-drift *problem* they contain is hygiene H1.)
- **unsafe-seam pattern** (row 20) — CONFIRM SKIP as a crate; a proc-macro buys
  almost nothing over `#[doc(hidden)] pub fn dbg_*` + convention. Document it (§5).

---

## 4. In-place hygiene — scoped sub-task proposals

*Proposals only. Do not file without the user's decision. Effort/risk each.*

### H1 — Single-source the sanitizer feature matrix — **PRIORITY 1**
**Scope.** Confirmed live: **no `scripts/matrix.{json,toml}` exists**;
`scripts/loom.mjs` and `scripts/miri.mjs` still carry per-test feature-set maps
with "MUST mirror ci.yml" comments (grep hit both). This is the ×5 drift class
that went stale once (deleted `alloc` feature, task #204). Create one
machine-readable matrix consumed by `loom.mjs`/`miri.mjs`/`tsan.mjs`, plus a
consistency test (or generator) validating it against `.github/workflows/ci.yml`.
**Effort:** Med (½–1 day — 3 runners + a validation test). **Risk:** Low-Med —
touches CI-adjacent scripts; a wrong mapping silently drops sanitizer coverage,
so the validation test is the load-bearing part (must fail if matrix ≠ ci.yml).
Highest value of the batch — it's the drift class with a proven incident.

### H2 — Bench-emitted MANIFEST to kill hand-mirrored constants — **PRIORITY 2**
**Scope.** Confirmed live: `scripts/bench-table.mjs:40` `const OPS = 1024`
(+ `SIZES`, `ARMS`) and `scripts/iai.mjs:121` `const BENCH_OPS = {...}` are
hand-mirrored from the bench sources. Have `benches/global_alloc.rs` and
`benches/perf_gate_iai.rs` emit a `MANIFEST key=value` line (same spirit as the
existing `RESULT` protocol); scripts derive instead of mirror. Absorbs both the
iai-judge and criterion-arms "in-place win" noted above. **Effort:** Med (1 day
— touch 2 benches + 2 scripts). **Risk:** Low — additive emit + parser; worst
case is a parse miss that surfaces immediately as a missing table row.

### H3 — Dead-`dbg_*`-hook detection — **PRIORITY 3**
**Scope.** A consistency test asserting every `#[doc(hidden)] pub fn dbg_*`
accessor is referenced by ≥1 file under `tests/`. Note: an *adjacent* test
exists — `tests/regression_r2_3_dbg_accessors_membership_guard.rs` — but it
guards membership, **not** the "referenced by ≥1 test" property (grep confirms it
doesn't walk `tests/`). So this is genuinely new coverage: it catches silent API
surface (hooks nothing exercises). **Effort:** Low (½ day — one walk-and-grep
test). **Risk:** Low, but expect an initial batch of flagged unused hooks to
triage (delete vs. add a test) — that triage is the real cost, not the test.

### H4 — Fold `examples/rss_probe.rs` onto proc-memstat — **PRIORITY 4**
**Scope.** Confirmed: #178 proc-memstat already absorbed most of it —
`first_alloc_process.rs` and `dealloc_only_unbound_thread.rs` now route through
`proc-probe`'s re-export of `proc-memstat`'s `snapshot()` (no hand-rolled FFI).
**Residual is exactly one file:** `examples/rss_probe.rs:136` still hand-rolls
`GetProcessMemoryInfo` extern + the Linux `/proc/self/statm` reader. Point it at
`proc-memstat::snapshot()` too and the last duplicated reader dies. **Effort:**
Low (½ day — one example). **Risk:** Low — but confirm proc-memstat exposes the
exact fields rss_probe prints (peak/commit) before swapping; otherwise a small
proc-memstat surface add is needed first.

---

## 5. Technique-docs chapter — proposed `docs/` outline

*Not crates. A `docs/` chapter (link from README), capturing the material the
survey repeatedly flagged as "technique, not code".* Suggested path:
`docs/techniques/` (one file per section, or one chapter with these sections):

1. **The two-tier confined-unsafe seam + self-verifying inventory.**
   `#![forbid(unsafe_code)]` upper world; tier-1 module-level
   `#![allow(unsafe_code)]` seams; tier-2 item-level allows each with `# Safety`
   + per-site `// SAFETY:`. The **comment-proof anchored grep**
   (`^\s*#!?\[allow\(unsafe_code\)\]`) as a self-verifying inventory instead of a
   hardcoded count; an uncovered `unsafe` token is a compile error in every
   feature config. Include the `#[doc(hidden)] pub fn dbg_*` forwarder pattern
   (test-only seams for `tests/`-only projects) and the dead-hook detection idea
   (H3) as its companion guard.

2. **Hardening-harness conventions.** Zero-runs-is-red (hard-fail on 0 tests
   selected / 0 tests ran — the stale-feature-name → silent-green class, tasks
   #29/#18/#204); verdict-by-output-scan (fail on TSan markers even when exit
   code is 0); the WSL traps (Windows `sccache.exe` leaking via `RUSTC_WRAPPER`
   into WSL; dedicated Linux `CARGO_TARGET_DIR`; runner-version pinning). Note the
   single-source-matrix rule (H1) as the structural fix for the drift these
   scripts otherwise accumulate.

3. **State-machine spec-model methodology.** From
   `docs/CROSS_THREAD_STATE_MACHINES.md`: write the ownership discipline as an
   explicit SM (SM-BLOCK/SM-CHANNEL, invariants like I-BLOCK-1 "never LIVE ∧
   free-listed"), model-check with loom, ship `#[should_panic]` counterfactuals
   proving the harness is non-vacuous. This is the reusable form of what the
   xthread SMs (§3 skip) can't ship as a crate. **Add here:** the lock-free
   *idempotent-push guard* rescued from intrusive-once-stack (2.2) — claim the
   link word before contesting head so a double-push degrades to a no-op — as a
   worked idempotency pattern.

4. **The platform-shim cfg convention.** "How to add a platform" — the vmem
   pattern (per-OS module behind `cfg`, one honest `page_size()`, the mock
   backend). Belongs as a page in the vmem crate docs and mirrored here.

5. **(optional) The marginal-per-op decomposition.** Rescued from iai-judge
   (2.3): subtract a bootstrap-constant proxy, divide by op count, so per-op
   regressions compare across benches whose raw sums are 58–90% shared bootstrap.
   Transferable to any crate with a large once-per-process constant.
