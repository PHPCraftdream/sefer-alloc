//! `peak_live_count`: the pre-pass `drive` reserves its model capacity from.
//! The counter must equal the TRUE maximum liveness — faithful to the real
//! up/down dynamics — not the history length.

// Nothing here compiles without the `internals` feature: the entire subject
// is the gated export.
#![cfg(feature = "internals")]

use globalalloc_model::{peak_live_count, Op};

#[test]
fn empty_and_all_dealloc_histories_have_zero_peak() {
    // `drive` skips every `Dealloc` against an empty live set, so a history
    // that never allocates keeps the model empty no matter how long it is.
    assert_eq!(peak_live_count(&[]), 0);
    let ops = [Op::Dealloc(0); 64];
    assert_eq!(peak_live_count(&ops), 0);
}

#[test]
fn alternating_alloc_dealloc_history_has_peak_one() {
    // The motivating shape: a long alternating history has peak liveness 1,
    // where reserving the history length would have reserved 10_000.
    let ops: Vec<Op> = (0..10_000)
        .map(|i| {
            if i % 2 == 0 {
                Op::Alloc { size: 16, align: 8 }
            } else {
                Op::Dealloc(0)
            }
        })
        .collect();
    assert_eq!(peak_live_count(&ops), 1);
}

#[test]
fn allocs_before_deallocs_peak_at_the_alloc_count() {
    let ops = [
        Op::Alloc { size: 8, align: 8 },
        Op::Alloc { size: 8, align: 8 },
        Op::Alloc { size: 8, align: 8 },
        Op::AllocZeroed { size: 8, align: 8 },
        Op::Dealloc(0),
        Op::Dealloc(1),
    ];
    assert_eq!(peak_live_count(&ops), 4);
}

#[test]
fn realloc_never_raises_the_peak() {
    let ops = [
        Op::Alloc { size: 8, align: 8 },
        Op::Realloc { i: 0, new_size: 64 },
        Op::Realloc { i: 0, new_size: 8 },
    ];
    assert_eq!(peak_live_count(&ops), 1);
}

#[test]
fn churn_peak_matches_a_hand_replayed_live_count() {
    // A churn history cross-checked by hand against the replay's live count
    // at every step (counts annotated per line; peak 3 at op 4).
    let ops = [
        Op::Alloc { size: 8, align: 8 },    // live 1
        Op::Alloc { size: 8, align: 8 },    // live 2
        Op::Dealloc(1),                     // live 1
        Op::Alloc { size: 8, align: 8 },    // live 2
        Op::Alloc { size: 8, align: 8 },    // live 3 (peak)
        Op::Realloc { i: 2, new_size: 32 }, // live 3 (replace in place)
        Op::Dealloc(0),                     // live 2
        Op::Dealloc(9),                     // live 1 (index reduced modulo live count)
    ];
    assert_eq!(peak_live_count(&ops), 3);
}
