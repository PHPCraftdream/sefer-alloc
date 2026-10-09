# Safety invariants

These are the properties `sefer-alloc` upholds. They are encoded as tests
(`tests/region_invariants.rs`, `tests/freelist_reuse.rs`, and the proptest harness
in `tests/differential.rs`) and form the spec that every future change must keep
green.

The canonical copy of the `sefer-region` invariants (I1–I7) lives at
`crates/sefer-region/src/invariants.md` and is rendered in the crate's rustdoc.
The text below is reproduced for workspace-level context; any drift is
resolved by updating the canonical copy.

- **I1 — resolution.** A handle returned by `insert` resolves via `get` to the
  inserted value until it is `remove`d.
- **I2 — tombstone.** After `remove(h)`, `get(h)` returns `None` for roughly
  `2^31` reuse cycles of that slot (a stale handle that has survived that many
  insert/remove cycles may wrap and spuriously resolve to a later value). A
  second `remove(h)` is a no-op `None`.
- **I3 — bounded stale-handle detection.** A stale handle — one whose slot has
  since been reused — does not resolve to a live value for roughly `2^31`
  reuse cycles of that slot. `slotmap`'s `DefaultKey` carries a 32-bit generation
  (odd = occupied, even = vacant): `insert` sets the low bit on reuse
  (`version | 1`), and `remove` separately advances it via `remove_from_slot`'s
  `version.wrapping_add(1)` — two different functions, so a full occupy/free
  cycle advances the generation by 2, and the old handle fails the generation
  check and yields `None`. After ~2^31 cycles the generation wraps and a very
  old handle may alias a later value. Memory safety is never affected.
- **I4 — accounting.** `len()` equals the number of live entries, and
  `is_empty()` agrees.
- **I5 — drop-once.** Every live value is dropped exactly once: on `remove`
  (returned to the caller) or on `Region` drop. None is dropped twice; the
  crate does not duplicate or internally forget values. Ownership contract:
  a stored value has exactly one owner; successful `remove` transfers
  ownership to the caller without calling `Drop`; values still owned when a
  normally-destroyed `Region` drops are dropped. Caller-side `mem::forget`
  of a removed value or the entire `Region` is outside this guarantee.
- **I6 — slot reuse and bounded growth.** Freed slots are reused by
  `insert`; capacity grows to a historical high-water mark of live entries
  and does not increase further under steady-state churn. Verified in
  `tests/freelist_reuse.rs` and in `crates/sefer-region/tests/coverage_gaps.rs`
  (`region_reserve_reuses_freed_slots_on_churn`). Note: `slotmap` does not
  physically compact — tombstone slots remain in the backing store; I6
  guarantees only reuse and bounded growth, not physical density.
- **I7 — instance isolation.** A `Handle<T>` resolves only through the
  `Region<T>` instance that minted it. Every accessor (`get`, `get_mut`,
  `remove`, `contains`) stamps its `region_id` at construction and checks
  it before touching the backing slotmap; a handle from a *different*
  `Region<T>` is rejected exactly like a stale handle (`None`/`false`),
  even when its raw `DefaultKey` collides with a live key in that region.
  Verified in `tests/region_invariants.rs` and in `crates/sefer-region/tests/smoke.rs`
  (`cross_region_handle_rejection`, `cross_region_different_value_types`,
  `cross_region_same_value_type`).

## Allocator invariants (Phase 8+, `alloc-core`)

These apply to the raw-pointer allocator faces (`AllocCore` and `SeferAlloc`)
under their enabled features and caller contracts. The re-exported
`sefer_region::Region`/`Handle` face retains its separate canonical I1–I7
contract; those guarantees are not raw-pointer substrate guarantees. The
Phase 8 design origin is `docs/ALLOC_PLAN.md` §4; current contracts must match
the implementation.

- **M1 — validity.** Every pointer returned by `alloc(layout)` is non-null
  (unless OOM), valid for `layout.size()` bytes, and aligned to `layout.align()`.
