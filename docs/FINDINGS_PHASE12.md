# Phase 12 — zero-trust findings backlog

Findings surfaced during per-phase zero-trust review that are **not blockers for
the phase in which they were found**, but must be resolved (or consciously
accepted) before the dependent phase ships. Each carries a severity and the
phase that owns the fix.

Legend: 🔴 must-fix before its target phase · 🟡 cleanup / hardening.

---

## From Phase 12.2 (HeapRegistry) review

### 1. 🔴 → fix in **12.4**: `abandoned_segs` truncates segment bases >4 GiB

`src/registry/tagged_ptr.rs` + `src/registry/heap_registry.rs`
(`push_abandoned_segment` / `pop_abandoned_segment`).

The `abandoned_segs` Treiber stack packs `(value | tag)` into an `AtomicU64`
with the **segment base in the low 32 bits** and the tag in the high 32. On
mainstream 64-bit Windows/Linux with ASLR, `mmap`/`VirtualAlloc` routinely
return anonymous mappings **above 4 GiB**, so `value as *mut u8` would truncate
the high address bits → a corrupted base. The agent's in-code comment claiming
"the kernel places mappings below 4 GiB in practice" is **false** for these
targets.

**Why it does not break 12.2:** `free_slots` (the other tagged stack) stores
slot *indices* (`< MAX_HEAPS = 4096`) — always sound. `abandoned_segs` is only
exercised by the `abandon_pop_round_trip` test, which deliberately pushes a low
fake base (`0x1000`), and `abandon_segments` is a no-op stub — so no real
segment base flows into it yet. (A real base >4 GiB would fire the
`debug_assert` in `push_abandoned_segment`.)

**Required fix (before 12.4 wires real segment bases):** rework `abandoned_segs`
to either
- an **intrusive head+next** stack — a `next_abandoned` link in the segment
  header (which gains an `owner` field in 12.3), storing the full 64-bit base in
  a separate `AtomicPtr` head (as `ThreadFreeStack` already does), or
- **tag-in-aligned-low-bits**: a segment base is `SEGMENT`-aligned (`1 << 22`),
  so its low **22 bits are always zero** — store the full base in the high
  42 bits and a **22-bit tag** in the low bits. Gives a full aligned 64-bit base
  with ~4M tag values.

Either form MUST pass loom on a push-pop-repush sequence (ABA-wrap) as part of
`tests/loom_registry.rs`. This requirement is recorded in task #24's description.

### 2. 🟡 (12.3+ cleanup): test-only helpers are `pub` in the shipped lib

`src/registry/bootstrap.rs`: `reset_for_test()` and `count_for_test()` are
plain `pub fn` (reachable via the `#[doc(hidden)] pub mod registry`), so they
ship in the `alloc-global` build. They are harmless (`reset_for_test` only
resets the init-state word; `ensure` does not reconstruct the const `static`,
so a production call is a benign no-op), but they are test-support code in the
production surface.

**Preferred:** gate them behind a dev/test feature (e.g. `registry-test`) or
otherwise keep them out of the shipped API once 12.3 reduces the test-only
`pub` surface (the `mod.rs` doc comment already anticipates this:
"the test-only pub surface here shrinks once 12.3 caches the pointer in TLS").

### 3. 🟡 (latent, single-thread unreachable): slot leak on OOM-at-first-claim

`src/registry/heap_registry.rs::claim`. On the slot's first claim the
generation is bumped to 1 *before* `HeapCore::new`. If `HeapCore::new` returns
`None` (primordial OOM), the code rolls the slot state `LIVE → FREE` and returns
null — but it does **not** push the slot onto `free_slots`, nor roll back
`count`, nor reset `generation` to 0. The slot is therefore unreachable (never
re-handed-out) and, were it ever reclaimed, its `generation == 1` would make the
`new_gen == 1` first-claim detector skip materialisation → hand out an
uninitialised heap.

**Why it does not bite now:** `claim` only mints via `bump_count` (monotonic) or
`pop_free_slot`; a rolled-back-but-not-pushed slot is reachable by neither, so
the inconsistency is dormant. OOM-at-first-claim is also essentially
unreachable single-threaded (the primordial reservation failing means the
process is already out of address space). Fix when adoption/decommit (12.4/12.5)
touch the claim/recycle accounting: on first-claim OOM, reset `generation` to 0
(restore the bootstrap state) before the `LIVE → FREE` rollback so the slot is
safe if ever reused, and decide whether to push it to `free_slots`.

---

## From Phase 12.3 (raw-TLS + fallback) review

