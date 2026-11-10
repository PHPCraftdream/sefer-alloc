// expect: not in the catalog table
impl ThpRuntime {
    fn ticket_log_snapshot() -> ([u64; TICKET_LOG_CAP], u64) {
        let mut entries = [0u64; TICKET_LOG_CAP];
        for (slot, entry) in CONTROL_TICKET_LOG.iter().zip(entries.iter_mut()) {
            *entry = slot.load(Ordering::Relaxed);
        }
        let cursor = CONTROL_TICKET_CURSOR
            .load(Ordering::SeqCst)
            .min(TICKET_LOG_CAP as u64);
        (entries, cursor)
    }
}
