//! Common assertions over saved native observer records, without current global counters.
use super::advice_sample::AdviceSample;

pub(crate) fn assert_sample(sample: &AdviceSample) {
    assert_eq!(sample.unscoped_records_before, 0);
    assert_eq!(sample.capture_slot_bytes, 4 * 1024 * 1024);
    assert!(sample.capture_static_bytes > sample.capture_slot_bytes);
    assert_eq!(
        sample.tls_scalar_bytes_per_thread,
        2 * core::mem::size_of::<usize>()
    );
    assert_eq!(sample.window.before.events, sample.window.begin);
    assert_eq!(sample.window.after.events, sample.window.end);
    assert!(sample.window.begin <= sample.window.end);
    for (index, records) in [sample.window.begin, sample.window.end]
        .into_iter()
        .enumerate()
    {
        let records = usize::try_from(records).unwrap();
        assert!(
            sample.snapshot_capacity_bytes[index]
                >= records * core::mem::size_of::<aligned_vmem::AdviceEvent>()
        );
        assert!(
            sample.replay_index_capacity_bytes[index]
                >= records * (core::mem::size_of::<usize>() + core::mem::size_of::<bool>())
        );
        assert!(
            sample.observer_wall_ns[index]
                >= sample.capture_ns[index]
                    .checked_add(sample.replay_ns[index])
                    .unwrap()
        );
    }
    assert_eq!(
        sample.replay_state_type_bytes,
        core::mem::size_of::<aligned_vmem::AdviceReplay>()
    );
    assert_eq!(
        sample.replay_index_descriptor_bytes,
        core::mem::size_of::<Vec<usize>>() + core::mem::size_of::<Vec<bool>>()
    );
    assert!(sample.cost_scope.contains("not RSS"));
    assert!(sample.observed_interval_ns >= sample.observer_wall_ns.iter().sum::<u64>());
}
