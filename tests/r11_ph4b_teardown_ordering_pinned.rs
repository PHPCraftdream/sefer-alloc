//! Ph4b (task #2092, chunk B): source pin for the production teardown
//! ordering that the shadow model
//! `tests/loom_r11_ph4b_publish_recycle_drain.rs` mirrors but cannot observe.
//!
//! In `impl Drop for AbandonGuard` (`src/global/tls_heap.rs`) the order is
//! load-bearing:
//!   1. `mark_local_torn()` — poison LOCAL before the slot is released, so a
//!      post-teardown resolver can never alias the next claimant;
//!   2. `lease.core().trim_for_recycle()` — the terminal sidecar cut and owner
//!      cache trim happen while this thread is STILL the sole owner;
//!   3. `drop(lease)` — HeapLease::drop's `LIVE → FREE` CAS, which must
//!      publish with `Ordering::Release` (`src/registry/heap_registry/claim.rs`,
//!      the S2 happens-before edge the loom model transcribes).
//!
//! A mutant that reorders any of the three — e.g. dropping the lease before
//! the trim (mutations after release), or releasing before stamping TORN
//! (stale-pointer window) — turns this red.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

/// The Drop body with whole-line `//` comments removed: a commented-out call
/// (or prose that merely names one) must not satisfy the position checks.
/// Source text with CRLF normalised (Windows checkouts convert line endings).
fn tls_src() -> &'static str {
    static CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CELL.get_or_init(|| include_str!("../src/global/tls_heap.rs").replace("\r\n", "\n"))
        .as_str()
}

/// Source text with CRLF normalised (Windows checkouts convert line endings).
fn claim_src() -> &'static str {
    static CELL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CELL.get_or_init(|| {
        include_str!("../src/registry/heap_registry/claim.rs").replace("\r\n", "\n")
    })
    .as_str()
}

fn abandon_guard_drop_body() -> String {
    let start = tls_src()
        .find("impl Drop for AbandonGuard")
        .expect("AbandonGuard implements Drop");
    let rest = &tls_src()[start..];
    let end = rest
        .find("\n}\n")
        .expect("end of the AbandonGuard Drop impl");
    rest[..end]
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn teardown_stamps_torn_then_trims_then_releases_the_slot() {
    let body = abandon_guard_drop_body();
    let torn = body
        .find("mark_local_torn()")
        .expect("teardown poisons LOCAL via the shared mark_local_torn choke point");
    let trim = body
        .find("trim_for_recycle()")
        .expect("teardown trims the core while still the sole owner");
    let lease_drop = body
        .find("drop(lease);")
        .expect("teardown releases the slot exactly once via `drop(lease);`");
    assert!(
        torn < trim,
        "mark_local_torn() must precede trim_for_recycle(): \
         the TORN stamp must not lag owner-side mutation"
    );
    assert!(
        trim < lease_drop,
        "trim_for_recycle() must precede drop(lease): no core mutation is \
         allowed after the LIVE → FREE release hands the slot away"
    );
}

#[test]
fn lease_release_cas_is_release_ordered() {
    let start = claim_src()
        .find("impl Drop for HeapLease")
        .expect("HeapLease implements Drop");
    let rest = &claim_src()[start..];
    let end = rest.find("\n}\n").expect("end of the HeapLease Drop impl");
    let body = &rest[..end];
    let cas = body
        .find("cas_state(")
        .expect("Drop performs the state CAS");
    let call = &body[cas..];
    let close = call.find(')').expect("cas_state call is closed");
    let args: Vec<&str> = call[..close].split(',').map(str::trim).collect();
    assert_eq!(args.len(), 4, "cas_state(old, new, success, failure)");
    assert!(
        args[0].ends_with("STATE_LIVE") && args[1] == "STATE_FREE",
        "the teardown CAS must be LIVE → FREE: {args:?}"
    );
    assert_eq!(
        args[2], "Ordering::Release",
        "the LIVE → FREE publication must be Release (S2 happens-before for \
         the next claimant's Acquire CAS): {args:?}"
    );
}
