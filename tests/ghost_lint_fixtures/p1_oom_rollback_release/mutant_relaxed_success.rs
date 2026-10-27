// expect: rule requires ["Release", "Relaxed"]
pub(super) fn push_back_after_oom(reg: &Registry, slot: &HeapSlot, idx: u32) {
    if slot
        .cas_state(
            SlotState::Initializing,
            SlotState::Free,
            Ordering::Relaxed,
            Ordering::Relaxed,
        )
        .is_err()
    {
        fatal(b"claim: oom rollback cas lost\n");
    }
    FREE_SLOTS.publish(idx as usize);
}
