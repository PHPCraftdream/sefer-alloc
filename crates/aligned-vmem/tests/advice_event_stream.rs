//! Pure codec/capture fixtures, not proof of native advice or mapping lifetimes.
#![cfg(feature = "bench-internals")]
use aligned_vmem::{AdviceEvent as Event, AdviceEventLog as Log};
#[path = "../src/bench_internals/advice/stream/slot.rs"]
mod slot;

#[cfg(any(
    miri,
    aligned_vmem_mock,
    not(any(target_os = "linux", target_os = "android"))
))]
#[test]
fn native_capture_is_rejected_on_models_and_unsupported_hosts() {
    let expected = "advice lifetime capture requires native Linux/Android";
    assert_eq!(
        aligned_vmem::AdviceCapture::begin_native().unwrap_err(),
        expected
    );
    assert_eq!(
        aligned_vmem::AdviceCapture::boundary().unwrap_err(),
        expected
    );
    assert_eq!(
        aligned_vmem::AdviceCapture::snapshot().unwrap_err(),
        expected
    );
}

fn fresh(key: usize) -> Event {
    Event::fresh(key, 4096, key + 128, 2048, 1024, 512, 2).unwrap()
}

#[test]
fn event_roundtrips_cover_all_groups_modes_thresholds_and_terminal_outcomes() {
    const MIB: usize = 1024 * 1024;
    const SPAN: usize = 1024 * MIB;
    for (group, requested) in [4096, MIB, 8 * MIB, 32 * MIB, 128 * MIB, 512 * MIB]
        .into_iter()
        .enumerate()
    {
        for mode in 0..=2 {
            for minimum in [0, requested, requested + 1] {
                let event =
                    Event::fresh(4096, SPAN + 8192, 8192, SPAN, requested, minimum, mode).unwrap();
                assert_eq!(event.group(), Some(group));
                assert_eq!(event.mode(), Some(mode));
                assert_eq!(event.threshold_eligible(), requested >= minimum);
                assert_eq!(Event::from_words(event.words()).unwrap(), event);
            }
            if mode != 0 {
                for bytes in [1, 4096, SPAN] {
                    for event in [
                        Event::accepted(4096, SPAN + 8192, 8192, SPAN, requested, bytes, mode)
                            .unwrap(),
                        Event::unknown_advice(
                            4096,
                            SPAN + 8192,
                            8192,
                            SPAN,
                            requested,
                            bytes,
                            mode,
                        )
                        .unwrap(),
                    ] {
                        assert_eq!(event.group(), Some(group));
                        assert_eq!(event.mode(), Some(mode));
                        assert_eq!(Event::from_words(event.words()).unwrap(), event);
                    }
                }
            }
        }
    }
    for confirmed in [false, true] {
        let event = Event::released(4096, 4096, confirmed).unwrap();
        assert_eq!(event.group(), None);
        assert_eq!(event.mode(), None);
        assert_eq!(event.confirmed_release(), confirmed);
        assert_eq!(Event::from_words(event.words()).unwrap(), event);
        for begin in [0, 17, 65536] {
            let marked = Event::released_at(4096, 4096, confirmed, begin).unwrap();
            assert_eq!(marked.release_begin(), Some(begin));
            assert_eq!(Event::from_words(marked.words()).unwrap(), marked);
            let large = Event::large_released_at(4096, 4096, confirmed, begin).unwrap();
            assert!(large.large_release());
            assert_eq!(Event::from_words(large.words()).unwrap(), large);
        }
    }
    assert!(Event::released_at(4096, 4096, true, 65537).is_err());
}

#[test]
fn independent_scalar_fixtures_pin_tags_fields_and_units() {
    let rows = [
        ([385, 4096, 12288, 8192, 4096, 1024, 512], Event::FRESH),
        ([66, 4096, 12288, 8192, 4096, 1024, 2048], Event::ACCEPTED),
        ([515, 4096, 12288, 0, 0, 0, 0], Event::RELEASED),
        ([1539, 4096, 12288, 7, 0, 0, 0], Event::RELEASED),
        ([3587, 4096, 12288, 7, 0, 0, 0], Event::RELEASED),
        (
            [132, 4096, 12288, 8192, 4096, 1024, 2048],
            Event::UNKNOWN_ADVICE,
        ),
    ];
    for (words, kind) in rows {
        let event = Event::from_words(words).unwrap();
        assert_eq!(event.kind(), kind);
        assert_eq!(event.words(), words);
    }
}

#[test]
fn malformed_reserved_and_false_labels_reject_without_normalization() {
    let original = [385, 4096, 12288, 8192, 4096, 1024, 512];
    for (field, value) in [
        (0, 0),
        (0, 5),
        (0, 6),
        (0, 7),
        (0, original[0] | (1 << 63)),
        (0, original[0] ^ 256),
        (0, original[0] | 8),
        (0, original[0] | (3 << 6)),
        (1, 0),
        (2, u64::MAX),
        (3, 16384),
        (4, 0),
        (5, 4097),
    ] {
        let mut invalid = original;
        invalid[field] = value;
        assert!(Event::from_words(invalid).is_err(), "{invalid:?}");
    }
    for field in 3..7 {
        let mut invalid = [515, 4096, 4096, 0, 0, 0, 0];
        invalid[field] = 1;
        assert!(Event::from_words(invalid).is_err());
    }
    for words in [
        [2563, 4096, 4096, 0, 0, 0, 0], // Large flag without a release marker.
        [3587, 4096, 4096, 65537, 0, 0, 0], // Marker beyond capture capacity.
        [3587, 4096, 4096, 2, 1, 0, 0], // Nonzero unused release field.
    ] {
        assert!(Event::from_words(words).is_err(), "{words:?}");
    }
    assert!(Event::accepted(4096, 4096, 4224, 2048, 1024, 512, 0).is_err());
    assert!(Event::unknown_advice(4096, 4096, 4224, 2048, 1024, 0, 2).is_err());
}

