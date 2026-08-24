//! # sefer-alloc
//!
//! A safe, *handle-addressed* region store. Instead of handing out raw
//! pointers, a [`Region<T>`] hands out small generational [`Handle<T>`]
//! values; the bytes live in `slotmap`'s contiguous slot array, resolved by a
//! single indirection (see `docs/BENCHMARKS.md`). A stale handle does not
//! resolve to a live value for roughly `2^31` reuse cycles of that slot
//! (after ~2^31 cycles the generation wraps and a very old handle may
//! alias a later value). It returns `None`, never undefined behaviour.
//!
//! ## The engine
//!
//! The single-threaded core is a thin **typed membrane** over
//! [`slotmap`](https://crates.io/crates/slotmap): [`Region<T>`] wraps a
//! `slotmap::SlotMap<DefaultKey, T>` and [`Handle<T>`] is a `#[repr(C)]`
//! struct of three fields — a `slotmap::DefaultKey`, a `region_id:
//! NonZeroUsize` that identifies which `Region` instance minted the handle
//! (see I7 below), and a `PhantomData<fn() -> T>` — so handles stay
//! generic-over-`T` and typed (which raw slotmap keys are not) while also
//! rejecting cross-`Region` aliasing. `slotmap`'s `unsafe` owns the
//! generational layout — the free list, generation bump on
//! remove, and 32-bit generation wrap after ~2^31 cycles; this crate adds the typed
//! boundary and instance isolation while staying `#![forbid(unsafe_code)]`.
//!
//! [`Region`], [`Handle`], and [`SyncRegion`] now live in the `sefer-region`
//! crate alongside `aligned-vmem` / `numa-shim` / `malloc-bench-rs`. They are
//! re-exported here for backward compatibility.
//!
//! ## Scope (honest)
//!
//! This is an *application-level* store, not a drop-in global allocator. For a
//! process-wide allocator, use `SeferAlloc` (opt-in `production` feature) or
//! reach for `mimalloc`.
//!
//! ## Monitoring `SeferAlloc` in production (`stats()`)
//!
//! `SeferAlloc` exposes a cheap, process-wide diagnostic snapshot via
//! `SeferAlloc::stats` → `AllocStats`: cache
//! hit rates, cross-thread reclaim/overflow counts, and cumulative
//! segment/heap totals (`segments_reserved_total - segments_released_total`
//! is the live segment count — the field to alert on for a segment leak;
//! `foreign_or_unroutable_frees` is the field to alert on for a cross-thread-
//! free leak under an `alloc-global`-without-`alloc-xthread` misconfiguration,
//! and requires the `alloc-stats` feature to be populated).
//! `stats()` is a handful of relaxed atomic loads (no locks, no allocation),
//! safe to poll on a metrics-scrape timer (requires the `alloc-global` feature;
//! runnable form in `tests/sefer_alloc_examples.rs`):
//!
//! ```text
//! use sefer_alloc::SeferAlloc;
//!
//! #[global_allocator]
//! static GLOBAL: SeferAlloc = SeferAlloc::new();
//!
//! let stats = GLOBAL.stats();
//! let segments_live = stats
//!     .segments_reserved_total
//!     .saturating_sub(stats.segments_released_total);
//! println!("segments_live={segments_live} tcache_hits={}", stats.tcache_hits);
//! ```
//!
//! **Multi-thread footgun:** `alloc-global` without `alloc-xthread` has no
//! sound cross-thread free path — a block freed on a different thread than
//! it was allocated on leaks (safely, but permanently) instead of racing.
//! See `SeferAlloc`'s "Multi-thread safety" doc
//! section for the full explanation. Use `["alloc-global", "alloc-xthread"]`
//! (or the `production` bundle) for any real multi-threaded deployment.
//!
//! `SeferAlloc` (and the whole allocator stack) is **`std`-only** — it needs
//! thread-local storage and `std::time::Instant`. `Region<T>` / `Handle<T>`
//! (this crate's other face) are `no_std` + `alloc`-only and unaffected.
//!
//! See `docs/INVARIANTS.md` for the safety invariants this crate upholds and
//! `docs/ARCHITECTURE.md` for the architecture.
//!
//! ## Example
//!
//! ```text
//! use sefer_alloc::Region;
//!
//! let mut region = Region::new();
//! let a = region.insert("alpha");
//! let b = region.insert("beta");
//!
//! assert_eq!(region.get(a), Some(&"alpha"));
//!
//! region.remove(a);
//! assert_eq!(region.get(a), None); // stale handle → None, never UB
//! assert_eq!(region.get(b), Some(&"beta")); // others stay valid
//! ```
//!
//! Runnable form: `tests/region_invariants.rs`.

