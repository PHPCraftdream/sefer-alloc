#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::SeferAlloc;

#[test]
fn fallback_lock_preserves_tight_spin_boundary() {
    assert_eq!(SeferAlloc::dbg_fallback_lock_spin_transition(0), (1, true));
    assert_eq!(
        SeferAlloc::dbg_fallback_lock_spin_transition(63),
        (64, true)
    );
    assert_eq!(
        SeferAlloc::dbg_fallback_lock_spin_transition(64),
        (64, false)
    );

    let mut spins = 0;
    for _ in 0..64 {
        let (next, tight_spin) = SeferAlloc::dbg_fallback_lock_spin_transition(spins);
        assert!(tight_spin);
        spins = next;
    }
    for _ in 0..64 {
        let (next, tight_spin) = SeferAlloc::dbg_fallback_lock_spin_transition(spins);
        assert!(!tight_spin);
        assert_eq!(next, 64);
        spins = next;
    }
}

#[test]
fn fallback_lock_max_counter_yields_without_overflow() {
    assert_eq!(
        SeferAlloc::dbg_fallback_lock_spin_transition(u32::MAX),
        (u32::MAX, false)
    );
}