### 4. 🟡 fallback.rs doc inaccuracy: fallback never installs a TFS

`src/global/fallback.rs` module docs claim "under alloc-xthread,
`HeapCore::install_thread_free` allocates a Box on the FIRST fallback
allocation". In fact `HeapCore::alloc` does NOT call `install_thread_free`
(only `bind_slow_tagged` on the registry TLS path does). The fallback's
`HeapCore.thread_free` stays `None`, so its `drain`/`stamp_owner` are no-ops and
the fallback `alloc` path performs **no `Box::new`**.

This is actually GOOD — it means the fallback `with_heap` (which holds a
non-reentrant spinlock) cannot self-deadlock by recursing into `SeferMalloc::alloc`
→ fallback → `acquire_lock` again. The doc should be corrected to state the
fallback is own-thread-only (no TFS), so the no-deadlock property is explicit.

### 5. 🟡 (by design, rare): fallback blocks freed cross-thread leak

Because the fallback never stamps `owner_thread_free` (no TFS), a block
allocated from the fallback (pre-TLS / teardown window) and later freed from a
normal thread routes `dealloc_routing` → `owner_thread_free.is_null()` →
`self.core.dealloc` on the FREEING thread's `AllocCore`, whose segment table
does not contain the fallback's segment → safe no-op → the block LEAKS (sound,
not a UAF). Rare path (fallback allocations are pre-TLS/teardown only); §2.3's
"routes correctly via owner" is not achieved for fallback blocks. Acceptable;
revisit if the fallback ever serves a hot path.

### 6. 🟡 (by design): plain `alloc-global` (no `alloc-xthread`) cross-thread free leaks

Under `alloc-global` WITHOUT `alloc-xthread`, a block allocated on thread A and
freed on thread B routes to B's own `AllocCore::dealloc` → foreign (not in B's
table) → no-op → leak (sound, not UAF). Cross-thread free correctness requires
`alloc-xthread` (the TFS routing). This matches the opt-in cross-thread design;
the MT end-to-end gate (12.5) must run under `alloc-xthread`.

---

## Status updates

- **Finding #1 (abandoned_segs >4 GiB): RESOLVED in 12.4** (commit c13ff0a).
  abandoned_segs now packs the full 64-bit base in the high bits + a 22-bit ABA
  tag in the low (SEGMENT-aligned) bits, with an intrusive next_abandoned link.
  Unit test `abandoned_head_packing_preserves_high_address` guards it.

- **Finding #7 (abandon/reuse double-ownership): RESOLVED in 12.5 via the
  SHARD MODEL architectural turn.** The original fix attempted to
  `clear_table` on abandon + re-acquire via adopt-or-reserve, but that path
  TRANSFERRED SEGMENTS BETWEEN HEAPS, creating a window where two heaps could
  write the same segment's BinTable/header concurrently (a data race that tore
  the SegmentHeader and corrupted free lists). The resolution is subtractive:
  a heap is a SHARD that stays whole across release→claim. Thread death now
  releases the SLOT ONLY (`recycle`); the HeapCore (segments + inline TFS)
  persists untouched in the slot. The reclaiming thread reuses the SAME
  HeapCore (claim does not re-materialise when `new_gen != 1`). Late
  cross-thread frees drain from the inline TFS on the new owner's first alloc
  (the shard-reuse discipline, mirroring `ShardedRegion` 7b). The
  abandon/adopt primitives (abandoned_segs Treiber, owner_state CAS) remain as
  a loom-proven substrate for a future decommit-when-empty policy, but are OFF
  the hot path. `owner_thread_free` is stamped ONCE (on first alloc) and never
  cleared/re-stamped — its address (the slot's inline TFS) is stable for the
  process lifetime. This removes the racy cross-thread header writes
  (clear-on-abandon, re-stamp-on-adopt) that were the root cause. The
  headline MT gate (`tests/global_alloc_mt.rs`) runs green under
  `alloc-global,alloc-xthread` with thread churn + cross-thread free.

  **Phase 12.5 remainder — RESOLVED in 12.6:** the 12.5 cross-thread drain
  DISCARDED drained blocks (a sound bounded leak). 12.6 makes it RECLAIM via a
  non-intrusive per-segment ring carrying `offset | class`; the owner reclaims
  lazily on its alloc-slow-path. The race this note attributed to "re-injection
  vs concurrent reuse" was actually a class-derivation logic bug (see §8 banner
  and `RACE_DRAIN_RECLAIM.md` §13), not a data race — TSan-confirmed. The
  discard-leak is gone; cross-thread-freed blocks are reused. M6 decommit / M11
  remain deferred behind a future `alloc-decommit` flag (#35). The single-thread
  `Heap` path (`heap::thread_free`) is unaffected and fully reuses.

## From Phase 12.4 (adoption) review

### 7. 🔴 → fix in **12.5**: abandon does not clear the heap's table → dormant double-ownership

`abandon_segments` (heap_registry.rs) marks each owned segment ABANDONED + pushes
it onto the abandoned stack, but does NOT remove it from the abandoning heap's
`AllocCore` segment table; and `recycle` + claim-reuse reuse the `HeapCore`
wholesale (no re-materialise). So after a thread exits and another thread
reclaims the slot, the segment is simultaneously (a) ABANDONED + on the global
abandoned stack and (b) in the reused HeapCore's table, actively allocated from.

**Why 12.4 is still SOUND:** `try_adopt` is NOT wired into the malloc cold path,
so nothing pops the stack in production; and `owner_id == slot-id` (the HeapCore
is reused in the same slot, same id) means `stamp_segment_owner` does not
re-stamp LIVE and `abandon_one_segment` does not re-push (it returns early on an
already-ABANDONED segment) — no double-push, no stack corruption, no UAF.

**Required in 12.5 (before wiring try_adopt):** make abandon/reuse consistent —
e.g. `abandon_segments` clears the heap's `AllocCore` table (transfers the
segments fully to the stack) and reclaim re-materialises / re-acquires via
adopt-or-reserve; OR re-stamp owner_state LIVE on reuse so stale stack entries
fail the adopter CAS. Without this, wiring `try_adopt` would let an adopter pop a
segment that the slot's current holder still uses → DOUBLE OWNERSHIP.

