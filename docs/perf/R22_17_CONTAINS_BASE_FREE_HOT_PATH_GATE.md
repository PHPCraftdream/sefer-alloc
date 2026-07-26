# R22-17 — `contains_base`'s share of a real free's `Ir`: measured MATERIAL (18.6%)

**Task #368 (R22-17), Round 22.** Follows up @ox's independent Round 19-21
review (`docs/reviews/2026-07-26-oh-review-r19-r21.md` §2.3.2), which flagged
`HeapCore::dealloc_routing`'s `contains_base` own-thread ownership probe
(`src/registry/heap_core_xthread.rs`) as a candidate hot-path cost, absent
from `OPEN_ITEMS.md` and from any perf plan — and is interpretable relative to
R22-15's freshly-landed mimalloc `Ir` arm (task #366, commit `ff48029`),
which measured SeferAlloc retiring 1.3x-2.4x more instructions per op than
mimalloc on matched workloads. This task's job: find out how much of THAT
gap, if any, is attributable to this one specific mechanism.

**Date:** 2026-07-26. **Base revision measured:** `main` @ `8c1f248` (working
tree otherwise clean at measurement time; the usual untracked
`docs/checkpoints/`/`docs/reviews/` files from a concurrent review session
present, not touched by this task). **Platform measured:** WSL2 (Ubuntu,
kernel `6.18.33.2-microsoft-standard-WSL2`) under Windows 10 Pro x86-64,
`valgrind 3.22.0`, `iai-callgrind-runner 0.14.2`, WSL rustc `1.98.0-nightly
(bd08c9e71 2026-06-25)` — same toolchain/host as every other `npm run iai`
measurement in this doc tree, including R22-15's.

---

## 0. Headline: `contains_base` is 18.6% of a real free's instruction count — MATERIAL, not negligible

| quantity | value |
|---|---:|
| Real free loop only (64 frees, shared pre-alloc prefix subtracted) | 5,920 Ir → 92.5 Ir/free |
| `contains_base` probe loop only (64 probe calls, same prefix subtracted) | 1,101 Ir → 17.2 Ir/call |
| **`contains_base`'s share of a real free's `Ir`** | **1,101 / 5,920 = 18.6%** |

This clears the task's own "double-digit percentage or more" MATERIAL bar.
Per the task's explicit instruction, this is NOT a NULL result and the report
does not stop at §1 — §2 below sketches (design only, not implemented) what a
header-first alternative would look like, and states explicitly why it is
not a free lunch for THIS crate's threat model.

---

## 1. Read the mechanism first — what `contains_base` actually does

### 1.1 `dealloc_routing` (`src/registry/heap_core_xthread.rs:750-788`)

```text
pub(super) fn dealloc_routing(&mut self, ptr: *mut u8, layout: Layout) {
    let base = os::segment_base_of_ptr(ptr);
    if self.core.contains_base(base) {
        // own-thread: dealloc_own_thread_with_base(ptr, layout, base)
        return;
    }
    // NOT ours: dealloc_foreign_slow(ptr, base, layout)  (cold, #[inline(never)])
}
```

`contains_base(base)` is called on EVERY free before anything else — before
any header byte of `ptr`'s segment is read. The doc comment at lines 753-770
is explicit about why: `contains_base` reads only the CALLING heap's own
`SegmentTable` (self-hosted registry data, never `base`'s memory), so it is
safe to call even if `base` is unmapped (a released/decommitted segment) —
this is exactly the guarantee a header-first replacement would have to
reproduce (see §2.2).

### 1.2 `SegmentTable::contains_base` (`src/alloc_core/segment_table.rs:455-468`)

```text
pub(crate) fn contains_base(&mut self, base: *mut u8) -> bool {
    let idx = Self::cache_index(base);              // (base >> SEGMENT_SHIFT) & 3
    if self.own_cache[idx] == base && !base.is_null() {
        return true;                                  // fast path: 1 load + 1 compare
    }
    if self.hash_contains(base) {                       // miss -> full probe
        self.own_cache[idx] = base;
        true
    } else {
        false
    }
}
```

This is **not a single mechanism** — it is a two-tier check, and which tier
dominates depends entirely on the workload's *segment diversity*, not on
"is it a hash probe":

- **Tier 1 — `OWN_CACHE_SIZE = 4`-entry direct-mapped cache**
  (`segment_table.rs:90-98, 483-489`): `cache_index` shifts out the
  `SEGMENT_SHIFT = 22`-bit segment alignment and masks to 2 bits. On a HIT
  (the common case for any workload reusing a small, stable set of ≤4
  concurrently-relevant segments — e.g. this crate's own churn-shaped
  benches, which live entirely in the primordial segment) this is one load
  from a 4-element array plus one pointer compare. It is NOT a hash probe at
  all in the hit case.
- **Tier 2 — `hash_contains`, open-addressing linear probe**
  (`segment_table.rs:839-861`): `HASH_CAPACITY = 8192` slots, load factor
  guaranteed ≤ 50% (`HASH_CAPACITY = 2 * MAX_SEGMENTS`), **backward-shift
  deletion** (R4-8/N3, lines 713-761) so the table never accumulates
  tombstones — an empty slot always unambiguously terminates a probe (no
  "keep scanning past a deleted marker" cost). Under a ≤50% load factor a
  linear probe's expected chain length is short (geometric, mean ≈ 2 for a
  50%-full table); each step is one `hash_slot_read` (pointer-arithmetic
  address computation + a plain load) plus a null check and an equality
  check.

