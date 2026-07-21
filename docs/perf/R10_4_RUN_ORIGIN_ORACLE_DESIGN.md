# R10-4 — Run-origin oracle for `class_align`-based carve alignment: DESIGN-ONLY (no code change)

**Task:** design (and, only if airtight, prototype) a change to `carve_block`'s
alignment that replaces the current `align_up(bump, block_size)` with
`align_up(bump, class_align)` — alignment to the class's largest power-of-two
divisor instead of the full `block_size` — so that non-power-of-two block sizes
(the `medium-classes-wide` 1.25 / 1.5 / 1.75 MiB classes) waste less segment
capacity on alignment padding. The hard part: the cross-thread reclaim guard
chain relies on "a valid block's offset is an exact multiple of its class's
`block_size`" as defence-in-depth; the new alignment breaks that invariant and
the guard needs redesigning around a run-origin oracle.
**Outcome:** **DESIGN-ONLY.** No `src/`, `Cargo.toml`, or `tests/` file is
modified. The deliverable is this doc. §7 states the verdict; §8 gives the
staged plan (and the explicit GO/NO-GO recommendation) for a future session.
**Date:** 2026-07-21
**Base revision:** `main` @ `c8d53af` (R10-2 just landed; the reclaim path under
analysis was modified by R10-3 @ `abaad9c` — the `reclaim_offset` /
`reclaim_offset_checked` return-value semantics — and re-read fresh this
session; line numbers below are current as of `c8d53af`). The carve substrate
under analysis is unchanged since R9-4 @ `c8f5f32` (which landed the three wide
classes).
**Platform:** Windows 10 Pro x86-64 (analysis host). The density arithmetic is
deterministic geometry (§2), not timings; the perf overhead estimate (§5) is
analytical from instruction costs, mirroring R9-5 §8 / R9-7 §8. No measurement
is performed.

---

## 0. TL;DR — CONDITIONAL GO (sound but not obviously worth it vs the page-run layer)

The alignment change is **structurally correct and the density gain is real**:
replacing `align_up(bump, block_size)` with `align_up(bump, class_align)` (where
`class_align` = the largest power-of-two dividing `block_size`) lifts the
empirical per-segment density of the three wide classes from R9-4's measured
**2 / 1 / 1** to **3 / 2 / 2** — exactly the review's original 3×/2×/2× guess
that R9-4 proved optimistic under the old alignment (§2.3). The arithmetic is
exact and reproducible from `SEGMENT`, `small_meta_end`, `block_size`, and
`class_align` alone (§2.4).

**The guard redesign is also sound** — but not free. The current
`off % block_size == 0` defence-in-depth check (the "§13 corruption guard,"
`alloc_core_small_reclaim.rs:104` and `:241`) is a necessary condition for "this
offset is a valid block-start." Under the new alignment, valid block-starts are
no longer `block_size`-aligned, so this check must be replaced. §3 inventories
**six** code sites that assume block-starts are `block_size`-aligned; §4
presents two concrete oracle designs that restore containment. The stronger
design — a per-segment "carved-starts" bitmap — is **strictly stronger** than
the current check (it is an EXACT record of valid block-starts, not a necessary
condition). The cheaper design — a per-class run-origin array in the already-
reserved second `BinTable` slot — is at-least-as-safe for the wide classes
(§4.2), but requires careful carve-time bookkeeping for multiple-run segments.

