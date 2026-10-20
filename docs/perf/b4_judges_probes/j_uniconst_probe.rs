// Unified runtime/const probe (one binary): alloc-only churn, 768 live slots,
// 16..512 B. Contains BOTH the runtime-Layout wrappers (w_alloc/w_free, layout
// passed in, black_boxed at the churn site) and the const-Layout wrappers
// (w_alloc16/w_free16/w_alloc64/w_free64, layout folded inside each wrapper).
// All wrappers #[inline(never)] so callgrind --toggle-collect measures exactly
// the wrapper cost per wrapper, in one binary, in one run.
use core::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;

#[inline(never)]
fn w_alloc_rt<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 { unsafe { a.alloc(l) } }
#[inline(never)]
fn w_free_rt<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) {
    unsafe { a.dealloc(p, l) };
    black_box(l.size()); // use the layout so identical free bodies are not ICF-folded (mimalloc ignores layout)
}

#[inline(never)]
fn w_alloc16<A: GlobalAlloc>(a: &A) -> *mut u8 {
    let l = Layout::from_size_align(16, 8).unwrap();
    unsafe { a.alloc(l) }
}
#[inline(never)]
fn w_free16<A: GlobalAlloc>(a: &A, p: *mut u8) {
    let l = Layout::from_size_align(16, 8).unwrap();
    unsafe { a.dealloc(p, l) };
    black_box(l.size());
}
#[inline(never)]
fn w_alloc64<A: GlobalAlloc>(a: &A) -> *mut u8 {
    let l = Layout::from_size_align(64, 8).unwrap();
    unsafe { a.alloc(l) }
}
#[inline(never)]
fn w_free64<A: GlobalAlloc>(a: &A, p: *mut u8) {
    let l = Layout::from_size_align(64, 8).unwrap();
    unsafe { a.dealloc(p, l) };
    black_box(l.size());
}

const WS: usize = 768;

// Runtime-layout churn (identical to alloc_only_probe.rs).
fn run_rt<A: GlobalAlloc>(a: &A, steps: usize) {
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut slots: Vec<(*mut u8, Layout)> = Vec::with_capacity(WS);
    let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    for _ in 0..WS { let l = Layout::from_size_align(16 + (rnd() as usize % 497), 8).unwrap(); slots.push((w_alloc_rt(a, l), l)); }
    for _ in 0..steps {
        let r = rnd();
        let i = (r as usize) % WS;
        let (p, l) = slots[i];
        w_free_rt(a, p, l);
        let nl = black_box(Layout::from_size_align(16 + ((r >> 20) as usize % 497), 8).unwrap());
        slots[i] = (w_alloc_rt(a, nl), nl);
    }
    for (p, l) in slots { w_free_rt(a, p, l); }
}

// Const-layout churn, 16-slot window (identical to const_probe.rs churn).
fn churn<A: GlobalAlloc, F: Fn(&A) -> *mut u8, G: Fn(&A, *mut u8)>(a: &A, steps: usize, w_a: F, w_f: G) {
    const N: usize = 16;
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    let mut slots: [*mut u8; N] = [core::ptr::null_mut(); N];
    for s in slots.iter_mut() { *s = w_a(a); }
    for _ in 0..steps {
        let i = (rnd() as usize) % N;
        let p = slots[i];
        w_f(a, p);
        slots[i] = w_a(a);
    }
    for s in slots { w_f(a, s); }
}

fn run<A: GlobalAlloc>(a: &A, steps: usize) {
    run_rt(a, steps);
    churn(a, steps, w_alloc16::<A>, w_free16::<A>);
    churn(a, steps, w_alloc64::<A>, w_free64::<A>);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    match args.get(1).map(String::as_str).unwrap_or("sefer") {
        "mimalloc" => run(&mimalloc::MiMalloc, steps),
        _ => { let s = sefer_alloc::SeferAlloc::new(); run(&s, steps) }
    }
}
