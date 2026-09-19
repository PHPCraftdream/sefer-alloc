//! Sharded tier of the concurrent family (Phase 3b-III).
//!
//! `sharded_handle` mints per-shard epoch-tier handles; `sharded_region`
//! routes operations across the shards built on them. Wiring only — no logic
//! lives here.

pub(crate) mod sharded_handle;
pub(crate) mod sharded_region;