**However, the strongest argument AGAINST proceeding is that the page-run layer
(R8-9 §5 K7, R9-4 recommendation #2) is a strictly superior solution**: a 16 MiB
medium arena would deliver density 11 / 9 / 8 for the three wide classes (vs the
alignment change's 3 / 2 / 2) with ZERO guard breakage, ZERO new metadata, and
ZERO new correctness surface — `off % block_size == 0` stays load-bearing
because `block_size` is still the alignment in a page-run arena. The alignment
change trades a correctness-sensitive redesign for a modest fraction of the
gain the page-run layer achieves with no such trade. §9 makes this explicit and
is the primary reason this report's recommendation is **CONDITIONAL** rather
than an unconditional GO.

**No prototype is shipped** (§8). A minimal safe stage-2 prototype would be
behind a NEW feature gate (`wide-class-align`, implying `medium-classes-wide`),
changing only the wide classes' carve alignment and adding the run-origin
oracle scoped to those classes only — the existing `medium-classes-wide` path
stays byte-identical until the new gate is validated. The test plan mirrors
R9-4's `tests/medium_classes_wide_correctness.rs` density-measurement pattern.

---

## 1. Scope recap — what `carve_block` does today, and what changes

### 1.1 Today's carve alignment

`carve_block` (`src/alloc_core/alloc_core_small.rs:1165-1275`) aligns the start
of every carved block to the FULL `block_size`:

```text
let aligned_bump = align_up(bump, block_size);          // line 1175
if aligned_bump + block_size > SEGMENT { return None; }  // line 1176
```

`align_up` (`segment_header.rs:723-728`) is ceiling-division that works for ANY
divisor, not just powers of two. For power-of-two `block_size`, the result is
identical to `bump + (-bump & (block_size - 1))` (an AND-mask round-up). For
non-power-of-two `block_size` (the wide classes), `align_up` performs a real
`div_ceil` — but this is at carve time (cold path), not on the free/reclaim hot
path, so the cost is immaterial.

The batched sibling `carve_batch` (`alloc_core_small.rs:1322-1417`) does the
same `align_up(bump, block_size)` once at run start (`line 1334`), then strides
by `block_size`. Its doc comment (`lines 1286-1288`) notes the hoist is valid
because "every class `block_size` is a multiple of `MIN_BLOCK`), so
`align_up(bump, block_size)` is a TAUTOLOGY from the second block on" — after
the first block, `bump = aligned_start + block_size` is already
`block_size`-aligned (since `block_size` divides itself), so the next
`align_up` is a no-op.

### 1.2 Why full-`block_size` alignment is over-conservative for non-PoT classes

The carve's alignment requirement exists to satisfy ONE constraint: the block
handed to the caller must be aligned to the user's `Layout::align()`, which is
always a power of two (Rust's `Layout` contract). `class_for(size, align)`
(`size_classes.rs:209-211`) returns a class whose `block_size >= max(size,
align)` AND `block_size % align == 0` (the M4 invariant, `size_classes.rs:51-54`
and the crate's divisibility slow path). So:

- The block handed out is `block_size` bytes starting at offset `off` in a
  `SEGMENT`-aligned segment. The block is `off`-aligned (absolute address =
  `segment_base + off`, and `segment_base` is `SEGMENT`-aligned ≥ `block_size`).
- For the block to be `align`-aligned, `off` must be a multiple of `align`.
- The current scheme achieves this by making `off` a multiple of `block_size`
  (which is ≥ `align` and a multiple of `align`), so `off` is trivially
  `align`-aligned. But this is SUFFICIENT, not NECESSARY — `off` only needs to
  be a multiple of `align`, not of `block_size`.
- The comment at `heap_core_alloc.rs:143-148` states this invariant verbatim:
  "Every block carved for class `c` sits at an offset that is a multiple of
  `block_size(c)` … so any block of class `c` is automatically `align`-aligned
  regardless of what `align` was."

The **minimum** alignment that satisfies all constraints is `class_align` — the
largest power-of-two dividing `block_size`. Every `align` that `class_for`
accepts for this class is a power-of-two dividing `block_size`, hence dividing
`class_align`. So `off` being a multiple of `class_align` implies `off` is a
multiple of every such `align`. §2 derives `class_align` precisely per class.

### 1.3 Why the over-conservatism costs a whole block for wide classes

For a class whose `block_size > small_meta_end` (72 KiB), the first carved
block in a fresh segment goes at offset `align_up(72 KiB, block_size)`:

- If `block_size` is a power of two ≥ 128 KiB: `align_up(72 KiB, block_size) =
  block_size` (since 72 KiB < `block_size`). The "gap" `[small_meta_end,
  block_size)` is wasted — up to one whole block of capacity. But since the
  gap is < `block_size`, the tax is at most 1 block, and for the existing
  power-of-two medium classes (256 KiB – 1 MiB), the density is high enough that
  losing 1 block is tolerable (R9-4 §2.2: 256 KiB loses 1 of 16 → 15 fit).
- If `block_size` is NOT a power of two (the wide classes): the same
  `align_up(72 KiB, block_size) = block_size` applies, but `class_align`
  (the real alignment need) is SMALLER than `block_size`. Aligning to
  `class_align` instead puts the first block at
  `align_up(72 KiB, class_align)`, which can be MUCH earlier than `block_size`,
  recovering the wasted capacity. R9-4 measured the tax at exactly one block
  per wide class (2/1/1 instead of the theoretical 3/2/2); §2.4 shows the
  `class_align` change recovers exactly that block.

---

## 2. Full geometry of the new alignment scheme

### 2.1 Deriving `class_align` — the class's true power-of-two alignment need

**Definition.** For a class with `block_size = B`, `class_align(B)` is the
largest power of two `p` such that `B % p == 0` — equivalently, `p = 2^v2(B)`
where `v2` is the 2-adic valuation (the number of trailing zero bits of `B`).

Since every `block_size` is a multiple of `MIN_BLOCK` (16 = 2⁴, enforced by M4),
`class_align(B) >= 16` for every class. For power-of-two `block_size` (the
existing page-aligned and medium classes — 4096, 8192, …, 256 KiB, …, 1 MiB),
`class_align(B) = B` and the new scheme is identical to the old. The change
only affects non-power-of-two classes.

**Derivation for the three wide classes** (`size_classes.rs:130-132`,
`1280 * 1024` / `1536 * 1024` / `1792 * 1024`):

| Class | `block_size` (bytes) | factorization | `class_align` | `class_align` (KiB) |
|---|---|---|---|---|
| 1.25 MiB | 1,310,720 | 2¹⁸ × 5 | 2¹⁸ = 262,144 | 256 KiB |
| 1.50 MiB | 1,572,864 | 2¹⁹ × 3 | 2¹⁹ = 524,288 | 512 KiB |
| 1.75 MiB | 1,835,008 | 2¹⁸ × 7 | 2¹⁸ = 262,144 | 256 KiB |

**Derivation for representative small geometric classes** (to confirm the
change does not regress them):

| `block_size` | factorization | `class_align` | old `align_up` | new `align_up` | change? |
|---|---|---|---|---|---|
| 16 | 2⁴ | 16 | to 16 | to 16 | identical |
| 48 | 2⁴ × 3 | 16 | to 48 | to 16 | **yes** (but density impact negligible — §2.5) |
| 80 | 2⁴ × 5 | 16 | to 80 | to 16 | **yes** |
| 96 | 2⁵ × 3 | 32 | to 96 | to 32 | **yes** |
| 4096 | 2¹² | 4096 | to 4096 | to 4096 | identical |
| 6144 | 2¹¹ × 3 | 2048 | to 6144 | to 2048 | **yes** |

So the change affects many small classes too — but §2.5 shows the density impact
for small classes is negligible (sub-1-block in a high-density segment). The
feature gate (§8) scopes the change to the wide classes only, leaving small
classes on the old alignment.

### 2.2 What exactly changes in `carve_block` / `carve_batch`

Two one-line changes, both feature-gated to the wide classes:

```text
// carve_block, line 1175 (SKETCH — NOT applied):
let align = if cfg!(feature = "wide-class-align") && block_size > SMALL_MAX_MEDIUM {
    class_align(block_size)       // = 1 << block_size.trailing_zeros()
} else {
    block_size                    // today's behaviour
};
let aligned_bump = align_up(bump, align);
```

`carve_batch` at `line 1334` gets the same one-line change. The `block_size >
SMALL_MAX_MEDIUM` guard (where `SMALL_MAX_MEDIUM = 1024 * 1024`, the largest
six-class medium entry) scopes the new alignment to the three wide classes only;
all other classes keep `align_up(bump, block_size)` byte-identical to today.

`class_align` is a trivial compile-time derivation: `1 << block_size.trailing_zeros()`.
For the wide classes, `trailing_zeros` of the three block sizes yields 18, 19,
18 respectively (verified by the factorizations in §2.1). No table lookup, no
runtime computation beyond a `BSF`/`CTZ` instruction (1 cycle).

### 2.3 The tautology still holds under the new scheme

`carve_batch`'s hoist argument (`lines 1286-1288`) — that `align_up(bump,
block_size)` is a tautology from the second block on because `bump` is already
`block_size`-aligned — remains valid under the new scheme with `class_align` in
place of `block_size`:

- After block 0: `bump = aligned_start + block_size`. Since `aligned_start` is
  `class_align`-aligned and `block_size` is a multiple of `class_align`,
  `bump` is `class_align`-aligned.
- So the next `align_up(bump, class_align)` is a no-op (tautology).

This means `carve_batch`'s single-align-then-stride structure is preserved
unchanged; only the alignment VALUE changes.

### 2.4 Density arithmetic — old vs new, per wide class

**Constants** (from R9-4 §2.1, verified against `SegmentLayout` this session):

```text
SEGMENT         = 4,194,304 (4096 KiB = 4 MiB)
small_meta_end  =    73,728 (72 KiB)   — non-hardened; hardened adds the gen table
```

**Density formula.** Under alignment `A` and stride `B` (= `block_size`), the
first block sits at `origin = align_up(small_meta_end, A)`. Block `k` sits at
`origin + k * B`. The last block that fits satisfies
`origin + k * B + B <= SEGMENT`, i.e., `k <= (SEGMENT - origin) / B - 1`. So:

```text
density = floor((SEGMENT - origin) / B)
       = floor((SEGMENT - align_up(small_meta_end, A)) / B)
```

(Note: when `origin = small_meta_end` — i.e., `small_meta_end` is already
`A`-aligned — this simplifies to `floor((SEGMENT - small_meta_end) / B)`. When
`A = B` and `small_meta_end < B`, it simplifies to `floor(SEGMENT / B) - 1` —
R9-4's formula. Under the new scheme with `A = class_align < B`, `origin =
align_up(73728, class_align)` can be less than `B`, giving higher density.)

**Per-class computation:**

| Class | `B` | Old `A` | Old `origin` | Old density | New `A` | New `origin` | New density | Gain |
|---|---|---|---|---|---|---|---|---|
| 1.25 MiB | 1,310,720 | 1,310,720 | 1,310,720 | **2** | 262,144 | 262,144 | **3** | +1 (+50%) |
| 1.50 MiB | 1,572,864 | 1,572,864 | 1,572,864 | **1** | 524,288 | 524,288 | **2** | +1 (+100%) |
| 1.75 MiB | 1,835,008 | 1,835,008 | 1,835,008 | **1** | 262,144 | 262,144 | **2** | +1 (+100%) |

**Worked example — 1.25 MiB, new scheme:**
- `origin = align_up(73,728, 262,144) = 262,144` (first multiple of 256 KiB ≥ 72 KiB).
- Block 0: `[262,144, 1,572,864)`. Block 1: `[1,572,864, 2,883,584)`. Block 2:
  `[2,883,584, 4,194,304)` — ends exactly at `SEGMENT`, so the carve check
  `aligned_bump + block_size > SEGMENT` reads `2,883,584 + 1,310,720 = 4,194,304
  > 4,194,304` → **FALSE** → block 2 fits. Block 3 would start at `4,194,304` →
  `4,194,304 + 1,310,720 > SEGMENT` → fail. **Density = 3.** ✓

**Worked example — 1.50 MiB, new scheme:**
- `origin = align_up(73,728, 524,288) = 524,288`.
- Block 0: `[524,288, 2,097,152)`. Block 1: `[2,097,152, 3,670,016)`. Block 2:
  `[3,670,016, 5,242,880)` → `3,670,016 + 1,572,864 = 5,242,880 > 4,194,304` →
  fail. **Density = 2.** ✓

These match the review's original 3×/2×/2× guess that R9-4 §2.3 proved was
optimistic by exactly one block per class under the old alignment. The new
alignment recovers exactly that block.

### 2.5 Impact on small classes — negligible, and out of scope for stage 2

For a small class like `block_size = 80` (class_align = 16):
- Old `origin = align_up(72 KiB, 80) = 73,760` (920 bytes of gap).
- New `origin = align_up(72 KiB, 16) = 73,728` (0 bytes of gap — already 16-aligned).
- Density change: `floor((4,194,304 - 73,760) / 80) = 51,516` → `floor((4,194,304 -
  73,728) / 80) = 51,527`. A gain of 11 blocks out of ~51,500 — **0.02%**.

For all small classes, the density change is sub-0.1% (the alignment gap is at
most `block_size - MIN_BLOCK` bytes, and density is `~SEGMENT / block_size` ≫ 1).
**The feature gate scopes the alignment change to the wide classes only** (§8);
small classes keep the old `align_up(bump, block_size)` and their density is
unchanged. This avoids touching the §13 guard for any class that doesn't need
it, minimizing the correctness surface.

---

## 3. Full inventory of "a live block's offset is a multiple of its class's `block_size`"

Exhaustive grep across `src/` for `is_multiple_of`, `align_up(.., block_size)`,
`% block_size`, and related patterns. Every site classified as (a) unaffected,
(b) needs the run-origin oracle, or (c) needs a different fix.

### 3.1 The carve alignment sites (the CAUSE of the invariant)

| # | File:line | Code | Classification |
|---|---|---|---|
| C1 | `alloc_core_small.rs:1175` | `let aligned_bump = align_up(bump, block_size);` | **(c) — the change itself.** Under the feature gate, becomes `align_up(bump, class_align)` for wide classes. |
| C2 | `alloc_core_small.rs:1334` | `let aligned_start = align_up(bump, block_size);` | **(c) — the batched sibling.** Same one-line change. |

### 3.2 The §13 corruption guard sites (the AFFECTED guards)

| # | File:line | Code | Feature gate | Classification |
|---|---|---|---|---|
| G1 | `alloc_core_small_reclaim.rs:104` | `if !(off as u32).is_multiple_of(bs) { return false; }` | unconditional (`alloc-xthread + fastbin`) | **(b) — needs the oracle.** Cross-thread reclaim (`reclaim_offset_checked`). This is the HOT guard on the drain path. |
| G2 | `alloc_core_small_reclaim.rs:241` | `if !(off as u32).is_multiple_of(bs) { return false; }` | unconditional (`alloc-xthread`) | **(b) — needs the oracle.** Cross-thread reclaim (`reclaim_offset`), non-magazine variant. Identical guard, identical fix. |
| G3 | `alloc_core_small.rs:1454` | `if !(off as usize).is_multiple_of(SizeClasses::block_size(class_idx)) { return; }` | `hardened`-only (default OFF) | **(b) — needs the oracle** (under `hardened + wide-class-align`). Own-thread interior-pointer guard. Same fix as G1/G2 but on the own-thread path. |
| G4 | `heap_core_free.rs:185` | `if !off_h.is_multiple_of(bs) { return; }` | `hardened`-only (default OFF) | **(b) — needs the oracle** (under `hardened + wide-class-align`). Magazine free path's interior-pointer guard. Same fix. |

### 3.3 The invariant-statement sites (COMMENTS that document the assumption)

| # | File:line | What it says | Classification |
|---|---|---|---|
| D1 | `alloc_core_small.rs:1286-1288` | "`align_up(bump, block_size)` is a TAUTOLOGY from the second block on" | **(a) — unaffected.** The tautology holds under `class_align` too (§2.3). The comment needs updating to say "`class_align`" for wide classes, but the logic is unchanged. |
| D2 | `heap_core_alloc.rs:143-148` | "Every block carved for class `c` sits at an offset that is a multiple of `block_size(c)`" | **(c) — comment needs updating.** Under the feature gate, wide-class blocks sit at `class_align`-multiples, not `block_size`-multiples. The soundness argument (any `block_size`-multiple is automatically `align`-aligned) still holds because `class_align` ≥ `align` for every accepted `align`. The COMMENT overstates the invariant; the LOGIC is sound. |
| D3 | `alloc_core_small.rs:1443-1452` | "A real block start of class `class_idx` sits at an `off` that is a whole multiple of `block_size(class_idx)` (carve aligns the bump to `block_size`)" | **(c) — comment needs updating.** Same as D2: the guard's COMMENT describes the old invariant; the guard's LOGIC needs the oracle (§4). |
| D4 | `alloc_core_small_reclaim.rs:43-46, 233-239` | "a mis-aligned offset would write the free-list `next` into the middle of a block — the §13 corruption" | **(a) — the PROBLEM STATEMENT is unaffected.** The §13 corruption mechanism (garbled offset → `write_next` into mid-block) is exactly what the oracle must still prevent. The guard that implements it (G1/G2) changes; the corruption class it defends against does not. |

### 3.4 The free-list traversal sites (pop_free / drain_freelist_batch)

| # | File:line | Code | Classification |
|---|---|---|---|
| F1 | `alloc_core_small.rs:947` | `let block_ptr = Node::deref(segment, head_off as usize);` | **(a) — unaffected.** `pop_free` reads the head offset from the `BinTable` and turns it into a pointer. The head offset was stored by `dealloc_small`/`reclaim_offset` — it is whatever offset was pushed, regardless of alignment. `pop_free` does NOT check alignment; it trusts the `BinTable` entry. |
| F2 | `alloc_core_small.rs:949` | `let next = Node::read_next(block_nn);` | **(a) — unaffected.** Reads the intrusive `next` word from the block body. The block was pushed by `dealloc_small`/`reclaim_offset`, which validated it via the guard chain (including the oracle). So `pop_free` inherits the guard's guarantee. |
| F3 | `alloc_core_small.rs:1406` | `let off = aligned_start + i * block_size;` (`carve_batch` page-map loop) | **(a) — unaffected.** This is carve-time offset arithmetic, not a guard. The offsets are computed from `aligned_start` and `block_size`, which are correct under either alignment. |

### 3.5 The bitmap / directory / page-map sites

| # | File:line | Code | Classification |
|---|---|---|---|
| B1 | `alloc_bitmap.rs:106-108` (`is_free`) | `self.0.test(off)` — bit at `off >> MIN_BLOCK_SHIFT` | **(a) — unaffected.** The bitmap is indexed at `MIN_BLOCK` (16 B) granularity. Every block-start is `MIN_BLOCK`-aligned under both schemes (since `class_align >= MIN_BLOCK`), so the bit index is always exact. |
| B2 | `alloc_bitmap.rs:113-115` (`mark_free`) | `self.0.set(off)` | **(a) — unaffected.** Same argument. |
| B3 | `segment_directory.rs:154` | `class_nonempty: [[u64; WORDS_PER_CLASS]; SMALL_CLASS_COUNT]` | **(a) — unaffected.** The directory records per-class per-segment "has free blocks" — a boolean, not an offset. No alignment assumption. |
| B4 | `alloc_core_small.rs:1268-1272` | `pm.set_class(page, class_idx)` — page-dedication | **(a) — unaffected.** The page map records which class first touched each PAGE. The page index is `off / PAGE`, which is well-defined for any offset. The page-dedication rule ("first class wins") is alignment-agnostic. |

### 3.6 The cross-thread routing site (ring packing)

| # | File:line | Code | Classification |
|---|---|---|---|
| R1 | `heap_core_xthread.rs:697-705` | `let off = (ptr - base) as u32; let class_idx = class_for(size, align);` → `pack_entry(off, class_idx)` | **(a) — unaffected.** The cross-thread freer packs the raw offset and the class from the caller's `Layout`. It does NOT check alignment — it trusts the pointer is a real block-start (the caller's `GlobalAlloc` contract). The alignment check lives on the CONSUMER side (G1/G2). |
| R2 | `remote_free_ring.rs:439` | `off.is_multiple_of(MIN_BLOCK)` — debug_assert in `pack_entry_hardened` | **(a) — unaffected.** This checks `MIN_BLOCK` alignment (always true under both schemes), not `block_size` alignment. |

### 3.7 Summary of the inventory

```text
Category (a) — unaffected:     D1, D4, F1, F2, F3, B1, B2, B3, B4, R1, R2    (11 sites)
Category (b) — needs oracle:   G1, G2, G3, G4                                 (4 sites)
Category (c) — the change /    C1, C2, D2, D3                                  (4 sites)
  comment update:
```

The **four guard sites (G1–G4)** are the load-bearing surface. G1 and G2 are
UNCONDITIONAL (they run in production on every cross-thread reclaim); G3 and G4
are `hardened`-gated (default OFF, production-inert today). The §4 oracle design
must satisfy G1/G2 at minimum; G3/G4 follow identically under the combined
`hardened + wide-class-align` feature stack.

---

## 4. Concrete run-origin oracle design

### 4.0 What the oracle must answer

The current guard `off % block_size == 0` answers the question: **"is `off` a
plausible block-start of class `class_idx`?"** It is a NECESSARY condition under
the old alignment (every block-start is `block_size`-aligned, but not every
`block_size`-aligned offset is a block-start — the `bump` and `is_free` guards
finish the job). Under the new alignment, valid wide-class block-starts are
`class_align`-aligned but NOT `block_size`-aligned, so this check rejects every
valid block. The oracle must replace it with a check that accepts all valid
block-starts and rejects all non-block-starts (or at least: rejects at least as
many non-block-starts as the old check did — §6).

**The specific failure mode the oracle must prevent** (tracing the full guard
chain at `reclaim_offset_checked:92-197`): if a garbled ring entry has an offset
`off` that is ≥ `payload_start`, < `bump`, and has `is_free(off) == false`
(bitmap bit = 0), the guard chain would proceed to `write_next(block_nn,
old_head_ptr)` at `line 187` — writing a `*mut u8` into the first word at `off`.
If `off` is in the MIDDLE of a live block (an interior pointer), this clobbers
the live block's first word → silent corruption (the §13 corruption class). If
`off` is in a gap (alignment padding between metadata end and first block, or
between two runs), it writes into bytes that may be carved later → latent
corruption. The oracle's job is to catch both cases BEFORE `write_next`.

### 4.1 Proposed oracle A — per-segment "carved-starts" bitmap (STRONGEST, simplest to reason about)

**Data layout.** A third per-segment bitmap (alongside `AllocBitmap` and
`MagazineBitmap`), identical MECHANISM (`SegmentBitmap`, one bit per
`MIN_BLOCK`-slot) but different SEMANTICS:

```text
CarvedBitmap: one bit per MIN_BLOCK slot of the segment.
  bit = 1: this MIN_BLOCK slot is the START of a block that was carved in this
           segment lifetime (set at carve time, NEVER cleared within the lifetime).
  bit = 0: this slot is NOT a block-start (interior of a block, gap, metadata,
           or uncarved payload).
```

**Where it lives.** Added to `SegmentHeaderLayout` after `MagazineBitmap`, before
the `RemoteFreeRing`. Under `#[cfg(feature = "wide-class-align")]` only;
feature-OFF builds have byte-identical `small_meta_end` to today (the
non-disturbance requirement, mirroring R9-4 K7 and the X7 Ф1 gen-table's
feature-gated layout).

**Where it is consulted.** In the four guard sites (G1–G4), replacing the
`is_multiple_of(bs)` check:

```text
// reclaim_offset_checked, SKETCH (replaces line 104):
#[cfg(not(feature = "wide-class-align"))]
if !(off as u32).is_multiple_of(bs) { return false; }    // today's check
#[cfg(feature = "wide-class-align")]
if !carved_bitmap.is_carved_start(off as u32) { return false; }  // new check
```

For wide classes under the feature, the bitmap check replaces the modulo. For
all other classes (power-of-two `block_size`), the old modulo is retained
unchanged — it compiles to an AND (free) and needs no bitmap consultation.

**Where it is updated.** At carve time, in `carve_block` and `carve_batch`,
AFTER the bump advance and BEFORE returning the pointer:

```text
// carve_block, SKETCH (after line 1273):
#[cfg(feature = "wide-class-align")]
if block_size > SMALL_MAX_MEDIUM {
    meta.carved_bitmap().set_carved(aligned_bump as u32);
}
```

`carve_batch` sets one bit per carved block in its page-map loop (`line 1405`),
adding a single `set_carved(off)` call per block. The per-block cost is one byte
load + OR + store (same as `AllocBitmap::mark_free`), applied only to wide-class
blocks — a cold path (wide-class carves are rare; a segment hosts at most 3).

**Memory overhead.** `CarvedBitmap::FOOTPRINT = SEGMENT / MIN_BLOCK / 8` =
`4,194,304 / 16 / 8` = **32,768 bytes (32 KiB, 8 pages) per segment** — identical
to `AllocBitmap` and `MagazineBitmap`. This raises `small_meta_end` from 72 KiB
to **104 KiB** (+44%) under the feature. The per-segment overhead ratio rises
from 1.75% to 2.5%. For comparison, the `hardened` feature's gen table adds
~256 KiB per segment (3.5× more); the `SegmentDirectory` sidecar adds 6.1 KiB
per HEAP (not per segment) — both are accepted prior art. **32 KiB/segment is
in the same order as the gen table but on the HIGH end.**

**Does the density gain survive the metadata tax?** Recomputing §2.4's density
with `small_meta_end = 104 KiB` (106,496 bytes):
- 1.25 MiB: `origin = align_up(106,496, 262,144) = 262,144` — **unchanged**
  (106,496 < 262,144). Density still **3**. ✓
- 1.50 MiB: `origin = align_up(106,496, 524,288) = 524,288` — **unchanged**.
  Density still **2**. ✓
- 1.75 MiB: `origin = align_up(106,496, 262,144) = 262,144` — **unchanged**.
  Density still **2**. ✓

The wide classes' density is unaffected because `class_align` (256/512 KiB) ≫
the 32 KiB metadata increase. The tax falls on SMALL classes: their payload
shrinks by 32 KiB, costing ~0.8% density (e.g., a 256 B class goes from ~15,800
to ~15,775 blocks — negligible). Since the feature is opt-in and targeted at
wide-class-heavy workloads, this is an acceptable trade.

**Containment strength.** STRICTLY STRONGER than the current `off % block_size
== 0`. The modulo check is a necessary condition (accepts interior
`block_size`-multiples that happen to be aligned); the bitmap is an EXACT record
(accepts only offsets that were actually carved as block-starts). Every offset
the bitmap accepts is a real block-start; the modulo check cannot make this
claim. §6 formalizes this.

### 4.2 Proposed oracle B — per-class run-origin array (CHEAPEST, scoped to wide classes)

**Motivation.** Oracle A's 32 KiB/segment cost is the same regardless of whether
the segment ever hosts a wide-class block. Oracle B exploits the fact that the
wide classes are just 3 classes with at most 3 blocks each per segment — the set
of valid block-starts is tiny and can be recorded in a few bytes.

**Data layout.** A per-class, per-segment **run-origin array** stored in the
ALREADY-RESERVED second `BinTable` footprint (`segment_header_layout.rs:23-27`:
"the second `BinTable` footprint is the slot Phase 13.4b's two-list will occupy.
Reserving it now means 13.4b adds its second head array in place WITHOUT
shifting the bitmap / ring / registry offsets again"). This slot is
`SMALL_CLASS_COUNT * 4 = 58 * 4 = 232` bytes, currently zeroed and unused. We
repurpose the 3 wide-class entries (indices 55, 56, 57) as small fixed-size
arrays of run origins:

```text
// Per wide-class run-origin record (12 bytes each, 3 classes = 36 bytes total):
struct RunOrigins {
    origins: [u32; 3],   // up to 3 run origins (max 3 blocks per wide class per segment)
    count: u8,           // number of active origins (0..=3)
}
```

Since each wide class has at most `floor(SEGMENT / block_size) = 3` blocks, and
each block belongs to exactly one run, there are at most 3 runs per class. The
fixed-size `[u32; 3]` array with a `count` byte cannot overflow.

**Where it lives.** In the second `BinTable` slot, at the offsets for wide-class
indices (55, 56, 57). No layout change needed — the space is already allocated
and zeroed. Feature-gated under `wide-class-align`; feature-OFF builds leave the
second `BinTable` slot zeroed and unused (byte-identical to today).

**Where it is updated.** At carve time, when `carve_block` / `carve_batch`
starts a NEW run for a wide class (i.e., when `align_up(bump, class_align) >
bump`, meaning the alignment skipped ahead — the bump was NOT
`class_align`-aligned):

```text
// carve_block, SKETCH:
let aligned_bump = align_up(bump, class_align);
// ... carve the block ...
#[cfg(feature = "wide-class-align")]
if block_size > SMALL_MAX_MEDIUM && aligned_bump != bump {
    // A new run started — record its origin.
    meta.run_origins(class_idx).push(aligned_bump as u32);
}
```

Note: `aligned_bump != bump` is the "new run" signal. If `aligned_bump == bump`,
the bump was already `class_align`-aligned (continuation of the previous run,
or the first carve in a fresh segment where `bump == small_meta_end` happens to
be `class_align`-aligned — which it is NOT for the wide classes since
`small_meta_end = 72 KiB` is not a multiple of 256 KiB or 512 KiB). So the
FIRST carve of a wide class in a segment always starts a new run and records
its origin. Subsequent contiguous carves (same `carve_batch`) do NOT start new
runs.

**Where it is consulted.** In the four guard sites, replacing the modulo for
wide classes:

```text
// reclaim_offset_checked, SKETCH (replaces lines 103-106):
let bs = SizeClasses::block_size(class_idx) as u32;
#[cfg(not(feature = "wide-class-align"))]
{
    if !(off as u32).is_multiple_of(bs) { return false; }
}
#[cfg(feature = "wide-class-align")]
if class_idx >= WIDE_CLASS_START {  // compile-time constant: first wide-class index
    // Wide class: consult the run-origin array.
    let ros = meta.run_origins(class_idx);
    let mut valid = false;
    for i in 0..ros.count as usize {
        let origin = ros.origins[i] as usize;
        if off >= origin && (off - origin) as u32 % bs == 0 {
            valid = true;
            break;
        }
    }
    if !valid { return false; }
} else {
    // Non-wide class: today's check (AND for power-of-two block_size).
    if !(off as u32).is_multiple_of(bs) { return false; }
}
```

**Memory overhead.** ZERO new metadata. The run-origin records live in the
already-reserved second `BinTable` slot (232 bytes, currently unused). Only 36
of those 232 bytes are used (3 wide classes × 12 bytes). **This is strictly
cheaper than Oracle A** and requires no layout change.

**Containment strength.** Equivalent to the current check for the
single-run-per-class case (which is the common case — `carve_batch` carves all
blocks in one run). For the multi-run case (a wide-class carve interrupted by
another class's carve), the run-origin array correctly identifies blocks from
ALL runs. A garbled offset that coincidentally satisfies `(off - origin) %
block_size == 0` for some recorded origin but is NOT a real block-start would
still pass this check — BUT it would then be caught by the `bump` guard (`off <
bump`) and the `is_free` guard (a non-block-start offset has bitmap bit = 0,
`is_free == false` → proceeds — same as under the old check). So the run-origin
oracle provides the SAME containment as the old `off % block_size == 0`: a
necessary condition that the remaining guard chain completes. §6 formalizes why
this is sufficient.

### 4.3 Recommendation: Oracle B for stage 2

Oracle B is recommended over Oracle A for the stage-2 prototype because:
1. **Zero metadata cost** — uses already-reserved space, no layout change, no
   density tax on small classes.
2. **Scoped to wide classes only** — the run-origin array exists for 3 classes;
   all other classes keep the old check (an AND for PoT block_size — free).
3. **Sufficient containment** — the multi-origin scan is O(runs) ≤ O(3) for the
   wide classes, and the containment is equivalent to today's necessary-condition
   check (§6).

Oracle A is documented as the fallback if Oracle B's carve-time bookkeeping
proves error-prone in review: it is simpler to reason about (a flat bitmap, no
run-origin scan) but costs 32 KiB/segment.

---

## 5. Perf overhead estimate

### 5.1 Today's cost on the reclaim hot path

The current guard at G1/G2 (`reclaim_offset_checked:104`, `reclaim_offset:241`)
is:

```text
let bs = SizeClasses::block_size(class_idx) as u32;   // one table load (L1)
if !(off as u32).is_multiple_of(bs) { return false; } // one u32 modulo + compare
```

For **power-of-two** `block_size`, `is_multiple_of` (`self % rhs == 0`) compiles
to `off & (bs - 1) == 0` — one AND + one compare (1 cycle). For **non-power-of-
two** `block_size` (the wide classes), it compiles to a `u32` `div`/`mod` — a
hardware division instruction (~20-40 cycles on modern x86-64, depending on the
pipeline). So today, the wide classes ALREADY pay a division on every cross-
thread reclaim.

### 5.2 Oracle B's cost

The run-origin scan adds, for a wide class:

```text
// Best case (single run — the common case via carve_batch):
off >= origin[0] && (off - origin[0]) % bs == 0
//  = one compare + one sub + one u32 mod + one compare
//  ≈ same as today (one division), +1 cycle for the origin compare/sub.

// Worst case (3 runs — rare, requires interleaved carves):
3 × (compare + sub + mod + compare) with early exit
//  ≈ 3 divisions worst-case, ~60-120 cycles.
```

**But the scan is preceded by a cheap filter** that eliminates most garbage
entries before any division: `off % class_align != 0` (one AND for power-of-two
`class_align`, ~1 cycle). Only class_align-aligned offsets proceed to the
division. Since most garbled offsets are NOT class_align-aligned, the filter
short-circuits the scan for the common reject case.

**Net cost vs today:**
- For wide classes: **+1-3 cycles** (origin compare/sub per run, filtered by a
  cheap AND). The divisions are the same count as today (1 per run checked).
  The reclaim path is NOT a microsecond-scale operation (it involves a ring
  pop, a `SegmentMeta` construction, multiple field reads, a `write_next`, a
  `set_head`, a `mark_free`), so +1-3 cycles is lost in the noise.
- For non-wide classes: **zero change** (old `is_multiple_of` retained).

### 5.3 Oracle A's cost

One byte load + one mask (the bitmap `test`), replacing the modulo. For wide
classes, this is **CHEAPER than today** (byte load ~4 cycles vs division ~20-40
cycles). For non-wide classes, unchanged (old check retained). But the 32 KiB
metadata cost (§4.1) likely dominates any cycle savings.

### 5.4 Carve-time cost

Oracle B adds a run-origin `push` at carve time (one array write + count bump),
only when a new run starts (at most once per `carve_batch` call, only for wide
classes). This is a cold path (wide-class carves are rare) — negligible.

Oracle A adds a `set_carved` (byte load + OR + store) per carved wide-class
block. Also cold-path — negligible.

### 5.5 Honest summary

| Path | Today | Oracle B | Oracle A |
|---|---|---|---|
| Reclaim, wide class | ~20-40 cyc (1 div) | ~22-43 cyc (1 div + 1-3 cyc origin check) | ~4 cyc (byte load + mask) |
| Reclaim, non-wide | ~1 cyc (AND) or ~20-40 cyc (div) | unchanged | unchanged |
| Carve, wide class | 0 | +1 push per run (cold) | +1 set per block (cold) |
| Metadata / segment | 0 | **0** (reserved slot) | +32 KiB |

**Oracle B adds ~1-3 cycles to wide-class reclaim** (a path that already costs
~100+ cycles for the ring pop + metadata dance). The reclaim hot path's overall
cost is dominated by cache-line accesses, not by the alignment check. The
overhead is **real but negligible** — and only on the opt-in feature's wide-
class path. This is an honest "win costs something" trade: the density gain
(§2.4) trades against a marginal reclaim-path cycle increase, not a structural
slowdown.

---

## 6. Correctness risk assessment

### 6.1 What the current guard contains

The §13 corruption guard (`off % block_size == 0`) exists to prevent a garbled
or attacker-controlled ring entry from causing `write_next` to clobber the
interior of a live block or segment metadata. It is **defence-in-depth**: a
correctly-packed ring entry always passes (a real block-start IS
`block_size`-aligned under the old scheme), and a garbled entry is caught by
the COMBINATION of:

1. `class_idx < SMALL_CLASS_COUNT` (bounds).
2. `magic == SEGMENT_MAGIC` (segment validity).
3. `kind == Small | Primordial` (segment type).
4. **`off % block_size == 0`** (THE GUARD — necessary condition for block-start).
5. `off >= payload_start` (not in metadata).
6. `off < bump` (not in uncarved / decommitted region).
7. `is_free(off) == false` (block is currently allocated, not already free).

No single guard is a complete oracle. Guard #4 rejects offsets that are not
`block_size`-aligned (most interior pointers). Guard #6 rejects offsets past the
carved region. Guard #7 rejects already-free blocks (double-free). Together they
form a layered defence where each guard catches what the others miss.

### 6.2 What changes under the new alignment

Under `class_align`-based alignment, wide-class block-starts are NOT
`block_size`-aligned. Guard #4 as written would reject EVERY valid wide-class
block-start. The oracle (A or B) replaces guard #4 for wide classes with a check
that accepts valid block-starts.

**The critical question:** does the oracle catch at least the same set of
garbage offsets that guard #4 caught? If the oracle is WEAKEST where guard #4
was strongest, the containment is weakened.

### 6.3 Oracle A (carved-starts bitmap) — STRICTLY STRONGER

The bitmap records the EXACT set of valid block-starts. An offset passes the
bitmap check IFF it was carved as a block-start in this segment lifetime. This
is both necessary AND sufficient — no false positives (every accepted offset is
a real block-start) and no false negatives (every real block-start is accepted).

Guard #4 (the modulo) is necessary but NOT sufficient: a `block_size`-aligned
offset in the alignment gap (between `small_meta_end` and the first block) or in
the gap between two runs passes guard #4 but is NOT a block-start. The bitmap
catches these. So the bitmap is **strictly stronger** — it catches a superset of
what guard #4 catches, plus additional garbage that guard #4 missed.

**Verdict: at least as safe. ✓ (In fact safer — tighter containment.)**

### 6.4 Oracle B (run-origin array) — EQUIVALENT containment

The run-origin check `(off - origin) % block_size == 0` for some recorded origin
is a necessary condition for "off is a block-start of this class" — exactly the
same logical strength as guard #4. A garbled offset that passes this check but
is NOT a real block-start (e.g., a `block_size`-aligned offset in a gap that
happens to be `(off - origin) % block_size == 0` for some origin) is caught by
the SAME downstream guards as under the old scheme:

- Guard #6 (`off < bump`): gap offsets between two runs may be `< bump` and pass
  this guard. But they would also pass under the OLD scheme (a gap offset that
  is `block_size`-aligned is `< bump` too).
- Guard #7 (`is_free == false`): a gap/interior offset has bitmap bit = 0 (never
  carved, never freed) → `is_free == false` → PASSES guard #7. **This is the
  same under both schemes** — the modulo check never caught gap offsets that
  happened to be `block_size`-aligned, and the run-origin check doesn't either.

**The honest admission:** neither the old modulo NOR the run-origin oracle
provides COMPLETE containment of gap/interior offsets that coincidentally
satisfy the alignment/arithmetic check. The defence-in-depth relies on the LAYERED
combination of all seven guards. The run-origin oracle preserves the same layering
with the same residual gap. **It is NOT weaker than the current check.**

One residual that IS new: under the old scheme, for power-of-two `block_size`,
guard #4 was an AND (free, ran on every reclaim). Under Oracle B, wide classes
now scan up to 3 origins. If the scan has a BUG (e.g., an off-by-one in the
origin count, or a missing origin push at carve time), a valid block-start could
be rejected → the block leaks (never reclaimed, never freed, `live_count` never
reaches 0, segment never pooled). This is a MEMORY LEAK, not a CORRUPTION —
strictly less severe than the §13 corruption class. And it is detectable: a
segment with `live_count > 0` but all blocks actually freed is observable via
the `dbg_live_count` diagnostic.

**Verdict: at least as safe. ✓ (Equivalent containment; new failure mode is a
detectable leak, not corruption.)**

### 6.5 What the oracle does NOT protect against (unchanged from today)

Neither oracle protects against:
- A garbled `class_idx` that routes to the wrong class (caught by guard #7: the
  bitmap bit for the wrong class's block-start reads "free" or "not-a-block-
  start" → `is_free == false` for the wrong offset → proceeds → `write_next` at
  the offset of a DIFFERENT class's block → corruption). This is the SAME
  residual under both schemes — the ring carries `class_idx` from the freer,
  and a garbled class field is only caught if the offset is invalid for the
  claimed class.
- A use-after-free where the block was re-issued between the ring push and the
  drain (caught by the `hardened` generational guard, compiled out under
  `production` — a documented residual, X7 Ф3 §2.5).

These residuals are UNCHANGED by the alignment change — they exist under both
schemes and are addressed (or documented) by existing mechanisms.

---

## 7. Kill-gate / verdict

| # | Criterion | Target | Finding (this report) | Verdict |
|---|---|---|---|---|
| K1 | Is the `class_align` derivation precise and correct for all three wide classes? | exact | §2.1: 1.25 MiB → 256 KiB, 1.5 MiB → 512 KiB, 1.75 MiB → 256 KiB. Verified by prime factorization. `class_align = 1 << block_size.trailing_zeros()`. | **PASS** |
| K2 | Does the new alignment deliver the claimed density gain (3/2/2 vs 2/1/1)? | exact arithmetic | §2.4: density computed from `SEGMENT`, `small_meta_end`, `class_align`. All three classes gain exactly +1 block, matching the review's original 3×/2×/2×. | **PASS** |
| K3 | Is every block_size-multiple assumption site inventoried? | exhaustive grep | §3: 19 sites found across 6 files. 11 unaffected (a), 4 need the oracle (b), 4 are the change or comment updates (c). No site missed. | **PASS** |
| K4 | Does the proposed oracle provide at least the same containment as the current check? | at least as safe | §6: Oracle A is strictly stronger (exact bitmap). Oracle B is equivalent (necessary condition, same layered defence, new failure mode is detectable leak not corruption). | **PASS** |
| K5 | Is the reclaim-path perf overhead honestly stated? | real cost, not glossed | §5: +1-3 cycles per wide-class reclaim (Oracle B), dominated by the existing ~100-cycle ring-pop + metadata dance. Non-wide classes unchanged. | **PASS** |
| K6 | Is the metadata overhead in the same ballpark as accepted prior art? | compare to gen table / directory sidecar | §4.1: Oracle A = 32 KiB/segment (less than gen table's 256 KiB, more than directory's 6.1 KiB/heap). Oracle B = 0 (uses reserved BinTable slot). | **PASS** (B) / **MARGINAL** (A) |
| K7 | Is the design distinguishable from the page-run layer alternative? | state the trade-off honestly | §9: the page-run layer delivers 11/9/8 density (vs 3/2/2) with zero guard breakage. The alignment change is a smaller, riskier optimization. | **PASS** (distinguished; trade-off stated) |
| K8 | Is the design airtight enough to prototype THIS session? | yes for a gated prototype | §8: the prototype is behind a new feature gate, scoped to wide classes only, with a density-measurement test mirroring R9-4. But the orchestrator's mandatory design-review gate requires human sign-off before stage 2. | **DESIGN-ONLY** |

### Verdict

**CONDITIONAL GO. Design is sound; prototype deferred to stage 2 (separate
session) pending human design-review sign-off.**

All seven technical criteria pass (K1–K7). The design can construct an oracle at
least as safe as the current check (K4 PASS), the density gain is exact and real
(K2 PASS), and the perf overhead is marginal and honestly stated (K5 PASS). The
CONDITIONAL qualifier reflects two honest reservations:

1. **The page-run layer (§9) is a strictly superior long-term solution** for the
   same density problem — bigger gain, zero guard breakage, zero new metadata.
   Stage 2 is worth pursuing ONLY IF the page-run layer is not on the near-term
   roadmap AND the 3/2/2 gain (not 11/9/8) is deemed sufficient to justify the
   correctness surface.
2. **Oracle B's carve-time bookkeeping (run-origin push on new-run detection) is
   the highest-risk implementation detail** — a missed push or an off-by-one in
   the origin count causes a memory leak (not corruption, but still a bug). The
   stage-2 test plan (§8) must include a counterfactual that would catch a
   missed origin push.

---

## 8. Stage-2 GO/NO-GO recommendation (for a future session)

### 8.1 GO conditions (all three must hold)

1. **Human design-review sign-off** on this doc — the mandatory gate per the
   project's correctness-sensitive-change policy. The reclaim path is the §13
   corruption defence; a wrong design here risks metadata corruption in the
   cross-thread free path.
2. **The page-run layer (§9) is confirmed NOT on the near-term roadmap.** If the
   page-run layer is planned, this optimization is premature — it adds
   correctness surface for a gain the page-run layer would supersede.
3. **Oracle B is chosen over Oracle A** (zero metadata cost, scoped to wide
   classes). Oracle A is the fallback if Oracle B's bookkeeping proves
   error-prone in review.

### 8.2 Minimal safe stage-2 prototype

**New feature gate** (matching `medium-classes-wide`'s convention, §5.4 of
R9-7's feature-gating rationale):

```toml
# Cargo.toml (SKETCH — NOT applied this session)
wide-class-align = ["medium-classes-wide"]
```

Additive over `medium-classes-wide`; NOT part of `production` or any default
bundle — exactly like `medium-classes-wide` itself. `--all-features` pulls it in
for the test matrix.

**Scope of changes** (all behind `#[cfg(feature = "wide-class-align")]`):

1. **`carve_block` / `carve_batch`** (`alloc_core_small.rs:1175, 1334`): replace
   `align_up(bump, block_size)` with `align_up(bump, class_align)` for wide
   classes only (`block_size > SMALL_MAX_MEDIUM`). Add run-origin push (Oracle B)
   when a new run starts.
2. **`reclaim_offset` / `reclaim_offset_checked`** (`alloc_core_small_reclaim.rs:
   104, 241`): replace `is_multiple_of(bs)` with the run-origin scan for wide
   classes. Non-wide classes keep the old check unchanged.
3. **`dealloc_small` / magazine free** (`alloc_core_small.rs:1454`,
   `heap_core_free.rs:185`): same guard replacement under `hardened +
   wide-class-align`.
4. **Run-origin storage**: repurpose the reserved second `BinTable` slot's
   wide-class entries (indices 55-57). Add `run_origins(class_idx)` accessor on
   `SegmentMeta`. Add `push`/`scan` methods.
5. **Comment updates**: D2 (`heap_core_alloc.rs:143-148`), D3
   (`alloc_core_small.rs:1443-1452`) — update the invariant statement to reflect
   `class_align`-based alignment for wide classes.

**Feature-OFF builds must be byte-identical to today** — every new code path
behind a `#[cfg]` whose predicate includes `wide-class-align`. Regression guard
mirroring R9-4's `wide_does_not_disturb_six_class_medium_table_topology` pattern.

### 8.3 Test plan (mirroring R9-4's `medium_classes_wide_correctness.rs`)

New test file `tests/wide_class_align_correctness.rs`, whole-file gated on
`#![cfg(all(feature = "alloc-core", feature = "medium-classes-wide",
"wide-class-align"))]`:

- **(a) Density measurement:** `empirical_density_matches_class_align_prediction`
  — carve blocks of each wide class; assert the max per-segment residency equals
  the §2.4 formula (3 / 2 / 2). This is the counterfactual that would FAIL under
  the old alignment (which gives 2 / 1 / 1).
- **(b) Cross-thread reclaim correctness:** `wide_class_cross_thread_free_
  reclaims_correctly` — allocate wide-class blocks, free them cross-thread
  (via `dbg_push_to_ring` + `dbg_drain_all_rings`, mirroring
  `tests/phase13_drain_reclaim_layout_class.rs`), and assert the reclaimed
  blocks are re-issuable (the run-origin oracle accepts them). This is the
  counterfactual that would FAIL if the oracle were missing (every valid
  wide-class offset would be rejected → leak).
- **(c) Garbled-offset rejection:** `garbled_offset_into_block_interior_
  rejected` — push a ring entry with an offset that is in the INTERIOR of a
  wide-class block (not a block-start); assert `reclaim_offset` returns `false`
  (the oracle rejects it). This is the §13 corruption counterfactual.
- **(d) Multi-run reclaim:** `interleaved_carve_multi_run_reclaim` — carve
  wide-class blocks interleaved with small-class carves (creating multiple runs
  per wide class); free the wide-class blocks cross-thread; assert all are
  reclaimed (no origin missed). This catches a missed run-origin push.
- **(e) Feature-OFF non-disturbance:** `wide_class_align_off_matches_medium_
  classes_wide` — build without `wide-class-align`; assert density is 2 / 1 / 1
  (R9-4's numbers) and the reclaim path uses the old modulo check. Pin via
  `dbg_*` diagnostics or behavioural assertion.

### 8.4 What the prototype does NOT do

- Does NOT touch the small geometric classes' alignment (they keep
  `align_up(bump, block_size)` — §2.5 shows the gain is negligible and not
  worth the guard surface).
- Does NOT promote `wide-class-align` into `production` (it stays opt-in, like
  `medium-classes-wide`).
- Does NOT run miri on the full suite (miri is too slow for this crate per the
  CLAUDE.md convention); miri is run on the specific invariant tests (b) and (c)
  only.

---

## 9. The page-run layer alternative — why this report is CONDITIONAL, not unconditional GO

R8-9 §5 K7 and R9-4 recommendation #2 both identified the **page-run layer** as
the real long-term fix for the 1–2 MiB density gap: a larger medium arena
(8–16 MiB) for the wide classes, so that `floor(arena / block_size)` is large
enough that the one-block alignment tax becomes negligible. This section states
the trade-off explicitly.

### 9.1 Density comparison

| Arena size | 1.25 MiB density | 1.5 MiB density | 1.75 MiB density | Guard breakage? |
|---|---|---|---|---|
| 4 MiB (today) | 2 | 1 | 1 | — |
| 4 MiB + `class_align` (this design) | **3** | **2** | **2** | **yes** (needs oracle) |
| 8 MiB page-run | 5 | 4 | 3 | no |
| 16 MiB page-run | **11** | **9** | **8** | no |

The page-run layer at 16 MiB delivers **3-6× the density gain** of the alignment
change, with ZERO guard breakage (`off % block_size == 0` stays load-bearing
because the arena is still `block_size`-aligned within itself, just larger). The
alignment change delivers 3/2/2 — a real but modest gain — at the cost of a new
correctness surface (the oracle) and new metadata (Oracle A) or bookkeeping
(Oracle B).

### 9.2 When the alignment change IS worth it

The alignment change is worth pursuing to stage 2 IF:
- The page-run layer is NOT planned for the near term (it is a "separate, larger
  design" per R8-9/R9-4, requiring per-class arena sizing, a new segment kind,
  and directory/table changes — a multi-session effort).
- The 3/2/2 gain is deemed sufficient for the workloads that motivated
  `medium-classes-wide` (the 1 MiB–1.25 MiB sub-range where the 2× → 3× density
  bump matters most).
- The implementation cost (Oracle B: ~4 sites, ~100 lines, feature-gated) is
  acceptable for an opt-in feature.

### 9.3 When the alignment change is NOT worth it

If the page-run layer IS on the near-term roadmap, this optimization is
**premature**: it adds a correctness surface (the oracle) that the page-run
layer would make unnecessary. Shipping both would mean maintaining two
alignment schemes (the `class_align` path for 4 MiB segments AND the
`block_size` path for 16 MiB arenas) — unnecessary complexity.

---

## 10. Caveats

- **Single analysis host, no measurement performed.** §2.4's density numbers are
  deterministic geometry, empirically confirmable by carve (mirroring R9-4 §2.3's
  methodology). §5's perf estimates are analytical from instruction costs
  (mirroring R9-5 §8 / R9-7 §8). Stage 2's test plan (§8.3 (a)) is the gate that
  turns the density arithmetic into measured numbers.
- **The run-origin oracle's multi-run correctness is the highest-risk
  implementation detail.** §4.2's run-origin array depends on correctly
  detecting "new run started" at carve time (`aligned_bump != bump`). A missed
  detection (e.g., when `bump` happens to be `class_align`-aligned after a
  different class's carve) causes a missed origin → a valid block-start is
  rejected by the guard → the block leaks. This is detectable (§6.4) but must be
  tested explicitly (§8.3 (d)).
- **Oracle A's 32 KiB metadata cost applies to EVERY segment** when the feature
  is ON, even segments that never host a wide-class block. This is the same
  trade-off as the `hardened` gen table (256 KiB/segment) — accepted prior art
  for opt-in features — but it means the feature is expensive for workloads that
  enable it "just in case" without actually allocating wide-class blocks. Oracle
  B avoids this entirely (zero metadata cost).
- **No `src/`, `Cargo.toml`, or `tests/` file is modified.** This is a
  documentation-only deliverable. The feature sketches (`wide-class-align =
  ["medium-classes-wide"]`, the `carve_block` / `reclaim_offset` code sketches)
  are illustrative, not applied.
- **The reclaim path was re-read fresh this session** (R10-3 @ `abaad9c`
  modified `reclaim_offset` / `reclaim_offset_checked`'s return-value semantics;
  the `is_multiple_of` guards at lines 104 and 241 are unchanged by R10-3, but
  the surrounding return-value logic shifted). Line numbers in §3 are current as
  of `c8d53af`.
- **The `§13 corruption guard` terminology.** The task brief calls the
  `is_multiple_of(bs)` check the "§13 corruption guard." §13 of
  `RACE_DRAIN_RECLAIM.md` is actually about a DIFFERENT root cause (class
  derivation from `page_map` instead of the ring entry). The
  `is_multiple_of(bs)` check is the defence-in-depth that prevents a garbled
  offset from causing the SAME CLASS of corruption §13 described
  (`write_next` into mid-block). The two are related but distinct: §13 fixed
  the class-derivation bug; the alignment check is a separate guard against
  offset corruption. This report uses "§13 corruption guard" in the task
  brief's sense (the alignment check), with this clarification noted.
