//! Actual Box Drop calls through an installed allocator, including narrow
//! typed provenance. Keep this target tiny enough for focused Miri probes.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::global::tls_heap;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RoutePin};
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

struct Tiny([u8; 7]);

fn marked_free_on_owner(ptr: *mut u8) -> bool {
    let heap = tls_heap::current_for_trim().expect("Box allocation bound this thread's heap");
    // SAFETY: current_for_trim returns only this thread's still-owned slot.
    // The ephemeral shared borrow performs address-only canonical lookup and
    // reads owner metadata; it ends before any further global allocation.
    unsafe { (&*heap).dbg_is_free_for(ptr) }
}

fn assert_retired(ptr: *mut u8, pin: &RoutePin) {
    assert!(
        !pin.pending_for_test(ptr),
        "trim consumed the completed typed Drop publication"
    );
    // A released segment has no route. A retained one must have transferred
    // this exact issued block to the allocator-owned free state.
    assert!(
        RouteDirectory::global().lookup(ptr).is_none() || marked_free_on_owner(ptr),
        "typed allocation's credit was not retired"
    );
}

#[test]
fn installed_box_drop_narrow_transfer_and_reissue() {
    for round in 0..2u8 {
        let mut byte = Box::new(0x31u8 + round);
        let mut tiny = Box::new(Tiny([0x52u8 + round; 7]));
        let (byte_ptr, tiny_ptr) = {
            // These genuine narrow borrows end before the Boxes are moved.
            let narrow_byte: &mut u8 = &mut byte;
            *narrow_byte ^= 0x08;
            let narrow_tiny: &mut [u8; 7] = &mut tiny.0;
            narrow_tiny[6] ^= 0x04;
            (narrow_byte as *mut u8, narrow_tiny.as_mut_ptr())
        };
        let byte_pin = RouteDirectory::global()
            .lookup(byte_ptr)
            .expect("issued Box route");
        let tiny_pin = RouteDirectory::global()
            .lookup(tiny_ptr)
            .expect("issued tiny object route");
        assert!(matches!(
            byte_pin.kind(),
            RouteKind::Small | RouteKind::Primordial
        ));
        assert!(matches!(
            tiny_pin.kind(),
            RouteKind::Small | RouteKind::Primordial
        ));
        assert!(
            !marked_free_on_owner(byte_ptr),
            "new/reissued byte Box must own its block"
        );
        assert!(
            !marked_free_on_owner(tiny_ptr),
            "new/reissued tiny Box must own its block"
        );

        std::thread::spawn(move || {
            assert_eq!(*byte, (0x31u8 + round) ^ 0x08);
            assert_eq!(&tiny.0[..6], &[0x52u8 + round; 6]);
            assert_eq!(tiny.0[6], (0x52u8 + round) ^ 0x04);
            // Actual Box destructors invoke this binary's installed GlobalAlloc.
            // No manual dealloc, pointer reconstruction, or synthetic free.
            drop(byte);
            drop(tiny);
        })
        .join()
        .unwrap();

        GLOBAL.trim_current_thread();
        assert_retired(byte_ptr, &byte_pin);
        assert_retired(tiny_ptr, &tiny_pin);
        // The second round checks issue -> foreign Drop -> retire again after
        // the first owner cut, without assuming an allocator address policy.
    }
}

#[cfg(miri)]
#[test]
fn installed_box_drop_retires_before_terminal_producer_resumes() {
    use sefer_alloc::registry::segment_route::TerminalPublicationGate as Gate;

    struct ResumeProducer;
    impl Drop for ResumeProducer {
        fn drop(&mut self) {
            Gate::resume_producer();
        }
    }

    let mut byte = Box::new(0x70u8);
    let ptr = {
        let narrow: &mut u8 = &mut byte;
        *narrow ^= 1;
        narrow as *mut u8
    };
    let pin = RouteDirectory::global()
        .lookup(ptr)
        .expect("issued real Box route");
    assert!(!marked_free_on_owner(ptr));
    Gate::arm(ptr.addr());
    let resume = ResumeProducer;
    let producer = std::thread::spawn(move || {
        // Warm both sides of the independent static synchronization before
        // entering the actual Box Drop/GlobalAlloc dealloc frames.
        Gate::prepare_producer();
        assert_eq!(*byte, 0x71);
        drop(byte);
    });
    Gate::allow_prepared_producer();
    Gate::wait_for_publication();

    // The producer is now paused AFTER its successful terminal RMW but still
    // INSIDE publish_foreign, GlobalAlloc::dealloc, and real Box Drop. This
    // owner cut must retire the exact record before any of those frames return.
    GLOBAL.trim_current_thread();
    assert_retired(ptr, &pin);
    drop(resume);
    producer.join().unwrap();
}
