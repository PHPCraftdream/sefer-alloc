// expect: not preceded by the required operation
impl ThpRuntime {
    fn next_control(request_size: usize) -> (bool, u64) {
        let ordinal = 0usize; // stale: the decision consumes no fresh ticket
        let selected = LargeThpControlSelector::decision_at(CONTROL_SEED, ordinal);
        let ticket = CONTROL_ORDINAL.fetch_add(1, Ordering::Relaxed);
        (selected, ticket)
    }
}
