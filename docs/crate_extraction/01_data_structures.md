# Crate-extraction survey — data-structure & algorithmic primitives

**Lane:** data structures / size classes / algorithmic primitives (one of 4 parallel
research reports; siblings cover concurrency primitives, OS/platform, test-infra).
**Date:** 2026-07-16. **Read-only survey** — file/line references are against the
working tree at time of writing; note that `src/alloc_core/alloc_core_{large,small}*.rs`
were mid-refactor (untracked split of `alloc_core.rs`) and are treated as unstable.

**Already extracted** (do not re-propose): `aligned-vmem` (`crates/vmem`),
`sefer-region` (`crates/region`), `numa-shim` (`crates/numa`), `malloc-bench`
(`crates/malloc-bench`). The precedent that matters: `aligned-vmem` was extracted
*because* its entire purpose is a confined concern with standalone community value —
the bar the candidates below are measured against.

---

## 1. Size-class table + const-built O(1) `SIZE2CLASS` lookup — **extract**

**What / where.** `src/alloc_core/size_classes.rs` (~500 lines, single file).
Three cooperating pieces, all `const`-evaluated:

- `build_table()` — a `const fn` sorted-merge of a 1.25×-geometric progression
  (40 classes), 8 explicit page-aligned classes (512 B–16 KiB), the exact 256 B
  class, and (feature-gated) 6 medium classes up to 1 MiB → `SIZE_CLASS_TABLE`.
- `build_size2class()` — derives the O(1) size→class lookup (`static SIZE2CLASS:
  [u8; SMALL_MAX/MIN_BLOCK + 1]`) from the table at compile time with the
  monotone-pointer technique (O(buckets + classes) const-eval), with a
  compile-time `assert!(SMALL_CLASS_COUNT < 256)` pinning the `u8` entry type.
- `SizeClasses::class_for(size, align)` — O(1) fast path for `align ≤ 16`, and a
  provably-equivalent *jump* slow path for larger alignments (round `block` up to
  the next multiple of `align` via bitmask, re-seed through `SIZE2CLASS` — skips
  runs of non-divisible classes instead of stepping by 1).

**Coupling.** Nearly zero — pure safe `const` integer arithmetic, no memory
touches, no `unsafe`. Only two threads to cut: `HUGE_THRESHOLD = super::os::SEGMENT`
(a policy constant, trivially parameterizable) and the doc-level invariant
`MIN_BLOCK >= node::NODE_SIZE` (becomes a caller-side `const` assert). `AllocKind`
is allocator-flavoured but optional.

**Effort: LOW–MEDIUM.** The mechanism extracts as-is in a day. The *worthwhile*
extra work is generalizing: today the table shape (40 geo + extras, MIN_BLOCK=16,
1.25× step) is hardcoded. A community crate wants a `const`-generic builder —
"give me a mimalloc-style class table for (min_block, growth num/den, max) plus
explicit extra classes, and derive the O(1) lookup + the alignment-aware
classifier". Rust `const fn` is now capable of all of it (this file proves it).

**Testability gain: HIGH, and the tests already exist as integration tests** —
`tests/size_classes_lookup.rs`, `tests/size_classes_proptest.rs`,
`tests/size_classes_slow_path_equivalence.rs` (the counterfactual proving the
jump path ≡ the step-by-1 walk) move over almost verbatim. Extracted, the table
can also be property-tested against arbitrary parameterizations, which the
in-tree version cannot (its constants are baked).

**Community value: HIGH.** Every slab/pool/arena allocator reinvents exactly
this trio (class table, O(1) size→class, alignment-aware classification), and
most reinvent it with a runtime loop or a wrong alignment story. Nothing on
crates.io offers "const-built mimalloc-style size classes with a derived O(1)
lookup and an alignment-divisibility classifier" as a standalone unit. The
alignment slow path (the #114/B1 fix: `align > min_block` still resolving to a
small class via divisibility) is genuinely novel packaging — the failure mode it
fixes (every `align ≥ 512` request silently falling to a whole-segment path) is
a real bug class in hand-rolled allocators.

**Suggested crate: `size-classes`** (or `const-size-classes`).
API sketch:

```text
const SC: SizeClasses<{ params }> = SizeClasses::build(Params {
    min_block: 16, growth: (5, 4), geo_count: 40, extras: &[256, 512, ..., 16384] });
SC.class_for(size, align) -> Option<usize>;   // O(1) fast, O(jumps) aligned
SC.block_size(idx) -> usize;  SC.count() -> usize;  SC.max() -> usize;
```

