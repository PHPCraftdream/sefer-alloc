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
//! `bind_slow_tagged` claims a registry slot. M5's rule is narrow and
//! precise: NO allocator path may recurse into the SELECTED global
//! allocator (a `Box`/`Vec`/`format!` there would re-enter
//! `SeferAlloc::alloc` → `bind_slow_tagged` → …; see
//! `registry::heap_core`). It is NOT a zero-allocation rule: the first
//! routed construction of a heap materialises its terminal-route
//! bookkeeping BEFORE any issue — the slot's `RouteSlots` handle array,
//! each route entry, and its Small sidecar or Large descriptor word — via
//! explicit `System` allocations (`segment_table/route_slots.rs`,
//! `segment_route/directory.rs`) and OS-backed reservations, which bypass
//! the installed allocator by construction and therefore cannot recurse.
//! There is no per-thread handle left to install at claim time (the
//! former cross-thread free-stack plant is gone); the claim-time bind
//! (`bind_slot_counters`, called from `HeapRegistry::claim_lease`/
//! `claim_lease_with_config`) plants only the slot's diagnostic
//! hit-counter handles. TERMINAL publication is the allocation-free half
//! of the protocol: a foreign free resolves the block's route and
//! performs one RMW/CAS into the already-materialised independent
//! sidecar or descriptor word — lookup, removal, and pin release never
//! allocate (`segment_route/mod.rs`). The `HeapCore` alloc/dealloc paths
//! use no `std` collections on top of these seams; nothing reachable
//! from them touches the selected global allocator.
//!
//! ## No-panic discipline -- how it is upheld
//!
//! **Ordinary service failures do not panic.** `alloc`/`realloc` return null
//! on OOM or a layout they cannot serve. A missing foreign route or rejected
//! sidecar publication drops the free rather than mutating another heap;
//! this defensive behavior does not make an invalid `dealloc` pointer or
//! `Layout` a valid `GlobalAlloc` call. `alloc_zeroed` delegates to
//! `HeapCore::alloc_zeroed` (an explicit zero-fill for a reused/non-virgin
//! block; under the opt-in `virgin-zero-skip` feature, a genuinely virgin
//! bump-carved block skips the fill entirely — see that method's own doc):
//! - `alloc`: `current_for_alloc()` → `&mut HeapCore` → `HeapCore::alloc`
//!   (returns null on OOM). If `current_for_alloc()` itself yields the
//!   fallback (TLS teardown), the fallback's `with_heap` returns `None` only
//!   on true OOM → null.
//! - `dealloc`: resolves via the DEALLOC-ONLY
//!   [`tls_heap::current_for_dealloc`](super::tls_heap::current_for_dealloc)
//!   (not `current_for_alloc`), which never claims a registry slot just to
//!   free a pointer. `CurrentHeapForDealloc::Own` routes to that heap's
//!   `HeapCore::dealloc` (own-thread or cross-thread via `dealloc_routing` —
//!   `alloc-global` unconditionally implies `alloc-xthread`, R5-01);
//!   `ForeignNoBind` (TLS never bound, or torn down) calls the heap-instance-
//!   independent `HeapCore::publish_foreign`, without constructing a
//!   `*mut HeapCore`. It looks up the address in the route directory and
//!   publishes through an independently pinned sidecar; an idle fallback can
//!   reclaim synchronously. A missing route or rejected publication drops the
//!   free and increments `foreign_or_unroutable_frees`. The caller's live-
//!   allocation/`Layout` contract remains required; this is not a promise
//!   that invalid or unmapped pointers are safe to pass.
//! - `realloc`: an in-place fast path for same-class / compatible growth (C2:
//!   own-thread reallocs delegate to `AllocCore::realloc`, which short-circuits
//!   when the block can stay put), falling back to `alloc` + copy + `dealloc`
//!   otherwise — all null-returning.
//! - `alloc_zeroed`: `HeapCore::alloc_zeroed` — explicit zero-fill on a
//!   reused/non-virgin block, or (opt-in `virgin-zero-skip`) a skipped fill
//!   on a genuinely virgin bump-carved block.
//!
//! **Four release-surviving invariant tripwires (fail-loud by design,
//! unreachable under correct operation).** Beyond
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
//! reachable from `global_alloc.rs`'s `GlobalAlloc` impl under `production`:
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
//!   first demoted to `debug_assert!` (#1984, alloc-core perf review P1-2):
//!   both callers already prove membership, so a release panic and duplicate
//!   probe were unnecessary. It was later replaced by fallible
//!   `canonical_base_of(key)?`, using a payload-derived segment key. A missing
//!   address or a supplied base inconsistent with the key/root returns `None`
//!   without panicking. A hit supplies the table's allocator-owned root for
//!   block reconstruction and metadata reads; Large in-place growth also
//!   checks the reconstructed pointer against the header's `payload_offset`
//!   before changing its size. There is no remaining debug-only re-probe.
//!   `tests/no_panic_doc_accuracy.rs` pins this fallible root resolution and
//!   the absence of a release `assert!` in that file.
//!
//! **`GlobalAlloc` methods must not unwind — upheld at the source, NOT
//! delegated to the std shims (R2-08).** `GlobalAlloc`'s safety contract
//! forbids unwinding out of `alloc` / `dealloc` / `realloc` / `alloc_zeroed`,
//! unconditionally. This crate does not rely on the std `__rust_alloc` /
//! `__rust_dealloc` / `__rust_realloc` / `__rust_alloc_zeroed` shims being
//! `#[rustc_nounwind]` to make an escaping panic harmless:
//!
//! - a DIRECT trait call (`GlobalAlloc::alloc(&instance, layout)`, generic
//!   `A: GlobalAlloc` or `&dyn GlobalAlloc` code) never passes through those
//!   shims at all;
//! - even on the `#[global_allocator]` path the marking is not an
//!   abort-on-unwind guarantee to lean on: on rustc 1.97.0
//!   (x86_64-pc-windows-msvc, debug) the pre-R2-08 config-conflict panic
//!   unwound straight through `__rust_alloc` and `alloc::alloc::Global` to
//!   the thread boundary instead of aborting
//!   (`tests/regression_r2_08_global_allocator_path_no_unwind.rs` records
//!   the scenario);
//! - either way the panic runtime first runs the panic hook (the default hook
//!   can allocate; a user hook may do anything) and, when unwinding, boxes
//!   the panic payload through the global allocator — re-entering this
//!   allocator mid-operation — before any abort could happen.
//!
//! So the guarantee is made where the code is: no path reachable from a
//! `GlobalAlloc` method by a contract-respecting caller — steady state or the
//! cold TLS bind / registry-claim path, debug or release, `panic = "unwind"`
//! or `"abort"` — panics. Expected-but-unusual conditions are signalled
//! without panicking: OOM → null; an unrecognised pointer → no-op; a
//! multi-instance config collision on a recycled registry slot → first-wins
//! plus the always-compiled [`AllocStats::config_conflicts`] counter (a
//! former debug-build `debug_assert!` there unwound out of
//! `GlobalAlloc::alloc` — R2-08, `tests/regression_r2_08_globalalloc_no_unwind.rs`).
//! The one deliberate process kill on the alloc path is a direct
//! `std::process::abort()` (registry chunk-materialisation OOM,
//! `registry/bootstrap/registry.rs`), which neither unwinds nor runs the
//! panic hook. What remains panic-capable on these paths is internal-invariant
//! checking only — the `debug_assert!`s, bounds-checked indexing / `expect`s
//! on internally-derived indices (e.g. a size-class index), and the four
//! release tripwires above — none of which a contract-respecting caller
//! (valid non-zero-size `Layout`, live pointer, any configuration) can reach
//! without a bug in this crate having already corrupted allocator metadata.
//! Should one ever fire, its outcome is whatever the panic runtime does on
//! the given call surface — NOT a guaranteed abort.
//!
//! [`AllocStats::config_conflicts`]: crate::AllocStats::config_conflicts
//!
//! [`current_for_alloc`]: super::tls_heap::current_for_alloc

#[cfg(feature = "batch-api")]
mod batch;
mod core;
mod diag;
mod global_alloc;
mod maintenance;

pub use core::SeferAlloc;
