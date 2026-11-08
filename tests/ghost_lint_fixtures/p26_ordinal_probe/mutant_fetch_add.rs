// expect: not in the catalog table
impl ThpRuntime {
    fn control_ordinal_probe() -> u64 {
        CONTROL_ORDINAL.fetch_add(1, Ordering::Relaxed)
    }
}
