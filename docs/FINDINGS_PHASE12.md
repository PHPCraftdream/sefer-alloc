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
