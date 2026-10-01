//! Ph1b witness for the opt-in `exact-object-proto`: narrow requests are exact
//! `System` objects plus out-of-object descriptors, in the real installed
//! `SeferAlloc`. One scenario per process, selected by name; no libtest.
//! Mutants `m1`, `m2` (Miri only) and `m3` are expected to FAIL (red).

use sefer_alloc::global::exact_object::ExactNarrow;
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn counts() -> (usize, usize) {
    ExactNarrow::dbg_counts()
}

fn generation(addr: usize) -> Option<u64> {
    ExactNarrow::dbg_generation_of(addr)
}

fn narrow_round<const N: usize>(round: u8) {
    let fill = 0x40u8.wrapping_add(N as u8).wrapping_add(round);
    let b: Box<[u8; N]> = Box::new([fill; N]);
    let addr = b.as_ptr().addr();
    assert!(generation(addr).is_some(), "N={N}: not routed exact");
    std::thread::spawn(move || {
        assert!(b.iter().all(|&x| x == fill));
        drop(b);
        assert!(
            generation(addr).is_none(),
            "N={N}: descriptor outlived free"
        );
    })
    .join()
    .unwrap();
}

fn narrow() {
    for round in 0..2u8 {
        narrow_round::<1>(round);
        narrow_round::<2>(round);
        narrow_round::<3>(round);
        narrow_round::<4>(round);
        narrow_round::<5>(round);
        narrow_round::<6>(round);
        narrow_round::<7>(round);
        narrow_round::<16>(round);
    }
    // One byte past the boundary stays on the ordinary path (dropped on a
    // foreign thread: the own-thread drop is the `legacy17` baseline).
    let wide: Box<[u8; 17]> = Box::new([7; 17]);
    assert!(generation(wide.as_ptr().addr()).is_none());
    std::thread::spawn(move || drop(wide)).join().unwrap();
    println!("[r11_exact_object_proto] COMPLETE narrow");
}

/// Baseline, not a positive: a non-narrow Box dropped on its owner thread takes
/// the legacy slab path (`write_next` into the block while the Box argument is
/// protected). Expected RED under Miri; shows the legacy P1 independent of any pause.
fn legacy17() {
    let wide: Box<[u8; 17]> = Box::new([7; 17]);
    drop(wide);
    println!("[r11_exact_object_proto] COMPLETE legacy17");
}

fn zero() {
    for size in [1usize, 3, 8, 16] {
        let layout = Layout::from_size_align(size, 1).unwrap();
        // SAFETY: matched alloc/dealloc pairs with the same layout.
        unsafe {
            let dirty = GLOBAL.alloc(layout);
            assert!(!dirty.is_null());
            dirty.write_bytes(0xAA, size);
            GLOBAL.dealloc(dirty, layout);
            let z = GLOBAL.alloc_zeroed(layout);
            assert!(!z.is_null());
            assert!(generation(z.addr()).is_some());
            assert!((0..size).all(|i| z.add(i).read() == 0), "size {size}");
            GLOBAL.dealloc(z, layout);
        }
    }
    let v = vec![0u8; 5];
    assert!(v.iter().all(|&x| x == 0));
    drop(v);
    println!("[r11_exact_object_proto] COMPLETE zero");
}

fn release() {
    const K: usize = 32;
    let (a0, d0) = counts();
    for i in 0..K {
        let b = Box::new([i as u8; 5]);
        if i % 2 == 0 {
            std::thread::spawn(move || drop(b)).join().unwrap();
        } else {
            drop(b);
        }
    }
    let (a1, d1) = counts();
    let (da, dd) = (a1 - a0, d1 - d0);
    assert!(da >= K, "allocs {da} < {K}");
    assert_eq!(da, dd, "exact alloc/dealloc not balanced");
    println!("[r11_exact_object_proto] COMPLETE release allocs={da} deallocs={dd}");
}

