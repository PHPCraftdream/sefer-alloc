//! R6-02: fallback owner stamps route frees without depending on TLS state.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals",
    not(miri)
))]

use std::alloc::{GlobalAlloc, Layout};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use sefer_alloc::alloc_core::segment_header::OWNER_ID_FALLBACK;
use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;

const CASE_ENV: &str = "SEFER_R6_02_FALLBACK_CASE";
const CHILD_TIMEOUT: Duration = Duration::from_secs(15);

static TEARDOWN_PTR: AtomicUsize = AtomicUsize::new(0);
static TEARDOWN_FREE_RAN: AtomicBool = AtomicBool::new(false);

struct FreeAfterFallbackAlloc;
struct FallbackAllocAfterGuard;

thread_local! {
    static FREE_AFTER_ALLOC: FreeAfterFallbackAlloc = const { FreeAfterFallbackAlloc };
    static ALLOC_AFTER_GUARD: FallbackAllocAfterGuard = const { FallbackAllocAfterGuard };
}

fn layout() -> Layout {
    Layout::from_size_align(64, 8).expect("valid layout")
}

impl Drop for FallbackAllocAfterGuard {
    fn drop(&mut self) {
        // GUARD is registered after both test keys, so it has already marked
        // LOCAL TORN and recycled the registry slot before this destructor.
        // SAFETY: the layout is valid and non-zero.
        let ptr = unsafe { SeferAlloc::new().alloc(layout()) };
        TEARDOWN_PTR.store(ptr as usize, Ordering::Release);
    }
}

impl Drop for FreeAfterFallbackAlloc {
    fn drop(&mut self) {
        let ptr = TEARDOWN_PTR.load(Ordering::Acquire) as *mut u8;
        if !ptr.is_null() {
            // SAFETY: the preceding TLS destructor allocated this live block
            // with the same allocator and layout, then published it here.
            unsafe { SeferAlloc::new().dealloc(ptr, layout()) };
            TEARDOWN_FREE_RAN.store(true, Ordering::Release);
        }
    }
}

fn run_child(case: &str) {
    let mut child = Command::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("fallback_child")
        .arg("--ignored")
        .arg("--nocapture")
        .env(CASE_ENV, case)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn isolated regression child");
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll regression child") {
            let output = child.wait_with_output().expect("collect child output");
            assert!(
                status.success(),
                "case {case} failed: status={status}\nstdout={}\nstderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        if started.elapsed() >= CHILD_TIMEOUT {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect timed-out child");
            panic!(
                "case {case} deadlocked past {CHILD_TIMEOUT:?}\nstdout={}\nstderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn actual_tls_teardown_fallback_free_is_reclaimed() {
    run_child("teardown");
}

#[test]
fn busy_fallback_lock_uses_remote_route_without_losing_free() {
    run_child("busy");
}

#[test]
#[ignore = "run only in an isolated child process"]
fn fallback_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    match case.as_str() {
        "teardown" => real_teardown_case(),
        "busy" => busy_lock_case(),
        other => panic!("unknown child case: {other}"),
    }
}

fn real_teardown_case() {
    std::thread::spawn(|| {
        // Register both Drop keys before allocator binding. At thread exit the
        // allocator GUARD drops first, then alloc-after-GUARD, then free.
        FREE_AFTER_ALLOC.with(|_| ());
        ALLOC_AFTER_GUARD.with(|_| ());
        let allocator = SeferAlloc::new();
        // SAFETY: the layout is valid and non-zero.
        let own = unsafe { allocator.alloc(layout()) };
        assert!(!own.is_null(), "registry allocation failed");
        // SAFETY: `own` is live and was returned above with this layout.
        unsafe { allocator.dealloc(own, layout()) };
    })
    .join()
    .expect("teardown worker panicked");

    assert!(TEARDOWN_FREE_RAN.load(Ordering::Acquire));
    let ptr = TEARDOWN_PTR.load(Ordering::Acquire) as *mut u8;
    assert!(
        !ptr.is_null(),
        "teardown allocation did not publish a block"
    );
    assert_eq!(
        HeapCore::dbg_with_fallback_for_test(|heap| {
            assert_eq!(heap.dbg_owner_id_for(ptr), Some(OWNER_ID_FALLBACK));
            heap.dbg_is_free_for(ptr)
        }),
        Some(true),
        "the final TLS destructor must reclaim the fallback block directly"
    );
}

fn busy_lock_case() {
    let ptr = HeapCore::dbg_with_fallback_for_test(|heap| {
        let ptr = heap.alloc(layout());
        assert!(!ptr.is_null());
        assert_eq!(heap.dbg_owner_id_for(ptr), Some(OWNER_ID_FALLBACK));
        ptr
    })
    .expect("fallback init");

    // This test hook re-enters dealloc while the production fallback lock is
    // held. try_with_heap must fail immediately; the ring route retains the
    // free until the subsequent owner drain.
    // SAFETY: `ptr` is a live fallback allocation with the stated layout.
    unsafe { SeferAlloc::dbg_dealloc_while_fallback_lock_held(ptr, layout()) };
    assert_eq!(
        HeapCore::dbg_with_fallback_for_test(|heap| {
            assert!(!heap.dbg_is_free_for(ptr));
            heap.dbg_drain_all_rings();
            heap.dbg_is_free_for(ptr)
        }),
        Some(true),
        "busy-lock remote free must remain reclaimable"
    );
}
