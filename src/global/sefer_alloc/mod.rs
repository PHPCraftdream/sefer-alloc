//! Phase 12.3 -- the alloc face: [`SeferAlloc`], an `unsafe impl GlobalAlloc`
//! over the global heap registry (Phase 12.2) via raw-pointer TLS (Phase 12.3).
//!
//! This is the **drop-in face** -- the campaign's victory deliverable, over
//! the segment-backed, self-hosted, registry-resident heap allocator. Note:
//! the `Handle` face (`Region<T>`/`Handle<T>`, typed, generational,
//! relocatable) is a SEPARATE, independent API over third-party `slotmap` --
//! it shares no backing memory with this segment substrate (corrected
//! 2026-08-09 per the static release audit's F5; the original "one
//! substrate, two faces" design intent is preserved as history in
//! `docs/ALLOC_PLAN.md`, but is not what shipped).
//!
//! ## Phase 12.3 rewiring
//!
//! Previously (Phase 11) this face routed through a now-removed
//! `RefCell<Option<Heap>>` TLS binding. That binding ABORTED under
//! libtest's reentrant harness: `RefCell::try_borrow_mut` returns `Err` on
//! a reentrant borrow → the alloc face returned null → std aborted.
//!
//! Phase 12.3 replaces that with
//! [`tls_heap::current_for_alloc`](super::tls_heap::current_for_alloc): a raw
//! `Cell<*mut HeapCore>` TLS cache (no borrow state to fail) over the
//! global [`HeapRegistry`](crate::registry::HeapRegistry). The heap lives in
//! a registry slot (not in TLS); thread exit recycles the slot (whole-slot
//! reuse — the `HeapCore` stays whole, not dropped). The alloc face is therefore **reentrancy-safe**
//! (M5) and **never-null** (M10): [`current_for_alloc`] resolves to a
//! non-null pointer in every case (cached slot, fresh claim, or the
//! process-global fallback heap).
//!
//! ## M5 (reentrancy-freedom) -- how it is upheld
//!
//! The whole point (§4 M5, §8 of `ALLOC_PLAN.md`): when WE are the global
//! allocator, ANY use of `Vec`/`Box`/`HashSet`/`std::alloc`/`format!` on the
//! alloc path would recurse infinitely. This module contains NONE of those.
//! `current_for_alloc()` is a plain thread-local load + null check.
//! `bind_slow_tagged` claims a registry slot (which bootstraps via the OS
//! aperture, never `std::alloc`); the bind path performs NO `std::alloc` at
//! all: since task H1 (#13), the cross-thread free head (TFS) is a
//! slot-resident `'static AtomicPtr<u8>` (or `FALLBACK_TFS` for the fallback
//! heap), planted by `HeapCore::bind_thread_free` at claim time — before
//! `bind_slow_tagged` ever sees the heap pointer, so no per-bind allocation is
//! needed at all (a `Box` there would have recursed into `SeferAlloc::alloc`
//! → `bind_slow_tagged` → …; see
//! `registry::heap_core`). The `HeapCore` alloc/dealloc paths are pure safe integer
//! arithmetic + the `node` seam (intrusive pointer r/w). No `std` collection
//! is reachable from here.
//!
//! ## No-panic discipline -- how it is upheld
//!
//! **Failure paths (the common case) never panic.** `alloc`/`realloc` return
//! null on failure (OOM, a foreign pointer, a layout we refuse to serve);
//! `dealloc` is a safe no-op on any failure (an unrecognised block is leaked
//! rather than corrupting state); `alloc_zeroed` is `alloc` + zero-fill:
//! - `alloc`: `current_for_alloc()` → `&mut HeapCore` → `HeapCore::alloc`
//!   (returns null on OOM). If `current_for_alloc()` itself yields the
//!   fallback (TLS teardown), the fallback's `with_heap` returns `None` only
//!   on true OOM → null.
//! - `dealloc`: `current_for_alloc()` → `HeapCore::dealloc`. If TLS is torn down, the
//!   fallback's `with_heap` deallocs under the spinlock; a torn-down-TLS
//!   dealloc still routes correctly (the segment's owner routes via the
//!   header). On any failure this is a no-op (the block is leaked safely).
//! - `realloc`: an in-place fast path for same-class / compatible growth (C2:
//!   own-thread reallocs delegate to `AllocCore::realloc`, which short-circuits
//!   when the block can stay put), falling back to `alloc` + copy + `dealloc`
//!   otherwise — all null-returning.
//! - `alloc_zeroed`: `alloc` + zero-fill.
//!
//! **Four release-surviving invariant tripwires (abort by design).** Beyond
//! those failure paths a small number of "cannot happen" checks remain as
//! *release* panics (not `debug_assert!`). Each is a precondition the
//! immediate caller already proves on the same `&mut self` owner-only path,
//! so under correct operation none is reachable; an independent audit
//! (release-stabilization F-5) could not construct a violation of any of the
//! four. They are deliberately kept as release panics rather than softened to
//! silent no-ops (the contrasting `AllocCore::reclaim_offset` style —
//! "bounds-check FIRST and no-op"): each guards allocator metadata whose
//! silent corruption would be strictly worse than an immediate abort, so a
//! future bug that broke one trips loudly at the point of corruption instead
//! of continuing with inconsistent state (defence in depth). The four, all
//! reachable from this file's `GlobalAlloc` impl under `production`:
//!
//!   (Line numbers are deliberately omitted here — they drift as unrelated
//!   edits shift surrounding code; `tests/no_panic_doc_accuracy.rs` pins the
//!   four by message string + occurrence count instead, which is the
//!   drift-proof identifier. File + function name is unambiguous without a
//!   line number.)
//!
//!   1. `alloc_core/large/alloc_core_large_cache.rs` — `.expect("large_cache
//!      _slot_take: empty base slot")` in `large_cache_slot_take`
//!      (`alloc-decommit`, in `production`).
//!   2. `alloc_core/large/alloc_core_large_cache.rs` — `.expect("large_cache
//!      _slot_take: empty extension slot")` in `large_cache_slot_take`
//!      (`alloc-decommit`).
//!   3. `alloc_core/large/alloc_core_large_cache.rs` — `unreachable!(…)` in
//!      `large_cache_slot_take` (`alloc-decommit`).
//!   4. `alloc_core/large/alloc_core_large_cache.rs` — `unreachable!(…)` in
//!      `large_cache_slot_set` (`alloc-decommit`).
//!
//!   All four live in the large-cache slot take/set helpers. Their callers
//!   only ever pass an index proven occupied by an ARRAY read: the best-fit
//!   scan and `oldest_occupied_slot` enumerate candidate indices from the
//!   `large_cache_occupied` bitmask (R32-12, task #503; wired into these two
//!   scans by #1985) but still consult `large_cache_slot_get(i)` before using
//!   a slot, and only ever return an index whose ARRAY entry was `Some`.
//!   A bitmask/array desync therefore cannot reach these `.expect()`/
//!   `unreachable!()` arms — a stale set bit merely makes a scan SKIP that
//!   index. The worst a desync can do is
//!   `large_cache_find_free_slot` handing back an index the array already
//!   holds occupied (an overwrite on `set`, silent data loss — never a
//!   take-side panic). `tests/no_panic_doc_accuracy.rs` pins the four by
//!   their message strings.
//!
//!   A former FIFTH release tripwire — the ownership re-check in
//!   `realloc_inplace_fast_path_known_base`
//!   (`alloc_core/alloc_core/mem/realloc_fastpath.rs`: `assert!(self.table
//!   .contains_base_ro(base), "known-base realloc …")`) — was
//!   demoted to `debug_assert!` (#1984, alloc-core perf review P1-2). Both
//!   callers (`AllocCore::realloc` / `HeapCore::realloc`) already prove
//!   `contains_base(base)` on the same path before calling, so the re-probe
//!   was redundant, and a release-surviving panic on the alloc path
//!   contradicted the no-panic discipline above. The check is retained as a
//!   debug-only falsification pin (the F12 style in `alloc_core_large.rs`);
//!   `tests/no_panic_doc_accuracy.rs` pins both its message string and its
//!   demoted form.
//!
//! **Panic-in-`GlobalAlloc` is abort, not UB.** On current Rust the
//! `__rust_alloc` / `__rust_dealloc` / `__rust_realloc` / `__rust_alloc_zeroed`
//! shims are `#[rustc_nounwind]`, so a panic that escapes any `GlobalAlloc`
//! method aborts the process — it is not undefined behaviour. Nothing in this
//! crate relies on anything stronger, and nothing requires a downstream
//! `panic = "abort"` profile: the nounwind shims guarantee the abort
//! regardless of the consumer's panic strategy. Stated here explicitly
//! rather than left implicit inside the failure-path bullets above.
//!
//! [`current_for_alloc`]: super::tls_heap::current_for_alloc

#[cfg(feature = "batch-api")]
mod batch;
mod core;
mod diag;
mod global_alloc;

pub use core::SeferAlloc;
