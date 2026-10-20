//! B4 variant (в) — STATE-BYTE TRANSITION TABLE (TS1), substrate edition.
//!
//! One test per state transition of the per-granule state byte
//! (`byte = (class + 1) | (state << 6)`, state ∈ {LIVE=0, MAG=1, FREE=2}),
//! driven through the publicly-accessible `AllocCore` API and observed via
//! the `dbg_state_bytes_for` dump hook (the byte-level oracle). The
//! HeapCore/tcache-level MAG transitions (own free push, batch push, alloc
//! hit, refill-retained) live in `state_byte_heap_magazine.rs` — the dump
//! hook exists only at the `AllocCore` level, so this file pins every
//! transition whose commit site lives in `src/alloc_core/`.
//!
//! Also pins each REJECT of the own-thread free guard: a rejected free is a
//! no-op AND leaves the dumped state bytes byte-for-byte unchanged.
//!
//! Mutant contract (counterfactuals, all verified RED during authoring):
//! remove the `StateByte::write(.., LIVE)` at carve (try_carve_batch), at
//! `pop_free`, at `drain_freelist_batch`; corrupt the FREE store in
//! `dealloc_small`, `flush_run`, `reclaim_sidecar_record`; remove
//! `StateByte::init_in_place()` on the decommit reset — the corresponding
//! test here goes red.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

const BLOCK: usize = 16; // MIN_BLOCK, class 0
const SEGMENT: usize = 4 * 1024 * 1024;
/// `StateByte::FOOTPRINT` — one byte per `MIN_BLOCK` granule.
const FOOTPRINT: usize = SEGMENT / BLOCK;

const STATE_LIVE: u8 = 0;
const STATE_FREE: u8 = 2;

fn granule_of(p: *mut u8) -> usize {
    ((p as usize) & (SEGMENT - 1)) >> 4
}

fn state_of_byte(b: u8) -> u8 {
    b >> 6
}

fn class_field_of_byte(b: u8) -> u8 {
    b & 0x3F
}

/// Dump the FULL state-byte region of `anchor`'s segment.
fn dump(ac: &AllocCore, anchor: *mut u8) -> Vec<u8> {
    let mut v = vec![0u8; FOOTPRINT];
    // SAFETY: `anchor` is a live allocation owned by `ac` (callers keep the
    // block issued for the duration of the dump).
    unsafe { ac.dbg_state_bytes_for(anchor, &mut v) };
    v
}

fn expect_state(buf: &[u8], p: *mut u8, want_state: u8, class: usize, what: &str) {
    let g = granule_of(p);
    let b = buf[g];
    assert_ne!(b, 0, "{what}: byte at granule {g} is zero");
    assert_eq!(
        state_of_byte(b),
        want_state,
        "{what}: granule {g} state is {}, want {want_state} (byte {b:#04x})",
        state_of_byte(b),
    );
    assert_eq!(
        class_field_of_byte(b),
        class as u8 + 1,
        "{what}: granule {g} class field is {}, want {}",
        class_field_of_byte(b),
        class + 1,
    );
}

fn layout0() -> Layout {
    Layout::from_size_align(BLOCK, BLOCK).unwrap()
}

/// Allocate `n` 16-byte blocks, all non-null.
fn alloc_n(ac: &mut AllocCore, n: usize) -> Vec<*mut u8> {
    let l = layout0();
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        let p = ac.alloc(l);
        assert!(!p.is_null());
        v.push(p);
    }
    v
}

// ── TS1: transitions ──────────────────────────────────────────────────────

/// carve → LIVE: the carved-and-issued block commits LIVE with the block's
/// class field; interior granules of multi-granule blocks stay zero (И1).
#[test]
fn ts1_carve_writes_live() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 40);
    let buf = dump(&ac, blocks[0]);
    for &p in &blocks {
        expect_state(&buf, p, STATE_LIVE, 0, "carve");
    }
    // И1: never-issued granules before the first block read zero.
    let g0 = granule_of(blocks[0]);
    assert!(buf[..g0].iter().all(|&b| b == 0));
}