// ── Workspace: ten independently-publishable companion crates ────────────────
//
// The workspace extracted ten building blocks that can also be used
// standalone. Six are pulled into sefer-alloc's runtime dep tree under named
// feature gates; the other four are dev-only infra:
//
//   sefer-region       (crates/sefer-region)             — typed handle store (this re-export; runtime, no feature gate)
//   aligned-vmem       (crates/aligned-vmem)               — OS virtual-memory aperture          (feature: alloc-core)
//   numa-shim          (crates/numa-shim)               — NUMA detection + binding            (feature: numa-aware)
//   racy-ptr-cell      (crates/racy-ptr-cell)      — lazy CAS-published pointer cell     (feature: alloc-core)
//   size-classes       (crates/size-classes)       — const-built size-class tables       (feature: alloc-core)
//   tagged-index-stack (crates/tagged-index-stack) — ABA-tagged free-index stack         (feature: alloc-global)
//   malloc-bench-rs    (crates/malloc-bench-rs)       — portable GlobalAlloc bench harness  (dev-only)
//   globalalloc-model  (crates/globalalloc-model)  — differential op-stream test harness (dev-only)
//   proc-memstat       (crates/proc-memstat)       — same-instant RSS / commit self-probe (dev-only)
//   proc-probe         (crates/proc-probe)         — RESULT key=value stdout protocol    (dev-only)
//
// (`ring-mpsc`, a standalone bounded MPSC index ring, was removed from the
// workspace, task #655 — it had zero production consumers; the in-tree
// RemoteFreeRing/HeapOverflow swap it was extracted for was investigated and
// found NO-GO, see docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md.)
//
// ── Unsafe inventory — the complete, verifiable picture ───────────────────────
//
// Source of truth: `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`
// — two tiers in one command: `#![...]` matches are the module-level seams
// (tier 1), `#[...]` matches are item-scoped `unsafe fn` declarations and
// their internal call-site `unsafe {}` blocks (tier 2, task #101 / R4-9).
// Both are comment-proof: `^\s*#!?\[` requires the line to begin with the
// attribute, not a `//` prefix (the unanchored form has false positives here
// and in `src/registry/heap_overflow.rs`).
//
// EXTERNAL publishable crates (each independently auditable):
//
//   aligned-vmem  (crates/aligned-vmem/src/lib.rs)         — #![allow(unsafe_code)]
//     Sole purpose: SEGMENT-aligned mmap/VirtualAlloc + page decommit/recommit.
//     Entire crate = OS aperture. Small, single-responsibility. Audit in isolation.
//
//   numa-shim     (crates/numa-shim/src/lib.rs)         — #![allow(unsafe_code)]
//     Sole purpose: Linux mbind(2) via syscall(2), Windows VirtualAllocExNuma.
//     No libnuma dep. Small, single-responsibility. Audit in isolation.
//
//   malloc-bench-rs (crates/malloc-bench-rs/src/lib.rs) — #![allow(unsafe_code)]
//     Confined to alloc_block / free_block / drain_mailbox helpers plus one
//     `unsafe impl Send for Block` (the cross-thread ownership-transfer token);
//     every unsafe call carries a // SAFETY: comment. Bench harness, not runtime.
//     Callers must supply a stateless-facade `A` (see `run`'s contract doc).
//
//   racy-ptr-cell (crates/racy-ptr-cell/src/lib.rs) — #![allow(unsafe_code)]
//     Single documented reason: `unsafe impl Send/Sync` for the AtomicPtr-backed
//     cell + `NonNull::new_unchecked`. Lazy CAS-published pointer cell; every
//     site has `# Safety` / `// SAFETY:`. Pulled in under `alloc-core`.
//
//   globalalloc-model (crates/globalalloc-model/src/lib.rs) — #![allow(unsafe_code)]
//     Single documented reason: the `unsafe trait RawAllocator` (impls must
//     return valid pointers for the requested layout); every impl + call
//     carries `// SAFETY:`. Dev-only differential-test harness.
//
//   proc-memstat  (crates/proc-memstat/src/lib.rs)  — #![allow(unsafe_code)]
//     Sole purpose: OS-FFI self-probe (Windows K32GetProcessMemoryInfo, macOS
//     task_info, Linux /proc). Every block carries `// SAFETY:`. Dev-only.
//
//   sefer-region  (crates/sefer-region/src/lib.rs)        — #![forbid(unsafe_code)]
//     Handle<T> / Region<T> / SyncRegion<T>. Zero own unsafe; slotmap's
//     unsafe owns the generational layout. No version-scoped audit record
//     for slotmap is tracked by this project (see crates/sefer-region/README.md
//     "## Safety").
//
//   size-classes  (crates/size-classes/src/lib.rs)  — #![forbid(unsafe_code)]
//     const-evaluated size-class tables + derived O(1) size→class lookup +
//     alignment classifier. no_std, zero-dependency, no raw pointers anywhere.
//     sefer's size_classes.rs is a thin shim over it. Pulled in under `alloc-core`.
//
//   tagged-index-stack (crates/tagged-index-stack/src/lib.rs) — #![forbid(unsafe_code)]
//     ABA-tagged Treiber free-index stack via a single packed AtomicUsize head
//     word (monotonic tag in the high bits). no_std, no raw-pointer derefs.
//     sefer's registry free_slots uses it. Pulled in under `alloc-global`.
//
//   proc-probe    (crates/proc-probe/src/lib.rs)    — #![forbid(unsafe_code)]
//     The RESULT key=value stdout protocol + a re-export of proc-memstat's
//     snapshot. Pure protocol crate; the OS FFI stays in proc-memstat. Dev-only.
//
// INTERNAL sefer-alloc seams (compiler-enforced — a stray `unsafe` outside
// these named modules is a hard compile error in every configuration):
//
//  With NO features (or only `std`): `#![forbid(unsafe_code)]` — no `unsafe`
//  is possible anywhere in the crate.
//
//  With `experimental` (3b-II `crossbeam-epoch` tier) and/or `alloc-core`
//  (Phase 8 self-hosted segment substrate) and/or `alloc-global`
//  (Phase 11 `SeferAlloc` `GlobalAlloc` face): the crate is
//  `#![deny(unsafe_code)]` (any `unsafe` outside an allowed module is a hard
//  error), and the confined modules lift this with `#![allow(unsafe_code)]`:
//
//    Production path (`production` = alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory + primordial-lazy-commit + class-aware-dirty):
//      * `alloc_core::os`   — thin interop wrapper around aligned-vmem; any
//                             additional unsafe blocks carry `// SAFETY:` proof.
//                             (under `alloc-core`)
//      * `alloc_core::node` — intrusive free-list node r/w through raw pointers;
//                             the generalized `hand` discipline. (under `alloc-core`)
//      * `alloc_core::sidecar` — the owner-only lazily-materialised sidecar
//                             primitive (`reserve`/`deref`/`deref_mut`) shared
//                             by `os`'s SegmentDirectory reservation and
//                             `large_cache_extended`'s LargeCacheExtension
//                             reservation. (under `alloc-core`)
//      * `global::sefer_alloc` — the `unsafe impl GlobalAlloc` alloc-face seam
//                             (trait obligation + pointer handoff). (under `alloc-global`)
//      * `global::tls_heap`     — raw-pointer TLS binding + `AbandonGuard` seam.
//                             (under `alloc-global`)
//      * `global::fallback`     — primordial fallback heap seam —
//                             `static mut MaybeUninit<HeapCore>` + atomic-init
//                             state-machine + spinlock-guarded `&mut` handout.
//                             (under `alloc-global`)
//      * `registry::bootstrap`     — primordial-segment carve / SegmentTable
//                             bootstrap seam — raw-pointer footprint carving
//                             of the metadata region under the atomic
//                             single-writer bootstrap protocol.
//                             (under `alloc-global`)
//      * `registry::heap_slot`     — `Sync`/`Send` impls + `UnsafeCell` hand-off.
//                             (under `alloc-global`)
//      * `registry::heap_registry` — `*mut HeapCore` pointer handoff out of a slot.
//                             (under `alloc-global`)
//
//    Optional `numa-aware` path: no new unsafe seams — `alloc_core::numa`
//    is pure safe delegation to numa-shim (its test-only `bind_segment`
//    unsafe seam was removed in task #1306, together with numa-shim's
//    `bind_range`). (under `numa-aware`)
//
//    Optional `class-aware-dirty` path (R12-7 stage 2, EXPERIMENTAL):
//      * `alloc_core::dirty_by_class` — dereferences the `RacyPtrCell`-
//                             published per-(segment, class) dirty-bit
//                             sidecar pointer. (under `class-aware-dirty`)
//
//    Optional `large-cache-extended` path (R13-6):
//      * `alloc_core::large_cache_extended` — the lazily-materialised
//                             large-cache extension sidecar (owner-only, no
//                             `RacyPtrCell`); reserves via `sidecar::reserve`,
//                             dereferences via `sidecar::deref[_mut]`.
//                             (under `large-cache-extended`)
//
//    Research / older tiers (not in production build):
//      * `concurrent::hand`         — epoch-tier AtomicSlot<T>. (under `experimental`, legacy/research-tier)
//
//  So "the `unsafe` lives in named modules" is enforced by the compiler in
//  EVERY configuration. Verifiable: `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' src/ crates/`
//
// ── The soundness boundary is WIDER than the unsafe-syntax boundary ────────────
//
//  The confinement above is real, but it enforces the localization of unsafe
//  *syntax* only — it is NOT a claim that a bug outside these modules cannot
//  cause UB. The soundness boundary is the seams PLUS every safe membrane
//  function that calls into them with a documented PROSE contract: violating
//  that contract from safe code is UB even though no `unsafe` keyword appears
//  at the violation site. Concrete membranes to audit as part of the trusted
//  computing base:
//    * `alloc_core::node::{write_usize, write_struct, offset, zero, ...}` — safe
//      `pub(crate)` fns whose whole body is a raw r/w; soundness rests on the
//      caller's bounds/exclusivity/`'static` invariants stated in prose.
//    * `os::release_segment` — a safe fn; a double call (double-release) from
//      safe code is UB (the OS reservation is freed twice).
//    * `os::{decommit_pages, recommit_pages}` — safe fns; the range-containment
//      invariant is the caller's, unchecked.
//    * `registry::heap_slot::HeapSlot` — its `state`/`heap` single-writer
//      invariant (which the slot's `Sync` proof and the `claim`/`recycle`
//      protocol depend on) is a prose contract; a safe CAS of `state` LIVE→FREE
//      from the wrong place breaks it. (Non-test fields are `pub(crate)` to keep
//      this membrane inside the crate — see that module's M7 note.)
//  In short: the membrane pattern concentrates the *unsafe blocks* into a
//  small named set of `#![allow(unsafe_code)]` files (tier 1, inventoried
//  above) PLUS item-scoped `#[allow(unsafe_code)]` sites (tier 2 — individual
//  `unsafe fn` boundaries with unverifiable caller-pointer contracts, and the
//  `unsafe {}` blocks at their internal call sites; task #101 / R4-9), for
//  audit — but the *soundness argument* spans those safe callers too.
//  This is a deliberate, worthwhile trade — named here so a future editor does
//  not misread "no stray unsafe" as "no UB reachable from safe code".
#![cfg_attr(
    not(any(feature = "experimental", feature = "alloc-core")),
    forbid(unsafe_code)
)]
#![deny(unsafe_code)]
// The single-threaded core (`Region<T>` / `Handle<T>`) needs only `alloc` and
// builds under `no_std`. Disable the default `std` feature to drop `SyncRegion`
// and the concurrent/byte tiers and get the bare `no_std` + `alloc` core.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

