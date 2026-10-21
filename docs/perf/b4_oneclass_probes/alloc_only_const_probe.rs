// Allocator-only cost probe, CONSTANT-Layout mode: same churn as
// alloc_only_probe.rs (768 live slots, larson-like), but the measured
// wrappers take no Layout parameter — `Layout::from_size_align(16, 8)` /
// `(64, 8)` is a compile-time constant inside each #[inline(never)] wrapper,
// so class_for/divides fold like in iai/Box<T> usage. The original
// runtime-Layout wrappers (w_alloc/w_free) and run() are kept untouched
// for control profiling (modes: rt = original, c16/c64 = const).
use core::alloc::{GlobalAlloc, Layout};
#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 { unsafe { a.alloc(l) } }
#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) { unsafe { a.dealloc(p, l) } }

#[inline(never)]
fn w_alloc16<A: GlobalAlloc>(a: &A) -> *mut u8 {
    let l = Layout::from_size_align(16, 8).unwrap();
    unsafe { a.alloc(l) }
}
#[inline(never)]
fn w_free16<A: GlobalAlloc>(a: &A, p: *mut u8) {
    let l = Layout::from_size_align(16, 8).unwrap();
    unsafe { a.dealloc(p, l) }
}
#[inline(never)]
fn w_alloc64<A: GlobalAlloc>(a: &A) -> *mut u8 {
    let l = Layout::from_size_align(64, 8).unwrap();
    unsafe { a.alloc(l) }
}
#[inline(never)]
fn w_free64<A: GlobalAlloc>(a: &A, p: *mut u8) {
    let l = Layout::from_size_align(64, 8).unwrap();
    unsafe { a.dealloc(p, l) }
}

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

// Same loop shape as run(), but fixed-size slots through the const wrappers.
fn run_const<A: GlobalAlloc, const SZ: usize>(a: &A, steps: usize) {
    const WS: usize = 768;
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut slots: Vec<*mut u8> = Vec::with_capacity(WS);
    let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    for _ in 0..WS {
        slots.push(if SZ == 16 { w_alloc16(a) } else { w_alloc64(a) });
    }
    for _ in 0..steps {
        let r = rnd();
        let i = (r as usize) % WS;
        let p = slots[i];
        if SZ == 16 { w_free16(a, p); } else { w_free64(a, p); }
        slots[i] = if SZ == 16 { w_alloc16(a) } else { w_alloc64(a) };
    }
    for p in slots { if SZ == 16 { w_free16(a, p); } else { w_free64(a, p); } }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    match args.get(1).map(String::as_str).unwrap_or("sefer") {
        "mimalloc" => match args.get(3).map(String::as_str).unwrap_or("rt") {
            "c16" => run_const::<mimalloc::MiMalloc, 16>(&mimalloc::MiMalloc, steps),
            "c64" => run_const::<mimalloc::MiMalloc, 64>(&mimalloc::MiMalloc, steps),
            _ => run(&mimalloc::MiMalloc, steps),
        },
        a => {
            let s = sefer_alloc::SeferAlloc::new();
            match args.get(3).map(String::as_str).unwrap_or("rt") {
                "c16" => run_const::<sefer_alloc::SeferAlloc, 16>(&s, steps),
                "c64" => run_const::<sefer_alloc::SeferAlloc, 64>(&s, steps),
                _ => run(&s, steps),
            }
            let _ = a;
        }
    }
}
