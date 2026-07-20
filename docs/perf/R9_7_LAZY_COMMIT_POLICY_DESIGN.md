# R9-7 — Small-segment pool commit/RSS tradeoff policy: DESIGN-ONLY (no code change)

**Task:** design a policy for the small-segment hysteresis pool's commit/RSS
tradeoff — today's "latency-first" behaviour (R8-10) vs a new opt-in "low-RSS"
preset/policy the external review asked for — so a memory-constrained
deployment can trade some latency back for lower committed RSS.
**Outcome:** **DESIGN-ONLY.** No `src/`, `Cargo.toml`, or `tests/` file is
modified. The deliverable is this doc. §9 states precisely *why* no prototype
ships this session; §11 gives the staged plan that would land it.
**Date:** 2026-07-20
**Base revision:** `main` @ `fd28ff8` (R9-6 just landed; the small-path
substrate under analysis is R8-10 task #223 @ `852828e`, which landed
2026-07-20). The decay mechanism under analysis
(`maybe_decay_small_pool`, `src/alloc_core/alloc_core_small_pool.rs:445`) and
the decommit primitive R9-5 characterized (`decommit_empty_segment_impl`,
`alloc_core_small_pool.rs:631`) are unchanged since R8-10.
**Platform:** Windows 10 Pro x86-64 (analysis host). The correctness argument
is platform-parametric (Windows `MEM_DECOMMIT`/`MEM_COMMIT` vs Unix
`MADV_DONTNEED` vs macOS `MADV_DONTNEED` laziness) and is resolved per-platform
in §6; no measurement here is host-dependent because no measurement is
performed (§8 is analytical, mirroring R9-5 §8).

---

## 0. TL;DR

The review's concern is real and verified: today's latency-first pool can
retain **exactly 16 MiB of committed payload per materialized heap** while
pooled (§2 — the review's "16 MiB" number is correct against
`SmallSegmentPoolConfig::DEFAULT`: `DEFAULT_POOL_SEGMENTS = 4` × `SEGMENT =
4 MiB` = 16 MiB, `src/alloc_core/small_segment_pool_config.rs:113-117`), and
the only drain is `maybe_decay_small_pool`, which FULLY RELEASES one
FIFO-oldest segment per `decay_interval` (default 1 s) — there is no
intermediate "committed-but-cheap-to-revive" state. The review asks for a
THIRD state: decommitted-but-still-pooled (payload returned to the OS, segment
stays registered and in the pool list, ready for a cheap recommit-and-reuse
rather than a full fresh reserve).

**This report's central technical finding (§3) is that the review's imagined
shape for that third state — "decommit the payload, keep the free-list metadata
intact for a cheap recommit" — is unsound as stated.** The small-path free list
is an INTRUSIVE singly-linked chain whose `next` link lives in the FIRST WORD
OF THE BLOCK BODY, i.e. INSIDE the payload (`Node::write_next` /
`Node::read_next`, `src/alloc_core/node.rs:74-108`; written by `dealloc_small`
at `alloc_core_small.rs:1396`, read by `pop_free` at `alloc_core_small.rs:836`).
Decommitting the payload therefore DESTROYS the chain links, not just the block
contents: on Windows/Linux the recommit is demand-zero, so every `next` reads
as NULL and each class free list collapses to its single head block (the rest
of each chain is silently leaked); on macOS the recommit is advisory and `next`
reads as garbage (wild pointers, caught only by the `hardened`-gated
interior/membership guard, not by production). Only the BinTable HEADS (which
live in the never-decommitted metadata `[0, small_meta_end)`) survive — and a
head with no chain is not a usable free list. `pop_free`'s own comment
(`alloc_core_small.rs:885-892`) already encodes this invariant: "a decommitted
segment was reset to an empty free list, so `pop_free` finds nothing there" —
which is why `pop_free` has NO recommit branch today.

The ONLY existing decommit-stay-registered primitive —
`decommit_empty_segment_impl(_, _, release_follows=false)`, the "B3" path
R9-5 §4.3 characterized — does NOT keep the free list; it does a FULL RESET
(nulled free lists, `bump` rewound to `payload_start`, page-map / alloc-bitmap
/ magazine-bitmap re-init, `alloc_core_small_pool.rs:689-729`), turning the
segment into a BLANK registered carve target. So the only VIABLE shape for the
third state is **"decay-gated B3-style decommit-and-reset to a blank registered
carve target"** (§5), NOT the review's "keep the free list intact" model.

That viable shape is a real policy win (§7–§8: it decouples the pool's REUSE
BREADTH from its COMMITTED RSS, so a deployment can keep many segments
available for cheap recommit-reuse while committing only the hot few), but it
re-introduces the `release_follows=false` decommit primitive to production —
EXACTLY the re-introduction R9-5 risk area 3 (§4.3) flagged as load-bearing
for the (unshipped) virgin-zero-skip, and it needs carve-target-pop wiring in
`reserve_small_segment` that does not exist today (§5.3). That is a
decommit/recommit correctness surface of the same class R9-5 said warrants an
"especially careful line-by-line safety review," and it is strictly larger than
this session's "clearly minimal and safe" prototype bar. **Design-only is the
honest, high-value outcome** — mirroring R9-5's outcome for a task with the
same risk profile. A safe stopgap that ships NOW with zero new decommit surface
is documented in §10 (knob-tuning recipe).

---

## 1. Scope recap — today's two states and the missing third

A small segment that just emptied (`live_count == 0`, not the current carve
target) is routed through `release_or_pool_empty_segment`
(`alloc_core_small_pool.rs:236`). Today it lands in exactly one of TWO states:

