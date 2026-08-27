# size-classes consumer-integration review — does `src/alloc_core/size_classes.rs` satisfy every documented precondition?

Reviewer: `size-classes-review2-consumer-integration` (read-only static review, 2026-08-26).
Scope: the ONE real consumer — the shim `src/alloc_core/size_classes.rs` and every call site
that reaches `SizeClasses::class_for` / the raw LUT from the allocator's hot paths. NOT in
scope: the `size-classes` crate's own publication readiness (covered by sibling reviews);
regression-hunting in wave 1's fixes (`size-classes-review2-regression-hunt`).

## Verdict

**GO** — for INTEGRATION correctness. All four documented preconditions are satisfied by
the real consumer, with independently-verified evidence (not the shim's own comments) for
each. Three documentation-level findings (2 × P3, 1 × accepted-risk note) — none is a
behavioral defect; the integration is sound as built.

Method note: everything below was verified by reading the cited lines in this working tree.
No build/test/clippy was run (read-only mode). "N sites" claims are backed by the pasted
`rg` output in the appendix.

---

## The call-site inventory (basis for everything below)

Every production path into the classifier. `rg -n "::class_for\(" src/` (code lines only)
yields 11 consumer call sites plus 2 forwarder bodies; `rg -n "Self::classify\(" src/`
yields 4 more that reach `class_for` via `AllocCore::classify` (`alloc_core.rs:2552-2556`):

| # | Site | size source | align source |
|---|------|-------------|--------------|
| 1 | `alloc_core.rs:2252` (OPT-F realloc) | `old_size` = `old_layout.size().max(MIN_BLOCK)` (:2248) | `old_layout.align()` (:2249) |
| 2 | `alloc_core.rs:2253` (OPT-F realloc) | `clamped_new` = `new_size.max(MIN_BLOCK)` (:2250) | `old_layout.align()` |
| 3 | `alloc_core.rs:2553` (`AllocCore::classify`) | callers clamp, see #9–#12 | callers pass `layout.align()` |
| 4 | `heap_core_alloc.rs:79` (`HeapCore::alloc`) | `layout.size().max(MIN_BLOCK)` (:67-69) | `layout.align()` (:74) |
| 5 | `heap_core_alloc.rs:548` (`alloc_zeroed`) | `layout.size().max(MIN_BLOCK)` (:544-546) | `layout.align()` (:547) |
| 6 | `heap_core_alloc.rs:884` (`alloc_batch`) | `layout.size().max(MIN_BLOCK)` (:882) | `layout.align()` (:883) |
| 7 | `heap_core_dealloc_batch.rs:192` | `layout.size().max(MIN_BLOCK)` (:190) | `layout.align()` (:191) |
| 8 | `heap_core_free.rs:315` (`dealloc_own_thread_with_base`) | `layout.size().max(MIN_BLOCK)` (:307) | `layout.align()` (:308) |
| 9 | `heap_core_free.rs:956` (realloc A1 drain) | `new_size.max(MIN_BLOCK)` (:957) | `old_layout.align()` (:958) |
| 10 | `heap_core_free.rs:1052` (medium promotion gate) | `old_layout.size().max(MIN_BLOCK)` (:1053-1055) | `old_layout.align()` (:1056) |
| 11 | `heap_core_xthread.rs:988` (remote free, variant 2) | `layout.size().max(MIN_BLOCK)` (:984-986) | `layout.align()` (:988) |
| 12 | `alloc_core.rs:1514` (`AllocCore::alloc`) | `layout.size().max(MIN_BLOCK)` (:1512) | `layout.align()` (:1513) |
| 13 | `alloc_core.rs:1547` (`alloc_zeroed`) | `layout.size().max(MIN_BLOCK)` (:1545) | `layout.align()` (:1546) |
| 14 | `alloc_core.rs:1901` (`AllocCore::dealloc`, small branch) | `layout.size().max(MIN_BLOCK)` (:1899) | `layout.align()` (:1900) |
| 15 | `alloc_core_core_diag.rs:648` (`dbg_layout_class_for`, test-only) | `layout.size().max(MIN_BLOCK)` (:647) | `layout.align()` (:648) |

Forwarder bodies (not themselves call sites): the shim `size_classes.rs:228-230`
(`SC.class_for`) and the public re-export `segment_layout.rs:88-90`.

**The raw LUT is never indexed in `src/`.** `rg "SIZE2CLASS\[|size2class\[|\.size2class\("`
over the repo returns, for the consumer crate, only doc-comment mentions
(`size_classes.rs:174`, `segment_layout.rs:66/:80`) — zero code uses of
`SizeClasses::size2class()` or `SIZE2CLASS[..]`. Direct LUT indexing exists only in tests
(`tests/size_classes_lookup.rs`, `tests/size_classes_slow_path_equivalence.rs:157`, indexed
as `(128 - 1) >> MIN_BLOCK_SHIFT`, in-contract) and in the crate's own test suite (crate
scope). So the crate's raw-accessor preconditions (`size >= 1`, `size <= small_max`) have
no production surface to violate.