/// alloc_zeroed carve / zeroed-hit → LIVE (same commit sites as plain carve
/// downstream of the zeroed face).
#[test]
fn ts1_zeroed_carve_writes_live() {
    let mut ac = AllocCore::new().expect("primordial");
    let l = Layout::from_size_align(64, 8).unwrap(); // class with 4 granules
    let mut ps = Vec::new();
    for _ in 0..24 {
        let p = ac.alloc_zeroed(l);
        assert!(!p.is_null());
        ps.push(p);
    }
    let buf = dump(&ac, ps[0]);
    let cls = ac.dbg_layout_class_for(l).expect("small class");
    for &p in &ps {
        expect_state(&buf, p, STATE_LIVE, cls, "zeroed carve");
        // Only the START granule is marked; the 3 interior granules stay 0.
        for k in 1..4usize {
            assert_eq!(
                buf[granule_of(p) + k],
                0,
                "interior granule of a 64B block must stay zero"
            );
        }
    }
}

/// dealloc_small (own free, substrate) → FREE, class field preserved.
#[test]
fn ts1_dealloc_small_writes_free() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 40);
    let before = dump(&ac, blocks[0]);
    for (i, &p) in blocks.iter().enumerate() {
        if i % 2 == 0 {
            // SAFETY: live allocation, freed exactly once here.
            unsafe { ac.dealloc(p, layout0()) };
        }
    }
    let after = dump(&ac, blocks[1]); // anchor stays LIVE
    for (i, &p) in blocks.iter().enumerate() {
        if i % 2 == 0 {
            expect_state(&after, p, STATE_FREE, 0, "dealloc_small");
        } else {
            expect_state(&after, p, STATE_LIVE, 0, "kept live");
        }
    }
    // Odd (untouched) granules byte-identical to `before`.
    for (i, &p) in blocks.iter().enumerate() {
        if i % 2 == 1 {
            assert_eq!(before[granule_of(p)], after[granule_of(p)]);
        }
    }
}

/// free-list pop → LIVE: a block flushed to a bin and re-issued by the
/// regular alloc path commits FREE → LIVE at `pop_free`.
#[test]
fn ts1_free_list_pop_writes_live() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 8);
    let anchor = blocks[0]; // stays LIVE as the dump anchor
    for &p in &blocks[1..] {
        // SAFETY: live, single free.
        unsafe { ac.dealloc(p, layout0()) };
    }
    let buf = dump(&ac, anchor);
    for &p in &blocks[1..] {
        expect_state(&buf, p, STATE_FREE, 0, "flushed to bin");
    }
    // Re-issue: every pop commits its granule to LIVE.
    let mut reissued = Vec::new();
    for _ in 0..7 {
        let p = ac.alloc(layout0());
        assert!(!p.is_null());
        reissued.push(p);
    }
    let buf2 = dump(&ac, anchor);
    for &p in &reissued {
        let b = buf2[granule_of(p)];
        assert_ne!(b, 0);
        assert_ne!(state_of_byte(b), STATE_FREE, "re-issued block still FREE");
    }
}

/// drain → LIVE: every block returned by `dbg_drain_freelist_batch` ends
/// state-issued (LIVE), none left FREE.
#[test]
fn ts1_drain_writes_live() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 8);
    let anchor = blocks[0];
    for &p in &blocks[1..] {
        // SAFETY: live, single free.
        unsafe { ac.dealloc(p, layout0()) };
    }
    // Drain the WHOLE class-0 bin (it may also hold the refill's pre-carved
    // blocks) and require every drained block to end state-issued (LIVE).
    let mut out = [core::ptr::null_mut(); 64];
    let mut drained: Vec<*mut u8> = Vec::new();
    loop {
        // SAFETY: anchor is a live allocation of `ac`; the drain visits
        // class-0 free-list nodes of its own segment only.
        let got = unsafe { ac.dbg_drain_freelist_batch(anchor, 0, &mut out) };
        assert!(got > 0 || drained.len() + got > 0);
        if got == 0 {
            break;
        }
        drained.extend_from_slice(&out[..got]);
        if got < out.len() {
            break;
        }
    }
    assert!(
        drained.len() >= 7,
        "bin must hold at least the 7 freed blocks"
    );
    for &p in &blocks[1..] {
        assert!(
            drained.contains(&p),
            "freed block {p:p} must have been drained"
        );
    }
    let buf = dump(&ac, anchor);
    for &p in &drained {
        expect_state(&buf, p, STATE_LIVE, 0, "drained block");
        assert!(!ac.dbg_is_free_for(p), "drained block must not read FREE");
    }
}