1. **POOLED-WARM** (`alloc_core_small_pool.rs:257-272`, the R8-10 admission
   path): the segment is pushed onto the intrusive pool list and left EXACTLY
   as it was the instant it emptied — still registered in the `SegmentTable`,
   pages still COMMITTED, `bump` wherever it was (near `SEGMENT`, fully
   carved), `decommitted == false`, every class free list still populated with
   the just-freed blocks. NOTHING is reset. Reuse is via
   `find_segment_with_free`'s free-list path (`pop_free`), which costs NO OS
   syscall — the hysteresis win. This is the state R8-10 made load-bearing.

2. **RELEASED** (`alloc_core_small_pool.rs:274-285` pool-full / disabled path,
   and `maybe_decay_small_pool:466-485` decay path): the release-follows reset
   (`decommit_empty_segment_for_release` →
   `decommit_empty_segment_impl(_, _, release_follows=true)`, the fast path
   that only rewinds `bump` + sets `decommitted=true`) + `table.recycle`, then
   the whole 4 MiB reservation goes back to the OS (`os::release_segment` /
   `MEM_RELEASE` / `munmap`). The segment ceases to be registered. A future
   allocation at any base pays a full fresh `reserve_small_segment`
   (`mmap` / `VirtualAlloc(MEM_RESERVE|MEM_COMMIT)` + full metadata init).

The decay timer (`maybe_decay_small_pool`, `alloc_core_small_pool.rs:445`)
moves a segment from state 1 to state 2 — one FIFO-oldest (list TAIL) entry per
elapsed `decay_interval` (default 1000 ms,
`large_cache_config.rs:51 DEFAULT_DECAY_INTERVAL_MS`). It is the ONLY
production path that ever evicts a pooled segment, and it always evicts to
FULL RELEASE. There is no path that evicts to "decommitted but still
registered + pooled." That is the missing THIRD state.

---

## 2. Baseline — verifying the review's "16 MiB" claim

**Claim (review):** "the standard pool retains up to 16 MiB per materialized
heap."

**Verification against `SmallSegmentPoolConfig`
(`src/alloc_core/small_segment_pool_config.rs`):**

```text
DEFAULT_POOL_SEGMENTS  = 4                      (small_segment_pool_config.rs:114)
DEFAULT_POOL_BYTE_CAP  = 16 * 1024 * 1024       (small_segment_pool_config.rs:117)
SEGMENT                = 4 MiB                  (os::SEGMENT)
effective cap          = min(4, 16 MiB / 4 MiB) = min(4, 4) = 4 segments
committed payload cap  = 4 * 4 MiB              = 16 MiB   ← exactly the review's number
```

The pool is per-`AllocCore`, and `AllocCore` is per-heap (one owner thread
each, `src/registry/heap_core.rs`). So the "16 MiB per materialized heap"
figure is **CORRECT and precise**: a process with H materialized heaps can hold
up to `H × 16 MiB` of committed small-pool payload at steady state. (This is
COMMITTED payload, i.e. the commit-charge / RSS axis `proc-memstat::MemStat`
calls `commit`/`rss` — see §4(c). Metadata pages `[0, small_meta_end)` are
additionally committed per segment but are ≈ tens of KiB and not the review's
concern.) **The review's number is verified, not a rounding.**

The cap is a SYNCHRONOUS, instant-of-admission bound
(`alloc_core_small_pool.rs:206-209`: "the pool never holds MORE than `pool_cap`
at any instant, mid-scan or otherwise") — so 16 MiB is a hard ceiling, not an
amortized figure. The `decay_interval` only governs how fast the ceiling
drains to zero once the workload goes quiet, not how high it can spike.

---

## 3. The central finding — why "keep the free list intact" is unsound

The review's option (a) imagines the third state as: "decommit the payload,
keep registered + pooled + free-list metadata intact for a cheap recommit."
This section proves that shape is unsound as stated, and that the only viable
shape is a full-reset blank carve target (§5).

### 3.1 Where the free-list chain physically lives

The small-path per-segment free list is an INTRUSIVE singly-linked list with
TWO storage regions:

- **The HEAD offset** — one `u32` per class, stored in the `BinTable`, which
  lives in the segment's METADATA region `[0, small_meta_end)`. This region is
  NEVER decommitted (`alloc_core_small_pool.rs:660-661`: "Metadata and the
  remote-free ring are NEVER decommitted: they live in `[0, meta_end)`"). So
  the head survives a payload decommit. ✓
- **The `next` link** — an absolute `*mut u8` stored in the FIRST WORD of each
  free block's BODY, i.e. INSIDE the payload region
  `[small_meta_end, SEGMENT)`. `Node::write_next(block, next)` does
  `block.as_ptr() as *mut *mut u8; ptr.write_unaligned(next)`
  (`src/alloc_core/node.rs:74-91`); `Node::read_next(block)` reads it back
  (`node.rs:100-108`). `dealloc_small` writes it on every free
  (`alloc_core_small.rs:1396`); `pop_free` reads it on every reuse
  (`alloc_core_small.rs:836`). **This word is payload, and a payload decommit
  discards it.**

So "keep the free-list metadata intact" preserves only the head offset. The
chain itself is payload-resident and is destroyed by any payload decommit.

### 3.2 What decommit + recommit does to the chain, per platform

A pooled segment is, by the admission rule (`alloc_core_small_pool.rs:177-
188`), FULLY CARVED (`bump` near `SEGMENT`) with every block on some class free
list (it emptied at `live_count == 0`, so every block is free). Decommitting
its payload `[small_decommit_start(), SEGMENT)` and later recommitting it
yields, per backend (`crates/vmem/src/lib.rs:416-503`):

- **Windows (`MEM_DECOMMIT` then `VirtualAlloc(MEM_COMMIT)`):** recommit is
  demand-zero. Every block's first word reads `0x0` → `Node::read_next`
  returns null → `pop_free`'s `next.is_null()` arm
  (`alloc_core_small.rs:864-866`) sets the new head to `FREE_LIST_NULL`. The
  chain collapses to its SINGLE head block; every subsequent block in that
  class's chain is **silently leaked** (still marked free in the alloc bitmap,
  still counted as a free block, but unreachable from any free list). This is
  a correctness disaster, not a performance hiccup.
- **Linux (`MADV_DONTNEED`):** re-access supplies a fresh zero page
  transparently. Identical to Windows: `next` reads null, chain collapses.
- **macOS (`MADV_DONTNEED`, advisory + lazy):** re-access is NOT zero-fill
  (`crates/vmem/src/lib.rs:448-456`, and R9-5 §2 "OS-zero guarantee"). `next`
  reads as ARBITRARY GARBAGE — a wild pointer. `pop_free` computes
  `(next - segment) as u32` (`alloc_core_small.rs:870`), producing a garbage
  offset, and the NEXT `pop_free`/`drain_freelist_batch` derefs it via
  `Node::deref(segment, off)` (`segment.add(off)`) — an out-of-bounds add, UB
  per `node.rs`'s SAFETY contract. The `hardened`-gated membership guard
  (`alloc_core_small.rs:858-863`) truncates a wild chain, but `hardened` is
  OFF in `production` (`Cargo.toml:209 production = […]` does not include
  `hardened`), so production macOS would corrupt.

### 3.3 Why `pop_free` has no recommit branch today (and why that matters)

`pop_free`'s own comment (`alloc_core_small.rs:885-892`) encodes the
invariant:

> "A popped block always comes from a COMMITTED payload (a decommitted
> segment was reset to an empty free list, so `pop_free` finds nothing there),
> so no recommit is needed on this path — only `carve_block` writes fresh
> payload and thus recommits."

