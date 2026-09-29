//! Slot discovery without an intrusive free list.

use core::sync::atomic::{AtomicU32, Ordering};

use crate::registry::bootstrap::saturation::SaturationHint;
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

/// The claim selector, parameterised so a small deterministic model can
/// exercise the production fast path without constructing 4096 heaps.
#[doc(hidden)]
pub fn pick_with_saturation(
    hint: &AtomicU32,
    count: &AtomicU32,
    saturation: &SaturationHint,
    max: usize,
    scan: impl FnOnce() -> Option<usize>,
    bump: impl FnOnce() -> Option<usize>,
) -> Option<usize> {
    let candidate = hint.swap(max as u32, Ordering::AcqRel) as usize;
    let before = (count.load(Ordering::Acquire) as usize).min(max);
    if candidate < before {
        return Some(candidate);
    }
    if before == max && saturation.is_saturated() {
        return None;
    }
    let version = saturation.snapshot();
    if let Some(index) = scan() {
        return Some(index);
    }
    if let Some(index) = bump() {
        return Some(index);
    }
    // A scan begun below the cap might have missed a concurrently minted
    // EMPTY index. It must not certify the now-full range as saturated.
    if before == max && count.load(Ordering::Acquire) as usize == max {
        saturation.try_mark_saturated(version);
    }
    None
}
