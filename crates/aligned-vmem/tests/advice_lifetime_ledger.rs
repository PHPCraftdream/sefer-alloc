//! Pure inert event-stream accounting; no mapping, syscall or native-outcome proof.
#![cfg(feature = "bench-internals")]
use aligned_vmem::{AdviceLifetimeLedger as Ledger, AdviceLifetimeSnapshot as Snapshot};

fn accept(ledger: &mut Ledger, key: usize, bytes: usize, group: usize) {
    ledger
        .accepted(key, 4096, key + 128, 2048, bytes, group)
        .unwrap();
}

#[test]
fn accepted_ranges_survive_until_confirmed_original_reservation_release() {
    let mut ledger = Ledger::new();
    accept(&mut ledger, 4096, 1024, 2);
    accept(&mut ledger, 8192, 512, 3);
    let both = ledger.snapshot().unwrap();
    assert_eq!(both.process, [1536, 1536, 1536, 0]);
    assert_eq!(both.groups[2], [1024, 1024, 1024, 0]);
    assert_eq!(both.groups[3], [512, 512, 512, 0]);
    assert!(!ledger.released(1 << 20, 4096, true).unwrap());
    assert_eq!(ledger.snapshot().unwrap(), both);
    assert!(ledger.released(4096, 4096, true).unwrap());
    assert_eq!(ledger.snapshot().unwrap().process, [1536, 512, 1536, 1024]);
    assert!(ledger.released(8192, 4096, true).unwrap());
    assert_eq!(ledger.snapshot().unwrap().process, [1536, 0, 1536, 1536]);
    assert_eq!(ledger.retained_mapping_count(), 0);
    assert!(
        !ledger.released(8192, 4096, true).unwrap(),
        "never return bytes twice"
    );
    // Numeric address reuse is supported only after the prior confirmed terminal event.
    accept(&mut ledger, 4096, 128, 1);
    assert_eq!(ledger.snapshot().unwrap().process, [1664, 128, 1536, 1536]);
}

#[test]
fn process_peak_is_not_the_sum_of_disjoint_group_peaks() {
    let mut ledger = Ledger::new();
    accept(&mut ledger, 4096, 1024, 0);
    ledger.released(4096, 4096, true).unwrap();
    accept(&mut ledger, 8192, 2048, 5);
    let snapshot = ledger.snapshot().unwrap();
    assert_eq!(snapshot.groups[0][Snapshot::PEAK], 1024);
    assert_eq!(snapshot.groups[5][Snapshot::PEAK], 2048);
    assert_eq!(snapshot.process[Snapshot::PEAK], 2048);
    assert_eq!(snapshot.process[Snapshot::TOTAL], 3072);
    assert_eq!(snapshot.process[Snapshot::NOW], 2048);
    assert_eq!(snapshot.process[Snapshot::RELEASED], 1024);
}

#[test]
fn unsupported_release_preserves_entries_and_never_recovers_via_retry_or_reuse() {
    let mut ledger = Ledger::new();
    accept(&mut ledger, 4096, 1024, 2);
    let message = ledger.released(4096, 4096, false).unwrap_err();
    assert_eq!(message, "tracked native release outcome unproven");
    assert_eq!(ledger.retained_mapping_count(), 1);
    assert_eq!(ledger.snapshot().unwrap_err(), message);
    assert_eq!(ledger.released(4096, 4096, true).unwrap_err(), message);
    assert_eq!(
        ledger.accepted(4096, 4096, 4224, 2048, 128, 2).unwrap_err(),
        message
    );
    assert_eq!(ledger.retained_mapping_count(), 1);
    let mut unknown = Ledger::new();
    accept(&mut unknown, 8192, 512, 1);
    assert_eq!(
        unknown.unknown_advice().unwrap_err(),
        "native advice effect unproven"
    );
    assert!(unknown.snapshot().is_err());
    assert_eq!(unknown.retained_mapping_count(), 1);
}

