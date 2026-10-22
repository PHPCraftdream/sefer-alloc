//! D1′ phase-1a actual-source Loom model: start-bit set (owner issue) ↔
//! producer publish ↔ owner drain, on the REAL `sidecar_bitmap.rs` /
//! `bitmap_scan.rs` / `bitmap_cut.rs` (same source-include shape as
//! `tests/loom_sidecar_bitmap_actual.rs`, reduced 2-word geometry).
//!
//! Modeled protocol:
//! - OWNER: `SidecarBitmap::issue(off)` sets the granule's start bit at
//!   exactly the leaf-issue point (single-writer `load(Acquire)` →
//!   `store(old | mask, Release)`).
//! - PRODUCER: `SidecarBitmap::publish(off)` — payload/alignment filter,
//!   then the Acquire start-bit check, then the terminal `fetch_or(AcqRel)`.
//! - OWNER: the discovery cut (`BitmapScan::next_cut` → `BitmapCut::pop`,
//!   offset only).
//!
//! Properties checked over ALL interleavings:
//! - (P1 delivery) a publication that returned `true` is drained exactly
//!   once (never lost, never duplicated);
//! - (P2 no phantom) a publication that returned `false` (producer observed
//!   the zero start bit — including a producer PAUSED before its RMW while
//!   the owner issues) plants NO pending bit: the owner's drain finds
//!   nothing and a later legitimate issue+publish still works;
//! - (P3 garbage) a garbage free of a NEVER-ISSUED granule is rejected in
//!   every interleaving with the owner's issue of a DIFFERENT granule — the
//!   paused-producer window (onclass rush 1) cannot turn it into a reclaim;
//! - (P4 no phantom publication from the start-bit write) the owner's
//!   start-bit store itself never creates a pending bit.
#![cfg(loom)]

mod alloc_core {
    pub mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            use std::io::Write;
            let _ = std::io::stderr().write_all(msg);
            std::process::abort()
        }
    }
    pub mod os {
        /// 2 pending words at `MIN_BLOCK = 16`.
        pub const SEGMENT: usize = 2 * 64 * 16;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 16;
        pub const SMALL_CLASS_COUNT: usize = 8;
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
}

pub(crate) use loom::sync::atomic::{AtomicPtr, AtomicU64, AtomicU8, Ordering};

#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;

mod remote_bitmap {
    pub(crate) use super::bitmap_cut::BitmapCut;
    pub(crate) use super::bitmap_scan::BitmapScan;
    pub(crate) use super::sidecar_bitmap::SidecarBitmap;
}

// The included sources resolve `BitmapCut`/`BitmapScan`/`ClassLeaves` via
// `super::` — re-export them at this root (same shape as the pin-cache
// actual-source model's registry shim).
pub(crate) use remote_bitmap::{BitmapCut, BitmapScan};

use remote_bitmap::SidecarBitmap;

use loom::sync::Arc;
use loom::thread;

const HIGH_WATER: usize = 2 * 64 * 16;

struct Backing {
    words: Vec<AtomicU64>,
    starts: Vec<AtomicU64>,
}

impl Backing {
    fn new() -> Self {
        Self {
            words: (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
            // D1' phase-1a start bank, zero-initialized.
            starts: (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
        }
    }

    fn bitmap(&self) -> SidecarBitmap<'_> {
        // payload_start 0: the whole (reduced) segment is payload here.
        SidecarBitmap::from_pending(&self.words, &self.starts).unwrap()
    }
}

/// Owner discovery over the whole window; returns the drained offsets.
fn drain(map: &SidecarBitmap<'_>) -> Vec<u32> {
    let mut scan = map.scan(HIGH_WATER).expect("in-bounds high water");
    let mut out = Vec::new();
    while let Some(mut cut) = scan.next_cut() {
        while let Some(offset) = cut.pop() {
            out.push(offset);
        }
    }
    out
}

/// P1 + P2: the producer is paused (yields) BEFORE its RMW while the owner
/// issues granule 0. Whatever the interleaving, `publish(0)` either returns
/// `true` (start bit observed set) and the bit is drained exactly once, or
/// `false` (start bit observed zero — only possible before the issue) and NO
/// pending bit exists; a later publish after the issue must then succeed.
#[test]
fn loom_oneclass_paused_producer_start_bit_gate_delivery_exact() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();

