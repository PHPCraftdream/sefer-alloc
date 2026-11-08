// expect: not in the catalog table
impl ThpRuntime {
    fn control_ordinal_probe() -> u64 {
        CONTROL_ORDINAL.load(Ordering::SeqCst)
    }
}
