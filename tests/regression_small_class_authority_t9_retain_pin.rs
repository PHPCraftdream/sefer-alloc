//! B4 У1 (`small-class-authority`, ADR §8 T9) — retain pin: a ROUTED core +
//! `dbg_force_decommit_retain_for` ⇒ FATAL (abort). Retain would drop the
//! payload pages whose `ClassLeaves` start-granule bytes the free path treats
//! as authoritative, so under У1 the hook must never survive on a routed
//! core. The existing retain consumers (`alloc_zeroed_virgin_small_skip`,
//! `segment_state_reconciliation_oracle`) drive the hook on UNROUTED
//! `AllocCore::new()` cores and stay green (structurally: no route can exist
//! there).
//!
//! `fatal` aborts (no unwind), so the oracle is a subprocess, in the
//! `regression_freelist_next_validation` shape: the child prints ARMED after
//! setup; SURVIVED must never appear and the process must die on the abort.
//!
//! MUTANT ("fatal gate removed"): the child prints SURVIVED and exits 0 — RED.

#![cfg(all(
    feature = "production",
    feature = "small-class-authority",
    feature = "internals",
    feature = "bench-internals"
))]

use std::io::Write;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

const DIAGNOSTIC: &str = "small-class-authority: retain decommit of a routed segment is forbidden";

/// Subprocess child: routed heap + live Small block + retain hook.
#[test]
fn retain_pin_child() {
    let Ok(mode) = std::env::var("B4_T9_CASE") else {
        return;
    };
    assert_eq!(mode, "routed");
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = std::alloc::Layout::from_size_align(16, 16).unwrap();
    // SAFETY: valid layout on the calling thread's own heap slot.
    let p = heap.alloc(layout);
    assert!(!p.is_null());
    println!("ARMED");
    std::io::stdout().flush().unwrap();
    // SAFETY: `p` is a live Small allocation of this (routed) core; the
    // hook's live_count precondition is upheld trivially (live_count >= 1 is
    // the precondition shape the hook documents for its callers — the retain
    // resets an EMPTY segment; the expected outcome here is the У1 fatal
    // BEFORE any state is touched).
    let _expects_abort = unsafe { heap.dbg_force_decommit_retain_for(p) };
    println!("SURVIVED");
    std::io::stdout().flush().unwrap();
}

#[test]
fn routed_retain_is_fatal() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "retain_pin_child", "--nocapture"])
        .env("B4_T9_CASE", "routed")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("ARMED"), "child setup failed: {stdout} {stderr}");
    assert!(
        !stdout.contains("SURVIVED"),
        "retain hook SURVIVED on a routed core (mutant: fatal gate removed): {stdout}"
    );
    assert!(
        stderr.contains(DIAGNOSTIC),
        "missing У1 retain diagnostic: {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "abort must not be a panic: {stderr}"
    );
    assert_ne!(
        output.status.code(),
        Some(0),
        "routed retain must terminate the process: {stdout} {stderr}"
    );
}
