# Read-only review — acceleration / code / project (independent, post-R28)

Date: 2026-07-29
Tree state: `b7ff9fe` (`main`), clean except the untracked prior review doc.
Review mode: **read-only filesystem inspection**. No build, test, bench,
formatter, linter or project executable was run. The only filesystem change
this review makes is this file.

Scope: an independent second opinion on three questions — what can still be
accelerated, what needs improvement in the code, what needs improvement in the
project — deliberately aimed at subsystems and risk classes the very recent
`docs/reviews/2026-07-29-r28-readonly-review.md` did **not** cover.

---

## 0. Executive verdict

The prior review's answer to **question 1 (acceleration)** is largely correct
and I do not repeat it. But it has one important blind spot: it inherited the
project's own four-round focus on the **free side** (magazine overflow,
`flush_class`, `STAGE_CAP`, `FLUSH_N`) and on **segment-lifecycle churn** (pool
cap, reservation-only tier, `mremap`). It never looked at the **alloc-hit
path**, which under plain `production` carries an unconditional
segment-metadata read-modify-write per allocation whose cost **has never been
measured by any report in this project's history** — the project's own record
explicitly says so. That is the single largest genuinely-unexploited
measurement gap I found (§1.1).

On **question 2 (code)** the prior review is thin — it lists seven mostly
stylistic items. I found two concrete soundness holes of exactly the **R25-1
class the project wrote a CLAUDE.md rule about**, both reachable from 100%-safe
downstream code in a plain `--features production` build, both surviving the
R24-6 audit because that audit searched only for hooks that were *already*
`unsafe fn` (§2.1, §2.2). One of them is on a **fully public re-exported type**
(`sefer_alloc::AllocCore`), not just a `#[doc(hidden)]` module.

On **question 3 (project)** the prior review's items are reasonable but
generic. I found a structural hole in the index machinery itself: **eight
explicitly-labelled "honest-reject" sections with revisit triggers live in
`docs/perf/IAI_BASELINE.md` — a file that is squarely in `OPEN_ITEMS.md`'s own
declared scope — and not one of them has ever been indexed** (§3.1). Plus at
least three implemented, CI-tested, gate-reported features whose promotion
decision was explicitly deferred and then tracked nowhere (§3.2).

Net: the prior review is adequate on question 1's *strategic* answer and on
question 3's *process-cost* observations; it is **not** adequate on question 2,
and it missed a large, concrete, in-scope indexing gap on question 3.

---

## What I checked vs. the prior review

| Area | Prior review (`2026-07-29-r28-readonly-review.md`) | This review |
|---|---|---|
| Magazine-**overflow** free path (`flush_class`, `STAGE_CAP`, `FLUSH_N`) | covered in depth; concluded exhausted | agreed, not re-derived |
| Small-segment pool cap `(4,16 MiB)` vs `(8,32 MiB)` | covered in depth | agreed, not re-derived |
| Reservation-only overflow tier (R27-11) | covered; correctly gated on Stage-1 | agreed, not re-derived |
| Linux `mremap` for medium→Large promotion | covered | agreed, not re-derived |
| Batch API consumer adoption / `medium-classes` profile | covered | agreed, not re-derived |
| R28-2 anomalous test failure | covered (P0) | agreed, not re-derived |
| Artifact volume / provenance | covered (P2) | agreed; one addition (§3.4) |
| **Alloc-HIT path memory traffic** | not covered | **§1.1 — new** |
| **Large-cache 256 MiB/heap idle-retention floor** | not covered | **§1.2 — new** |
| **Safe `pub fn` hooks that install/act on raw pointers** | not covered | **§2.1, §2.2 — new** |
| **`IAI_BASELINE.md` honest-rejects off-index** | not covered | **§3.1 — new** |
| **Unpromoted feature decisions tracked nowhere** | not covered | **§3.2 — new** |
| NUMA paths / pooled-segment node affinity | not covered | **§2.4, §3.3 — new** |
| `tcache.rs` doc-vs-code drift | not covered | **§2.3 — new** |
| TODO/FIXME sweep, inline-tests, doctests, scratch dirs, cited-evidence commit hygiene | not covered | **§4 — checked, clean** |

---

## 1. Что ещё можно сильно ускорить

### 1.1 [P1 — the leading genuinely-new candidate] The alloc-HIT path pays an un-measured segment-metadata RMW on every allocation

**The code.** `src/registry/heap_core_alloc.rs:232-236`, inside the magazine-hit
fast path (compiled under `alloc-global + fastbin`, i.e. plain `production`):

```rust
{
    let base = os::segment_base_of_ptr(issued);
    let off = (issued as usize - base as usize) as u32;
    SegmentMeta::new(base).magazine_bitmap().clear_magazine(off);
}
return issued;
```

The code's own comment (`:225-231`) is unusually frank about it:

> *"THE HOT PATH: unlike the `hardened`-only gen-table bump below, this runs on
> EVERY magazine hit under `production`, so it forces a `segment_base_of_ptr` +
> bitmap read-modify-write that this path previously did not pay AT ALL."*

This is a load **and** a store into the segment's metadata pages — a cache line
in an entirely different region from the magazine array the pop itself touched
(the bitmap lives at `Layout::magazine_bitmap_off()`, 32 KiB/segment, carved
right after `AllocBitmap`; `src/alloc_core/magazine_bitmap.rs`). So every
`malloc` hit touches **at least two distinct cache-line regions**, not one.

