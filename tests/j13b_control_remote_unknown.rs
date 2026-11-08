//! J13b control telemetry: remote consumption of a live control records the
//! UNKNOWN/invalidated remote-reclaim bucket, not a valid observation. One
//! scenario per binary.

#![cfg(all(
    any(target_os = "linux", target_os = "android"),
    not(miri),
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeThpMode, SeferAlloc};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const MIB: usize = 1024 * 1024;
const BLOCK: usize = 16 * MIB;
const COHORT: usize = 8;
const GROUP_8_TO_32_MIB: usize = 2;

struct SendPtr(*mut u8);
// SAFETY: ownership crosses the channel once; only the receiver frees it.
unsafe impl Send for SendPtr {}

#[test]
fn remote_free_records_unknown_and_never_a_valid_observation() {
    // Bootstrap the process tuple through an explicit config, then drop it.
    drop(
        AllocCore::new_with_config(LargeCacheConfig::new().huge_page_mode(LargeThpMode::Auto))
            .expect("primordial bootstrap"),
    );

    let layout = Layout::from_size_align(BLOCK, 8).expect("valid 16 MiB layout");
    let mut blocks = [core::ptr::null_mut(); COHORT];
    for block in &mut blocks {
        // SAFETY: valid non-zero layout; the returned pointer is retained.
        *block = unsafe { GLOBAL.alloc(layout) };
        assert!(!block.is_null(), "16 MiB allocation must succeed");
        // Deliberately no payload writes: the control stays fresh.
    }
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB][0],
        1,
        "the fresh cohort selects one control"
    );

    let learning_before = AllocCore::dbg_thp_learning_telemetry();
    let (tx, rx) = mpsc::sync_channel::<SendPtr>(1);
    let (ack_tx, ack_rx) = mpsc::sync_channel::<()>(1);
    let reaper = std::thread::spawn(move || {
        for SendPtr(ptr) in rx {
            // SAFETY: freed exactly once with the allocation layout.
            unsafe { GLOBAL.dealloc(ptr, layout) };
            let _ = ack_tx.send(());
        }
    });
    for block in blocks {
        tx.send(SendPtr(block)).unwrap();
        ack_rx.recv().unwrap();
    }
    drop(tx);
    reaper.join().unwrap();

    // Against a foreign free the protocol only guarantees: selections 1,
    // premise failures 0, valid observations 0, realloc invalidations 0,
    // errors 0, resident 0, sampled 0. The foreign publication never
    // mutates the owner-only token and never samples. Field 3 (UNKNOWN
    // remote-reclaim invalidations) is intentionally unconstrained here —
    // only the explicit owner trim establishes it.
    let control = AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB];
    assert_eq!(
        control[0], 1,
        "the fresh cohort selected exactly one control"
    );
    assert_eq!(control[1], 0, "no premise failures against a foreign free");
    assert_eq!(
        control[2], 0,
        "a foreign free must never produce a valid observation"
    );
    assert_eq!(control[4], 0, "no realloc invalidations");
    assert_eq!(control[5], 0, "no errors on the foreign-free path");
    assert_eq!(
        control[6], 0,
        "a foreign free must not report resident pages"
    );
    assert_eq!(
        control[7], 0,
        "a foreign free must not sample payload pages"
    );

    // The single explicit owner boundary: the guaranteed owner drain
    // consumed the pending control as UNKNOWN exactly once — never as a
    // valid observation, never with resident/sampled pages.
    GLOBAL.dbg_trim_current_thread();
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 1, 0, 0, 0, 0],
        "owner drain records the remote consumption as UNKNOWN exactly once"
    );

    // Exactly-once consumption oracle: the token is already consumed; a
    // second drain must not recount it.
    GLOBAL.dbg_trim_current_thread();
    assert_eq!(
        AllocCore::dbg_thp_control_telemetry()[GROUP_8_TO_32_MIB],
        [1, 0, 0, 1, 0, 0, 0, 0],
        "a second owner drain must not recount the consumed control"
    );
    assert_eq!(
        AllocCore::dbg_thp_learning_telemetry(),
        learning_before,
        "remote UNKNOWN must never train the owner estimator"
    );
}
