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
machinery. **We adopt `slotmap` (`slotmap = "1"`) as the single-threaded
engine** rather than hand-build it — a battle-tested slotmap with years of
production exposure and fuzzing is *safer* than fresh hand-rolled code, even
though slotmap itself uses internal `unsafe`. Our crate keeps a thin **typed
membrane** on top: `Region<T>` wraps `slotmap::SlotMap<DefaultKey, T>`, and
`Handle<T>` is a newtype over `slotmap::DefaultKey` + `PhantomData<fn() -> T>`
so handles stay generic-over-`T` and typed (which raw slotmap keys are not).
This frees the creative budget for the genuinely novel work, where there is
**no safe, ready-made answer**: Phases 3b–4 — the concurrent lock-free read
tier and the byte / global-allocator descent.

`slotmap`'s audited `unsafe` does the job our own Hand would have done in the
single-threaded core; our own Hand organ now appears ONLY in the concurrent
epoch tier (3b-II) and the byte tier (4).

## 2. Architecture (summary)

Three organs (full treatment in [`DESIGN.md`](DESIGN.md)). For the
single-threaded core the Cartographer + Hand are now **provided by `slotmap`**
(an audited dependency); our code contributes the typed Membrane and stays
`#![forbid(unsafe_code)]`. Our own Hand organ appears only in the concurrent
epoch tier (3b-II) and the byte mode (4).

- **Cartographer** (safe) — all placement / free-list / compaction logic; pure
  integer arithmetic over indices, never touches memory. In the
  single-threaded core this is `slotmap`'s.
- **Membrane** (safe) — the typed `Handle<T>` API and generation checks;
  *total*, cannot express UB. **This is our contribution** in the
  single-threaded core.
- **Hand** (unsafe) — the single confined `unsafe` organ, present only in the
  epoch tier and the byte mode.

Data layout (single-threaded core): the dense generational layout is the one
`slotmap` gives us — a stable `slots` array (`handle.index` indexes it; each
slot carries a generation and is either `Occupied{dense}` or
`Vacant{next_free}`), a compact `dense: Vec<T>` of the live values, a
`dense_to_slot` back-pointer array, and a `free_head`. All operations are
`O(1)`; the dense array is compact by construction. (We adopt this rather than
re-implement it.)

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
| `#![forbid(unsafe_code)]` except named seam modules | confinement of `unsafe` | structural, compiler-checked |

The structural promise — `unsafe` is confined to named seam modules, each
lifting the ban with `#![allow(unsafe_code)]` for a single documented reason —
is checked by the compiler, not asserted in prose. The seams are inventoried in
README §"Where unsafe lives — the complete list"; the current, authoritative
list (no hardcoded count) is whatever `grep -rln 'allow(unsafe_code)' src/
crates/` prints, and a stray `unsafe` outside a named seam is a compile error.

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

### Phase 1 — Single-threaded engine: adopt `slotmap` + typed `Region<T>`/`Handle<T>` membrane · task #120

- **Goal:** the typed core — zero `unsafe` of our own, miri-clean — built by
  adopting `slotmap` as the engine and wrapping it in a thin typed membrane.
- **Deliverables:** `Region<T>` wrapping `slotmap::SlotMap<DefaultKey, T>`;
  `Handle<T>` as a newtype over `slotmap::DefaultKey` + `PhantomData<fn() ->
  T>`; the full op set; the proptest differential harness kept as a
  **conformance check** (does our typed wrapper behave like the reference
  model?); a miri CI gate; a one-export-per-file module structure.
