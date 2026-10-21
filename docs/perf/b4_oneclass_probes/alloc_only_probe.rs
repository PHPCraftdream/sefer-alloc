// Allocator-only cost probe: larson-like single-thread churn with the
// allocator calls behind #[inline(never)] wrappers, so callgrind's inclusive
// cost of `w_alloc`/`w_free` is exactly the allocator's work.
use core::alloc::{GlobalAlloc, Layout};
#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 { unsafe { a.alloc(l) } }
#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) { unsafe { a.dealloc(p, l) } }
fn run<A: GlobalAlloc>(a: &A, steps: usize) {
    const WS: usize = 768;
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut slots: Vec<(*mut u8, Layout)> = Vec::with_capacity(WS);
    let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    for _ in 0..WS { let l = Layout::from_size_align(16 + (rnd() as usize % 497), 8).unwrap(); slots.push((w_alloc(a, l), l)); }
    for _ in 0..steps {
        let r = rnd();
        let i = (r as usize) % WS;
        let (p, l) = slots[i];
        w_free(a, p, l);
        let nl = Layout::from_size_align(16 + ((r >> 20) as usize % 497), 8).unwrap();
        slots[i] = (w_alloc(a, nl), nl);
    }
    for (p, l) in slots { w_free(a, p, l); }
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    match args.get(1).map(String::as_str).unwrap_or("sefer") {
        "mimalloc" => run(&mimalloc::MiMalloc, steps),
        _ => { let s = sefer_alloc::SeferAlloc::new(); run(&s, steps) }
    }
}
