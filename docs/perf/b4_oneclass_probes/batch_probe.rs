// Batch probe: rounds of 256 allocations of a fixed size, then free all 256
// in deterministic LIFO order, with the allocator calls behind
// #[inline(never)] wrappers so callgrind's inclusive cost of `w_alloc`/
// `w_free` is exactly the allocator's work.
use core::alloc::{GlobalAlloc, Layout};
#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 { unsafe { a.alloc(l) } }
#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) { unsafe { a.dealloc(p, l) } }
fn run<A: GlobalAlloc>(a: &A, steps: usize, size: usize) {
    const N: usize = 256;
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut slots: Vec<(*mut u8, Layout)> = Vec::with_capacity(N);
    let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    let l = Layout::from_size_align(size, 8).unwrap();
    for _ in 0..steps {
        slots.clear();
        for _ in 0..N { slots.push((w_alloc(a, l), l)); }
        // LIFO free order (deterministic)
        while let Some((p, l)) = slots.pop() { w_free(a, p, l); }
        let _ = rnd();
    }
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let size: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(16);
    match args.get(1).map(String::as_str).unwrap_or("sefer") {
        "mimalloc" => run(&mimalloc::MiMalloc, steps, size),
        _ => { let s = sefer_alloc::SeferAlloc::new(); run(&s, steps, size) }
    }
}