- **Steps:**
  1. Restructure into **one-export-per-file** modules (per project convention):
    `region.rs` for `Region<T>`, `handle.rs` for `Handle<T>`, `lib.rs` only
    re-exports. No logic in `mod.rs`.
  2. Implement the typed membrane over `slotmap`: `new` / `with_capacity` /
    `len` / `is_empty` / `capacity` / `insert` / `get` / `get_mut` / `contains`
    / `remove` / `iter` / `iter_mut` / `clear` — delegating to `slotmap` and
    only adding the typed `Handle<T>` boundary.
  3. **Generation saturation / slot retirement** is now `slotmap`'s
    responsibility — `DefaultKey` already handles version saturation safely
    (it retires a slot rather than wrapping a generation into alias). The
    hand-rolled retirement is **removed**; note this explicitly so it is not
    re-introduced.
  4. **Keep the proptest differential harness** as a conformance check on our
    wrapper (expand the op-set to include `get_mut`, `clear`, and a
    drop-counting payload so I5 is checked under random sequences).
  5. **Run under miri** (`cargo +nightly miri test`). Even with zero
    `unsafe` of our own, miri validates the logic (and `slotmap`'s) and guards
    future `unsafe`; make it a CI gate.
- **Gate:** I1–I5 green (unit + proptest) **+ miri clean + `forbid(unsafe_code)`
  compiles for our crate**. (Version saturation handled by `slotmap`.)
- **Tools:** slotmap, proptest, miri.
- **Status:** old hand-rolled core landed (commit before this decision); now
  PIVOTING to the `slotmap`-engine + typed membrane.

### Phase 2 — Container choice benches + delegated compaction · task #121

- **Goal:** empirically **choose** the backing container, and confirm the
  compaction/capacity properties that `slotmap` now owns on our behalf.
- **Why this phase changed:** compaction and capacity are now mostly DELEGATED
  to `slotmap` (it owns the dense layout, the free list, and capacity growth).
  Our work shifts to measuring and to asserting slotmap's properties as a
  contract.
- **Deliverables:** criterion benches that empirically CHOOSE the container
  (`SlotMap` vs `DenseSlotMap` vs `HashMap` vs `Vec<Box<T>>`) with an honest
  verdict; I6 (compaction) confirmed as a property of `slotmap` via a test.
- **Steps:**
  1. **Honest verdict on container choice:** the dense layout (`DenseSlotMap`)
     wins **ITERATION** (contiguous `dense` array, cache-friendly); but the
     standard `SlotMap` has the faster single-indirection **LOOKUP**, which is
     the hotter path for resocks5's read-mostly workload (per-packet lookups
     vastly outnumber connect/disconnect). Bench both and record the verdict
     honestly — including if `DenseSlotMap`'s extra indirection on lookup
     matters more than its iteration win.
  2. Benchmark insert/remove throughput against `HashMap` and `Vec<Box<T>>`.
  3. **I6 (compaction) as a property of `slotmap`:** a test that after a
     sequence of inserts and removes, live-handle resolution is preserved and
     the dense store has no live-value fragmentation — asserting `slotmap`'s
     compaction-by-construction holds through our wrapper.
- **Gate:** I6 green (test over our wrapper); benches recorded with an honest
  verdict naming the chosen container and why.
- **Tools:** proptest, criterion.

### Phase 3a — `RwLock` concurrent wrapper · baseline, always-shippable · task #122

- **Goal:** a correct, coarse-grained concurrent API available immediately.
- **Deliverables:** `SyncRegion<T>` wrapping `RwLock<Region<T>>` with an
  ergonomic guard-based API; a concurrent stress test.
- **Steps:** wrap; expose read/write guards; thread stress test with random
  interleavings; document as the safe concurrent default.
- **Gate:** stress test clean; still `forbid(unsafe_code)`.
- **Tools:** std threads, proptest.

### Phase 3b — Lock-free read tier · two staged incarnations · loom-gated, experimental · task #123

This is the crate's **true reason to exist**: making concurrent
handle-addressed storage safe AND fast at once — collapsing the usual choice
between "`RwLock` (safe, slow under contention)" and "hand-rolled lock-free
(fast, unsafe, easy to get wrong)". resocks5's read-mostly hot paths
(per-packet lookups vastly outnumber connect/disconnect) are the target.

We admit only as much `unsafe` as each stage genuinely needs. 3b-I is the
trusted default; 3b-II is a heavier fallback taken only if 3b-I proves
inadequate.

#### 3b-I — RCU reads via `arc-swap` (page-granularity CoW) · ZERO `unsafe`

- **Goal:** lock-free reads via read-copy-update with page-granularity
  copy-on-write (the Btrfs-CoW principle) — with **zero `unsafe`** of our own.
- **Deliverables:** behind the `experimental` feature; readers load an
  **immutable snapshot** (an `Arc` to the current page table) and look up
  lock-free; rare writers serialise, CoW only the **touched page**, and publish
  via `arc.store`; reclamation is plain `Arc` refcounting (no epoch handoff,
  no `unsafe`).
- **Steps:**
  1. Design the page-granularity model: how readers pin a snapshot, how a
     writer CoWs only the touched page and atomically publishes, how `Arc`
     drop reclaims the old version once the last reader releases it.
  2. Implement with `arc-swap` — no `hand` module here; this stage stays under
    `#![forbid(unsafe_code)]`.
  3. Concurrent stress test under ThreadSanitizer.
