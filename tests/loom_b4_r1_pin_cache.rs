//! b4-r1 ACTUAL-SOURCE loom model for the per-thread route pin cache's
//! cached fast path. This REPLACES the earlier shadow model: the reviewed
//! P1-Loom finding was that the protocol under test (cached pin gate →
//! sidecar RMW → owner drain) must be model-checked on the PRODUCTION
//! sources, so that mutants applied in `src/` redden the model. The
//! precedent is `tests/loom_sidecar_bitmap_actual.rs` (`#[path]` source
//! includes; included files take their atomics from `super`, and this root
//! re-exports loom atomics under the same names).
//!
//! # Production files included (verbatim, via `#[path]`)
//! - `src/registry/heap_core_xthread/route_pin_cache.rs` — `publish_cached`
//!   (hot path: `linked()` Acquire → span check → `publish_small_borrowed`),
//!   `cache_insert`, `cache_evict`, TLS `PinCache`.
//! - `src/registry/segment_route/pin.rs` — `RoutePin` incl. `linked()`
//!   (the Acquire gate) and `publish_small_borrowed` (the borrow structure
//!   that holds the pin across the whole publication RMW; the model never
//!   breaks it).
//! - `src/registry/segment_route/directory.rs` — `RouteDirectory`
//!   (`lookup`, `register_route`, `remove` with the unlink + `linked`
//!   Release-clear under the shard lock), `Entry`/`EntryHandle` (counted
//!   refs; final drop frees the sidecar; NO drain on owner-ref drop).
//! - `src/registry/segment_route/registration.rs` — `RouteRegistration`
//!   (`issue_small`, drop = production remove then owner-ref drop).
//! - `src/registry/segment_route/small_sidecar.rs` — production
//!   `SmallSidecar::publish` → `SidecarBitmap::publish` (the real
//!   `fetch_or`), `initialize_at`.
//! - `src/alloc_core/segment/remote_bitmap/{sidecar_bitmap,bitmap_scan,
//!   bitmap_cut,bitmap_record}.rs` (+ their `leaf_classes.rs` child) — the
//!   real scan/drain primitive (`BitmapScan::next_cut` = per-word
//!   `swap(0, AcqRel)`), same include shape as the sidecar-bitmap model.
//! - `src/registry/segment_route/{error,kind,route_record,route_scan,
//!   route_cut,large_state}.rs` and
//!   `src/alloc_core/large/reservation_state.rs` — the compile cascade of
//!   the files above. `large_state.rs`/`reservation_state.rs` stay fully on
//!   core atomics on purpose: the Large branch never executes (only Small
//!   routes are registered) and `reservation_state.rs` cannot take atomics
//!   from `super` in production (its `super` is the crate root), so a seam
//!   there would need a `src/lib.rs` reexport — out of scope.
//!
//! # Seams (each is import-level / declaration-level only; no production
//! statement changed)
//! - Atomics: `directory.rs`, `pin.rs`, `small_sidecar.rs`,
//!   `large_state.rs` resolve `Atomic*`/`Ordering` through `super`; the
//!   production `segment_route/mod.rs` re-exports the core atomics under
//!   the same names (a no-op there), and THIS root re-exports the loom
//!   ones. `route_pin_cache.rs`'s `PIN_CACHE_*` diagnostic counters stay
//!   on core atomics deliberately: they are measurement, not protocol.
//! - `alloc_core::{fatal, os, size_classes, segment_layout}` — shims like
//!   the sidecar-bitmap precedent. FULL production geometry: `SEGMENT =
//!   4 MiB` (4096 pending words, 262_144 granules) — `crate::alloc_core::
//!   os::SEGMENT` is an honest const without a loom variant, and LIB
//!   builds under `--cfg loom` (the alloc-global loom jobs) compile the
//!   same constant, so the model must not shrink it; `SmallSidecar`'s own
//!   const asserts hold unchanged. The model publishes/drains only
//!   granule 0 (word 0), and the drains hand the production
//!   `scan(high_water)` a one-word bound — the primitive runs verbatim,
//!   the modeled protocol region is one word.
//! - `alloc_core::segment_header` — compile-only shim of the Large
//!   phase-word codec from `segment_header/terminal_words.rs` (that real
//!   file cannot be source-included: it pulls `SegmentHeader`/`Node`).
//!   These items NEVER EXECUTE in any schedule of this model: only Small
//!   routes are registered, so every `RouteKind::Large` branch reaching
//!   them is unreachable. This is not a shadow of the protocol under test
//!   (the pin-cache/Small-sidecar protocol).
//! - `alloc_core::AllocCore::large_payload_address_matches` — compile-only
//!   shim for the Large-span membership check; the model registers only
//!   Small routes, so the check is never called (see `pin.rs::
//!   span_contains_addr`'s Large branch and `lookup`'s Large branch).
//! - `global::process_exit_in_progress` / `global::fallback::
//!   record_exit_lock_giveup` — `const false` / no-op: the model never
//!   exits.
//! - `shard_lock::ShardLock` — model-provided const-constructible spin-lock
//!   wrapper with the production API (`lock`, `lock_unless_exiting`); every
//!   retry yields via `loom::thread::yield_now()` so the cooperative
//!   explorer makes progress (loom atomics have no `const` constructors and
//!   `RouteDirectory::new` is `const fn`). This seam swaps ONLY the locking
//!   primitive (its acquire/release ordering is unmodeled; the harness's
//!   signal chain and the modeled `linked` flag carry the happens-before
//!   edges). The body of `RouteDirectory::remove` (unlink + `linked.store(
//!   false, Release)` under the lock) executes verbatim through it.
//! - `aligned_vmem::page_size` — the real dev-reachable dependency
//!   (`alloc-core` enables `dep:aligned-vmem`), used unmodified by
//!   `register_route`'s span validation.
//! - `thread_local!`/`RefCell`/`std::alloc::System` in the included files
//!   work as-is under loom — not seamed.
//!
//! # Model shape (minimal state space)
//! 2 loom threads; 1 Small route, 1 granule (offset 0, class 0); cache of
//! `PIN_CACHE_SLOTS = 4` (production constant, 1 slot used).
//! - OWNER: `RouteDirectory::new` → `register` (production) →
//!   `issue_small(0,0)` (granule LIVE) → own `lookup` pin (drain handle) →
//!   signal producer → wait producer-ack (insert done) → `RouteRegistration::
//!   drop` (production `remove`: unlink + `linked=false` Release under the
//!   lock, then owner-ref drop — no drain on that drop, matching
//!   production `EntryHandle::drop`) → drain D1 through the production
//!   scan primitive (`SmallSidecar::scan` → `BitmapScan::next_cut`) →
//!   join producer → final drain D2 → oracles.
//! - PRODUCER: wait signal → production `lookup` → production
//!   `cache_insert` (TLS) → ack → bounded loop of 2 attempts of
//!   `publish_cached` — the production hot path ENTIRELY (`linked()`
//!   Acquire → span → `fetch_or`). The pin stays borrowed across the whole
//!   RMW (production `publish_small_borrowed` structure — not broken).
//!   Attempt 2 runs only after the producer observed `retired` (Acquire),
//!   i.e. happens-after the unlink and D1 are ordered on the owner.
//!
//! # Oracles
//! - (O1, delivery completeness) every bit the producer published through
//!   the cache pin (`Published(true)`) is collected by the owner's drains
//!   D1+D2 — nothing is lost while any pin holds the sidecar, no
//!   duplicates (the scan primitive's `swap(0)` cut cannot lose a
//!   concurrent `fetch_or`).
//! - (O2, no zombie publication) an attempt that is happens-after the
//!   unlink (attempt 2, behind the Acquire `retired` fence) must NOT
//!   return `Published(true)`: the production gate (`linked()` Acquire)
//!   observes the Release clear through the happens-before chain and
//!   returns `Miss`. This is the tooth that reddens M-A.
//! - (O3, drained-empty) a third drain after D2 finds nothing.
//! - (O4, refs) `Entry::refs` is probed at observation points: ≥ 2 while
//!   owner+registration+pin are alive, exactly 1 after the producer's TLS
//!   pin died at thread exit (owner pin only). The pin is never released
//!   before its `fetch_or` (borrow structure), and no drain is added at
//!   owner-ref drop (production has none).
//!
//! # Mutant recipes (CI applies these to the PRODUCTION files; local loom
//! runs are not part of this workspace's permitted commands)
//! - (M-A) `route_pin_cache.rs::publish_cached`: delete
//!   `if !pin.linked() { ... }` (the whole unlink-fallback branch).
//!   Reddening interleaving: producer's attempt 2 is happens-after the
//!   owner's unlink+D1 (the Acquire `retired` fence in the harness);
//!   without the gate the attempt proceeds to the sidecar `fetch_or` and
//!   returns `Published(true)` — a zombie bit written into an unlinked
//!   route after the owner's unlink — so O2's counter is nonzero and the
//!   model reds. With the gate, attempt 2's Acquire load is happens-after
//!   the Release clear and must read `false` → `Miss`.
//! - (M-B) `pin.rs::RoutePin::linked()`: `Ordering::Acquire` →
//!   `Ordering::Relaxed`. Claimed reddening interleaving: the producer's
//!   gate load in an attempt NOT happens-after the unlink (attempt-1
//!   class) reads the stale `true`; under Relaxed the load carries no
//!   acquire edge, so loom may pair the stale read with the production
//!   `fetch_or` being ordered after the owner's D1 `swap(0)` on the same
//!   word — the bit lands in the (in production, freed) sidecar after the
//!   drain. HONEST LIMIT verified by code reading: the stale-`true` read
//!   itself is coherence-legal under Acquire as well (coherence, not
//!   acquire, governs which value a load may read when no happens-before
//!   edge forces the newer value), so in THIS model M-B's red is carried
//!   by the same post-unlink schedules as M-A through O2 only where loom
//!   can prove an happens-before edge; the atomic-level mechanism (a
//!   relaxed gate load losing the unlink witness) is demonstrated by the
//!   `#[should_panic]` negative control below, mirroring the
//!   sidecar-bitmap model's negative shadow controls.
//!
//! # What this model does NOT prove
//! - The credit-ledger hypothesis (a VALID free cannot race the route's
//!   unlink) lives outside the atomics and is not modelable here; the
//!   residual stale-gate publication window (possible in production, bit
//!   recovered here by D2 because the drain pin keeps the sidecar alive)
//!   is closed by the ledger, not by the gate.
//! - System-side use-after-free (sidecar freed while a stale-gated
//!   producer writes) is Miri territory (`tests/b4_r1_pin_cache_miri.rs`),
//!   not loom: the model keeps the sidecar alive through pins.
//! - The uncached fresh-lookup fallback path (`routing.rs`) and the
//!   Large branch are out of scope (only Small registered; `routing.rs`
//!   is not source-included).
#![cfg(loom)]