**Why this is unexploited, not merely known.** The one prior attempt is
`docs/perf/IAI_BASELINE.md:1206-1262` ("R3 honest-reject, 2026-07-13 —
batching `MagazineBitmap`'s per-hit clear off the issue path"). Its verdict is
`NO-GO`, and it ends with:

> *"**No iai baseline was taken; there is nothing to measure.**"*

That is the crux. R3 rejected **one specific remedy** (defer the clear to
refill/flush time), on sound correctness grounds — the bit's exactness at the
issue instant is load-bearing for two consumers: the own-thread free oracle
(`is_in_magazine` in `HeapCore::dealloc_own_thread_with_base`) and, critically,
`AllocCore::reclaim_offset_checked` (`src/alloc_core/alloc_core_small.rs:179`),
which by design "has no magazine concept" and uses the bitmap as its *only*
window into magazine state. Both would leak blocks on a stale `1`.

But R3 **never measured what the operation costs**, so:

1. There is no number to compare any alternative design against;
2. The rejection covers *deferral*, not *cheapening*;
3. Five rounds later, the free side has `449 Ir` / `56.1 Ir/block` / `84 Ir` /
   `54 Ir` figures for every sub-mechanism (R24-2, R24-5, R26-7, R28-1), and
   the alloc-hit side has **zero** decomposition below R23-3's aggregate
   `22.38 Ir/op` for the whole pop.

**Magnitude estimate (arithmetic, not measured).** R23-3 puts the magazine-hit
pop at **22.38 Ir/op**, 32.4% of `small_churn_16b`'s 69.0 Ir alloc+free pair.
R23-1 isolated `segment_base_of_ptr` at **9.03 Ir/call** (as a non-inlined
probe — an upper envelope; inlined it should collapse toward a single `AND`).
If base-derivation + the byte RMW is even a third of the 22.38, that is ~7 Ir
of a 69 Ir pair — **~10% of ordinary interleaved churn**, on the path executed
more often than any other in the allocator. That is the same order as several
wins the project *did* pursue on the free side.

**What evidence is needed before attempting anything** (in strict order — this
is a measurement request, not an implementation recommendation):

1. **Stage 1 — isolate the cost.** A paired iai arm in the R28-1 shape: a
   `bench-internals`-gated `pub unsafe fn` calling only the
   `segment_base_of_ptr` + `clear_magazine` pair on N live magazine-resident
   blocks, plus a shared-prefix arm, subtracted. This directly fills the hole
   R3 left. *This is the whole ask.* If the answer is <3 Ir, close the item
   permanently, in `OPEN_ITEMS.md`, with the number attached — which is more
   than R3 achieved.
2. **Stage 1b — cache-line accounting.** Whether the second touched region is a
   real L1 miss under a realistic multi-segment working set, or effectively
   free because the bitmap line stays resident. iai's `Ir` will not show this;
   `Estimated Cycles` / D1 misses will.
3. **Only then, Stage 2 design options** (none is recommended today):
   - store `(ptr, base)` or a packed `(segment_id, offset)` pair in the
     magazine slot so the base is not re-derived per hit — trades magazine
     footprint (currently `TCACHE_CAP * SMALL_CLASS_COUNT` pointers) for one
     less dependent computation;
   - move the residency bit to a structure `AllocCore` can also read *without*
     going through the segment metadata (R3's blocker #2 is precisely that
     `AllocCore` cannot see `tcache.slots` — it is a *visibility* constraint,
     not an inherent need for the bit to live in the segment).

**Expected pitfall, from this project's own record.** R24-3, R24-4, R25-3,
R26-7 all failed with "added bookkeeping costs more than the coalesced work,"
and R26-7 specifically found a **~10× Heisenberg gap** between a standalone
isolation and the in-context cost. Any Stage-2 attempt here must be an
in-context A/B on `small_churn_16b`, never a standalone-hook extrapolation.

### 1.2 [P1] The large cache has a 256 MiB-per-heap retention floor that never idle-decays — and, unlike the small pool, has never been measured

Rounds 25–27 spent four tasks and three gate reports quantifying the small
pool's `~+8 MiB/heap` post-teardown retention. The **large cache's** analogous
number is 32× larger by default and has no gate report at all.

The mechanism, in three lines:

- `src/alloc_core/large_cache_config.rs:48` —
  `DEFAULT_HEADROOM_BYTES = 256 * 1024 * 1024` (per heap), with the doc's own
  wording: *"The cache does not decay below this level."*
- `src/alloc_core/alloc_core_large_cache.rs:336` — `maybe_decay_large_cache`
  early-returns when `large_cache_used_bytes <= headroom_bytes`, so the clock
  is not even read below the floor.
- `src/alloc_core/alloc_core_large_cache.rs:367` — `run_decay_step`'s target
  *is* `headroom_bytes`, so even when decay does run it asymptotes to the floor
  and stops.

Combine with the two structural facts R27-3 established for the small pool and
that hold identically here:

- decay is **event-driven only** (fired from `alloc_large` / the large-dealloc
  branch); there is no background thread, so **pure idle reclaims nothing**;
- the only unconditional reclamation is `HeapCore::trim_for_recycle`
  (`src/registry/heap_core_ownership.rs:252-265`, `evict_all()` at `:263`),
  which runs **only at thread exit** (`src/global/tls_heap.rs:261`).

⇒ A **long-lived** thread (thread-pool worker, tokio worker, `main`) that once
peaked at large-object usage retains up to `min(8 cached spans, 256 MiB)` of
committed OS reservations **for the process lifetime**, plus the small pool's
`pool_byte_cap` (16 MiB default, R27-3-proven not to idle-decay). That is a
per-long-lived-thread idle floor on the order of **~272 MiB**, of which the
project has measured **16 MiB**.

Note this is *policy*, not a bug — "headroom" means exactly this. The finding
is the **measurement asymmetry and the documentation gap**, not the design.
Two specific concerns:

1. The prior review's P0 already flags that README calls `SeferAlloc::new()`'s
   defaults *"tuned for throughput-first workloads"* while the small-pool
   decision deliberately chose the RSS-conservative option. The large-cache
   default is the *opposite* skew (aggressively throughput-first at 256 MiB/heap
   with no idle shrink-back), so the README sentence is simultaneously wrong in
   both directions for two different subsystems.
2. R27-5 rejected an adaptive small-pool budget partly because *"the
   idle-shrink-back sub-problem is UNSOLVED within the project's
   no-background-thread constraint."* The same sub-problem exists here at 32×
   the scale and was never raised as a reason to look at it.

**Evidence needed:** run R27-3's own probe shape (subprocess-per-arm, victim
activation proven via `dbg_large_cache_used()` /
`dbg_large_cache_slot_sizes()`, peak / post-teardown / post-idle RSS) against a
**large-object** workload rather than the 1024-byte small churn, sweeping
`headroom_bytes` ∈ {0, 16 MiB, 64 MiB, 256 MiB} at 1/8/32 threads with
**long-lived, non-exiting** threads. Until that exists, no claim about
SeferAlloc's steady-state RSS versus mimalloc's is complete — mimalloc's
`mi_option_purge_delay` *does* reclaim on idle.

