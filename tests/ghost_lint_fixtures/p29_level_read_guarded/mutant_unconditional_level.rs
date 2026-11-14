// expect: outside a branch matching
impl ThpRuntime {
    fn next_control(request_size: usize) -> (bool, u64) {
        let ordinal = CONTROL_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let candidate = LargeThpControlSelector::decision_at(CONTROL_SEED, ordinal);
        let group = group_index(request_size);
        let level = CADENCE[group].level();
        let selected = candidate && LargeThpCadence::live(CONTROL_SEED, ordinal, level);
        (selected, ordinal)
    }
}