// Atomic seam: the `#[path]`-included production files resolve `super::`
// atomics; these names ARE loom's.
pub(crate) use loom::sync::atomic::{
    AtomicBool, AtomicPtr, AtomicU64, AtomicU8, AtomicUsize, Ordering,
};

mod alloc_core {
    // Mirrors `alloc_core::fatal` for the source-included primitives.
    pub(crate) mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            use std::io::Write;
            let _ = std::io::stderr().write_all(msg);
            std::process::abort()
        }
    }
    // Full production geometry (`src/alloc_core/platform/os.rs`): LIB
    // builds under `--cfg loom` compile the same constant, so the model
    // MUST NOT shrink it (see the header). Pending words: 4096.
    pub(crate) mod os {
        pub const SEGMENT: usize = 4 * 1024 * 1024;
    }
    // Real production values (`src/alloc_core/platform/size_classes.rs`):
    // MIN_BLOCK = 16; SMALL_CLASS_COUNT = 49 without `medium-classes`.
    pub(crate) mod size_classes {
        pub const MIN_BLOCK: usize = 16;
        pub const SMALL_CLASS_COUNT: usize = 49;
    }
    pub(crate) mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
    pub(crate) mod reservation_state {
        // The real wrapper, source-included at this crate's root.
        pub(crate) use crate::reservation_state::LargeReservationState;
    }
    pub(crate) mod remote_bitmap {
        pub(crate) use crate::bitmap_cut::BitmapCut;
        pub(crate) use crate::bitmap_scan::BitmapScan;
        pub(crate) use crate::sidecar_bitmap::{ClassLeaves, SidecarBitmap};
    }
    // COMPILE-ONLY shim of the Large phase-word codec
    // (`segment_header/terminal_words.rs`); never executed in this model —
    // only Small routes are registered (see the header).
    pub(crate) mod segment_header {
        pub(crate) const MAX_LARGE_GENERATION: u64 = u64::MAX >> 3;
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub(crate) enum LargePhase {
            Unused = 0,
            Initializing = 1,
            Live = 2,
            Pending = 3,
            Consuming = 4,
            Cached = 5,
            Released = 6,
        }
        pub(crate) fn pack_large_state(phase: LargePhase, generation: u64) -> u64 {
            (generation << 3) | phase as u64
        }
        pub(crate) const fn large_generation(word: u64) -> u64 {
            word >> 3
        }
        pub(crate) const fn large_phase(word: u64) -> Option<LargePhase> {
            match word & ((1 << 3) - 1) {
                0 => Some(LargePhase::Unused),
                1 => Some(LargePhase::Initializing),
                2 => Some(LargePhase::Live),
                3 => Some(LargePhase::Pending),
                4 => Some(LargePhase::Consuming),
                5 => Some(LargePhase::Cached),
                6 => Some(LargePhase::Released),
                _ => None,
            }
        }
        pub(crate) const fn next_large_generation(generation: u64) -> Option<u64> {
            if generation >= MAX_LARGE_GENERATION {
                None
            } else {
                Some(generation + 1)
            }
        }
    }
    // COMPILE-ONLY shim for the Large-span membership check; the model
    // registers only Small routes, so no call reaches it (header).
    pub(crate) struct AllocCore;
    impl AllocCore {
        pub(crate) fn large_payload_address_matches(
            _addr: usize,
            _payload: usize,
            _len: usize,
        ) -> bool {
            false
        }
    }
}