### 1.3 [P2] `virgin-zero-skip` is a built, tested, unshipped `calloc` optimization with a dangling promotion decision

`alloc_zeroed` is not a rare call — `Vec::with_capacity` for zeroable types,
`HashMap`, `vec![0u8; n]` and most FFI `calloc` traffic route through it. The
feature that lets it skip the `Node::zero` pass on genuinely virgin (freshly
committed, already-zero) pages exists, is CI-tested
(`.github/workflows/ci.yml`: `test (--features "production virgin-zero-skip
alloc-stats")`), and has two design docs plus a gate report. It is **not** in
`production`, and the promotion decision is dangling — see §3.2 for the
process side.

Magnitude: unknown, and honestly the one measurement that exists
(`docs/perf/R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md`) says *"No scenario shows
a statistically significant difference at this sample size"* — but that bench
is a single-threaded loop that its own report says does **not** capture the
shape the feature targets. **Evidence needed:** a `calloc`-shaped iai arm
(large `alloc_zeroed` on virgin pages vs. recycled pages) with the standard
paired-prefix subtraction, plus one wall-clock arm at a size where the memset
dominates (≥ 64 KiB). This is cheap — the feature already exists; only the
judge is missing.

### 1.4 Where I independently agree with the prior review, briefly

- Reservation-only overflow tier (R27-11): correctly gated; trigger 2 is
  genuinely unmeasured; do not open it before the Stage-1 breakdown.
- Linux sub-region `mremap`: highest asymptotic upside, correctly gated on a
  promotion-frequency Stage-1.
- Batch API: needs a consumer, not more internal tuning.
- `medium-classes`: profile, not default.
- Further `flush_class` micro-tuning: exhausted; five data points is enough.

I add nothing to these and did not re-derive them.

---

## 2. Что нужно улучшить в коде

### 2.1 [P0 — soundness] `tls_heap::dbg_restore_local_for_test` is a safe, ungated `pub fn` that installs a caller-supplied raw pointer as this thread's heap

**Site:** `src/global/tls_heap.rs:744`

```rust
#[doc(hidden)]
pub fn dbg_restore_local_for_test(saved: *mut HeapCore) {
    let _ = LOCAL.try_with(|c| c.set(saved));
}
```

It carries **no `#[cfg]` gate of any kind** (not `bench-internals`, not
`cfg(test)`), and its companion at `:732`:

```rust
#[doc(hidden)]
#[must_use]
pub fn dbg_mark_local_torn_for_test() -> *mut HeapCore { /* returns the real ptr */ }
```

hands the caller the live `*mut HeapCore`.

**Reachability from 100% safe code in a plain `--features production` build:**
`src/lib.rs:313` — `#[cfg(feature = "alloc-global")] #[doc(hidden)] pub mod
global;`; `src/global/mod.rs:30` — `pub mod tls_heap;`. `#[doc(hidden)]` hides
from rustdoc; it does **not** restrict Rust reachability — the project itself
states this verbatim at `src/alloc_core/alloc_core_core_diag.rs:299-302`.

**Failure scenario A (arbitrary pointer).** Safe downstream code calls
`sefer_alloc::global::tls_heap::dbg_restore_local_for_test(0xdead_beef as *mut _)`.
`current_for_alloc` (`tls_heap.rs:407`) classifies any value in
`1..=usize::MAX-1` as `CurrentHeap::Own(p)`; `SeferAlloc::alloc`
(`src/global/sefer_alloc.rs:567`) then does `unsafe { (*heap).alloc(layout) }`.
Immediate UB, no `unsafe` block anywhere in the caller.

