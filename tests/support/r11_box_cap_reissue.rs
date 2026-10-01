//! W+3 / VA-incarnation witnesses over an installed System wrapper.
//! Not Sefer acceptance and not a shipping allocator.
#![allow(dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use std::sync::Barrier;

// Out-of-band descriptor: no field lives in the payload.
struct Descriptor {
    arm_size: AtomicUsize,
    exact_size: AtomicUsize,
    root: AtomicPtr<u8>,
    target: AtomicPtr<u8>,
    cap: AtomicPtr<u8>,
    live: AtomicUsize,
    generation: AtomicU64,
}

static DESCRIPTOR: Descriptor = Descriptor {
    arm_size: AtomicUsize::new(0),
    exact_size: AtomicUsize::new(0),
    root: AtomicPtr::new(ptr::null_mut()),
    target: AtomicPtr::new(ptr::null_mut()),
    cap: AtomicPtr::new(ptr::null_mut()),
    live: AtomicUsize::new(0),
    generation: AtomicU64::new(0),
};
static PUBLISHED: Barrier = Barrier::new(2);
static RESUME: Barrier = Barrier::new(2);

/// Owner-cap gate: the producer's terminal dealloc keeps the incoming
/// pointer as `cap` and pauses inside the real `Box::drop` frame; the
/// owner alone decides whether the cap is freed, re-issued or retired.
pub struct Gate;

unsafe impl GlobalAlloc for Gate {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: System receives the unchanged requested layout.
        let ptr = unsafe { System.alloc(layout) };
        if layout.align() == 1 && layout.size() == DESCRIPTOR.arm_size.load(Ordering::Relaxed) {
            let _ = DESCRIPTOR.root.compare_exchange(
                ptr::null_mut(),
                ptr,
                Ordering::Relaxed,
                Ordering::Relaxed,
            );
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr != DESCRIPTOR.target.load(Ordering::Acquire) {
            // SAFETY: Unchanged pointer and layout from this wrapper's alloc.
            unsafe { System.dealloc(ptr, layout) };
            return;
        }
        assert_eq!(layout.size(), DESCRIPTOR.exact_size.load(Ordering::Relaxed));
        assert_eq!(layout.align(), 1);
        DESCRIPTOR.cap.store(ptr, Ordering::Relaxed);
        assert_eq!(DESCRIPTOR.live.fetch_sub(1, Ordering::AcqRel), 1);
        PUBLISHED.wait();
        RESUME.wait();
    }
}

#[derive(Clone, Copy)]
pub enum Scenario {
    /// W+3 (A): re-issue the retained cap while the producer is still paused.
    ReissueLive,
    /// W+3 (B): re-issue the retained cap after RESUME and join.
    ReissueAfter,
    /// W+3 (C): re-issue while paused; the fresh Box stays live across the
    /// producer's frame exit (RESUME + join) and is freed only afterwards.
    ReissueLiveAcross,
    /// Miri negative control: re-issue through the original root, not the cap.
    #[cfg(miri)]
    ReissueLiveRoot,
    /// VA reuse / incarnation (ABA) observation; owner physically frees cap.
    VaOverlap,
}

pub fn run(scenario: Scenario) {
    match scenario {
        Scenario::VaOverlap => run_va(),
        _ => {
            for round in 0..2u8 {
                reissue_one::<1>(round, scenario);
                reissue_one::<2>(round, scenario);
                reissue_one::<3>(round, scenario);
                reissue_one::<4>(round, scenario);
                reissue_one::<5>(round, scenario);
                reissue_one::<6>(round, scenario);
                reissue_one::<7>(round, scenario);
            }
            println!("COMPLETE reissue sizes=1..7 rounds=2");
        }
    }
}

