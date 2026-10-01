//! A valid cross-thread Large free must not increment the dropped-free counter.
//! The independent route descriptor provides the negative span witness without
//! violating `GlobalAlloc::dealloc`'s exact-layout contract. The old test sent
//! a wrong `Layout` to `dealloc` and then read and re-freed the same pointer;
//! neither action is licensed by the unsafe trait contract.

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

#[test]
fn checked_foreign_span_and_valid_cross_thread_free() {
    let _guard = SerialGuard::acquire();
    const SIZE: usize = 2 * 1024 * 1024;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    // SAFETY: valid nonzero Layout; the returned pointer is checked below.
    let p = unsafe { GLOBAL.alloc(layout) };
    assert!(!p.is_null(), "large allocation failed");
    let route = RouteDirectory::global()
        .lookup(p)
        .expect("routed allocation");
    assert_eq!(route.kind(), RouteKind::Large);
    assert!(route.contains_payload(p, SIZE));
    // Beyond any reserved span: `large-reserved-capacity` reserves up to 4x the
    // payload (capped at 64 MiB), so a 4x probe would still be inside the route.
    assert!(
        !route.contains_payload(p, SIZE * 64),
        "oversized span must fail the checked descriptor oracle"
    );

    // SAFETY: `p` is live and valid for its exact SIZE-byte layout.
    unsafe { p.write(0xCC) };
    // SAFETY: the byte just written is still owned by this thread.
    assert_eq!(unsafe { p.read() }, 0xCC);
    let before = GLOBAL.stats().foreign_or_unroutable_frees;
    let address = p.expose_provenance();
    thread::spawn(move || {
        // SAFETY: the unique live allocation and its exact original Layout
        // were transferred to this thread; this is its only free.
        unsafe { GLOBAL.dealloc(std::ptr::with_exposed_provenance_mut(address), layout) };
    })
    .join()
    .expect("foreign free thread panicked");

    assert!(route.pending_for_test(p), "foreign free must publish once");
    GLOBAL.trim_current_thread();
    assert!(
        !route.pending_for_test(p),
        "owner trim must consume the free"
    );
    assert!(RouteDirectory::global().lookup(p).is_none());
    assert_eq!(
        GLOBAL.stats().foreign_or_unroutable_frees,
        before,
        "a valid free is not a dropped free"
    );

    // SAFETY: exact nonzero Layout, checked for null and freed once.
    let next = unsafe { GLOBAL.alloc(layout) };
    assert!(!next.is_null(), "heap unusable after cross-thread free");
    // SAFETY: `next` is the unique live allocation returned just above.
    unsafe { GLOBAL.dealloc(next, layout) };
}
