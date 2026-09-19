//! Epoch-reclamation tier of the concurrent family (Phase 3b-II).
//!
//! `hand` is the crate's single confined-`unsafe` organ (the `AtomicSlot<T>`
//! generation-CAS slot); `epoch_handle` and `epoch_region` are the
//! epoch-reclaimed handle and region built on top of it. Wiring only — no
//! logic lives here.

pub(crate) mod epoch_handle;
pub(crate) mod epoch_region;
pub(crate) mod hand;
