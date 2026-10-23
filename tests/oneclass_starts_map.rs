//! D1′ phase-1a — the dense "start was issued" map in `SmallSidecar` and the
//! producer payload/start-bit filter.
//!
//! T1: a garbage foreign free of a granule in the never-carved tail is
//!     rejected (no pending RMW); later carves/issues/strict trim are
//!     unaffected and `live_count` stays exact.
//! T2: a foreign free of a 16-aligned INTERIOR pointer of a live block is
//!     owner-rejected (start bit 0), the payload is untouched.
//! T4: a retain-reset (test hook, quiescent) clears pending AND start bits
//!     atomically; a stale publication that arrived before the reset cannot
//!     reclaim any freshly re-carved block of a different class.
//! T5: a misaligned foreign free is rejected by the producer filter
//!     (`publish == false`), the block stays live.
//! T6: a legitimate remote free of EVERY small class reclaims exactly once,
//!     the class coming from the state byte (`encode(c, LIVE)` only); a
//!     duplicate publication of a FREE block is a quiet owner no-op
//!     (REMOTE_REJECT_FREE).
//!
//! Mutant contracts (counterfactuals):
//! - remove the start-bit set at the owner per-block issue points
//!   (`SidecarBitmap::issue`, reached from both the drain and the
//!   carve-batch paths through `SmallIssueRun::issue`) → T6 goes RED
//!   (legitimate publications are dropped by the zero start bit, the drain
//!   reclaims nothing);
//! - remove the Acquire start-bit check in `SidecarBitmap::publish` (or move
//!   the RMW before it) → T1/T2/T5 go RED (garbage/interior/misaligned frees
//!   publish and the owner aborts or reclaims foreign blocks);
//! - drop the payload filter (`payload_start`) → T5's misaligned case still
//!   fails on alignment, but the metadata region becomes publishable;
//! - clear the pending bank with load+store (or clear only one bank) at the
//!   retain-reset → T4 goes RED (stale bit reclaims a fresh block, or the
//!   owner fatals on a cleared start bit);
//! - let reclaim accept a byte other than `encode(c, LIVE)` → T6's duplicate
//!   publication would reclaim a second time (drain > 0).
// `HeapCore::dbg_live_count_for` is `alloc-decommit`-gated and
// `dbg_class_for` is `fastbin`-gated (registry diag queries).
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit",
    feature = "fastbin"
))]

use core::alloc::Layout;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SegmentLayout;

const SEGMENT: usize = SegmentLayout::SEGMENT;

// Tests share the process-wide route directory and allocator; serialize.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The full producer terminal-publication path through the global directory
/// pin (cached-pin path excluded) — the filter under test sits inside
/// `SmallSidecar::publish`. Returns the publication verdict verbatim.
///
/// # Safety (deliberate negative probes)
/// For the garbage/interior/misaligned probes the "current issued instance"
/// ownership contract is VIOLATED ON PURPOSE (that is the misuse the filter
/// must reject). The sidecar RMW never dereferences the pointer, so the
/// probe itself touches no reservation byte; a `false` verdict leaves no
/// side effect behind.
unsafe fn try_publish(ptr: *mut u8) -> bool {
    let Some(pin) = RouteDirectory::global().lookup(ptr) else {
        return false;
    };
    if !matches!(pin.kind(), RouteKind::Small | RouteKind::Primordial) {
        return false;
    }
    let offset = (ptr.addr() & (SEGMENT - 1)) as u32;
    // SAFETY: see the function doc — legitimate probes transfer the current
    // issued instance exactly once; negative probes deliberately publish a
    // never-issued address and rely on the filter's `false`.
    unsafe { pin.publish_small(offset) }
}

fn l(size: usize) -> Layout {
    Layout::from_size_align(size, 16).unwrap()
}

