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
    /// Bench-internals observer for the U31 owner-drain-skip trigger
    /// (OPEN_ITEMS item 90): Relaxed read of the route `Entry.refs` refcount.
    /// Base is 1 (the owner reference created at registration); every held
    /// pin adds exactly one count. IMPORTANT: this is a SECONDARY slice
    /// snapshotting the CURRENT holders (base) at the moment of the read —
    /// `refs == 1` does NOT mean "no pins were published since the previous
    /// scan": `RoutePin::publish_small` publishes and immediately releases
    /// its pin, so `refs` returns to base even with nonempty pending bits
    /// (`pin.rs:63-80`, `directory.rs:218-225`; counterexample
    /// `tests/r6_route_directory.rs:109-129`). For the U31 upper bound of
    /// skippable scans use `B4_U31_SCAN_NO_PENDING /
    /// B4_U31_DISCOVERY_SCANS`. Diagnostic only (Relaxed); no RMW.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_entry_refs_relaxed(&self) -> usize {
        use core::sync::atomic::Ordering;
        self.entry.entry().refs.load(Ordering::Relaxed)
    }
    pub fn small_sidecar(&self) -> Option<&SmallSidecar> {
        self.entry.small_sidecar()
    }

    /// Owner-capability: регистрация !Sync, из pin конструктор недоступен.
    #[cfg(any(feature = "internals", loom))]
    #[allow(unsafe_code)] // Вызов seam через !Sync owner-capability.
    pub(crate) fn issue_small(&self, offset: u32) {
        let sidecar = self.small_sidecar().unwrap_or_else(|| {
            crate::alloc_core::fatal::fatal(b"registration: issue on non-small route\n")
        });
        // SAFETY: единственная регистрация !Sync; её shared borrow не Send.
        // Производные issue-run сохраняют lifetime и !Send/!Sync через marker.
        unsafe { sidecar.issue(offset) };
    }

    #[cfg(any(feature = "internals", loom))]
    #[doc(hidden)]
    pub fn issue_small_for_test(&self, offset: u32) {
        self.issue_small(offset);
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
// All touched directory internals are private again (P2-2): the hooks read
// through the `pub(super)` read-only observers defined in `directory.rs`.
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
        self.with_shard_of_key_locked(key, f);
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
        self.registrations_total()
    }

    /// Monotonic count of successful unlink operations on this directory.
    /// Diagnostic only; production behavior never reads this counter.
    #[doc(hidden)]
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_unlinks_total(&self) -> u64 {
        self.unlinks_total()
    }

    /// Pointer cells retained: block indexes plus whole blocks (spares too).
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn retained_pointer_capacity_for_test(&self) -> usize {
        self.retained_pointer_capacity()
    }

    /// Pointer cells moved by updates (shifts, splits, merges, index growth
    /// copies) since construction.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn moved_pointer_cells_for_test(&self) -> u64 {
        self.moved_pointer_cells()
    }

    /// Live routes by kind: `(small, primordial, large)`.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    #[must_use]
    pub fn live_route_census_for_test(&self) -> (usize, usize, usize) {
        self.live_route_census()
    }
}
