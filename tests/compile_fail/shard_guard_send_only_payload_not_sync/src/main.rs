//! R13-05 negative compile-fail oracle, compiled directly with rustc.
//! Includes the actual private shard_lock.rs by path; every statement is
//! valid except the final `Sync` requirement, which must fail with exactly
//! one E0277 while the `PhantomData<&'a mut T>` marker in shard_lock.rs
//! stands.

#[path = "../../../../src/registry/segment_route/shard_lock.rs"]
mod shard_lock;

use core::cell::Cell;
use shard_lock::ShardLock;

fn require_sync<T: Sync>(_: &T) {}

fn main() {
    let lock = ShardLock::new(Cell::new(0u32));
    let guard = lock.lock();
    let _ = guard.get();
    require_sync(&guard);
}
