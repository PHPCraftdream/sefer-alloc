// B4 X-mix probe (worktree b4-judges): allocator-only larson-like churn with
// the EXACT larson size distribution (`pick_size` copied bit-for-bit from
// crates/malloc-bench-rs/src/lib.rs) — the only difference vs
// alloc_only_probe.rs (16..512 B uniform). Allocator calls stay behind
// #[inline(never)] wrappers so callgrind's inclusive cost of `w_alloc` /
// `w_free` is exactly the allocator's work.
use core::alloc::{GlobalAlloc, Layout};
#[inline(never)]
fn w_alloc<A: GlobalAlloc>(a: &A, l: Layout) -> *mut u8 { unsafe { a.alloc(l) } }
#[inline(never)]
fn w_free<A: GlobalAlloc>(a: &A, p: *mut u8, l: Layout) { unsafe { a.dealloc(p, l) } }

/// Deterministic, dependency-free PRNG (xorshift64*), copied bit-for-bit from
/// crates/malloc-bench-rs/src/lib.rs.
struct XorShift64(u64);
impl XorShift64 {
    fn new(seed: u64) -> Self { Self(seed | 1) }
    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// Exact copy of `pick_size` from crates/malloc-bench-rs/src/lib.rs
/// (~97% 16..512 B, ~3% 512 B..8 KiB).
#[inline]
fn pick_size(rng: &mut XorShift64) -> usize {
    let r = rng.next_u64();
    if r.is_multiple_of(32) {
        // ~3% large: 512 B .. 8 KiB.
        512 + (r >> 8) as usize % (8 * 1024 - 512)
    } else {
        // ~97% small: 16 .. 512 B.
        16 + (r >> 8) as usize % (512 - 16)
    }
}

fn run<A: GlobalAlloc>(a: &A, steps: usize) {
    const WS: usize = 768;
    let mut rng = XorShift64::new(0x9e37_79b9_7f4a_7c15);
    let mut slots: Vec<(*mut u8, Layout)> = Vec::with_capacity(WS);
    for _ in 0..WS {
        let l = Layout::from_size_align(pick_size(&mut rng), 8).unwrap();
        slots.push((w_alloc(a, l), l));
    }
    for _ in 0..steps {
        let r = rng.next_u64();
        let i = (r as usize) % WS;
        let (p, l) = slots[i];
        w_free(a, p, l);
        let nl = Layout::from_size_align(pick_size(&mut rng), 8).unwrap();
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