---

## 2. `TaggedPtr` + the `free_slots` tagged-Treiber index stack — **extract as one crate**

**What / where.** Two halves of one protocol:

- `src/registry/tagged_ptr.rs` (~220 lines) — a packed `(index | tag)` `u64`:
  low 16 bits = slot index (with `0xFFFF` empty sentinel), high 48 bits =
  monotonic ABA tag bumped on every push; a `const` assert pins
  `MAX_HEAPS < 2^INDEX_BITS`. Pure bit arithmetic, zero `unsafe`.
- `src/registry/heap_registry.rs` lines ~499–600 (`pop_free_slot` /
  `push_free_slot`) — the Treiber pop/push over a single `AtomicU64` head, with
  slot-resident `next_free: AtomicU32` links and the H-2 empty-transition fix
  (a drain-to-empty pop packs the *running* tag, not tag 0, so the ABA window
  never reopens across empty→non-empty churn).

**Coupling.** `TaggedPtr` alone: one `const` assert against
`super::bootstrap::MAX_HEAPS` (becomes a generic parameter). The stack half:
needs a "slot with a `next_free: AtomicU32` field" abstraction — a small trait
or a closure pair (`load_next(i)` / `store_next(i, n)`). Both halves are
otherwise standalone safe code (the stack is safe Rust — all atomics, no raw
pointers).

**Effort: LOW.** ~300 lines total, no `unsafe`, no OS. The main design decision
is const-genericizing the index width (`INDEX_BITS`).

**Testability gain: HIGH — the verification artifacts move with it.**
`tests/loom_free_slots_aba.rs` (which today *transcribes the protocol verbatim*
into a local model precisely because it cannot import the real code — an
acknowledged fidelity risk) would test the *actual* crate code under loom, plus
its non-vacuity counterfactual (untagged head → loom finds the corruption).
`tests/regression_counter_wrap.rs`'s 48-bit wrap boundary tests come too, and the
`#[doc(hidden)] dbg_*` forwarders (a CLAUDE.md-sanctioned wart) disappear —
the crate's API is simply public.

**Community value: MEDIUM–HIGH.** "Lock-free free-list of small indices with an
ABA tag in one atomic word" is the canonical slot-recycler for slabs, object
pools, entity-component stores, and id allocators — routinely reinvented and
routinely reinvented *wrong* (tag reset on empty is exactly the H-2 bug). A
loom-verified, `no_std`, zero-`unsafe` version with a documented tag-width
budget analysis (the "∼89 years at 100k pushes/sec" bound) is a credible pitch.
Honest caveat: `TaggedPtr` alone is too small to be a crate; it only earns
extraction *together with* the stack protocol. (Boundary note: the Treiber
protocol itself brushes the concurrency sibling's lane — coordinate so it is
proposed once, from whichever side; the data-structure framing here is the
"index freelist" use case.)

**Suggested crate: `tagged-index-stack`** (or `slot-recycler`).
API sketch:

```text
let stack: TaggedIndexStack<INDEX_BITS = 16> = TaggedIndexStack::new();
stack.push(idx, |i, next| links[i].store(next));   // or a Links trait
stack.pop(|i| links[i].load()) -> Option<u32>;
```

---

## 3. `Node` — the raw-memory access membrane — **extract only with a repositioned pitch**

**What / where.** `src/alloc_core/node.rs` (~600 lines), the tier-1 `unsafe`
seam: intrusive free-list `write_next`/`read_next` (block's first word IS the
node), `deref`/`offset` address arithmetic, typed `read_struct`/`write_struct`,
width-specific aligned/unaligned reads/writes, and — the interesting part —
**atomic field views over hand-laid-out memory**: `atomic_u8_at` / `atomic_u32_at` /
`atomic_u64_at` (materialize `&AtomicUN` at `base + off`) and `atomic_ptr_ref`
(exposed-provenance reconstruction of a shared `AtomicPtr`, the task #142
Stacked/Tree-Borrows fix).

**Coupling.** The *code* is dependency-free (`core` only). The *safety
contracts* are the coupling: every `// SAFETY:` proof leans on allocator-specific
invariants (single-writer segment discipline, "block bytes untouched" remote-free
rule, per-path segment-liveness arguments for the `'static` atomic views).
Extracted, those proofs must be rewritten as *caller obligations* in generic
terms — which is real design work, not a mechanical move.