- **Gate:** stress test + TSan clean; still `#![forbid(unsafe_code)]`. This is
  the trusted concurrent path unless its write cost / reader-pinning proves
  unacceptable.
- **Tools:** arc-swap, ThreadSanitizer, miri.

#### 3b-II — `crossbeam-epoch` + per-slot atomics · the single confined `unsafe` Hand

- **Goal:** a heavier lock-free design with finer-grained writes — taken ONLY
  if 3b-I's write cost / reader-pinning proves unacceptable.
- **Deliverables:** behind the `experimental` feature; `crossbeam-epoch` reader
  guards; writers use per-slot atomics with old versions reclaimed at epoch
  boundaries; the **first `hand` (unsafe) module**, every block carrying a
  `// SAFETY:` proof; a loom harness.
- **Steps:**
  1. Design the per-slot concurrency model (what readers see, how writers
     publish a new version of a single slot, when old versions are reclaimed).
  2. Implement the confined `unsafe` Hand.
  3. **loom** model-check the core scenarios (1 writer + 2 readers over a small
     region, exhaustive interleavings, assert linearizability of `get` vs
     `insert`/`remove`).
  4. ThreadSanitizer under stress; aarch64 CI.
- **Gate:** **loom-green + TSan clean** → ship behind the feature; **if loom is
  not satisfied, it stays experimental and 3b-I / 3a remain the trusted path**.
  No false confidence.
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

### Phase 7 — Shared-nothing per-thread substrate (parallel writes) · tasks #13–#16

The reads are already fast and parallel everywhere; the *write* path still
funnels through one `Mutex` in every tier. Phase 7 dissolves that — not by
sharding a lock (striping still collides), but by the **single-writer
principle**: give each writer **thread** its own vessel (shard), so two writers
in different shards never meet on a lock. The shard is the master key for
parallel write, twin of the handle-key that unlocked safe relocation.

**Invariant of the descent:** the single-threaded vessels
(`EpochRegion`/`Region`/`ByteRegion`) are reused **UNCHANGED**; only *who picks
which vessel* changes. New `unsafe` appears in exactly one place and only in 7b
(a re-audit of the existing `hand.rs`). The "unsafe is two screens" promise
holds.

DAG: `7a → 7b → { 7c, 7d }`. 7a already carries ~90 % of the win for bounded,
long-lived thread pools, with **zero new `unsafe`**.

#### 7a — `ShardedRegion<T>` — sharding skeleton · zero new `unsafe` · task #13

- **Goal:** N-way parallel writes via thread-local shard binding; reads stay the
  untouched lock-free `EpochRegion` seqlock. Pure safe composition.
- **Deliverables (behind `experimental`):** `ShardedHandle<T> { shard: u16,
  inner: EpochHandle<T> }` (hand-written `Copy`/`Eq`/`Hash`/`Debug`);
  `ShardedRegion<T> = Box<[EpochRegion<T>]>`, `with_shards(n, cap_per_shard)`,
  default `n = available_parallelism()`; a **TLS router**
  (`thread_local! { MY_SHARD }`) that lazily claims a free shard on a thread's
  first `insert` (atomic round-robin; threads > N share a shard — graceful
  degradation, still correct); `insert → own shard → ShardedHandle` (`Err` when
  the shard is full); `get_with` / `remove` route by `handle.shard`.
- **Steps:** the handle + the shard array; the TLS lazy-bind router + free-shard
  registry; routing for read/remove; extend the differential proptest to
  multi-shard.
- **Gate:** multi-shard differential proptest (a handle from shard A **never**
  resolves in shard B; I1–I4 hold across shards); a **write-scaling bench**
  (write throughput rising with thread count vs `SyncRegion` / `Arc<Mutex>`);
  the module compiles under `#![forbid(unsafe_code)]` (no new `unsafe`);
  miri-clean.
- **Tools:** proptest, criterion. (No loom — the router is safe.)
- **Honest edge:** a claimed shard is not released in 7a, so it fits a *bounded
  pool of long-lived threads*; thread-per-connection churn would exhaust shards
  (the lifecycle lands in 7b).

#### 7b — Remote-free + shard lifecycle · loom-gated · task #14

- **Goal:** cross-thread `remove` becomes lock-free (no contention on the owner
  shard); threads die safely; **live blocks of a dead thread stay valid**.