/// overflow/R1-01/teardown flush (substrate `flush_class`/`flush_run`) →
/// FREE: every accepted run block commits LIVE → FREE.
#[test]
fn ts1_flush_run_writes_free() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 8);
    let anchor = blocks[0]; // stays LIVE as the dump anchor
    let buf_before = dump(&ac, anchor);
    for &p in &blocks[1..] {
        expect_state(&buf_before, p, STATE_LIVE, 0, "pre-flush");
    }
    let rest = &blocks[1..];
    // SAFETY: all blocks are LIVE class-0 blocks of `ac`'s segment.
    unsafe { ac.flush_class(0, rest) };
    for &p in rest {
        assert!(ac.dbg_is_free_for(p), "flushed block must read FREE");
    }
    let buf = dump(&ac, anchor);
    for &p in rest {
        expect_state(&buf, p, STATE_FREE, 0, "flush_run");
    }
    expect_state(&buf, anchor, STATE_LIVE, 0, "anchor stays live");
}

/// reclaim (sidecar record drain on the owner) → FREE: a LIVE block
/// published through the sidecar and reclaimed by the owner commits
/// LIVE → FREE at `reclaim_sidecar_record`.
#[test]
#[cfg(all(feature = "alloc-xthread", feature = "bench-internals"))]
fn ts1_reclaim_writes_free() {
    let mut ac = AllocCore::dbg_new_routed_for_test().expect("routed core");
    let mut blocks = alloc_n(&mut ac, 4);
    let victim = blocks.remove(0);
    // Still LIVE, still not free.
    assert!(!ac.dbg_is_free_for(victim));
    // SAFETY: victim is a live class-0 block of `ac`; its single free right
    // is transferred to the sidecar here.
    assert!(unsafe { ac.dbg_publish_small_sidecar_free(victim) });
    // Publication alone must NOT change the byte: still not on a bin.
    assert!(
        !ac.dbg_is_free_for(victim),
        "published but not yet reclaimed"
    );
    // Owner drains the sidecar → reclaim commits LIVE → FREE.
    let reclaimed = ac.dbg_drain_sidecar_ingress();
    assert_eq!(reclaimed, 1);
    assert!(
        ac.dbg_is_free_for(victim),
        "reclaimed record must read FREE"
    );
    // And it is reusable: popping it commits FREE → LIVE again.
    let re = ac.alloc(layout0());
    assert_eq!(re, victim, "reclaimed block is the one reissued");
    assert!(!ac.dbg_is_free_for(re));
}

// ── decommit / recycle reset → 0 ─────────────────────────────────────────

