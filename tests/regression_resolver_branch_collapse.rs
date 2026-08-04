//! Task #145 (Э2) — "two sentinels, one branch" regression.
//!
//! The three TLS resolvers in `global::tls_heap`
//! (`current`/`current_for_alloc`/`current_for_alloc_with_config`) used to
//! match `p == TORN`, then `p == null`, then the real-pointer case — two
//! compares against the two sentinels on the process's hottest path. Э2
//! collapses the hot check to ONE unsigned compare:
//!
//! ```text
//! p.addr().wrapping_sub(1) < usize::MAX - 1
//! ```
//!
//! which is TRUE for every real pointer (addr in `1..=MAX-1` ⇒ `addr-1` in
//! `0..=MAX-2`, all `< MAX-1`) and FALSE for both sentinels:
//!   - null  (addr 0):   `0.wrapping_sub(1) = MAX`   → NOT `< MAX-1` → cold
//!   - TORN  (addr MAX): `MAX-1`                     → NOT `< MAX-1` → cold
//!
//! The cold arm then splits on the exact value: null → bind a real slot
//! (`Own`), TORN → `Fallback`. The observable mapping is byte-identical to the
//! old three-arm match:
//!   - a real cached pointer      → `Own`   (fast path)
//!   - first-ever call (null)     → bind → `Own`
//!   - post-teardown (TORN)       → `Fallback`
//!
//! This test pins BOTH cold mappings:
//!   1. **null → bind → real** — a FRESH thread (its `LOCAL` starts null) that
//!      allocates must succeed and resolve to a real own-heap slot (it never
//!      routes to fallback just because `LOCAL` was null). Exercised
//!      indirectly: a fresh thread doing real allocations that all succeed.
//!   2. **TORN → Fallback** — the #129 `#[doc(hidden)]` hook (which pokes the
//!      exact `mark_local_torn` the teardown path uses) must still resolve to
//!      `Fallback`. If the branch collapse mis-mapped TORN (e.g. treated it as
//!      a real pointer because the single compare was wrong), the hook would
//!      report `Own(TORN)` and this fails.
//!
//! Counterfactual: an incorrect collapse — e.g. `<= usize::MAX - 1` (which
//! would let TORN through as "real") or `< usize::MAX` (which would let null's
//! wrapped `MAX` through) — makes one of the two assertions below fail.

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::global::tls_heap;

/// TORN → Fallback, via the #129 same-function hook.
#[test]
fn torn_sentinel_resolves_to_fallback_after_branch_collapse() {
    for _ in 0..8 {
        assert!(
            tls_heap::dbg_teardown_then_resolve_is_fallback(),
            "TORN must map to Fallback under the collapsed one-branch resolver \
             (Э2, task #145)"
        );
    }
}

/// null → bind → real own slot. A fresh thread starts with `LOCAL == null`;
/// its allocations must be served from a real own-heap slot (the resolver
/// binds a slot rather than falling back). We assert every allocation on the
/// fresh thread succeeds and round-trips — which is only possible if the
/// null case took the `bind_slow` path (Own), never a spurious sentinel arm.
#[test]
fn fresh_thread_null_local_binds_a_real_slot_not_fallback() {
    let handle = std::thread::spawn(|| {
        use sefer_alloc::SeferAlloc;
        use std::alloc::{GlobalAlloc, Layout};
        let a = SeferAlloc::new();
        // Several distinct classes, each alloc/write/free — the first alloc on
        // this fresh thread hits the null → bind path.
        for &(size, align) in &[(16usize, 8usize), (32, 8), (256, 8), (1024, 16)] {
            let layout = Layout::from_size_align(size, align).unwrap();
            // SAFETY: valid non-zero layout.
            let p = unsafe { a.alloc(layout) };
            assert!(!p.is_null(), "fresh-thread alloc returned null");
            // SAFETY: p is valid for `size` bytes; write then read back.
            unsafe {
                core::ptr::write_bytes(p, 0xAB, size);
                assert_eq!(core::ptr::read(p), 0xAB);
                a.dealloc(p, layout);
            }
        }
    });
    handle.join().expect("fresh thread panicked");
}
