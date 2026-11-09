//! Ordered synthetic event streams; native-emitter proof lives in separate tests.
#![cfg(feature = "bench-internals")]
use aligned_vmem::{
    AdviceEvent as Event, AdviceLifetimeSnapshot as Totals, AdviceReplay as Replay,
};

fn fresh(key: usize, minimum: usize, mode: u8) -> Event {
    Event::fresh(key, 4096, key + 128, 2048, 1024, minimum, mode).unwrap()
}
fn accepted(key: usize, mode: u8) -> Event {
    Event::accepted(key, 4096, key + 128, 2048, 1024, 1024, mode).unwrap()
}

#[test]
fn real_disposition_fields_are_distinct_from_advice_and_reservation_bytes() {
    for mode in 0..=2 {
        let mut replay = Replay::new();
        replay.push(0, fresh(4096, 512, mode)).unwrap();
        let first = replay.snapshot().unwrap();
        assert_eq!(first.requests[0], [1, 1, u64::from(mode != 0)]);
        assert_eq!(first.requested_bytes[0], 1024);
        assert_eq!(first.usable_span_bytes[0], 2048);
        assert_eq!(first.reservation_bytes[0], 4096);
        assert_eq!(first.outstanding_fresh, 1);
        assert_eq!(first.advice.process, [0; 4]);
        let next = if mode != 0 {
            replay.push(1, accepted(4096, mode)).unwrap();
            assert_eq!(
                replay.snapshot().unwrap().advice.process,
                [1024, 1024, 1024, 0]
            );
            2
        } else {
            1
        };
        replay
            .push(next, Event::released(4096, 4096, true).unwrap())
            .unwrap();
        let end = replay.snapshot().unwrap();
        assert_eq!(end.outstanding_fresh, 0);
        assert_eq!(end.advice.process[Totals::NOW], 0);
        assert_eq!(end.events, next + 1);
    }
    let mut below = Replay::new();
    below.push(0, fresh(4096, 2048, 1)).unwrap();
    assert_eq!(below.snapshot().unwrap().requests[0], [1, 0, 0]);
    assert!(below.push(1, accepted(4096, 1)).is_err());
    assert!(below.snapshot().is_err());
}

#[test]
fn advice_requires_exact_prior_fresh_geometry_mode_and_original_request() {
    let mut absent = Replay::new();
    assert!(absent.push(0, accepted(4096, 1)).is_err());
    assert!(absent.snapshot().is_err());
    for advice in [
        Event::accepted(4096, 8192, 4224, 2048, 1024, 512, 1).unwrap(),
        Event::accepted(4096, 4096, 4352, 2048, 1024, 512, 1).unwrap(),
        Event::accepted(4096, 4096, 4224, 2048, 512, 512, 1).unwrap(),
        accepted(4096, 2),
    ] {
        let mut replay = Replay::new();
        replay.push(0, fresh(4096, 512, 1)).unwrap();
        assert!(replay.push(1, advice).is_err());
        assert!(replay.snapshot().is_err());
    }
    let mut duplicate = Replay::new();
    duplicate.push(0, fresh(4096, 0, 1)).unwrap();
    duplicate.push(1, accepted(4096, 1)).unwrap();
    assert!(duplicate.push(2, accepted(4096, 1)).is_err());
    assert!(duplicate.snapshot().is_err());
}

#[test]
fn sequence_missing_or_duplicate_fresh_and_terminal_geometry_are_errors() {
    let mut gap = Replay::new();
    assert!(gap.push(1, fresh(4096, 0, 1)).is_err());
    assert!(
        gap.push(0, fresh(4096, 0, 1)).is_err(),
        "no repaired stream after a gap"
    );
    for bad in [
        fresh(4096, 0, 1),
        fresh(6144, 0, 1),
        Event::released(4096, 8192, true).unwrap(),
    ] {
        let mut replay = Replay::new();
        replay.push(0, fresh(4096, 0, 1)).unwrap();
        assert!(replay.push(1, bad).is_err());
        assert!(replay.snapshot().is_err());
    }
}

#[test]
fn unknown_release_and_partial_advice_never_yield_zero_or_repaired_rows() {
    for mode in 0..=2 {
        let mut replay = Replay::new();
        replay.push(0, fresh(4096, 0, mode)).unwrap();
        assert!(replay
            .push(1, Event::released(4096, 4096, false).unwrap())
            .is_err());
        assert!(replay.snapshot().is_err());
        assert!(replay
            .push(2, Event::released(4096, 4096, true).unwrap())
            .is_err());
    }
    let mut partial = Replay::new();
    partial.push(0, fresh(4096, 0, 1)).unwrap();
    assert!(partial
        .push(
            1,
            Event::unknown_advice(4096, 4096, 4224, 2048, 1024, 1024, 1).unwrap()
        )
        .is_err());
    assert!(partial.snapshot().is_err());
}

#[test]
fn unrelated_releases_preserve_owned_advice_and_confirmed_terminal_allows_key_reuse() {
    let mut replay = Replay::new();
    replay.push(0, fresh(4096, 0, 1)).unwrap();
    replay.push(1, accepted(4096, 1)).unwrap();
    replay
        .push(2, Event::released(1 << 20, 4096, true).unwrap())
        .unwrap();
    assert_eq!(
        replay.snapshot().unwrap().advice.process,
        [1024, 1024, 1024, 0]
    );
    assert_eq!(replay.snapshot().unwrap().outstanding_fresh, 1);
    replay
        .push(3, Event::released(4096, 4096, true).unwrap())
        .unwrap();
    replay.push(4, fresh(4096, 0, 1)).unwrap();
    replay.push(5, accepted(4096, 1)).unwrap();
    assert_eq!(
        replay.snapshot().unwrap().advice.process,
        [2048, 1024, 1024, 1024]
    );
    assert_eq!(replay.snapshot().unwrap().requests[0], [2, 2, 2]);
}

