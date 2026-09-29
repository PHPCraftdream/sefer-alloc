use core::alloc::Layout;

use super::{os, AllocCore, SegmentMeta};

#[test]
fn real_pending_reclaim_consumes_before_unregister() {
    let mut core = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    let base = core
        .table
        .canonical_base_of(os::segment_base_of_ptr(ptr))
        .expect("registered Large");
    let meta = SegmentMeta::new(base);
    assert!(meta.publish_large_pending(1));
    // Publication transferred this allocation; no producer access follows.
    core.reclaim_large_segment(base);
    assert!(!core.table.contains_base(base));
}
