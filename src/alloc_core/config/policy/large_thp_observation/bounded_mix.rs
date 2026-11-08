//! `bounded_mix` — the bounded seeded decorrelation primitive shared by
//! the large-THP observation family (control-cohort placement and
//! residency-window placement).
//!
//! ## Contract
//!
//! Exactly one splitmix64-style finalization per call: constant, bounded
//! work — NO rejection loops, no state carried between calls, no
//! allocation. The function is total for every `(seed, stream_index)`
//! pair:
//!
//! - a zero seed is valid; a zero mixed word is also a valid result;
//! - overflow is defined: every arithmetic step wraps;
//! - long streams are defined: the mixed value is the sum
//!   `seed + (stream_index + 1) * GOLDEN` (mod `2^64`), finalized once. The
//!   per-seed stream cycles with period `2^64` over `stream_index`
//!   (GOLDEN is odd); two seeds aliasing onto the same sum share a
//!   stream — a defined, documented aliasing, not a bug.
//!
//! The finalization chain (two xorshifts with odd multipliers, one final
//! xorshift) is a bijection on `u64`, so two distinct input sums never
//! finalize to the same output. The property the family's tests rely on:
//! `bounded_mix(s, i) != bounded_mix(s', i)` whenever `s != s'`, and
//! `bounded_mix(s, i) != bounded_mix(s, i')` whenever `i != i'` — the
//! pre-mix sums differ and the finalizer is injective.
//!
//! This is NOT a cryptographic primitive and NOT a general-purpose RNG:
//! it is a bounded decorrelation primitive, per the J13b prereg's
//! stratified-systematic selection (one draw per cohort of 8) and the
//! per-stratum window placement. Bounded decorrelation is NOT a
//! representativeness guarantee.
//!
//! This module is a standalone pure component; runtime integration into
//! the allocator is a separate future task.

/// One bounded seeded decorrelation draw.
///
/// Total, side-effect-free and const-evaluable: no rejection loop, no
/// allocation, no panic for any input; every arithmetic step wraps.
#[must_use]
pub const fn bounded_mix(seed: u64, stream_index: u64) -> u64 {
    const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut z = seed.wrapping_add(stream_index.wrapping_mul(GOLDEN));
    z = z.wrapping_add(GOLDEN);
    z ^= z >> 30;
    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z ^= z >> 27;
    z = z.wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    z
}
