//! `LargeThpRequestGroup` — the six MiB-sized request-size buckets the
//! runtime will classify large requests into (one estimator per group in
//! the future integration).
//!
//! ## Buckets
//!
//! The buckets partition `u64` request byte counts on MiB boundaries:
//! `[0, 1)`, `[1, 8)`, `[8, 32)`, `[32, 128)`, `[128, 512)`,
//! `[512, ∞)` MiB. Classification is total for every `u64` input
//! (`of_bytes` matches on `bytes / MIB` with a compile-time-constant
//! non-zero divisor and explicit, exhaustive range arms — no wildcard arm
//! on this crate-owned enum).
//!
//! Groups classify REQUEST sizes only; they never change allocation sizes.
//!
//! This module remains pure fixed-point math; the allocator uses it through
//! the separate `large_thp_runtime` adapter.

/// A large-request size group (MiB-bucketed).
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LargeThpRequestGroup {
    /// `[0, 1)` MiB.
    Under1MiB,
    /// `[1, 8)` MiB.
    MiB1To8,
    /// `[8, 32)` MiB.
    MiB8To32,
    /// `[32, 128)` MiB.
    MiB32To128,
    /// `[128, 512)` MiB.
    MiB128To512,
    /// `[512, ∞)` MiB.
    Over512MiB,
}

impl LargeThpRequestGroup {
    /// One MiB in bytes (`2^20`).
    pub const MIB: u64 = 1 << 20;
    /// `1 MiB` boundary (first byte NOT in `Under1MiB`).
    pub const B1_MIB: u64 = Self::MIB;
    /// `8 MiB` boundary.
    pub const B8_MIB: u64 = 8 * Self::MIB;
    /// `32 MiB` boundary.
    pub const B32_MIB: u64 = 32 * Self::MIB;
    /// `128 MiB` boundary.
    pub const B128_MIB: u64 = 128 * Self::MIB;
    /// `512 MiB` boundary.
    pub const B512_MIB: u64 = 512 * Self::MIB;

    /// Classify a request byte count into its group. Total for every
    /// `u64` input: the divisor `MIB` is a compile-time non-zero constant
    /// (no div-by-zero) and the arms are explicit and exhaustive — no
    /// wildcard arm.
    #[must_use]
    pub const fn of_bytes(bytes: u64) -> Self {
        match bytes / Self::MIB {
            0 => Self::Under1MiB,
            1..=7 => Self::MiB1To8,
            8..=31 => Self::MiB8To32,
            32..=127 => Self::MiB32To128,
            128..=511 => Self::MiB128To512,
            512..=u64::MAX => Self::Over512MiB,
        }
    }
}
