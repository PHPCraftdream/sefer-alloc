use super::exact_fatal::fatal;
use super::exact_shard::ExactShard;
use super::insert_outcome::InsertOutcome;
use core::sync::atomic::{AtomicU64, Ordering};

const SHARDS: usize = 64;

static TABLE: [ExactShard; SHARDS] = [const { ExactShard::new() }; SHARDS];
/// Incarnation counter: each registration gets a fresh non-zero generation.
static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);

/// Process-wide out-of-object descriptor table keyed by exact address.
pub(super) struct ExactTable;

impl ExactTable {
    fn shard(addr: usize) -> &'static ExactShard {
        &TABLE[(ExactShard::hash(addr) as usize) & (SHARDS - 1)]
    }

    /// Register a fresh object; `None` on table OOM. A live duplicate address
    /// is a protocol violation (a free published after its VA was reused).
    pub(super) fn register(addr: usize, size: usize, align: usize) -> Option<u64> {
        let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
        match Self::shard(addr).insert(addr, size, align, generation) {
            InsertOutcome::Inserted => Some(generation),
            InsertOutcome::Oom => None,
            InsertOutcome::Duplicate => {
                fatal(b"exact-object: duplicate live descriptor for address\n")
            }
        }
    }

    pub(super) fn peek(addr: usize) -> Option<(usize, usize, u64)> {
        Self::shard(addr).peek(addr)
    }

    pub(super) fn take(addr: usize) -> Option<(usize, usize, u64)> {
        Self::shard(addr).take(addr)
    }
}
