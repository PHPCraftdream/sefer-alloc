//! Strict-trim payload-start regression using joined cross-thread frees.
#![cfg(all(
    feature = "production",
    feature = "bench-internals",
    feature = "internals"
))]
use sefer_alloc::SeferAlloc;
use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;
use std::thread;

#[test]
fn strict_trim_reclaims_and_visits_exact_payload_word_sum() {
    // 32-byte requests classify Small; <=16-byte exact-object-proto bypasses HeapCore.
    let layout = Layout::from_size_align(32, 8).unwrap();
    let (addresses_tx, addresses_rx) = mpsc::channel::<Vec<usize>>();
    let (trim_tx, trim_rx) = mpsc::channel::<(u64, u64, u64, u64)>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let owner = thread::spawn(move || {
        let allocator = SeferAlloc::new();
        let mut addresses = Vec::with_capacity(20);
        for _ in 0..20 {
            // SAFETY: valid non-zero layout; live blocks remain owned until handoff.
            let ptr = unsafe { allocator.alloc(layout) };
            assert!(!ptr.is_null());
            addresses.push(ptr.expose_provenance());
        }
        addresses_tx.send(addresses).unwrap();
        release_rx.recv().unwrap();
        let before = allocator.dbg_prf01_strict_words_visited();
        allocator.trim_current_thread();
        let after = allocator.dbg_prf01_strict_words_visited();
        let records = after.1 - before.1;
        trim_tx
            .send((after.0 - before.0, before.2, before.3, records))
            .unwrap();
    });
    let addresses = addresses_rx.recv().unwrap();
    let producer = thread::spawn(move || {
        let allocator = SeferAlloc::new();
        for address in addresses {
            let ptr = core::ptr::with_exposed_provenance_mut::<u8>(address);
            // SAFETY: address denotes a live handed-off allocation of this layout.
            unsafe { allocator.dealloc(ptr, layout) };
        }
    });
    producer.join().unwrap();
    release_tx.send(()).unwrap();
    let (actual, end, start, records) = trim_rx.recv().unwrap();
    // Scanning from word 0 makes actual == end, so this fails when start > 0.
    assert_eq!(actual, end - start);
    assert!(
        start > 0 && end > start,
        "legacy word-zero prefix is nonempty"
    );
    assert_eq!(records, 20);
    owner.join().unwrap();
}
