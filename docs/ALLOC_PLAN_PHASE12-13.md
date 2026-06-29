# Implementation plan — Phases 12–13 (detailed): production trust + speed parity

The continuation of [`ALLOC_PLAN.md`](ALLOC_PLAN.md) (Phases 8–11, shipped +
pushed). Phases 8–11 produced a **working, safe, fast allocator**: honest verdict
(`docs/ALLOC_BENCH.md`) — competitive with `mimalloc` (wins at 1024 B churn and
on realistic `Vec` push/grow; ~1.2–2× behind on small fixed-size churn) and
**~2.5–5× faster than the Windows system allocator**.

Two things remain, and this document specifies them at implementation depth:

- **Phase 12** — make the process-wide multithreaded `#[global_allocator]`
  **production-trusted** (today it aborts under libtest's reentrant harness).
- **Phase 13** — **close the small-size speed gap** to `mimalloc`.

The same conventions as the rest of the crate apply (`CLAUDE.md`): one export per
file, `mod.rs` re-exports only, tests in `tests/`, `unsafe` confined to the
documented seams with `// SAFETY:`, no version bumps, verification-first
(proptest ~64, bounded loom `preemption_bound=3`, targeted miri), per-phase
zero-trust review + commit.

---

## 1. The keystone — one inversion dissolves three "remaining" items

The three deferred hardening items look independent but share one root:

> A `Heap` lives in `thread_local! { RefCell<Option<Heap>> }` and is
> **RAII-dropped on thread exit**. From that one fact flow all three evils:
> `RefCell` turns *reentrancy* into a *refusal* (`try_borrow_mut → Err → null →
> abort`); dropping the heap on thread death is both the UAF and the
> "TLS-already-destroyed → abort"; binding segments to one thread's lifetime
> leaves no one to hand them to (no adoption) and no central place to return them
> to the OS (no M6 decommit).

**The descent:** *Heaps become slots in a global, self-hosted registry (the
`Region` slot-table discipline), carved from segments. A thread does not own its
heap — it caches a raw pointer to it in TLS. Thread exit does not drop the heap;
it abandons it back to the registry.* This is the same safe slot-table dyscipline
used by the `Handle` face, the `malloc` face, and the segment table — reflected
one level deeper: **the heap pool itself becomes a slot table.** Fractal, and
M5-clean (self-hosted in segment memory; no `Vec`/`Box`/`std::alloc`).

From the one move, all three resolve (reentrancy-safe TLS, adoption, M6) — plus a
**never-null** guarantee via a primordial fallback heap. Details below.

---

## 2. Phase 12 — "The unified heap registry" (production trust)

**Feature:** extend `alloc-global`; the registry becomes the substrate of
`SeferMalloc`. Default `alloc` (explicit single-thread `Heap` API) stays as-is
and sound.

### 2.0 Ownership model — segment-centric (the refactor that makes adoption O(1))

Today (Phase 9) the `Heap` keeps its small free-lists in `Heap.bins:
[FreeList; N]`, *separate* from each segment's own `BinTable` (which Phase 8
built but the heap layer bypasses). That is why adoption is hard: the free state
lives in the thread-local heap, not in the segments.

**Phase 12 moves the small free-list state INTO the segments** (mimalloc's "each
page owns its free list" rule, which `segment_header::BinTable` already models):

- A **segment** is self-describing: its header carries `owner` (heap id +
  generation), per-class free heads (`BinTable`), per-page class map (`PageMap`),
  and a `live_count`.
- A **heap** becomes thin: an id + a *current segment per size class* (the bump
  target) + the cross-thread `ThreadFreeStack`. Its "free lists" are just the
  `BinTable`s of the segments it owns.
- **Own-thread free** → `segment = segment_base_of(ptr)`; push the block onto
  `segment.BinTable[class]` (pure arithmetic + one node write); `live_count -= 1`.
- **Cross-thread free** → push onto the owning heap's `ThreadFreeStack` (the
  loom-verified Treiber path), drained by the owner into the right segment
  `BinTable`.
- **Abandonment** → mark every owned segment's `owner.state = ABANDONED` (one
  field) and link them into the global abandoned list. The free state travels
  *with* the segments (it is in their `BinTable`s) — **no bin merging needed**.
- **Adoption** → a thread pops an abandoned segment, CAS-claims its `owner`
  (Abandoned→Live with a generation bump — the Phase-7b linearization point), and
  it is now a fully-usable segment of the adopter, free lists intact.

