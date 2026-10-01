//! PG-1 W0: MODEL-LIMIT probe — tcache reuse of a Box's bytes while the
//! by-value frame that freed it is still live, vs after the frame returns,
//! vs a same-frame control. Installed `SeferAlloc`, genuine `Box<T>`,
//! harness-free, one scenario per process, selector = first argument.

#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();

const PATTERN: u8 = 0xA5;
/// Intra-round alloc/free repetitions: give the allocator chances to hand
/// back the same bytes so an address match is a real reuse witness.
const INNER: usize = 4;

#[inline(never)]
fn consume_then_alloc<const N: usize>(b: Box<[u8; N]>) -> (usize, usize) {
    let b_addr = std::ptr::from_ref::<[u8; N]>(&*b) as usize;
    drop(b);
    // `b`'s by-value frame is still live here; the new Box asks the installed
    // global allocator while that frame has not returned yet.
    let mut c: Box<[u8; N]> = Box::new([PATTERN; N]);
    let c_addr = std::ptr::from_ref::<[u8; N]>(&*c) as usize;
    // Write + read through `c` (Miri checks these accesses against `b`'s tag).
    let c_mut: &mut [u8; N] = &mut c;
    for slot in c_mut.iter_mut() {
        *slot ^= 0x5A;
    }
    let sum: u8 = c.iter().fold(0u8, |acc, v| acc.wrapping_add(*v));
    assert_eq!(sum, (PATTERN ^ 0x5A).wrapping_mul(N as u8));
    (b_addr, c_addr)
}

#[inline(never)]
fn consume_only<const N: usize>(b: Box<[u8; N]>) -> usize {
    let b_addr = std::ptr::from_ref::<[u8; N]>(&*b) as usize;
    drop(b);
    b_addr
}

fn run_w0<const N: usize>() -> (usize, usize) {
    let mut matches = 0;
    let mut total = 0;
    for round in 0..2u8 {
        for _ in 0..INNER {
            let b: Box<[u8; N]> = Box::new([0x30 + round; N]);
            let (b_addr, c_addr) = consume_then_alloc(b);
            if b_addr == c_addr {
                matches += 1;
            }
            total += 1;
        }
    }
    (matches, total)
}

fn run_w0_after<const N: usize>() -> (usize, usize) {
    let mut matches = 0;
    let mut total = 0;
    for round in 0..2u8 {
        for _ in 0..INNER {
            let b: Box<[u8; N]> = Box::new([0x40 + round; N]);
            let b_addr = consume_only(b);
            // The by-value frame of `consume_only` has fully returned here.
            let mut c: Box<[u8; N]> = Box::new([PATTERN; N]);
            let c_addr = std::ptr::from_ref::<[u8; N]>(&*c) as usize;
            let c_mut: &mut [u8; N] = &mut c;
            for slot in c_mut.iter_mut() {
                *slot ^= 0x5A;
            }
            let sum: u8 = c.iter().fold(0u8, |acc, v| acc.wrapping_add(*v));
            assert_eq!(sum, (PATTERN ^ 0x5A).wrapping_mul(N as u8));
            if b_addr == c_addr {
                matches += 1;
            }
            total += 1;
        }
    }
    (matches, total)
}

fn run_w0_same_frame<const N: usize>() -> (usize, usize) {
    let mut matches = 0;
    let mut total = 0;
    for round in 0..2u8 {
        for _ in 0..INNER {
            let b: Box<[u8; N]> = Box::new([0x50 + round; N]);
            let b_addr = std::ptr::from_ref::<[u8; N]>(&*b) as usize;
            drop(b);
            // No by-value argument frame at all: same-frame control.
            let mut c: Box<[u8; N]> = Box::new([PATTERN; N]);
            let c_addr = std::ptr::from_ref::<[u8; N]>(&*c) as usize;
            let c_mut: &mut [u8; N] = &mut c;
            for slot in c_mut.iter_mut() {
                *slot ^= 0x5A;
            }
            let sum: u8 = c.iter().fold(0u8, |acc, v| acc.wrapping_add(*v));
            assert_eq!(sum, (PATTERN ^ 0x5A).wrapping_mul(N as u8));
            if b_addr == c_addr {
                matches += 1;
            }
            total += 1;
        }
    }
    (matches, total)
}

macro_rules! for_each_n {
    ($run:ident, $name:literal) => {{
        let mut out = String::new();
        let (m1, t1) = $run::<1>();
        let (m7, t7) = $run::<7>();
        let (m16, t16) = $run::<16>();
        let (m17, t17) = $run::<17>();
        let (m32, t32) = $run::<32>();
        assert_eq!((t1, t7, t16, t17, t32), (8, 8, 8, 8, 8));
        out.push_str(&format!(
            "N=1 match={}/{}; N=7 match={}/{}; N=16 match={}/{}; N=17 match={}/{}; N=32 match={}/{}",
            m1, t1, m7, t7, m16, t16, m17, t17, m32, t32
        ));
        println!("[r11_w0_box_reuse] {} {}", $name, out);
    }};
}

fn main() {
    let mut args = std::env::args_os();
    let _program = args.next();
    let selected = args.next();
    if args.next().is_some() {
        std::process::exit(2);
    }
    match selected.as_deref().and_then(|s| s.to_str()) {
        Some("w0") => {
            for_each_n!(run_w0, "w0");
            println!("[r11_w0_box_reuse] COMPLETE w0");
        }
        Some("w0-after") => {
            for_each_n!(run_w0_after, "w0-after");
            println!("[r11_w0_box_reuse] COMPLETE w0-after");
        }
        Some("w0-same-frame") => {
            for_each_n!(run_w0_same_frame, "w0-same-frame");
            println!("[r11_w0_box_reuse] COMPLETE w0-same-frame");
        }
        _ => std::process::exit(2),
    }
}