/// T1 — garbage foreign free in the never-carved tail: rejected before any
/// RMW; the later carves/issues/strict-trim cycle is unaffected and
/// `live_count` stays exact.
#[test]
fn t1_garbage_tail_free_is_rejected_and_never_reclaimed() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l(16));
    assert!(!p.is_null());
    let base = p.addr() & !(SEGMENT - 1);
    // Deep-tail granule: no bump carve can reach the last granule of a
    // freshly routed segment's payload.
    let garbage = base + SEGMENT - 16;
    let live0 = heap.dbg_live_count_for(p).unwrap();
    let (u0, f0, m0, cb0, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();

    // SAFETY: deliberate negative probe (see `try_publish`).
    assert!(
        !unsafe { try_publish(garbage as *mut u8) },
        "never-issued tail granule must not publish"
    );
    // Strict trim: the garbage free must have planted NOTHING.
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    assert_eq!(heap.dbg_live_count_for(p).unwrap(), live0);
    let (_, _, _, cb1, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();
    assert_eq!(cb0, cb1, "garbage free must not create carve-clear work");
    let (u1, f1, m1, _, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();
    assert_eq!((u0, f0, m0), (u1, f1, m1), "no owner rejects yet");

    // Later carves and issues (real allocations), then a strict trim: the
    // exact number of published blocks is reclaimed — no phantom from the
    // garbage bit, `live_count` exact.
    let mut blocks = Vec::new();
    for _ in 0..64 {
        let q = heap.alloc(l(16));
        assert!(!q.is_null());
        blocks.push(q);
    }
    for q in &blocks {
        // SAFETY: each q is live and transferred exactly once here.
        assert!(unsafe { try_publish(*q) }, "legit block must publish");
    }
    // SAFETY: p is live and transferred exactly once here.
    assert!(unsafe { try_publish(p) });
    // Deterministic live-count oracle: snapshot AFTER all 64 allocations and
    // every `try_publish` (publication never touches `live_count`, so the
    // snapshot is unaffected by them) but BEFORE the drain, so the baseline
    // absorbs the magazine-resident blocks and any residue from earlier tests
    // in this process, independent of which segments served the 64 blocks.
    let pre = heap.dbg_live_count_for(p).unwrap();
    let reclaimed = heap.dbg_drain_sidecar_ingress();
    assert_eq!(reclaimed, blocks.len() + 1);
    // The drain reclaimed exactly the published set (asserted above). Of
    // that set only p and the fresh blocks the magazine served from p's
    // segment's OWN free lists (reuse of blocks freed by earlier tests in
    // this process) belong to p's segment; blocks carved into other segments
    // stay there. The segment's live count must drop by exactly that many —
    // one per reclaimed block of the segment, never more, never less.
    let same_segment = blocks
        .iter()
        .filter(|q| q.addr() & !(SEGMENT - 1) == base)
        .count() as u32
        + 1; // p itself
    assert_eq!(
        heap.dbg_live_count_for(p).unwrap(),
        pre - same_segment,
        "live_count must be exact"
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    // Note: alloc-side accounting for the 64 T1 allocations themselves (the
    // refill counter model) is NOT re-checked here; that oracle lives in
    // `tests/r11_ph3a_magazine_refill_partial.rs`.
    // SAFETY: all issued allocations were retired by the owner-side drain.
    drop(lease);
}

/// T2 — a 16-aligned interior pointer of a live block is owner-rejected; the
/// payload is untouched.
#[test]
fn t2_interior_free_of_live_block_is_rejected_payload_intact() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let layout = l(64);
    let p = heap.alloc(layout);
    assert!(!p.is_null());
    let class = heap.dbg_class_for(layout).expect("issued class");
    // Marker bytes in the payload — the rejected probe must not touch them.
    let marker = [0x5Au8; 64];
    // SAFETY: p is a live 64-byte allocation of this lease.
    unsafe { core::ptr::copy_nonoverlapping(marker.as_ptr(), p, 64) };

    let live0 = heap.dbg_live_count_for(p).unwrap();
    let interior = p.with_addr(p.addr() + 16);
    // SAFETY: deliberate negative probe (see `try_publish`).
    assert!(
        !unsafe { try_publish(interior) },
        "interior granule has no start bit and must not publish"
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    assert_eq!(heap.dbg_live_count_for(p).unwrap(), live0);
    assert_eq!(heap.dbg_state_byte_state_for(p), Some(0), "still LIVE");
    // SAFETY: p is still live and exclusively owned by this lease.
    let payload = unsafe { core::slice::from_raw_parts(p, 64) };
    assert_eq!(payload, &marker, "payload untouched by the rejected free");

    // The legitimate block itself still publishes and reclaims exactly once.
    // SAFETY: p is live and transferred exactly once here.
    assert!(unsafe { try_publish(p) });
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    assert_eq!(heap.dbg_live_count_for(p).unwrap(), live0 - 1);
    let second = if cfg!(debug_assertions) {
        // SAFETY: synthetic probe of the owner reclaim primitive on a
        // FREE-state block; the reservation stays live and owned.
        unsafe { heap.dbg_reclaim_sidecar_record_for_test(p, class as u8) }
    } else {
        false
    };
    assert!(
        !second,
        "FREE byte: a second reclaim must be a quiet owner no-op"
    );
    // SAFETY: all issued allocations were retired by the owner-side drain.
    drop(lease);
}

/// T4 — retain-reset (quiescent test hook) atomically clears BOTH remote
/// banks: a stale pending bit set before reset is gone afterwards, the start
/// bit is gone with it (`publish` returns false), and an allocation of a
/// DIFFERENT class on the SAME reset segment (same route, not same offset)
/// is neither marked pending nor reclaimed by stale state. The allocator may
/// initially choose other routes; the fixture follows real allocations in a
/// fixed 4096-allocation bound and asserts SAME route on the selected block.
#[cfg(feature = "alloc-decommit")]
#[test]
fn t4_retain_reset_clears_stale_bits_recarve_other_class() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    // The retain hook refuses the Primordial segment, so force a genuinely
    // fresh ordinary Small segment with a large small-class allocation
    // (same pattern as `tests/alloc_zeroed_virgin_small_skip.rs`).
    let layout = l(64 * 1024);
    // Use a distinct small class; the route is verified on every candidate.
    let other_layout = l(32 * 1024);
    assert_ne!(
        heap.dbg_class_for(layout),
        heap.dbg_class_for(other_layout),
        "fixture: the re-carve must use a genuinely different class"
    );
    // Drain this owner's magazines before carving the target segment; this
    // clears preexisting magazine state, not cursor or route ordering.
    heap.dbg_flush_all();
    let reserved_before = sefer_alloc::AllocCore::dbg_segments_reserved_total();
    let mut a = None;
    let mut keep = Vec::new();
    for _ in 0..20_000 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        keep.push(p);
        if sefer_alloc::AllocCore::dbg_segments_reserved_total() > reserved_before {
            a = Some(p);
            break;
        }
    }
    let a = a.expect("a fresh Small segment must form within 20k allocs");
    // `a` is the FIRST and only live block carved from the fresh segment S.

    // SAFETY: a is live and transferred exactly once; deliberately NOT
    // drained — its pending bit is the STALE bit the reset must clear.
    assert!(unsafe { try_publish(a) });
    let pin = RouteDirectory::global().lookup(a).expect("issued route");
    assert!(
        pin.pending_for_test(a),
        "precondition: the stale pending bit is set"
    );

    // SAFETY: test hook (see its `# Safety`); quiescent: no concurrent
    // producer or owner activity on this segment. The live-count check is
    // deliberately waived by this probe: `a`'s ownership was already
    // transferred (no reservation byte is ever touched again), the retain
    // leg keeps the reservation mapped, and the sidecar is external storage.
    let reset = unsafe { heap.dbg_force_decommit_retain_for(a) };
    assert!(reset, "retain reset must find a's (Small) segment");

    // BOTH banks cleared atomically: no stale pending bit, and the start bit
    // is gone — a delayed duplicate of the stale producer RMW is rejected.
    assert!(
        !pin.pending_for_test(a),
        "retain reset must clear the stale PENDING bit"
    );
    // SAFETY: deliberate negative probe — the granule is now never-issued
    // again (fresh incarnation), the filter must reject it.
    assert!(
        !unsafe { try_publish(a) },
        "retain reset must clear the START bit (stale RMW rejected)"
    );

    // Re-carve with a DIFFERENT class; cursor/free-list state may choose an
    // existing route first. Allocate and retain results in a fixed 4096
    // allocation bound (segment-table capacity), requiring actual allocator
    // routing to reach the reset segment; never skip or rerun for luck.
    let mut q = core::ptr::null_mut();
    let mut q_keep = Vec::new();
    for _ in 0..4096 {
        let candidate = heap.alloc(other_layout);
        assert!(!candidate.is_null());
        let candidate_pin = RouteDirectory::global()
            .lookup(candidate)
            .expect("candidate route");
        if candidate_pin.root() == pin.root() {
            q = candidate;
            break;
        }
        q_keep.push(candidate);
    }
    assert!(
        !q.is_null(),
        "bounded class-cursor coverage must revisit reset route"
    );
    // ADDRESS-LEVEL fixture check (D1′ round-2 P2#8): the recarve must reuse
    // the SAME segment (route) whose banks the reset cleared — read ONLY the
    // immutable descriptor root via the safe pin observer (no reservation
    // byte is touched).
    //
    // The lazy-commit reset frontier lies above the stale granule. The key
    // oracle here is the preserved same-route root assertion, not address reuse.
    let pin_q = RouteDirectory::global().lookup(q).expect("q route");
    assert_eq!(
        pin_q.root(),
        pin.root(),
        "the post-reset recarve must reuse the same reset segment (same route root)"
    );
    assert_eq!(
        heap.dbg_state_byte_state_for(q),
        Some(0),
        "fresh block must be LIVE"
    );
    assert!(
        !pin.pending_for_test(q),
        "no stale bit may sit on the fresh block"
    );
    assert_eq!(
        heap.dbg_drain_sidecar_ingress(),
        0,
        "no stale bit may reclaim a fresh block"
    );

    // Control: the fresh block publishes and reclaims exactly once.
    // SAFETY: q is live and transferred exactly once here.
    assert!(unsafe { try_publish(q) });
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    // SAFETY: all live allocations of this lease were retired above.
    drop(lease);
    drop(q_keep);
    drop(keep);
}

/// T5 — a misaligned foreign free is rejected by the producer filter
/// (`publish == false`); the live block is untouched.
#[test]
fn t5_misaligned_free_is_rejected() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l(64));
    assert!(!p.is_null());
    let live0 = heap.dbg_live_count_for(p).unwrap();
    let misaligned = p.with_addr(p.addr() + 8);
    // SAFETY: deliberate negative probe (see `try_publish`).
    assert!(
        !unsafe { try_publish(misaligned) },
        "misaligned address must be rejected by the producer filter"
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    assert_eq!(heap.dbg_live_count_for(p).unwrap(), live0);
    assert_eq!(heap.dbg_state_byte_state_for(p), Some(0), "still LIVE");
    // SAFETY: all issued allocations were retired by the owner-side drain.
    drop(lease);
}