This refactor is the core of Phase 12. It *simplifies* the heap (thinner) and is
what the whole inversion rests on.

### 2.1 Step 1 — `HeapRegistry` (self-hosted global slot table)

New module `src/registry/` (re-exports only in `mod.rs`):

- `src/registry/heap_registry.rs`:
  ```text
  pub(crate) struct HeapRegistry {
      slots: *mut HeapSlot,        // fixed array carved from the bootstrap segment
      cap: usize,                  // MAX_HEAPS (e.g. 4096); grow via linked registry
                                   // segments if exceeded (or cap + document)
      count: AtomicU32,            // high-water of allocated slots
      free_slots: AtomicU64,       // Treiber stack of free slot ids (index|tag — ABA)
      abandoned_segs: AtomicU64,   // Treiber stack of abandoned SEGMENT bases (tagged)
  }
  struct HeapSlot {
      state: AtomicU8,             // FREE=0 | LIVE=1 (owned by a thread)
      generation: u32,             // bumped on each (re)claim — M8/M9 coherence
      heap: UnsafeCell<HeapCore>,  // thin heap (id + per-class current segment + TFS)
      next_free: AtomicU32,        // intrusive link for the free_slots stack
  }
  ```
- **Self-hosting bootstrap:** the registry array lives in a dedicated primordial
  registry segment (reuse `bootstrap::primordial` discipline). A one-time,
  allocation-free init guarded by an `AtomicU8` state machine
  (`UNINIT→INITIALIZING→READY`), so the first concurrent allocators race safely
  (losers spin until READY). No `std::sync::Once` (it may allocate); hand-rolled
  with atomics.
- **API:** `claim() -> *mut HeapCore` (pop a free slot or bump `count`; CAS
  FREE→LIVE; return `&mut` via the slot's `UnsafeCell`), `recycle(id)` (CAS
  LIVE→FREE, push to `free_slots`), `abandon_segments(heap)` /
  `pop_abandoned_segment() -> Option<*mut u8>`.
- **ABA:** the Treiber stacks store `index|generation_tag` in an `AtomicU64`
  (index in low 32, tag in high 32; tag bumped on every push) — the standard
  tagged-stack ABA fix. Document the tag-width vs realistic churn.

### 2.2 Step 2 — raw-pointer TLS + thread-exit abandon guard

`src/global/tls_heap.rs` (replaces the `RefCell` parts of `heap/tls.rs` for the
global face; the explicit-`Heap`-API `with_heap` can stay for `alloc`):

```text
thread_local! { static LOCAL: Cell<*mut HeapCore> = const { Cell::new(null) }; }
thread_local! { static GUARD: AbandonGuard = const { AbandonGuard }; } // Drop → abandon

fn current() -> *mut HeapCore {           // the hot accessor — branch-light
    match LOCAL.try_with(|c| c.get()) {
        Ok(p) if !p.is_null() => p,
        Ok(_) => bind_slow(),             // first touch: claim from registry, publish, arm GUARD
        Err(_) => fallback_heap(),        // TLS destroyed (teardown) → never null (§2.3)
    }
}
```

- **No `RefCell`.** Access to `*mut HeapCore` is sound under the single-writer
  (own-thread) invariant; the only writer of a heap's bins is its owning thread.
  Reentrancy is structurally excluded (M5); there is no borrow to fail.
- **`AbandonGuard::drop`** runs on thread exit: if the heap has live blocks →
  `registry.abandon_segments(heap)` + `recycle` the slot; else recycle directly.
  Ordering note: the guard's TLS must outlive `LOCAL` or tolerate `LOCAL` already
  gone (read the heap id from a copy held in the guard, not from `LOCAL`).
- **`#[inline]`** `current()`; `bind_slow`/`fallback_heap` are `#[cold]`.

### 2.3 Step 3 — primordial fallback heap (the never-null guarantee)

`src/global/fallback.rs`:

- A process-global, always-live heap for the **pre-TLS** (very early runtime
  init) and **post-TLS-teardown** windows. Correctness-not-speed: a
  `Mutex<HeapCore>` (or a small lock-free global heap) is fine — these windows
  are rare. It is **never** dropped.
- `alloc`/`dealloc` during those windows route here. The malloc face therefore
  **never returns null for a serviceable request** — the cardinal sin (null →
  `handle_alloc_error` → process abort) is designed out, not guarded against.
