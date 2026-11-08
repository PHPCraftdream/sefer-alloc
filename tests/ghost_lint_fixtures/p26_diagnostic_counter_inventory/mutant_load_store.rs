// expect: not in the catalog table
fn note_group_counter(counters: &[AtomicU64; CONTROL_GROUP_COUNT], group: usize, amount: u64) {
    let old = counters[group].load(Ordering::Relaxed);
    counters[group].store(old.wrapping_add(amount), Ordering::Relaxed);
}