For the workload this gate measures (`CHURN_OPS = 64` alloc/free pairs of a
16 B block, all served out of the primordial segment — the exact shape
`small_churn_16b` already exercises under `production`), **every
`contains_base` call after the very first is a Tier-1 cache HIT**: one base
address, reused 64 times, sitting in `own_cache` from the first probe onward.
So the 17.2 Ir/call figure below is the cache-HIT cost of the check, not a
worst-case hash-probe cost — and it is STILL an 18.6% share of the free's
total. A workload with more concurrently-hot segments than `OWN_CACHE_SIZE`
(4) would fall through to Tier 2 more often and the share would be larger,
not smaller, than what is reported here — this measurement is a
**conservative (lower-bound) estimate** of `contains_base`'s cost on a
worse-shaped workload.

---

## 2. Measurement method — reusing the existing deterministic iai gate

Per the task's instruction, no new profiling infrastructure was built. Three
new `#[library_benchmark]` arms were added to the EXISTING
`benches/perf_gate_iai.rs` (same file R22-15 added its mimalloc arms to,
following that same just-established pattern), plus one new `#[doc(hidden)]`
test-only measurement hook on `HeapCore`
(`src/registry/heap_core_diag.rs::dbg_contains_base`) — mirroring the
established `dbg_*` test-hook pattern already used throughout this file
(`dbg_segment_base_of_ptr`, `dbg_owner_id_for`, etc.), not a new mechanism.

**A real correction made mid-task, disclosed here rather than silently
fixed:** the first draft's doc comment claimed iai-callgrind only times "the
function body from the point callgrind attaches", implying a pre-allocation
setup pass inside the benched function would be excluded from measurement.
That is WRONG — iai-callgrind's `#[library_benchmark]` times the ENTIRE
annotated function call under Callgrind (there is no criterion-style
`iter()` closure that separates timed from untimed code). The comment was
corrected in the committed code, and the measurement design was adjusted to
compensate: a third arm (`dealloc_prealloc_only_16b`) measures the shared
pre-allocation prefix ALONE, so it can be subtracted from the other two arms'
raw Ir to isolate each one's own loop-only cost. This is why three arms exist
instead of two.

### 2.1 The three arms (`benches/perf_gate_iai.rs`)

All three share a BYTE-IDENTICAL pre-allocation pass: `bootstrap::ensure()` +
`HeapRegistry::claim()` + a loop of `CHURN_OPS = 64` `HeapCore::alloc(Layout
16B/align8)` calls (the same op count and layout `small_churn_16b` uses).

- **`dealloc_prealloc_only_16b`** — pre-allocation pass, then nothing (the
  64 pointers are deliberately leaked; each `#[library_benchmark]` runs in
  its own fresh process under Callgrind, so this has no effect on any other
  arm). Measures the shared prefix's Ir alone.
- **`dealloc_free_only_16b`** — pre-allocation pass, then frees all 64
  pointers through the REAL production path: `HeapCore::dealloc` →
  `dealloc_routing` → `contains_base` → `dealloc_own_thread_with_base`. This
  is the exact same call `small_churn_16b`'s dealloc half makes; nothing
  here is a bypass or alternate implementation.
