// expect: not in the catalog table
impl ThpRuntime {
    fn next_control(request_size: usize) -> (bool, u64) {
        let ordinal = CONTROL_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let _retry_ordinal = CONTROL_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let selected = LargeThpControlSelector::decision_at(CONTROL_SEED, ordinal);
        (selected, ordinal)
    }
}
