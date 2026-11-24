#![deny(unsafe_code)]

use std::alloc::{Allocator, Layout};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::process::Command;
use std::thread;

use sefer_alloc::SeferAlloc;
use sefer_alloc_api::SeferAllocator;

fn allocator() -> SeferAllocator {
    SeferAllocator::new()
}

#[test]
#[allow(unsafe_code)]
fn alignment_zst_and_collection_lifetimes() {
    let a = allocator();
    let layout = Layout::from_size_align(73, 4096).unwrap();
    let block = a.allocate(layout).unwrap();
    assert_eq!(block.len(), layout.size());
    assert_eq!(block.as_ptr().cast::<u8>().addr() % 4096, 0);
    // SAFETY: allocated with this allocator and exact layout, freed once.
    unsafe { a.deallocate(block.cast::<u8>(), layout) };

    let zst = Layout::from_size_align(0, 4096).unwrap();
    let zero = a.allocate(zst).unwrap();
    assert_ne!(zero.cast::<u8>().as_ptr().addr(), 0);
    assert_eq!(zero.cast::<u8>().as_ptr().addr() % 4096, 0);

    let mut values = Vec::new_in(allocator());
    values.extend(0..32);
    assert_eq!(values[31], 31);
    drop(values);
    let values = Vec::<u64, _>::with_capacity_in(12, allocator());
    assert!(values.capacity() >= 12);
    drop(values);
    drop(Box::new_in(String::from("allocated"), allocator()));
    let zst_boxes: Vec<_, _> = (0..4).map(|_| Box::new_in((), allocator())).collect();
    drop(zst_boxes);
}

#[test]
#[allow(unsafe_code)]
fn grow_shrink_change_alignment_and_zero_tail() {
    let a = allocator();
    let old = Layout::from_size_align(96, 8).unwrap();
    let grown_layout = Layout::from_size_align(2048, 4096).unwrap();
    let initial = a.allocate(old).unwrap();
    let p = initial.cast::<u8>();
    // SAFETY: p denotes a live 96-byte block allocated by a with old layout.
    unsafe { p.as_ptr().write_bytes(0xA5, old.size()) };
    // SAFETY: p/old are the live allocation pair; replacement has greater size.
    let grown = unsafe { a.grow_zeroed(p, old, grown_layout) }.unwrap();
    assert_eq!(grown.len(), grown_layout.size());
    assert_eq!(grown.cast::<u8>().as_ptr().addr() % grown_layout.align(), 0);
    // SAFETY: returned slice covers the full allocation.
    let bytes = unsafe { std::slice::from_raw_parts(grown.cast::<u8>().as_ptr(), grown.len()) };
    assert!(bytes[..old.size()].iter().all(|b| *b == 0xA5));
    assert!(bytes[old.size()..].iter().all(|b| *b == 0));

    let p = grown.cast::<u8>();
    let smaller = Layout::from_size_align(31, 64).unwrap();
    // SAFETY: p/grown_layout are the live allocation pair.
    let shrunk = unsafe { a.shrink(p, grown_layout, smaller) }.unwrap();
    assert_eq!(shrunk.cast::<u8>().as_ptr().addr() % smaller.align(), 0);
    // SAFETY: returned slice covers the live 31-byte allocation.
    let bytes = unsafe { std::slice::from_raw_parts(shrunk.cast::<u8>().as_ptr(), smaller.size()) };
    assert!(bytes.iter().all(|b| *b == 0xA5));
    // SAFETY: returned pointer and layout form the live allocation pair.
    unsafe { a.deallocate(shrunk.cast::<u8>(), smaller) };

    let old_zero = Layout::from_size_align(16, 16).unwrap();
    let old_block = a.allocate(old_zero).unwrap();
    let old_ptr = old_block.cast::<u8>();
    // SAFETY: old_ptr denotes this live allocation.
    unsafe { old_ptr.as_ptr().write_bytes(0x31, old_zero.size()) };
    // SAFETY: live old allocation and valid zero-sized destination layout.
    let empty =
        unsafe { a.shrink(old_ptr, old_zero, Layout::from_size_align(0, 64).unwrap()) }.unwrap();
    assert_eq!(empty.len(), 0);
    assert_eq!(empty.cast::<u8>().as_ptr().addr() % 64, 0);
}

#[test]
#[allow(unsafe_code)]
fn zeroed_allocation_clears_reused_requested_bytes() {
    let a = allocator();
    let layout = Layout::from_size_align(256, 16).unwrap();
    for _ in 0..32 {
        let dirty = a.allocate(layout).unwrap();
        // SAFETY: dirty block has requested layout and is exclusively held here.
        unsafe { dirty.cast::<u8>().as_ptr().write_bytes(0xCC, layout.size()) };
        let dirty_ptr = dirty.cast::<u8>();
        // SAFETY: this is the live allocation pair just created.
        unsafe { a.deallocate(dirty_ptr, layout) };
        let clean = a.allocate_zeroed(layout).unwrap();
        assert_eq!(clean.cast::<u8>(), dirty.cast::<u8>());
        // SAFETY: clean block is valid for the requested length.
        let bytes =
            unsafe { std::slice::from_raw_parts(clean.cast::<u8>().as_ptr(), layout.size()) };
        assert!(bytes.iter().all(|b| *b == 0));
        // SAFETY: clean is the live allocation with layout.
        unsafe { a.deallocate(clean.cast::<u8>(), layout) };
    }
}

