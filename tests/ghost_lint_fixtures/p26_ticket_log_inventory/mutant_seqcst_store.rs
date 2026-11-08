// expect: not in the catalog table
fn record_control_ticket(ordinal: u64, selected: bool) {
    let slot = CONTROL_TICKET_CURSOR.fetch_add(1, Ordering::Relaxed);
    if slot < TICKET_LOG_CAP {
        CONTROL_TICKET_LOG[slot].store(ordinal << 1 | selected as u64, Ordering::SeqCst);
    }
}
