use core::sync::atomic::AtomicU64;

pub(super) static EXIT_LOCK_GIVEUPS: AtomicU64 = AtomicU64::new(0);
