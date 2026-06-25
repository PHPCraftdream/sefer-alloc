# Implementation plan — `sefer-malloc`: the descent to a real allocator

This is the plan for turning `sefer-alloc` from a *safe handle-store with a
research byte tier* into a **general-purpose allocator that is as fast as the
best (`mimalloc`/`jemalloc`) and safe by construction** — without abandoning the
crate's reason to exist.

It is the sibling of [`PLAN.md`](PLAN.md) (Phases 0–7, shipped) and continues the
phase numbering at **Phase 8**. Architecture context lives in
[`DESIGN.md`](DESIGN.md); the new allocator invariants are spec'd in §4 below and
will be lifted into [`INVARIANTS.md`](INVARIANTS.md) when Phase 8 lands.

> **Honest scope, stated up front.** This is a multi-phase systems project that
> re-derives the architecture of a state-of-the-art allocator. It is large. It
> is the right size of large *only* because every safe organ it needs already
> exists in this crate (Phases 1–7) and is reused — Phase 8+ is a **refactoring
> that re-bases those organs onto OS memory**, not a from-scratch rewrite. The
> previous byte tier (`ByteRegion`/`ByteAllocator`/`ShardedByteArena`) was honest
> research; it does NOT become production by polishing — it is **superseded** by
> the self-hosted segment substrate below, which dissolves the four reasons it
> could never be a global allocator (see §2).

---

## 1. The principle (why this can be both safe and fast)

The governing idea, unchanged from the crate's founding and now applied to raw
memory at full depth:

> **Proven, simple, *safe* high-order tools descend to govern dangerous (raw)
> memory.** The *intelligence* of the allocator — size classes, free lists,
> placement, coalescing, reclamation policy — is pure safe integer arithmetic
> over indices. The *danger* — the `*mut u8`, the OS syscall, the intrusive node
> write — is confined to two thin, audited seams. Safety lives on the **cold
> path**; the **hot path** is the same lock-free free-list pop the best
> allocators use, so safety costs nothing per allocation.

### The keystone: the Membrane Inversion

Today the safe `Region<T>` **stands on** memory — its backing is `Vec<T>`, so
`std` owns the bytes and our safe structure is a *consumer* of the global
allocator. That is the root of every reason the byte tier cannot be the global
allocator (it calls `Vec`/`HashSet`/`std::alloc` *inside* its own allocation →
reentrancy/recursion).

Phase 8 **inverts** this: the safe slot-table discipline stops consuming memory
and starts **governing** it. The allocator owns OS-backed segments; the safe
`Region`-style slot tables that hold the allocator's own metadata are themselves
**carved from those segments**. The proven safe tool becomes the *brain* of the
allocator, not its *client*. This is the self-referential circle that makes a
real allocator possible: **the safe arena allocates the metadata of the byte
arena.**

---

## 2. The four showstoppers and how the architecture dissolves them

The honest blockers identified for "byte tier as `#[global_allocator]`", each
resolved by a specific layer below (not by hope):

| Showstopper (today) | Resolution (Phase 8+) | Where |
| --- | --- | --- |
| Metadata in `Vec`/`HashSet` → **reentrancy** (allocates inside alloc) | **Self-hosted metadata**: segment/page/heap tables live in fixed slot arrays carved from the segments themselves; intrusive free-list nodes store `next` inside the free block → zero metadata allocation on the hot path | §5 P8, P9 |
| `alloc_large` calls `std::alloc::alloc` → **infinite recursion** when we *are* the allocator | The OS aperture calls `mmap`/`VirtualAlloc` **directly**; large/huge get dedicated segments. `std::alloc` is never on any path | §5 P8 |
| `Mutex` on every alloc → **5–10× slower** than mimalloc | **Per-thread heap** with single-owner intrusive free lists: hot path is a lock-free `pop` (no lock, no atomic) — the single-writer principle from Phase 7 | §5 P9 |
| Chunks pinned for life → **never returns memory to the OS** | **Decommit policy**: an empty segment's pages are returned via `madvise(DONTNEED)`/`VirtualFree(MEM_DECOMMIT)`, governed by the safe segment table; generations prevent stale resolution | §5 P10 |