// 0.3.0 (task A2) — defence-in-depth: `fastbin` is unsound without
// `alloc-xthread`. `Cargo.toml`'s `fastbin = ["alloc-global", "alloc-xthread"]`
// feature-unification is the primary fix (any normal `--features fastbin`
// build pulls `alloc-xthread` in automatically); this `compile_error!` is a
// belt-and-suspenders guard for the unlikely case someone builds with
// `--no-default-features --features fastbin` against a stale `Cargo.toml` /
// vendored copy, or a future edit accidentally drops the dependency again.
// Without `alloc-xthread`, a cross-thread free of a small block has no
// ownership-checked routing path (`dealloc_routing`'s owner-identity stamp
// and the per-segment `RemoteFreeRing` both live behind `alloc-xthread`), so
// a naive cross-thread free would write directly into another thread's
// private magazine/free-list — an unsynchronised data race, not a
// correctness nicety.
#[cfg(all(feature = "fastbin", not(feature = "alloc-xthread")))]
compile_error!(
    "sefer-alloc: `fastbin` requires `alloc-xthread` (cross-thread free \
     without it races the per-thread magazine/free-list — unsound). Enable \
     both, e.g. `--features fastbin,alloc-xthread`, or use the `production` \
     feature bundle."
);

// Phase 1: typed handle store, extracted to `sefer-region`. Re-exported here
// for backward compatibility — existing users of `sefer_alloc::{Region, Handle,
// SyncRegion}` continue to work unchanged. New consumers who want ONLY the
// handle store (no allocator stack) should depend on `sefer-region` directly.