---

## Precondition 1 — base-address alignment: **VERIFIED SOUND** (with one doc finding)

The crate cannot check that block 0's address is `align`-aligned; it explicitly warns the
address that matters is block 0's, "not the span's OS reservation base, if the two differ"
(`crates/size-classes/src/lib.rs:717-726`). In sefer they DO differ (metadata prefix), so
this is the precondition that needed real tracing. It holds, via a two-part chain:

### Part A — every small/primordial segment base is 2^22-aligned, by construction

- `SEGMENT = 1 << 22` (`os.rs:65`). Every reservation path passes `SEGMENT` as the
  alignment to `aligned-vmem`:
  - `Segment::reserve` → `vmem::reserve_aligned(usable, SEGMENT)` (`os.rs:152`);
  - `reserve_exact` (`exact-span-large`) → `vmem::reserve_aligned(len, SEGMENT)` (`os.rs:207`);
  - `reserve_lazy`/`reserve_capacity_lazy`/`reserve_lazy_for_measurement` →
    `vmem::reserve_aligned_lazy(_, SEGMENT, _)` (`os.rs:272/:317/:344`);
  - `numa::reserve_aligned_on_node` (`numa.rs:70-94`): `NO_NODE` → plain
    `reserve_aligned` (:78); a real node on Linux/x86_64/aarch64 → the shim's
    `reserve_preferred_on_node`, which **delegates the reservation itself to
    `aligned_vmem::try_reserve_aligned(size, align)` and only `mbind`s the result**
    (`crates/numa-shim/src/lib.rs:1403-1416`); every other platform/miri returns
    `UnsupportedPlatform`/`UnsupportedArchitecture` (`numa-shim` :2282-2346), which
    `numa.rs:88-93` degrades to plain `reserve_aligned`. **No NUMA path exists that
    reserves with a weaker alignment.**