- **`dealloc_contains_base_probe_only_16b`** — pre-allocation pass, then for
  each of the 64 pointers: `dbg_segment_base_of_ptr(ptr)` +
  `dbg_contains_base(base)`, discarding the result via `black_box`. This
  calls the SAME production `AllocCore::contains_base` →
  `SegmentTable::contains_base` `dealloc_routing` itself calls — just with
  none of the surrounding free bookkeeping (bitmap/magazine/stamp writes)
  around it. The 64 pointers are never freed (they leak for the duration of
  that one process) — again harmless under Callgrind's per-bench fresh
  process.

### 2.2 The new measurement hook (`src/registry/heap_core_diag.rs`)

```text
#[doc(hidden)]
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
pub fn dbg_contains_base(&mut self, base: *mut u8) -> bool {
    self.core.contains_base(base)
}
```

Thin delegation, gated exactly on the features that compile `AllocCore::
contains_base` (`alloc-global`) and that make it meaningful to isolate
(`alloc-xthread`, the feature that compiles in `dealloc_routing`). No
`production` feature-list change — confirmed by grep (§4).

**This is the production check itself, exposed read-only for isolated
timing — explicitly NOT an alternate/bypass implementation.** No production
call site was touched; `dealloc_routing` still calls `self.core.
contains_base(base)` exactly as before this task.

---

## 3. Results — real, deterministic `npm run iai` numbers (not estimated)

Two independent full-suite `npm run iai` runs (23 benches each, `--features
production`, the CI default), raw stdout committed in full:

- `docs/perf/_raw_r22_17_contains_base_free_hot_path.log`
- `docs/perf/_raw_r22_17_contains_base_free_hot_path_rerun1.log`

Both runs produced **byte-identical** `Ir`/L1/L2/RAM/EstCycles for every
bench, including the three new arms — the expected determinism property of
Callgrind emulation this whole gate exists to exploit (per
`benches/perf_gate_iai.rs`'s own module doc).

| bench | raw Ir | ops | loop-only Ir (raw − 7,003 prefix) | loop-only Ir/op |
|---|---:|---:|---:|---:|
| `small_churn_16b` (context: alloc+dealloc together) | 8,051 | 64 | — | 74.1 (alloc+free combined) |
| `dealloc_prealloc_only_16b` (shared prefix alone) | 7,003 | 64 | 0 (baseline) | — |
| `dealloc_free_only_16b` (real free loop) | 12,923 | 64 | **5,920** | **92.5** |
| `dealloc_contains_base_probe_only_16b` (probe loop alone) | 8,104 | 64 | **1,101** | **17.2** |
| `large_alloc_free_cycle` (bootstrap proxy, context only) | 3,308 | 1 | — | — |

**`contains_base`'s share of a real free's `Ir` = 1,101 / 5,920 = 0.1860 →
18.6%.**

Sanity cross-check against `small_churn_16b`: alloc+dealloc together cost
74.1 Ir/op (marginal, bootstrap-subtracted via `large_alloc_free_cycle` per
R22-15's convention); this gate's isolated free-only loop costs 92.5 Ir/op
BEFORE subtracting `small_churn_16b`'s own bootstrap constant — the two
numbers are not directly comparable (different arms use different bootstrap
baselines and one measures alloc+free while the other measures free alone
against pre-existing pointers), but both are the same order of magnitude,
which is the expected sanity signal (no obviously broken measurement, e.g.
free-only costing 10x more than alloc+free combined would have signaled a
methodology bug).

Companion machine-readable summary:
`docs/perf/R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE_summary.csv`.

---

## 4. Verdict: MATERIAL (not NULL) — worth a future A/B design task

`contains_base` accounts for **18.6%** of a real free's instruction count in
this gate's workload (single hot segment, Tier-1 cache-hit case — see §1.2's
note that this is a conservative/lower-bound estimate; a workload spanning
more than `OWN_CACHE_SIZE` (4) concurrently-hot segments would show a LARGER
share, since Tier 2's hash probe costs strictly more per call than a Tier-1
hit). This clears the task's own bar ("double-digit percentage or more") for
proceeding to a design sketch. Per the task's explicit scope, this section
sketches a header-first alternative for a FUTURE round — it is NOT
implemented here, and no real allocator behavior changes in this commit.

### 4.1 Sketch: header-based ownership check instead of a table probe

mimalloc's equivalent step (per @ox's review) is a pointer mask plus one
header-field read: derive the segment/page base from the pointer's bits,
then read a field already resident in that page's header to determine
ownership — no probe into a separate registry structure at all.