- **M2 — allocation lifetime and defensive rejection.** A successful allocation
  remains live until its one valid deallocation. Unsafe `dealloc` and `realloc`
  calls require a currently live allocation and the exact required layout.
  Foreign, stale, interior, and double-freed pointers violate that caller
  contract; defensive checks do not guarantee universal detection, no-op
  behavior, or freedom from UB under misuse.

  Small sidecar reclamation checks current allocation-bitmap and magazine-
  residency state before mutation. These checks do not identify a former
  allocation instance after reissue. Own-thread duplicate free/reissue before
  pending sidecar reclamation is outside the caller contract. The former ring
  residual test and composition model are absent from the current tree and
  provide no current coverage claim. `docs/FASTBIN_DESIGN.md` retains the
  historical ring design context.

  > **UB-vs-soundness distinction (task #202/#213).** Deliberate double-free or
  > UAF through unsafe allocator calls is a caller-contract violation, not by
  > itself a defect reachable through contract-respecting safe allocation APIs.
  > Historically, task #202's SIGSEGV (fixed in `f165ced`) concerned a cfg-gated
  > path reached through deliberate unsafe misuse, not a safe-caller M1/M3
  > violation. The safe allocator boundary requires valid allocations, no
  > overlap, and correct lifetime handling; unrelated unsafe corruption is
  > outside that guarantee. Task #212 (`403e216`) recorded safe-surface
  > observations in `tests/stress_safe_surface_no_aliasing.rs`, not a universal
  > proof against unsafe misuse.

- **M3 — no overlap (soundness-critical).** Two simultaneously-live allocations never share a byte. This is the invariant the crate's "impossible from safe code" soundness claim rests on: as long as `alloc` never hands out a pointer aliasing a still-live allocation, no combination of purely-safe `Box`/`Vec`/`Rc`/`Arc` usage can trigger a double-free or UAF, under contract-respecting allocator use; unrelated `unsafe` corruption elsewhere in the process is outside this guarantee. Historical evidence: (two independent static code-reading passes during task #202's investigation found no violation path) and runtime observations from `tests/stress_safe_surface_no_aliasing.rs` (6 threads × 1500 iters × 6 size classes spanning small/medium/Large paths; pure-safe-API sentinel + address-sorted overlap tracking; zero M1/M3 violations across 30+ independent runs).
- **M4 — alignment & size fidelity.** Successful allocations satisfy the
  requested size and alignment. Large requests with `align >= SEGMENT` use a
  biased reservation with an explicit payload offset. Checked-geometry,
  capacity, or reservation failure may return null; high alignment alone is
  not rejected.
- **M5 — installed-global reentrancy freedom (load-bearing).** Allocator paths
  must not allocate through the installed global allocator or recursively
  acquire their own initialization or ownership resources. Segment metadata
  uses VM-backed storage; independent route metadata and mixed class leaves
  use `System` directly. Direct `System` allocation is not recursion through
  `SeferAlloc`; this invariant prohibits neither all allocation nor all locks.
  The historical counting-global workload in `tests/alloc_core_reentrancy.rs`
  observes installed-global calls, not independent `System` allocation.
  Miri uses a separate backing-allocation instrumentation path.
- **M6 — bounded cold retention and OS release.** Under `alloc-decommit`, newly
  empty Small segments may remain registered with their existing committed
  pages and free lists in the bounded hysteresis pool. Disabled or full pooling
  releases the whole reservation. Cold trim releases pooled Small segments and
  cached Large spans. The explicit decommit-retain test hook instead decommits
  payload pages and resets metadata while retaining the reservation; reuse may
  recommit pages as needed. That test-hook decommit/recommit cycle is not the
  production committed-pool policy. Historically, Phase 35 introduced eager
  decommit. Decommit, reservation release, and immediate RSS reduction are
  distinct effects; no unconditional wall-clock reclamation deadline is promised.
- **M7 — owner routing and exactly-once reclamation.** Segment masking supplies
  an address lookup key, not necessarily the canonical reservation root for
  biased Large allocations. Own-path table lookup recovers the allocator-held
  root and block pointer. A valid foreign free pins an independent route
  descriptor and transfers one terminal obligation for owner reclamation; the
  idle fallback may instead reclaim synchronously under exclusive ownership.
  Exactly-once accounting assumes the caller transfers the current allocation
  only once.
- **M8 — Handle coherence is a separate contract.** Public `Region` and `Handle`
  are re-exported from `sefer-region`; stale-handle rejection and instance
  isolation follow canonical I3/I7 and their stated limits. Raw
  `AllocCore`/`SeferAlloc` pointers carry no equivalent stale-pointer Handle
  guarantee. Large terminal lifecycle generations govern reservation-state
  transitions, not general raw-pointer stale-use detection.

## Why handles, not pointers

A raw pointer into a `Vec` dangles the moment the `Vec` reallocates or the
element is removed — and dereferencing it is undefined behaviour. A handle is
an *index plus a generation*: the worst case is a checked lookup that returns
`None`. We trade one unconditional `unsafe` dereference for one safe integer
compare. That is the whole idea, and it is why the single-threaded core needs
no `unsafe` at all — the dense `Vec<T>` performs every initialization and drop.
