impl ThpRuntime {
    fn control_ordinal_probe() -> u64 {
        CONTROL_ORDINAL.load(Ordering::Relaxed)
    }
}