        let producer = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                // Pause BEFORE the publish RMW: the whole decision window
                // (filter → start-bit Acquire load) races the owner's issue.
                thread::yield_now();
                // SAFETY (model): granule 0 is the modeled transfer.
                backing.bitmap().publish(0, 0)
            })
        };
        let owner = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                // Leaf-issue point: sets the class AND the start bit.
                backing.bitmap().issue(0);
            })
        };
        let published = producer.join().unwrap();
        owner.join().unwrap();

        let mut records = drain(&map);
        if published {
            assert_eq!(
                records,
                vec![0],
                "P1: a true publication is drained exactly once"
            );
        } else {
            assert!(
                records.is_empty(),
                "P2: a false publication plants no pending bit"
            );
            // The issue happened; a fresh publication must now succeed and
            // be discoverable (the gate decision is the start bit).
            assert!(
                map.publish(0, 0),
                "post-issue publish must see the start bit"
            );
            records = drain(&map);
            assert_eq!(records, vec![0]);
        }
        // Fully drained either way.
        assert!(
            drain(&map).is_empty(),
            "the discovery cut consumes every pending bit"
        );
    });
}

/// P3: a garbage free of a never-issued granule (16) races the owner's issue
/// of a DIFFERENT granule (0) — including the paused-producer window. The
/// start-bit gate must reject it in every interleaving; the owner's drain
/// finds only what was legitimately published.
#[test]
fn loom_oneclass_garbage_never_issued_granule_never_publishes() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();

        let garbage = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                // SAFETY (model): deliberate garbage free of a never-issued
                // granule; the filter must reject it without any RMW.
                !backing.bitmap().publish(16, 0)
            })
        };
        let owner = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                backing.bitmap().issue(0);
            })
        };
        // `rejected` is `true` iff the garbage free observed start bit 0 —
        // which holds in EVERY interleaving: granule 16 is never issued.
        let rejected = garbage.join().unwrap();
        owner.join().unwrap();
        assert!(
            rejected,
            "P3: a never-issued granule must be rejected in every interleaving"
        );
        assert!(drain(&map).is_empty(), "P3: no phantom pending bit");
        // The legitimately issued granule still publishes and drains once.
        assert!(map.publish(0, 0));
        assert_eq!(drain(&map), vec![0]);
        assert!(drain(&map).is_empty());
    });
}

/// P4 (negative shadow, precedent:
/// `loom_actual_negative_cut_via_load_store_loses_concurrent_bit`): a
/// load+store clear of the pending word (the FORBIDDEN carve-clear form)
/// loses a concurrent legitimate publication — this is why
/// `SmallSidecar::clear_pending_start_bit` must be the RMW
/// (`fetch_and(!mask, AcqRel)`) form.
#[test]
#[should_panic(expected = "published bit lost by load+store carve-clear")]
fn loom_oneclass_negative_load_store_carve_clear_loses_concurrent_bit() {
    loom::model(move || {
        let backing = Arc::new(Backing::new());
        // Legality precondition: granule 0's stale pending bit was legitimately
        // published BEFORE the spawn (issue + publish), so the owner's clear of
        // bit 0 is exactly the production carve-clear — never a bit theft.
        backing.bitmap().issue(0);
        assert!(backing.bitmap().publish(0, 0));
        // The concurrent producer publishes granule 16 (bit 1 of the SAME
        // word); its start bit is pre-issued so the only gate in question is
        // the pending RMW race against the clear.
        backing.bitmap().issue(16);

        let producer = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                thread::yield_now();
                backing.bitmap().publish(16, 0)
            })
        };
        let owner = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                // Mutant carve-clear: load + masked store (NOT the production
                // `fetch_and(!mask, AcqRel)` RMW). The yield between the load
                // and the store opens the window in which the producer's RMW
                // lands and is then destroyed by the store.
                thread::yield_now();
                let cleared = backing.words[0].load(Ordering::Acquire);
                thread::yield_now();
                backing.words[0].store(cleared & !1, Ordering::Release);
            })
        };
        let neighbour_published = producer.join().unwrap();
        owner.join().unwrap();

        // Production RMW contract: the concurrently published neighbour bit
        // survives in EVERY interleaving when its publish won the race. The
        // load+store form loses bit 1 whenever the producer RMW lands
        // between the owner's load and store → this assertion panics → the
        // negative control is RED on the forbidden form and stays GREEN on
        // the real `fetch_and` form. (The stale bit-0 clear itself is
        // deliberately NOT asserted word-exactly: against a plain store this
        // model's producer RMW write may retire in either order, so only the
        // LOSS property — the one the forbidden form uniquely admits — is
        // oracle-grade here.)
        let word = backing.words[0].load(Ordering::Acquire);
        assert!(
            word & 2 != 0 || !neighbour_published,
            "published bit lost by load+store carve-clear"
        );
    });
}