fn reissue() {
    // Free a whole batch, then reissue a batch: allocators (random-order LFH
    // included) hand back many of the same VAs. Every reused VA must carry a
    // fresh generation and reject the old one.
    let n = if cfg!(miri) { 128 } else { 512 };
    let first: Vec<Box<u8>> = (0..n).map(|_| Box::new(1u8)).collect();
    let mut old: Vec<(usize, u64)> = first
        .iter()
        .map(|b| {
            let addr = (&**b as *const u8).addr();
            (addr, generation(addr).expect("exact"))
        })
        .collect();
    old.sort_unstable();
    drop(first);
    assert!(old.iter().all(|&(a, _)| generation(a).is_none()));
    let second: Vec<Box<u8>> = (0..n).map(|_| Box::new(2u8)).collect();
    let mut hits = 0usize;
    for b in &second {
        let addr = (&**b as *const u8).addr();
        let g2 = generation(addr).expect("reissue registered");
        if let Ok(i) = old.binary_search_by_key(&addr, |&(a, _)| a) {
            let g1 = old[i].1;
            assert_ne!(g1, g2, "same-VA reissue kept the old generation");
            assert_ne!(generation(addr), Some(g1), "stale descriptor accepted");
            hits += 1;
        }
    }
    if hits == 0 {
        println!("[r11_exact_object_proto] FAIL reissue: no VA reused in {n} objects");
        std::process::exit(3);
    }
    println!("[r11_exact_object_proto] COMPLETE reissue same_va_reissues={hits} of {n}");
}

fn m3() {
    let layout = Layout::from_size_align(1, 1).unwrap();
    let wrong = Layout::from_size_align(2, 1).unwrap();
    // SAFETY: deliberately violates the contract; the descriptor check must abort.
    unsafe {
        let p = GLOBAL.alloc(layout);
        GLOBAL.dealloc(p, wrong);
    }
    println!("[r11_exact_object_proto] m3 NOT DETECTED: wrong layout accepted");
    std::process::exit(3);
}

#[cfg(miri)]
mod paused {
    use super::*;
    use sefer_alloc::registry::segment_route::TerminalPublicationGate as Gate;

    struct ResumeProducer;
    impl Drop for ResumeProducer {
        fn drop(&mut self) {
            Gate::resume_producer();
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    pub enum Mode {
        Positive,
        /// W-1: write through the root pointer while the producer is paused.
        RootWrite,
        /// Reissue the freed VA while the (early-freeing) producer is paused.
        ReuseWhilePaused,
    }

    pub fn run(global: &SeferAlloc, mode: Mode) {
        let mut byte = Box::new(0x70u8);
        let root: *mut u8 = &mut *byte;
        let addr = root.addr();
        assert!(generation(addr).is_some());
        let (_, d0) = counts();
        Gate::arm(addr);
        let resume = ResumeProducer;
        let producer = std::thread::spawn(move || {
            Gate::prepare_producer();
            assert_eq!(*byte, 0x70);
            drop(byte);
        });
        Gate::allow_prepared_producer();
        Gate::wait_for_publication();

        // Producer is paused inside Box::drop / GlobalAlloc::dealloc.
        match mode {
            Mode::Positive => {
                assert!(generation(addr).is_none(), "descriptor not unlinked");
                assert_eq!(counts().1, d0 + 1, "physical free not accounted");
                global.trim_current_thread();
                let fresh = Box::new(0x55u8);
                assert!(generation((&*fresh as *const u8).addr()).is_some());
                assert_eq!(*fresh, 0x55);
                drop(resume);
                producer.join().unwrap();
                drop(fresh);
                println!("[r11_exact_object_proto] COMPLETE paused");
            }
            Mode::RootWrite => {
                global.trim_current_thread();
                // SAFETY: intentionally invalid (W-1 mutant): the object is freed.
                unsafe { root.write(0x99) };
                drop(resume);
                producer.join().unwrap();
                println!("[r11_exact_object_proto] m1 NOT DETECTED");
                std::process::exit(3);
            }
            Mode::ReuseWhilePaused => {
                let mut held: [Option<Box<u8>>; 64] = [const { None }; 64];
                for slot in held.iter_mut() {
                    *slot = Some(Box::new(1u8));
                }
                drop(resume);
                producer.join().unwrap();
                println!("[r11_exact_object_proto] m2 NOT DETECTED");
                std::process::exit(3);
            }
        }
    }
}

fn main() {
    let mut args = std::env::args_os();
    let _program = args.next();
    let selected = args.next();
    if args.next().is_some() {
        std::process::exit(2);
    }
    match selected.as_deref().and_then(|s| s.to_str()) {
        Some("narrow") => narrow(),
        Some("zero") => zero(),
        Some("release") => release(),
        Some("reissue") => reissue(),
        Some("m3") => m3(),
        Some("legacy17") => legacy17(),
        #[cfg(miri)]
        Some("paused") => paused::run(&GLOBAL, paused::Mode::Positive),
        #[cfg(miri)]
        Some("m1") => paused::run(&GLOBAL, paused::Mode::RootWrite),
        #[cfg(miri)]
        Some("m2") => {
            ExactNarrow::dbg_set_early_free(true);
            paused::run(&GLOBAL, paused::Mode::ReuseWhilePaused)
        }
        _ => std::process::exit(2),
    }
}