**Effort: MEDIUM.** Mechanically trivial; contractually substantial. The
`'static` lifetime on the atomic views is the hard part — a general-purpose
crate should probably return a lifetime-parameterized reference or a raw handle
instead, which then ripples into `sefer-alloc`'s `#![forbid(unsafe_code)]` upper
world (the `'static` exists precisely so safe modules can hold the reference).

**Testability gain: MEDIUM.** Today `Node` is tested only through the allocator
(plus miri over the whole). Standalone, each primitive gets direct miri
coverage — especially valuable for `atomic_ptr_ref`'s exposed-provenance dance,
which is subtle enough to deserve its own dedicated miri test matrix.

**Community value: MEDIUM.** `bytemuck`/`zerocopy` cover typed plain reads;
nothing mainstream covers "atomic views at offsets into memory you laid out
yourself" + "intrusive freelist word" as a curated, provenance-clean toolkit.
The audience is narrow (allocator/arena/shared-memory authors) but it is
exactly the audience that gets provenance wrong. This would be `aligned-vmem`'s
sibling: the second half of the unsafe story ("vmem gives you the span,
`carved-mem` gives you sound access into it").

**Suggested crate: `carved-mem`** (or `intrusive-node` for a narrower cut).
API sketch:

```text
Node::write_next(block: NonNull<u8>, next: *mut u8);  Node::read_next(..) -> *mut u8;
carved::atomic_u32_at(base, off) -> &'a AtomicU32;    // lifetime-honest version
carved::atomic_ptr_exposed(addr) -> &'a AtomicPtr<u8>; // provenance-clean shared atomic
```

---

## 4. `RemoteFreeRing` — bounded MPSC ring over borrowed memory — **worth it, jointly with the concurrency lane**

**What / where.** `src/alloc_core/remote_free_ring.rs` (~960 lines). A
Vyukov-style bounded MPSC queue of `u32` payloads whose storage is **not owned**
— it is a view over caller-provided raw memory (carved from segment metadata),
with a cache-line-separated layout (consumer cursor / producer cursors /
data slots each on their own 64 B line, the G8/ML4 fix), CAS-reserved push,
publish-store, stop-on-unpublished-slot drain, and an overflow counter
(full ring → bounded leak, never corruption).

**Coupling.** All atomics go through `Node::atomic_u32_at`; the layout offsets
come from the segment `Layout`; the P4/A4 dirty-routing visibility contract and
the `hardened` generation stamping are allocator-specific bolt-ons. Extraction
means: own the layout constants (they are already self-contained: `CURSOR_BLOCK
= 128` + `RING_CAP × 4`), take `(*mut u8, cap)` at construction, drop the
dirty-bit/generation hooks (or expose a post-publish callback).

**Effort: MEDIUM–HIGH** (the bolt-ons and feature gates are woven through).

**Testability gain: HIGH.** `tests/loom_remote_ring.rs`,
`tests/loom_remote_ring_drain_guard.rs`, `tests/remote_ring_unit.rs`, and the
ring-overflow regression tests all target this protocol; standalone they run
against the real code without the allocator harness around it.

**Community value: MEDIUM–HIGH — with the right pitch.** Plenty of MPSC rings
exist (crossbeam, rtrb, heapless), but almost all *own* their storage. A
`no_std`, no-alloc, loom-verified MPSC ring that is a **view over memory you
point it at** is the primitive people need for shared-memory IPC (two processes
over one mmap), DMA/driver mailboxes, and in-arena metadata queues — a real gap.

**Boundary note:** this is as much a concurrency primitive as a data structure;
the concurrency-lane sibling should co-own the proposal. Listed here because its
distinguishing feature (in-place over borrowed memory, integer payloads) is
structural, not synchronization-specific.

**Suggested crate: `inplace-ring`** (or `extern-mpsc`).
API sketch:

```text
let ring = MpscRing::<u32>::over(base: *mut u8, CAP);  // caller owns the bytes
ring.push(val) -> Result<(), Full>;                    // any thread
ring.drain(|val| ...);                                 // single consumer
```

---

## 5. `SegmentBitmap` / `AllocBitmap` / `MagazineBitmap` — **not worth extracting**

**What / where.** `src/alloc_core/segment_bitmap.rs` (~120 lines, the shared
mechanism: one bit per 16 B granule, test/set/clear over a `*mut u8` via the
`Node` seam) plus two `#[repr(transparent)]` domain newtypes
(`alloc_bitmap.rs` — the O(1) exact double-free oracle; `magazine_bitmap.rs` —
magazine residency).

**Honest verdict:** the mechanism is ~40 lines of trivial bit arithmetic; all
the *value* is in the domain semantics (M2 double-free exactness, single-writer
proofs, the "which of two orthogonal bitmaps at which layout offset" story),
and those don't generalize. `bitvec`/`fixedbitset` own the general niche; a
"bitmap view over a raw pointer" crate would be a shim too thin to justify its
supply-chain slot. If `carved-mem` (§3) happens, a `BitView` type belongs
*inside it* as a convenience — not as its own crate.

## 6. `SegmentTable`'s open-addressing hash + slot free-list — **not worth extracting**

`src/alloc_core/segment_table.rs` (~970 lines): linear-probe hash over
fixed self-hosted memory with **backward-shift deletion** (no tombstones —
`hash_remove`, R4-8/N3), a `u32` free-list stack of recyclable slots, and a
4-entry direct-mapped "proven present" cache. Algorithmically these are nice
(backward-shift deletion is under-known), but the structure is inseparable from
its reason to exist: self-hosting inside the primordial segment (M5
reentrancy-freedom), pointer-keyed by `base >> SEGMENT_SHIFT`, load-factor
guaranteed by `MAX_SEGMENTS` policy. A generic no-alloc hash table is already
`heapless::IndexMap` territory. The proptest suite
(`tests/segment_table_backshift_proptest.rs`, `segment_table_hash.rs`,
`segment_table_o1.rs`) already covers it well in-tree. Leave it.

## 7. `SegmentDirectory`, `PageMap`, `BinTable`, `SegmentHeader` views — **no**

`segment_directory.rs` (2-D per-class nonempty bitmap over table slots),
`PageMap`/`BinTable` (`segment_header.rs`: per-page class descriptors, per-class
`u32` free-list heads). All are thin views whose geometry *is* the segment
layout — extraction would export sefer-alloc's internal ABI, not a primitive.
Zero community value detached from the allocator.

## 8. `Tcache` magazine + `refill_n_for_class` byte-budget policy — **marginal, skip for now**

`src/registry/tcache.rs` (~180 lines): fixed per-class pointer magazines plus
one genuinely reusable idea — the D3 refill policy
`clamp(BYTE_BUDGET / block_size, 1, CAP)` (bound the *bytes* a refill parks per
thread, not the count). A generic Bonwick-style magazine crate is a plausible
future project, but this implementation is deliberately trivial (array + len)
and its interesting parts (the two double-free oracles, flush interplay with
`AllocBitmap`) live in `HeapCore`, not here. Extract only if #1/#2 succeed and
there is appetite for a "pool-building blocks" family.

## 9. PRNG / hash helpers — **nothing to extract**

Verified by grep: there is **no PRNG in `src/`** (matches for "rng"/"seed" are
prose in doc comments — "seed class", "ring"). The only hash is
`SegmentTable::hash_index` = `(base >> 22) & (cap-1)`, two ops keyed to the
segment geometry. No candidate exists in this category.

## 10. Large-segment cache (`alloc_core_large_cache.rs`, `large_cache_config.rs`, `large_cache_mode.rs`) — **no (and currently in-flight)**

A bounded, budgeted cache of released Large segments with decay/mode knobs.
Policy-heavy, coupled to `Segment`/OS release and stats, and the file itself is
an untracked in-progress split of `alloc_core.rs` at time of writing — not a
stable extraction target. Its ideas (budget + decay) are policy, not a data
structure.

---

## Ranked shortlist

1. **`size-classes`** (§1) — lowest coupling (pure `const` arithmetic), tests
   already in `tests/` and portable, highest reinvention rate in the wild, and
   the const-derivation + alignment-jump classifier is a real differentiator.
   Extract first.
2. **`tagged-index-stack`** (§2, TaggedPtr + free_slots protocol) — small, safe,
   loom-verified; extraction *upgrades* verification fidelity (the loom model
   stops being a transcription) and deletes the `dbg_*` forwarder wart.
   Coordinate with the concurrency lane.
3. **`inplace-ring`** (§4, RemoteFreeRing) — strongest external pitch
   (shared-memory IPC gap) but the most disentangling work; joint proposal with
   the concurrency lane.
4. **`carved-mem`** (§3, the `Node` seam) — do only as a deliberate follow-up to
   `aligned-vmem` ("the second half of the unsafe story"), and only after
   resolving the `'static`-vs-lifetime question; the safety-contract rewrite is
   the real cost.
5. Everything else (bitmaps, segment table/directory, page map/bin table,
   tcache, large cache): **keep in-tree** — too thin, too coupled, or pure
   internal ABI.
