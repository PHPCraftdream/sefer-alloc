//! PRF-04 L1 deterministic claim/release bodies. No oracle reads in these cells.
// Shared by the L1/L2 benches and oracles; each includer uses only its own subset.
#![allow(dead_code)]
use sefer_alloc::{registry::HeapRegistry, SeferAlloc};
use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

static INNER: SeferAlloc = SeferAlloc::new();
static ARMED: AtomicBool = AtomicBool::new(false);
thread_local! {
    static FIRST: Cell<bool> = const { Cell::new(true) };
    static TARGET_CALLS: Cell<u64> = const { Cell::new(0) };
}

pub struct FirstAlloc<const OBSERVE: bool>;

/// # Safety
/// `layout` must satisfy the allocation contract of `GlobalAlloc`.
#[inline(never)]
pub unsafe fn first_global_alloc(layout: Layout, zeroed: bool) -> *mut u8 {
    if zeroed {
        // SAFETY: the caller upholds GlobalAlloc's valid-layout contract; the layout is forwarded unchanged.
        unsafe { INNER.alloc_zeroed(layout) }
    } else {
        // SAFETY: the caller upholds GlobalAlloc's valid-layout contract; the layout is forwarded unchanged.
        unsafe { INNER.alloc(layout) }
    }
}

impl<const OBSERVE: bool> FirstAlloc<OBSERVE> {
    /// # Safety
    /// `layout` must be valid for the selected GlobalAlloc allocation operation.
    #[inline(never)]
    unsafe fn alloc_inner(&self, layout: Layout, zeroed: bool) -> *mut u8 {
        let first = FIRST.try_with(|flag| flag.replace(false)).unwrap_or(false);
        if first && ARMED.load(Ordering::Relaxed) {
            if OBSERVE {
                let _ = TARGET_CALLS.try_with(|count| count.set(count.get() + 1));
            }
            // SAFETY: this method's caller upholds the GlobalAlloc layout contract.
            unsafe { first_global_alloc(layout, zeroed) }
        } else if zeroed {
            // SAFETY: the GlobalAlloc caller supplies a valid layout; forwarding preserves it and ownership.
            unsafe { INNER.alloc_zeroed(layout) }
        } else {
            // SAFETY: the GlobalAlloc caller supplies a valid layout; forwarding preserves it and ownership.
            unsafe { INNER.alloc(layout) }
        }
    }
}

