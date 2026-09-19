//! Lock-free tier of the concurrent family (Phase 3b-I, arc-swap RCU).
//!
//! `lock_free_handle` and `lock_free_region` are the handle and region pair
//! for the copy-on-write lock-free read tier. Wiring only — no logic lives
//! here.

pub(crate) mod lock_free_handle;
pub(crate) mod lock_free_region;