/// decommit/recycle reset → 0: when an emptied pooled segment is decommitted
/// the WHOLE state-byte region is re-zeroed; after recycle a re-carved block
/// is LIVE and every other formerly-issued granule reads exactly zero again.
/// Mutant: remove `StateByte::init_in_place()` at the decommit reset →
/// stale FREE bytes survive → RED.
#[test]
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
fn ts1_decommit_reset_zeroes_state_bytes() {
    let mut ac = AllocCore::new().expect("primordial");
    let l = layout0();
    // The primordial never decommits; the CURRENT small segment never
    // decommits either. Fill until THREE distinct segments exist and use the
    // middle one as the decommit target.
    let prim_base = {
        let first = ac.alloc(l);
        assert!(!first.is_null());
        SegmentLayout::segment_base_of(first as usize)
    };
    let mut order: Vec<usize> = vec![prim_base];
    let mut seg_blocks: std::collections::BTreeMap<usize, Vec<*mut u8>> =
        std::collections::BTreeMap::new();
    let mut last_base = prim_base;
    for _ in 0..800_000 {
        let p = ac.alloc(l);
        assert!(!p.is_null(), "allocation failed while filling");
        let b = SegmentLayout::segment_base_of(p as usize);
        if b != last_base {
            if !seg_blocks.contains_key(&b) {
                order.push(b);
            }
            last_base = b;
        }
        seg_blocks.entry(b).or_default().push(p);
        // Stop once the middle segment has enough blocks AND is no longer
        // the current segment (only non-current segments decommit).
        if order.len() >= 3 && seg_blocks[&order[order.len() - 1]].len() >= 200 {
            break;
        }
    }
    assert!(order.len() >= 3, "need a third segment");
    // Target = the CURRENT segment: after the retain-leg decommit the very
    // next carve recommits and reuses it (same base).
    let target_base = order[order.len() - 1];
    let target_blocks = &seg_blocks[&target_base];
    assert!(target_blocks.len() >= 200);

    // Free everything in the target segment; the other segments stay live.
    for &p in target_blocks.iter() {
        // SAFETY: live, single free.
        unsafe { ac.dealloc(p, layout0()) };
    }
    let target = target_blocks[0];
    assert!(ac.dbg_is_free_for(target));

    // Drive the retain-leg decommit directly on the emptied target segment:
    // the payload goes back to the OS, the bump cursor resets, the free
    // lists empty AND the whole state-byte region is re-zeroed, while the
    // segment STAYS registered for a future recommit-on-reuse carve.
    // SAFETY: every block carved from the target segment was freed above,
    // so its live_count is 0; the segment is exclusively owned by `ac`.
    assert!(unsafe { ac.dbg_force_decommit_retain_for(target) });

    // Recycle: the next allocation must come back from the reset segment
    // (same base, recommit-on-reuse carve).
    let recycled = ac.alloc(layout0());

    assert!(!recycled.is_null());
    let rec_base = SegmentLayout::segment_base_of(recycled as usize);
    assert_eq!(
        rec_base, target_base,
        "pooled segment was not recycled — reset observation impossible"
    );

    let buf = dump(&ac, recycled);
    // The recycled block is LIVE (fresh carve after reset).
    expect_state(&buf, recycled, STATE_LIVE, 0, "recycled carve");
    // The recommit-on-reuse carve opens a NEW carve window at the reset bump
    // (payload start): the issued block plus its freshly carved (FREE)
    // refill siblings legitimately occupy a small window at the start. The
    // segment previously held >= 200 issued blocks, so every granule OUTSIDE
    // [0, g(recycled) + 64) must be exactly zero — a stale state byte that
    // survived the decommit reset lands there.
    let g0 = granule_of(recycled);
    assert!(buf[..g0].iter().all(|&b| b == 0));
    for (g, &b) in buf.iter().enumerate().skip(g0 + 64) {
        assert_eq!(b, 0, "state byte survived decommit reset at granule {g}");
    }
}

// ── rejects: no-op + UNCHANGED bytes ─────────────────────────────────────

/// Substrate double free: second `dealloc_small` is a no-op and the dumped
/// state bytes are unchanged.
#[test]
fn ts1_reject_double_free_byte_unchanged() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 4);
    // SAFETY: live, single free.
    unsafe { ac.dealloc(blocks[0], layout0()) };
    let after_first = dump(&ac, blocks[1]);
    expect_state(&after_first, blocks[0], STATE_FREE, 0, "first free");
    // Double free — must be swallowed by the exact FREE oracle.
    // SAFETY: blocks[0] is FREE; this is the deliberate double-free probe.
    unsafe { ac.dealloc(blocks[0], layout0()) };
    let after_second = dump(&ac, blocks[1]);
    assert_eq!(
        after_first, after_second,
        "double free mutated the state bytes"
    );
    expect_state(&after_second, blocks[0], STATE_FREE, 0, "post double free");
    // The block is issued at most once more.
    let p1 = ac.alloc(layout0());
    let p2 = ac.alloc(layout0());
    assert_ne!(p1, p2, "double free caused a double issue");
}

/// Interior pointer (not a block start): hardened guard rejects as a no-op,
/// bytes unchanged. Mutant: remove the H1 alignment guard → this test goes
/// red (mid-block `write_next` + a spurious FREE commit).
#[test]
#[cfg(feature = "hardened")]
fn ts1_reject_interior_byte_unchanged() {
    let mut ac = AllocCore::new().expect("primordial");
    let l = Layout::from_size_align(64, 8).unwrap(); // 4 granules per block
    let mut blocks = Vec::new();
    for _ in 0..4 {
        let p = ac.alloc(l);
        assert!(!p.is_null());
        blocks.push(p);
    }
    let before = dump(&ac, blocks[0]);
    let interior = unsafe { blocks[0].add(16) }; // granule 1 of a 4-granule block
                                                 // SAFETY: deliberate reject probe; the guard must swallow it.
    unsafe { ac.dealloc(interior, l) };
    let after = dump(&ac, blocks[0]);
    assert_eq!(before, after, "interior free mutated the state bytes");
    for &p in &blocks {
        assert!(!ac.dbg_is_free_for(p));
    }
}

