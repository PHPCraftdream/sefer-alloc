//! Access attempted during one bounded scheduler step.

/// Access observations only; neither field means that a free was reclaimed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Visit {
    /// A materialised FREE registry slot was exclusively leased.
    pub slot_leased: bool,
    /// The already-ready fallback heap's lock was acquired without waiting.
    pub fallback_locked: bool,
}
