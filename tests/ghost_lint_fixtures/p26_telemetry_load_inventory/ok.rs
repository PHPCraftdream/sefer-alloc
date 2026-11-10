// expect: not in the catalog table
impl ThpRuntime {
    fn control_telemetry() -> [[u64; 8]; CONTROL_GROUP_COUNT] {
        let mut result = [[0; 8]; CONTROL_GROUP_COUNT];
        for group in 0..CONTROL_GROUP_COUNT {
            result[group] = [
                CONTROL_SELECTIONS[group].load(Ordering::Relaxed),
                CONTROL_PREMISE_FAILURES[group].load(Ordering::Relaxed),
                CONTROL_VALID_OBSERVATIONS[group].load(Ordering::Relaxed),
                CONTROL_INVALIDATED_OBSERVATIONS[group].load(Ordering::Relaxed),
                CONTROL_INVALIDATED_REALLOC[group].load(Ordering::Relaxed),
                CONTROL_OBSERVATION_ERRORS[group].load(Ordering::Relaxed),
                CONTROL_RESIDENT_PAGES[group].load(Ordering::Relaxed),
                CONTROL_SAMPLED_PAGES[group].load(Ordering::Relaxed),
            ];
        }
        result
    }
}
