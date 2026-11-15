//! V12: VmemError to std::io::Error bridge tests.

use aligned_vmem::VmemError;
use std::io::{Error, ErrorKind};

/// Test that VmemError with an OS code maps to io::Error::from_raw_os_error.
#[test]
fn vmemerror_with_os_code_maps_to_raw_os_error() {
    // Simulate an OS error with a specific code (e.g., ENOMEM on Unix = 12)
    let error_with_code = VmemError::from_os_code(12);
    let io_error: Error = error_with_code.into();

    // The raw_os_error should be Some(12)
    assert_eq!(io_error.raw_os_error(), Some(12));
}

/// Test that VmemError::invalid_argument() maps to ErrorKind::InvalidInput.
#[test]
fn vmemerror_invalid_argument_maps_to_invalid_input() {
    let invalid_arg = VmemError::invalid_argument();
    let io_error: Error = invalid_arg.into();

    // Should be InvalidInput kind
    assert_eq!(io_error.kind(), ErrorKind::InvalidInput);
    // No OS code for contract violations
    assert_eq!(io_error.raw_os_error(), None);
    // The error message should mention the violation
    let error_msg = format!("{}", io_error);
    assert!(error_msg.contains("invalid argument"));
}

/// Test that VmemError::os_refusal_unknown_code() maps to ErrorKind::Other.
#[test]
fn vmemerror_unknown_code_maps_to_other() {
    let unknown_code = VmemError::os_refusal_unknown_code();
    let io_error: Error = unknown_code.into();

    // Should be Other kind (fallback for unknown OS errors)
    assert_eq!(io_error.kind(), ErrorKind::Other);
    // No OS code available
    assert_eq!(io_error.raw_os_error(), None);
    // The error message should mention unknown code
    let error_msg = format!("{}", io_error);
    assert!(error_msg.contains("unknown OS error code"));
}

/// Test that VmemError with an OS code above i32::MAX maps to io::Error::other
/// (the high-bit branch of the From<VmemError> for io::Error impl).
/// This tests the fix from task #946/G-2, which replaced an unchecked `as i32`
/// cast with i32::try_from to avoid truncating codes with the high bit set.
#[test]
fn vmemerror_high_bit_code_maps_to_other() {
    // 0x8007_000E is a HRESULT with the high bit set; it does not fit in i32.
    // The Windows HRESULT facility uses the high bit to indicate failure.
    let error_with_high_bit = VmemError::from_os_code(0x8007_000E);
    let io_error: Error = error_with_high_bit.into();

    // Should be Other kind (overflow path falls back to io::Error::other)
    assert_eq!(io_error.kind(), ErrorKind::Other);
    // No raw_os_error available (code doesn't fit in i32)
    assert_eq!(io_error.raw_os_error(), None);
    // The error message should mention the OS error code (preserved via other)
    let error_msg = format!("{}", io_error);
    assert!(error_msg.contains("OS virtual-memory error (code 2147942414)")); // 0x8007_000E = 2147942414
}
