# Implementation plan — sefer-alloc

`sefer-alloc` is a safe, handle-addressed region store over bytes. It hands out
small generational handles instead of raw pointers, keeps values in a dense
cache-friendly backing store, and confines `unsafe` to a single audited organ
that appears only in the lower tiers. The build is **verification-first**: the
tests and their tooling (proptest / miri / loom / fuzz / multi-arch) are part of
each phase, not an afterthought.

This document is the canonical, detailed plan. Architecture lives in
[`DESIGN.md`](DESIGN.md); the safety spec lives in
[`INVARIANTS.md`](INVARIANTS.md).

---

## 0. Origin & purpose

Building self-referential structures in Rust — linked lists, graphs, trees,
slabs — fights the borrow checker because raw pointers dangle. The established
resolution is to store references **as indices** into a backing array and to
make those indices safe against reuse with a **generation** counter. A handle
is then "an index plus a generation": a stale one fails a checked lookup and
returns `None` instead of dereferencing freed memory. We trade one
unconditional `unsafe` dereference for one safe integer compare — and that
single move is what lets the entire upper tier be `#![forbid(unsafe_code)]`.

From that one principle the design descends in tiers, each admitting only as
much `unsafe` as its world genuinely requires:

- the **typed, single-threaded core** needs *none* (the dense `Vec<T>` performs
  every init and drop);
- the **concurrent tier** needs a confined, loom-checked `unsafe` for lock-free
  reads (the read-copy-update / shadow-paging principle);
- the **byte / global-allocator mode** needs a single irreducible `*mut u8`
  handoff to `std` — the one aperture a handle cannot replace, because
  `GlobalAlloc`'s contract demands a raw address. We keep that aperture
  minimal, single, and documented rather than pretending it away.

### What this crate is — and is not

- **Is:** an application-level, handle-addressed store for *your own* data —
  connection tables, caches, slabs, graph/ECS nodes — safe top to bottom in the
  single-threaded core.
- **Is not:** a drop-in process-wide allocator. The `GlobalAlloc` descent
  (Phase 4) is research-flagged and may never beat `mimalloc`. For a global
  allocator, reach for `mimalloc`; resocks5 stays on it regardless of how this
  crate evolves.

## 1. Prior art (honest)

`slotmap`, `thunderdome`, and `generational-arena` already provide the
single-threaded vessel, and `crossbeam-epoch` provides the reclamation
machinery. Phases 0–2 deliberately re-tread the single-threaded ground — we
build our own clean, verified core as craft and as the foundation the upper
tiers rest on. The genuinely novel work, where there is **no safe, ready-made
answer**, is Phases 3b–4: the concurrent epoch tier and the byte /
global-allocator descent.

If priorities change, the core can be swapped for `slotmap` and effort
refocused on 3b–4 without losing the architecture.

## 2. Architecture (summary)

Three organs (full treatment in [`DESIGN.md`](DESIGN.md)):

- **Cartographer** (safe) — all placement / free-list / compaction logic; pure
  integer arithmetic over indices, never touches memory.
- **Membrane** (safe) — the typed `Handle<T>` API and generation checks;
  *total*, cannot express UB.
- **Hand** (unsafe) — the single confined `unsafe` organ, present only in the
  epoch tier and the byte mode.

Data layout (single-threaded core): a stable `slots` array (`handle.index`
indexes it; each slot carries a generation and is either `Occupied{dense}` or
`Vacant{next_free}`), a compact `dense: Vec<T>` of the live values, a
`dense_to_slot` back-pointer array, and a `free_head`. All operations are
`O(1)`; the dense array is compact by construction.

## 3. Verification methodology (first-class)

TDD alone is necessary but **insufficient** for this domain: the catastrophic
bugs (data races, missed memory fences, ABA, heap corruption) are
non-deterministic and architecture-dependent — a green test on x86 can hide a
crash on ARM's weaker memory model. The build therefore leans on tools matched
to each tier:

| Tool | Catches | Where |
| --- | --- | --- |
| **proptest** (differential vs a reference model) | logic divergence over random op sequences | every tier |
| **miri** | undefined behaviour in any `unsafe` | CI gate, Phase 1+ |
| **loom** | missed fences / ABA / ordering in lock-free code | Phase 3b gate |
| **ThreadSanitizer** | data races at runtime | Phase 3b |
| **cargo-fuzz** | corruption on adversarial op streams | Phase 5 |
| **multi-arch CI** (x86_64 + aarch64) | weak-memory bugs x86 hides | Phase 5 |
| `#![forbid(unsafe_code)]` except one module | confinement of `unsafe` | structural, compiler-checked |

The structural promise — "the `unsafe` is one screenful" — is checked by the
compiler (forbid everywhere but one documented module), not asserted in prose.

## 4. Invariants

The properties every change must keep green (full text in
[`INVARIANTS.md`](INVARIANTS.md)):

- **I1 — resolution:** a fresh handle resolves to its value until removed.
- **I2 — tombstone:** a removed handle is `None` forever; second remove is a
  no-op.
- **I3 — no ABA:** a stale handle (slot reused) never resolves to a live value.
- **I4 — accounting:** `len()` equals the live count.
- **I5 — drop-once:** every value is dropped exactly once (on remove or on
  `Region` drop), never twice, never leaked.
- **I6 — compaction (Phase 2):** compaction preserves live-handle resolution.

---

## 5. Phases (detailed)

Each phase lists its **goal**, **deliverables**, **steps**, and the **gate**:
the objective, tool-checkable condition for "done".

### Phase 0 — Scaffold + verification harness · ✅ DONE (commit 45771da) · task #119

- **Goal:** a buildable crate whose tests already encode the safety invariants.
- **Deliverables:** crate skeleton; dual MIT/Apache license; MSRV 1.88;
  `proptest` dev-dependency; the differential harness; the invariants doc.
- **Steps (done):** cargo project; `Region`/`Handle` types; proptest
  differential-vs-reference-model harness; unit tests for I1–I5.
- **Gate:** harness compiles and the properties are real. ✅ Met — 5 unit
  tests + 1 proptest + 1 doc-test green.
- **Remaining for full CI:** wire an actual GitHub Actions workflow (fmt,
  clippy, test, miri) — deferred until a remote exists.

### Phase 1 — Single-threaded dense generational `Region<T>` · core landed early · task #120

- **Goal:** the typed core — zero `unsafe`, miri-clean, with generation
  saturation handled.
- **Deliverables:** the full op set (done); a miri CI gate; slot retirement at
  generation saturation; an expanded proptest op-set.
- **Steps:**
  1. ✅ `new` / `with_capacity` / `len` / `is_empty` / `capacity` / `insert` /
     `get` / `get_mut` / `contains` / `remove` / `iter` / `iter_mut` / `clear`.
  2. **Generation saturation:** when a slot's generation would wrap, **retire**
     the slot (never return it to the free list) so handles can never alias.
     Track a retired count; document the (astronomically rare) capacity cost.
  3. **Expand the proptest op-set** to include `get_mut`, `clear`, and a
     drop-counting payload so I5 is checked under random sequences, not only the
     unit test.
  4. **Run under miri** (`cargo +nightly miri test`). Even with zero `unsafe`,
     miri validates the logic and guards future `unsafe`; make it a CI gate.
- **Gate:** I1–I5 green (unit + proptest) **+ miri clean + `forbid(unsafe_code)`
  compiles + saturation handled**.
- **Tools:** proptest, miri.
- **Status:** core implemented and green; OUTSTANDING — saturation retirement,
  a miri run, proptest op-set expansion.

### Phase 2 — Compaction, capacity policy, cache-locality benches · task #121

- **Goal:** explicit reclamation of the sparse slot array, a capacity policy,
  and a measured cache-locality win.
- **Deliverables:** `shrink_to_fit` + trailing-vacant trim; an optional
  remap-returning compaction; criterion benches; the I6 property.
