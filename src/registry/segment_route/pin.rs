#[cfg(all(feature = "internals", feature = "bench-internals"))]
use core::ptr::NonNull;
#[cfg(all(feature = "internals", feature = "bench-internals"))]
use core::sync::atomic::AtomicU64;

use super::directory::EntryHandle;
use super::RouteKind;
#[cfg(all(feature = "internals", feature = "bench-internals"))]
use crate::alloc_core::remote_bitmap::SidecarBitmap;

/// Counted descriptor and sidecar capability, not a reservation credit.
/// No allocator-origin root is accessible through a producer pin.
pub struct RoutePin {
    pub(super) entry: EntryHandle,
}

impl RoutePin {
    pub fn owner(&self) -> usize {
        self.entry.owner()
    }
    pub fn kind(&self) -> RouteKind {
        self.entry.kind()
    }
    pub fn incarnation(&self) -> u64 {
        self.entry.incarnation()
    }
    /// Capacity validation reads only the independent immutable descriptor.
    pub fn contains_payload(&self, ptr: *mut u8, size: usize) -> bool {
        self.entry.contains_payload(ptr.addr(), size)
    }
    /// Read-only terminal-publication oracle; does not grant owner mutation.
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn pending_for_test(&self, ptr: *mut u8) -> bool {
        if let Some(state) = self.entry.large_state() {
            state.pending_for_test()
        } else {
            let offset = (ptr.addr() & (crate::alloc_core::os::SEGMENT - 1)) as u32;
            self.entry
                .small_sidecar()
                .is_some_and(|sidecar| sidecar.pending_for_test(offset))
        }
    }
    /// Terminal publication consumes the producer capability. Drop then
    /// touches only the independently allocated descriptor/sidecar.
    ///
    /// # Safety
    /// This pin and `offset` identify the caller's current issued Small or
    /// Primordial allocation. The caller transfers its unique ownership once
    /// and must not access or free that allocation after successful publication.
    /// Address lookup alone does not establish this ownership.
    #[allow(unsafe_code)]
    pub unsafe fn publish_small(self, offset: u32) -> bool {
        self.entry
            .small_sidecar()
            .is_some_and(|sidecar| sidecar.publish(offset))
    }
    /// Publish only the authoritative pending bit, simulating a producer
    /// paused before notification. Consumes the pin without touching reservation.
    ///
    /// # Safety
    /// This pin and offset must identify a current, uniquely owned issued Small
    /// or Primordial block. Transfer that ownership exactly once; after success
    /// never access or deallocate the block. The pin must remain held through
    /// the pending LP (enforced here by consuming self after publication).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    #[allow(unsafe_code)]
    pub unsafe fn dbg_publish_pending_only_for_test(self, offset: u32) -> bool {
        // SAFETY: identical ownership-transfer contract; abandoning the handle
        // omits only the notification, not the actual pending half.
        unsafe { self.dbg_pause_publish_for_test(offset) }.is_some()
    }

    /// Pause the shared production invocation sequence after its first half.
    ///
    /// # Safety
    /// This pin and offset identify the caller's current uniquely owned issued
    /// Small/Primordial allocation. Successful return transfers ownership at LP:
    /// never access or free that instance again, even if the handle is dropped.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    #[allow(unsafe_code)]
    pub unsafe fn dbg_pause_publish_for_test(self, offset: u32) -> Option<super::PausedPublish> {
        let (summary, bit, notify) = self.entry.small_sidecar()?.pause_publish(offset)?;
        let summary = core::ptr::NonNull::from(summary);
        Some(super::PausedPublish::new(
            self, summary, bit, notify, offset,
        ))
    }

    /// Terminal LIVE(g) -> PENDING(g) CAS, consuming this pin.
    ///
    /// # Safety
    /// This pin identifies the caller's current issued Large allocation.
    /// The caller transfers its unique ownership once and must not access or
    /// free that allocation after successful publication.
    #[allow(unsafe_code)]
    pub unsafe fn publish_large(self) -> bool {
        self.entry
            .large_state()
            .is_some_and(|state| state.publish_pending())
    }
}

/// Keeps the independent descriptor/sidecar alive after terminal publication.
/// Not Clone: notification can be resumed at most once. Dropping without resume
/// abandons only a discovery hint; strict/maintenance still consume pending.
#[cfg(all(feature = "internals", feature = "bench-internals"))]
#[doc(hidden)]
#[must_use = "resume the discovery notification, or deliberately abandon the hint"]
pub struct PausedPublish {
    pin: RoutePin,
    summary: NonNull<AtomicU64>,
    bit: u64,
    // C'': set only when the producer's pending RMW observed prev == 0.
    notify: bool,
    // Used only by the reversed-order counterfactual's remaining pending half.
    offset: u32,
}

#[cfg(all(feature = "internals", feature = "bench-internals"))]
impl PausedPublish {
    /// The address was obtained before LP from this pin's own sidecar. The pin
    /// is moved here before its counted reference can be dropped. No reference
    /// with an invented lifetime escapes; owner scans use shared atomics only.
    pub(super) fn new(
        pin: RoutePin,
        summary: NonNull<AtomicU64>,
        bit: u64,
        notify: bool,
        offset: u32,
    ) -> Self {
        Self {
            pin,
            summary,
            bit,
            notify,
            offset,
        }
    }

    /// Execute the actual second publication half, then release the pin.
    /// Baseline: no lookup, class/pending read, reservation or ledger operation.
    /// M5' only: the common order selects the still-unexecuted pending half;
    /// its lazy capability reads the pinned sidecar, never a reservation root.
    #[allow(unsafe_code)] // item-level tier-2 site; counted in README's seam list
    pub fn resume(self) {
        // SAFETY: the private constructor binds this pre-LP sidecar address to
        // the counted pin retained in self. Retirement removes the route/root,
        // not this independently allocated atomic backing. No exclusive borrow
        // of the backing is formed. The pin is dropped only after this access.
        SidecarBitmap::resume_publish(
            unsafe { self.summary.as_ref() },
            self.bit,
            self.notify,
            || {
                self.pin
                    .entry
                    .small_sidecar()
                    .is_some_and(|sidecar| sidecar.resume_pending_for_test(self.offset))
            },
        );
        drop(self.pin);
    }
}
