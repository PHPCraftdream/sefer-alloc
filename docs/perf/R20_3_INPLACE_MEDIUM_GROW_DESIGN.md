# R20-3 — In-place medium-class grow within a segment: design (NOT implementation)

**Task:** R20-3 (task #348, P1). **DESIGN-ONLY.** No `src/` change, no
`Cargo.toml` change, no `tests/` change, no benchmark run. This document
proposes a mechanism, its data-structure/call-site sketch, and a measurement
plan for a FUTURE round; it implements and benchmarks nothing itself.

**Date:** 2026-07-26. **Base revision:** `main` @ `ee5f2aa` (R20-2, task
#347; the immediately preceding commits this round are R19-1..R19-9,
R20-1, R20-2, none of which touch the files this design reads).

**Where this task comes from:** `docs/perf/OPEN_ITEMS.md` Active item 1 —
*"R10-2 §5 #1 — in-place medium-class grow within a segment... NOT designed,
NOT implemented"* — reaffirmed as the one lever no existing-feature
coordination addresses by three independent measurement rounds:
`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md` §5 (named the lever, did not
design it), `docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` (R18-2's
re-run: ruled out cache-slot pressure and the R17-4 leak as explanations),
and `docs/perf/R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` (ruled out
`large-reserved-capacity` headroom — the copy happens before any headroom on
the *destination* Large segment could apply). This is the first document to
propose an actual mechanism rather than name the gap.

---

## 0. Recap of the diagnosed problem (not re-measured here)

`docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md` measured a growing `realloc`
of a medium-classified block (256 KiB–1 MiB, `medium-classes` feature)
crossing `MEDIUM_REALLOC_PROMOTION_THRESHOLD` (256 KiB,
`src/registry/heap_core_free.rs:157`) at **~2,111× slower** wall-clock than
the baseline Large path (79 ms vs 0.037 ms over 960 ops); R18-2's re-run
after fixing an unrelated segment leak still showed **~1,180×/~380×**
depending on feature combination; R20-2 showed `large-reserved-capacity`
headroom on the destination Large segment moves this by ~0–2%, statistically
unresolvable (t=1.209, sign 10/20 dead-even).

The mechanism (`try_promote_to_large`, `src/registry/heap_core_free.rs:1211`)
is: on the first grow past the threshold, allocate a **fresh** Large
segment via `AllocCore::alloc_large`, `copy_nonoverlapping` the full old
payload into it, then free the old medium block. Every *subsequent* grow of
the same (now-Large) block rides OPT-G (`try_realloc_inplace_known_base`,
`src/alloc_core/alloc_core.rs:1901`) for near-zero cost — so the expensive
step is specifically **the first crossing**, and specifically **the copy
that moves the payload out of the medium segment**, not anything about the
destination.

R10-2 §5 named the only remaining lever precisely: *"if a realloc-grow
target class has free space in the SAME segment the block already lives in,
the move-leg could be avoided (carve the new slot in-place, copy within the
segment)."* This document investigates whether, and how, that is actually
buildable against the real carve/`BinTable` substrate — and finds the honest
answer is narrower than the one-line summary suggests: a real, safe,
minimal-footprint mechanism exists, but its applicability is structurally
bounded to one specific geometric case, not "any block with room in its
segment."

---

## 1. Grounding: how carving and freeing a medium block actually work today

### 1.1 First claim to verify: "a segment carves fixed-size blocks of ONE class" — FALSE

Read against `src/alloc_core/segment_header.rs`'s `PageMap` doc
(lines 177–191) and `src/alloc_core/alloc_core_small.rs`'s `carve_block`
(lines 1419–1557): a small/medium segment does **not** dedicate itself to one
size class. It carves from **one segment-wide bump cursor** (`SegmentHeader
::bump`, a single `usize` field, `SegmentMeta::bump_of`/`set_bump`,
`segment_header.rs:1138/1149`) shared across **every** class that ever gets
carved from that segment. `carve_block(class_idx, block_size)` does exactly:

```text
let bump = meta.bump_of();
let aligned_bump = align_up(bump, block_size);      // round UP to a block_size multiple
if aligned_bump + block_size > SEGMENT { return None; }
... (lazy-commit grow-on-carve, if that feature is on) ...
meta.set_bump(aligned_bump + block_size);
... (page-map "first class wins" marking, diagnostic-only) ...
Node::deref(segment, aligned_bump)
```

So a 256 KiB block and a 64-byte block can — and routinely do — sit
back-to-back in the same segment, in carve order, sharing one advancing
cursor. `PageMap`'s own doc states this explicitly: *"under this substrate's
shared-bump-cursor model a page is mixed-class... `PageMap` is therefore NOT
a reliable class oracle — no production `dealloc` path derives a block's
class from it"* (`segment_header.rs:181–191`). This is a real, already-shipped
design decision (it is what "mixed-class" means in this codebase), not
something this design has to invent room for.

**Consequence for this design:** the substrate already tolerates
different-class blocks coexisting in one segment. The question is not "can a
bigger block fit alongside others in the segment" (yes, trivially, via the
bump cursor) but "can THIS SPECIFIC already-carved block become bigger
without moving" — and that is a much narrower question, answered in §1.3.