This invariant HOLDS today precisely because the only decommit primitive that
leaves a segment registered (`release_follows=false`, the B3 path) does a FULL
RESET that NULLS every class free list (`alloc_core_small_pool.rs:697-701`:
`bt.set_head(c, FREE_LIST_NULL)` for all classes). So after a B3 decommit, the
BinTable is empty → `find_segment_with_free`'s head-non-null check
(`alloc_core_small.rs:482, 724`) never selects the segment → `pop_free` never
runs on it. The invariant is load-bearing: it is the reason the payload-resident
`next` link is always readable when `pop_free` reads it.

**A third state that decommits the payload but keeps the free list non-empty
(as the review imagines) BREAKS this invariant.** `find_segment_with_free`
would select the segment (head non-null), return it, and `pop_free` would read
a destroyed `next` word — the corruption in §3.2. This is why the naive shape
is unsound, and it is the finding that makes this report's recommendation
differ from the review's suggestion.

### 3.4 The rejected alternative — rebuild the chain on recommit

One could, in principle, recommit-then-rebuild: walk the alloc bitmap (which
records free vs allocated per `MIN_BLOCK` slot, and survives decommit because
it lives in metadata) to re-discover every free offset, then re-thread the
`next` links. This is rejected:

- It is `O(payload)` work on the REUSE path (a ~4 MiB bitmap walk + a write per
  free block) — which **defeats the entire purpose** of the third state (a
  "cheap recommit-and-reuse"). A fresh carve target costs `O(1)` to revive
  (§5); a chain rebuild costs `O(blocks)`. The third state exists to be
  cheaper than a full release; a chain rebuild is not.
- It is a large, novel correctness surface (bitmap-walk + free-list
  reconstruction + directory-bitmap sync), strictly riskier than the §5 model.

So the chain is NOT rebuildable cheaply. The only viable third state does not
carry a free list at all — §5.

---

## 4. Candidate evaluation

### (a) Selective decommit-on-decay (review's primary suggestion)

**As stated by the review ("keep free-list metadata intact"):** UNSOUND (§3).
Not viable without either a chain rebuild (§3.4, too expensive) or a `pop_free`
recommit branch + wild-pointer hardening (a correctness surface larger than
this session's bar).

**Corrected form — decay-gated B3-style decommit-and-reset to a blank carve
target (§5):** VIABLE and the recommended policy. The decay tick, instead of
fully releasing the FIFO-oldest entry, runs the EXISTING
`decommit_empty_segment_impl(_, _, release_follows=false)` full-reset on it
(payload decommitted, free lists nulled, `bump` rewound, bitmaps re-init) and
KEEPS it registered + in the pool list as a blank carve target; a SECOND decay
interval with no reuse fully releases it. This re-introduces the
`release_follows=false` primitive to production (R9-5 §4.3) and needs
carve-target-pop wiring (§5.3) — a real but bounded surface.

### (b) Two-tier pool cap (hot sub-pool always committed + cold decommit-eligible)

This is option (a) with extra config structure: a `hot_cap` (always-warm) and
a `cold_cap` (decommit-eligible), summing to the pool cap. It does NOT avoid
any of (a)'s correctness surface — it still needs the decay-gated decommit
primitive and still hits the §3 chain problem (the cold tier's free lists are
destroyed by decommit). Its only addition is a SECOND knob (`hot_cap`) that
bounds how many segments stay warm regardless of decay. Recommendation:
**reject as a first cut.** It is strictly more config surface than (a) for no
correctness advantage; the "how many stay warm" question is already answered
by (a)'s decay timer (a segment stays warm until it has idled one
`decay_interval`). If field experience shows the decay timer is too coarse a
"hotness" definition, (b) can be layered on top of a shipped (a) as a
follow-up — but it is premature now.

### (c) Memory-pressure-triggered decommit (react to an OS signal)

**What is already available in this codebase:** `crates/proc-memstat` provides
a same-instant OBSERVATION primitive — `snapshot() -> MemStat { rss, commit,
peak_rss }` (`crates/proc-memstat/src/lib.rs:62-88`, Windows
`K32GetProcessMemoryInfo`, Linux `/proc/self/status`, macOS `task_info`). It
is a POLLING/measurement API, NOT an OS memory-pressure event/callback. There
is no pressure-notification wiring (no Linux PSI, no Windows
`CreateMemoryResourceNotification`, no macOS dispatch source) anywhere in the
tree (verified by grep across `crates/` and `src/` — only doc comments mention
"memory pressure"). `crates/proc-probe` merely re-exports `proc_memstat` for
probe binaries.

