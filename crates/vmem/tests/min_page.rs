//! V13: MIN_PAGE alias tests.

use aligned_vmem::{MIN_PAGE, PAGE};

/// Test that MIN_PAGE is an exact alias for PAGE (V13: this is the
/// contract — they MUST stay identical, otherwise the alias is misleading).
#[test]
fn min_page_equals_page() {
    assert_eq!(MIN_PAGE, PAGE);
}

/// Test that MIN_PAGE has the expected value (4 KiB).
#[test]
fn min_page_is_4kib() {
    assert_eq!(MIN_PAGE, 1 << 12);
    assert_eq!(MIN_PAGE, 4096);
}
