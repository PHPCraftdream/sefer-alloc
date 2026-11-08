fn note_group_counter(counters: &[AtomicU64; CONTROL_GROUP_COUNT], group: usize, amount: u64) {
    counters[group].fetch_add(amount, Ordering::Relaxed);
}