// Common arm: boxed object, producer paused inside Box::drop after PUBLISHED.
fn arm<const N: usize>(round: u8) -> (*mut u8, std::thread::JoinHandle<()>) {
    DESCRIPTOR.root.store(ptr::null_mut(), Ordering::Relaxed);
    DESCRIPTOR.cap.store(ptr::null_mut(), Ordering::Relaxed);
    DESCRIPTOR.target.store(ptr::null_mut(), Ordering::Relaxed);
    DESCRIPTOR.exact_size.store(N, Ordering::Relaxed);
    DESCRIPTOR.live.store(1, Ordering::Relaxed);
    DESCRIPTOR.arm_size.store(N, Ordering::Relaxed);
    let mut boxed = Box::new([0x41u8.wrapping_add(round); N]);
    DESCRIPTOR.arm_size.store(0, Ordering::Relaxed);
    let target = {
        let narrow: &mut [u8; N] = &mut boxed;
        narrow[N - 1] ^= 1;
        narrow.as_mut_ptr()
    };
    assert_eq!(
        DESCRIPTOR.root.load(Ordering::Relaxed).addr(),
        target.addr()
    );
    DESCRIPTOR.target.store(target, Ordering::Release);
    let producer = std::thread::spawn(move || {
        assert_eq!(boxed[N - 1], (0x41u8.wrapping_add(round)) ^ 1);
        drop(boxed);
    });
    PUBLISHED.wait();
    assert_eq!(DESCRIPTOR.live.load(Ordering::Acquire), 0);
    (target, producer)
}

// SAFETY contract as for `reissue`; the returned Box is the sole owner.
unsafe fn adopt<const N: usize>(p: *mut u8, pattern: u8) -> Box<[u8; N]> {
    // SAFETY: per the contract above.
    let mut fresh: Box<[u8; N]> = unsafe { Box::from_raw(p.cast::<[u8; N]>()) };
    *fresh = [pattern; N];
    fresh
}

fn check_pattern<const N: usize>(b: &mut Box<[u8; N]>, pattern: u8) {
    assert!(b.iter().all(|x| *x == pattern));
    b[N - 1] ^= 0x0F;
    assert_eq!(b[N - 1], pattern ^ 0x0F);
}

// SAFETY contract: `p` is the retained, never-freed allocation of exactly
// `[u8; N]` (align 1); the caller retired the exact-address route first.
unsafe fn reissue<const N: usize>(p: *mut u8, round: u8) {
    // SAFETY: per the contract above; this Box becomes the sole owner.
    let mut fresh: Box<[u8; N]> = unsafe { Box::from_raw(p.cast::<[u8; N]>()) };
    *fresh = [0x5Au8.wrapping_add(round); N];
    fresh[N - 1] ^= 0x0F;
    for (i, b) in fresh.iter().enumerate() {
        let want = 0x5Au8.wrapping_add(round) ^ if i == N - 1 { 0x0F } else { 0 };
        assert_eq!(*b, want);
    }
    // Normal Box::drop: one physical System.dealloc with the exact layout.
    drop(fresh);
}

fn reissue_one<const N: usize>(round: u8, scenario: Scenario) {
    let (target, producer) = arm::<N>(round);
    let cap = DESCRIPTOR.cap.load(Ordering::Relaxed);
    assert_eq!(cap.addr(), target.addr());
    match scenario {
        Scenario::ReissueLive => {
            DESCRIPTOR.target.store(ptr::null_mut(), Ordering::Release);
            // SAFETY: cap is the retained exact allocation; no physical free yet.
            unsafe { reissue::<N>(cap, round) };
            RESUME.wait();
            producer.join().unwrap();
        }
        Scenario::ReissueLiveAcross => {
            DESCRIPTOR.target.store(ptr::null_mut(), Ordering::Release);
            let first = 0x5Au8.wrapping_add(round);
            // SAFETY: cap is the retained exact allocation; no physical free yet.
            let mut fresh = unsafe { adopt::<N>(cap, first) };
            check_pattern::<N>(&mut fresh, first);
            RESUME.wait();
            producer.join().unwrap();
            // Producer frame is gone; the fresh object must still be usable.
            let second = 0xA5u8.wrapping_add(round);
            fresh.fill(second);
            check_pattern::<N>(&mut fresh, second);
            // Sole physical System.dealloc with the exact layout.
            drop(fresh);
        }
        Scenario::ReissueAfter => {
            RESUME.wait();
            producer.join().unwrap();
            DESCRIPTOR.target.store(ptr::null_mut(), Ordering::Release);
            // SAFETY: as above; the Box::drop frame has completed.
            unsafe { reissue::<N>(cap, round) };
        }
        #[cfg(miri)]
        Scenario::ReissueLiveRoot => {
            let root = DESCRIPTOR.root.load(Ordering::Relaxed);
            DESCRIPTOR.target.store(ptr::null_mut(), Ordering::Release);
            // SAFETY: Deliberately invalid Miri negative control: root is
            // not the incoming pointer and conflicts with the live protector.
            unsafe { reissue::<N>(root, round) };
            std::process::exit(3);
        }
        Scenario::VaOverlap => unreachable!(),
    }
    println!("ROUND reissue size={N} round={round} joined");
}

