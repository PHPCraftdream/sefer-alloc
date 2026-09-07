//! The unforgeable opt-in token for the M2 double-free-is-no-op oracle.

/// Proof that the allocator under test treats a redundant `dealloc` of an
/// already-freed pointer as a safe no-op. Unforgeable in safe code: the M2
/// oracle is UB against a real `malloc`.
///
/// The only constructor is [`DoubleFreeOk::new`], a `const unsafe fn`, so
/// producing a value — and with it enabling
/// [`Config::double_free`](crate::Config::double_free) — always crosses an
/// explicit `unsafe` boundary: exactly the consent the M2 oracle requires,
/// which a plain `bool` field could never capture. The field is private, so
/// the type cannot be pattern-constructed outside this module either.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoubleFreeOk(());

// No `Default` impl on purpose: a safe default constructor would
// reintroduce the exact hole this type closes.
#[allow(clippy::new_without_default)]
impl DoubleFreeOk {
    /// # Safety
    /// The allocator subsequently passed to [`drive`](crate::drive) must
    /// document that a second `dealloc` of an already-freed pointer is a
    /// safe no-op (e.g. sefer's `AllocCore`). This is STRONGER than
    /// `GlobalAlloc`, which makes it undefined behaviour — never construct
    /// this for `System` or any allocator reached through the blanket
    /// `GlobalAlloc` impl.
    pub const unsafe fn new() -> Self {
        Self(())
    }
}
