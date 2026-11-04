// Scratch probe: cross-thread free of N same-class blocks issued by the main thread (foreign frees),
// the shape of benches/support/r19_prf01.rs producer window. Callgrind toggles on `producer`.
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

#[inline(never)]
fn producer(addrs: &[usize], layout: Layout) {
    for &a in addrs {
        let p = core::ptr::with_exposed_provenance_mut::<u8>(a);
        // SAFETY: live block issued with this layout, freed exactly once here.
        unsafe { GLOBAL.dealloc(p, layout) };
    }
}

fn main() {
    let layout = Layout::from_size_align(16, 16).unwrap();
    let n = 4096;
    let mut addrs = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: valid layout.
        let p = unsafe { GLOBAL.alloc(layout) };
        assert!(!p.is_null());
        addrs.push(p.expose_provenance());
    }
    std::thread::scope(|s| {
        s.spawn(|| producer(&addrs, layout)).join().unwrap();
    });
}
