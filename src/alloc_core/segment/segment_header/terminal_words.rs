//! Atomic terminal-publication words outside the plain `SegmentHeader` copy.

use core::mem::{align_of, offset_of, size_of};
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::alloc_core::node::Node;

use super::{align_up_const, SegmentHeader, SegmentMeta};

/// Neither word is part of `SegmentHeader::read_at`'s byte copy.
#[repr(C)]
pub(crate) struct SegmentTerminalWords {
    remote_head: AtomicU32,
    large_state: AtomicU64,
}

pub(crate) const TERMINAL_WORDS_OFF: usize = align_up_const(
    size_of::<SegmentHeader>(),
    align_of::<SegmentTerminalWords>(),
);
pub(crate) const REMOTE_HEAD_OFF: usize =
    TERMINAL_WORDS_OFF + offset_of!(SegmentTerminalWords, remote_head);
pub(crate) const LARGE_STATE_OFF: usize =
    TERMINAL_WORDS_OFF + offset_of!(SegmentTerminalWords, large_state);
pub(crate) const REMOTE_HEAD_EMPTY: u32 = u32::MAX;

const PHASE_BITS: u32 = 3;
pub(crate) const MAX_LARGE_GENERATION: u64 = u64::MAX >> PHASE_BITS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
#[allow(dead_code)] // Pending/Consuming belong to the next ingress stage.
pub(crate) enum LargePhase {
    Unused = 0,
    Initializing = 1,
    Live = 2,
    Pending = 3,
    Consuming = 4,
    Cached = 5,
    Released = 6,
}

#[inline(always)]
pub(crate) const fn pack_large_state(phase: LargePhase, generation: u64) -> u64 {
    assert!(generation <= MAX_LARGE_GENERATION);
    (generation << PHASE_BITS) | phase as u64
}

#[inline(always)]
pub(crate) const fn large_generation(word: u64) -> u64 {
    word >> PHASE_BITS
}

#[inline(always)]
#[cfg_attr(not(feature = "alloc-decommit"), allow(dead_code))]
pub(crate) const fn large_phase(word: u64) -> Option<LargePhase> {
    match word & ((1 << PHASE_BITS) - 1) {
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

#[inline(always)]
pub(crate) const fn next_large_generation(generation: u64) -> Option<u64> {
    if generation >= MAX_LARGE_GENERATION {
        None
    } else {
        Some(generation + 1)
    }
}

/// A value-only diagnostic view; its two loads are not one atomic transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalSnapshot {
    pub remote_head: u32,
    pub large_state: u64,
}

impl SegmentMeta {
    /// Initialize the separate atomic objects on a fresh Small/Primordial
    /// reservation, before registration or any remote access.
    pub(crate) fn init_small_terminal(&self) {
        Node::init_atomic_u32_at(self.base, REMOTE_HEAD_OFF, REMOTE_HEAD_EMPTY);
        Node::init_atomic_u64_at(
            self.base,
            LARGE_STATE_OFF,
            pack_large_state(LargePhase::Unused, 0),
        );
    }

    /// Initialize the separate atomic objects on a fresh Large reservation.
    pub(crate) fn init_large_terminal(&self) {
        Node::init_atomic_u32_at(self.base, REMOTE_HEAD_OFF, REMOTE_HEAD_EMPTY);
        Node::init_atomic_u64_at(
            self.base,
            LARGE_STATE_OFF,
            pack_large_state(LargePhase::Live, 1),
        );
    }

    /// Requires a live reservation and an initialized terminal-word region.
    /// The caller must not retain or access this reference after release.
    #[inline(always)]
    pub(crate) fn remote_head_atomic(&self) -> &AtomicU32 {
        Node::atomic_u32_at(self.base, REMOTE_HEAD_OFF)
    }

    /// Requires a live reservation and an initialized terminal-word region.
    /// The caller must not retain or access this reference after release.
    #[inline(always)]
    pub(crate) fn large_state_atomic(&self) -> &AtomicU64 {
        Node::atomic_u64_at(self.base, LARGE_STATE_OFF)
    }

    /// Atomic loads only; never copies bytes of either atomic object.
    #[inline(always)]
    pub(crate) fn terminal_snapshot(&self) -> TerminalSnapshot {
        TerminalSnapshot {
            remote_head: self.remote_head_atomic().load(Ordering::Acquire),
            large_state: self.large_state_atomic().load(Ordering::Acquire),
        }
    }

    /// Owner-only transition after unregister, before placing a Large
    /// reservation in cache. This word is not yet the authoritative ingress.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn mark_large_cached(&self) {
        let word = self.large_state_atomic().load(Ordering::Acquire);
        if !matches!(
            large_phase(word),
            Some(LargePhase::Live | LargePhase::Consuming)
        ) {
            std::process::abort();
        }
        self.large_state_atomic().store(
            pack_large_state(LargePhase::Cached, large_generation(word)),
            Ordering::Release,
        );
    }

    /// Owner-only cache-hit preparation. Exhausted generations retire this
    /// reservation; the caller must release it and allocate fresh memory.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn begin_large_reuse(&self) -> Option<u64> {
        let word = self.large_state_atomic().load(Ordering::Acquire);
        if large_phase(word) != Some(LargePhase::Cached) {
            std::process::abort();
        }
        let next = next_large_generation(large_generation(word))?;
        self.large_state_atomic().store(
            pack_large_state(LargePhase::Initializing, next),
            Ordering::Release,
        );
        Some(next)
    }

    /// Complete a cache-hit reset before the table exposes the reservation.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn finish_large_reuse(&self, generation: u64) {
        self.remote_head_atomic()
            .store(REMOTE_HEAD_EMPTY, Ordering::Relaxed);
        self.large_state_atomic().store(
            pack_large_state(LargePhase::Live, generation),
            Ordering::Release,
        );
    }

    /// Terminal store while still mapped, strictly before OS release.
    pub(crate) fn mark_large_released(&self) {
        let word = self.large_state_atomic().load(Ordering::Acquire);
        self.large_state_atomic().store(
            pack_large_state(LargePhase::Released, large_generation(word)),
            Ordering::Release,
        );
    }
}
