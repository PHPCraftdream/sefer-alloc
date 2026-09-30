use sefer_alloc::{AllocCore, SegmentLayout};
use std::alloc::Layout;

fn main() {
    let mut core = AllocCore::new().unwrap();
    let layout = Layout::from_size_align(1, SegmentLayout::SEGMENT).unwrap();
    let allocation = core.alloc(layout);
    assert!(!allocation.is_null());
    let address = allocation.addr() + SegmentLayout::SEGMENT / 2;
    let address_only = std::ptr::without_provenance_mut::<u8>(address);
    println!("querying safe Small observer on Large numeric address");
    assert!(!core.dbg_is_free_for(address_only));
    println!("safe observer rejected Large geometry");
}