### 1.2 Second fact: freeing is class-keyed by offset, not by any stored per-block tag

`BinTable` (`segment_header.rs:999–1011`) is one `u32` free-list-head OFFSET
per class — `SMALL_CLASS_COUNT` entries, 160 B at the default 40-class table.
`dealloc_small` (`alloc_core_small.rs:1735`) receives `class_idx` (derived by
the caller from the **freed** `Layout`'s size, via `SizeClasses::class_for`)
and pushes the freed block's offset onto `BinTable`'s free list for that
class (`bt.set_head(class_idx, off)`, line 1812). There is **no per-block
stored class tag anywhere** — a block's class, at both alloc and dealloc
time, is derived purely from `(size, align)` via `class_for`, applied to
whatever `Layout` the caller currently supplies. This is exactly the
mechanism `OPT-F`'s own doc comment (`alloc_core.rs:1734–1771`) explains at
length for the shrink case:

> "A block is carved at an offset that is a multiple of ITS class's
> `block_size`; that offset is NOT necessarily a multiple of a *smaller*
> class's `block_size`... if we returned `ptr` unchanged for a shrink that
> crosses into a smaller class... the eventual `dealloc` would push this
> block's offset onto the SMALLER class's free list, where the offset is
> misaligned — corrupting that free list so a later `alloc` from it returns a
> mis-placed pointer."

This is the load-bearing invariant any in-place-grow mechanism must respect,
generalized: **a block may only be freed into class C's free list if its
offset is an exact multiple of `block_size(C)`** (`carve_block`'s own
`align_up(bump, block_size)` guarantees this for every ordinarily-carved
block; nothing else in the substrate checks it at dealloc time — it is an
invariant maintained by construction at carve time, never re-verified).

### 1.3 The only genuinely free lunch: bump-tail adjacency

`carve_block` never leaves a gap: after carving a block at `aligned_bump`,
`bump` becomes exactly `aligned_bump + block_size` — the carved block's
**own end** is always the new bump cursor. So for exactly one block per
segment at any given moment — whichever was carved **most recently and has
not yet been grown or freed** — the bytes immediately following it,
`[bump, SEGMENT)`, are guaranteed uncarved (no other live or freed block
occupies them; nothing has claimed them yet). This is the segment-local
analogue of OPT-G's Large-grow-in-span check (`alloc_core.rs:1693–1732`),
which asks "does the grown size still fit the segment's committed span" —
here the equivalent question is "does the grown size still fit
`[old_offset, SEGMENT)` with nothing already carved in between," which is
true **if and only if** the block is currently the segment's bump tail.

For every block that is **not** the current tail (the overwhelmingly common
case once more than one object has been carved into a segment), the bytes
after it are already occupied by another live or freed block — extending
into them would silently corrupt that other block. There is no way around
this without either (a) reserving slack at carve time (R10-2 §5 item 2's
separate, already-named "over-allocation" lever — see §5.4 below for why
that is a different trade-off, not this mechanism), or (b) a wholly different
segment layout (one-object-per-segment above some size, i.e. re-inventing
the Large path for medium sizes — which is just Large with extra steps, and
this codebase already has a real Large path). This design does not chase
either of those; it proposes the mechanism that is genuinely free (no
copy, no extra reserved memory) for the case where it applies.

---

## 2. Proposal: OPT-H — tail-of-segment cross-class in-place grow

Named to sit alongside the existing OPT-F (Small same-class in-place,
`alloc_core.rs:1734`) and OPT-G (Large grow-in-span, `alloc_core.rs:1693`) —
this is the "third in-place-grow mechanism" the task brief anticipated, for a
Small/medium-classified block growing to a **larger** Small/medium class
within the segment it already occupies.

### 2.1 Preconditions (all must hold — mirrors OPT-G's precondition list shape)

Given `base` (segment), `ptr` (the block), `old_class`/`new_class` (both
`Some`, i.e. neither classifies Large), with `off = ptr - base`:

1. **Growing, cross-class.** `new_class != old_class` and
   `block_size(new_class) > block_size(old_class)` (same-class growth is
   already OPT-F's job; OPT-H is specifically the case OPT-F declines).
2. **Segment kind is `Small` or `Primordial`** (has a bump cursor / BinTable
   — mirrors OPT-F's kind check).
3. **Tail-adjacency:** `off + block_size(old_class) == meta.bump_of()` — this
   block is the most-recently-carved, not-yet-grown-or-freed block in its
   segment. (§1.3.)
4. **New-class alignment:** `off % block_size(new_class) == 0` — the offset
   is a **legal carve position** for the new class, i.e. indistinguishable
   from an ordinarily-carved `new_class` block to every other subsystem that
   ever reads this offset again (BinTable free-list reuse on a later dealloc;
   the `hardened` generation table, which is a flat per-`MIN_BLOCK`-granule
   byte array indexed by raw offset, `segment_header.rs:123-142`, and is
   therefore automatically indifferent to which class "owns" a granule — no
   extra check needed there). This is the general rule OPT-F's own doc
   already established (§1.2); OPT-H must check it explicitly because,
   unlike OPT-F, it is not implied by "same class."
5. **Segment capacity:** `off + block_size(new_class) <= SEGMENT`.
6. **Lazy-commit grow-on-carve, if applicable.** Under
   `primordial-lazy-commit`/`small-segment-lazy-commit`, the newly-claimed
   tail bytes may not yet be OS-committed. OPT-H must run the identical
   frontier-check-and-commit step `carve_block` already performs
   (`alloc_core_small.rs:1492-1533`: if `carve_end > committed_payload_end`,
   `os::commit_pages` up to `align_up(carve_end, GROW_CHUNK)`, only advancing
   the bump/frontier on success) — reusing that logic rather than
   hand-duplicating it (§4).

When all six hold: advance the segment's `bump` to `off +
block_size(new_class)` and return the **same** `ptr` — no alloc, no copy, no
dealloc, `live_count`/alloc-bitmap untouched (the block was never freed, so
nothing there needs to change — same "block stays live" argument OPT-F's doc
gives). When any precondition fails, decline (return `None`/fall through) —
**exactly** the existing fallback chain (§2.3), unchanged.

### 2.2 Why no new `SegmentHeader` field or `BinTable` variant is needed

This is the answer to the task brief's §4 question, and it is more minimal
than a first read of "in-place medium grow" suggests: **every input OPT-H's
preconditions need already exists** — `bump` (existing field), `off` (a
pointer subtraction), `block_size(old_class)`/`block_size(new_class)`
(existing `SizeClasses::block_size`, already used throughout the realloc
path). No per-block "what class is this really" tag needs to be invented,
because OPT-H's precondition 4 (`off % block_size(new_class) == 0`) is
*exactly* the condition under which the block, after the grow, is
**indistinguishable from an ordinarily-carved `new_class` block** — the
substrate's existing dealloc/realloc paths, which derive class purely from
the caller's current `Layout` (§1.2), continue to work completely unchanged
on it. There is nothing new to remember about this block; it simply
retroactively "was" a `new_class` block all along, in every sense the rest of
the system checks.

The one genuinely new piece of logic is the grow-in-place **operation**
itself (§4), not a new stored fact.

### 2.3 Call-site sketch — where OPT-H fits in the existing fallback chain

`AllocCore::realloc_inplace_fast_path_known_base`
(`alloc_core.rs:1778-1835`) currently tries OPT-G then OPT-F and returns
`None` on both failing. OPT-H slots in as a third attempt, still inside this
same function (so both `AllocCore::realloc`'s and
`HeapCore::realloc`'s/`try_realloc_inplace_known_base`'s callers get it for
free, exactly as OPT-F/OPT-G already are shared this way):

```text
fn realloc_inplace_fast_path_known_base(&mut self, base, ptr, old_layout, new_size) -> Option<*mut u8> {
    // OPT-G: Large→Large in-place grow. [unchanged]
    // OPT-F: Small→Small same-class in-place. [unchanged; returns Some(ptr)
    //   before ever reaching OPT-H if new_class == old_class]
    // OPT-H (NEW): Small/medium cross-class tail-of-segment in-place grow.
    if matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
        if let (Some(old_class), Some(new_class)) = (class_for(old_size, align), class_for(new_size, align)) {
            if new_class != old_class && block_size(new_class) > block_size(old_class)
                && self.try_grow_tail_in_place(base, ptr, old_class, new_class)
            {
                return Some(ptr);
            }
        }
    }
    None
}
```

In `src/registry/heap_core_free.rs`'s `realloc`, this widens step (2)'s
existing comment ("In-place attempt — try OPT-F (Small same-class) and OPT-G
(Large grow-in-span)") to include OPT-H, **before** step (2.5)'s
`try_promote_to_large` call and before step (3)'s move leg — i.e. OPT-H gets
first refusal on exactly the case `try_promote_to_large` currently always
pays the copy for. No change to the call site's control flow shape (still
"try in-place, then promotion, then move-leg" — OPT-H just makes the
in-place leg's precondition set wider), and — critically — **no change to
`try_promote_to_large` itself**: when OPT-H declines (the common case — not
tail-adjacent, or fails the alignment check), the existing promotion path is
completely unmodified and still fires exactly as it does today. OPT-H is a
strict *addition* in front of the existing chain, not a replacement for any
part of it (per the task brief's explicit non-goal: promotion must remain
intact as the fallback for cases OPT-H cannot serve).

---

## 3. Interaction with existing medium-classes machinery

- **R14-4's promotion mechanism (`try_promote_to_large`):** unaffected, still
  the fallback for every non-tail-adjacent grow (§2.3). OPT-H and promotion
  are mutually exclusive per attempt (OPT-H returns `Some` or falls through
  to the *existing* promotion check, never both).
- **The size-class ladder walk (grows below the promotion threshold, or under
  the R15-3 exclusion where promotion is compiled out):** OPT-H is gated only
  on "cross-class Small/medium grow with room," **not** on
  `MEDIUM_REALLOC_PROMOTION_THRESHOLD` — it can also intercept an ordinary
  sub-threshold class-to-class grow that would otherwise take the plain move
  leg (alloc-smaller-step + copy + dealloc). This is a genuine, if
  incidental, secondary benefit beyond the promotion-crossing case R10-2
  specifically measured — noted here as a side-effect the next round's
  measurement plan (§6) should also observe, not as this design's primary
  claim.
- **R15-3's `exact-span-large` zero-headroom exclusion:** does **not** apply
  to OPT-H. That exclusion is specifically about `try_promote_to_large`
  compiling out because a *freshly promoted Large segment* gets zero spare
  span under `exact-span-large` without `large-reserved-capacity` (so the
  very next grow can never fit OPT-G). OPT-H never touches a Large segment or
  `exact-span-large`'s span-rounding at all — it operates entirely within the
  medium/Small segment, before promotion is ever considered. So OPT-H should
  be gated simply on `medium-classes` (plus, if wanted, extended to the whole
  Small ladder independent of `medium-classes` — see §7 point 3), **not** on
  the narrower `medium_promotion_reachable!` predicate
  (`src/registry/heap_core_free.rs:97-130`) that gates promotion. This is an
  important, concrete difference from how promotion is wired: OPT-H's
  `#[cfg]` surface is simpler than promotion's.
- **`large-reserved-capacity` (R20-2's NULL result):** R20-2 showed
  destination-side headroom cannot retroactively cheapen a copy that already
  happened. OPT-H is structurally different — it is not a headroom
  mechanism at all; it asks "is the space physically free RIGHT NOW,
  immediately adjacent, in the segment the block already lives in" and, if
  lazy-commit is on, pays the identical incremental-commit cost an ordinary
  carve already pays (§2.1 point 6) — there is no "established after the
  copy" timing problem here, because there is no copy in the success case.

---

## 4. Sketch of the new code (file-by-file, not full implementation)

- **`src/alloc_core/alloc_core_small.rs`** — one new `pub(super) fn
  try_grow_tail_in_place(&mut self, base: *mut u8, off: usize, old_block_size:
  usize, new_block_size: usize) -> bool`, placed next to `carve_block`
  (`alloc_core_small.rs:1429`) so it can share that function's lazy-commit
  frontier-check-and-commit body (extract that ~15-line block, currently
  inlined in `carve_block`, into a small shared helper both call — the
  cleanest way to avoid hand-duplicating the exact commit arithmetic, per
  this codebase's own stated aversion to unmarked duplication, see the
  `realloc_inplace_fast_path`/`try_realloc_inplace_known_base` "ONE place"
  precedent, `alloc_core.rs:1577-1584`). Body: re-derive `meta =
  SegmentMeta::new(base)`, check preconditions 3/4/5 from §2.1 (tail,
  alignment, capacity), run the shared commit-frontier helper for precondition
  6, then `meta.set_bump(off + new_block_size)`. No `live_count`/bitmap
  touch (the block was never freed). `#[cfg(feature = "page-map-diag")]`:
  extend the existing per-block "mark newly entered pages" loop
  (`carve_block:1546-1554`) over the newly-claimed byte range, for
  diagnostic consistency only (non-load-bearing, per `PageMap`'s own doc).
- **`src/alloc_core/alloc_core.rs`** — `realloc_inplace_fast_path_known_base`
  (`alloc_core.rs:1778`) gains the OPT-H branch sketched in §2.3, calling the
  new `try_grow_tail_in_place`. Doc comment gains an "# OPT-H" section
  alongside the existing "# OPT-G"/"# OPT-F" sections, same format.
- **`src/registry/heap_core_free.rs`** — the `realloc` doc comment's step (2)
  description widens to mention OPT-H; **no functional change** to this
  file's own code, since OPT-H is entirely inside the shared
  `try_realloc_inplace_known_base` call this file already makes (line 908-919
  for the own-segment branch).
- **No `SegmentHeader` field added. No `BinTable` variant added. No new
  feature flag strictly required** — OPT-H can be gated on bare
  `medium-classes` (§3), reusing the feature surface that already exists;
  whether to also offer it independent of `medium-classes` (extending the
  whole Small ladder, §3/§7.3) is a scope decision for the implementing
  round, not settled here.
- **`tests/`** (next round, not this one): a new regression test asserting
  (a) OPT-H fires and returns the same pointer + correct data when all six
  preconditions hold on a hand-constructed tail-adjacent block, (b) OPT-H
  declines and falls through to the existing move-leg/promotion path
  correctly when any single precondition is violated in isolation (not
  tail-adjacent; alignment fails; capacity exceeded), (c) a block grown via
  OPT-H, later shrunk or freed, round-trips correctly through the ordinary
  `class_for`-driven dealloc path (the direct counterfactual for §1.2's
  invariant) — mirroring the rigor `tests/regression_realloc_cross_class_shrink.rs`
  already applies to OPT-F's `==`-not-`<=` rule.

---

## 5. Honest scope assessment — does this close R10-2's kill gate?

This is the section that keeps this design honest rather than presenting a
correct-but-narrow mechanism as a full fix.

### 5.1 Tail-adjacency is a single-slot-per-segment resource

At any instant, **at most one** block per segment satisfies precondition 3
(tail-adjacency) — whichever was carved last and has not yet grown or been
freed. Growing any *other* live block in that segment structurally cannot
take OPT-H, no matter how much nominal room remains in the segment overall
(§1.3) — the bytes after it are already claimed by something else.

### 5.2 What this predicts for R10-2's own harness

R10-2/R18-2/R20-2's harness (`examples/_shared/paired_ab_medium_workload.rs`)
allocates **16 simultaneously-live** 256 KiB objects up front (untimed), then
times growing **all 16** through 384 → 512 → 768 KiB. With 16 × 256 KiB =
4 MiB fitting almost exactly one segment (modulo header/`BinTable`/`PageMap`
overhead pushing a few objects into a second segment), `bump` stays fixed for
the whole timed phase (growth never carves a fresh small block — it always
promotes or, under OPT-H, extends in place) — so **exactly one of the ~15-16
objects per segment** (the very last one carved into it) is ever
tail-adjacent, and only for its *own* first grow attempt. Every other object
in the same segment fails precondition 3 unconditionally.

Layering precondition 4 (alignment) on top narrows this further: for the
medium-class ladder (256 / 320 / 384 / 512 / 768 KiB / 1 MiB,
`src/alloc_core/size_classes.rs:96-112`), `block_size(new_class)` does not
evenly divide most `block_size(old_class)` values, so even the one
tail-adjacent candidate per segment only clears precondition 4 for **some**
carve-order positions (the divisibility works out for roughly 1-in-3 carve
slots for the 256→384 KiB step, by direct computation: an offset that is `k ×
256 KiB` satisfies `off % 384 KiB == 0` iff `k` is a multiple of 3 — worked
example, not a general claim about every transition in the ladder, which
would need to be checked transition-by-transition).

**Honest conclusion: this mechanism, as designed, would intercept only a
small minority of R10-2's own 960 realloc-grow operations** — very roughly
in the range of one-in-several-dozen once both preconditions are combined,
nowhere near enough to move a ~1,180×–2,111× regression into the <20%
kill-gate threshold. **This design does NOT claim to close R10-2's existing
kill-gate harness**, and the next round should not expect it to.

### 5.3 Where this mechanism genuinely helps: the single-hot-buffer pattern

R10-2 §5's own workload-profile table already named the case where this
matters: *"Buffer construction (grow to target, then operate)"* — the
classic `Vec`-style pattern of one buffer being repeatedly appended to and
reallocated, with **nothing else being carved into its segment in between**
grow steps. In that pattern, the buffer being grown typically **is** the most
recently carved allocation at every step (nothing else is being carved
concurrently into the same segment), so tail-adjacency (precondition 3) holds
on essentially every grow — and whether precondition 4 holds depends only on
that one buffer's own carve-order position, not on competition from 15 other
live objects. For this pattern, OPT-H could plausibly eliminate most or all
of the promotion-copy cost for the buffer's lifetime, which is a real,
valuable, and currently completely un-measured case.

**R10-2's harness does not represent this pattern** — it was deliberately
built (§2.3 of that report) to be adversarial (many simultaneously-live
objects, 2× the Large-cache size) specifically to expose the baseline's
Large-cache-miss cost cleanly. It is the wrong harness to evaluate OPT-H's
actual target case. §6 below proposes the harness this design's own
measurement needs instead.

### 5.4 Relationship to R10-2 §5's OTHER named lever (over-allocation)

R10-2 §5 item 2 — *"give each medium-class block a growth headroom (e.g.
1.25×), trading internal fragmentation for realloc speed"* — is a
**different** mechanism (reserve slack at carve time, for every block,
whether it ever grows or not) with a different, opposite trade-off: it helps
the N-simultaneous-object case (every object gets its own headroom,
independent of tail position) at the cost of reduced density for every
object that never grows (undermining `medium-classes`' whole reason for
being attractive — R10-2 §3.1's alloc/free density win). OPT-H pays **no**
density cost (it only ever uses bytes that were going to be uncarved anyway,
and only for the one object currently at the tail) but only helps the
narrower tail-adjacent case. The two are complementary, not competing —
implementing one does not foreclose the other, and this design's §7 next-round
plan explicitly scopes only OPT-H, leaving over-allocation as R10-2 §5 item
2's own separate, still-undesigned lever.

---

## 6. Measurement methodology for the round that implements this

Per this project's established two-stage discipline
(`docs/perf/R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` §5.1, itself following
R9-6→R12-7's precedent): **measure the mechanism's actual hit rate before
investing in a wall-clock gate**, because §5's analysis already predicts a
small, workload-shape-dependent effect — exactly the situation that
discipline exists for.

### 6.1 Stage 1 — diagnostic hit-rate counters, no wall-clock claim yet

Add two `#[cfg(feature = "alloc-stats")]`-gated, `Relaxed`-ordering counters
(no behavior change, mirroring R9-6's `WASTED_DIRTY_DRAINS` precedent):
`OPT_H_ATTEMPTS` (every time the new call site in
`realloc_inplace_fast_path_known_base` is reached for a cross-class
Small/medium grow) and `OPT_H_HITS` (every time all six preconditions hold).
Run this instrumented build (no behavior change from today — the counters
are pure observation) against **two** workload shapes:

1. **R10-2's existing harness, unmodified** (`scripts/r10_2_medium_gate.mjs`,
   reused exactly) — to get the concrete hit-rate number for the adversarial
   N=16-simultaneous-grow case §5.2 predicts is small. This confirms or
   corrects that prediction with a real count instead of the worked-example
   arithmetic above.
2. **A NEW single-hot-buffer harness** (does not exist yet — the most
   concrete, actionable deliverable this design leaves for the next round):
   one buffer, repeatedly grown through the same medium-class ladder
   (256→320→384→512→768 KiB→1 MiB), with **nothing else allocated
   concurrently** in the timed region — the direct realization of R10-2 §5's
   "buffer construction" profile. This is the workload §5.3 predicts OPT-H
   should help; Stage 1 measures whether it actually does, and at what hit
   rate, before writing any wall-clock gate.

**Decision gate:** if the new single-hot-buffer harness shows a hit rate that
is not close to "every eligible grow" (i.e. if precondition 4's alignment
constraint alone kills most attempts even in the friendliest case), the
mechanism should be reconsidered (e.g. is there a carve-order policy that
biases medium-class placement toward alignment-friendly offsets, out of
scope for this document) **before** proceeding to Stage 2's wall-clock work
— exactly the same "don't build sub-design B speculatively" discipline
R17-10 §5.1 applied to its own second sub-design.

### 6.2 Stage 2 — wall-clock gate, only if Stage 1 justifies it

If Stage 1 shows a material hit rate on the single-hot-buffer harness: build
a paired A/B/B/A wall-clock judge for that harness specifically (new
`examples/paired_ab_opt_h_{off,on}.rs` + a shared workload +
`scripts/r20_x_opt_h_gate.mjs`, following the exact `scripts/r10_2_medium_gate.mjs`
/ `scripts/paired-ab-runner.mjs` pattern this whole report family already
established — same dual-axis sub-window + full-round reporting, same
same-vs-same control, same environment-load disclosure, same ≥2 independent
repeats before drawing a conclusion, per `CLAUDE.md`'s standing rules).
**Separately**, re-run `scripts/r10_2_medium_gate.mjs` (unmodified) with
OPT-H compiled in, to report — honestly, per §5.2's prediction — how little
it moves that harness's kill-gate number, so a future reader does not
mistake a single-hot-buffer win for a general fix of R10-2's own gate.

**What "closes the kill-gate" would look like, precisely stated:** R10-2's
existing gate is defined on ITS OWN harness (§4.1 of that report, >20%
regression threshold on the N=16-simultaneous realloc phase). Per §5.2, this
design predicts OPT-H will **not** clear that specific gate. The honest
target for OPT-H's own gate is instead: the single-hot-buffer harness's
realloc-phase wall-clock, measured the same paired way, showing a
statistically significant (paired t > crit, sign test lopsided, ≥2
independent repeats) reduction versus today's promote-every-crossing
baseline — a **different**, additional gate, not a substitute claim about
R10-2's own.

---

## 7. Risks / open questions

1. **Correctness surface: this is new logic on the realloc hot path,
   touching carve/bump-cursor state.** The precondition checks (§2.1) are
   individually simple (integer comparisons, no new syscalls beyond the
   already-existing lazy-commit path), but a **double-carve hazard** is the
   specific new failure mode to guard against: if OPT-H's tail check and the
   actual `set_bump` write are not atomic with respect to a concurrent
   ordinary `carve_block`/`carve_batch` call on the SAME segment, two writers
   could both believe they own `[bump, bump+delta)`. Mirroring the existing
   `bump` field's documented discipline (`segment_header.rs:317-322`: *"the
   Owner touches ONLY the `bump` field... a plain field write is race-free"*
   because `bump` is **owner-only**, never read by a Remote), OPT-H must be
   called **only from the segment's owning thread's own-thread realloc path**
   (exactly where `try_realloc_inplace_known_base` already runs today — it is
   never reached from `dealloc_routing`'s cross-thread free path), so the
   same single-writer argument that makes ordinary `carve_block` race-free
   applies verbatim to OPT-H's `set_bump` call. This must be explicitly
   re-verified against the actual diff at implementation time (per this
   project's zero-trust review convention), not merely asserted from this
   design.
2. **Concurrent-free-during-grow race, under `alloc-xthread`.** Precondition 3
   reads `meta.bump_of()` and precondition 4/6 may write it; between the read
   and the write, could a cross-thread free of some OTHER block in this
   segment (routed via `dealloc_routing` → the `RemoteFreeRing`, not a direct
   `bump` write — remote frees never touch `bump`, only push ring entries,
   per `RACE_DRAIN_RECLAIM.md`'s established protocol) invalidate the tail
   check mid-flight? No — remote frees do not write `bump` at all (only the
   owner does, via `carve_block`/`drain_dirty_segments`'s reclaim path, both
   owner-only), so `bump` cannot change underneath OPT-H's single-threaded,
   owner-only read-then-write sequence. This should be stated as an explicit
   invariant check in the implementation's own doc comment, not left
   implicit.
3. **Interaction with the M2 double-free guards.** OPT-H never frees or
   re-allocates the block (`live_count`/alloc-bitmap untouched, §2.1) — the
   block's M2 state (bit0=allocated in the segment's alloc bitmap) is
   identical before and after an OPT-H grow. A later legitimate free of the
   grown block goes through the ordinary `dealloc_small` path with the
   caller's (now-grown) `Layout`, computing `class_for(new_size)` and
   pushing the SAME offset onto that class's free list — sound per §1.2/§2.2's
   argument, since precondition 4 already proved that offset is a legal
   carve position for that class. No new M2 surface is introduced; this
   should still be exercised by the round's regression suite (§4's third
   test) as a direct check, not assumed.
4. **Hardened generation-table interaction.** `hardened`'s per-`MIN_BLOCK`-
   granule generation table (`segment_header.rs:123-142`) is indexed by raw
   byte offset, independent of class — a block that grows via OPT-H simply
   has more granules "belonging" to it after the grow, which the generation
   table does not need to know about specially (it tracks staleness per
   granule at carve/free time, not per logical block) — but this needs the
   same "verify against the actual diff, don't assume" caveat as points 1–2,
   since the generation table's exact write discipline was not re-derived in
   full here (out of this design's read scope — flagged for the
   implementing round to confirm, not silently assumed safe).
5. **Precondition 4's divisibility constraint may make OPT-H's real-world hit
   rate too small to justify the added complexity even for the
   single-hot-buffer case**, if the medium-class ladder's specific sizes
   (256/320/384/512/768 KiB/1 MiB) turn out to have poor divisibility
   properties across most transitions (§5.2's worked example found ~1-in-3
   for one specific transition; the others were not individually computed
   here). This is exactly what Stage 1 (§6.1) must measure directly before
   Stage 2's wall-clock investment — flagged as the single most likely way
   this design turns out to be a NO-GO even for its own narrower target.
6. **Feature-gate interaction not exhaustively checked here.** §3 argues
   OPT-H's `#[cfg]` should be simpler than promotion's
   (`medium_promotion_reachable!`), gated on bare `medium-classes` rather
   than the extended predicate — but this argument was made by reading the
   mechanism's logical dependencies, not by building and running the
   `cargo-hack` feature-powerset matrix (`docs/perf/OPEN_ITEMS.md`'s own
   R14-10 precedent) against an actual implementation. The implementing round
   should re-verify this claim against the real feature matrix, not treat it
   as settled by this document's reasoning alone.

---

## 8. Next-round plan (implementation + gate — NOT done here)

1. **Stage 1 diagnostic counters** (§6.1): `OPT_H_ATTEMPTS`/`OPT_H_HITS`
   behind `alloc-stats`, no behavior change.
2. **Build the single-hot-buffer harness** (§6.1 point 2) — the concrete,
   currently-nonexistent artifact this design leaves as its most actionable
   deliverable; needed regardless of what Stage 1 finds on R10-2's existing
   harness, since §5.2 already predicts that harness alone will not settle
   the question.
3. **Run Stage 1 on both harnesses; apply the decision gate** (§6.1) — do not
   proceed to implementation-proper if the single-hot-buffer hit rate is
   itself small.
4. **Implement OPT-H** (§2, §4) if Stage 1 passes: the new
   `try_grow_tail_in_place` function, the shared lazy-commit helper
   extraction, the new call-site branch in
   `realloc_inplace_fast_path_known_base`, doc-comment updates.
5. **Regression suite** (§4's three new tests) plus the existing
   `medium-classes`/`alloc-decommit`/`alloc-xthread` test and loom suites run
   against the diff, per this project's zero-trust review convention.
6. **Stage 2 dual-axis wall-clock gate** (§6.2) on the single-hot-buffer
   harness, **plus** an honest re-report of R10-2's own existing gate with
   OPT-H compiled in (predicted, per §5.2, to show little movement) — written
   up as `docs/perf/R2X_Y_OPT_H_INPLACE_MEDIUM_GROW_GATE.md`, following this
   report family's raw-log/summary-CSV conventions.
7. **Promotion recommendation, not decision** — the gate report recommends
   GO/CONDITIONAL-GO/NO-GO for shipping OPT-H behind `medium-classes`; the
   orchestrator/user decides.

---

## 9. Recommendation

**CONDITIONAL-GO.**

**Trigger for proceeding to implementation:** Stage 1's diagnostic hit-rate
counters (§6.1), run against the NEW single-hot-buffer harness this design
specifies (§6.1 point 2, §8 step 2), show that OPT-H's combined
tail-adjacency + alignment preconditions (§2.1 points 3–4) hold for a
material majority of that harness's cross-class grow attempts (a concrete
number is deliberately not fixed here — that is Stage 1's job to establish;
the qualitative bar is "most grows of a single actively-building buffer take
the fast path," not merely "measurably more than zero").

**Explicitly NOT a trigger:** re-running R10-2's existing
N=16-simultaneous-object harness alone. §5.2 already gives a reasoned,
worked-example-grounded prediction that this specific harness will show only
a small minority hit rate no matter how correct the mechanism is, because at
most one object per segment is ever tail-adjacent at a time — that harness
was built to expose a different thing (Large-cache-miss cost) and is
structurally the wrong instrument for OPT-H's actual target case. A future
round should not read a small hit-rate number on R10-2's own harness as "OPT-H
doesn't work" — it should build and run the single-hot-buffer harness before
drawing any conclusion either way.

**Why CONDITIONAL-GO and not a plain GO:** the mechanism is genuinely sound
(§1–§2: zero new stored state, a precisely bounded and checkable set of
preconditions, no density cost, no headroom-timing problem of the kind R20-2
found for `large-reserved-capacity`) and directly addresses a real, currently
completely un-served workload pattern (§5.3) that R10-2 §5 itself named but
never quantified. But its value is conditional on a geometric fact (how often
a growing block is genuinely the segment's bump tail, and how often the
divisibility constraint on top of that holds) that this design could bound
qualitatively (§5.2's worked example) but could not — and should not try to —
settle without building the harness and counting, per this project's
standing "measure before you build the expensive part" discipline.

**Why CONDITIONAL-GO and not NO-GO:** unlike several of this project's other
deferred designs (e.g. `docs/perf/R12_13_PAGE_RUN_LAYER_DEFERRED.md`, deferred
for lack of ANY demonstrated victim workload), this design's target workload
(single actively-growing buffer) is not speculative — it is one of R10-2 §5's
own three named realistic profiles, and it is plausible on first-principles
grounds (§5.3) that the mechanism serves it well. The missing piece is
narrowly empirical (a hit-rate count on a harness that does not yet exist),
not a fundamental doubt about the mechanism's soundness or its target's
existence — that combination is exactly what CONDITIONAL-GO (rather than
NO-GO) is for.

---

## 10. Files/lines this document is grounded in (for the next round's reader)

- `src/alloc_core/alloc_core.rs:1509-1912` — `realloc`, `safe_payload_read_span`,
  `realloc_inplace_fast_path_known_base` (OPT-F/OPT-G, the function OPT-H's
  new branch is added to), `try_realloc_inplace_known_base`,
  `try_grow_large_reserved_capacity` (R12-4, the closest existing precedent
  for "grow into adjacent uncommitted-but-reserved space").
- `src/alloc_core/alloc_core_small.rs:1419-1557` — `carve_block` (the
  bump-cursor carve logic OPT-H's tail check and lazy-commit reuse are
  grounded in); `:1608-1660+` — `carve_batch` (confirms the same bump
  discipline for batched carves).
- `src/alloc_core/alloc_core_small.rs:1735-1835` — `dealloc_small` (the
  class-keyed-by-current-`Layout` free path OPT-H's precondition 4 must stay
  consistent with).
- `src/alloc_core/segment_header.rs:144-202` — `SegmentKind`, `PageMap`'s
  "mixed-class"/"NOT a reliable class oracle" doc (§1.1's grounding).
- `src/alloc_core/segment_header.rs:313-407` — `SegmentHeader`'s `bump`
  field doc (owner-only, single-writer discipline — §7 point 1's grounding).
- `src/alloc_core/segment_header.rs:992-1011` — `BinTable` (per-class
  free-list heads, §1.2's grounding).
- `src/alloc_core/segment_header.rs:1084-1160` (approx.) — `SegmentMeta`,
  `bump_of`/`set_bump`.
- `src/alloc_core/size_classes.rs:85-134` — `EXTRAS` (the six medium classes:
  256/320/384/512/768 KiB/1 MiB — §5.2's worked-example ladder).
- `src/registry/heap_core_free.rs:1-160` — the `medium_promotion_reachable!`
  macro and `MEDIUM_REALLOC_PROMOTION_THRESHOLD` (§3's point that OPT-H's own
  `#[cfg]` should be simpler than promotion's).
- `src/registry/heap_core_free.rs:846-1033` — `HeapCore::realloc` (the
  own-segment branch: A1 drain, in-place attempt, promotion, move-leg — the
  call-site sequence OPT-H is inserted into, §2.3).
- `src/registry/heap_core_free.rs:1105-1279` — `try_promote_to_large` (the
  existing mechanism OPT-H does NOT replace, only precedes).
- `docs/perf/R10_2_MEDIUM_CLASSES_NATIVE_GATE.md` §4/§5 — the kill-gate
  definition and the three named mitigation levers (item 1 is this design;
  item 2 is the separate, still-undesigned over-allocation lever, §5.4).
- `docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` (R18-2's re-run) — ruled
  out cache-slot pressure and the R17-4 leak as explanations.
- `docs/perf/R20_2_C4_RESERVED_CAPACITY_HEADROOM_GATE.md` — ruled out
  destination-side reserved-capacity headroom; §3's argument for why OPT-H
  does not share that mechanism's headroom-timing problem.
- `docs/perf/R17_10_BATCHED_DEFERRED_RECLAIM_DESIGN.md` §5.1 — the two-stage
  "measure the hit rate before building the wall-clock gate" precedent this
  design's §6 follows.
- `docs/perf/OPEN_ITEMS.md` — Active item 1 (updated by this task to cite
  this design and its CONDITIONAL-GO verdict, without moving to "Recently
  resolved" — design is not implementation).
