//! Slot discovery without an intrusive free list.

use core::sync::atomic::Ordering;

use crate::registry::bootstrap::{Registry, MAX_HEAPS};
use crate::registry::heap_slot::{HeapSlot, STATE_EMPTY, STATE_FREE};

/// Prefer claimable materialised slots, then an index in an unmaterialised
/// chunk. The latter may have been minted before a failed chunk reservation;
/// only the caller's slot-state CAS grants ownership after materialisation.
pub(super) fn scan_claimable_slot(reg: &Registry) -> Option<usize> {
    let count = (reg.count.load(Ordering::Acquire) as usize).min(MAX_HEAPS);
    if count == 0 {
        return None;
    }
    let start = reg.scan_cursor.fetch_add(1, Ordering::Relaxed) as usize % count;
    let mut unmaterialised = None;
    for offset in 0..count {
        let idx = (start + offset) % count;
        let Some(slot) = reg.slot_if_materialised(idx) else {
            if unmaterialised.is_none() {
                unmaterialised = Some(idx);
            }
            continue;
        };
        let state = slot.state.load(Ordering::Acquire);
        if state == STATE_FREE
            || (state == STATE_EMPTY && !slot.initialised.load(Ordering::Acquire))
        {
            return Some(idx);
        }
    }
    unmaterialised
}

/// One fair pass over the high-water range, skipping unmaterialised chunks.
/// No result is authoritative until the caller's FREE→MAINTENANCE CAS wins.
pub(super) fn scan_free_slot(reg: &Registry) -> Option<(usize, &'static HeapSlot)> {
    let count = (reg.count.load(Ordering::Acquire) as usize).min(MAX_HEAPS);
    if count == 0 {
        return None;
    }
    let start = reg.scan_cursor.fetch_add(1, Ordering::Relaxed) as usize % count;
    for offset in 0..count {
        let idx = (start + offset) % count;
        let Some(slot) = reg.slot_if_materialised(idx) else {
            continue;
        };
        if slot.initialised.load(Ordering::Acquire)
            && slot.state.load(Ordering::Acquire) == STATE_FREE
        {
            return Some((idx, slot));
        }
    }
    None
}

/// Reserve a fresh numeric index without ever overshooting the hard cap.
/// The slot remains EMPTY until its claimant wins EMPTY→INITIALIZING.
pub(super) fn bump_count(reg: &Registry) -> Option<usize> {
    let mut count = reg.count.load(Ordering::Acquire);
    loop {
        if count as usize >= MAX_HEAPS {
            return None;
        }
        match reg
            .count
            .compare_exchange_weak(count, count + 1, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => return Some(count as usize),
            Err(actual) => count = actual,
        }
    }
}