#[test]
#[allow(unsafe_code)]
fn grow_zeroed_clears_reused_same_class_extension() {
    let a = allocator();
    let old = Layout::from_size_align(48, 16).unwrap();
    let new = Layout::from_size_align(96, 16).unwrap();
    let source = a.allocate(old).unwrap();
    let source_ptr = source.cast::<u8>();
    // SAFETY: source is a live allocation of exactly old.size() bytes.
    unsafe { source_ptr.as_ptr().write_bytes(0xA5, old.size()) };
    let dirty_target = a.allocate(new).unwrap();
    // SAFETY: dirty_target is a live allocation of exactly new.size() bytes.
    unsafe {
        dirty_target
            .cast::<u8>()
            .as_ptr()
            .write_bytes(0xCC, new.size())
    };
    let dirty_target_ptr = dirty_target.cast::<u8>();
    // SAFETY: dirty_target is freed while source remains live.
    unsafe { a.deallocate(dirty_target_ptr, new) };
    // SAFETY: source remains live and new is a valid larger layout.
    let grown = unsafe { a.grow_zeroed(source_ptr, old, new) }.unwrap();
    assert_eq!(grown.cast::<u8>(), dirty_target.cast::<u8>());
    // SAFETY: grown covers the full new allocation.
    let bytes = unsafe { std::slice::from_raw_parts(grown.cast::<u8>().as_ptr(), new.size()) };
    assert!(bytes[..old.size()].iter().all(|byte| *byte == 0xA5));
    assert!(bytes[old.size()..].iter().all(|byte| *byte == 0));
    // SAFETY: grown is the live allocation returned with new layout.
    unsafe { a.deallocate(grown.cast::<u8>(), new) };
}

#[test]
fn cross_thread_drop_after_owner_exit_reclaims_large_block() {
    if std::env::var_os("SEFER_ALLOC_API_XTHREAD_CHILD").is_some() {
        cross_thread_child();
        return;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("cross_thread_drop_after_owner_exit_reclaims_large_block")
        .arg("--nocapture")
        .env("SEFER_ALLOC_API_XTHREAD_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("SEFER_ALLOC_API_XTHREAD_RECLAIMED"),
        "child omitted reclamation marker (possibly zero tests selected): {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[allow(unsafe_code)]
fn cross_thread_child() {
    let global = SeferAlloc::new();
    let before = global.stats().large_xthread_reclaimed;
    let (tx, rx) = std::sync::mpsc::channel();
    let producer = thread::spawn(move || {
        let mut values = Vec::with_capacity_in(1024 * 1024, allocator());
        values.resize(1024 * 1024, 7u8);
        tx.send(values.into_boxed_slice()).unwrap();
    });
    let block = rx.recv().unwrap();
    producer.join().unwrap();
    drop(block);
    let a = allocator();
    let drain_layout = Layout::from_size_align(1024 * 1024 + 1, 8).unwrap();
    let drain = a.allocate(drain_layout).unwrap();
    // SAFETY: drain was allocated by a with drain_layout.
    unsafe { a.deallocate(drain.cast::<u8>(), drain_layout) };
    assert!(
        global
            .stats()
            .large_xthread_reclaimed
            .saturating_sub(before)
            >= 1
    );
    println!("SEFER_ALLOC_API_XTHREAD_RECLAIMED");
}

#[cfg(all(feature = "panic-test", target_os = "linux"))]
#[test]
fn panic_boundary_child() {
    if std::env::var_os("SEFER_ALLOC_API_PANIC_CHILD").is_some() {
        eprintln!("panic-boundary-probe-entered");
        sefer_alloc_api::panic_boundary_probe();
    }
}

#[cfg(all(feature = "panic-test", target_os = "linux"))]
#[test]
fn panic_crossing_trampoline_aborts_child_process() {
    let output = Command::new(std::env::current_exe().unwrap())
        // `--exact` prevents libtest from selecting the parent test too; both
        // names start with `panic_`, so a prefix filter would recurse.
        .arg("--exact")
        .arg("panic_boundary_child")
        .arg("--nocapture")
        .env("SEFER_ALLOC_API_PANIC_CHILD", "1")
        .output()
        .unwrap();
    // Linux's public ABI assigns SIGABRT the value 6 (asm-generic and x86).
    const SIGABRT: i32 = 6;
    assert_eq!(output.status.signal(), Some(SIGABRT), "{output:?}");
    assert_eq!(output.status.code(), None, "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("panic-boundary-probe-entered"), "{stderr}");
    assert!(
        stderr.lines().any(|l| l == "probe") && stderr.contains("panicked at"),
        "{stderr}"
    );
    assert!(
        stderr.contains("panic in a function that cannot unwind"),
        "{stderr}"
    );
}