#[cfg(feature = "experimental")]
mod concurrent;

// `alloc_core` is the Phase 8+ segment substrate. Its public surface is
// `AllocCore` / `SegmentLayout` (re-exported below via `pub use
// alloc_core::{...}`, which needs `mod alloc_core` in scope regardless of
// `internals`). With `internals` ON, the module is also `#[doc(hidden)] pub`
// so the isolated ring test (`tests/remote_ring_unit.rs`) can reach
// `alloc_core::remote_free_ring::RemoteFreeRing`'s `#[doc(hidden)]` test
// surface — this is the established test-only export pattern (see
// `registry` below). Nothing in `alloc_core` is stable public API.
//
// R34-3 (task #522, finding B1): `#[doc(hidden)]` alone hides this module
// from rustdoc but does NOT remove it from the public semver/ABI surface —
// see `SeferAlloc::alloc_batch`'s doc comment in `src/global/sefer_alloc.rs`
// for the same principle already applied to two individual methods. Two
// mutually-exclusive `mod alloc_core;` declarations below (one `pub`, one
// `pub(crate)`, `cfg`-disjoint on `internals`) give the module body itself
// unconditional visibility to the REST of this file (so the crate-root
// `pub use alloc_core::{AllocCore, SegmentLayout, ...}` re-exports keep
// working with `internals` OFF, unconditionally on `alloc-core`/
// `alloc-decommit` as before) while making the EXTERNAL path
// `sefer_alloc::alloc_core::*` — everything NOT re-exported at the crate
// root, i.e. every `dbg_*` hook and internal type — reachable only when
// `internals` is enabled. See `internals`'s own doc comment in `Cargo.toml`
// for the full boundary/decision writeup.
#[cfg(all(feature = "alloc-core", feature = "internals"))]
#[doc(hidden)]
pub mod alloc_core;
// `allow(dead_code, unused_imports)`: without `internals` this module's
// only external callers (integration tests reaching `dbg_*`/other
// non-re-exported items directly) are unreachable, so a handful of items
// (and the `mod.rs` `pub use` re-exports that surface them one level up)
// that exist solely for that white-box test surface would otherwise
// warn/error under `-D warnings`. This is a `pub(crate)`-visibility
// artifact of the `internals` gate, not new dead code — every one of these
// items is used by something outside `src/` (see `internals`'s doc comment
// in `Cargo.toml`) once `internals` is enabled, which is a real, exercised
// build
// configuration (`cargo test --features "production internals"`).
#[cfg(all(feature = "alloc-core", not(feature = "internals")))]
#[allow(dead_code, unused_imports)]
pub(crate) mod alloc_core;