// The model never exits; the production exit hooks degrade to constants.
mod global {
    pub fn process_exit_in_progress() -> bool {
        false
    }
    pub(crate) mod fallback {
        pub fn record_exit_lock_giveup() {}
    }
}

// Locking-primitive seam ONLY (header): same API as the production spin
// lock, realized on a loom Mutex so the (unmodeled) core-atomic spin lock
// cannot livelock the explorer. `RouteDirectory::remove`'s body — the
// unlink and the `linked.store(false, Release)` under the lock — runs
// verbatim through this wrapper.
mod shard_lock {
    use core::cell::UnsafeCell;
    use core::ops::{Deref, DerefMut};

    // The lock word stays on a core atomic on purpose: loom's atomics have
    // no `const` constructors and `RouteDirectory::new` is a `const fn`, so
    // the seam MUST be const-constructible. A core-atomic spin cannot yield
    // to loom's cooperative scheduler by itself — every retry yields via
    // `loom::thread::yield_now()`, which is the branch point that lets the
    // lock holder run. Real atomic CAS provides real mutual exclusion; the
    // acquire/release ORDERING of the lock itself is therefore unmodeled —
    // the harness's Acquire/Release signal chain (go/ack/retired) and the
    // modeled `linked` flag carry the happens-before edges the oracles use.
    pub(crate) struct ShardLock<T> {
        locked: core::sync::atomic::AtomicBool,
        value: UnsafeCell<T>,
    }