// SAFETY: every pointer/layout pair is forwarded to the same SeferAlloc instance.
unsafe impl<const OBSERVE: bool> GlobalAlloc for FirstAlloc<OBSERVE> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's caller supplies a valid layout; unchanged forwarding preserves it and ownership.
        unsafe { self.alloc_inner(layout, false) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: GlobalAlloc's caller supplies a valid layout; unchanged forwarding preserves it and ownership.
        unsafe { self.alloc_inner(layout, true) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: caller provides the pointer/layout pair returned by this allocator.
        unsafe { INNER.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: caller upholds GlobalAlloc realloc ownership/layout requirements.
        unsafe { INNER.realloc(ptr, old_layout, new_size) }
    }
}

static WARMED: AtomicBool = AtomicBool::new(false);

pub fn warm_parent_and_arm() {
    if WARMED.swap(true, Ordering::Relaxed) {
        return;
    }
    let layout = Layout::from_size_align(8, 8).expect("valid warmup layout");
    // SAFETY: a valid non-zero layout is passed through the installed allocator; retained to process exit.
    let ptr = unsafe { std::alloc::alloc(layout) };
    assert!(!ptr.is_null(), "installed-allocator warmup failed");
    ARMED.store(true, Ordering::Relaxed);
}

pub fn target_calls() -> u64 {
    TARGET_CALLS.try_with(Cell::get).unwrap_or(0)
}

pub const N: usize = 16;
pub const WAVE_SLOTS: usize = 64;
#[inline(never)]
pub fn cold(n: usize) {
    for _ in 0..n {
        let lease = HeapRegistry::dbg_claim_lease().expect("claim");
        black_box(&lease);
        std::mem::forget(lease);
    }
}
pub fn recycled_setup() {
    drop(HeapRegistry::dbg_claim_lease().expect("warmup"));
}
#[inline(never)]
pub fn recycled(n: usize) {
    for _ in 0..n {
        let lease = HeapRegistry::dbg_claim_lease().expect("recycled claim");
        black_box(&lease);
        drop(lease);
    }
}
pub fn wave_setup() {
    let mut held = Vec::with_capacity(WAVE_SLOTS);
    for _ in 0..WAVE_SLOTS {
        held.push(HeapRegistry::dbg_claim_lease().expect("wave"));
    }
    drop(held);
}
#[inline(never)]
pub fn wave_claim(n: usize) {
    for _ in 0..n {
        let lease = HeapRegistry::dbg_claim_lease().expect("wave claim");
        black_box(&lease);
        std::mem::forget(lease);
    }
}
pub fn wave_release_setup() -> Vec<sefer_alloc::registry::HeapLease> {
    let mut held = Vec::with_capacity(WAVE_SLOTS);
    for _ in 0..WAVE_SLOTS {
        held.push(HeapRegistry::dbg_claim_lease().expect("held lease"));
    }
    black_box(&held);
    held
}
#[inline(never)]
pub fn wave_release_measured(held: &mut Vec<sefer_alloc::registry::HeapLease>, n: usize) {
    assert!(n <= held.len());
    // Drain the tail: a front drain memmoves the remaining leases, which made
    // the window cost nonlinear in n (PRF-04 run01 linearity).
    let start = held.len() - n;
    for lease in held.drain(start..) {
        black_box(&lease);
        drop(lease);
    }
}
pub fn wave_release_cell(n: usize) {
    let mut held = wave_release_setup();
    wave_release_measured(&mut held, n);
    std::mem::forget(held);
}

struct State {
    ready: bool,
    release: bool,
}
pub struct Child {
    handle: JoinHandle<()>,
    gate: Arc<(Mutex<State>, Condvar)>,
}
#[inline(never)]
pub fn child_first_alloc() {
    use std::alloc::{alloc, Layout};
    let layout = Layout::from_size_align(8, 8).expect("valid layout");
    // SAFETY: the layout is valid and non-zero; the pointer is retained to process exit.
    let ptr = unsafe { alloc(layout) };
    assert!(!ptr.is_null());
    black_box(ptr);
}
impl Child {
    pub fn spawn() -> Self {
        let gate = Arc::new((
            Mutex::new(State {
                ready: false,
                release: false,
            }),
            Condvar::new(),
        ));
        let child_gate = Arc::clone(&gate);
        let handle = std::thread::spawn(move || {
            child_first_alloc();
            let (lock, cv) = &*child_gate;
            let mut state = lock.lock().expect("child gate");
            state.ready = true;
            cv.notify_all();
            while !state.release {
                state = cv.wait(state).expect("child gate");
            }
        });
        let child = Self { handle, gate };
        let (lock, cv) = &*child.gate;
        let mut state = lock.lock().expect("ready gate");
        while !state.ready {
            state = cv.wait(state).expect("ready gate");
        }
        drop(state);
        child
    }
    pub fn release(self) {
        let (lock, cv) = &*self.gate;
        lock.lock().expect("release gate").release = true;
        cv.notify_one();
        self.handle.join().expect("child panic");
    }
}
pub fn l2_cold(n: usize) {
    warm_parent_and_arm();
    std::mem::forget((0..n).map(|_| Child::spawn()).collect::<Vec<_>>());
}
fn l2_recycled_pair(n: usize) {
    warm_parent_and_arm();
    drop(HeapRegistry::dbg_claim_lease().expect("recycled setup"));
    for _ in 0..n {
        Child::spawn().release();
    }
}
pub fn l2_recycled_alloc(n: usize) {
    l2_recycled_pair(n);
}
pub fn l2_recycled_release(n: usize) {
    l2_recycled_pair(n);
}
pub fn l2_wave_setup() {
    warm_parent_and_arm();
    let mut held = wave_release_setup();
    wave_release_measured(&mut held, WAVE_SLOTS);
}
#[inline(never)]
pub fn l2_wave_claim(n: usize) {
    for _ in 0..n {
        std::mem::forget(Child::spawn());
    }
}
pub fn l2_wave(n: usize) {
    assert!(n <= WAVE_SLOTS);
    l2_wave_setup();
    l2_wave_claim(n);
}
pub fn l2_wave_release_setup() -> Vec<Child> {
    warm_parent_and_arm();
    (0..WAVE_SLOTS).map(|_| Child::spawn()).collect()
}
#[inline(never)]
pub fn l2_wave_release_measured(children: &mut Vec<Child>, n: usize) {
    assert!(n <= children.len());
    let start = children.len() - n;
    for child in children.drain(start..) {
        child.release();
    }
}
pub fn l2_wave_release(n: usize) {
    let mut children = l2_wave_release_setup();
    l2_wave_release_measured(&mut children, n);
    std::mem::forget(children);
}