- **Key soundness insight:** `AtomicSlot::evict` is *already atomic* — the
  `swap(value → null)` is the linearization point (exactly one thread gets the
  non-null value and evicts; a second sees null → no-op). So **any thread may do
  the VISIBLE part of a removal** (generation bump + null + `defer_destroy`) —
  I2/I3 hold immediately. Only the free-list re-add stays owner-only.
- **Deliverables:** a per-shard **remote-free queue** (`crossbeam::SegQueue<u32>`
  or a Treiber stack); a non-owner `remove` atomically evicts the slot and
  **enqueues the index** for the owner, who drains it on its next op
  (single-consumer); an **abandoned-shard registry** (epoch/`ArcSwap`): a dead
  thread's shard is abandoned (its live slots stay resolvable), a new thread may
  adopt and drain it; a **re-audit of `AtomicSlot`** under the relaxed
  "any thread may evict" contract (was writer-only) — updated `// SAFETY:`
  proving evict-vs-evict / evict-vs-install are race-free. `unsafe` stays ONLY
  in the existing `concurrent/hand.rs`.
- **Gate:** **loom** on the remote-free + multi-writer-evict protocol (1 owner +
  1 remote-remover + 1 reader, bounded preemption); differential proptest with
  cross-thread remove; a thread-death test (live blocks survive the owner's
  death); miri on the relaxed `AtomicSlot`. **loom-green → ship; else it stays
  experimental and 7a remains the trusted path.**
- **Tools:** crossbeam, loom, miri, criterion.

#### 7c — Pinning (thread-per-core) — the locality apex · task #15

- **Goal:** maximal cache locality; `shard == core`.
- **Deliverables:** a thread-per-core helper (pin workers to cores via
  `core_affinity`/libc; each takes a stable shard == its core id) + docs on
  integrating with `glommio`/`monoio`/`tokio` current-thread-per-core. Naturally
  async-safe: no lock on the hot path, so "lock across `.await`" cannot arise.
- **Gate:** a pinned write-scaling bench showing improvement (lower latency
  variance, higher throughput) vs unpinned; honest "depends on workload
  locality" note.
- **Tools:** core_affinity, criterion.

#### 7d — `ShardedByteArena` — parallel raw allocation · research · task #16

- **Goal:** parallel tuple/node allocation: `[ByteRegion; N]` per-thread.
- **Deliverables:** the same pattern for the byte tier — TLS shard, remote-free
  for cross-thread `dealloc`, **segment-aligned chunks** (a large power of two)
  so `ptr & !(SEGMENT-1)` yields the owner (safe Cartographer arithmetic) →
  route the free to its queue; an optional `#[global_allocator]`. `unsafe` stays
  ONLY in the existing `byte/*`.
- **Gate:** **miri-clean** (hard — byte has no crossbeam); a parallel alloc
  bench vs the single-`Mutex` `ByteAllocator`; an **honest verdict** (parallel
  now, still not aiming to beat `mimalloc`).
- **Tools:** miri, criterion.

**Where `unsafe` lives (the structural promise holds):** 7a — zero new; 7b —
only the existing `concurrent/hand.rs` (re-audited); 7d — only the existing
`byte/*`. Compiler-enforced `deny + allow` in exactly those two modules.

**Honest map:** 7a scales writes linearly *until two writers hit the same
shard* (the striping compromise; near-linear for thread-spread workloads); 7b is
the real concurrency depth and the only new correctness reasoning (hard
loom-gate); a fully lock-free *slot choice* (a Treiber stack of vacant indices
replacing the per-shard free-list `Mutex`) is a possible **7b′** finishing the
last lock. This is an **architecture** on the application side (partition by key
at ingress → route to the owning thread); the crate supplies the per-thread
vessel within it.

---

## 6. Dependency DAG

```text
0 ─▶ 1 ─▶ 2 ─▶ 4
          │
          └──▶ 5 ─▶ 6
     1 ─▶ 3a ─▶ 3b ─▶ 7a ─▶ 7b ─▶ { 7c, 7d }
```

`0 → 1 → { 2 → { 4, 5 → 6 }, 3a → 3b → 7a → 7b → { 7c, 7d } }`. Phases 2/3a can
proceed in parallel once 1 is green; 3b and 4 are the research frontier and do
not block 5/6. Phase 7 (parallel writes via per-thread sharding) builds on the
3b epoch tier: 7a is the zero-`unsafe` sharding baseline, 7b the loom-gated
remote-free depth, 7c/7d the locality apex and the parallel byte tier.

## 7. Risk register

- **Phase 4 may never beat `mimalloc`.** Research-flagged; exists to learn, not
  to ship. resocks5 stays on `mimalloc` regardless.