- **Steps:**
  1. The dense `Vec` is *already* compact, so values need no compaction. This
     phase compacts the **slot** array: `shrink_to_fit` and trimming a trailing
     run of vacant slots — both **handle-preserving** (they never move a live
     slot, so outstanding handles stay valid).
  2. A separate `compact_and_remap() -> HandleRemap` for callers who *can*
     update their handles and want full slot renumbering. Documented as
     handle-invalidating by design.
  3. `reserve` / `capacity` growth policy.
  4. criterion benches: dense iteration vs `HashMap` and `Vec<Box<T>>` (prove
     locality); insert/remove throughput.
- **Gate:** I6 green; benches show the dense-iteration locality advantage.
- **Tools:** proptest, criterion.

### Phase 3a — `RwLock` concurrent wrapper · baseline, always-shippable · task #122

- **Goal:** a correct, coarse-grained concurrent API available immediately.
- **Deliverables:** `SyncRegion<T>` wrapping `RwLock<Region<T>>` with an
  ergonomic guard-based API; a concurrent stress test.
- **Steps:** wrap; expose read/write guards; thread stress test with random
  interleavings; document as the safe concurrent default.
- **Gate:** stress test clean; still `forbid(unsafe_code)`.
- **Tools:** std threads, proptest.

### Phase 3b — Lock-free read tier via epoch reclamation · loom-gated, experimental · task #123

- **Goal:** lock-free reads — the Btrfs-CoW / RCU principle incarnate for
  concurrent memory.
- **Deliverables:** behind an `experimental` feature; `crossbeam-epoch` reader
  guards; writers CoW the slot array (or use per-slot atomics) with old
  versions reclaimed at epoch boundaries; the **first `hand` (unsafe) module**,
  every block carrying a `// SAFETY:` proof; a loom harness.
- **Steps:**
  1. Design the concurrency model (what readers see, how writers publish, when
     old versions are reclaimed).
  2. Implement the confined `unsafe` Hand.
  3. **loom** model-check the core scenarios (1 writer + 2 readers over a small
     region, exhaustive interleavings, assert linearizability of `get` vs
     `insert`/`remove`).
  4. ThreadSanitizer under stress; aarch64 CI.
- **Gate:** **loom-green + TSan clean** → ship behind the feature; otherwise it
  stays experimental/off and 3a remains the trusted path. No false confidence.
- **Tools:** crossbeam-epoch, loom, ThreadSanitizer, miri.

### Phase 4 — `ByteRegion` + `GlobalAlloc` experiment · the tzimtzum, research-flagged · task #124

- **Goal:** descend the design to raw bytes and to the system-allocator
  boundary — and document, honestly, where the safe membrane must open.