**Failure scenario B (the worse one — aliasing, with *legitimate* pointers).**
Thread A calls `dbg_mark_local_torn_for_test()`, obtains its real
`*mut HeapCore` `P`, and sends `P` to thread B (it is a plain `usize`-shaped
value; nothing stops this). Thread B calls
`dbg_restore_local_for_test(P)`. Both threads now resolve `CurrentHeap::Own(P)`
and take `&mut HeapCore` to the same registry slot — a direct violation of the
**single-writer invariant** this file's own module doc (`:24-34`) declares as
the entire soundness basis for the raw-pointer TLS design. Concurrent
mutation of one `HeapCore`'s bins/tcache/`small_cur` follows: free-list
corruption, then arbitrary memory corruption. No `unsafe` required.

**Why it survived R24-6.** That audit's stated search was *"`unsafe fn dbg_*`
hooks BOTH marked `unsafe fn` AND reachable from plain `--features
production`"* (`OPEN_ITEMS.md`, R24-6 note). These two are **safe** `fn`s, so
they were outside the search by construction — the same inversion that let
`dbg_overflow_bitmap_clear_pass` (R25-1) live for a full round. CLAUDE.md's
benchmark-hook rule 1 covers exactly this shape ("*was a **safe** `pub fn`
…*"), and rule 2 requires `bench-internals` gating for hooks with no production
caller. Both rules are unmet here.

**Callers:** `dbg_mark_local_torn_for_test` / `dbg_restore_local_for_test` are
used by `tests/dealloc_only_no_bind_torn.rs` and (per the R28-2 closure note in
`docs/CORRECTNESS_OPEN_ITEMS.md`) `tests/r14_4_promotion_free_correctness.rs`.
Both are integration tests — exactly the `bench-internals` case.

### 2.2 [P0 — soundness] `AllocCore::dbg_force_decommit_retain_for` is a safe `pub fn` on a fully-public type that decommits live payload pages

**Site:** `src/alloc_core/alloc_core_small_pool.rs:696`

```rust
#[doc(hidden)]
#[cfg(feature = "alloc-decommit")]
pub fn dbg_force_decommit_retain_for(&self, ptr: *mut u8) -> bool {
    let base = os::segment_base_of_ptr(ptr);
    if !self.table.contains_base_ro(base) { return false; }
    if !matches!(SegmentHeader::kind_at(base), SegmentKind::Small) { return false; }
    let mut meta = SegmentMeta::new(base);
    Self::decommit_empty_segment_impl(&mut meta, base, false);   // <-- no live_count check
    true
}
```

Its own doc (`:680-682`) states the hazard plainly: *"the caller is responsible
for having emptied the segment first (this hook does NOT check `live_count`)."*

**This is worse than §2.1 on the exposure axis.** `AllocCore` is not merely a
`#[doc(hidden)]` module item — it is re-exported at the crate root:
`src/lib.rs:341`, `#[cfg(feature = "alloc-core")] pub use
alloc_core::{AllocCore, SegmentLayout};`. So `sefer_alloc::AllocCore` is stable
public API and this is a plain safe method on it.

**Failure scenario.** Safe code holding a legitimately-allocated small pointer
`p` from an `AllocCore` (public constructor, public `alloc`) calls
`core.dbg_force_decommit_retain_for(p)`. Both guards pass (`p`'s segment *is*
registered and *is* `Small`). `decommit_empty_segment_impl` returns the payload
pages `[small_meta_end, SEGMENT)` to the OS and resets `bump`. Every live block
in that segment — including `p` itself — is now backed by unmapped memory. The
next read or write through `p` is a use-after-free / access violation.
Zero `unsafe` in the caller.

**Gate:** `alloc-decommit` only — which is in `production` (Cargo.toml:399).
The hook has **no production caller**: its own doc says the `release_follows ==
false` leg it drives *"has ZERO production callers today."* CLAUDE.md
benchmark-hook rule 2 says such a hook "MUST default to gating behind the
`bench-internals` feature." It does not.

**Prior partial disclosure, and why it was not enough.** R23-4 (task #373,
`OPEN_ITEMS.md` item 6) *did* notice this function — but framed it as
*"unreachable from any production alloc/dealloc/realloc path, so it does not
weaken the production-path argument."* That framing was correct for the
question R23-4 was answering (is the `mremap` monotonicity argument safe?), and
wrong as a safety verdict: the R25-1 rule that arrived *two rounds later*
moved the bar from "reachable from a production code path" to **"reachable as a
safe `pub fn` from a production build."** Nobody went back and re-evaluated
R23-4's disclosure against the new bar, and the item is in **neither**
open-items index — so nothing would have prompted it.

**Recommended shape of a fix (not applied):** `pub unsafe fn` + `# Safety`
contract naming the `live_count == 0` precondition, **and** re-gate to
`bench-internals` (its sole caller is
`tests/alloc_zeroed_virgin_small_skip.rs`). This is the positive
`dbg_dealloc_own_thread_with_base` / `dbg_flush_class_only` pattern the project
already established.

**Systemic recommendation.** Neither §2.1 nor §2.2 was found by any prior
audit, because every prior audit searched a shape (`unsafe fn` + production
gate). The durable fix is a **grep-shaped tripwire**, in the spirit of the
existing `tests/no_stale_doc_references.rs::readme_unsafe_inventory_counts_match_reality`:
enumerate every `pub fn dbg_*` in `src/` that (a) takes a raw pointer argument
**or** returns one, and (b) is not `unsafe fn`, and (c) is not gated on
`bench-internals`/`cfg(test)`, and assert the set matches a
reviewed-and-committed allowlist. That converts a rule that has now been
violated three times (R25-1, §2.1, §2.2) into a compile-time-checkable
invariant instead of a recurring manual audit. My non-exhaustive sweep
(`grep -rnE '^\s*pub fn dbg_[a-z_0-9]*\(.*\*(mut|const)' src/ crates/`) returns
**33** candidates; the majority are genuinely read-only and correctly guard with
`assert!(self.table.contains_base_ro(base))`, which is why an allowlist rather
than a blanket ban is the right instrument.

### 2.3 [P2] `src/registry/tcache.rs`'s module doc has been false since RAD-5 (2026-07-11)

`src/registry/tcache.rs:4-6`:

> *"Push/pop touch only the magazine array (hot, sequential, cache-friendly);
> the block's own memory is not read until the user uses it (**no dependent
> load on the hit path**)."*