---

## 3. Architecture — the descent (summit → foundation)

`unsafe` lives in exactly **two thin seams**: the OS aperture at the foundation,
and the intrusive node read/write at the slot boundary (the existing `hand`
discipline). Everything between is safe.

```text
  ┌───────────────────────────────────────────────────────────────┐
  │  MEMBRANE — two faces of one governed substrate (safe)         │
  │   • Handle face:  Region<T>/Handle<T> — typed, relocatable,    │
  │     generational (structured data: DB records, MVCC, frames)   │
  │   • malloc face:  GlobalAlloc — *mut u8 (drop-in replacement)  │
  ├───────────────────────────────────────────────────────────────┤
  │  CARTOGRAPHER — all intelligence, 100% safe integer arithmetic │
  │   size classes · bins · placement · coalescing · decommit      │
  │   policy · O(1) owner lookup via ptr & ~(SEGMENT-1)            │
  ├───────────────────────────────────────────────────────────────┤
  │  PER-THREAD HEAP (P9) — single-owner intrusive free lists      │
  │   hot path = lock-free pop/push, no lock, no atomic            │
  │  CROSS-THREAD FREE (P10) — atomic thread-free list (Treiber),  │
  │   owner-drained — the Phase-7b linearization, loom-verified    │
  ├───────────────────────────────────────────────────────────────┤
  │  SELF-HOSTED METADATA (P8) — segment/page/heap slot tables     │
  │   carved FROM the segments; generational; no std collections   │
  ├───────────────────────────────────────────────────────────────┤
  │  ░░ SEAM: intrusive node r/w (hand discipline) ░░  [unsafe]    │
  ├───────────────────────────────────────────────────────────────┤
  │  ░░ SEAM: OS SEGMENT APERTURE (P8) ░░              [unsafe]    │
  │   mmap/munmap/madvise · VirtualAlloc/Free/decommit             │
  │   hands up SEGMENT-aligned (e.g. 4 MiB) raw spans              │
  └───────────────────────────────────────────────────────────────┘
```

### The two faces (the unification)

Internally everything is addressed by safe `(segment_id, offset)` integers. A
`Handle` and a `*mut u8` are two **views** of the same governed memory:

- **Handle face** — typed, safe, relocatable, generational. For structured data
  (DB records, MVCC version chains, buffer-pool frames). Beats hand-rolled
  `unsafe`.
- **`malloc` face** — the `GlobalAlloc` contract, raw `*mut u8`. For drop-in
  replacement. Not slower than `mimalloc`.

One substrate, two faces — so we never choose between "safe handle store" and
"`mimalloc` replacement". The same managed-segment core serves both.

---

## 4. New invariants (the allocator's safety spec)

Lifted into [`INVARIANTS.md`](INVARIANTS.md) when P8 lands. In addition to I1–I6
(which continue to hold for the Handle face):

- **M1 — validity:** every pointer returned by `alloc(layout)` is non-null
  (unless OOM), valid for `layout.size()` bytes, and aligned to `layout.align()`.