// Out-of-band label (VA, generation): an old label must not accept a new
// object that merely landed on the same VA.
#[derive(Clone, Copy)]
struct Incarnation {
    addr: usize,
    generation: u64,
}

impl Incarnation {
    fn issue(addr: usize) -> Self {
        let generation = DESCRIPTOR.generation.fetch_add(1, Ordering::Relaxed) + 1;
        Self { addr, generation }
    }

    fn accepts(self, other: Self) -> bool {
        self.addr == other.addr && self.generation == other.generation
    }
}

#[derive(Default)]
struct VaStats {
    rounds: usize,
    live_reuse: usize,
    after_reuse: usize,
    rejected: usize,
}

const VA_SIZES: usize = 8;
const VA_ROUNDS: usize = 8;

fn run_va() {
    let mut stats = VaStats::default();
    for round in 0..VA_ROUNDS {
        let r = round as u8;
        va_round::<1>(r, &mut stats);
        va_round::<2>(r, &mut stats);
        va_round::<3>(r, &mut stats);
        va_round::<4>(r, &mut stats);
        va_round::<5>(r, &mut stats);
        va_round::<6>(r, &mut stats);
        va_round::<7>(r, &mut stats);
        va_round::<8>(r, &mut stats);
    }
    assert_eq!(stats.rounds, VA_ROUNDS * VA_SIZES);
    assert_eq!(stats.rejected, stats.live_reuse + stats.after_reuse);
    println!(
        "VA_SUMMARY rounds={} live_reuse={}/{} after_reuse={}/{} aba_rejected={} false_accepts=0",
        stats.rounds,
        stats.live_reuse,
        stats.rounds,
        stats.after_reuse,
        stats.rounds,
        stats.rejected
    );
}

fn va_round<const N: usize>(round: u8, stats: &mut VaStats) {
    let (target, producer) = arm::<N>(round);
    let old = Incarnation::issue(target.addr());
    let cap = DESCRIPTOR.cap.load(Ordering::Relaxed);
    assert_eq!(cap.addr(), target.addr());
    let layout = Layout::array::<u8>(N).unwrap();
    DESCRIPTOR.target.store(ptr::null_mut(), Ordering::Release);
    // SAFETY: the producer transferred this exact incoming pointer; this is
    // its sole physical free with the original layout.
    unsafe { System.dealloc(cap, layout) };

    // Phase "live": producer frame still alive.
    // SAFETY: exact nonzero layout, fresh System allocation.
    let a = unsafe { System.alloc_zeroed(layout) };
    assert!(!a.is_null());
    let a_label = Incarnation::issue(a.addr());
    assert!(a_label.accepts(a_label));
    let live_reuse = a.addr() == old.addr;
    if live_reuse {
        stats.live_reuse += 1;
        stats.rejected += usize::from(!old.accepts(a_label));
        assert!(!old.accepts(a_label), "ABA: old label accepted new object");
    }
    // SAFETY: freed once with its unchanged layout.
    unsafe { System.dealloc(a, layout) };

    RESUME.wait();
    producer.join().unwrap();

    // Phase "after": producer frame finished.
    // SAFETY: as above.
    let b = unsafe { System.alloc_zeroed(layout) };
    assert!(!b.is_null());
    let b_label = Incarnation::issue(b.addr());
    let after_reuse = b.addr() == old.addr;
    if after_reuse {
        stats.after_reuse += 1;
        stats.rejected += usize::from(!old.accepts(b_label));
        assert!(!old.accepts(b_label), "ABA: old label accepted new object");
    }
    // SAFETY: as above.
    unsafe { System.dealloc(b, layout) };
    stats.rounds += 1;
    println!("ROUND va size={N} round={round} live_reuse={live_reuse} after_reuse={after_reuse}");
}
