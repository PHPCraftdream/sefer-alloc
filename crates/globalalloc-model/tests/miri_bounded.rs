//! A tiny, hand-built op stream driven against `System` — the bounded-miri
//! coverage for the shared `drive` loop + M1–M4 oracle code under strict
//! provenance. Kept small (a few small allocations, no multi-MiB writes) so it
//! finishes fast under the interpreter; the exhaustive shape is the native
//! proptest/arbitrary tests. No feature gate — always available.

use std::alloc::System;

use globalalloc_model::{drive, Config, Op};

#[test]
fn bounded_ops_are_ub_free_against_system() {
    // A deterministic mix exercising every op + the run-end M3 sweep and the
    // teardown free walk, all through the crate's shared oracle path.
    let ops = vec![
        Op::Alloc { size: 32, align: 8 },
        Op::AllocZeroed {
            size: 64,
            align: 16,
        },
        Op::Alloc { size: 17, align: 1 },
        Op::Realloc { i: 0, new_size: 96 }, // grow
        Op::Realloc { i: 1, new_size: 8 },  // shrink
        Op::Dealloc(2),
        Op::Alloc {
            size: 128,
            align: 64,
        }, // over-aligned
        Op::Realloc { i: 0, new_size: 24 },
        Op::Dealloc(0),
    ];
    // double_free stays OFF — System would corrupt on a real double-free.
    drive(&System, Config::default(), &ops);
}
