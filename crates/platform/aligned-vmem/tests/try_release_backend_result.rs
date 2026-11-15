//! The backend-result surrogate skips native release and reaches the checked
//! backend's result classification with a no-code error. It proves propagation
//! only, not operating-system refusal. Native-attempt counters remain unchanged.

#![cfg(feature = "fault-injection")]

use aligned_vmem::fault_injection::{arm_fail_next_release, arm_fail_next_release_backend_result};
use aligned_vmem::{reserve_aligned, try_release, PAGE};
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn release_geometry_boundaries_match_layout_contract() {
    use aligned_vmem::release_size_align_is_valid_for_test;
    let accepted = isize::MAX as usize - (PAGE - 1);
    assert!(accepted.is_multiple_of(PAGE));
    assert!(release_size_align_is_valid_for_test(accepted, PAGE));
    assert!(!release_size_align_is_valid_for_test(0, PAGE));
    assert!(!release_size_align_is_valid_for_test(accepted - 1, PAGE));
    assert!(!release_size_align_is_valid_for_test(accepted + 1, PAGE));
    assert!(!release_size_align_is_valid_for_test(accepted + PAGE, PAGE));
    assert!(!release_size_align_is_valid_for_test(accepted, PAGE * 2));
    assert!(!release_size_align_is_valid_for_test(accepted, PAGE * 3));
    assert!(!release_size_align_is_valid_for_test(accepted, PAGE / 2));
    assert!(!release_size_align_is_valid_for_test(usize::MAX, PAGE));
}

#[test]
fn backend_result_surrogate_classifies_raw_refusal_without_releasing() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next_release(0);
    arm_fail_next_release_backend_result(0);
    let reservation = reserve_aligned(4 * PAGE, PAGE).expect("reserve");
    let (ptr, len, align) = reservation.into_parts();
    let before = backend_release_attempts();
    arm_fail_next_release_backend_result(1);
    // SAFETY: valid live reservation; surrogate skips release; single attempt,
    // never retried after Err, and no owning object remains.
    let err = unsafe { try_release(ptr, len, align) }.expect_err("surrogate refusal");
    assert!(err.os_code().is_none());
    assert!(!err.is_invalid_argument());
    assert_eq!(backend_release_attempts() - before, 0);
}

#[test]
fn backend_result_surrogate_covers_typed_and_safe_entry_points() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    arm_fail_next_release(0);
    arm_fail_next_release_backend_result(0);
    let reservation = reserve_aligned(2 * PAGE, PAGE).expect("reserve typed");
    let parts = reservation.into_reservation_parts();
    let before_typed = backend_release_attempts();
    arm_fail_next_release_backend_result(1);
    // SAFETY: parts came from a live reservation, consumed once; Err is never
    // retried and no live owning object is retained.
    let typed_err =
        unsafe { aligned_vmem::try_release_parts(parts) }.expect_err("typed surrogate refusal");
    assert!(typed_err.os_code().is_none());

    assert_eq!(backend_release_attempts() - before_typed, 0);

    let reservation = reserve_aligned(2 * PAGE, PAGE).expect("reserve safe");
    let before_safe = backend_release_attempts();
    arm_fail_next_release_backend_result(1);
    let safe_err = reservation
        .try_release()
        .expect_err("safe-method surrogate refusal");
    assert!(safe_err.os_code().is_none());
    assert_eq!(backend_release_attempts() - before_safe, 0);
}

#[cfg(all(unix, not(miri), feature = "bench-internals"))]
fn backend_release_attempts() -> u64 {
    aligned_vmem::unix_munmap_attempts()
}
#[cfg(all(windows, not(miri), feature = "bench-internals"))]
fn backend_release_attempts() -> u64 {
    aligned_vmem::windows_virtualfree_release_attempts()
}
#[cfg(not(any(
    all(unix, not(miri), feature = "bench-internals"),
    all(windows, not(miri), feature = "bench-internals")
)))]
fn backend_release_attempts() -> u64 {
    0
}