- **Deliverables:** `ByteRegion` (handle-addressed byte ranges with size
  classes — the allocator's internal model); an experimental
  `unsafe impl GlobalAlloc` whose *intelligence* (size classes, free lists) is
  the safe Cartographer and whose **only** raw aperture is the final `*mut u8`
  handed to `std` (the single irreducible tzimtzum, loudly documented);
  benchmarks vs the system allocator and `mimalloc`.
- **Steps:** size-class scheme; backing store (`Vec<u8>` or an `mmap`'d region);
  `alloc`/`dealloc`/`realloc`/`alloc_zeroed`; miri on `ByteRegion`; benches.
- **Gate:** miri clean on `ByteRegion`; benches recorded with an **honest
  verdict** (including "does not beat mimalloc" if that is the result).
- **Tools:** miri, criterion, mimalloc (for comparison).
- **Honest flag:** this exists to learn and to honour the design, not to ship.
  resocks5's global allocator stays on `mimalloc` (FFI) regardless.

### Phase 5 — Hardening + publish prep · task #125

- **Goal:** earn production trust for the shippable tiers and prepare release.
- **Deliverables:** a `cargo-fuzz` target over random op sequences (CPU-hours);
  multi-arch CI (x86_64 + aarch64); the unsafe-confinement proof
  (`#![forbid(unsafe_code)]` everywhere except the one documented `hand`
  module); a `no_std` + `alloc` feature (the arena needs only `alloc`); rustdoc
  completeness, examples, README positioning, dual-license headers, MSRV.
- **Gate:** fuzz runs clean; multi-arch green; docs build; ready to publish to
  crates.io (the publish itself awaits explicit user go).
- **Tools:** cargo-fuzz, miri, multi-arch CI.

### Phase 6 — Integrate into one resocks5 hot structure · task #126

- **Goal:** the footprint in our world — prove the vessel in resocks5.
- **Deliverables:** one hot structure (candidate: the sticky cache or the
  connection table) backed by `vessel::Region`/`SyncRegion`, behind a feature
  flag, only after the crate stands on its own; an end-to-end before/after
  bench.
- **Gate:** feature-flagged integration passes existing resocks5 tests + a
  before/after bench; generational handles demonstrably eliminate stale-entry
  hazards. The global allocator stays `mimalloc` — this is an app-level swap.
- **Tools:** resocks5's existing test + bench harness.

---

## 6. Dependency DAG

```text
0 ─▶ 1 ─▶ 2 ─▶ 4
          │
          └──▶ 5 ─▶ 6
     1 ─▶ 3a ─▶ 3b
```

`0 → 1 → { 2 → { 4, 5 → 6 }, 3a → 3b }`. Phases 2/3a can proceed in parallel
once 1 is green; 3b and 4 are the research frontier and do not block 5/6.

## 7. Risk register

- **Phase 4 may never beat `mimalloc`.** Research-flagged; exists to learn, not
  to ship. resocks5 stays on `mimalloc` regardless.
- **Phase 3b is the time sink.** 3a (`RwLock`) is the always-shippable
  concurrent answer; 3b dives into lock-free only under loom's protection. If
  loom is not satisfied, 3b stays experimental.
- **Phases 0–2 overlap `slotmap`.** Accepted deliberately (craft + verification
  foundation). The core can be swapped for `slotmap` if priorities shift.
- **Generation wrap (`u32`).** A handle outliving `2^32` reuses of *its* slot
  could alias; mitigated by slot retirement at saturation (Phase 1 gate).
- **`u32` index ceiling.** A region holds up to `u32::MAX` entries; documented.

## 8. Decisions log

- **Dense-slotmap layout** (values in a compact `Vec<T>`, slots index into it)
  over a sparse "array with holes" — gives cache-friendly iteration and
  compaction-by-construction.
- **Handles store `index + generation`**, with `PhantomData<fn() -> T>` so a
  handle is typed yet unconditionally `Copy + Send + Sync` and covariant in `T`.
- **`#![forbid(unsafe_code)]` for the upper world**; `unsafe` admitted only in
  the epoch tier (3b) and byte mode (4), each behind a feature and confined to
  one module.
- **`RwLock` baseline before lock-free** — ship a correct concurrent API first,
  treat lock-free as an opt-in, loom-gated upgrade.
- **Build the core ourselves despite `slotmap`** — craft and a verification
  foundation; reversible.
- **The global allocator is not the goal for resocks5** — `mimalloc` stays;
  Phase 4 is a research descent, honestly bounded.

## 9. Crate metadata

`no_std` + `alloc` (added in Phase 5; the arena needs only `alloc`); dual
**MIT OR Apache-2.0**; MSRV **1.88**; the only dependency so far is `proptest`
(dev). A standalone workspace root to avoid ancestor-workspace capture.

## 10. Open questions

- **Remote & name:** create a GitHub repo and grab the `sefer-alloc` name on
  crates.io? (No remote yet; `repository` is omitted from `Cargo.toml`.)
- **Core vs adopt:** keep building the core, or adopt `slotmap` and refocus on
  3b–4?
- **`ByteRegion` backing:** `Vec<u8>` vs `mmap`; which size-class scheme.
- **`no_std` timing:** Phase 5, or earlier if a `no_std` consumer appears.
