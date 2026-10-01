//! Tiny installed-System witnesses, not Sefer acceptance or a shipping allocator.

use std::alloc::{GlobalAlloc, Layout, System};
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
use std::sync::Barrier;

// Pre-existing out-of-band descriptor: no field lives in payload.
struct Descriptor {
    arm_size: AtomicUsize,
    exact_size: AtomicUsize,
    root: AtomicPtr<u8>,
    target: AtomicPtr<u8>,
    cap: AtomicPtr<u8>,
    live: AtomicUsize,
}

static DESCRIPTOR: Descriptor = Descriptor {
    arm_size: AtomicUsize::new(0),
    exact_size: AtomicUsize::new(0),
    root: AtomicPtr::new(ptr::null_mut()),
    target: AtomicPtr::new(ptr::null_mut()),
    cap: AtomicPtr::new(ptr::null_mut()),
    live: AtomicUsize::new(0),
};
static PUBLISHED: Barrier = Barrier::new(2);
static RESUME: Barrier = Barrier::new(2);

pub struct Gate<const OWNER_FREE: bool>;

unsafe impl<const OWNER_FREE: bool> GlobalAlloc for Gate<OWNER_FREE> {
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
            // SAFETY: This is the unchanged pointer and layout from this wrapper's alloc.
            unsafe { System.dealloc(ptr, layout) };
            return;
        }
        assert_eq!(layout.size(), DESCRIPTOR.exact_size.load(Ordering::Relaxed));
        assert_eq!(layout.align(), 1);
        if OWNER_FREE {
            DESCRIPTOR.cap.store(ptr, Ordering::Relaxed);
        } else {
            // Remove the old exact-address route before System can reuse its VA.
            DESCRIPTOR.target.store(ptr::null_mut(), Ordering::Release);
            // SAFETY: This is the original System allocation and exact incoming Box layout.
            unsafe { System.dealloc(ptr, layout) };
        }
        // Terminal accounting publication is inside the real Box::drop/dealloc frame.
        assert_eq!(DESCRIPTOR.live.fetch_sub(1, Ordering::AcqRel), 1);
        PUBLISHED.wait();
        RESUME.wait();
    }
}

#[derive(Clone, Copy)]
// W1 compiles this shared type but exposes no negative selector.
#[allow(dead_code)]
pub enum Scenario {
    Positive,
    #[cfg(miri)]
    RootOne,
    #[cfg(miri)]
    RootEight,
}

pub fn run<const OWNER_FREE: bool>(scenario: Scenario) {
    match scenario {
        Scenario::Positive => {
            for round in 0..2 {
                run_one::<OWNER_FREE, 1>(round, scenario);
                run_one::<OWNER_FREE, 2>(round, scenario);
                run_one::<OWNER_FREE, 3>(round, scenario);
                run_one::<OWNER_FREE, 4>(round, scenario);
                run_one::<OWNER_FREE, 5>(round, scenario);
                run_one::<OWNER_FREE, 6>(round, scenario);
                run_one::<OWNER_FREE, 7>(round, scenario);
            }
            println!("COMPLETE owner_free={OWNER_FREE} sizes=1..7 rounds=2");
        }
        #[cfg(miri)]
        Scenario::RootOne => run_one::<OWNER_FREE, 1>(0, scenario),
        #[cfg(miri)]
        Scenario::RootEight => run_one::<OWNER_FREE, 8>(0, scenario),
    }
}

fn run_one<const OWNER_FREE: bool, const N: usize>(round: u8, scenario: Scenario) {
    #[cfg(miri)]
    assert!(matches!(scenario, Scenario::Positive) || OWNER_FREE);
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

    match scenario {
        #[cfg(miri)]
        Scenario::RootOne => {
            let root = DESCRIPTOR.root.load(Ordering::Relaxed);
            // SAFETY: Deliberately invalid Miri negative control: this root
            // write conflicts with the still-active Box protector.
            unsafe { root.write(0x7f) };
            std::process::exit(3);
        }
        #[cfg(miri)]
        Scenario::RootEight => {
            let root = DESCRIPTOR.root.load(Ordering::Relaxed);
            // SAFETY: Deliberately invalid Miri negative control: all eight
            // bytes are in bounds; the intrusive-style root write conflicts.
            unsafe { root.cast::<u64>().write_unaligned(0) };
            std::process::exit(3);
        }
        Scenario::Positive => {}
    }

    if OWNER_FREE {
        let cap = DESCRIPTOR.cap.load(Ordering::Relaxed);
        assert_eq!(cap.addr(), target.addr());
        let layout = Layout::array::<u8>(N).unwrap();
        // Retire the route before a physical free can recycle the address.
        DESCRIPTOR.target.store(ptr::null_mut(), Ordering::Release);
        // SAFETY: The producer transferred this exact incoming dealloc pointer
        // before Release publication; Acquire observed it, and this is its
        // sole physical free with the original System allocation layout.
        unsafe { System.dealloc(cap, layout) };
    }
    let layout = Layout::array::<u8>(N).unwrap();
    // SAFETY: Exact nonzero layout; this is a fresh System allocation, not
    // same-offset reuse of the old live allocation.
    let fresh = unsafe { System.alloc_zeroed(layout) };
    assert!(!fresh.is_null());
    let va_reused = fresh.addr() == target.addr();
    for i in 0..N {
        // SAFETY: Each byte is in the newly allocated N-byte object.
        assert_eq!(unsafe { *fresh.add(i) }, 0);
    }
    // SAFETY: Fresh is freed once with its unchanged System layout.
    unsafe { System.dealloc(fresh, layout) };
    RESUME.wait();
    producer.join().unwrap();
    println!("ROUND owner_free={OWNER_FREE} size={N} round={round} va_reused={va_reused} joined");
}