/// Metadata offset (lands inside the segment's own metadata region): the
/// H-1 guard rejects as a no-op, bytes unchanged.
#[test]
fn ts1_reject_metadata_offset_byte_unchanged() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 4);
    let before = dump(&ac, blocks[0]);
    let base = SegmentLayout::segment_base_of(blocks[0] as usize);
    for off in [0usize, 8, 64, 4096] {
        let fake = (base + off) as *mut u8;
        // SAFETY: deliberate reject probe; computed `off` is below the
        // payload start, the guard must swallow it without a dereference.
        unsafe { ac.dealloc(fake, layout0()) };
    }
    let after = dump(&ac, blocks[0]);
    assert_eq!(
        before, after,
        "metadata-offset free mutated the state bytes"
    );
}

/// Out-of-bump offset (past the carve frontier — post-reset stale pointer
/// shape): the M-1 guard rejects as a no-op, bytes unchanged.
#[test]
fn ts1_reject_out_of_bump_byte_unchanged() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 4);
    let before = dump(&ac, blocks[0]);
    let base = SegmentLayout::segment_base_of(blocks[0] as usize);
    let stale = (base + SEGMENT - BLOCK) as *mut u8; // far past bump
                                                     // SAFETY: deliberate reject probe; `off >= bump` must be swallowed
                                                     // before any payload access.
    unsafe { ac.dealloc(stale, layout0()) };
    let after = dump(&ac, blocks[0]);
    assert_eq!(before, after, "out-of-bump free mutated the state bytes");
}

/// Foreign pointer (a live block of ANOTHER core freed through this core):
/// unresolvable base → no-op, the foreign segment's bytes are untouched and
/// the block stays usable in its owner.
#[test]
fn ts1_reject_foreign_noop() {
    let mut a = AllocCore::new().expect("primordial A");
    let mut b = AllocCore::new().expect("primordial B");
    let pa = a.alloc(layout0());
    assert!(!pa.is_null());
    let before = dump(&a, pa);
    // SAFETY: deliberate reject probe — `pa` belongs to core A, not `b`.
    unsafe { b.dealloc(pa, layout0()) };
    let after = dump(&a, pa);
    assert_eq!(before, after, "foreign free mutated core A's state bytes");
    expect_state(&after, pa, STATE_LIVE, 0, "foreign-freed block stays LIVE");
    // The block is still exclusively usable in its owner.
    // SAFETY: live, single free in its owner.
    unsafe { a.dealloc(pa, layout0()) };
    let re = a.alloc(layout0());
    assert_eq!(re, pa, "owner must be able to reissue its own block");
}

/// Byte ⇔ state consistency for the FREE probe: `dbg_is_free_for` decodes
/// the state bits ONLY (any nonzero class field), and is false for LIVE,
/// MAG-shaped (state 1) and zero bytes. Pins the decode contract the
/// magazine oracles rely on.
#[test]
fn ts1_free_probe_decodes_state_only() {
    let mut ac = AllocCore::new().expect("primordial");
    let blocks = alloc_n(&mut ac, 4);
    for &p in &blocks {
        assert!(!ac.dbg_is_free_for(p), "LIVE block must not read free");
    }
    // SAFETY: live, single free.
    unsafe { ac.dealloc(blocks[0], layout0()) };
    assert!(ac.dbg_is_free_for(blocks[0]), "FREE block must read free");
    assert!(!ac.dbg_is_free_for(blocks[1]), "sibling stays not-free");
    // Zero bytes: a never-issued granule is not "free".
    let base = SegmentLayout::segment_base_of(blocks[0] as usize);
    let g0 = granule_of(blocks[0]);
    if g0 > 0 {
        assert!(!ac.dbg_is_free_for((base as *mut u8).wrapping_add((g0 - 1) * BLOCK)));
    }
}
