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
//! **Reviewed release panic sites, not an exhaustive abort inventory.**
//! These checks are panic-capable source constructs, NOT explicit process
//! aborts. Their presence does not authorize unwinding from `GlobalAlloc`,
//! and a panic is not guaranteed to abort. The lexical regression guard
//! reviews feature-gated branches as well as the production bundle:
//!
//! - Four large-cache checks in `alloc_core/large/alloc_core_large_cache.rs`:
//!   two occupied-slot `expect`s in `large_cache_slot_take` and the
//!   extension-disabled range `unreachable!` arms in that function and
//!   `large_cache_slot_set`. All require `alloc-decommit`; the extension
//!   `expect` additionally requires `large-cache-extended`, while the range
//!   arms require its absence. Owner-only scans consult the slot array before
//!   taking an occupied entry; free-slot selection bounds insertion indices.
//! - `segment_header/terminal_words.rs::pack_large_state` asserts the packed
//!   generation bound. Fresh initialization uses 0/1; lifecycle transitions
//!   decode bounded generations and reuse uses fallible checked advancement.
//! - `alloc_core_small_magazine.rs::refill_class_bump_virgin_internal` asserts
//!   that output fits a `u16` mask (`alloc-xthread` + `fastbin` +
//!   `virgin-zero-skip`, the last not in `production`). Global allocation
//!   supplies a refill bounded by `TCACHE_CAP`, which is compile-time bounded
//!   by 16. Arbitrary oversized direct substrate calls are a different surface.
//! - `platform/numa.rs::reserve_aligned_on_node` has a `NodeId::new(node)`
//!   `expect` only in the non-sentinel branch (`numa-aware`, opt-in).
//! - `global/exact_object/exact_shard.rs::array_layout` has a descriptor-layout
//!   `expect` (`exact-object-proto`, opt-in). Initial capacity is 64 and rehash
//!   grows dynamically; there is no explicit checked capacity/layout bound.
//!   The lexical allowlist records this existing prototype limitation, not a
//!   proof that all growth is panic-free. No behavior is changed here.
//!
//! A former ownership assertion in `realloc_inplace_fast_path_known_base` was
//! first demoted to `debug_assert!`, then replaced by fallible
//! `canonical_base_of(key)?` using a payload-derived key. Missing or inconsistent
//! roots return `None`; Large resizing checks the exact `payload_offset`.
//! The guard pins that mechanism separately from the panic-site inventory.
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
//! Expected allocation failures use return-null paths, and failed in-place
//! resolution uses `None`. Registry claim uses `slot_or_none`: after chunk
//! OOM it first tries to recover an already-materialised FREE heap. If that
//! recovery fails, TLS binding selects fallback rather than invoking the
//! infallible registry abort. Config collisions are first-wins plus the
//! always-compiled [`AllocStats::config_conflicts`] counter, not an intentional
//! panic.
//!
//! **Explicit registry OOM abort.** `Registry::slot` / `ensure_chunk` retain
//! a direct `std::process::abort()` on chunk-materialisation failure for
//! infallible access, including diagnostic accessors. This is not the normal
//! fallible claim path and is not the only category of explicit abort.
//!
//! **Invariant aborts (representative, not a census).** Allocator-path
//! examples include route-slot issue/registration and route-directory
//! pin/reference-count consistency; Small sidecar/reclaim class/kind/offset
//! geometry and pointer reconstruction; Large terminal-state transitions;
//! live-count and registry lease ownership checks. The opt-in
//! `exact-object-proto` path aborts on duplicate live descriptors. Already-free
//! or magazine-resident Small records return `false` without reclaiming them.
//! These are not ordinary allocation OOM. Direct process abort does not
//! unwind or run the panic hook; panic-capable checks are a separate category.
//!
//! **Caller boundary.** Missing routes or rejected foreign publication can
//! drop a free (no-op), but this does not make arbitrary pointers safe.
//! Invalid layouts, stale/interior/unmapped pointers, wrong layouts and
//! double frees are unsupported caller misuse, not guaranteed no-op paths.
//! Debug assertions, indexing and the reviewed release panic sites remain
//! panic-capable; this documentation and lexical guard are not a total
//! no-panic proof, especially for opt-in prototype capacity growth.
//! If a panic fires, its outcome depends on the panic runtime and call
//! surface — NOT a guaranteed abort. The normative no-unwind obligation
//! remains; neither panic hooks nor std shims repair a violation.
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
