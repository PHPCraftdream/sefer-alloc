#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

#[test]
fn safe_live_count_diagnostic_uses_stored_root_for_address_only_key() {
    let mut core = AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(64, 16).unwrap();
    let block = core.alloc(layout);
    assert!(!block.is_null());

    let key = core::ptr::without_provenance_mut::<u8>(block.addr());
    assert_eq!(core.dbg_live_count_for(key), Some(1));

    // SAFETY: `block` was issued by this core with `layout` and is freed once.
    unsafe { core.dealloc(block, layout) };
    assert_eq!(core.dbg_live_count_for(key), Some(0));
}
