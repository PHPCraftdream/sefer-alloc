use core::cell::Cell;
use core::marker::PhantomData;
use core::mem::ManuallyDrop;

use super::directory::{EntryHandle, RouteDirectory};
use super::{LargeState, SmallSidecar};
#[cfg(feature = "internals")]
use super::{RouteError, RouteKind};

// Moved here (pure move, D1′ round-2 structural fix) from `directory.rs`
// together with the registration-failure test hooks below; `directory.rs`
// consumes it in `register_route` through the shared `super` module (this
// includes the Loom source-including harness, which mounts both files as
// sibling modules of the same parent).
#[cfg(feature = "internals")]
std::thread_local! {
    pub(super) static FAIL_NEXT_REGISTRATION: Cell<usize> = const { Cell::new(0) };
}

/// Owner-held admission handle. Drop unlinks first, then releases its counted
/// descriptor reference without waiting for producers already holding pins.
pub struct RouteRegistration<'a> {
    pub(super) directory: &'a RouteDirectory,
    pub(super) entry: ManuallyDrop<EntryHandle>,
    pub(super) owner_only: PhantomData<Cell<()>>,
}

impl RouteRegistration<'_> {
    /// Allocator-origin usable root, never reconstructed from an address key.
    pub fn root(&self) -> *mut u8 {
        self.entry.root()
    }
    #[cfg(feature = "internals")]
    pub fn incarnation(&self) -> u64 {
        self.entry.incarnation()
    }
    pub fn small_sidecar(&self) -> Option<&SmallSidecar> {
        self.entry.small_sidecar()
    }
    /// Doc-hidden internals observer (CLAUDE.md "test-only export pattern"):
    /// `(issued start, pending record)` of a global address's granule, read
    /// ONLY from the atomics the pin owns — the pinned route's
    /// `SmallSidecar` start/pending banks. A pin holds the sidecar and
    /// descriptor, never the reservation, so NO segment memory (state byte,
    /// header) is touched: every read here is a relaxed-safe atomic load on
    /// the sidecar the descriptor itself anchors. `None` for a foreign or
    /// unroutable address or a non-small route. (The old class-at-address
    /// probe read the owner state byte under a bare pin — an unmapped-
    /// address read and a race with the owner's byte writes — and was
    /// removed; class-at-address assertions live behind the lease-held
    /// dbg hooks.)
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn start_state_at_global_address_for_test(address: usize) -> Option<(bool, bool)> {
        let key = core::ptr::without_provenance_mut::<u8>(address);
        let pin = RouteDirectory::global().lookup(key)?;
        let offset = u32::try_from(address & (crate::alloc_core::os::SEGMENT - 1)).ok()?;
        let sidecar = pin.entry.small_sidecar()?;
        Some((sidecar.start_bit(offset), sidecar.pending_for_test(offset)))
    }
    pub fn large_state(&self) -> Option<&LargeState> {
        self.entry.large_state()
    }

    pub fn claim_large_pending(&self) -> Option<u64> {
        self.large_state()?.claim_pending()
    }

    // R6-T16: numeric-observer surface; consumed by the kani proofs and the
    // internals probes only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn cache_large_consumed(&self, generation: u64) -> bool {
        self.large_state()
            .is_some_and(|state| state.cache_consumed(generation))
    }

    /// Leaves the reservation INITIALIZING until owner reset completes.
    // R6-T16: numeric-observer surface; consumed by the kani proofs and the
    // internals probes only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn begin_large_reuse(&self) -> Option<u64> {
        self.large_state()?.begin_reuse()
    }

    /// Owner-only. Caller guarantees layout/table reset is complete; this
    /// substrate changes only the sidecar phase, not reservation bytes.
    // R6-T16: numeric-observer surface; consumed by the kani proofs and the
    // internals probes only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn finish_large_reuse_after_reset(&self, generation: u64) -> bool {
        self.large_state()
            .is_some_and(|state| state.finish_reuse(generation))
    }

    // R6-T16: numeric-observer surface; consumed by the kani proofs and the
    // internals probes only.
    #[cfg_attr(not(feature = "internals"), allow(dead_code))]
    pub fn release_cached_large(&self, generation: u64) -> bool {
        self.large_state()
            .is_some_and(|state| state.release_cached(generation))
    }
}

