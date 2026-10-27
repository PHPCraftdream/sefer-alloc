impl AllocCore {
    fn flush_run<const CHECKED: bool>(&mut self, class_idx: usize, base: *mut u8) -> (usize, bool) {
        let off = 0u32;
        if CHECKED {
            if (off as usize).wrapping_sub(payload_start) >= span {
                rejected += 1;
            }
        } else {
            crate::ghost::ghost_assert!(
                "mag-flush/mag-resident",
                (off as usize).wrapping_sub(payload_start) < span,
                "magazine flush of a block that is not MAG-resident in class {}",
                class_idx
            );
        }
        (0, false)
    }
}