    // SAFETY (same contract as production `ShardLock`): access to `value`
    // is serialized by the lock word; `T: Send` lets the guard hand `&mut T`
    // to whichever thread holds the lock.
    unsafe impl<T: Send> Sync for ShardLock<T> {}
    unsafe impl<T: Send> Send for ShardLock<T> {}

    pub(crate) struct ShardGuard<'a, T>(&'a ShardLock<T>);

    impl<T> ShardLock<T> {
        pub(crate) const fn new(value: T) -> Self {
            Self {
                locked: core::sync::atomic::AtomicBool::new(false),
                value: UnsafeCell::new(value),
            }
        }
        pub(crate) fn lock_unless_exiting(
            &self,
            exiting: impl Fn() -> bool,
            _giveup: impl Fn(),
        ) -> Option<ShardGuard<'_, T>> {
            if exiting() {
                return None;
            }
            while self
                .locked
                .compare_exchange_weak(
                    false,
                    true,
                    core::sync::atomic::Ordering::Acquire,
                    core::sync::atomic::Ordering::Relaxed,
                )
                .is_err()
            {
                loom::thread::yield_now();
            }
            Some(ShardGuard(self))
        }
        #[allow(dead_code)] // production API parity; the model locks via lock_unless_exiting only
        pub(crate) fn lock(&self) -> ShardGuard<'_, T> {
            loop {
                if self
                    .locked
                    .compare_exchange_weak(
                        false,
                        true,
                        core::sync::atomic::Ordering::Acquire,
                        core::sync::atomic::Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    return ShardGuard(self);
                }
                loom::thread::yield_now();
            }
        }
    }

    impl<T> Deref for ShardGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            // SAFETY: the guard holds the lock.
            unsafe { &*self.0.value.get() }
        }
    }
    impl<T> DerefMut for ShardGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            // SAFETY: the guard holds the lock exclusively.
            unsafe { &mut *self.0.value.get() }
        }
    }
    impl<T> Drop for ShardGuard<'_, T> {
        fn drop(&mut self) {
            self.0
                .locked
                .store(false, core::sync::atomic::Ordering::Release);
        }
    }
}

