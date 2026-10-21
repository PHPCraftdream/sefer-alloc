// Multi-segment thrash probe (N9 miss-path judge): 64 windows x 16 blocks x 16 B,
// each window followed by a ~4 MiB live filler of 8 KiB blocks, so consecutive
// windows land in different segments. Churn: free a random block of a random
// window, then alloc (LIFO magazine hands the same block back). The free side
// therefore resolves a random one of >= 32 segments per op.
use core::alloc::{GlobalAlloc, Layout};
#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 { unsafe { a.alloc(l) } }
#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) { unsafe { a.dealloc(p, l) } }
const WINDOWS: usize = 64;
const PER_WIN: usize = 16;
const SZ: usize = 16;
const FILL_SZ: usize = 8192;
const FILL_PER_WIN: usize = 512; // 4 MiB
fn run<A: GlobalAlloc>(a: &A, steps: usize, check: bool) {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    let l = Layout::from_size_align(SZ, 8).unwrap();
    let lf = Layout::from_size_align(FILL_SZ, 8).unwrap();
    let mut slots: Vec<Vec<*mut u8>> = Vec::with_capacity(WINDOWS);
    let mut fill: Vec<*mut u8> = Vec::with_capacity(WINDOWS * FILL_PER_WIN);
    for _ in 0..WINDOWS {
        let mut w = Vec::with_capacity(PER_WIN);
        for _ in 0..PER_WIN { w.push(w_alloc(a, l)); }
        slots.push(w);
        for _ in 0..FILL_PER_WIN { fill.push(unsafe { a.alloc(lf) }); }
    }
    let mut segs: Vec<usize> = slots.iter().flatten().map(|&p| (p as usize) >> 22).collect();
    segs.sort_unstable();
    segs.dedup();
    assert!(!check || segs.len() >= 32, "activation: only {} distinct segments", segs.len());
    eprintln!("[thrash64] distinct 16B-window segments: {}", segs.len());
    for _ in 0..steps {
        let wi = (rnd() as usize) % WINDOWS;
        let si = (rnd() as usize) % PER_WIN;
        let p = slots[wi][si];
        w_free(a, p, l);
        slots[wi][si] = w_alloc(a, l);
    }
    for w in slots { for p in w { w_free(a, p, l); } }
    for p in fill { unsafe { a.dealloc(p, lf) } }
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    match args.get(1).map(String::as_str).unwrap_or("sefer") {
        "mimalloc" => run(&mimalloc::MiMalloc, steps, false),
        _ => { let s = sefer_alloc::SeferAlloc::new(); run(&s, steps, true) }
    }
}
