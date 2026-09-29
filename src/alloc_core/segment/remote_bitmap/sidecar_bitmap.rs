//! Single-bank pending bits and owner-issued classes, outside the reservation.
//!
//! The future directory supplies stable, disjoint atomic backing for the
//! entire reservation lifetime. It prepares that backing on an allocation
//! path, before the segment can be published. No method allocates. A masked
//! user pointer is only an integer lookup key; this primitive never receives
//! or dereferences one. The directory must pin the backing through all
//! unpublished producers, published bits, and detached cuts.

use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

use super::BitmapScan;

use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::size_classes::{MIN_BLOCK, SMALL_CLASS_COUNT};

const GRANULES: usize = SEGMENT / MIN_BLOCK;
const WORDS: usize = GRANULES / 64;
const _: () = {
    assert!(SEGMENT == 4 * 1024 * 1024);
    assert!(MIN_BLOCK == 16);
    assert!(WORDS == 4096);
    assert!(GRANULES == 262_144);
    assert!(SMALL_CLASS_COUNT < u8::MAX as usize);
};

/// A live segment's external ingress storage. Construction borrows backing;
/// it does not allocate, initialize, or bind a reservation pointer.
pub(crate) struct SidecarBitmap<'a> {
    pub(super) pending: &'a [AtomicU64],
    pub(super) classes: &'a [AtomicU8],
}

impl<'a> SidecarBitmap<'a> {
    pub(crate) const WORDS: usize = WORDS;
    pub(crate) const GRANULES: usize = GRANULES;

    /// Backing must be separate from reservation bytes and zero-initialized
    /// before segment publication. Its lifetime must extend through all cuts.
    pub(crate) fn from_initialized(
        pending: &'a [AtomicU64],
        classes: &'a [AtomicU8],
    ) -> Option<Self> {
        (pending.len() == WORDS && classes.len() == GRANULES).then_some(Self { pending, classes })
    }

    /// Owner-only issue, before handing the allocation to another thread.
    /// The previous incarnation's bit must already be cut and reclaimed.
    /// A separate allocation handoff must carry this Release store to a
    /// producer; the bitmap does not itself establish that handoff.
    pub(crate) fn issue(&self, offset: u32, class: u8) -> bool {
        let Some(granule) = Self::granule(offset) else {
            return false;
        };
        if usize::from(class) >= SMALL_CLASS_COUNT {
            return false;
        }
        self.classes[granule].store(class + 1, Ordering::Release);
        true
    }

    /// Producer terminal AcqRel RMW. No class, header, or reservation access;
    /// no sidecar access is permitted after this RMW. Exactly one producer
    /// publishes each valid allocation instance. An unpublished producer
    /// retains its segment credit; a published one retains it until reclaim.
    pub(crate) fn publish(&self, offset: u32) -> bool {
        let Some(granule) = Self::granule(offset) else {
            return false;
        };
        self.pending[granule / 64].fetch_or(1 << (granule % 64), Ordering::AcqRel);
        true
    }

    /// Owner fixes an exclusive upper bound in bytes. The caller's high water
    /// must include every issued block that can still publish. This scans
    /// exactly `ceil(high_water / (64 * MIN_BLOCK))` words, without dirty hints
    /// or waiting for paused producers. An in-flight producer after a word's
    /// cut belongs to a later scan.
    pub(crate) fn scan(&self, high_water: usize) -> Option<BitmapScan<'a>> {
        if high_water > SEGMENT {
            return None;
        }
        let end_word = high_water.div_ceil(MIN_BLOCK * 64);
        Some(BitmapScan {
            bitmap: Self {
                pending: self.pending,
                classes: self.classes,
            },
            next_word: 0,
            end_word,
        })
    }

    fn granule(offset: u32) -> Option<usize> {
        let offset = offset as usize;
        (offset < SEGMENT && offset.is_multiple_of(MIN_BLOCK)).then_some(offset / MIN_BLOCK)
    }
}