// `#[doc(hidden)] pub` (when `internals` is on; `pub(crate)` otherwise — not
// private `mod`) so the task #129 teardown-ordering tests
// (`tests/tls_heap_teardown_torn_sentinel.rs`,
// `tests/tls_heap_teardown_ordering_stress.rs`) can reach `global::tls_heap`'s
// `#[doc(hidden)]` test hook `dbg_teardown_then_resolve_is_fallback` — the
// same established test-only export pattern as `alloc_core`/`registry`
// above. Nothing in `global` beyond `SeferAlloc`/`AllocStats` (re-exported
// below, unconditionally on `alloc-global`, independent of `internals`) is
// stable public API.
//
// R34-3 (task #522, finding B1): additive `internals` gate on the EXTERNAL
// path, same rationale and same two-declaration shape as `alloc_core` above
// — see `internals`'s doc comment in `Cargo.toml`.
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[doc(hidden)]
pub mod global;
// `allow(dead_code)`: same `internals`-off white-box-surface artifact as
// `alloc_core` above.
#[cfg(all(feature = "alloc-global", not(feature = "internals")))]
#[allow(dead_code, unused_imports)]
pub(crate) mod global;

// R34-3 (task #522, finding B1): additive `internals` gate on the EXTERNAL
// path, same two-declaration shape as `alloc_core` above — see
// `internals`'s doc comment in `Cargo.toml`.
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[doc(hidden)]
pub mod registry;
// `allow(dead_code)`: same `internals`-off white-box-surface artifact as
// `alloc_core` above.
#[cfg(all(feature = "alloc-global", not(feature = "internals")))]
#[allow(dead_code, unused_imports)]
pub(crate) mod registry;

pub use sefer_region::{Handle, Region};

#[cfg(feature = "std")]
pub use sefer_region::SyncRegion;

#[cfg(feature = "experimental")]
#[allow(deprecated)]
pub use concurrent::{
    EpochHandle, EpochRegion, LockFreeHandle, LockFreeRegion, ShardedHandle, ShardedRegion,
};

#[cfg(feature = "pinning")]
pub use concurrent::PinnedRunner;

#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::LargeCacheConfig;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::LargeCacheMode;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::LargeCachePolicy;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::Profile;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::SmallPoolPolicy;
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
pub use alloc_core::SmallSegmentPoolConfig;
#[cfg(feature = "alloc-core")]
pub use alloc_core::{AllocCore, SegmentLayout};

#[cfg(feature = "alloc-global")]
pub use global::{AllocStats, SeferAlloc};

#[cfg(kani)]
mod kani_proofs;
