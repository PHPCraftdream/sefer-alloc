//! `ThpAdviceOutcome` — classification of one range-advice attempt, which
//! decides what happens to the budget charge reserved for it.

/// Result class of a range-advice attempt.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ThpAdviceOutcome {
    /// The kernel accepted the hint for this many bytes. A hint, not proof of
    /// physical huge-page backing.
    Accepted(usize),
    /// Nothing was touched (validation, injected or unsupported failure): the
    /// reserved charge is returned at once.
    PreSyscall,
    /// An OS error with an error code: a partial effect cannot be excluded, so
    /// the reserved charge is HELD until the terminal release of the mapping.
    UnknownPartial,
}
