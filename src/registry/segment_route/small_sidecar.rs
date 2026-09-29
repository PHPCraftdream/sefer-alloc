//! Owning backing for the single authoritative SidecarBitmap primitive.

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
}
