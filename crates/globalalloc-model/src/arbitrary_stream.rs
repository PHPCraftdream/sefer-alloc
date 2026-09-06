//! The `arbitrary` front-end: [`OpStream`], an [`Arbitrary`] wrapper decoding
//! fuzzer bytes into a bounded `Vec<Op>` ready for [`crate::drive`].
//!
//! This is the `cargo fuzz` / libFuzzer front-end over the shared model. It
//! bounds fuzzer-derived sizes and alignments so a single input cannot ask the
//! OS for gigabytes (which would OOM the fuzzer, not find a bug), mirroring the
//! historical `global_alloc_ops` target. Sizes are bounded AND weighted: a
//! uniform `1..=2 MiB` draw spends the fuzz budget on multi-megabyte byte fills
//! instead of allocator state space, so the size distribution is small-heavy by
//! default (9:1), mirroring the proptest front-end.

use alloc::vec::Vec;
use arbitrary::{Arbitrary, Unstructured};

use crate::{Config, Op};

/// Maximum ops decoded from one fuzz input (caps sequence length so a single
/// input can't OOM the fuzzer with a giant stream).
const MAX_OPS: usize = 2048;
/// Absolute alignment exponent cap, independent of `Config`: generated
/// aligns never exceed `2^21` (2 MiB) even when `max_align` is larger —
/// staying below a typical 4 MiB segment so large-align routing is
/// exercised without hitting a rejected corridor. The EFFECTIVE cap is
/// `min(2^ALIGN_POW_CAP_EXP, Config::max_align)`: with the default
/// `max_align` (4096) aligns stop at `2^12`; a front-end wanting the full
/// 2 MiB reach passes `max_align: 2 MiB` (the in-tree fuzz target does).
const ALIGN_POW_CAP_EXP: u32 = 21;

/// Bound a fuzzer-derived raw size into `1..=small_max` or
/// `small_max+1..=large_max`, choosing the arm with `Config`'s own weight
/// ratio (default 9:1 small:large) and driving the magnitude from the
/// remaining high bits so arm choice and size vary independently.
fn bound_size(raw: u32, config: &Config) -> usize {
    let total = config
        .small_weight
        .saturating_add(config.large_weight)
        .max(1) as usize;
    let bucket = (raw as usize) % total;
    let magnitude = (raw as usize) / total;
    if bucket < config.small_weight as usize {
        magnitude % config.small_max.max(1) + 1
    } else {
        let lo = config.small_max.saturating_add(1);
        let hi = config.large_max.max(lo);
        // Saturating: an extreme `large_max` (e.g. `usize::MAX`) must not
        // overflow this arithmetic into a division-by-zero panic.
        lo + magnitude % hi.saturating_sub(lo).saturating_add(1)
    }
}

/// Derive a power-of-two alignment in `[1, min(2 MiB, max_align)]` from a fuzzer byte.
fn bound_align(raw: u8, config: &Config) -> usize {
    let cap = config.max_align.max(1);
    let max_exp = cap.trailing_zeros().min(ALIGN_POW_CAP_EXP);
    1usize << (raw as usize % (max_exp as usize + 1))
}

/// Raw fuzzer-decoded op, before size/align bounding. Kept private; the public
/// surface is the bounded [`OpStream`].
#[derive(Arbitrary, Debug)]
enum RawOp {
    /// Raw `Alloc` before bounding.
    Alloc {
        /// Raw size.
        size: u32,
        /// Raw alignment exponent.
        align_pow: u8,
    },
    /// Raw `AllocZeroed` before bounding.
    AllocZeroed {
        /// Raw size.
        size: u32,
        /// Raw alignment exponent.
        align_pow: u8,
    },
    // u16, not usize: the index is reduced modulo the live count downstream,
    // so a full machine word of fuzzer entropy per index buys nothing and a
    // 4 KiB libFuzzer input should decode hundreds of ops, not dozens.
    /// Raw `Dealloc` before bounding.
    Dealloc(u16),
    /// Raw `Realloc` before bounding.
    Realloc {
        /// Index into the live set.
        i: u16,
        /// Raw new size.
        new_size: u32,
    },
}

impl RawOp {
    fn bound(self, config: &Config) -> Op {
        match self {
            RawOp::Alloc { size, align_pow } => Op::Alloc {
                size: bound_size(size, config),
                align: bound_align(align_pow, config),
            },
            RawOp::AllocZeroed { size, align_pow } => Op::AllocZeroed {
                size: bound_size(size, config),
                align: bound_align(align_pow, config),
            },
            RawOp::Dealloc(i) => Op::Dealloc(i as usize),
            RawOp::Realloc { i, new_size } => Op::Realloc {
                i: i as usize,
                new_size: bound_size(new_size, config),
            },
        }
    }
}

/// A bounded op stream decoded from fuzzer bytes. Feed `OpStream::ops` to
/// `crate::drive`.
#[derive(Clone, Debug)]
pub struct OpStream {
    /// The decoded, bounded operations.
    pub ops: Vec<Op>,
}

impl OpStream {
    /// Decode a bounded op stream from `u`, with sizes/aligns shaped by
    /// `config` (this front-end reads `small_max`, `large_max`,
    /// `small_weight`, `large_weight`, `max_align`; `drive` additionally reads
    /// `Config::double_free`).
    ///
    /// The `Arbitrary` impl delegates here with `Config::default()`; this
    /// inherent constructor is the config-aware route, and the in-tree
    /// `global_alloc_ops` fuzz target drives it directly with an explicit
    /// `Config` (its historical 2 MiB size / 2^21 align reach).
    ///
    /// # Panics
    ///
    /// Panics before decoding anything if `config` violates a generator
    /// precondition (see [`Config::validate`]).
    pub fn arbitrary_with_config(
        u: &mut Unstructured<'_>,
        config: Config,
    ) -> arbitrary::Result<Self> {
        config.validate();
        // Each item is a `Result<RawOp>`; skip undecodable items (a truncated
        // trailing op at the end of the input) and cap the length (mirrors the
        // historical `arbitrary_iter().take(2048)`).
        let ops = u
            .arbitrary_iter::<RawOp>()?
            .take(MAX_OPS)
            .filter_map(Result::ok)
            .map(|raw| raw.bound(&config))
            .collect();
        Ok(OpStream { ops })
    }
}

impl<'a> Arbitrary<'a> for OpStream {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        Self::arbitrary_with_config(u, Config::default())
    }
}
