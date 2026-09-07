//! The peak-live-block pre-pass [`drive`](crate::drive) sizes its reference
//! model's `Vec` capacity from.

use crate::op::Op;

/// Peak number of simultaneously-live blocks across `ops` — the exact
/// capacity [`drive`](crate::drive) reserves for its reference model, so the
/// model `Vec` grows once (or never) while the stream replays instead of
/// tracking the whole history length (which a long alternating
/// Alloc/Dealloc history overshoots arbitrarily).
///
/// A counter-only pre-pass is faithful to the real dynamics, so no full
/// replay is needed: `Alloc`/`AllocZeroed` always push one entry;
/// `Dealloc` pops one only when the model is non-empty (`drive` skips the
/// op entirely against an empty live set), and which index a `Dealloc` or
/// `Realloc` targets is irrelevant to the count — `swap_remove` gives every
/// removal the same length effect; `Realloc` replaces its entry in place.
/// The running maximum of the counter is therefore exactly the maximum
/// `live.len()` the replay reaches for any history `drive` runs to
/// completion, and a run that stops early on an oracle panic needs no more.
#[must_use]
pub fn peak_live_count(ops: &[Op]) -> usize {
    let mut live = 0usize;
    let mut peak = 0usize;
    for op in ops {
        match *op {
            Op::Alloc { .. } | Op::AllocZeroed { .. } => {
                live += 1;
                peak = peak.max(live);
            }
            Op::Dealloc(_) => live = live.saturating_sub(1),
            Op::Realloc { .. } => {}
        }
    }
    peak
}
