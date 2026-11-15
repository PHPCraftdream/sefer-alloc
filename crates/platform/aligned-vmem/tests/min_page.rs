//! V13: MIN_PAGE alias tests.

use aligned_vmem::MIN_PAGE;

/// Test that MIN_PAGE has the expected value (4 KiB).
#[test]
fn min_page_is_4kib() {
    assert_eq!(MIN_PAGE, 1 << 12);
    assert_eq!(MIN_PAGE, 4096);
}
