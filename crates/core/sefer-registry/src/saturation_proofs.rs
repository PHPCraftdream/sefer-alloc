use super::SaturationHint;

#[kani::proof]
#[kani::unwind(5)]
fn stale_saturation_snapshot_never_marks() {
    let before: u64 = kani::any();
    let hint = SaturationHint::dbg_with_word_for_test(before);
    let stale = hint.snapshot();
    let publications: u8 = kani::any();
    kani::assume(publications >= 1 && publications <= 3);
    for _ in 0..publications {
        hint.publish_claimable();
    }
    let after = hint.snapshot();
    assert!(after == u64::MAX || (after > before && after & 1 == 0));
    if before == u64::MAX {
        assert_eq!(after, u64::MAX);
    }
    hint.try_mark_saturated(stale);
    assert_eq!(hint.snapshot(), after);
    assert!(!hint.is_saturated());
    kani::cover!(after != u64::MAX, "publication advances an enabled version");
}

#[kani::proof]
fn fresh_snapshot_marks() {
    let before: u64 = kani::any();
    // Reachable enabled snapshots are even and at most MAX-3.
    // MAX-1 is unreachable through publish_claimable: marking it would
    // produce DISABLED rather than saturation, intentionally excluded here.
    kani::assume(before & 1 == 0 && before <= u64::MAX - 3);
    let hint = SaturationHint::dbg_with_word_for_test(before);
    hint.try_mark_saturated(hint.snapshot());
    assert_eq!(hint.snapshot(), before | 1);
    assert!(hint.is_saturated());
    kani::cover!(
        hint.is_saturated(),
        "fresh reachable snapshot marks saturation"
    );
}