- **M2 — no double-free / no UAF:** a pointer is live from its `alloc` until its
  `dealloc`; freeing twice, or using after free, never corrupts the allocator
  (detected/no-op'd, never UB).
- **M3 — no overlap:** two simultaneously-live allocations never share a byte.
- **M4 — alignment & size fidelity:** the class chosen always satisfies size and
  alignment; large/huge honor arbitrary alignment.
- **M5 — reentrancy-freedom (the load-bearing one):** NO entry point on the
  alloc/dealloc path allocates through the global allocator, takes a global lock
  that could deadlock against itself, or panics. Proven structurally (no `Vec`/
  `Box`/`HashSet`/`std::alloc`/`format!` reachable from the hot path) — checked
  by a `#![no_std]`-style audit lint and a recursion test.
- **M6 — OS return:** memory freed back to empty segments is eventually returned
  to the OS (decommit); steady-state RSS does not grow unboundedly under
  churn.
- **M7 — owner routing:** a cross-thread free reaches exactly the owning heap
  (O(1) via segment alignment) and is reclaimed exactly once.
- **M8 — generational coherence (Handle face):** a stale `Handle` into reused
  memory never resolves to a live value (I3 carried to the segment substrate).

---

## 5. Phases (detailed)

Verification-first: each phase ships with its tests and the tools matched to its
risk. The fast/short-scenario policy from `CLAUDE.md` applies (quick criterion,
~64-case proptest, bounded loom, targeted miri); the heavy exhaustive runs
(CPU-hours fuzz, multi-arch, mimalloc head-to-head at scale) are the Phase 11
hardening gate.

### Phase 8 — Segment substrate + self-hosted metadata · the foundation · `alloc-core` feature

- **Goal:** OS-backed, SEGMENT-aligned memory with **self-hosting** metadata —
  the inversion. No `std` collection or `std::alloc` on any path. Single-threaded
  first (correctness before concurrency).
- **Deliverables (behind a new `alloc-core` feature):**
  - `os` — the **foundation seam**: a confined `unsafe` module wrapping
    `mmap`/`munmap`/`madvise` (unix) and `VirtualAlloc`/`VirtualFree` (windows)
    behind a safe `Segment { base, len }` API that yields SEGMENT-aligned spans
    (default 4 MiB). Every block carries a `// SAFETY:`. This replaces the
    `std::alloc` fallback entirely.
  - `SegmentTable` — the segment registry, **self-hosted**: a generational slot
    array (the `Region` discipline) whose backing is the first
    primordial segment, not a `Vec`. O(1) `segment_of(ptr) = ptr & ~(SEGMENT-1)`
    → header.
  - `PageMap` / `BinTable` — per-segment page descriptors and per-size-class free
    bins, all integer-indexed, all carved from segment memory.
  - A **bootstrap**: a one-screen primordial routine that hand-carves the first
    `SegmentTable` from the first segment (the `_mi_heap_main` analogue), after
    which the core self-hosts. The ONLY metadata-bootstrap `unsafe`.
  - A fine-grained **size-class scheme** (~40 classes to a threshold, then large
    segments, then huge direct-mmap) — replacing the 8-class toy.
- **Steps:** OS seam + miri-clean span tests → `SegmentTable` self-hosting +
  segment-relative addressing → page/bin tables → bootstrap → single-threaded
  `alloc`/`dealloc`/`realloc`/`alloc_zeroed` over it.
- **Gate:** single-threaded differential proptest (M1–M4) vs a reference model;
  **miri-clean** on the whole core (hard — there is no crossbeam here); a
  **reentrancy audit** proving no `std::alloc`/`Vec`/`Box` on the alloc path
  (M5, single-threaded); `forbid(unsafe_code)` everywhere except `os` + the node
  seam.
- **Tools:** miri, proptest. **Supersedes** `byte_region.rs`'s `std::alloc`
  fallback and `Vec`/`HashSet` metadata.

### Phase 9 — Per-thread heap + intrusive free lists · the hot path · `alloc` feature

- **Goal:** the lock-free fast path. A thread allocates from its own heap with no
  lock and no atomic — matching the best allocators instruction-for-instruction
  on the common case.
- **Deliverables:**
  - `Heap` — a per-thread set of size-class free lists. Free blocks are
    **intrusive**: a freed block stores the `next` index/pointer *inside itself*
    (zero metadata allocation). `alloc_small` = pop; `dealloc_small` (own-thread)
    = push. The node read/write is the **second seam** (the `hand` discipline).
  - TLS heap binding reusing the Phase-7a/7c router (shard == thread, lazy bind,
    release on exit; pinning makes heap == core).
  - Refill/flush policy (safe Cartographer): a heap refills a class from its
    segment's page when the free list drains; returns empty pages to the segment.
- **Gate:** single-thread throughput **bench vs `mimalloc`** (the honest target:
  within a small constant factor on the hot path); proptest M1–M4 through the
  heap; miri on the intrusive node seam; M5 reentrancy-freedom holds on the hot
  path.
- **Tools:** criterion (vs mimalloc), miri, proptest. Reuses the Phase-7 TLS
  router and single-writer principle.

### Phase 10 — Cross-thread free + OS return · the concurrency depth · loom-gated

- **Goal:** correct, lock-free cross-thread `free`, and memory returned to the
  OS — the two hardest correctness pieces, both already prototyped safely in
  Phase 7b.
- **Deliverables:**
  - **Thread-free list:** a freeing thread that does not own the block pushes it
    onto the owning heap's atomic Treiber stack (`compare_exchange` push); the
    owner drains in bulk on its next op. O(1) owner via segment alignment (M7).
    This is the Phase-7b linearization protocol, re-based onto segments.
  - **Decommit (M6):** when a segment's live count reaches zero, the safe segment
    table schedules `madvise(DONTNEED)`/`VirtualFree(MEM_DECOMMIT)`; generations
    + epoch reclamation ensure no stale handle/pointer resolves into decommitted
    pages.
  - Abandoned-heap adoption on thread death (Phase-7b lifecycle, re-based).
- **Gate:** **loom** on the thread-free + decommit protocol (1 owner + 1 remote
  freer + 1 reader, bounded preemption) — must FAIL on a naive non-CAS variant
  (counterfactual); cross-thread differential proptest; a **soak test** showing
  bounded RSS under churn (M6); miri on the relaxed seam. loom-green → ship; else
  it stays experimental and P9 (single-thread-fast) remains the trusted path.
- **Tools:** loom, miri, criterion (multi-thread scaling vs mimalloc), a soak
  harness. Reuses `AtomicSlot`/epoch from Phase 3b-II and the 7b protocol.

### Phase 11 — `GlobalAlloc` face + hardening + the verdict · production trust

- **Goal:** the drop-in face, earn production trust, and measure honestly against
  `mimalloc`.
- **Deliverables:**
  - `SeferMalloc` — `unsafe impl GlobalAlloc` over the heap substrate, **proven
    reentrancy-free** (M5) and **no-panic** (a panic in a global allocator =
    abort): the audit is a hard CI gate; an installation example as
    `#[global_allocator]`.
  - The **two-faces** API surface finalized: `Handle` face and `malloc` face over
    one substrate, documented.
  - Security hardening: free-list integrity (encoded/`debug`-checked next
    pointers), double-free no-op (M2), guard policy for huge allocations,
    graceful OOM (null, never panic).
  - **`docs/MALLOC_BENCH.md`** — the honest head-to-head vs `mimalloc`/system:
    mimalloc-bench-style workloads (mstress, rptest, larson, cfrac), single- and
    multi-thread, RSS-over-time. Stated plainly, win or lose.
  - `cargo-fuzz` over adversarial alloc/free/realloc streams (CPU-hours);
    multi-arch CI (x86_64 + **aarch64** — the weak memory model that x86 hides);
    ThreadSanitizer under stress.
- **Gate:** fuzz clean (CPU-hours); multi-arch green; TSan clean; the reentrancy
  + no-panic audit green; a published honest verdict. Only after all green is the
  `malloc` face called production-trusted.
- **Tools:** cargo-fuzz, miri, loom, ThreadSanitizer, criterion, multi-arch CI,
  mimalloc (comparison).

---

## 6. Where `unsafe` lives (the structural promise, extended)

Two thin seams, compiler-enforced (`deny(unsafe_code)` crate-wide; `allow` only
in these), plus the existing Phase-3b/4 organs during the transition:

- **`os`** — the OS segment aperture (the foundation tzimtzum): `mmap`/`munmap`/
  `madvise`/`VirtualAlloc`/`VirtualFree`. New in P8.
- **the node seam** — intrusive free-list node read/write and the
  `(segment, offset)` → `*mut u8` handoff (the `hand` discipline, generalized
  from `concurrent/hand.rs`).

Everything else — every placement, routing, coalescing, and reclamation
decision — is safe code. The promise "the `unsafe` is two screenfuls" is kept by
the compiler in every configuration.

---

## 7. Dependency DAG

```text
  (Phases 1–7 shipped)
        │
        ▼
  8 (segment substrate + self-hosted metadata, single-thread, miri)
        │
        ▼
  9 (per-thread heap + intrusive free lists, hot path vs mimalloc)
        │
        ▼
  10 (cross-thread free + decommit, loom-gated)
        │
        ▼
  11 (GlobalAlloc face + hardening + honest mimalloc verdict)
```

Strictly sequential: each layer rests on the one below. P8 is the inversion and
the gate to everything; if its self-hosting / reentrancy-freedom is not proven
miri-clean, **stop** — the rest is unsound without it.

---

## 8. Risk register (honest)

- **This is large.** P8–P11 re-derive a state-of-the-art allocator's
  architecture. It is justified only because Phases 1–7 already built every safe
  organ it reuses; it is a re-basing, not a green-field rewrite. If priorities
  shift, the crate's shipped value (the safe handle store) stands alone — this
  plan is additive.
- **P8 reentrancy-freedom is the make-or-break.** A single `Vec`/`Box`/`format!`/
  `std::alloc` reachable from the alloc path makes the global allocator deadlock
  or recurse. M5 must be proven structurally, not hoped — a dedicated audit lint
  + recursion test gate P8.
- **No-panic is non-negotiable at the `malloc` face.** A panic in `GlobalAlloc`
  aborts the process. Every `expect`/`unwrap`/index on the alloc path must become
  a checked, non-panicking branch by P11.
- **We may still not beat `mimalloc`.** The goal is "as fast as the best, and
  safe". If the honest P9/P11 benches show a persistent gap, that is a documented
  outcome — but the architecture (lock-free hot path, single-writer, segment
  owner-routing) is the same shape as the best, so the gap should be a small
  constant, not an order of magnitude.
- **Weak memory models.** The cross-thread protocols must be loom-verified (P10)
  and aarch64-tested (P11); x86 green is not evidence.
- **`#[global_allocator]` + TLS interactions.** TLS init/teardown ordering vs the
  allocator is subtle (TLS may need the allocator; the allocator uses TLS).
  P9/P11 must use a bootstrap-safe TLS discipline (lazy, allocation-free init).

---

## 9. Decisions log

- **Membrane Inversion** (DECIDED) — the safe slot-table discipline governs
  OS memory instead of consuming `Vec<T>`; metadata self-hosts in managed
  segments. This is the one change that makes a global allocator possible without
  abandoning the safe-tools-govern-raw-memory principle.
- **Two faces, one substrate** (DECIDED) — `Handle` and `*mut u8` are views of
  the same governed memory; we do not fork the project into "handle store" vs
  "malloc". Internally everything is `(segment, offset)`.
- **Reuse Phase 1–7 organs** (DECIDED) — `Region` slot discipline → metadata;
  `AtomicSlot`/epoch → cross-thread free + reclamation; `ShardedRegion` TLS
  router → per-thread heap; Phase-7b Treiber/owner-drain → thread-free list;
  Phase-7c pinning → heap==core. P8+ is a refactoring that re-bases these.
- **OS aperture replaces `std::alloc`** (DECIDED) — direct `mmap`/`VirtualAlloc`;
  the `std::alloc` fallback in `byte_region.rs` is removed (it recurses when we
  are the global allocator).
- **Single-writer hot path** (DECIDED) — per-thread heap, lock-free intrusive
  free lists; the `Mutex<ByteRegion>` of the research tier is NOT on the hot
  path. Cross-thread free is the only synchronized path, and it is lock-free.
- **The research byte tier is superseded, not extended** (DECIDED) —
  `ByteRegion`/`ByteAllocator`/`ShardedByteArena` remain as honest research
  artifacts behind their features; the production allocator is the new
  self-hosted substrate.

## 10. Open questions

- **Segment size** (4 MiB like mimalloc?) and **huge threshold** — bench-driven
  in P8/P9.
- **Size-class scheme** — adopt mimalloc's class spacing, or derive our own;
  measured for fragmentation vs speed in P9.
- **Decommit policy** — eager vs lazy (`mimalloc`'s reset delay) — P10, tuned by
  the RSS soak.
- **`no_std`** — the allocator core wants only the OS seam + `core`; a `no_std`
  build (with a user-supplied OS aperture) is a natural P11 stretch.
- **Crate split** — keep one crate with features, or split `sefer-core` (handle
  store) / `sefer-malloc` (allocator)? Decide before any crates.io publish.
