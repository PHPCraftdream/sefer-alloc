# Safety invariants

These are the properties `sefer-alloc` upholds. They are encoded as tests
(`tests/region_invariants.rs`, `tests/compaction.rs`, and the proptest harness
in `tests/differential.rs`) and form the spec that every future change must keep
green.

- **I1 — resolution.** A handle returned by `insert` resolves via `get` to the
  inserted value until it is `remove`d.
- **I2 — tombstone.** After `remove(h)`, `get(h)` is `None` forever and a
  second `remove(h)` is a no-op `None`.
- **I3 — no ABA.** A stale handle — one whose slot has since been reused —
  never resolves to a live value. The slot's generation is bumped on removal,
  so the old handle fails the generation check and yields `None`.
- **I4 — accounting.** `len()` equals the number of live entries, and
  `is_empty()` agrees.
- **I5 — drop-once.** Every live value is dropped exactly once: on `remove`
  (returned to the caller) or on `Region` drop. None is dropped twice; none is
  leaked.
- **I6 — compaction (Phase 2, implemented).** After compaction, every
  live handle still resolves to the same logical value, and reclaimed slots are
  reused. Compaction-by-construction is provided by the slotmap backing and is
  verified in `tests/compaction.rs`. See `docs/PLAN.md`.

## Allocator invariants (Phase 8+, `alloc-core`)

These hold for the segment substrate / allocator faces (`AllocCore` and the
future `GlobalAlloc` face). I1–I6 continue to hold for the Handle face. Spec
source: `docs/ALLOC_PLAN.md` §4. Encoded in `tests/alloc_core_*.rs`.

- **M1 — validity.** Every pointer returned by `alloc(layout)` is non-null
  (unless OOM), valid for `layout.size()` bytes, and aligned to `layout.align()`.
- **M2 — no double-free / no UAF.** A pointer is live from its `alloc` until its
  `dealloc`; freeing twice against **LIVE/MAPPED** memory, or freeing a foreign
  pointer, never corrupts the allocator — it is detected and no-op'd, never UB.
  A double-free against memory that has already been decommitted (and thus
  unmapped by the OS) is outside M2's scope: the pre-reuse `off >= bump`
  stale-free guard (#138) is the substrate-level check that catches the common
  reuse-window cases before the block can be handed out again. **Residual M2
  limit — ring↔magazine cross-thread double-free residual limit of M2** (task
  R2 / #154; real fix task #164): a block whose cross-thread free is still
  in-flight (queued in a segment's `RemoteFreeRing`, not yet drained by the
  owner) sets NEITHER own-thread oracle (it is not in the magazine's `slots`
  scan and the BinTable `is_free` bitmap still reads it as allocated), so a
  concurrent own-thread double-free of the same block is not detected.
  Pinned by `tests/regression_xthread_double_free_residual.rs`; modelled by
  `tests/loom_magazine_ring_compose.rs`. Full note in
  `docs/FASTBIN_DESIGN.md`.

  > **UB-vs-soundness distinction (task #202/#213).** A double-free or UAF
  > through the crate's own `unsafe fn dealloc`/`realloc` (or via manual
  > `GlobalAlloc` trait calls) is *documented caller UB* under the `unsafe fn`
  > contract — identical in kind to std's own `GlobalAlloc::dealloc` contract,
  > and to every other allocator (System / jemalloc / mimalloc). It is **not** a
  > soundness bug reachable from safe code. This framing is established precedent
  > in `tests/regression_xthread_double_free_residual.rs:71-89`; the task #202
  > SIGSEGV (fixed in `f165ced`) is a concrete worked example: the bug was a
  > cfg-gated path reachable only through deliberate `unsafe` misuse, not a
  > violation of M1/M3 that safe callers could hit. The real soundness boundary
  > depends on M1 (validity) and M3 (no overlap): as long as `alloc` never
  > hands out aliasing pointers, no purely-safe `Box`/`Vec`/`Rc`/`Arc` usage can
  > trigger memory corruption — empirically confirmed by
  > `tests/stress_safe_surface_no_aliasing.rs` (task #212, `403e216`).

- **M3 — no overlap (soundness-critical).** Two simultaneously-live allocations never share a byte. This is the invariant the crate's "impossible from safe code" soundness claim rests on: as long as `alloc` never hands out a pointer aliasing a still-live allocation, no combination of purely-safe `Box`/`Vec`/`Rc`/`Arc` usage can trigger a double-free or UAF, regardless of what `unsafe` misuse elsewhere in the process might do — safe code cannot reach the misuse path. Proven structurally (two independent static code-reading passes during task #202's investigation found no violation path) and at runtime by `tests/stress_safe_surface_no_aliasing.rs` (6 threads × 1500 iters × 6 size classes spanning small/medium/Large paths; pure-safe-API sentinel + address-sorted overlap tracking; zero M1/M3 violations across 30+ independent runs).
- **M4 — alignment & size fidelity.** The class chosen always satisfies size and
  alignment; large/huge allocations honour alignment up to `SEGMENT` (4 MiB) via
  a dedicated segment. Requests with `align >= SEGMENT` are rejected with `null`
  by design (task #130) — the dedicated-segment path cannot satisfy them.
- **M5 — reentrancy-freedom (load-bearing).** No entry point on the
  alloc/dealloc path allocates through the global allocator, takes a global lock
  that could deadlock against itself, or recurses. Proven structurally (no
  `Vec`/`Box`/`HashSet`/`std::alloc`/`format!` on the path — metadata self-hosts
  in segment memory) and at runtime by `tests/alloc_core_reentrancy.rs` (a
  counting global allocator observes a zero delta across an `AllocCore`
  workload). Under `miri` the `os` aperture falls back to `std::alloc` as a
  test-instrumentation path (`#[cfg(miri)]` only); the M5 runtime proof runs
  WITHOUT miri so the production path's freedom from `std::alloc` is still shown.
- **M6 — OS return (Phase 10).** Memory freed back to empty segments is
  eventually returned to the OS (decommit); steady-state RSS does not grow
  unboundedly under churn. Eager decommit was implemented in Phase 35 (feature
  `alloc-decommit`, part of the `production` bundle): an empty small segment's
  payload pages are decommitted when its live-block count drops to zero and
  recommitted on first reuse.
- **M7 — owner routing.** A pointer's owning segment is found in O(1) via
  `segment_of(ptr) = ptr & ~(SEGMENT-1)`; cross-thread free (Phase 10) reaches
  exactly the owning heap and reclaims exactly once.
- **M8 — generational coherence (Handle face).** A stale `Handle` into reused
  memory never resolves to a live value (I3 carried onto the segment substrate).

## Why handles, not pointers

A raw pointer into a `Vec` dangles the moment the `Vec` reallocates or the
element is removed — and dereferencing it is undefined behaviour. A handle is
an *index plus a generation*: the worst case is a checked lookup that returns
`None`. We trade one unconditional `unsafe` dereference for one safe integer
compare. That is the whole idea, and it is why the single-threaded core needs
no `unsafe` at all — the dense `Vec<T>` performs every initialization and drop.
