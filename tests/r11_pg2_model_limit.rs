//! PG-2 MODEL-LIMIT cells: paused-producer witnesses for (i) owner
//! trim/release of a segment whose last live block the producer freed,
//! (ii) owner reissue of the producer's bytes while its by-value Drop frame
//! is paused, (iii) Large reservation release/cache at pause. Installed
//! `SeferAlloc`, genuine `Box<T>`, harness-free, one scenario per process,
//! selector = first argument. Expected RED under Miri SB/TB with the
//! diagnostic at the reissue/release site (post-spike outcome is recorded,
//! not assumed). Gate-only machinery is miri-only; the target compiles
//! empty without `miri`.

#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();

#[cfg(miri)]
use sefer_alloc::registry::segment_route::{RouteDirectory, TerminalPublicationGate as Gate};

#[cfg(miri)]
struct ResumeProducer;
#[cfg(miri)]
impl Drop for ResumeProducer {
    fn drop(&mut self) {
        Gate::resume_producer();
    }
}

#[cfg(miri)]
/// Arm the gate on `ptr`, run `producer_body` on a second thread, and return
/// once the producer is paused after its terminal publication (inside the
/// `Box` Drop/dealloc frames). The caller MUST drop the returned guard to
/// resume the producer, and MUST join the thread handle.
fn pause_producer_after_publication(
    ptr: *mut u8,
    producer_body: impl FnOnce() + Send + 'static,
) -> (ResumeProducer, std::thread::JoinHandle<()>) {
    Gate::arm(ptr.addr());
    let resume = ResumeProducer;
    let producer = std::thread::spawn(move || {
        Gate::prepare_producer();
        producer_body();
    });
    Gate::allow_prepared_producer();
    Gate::wait_for_publication();
    (resume, producer)
}

/// (i) The producer's Box is retired at pause (its publication pending); the
/// owner trims (draining the publication and, if the segment empties,
/// recycling/releasing it) and then churns + trims again to force segment
/// pool/release pressure.
#[cfg(miri)]
fn run_seg_release() {
    let mut byte = Box::new(0x70u8);
    let ptr: *mut u8 = &mut *byte;
    let pin = RouteDirectory::global()
        .lookup(ptr)
        .expect("issued real Box route");
    let (resume, producer) = pause_producer_after_publication(ptr, move || {
        assert_eq!(*{ byte }, 0x70);
    });
    assert!(pin.pending_for_test(ptr), "pause after terminal publication");
    GLOBAL.trim_current_thread();
    // Segment pool/release pressure after the retire.
    for i in 0..64u8 {
        let filler = Box::new(i);
        drop(filler);
    }
    GLOBAL.trim_current_thread();
    drop(resume);
    producer.join().unwrap();
    println!("[r11_pg2_model_limit] COMPLETE seg-release");
}

/// (ii) Owner reissue: after retiring the paused producer's publication, the
/// owner allocates fresh objects until one lands on the same bytes, then
/// writes through it.
#[cfg(miri)]
fn run_reissue() {
    let mut byte = Box::new(0x70u8);
    let ptr: *mut u8 = &mut *byte;
    let pin = RouteDirectory::global()
        .lookup(ptr)
        .expect("issued real Box route");
    let (resume, producer) = pause_producer_after_publication(ptr, move || {
        assert_eq!(*{ byte }, 0x70);
    });
    assert!(pin.pending_for_test(ptr), "pause after terminal publication");
    GLOBAL.trim_current_thread();
    let mut reused = false;
    // Keep every probe alive: recycling the same few tcache slots would
    // never reach the retired block deep in the owner's free structures.
    let mut kept: Vec<Box<u8>> = Vec::new();
    for i in 0..255u8 {
        let mut c = Box::new(i);
        if std::ptr::from_ref(&*c) == ptr {
            reused = true;
            let cm: &mut u8 = &mut c;
            *cm ^= 0xFF;
            kept.push(c);
            break;
        }
        kept.push(c);
    }
    assert!(reused, "owner never reissued the producer's bytes");
    drop(resume);
    producer.join().unwrap();
    println!("[r11_pg2_model_limit] COMPLETE reissue (reused=true)");
}

/// (iii) Large reservation: a boxed large slice is dropped by the paused
/// producer; the owner trims, releasing or caching the Large reservation.
#[cfg(miri)]
fn run_large() {
    let big = std::vec![0x11u8; 1 << 20].into_boxed_slice();
    let ptr: *mut u8 = big.as_ptr() as *mut u8;
    let pin = RouteDirectory::global()
        .lookup(ptr)
        .expect("issued real large Box route");
    let (resume, producer) = pause_producer_after_publication(ptr, move || {
        assert_eq!(big[0], 0x11);
        drop(big);
    });
    assert!(pin.pending_for_test(ptr), "pause after terminal publication");
    GLOBAL.trim_current_thread();
    // Force Large cache pressure after the retire.
    for i in 0..4usize {
        let filler = std::vec![i as u8; 1 << 20].into_boxed_slice();
        drop(filler);
    }
    GLOBAL.trim_current_thread();
    drop(resume);
    producer.join().unwrap();
    println!("[r11_pg2_model_limit] COMPLETE large");
}

fn main() {
    let mut args = std::env::args_os();
    let _program = args.next();
    let selected = args.next();
    if args.next().is_some() {
        std::process::exit(2);
    }
    match selected.as_deref().and_then(|s| s.to_str()) {
        #[cfg(miri)]
        Some("seg-release") => run_seg_release(),
        #[cfg(miri)]
        Some("reissue") => run_reissue(),
        #[cfg(miri)]
        Some("large") => run_large(),
        _ => std::process::exit(2),
    }
}