- **Phase 3b is the time sink.** 3a (`RwLock`) is the always-shippable
  concurrent answer; 3b dives into lock-free only under loom's protection. If
  loom is not satisfied, 3b stays experimental.
- **~~Phases 0–2 overlap `slotmap`.~~** RESOLVED — we **adopted `slotmap`** as
  the single-threaded engine (§1, §8). The overlap is gone; the dense
  generational core is `slotmap`'s, and our effort is the typed membrane + the
  concurrent/byte tiers.
- **~~Generation wrap (`u32`).~~** Now `slotmap`'s responsibility — `DefaultKey`
  handles version saturation safely (retires a slot rather than wrapping into
  alias). The hand-rolled retirement is removed; the Phase 1 gate no longer
  asserts it.
- **`u32` index ceiling.** A region holds up to `u32::MAX` entries; documented.

## 8. Decisions log

- **Adopt `slotmap` as the single-threaded engine** (DECIDED) — a
  battle-tested slotmap with years of production exposure and fuzzing is
  *safer* than fresh hand-rolled code, even though slotmap uses internal
  `unsafe`. `slotmap`'s audited `unsafe` does the Cartographer/Hand job our
  own Hand would have done in the single-threaded core; our code keeps a thin
  typed membrane (`Region<T>` / `Handle<T>`) and stays
  `#![forbid(unsafe_code)]`.
- **The crate's differentiated value is the concurrent lock-free read tier**
  (3b) — collapsing the usual choice between "`RwLock` (safe, slow under
  contention)" and "hand-rolled lock-free (fast, unsafe, easy to get wrong)".
  This matches resocks5's read-mostly hot paths. The single-threaded core is
  not where we add value over existing crates.
- **`mimalloc` stays resocks5's global allocator.** Phase 4 (`ByteRegion` +
  `GlobalAlloc`) is honest research to honour the design, NOT a goal — it may
  never beat `mimalloc`, and that is an acceptable, documented outcome.
- **Dense-slotmap layout** (values in a compact `Vec<T>`, slots index into it)
  over a sparse "array with holes" — gives cache-friendly iteration and
  compaction-by-construction. Now obtained from `slotmap` rather than
  hand-built.
- **Handles store `index + generation`**, with `PhantomData<fn() -> T>` so a
  handle is typed yet unconditionally `Copy + Send + Sync` and covariant in `T`.
  In the `slotmap`-engine core, `Handle<T>` is a newtype over
  `slotmap::DefaultKey` + `PhantomData<fn() -> T>`.
- **`#![forbid(unsafe_code)]` for the upper world** — our own `unsafe` is
  admitted only in the epoch tier (3b-II) and byte mode (4), each behind a
  feature and confined to one module. (The single-threaded core has zero
  `unsafe` of our own; `slotmap`'s audited `unsafe` does that job.)
- **`RwLock` baseline before lock-free** — ship a correct concurrent API first
  (3a), treat lock-free as an opt-in, staged upgrade (3b-I RCU, then 3b-II
  epoch only if needed).
- **The global allocator is not the goal for resocks5** — `mimalloc` stays;
  Phase 4 is a research descent, honestly bounded.

## 9. Crate metadata

`no_std` + `alloc` (added in Phase 5; the arena needs only `alloc`); dual
**MIT OR Apache-2.0**; MSRV **1.88**. Dependencies:

- `slotmap = "1"` — a **normal (non-dev) dependency**, the single-threaded
  engine (Phase 1+).
- `proptest` — dev-dependency, the differential/conformance harness.
- `arc-swap` — enters with the `experimental` concurrent feature (3b-I).
- `crossbeam-epoch` / `loom` — enter with 3b-II (loom is dev-only).

A standalone workspace root to avoid ancestor-workspace capture.

## 10. Open questions

- **Remote & name:** create a GitHub repo and grab the `sefer-alloc` name on
  crates.io? (No remote yet; `repository` is omitted from `Cargo.toml`.)
- **~~Core vs adopt~~** — **RESOLVED (DECIDED → adopt `slotmap`)**: adopt
  `slotmap = "1"` as the single-threaded engine; the crate's differentiated
  value is the concurrent lock-free read tier (3b); `mimalloc` stays
  resocks5's global allocator. See §1 and §8 for rationale.
- **`ByteRegion` backing:** `Vec<u8>` vs `mmap`; which size-class scheme.
- **`no_std` timing:** Phase 5, or earlier if a `no_std` consumer appears.
