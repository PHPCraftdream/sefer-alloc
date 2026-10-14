//! PRF-04 L1 deterministic claim/release bodies. No oracle reads in these cells.
use sefer_alloc::registry::HeapRegistry;
use std::hint::black_box;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

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
    for lease in held.drain(..n) {
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
    std::mem::forget((0..n).map(|_| Child::spawn()).collect::<Vec<_>>());
}
fn l2_recycled_pair(n: usize) {
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
    (0..WAVE_SLOTS).map(|_| Child::spawn()).collect()
}
#[inline(never)]
pub fn l2_wave_release_measured(children: &mut Vec<Child>, n: usize) {
    assert!(n <= children.len());
    for child in children.drain(..n) {
        child.release();
    }
}
pub fn l2_wave_release(n: usize) {
    let mut children = l2_wave_release_setup();
    l2_wave_release_measured(&mut children, n);
    std::mem::forget(children);
}