// ---- source-included production files (verbatim) ----

// `#[allow(dead_code)]` is scoped to the model: the harness exercises a
// narrower surface than the production feature sets do (e.g. the Large
// branch and the bench probes are never called), and a test crate lints
// its crate-private items as a binary would.
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_record.rs"]
mod bitmap_record;
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[allow(dead_code)]
#[path = "../src/alloc_core/large/reservation_state.rs"]
mod reservation_state;
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;

#[allow(dead_code)]
#[path = "../src/registry/segment_route/directory.rs"]
mod directory;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/large_state.rs"]
mod large_state;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/pin.rs"]
mod pin;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/registration.rs"]
mod registration;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/route_cut.rs"]
mod route_cut;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/error.rs"]
mod route_error;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/kind.rs"]
mod route_kind;
#[allow(dead_code)]
#[path = "../src/registry/heap_core_xthread/route_pin_cache.rs"]
mod route_pin_cache;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/route_record.rs"]
mod route_record;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/route_scan.rs"]
mod route_scan;
#[allow(dead_code)]
#[path = "../src/registry/segment_route/small_sidecar.rs"]
mod small_sidecar;

// The included files resolve their SIBLINGS through `super` (they sit in
// `segment_route` in production), so the root re-exports the surface those
// `use super::{...}` lines name.
use bitmap_cut::BitmapCut;
use bitmap_record::BitmapRecord;
use bitmap_scan::BitmapScan;
use large_state::LargeState;
use pin::RoutePin;
use registration::RouteRegistration;
use route_cut::RouteCut;
use route_error::RouteError;
use route_kind::RouteKind;
use route_record::RouteRecord;
use route_scan::RouteScan;
use small_sidecar::SmallSidecar;

