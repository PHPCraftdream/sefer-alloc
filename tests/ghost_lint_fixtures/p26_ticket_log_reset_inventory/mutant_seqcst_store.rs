// expect: not in the catalog table
impl ThpRuntime {
    fn ticket_log_reset() {
        CONTROL_TICKET_CURSOR.store(0, Ordering::SeqCst);
    }
}
