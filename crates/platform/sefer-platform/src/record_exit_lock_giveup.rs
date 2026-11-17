use core::sync::atomic::Ordering;

#[cold]
#[doc(hidden)]
pub fn record_exit_lock_giveup() {
    super::exit_lock_giveups::EXIT_LOCK_GIVEUPS.fetch_add(1, Ordering::Relaxed);
}
