//! The unforgeable opt-in token for the M2 double-free-is-no-op oracle.

/// Proof that the allocator under test permits the M2 driver's one immediate
/// repetition of a just-completed matching `dealloc` as a safe no-op.
///
/// This does not authorize arbitrary stale-pointer deallocations: the repeated
/// call has the same pointer and layout and occurs before any other allocator
/// call. The token is unforgeable in safe code because this operation is
/// undefined behavior for an ordinary `GlobalAlloc`, including a system
/// allocator.
///
/// The only constructor is [`DoubleFreeOk::new`], a `const unsafe fn`, so
/// producing a value — and with it enabling
/// [`Config::double_free`](crate::Config::double_free) — always crosses an
/// explicit `unsafe` boundary: exactly the consent the M2 oracle requires,
/// which a plain `bool` field could never capture. The field is private, so
/// the type cannot be pattern-constructed outside this module either.
///
/// The driver cannot directly observe that the second call was a no-op. It can
/// only detect corruption that remains visible at a later fill verification;
/// transient corruption repaired between observations is outside the oracle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DoubleFreeOk(());

// No `Default` impl on purpose: a safe default constructor would
// reintroduce the exact hole this type closes. (No `#[allow(clippy::
// new_without_default)]` needed: that lint skips `unsafe fn new` already.)
impl DoubleFreeOk {
    /// Construct the token.
    ///
    /// # Safety
    /// The allocator subsequently passed to [`drive`](crate::drive) must
    /// document that immediately repeating a matching `dealloc`, with no
    /// intervening allocator call, is a safe no-op (e.g. sefer's `AllocCore`).
    /// This is stronger than `GlobalAlloc`, which makes the repeated call
    /// undefined behavior. Never construct this token for `System` or any
    /// allocator reached through the blanket `GlobalAlloc` implementation.
    pub const unsafe fn new() -> Self {
        Self(())
    }
}