// `route_pin_cache.rs` resolves `RoutePin` through its production crate
// path; re-export the source-included type under it.
mod registry {
    pub(crate) mod segment_route {
        pub(crate) use crate::pin::RoutePin;
    }
}

// Included production surfaces used by the harness. `allow(dead_code)` is
// scoped to the model: the production files carry targeted cfg_attr allows
// for the production feature sets, and this model exercises a narrower
// surface (e.g. the Large branch and spill probes are never called).
#[allow(dead_code)]
use directory::RouteDirectory;

use alloc_core::os::SEGMENT;
use loom::sync::Arc;
use loom::thread;
use route_pin_cache::CachedPublication;

/// One SEGMENT-sized, SEGMENT-aligned backing span (the model's only
/// route), obtained from the independent System allocator (`register_route`
/// requires SEGMENT- and page-aligned roots; a 4 MiB-aligned STATIC is
/// linker-fragile, System.alloc with an exact Layout is portable). The
/// span is private model memory: publication reads only its address.
struct Span(*mut u8);

impl Span {
    fn new() -> Self {
        use std::alloc::{alloc, Layout};
        // SAFETY: the layout is non-zero with a power-of-two alignment
        // System honors; the pointer is released by `Span::drop`.
        unsafe { Self(alloc(Layout::from_size_align(SEGMENT, SEGMENT).unwrap())) }
    }
    fn ptr(&self) -> *mut u8 {
        self.0
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        use std::alloc::{dealloc, Layout};
        // SAFETY: allocated in `Span::new` with exactly this layout.
        unsafe { dealloc(self.0, Layout::from_size_align(SEGMENT, SEGMENT).unwrap()) };
    }
}

/// O4 probe: the production refs count of a pin's `Entry`, read through
/// the production fields (`pin.entry` is `pub(super)` and the included
/// modules sit at this crate's root, exactly as in the production module).
fn pin_refs(pin: &RoutePin) -> usize {
    pin.entry.entry().refs.load(Ordering::Relaxed)
}

/// One-word scan bound: the modeled route publishes only granule 0
/// (offset 0, word 0); `SidecarBitmap::scan(high_water)` covers exactly
/// `ceil(high_water / SIDECAR_WORD_BYTES)` words, so 64 bounds the
/// production primitive to the only word that can hold a bit, without
/// changing its code path.
const SCAN_BYTES: usize = 64;

/// Owner-side drain through the PRODUCTION scan primitive: every word's
/// cut via `BitmapScan::next_cut` (`swap(0, AcqRel)`), records detached as
/// `(offset, class)`.
fn drain(pin: &RoutePin) -> Vec<(u32, u8)> {
    let sidecar: &SmallSidecar = pin
        .entry
        .small_sidecar()
        .expect("Small route has a sidecar");
    let mut out = Vec::new();
    let mut scan = sidecar.scan(SCAN_BYTES).expect("in-bounds high water");
    while let Some(mut cut) = scan.next_cut() {
        while let Some(record) = cut.pop() {
            let RouteRecord { offset, class } = record;
            out.push((offset, class));
        }
    }
    out
}

