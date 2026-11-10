// expect: not preceded by the required operation
impl ThpRuntime {
    fn next_control(request_size: usize) -> (bool, u64) {
        record_control_ticket(0, false);
        let ordinal = CONTROL_ORDINAL.fetch_add(1, Ordering::Relaxed);
        let selected = LargeThpControlSelector::decision_at(CONTROL_SEED, ordinal);
        (selected, ordinal)
    }
}
