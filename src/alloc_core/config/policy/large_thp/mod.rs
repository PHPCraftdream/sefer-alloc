//! `large_thp` — the large-THP (transparent-huge-page) hint-volume
//! gradient-estimator family: pure fixed-point Q31 adaptation math for
//! adapting a large-allocation THP-hint share per request-size group.
//!
//! ## Family overview
//!
//! - [`LargeThpShare`](LargeThpShare): a Q31 fixed-point share in
//!   `[0, 1]` (`SCALE == 2^31` raw units == `1.0`, resolution `2^-31`),
//!   clamped at construction — total for every input.
//! - [`LargeThpCurve`](LargeThpCurve): one of exactly three closed-form
//!   Q31 → Q31 shaping curves (floor-sqrt `Generous`, identity `Linear`,
//!   floor-square `Strict`), each with a documented `<= 1 ulp` error bound.
//! - [`LargeThpGradient`](LargeThpGradient): the normalized parameter set
//!   (asymmetric EMA shifts capped at `MAX_SHIFT == 31`, anchor, floor,
//!   exact rational gain, curve) with three presets
//!   (`CAUTIOUS`, `EAGER`, `STEADY`) and a total, const-normalizing
//!   `custom` constructor.
//! - [`LargeThpEstimator`](LargeThpEstimator): the two-state (fast `e`,
//!   slow mean `m`) asymmetric EMA starting at `1.0`, with UPDATE-THEN-
//!   REPORT semantics on `observe`; per-step quantization error `<= 0.5`
//!   ulp and accumulated EMA error bound `<= 2^(shift-32)` of full scale
//!   (`<= 2^-26` for all three presets, whose max shift is `6`).
//! - [`LargeThpRequestGroup`](LargeThpRequestGroup): the six MiB buckets
//!   (`[0,1)`, `[1,8)`, `[8,32)`, `[32,128)`, `[128,512)`, `[512,∞)` MiB)
//!   request sizes classify into; the runtime will keep one estimator per
//!   group.
//!
//! ## Deliberate limitation: explicit curve forms, not a real exponent
//!
//! The curve family is exactly the three closed forms above — NOT a
//! gamma-shaped real exponent `pow(k, r)` family — because real-exponent
//! curves require floating-point `pow`/`log`, which this `core`-only,
//! allocation-free module bans. Any new shape must be added as an explicit
//! new enum variant with its own bounded integer implementation.
//!
//! All arithmetic is integers only (`u64`/`i64` widened intermediates with
//! documented bounds), saturating/clamping, and total: no function in this
//! family panics for any input value.
//!
//! These components are standalone exported config/math types; runtime
//! integration into the allocator is a separate future task.

pub mod large_thp_curve;
pub mod large_thp_estimator;
pub mod large_thp_gradient;
pub mod large_thp_request_group;
pub mod large_thp_share;

pub use large_thp_curve::LargeThpCurve;
pub use large_thp_estimator::LargeThpEstimator;
pub use large_thp_gradient::LargeThpGradient;
pub use large_thp_request_group::LargeThpRequestGroup;
pub use large_thp_share::LargeThpShare;