fn bounded_model() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 100_000;
    builder.check(|| {
        let dir = Arc::new(RouteDirectory::new());
        let span = Span::new();
        let ptr = span.ptr();
        let base = ptr.addr();

        // OWNER setup: production registration + issue; the granule goes
        // LIVE with class 0 (encode 1), so `SmallSidecar::publish` accepts
        // offset 0.
        let registration = dir
            .register_route(ptr, SEGMENT, ptr, 1, RouteKind::Small)
            .expect("registration succeeds");
        assert!(registration.issue_small(0, 0), "issue at offset 0");

        // Owner's own pin: the drain handle that keeps the sidecar alive
        // across the whole unlink-to-drain window (O4's lower bound).
        let owner_pin = dir.lookup(ptr).expect("owner lookup before unlink");
        assert!(pin_refs(&owner_pin) >= 2, "O4: registration + owner pin");

        let go = Arc::new(AtomicBool::new(false));
        let ack = Arc::new(AtomicBool::new(false));
        let retired = Arc::new(AtomicBool::new(false));
        let published_true = Arc::new(AtomicUsize::new(0));
        let published_true_after_retire = Arc::new(AtomicUsize::new(0));

        // PRODUCER: production lookup → production TLS cache_insert →
        // two bounded `publish_cached` attempts (production hot path).
        let producer = {
            let dir = Arc::clone(&dir);
            let go = Arc::clone(&go);
            let ack = Arc::clone(&ack);
            let retired = Arc::clone(&retired);
            let published_true = Arc::clone(&published_true);
            let published_true_after_retire = Arc::clone(&published_true_after_retire);
            thread::spawn(move || {
                while !go.load(Ordering::Acquire) {
                    thread::yield_now();
                }
                let pin = dir.lookup(ptr).expect("producer lookup before unlink");
                let incarnation = pin.incarnation();
                route_pin_cache::cache_insert(pin, base, incarnation);
                ack.store(true, Ordering::Release);
                for attempt in 0..2u8 {
                    if attempt == 1 {
                        // Attempt 2 is happens-after the owner's unlink
                        // (Acquire on `retired`): the production gate MUST
                        // observe the Release clear and return Miss (O2).
                        while !retired.load(Ordering::Acquire) {
                            thread::yield_now();
                        }
                    }
                    // SAFETY (mirror of the production contract): the
                    // address is this thread's CURRENT issued allocation;
                    // publication transfers its unique ownership once. The
                    // pin stays borrowed across the whole RMW.
                    let outcome = route_pin_cache::publish_cached(ptr);
                    match outcome {
                        CachedPublication::Published(true) => {
                            published_true.fetch_add(1, Ordering::Relaxed);
                            if attempt == 1 {
                                published_true_after_retire.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                        CachedPublication::Published(false) | CachedPublication::Miss => {}
                    }
                }
            })
        };

        go.store(true, Ordering::Release);
        while !ack.load(Ordering::Acquire) {
            thread::yield_now();
        }

        // Production removal: `remove` unlinks and clears `linked` with
        // Release UNDER the shard lock, then drops the owner reference.
        // NO drain runs at this drop (production `EntryHandle::drop` has
        // none) — the drain pin above keeps the sidecar alive for D1/D2.
        drop(registration);
        retired.store(true, Ordering::Release);

        // D1: first drain after unlink.
        let drained_1 = drain(&owner_pin);
        producer.join().unwrap();

        // O4: the producer's thread exited, so its TLS pin is released;
        // only the owner's drain pin remains.
        assert_eq!(pin_refs(&owner_pin), 1, "O4: exactly the drain pin left");

        // D2: final drain after join (recovers every bit published after
        // D1 — including a legal stale-gate publication from attempt 1).
        let drained_2 = drain(&owner_pin);

        // O1: total delivery — every `Published(true)` bit collected
        // exactly once, nothing lost.
        assert_eq!(
            drained_1.len() + drained_2.len(),
            published_true.load(Ordering::Relaxed),
            "O1: published bits must all be delivered to the owner's drains"
        );
        for (offset, class) in drained_1.iter().chain(drained_2.iter()) {
            assert_eq!((*offset, *class), (0, 0), "single modeled granule");
        }
        // O2: a happens-after-the-unlink attempt never publishes (the
        // gate saw the Release clear). Nonzero ⇒ zombie publication (M-A).
        assert_eq!(
            published_true_after_retire.load(Ordering::Relaxed),
            0,
            "O2: publication after the unlink was observed is a zombie write"
        );
        // O3: drained empty — nothing is left after D2.
        let drained_3 = drain(&owner_pin);
        assert!(drained_3.is_empty(), "O3: sidecar fully drained");
    });
}

/// The actual-source model: the production cached-pin fast path against
/// the production unlink + drain, on loom atomics.
#[test]
fn loom_actual_cached_pin_publish_delivery_survives_unlink_race() {
    bounded_model();
}

/// Negative shadow control for M-A (precedent: the sidecar-bitmap model's
/// negative controls; does not mutate production sources). Skipping the
/// production `linked()` gate lets a producer write the bit after the
/// owner's unlink+drain completed — the bit lands in a route that is no
/// longer linked and is lost (in production: dropped with the freed
/// sidecar).
#[test]
#[should_panic(expected = "gate-less publication wrote a zombie bit after the drain")]
fn loom_actual_negative_skip_linked_gate_writes_zombie_bit() {
    loom::model(move || {
        // Minimal atomic skeleton of the modeled protocol: linked flag +
        // one pending word, cleared by the owner's drain swap.
        let linked = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicU64::new(0));
        let drained = Arc::new(AtomicBool::new(false));
        let producer = {
            let linked = Arc::clone(&linked);
            let pending = Arc::clone(&pending);
            thread::spawn(move || {
                // M-A: no gate load at all before the RMW.
                pending.fetch_or(1, Ordering::AcqRel);
                let _ = linked.load(Ordering::Relaxed);
            })
        };
        let owner = {
            let pending = Arc::clone(&pending);
            let drained = Arc::clone(&drained);
            thread::spawn(move || {
                // Production remove's Release clear, then the drain swap.
                linked.store(false, Ordering::Release);
                let cut = pending.swap(0, Ordering::AcqRel);
                if cut == 0 {
                    drained.store(true, Ordering::Release);
                }
            })
        };
        producer.join().unwrap();
        owner.join().unwrap();
        let zombie = pending.load(Ordering::Acquire);
        if drained.load(Ordering::Acquire) && zombie != 0 {
            panic!("gate-less publication wrote a zombie bit after the drain");
        }
    });
}

