//! R6-T29 (R6-CR-3) — a Large SHRINK whose realloc move-leg allocation fails
//! must NOT concede OOM: the block stays in place and the header records the
//! new effective size, returning the ORIGINAL pointer.
//!
//! Counterfactual: without the fix, the injected `alloc_large` failure makes
//! the move leg's `self.alloc(new_layout)` return null and `HeapCore::realloc`
//! propagates null — the test's `resized.addr() == ptr.addr()` assertion (and
//! the non-null check) goes red. The in-place success-path shrink stays
//! declined (perf item 83 NO-GO); only the OOM leg is fixed here.
//!
//! Scenario modelled on `r16_perf83_activation.rs`'s
//! `realloc_large_shrink_8_to_6mib` fixture: 8 MiB Large block, shrink to
//! 4 MiB.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::AllocCore;
use std::alloc::Layout;

const MIB: usize = 1024 * 1024;

/// Drop guard: the OOM-injection hook is process-global; a test that sets it
/// MUST clear it even on panic.
struct HookGuard;

impl Drop for HookGuard {
    fn drop(&mut self) {
        AllocCore::dbg_set_inject_large_alloc_oom(false);
    }
}

#[test]
fn large_shrink_move_oom_keeps_original_pointer() {
    let _ = bootstrap::ensure();
    let old_layout = Layout::from_size_align(8 * MIB, 8).expect("old layout");
    let new_layout = Layout::from_size_align(4 * MIB, 8).expect("new layout");
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let ptr = (*heap).alloc(old_layout);
    assert!(!ptr.is_null(), "8 MiB Large alloc");
    // SAFETY: ptr is exclusively owned, live and writable for old_layout.size().
    unsafe {
        core::ptr::write_bytes(ptr, 0x5A, old_layout.size());
    }

    AllocCore::dbg_set_inject_large_alloc_oom(true);
    let _guard = HookGuard;
    // SAFETY: ptr is live in this heap with precisely old_layout; the shrink
    // move leg allocates 4 MiB, which the hook forces to fail.
    let resized = unsafe { (*heap).realloc(ptr, old_layout, new_layout.size()) };
    assert!(
        !resized.is_null(),
        "R6-CR-3: Large shrink under move-leg OOM must keep the block"
    );
    assert_eq!(
        resized.addr(),
        ptr.addr(),
        "the original pointer must be returned unchanged"
    );
    drop(_guard);

    // The already-written 8 MiB prefix is intact: probe a few bytes of the
    // first 4 MiB (the only range the new layout guarantees live).
    // SAFETY: resized == ptr is a live allocation of new_layout.size() bytes.
    unsafe {
        for &probe in &[0usize, new_layout.size() / 2, new_layout.size() - 1] {
            assert_eq!(
                resized.add(probe).read(),
                0x5A,
                "byte {probe} lost across the OOM-shrink realloc"
            );
        }
    }
    // SAFETY: resized is a live allocation with the NEW layout in its owner
    // heap; freed exactly once here.
    unsafe { (*heap).dealloc(resized, new_layout) };
}