So option (c) reduces to: poll `proc_memstat::snapshot()` on the decay tick
(already a clock edge) and, when `commit` crosses a configured threshold,
decommit the cold pooled tier. This STILL needs the decay-gated decommit
primitive from (a) and STILL hits the §3 chain problem — (c) is a TRIGGER for
(a)'s mechanism, not an alternative to it. Its advantage over (a)'s pure
time-decay is reactiveness (decommit only under real system pressure, not on a
fixed cadence). Its costs are: (i) a new threshold knob + threshold-cross
hysteresis logic, (ii) coupling allocator internals to a process-wide metric
that the allocator itself influences (feedback loop risk — decommitting
reduces commit, which un-triggers the threshold, which stops decommitting,
which lets commit climb again), and (iii) `proc_memstat` reports honest zeros
on unknown targets and under miri (`proc-memstat/src/lib.rs:248-258`), so the
threshold is silently inert there. Recommendation: **defer.** (c) is a
plausible Phase-2 refinement layered on a shipped (a), but it adds a feedback
loop and a platform-coverage gap that (a)'s deterministic time-decay does not
have. Ship (a) first; revisit (c) only if (a)'s time-decay proves too aggressive
or too lax in the field.

### Recommendation

**(a) in its corrected §5 form** is the recommended low-RSS policy. It is the
minimal mechanism that delivers the review's ask, reuses an existing
primitive, and its only correctness hazard (the `release_follows=false`
re-introduction) is one R9-5 already characterized and that this report
cross-references explicitly (§6). (b) and (c) are documented as future
refinements, not first cuts.

---

## 5. The recommended design — decay-gated B3-style blank carve target

### 5.1 The third state, precisely

A pooled segment transitions through THREE ages, governed by the existing
decay timer (`maybe_decay_small_pool`, called from `reserve_small_segment`'s
cold path, `alloc_core_small_pool.rs:435-442`):

| Age | State | Committed payload | Free list | Reuse path | Reuse cost |
|---|---|---|---|---|---|
| 0 (just admitted) | POOLED-WARM | fully committed | intact | `find_segment_with_free` → `pop_free` | 0 OS syscalls (today's R8-10 win) |
| 1 (idle ≥ 1 `decay_interval`, no reuse) | **POOLED-COLD (NEW)** | **decommitted** (`release_follows=false` reset) | **nulled** (blank carve target) | **`reserve_small_segment` carve-target pop → `carve_block` recommit branch** | 1 recommit syscall (`recommit_pages`) — NO fresh reserve, NO metadata init |
| 2 (idle ≥ 2 `decay_interval`, no reuse) | RELEASED | n/a (slot recycled, reservation released) | n/a | fresh `reserve_small_segment` | full reserve + commit + metadata init (today's path) |

The transition 1→2 replaces today's ONLY transition (0→RELEASED on the first
decay tick). Today a warm segment is released the instant it becomes the
FIFO-oldest idle entry; under the low-RSS policy it is first DECOMMITTED (age
1), buying one extra `decay_interval` of "cheap to revive" before full release.

### 5.2 What "hot" means

A segment is HOT (age 0, stays warm) iff it has been REUSED since its admission
— equivalently, iff it has NOT yet reached its FIFO decay turn. Concretely:
admission pushes to the pool HEAD (warmest, `pool_push_front`,
`alloc_core_small_pool.rs:296`); each decay tick evicts from the TAIL (coldest,
`alloc_core_small_pool.rs:466-477`). So "hot" = "not yet the FIFO-oldest
entry at a decay-tick boundary." This needs NO new per-segment field (no reuse
counter, no last-reuse timestamp): the existing intrusive list ORDER already
encodes recency-of-admission, and the decay timer already encodes elapsed idle
time. This is the same "hotness = list position + clock" definition the large
cache's `maybe_decay_large_cache` already uses
(`alloc_core_large_cache.rs:57`). A segment that is reused is
`unpool_if_present`-removed (`alloc_core_small_pool.rs:412`) and, on re-empty,
re-admitted at the HEAD — so reuse naturally promotes it back to hottest. The
design adds no new hotness bookkeeping.

### 5.3 The reuse path — what `carve_block`'s recommit branch already does, and what is missing

The reuse of an age-1 (POOLED-COLD) segment is via FRESH CARVE, not free-list
pop (the free list is nulled). The recommit machinery ALREADY EXISTS at
`carve_block`'s `is_decommitted()` branch
(`src/alloc_core/alloc_core_small.rs:1070-1110`):

- **Eager path** (`not(feature = "alloc-lazy-commit")`): calls
  `os::recommit_pages(segment, SegLayout::small_decommit_start(), SEGMENT)`
  (`alloc_core_small.rs:1097`); on `false` (commit-charge exhaustion) it does
  NOT clear `decommitted` and returns `None` (reports "segment full" so the
  caller falls back to a fresh segment) — the correct, honest-OOM behaviour
  (`alloc_core_small.rs:1098-1107`). On success it clears `decommitted` and
  proceeds to carve. **This branch already does the right thing for a blank
  carve target.** ✓
- **Lazy-commit path** (`alloc-lazy-commit`): just clears the `decommitted`
  flag (`alloc_core_small.rs:1093`) — because the B3 reset already set
  `committed_payload_end` to `initial_frontier` (the initial lazy chunk stays
  committed), so the first chunk is carveable with no syscall, and B2's
  grow-on-carve (`alloc_core_small.rs:1122-1144`) recommits incrementally as
  the bump advances. ✓

