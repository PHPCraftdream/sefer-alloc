//! J13b C4: classification of range-advice results into charge handling.
#![cfg(all(
    feature = "internals",
    feature = "alloc-decommit",
    not(feature = "numa-aware")
))]

use aligned_vmem::VmemError;
use sefer_alloc::alloc_core::policy::large_thp_runtime::{ThpAdvice, ThpAdviceOutcome};

#[test]
fn ok_is_accepted() {
    assert_eq!(ThpAdvice::classify(Ok(0)), ThpAdviceOutcome::Accepted(0));
    assert_eq!(
        ThpAdvice::classify(Ok(4 << 20)),
        ThpAdviceOutcome::Accepted(4 << 20)
    );
}

#[test]
fn invalid_argument_is_pre_syscall() {
    assert_eq!(
        ThpAdvice::classify(Err(VmemError::invalid_argument())),
        ThpAdviceOutcome::PreSyscall
    );
}

#[test]
fn unknown_code_refusal_is_pre_syscall() {
    let e = VmemError::os_refusal_unknown_code();
    assert!(e.os_code().is_none());
    assert_eq!(ThpAdvice::classify(Err(e)), ThpAdviceOutcome::PreSyscall);
}

#[test]
fn os_codes_are_unknown_partial() {
    for code in [1u32, 2, 12, 22, 38, 95, 1455] {
        let e = VmemError::from_os_code(code);
        assert_eq!(e.os_code(), Some(code));
        assert!(!e.is_invalid_argument());
        assert_eq!(
            ThpAdvice::classify(Err(e)),
            ThpAdviceOutcome::UnknownPartial,
            "code {code}"
        );
    }
}
