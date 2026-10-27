// expect: no ghost_assert!/ghost_event! with id mag-flush/mag-resident
impl AllocCore {
    fn flush_run<const CHECKED: bool>(&mut self, class_idx: usize, base: *mut u8) -> (usize, bool) {
        let off = 0u32;
        if CHECKED {
            if (off as usize).wrapping_sub(payload_start) >= span {
                rejected += 1;
            }
        }
        (0, false)
    }
}
