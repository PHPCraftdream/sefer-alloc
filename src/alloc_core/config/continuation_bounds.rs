//! Root compatibility wrapper over canonical segment continuation geometry.

use sefer_segment::support::continuation_bounds_impl as canonical;

#[derive(Clone, Copy)]
pub(crate) struct ContinuationBounds(pub(crate) canonical::ContinuationBounds);

impl ContinuationBounds {
    #[allow(unsafe_code)] // P2.1 tier-2: canonical `from_segment_base` contract.
    #[inline(always)]
    pub(crate) fn new(base: *mut u8) -> Self {
        // SAFETY: private trusted callers build bounds while the owning live segment is exclusively controlled.
        Self(unsafe { canonical::ContinuationBounds::from_segment_base(base) })
    }
}
