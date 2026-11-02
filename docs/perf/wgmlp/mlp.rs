// WG-MLP pretest: random free-shaped accesses over a big table, singles vs batch-16 with prefetch.
use std::time::Instant;
use std::hint::black_box;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0};

#[inline(always)]
fn work(mut x: u64, n: u32) -> u64 { for _ in 0..n { x = x.wrapping_mul(0x9E3779B97F4A7C15).rotate_left(7) ^ 1; } x }

fn main() {
    let mb: usize = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(87);
    let w: u32 = std::env::args().nth(2).map(|s| s.parse().unwrap()).unwrap_or(10);
    let n = mb << 20;
    let mut table = vec![1u8; n];
    // pre-touch
    for i in (0..n).step_by(4096) { table[i] = 1; }
    let ops: usize = 4_000_000;
    // pseudo-random line-aligned indexes; two lines per free (state + block head)
    let mut s = 0x1234_5678_9abc_def0u64;
    let mut idx: Vec<(u32, u32)> = Vec::with_capacity(ops);
    for _ in 0..ops {
        s ^= s << 13; s ^= s >> 7; s ^= s << 17;
        let a = ((s >> 8) as usize % (n / 64)) * 64;
        let b = ((s >> 32) as usize % (n / 64)) * 64;
        idx.push((a as u32, b as u32));
    }
    let p = table.as_mut_ptr();
    let run_single = |p: *mut u8| -> (u64, f64) {
        let t = Instant::now();
        let mut acc = 0u64;
        for &(a, b) in &idx {
            unsafe {
                let st = *p.add(a as usize);
                if st != 0 { *p.add(b as usize) = st.wrapping_add(1); }
                acc = work(acc ^ st as u64, w);
            }
        }
        (acc, t.elapsed().as_nanos() as f64 / ops as f64)
    };
    let run_batch = |p: *mut u8, bn: usize| -> (u64, f64) {
        let t = Instant::now();
        let mut acc = 0u64;
        for ch in idx.chunks(bn) {
            #[cfg(target_arch = "x86_64")]
            for &(a, b) in ch { unsafe {
                _mm_prefetch(p.add(a as usize) as *const i8, _MM_HINT_T0);
                _mm_prefetch(p.add(b as usize) as *const i8, _MM_HINT_T0);
            } }
            for &(a, b) in ch { unsafe {
                let st = *p.add(a as usize);
                if st != 0 { *p.add(b as usize) = st.wrapping_add(1); }
                acc = work(acc ^ st as u64, w);
            } }
        }
        (acc, t.elapsed().as_nanos() as f64 / ops as f64)
    };
    // warm
    black_box(run_single(p));
    let mut rows = Vec::new();
    for round in 0..7 {
        let (_, s1) = run_single(p);
        let (_, b16) = run_batch(p, 16);
        let (_, b64) = run_batch(p, 64);
        let (_, s2) = run_single(p);
        rows.push((s1.min(s2), b16, b64));
        let _ = round;
    }
    let med = |f: fn(&(f64,f64,f64)) -> f64| { let mut v: Vec<f64> = rows.iter().map(f).collect(); v.sort_by(|a,b| a.partial_cmp(b).unwrap()); v[v.len()/2] };
    let s = med(|r| r.0); let b16 = med(|r| r.1); let b64 = med(|r| r.2);
    println!("mb={} work={} single={:.1}ns batch16={:.1}ns ({:.3}x) batch64={:.1}ns ({:.3}x)", mb, w, s, b16, b16/s, b64, b64/s);
}