A structurally similar scheme for SeferAlloc: `os::segment_base_of_ptr(ptr)`
is already computed for free (line 751, unconditionally, before the
`contains_base` call). If the segment header at that base already carries an
`owner_thread_free`/owner stamp — and per the task's own citation, several
free paths already read exactly such a field (grep confirms
`SegmentMeta::owner_state_atomic` / `unpack_owner_id`, used by
`dbg_owner_id_for` and the cross-thread stamp-comparison paths in
`heap_core_xthread.rs`'s own `dealloc_foreign_routing` doc comment) — a
future design could read that stamp DIRECTLY instead of probing
`SegmentTable`'s hash/cache first, checking `owner_tf.is_null() || owner_tf
== our_head` the way the OLD (pre-`contains_base`, per task #135's own doc
comment at `heap_core_xthread.rs:765`) mechanism did.

### 4.2 The soundness caveat this idea must NOT skip (per the task's explicit instruction)

**A header-first scheme dereferences memory whose ownership/liveness has NOT
yet been established, which `contains_base`'s CURRENT ordering exists
specifically to avoid.** Quoting `dealloc_routing`'s own doc comment
(`heap_core_xthread.rs:753-758`, verbatim intent): `contains_base` is called
**FIRST, before touching any segment memory**, and is safe to call even if
`base` is unmapped, because it reads ONLY the calling heap's own
self-hosted `SegmentTable` data — never `base`'s memory. A header-first
check inverts this: it would read `base`'s own memory (the header field)
BEFORE any proof that `base` is a live, registered segment at all.

For a genuinely foreign or corrupt pointer — exactly the threat model the
existing foreign-pointer no-op branch and the hardened-misuse-guard
counters (`HARDENED_LARGE_NOOP_COUNT`, R22-12/task #363; the
`DEALLOC_FOREIGN_NOOP`-class defensive paths) exist to defend against — a
header-first read could dereference unmapped or unrelated memory. This
crate's own philosophy (verification-first, `#![forbid(unsafe_code)]` for
the upper world, hardened defensive no-ops rather than trusting caller
input) treats "a faster free that dereferences unvalidated pointers" as
categorically unacceptable, not a trade-off to weigh against the Ir savings
— per this task's own explicit instruction, that trade is NOT proposed here.

**How a future real implementation would need to preserve the guarantee:**
some OTHER cheap liveness proof would have to run before the header read —
candidates worth a future round's real feasibility study (not evaluated
here, sketch only):

1. **A cheaper pre-check than the full table probe, but still memory-safe
   without touching `base`.** E.g., a coarser, single-word "is this address
   range even OS-reserved by this process" bitmap indexed more cheaply than
   `SegmentTable`'s current hash (a bloom-filter-shaped structure trades
   false positives — falling back to the real `contains_base` on a
   positive — for a cheaper common-case negative-or-positive check). This
   still reads only allocator-owned metadata, never `base`, so the ordering
   guarantee is preserved; whether it is actually cheaper than the existing
   Tier-1 4-entry cache (already a single load + compare on the common hit
   path per §1.2) is exactly the open question a future round's A/B would
   need to answer — for THIS crate's workloads, Tier-1 may already be close
   to this idea's own floor.
2. **Scoping the optimization to a configuration where liveness is
   guaranteed some other way** — e.g., a build/feature combination where
   segments are never released back to the OS mid-process (no
   `alloc-decommit`), so "unmapped" cannot happen and a header read is safe
   PROVIDED the address is still confirmed to be one of this process's OWN
   VirtualAlloc/mmap reservations by some cheaper structural means than the
   full table (still an open design question, not resolved here).
3. **No way was found, in this task's scope, to make a bare header-first
   read safe against a genuinely foreign or use-after-decommit pointer
   WITHOUT some prior liveness check that itself costs something** — i.e.
   this task did not find a free lunch. Any future implementation task
   inherits this as its first open question, not an assumed-solved
   prerequisite.

