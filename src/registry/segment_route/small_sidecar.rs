use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::size_classes::{MIN_BLOCK, SMALL_CLASS_COUNT};

const GRANULES: usize = SEGMENT / MIN_BLOCK;
const WORDS: usize = GRANULES / 64;
const _: () = {
    assert!(SEGMENT == 4 * 1024 * 1024);
    assert!(MIN_BLOCK == 16);
    assert!(SMALL_CLASS_COUNT < u8::MAX as usize);
};

/// Atomic ingress storage outside the allocator reservation.
#[repr(C)]
pub struct SmallSidecar {
    pending: [AtomicU64; WORDS],
    classes: [AtomicU8; GRANULES],
}

impl SmallSidecar {
    pub const WORDS: usize = WORDS;

    /// Owner-only issue, before the block becomes visible to a producer.
    pub fn issue(&self, offset: u32, class: u8) -> bool {
        let Some(index) = Self::index(offset) else {
            return false;
        };
        if usize::from(class) >= SMALL_CLASS_COUNT {
            return false;
        }
        self.classes[index].store(class + 1, Ordering::Release);
        true
    }

    /// Terminal publication. The caller may only release its pin afterward.
    pub(super) fn publish(&self, offset: u32) -> bool {
        let Some(index) = Self::index(offset) else {
            return false;
        };
        self.pending[index / 64].fetch_or(1_u64 << (index % 64), Ordering::AcqRel);
        true
    }

    /// Exclusive owner cut of one word in a bounded pass.
    pub fn cut_word(&self, word: usize) -> Option<u64> {
        self.pending.get(word).map(|v| v.swap(0, Ordering::AcqRel))
    }

    pub fn class_at(&self, offset: u32) -> Option<u8> {
        let index = Self::index(offset)?;
        self.classes[index].load(Ordering::Acquire).checked_sub(1)
    }

    fn index(offset: u32) -> Option<usize> {
        let offset = offset as usize;
        (offset < SEGMENT && offset.is_multiple_of(MIN_BLOCK)).then_some(offset / MIN_BLOCK)
    }
}
