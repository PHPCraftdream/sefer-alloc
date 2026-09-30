#[cfg(all(
    miri,
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]
mod probe {
    use sefer_alloc::global::tls_heap;
    use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RoutePin};
    use sefer_alloc::SeferAlloc;

    #[global_allocator]
    static GLOBAL: SeferAlloc = SeferAlloc::new();

    struct Tiny([u8; 7]);

    fn marked_free_on_owner(ptr: *mut u8) -> bool {
        let heap = tls_heap::current_for_trim().expect("Box bound the owner heap");
        // SAFETY: the owner heap remains live; this diagnostic borrow ends here.
        unsafe { (&*heap).dbg_is_free_for(ptr) }
    }

    fn assert_retired(ptr: *mut u8, pin: &RoutePin) {
        assert!(!pin.pending_for_test(ptr));
        assert!(RouteDirectory::global().lookup(ptr).is_none() || marked_free_on_owner(ptr));
    }

    pub fn run() {
        for round in 0..2u8 {
            let mut byte = Box::new(0x31u8 + round);
            let mut tiny = Box::new(Tiny([0x52u8 + round; 7]));
            let (byte_ptr, tiny_ptr) = {
                let narrow_byte: &mut u8 = &mut byte;
                *narrow_byte ^= 0x08;
                let narrow_tiny: &mut [u8; 7] = &mut tiny.0;
                narrow_tiny[6] ^= 0x04;
                (narrow_byte as *mut u8, narrow_tiny.as_mut_ptr())
            };
            let byte_pin = RouteDirectory::global()
                .lookup(byte_ptr)
                .expect("issued byte route");
            let tiny_pin = RouteDirectory::global()
                .lookup(tiny_ptr)
                .expect("issued tiny route");
            assert!(matches!(
                byte_pin.kind(),
                RouteKind::Small | RouteKind::Primordial
            ));
            assert!(matches!(
                tiny_pin.kind(),
                RouteKind::Small | RouteKind::Primordial
            ));
            assert!(!marked_free_on_owner(byte_ptr));
            assert!(!marked_free_on_owner(tiny_ptr));

            std::thread::spawn(move || {
                assert_eq!(*byte, (0x31u8 + round) ^ 0x08);
                assert_eq!(&tiny.0[..6], &[0x52u8 + round; 6]);
                assert_eq!(tiny.0[6], (0x52u8 + round) ^ 0x04);
                drop(byte);
                drop(tiny);
            })
            .join()
            .unwrap();

            GLOBAL.trim_current_thread();
            assert_retired(byte_ptr, &byte_pin);
            assert_retired(tiny_ptr, &tiny_pin);
        }
    }
}

#[cfg(all(
    miri,
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]
fn main() {
    probe::run();
}

#[cfg(not(all(
    miri,
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
)))]
fn main() {}
