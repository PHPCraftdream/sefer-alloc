//! `LargeThpObservationError` — the single explicit rejection vocabulary
//! of the `large_thp_observation` family.
//!
//! Every variant is a REJECTED input, reported explicitly: the family
//! never silently clamps, silently truncates, or lets a request fall out
//! of range. The geometry constructor and the residency-plan constructor
//! both report through this one enum so callers match one error type.
//!
//! This module is a standalone pure component; runtime integration into
//! the allocator is a separate future task.

/// The explicit rejection reasons of the span-geometry and
/// residency-plan constructors. Total: constructing and matching these
/// values never panics.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LargeThpObservationError {
    /// Span size was `0` — a live span always spans at least one page.
    ZeroSpanSize,
    /// Base page size was `0` — division by zero is rejected, never
    /// attempted.
    ZeroPageSize,
    /// Base page size was not a power of two — unsupported geometry
    /// (real base pages are power-of-two sized).
    PageSizeNotPowerOfTwo,
    /// The span holds fewer than one full base page (`span_size_bytes <
    /// base_page_size`); a trailing partial page alone is not
    /// sampleable.
    NoFullPages,
    /// The span's base address was not a multiple of the base page size —
    /// rejected as an UNSUPPORTED base premise, NOT because every
    /// unaligned span fails to host a whole aligned page (a span like
    /// `[1, 8193)` with 4096-byte pages does contain the aligned page
    /// `[4096, 8192)`); inward re-alignment is not offered, so such
    /// geometry is out of scope by contract. The premise is VALIDATED
    /// here, not deferred to the caller.
    BaseNotPageAligned,
    /// `base_address + span_size_bytes` exceeds `u64::MAX` — the
    /// absolute range end is unrepresentable.
    AddressOverflow,
    /// `requested_windows == 0` — an empty sample is a rejected request,
    /// not a silently empty plan.
    EmptySample,
    /// `requested_windows > MAX_WINDOWS (16)` — the 16-window cap is a
    /// hard contract, never silently clamped.
    WindowCapExceeded,
    /// `window_bytes == 0` — a zero-page window samples nothing.
    ZeroWindow,
    /// `window_bytes > MAX_WINDOW_BYTES (2 MiB)` — the per-window cap is
    /// a hard contract, never silently clamped.
    WindowTooLarge,
    /// `window_bytes` is not a multiple of the base page size — a window
    /// must be page-aligned so the future owning `mincore` walk sees
    /// whole base pages.
    WindowNotPageAligned,
    /// A single window of `window_bytes` does not fit into the span's
    /// full pages at all.
    WindowExceedsSpan,
    /// Defensive arm: an internal invariant-bound narrowing failed.
    /// Unreachable by construction — every planned byte offset is bounded
    /// by the span size — but present so the constructors fail closed
    /// instead of silently wrapping out of range.
    InternalRange,
}