impl Drop for RouteRegistration<'_> {
    /// # Safety
    /// Drop the counted owner exactly once, only after successful unlink;
    /// failed unlink must retain the descriptor and sidecar owner count.
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if self.directory.remove(&self.entry) {
            // SAFETY: successful unlink released the shard guard before this
            // sole owner-count drop. ManuallyDrop suppresses automatic cleanup.
            unsafe { ManuallyDrop::drop(&mut self.entry) };
        }
        // Failed unlink leaves a non-owning shard pointer: retain its owner
        // count intentionally, including its independent sidecar, until exit.
    }
}

// Test/diagnostic hooks of [`RouteDirectory`], moved here verbatim (pure
// move, D1′ round-2 structural fix: `directory.rs` was over the 1000-line
// file limit; this directory already sits at its entry-count ratchet, so the
// split reuses the existing `registration.rs` file instead of a new one).
// Inherent impls may live in any module of the defining crate; the touched
// private items (`shards`, `next_incarnation`, `registrations_total`,
// `shard_index`) were widened to `pub(super)` — no behavior change.
impl RouteDirectory {
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn fail_next_registration_for_test() {
        let _ = FAIL_NEXT_REGISTRATION.try_with(|left| left.set(1));
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn fail_next_registrations_for_test(count: usize) {
        let _ = FAIL_NEXT_REGISTRATION.try_with(|left| left.set(count));
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn with_initial_incarnation_for_test(value: u64) -> Self {
        let directory = Self::new();
        directory
            .next_incarnation
            .store(value, core::sync::atomic::Ordering::Relaxed);
        directory
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn shard_index_for_test(key: usize) -> usize {
        Self::shard_index(key)
    }

    /// Test surface of [`RouteDirectory::register_route`] (R6-T38),
    /// `internals` only.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn register(
        &self,
        root: *mut u8,
        len: usize,
        route_ptr: *mut u8,
        owner: usize,
        kind: RouteKind,
    ) -> Result<RouteRegistration<'_>, RouteError> {
        self.register_route(root, len, route_ptr, owner, kind)
    }

    /// Runs a test closure while holding the shard selected by the address.
    /// The pointer is only a numeric shard key and is never dereferenced.
    /// Reentering this shard from `f` can deadlock: a liveness risk, not a
    /// memory-safety precondition. Unwinding releases the ordinary guard.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_with_shard_of_locked_for_test(&self, ptr: *mut u8, f: impl FnOnce()) {
        let key = ptr.addr() & !(crate::alloc_core::os::SEGMENT - 1);
        let guard = self.shards[Self::shard_index(key)].lock();
        f();
        drop(guard);
    }

    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    pub fn dbg_route_pin_refs(pin: &super::RoutePin) -> usize {
        pin.entry
            .entry()
            .refs
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Monotonic successful route registrations since directory construction.
    #[doc(hidden)]
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_registrations_total(&self) -> u64 {
        self.registrations_total
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Monotonic count of successful unlink operations on this directory.
    /// Diagnostic only; production behavior never reads this counter.
    #[doc(hidden)]
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_unlinks_total(&self) -> u64 {
        self.unlinks_total
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    // The live census SAFETY comment is inherited verbatim from the move
    // source (`directory.rs`, which carries the crate-file
    // `#![allow(unsafe_code)]` this module does not).
    /// Pointer cells retained: block indexes plus whole blocks (spares too).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[allow(unsafe_code)]
    pub fn retained_pointer_capacity_for_test(&self) -> usize {
        self.shards
            .iter()
            .map(|shard| {
                let guard = shard.lock();
                let spare = usize::from(
                    !guard
                        .spare
                        .load(core::sync::atomic::Ordering::Relaxed)
                        .is_null(),
                );
                guard.index.cap + (guard.index.len + spare) * super::directory::BLOCK_CAP
            })
            .sum()
    }

    /// Pointer cells moved by updates (shifts, splits, merges, index growth
    /// copies) since construction.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn moved_pointer_cells_for_test(&self) -> u64 {
        self.shards.iter().map(|shard| shard.lock().moved).sum()
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[allow(unsafe_code)]
    pub fn live_route_census_for_test(&self) -> (usize, usize, usize) {
        let mut counts = (0, 0, 0);
        for shard in &self.shards {
            let guard = shard.lock();
            for b in 0..guard.index.len {
                let block = guard.block(b);
                for index in 0..block.len {
                    // SAFETY: the shard lock retains the linked owner reference.
                    match unsafe { &*block.get(index) }.kind {
                        RouteKind::Small => counts.0 += 1,
                        RouteKind::Primordial => counts.1 += 1,
                        RouteKind::Large => counts.2 += 1,
                    }
                }
            }
        }
        counts
    }
}