Since RAD-5 the hit path does `segment_base_of_ptr(issued)` and then a
read-modify-write of a byte in the segment's `magazine_bitmap` — a load and a
store dependent on the popped pointer, into a region the magazine array does
not cover (§1.1). The block *body* is still untouched, so the sentence is half
true, but "no dependent load on the hit path" is now simply wrong.

This is not cosmetic. A stale performance claim in the doc of the hottest data
structure is exactly the kind of thing that makes a reviewer skip the path —
and the alloc-hit path *has* gone un-decomposed for five rounds while the free
path got five separate gates.

### 2.4 [P2] The small-segment pool is NUMA-blind while `numa-aware` is on

`src/alloc_core/alloc_core_small.rs:1882-1891` reserves fresh small segments on
`self.current_node_cached()`, and `SegmentHeader` carries a `node_id` field
(`src/alloc_core/segment_header.rs:619`, sentinel `NO_NODE_RAW` at `:708`).
But `src/alloc_core/alloc_core_small_pool.rs` contains **no** reference to
`node_id` at all — neither `release_or_pool_empty_segment` (`:236`) nor
`unpool_if_present` (`:483`) nor the `find_segment_with_free` reuse path
consults it.

Consequence: `current_node_cached` deliberately re-queries every
`NUMA_NODE_REFRESH_PERIOD = 128` calls (`src/alloc_core/alloc_core.rs:1024`)
precisely because *"a thread may migrate to another NUMA node"* — and when it
does, the pool silently hands back segments bound to the **old** node, and
`find_segment_with_free` silently reuses off-node free-list blocks. The
allocator pays the `mbind`/`VirtualAllocExNuma` cost at reservation time and
then discards the resulting affinity at reuse time.

Registry-level hygiene is correct — `invalidate_numa_node_cache` *is* called on
re-claim (`src/registry/heap_registry.rs:176`) — so this is specifically a
*segment-reuse* gap, not a cache-staleness gap.

This is a feature that is off by default and has no committed multi-node
measurement, so P2 rather than P1. But it means the `numa-aware` tier's central
value proposition (node-local memory) is **not** upheld in steady state under
the pool/hysteresis path that `production` makes the dominant reuse route.
Evidence needed before any change: a two-node measurement (the QEMU work
`.github/workflows/ci.yml:1024-1028` already names as the un-done Phase 2.1),
because on the single-node CI runners this is unobservable by construction.

### 2.5 [P2] Silent feature interaction: `numa-aware` disables `small-segment-lazy-commit`

`src/alloc_core/alloc_core_small.rs:1889-1921`: the `#[cfg(feature =
"numa-aware")]` arm always uses the eager `numa::reserve_aligned_on_node`, and
the comment states the intent (*"NUMA reservations go through
VirtualAllocExNuma and must not be disturbed (P2 gate)"*). That is a
defensible decision, but it means a user who enables **both** features gets the
eager path with no diagnostic — `small-segment-lazy-commit` becomes a silent
no-op. I found no note of this in the README feature table. It belongs in the
feature documentation, next to the analogous `min(pool_segments, bytes/SEGMENT)`
no-op that R27-1 had to discover the hard way.

### 2.6 [P3] `README.md:1101`'s `bench-internals` row lists two hooks; there are three

The tier-2 table at `README.md:443` is correct and includes `dbg_flush_class_only`
(R28-1). The feature-table row at `:1101` still enumerates only
`dbg_dealloc_own_thread_with_base` and `dbg_push_coarse_only_entry`. Two
statements of the same fact in one file, one stale. The counts tripwire
(`tests/no_stale_doc_references.rs:312`) checks *counts*, not this enumeration,
so nothing catches it.

### 2.7 Code items where I agree with the prior review

Its seven code recommendations (keep the `flush_run` guards, avoid copied
experimental implementations, split `heap_core_diag.rs` by concern, stricter
than `#[doc(hidden)]` visibility, per-segment state accounting, keep batch APIs
out of `production`, investigate R28-2 first) are all sound. §2.1/§2.2 above
are the concrete, citable instance of its item 4 that it stated only in the
abstract.

---

## 3. Что нужно улучшить в проекте

### 3.1 [P0 — process] `docs/perf/IAI_BASELINE.md` holds eight honest-rejects with revisit triggers, and none of them is in `OPEN_ITEMS.md` — despite being squarely in that index's declared scope

`docs/perf/OPEN_ITEMS.md:44-46` defines its own scope:

> *"**Scope.** This index covers `docs/perf/*.md` only (gate reports + perf
> design docs)."*