#[test]
fn unadvised_fresh_mappings_also_obey_the_bounded_capture_domain() {
    let mut replay = Replay::new();
    for index in 0..256 {
        replay
            .push(
                index,
                fresh((usize::try_from(index).unwrap() + 1) * 8192, 0, 0),
            )
            .unwrap();
    }
    assert_eq!(replay.snapshot().unwrap().outstanding_fresh, 256);
    assert!(replay.push(256, fresh(257 * 8192, 0, 0)).is_err());
    assert!(replay.snapshot().is_err());
}

fn capture(records: Vec<Event>) -> aligned_vmem::AdviceCaptureSnapshot {
    aligned_vmem::AdviceCaptureSnapshot {
        encoding_version: Event::ENCODING_VERSION,
        boundary: u64::try_from(records.len()).unwrap(),
        records,
        slot_storage_bytes: aligned_vmem::AdviceEventLog::<65536>::slot_storage_bytes(),
        total_storage_bytes: aligned_vmem::AdviceEventLog::<65536>::total_storage_bytes(),
        tls_context_bytes_per_thread:
            aligned_vmem::AdviceReleaseScope::scalar_storage_bytes_per_thread(),
    }
}

#[test]
fn confirmed_late_release_resolves_proven_native_address_reuse() {
    let rows = capture(vec![
        fresh(4096, 0, 1),
        accepted(4096, 1),
        fresh(4096, 0, 1),
        accepted(4096, 1),
        Event::large_released_at(4096, 4096, true, 2).unwrap(),
        Event::large_released_at(4096, 4096, true, 4).unwrap(),
    ]);
    let replay = Replay::from_capture(&rows)
        .expect("old munmap completed before mmap reuse; its callback arrived later");
    let end = replay.snapshot().unwrap();
    assert_eq!(end.advice.process, [2048, 0, 1024, 2048]);
    assert_eq!(end.outstanding_fresh, 0);
    assert_eq!(end.events, 6);
    assert_eq!(end.canonical_releases, 1);
    let with_untracked = capture(vec![
        fresh(4096, 0, 1),
        accepted(4096, 1),
        fresh(4096, 0, 1),
        accepted(4096, 1),
        Event::released_at(4096, 4096, true, 2).unwrap(),
        Event::large_released_at(4096, 4096, true, 2).unwrap(),
        Event::large_released_at(4096, 4096, true, 4).unwrap(),
    ]);
    let closed = Replay::from_capture(&with_untracked)
        .unwrap()
        .snapshot()
        .unwrap();
    assert_eq!(
        closed.advice, end.advice,
        "untracked VM callbacks cannot credit or ambiguate Large ownership"
    );
    assert_eq!(closed.canonical_releases, 1);
}

#[test]
fn uncertain_missing_and_ambiguous_markers_cannot_repair_overlapping_lifetimes() {
    for result in [
        Event::large_released_at(4096, 4096, false, 2).unwrap(),
        Event::large_released_at(4096, 4096, true, 0).unwrap(),
        Event::large_released_at(4096, 4096, true, 1).unwrap(),
        Event::large_released_at(4096, 4096, true, 3).unwrap(),
        Event::large_released_at(4096, 8192, true, 2).unwrap(),
        Event::large_released_at(4096, 4096, true, 6).unwrap(),
        Event::released(4096, 4096, true).unwrap(),
    ] {
        let rows = capture(vec![
            fresh(4096, 0, 1),
            accepted(4096, 1),
            fresh(4096, 0, 1),
            accepted(4096, 1),
            result,
        ]);
        assert!(Replay::from_capture(&rows).is_err());
    }
    let ambiguous = capture(vec![
        fresh(4096, 0, 1),
        accepted(4096, 1),
        fresh(4096, 0, 1),
        Event::large_released_at(4096, 4096, true, 2).unwrap(),
        Event::large_released_at(4096, 4096, true, 2).unwrap(),
    ]);
    assert!(Replay::from_capture(&ambiguous).is_err());
}

#[test]
fn delayed_untracked_vm_result_cannot_retire_a_newer_large_generation() {
    let prefix = capture(vec![
        fresh(4096, 0, 1),
        accepted(4096, 1),
        Event::large_released_at(4096, 4096, true, 0).unwrap(),
        Event::released_at(4096, 4096, true, 2).unwrap(),
        Event::released_at(4096, 4096, false, 2).unwrap(),
    ]);
    let replay = Replay::from_capture(&prefix).unwrap();
    assert_eq!(
        replay.snapshot().unwrap().advice.process,
        [1024, 1024, 1024, 0]
    );
    assert_eq!(replay.snapshot().unwrap().outstanding_fresh, 1);
    let mut end = prefix.records;
    end.push(Event::large_released_at(4096, 4096, true, 2).unwrap());
    assert_eq!(
        Replay::from_capture(&capture(end))
            .unwrap()
            .snapshot()
            .unwrap()
            .advice
            .process,
        [1024, 0, 1024, 1024]
    );
}
