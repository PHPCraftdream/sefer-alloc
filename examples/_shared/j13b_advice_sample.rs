//! Actual observer timing and logical storage, not an admitted runtime row.
use aligned_vmem::AdviceWindowSnapshot;

#[derive(Debug)]
pub(crate) struct AdviceSample {
    pub window: AdviceWindowSnapshot,
    pub unscoped_records_before: u64,
    pub capture_slot_bytes: usize,
    pub capture_static_bytes: usize,
    pub tls_scalar_bytes_per_thread: usize,
    pub snapshot_capacity_bytes: [usize; 2],
    pub replay_index_capacity_bytes: [usize; 2],
    pub replay_state_type_bytes: usize,
    pub replay_index_descriptor_bytes: usize,
    pub capture_ns: [u64; 2],
    pub replay_ns: [u64; 2],
    pub observer_wall_ns: [u64; 2],
    /// From inside observer begin to before record return; not whole child time.
    pub observed_interval_ns: u64,
    /// These clocks stop before record return; the outer window/child includes
    /// remaining bookkeeping. Byte capacities are not allocator/RSS/stack bounds.
    pub cost_scope: &'static str,
}
