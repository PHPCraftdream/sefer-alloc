// expect: rule requires ["AcqRel", "Acquire"]
impl HeapRegistry {
    pub(super) fn try_maintenance_at(index: usize) -> Option<MaintenanceLease> {
        if slot.state.load(Ordering::Relaxed) != SlotState::Free as u8 {
            return None;
        }
        if slot
            .cas_state(
                SlotState::Free,
                SlotState::Maintenance,
                Ordering::Release,
                Ordering::Acquire,
            )
            .is_err()
        {
            return None;
        }
        Some(MaintenanceLease { index: index as u32, slot })
    }
}

impl Drop for MaintenanceLease {
    fn drop(&mut self) {
        if std::thread::panicking() {
            fatal(b"claim: maintenance lease dropped while panicking\n");
        }
        if self
            .slot
            .cas_state(
                SlotState::Maintenance,
                SlotState::Free,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_err()
        {
            fatal(b"claim: maintenance lease free rollback cas lost\n");
        }
    }
}