**This report does not recommend implementing any of the above.** It
recommends only that a future round treat this as an open design question
(tracked via `docs/perf/OPEN_ITEMS.md`, see §5) with the soundness ordering
constraint stated explicitly up front, rather than rediscovering it after a
prototype is already written.

---

## 5. Open-items tracking

Per `CLAUDE.md`'s round-start rule, this MATERIAL (not NULL) finding is a new
perf-relevant open item: **"contains_base's share of free is 18.6%
(measured, R22-17/task #368) — design-and-measure a header-first alternative
respecting the pre-dereference liveness-proof ordering constraint (§4.2)"**
should be added to `docs/perf/OPEN_ITEMS.md` in the same commit as this
report (left to the reviewing/committing session, per this task's
instruction not to commit).

---

## 6. Verification performed

- **Read `dealloc_routing` in full**
  (`src/registry/heap_core_xthread.rs:748-788`, plus its doc comment through
  line 815 for the cross-thread tail context) — confirmed `contains_base` is
  called unconditionally, first, before any other work, exactly as the task
  described.
- **Read `SegmentTable::contains_base`/`contains_base_ro` in full**
  (`src/alloc_core/segment_table.rs:429-489`), PLUS `hash_contains`
  (`:839-861`), `hash_insert`/`hash_remove` (`:688-784`) for the actual probe
  mechanics — confirmed it is a two-tier check (4-entry direct-mapped
  own-segment cache, THEN an open-addressing linear probe over an
  8,192-slot table at ≤50% load factor with backward-shift deletion), not
  merely "a hash lookup" — see §1.2 for the mechanics in my own words.
- **Real measured numbers from my own `npm run iai` runs** (not estimated):
  two independent full-suite runs, byte-identical Ir across both — raw logs
  committed (`git add -f` needed, listed below).
- **Verdict: MATERIAL, 18.6%** — reasoning in §0/§3/§4.
- **Design sketch + soundness caveat** — §4.1/§4.2, addressing the exact
  pre-dereference liveness-proof ordering constraint the task called out,
  including an honest statement that no free-lunch solution was found in
  this task's scope.
- **`production`'s feature composition is unchanged**: confirmed via
  `grep -n "^production = " Cargo.toml` → still `["alloc-global",
  "alloc-xthread", "alloc-decommit", "fastbin", "alloc-segment-directory",
  "primordial-lazy-commit", "class-aware-dirty"]`, byte-identical to the
  pre-task value. The new bench arms are additive rows in
  `benches/perf_gate_iai.rs`'s `library_benchmark_group!` list, gated
  `#[cfg(feature = "alloc-xthread")]`; the new `dbg_contains_base` hook in
  `src/registry/heap_core_diag.rs` is gated `#[cfg(all(feature =
  "alloc-global", feature = "alloc-xthread"))]` and marked `#[doc(hidden)]`
  — not part of any stable public surface, mirrors the established
  `dbg_segment_base_of_ptr`/`dbg_owner_id_for` test-hook pattern in the same
  file.
- **Compile/lint checks run**: `cargo fmt --check` (clean),
  `cargo clippy --bench perf_gate_iai --features production --all-targets
  -- -D warnings` (clean, exit 0), `cargo test --lib --features production`
  smoke-compiled (0 failures; the new `dbg_*` hook has no dedicated test yet
  — it is measurement-only tooling, consistent with the other `dbg_*` hooks
  in this file that exist purely to serve one test/bench caller).

## Raw logs and files needing `git add -f`

`.gitignore` excludes `docs/perf/_raw_*.log` by default (R13-10/task #280);
these two are the evidentiary basis for this report's verdict and need
`git add -f`:

- `docs/perf/_raw_r22_17_contains_base_free_hot_path.log` — full raw
  `npm run iai` stdout, run 1 (production, 23 benches).
- `docs/perf/_raw_r22_17_contains_base_free_hot_path_rerun1.log` — full raw
  `npm run iai` stdout, run 2 (independent process re-run, confirms
  byte-identical Ir).

Not gitignored, tracked normally (no `-f` needed):

- `docs/perf/R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE.md` — this report.
- `docs/perf/R22_17_CONTAINS_BASE_FREE_HOT_PATH_GATE_summary.csv` —
  machine-readable companion (R14-10/task #295 convention).
- `benches/perf_gate_iai.rs` — the three new bench arms.
- `src/registry/heap_core_diag.rs` — the new `dbg_contains_base` hook.
