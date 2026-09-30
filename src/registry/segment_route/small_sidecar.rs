//! Owning backing for the single authoritative SidecarBitmap primitive.

#[cfg(feature = "internals")]
use core::sync::atomic::Ordering;
use core::sync::atomic::{AtomicU64, AtomicU8};

use crate::alloc_core::remote_bitmap::SidecarBitmap;

use super::RouteScan;

#[repr(C)]
pub struct SmallSidecar {
    pending: [AtomicU64; SidecarBitmap::WORDS],
    classes: [AtomicU8; SidecarBitmap::GRANULES],
}

impl SmallSidecar {
    pub const WORDS: usize = SidecarBitmap::WORDS;

    fn bitmap(&self) -> SidecarBitmap<'_> {
        SidecarBitmap::from_initialized(&self.pending, &self.classes)
            .unwrap_or_else(|| std::process::abort())
    }

    /// Owner-only class issue before allocation handoff.
    pub fn issue(&self, offset: u32, class: u8) -> bool {
        self.bitmap().issue(offset, class)
    }

    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn class_at_for_test(&self, offset: u32) -> Option<u8> {
        let offset = offset as usize;
        let granule = crate::alloc_core::size_classes::MIN_BLOCK;
        if offset >= crate::alloc_core::os::SEGMENT || !offset.is_multiple_of(granule) {
            return None;
        }
        self.classes[offset / granule]
            .load(Ordering::Acquire)
            .checked_sub(1)
    }
    #[cfg(feature = "internals")]
    pub(super) fn pending_for_test(&self, offset: u32) -> bool {
        let bit = offset as usize / crate::alloc_core::size_classes::MIN_BLOCK;
        bit < SidecarBitmap::GRANULES
            && self.pending[bit / 64].load(Ordering::Acquire) & (1u64 << (bit % 64)) != 0
    }

    /// Producer terminal publication, called by consuming RoutePin.
    pub(super) fn publish(&self, offset: u32) -> bool {
        self.bitmap().publish(offset)
    }

    /// Owner-only bounded per-word cut through the existing bitmap scanner.
    pub fn scan(&self, high_water: usize) -> Option<RouteScan<'_>> {
        self.bitmap()
            .scan(high_water)
            .map(|inner| RouteScan { inner })
    }

    pub(crate) fn scan_from(&self, high_water: usize, word: usize) -> Option<RouteScan<'_>> {
        self.bitmap()
            .scan_from(high_water, word)
            .map(|inner| RouteScan { inner })
    }
}