#[test]
fn published_slots_cannot_be_reused_or_read_before_completion() {
    let slot = slot::AdviceSlot::new();
    let first = fresh(4096).words();
    assert!(slot.read(1).is_err());
    assert!(slot.publish(0, first).is_err());
    assert!(slot.publish(u64::MAX, first).is_err());
    slot.publish(1, first).unwrap();
    assert_eq!(slot.read(1).unwrap(), first);
    assert!(slot.publish(1, fresh(8192).words()).is_err());
    assert!(slot.publish(2, fresh(8192).words()).is_err());
    assert_eq!(slot.read(1).unwrap(), first);
    assert!(slot.read(2).is_err());
}

#[test]
fn bounded_append_and_restart_failures_are_permanent_not_truncated_success() {
    let log = Log::<2>::new();
    assert!(log.boundary().is_err());
    log.begin().unwrap();
    assert_eq!(log.append(fresh(4096)).unwrap(), 0);
    assert_eq!(log.append(fresh(8192)).unwrap(), 1);
    assert_eq!(log.boundary().unwrap(), 2);
    assert_eq!(log.read(0, 2).unwrap(), fresh(4096));
    assert!(log.append(fresh(12288)).is_err());
    assert!(log.boundary().is_err());
    assert!(log.read(0, 2).is_err());
    assert!(log.append(fresh(16384)).is_err());
    let restart = Log::<2>::new();
    restart.begin().unwrap();
    restart.append(fresh(4096)).unwrap();
    assert!(restart.begin().is_err());
    assert!(restart.boundary().is_err());
    assert!(Log::<0>::new().begin().is_err());
    assert_eq!(Log::<65536>::slot_storage_bytes(), 4 * 1024 * 1024);
    assert!(Log::<65536>::total_storage_bytes() > Log::<65536>::slot_storage_bytes());
}

#[test]
fn concurrent_appends_publish_unique_complete_actual_records() {
    let log = Log::<16>::new();
    log.begin().unwrap();
    let positions = std::thread::scope(|scope| {
        let mut jobs = Vec::new();
        for worker in 0..4 {
            let log = &log;
            jobs.push(scope.spawn(move || {
                let mut positions = Vec::new();
                for operation in 0..4 {
                    let key = (worker * 4 + operation + 1) * 8192;
                    positions.push((log.append(fresh(key)).unwrap(), fresh(key)));
                }
                positions
            }));
        }
        let outcomes: Vec<_> = jobs.into_iter().map(|job| job.join()).collect();
        outcomes
            .into_iter()
            .flat_map(|result| result.unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(log.boundary().unwrap(), 16);
    let mut positions = positions;
    positions.sort_by_key(|(sequence, _)| *sequence);
    for (index, (sequence, expected)) in positions.into_iter().enumerate() {
        assert_eq!(sequence, u64::try_from(index).unwrap());
        assert_eq!(log.read(sequence, 16).unwrap(), expected);
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn incomplete_and_out_of_order_publication_require_a_complete_prefix() {
    let incomplete = Log::<2>::new();
    incomplete.begin().unwrap();
    let first = incomplete.reserve_unpublished_for_test().unwrap();
    assert_eq!(incomplete.append(fresh(8192)).unwrap(), 1);
    assert!(
        incomplete.boundary().is_err(),
        "cursor=2 is not two completed records"
    );
    assert!(
        incomplete
            .complete_reserved_for_test(first, fresh(4096))
            .is_err(),
        "no repair after invalid closure"
    );
    let ordered = Log::<2>::new();
    ordered.begin().unwrap();
    let first = ordered.reserve_unpublished_for_test().unwrap();
    assert_eq!(ordered.append(fresh(8192)).unwrap(), 1);
    ordered
        .complete_reserved_for_test(first, fresh(4096))
        .unwrap();
    assert_eq!(ordered.boundary().unwrap(), 2);
    assert_eq!(ordered.read(0, 2).unwrap(), fresh(4096));
    assert_eq!(ordered.read(1, 2).unwrap(), fresh(8192));
}

#[cfg(feature = "fault-injection")]
#[test]
fn cursor_wrap_never_turns_failed_capture_into_an_empty_success() {
    let log = Log::<2>::new();
    log.begin().unwrap();
    log.append(fresh(4096)).unwrap();
    log.seed_cursor_for_test(u64::MAX);
    assert!(log.append(fresh(8192)).is_err());
    assert!(log.boundary().is_err());
    assert!(log.append(fresh(12288)).is_err());
    assert!(log.read(0, 1).is_err());
}

#[test]
fn producer_errors_and_wrong_boundary_reads_disable_evidence() {
    let invalid = Log::<2>::new();
    invalid.record(Err("unarmed metadata failure"));
    invalid.begin().unwrap();
    invalid.record(Err("producer conversion failure"));
    assert!(invalid.boundary().is_err());
    assert!(invalid.append(fresh(4096)).is_err());
    let boundary = Log::<2>::new();
    boundary.begin().unwrap();
    boundary.append(fresh(4096)).unwrap();
    assert_eq!(boundary.boundary().unwrap(), 1);
    assert!(boundary.read(0, 2).is_err());
    assert!(boundary.boundary().is_err());
}