- Blocks allocated from the fallback are normal segment blocks (owner = the
  fallback heap id), so a later cross-thread free routes correctly via
  `segment_base_of` → header owner. No special-casing on the free path.

### 2.4 Step 4 — adoption protocol (replaces the Phase-10 leak)

- On a heap's cold path (free-list miss before reserving a fresh segment), or on
  a periodic tick, the thread calls `try_adopt()`:
  `pop_abandoned_segment()` → CAS the segment header `owner` Abandoned→Live(me,
  gen+1) → it becomes a current/secondary segment of this heap; drain its
  `ThreadFreeStack` into its `BinTable`s.
- **M9 (adopt-exactly-once):** the Abandoned→Live CAS on the segment owner is the
  single linearization point — exactly one adopter wins per generation (the
  Phase-7b `try_evict_at` pattern, re-based). Loom-verified.
- Removes both the Phase-10 thread-death UAF and the abandonment-leak: abandoned
  memory is reclaimed, never dangles (the registry holds it until adopted or
  decommitted).

### 2.5 Step 5 — M6 decommit wiring (free-for-real)

- The owning segment's `live_count` (AtomicU32) is decremented on each free
  (own-thread direct; cross-thread on drain). Reaches zero ⇒ the segment is
  *empty*.
- Policy (start eager, tune later): an empty, non-current segment is scheduled
  for decommit — call the existing `os::decommit_pages(payload_range)`
  (`madvise(MADV_DONTNEED)` / `VirtualFree(MEM_DECOMMIT)`), keeping the header
  page mapped (so `segment_base_of` + owner read stays valid). The segment stays
  registered (address space reserved); a later alloc `recommit`s on demand.
- **M11 (decommit safety):** a cross-thread freer may hold a pointer into a
  segment that is concurrently emptying. Guard with **epoch reclamation** (the
  `crossbeam-epoch` organ from Phase 3b-II): decommit is `defer`-red behind an
  epoch advance, so any in-flight freer that already loaded the owner pointer
  finishes (drains) before pages are returned. Generations on the segment owner
  ensure a stale pointer never resolves into a recommitted-for-a-different-owner
  segment.

### 2.6 New invariants (extend `docs/INVARIANTS.md`)

- **M9 — adopt-exactly-once:** an abandoned segment is adopted by at most one
  thread (the Abandoned→Live owner-CAS is the linearization point); its blocks
  are never double-owned.
- **M10 — never-null-when-serviceable:** the global face returns null only on
  true OOM; during pre-TLS / teardown / reentrant edges it routes to the fallback
  heap, never aborts the process.
- **M11 — decommit safety:** a segment's pages are returned to the OS only when
  `live_count == 0`, behind an epoch barrier; no stale/in-flight pointer ever
  reads or writes decommitted memory; generations prevent stale-owner resolution.
- **M12 — registry coherence:** every live segment has exactly one owner slot
  (LIVE or the fallback); abandoned segments are reachable from the abandoned
  list exactly once.

### 2.7 Gate (verify each by hand; zero-trust)

- The end-to-end `#[global_allocator]` test that currently **segfaults/aborts**
  under libtest becomes **green, multithreaded, end-to-end**: install
  `SeferMalloc`, run `Vec`/`String`/`HashMap`/`Box` churn across parallel test
  threads *with threads spawning and exiting mid-allocation* (forces
  abandon+adopt). This is the headline gate.
- **loom** (`tests/loom_registry.rs`): 1 owner + 1 remote-freer + 1 adopter +
  1 reader, `preemption_bound=3`, on claim/abandon/adopt + decommit. **Must FAIL
  on a naive non-CAS adopt** (counterfactual, proven non-vacuous).
- **soak** (`tests/registry_soak.rs`): sustained churn that empties segments;
  assert **bounded RSS** via actual decommit accounting (not just reuse) — M6/M11.