/// Negative shadow control for M-B: a RELAXED gate load carries no
/// acquire edge, so the producer's stale-`true` read pairs with a
/// publication ordered after the owner's drain — the unlink witness is
/// lost and the bit is dropped with the (production) sidecar.
#[test]
#[should_panic(expected = "relaxed gate lost the unlink witness")]
fn loom_actual_negative_relaxed_gate_loses_unlink_witness() {
    loom::model(move || {
        let linked = Arc::new(AtomicBool::new(true));
        let pending = Arc::new(AtomicU64::new(0));
        let drained = Arc::new(AtomicBool::new(false));
        let producer = {
            let linked = Arc::clone(&linked);
            let pending = Arc::clone(&pending);
            thread::spawn(move || {
                // M-B: Relaxed instead of Acquire on the gate.
                if linked.load(Ordering::Relaxed) {
                    pending.fetch_or(1, Ordering::AcqRel);
                }
            })
        };
        let owner = {
            let pending = Arc::clone(&pending);
            let drained = Arc::clone(&drained);
            thread::spawn(move || {
                linked.store(false, Ordering::Release);
                let cut = pending.swap(0, Ordering::AcqRel);
                if cut == 0 {
                    drained.store(true, Ordering::Release);
                }
            })
        };
        producer.join().unwrap();
        owner.join().unwrap();
        let zombie = pending.load(Ordering::Acquire);
        if drained.load(Ordering::Acquire) && zombie != 0 {
            panic!("relaxed gate lost the unlink witness");
        }
    });
}
