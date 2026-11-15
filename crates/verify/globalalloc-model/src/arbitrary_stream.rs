//! The `arbitrary` front-end: [`OpStream`], an [`Arbitrary`] wrapper decoding
//! fuzzer bytes into a bounded `Vec<Op>` ready for [`crate::drive`].
//!
//! This is the `cargo fuzz` / libFuzzer front-end over the shared model.
//! Its unconditional input bound is the op COUNT (`MAX_OPS`, below); per-op
//! sizes and alignments are bounded by the [`Config`] handed in, so a
//! default/small config keeps sizes modest, but the mechanism does not itself
//! cap sizes below u32/gigabyte ranges — the config-aware constructor
//! deliberately reaches above `u32::MAX` when configured to, and only the
//! op-count bound is unconditional (mirroring the historical
//! `global_alloc_ops` target, which passes an explicit config for its wider
//! reach). Sizes are bounded AND weighted: a
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

/// Draw a size from the configured weighted arms.
///
/// Arm selection uses the complete, non-overflowing `u64` weight sum. The
/// size is a separate `Unstructured` range draw, so its reach is not limited
/// by the arm-choice bits or by `u32`. Exact byte-to-value reduction belongs
/// to `arbitrary`; this API promises bounds and relative buckets, not a stable
/// or statistically uniform fuzz-input encoding.
fn bound_size(u: &mut Unstructured<'_>, config: &Config) -> arbitrary::Result<usize> {
    let small_weight = u64::from(config.small_weight);
    let total = small_weight + u64::from(config.large_weight);
    // `arbitrary_with_config` validates that at least one arm is enabled.
    let bucket = u.int_in_range(0..=total - 1)?;

    if bucket < small_weight {
        u.int_in_range(1..=config.small_max.max(1))
    } else {
        let lo = config.small_max.saturating_add(1);
        let hi = config.large_max.max(lo);
        u.int_in_range(lo..=hi)
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
        /// Raw alignment exponent.
        align_pow: u8,
    },
    /// Raw `AllocZeroed` before bounding.
    AllocZeroed {
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
    },
}

impl RawOp {
    fn bound(self, u: &mut Unstructured<'_>, config: &Config) -> arbitrary::Result<Op> {
        Ok(match self {
            RawOp::Alloc { align_pow } => Op::Alloc {
                size: bound_size(u, config)?,
                align: bound_align(align_pow, config),
            },
            RawOp::AllocZeroed { align_pow } => Op::AllocZeroed {
                size: bound_size(u, config)?,
                align: bound_align(align_pow, config),
            },
            RawOp::Dealloc(i) => Op::Dealloc(i as usize),
            RawOp::Realloc { i } => Op::Realloc {
                i: i as usize,
                new_size: bound_size(u, config)?,
            },
        })
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
    /// Byte-to-op decoding is not a stable format; generator changes may
    /// reinterpret existing corpus bytes while preserving these bounds.
    ///
    /// `Arbitrary` takes no parameters, so a custom [`Config`] cannot be
    /// threaded through the plain `fuzz_target!(|stream: OpStream| ...)`
    /// form — and the obvious workaround, an untyped
    /// `fuzz_target!(|data: &[u8]|)` closure that decodes by hand, silently
    /// loses libFuzzer's structured crash report (the `Debug`-based
    /// rendering behind `cargo fuzz fmt`). Wrap the stream in a local
    /// newtype whose `Arbitrary` impl delegates here instead; the typed
    /// fuzz target keeps the crash report:
    ///
    /// ```text
    /// #[derive(Debug)]
    /// struct MyStream(OpStream);
    ///
    /// impl<'a> arbitrary::Arbitrary<'a> for MyStream {
    ///     fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
    ///         OpStream::arbitrary_with_config(u, my_config()).map(Self)
    ///     }
    /// }
    ///
    /// // `MyStream`'s `Debug` output renders on a crash, exactly as with
    /// // the default-config form:
    /// fuzz_target!(|s: MyStream| {
    ///     drive(&allocator_under_test, my_config(), &s.0.ops);
    /// });
    /// ```
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
        let mut ops = Vec::new();
        for _ in 0..MAX_OPS {
            // Keep `arbitrary_iter`'s continuation-bit and truncated-item
            // behavior while allowing each op's size to draw from `u`.
            if !u.arbitrary::<bool>().unwrap_or(false) {
                break;
            }
            if let Ok(op) = RawOp::arbitrary(u).and_then(|raw| raw.bound(u, &config)) {
                ops.push(op);
            }
        }
        Ok(OpStream { ops })
    }
}

impl<'a> Arbitrary<'a> for OpStream {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        Self::arbitrary_with_config(u, Config::default())
    }
}
