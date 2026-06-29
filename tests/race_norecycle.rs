//! Control experiment for task #33/#37: the SAME cross-thread-free reclaim
//! stress as `race_repro`, but with **long-lived producer threads** — they are
//! spawned ONCE and never exit until the end, so their registry slots are
//! **never recycled**.
//!
//! Hypothesis under test: the reclaim crash requires slot-recycle (short-lived
//! producers dying while their blocks are in flight to a long-lived consumer).
//!
//!   - If this test is GREEN while `race_repro` (per-wave producers, recycle) is
//!     RED → the recycle boundary is the cause → the boundary discipline
//!     (epoch / quiesce, spec §5) is the correct fix.
//!   - If this test ALSO crashes → recycle is NOT the cause; the bug is in the
//!     plain 2-thread cross-thread-free reclaim, and the fix lies elsewhere.
//!
//! No mutex is held across an alloc/free (lock-order hazard). Bounded by a
//! watchdog so a deadlock fails fast.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const DEADLINE_SECS: u64 = 30;

struct Watchdog {
    done: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Watchdog {
    fn start(label: &'static str) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let d = Arc::clone(&done);
        let handle = std::thread::Builder::new()
            .name(format!("wd-{label}"))
            .spawn(move || {
                let start = std::time::Instant::now();
                while start.elapsed().as_secs() < DEADLINE_SECS {
                    if d.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                eprintln!("[wd-{label}] exceeded {DEADLINE_SECS}s — aborting (deadlock?)");
                std::process::abort();
            })
            .expect("spawn watchdog");
        Watchdog {
            done,
            handle: Some(handle),
        }
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

struct SendPtr(*mut u8);
unsafe impl Send for SendPtr {}

#[test]
fn cross_thread_reclaim_no_recycle() {
    let _wd = Watchdog::start("no-recycle");

    const PRODUCERS: usize = 2;
    const ALLOCS: usize = 50_000; // per producer
    const SIZE: usize = 32;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    let total_sent = Arc::new(AtomicU64::new(0));
    let total_recv = Arc::new(AtomicU64::new(0));

    let (tx, rx) = mpsc::channel::<(SendPtr, u64)>();

    // ONE long-lived consumer: frees every block it receives (cross-thread).
    let total_recv_c = Arc::clone(&total_recv);
    let consumer = std::thread::spawn(move || {
        let mut acc: u64 = 0;
        for (SendPtr(ptr), val) in rx {
            // Tag must survive until we free it (catches reuse-while-in-flight).
            let read_back = unsafe { std::ptr::read(ptr as *const u64) };
            assert_eq!(
                read_back, val,
                "tag corruption: wrote {val:#x} read {read_back:#x} (reclaim UAF)"
            );
            acc = acc.wrapping_add(val);
            unsafe { GLOBAL.dealloc(ptr, layout) };
        }
        total_recv_c.store(acc, Ordering::Release);
    });

    // LONG-LIVED producers: spawned once, never exit mid-test → NO slot recycle.
    let producers: Vec<_> = (0..PRODUCERS)
        .map(|p| {
            let tx = tx.clone();
            let total_sent = Arc::clone(&total_sent);
            std::thread::spawn(move || {
                let mut local: u64 = 0;
                for i in 0..ALLOCS {
                    let ptr = unsafe { GLOBAL.alloc(layout) };
                    assert!(!ptr.is_null(), "alloc null");
                    let val = (p as u64)
                        .wrapping_mul(0x9E37)
                        .wrapping_add(i as u64)
                        .max(1);
                    unsafe { std::ptr::write(ptr as *mut u64, val) };
                    local = local.wrapping_add(val);
                    if tx.send((SendPtr(ptr), val)).is_err() {
                        unsafe { GLOBAL.dealloc(ptr, layout) };
                        break;
                    }
                }
                total_sent.fetch_add(local, Ordering::Relaxed);
            })
        })
        .collect();
    drop(tx);

    for h in producers {
        h.join().expect("producer aborted");
    }
    consumer.join().expect("consumer aborted");

    let sent = total_sent.load(Ordering::Acquire);
    let recv = total_recv.load(Ordering::Acquire);
    assert_eq!(sent, recv, "checksum mismatch: sent={sent} recv={recv}");
}
