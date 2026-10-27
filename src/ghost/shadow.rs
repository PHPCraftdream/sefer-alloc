//! Shadow table for ghost events: a fixed open-addressing map
//! `(rule, key) -> i64` on `core` atomics (no allocation — ghost code may run
//! inside the global allocator — and invisible to loom's model).

// Unused until the first production call site (G2) outside the internals probe.
#![allow(dead_code)]

use core::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};

const SLOTS: usize = 1024;

static KEYS: [AtomicU64; SLOTS] = [const { AtomicU64::new(0) }; SLOTS];
static VALS: [AtomicI64; SLOTS] = [const { AtomicI64::new(0) }; SLOTS];

/// Nonzero slot key for `(rule, key)`: FNV-1a of the rule id mixed with the key.
fn slot_key(rule: &str, key: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in rule.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }
    (h ^ key.wrapping_mul(0x9e37_79b9_7f4a_7c15)) | 1
}

/// Panics naming the failed ghost rule.
#[cold]
#[inline(never)]
#[track_caller]
pub fn fail(rule: &'static str, msg: core::fmt::Arguments<'_>) -> ! {
    panic!("ghost check {rule} failed: {msg}")
}

/// Adds `delta` to the counter of `(rule, key)`.
pub fn record(rule: &'static str, key: u64, delta: i64) {
    let k = slot_key(rule, key);
    let mut i = (k as usize) % SLOTS;
    for _ in 0..SLOTS {
        match KEYS[i].compare_exchange(0, k, Relaxed, Relaxed) {
            Ok(_) => {
                VALS[i].fetch_add(delta, Relaxed);
                return;
            }
            Err(cur) if cur == k => {
                VALS[i].fetch_add(delta, Relaxed);
                return;
            }
            Err(_) => i = (i + 1) % SLOTS,
        }
    }
    fail(
        rule,
        format_args!("ghost shadow table full ({SLOTS} slots)"),
    )
}

/// Current counter of `(rule, key)`; 0 if never recorded.
pub fn count(rule: &'static str, key: u64) -> i64 {
    let k = slot_key(rule, key);
    let mut i = (k as usize) % SLOTS;
    for _ in 0..SLOTS {
        match KEYS[i].load(Relaxed) {
            0 => return 0,
            cur if cur == k => return VALS[i].load(Relaxed),
            _ => i = (i + 1) % SLOTS,
        }
    }
    0
}

/// Clears the table (a harness calls this at the start of each model iteration).
pub fn reset() {
    for i in 0..SLOTS {
        VALS[i].store(0, Relaxed);
        KEYS[i].store(0, Relaxed);
    }
}
