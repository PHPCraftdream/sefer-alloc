#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::AllocCore;

#[test]
fn ordinary_standalone_core_allocates_frees_and_reissues_with_production_features() {
    let mut core = AllocCore::new().expect("standalone primordial reservation");
    let layouts = [1, 7, 64, 96, 1024].map(|size| Layout::from_size_align(size, 8).unwrap());
    for _ in 0..2 {
        let mut pointers = [std::ptr::null_mut(); 5];
        for (index, &layout) in layouts.iter().enumerate() {
            let ptr = core.alloc(layout);
            assert!(!ptr.is_null());
            assert_eq!(ptr.addr() % layout.align(), 0);
            for &other in &pointers[..index] {
                assert_ne!(
                    ptr, other,
                    "simultaneously issued allocations need distinct starts"
                );
            }
            let canary = 0xA0 + index as u8;
            // SAFETY: ptr is a current unique allocation, and both byte offsets
            // are within its nonzero requested Layout.
            unsafe {
                ptr.write(canary);
                ptr.add(layout.size() - 1).write(canary);
            }
            pointers[index] = ptr;
        }
        for (index, (&ptr, &layout)) in pointers.iter().zip(&layouts).enumerate() {
            let canary = 0xA0 + index as u8;
            // SAFETY: every block remains uniquely issued; verify its requested
            // bounds before transferring ownership exactly once to this core.
            unsafe {
                assert_eq!(ptr.read(), canary);
                assert_eq!(ptr.add(layout.size() - 1).read(), canary);
                core.dealloc(ptr, layout);
            }
        }
    }
}
