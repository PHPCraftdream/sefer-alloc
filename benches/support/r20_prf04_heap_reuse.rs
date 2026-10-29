//! P4-1 wave-release and shipping-layer support bodies.

#[path = "r20_prf04.rs"]
pub mod prf04;

use sefer_alloc::registry::HeapRegistry;
use std::hint::black_box;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

pub const HR_N: usize = prf04::PRF04_N;
pub const HR_2N: usize = prf04::PRF04_2N;
pub const HR_4N: usize = prf04::PRF04_4N;
pub const HR_WAVE_SLOTS: usize = prf04::PRF04_WAVE_SLOTS;

pub fn wave_release_warmup() -> Vec<sefer_alloc::registry::HeapLease> {
    let mut held = Vec::with_capacity(HR_WAVE_SLOTS);
    for _ in 0..HR_WAVE_SLOTS {
        held.push(HeapRegistry::dbg_claim_lease().expect("wave warmup claim"));
    }
    black_box(&held);
    held
}

pub fn wave_release_measured(held: &mut Vec<sefer_alloc::registry::HeapLease>, releases: usize) {
    assert!(releases <= held.len());
    for lease in held.drain(..releases) {
        black_box(&lease);
        drop(lease);
    }
}

/// Leaves the unmeasured remainder LIVE, excluding its drop cost from slopes.
pub fn wave_release_first(releases: usize) {
    let mut held = wave_release_warmup();
    wave_release_measured(&mut held, releases);
    std::mem::forget(held);
}

#[cfg(feature = "bench-internals")]
static TARGET_CLAIMS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
static TARGET_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "bench-internals")]
pub fn reset_target_activation() {
    TARGET_CLAIMS.store(0, std::sync::atomic::Ordering::Relaxed);
    TARGET_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(not(feature = "bench-internals"))]
pub fn reset_target_activation() {}
#[cfg(feature = "bench-internals")]
pub fn target_activation() -> (u64, u64) {
    (
        TARGET_CLAIMS.load(std::sync::atomic::Ordering::Relaxed),
        TARGET_CALLS.load(std::sync::atomic::Ordering::Relaxed),
    )
}
#[cfg(not(feature = "bench-internals"))]
pub fn target_activation() -> (u64, u64) {
    (0, 0)
}

#[inline(never)]
pub fn child_first_alloc() -> u64 {
    use std::alloc::{GlobalAlloc, Layout};
    #[cfg(feature = "bench-internals")]
    {
        let before = claim_total();
        let layout = Layout::from_size_align(8, 8).expect("valid layout");
        // SAFETY: valid non-zero layout; pointer is deliberately retained until process exit.
        let ptr = unsafe {
            <sefer_alloc::SeferAlloc as GlobalAlloc>::alloc(&sefer_alloc::SeferAlloc::new(), layout)
        };
        assert!(!ptr.is_null(), "child allocation failed");
        black_box(ptr);
        let delta = claim_total() - before;
        TARGET_CLAIMS.fetch_add(delta, std::sync::atomic::Ordering::Relaxed);
        TARGET_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        delta
    }
    #[cfg(not(feature = "bench-internals"))]
    {
        let layout = Layout::from_size_align(8, 8).expect("valid layout");
        // SAFETY: valid non-zero layout; pointer is deliberately retained until process exit.
        let ptr = unsafe {
            <sefer_alloc::SeferAlloc as GlobalAlloc>::alloc(&sefer_alloc::SeferAlloc::new(), layout)
        };
        assert!(!ptr.is_null(), "child allocation failed");
        black_box(ptr);
        0
    }
}

#[cfg(feature = "bench-internals")]
fn claim_total() -> u64 {
    HeapRegistry::dbg_registry_claim_sources()
        .iter()
        .copied()
        .sum()
}

#[cfg(not(feature = "bench-internals"))]
fn claim_total() -> u64 {
    0
}

pub fn l2_child_cold(claims: usize) {
    let holders = l2_wave_holders(claims);
    std::mem::forget(holders);
}

pub fn l2_recycled_warmup() {
    drop(HeapRegistry::dbg_claim_lease().expect("L2 recycled warmup"));
}

pub fn l2_recycled(claims: usize) {
    for _ in 0..claims {
        L2Holder::spawn().release();
    }
}

struct HolderState {
    ready: bool,
    go: bool,
}
pub struct L2Holder {
    handle: JoinHandle<()>,
    gate: Arc<(Mutex<HolderState>, Condvar)>,
}

impl L2Holder {
    fn spawn() -> Self {
        let gate = Arc::new((
            Mutex::new(HolderState {
                ready: false,
                go: false,
            }),
            Condvar::new(),
        ));
        let child_gate = Arc::clone(&gate);
        let handle = std::thread::spawn(move || {
            child_first_alloc();
            let (lock, cv) = &*child_gate;
            let mut state = lock.lock().expect("holder gate poisoned");
            state.ready = true;
            cv.notify_all();
            while !state.go {
                state = cv.wait(state).expect("holder gate poisoned");
            }
        });
        let holder = Self { handle, gate };
        let (lock, cv) = &*holder.gate;
        let mut state = lock.lock().expect("holder gate poisoned");
        while !state.ready {
            state = cv.wait(state).expect("holder gate poisoned");
        }
        drop(state);
        holder
    }

    fn release(self) {
        let (lock, cv) = &*self.gate;
        lock.lock().expect("holder gate poisoned").go = true;
        cv.notify_one();
        self.handle.join().expect("holder child panicked");
    }
}

pub fn l2_wave_holders(count: usize) -> Vec<L2Holder> {
    (0..count).map(|_| L2Holder::spawn()).collect()
}

pub fn l2_wave_release_measured(holders: &mut Vec<L2Holder>, releases: usize) {
    assert!(releases <= holders.len());
    for holder in holders.drain(..releases) {
        holder.release();
    }
}

pub fn l2_wave_release_first(releases: usize) {
    let mut holders = l2_wave_holders(HR_WAVE_SLOTS);
    l2_wave_release_measured(&mut holders, releases);
    std::mem::forget(holders);
}
