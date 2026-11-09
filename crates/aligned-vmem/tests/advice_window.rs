//! Synthetic window accounting; native boundaries and observer cost need separate proof.
#![cfg(feature = "bench-internals")]
use aligned_vmem::{
    AdviceCaptureSnapshot, AdviceEvent as Event, AdviceEventLog, AdviceLifetimeSnapshot as Totals,
    AdviceReleaseScope, AdviceReplay as Replay,
};

fn fresh(key: usize) -> Event {
    Event::fresh(key, 4096, key + 128, 2048, 1024, 0, 1).unwrap()
}
fn advice(key: usize, bytes: usize) -> Event {
    Event::accepted(key, 4096, key + 128, 2048, 1024, bytes, 1).unwrap()
}
fn release(key: usize, begin: u64) -> Event {
    Event::large_released_at(key, 4096, true, begin).unwrap()
}
fn capture(records: Vec<Event>) -> AdviceCaptureSnapshot {
    AdviceCaptureSnapshot {
        encoding_version: Event::ENCODING_VERSION,
        boundary: u64::try_from(records.len()).unwrap(),
        records,
        slot_storage_bytes: AdviceEventLog::<65536>::slot_storage_bytes(),
        total_storage_bytes: AdviceEventLog::<65536>::total_storage_bytes(),
        tls_context_bytes_per_thread: AdviceReleaseScope::scalar_storage_bytes_per_thread(),
    }
}

#[test]
fn window_peak_includes_incoming_cached_advice_and_new_lifetimes() {
    let rows = capture(vec![
        fresh(4096),
        advice(4096, 1024),
        fresh(8192),
        advice(8192, 512),
        release(4096, 4),
        release(8192, 5),
    ]);
    let window = Replay::window(&rows, 2).unwrap();
    assert_eq!((window.begin, window.end), (2, 6));
    assert_eq!(window.before.advice.process[Totals::NOW], 1024);
    assert_eq!(window.peak_process, 1536);
    assert_eq!(window.peak_groups[0], 1536);
    assert_eq!(window.after.advice.process, [1536, 0, 1536, 1536]);
    assert_eq!(window.before.requests[0][0], 1);
    assert_eq!(window.after.requests[0][0], 2);
}

#[test]
fn historical_cumulative_peak_is_not_the_next_window_peak() {
    let rows = capture(vec![
        fresh(4096),
        advice(4096, 1024),
        release(4096, 2),
        fresh(8192),
        advice(8192, 512),
    ]);
    let window = Replay::window(&rows, 3).unwrap();
    assert_eq!(window.before.advice.process[Totals::NOW], 0);
    assert_eq!(window.after.advice.process[Totals::PEAK], 1024);
    assert_eq!(window.peak_process, 512);
    assert_eq!(window.peak_groups[0], 512);
}

#[test]
fn window_process_peak_is_not_the_sum_of_disjoint_group_peaks() {
    const MIB: usize = 1024 * 1024;
    let first = 4096;
    let second = 16 * MIB;
    let rows = capture(vec![
        Event::fresh(first, MIB + 4096, first + 128, MIB, MIB, 0, 1).unwrap(),
        Event::accepted(first, MIB + 4096, first + 128, MIB, MIB, 1024, 1).unwrap(),
        Event::large_released_at(first, MIB + 4096, true, 2).unwrap(),
        Event::fresh(second, 8 * MIB + 4096, second + 128, 8 * MIB, 8 * MIB, 0, 1).unwrap(),
        Event::accepted(
            second,
            8 * MIB + 4096,
            second + 128,
            8 * MIB,
            8 * MIB,
            2048,
            1,
        )
        .unwrap(),
        Event::large_released_at(second, 8 * MIB + 4096, true, 5).unwrap(),
    ]);
    let window = Replay::window(&rows, 0).unwrap();
    assert_eq!(window.peak_groups, [0, 1024, 2048, 0, 0, 0]);
    assert_eq!(window.peak_process, 2048);
    assert_ne!(window.peak_process, window.peak_groups.iter().sum());
}

#[test]
fn canonical_release_order_also_defines_window_accounting_peak() {
    let rows = capture(vec![
        fresh(4096),
        advice(4096, 1024),
        fresh(4096),
        advice(4096, 1024),
        release(4096, 2),
        release(4096, 4),
    ]);
    let window = Replay::window(&rows, 0).unwrap();
    assert_eq!(window.after.canonical_releases, 1);
    assert_eq!(window.peak_process, 1024);
    assert_eq!(window.after.advice.process, [2048, 0, 1024, 2048]);
}

#[test]
fn a_late_result_cannot_cross_a_claimed_quiescent_window_boundary() {
    let rows = capture(vec![
        fresh(4096),
        advice(4096, 1024),
        fresh(4096),
        advice(4096, 1024),
        release(4096, 2),
        release(4096, 4),
    ]);
    assert!(Replay::from_capture(&rows).is_ok());
    assert!(Replay::window(&rows, 3).is_err());
}

#[test]
fn empty_windows_preserve_incoming_ownership_and_bad_boundaries_reject() {
    let rows = capture(vec![fresh(4096), advice(4096, 1024)]);
    let window = Replay::window(&rows, 2).unwrap();
    assert_eq!(window.before, window.after);
    assert!(
        window.replay_index_capacity_bytes
            >= rows.records.len() * (core::mem::size_of::<usize>() + core::mem::size_of::<bool>())
    );
    assert_eq!(window.peak_process, 1024);
    assert_eq!(window.before.outstanding_fresh, 1);
    assert!(Replay::window(&rows, 3).is_err());
    let empty = Replay::window(&capture(Vec::new()), 0).unwrap();
    assert_eq!(empty.peak_process, 0);
    assert_eq!(empty.replay_index_capacity_bytes, 0);
    assert_eq!(empty.peak_groups, [0; 6]);
}

#[test]
fn unknown_history_never_becomes_a_readable_window_or_zero_row() {
    let rows = capture(vec![
        fresh(4096),
        advice(4096, 1024),
        Event::large_released_at(4096, 4096, false, 2).unwrap(),
    ]);
    assert!(Replay::window(&rows, 0).is_err());
    assert!(Replay::window(&rows, 3).is_err());
    let mut malformed = capture(Vec::new());
    malformed.boundary = 1;
    assert!(Replay::window(&malformed, 0).is_err());
}