What the recommit branch does NOT have to change: it already handles both
commit features and the OOM case. **The MISSING piece is upstream**, in
`reserve_small_segment`: today a pooled segment is NEVER surfaced as a carve
target (the pool is a free-list reserve, not a carve reserve —
`alloc_core_small_pool.rs:185-188`). For age-1 segments to be reused,
`reserve_small_segment` must, before doing a fresh OS reserve, consult the pool
for a DECOMMITTED entry and pop it as `small_cur` (so the next `carve_block`
hits the recommit branch). That consult-and-pop is the wiring that does not
exist today and is the bulk of the implementation surface (§9). It must:

1. Distinguish age-0 (warm, free-list-intact — must NOT be popped as a carve
   target, only reused via `find_segment_with_free`) from age-1 (cold, blank —
   the carve target). This needs a way to tell them apart. The natural marker
   is the existing `decommitted` flag (`segment_header.rs:367-372`): age-1 has
   `decommitted == 1` and nulled free lists; age-0 has `decommitted == 0`.
   `reserve_small_segment`'s pool scan pops the first entry with
   `decommitted == 1`. (Age-0 entries have `decommitted == 0` and are skipped
   by the carve-target scan — they remain free-list reuse candidates.)
2. On a successful recommit-in-`carve_block`, the segment is already
   `unpool_if_present`-removed at pop time (so it is not double-counted).
3. Handle recommit OOM: if `carve_block`'s recommit returns `false`, the
   segment stays `decommitted == 1` and `carve_block` returns `None`;
   `reserve_small_segment` then falls back to a fresh OS reserve (or the next
   age-1 entry). The failed-recommit age-1 segment stays pooled-cold and is
   fully released on the next decay tick. (Open question: whether to
   immediately release a failed-recommit segment rather than leaving it
   pooled-cold — see §11 Stage 1.)

### 5.4 The opt-in surface — new Cargo feature `low-rss-pool`

This codebase's established convention for behavioural/semantic changes to the
pool is a **Cargo feature**, not a runtime preset enum: R7
(`docs/perf/R7_PLAN.md:95`) deliberately chose builder-method config
(`pool_segments`/`pool_byte_cap`) over preset enums, and R9-4
(`Cargo.toml:285-316`) gates a behavioural change behind an additive,
non-`production` feature (`medium-classes-wide = ["medium-classes"]`). The
low-RSS policy changes COMMIT SEMANTICS (it introduces commit/decommit
syscalls on the decay path that do not exist today) — that is a semantic
change, not a knob value, so a feature flag is the right shape, matching
`medium-classes-wide`'s convention:

```text
# Cargo.toml (SKETCH — NOT applied this session)
low-rss-pool = ["alloc-decommit"]
```

Gated `#[cfg(feature = "low-rss-pool")]`, the policy:

- Changes `maybe_decay_small_pool`'s eviction (age 0→1 decommit-and-retain on
  the FIRST tick; age 1→2 full release on the SECOND tick) — behind the flag,
  the default (flag OFF) keeps today's 0→RELEASED behaviour byte-identical.
