// Batch probe, CONSTANT-Layout mode: same 256-round alloc then LIFO
// free-all as batch_probe.rs, but size fixed at 16 B with the Layout as a
// compile-time constant inside #[inline(never)] wrappers (w_alloc16/w_free16).
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

fn run_const<A: GlobalAlloc>(a: &A, steps: usize) {
    const N: usize = 256;
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut slots: Vec<*mut u8> = Vec::with_capacity(N);
    let mut rnd = || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    for _ in 0..steps {
        slots.clear();
        for _ in 0..N { slots.push(w_alloc16(a)); }
        while let Some(p) = slots.pop() { w_free16(a, p); }
        let _ = rnd();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let steps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
    let mode: String = args.get(3).cloned().unwrap_or_else(|| "16".to_string());
    match args.get(1).map(String::as_str).unwrap_or("sefer") {
        "mimalloc" => {
            let a = mimalloc::MiMalloc;
            if mode == "const" { run_const(&a, steps); } else { run(&a, steps, mode.parse().unwrap_or(16)); }
        }
        _ => {
            let s = sefer_alloc::SeferAlloc::new();
            if mode == "const" { run_const(&s, steps); } else { run(&s, steps, mode.parse().unwrap_or(16)); }
        }
    }
}
