//! Synthetic diagnostic-boundary mutants, not runtime or campaign measurements.
#[path = "../examples/_shared/j13b_control_interval.rs"]
mod control_interval;
#[path = "../examples/_shared/j13b_control_snapshot.rs"]
#[allow(dead_code)]
mod control_snapshot;
use control_interval::ControlInterval;
use control_snapshot::ControlSnapshot;
const ONE: u64 = 1 << 31;
fn pair(resident: u64, sampled: u64) -> (ControlSnapshot, ControlSnapshot, [u64; 8]) {
    let before = ControlSnapshot {
        ordinal: 0,
        controls: [[0; 8]; 6],
        learning: [[ONE, ONE, ONE, 0, 0]; 6],
    };
    let mut after = before;
    after.ordinal = 8;
    after.controls[1] = [1, 0, 1, 0, 0, 0, resident, sampled];
    after.learning[1][3] = 1;
    let tickets = std::array::from_fn(|index| {
        (u64::try_from(index).unwrap() << 2) | (u64::from(index == 3) << 1) | 1
    });
    (before, after, tickets)
}
fn close(
    before: &ControlSnapshot,
    after: &ControlSnapshot,
    tickets: &[u64],
) -> std::io::Result<ControlInterval> {
    ControlInterval::close(1, 4096, before, after, tickets)
}

#[test]
fn single_control_classes_use_strict_exact_integer_thresholds() {
    for (resident, sparse, dense) in [
        (0, true, false),
        (9, true, false),
        (10, false, false),
        (90, false, false),
        (91, false, true),
        (100, false, true),
    ] {
        let (before, after, tickets) = pair(resident, 100);
        let interval = close(&before, &after, &tickets).unwrap();
        assert_eq!(interval.eligible_fresh, 8);
        assert_eq!(interval.class_valid(true), sparse);
        assert_eq!(interval.class_valid(false), dense);
        assert_eq!(interval.applied, 1);
        assert_eq!(interval.dropped, 0);
        assert_eq!(interval.target_share_q31, ONE);
    }
}

#[test]
fn aggregate_or_incomplete_windows_cannot_become_individual_samples() {
    let (before, mut after, tickets) = pair(50, 100);
    after.controls[1][0] = 2;
    after.controls[1][2] = 2;
    after.learning[1][3] = 2;
    assert!(close(&before, &after, &tickets).is_err());
    let (before, mut after, tickets) = pair(1, 100);
    after.controls[1][2] = 0;
    assert!(
        close(&before, &after, &tickets).is_err(),
        "pending terminal reason"
    );
    after.ordinal = 7;
    assert!(close(&before, &after, &tickets).is_err());
}

#[test]
fn actual_ticket_capture_is_required_beyond_ordinal_totals() {
    let (before, after, tickets) = pair(1, 100);
    assert!(close(&before, &after, &tickets[..7]).is_err());
    for kind in 0..5 {
        let mut mutant = tickets;
        match kind {
            0 => mutant[0] = mutant[1],
            1 => mutant[0] = 0,
            2 => mutant[3] &= !2,
            3 => mutant[0] |= 2,
            _ => mutant[0] = (8 << 2) | 1,
        }
        assert!(
            close(&before, &after, &mutant).is_err(),
            "ticket mutant {kind}"
        );
    }
    let mut unaligned = before;
    unaligned.ordinal = 1;
    assert!(close(&unaligned, &after, &tickets).is_err());
}

#[test]
fn nonwrapping_counters_and_unchanged_other_groups_are_required() {
    let (before, after, tickets) = pair(1, 100);
    for column in 0..8 {
        let mut prior = before;
        prior.controls[1][column] = after.controls[1][column] + 1;
        assert!(close(&prior, &after, &tickets).is_err());
    }
    let mut prior = before;
    let mut end = after;
    prior.ordinal = u64::MAX - 7;
    end.ordinal = 0;
    assert!(close(&prior, &end, &tickets).is_err());
    end = after;
    end.controls[2][0] = 1;
    assert!(close(&before, &end, &tickets).is_err());
    end = after;
    end.learning[2][0] -= 1;
    assert!(close(&before, &end, &tickets).is_err());
}

#[test]
fn failed_premise_remote_realloc_and_error_never_supply_class_samples() {
    for reason in [1, 3, 4, 5] {
        let (before, mut after, tickets) = pair(0, 0);
        after.controls[1][2] = 0;
        after.controls[1][reason] = 1;
        after.learning[1][3] = 0;
        let interval = close(&before, &after, &tickets).unwrap();
        assert!(!interval.class_valid(true));
        assert!(!interval.class_valid(false));
        assert_eq!(interval.sampled_pages, 0);
    }
}

#[test]
fn admission_loss_is_separate_from_valid_sample_and_state_application() {
    let (before, mut after, tickets) = pair(1, 100);
    after.learning[1][3] = 0;
    after.learning[1][4] = 1;
    let interval = close(&before, &after, &tickets).unwrap();
    assert!(interval.class_valid(true));
    assert_eq!(interval.applied, 0);
    assert_eq!(interval.dropped, 1);
    after.learning[1][0] -= 1;
    assert!(close(&before, &after, &tickets).is_err());
    after.learning[1][0] = ONE;
    after.learning[1][4] = 0;
    assert!(close(&before, &after, &tickets).is_err());
    after.learning[1][3] = 1;
    after.learning[1][4] = u64::MAX;
    assert!(close(&before, &after, &tickets).is_err());
}

#[test]
fn residency_and_page_geometry_fail_closed_without_clamping() {
    for (resident, sampled) in [(0, 0), (2, 1), (0, 8193)] {
        let (before, after, tickets) = pair(resident, sampled);
        assert!(close(&before, &after, &tickets).is_err());
    }
    let (before, after, tickets) = pair(1, 100);
    for page in [0, 3] {
        assert!(ControlInterval::close(1, page, &before, &after, &tickets).is_err());
    }
    assert!(ControlInterval::close(6, 4096, &before, &after, &tickets).is_err());
}

#[test]
fn q31_and_learning_counter_boundaries_are_checked() {
    let (before, after, tickets) = pair(1, 100);
    for column in 0..3 {
        let mut mutant = after;
        mutant.learning[1][column] = ONE + 1;
        assert!(close(&before, &mutant, &tickets).is_err());
    }
    for column in [3, 4] {
        let mut prior = before;
        prior.learning[1][column] = 2;
        assert!(close(&prior, &after, &tickets).is_err());
    }
}
