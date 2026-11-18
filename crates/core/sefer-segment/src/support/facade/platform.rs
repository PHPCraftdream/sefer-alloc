// Root-compat aliases; consumed by `sefer-alloc`, not by this crate.
#![allow(unused_imports)]
#![allow(dead_code)]
pub(crate) mod fatal {
    pub(crate) use sefer_platform::fatal::fatal;
}
pub(crate) mod os {
    pub(crate) use sefer_platform::os::*;
    #[inline(always)]
    #[allow(unsafe_code)]
    pub(crate) fn release_segment(base: *mut u8, len: usize) {
        // SAFETY: the table only releases its verified, exclusively owned reservation.
        unsafe { sefer_platform::os::release_segment(base, len) }
    }
}
pub(crate) use sefer_platform::process_exit::process_exit_in_progress;
pub(crate) use sefer_platform::record_exit_lock_giveup::record_exit_lock_giveup;