and its `[L]` tier (`:53-56`):

> *"**[L]** low-priority — an 'honest reject with a revisit trigger'; not
> recommended now but documented for completeness."*

`docs/perf/IAI_BASELINE.md` is a `docs/perf/*.md` file, 1,488 lines, containing
**eight** sections whose headings literally say "honest-reject":

| section | line | subject |
|---|---:|---|
| X4 | 219 | both recycle experiments, measured and declined |
| X6 | 246 | clz `class_for` vs the 16 KiB `SIZE2CLASS` LUT |
| X5 | 270 | per-class segment-queue bitmap |
| G1 | 530 | magazine double-free oracle fold into `AllocBitmap` |
| T10 | 1088 | per-class "last found segment" hint |
| **R3** | **1206** | **batching `MagazineBitmap`'s per-hit clear (see §1.1)** |
| R1 | 1264 | per-segment availability hint for `find_segment_with_free` |
| R5-R2b | 1335 | the wall-clock churn regression is not an Ir regression |

Several carry explicit revisit conditions — X5's is recorded verbatim
(`:1200-1204`: *"The shape to revisit is the FULL per-class queue…"*).

`grep -c` for each of `X4`, `X6`, `X5`, `G1`, `T10`, `R3 honest`, `R1 honest`,
`R5-R2b`, and for the string `IAI_BASELINE` itself, against
`docs/perf/OPEN_ITEMS.md`: **all zero.**

**Why this matters more than the count suggests.** The index's mandatory
round-start ritual is *"read this file end-to-end and decide, for each open
item, whether this round closes it, defers it, or leaves it."* Because these
eight were never migrated, that ritual is **structurally blind** to the single
largest concentration of documented, trigger-bearing rejects in the project.
§1.1 is the concrete damage: R3's "no baseline was taken; there is nothing to
measure" is an explicit, self-declared measurement debt on the hottest path,
recorded in 2026-07-13 and never surfaced to any round-start queue in the
sixteen rounds since.

This is the **same failure class** as R18-8 (an open item hung three rounds)
and R22-3 (follow-ups in no index) — the two incidents that created these
indexes. It is larger than either.

**Recommendation:** a one-time migration pass adding all eight to the `[L]`
tier with their existing verdicts and revisit triggers (cheap — they are
already written), plus one line in the index's Convention section naming
`IAI_BASELINE.md` explicitly as an in-scope source. Optionally, a tripwire in
`tests/no_stale_doc_references.rs` asserting that every `## ... honest-reject`
heading in `docs/perf/*.md` has a matching citation in `OPEN_ITEMS.md`.

### 3.2 [P1 — process] At least three implemented, CI-tested features have an explicitly-deferred promotion decision tracked in neither index

Cross-referencing every Cargo feature against both indexes:

| feature | in `production`? | perf docs mentioning it | `OPEN_ITEMS.md` | `CORRECTNESS_OPEN_ITEMS.md` |
|---|---|---:|---:|---:|
| `virgin-zero-skip` | no | 6 | **0** | **0** |
| `small-segment-lazy-commit` | no | 4 | **0** | **0** |
| `alloc-lazy-commit` | no | 9 | **0** | **0** |
| `page-map-diag` | no | 2 | 0 | 0 |
| `exact-span-large` | no | 13 | 4 | 4 |
| `large-reserved-capacity` | no | 12 | 1 | 0 |
| `batch-api` | no | 7 | 3 | 0 |
| `medium-classes` | no | 33 | 10 | 25 |

The `virgin-zero-skip` case is the sharpest, because the deferral is
**explicit and in-tree**, `Cargo.toml:714-716`:

> *"Promotion to `production` is a separate, explicit decision — see this
> task's own report for the GO/CONDITIONAL-GO/NO-GO recommendation."*

Following that pointer: `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:563-567`
defines a *"Stage 3 (promotion gate, ~1h)"* — *"on a green Stage 3 consider
promoting `virgin-zero-skip` into `production`."*
`docs/perf/R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md` is the only later
measurement, and it **states no promotion verdict at all** (it is a
was/now gate for the R13-3 *magazine fix*, and its own headline is *"No
scenario shows a statistically significant difference at this sample size"*).

So: Cargo.toml points at a GO/NO-GO recommendation that does not exist; the
design doc's Stage-3 gate was never formally run or closed; and neither index
carries the item. A pending shipping decision lives only inside a
`Cargo.toml` comment — precisely the *"a flag that lives only inside a single
commit message body or code comment is exactly the failure mode this index
exists to prevent"* case (`docs/CORRECTNESS_OPEN_ITEMS.md:46-50`).

`small-segment-lazy-commit` is analogous: `docs/perf/R12_9_PRIMORDIAL_LAZY_COMMIT.md:16`
explicitly scopes itself as *not* promoting that surface, and its sibling
`primordial-lazy-commit` **was** promoted (Cargo.toml:399) while it was not —
with no recorded decision.

**Recommendation:** add a **third, mechanical section** to `OPEN_ITEMS.md` (or a
small standalone `docs/FEATURE_PROMOTION_STATUS.md`): one row per non-`production`
feature, with columns *shipped-behind-flag / has-gate-report / promotion verdict
(GO / NO-GO / NEVER-DECIDED) / evidence*. This is derivable in an afternoon
from what already exists, and it makes "never decided" visible rather than
indistinguishable from "decided NO-GO."

### 3.3 [P2 — CI] Every per-PR job compiles the NUMA shim in **mock** mode; the real-syscall integration compiles only weekly

`--all-features` enables `numa-aware-mock` (Cargo.toml:592), which enables
`numa-shim/mock`. `crates/numa/src/lib.rs` gates its real implementations
`#[cfg(not(feature = "mock"))]` (`:158`, `:204`, `:248`). Therefore the
`clippy (--all-features)` and `test (--all-features)` jobs — the broadest
per-PR coverage — compile the **mock** arms, not the `mbind(2)` /
`VirtualAllocExNuma` arms.

The shim crate itself *is* covered per-PR on real kernels
(`numa-shim-windows`, `numa-shim-macos` run `cargo test -p numa-shim` without
`mock`). What is **not** covered per-PR is the *integration*: `sefer-alloc`
built with `numa-aware` and **without** `numa-aware-mock`. The only job doing
that is `numa-real-kernel`, guarded
`if: github.event_name == 'schedule' || 'workflow_dispatch'`
(`.github/workflows/ci.yml:1036`).

So a compile break in `src/alloc_core/numa.rs`'s integration (e.g. a signature
drift in `reserve_aligned_on_node`, `src/alloc_core/numa.rs:85`) surfaces at
worst a week later. The `feature-powerset` job would also catch it — but it is
weekly too (`cron '0 6 * * 1'`, `:1132`).