- Adds the `reserve_small_segment` carve-target consult-and-pop for age-1
  entries — behind the flag, flag-OFF builds never consult the pool for carve
  targets (today's behaviour).
- Is additive over `alloc-decommit` (the pool does not exist without it) and
  NOT part of `production` (`Cargo.toml:209`) or any default bundle — exactly
  like `medium-classes-wide`. `--all-features` pulls it in (as every opt-in
  feature), which is intended for the test matrix.

A `SmallSegmentPoolConfig::low_rss()` preset FN is NOT recommended: R7
explicitly rejected preset enums in favour of builder methods, and the policy's
behaviour is already fully gated by the feature flag. A deployment that wants
low-RSS enables `low-rss-pool` AND tunes the existing knobs
(`pool_segments` larger for reuse breadth, `decay_interval` shorter for faster
cold-down) per §10's recipe — no new config type needed.

---

## 6. Cross-reference to R9-5 risk area 3 (the mandatory coupling)

R9-5 §4.3 (`docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:238-285`) established
that `decommit_empty_segment_impl(_, _, release_follows=false)` — the EXACT
primitive §5's age-0→1 transition re-introduces to production — currently has
ZERO production callers, and that this fact is load-bearing for macOS
correctness of the (unshipped) virgin-zero-skip optimization. R9-5 §12
(`R9_5…DESIGN.md:608-619`) states the rule verbatim:

> "Any future change that re-introduces an in-place decommit of a registered
> small segment (e.g. re-enabling a B3-style decommit-on-pool-admission, or a
> new decommit policy) MUST also set `payload_virgin=false`, or the skip
> becomes unsound on macOS."

**This task IS that "new decommit policy."** Three points, made explicit per
the task's requirement:

1. **The re-introduction is acknowledged, not silent.** §5's age-0→1
   transition is the first production caller of
   `decommit_empty_segment_impl(_, _, release_follows=false)` since R8-10
   removed the B3 admission path. R9-5 characterized this primitive's
   macOS-correctness profile ("closed by R8-10, but fragile to
   reintroduction", `R9_5…DESIGN.md:43-44, 282-285`); this report re-opens it
   knowingly and ties the closure to the §6.2 coupling below.

2. **The R9-5 `payload_virgin=false` reset is mandatory IF R9-5 ever ships.**
   R9-5's design table (`R9_5…DESIGN.md:169-176`) specifies that
   `decommit_empty_segment_impl(release_follows=false)` must set
   `payload_virgin=false`, so that a later carve from the recommitted segment
   is NOT treated as OS-zero-guaranteed. Under §5's design, an age-1 segment
   is reused via FRESH CARVE (`carve_block` after recommit), NOT via `pop_free`
   — so the carve's byte range `[bump, bump + block_size)` is exactly the
   "recommit-then-carve" path R9-5 §4.3(b) analyzed, and on macOS those bytes
   are NOT zero-guaranteed. TODAY this is harmless (`HeapCore::alloc_zeroed`'s
   small arm applies an UNCONDITIONAL `Node::zero`, R9-5 §7 — so a recommitted
   carve is zeroed regardless of OS state). The hazard appears ONLY IF R9-5's
   virgin-zero-skip is implemented: then a carve might skip `Node::zero` based
   on `payload_virgin`, which MUST be false for an age-1 segment. **Whichever
   of R9-5 or R9-7 lands second MUST carry this coupling.** If R9-7 ships
   first, §5's `release_follows=false` call site is the exact location R9-5's
   `payload_virgin=false` reset must be added (it is the `decommitted == 1`
   branch of the age-0→1 transition). If R9-5 ships first, R9-5's defensive
   `payload_virgin=false` in `decommit_empty_segment_impl(release_follows=false)`
   already covers R9-7's call site (R9-5 §3 reset table, row 6). The coupling
   is one line either way; it is documented here so it is not missed.

3. **The task's "pop_free is never zero-guaranteed" reasoning holds — but it
   governs the REJECTED shape, not the recommended one.** The task brief
   argues the third state is macOS-safe because "a pooled segment's free-list
   blocks are consumed via `pop_free`, which is NEVER treated as zero-
   guaranteed." That reasoning is correct AS FAR AS IT GOES: a `pop_free`-
   served block is reported non-virgin by R9-5's dispatch test
   (`R9_5…DESIGN.md:124-136`) and `alloc_zeroed` zeroes it unconditionally
   today. BUT §3 of THIS report proves the review's "keep the free list
   intact" shape (which is the shape that would reuse via `pop_free`) is
   unsound for an INDEPENDENT reason — the `next` chain is destroyed, not the
   zero guarantee. The recommended §5 shape reuses via FRESH CARVE, for which
   the governing correctness rule is the R9-5 `payload_virgin` coupling in (2)
   above, NOT the `pop_free` dispatch argument. Both arguments are needed:
   §3's chain argument rules out the review's shape; R9-5's `payload_virgin`
   argument governs the carve-reuse shape that replaces it. State both.

**Net:** the design is macOS-correct TODAY (unconditional `Node::zero` covers
it) and is rendered macOS-correct-under-R9-5 by the one-line `payload_virgin`
coupling documented above. No new macOS hazard is created by THIS design in
isolation.

---

## 7. RSS ceiling reduction (analytical)

The low-RSS policy's value is NOT primarily a lower idle-drain rate (today's
decay already releases one segment per `decay_interval`) — it is that it
**decouples the pool's REUSE BREADTH from its COMMITTED RSS**, letting a
deployment keep many segments available for cheap recommit-reuse while
committing only the hot few.

**Today (latency-first, `pool_segments = 4`, all warm):**

- Committed pool payload ceiling: `4 × 4 MiB = 16 MiB` per heap, hard
  (synchronous admission cap, §2).
- Reuse breadth: 4 segments. A 5th simultaneous churn class pays a full fresh
  reserve.

**Low-RSS (e.g. `pool_segments = 8`, `low-rss-pool` ON, hot tier effectively
`decay_interval`-recent):**

- Committed pool payload ceiling: only the age-0 (warm) segments — those
  admitted/reused within the last `decay_interval`. Under sustained churn this
  is bounded by the churn breadth (the distinct size classes actively
  oscillating); under idle it decays to ZERO (every segment transitions to
  age-1 decommitted after one `decay_interval`, then released after two).
  Steady-idle committed pool RSS: ≈ metadata-only per retained segment
  (tens of KiB), vs today's up-to-16 MiB held until the FIFO release turn.
- Reuse breadth: 8 segments, but 4+ of them are age-1 (decommitted, cheap
  recommit-reuse). A reuse hitting an age-1 segment pays one `recommit_pages`
  syscall (Windows `VirtualAlloc(MEM_COMMIT)` on ~4 MiB; Unix no-op implicit)
  — NOT a full `mmap`/reserve + metadata init.

**The ceiling reduction, stated as a bound:** with `low-rss-pool` ON, the
committed pool payload at any instant is bounded by the number of age-0
segments × 4 MiB. Because age-0 lasts exactly one `decay_interval` (default
1 s) after last admission/reuse, the age-0 count tracks the
short-term-reuse working set, NOT the configured `pool_segments`. A deployment
can therefore RAISE `pool_segments` (more reuse breadth → fewer fresh reserves
under bursty churn) WITHOUT raising the committed-RSS ceiling proportionally —
the opposite of today, where raising `pool_segments` from 4 to 8 directly
raises the ceiling from 16 to 32 MiB. **This decoupling is the review's ask,
answered.**

(These are analytical bounds from the admission/decay rules, not measured
figures — mirroring R9-5 §8's methodology. A measurement gate is §11 Stage 0.)

---

## 8. Latency cost re-added, and why decay-gating avoids the R8-10 50-75× blowup

**The latency cost.** Reusing an age-1 (decommitted) segment pays one
`recommit_pages` syscall that an age-0 (warm) segment does not. On Windows
that is one `VirtualAlloc(MEM_COMMIT)` over ~4 MiB (a page-table update, low
µs range, no byte copy); on Unix `recommit` is a no-op (re-access is implicit,
`vmem/src/lib.rs:481-484`) so the cost is the first-touch fault-in of the
carved block's pages, paid lazily as the caller writes. This is strictly
CHEAPER than reusing a RELEASED segment, which pays a full
`mmap`/`VirtualAlloc(MEM_RESERVE|MEM_COMMIT)` + the full metadata init
(`reserve_small_segment`'s 49 BinTable heads, page-map, alloc-bitmap,
magazine-bitmap — the work `decommit_empty_segment_impl`'s full reset elides
on the release-follows path, `alloc_core_small_pool.rs:599-622`).

**Why decay-gating avoids the 50-75× blowup that killed admission-gated B3.**
R8-10 (commit `852828e`) found the OLD B3 path — which decommitted the payload
ON ADMISSION (`release_or_pool_empty_segment`'s admission arm) — cost 50-75×
more `commit_range`/decommit syscalls per empty→pool→reuse→refill cycle than
the eager path, because EVERY admitted segment was decommitted, so EVERY
first-reuse paid a recommit (`alloc_core_small_pool.rs:190-203`). The
amplification was per-ADMISSION (per churn cycle), and a churn workload admits
segments at the allocation rate.

§5's decay-gated decommit fires on a fundamentally different schedule:

- It fires on the DECAY TICK, not on admission. The decay tick runs at most
  once per `decay_interval` (default 1 s), only when the pool is non-empty
  AND a `reserve_small_segment` cold path is hit (`maybe_decay_small_pool`'s
  trigger, `alloc_core_small_pool.rs:435-442`), and evicts at most ONE segment
  per tick (`alloc_core_small_pool.rs:466-477`). So the decommit rate is
  bounded by `min(pool_cap, churn-breadth)` per second — NOT by the allocation
  rate (which can be millions/sec).
- For a HOT churn workload (reuse interval < `decay_interval`), NO segment
  ever reaches age 1: every pooled segment is reused (promoted back to HEAD)
  before its decay turn. So zero decommits fire, zero recommits fire, and the
  latency cost is ZERO — the policy is latency-neutral for the workload R8-10
  optimized for. The recommit cost is paid ONLY by genuinely cold segments
  (idle ≥ `decay_interval`) that are later revived — exactly the
  latency-for-RSS trade the review asked to make available.

**Quantified contrast:** if a workload admits+reuses N segments per second
under B3, B3 issued N decommits + N recommits per second (the 50-75× figure
measured against an eager baseline of ~0 syscalls/reuse). Under §5's
decay-gating, the same workload issues 0 decommits/recommits per second while
hot (reuse interval < 1 s), and at most `pool_cap` decommits per second while
draining idle. The blowup is avoided because the decommit is gated on IDLE
TIME (a slow clock), not on ADMISSION (a fast allocation edge).

---

## 9. Why no prototype this session (the correctness-surface audit)

Per the task's "only if clearly minimal and safe" bar, this section audits the
implementation surface and concludes it exceeds the bar. The prototype would
touch, at minimum:

1. **`maybe_decay_small_pool` (`alloc_core_small_pool.rs:445-486`)** — split
   the single eviction into the age-0→1 (decommit-and-retain) and age-1→2
   (release) transitions. This re-introduces
   `decommit_empty_segment_impl(_, _, release_follows=false)` as a production
   caller (R9-5 §4.3, cross-referenced in §6). The full-reset body
   (`alloc_core_small_pool.rs:645-729`) already nulls free lists / rewinds
   bump / re-inits bitmaps, so the reset itself is reusable — but wiring the
   retain-vs-release branch needs a per-pooled-entry age marker. The existing
   `decommitted` flag (`segment_header.rs:367-372`) is the natural marker
   (age-1 ⇒ `decommitted == 1`), but using it this way couples the flag's
   meaning to pool age, which must be audited against every other `is_decommitted()`
   read site (`alloc_core_small.rs:1071, 1229`; `pop_free`'s invariant comment
   `alloc_core_small.rs:885-892`; `dec_live_and_maybe_decommit`'s idempotency
   guard `alloc_core_small_pool.rs:85`).
2. **`reserve_small_segment` (`alloc_core_small.rs`, the cold path)** — add
   the age-1 carve-target consult-and-pop BEFORE the fresh OS reserve (§5.3).
   This is the bulk of the new logic and has no precedent: the pool has NEVER
   been a carve reserve. It must correctly skip age-0 entries (warm,
   free-list-intact — these stay free-list reuse candidates), pop the first
   age-1 entry as `small_cur`, and handle the recommit-OOM fall-through.
3. **The directory sidecar (`alloc-segment-directory`)** — an age-1 segment
   has nulled free lists, so its directory bits must be cleared at the age-0→1
   transition (else `find_segment_with_free`'s directory-driven lookup
   (`alloc_core_small.rs:379-504`) returns a stale positive for a segment
   whose free list is now empty — caught by validation step 3
   (`alloc_core_small.rs:479-491`), but it churns `DIRECTORY_STALE_HITS` and
   wastes a drain). Wiring this correctly across the directory's incremental-
   sync invariants (R8-1, task #214) is a non-trivial interaction.
4. **`drain_small_pool` (`alloc_core_small_pool.rs:565-579`)** and
   `alloc_large_slow`'s OS-failure fallback — both pop pooled segments for
   full release; under `low-rss-pool` they must handle age-1 entries (already
   decommitted; the release path must not double-decommit). The
   `release_follows=true` fast path (`alloc_core_small_pool.rs:637-644`)
   already sets `decommitted=true` unconditionally, so a segment already at
   `decommitted==1` is a benign no-op — but this must be verified, not
   assumed.

This is 4 distinct touch-points, three of them (2, 3, 4) with no existing
precedent for the "pool as carve reserve" model, and one of them (1)
re-introducing a primitive R9-5 explicitly characterized as
"fragile-to-reintroduction" with a mandatory coupling to an unshipped
optimization. The mandatory test plan (decommitted-pooled-segment recommit
serves correct non-corrupted blocks; low-RSS preset reduces measured commit
charge vs default on an idle-then-reuse workload; default behaviour UNCHANGED
with the flag OFF) is achievable, but the correctness REVIEW budget the
orchestrator flagged as especially high for decommit/recommit changes is not
satisfiable inside this session's "minimal and safe" envelope — the surface is
genuinely the same class R9-5 declined to rush. **Design-only, mirroring
R9-5's K8 verdict.**

---

## 10. Safe stopgap — knob-tuning recipe (shippable NOW, zero new decommit surface)

A deployment that needs lower committed-RSS TODAY, before §5 lands, can get a
coarser but real reduction using ONLY existing knobs — no new feature, no new
decommit primitive, no correctness surface:

```text
use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

const LOW_RSS_STOPGAP: LargeCacheConfig = LargeCacheConfig::new()
    .pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(1)                   // was 4 → committed ceiling 4 MiB, not 16
            .pool_byte_cap(4 * 1024 * 1024)     // matched byte cap
    )
    .decay_interval_ms(250);                    // was 1000 → idle drains 4× faster
```

This does NOT create the third state (it shrinks the WARM pool and speeds
release, rather than decommitting cold segments while keeping them registered),
so it trades latency more aggressively than §5 (a 2nd simultaneous churn class
pays a full fresh reserve, since only 1 segment stays warm). But it is
byte-identical to today's mechanism — just different knob values — and needs
no review beyond R7's already-shipped config contract. It is the recommended
INTERIM answer to the review's ask; §5 is the complete answer for a future
task.

---

## 11. Staged implementation recommendation (for a future task)

**Stage 0 (measurement gate, ~1h, no `src/` change):** add a criterion bench
(criterion fast profile, mirroring `benches/perf_gate_iai.rs`'s shape) +
a `first_alloc_process`-style commit/RSS probe (using `proc_memstat::snapshot`)
that runs an idle-then-reuse workload under (i) today's default
(`pool_segments=4`), (ii) the §10 stopgap (`pool_segments=1, decay 250 ms`),
and (iii) a hand-instrumented §5 prototype. **Exit criterion:** (iii) shows a
measurable committed-RSS reduction vs (i) on the idle phase AND a recommit
cost on the reuse phase that is materially below a fresh-reserve cost. If §5's
recommit cost is indistinguishable from a fresh reserve (i.e. the third state
buys nothing over release), **re-evaluate whether to proceed** — the policy's
premise is that recommit is cheaper than reserve.

**Stage 1 (mechanism behind `low-rss-pool`, ~3-4h):** the minimal change.
Gate the age-0→1 decommit-and-retain in `maybe_decay_small_pool` behind
`#[cfg(feature = "low-rss-pool")]`, using `decommitted==1` as the age marker;
add the `reserve_small_segment` age-1 carve-target consult-and-pop; clear
directory bits at the age-0→1 transition; verify `drain_small_pool` /
`alloc_large_slow` handle age-1 entries. Feature-OFF builds must be
byte-identical to today (regression guard mirroring R9-4's
topology-non-disturbance pattern — every new code path behind a `#[cfg]` whose
predicate includes `low-rss-pool`).

**Stage 2 (mandatory test plan, ~2h):**
- (a) `decommitted_pooled_segment_recommits_and_serves_correct_blocks` — drive
  a segment empty→pool→decay-to-age-1→reuse; assert the reused block reads
  back exactly what was written (no corruption from the §3 chain problem —
  this is the counterfactual that would fail if the design kept the free list
  instead of resetting).
- (b) `low_rss_preset_reduces_commit_charge_on_idle` — using
  `proc_memstat::snapshot().commit`, run an idle-then-reuse workload under
  `low-rss-pool` ON vs OFF; assert ON shows strictly lower committed bytes
  during the idle phase (the review's quantified ask).
- (c) `default_pool_unchanged_without_flag` — the regression guard: with
  `low-rss-pool` OFF, `dbg_decommit_count` / `dbg_pooled_count` /
  `dbg_pool_cap` and the `small_segment_pool` + `regression_c3_unbounded_recycle`
  suites are byte-identical to today (R9-4 non-disturbance pattern).
- (d) miri-gated: an age-1 segment under `cfg!(miri)` (where `recommit` is a
  no-op and pages stay accessible, `vmem` miri aperture) still serves correct
  blocks — miri cannot catch the macOS `next`-garbage case, but it CAN catch
  any pointer-arith UB in the new carve-target-pop wiring.

**Stage 3 (promotion gate, ~1h):** re-run Stage 0's bench with Stage 1 live;
confirm the measured RSS reduction and recommit cost match §7-§8's analytical
bounds. Only on green Stage 3 consider whether `low-rss-pool` merits inclusion
in a `production-low-rss` profile bundle — and even then, run under miri
(`region_invariants` + a bounded proptest on the age-0→1→2 transitions) and
the differential gate first, given the R9-5-class correctness profile.

---

## 12. Caveats

- **Single analysis host, no measurement performed.** §7-§8 are analytical
  bounds from the admission/decay rules and the syscall semantics, not measured
  figures (mirroring R9-5 §8). Stage 0 (§11) is the gate that turns them into
  measured numbers.
- **The §3 chain-destruction finding is this report's load-bearing
  contribution.** It is verified by reading (`node.rs:74-108`,
  `alloc_core_small.rs:836, 1396`, `pop_free`'s invariant comment
  `alloc_core_small.rs:885-892`), not by a failing test — there is no test that
  exercises a decommitted-but-free-list-non-empty segment, because no
  production path produces one today. Stage 2 test (a) is the
  counterfactual that would FAIL if a future implementation accidentally kept
  the free list; it is specified to guard exactly the §3 invariant.
- **The R9-5 `payload_virgin` coupling (§6) is documented, not implemented.**
  R9-5 has no code yet; this design has no code yet. Whichever lands second
  carries the one-line reset. This is noted for the orchestrator's
  cross-task tracking, not as a blocker on either.
- **No `src/`, `Cargo.toml`, or `tests/` file is modified.** This is a
  documentation-only deliverable. The §5 feature sketch (`low-rss-pool =
  ["alloc-decommit"]`) and the §10 knob recipe are illustrative, not applied.
- **The prior R8-10 / R9-5 history is a feature, not a bug.** R8-10 (the
  substrate that made the warm pool load-bearing) and R9-5 (the
  characterization of the `release_follows=false` primitive this design
  re-introduces) are both cited and built on here; this report does not pretend
  either didn't happen. The dating (R9-5 landed the same day as this task,
  both on 2026-07-20) is why the §6 cross-reference is mandatory rather than
  optional — the primitive's "fragile-to-reintroduction" caveat is fresh.
