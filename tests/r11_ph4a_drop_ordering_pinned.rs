//! Ph4a (task #2091): source pin for the production `HeapLease::drop` ordering.
//!
//! `tests/loom_r11_ph4a_heap_lease.rs` is a shadow model (the real lease cannot
//! run under loom), so it cannot see a weakened production ordering. This test
//! pins the production site itself: the `LIVE → FREE` CAS in `HeapLease::drop`
//! must publish with `Ordering::Release` (S2 happens-before for the next
//! claimant's Acquire CAS). A mutant that weakens it to `Relaxed` turns this red.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

/// Source text with CRLF normalised (Windows checkouts convert line endings).
fn claim_src() -> &'static str {
    static CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        include_str!("../src/registry/heap_registry/claim.rs").replace("\r\n", "\n")
    })
    .as_str()
}

fn lease_drop_body() -> &'static str {
    let start = claim_src()
        .find("impl Drop for HeapLease")
        .expect("impl Drop for HeapLease present");
    let rest = &claim_src()[start..];
    let end = rest.find("\n}\n").expect("end of the Drop impl");
    &rest[..end]
}

#[test]
fn lease_drop_publishes_live_to_free_with_release() {
    let body = lease_drop_body();
    let cas = body.find("cas_state(").expect("Drop performs a state CAS");
    let call = &body[cas..];
    let close = call.find(')').expect("cas_state call is closed");
    let args: Vec<&str> = call[..close].split(',').map(str::trim).collect();
    assert_eq!(args.len(), 4, "cas_state(old, new, success, failure)");
    assert!(
        args[0].ends_with("STATE_LIVE"),
        "CAS expects LIVE: {args:?}"
    );
    assert_eq!(args[1], "STATE_FREE", "CAS publishes FREE: {args:?}");
    assert_eq!(
        args[2], "Ordering::Release",
        "the LIVE→FREE publication must be Release (S2): {args:?}"
    );
}

#[test]
fn lease_drop_aborts_on_a_lost_cas() {
    let body = lease_drop_body();
    assert!(
        body.contains("std::process::abort()"),
        "a lost LIVE→FREE CAS must abort (single-writer protocol broken)"
    );
}
