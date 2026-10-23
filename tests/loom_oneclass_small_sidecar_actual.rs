//! D1′ actual-source Loom battery for the PRODUCTION
//! `src/registry/segment_route/small_sidecar.rs` (compiled via `#[path]`,
//! reduced-shim style of `tests/loom_sidecar_bitmap_actual.rs`).
//!
//! - M-b (carve-clear via load+store): the production
//!   `clear_pending_start_bit` must be the RMW form — a concurrent
//!   legitimate publication of a NEIGHBOURING granule in the same word must
//!   survive it in every interleaving. The negative control
//!   `loom_actual_negative_carve_clear_via_load_store_loses_concurrent_bit`
//!   models the forbidden form locally; THIS model compiles the real hook,
//!   so mutating the production body to load+store goes RED here.
//! - M-h (owner start-bit issue after the pointer handoff): contract model
//!   — a producer that observed the handoff (Release/Acquire flag) must
//!   observe the start bit set (`publish == true`) in every interleaving.
#![cfg(loom)]
// The source-included production modules keep their non-modeled surface;
// loom builds have no `internals`, so several reviewed items are unused.
#![allow(dead_code)]

mod alloc_core {
    pub mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            use std::io::Write;
            let _ = std::io::stderr().write_all(msg);
            std::process::abort()
        }
    }
    pub mod os {
        // Production geometry: `small_sidecar.rs` pins SEGMENT == 4 MiB.
        pub const SEGMENT: usize = 4 * 1024 * 1024;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 16;
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
    pub mod remote_bitmap {
        // Resolved after the source-include below; uses `super::` paths.
        pub(crate) use crate::bitmap_scan::BitmapScan;
        pub(crate) use crate::sidecar_bitmap::ChunkedBank;
    }
}

// The included production files resolve `super::` at this root module and
// `crate::alloc_core::…` at the shim above.
pub(crate) use loom::sync::atomic::{AtomicU64, Ordering};

#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;
#[path = "../src/registry/segment_route/small_sidecar.rs"]
mod small_sidecar;

pub(crate) use bitmap_cut::BitmapCut;
pub(crate) use bitmap_scan::BitmapScan;

/// `small_sidecar.rs` constructs `RouteScan { inner }` (its parent-module
/// type); the shim mirrors the registry's shape.
pub struct RouteScan<'a> {
    pub(crate) inner: BitmapScan<'a>,
}

use small_sidecar::SmallSidecar;

use loom::sync::Arc;
use loom::thread;

/// Backing уничтожается после join всех readers в каждой итерации модели.
fn zeroed_sidecar() -> Box<SmallSidecar> {
    let mut backing = Box::<SmallSidecar>::new_uninit();
    // SAFETY: приватный aligned allocation; до возврата ссылок на Self нет.
    unsafe { SmallSidecar::initialize_at(backing.as_mut_ptr()) };
    // SAFETY: ready и оболочки построены; payload законно остаётся MaybeUninit.
    unsafe { backing.assume_init() }
}

/// Owner discovery over word 0 only (both modeled granules live there); a
/// bounded window keeps the loom branch budget independent of the 4 MiB
/// production geometry that `small_sidecar.rs` const-pins.
fn drain(sidecar: &SmallSidecar) -> Vec<u32> {
    let mut scan = sidecar.scan_from(1024, 0).expect("in-bounds high water");
    let mut out = Vec::new();
    while let Some(mut cut) = scan.inner.next_cut() {
        while let Some(offset) = cut.pop() {
            out.push(offset);
        }
    }
    out
}

/// M-b (actual source): the production `clear_pending_start_bit` of granule
/// 0 races a legitimate publish of NEIGHBOUR granule 1 (same word). The
/// RMW form preserves the neighbour's bit whenever its publish won the
/// race; the forbidden load+store form loses it in an interleaving → RED.
fn model_carve_clear_keeps_concurrent_neighbour_bit() {
    loom::model(move || {
        let sidecar = zeroed_sidecar();
        // SAFETY: fixture — единственный owner; другие issue/retain исключены.
        unsafe { sidecar.issue_for_test(0) };
        // SAFETY: fixture — единственный owner; другие issue/retain исключены.
        unsafe { sidecar.issue_for_test(16) };
        // Plant the stale pending bit on granule 0 BEFORE the spawn, so the
        // production clear_pending_start_bit actually takes its RMW branch
        // (without it the `if pending & mask != 0` skip makes any mutation
        // INSIDE the branch unobservable).
        assert!(sidecar.publish(0, 0));
        let published = {
            let s = &*sidecar as *const SmallSidecar;
            thread::spawn(move || {
                thread::yield_now();
                // SAFETY (model): the fixture owns the synthetic sidecar.
                unsafe { (*s).publish(16, 0) }
            })
        };
        {
            let s = &*sidecar as *const SmallSidecar;
            thread::spawn(move || {
                thread::yield_now();
                // SAFETY (model): as above.
                unsafe { (*s).clear_pending_start_bit(0) }
            })
        }
        .join()
        .unwrap();
        let published = published.join().unwrap();
        let drained = drain(&sidecar);
        if published {
            assert_eq!(
                drained,
                vec![16],
                "the neighbour's legitimate bit must survive the carve-clear \
                 exactly when its publish won the race"
            );
        } else {
            assert!(
                drained.is_empty(),
                "a false publication plants no pending bit"
            );
        }
        assert!(
            !drained.contains(&0),
            "the cleared granule's stale bit must be gone in every interleaving"
        );
    });
}

/// M-h (contract model): a start-bit issue that precedes the pointer
/// handoff (Release store of the handoff flag) is visible to every
/// producer that acquired the handoff — `publish` must return `true` in
/// every interleaving. Reordering the issue after the handoff (the M-h
/// class of mutation, PR2/S1 scope in `carve_block`) makes `publish`
/// observe the zero start bit → RED.
fn model_issue_precedes_handoff_publication() {
    loom::model(move || {
        let sidecar = zeroed_sidecar();
        let handoff = Arc::new(AtomicU64::new(0));
        let s = &*sidecar as *const SmallSidecar;
        let producer = {
            let handoff = Arc::clone(&handoff);
            thread::spawn(move || {
                while handoff.load(Ordering::Acquire) == 0 {
                    thread::yield_now();
                }
                // SAFETY (model): the fixture owns the synthetic sidecar;
                // the handoff above carried the issue to this thread.
                unsafe { (*s).publish(0, 0) }
            })
        };
        // Owner: the issue happens BEFORE the handoff flag is published
        // (the exact ordering contract of `carve_block`'s `issue_small`).
        // SAFETY: fixture — единственный owner; другие issue/retain исключены.
        unsafe { sidecar.issue_for_test(0) };
        handoff.store(1, Ordering::Release);
        assert!(
            producer.join().unwrap(),
            "M-h RED: a producer that acquired the handoff observed the \
             zero start bit — the issue was reordered past the handoff"
        );
    });
}

/// Single harness entry: loom models must never run concurrently in
/// different libtest pool threads, so both models are sequenced here.
#[test]
fn loom_oneclass_small_sidecar_actual_battery() {
    model_carve_clear_keeps_concurrent_neighbour_bit();
    model_issue_precedes_handoff_publication();
}
