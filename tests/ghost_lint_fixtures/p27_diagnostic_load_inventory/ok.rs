impl ThpRuntime {
    pub(crate) fn learning_telemetry() -> [[u64; 5]; CONTROL_GROUP_COUNT] {
        let gradient = Self::current().gradient;
        let mut result = [[0; 5]; CONTROL_GROUP_COUNT];
        for group in 0..CONTROL_GROUP_COUNT {
            let estimator = LEARNERS[group].snapshot();
            result[group] = [
                u64::from(estimator.e_raw()),
                u64::from(estimator.m_raw()),
                u64::from(estimator.bounded_share(&gradient).raw()),
                LEARNING_APPLIED[group].load(Ordering::Relaxed),
                LEARNING_DROPPED[group].load(Ordering::Relaxed),
            ];
        }
        result
    }
}