---

## From Phase 12.6a (drain-reclaim hypothesis — FALSIFIED)

### 8. ❌ Naive drain→BinTable restore SEGFAULTS — RESOLVED in 12.6 (true root ≠ what this note guessed)

> **RESOLVED (Phase 12.6, commit `255e18c`).** The crash was real, but BOTH this
> note's diagnosis ("intrusive-word race at the slot-reuse handoff") AND its
> proposed fix ("generation-tag") were wrong — layers peeled off by zero-trust
> verification. The TRUE root (found via ThreadSanitizer, which proved there is
> NO data race, plus a free-list audit on a reliable Linux repro) is in
> `RACE_DRAIN_RECLAIM.md` §13: a segment's single bump cursor interleaves size
> classes across pages, so `page_map.class_of()` is unreliable; the cross-thread
> reclaim derived the class from `page_map` (wrong class → wrong `block_size` →
> corrupted free list). Fix (§14): carry the class from the freer's `Layout`
> through the ring (`offset | class<<22`) and drop the redundant eager drain — no
> generation-tag. Reclaim now works (Windows + Linux green). The text below is
> the historical (falsified) hypothesis, kept as the diagnostic trail.

The /oxx hypothesis ("the 12.5 cross-thread-free leak is a scar; in the clean
shard model the owner is the sole BinTable writer, so simply restoring
`drain_thread_free` to return blocks to the BinTable is sound — no epoch") was
**TESTED AND FALSIFIED**. With the naive restore (swap + walk +
`dealloc_small_by_segment`), the committed `global_allocator_cross_thread_free`
MT test fails with **STATUS_ACCESS_VIOLATION (0xc0000005)** — a real UAF, not a
test bug (the discard version passes the same test).

The single-writer reasoning missed a real race at the **slot-reuse handoff /
intrusive-word**: a cross-thread freer can push a block X onto a slot's TFS
*after* the slot's current owner has already drained-and-reused X (or is
concurrently reusing it), so the drain reads X's first word — now user data —
as a free-list `next` pointer → out-of-segment deref / corrupted free list →
fault. The owner is the sole BinTable *writer*, but the BLOCK's intrusive word
is contended between the cross-thread pusher and the owner's reuse, across the
release→claim boundary.

**Conclusion:** the bounded-leak **discard** (shipped in 12.5, `abe5610`) is the
correct SOUND choice. Closing the leak requires a real guard — a per-segment (or
per-slot) **generation-tag**: stamp a generation that bumps on the
release→claim boundary; the cross-thread freer records the generation it
observed; the drain accepts a block into the BinTable only if the segment's
generation still matches (else the block is from a stale epoch and is skipped /
re-routed). This is the same family as the M11 decommit epoch-guard (#35). Until
that guard exists, the discard-leak stays. Do NOT re-attempt the naive restore.