- **miri** on the registry + TLS + adoption seam (small bounded case).
- `cargo test` across all configs green; `clippy -D warnings` clean across all
  configs; default `alloc` path unchanged; `unsafe` still only in
  `os`/`node`/`global` (+ the registry's atomic seam, documented).
- Removes the "NOT production-trusted (process-wide MT)" caveat from
  `ALLOC_BENCH.md`.

### 2.8 Sequencing within Phase 12 (each a reviewable sub-commit)

1. Segment-centric free state (move bins → `BinTable`; thin `HeapCore`) — keep
   all Phase 8–11 tests green (refactor, no behaviour change for single-thread).
2. `HeapRegistry` + bootstrap (claim/recycle) — single-thread first.
3. Raw-pointer TLS + abandon guard + fallback heap (never-null) — the libtest
   test should now *run* (may still leak without adoption).
4. Adoption protocol + loom.
5. M6 decommit + epoch barrier + soak.
6. Full gate + zero-trust review + commit.

---

## 3. Phase 13 — "Speed parity" (close the small-size gap)

We already beat mimalloc at 1024 B and on `Vec` push/grow; the gap is small
fixed-size churn (16–256 B). Highest leverage first; each step is independently
measurable against `docs/ALLOC_BENCH.md`.

### 3.1 O(1) size-class lookup (biggest small-size win)

- Today `SizeClasses::class_for` is a **linear scan** of ~40 classes per alloc.
- Replace with a `const` lookup table indexed by the size in `MIN_BLOCK` units:
  `const SIZE2CLASS: [u8; (SMALL_MAX / MIN_BLOCK) + 1]` filled at compile time in
  `build_table`. `class_for(size, align)` for the small path becomes:
  `if align <= SMALL_ALIGN_MAX && size <= SMALL_MAX { SIZE2CLASS[(size - 1) >>
  MIN_BLOCK_SHIFT] as usize } else { LARGE }` — two branches + one array load, no
  loop.
- Keep the existing table as the source of truth; the lookup is derived from it
  (a `const fn` builds both, so they cannot drift). Add a unit test asserting the
  lookup agrees with the linear scan for every size in range (non-vacuous,
  catches drift).

### 3.2 Raw-pointer TLS (inherited from Phase 12)

- Removing `RefCell` (§2.2) takes the borrow-check off every alloc/free. Confirm
  the `current()` accessor inlines to ~a TLS load + null check on the hot path.

### 3.3 Inlined, arithmetic-only own-thread free

- `#[inline]` `FreeList::pop`/`push`, `Node::read_next`/`write_next`/`deref`, and
  the small `alloc`/`dealloc` entry points.
- Own-thread `dealloc` must be pure arithmetic: `seg = ptr & ~(SEGMENT-1); class
  = PageMap[page]; push to seg.BinTable[class]` — **no `SEGMENT_MAGIC` read on
  the hot path** (keep the magic/foreign check only on the defensive
  cross-thread / `dealloc_any_thread` path). This shaves a dependent load per
  free.

### 3.4 Two-list free (`local_free` + `thread_free`)

- mimalloc splits a page's free list into `free` (alloc pops here) and
  `local_free` (own-thread frees push here), periodically moving `local_free →
  free`. This reduces branch misprediction and separates the own/remote queues.
- Our `ThreadFreeStack` is already the remote half; add the `local_free` split to
  the per-segment `BinTable` (a second head per class). Measure — adopt only if
  it actually helps our numbers (honest).

### 3.5 Refill batch tuning

- Bump `REFILL_BATCH` from 31 toward a full page worth of blocks (~256–512 for
  small classes); fewer substrate trips, better locality. Sweep the value in the
  bench and pick by measurement.

### 3.6 `heap == core` pinning (multithread scaling)

- Wire the Phase-7c `core_affinity` pinning organ to the registry's heaps:
  pin worker *i* to core *i* and bind it to heap *i*, so a heap's segments stay
  warm in one core's cache (no cross-core bouncing). Optional `pinning` feature,
  as in Phase 7c. Measure multithread scaling vs mimalloc.

### 3.7 Honest macro-bench

- Add `benches/malloc_macro.rs`: port mimalloc-bench-style workloads —
  `mstress` (many threads, mixed sizes, cross-thread free), `larson` (server
  churn), `rptest` (round-trip), `cfrac`/`xmalloc`-style — instead of the
  criterion micro-loop (whose per-iteration overhead muddies small-size numbers).
  Single- and multi-thread, RSS-over-time. Refresh `docs/ALLOC_BENCH.md`.

### 3.8 Gate

- Within a small constant factor of mimalloc on `mstress`/`larson` (single- and
  multi-thread); refreshed honest `ALLOC_BENCH.md`; **no correctness
  regression** (all Phase 8–12 gates — proptest, loom, miri, soak — still green);
  `clippy -D warnings` clean.

---

## 4. The heavy gate (over both phases; needs CI / non-Windows)

- `cargo-fuzz` — CPU-hours over adversarial alloc/free/realloc/cross-thread
  streams (`fuzz/` target exists from Phase 5; extend to the global face).
- **aarch64** multi-arch CI — the weak memory model x86 hides; the cross-thread +
  adoption + decommit protocols MUST be exercised there (loom is a model, aarch64
  is the metal).
- **ThreadSanitizer** under stress (Linux).
- Only after all green is the `malloc` face called **production-trusted** on every
  target. `docs/ALLOC_BENCH.md` states the final verdict.

---

## 5. Dependency DAG

```text
  (Phases 8–11 shipped + pushed)
        │
        ▼
  12  unified heap registry
      12.1 segment-centric free state (refactor)
      12.2 HeapRegistry + bootstrap
      12.3 raw TLS + abandon guard + fallback (never-null)
      12.4 adoption (loom)
      12.5 M6 decommit + epoch barrier (soak)
        │   ── removes "NOT production-trusted (process-wide MT)"
        ▼
  13  speed parity
      13.1 O(1) class lookup ─ 13.3 inlined arithmetic free ─ 13.2 raw TLS
      13.4 two-list ─ 13.5 refill tuning ─ 13.6 pinning ─ 13.7 macro-bench
        │
        ▼
  heavy gate (fuzz · aarch64 · TSan)  ── production-trusted on all targets
```

Strictly: **12 before 13** — the registry inversion is the prerequisite for both
the raw-pointer TLS (which 13's speed leans on) and production trust. Within 12,
the segment-centric refactor (12.1) is the foundation everything else rests on.

---

## 6. Risk register (honest)

- **The segment-centric refactor (12.1) touches the Phase-9 hot path.** Mitigate:
  it is a behaviour-preserving refactor for single-thread; keep every Phase 8–11
  test green at each sub-step before adding registry/adoption.
- **Concurrency is the hardest part.** claim/abandon/adopt/decommit are lock-free
  and cross-thread; correctness rests on loom + the epoch barrier + generations.
  If loom cannot prove a piece, it stays behind `alloc-xthread`/experimental and
  the single-thread path remains the trusted default (the same fallback discipline
  as Phase 10).
- **Decommit + epoch interaction (M11) is subtle.** A wrong barrier = UAF on
  decommitted pages. Gate hard on loom + miri + the soak; start with a
  conservative (later) decommit policy before tuning eager.
- **ABA on the registry stacks.** Tagged indices fix it; document the tag width
  vs realistic churn; loom must exercise the wrap.
- **TLS destructor ordering** (the guard vs `LOCAL`) is platform-subtle. The guard
  must tolerate `LOCAL` already torn down (hold its own heap-id copy). Test on
  Windows + (CI) Linux/macOS.
- **Speed may still trail mimalloc on tiny sizes.** The architecture is the right
  shape; if a small constant remains after §3, that is a documented honest
  outcome — not an order of magnitude.

## 7. Decisions log

- **One inversion, three cures (DECIDED).** Heaps → global self-hosted registry;
  TLS → raw-pointer cache; thread-exit → abandon (not drop). Solves
  reentrancy-safe TLS + adoption + M6 together. Rejected: patching the three
  independently.
- **Segment-centric free state (DECIDED for P12).** Move small free-lists from
  `Heap.bins` into the per-segment `BinTable` so segments are self-describing and
  adoption is O(1) (no bin merging). Supersedes the Phase-9 heap-local bins.
- **Never return null (DECIDED).** A process-global fallback heap serves the
  pre-/post-TLS windows; null→abort is designed out (M10).
- **Correctness before speed (DECIDED).** Phase 12 before 13; the registry also
  enables the fastest TLS, so the order compounds.

## 8. Open questions / not scheduled

- **Arena (bump + reset) third face** on the segment substrate (with `bumpalo` as
  a dev-bench comparator) — a "three faces, one substrate" extension; natural
  after 13, not scheduled.
- **`no_std`** allocator build (OS seam + `core` only) — a P13 stretch.
- **Crate split** (`sefer-core` vs `sefer-malloc`) — decide before any crates.io
  publish.
- **Registry growth beyond `MAX_HEAPS`** — linked registry segments vs a hard cap;
  decide by measuring realistic thread counts.
