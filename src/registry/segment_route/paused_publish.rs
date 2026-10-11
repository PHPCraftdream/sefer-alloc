//! Owned post-LP notification capability; never a reservation credit.
#![allow(unsafe_code)]

use core::ptr::NonNull;
use core::sync::atomic::AtomicU64;

use super::RoutePin;
use crate::alloc_core::remote_bitmap::SidecarBitmap;

/// Keeps the independent descriptor/sidecar alive after terminal publication.
/// Not Clone: notification can be resumed at most once. Dropping without resume
/// abandons only a discovery hint; strict/maintenance still consume pending.
#[doc(hidden)]
#[must_use = "resume the discovery notification, or deliberately abandon the hint"]
pub struct PausedPublish {
    pin: RoutePin,
    summary: NonNull<AtomicU64>,
    bit: u64,
    // Used only by the reversed-order counterfactual's remaining pending half.
    offset: u32,
}

impl PausedPublish {
    /// The address was obtained before LP from this pin's own sidecar. The pin
    /// is moved here before its counted reference can be dropped. No reference
    /// with an invented lifetime escapes; owner scans use shared atomics only.
    pub(super) fn new(pin: RoutePin, summary: NonNull<AtomicU64>, bit: u64, offset: u32) -> Self {
        Self {
            pin,
            summary,
            bit,
            offset,
        }
    }

    /// Execute the actual second publication half, then release the pin.
    /// Baseline: no lookup, class/pending read, reservation or ledger operation.
    /// M5' only: the common order selects the still-unexecuted pending half;
    /// its lazy capability reads the pinned sidecar, never a reservation root.
    pub fn resume(self) {
        // SAFETY: the private constructor binds this pre-LP sidecar address to
        // the counted pin retained in self. Retirement removes the route/root,
        // not this independently allocated atomic backing. No exclusive borrow
        // of the backing is formed. The pin is dropped only after this access.
        SidecarBitmap::resume_publish(unsafe { self.summary.as_ref() }, self.bit, || {
            self.pin
                .entry
                .small_sidecar()
                .is_some_and(|sidecar| sidecar.resume_pending_for_test(self.offset))
        });
        drop(self.pin);
    }
}
