use core::sync::atomic::Ordering;

#[cfg(feature = "bench-internals")]
#[doc(hidden)]
#[allow(missing_docs)]
pub fn dbg_exit_lock_giveups() -> u64 {
    super::exit_lock_giveups::EXIT_LOCK_GIVEUPS.load(Ordering::Relaxed)
}