- What `reserve_aligned` actually guarantees (verified in the backends, not from docs):
  - **64-bit Unix** (the only shape sefer uses; the 32-bit exact-size fast path is
    `target_pointer_width = "32"`-gated, `unix.rs:181`): one `mmap(size + align)`, then
    `base_addr = align_up_addr(region_addr, align)` (`unix.rs:202-260`) — alignment is
    arithmetic, not an OS promise. The lazy variant forwards to the same function
    (`unix.rs:575-581`). (The kernel-guaranteed huge-page fast path `unix.rs:144-165`
    adds a real runtime `is_multiple_of(align)` check anyway, and sefer never calls
    `reserve_aligned_huge` — no `reserve_aligned_huge` reference anywhere in `src/`.)
  - **Windows**: `align = 4 MiB > WIN_ALLOCATION_GRANULARITY (64 KiB)`, so the single-call
    fast path (`windows.rs:112`) is structurally unreachable for sefer's reservations; the
    two-call path over-reserves and computes `base = align_up_addr(region_addr, align)`
    (`windows.rs:282-302`). The single-call path, if a smaller `align` ever took it,
    carries an unconditional runtime alignment check with fall-through
    (`windows.rs:180-190`, task #917/H2C6).
  - **miri**: `std::alloc::alloc(Layout::from_size_align(size, align))` (`miri.rs:18-21`)
    — alignment guaranteed by the `std` allocation contract.

### Part B — every small-path block address is `base + m·block_size` in ABSOLUTE coordinates

This is the half the shim's doc does not state (see finding F1), and it is load-bearing:

- Fresh segments initialize the bump cursor at `SegLayout::small_meta_end()`
  (`alloc_core_small.rs:2086-2098`); the primordial writes `bump = 0` and fixes it up past
  its metadata (`bootstrap.rs:126-134`). `bump` is a byte offset FROM THE SEGMENT BASE
  (`segment_header.rs:317-322`; the carve bounds check `aligned_bump + block_size >
  SEGMENT` at `alloc_core_small.rs:1453` confirms absolute semantics).
- `carve_block` does `aligned_bump = align_up(bump, block_size)` and returns
  `Node::deref(segment, aligned_bump)` (`alloc_core_small.rs:1451-1452, 1568`) — i.e. the
  block address is the 2^22-aligned base plus an ABSOLUTE multiple of `block_size`,
  regardless of where in the segment the metadata ended. `carve_batch` is the same
  (`align_up(bump, block_size)` at :1633, blocks at `aligned_start + i·block_size`, :1718).
- Every reuse path preserves that property: `dealloc_small` rejects offsets not a multiple
  of the class's `block_size` (`alloc_core_small.rs:1769`); the remote-ring
  `reclaim_offset` re-checks `off % block_size == 0` (`alloc_core_small_reclaim.rs:130-133`);
  `pop_free` hands back exactly the stored segment-relative offset
  (`alloc_core_small.rs:1215-1219`); magazine slots hold pointers obtained from those same
  paths.
- `class_for` returns a class only when `block_size % align == 0` and `need = max(size,
  align) <= SMALL_MAX` (crate `lib.rs:737-784`). Max servable pow2 `align` per build:
  16 KiB (default, class 16384 in `EXTRAS`, `size_classes.rs:98`), 1 MiB
  (`medium-classes`, the 1 MiB class, :115), 1 MiB (`medium-classes-wide` — 2 MiB > its
  SMALL_MAX of 1.75 MiB is rejected). Every one divides 2^22.

Therefore `ptr % align == (base + m·b) % align == 0`, since `align | b` and `align | 2^22`.
**Verified for every build config and every reservation path, NUMA included.**

Why Part B is load-bearing (the counterfactual): `small_meta_end()` is aligned **only to
`PAGE` (4 KiB)** — `segment_header_layout.rs:93-95/:108-110`, deliberately ("the TIGHT
metadata boundary"), with no assert of any larger alignment. If `carve_block`'s absolute
`align_up(bump, block_size)` were removed, blocks would sit at
`base + small_meta_end + k·block_size`, and any build where `small_meta_end % 16384 != 0`
would silently misalign every `align = 8192/16384` request (default build!) — the segment
base's 4 MiB alignment alone would NOT save it. The guarantee is real today, but it is
delivered by `carve_block`, not by the reservation alone.

Adjacent (outside the strict precondition, checked for completeness): requests with
`align > SMALL_MAX` fall to `alloc_large`, which honors `align` independently — payload at
`align_up(size_of::<SegmentHeader>(), align.max(PAGE))` from a SEGMENT-aligned base
(`alloc_core_large.rs:144-147`), and `align >= SEGMENT` is rejected with null as a legal
alloc-failure (`alloc_core_large.rs:134-136`, task #130). The medium→Large promotion case
(a small-classifying dealloc layout on a Large-segment block, `heap_core_free.rs:326-354`)
frees via the kind-based Large branch — no small-path carving ever happens in Large
segments.

### F1 — P3, doc (conclusion true, stated proof incomplete + one inverted relation)

`src/alloc_core/size_classes.rs:51-58` (M4): "It yields an aligned block ADDRESS because
small segments are additionally reserved with a base aligned to `SEGMENT` … which divides
every power-of-two `align` this scheme ever serves".

Two problems, neither behavioral:
1. As an argument this is insufficient: the reservation base is NOT the carve base. Block
   0's address is `base + align_up(small_meta_end…, block_size)`; the load-bearing second
   fact (Part B above — `carve_block`'s absolute `align_up` to `block_size`) is exactly
   what the doc omits, and it is what makes the 4-KiB-only alignment of `small_meta_end`
   harmless. A reader "optimizing" `carve_block` on the strength of this comment would
   introduce a real misalignment bug (see counterfactual above).
2. "which divides every power-of-two `align` this scheme ever serves" is the inverted
   relation: 4 MiB does not divide 16 KiB; every served `align` divides 4 MiB.

Recommended fix: state the actual invariant — "block addresses are
`SEGMENT`-aligned-base + an absolute multiple of `block_size` (`carve_block` aligns the
bump cursor to `block_size` in segment-relative coordinates), and every `align` the scheme
resolves divides `block_size` and `SEGMENT`". (Possible overlap with
`size-classes-review2-holistic`; reported here because the text lives in the consumer
shim.)

---

## Precondition 2 — `size >= 1` / `need = max(size, align) >= 1`: **VERIFIED SOUND**

- Every one of the 15 consumer call sites (table above) clamps the size operand with
  `.max(MIN_BLOCK)` (16) before classifying — 16 clamp sites listed in the appendix,
  matching the 15 sites plus the Large-realloc pair at `alloc_core.rs:2210-2211`. The
  clamp list and the call-site list were produced by the same `rg` pass and cross-check.
- A zero-size `Layout` (legal to construct: `Layout::from_size_align(0, 4)`) therefore
  reaches `class_for` as `class_for(16, align)` — in-contract. The `(0, 0)` double
  violation is unreachable from production: `align` always comes from a `Layout`, whose
  `align` is `>= 1` by type invariant, so `need >= 16 >= 1` on every path. The `GlobalAlloc`
  face (`src/global/sefer_alloc.rs:973-1125`) forwards `Layout` objects only; `realloc`'s
  `new_size` (possibly 0) is clamped at every downstream use, and the move leg rebuilds a
  `Layout` via `Layout::from_size_align(new_size, old_layout.align())` with the `Err` arm
  returning null (`alloc_core.rs:2035-2038`) before `self.alloc` clamps.
- The raw LUT (`size2class()` / `SIZE2CLASS[...]`) — the accessor whose `size >= 1`
  precondition is sharpest — has **zero production call sites** (verified by `rg`, see
  inventory). Its only in-repo users are tests indexing `(size - 1) >> MIN_BLOCK_SHIFT`
  with `size >= 128`.

### F2 — P3, doc/contract nit (test surface, no production impact)

`tests/medium_classes_correctness.rs:192` drives the public re-export as
`SegmentLayout::class_for(1, align)`. `size = 1` is below `MIN_BLOCK`, violating the
re-export's own documented contract ("`size` must be `>= MIN_BLOCK` (the caller's
contract…)", `segment_layout.rs:78-79`) — even though it stays inside the crate's actual
well-defined domain (`need = max(1, align) >= 1`, per the crate's `class_for` doc). The
shim doc and the re-export doc state the STRICTER contract than the mechanism requires;
either relax both to the crate's real contract (`size >= 1` given `align >= 1`, with
`size >= MIN_BLOCK` merely the convention of allocator entry points) or clamp in the test.
Today nothing can go wrong — but the written contract and the in-tree test disagree, which
is precisely the kind of drift this repo's audit history flags.

---

## Precondition 3 — `static`, not `const`: **VERIFIED SOUND**

- `pub(crate) static SIZE2CLASS: [u8; S2C_LEN]` (`size_classes.rs:186`) and
  `static SC: SizeClassesImpl<TABLE_LEN, S2C_LEN>` (`size_classes.rs:198`) are both
  declared `static` **unconditionally** — the file's only `#[cfg]`s are on the three
  `EXTRAS` variants (:97-137), which change table VALUES, never the `static` keyword.
- Nothing downstream re-derives a const copy: `rg "build_size2class|build_table|SizeClassesImpl|SC\."`
  over `src/` hits only the shim itself — it is the single instantiation of the crate's
  builder. `SegmentLayout::SIZE_CLASS_TABLE` / `::SIZE2CLASS` (`segment_layout.rs:63/:75`)
  are `const` REFERENCES to the shim's items (`&'static [..] = &…`), i.e. one promoted
  copy each, not value copies. `kani_proofs.rs:307/:346` imports only scalars
  (`SMALL_CLASS_COUNT`, `MIN_BLOCK`, `MIN_BLOCK_SHIFT`). `SC.` is referenced only by the
  shim's three forwarders (`class_for`/`block_size`/`is_huge`, :229/:239/:247).
- The known double `.rodata` copy (`SC`'s embedded table + the separate `SIZE2CLASS`
  static) is documented as a deliberate trade-off in the shim itself (:183-185). No
  regression to `const` in any feature combination.

---

## Precondition 4 — `align` power-of-two before `class_for`: **VERIFIED SOUND**

- All 15 consumer call sites pass `layout.align()` / `old_layout.align()` — a
  `core::alloc::Layout`'s align, a power of two by type construction. No site computes an
  `align` by hand: the inventory table's align column is exhaustive (`rg`-verified), and
  the only place a `Layout` is synthesized on the path (`alloc_core.rs:2035`) reuses
  `old_layout.align()`.
- The `GlobalAlloc` impl (`sefer_alloc.rs:975/:994/:1107` and `alloc_zeroed`) forwards
  caller `Layout`s verbatim; per the `GlobalAlloc` contract those aligns are pow2.
- History confirms this precondition is not hypothetical: `tests/medium_classes_correctness.rs`
  passed non-pow2 medium sizes (320/384/768 KiB) as `align` until task #779 (CHANGELOG
  0.3.0 entry) tripped the new `debug_assert!`; the fix (:179-186) now filters
  `MEDIUM_SIZES` by `is_power_of_two()` for the align axis. That was the only such site;
  no in-tree caller today passes a non-pow2 align.

### N1 — accepted-risk note (public API surface, not an integration defect)

`SegmentLayout::class_for` is re-exported at the crate root (`lib.rs:411`), so an EXTERNAL
consumer can pass a non-pow2 `align`: debug builds panic in the crate's `debug_assert!`,
release builds silently compute a wrong class (the crate's documented, deliberate stance —
wrong class choice, never UB inside the crate). The in-tree consumer never does this. No
action required for integration GO; worth remembering if `SegmentLayout`'s surface is
ever widened.

---

## Findings summary

| ID | Severity | Location | Summary |
|----|----------|----------|---------|
| F1 | P3 (doc) | `src/alloc_core/size_classes.rs:51-58` | M4 alignment justification incomplete (omits `carve_block`'s absolute `align_up` — the load-bearing half) + inverted "divides" relation. Conclusion still true. |
| F2 | P3 (doc) | `segment_layout.rs:78-79` vs `tests/medium_classes_correctness.rs:192` | Re-export documents `size >= MIN_BLOCK`; the in-tree test passes `size = 1` (valid per the crate's real `need >= 1` contract, invalid per the written one). Align the doc or the test. |
| N1 | note | `segment_layout.rs:88` / `lib.rs:411` | Public `class_for` re-export exposes the crate's non-pow2-`align` release-mode silence to external users. Accepted crate stance; in-tree consumer unaffected. |

No P0/P1/P2 integration findings. All four preconditions verified sound with independent
evidence trails.

## What I did NOT do

- No `cargo` commands of any kind (read-only mode); all claims are from reading source.
- Did not audit the `size-classes` crate's internals beyond the precondition docs quoted
  here (sibling reviews' scope), nor wave-1 fix regressions (`regression-hunt`'s scope).
- The `SegmentLayout::SIZE2CLASS[k]` semantics doc drift already reported by
  `claude-2026-08-26-1742.md` is not re-reported (out of my scope; still open as far as I
  can see — noting the overlap only).

## Appendix — countable evidence

Call sites (code lines only, `rg -n "::class_for\(" src/ | rg -v "///|//!"`):

```text
src/alloc_core/alloc_core.rs:2252, 2253, 2553
src/registry/heap_core_alloc.rs:79, 548, 884
src/registry/heap_core_dealloc_batch.rs:192
src/registry/heap_core_free.rs:315, 956, 1052
src/registry/heap_core_xthread.rs:988
src/alloc_core/segment_layout.rs:89          (forwarder body)
```

`Self::classify` reachers: `alloc_core.rs:1514, 1547, 1901`; `alloc_core_core_diag.rs:648`.

Clamp sites (`rg "max" … | rg MIN_BLOCK`): `alloc_core.rs:1512, 1545, 1899, 2210, 2211,
2248, 2250`; `heap_core_alloc.rs:69, 546, 882`; `heap_core_free.rs:307, 957, 1055`;
`heap_core_dealloc_batch.rs:190`; `heap_core_xthread.rs:986`;
`alloc_core_core_diag.rs:647` — 16 sites covering every size operand above.

Raw-LUT non-use in `src/`: `rg "SIZE2CLASS\[|size2class\[|\.size2class\("` over the repo
returns consumer hits only in doc comments (`size_classes.rs:174`,
`segment_layout.rs:66, 80`); all code hits are in `crates/size-classes` itself and
`tests/`.

Single-instantiation check (`rg "build_size2class|build_table|SizeClassesImpl|SC\." src/`):
all hits inside `src/alloc_core/size_classes.rs` (declaration + the three `SC.` forwarder
bodies) plus doc mentions.
