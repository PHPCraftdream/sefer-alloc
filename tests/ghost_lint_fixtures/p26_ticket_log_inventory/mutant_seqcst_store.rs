// expect: not in the catalog table
fn record_control_ticket(ordinal: u64, selected: bool, level: u64) {
    let slot = CONTROL_TICKET_CURSOR.fetch_add(1, Ordering::Relaxed);
    if slot < TICKET_LOG_CAP {
        CONTROL_TICKET_LOG[slot]
            .store(ThpRuntime::ticket_log_pack(ordinal, selected), Ordering::SeqCst);
        CONTROL_LEVEL_LOG[slot].store(level, Ordering::Relaxed);
    }
}
