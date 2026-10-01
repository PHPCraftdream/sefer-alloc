/// Result of a descriptor insertion.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InsertOutcome {
    Inserted,
    /// Fallible `System` growth failed; nothing was inserted.
    Oom,
    /// A live descriptor already exists at this exact address.
    Duplicate,
}
