//! The `arbitrary` front-end: [`OpStream`], an [`Arbitrary`] wrapper decoding
//! fuzzer bytes into a bounded `Vec<Op>` ready for [`crate::drive`].
//!
//! This is the `cargo fuzz` / libFuzzer front-end over the shared model. It
//! bounds fuzzer-derived sizes and alignments so a single input cannot ask the
//! OS for gigabytes (which would OOM the fuzzer, not find a bug), mirroring the
//! historical `global_alloc_ops` target.

use arbitrary::{Arbitrary, Unstructured};

use crate::Op;

/// Maximum ops decoded from one fuzz input (caps sequence length so a single
/// input can't OOM the fuzzer with a giant stream).
const MAX_OPS: usize = 2048;
/// Size modulus: `1 ..= 2 MiB` covers small classes and a bit past a typical
/// large threshold.
const SIZE_MOD: usize = 2 * 1024 * 1024;
/// Alignment exponent modulus: `2^0 .. 2^21` (1 .. 2 MiB), staying below a
/// typical 4 MiB segment so large-align routing is exercised without hitting a
/// rejected corridor.
const ALIGN_POW_MOD: u8 = 22;

/// Bound a fuzzer-derived raw size into `1 ..= 2 MiB`.
fn bound_size(raw: u32) -> usize {
    (raw as usize % SIZE_MOD) + 1
}

/// Derive a power-of-two alignment in `[1, 2 MiB]` from a fuzzer byte.
fn bound_align(raw: u8) -> usize {
    1usize << (raw % ALIGN_POW_MOD)
}

/// Raw fuzzer-decoded op, before size/align bounding. Kept private; the public
/// surface is the bounded [`OpStream`].
#[derive(Arbitrary, Debug)]
enum RawOp {
    Alloc { size: u32, align_pow: u8 },
    AllocZeroed { size: u32, align_pow: u8 },
    Dealloc(usize),
    Realloc { i: usize, new_size: u32 },
}

impl RawOp {
    fn bound(self) -> Op {
        match self {
            RawOp::Alloc { size, align_pow } => Op::Alloc {
                size: bound_size(size),
                align: bound_align(align_pow),
            },
            RawOp::AllocZeroed { size, align_pow } => Op::AllocZeroed {
                size: bound_size(size),
                align: bound_align(align_pow),
            },
            RawOp::Dealloc(i) => Op::Dealloc(i),
            RawOp::Realloc { i, new_size } => Op::Realloc {
                i,
                new_size: bound_size(new_size),
            },
        }
    }
}

/// A bounded op stream decoded from fuzzer bytes. Feed [`OpStream::ops`] to
/// [`crate::drive`].
#[derive(Debug)]
pub struct OpStream {
    /// The decoded, bounded operations.
    pub ops: Vec<Op>,
}

impl<'a> Arbitrary<'a> for OpStream {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // Each item is a `Result<RawOp>`; stop at the first decode error and cap
        // the length (mirrors the historical `arbitrary_iter().take(2048)`).
        let ops = u
            .arbitrary_iter::<RawOp>()?
            .take(MAX_OPS)
            .filter_map(Result::ok)
            .map(RawOp::bound)
            .collect();
        Ok(OpStream { ops })
    }
}