**Cheap fix (evidence: none needed, it is a compile-only concern):** add a
single `cargo check --features "production numa-aware"` step to an existing
per-PR job. That is one typecheck invocation, not a new job.

### 3.4 [P2 — process] Two additions to the prior review's provenance recommendation

The prior review recommends recording immutable source identity per
measurement. Two concrete supporting observations from this pass:

1. **The `paired_ab_runs` citation discipline is working and should be named
   as the model.** `docs/perf/paired_ab_runs/` is `.gitignore`d wholesale
   (`.gitignore:6-8`), yet all **10** JSON files cited by name across
   `OPEN_ITEMS.md` / `R10_2` / `R10_5` / `R14_3` **are** tracked (`git ls-files`
   confirms), while the ~40 uncited ones on disk are not. That is precisely the
   `git add -f`-what-you-cite policy CLAUDE.md defines for `_raw_*.log`,
   applied correctly to a second artifact class. It is worth naming in the
   convention text so it is not re-derived.
2. **The `_raw_*.log` corpus is now 114 tracked files.** Combined with
   `docs/perf/`'s 6.4 MB, the prior review's P2 on artifact volume is
   quantitatively supported. I add only that any archival pass must preserve
   the cited-name → tracked-file mapping above, or the citations silently
   break.

### 3.5 [P2 — process] The measurement effort is asymmetric across the alloc/free axis, and nothing in the process notices

Free-path sub-costs with committed numbers: cheap push (43–44 Ir), one overflow
event (571→581 Ir), bitmap-clear pass (84 Ir), `flush_class` (449 Ir),
compaction+push residual (~48 Ir), stage zero-init (~54 Ir), STAGE_CAP delta
(−4,065 Ir), `FLUSH_N` sweep, N-grid at 0/1/8/16/17/32/64/80/81/128/200/256/512/1024.

Alloc-path sub-costs with committed numbers: the magazine-hit pop as **one
aggregate** (22.38 Ir), `segment_base_of_ptr` (9.03 Ir), pure carve (23.05
Ir), two *derived* refill figures (~1099 / ~961 Ir, explicitly not isolated).

Every allocation is followed by at most one free, so the two paths are executed
equally often; the attention ratio is roughly 8:1. The proximate cause is
traceable: R23-3's headline mis-attributed 80.8% of a free's cost to the
own-thread body, R24-1 corrected it, and every subsequent round chased the
corrected free-side target — a perfectly rational local sequence that nothing
in the process was positioned to interrupt.

**Recommendation:** make the round-start ritual include one line — *"which side
of alloc/free has the older newest measurement?"* — or, equivalently, keep a
tiny table in `IAI_BASELINE.md` of "last round in which each named hot-path
sub-mechanism was measured." This costs nothing and would have surfaced §1.1
several rounds ago.

---

## 4. What I checked and found clean (recorded so it is not re-checked)

- **TODO / FIXME / XXX / HACK / `todo!` / `unimplemented!` across `src/`,
  `crates/`, `benches/`, `examples/`:** zero real markers. The seven `grep`
  hits for "placeholder" are all descriptive prose about a value patched later
  in the same function (`alloc_core_large.rs:291,315`;
  `segment_header.rs:745,766`; `lock_free_region.rs:422`), not deferred work.