/// T6 — a legitimate remote free of every small class reclaims exactly once,
/// the class coming from the state byte; a duplicate publication of a FREE
/// block is a quiet owner no-op.
#[test]
fn t6_each_class_remote_free_reclaims_once_class_from_byte() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    // Full small-class coverage: enumerate every size that maps to a
    // distinct small class (16 ..= SMALL_MAX, step 16), deduplicated through
    // the class oracle itself, and require that the run covers ALL classes
    // (`SIZE_CLASS_TABLE.len()`).
    let small_max = SegmentLayout::SIZE_CLASS_TABLE[SegmentLayout::SIZE_CLASS_TABLE.len() - 1];
    let class_count = SegmentLayout::SIZE_CLASS_TABLE.len();
    let mut sizes: Vec<usize> = Vec::new();
    let mut seen: Vec<usize> = Vec::new();
    let mut size = 16usize;
    while size <= small_max {
        if let Some(c) = heap.dbg_class_for(l(size)) {
            if !seen.contains(&c) {
                seen.push(c);
                sizes.push(size);
            }
        }
        size += 16;
    }
    assert_eq!(
        seen.len(),
        class_count,
        "fixture: the 16..={small_max} sweep must cover ALL {class_count} small classes"
    );
    let mut blocks = Vec::new();
    for &size in &sizes {
        let p = heap.alloc(l(size));
        assert!(!p.is_null(), "size {size}");
        blocks.push(p);
    }
    let (_, f0, m0, _, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();

    for p in &blocks {
        // SAFETY: each block is live and transferred exactly once here.
        assert!(unsafe { try_publish(*p) }, "legit block must publish");
    }
    // Per-segment live credit: count the blocks that share blocks[0]'s
    // segment (the run may span several segments on a busy registry).
    let base0 = blocks[0].addr() & !(SEGMENT - 1);
    let same_segment = blocks
        .iter()
        .filter(|b| b.addr() & !(SEGMENT - 1) == base0)
        .count() as u32;
    assert!(same_segment >= 1);
    let live_all = heap.dbg_live_count_for(blocks[0]).unwrap();
    let reclaimed = heap.dbg_drain_sidecar_ingress();
    assert_eq!(reclaimed, blocks.len(), "every class reclaims exactly once");
    for p in &blocks {
        assert!(heap.dbg_is_free_for(*p), "state byte committed FREE");
        assert_eq!(heap.dbg_state_byte_state_for(*p), Some(2));
    }
    assert_eq!(
        heap.dbg_live_count_for(blocks[0]).unwrap(),
        live_all - same_segment,
        "live_count exact across all classes"
    );
    // FREE/MAG reject counters: no MAG occurred here; the duplicate
    // publication below is a quiet FREE no-op.
    assert_eq!(m0, sefer_alloc::AllocCore::dbg_oneclass_counters().2);

    // Duplicate foreign free of an already-FREE block: the pending RMW still
    // passes the start-bit gate (the block WAS issued), but the owner sees
    // the FREE byte and quietly drops it.
    let dup = blocks[0];
    // SAFETY: deliberate duplicate-free probe; the owner-side FREE-byte
    // reject is the behavior under test (the bit was already consumed, so
    // this only re-plants one pending bit for one quiet no-op).
    assert!(
        unsafe { try_publish(dup) },
        "the start bit of an issued (now FREE) block stays set"
    );
    assert_eq!(
        heap.dbg_drain_sidecar_ingress(),
        0,
        "FREE byte is a quiet owner no-op, never a second reclaim"
    );
    assert!(
        heap.dbg_is_free_for(dup),
        "still FREE, live_count untouched"
    );
    let (_, f1, _, _, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();
    assert_eq!(f1, f0 + 1, "REMOTE_REJECT_FREE counted the observed bit");
    // SAFETY: all issued allocations were retired by the owner-side drain.
    drop(lease);
}