#[test]
fn malformed_geometry_duplicate_and_overlapping_records_fail_closed() {
    for (reservation, len, usable, span, bytes, group) in [
        (0, 4096, 128, 2048, 1024, 0),
        (4096, 0, 4096, 0, 0, 0),
        (4096, 4096, 4095, 2048, 1024, 0),
        (4096, 4096, 8192, 1, 1, 0),
        (4096, 4096, 4224, 2048, 2049, 0),
        (4096, 4096, 4224, 2048, 0, 0),
        (4096, 4096, 4224, 2048, 1024, 6),
        (usize::MAX, 1, usize::MAX, 1, 1, 0),
        (1, usize::MAX - 1, usize::MAX, 1, 1, 0),
    ] {
        let mut ledger = Ledger::new();
        assert!(ledger
            .accepted(reservation, len, usable, span, bytes, group)
            .is_err());
        assert!(
            ledger.snapshot().is_err(),
            "unsupported is not an empty snapshot"
        );
        assert_eq!(ledger.retained_mapping_count(), 0);
    }
    let mut duplicate = Ledger::new();
    accept(&mut duplicate, 4096, 1024, 2);
    assert_eq!(
        duplicate
            .accepted(4096, 4096, 4224, 2048, 1024, 2)
            .unwrap_err(),
        "duplicate live advice reservation"
    );
    assert_eq!(duplicate.retained_mapping_count(), 1);
    let mut overlap = Ledger::new();
    accept(&mut overlap, 4096, 1024, 2);
    assert_eq!(
        overlap
            .accepted(6144, 4096, 6272, 2048, 1024, 2)
            .unwrap_err(),
        "overlapping live advice reservations"
    );
    assert_eq!(overlap.retained_mapping_count(), 1);
    let mut wrong_release = Ledger::new();
    accept(&mut wrong_release, 4096, 1024, 2);
    assert_eq!(
        wrong_release.released(4096, 2048, true).unwrap_err(),
        "release geometry differs from original reservation"
    );
    assert_eq!(wrong_release.retained_mapping_count(), 1);
    assert!(wrong_release.snapshot().is_err());
}

#[test]
fn capacity_is_a_hard_bound_not_a_lossy_eviction_policy() {
    let mut ledger = Ledger::new();
    for index in 1..=Ledger::CAPACITY {
        accept(&mut ledger, index * 4096, 1024, index % 6);
    }
    assert_eq!(ledger.retained_mapping_count(), Ledger::CAPACITY);
    assert_eq!(
        ledger.snapshot().unwrap().process[Snapshot::NOW],
        u64::try_from(Ledger::CAPACITY * 1024).unwrap()
    );
    assert!(ledger.released(4096, 4096, true).unwrap());
    accept(&mut ledger, 4096, 512, 0);
    assert_eq!(
        ledger
            .accepted(
                (Ledger::CAPACITY + 1) * 4096,
                4096,
                (Ledger::CAPACITY + 1) * 4096,
                2048,
                1024,
                0
            )
            .unwrap_err(),
        "advice lifetime capacity exhausted"
    );
    assert_eq!(ledger.retained_mapping_count(), Ledger::CAPACITY);
    assert!(ledger.snapshot().is_err());
}

#[test]
fn empty_stream_is_genuine_zero_and_partial_effect_is_not_zero() {
    let mut empty = Ledger::new();
    let snapshot = empty.snapshot().unwrap();
    assert_eq!(snapshot.process, [0; 4]);
    assert_eq!(snapshot.groups, [[0; 4]; 6]);
    assert!(
        !empty.released(4096, 4096, false).unwrap(),
        "an unrelated mapping is not accepted advice"
    );
    assert_eq!(empty.snapshot().unwrap(), snapshot);
    assert!(empty.unknown_advice().is_err());
    assert!(empty.snapshot().is_err());
}

#[cfg(target_pointer_width = "64")]
#[test]
fn cumulative_byte_overflow_rejects_without_inserting_a_new_mapping() {
    let bytes = usize::MAX / 2;
    for groups in [[0, 0, 0], [0, 1, 2]] {
        let mut ledger = Ledger::new();
        for group in &groups[..2] {
            ledger.accepted(1, bytes, 1, bytes, bytes, *group).unwrap();
            assert!(ledger.released(1, bytes, true).unwrap());
        }
        assert_eq!(
            ledger.snapshot().unwrap().process[Snapshot::TOTAL],
            u64::MAX - 1
        );
        assert_eq!(ledger.retained_mapping_count(), 0);
        assert_eq!(
            ledger.accepted(1, 2, 1, 2, 2, groups[2]).unwrap_err(),
            "accepted advice total overflow"
        );
        assert_eq!(ledger.retained_mapping_count(), 0);
        assert!(ledger.snapshot().is_err());
    }
}