- **CLAUDE.md's "no inline tests" rule:** `grep -rn '#\[cfg(test)\]' src/
  crates/*/src/` → **zero** matches. Fully upheld.
- **CLAUDE.md's "no doctests" rule:** zero runnable rustdoc fences in `src/`.
  Fully upheld.
- **Unsafe inventory tripwire:** `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]'`
  yields 20 tier-1 + 62 tier-2 across 17 files; `README.md:448-449` states
  exactly `20` / `62` / `17`. Consistent. (This is the counts check; §2.6 is a
  separate enumeration drift the counts check cannot see.)
- **Repo hygiene:** `examples_scratch/` and `examples_scratch_tmp/` exist but
  are empty and untracked — harmless, though deleting them would remove two
  confusing directory names from a fresh clone's `ls`.
- **`#[ignore]`d tests:** exactly three, each with an in-file rationale and a
  named deterministic replacement or manual-run note
  (`dealloc_sublinear.rs:90`, `regression_segment_table_tombstone_rebuild.rs:120`,
  `regression_w3_stats_aliasing_miri.rs:145`). No silent coverage loss.
- **I1–I6 invariants** (`docs/INVARIANTS.md:8-20`, Region/Handle face): covered
  by 11 test files including `region_invariants.rs` (the miri target),
  `differential.rs`, `compaction.rs`, and three loom suites. Still fully
  covered; nothing to flag.
- **Cross-thread `realloc`:** `HeapCore::realloc`
  (`src/registry/heap_core_free.rs:911`) correctly splits own-segment vs.
  foreign, validates segment magic **and** `old_layout.size()` against the
  committed span before any copy (`:870-885`), and returns null rather than
  reading out of bounds. The `Vec`-moved-across-threads-then-grown case is
  handled. No finding.
- **TLS teardown ordering:** `src/global/tls_heap.rs:36-73`'s three-way
  argument (no `Drop` on `LOCAL`; TLS-access monotonicity in program order;
  `Err` → fallback) is correct and does not depend on destructor order.
  `finish_bind`'s arm-guard-before-publish rollback (`:629-655`) correctly
  closes the claimed-but-unguarded slot leak. No finding beyond §2.1's misuse
  surface.
- **CI breadth:** 34 jobs including miri (5 variants), loom (4), TSan,
  multi-arch, `no_std`, MSRV, `cargo-deny`, fuzz-build, and the weekly
  `feature-powerset`. Genuinely strong; §3.3 is the one narrow gap.

---

## 5. Recommended additions to the Round 29 queue

These are **additive** to the six tasks already filed (#432–#437); none
duplicates them.

**P0 — soundness, before anything else**

1. Re-gate + `unsafe`-ify `tls_heap::dbg_restore_local_for_test` /
   `dbg_mark_local_torn_for_test` (§2.1) and
   `AllocCore::dbg_force_decommit_retain_for` (§2.2). Both callers are
   integration tests; `bench-internals` is the established instrument.
2. Add the `pub fn dbg_*` + raw-pointer + not-`unsafe` + not-`bench-internals`
   **allowlist tripwire** (§2.2, systemic). Three violations of the same rule
   in four rounds is a tooling problem, not an attention problem.

**P1 — the one measurement that would change what Round 30 works on**

3. Stage-1 isolation of the alloc-hit `segment_base_of_ptr` + `clear_magazine`
   pair (§1.1). This is the single highest-information-per-hour measurement
   available: it closes a 16-round-old self-declared debt, on the hottest path,
   and either opens a new optimization region or permanently closes one with a
   number attached.

**P1 — index integrity**

4. Migrate `IAI_BASELINE.md`'s eight honest-rejects into `OPEN_ITEMS.md`'s
   `[L]` tier and name that file as in-scope in the Convention section (§3.1).
5. Add the feature-promotion status table; resolve or explicitly re-defer
   `virgin-zero-skip`, `small-segment-lazy-commit`, `alloc-lazy-commit` (§3.2).

**P2**

6. A large-cache retention gate mirroring R27-3's methodology, at
   `headroom_bytes` ∈ {0, 16, 64, 256 MiB} with long-lived threads (§1.2).
7. One `cargo check --features "production numa-aware"` step in a per-PR job
   (§3.3).
8. Doc corrections: `tcache.rs`'s "no dependent load" claim (§2.3), the
   `numa-aware` × `small-segment-lazy-commit` interaction (§2.5), the
   `bench-internals` hook enumeration (§2.6).

---

## 6. Final answer to the three questions

**Что ещё можно сильно ускорить.** The prior review's list (reservation-only
tier, Linux `mremap`, batch consumers, medium-classes profile) is correct and I
endorse it unchanged. What it misses is that the project has decomposed the
**free** path to five decimal places while the **alloc-hit** path — executed
just as often — still carries one mandatory segment-metadata RMW whose cost
the project's own record says was never measured. That is the cheapest
remaining high-information measurement, and it is a prerequisite, not a
competitor, to the strategic items already queued. The second unexploited item
is a `calloc` optimization that is already built, tested and unshipped.

**Что нужно улучшить в коде.** Two safe `pub fn`s reachable from a plain
`production` build let 100%-safe downstream code (a) install an arbitrary or
foreign-thread `*mut HeapCore` as this thread's heap, breaking the
single-writer invariant the whole TLS design rests on, and (b) decommit the
pages under live allocations. Both are the exact R25-1 shape the project wrote
a rule about; both survived every prior audit because those audits searched for
`unsafe fn`s. The durable fix is a tripwire, not another manual pass.

**Что нужно улучшить в проекте.** The two open-items indexes are the project's
best institutional invention, and they have a hole: the largest single
collection of trigger-bearing honest-rejects in the tree
(`docs/perf/IAI_BASELINE.md`, eight sections, in-scope by the index's own
definition) has never been indexed, and at least three shipped-behind-flag
features have promotion decisions recorded only in a `Cargo.toml` comment. Both
are the same failure class the indexes were created to prevent, and both are
cheap to close because the underlying analysis already exists — it just needs
to be findable at round start.
