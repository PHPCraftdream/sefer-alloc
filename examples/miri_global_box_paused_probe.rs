#[cfg(all(
    miri,
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]
mod probe {
    use sefer_alloc::global::tls_heap;
    use sefer_alloc::registry::segment_route::{
        RouteDirectory, RoutePin, TerminalPublicationGate as Gate,
    };
    use sefer_alloc::SeferAlloc;

    #[global_allocator]
    static GLOBAL: SeferAlloc = SeferAlloc::new();

    struct ResumeProducer;

    impl Drop for ResumeProducer {
        fn drop(&mut self) {
            Gate::resume_producer();
        }
    }

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
        let mut byte = Box::new(0x70u8);
        let ptr = {
            let narrow: &mut u8 = &mut byte;
            *narrow ^= 1;
            narrow as *mut u8
        };
        let pin = RouteDirectory::global()
            .lookup(ptr)
            .expect("issued Box route");
        assert!(!marked_free_on_owner(ptr));
        Gate::arm(ptr.addr());
        let resume = ResumeProducer;
        let producer = std::thread::spawn(move || {
            Gate::prepare_producer();
            assert_eq!(*byte, 0x71);
            drop(byte);
        });
        Gate::allow_prepared_producer();
        Gate::wait_for_publication();
        GLOBAL.trim_current_thread();
        assert_retired(ptr, &pin);
        drop(resume);
        producer.join().unwrap();
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
