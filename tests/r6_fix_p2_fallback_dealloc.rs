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
#[cfg(feature = "alloc-decommit")]
use std::sync::atomic::AtomicU32;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use sefer_alloc::alloc_core::segment_header::OWNER_ID_FALLBACK;
use sefer_alloc::global::dbg_fallback_lock_acquisitions;
use sefer_alloc::registry::HeapCore;
use sefer_alloc::SeferAlloc;

const CASE_ENV: &str = "SEFER_R6_02_FALLBACK_CASE";
const CHILD_TIMEOUT: Duration = Duration::from_secs(15);

static TEARDOWN_PTR: AtomicUsize = AtomicUsize::new(0);
static TEARDOWN_FREE_RAN: AtomicBool = AtomicBool::new(false);
static TEARDOWN_LOCK_BEFORE: AtomicU64 = AtomicU64::new(0);
static TEARDOWN_LOCK_AFTER: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "alloc-decommit")]
static TEARDOWN_LIVE_BEFORE_FREE: AtomicU32 = AtomicU32::new(0);

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
        #[cfg(feature = "alloc-decommit")]
        if !ptr.is_null() {
            let live = HeapCore::dbg_with_fallback_for_test(|heap| {
                heap.dbg_live_count_for(ptr).unwrap_or(0)
            })
            .unwrap_or(0);
            TEARDOWN_LIVE_BEFORE_FREE.store(live, Ordering::Release);
        }
        TEARDOWN_PTR.store(ptr as usize, Ordering::Release);
    }
}

impl Drop for FreeAfterFallbackAlloc {
    fn drop(&mut self) {
        let ptr = TEARDOWN_PTR.load(Ordering::Acquire) as *mut u8;
        if !ptr.is_null() {
            TEARDOWN_LOCK_BEFORE.store(dbg_fallback_lock_acquisitions(), Ordering::Relaxed);
            // SAFETY: the preceding TLS destructor allocated this live block
            // with the same allocator and layout, then published it here.
            unsafe { SeferAlloc::new().dealloc(ptr, layout()) };
            TEARDOWN_LOCK_AFTER.store(dbg_fallback_lock_acquisitions(), Ordering::Relaxed);
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
    let lock_delta = TEARDOWN_LOCK_AFTER
        .load(Ordering::Relaxed)
        .wrapping_sub(TEARDOWN_LOCK_BEFORE.load(Ordering::Relaxed));
    assert_eq!(
        lock_delta, 1,
        "the teardown dealloc must acquire the fallback lock exactly once"
    );

    let free = HeapCore::dbg_with_fallback_for_test(|heap| {
        assert_eq!(heap.dbg_owner_id_for(ptr), Some(OWNER_ID_FALLBACK));
        heap.dbg_is_free_for(ptr)
    })
    .expect("fallback remains initialized");

    #[cfg(feature = "fastbin")]
    {
        let (class, in_magazine) = HeapCore::dbg_with_fallback_for_test(|heap| {
            let class = heap.dbg_class_for(layout()).expect("small class");
            (class, heap.dbg_tcache_contains(class, ptr))
        })
        .expect("fallback remains initialized");
        assert!(
            in_magazine,
            "direct fallback dealloc did not park the block in its class magazine: \
             class={class}, free={free}"
        );
        // Magazine-resident blocks remain allocated in the bitmap and live
        // count; the subsequent alloc proves immediate reuse.
        assert!(!free, "magazine resident block remains allocated in bitmap");
    }

    #[cfg(not(feature = "fastbin"))]
    {
        assert!(free, "non-fastbin free must return the block to its bitmap");
    }

    #[cfg(feature = "alloc-decommit")]
    {
        let live_after = HeapCore::dbg_with_fallback_for_test(|heap| heap.dbg_live_count_for(ptr))
            .expect("fallback remains initialized");
        let live_before = TEARDOWN_LIVE_BEFORE_FREE.load(Ordering::Acquire);
        assert!(
            live_before > 0,
            "allocation must be live before teardown free"
        );

        #[cfg(feature = "fastbin")]
        assert_eq!(
            live_after,
            Some(live_before),
            "magazine parking must leave the live count unchanged"
        );

        #[cfg(not(feature = "fastbin"))]
        assert_eq!(
            live_after,
            Some(live_before - 1),
            "non-fastbin free must decrement live count exactly once"
        );
    }

    let reused = HeapCore::dbg_with_fallback_for_test(|heap| {
        let reused = heap.alloc(layout());
        // SAFETY: `reused` came from this heap with this layout and is live.
        unsafe { heap.dealloc(reused, layout()) };
        reused
    })
    .expect("fallback reallocation");
    assert_eq!(
        reused, ptr,
        "the directly reclaimed block must be immediately reusable"
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
            heap.dbg_drain_sidecar_ingress();
            heap.dbg_is_free_for(ptr)
        }),
        Some(true),
        "busy-lock remote free must remain reclaimable"
    );
}
