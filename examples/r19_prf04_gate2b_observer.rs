//! Allocation-free first allocating call observer; const TLS has no Drop.
//! SAFETY boundary: only forwards GlobalAlloc contracts, never inspects pointers.
#![allow(unsafe_code)] // the one `unsafe impl GlobalAlloc` below; see its SAFETY note
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

static INNER: SeferAlloc = SeferAlloc::new();
static EPOCH: OnceLock<Instant> = OnceLock::new();
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Sample {
    calls: AtomicU64,
    start: AtomicU64,
    elapsed: AtomicU64,
    deallocs: AtomicU64,
}
impl Sample {
    const fn new() -> Self {
        Self {
            calls: AtomicU64::new(0),
            start: AtomicU64::new(0),
            elapsed: AtomicU64::new(0),
            deallocs: AtomicU64::new(0),
        }
    }
}
// Main + at most 4096 workers; overflow is observable rejection, not panic.
static SAMPLES: [Sample; 8192] = [const { Sample::new() }; 8192];
thread_local! {
    static STATE: Cell<(usize, bool)> = const { Cell::new((usize::MAX, false)) };
}
fn clock() -> u64 {
    let nanos = EPOCH.get_or_init(Instant::now).elapsed().as_nanos();
    u64::try_from(nanos).unwrap_or(u64::MAX)
}
fn slot() -> (usize, bool) {
    STATE.with(|state| {
        let (index, first) = state.get();
        if index == usize::MAX {
            let index = NEXT.fetch_add(1, Ordering::Relaxed);
            state.set((index, first));
            (index, first)
        } else {
            (index, first)
        }
    })
}
fn begin() -> Option<(usize, u64)> {
    let (index, first) = slot();
    if first {
        return None;
    }
    // Mark before forwarding: recursion cannot produce a second sample.
    STATE.with(|state| state.set((index, true)));
    let start = clock();
    if let Some(sample) = SAMPLES.get(index) {
        sample.start.store(start, Ordering::Relaxed);
        Some((index, start))
    } else {
        None
    }
}
fn end(event: Option<(usize, u64)>) {
    if let Some((index, start)) = event {
        if let Some(sample) = SAMPLES.get(index) {
            sample
                .elapsed
                .store(clock().saturating_sub(start), Ordering::Relaxed);
            sample.calls.fetch_add(1, Ordering::Relaxed);
        }
    }
}
fn observation(release: u64) -> [u64; 5] {
    let (index, _) = slot();
    match SAMPLES.get(index) {
        Some(s) => [
            s.calls.load(Ordering::Relaxed),
            s.start.load(Ordering::Relaxed),
            s.elapsed.load(Ordering::Relaxed),
            s.deallocs.load(Ordering::Relaxed),
            release,
        ],
        None => [0, 0, 0, 0, release],
    }
}
struct Observer;
// SAFETY: forwards each operation and its unchanged layout/pointer to INNER;
// bookkeeping uses only const no-Drop TLS and preallocated atomic storage.
// No allocation, pointer access, callback, formatting or panic in the wrapper.
unsafe impl GlobalAlloc for Observer {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let event = begin();
        // SAFETY: caller supplies the GlobalAlloc layout contract, unchanged.
        let p = unsafe { INNER.alloc(layout) };
        end(event);
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let event = begin();
        // SAFETY: caller supplies the GlobalAlloc layout contract, unchanged.
        let p = unsafe { INNER.alloc_zeroed(layout) };
        end(event);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let event = begin();
        // SAFETY: live allocation, old layout and new size are caller obligations;
        // they are forwarded to the same allocator instance without modification.
        let p = unsafe { INNER.realloc(p, layout, size) };
        end(event);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        let (index, first) = slot();
        if !first {
            if let Some(s) = SAMPLES.get(index) {
                s.deallocs.fetch_add(1, Ordering::Relaxed);
            }
        }
        // SAFETY: caller supplies a live INNER allocation with matching layout.
        unsafe { INNER.dealloc(p, layout) };
    }
}
#[global_allocator]
static GLOBAL: Observer = Observer;
#[path = "r19_prf04_gate2b_shared/harness.rs"]
mod harness;
fn main() {
    harness::run(&INNER, true);
}
