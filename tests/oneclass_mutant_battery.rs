//! D1′ one-class mutant battery (design §6.3 M-a, M-c′, M-d, M-e, M-f, M-l).
//!
//! Each test pins one production mutation that previously stayed green:
//! - M-a (`carve_block`/`try_carve_batch` lose their
//!   `clear_pending_start_bit` call): a stale pending bit planted on the
//!   offset of a FUTURE carve (deterministically reachable only after a
//!   retain-reset re-planted the start bit through the doc-hidden
//!   `issue_for_test` hook) must be dropped by the carve itself — oracle:
//!   `CARVE_CLEAR_BITS` delta ≥ 1 and the following drain reclaims nothing.
//!   Runs in a child process: on the mutant the drain would otherwise
//!   reclaim a byte-0 start and abort the whole test binary.
//! - M-c′ (retain-reset clears a NARROWED range, upper bound < old_bump):
//!   stale pending bits at the LOW, MIDDLE and HIGH granules of
//!   `[payload_start, old_bump)` must ALL be cleared. (The originally
//!   formulated M-c "range starts at 0 instead of payload_start" is
//!   equivalent: no legitimate bit ever exists below `payload_start` —
//!   `issue` only ever runs at carve offsets — so clearing below it is
//!   unobservable; the meaningful corruption is the narrowed upper bound.)
//! - M-d (`reclaim_sidecar_record` accepts a zero state byte quietly):
//!   after a retain-reset zeroed the byte, re-planting the start bit
//!   (`issue_for_test`) and publishing reaches reclaim with byte 0 at a
//!   start → the clean owner must `fatal`-abort; the mutant's quiet no-op
//!   lets the child survive → RED.
//! - M-e (reclaim bins a record under a foreign class): after a legit
//!   reclaim the block must sit at the head of ITS OWN class bin
//!   (`owner_state_for_test`) and be re-issued by the same Layout — and
//!   only by it.
//! - M-f (`SidecarBitmap::publish` loses the payload filter): on a
//!   Primordial-kind fixture, `publish` of a METADATA granule
//!   (`[SMALL_META_END, PRIMORDIAL_META_END)`) whose start bit was set by
//!   the owner hook must still be rejected (`false`, no pending bit) —
//!   the payload filter fires BEFORE the start gate.
//! - M-l (reclaim loses the `(off, off+bs)` start-overlap fatal): a
//!   deliberate state-byte corruption claims a 64 B block as class 256 B;
//!   issued 16/32/48 B neighbours start inside the claimed open interval →
//!   the clean owner must `fatal`-abort; the mutant accepts the block.
//!
//! All child-process cases print `ARMED` before the decisive call and
//! `SURVIVED` only where the clean owner must have aborted; the parent
//! asserts exit status and the absence of `SURVIVED`.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit",
    feature = "fastbin"
))]

use core::alloc::Layout;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RouteRegistration};
use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;

const SEGMENT: usize = SegmentLayout::SEGMENT;
const BIG: usize = 64 * 1024; // the large small-class used to force fresh segments (T4 pattern)

static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn l(size: usize) -> Layout {
    Layout::from_size_align(size, 16).unwrap()
}

/// Full producer publication path through the global directory (same shape
/// as `tests/oneclass_starts_map.rs::try_publish`).
///
/// # Safety (deliberate negative probes)
/// For negative probes the current-issued-instance contract is violated on
/// purpose; a `false` verdict leaves no side effect behind.
unsafe fn try_publish(ptr: *mut u8) -> bool {
    let Some(pin) = RouteDirectory::global().lookup(ptr) else {
        return false;
    };
    if !matches!(pin.kind(), RouteKind::Small | RouteKind::Primordial) {
        return false;
    }
    let offset = (ptr.addr() & (SEGMENT - 1)) as u32;
    unsafe { pin.publish_small(offset) }
}

/// Force a genuinely fresh ordinary Small segment and return its FIRST
/// carved block (T4 pattern from `tests/oneclass_starts_map.rs`).
fn first_block_of_fresh_small_segment(heap: &mut HeapCore) -> *mut u8 {
    let reserved_before = sefer_alloc::AllocCore::dbg_segments_reserved_total();
    let mut a = None;
    let mut keep = Vec::new();
    for _ in 0..20_000 {
        let p = heap.alloc(l(BIG));
        assert!(!p.is_null());
        keep.push(p);
        if sefer_alloc::AllocCore::dbg_segments_reserved_total() > reserved_before {
            a = Some(p);
            break;
        }
    }
    let a = a.expect("a fresh Small segment must form within 20k allocs");
    keep.shrink_to_fit();
    core::mem::forget(keep); // the lease keeps them alive; never freed below
    a
}

// ---------------------------------------------------------------------------
// Child harness (pattern: tests/r6_t06_freelist_continuation_bounds.rs)
// ---------------------------------------------------------------------------

const CASE_ENV: &str = "SEFER_OCMUT_CASE";

fn run_child(case: &str) -> std::process::Output {
    let exe = std::env::current_exe().expect("test executable");
    std::process::Command::new(&exe)
        .args(["--exact", "ocmut_child", "--nocapture"])
        .env(CASE_ENV, case)
        .output()
        .unwrap_or_else(|e| panic!("run ocmut child {case}: {e}"))
}

/// Parent oracle: the child must ARM, must NOT survive (the clean owner
/// aborts at the decisive call), and must fail the process.
fn child_must_abort(case: &str) {
    let output = run_child(case);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("ARMED"),
        "{case}: child never armed: {stdout} {stderr}"
    );
    assert!(
        !output.status.success(),
        "{case}: clean owner must abort — child survived: {stdout} {stderr}"
    );
    assert!(
        !stdout.contains("SURVIVED"),
        "{case}: decisive call was a quiet no-op: {stdout} {stderr}"
    );
}

/// M-a: carve must drop the stale pending bit on its own start granule.
/// The bit is planted deterministically: retain-reset clears both banks,
/// the doc-hidden owner hook re-plants the START bit, the producer RMW
/// plants PENDING — the next carve of the SAME offset must clear it before
/// the LIVE write, or the drain would reclaim a block whose state byte is
/// still zero (clean owner: fatal; hence the child isolation).
fn child_carve_clear_muta() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let a = first_block_of_fresh_small_segment(heap);
    let off_a = (a.addr() & (SEGMENT - 1)) as u32;

    // SAFETY: a is live and transferred exactly once; deliberately NOT
    // drained — its pending bit is the stale bit the reset must clear.
    assert!(unsafe { try_publish(a) });
    let pin = RouteDirectory::global().lookup(a).expect("route");
    assert!(pin.pending_for_test(a), "precondition: stale bit planted");

    // SAFETY: quiescent test hook (see oneclass_starts_map T4).
    assert!(unsafe { heap.dbg_force_decommit_retain_for(a) });
    assert!(!pin.pending_for_test(a), "reset cleared the pending bank");
    assert!(
        !unsafe { try_publish(a) },
        "reset cleared the start bit (precondition for the re-plant)"
    );

    // Re-plant the start bit through the owner hook, then publish: the
    // "delayed producer" RMW lands AFTER the reset — exactly the bit the
    // next carve must clear.
    let sidecar = pin.dbg_small_sidecar_for_test().expect("small sidecar");
    sidecar.issue_for_test(off_a);
    assert!(unsafe { try_publish(a) }, "re-planted start gate passes");
    assert!(pin.pending_for_test(a), "pending bit planted on off_a");

    // The decisive carve: the next same-class allocation reuses the reset
    // bump cursor — the exact offset the stale bit points at (T4 pattern).
    println!("ARMED");
    let (_, _, _, cb0, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();
    let q = heap.alloc(l(BIG));
    assert!(!q.is_null());
    assert_eq!(
        q.addr(),
        a.addr(),
        "fixture: the fresh carve must reuse the reset offset"
    );
    let (_, _, _, cb1, _) = sefer_alloc::AllocCore::dbg_oneclass_counters();
    assert!(
        cb1 > cb0,
        "M-a RED: carve_block issued NO clear_pending_start_bit \
         (CARVE_CLEAR_BITS delta 0) — the stale pending bit on the fresh \
         block's start granule survives"
    );
    // The drain must find NOTHING: the carve consumed the stale bit.
    assert_eq!(
        heap.dbg_drain_sidecar_ingress(),
        0,
        "stale bit must not survive the carve into a reclaim"
    );
    assert_eq!(
        heap.dbg_state_byte_state_for(q),
        Some(0),
        "the fresh block stays LIVE"
    );
    assert!(!pin.pending_for_test(q), "no bit left on the fresh block");
    println!("SURVIVED");
    // SAFETY: q (== a) is live and owned; the planted record was consumed
    // by the carve-clear, so the lease drops clean.
    drop(lease);
}

/// M-d: reclaim must fatal on a zero state byte at a start. The byte is
/// zeroed legitimately (retain-reset), the start bit and pending bit are
/// re-planted through the reviewed doc-hidden hooks — the drain then cuts a
/// record whose byte is 0 at an issued start.
fn child_zero_state_byte_md() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let a = first_block_of_fresh_small_segment(heap);
    let off_a = (a.addr() & (SEGMENT - 1)) as u32;

    // SAFETY: a is live and transferred exactly once (see T4).
    assert!(unsafe { try_publish(a) });
    // SAFETY: quiescent test hook; zeroes a's state byte (F7 reset range).
    assert!(unsafe { heap.dbg_force_decommit_retain_for(a) });

    // Grow the bump again: the drain scans words `[payload_word,
    // ceil(bump/1024))`, so a record at the reset bump itself would sit in
    // the EMPTY scan window. The next same-class carve moves the bump past
    // off_a; the byte-0 record is then planted at an INTERIOR 64-aligned
    // granule strictly below the new bump (interior granules never carry a
    // state byte, so the byte there is legitimately zero).
    let q = heap.alloc(l(BIG));
    assert!(!q.is_null());
    assert_eq!(q.addr(), a.addr(), "fixture: reuse of the reset offset");
    let off_q1 = off_a + 64; // 64-aligned interior granule of q
    assert!(off_q1 < off_a + BIG as u32);
    let q1 = a.with_addr(a.addr() + 64);

    let pin = RouteDirectory::global().lookup(a).expect("route");
    let sidecar = pin.dbg_small_sidecar_for_test().expect("small sidecar");
    sidecar.issue_for_test(off_q1);
    assert!(
        unsafe { try_publish(q1) },
        "start gate passes after the hook re-plant"
    );
    assert!(
        pin.pending_for_test(q1),
        "pending record with byte 0 armed below the bump"
    );
    println!("ARMED");
    // Decisive call: the drain cuts the byte-0 record. Clean owner: fatal.
    let drained = heap.dbg_drain_sidecar_ingress();
    println!("SURVIVED (drained {drained})");
    // SAFETY: unreachable on the clean owner (fatal above).
    drop(lease);
}

/// M-l: reclaim must fatal when the claimed `(off, off+bs)` open interval
/// contains an issued start. The state byte of a live 64 B block is
/// corrupted (deliberate probe) to claim a bigger class; issued 64 B
/// neighbours start inside the claimed interval.
///
/// The state-byte region is located EXACTLY by a transition oracle: after
/// p is published and drained, its byte flips LIVE→FREE (gains 0x80) at
/// exactly one byte address — that address is `state_byte_off + granule(p)`.
fn child_overlap_claim_ml() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = l(64);

    // Fresh Small segment, then a dense run of 64 B blocks in it (fresh
    // child process: no tcache/magazine debris shifts the layout).
    let reserved_before = sefer_alloc::AllocCore::dbg_segments_reserved_total();
    let mut p0 = None;
    for _ in 0..200_000 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        if sefer_alloc::AllocCore::dbg_segments_reserved_total() > reserved_before {
            p0 = Some(p);
            break;
        }
    }
    let p0 = p0.expect("a fresh Small segment must form");
    let base = p0.addr() & !(SEGMENT - 1);
    let mut run: Vec<u32> = vec![(p0.addr() - base) as u32];
    for _ in 0..8 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        if p.addr() & !(SEGMENT - 1) == base {
            run.push((p.addr() - base) as u32);
        }
    }
    run.sort_unstable();
    run.dedup();
    assert!(
        run.len() >= 8,
        "fixture: eight carved blocks in the segment"
    );
    for w in run.windows(2) {
        assert_eq!(w[1] - w[0], 64, "fixture: consecutive 64 B carving");
    }
    // p: a 512-aligned block of the run with three issued successors.
    let mut p_off = None;
    for (i, &off) in run.iter().enumerate() {
        if off % 512 == 0
            && i + 3 < run.len()
            && run[i + 1] == off + 64
            && run[i + 2] == off + 128
            && run[i + 3] == off + 192
        {
            p_off = Some(off);
            break;
        }
    }
    let off_p = p_off.expect("fixture: a 512-aligned run block with successors");
    let p = (base + off_p as usize) as *mut u8;
    assert!(off_p + 512 <= run[run.len() - 1] + 64, "geometry fit");

    let c64 = heap.dbg_class_for(l(64)).expect("64 B class") as u8;
    let c512 = heap.dbg_class_for(l(512)).expect("512 B class") as u8;
    assert!(c512 > c64);
    let free64 = (c64 + 1) | 0x80; // encode(c64, FREE)

    // The region sits below the payload; probe a small, mapped window that
    // certainly contains `state_byte_off + granule(off_p)` for an early run
    // block (the whole run is carved right after the fresh segment formed).
    const WIN: usize = SegmentLayout::SMALL_META_END + 8192;
    let win = |addr: usize| -> u8 {
        // SAFETY: the lease exclusively owns the mapped segment; the window
        // covers metadata and the committed first payload chunk only.
        unsafe { (base as *const u8).byte_add(addr).read_volatile() }
    };

    // Transition oracle: publish p (one record), drain it (LIVE -> FREE).
    // SAFETY: p is live and transferred exactly once here.
    assert!(unsafe { try_publish(p) });
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1, "reclaimed once");
    let changed: Vec<usize> = (0..WIN).filter(|&a| win(a) == free64).collect();
    assert_eq!(
        changed.len(),
        1,
        "fixture: exactly one byte carries encode(c64, FREE)"
    );
    let a_true = changed[0];
    assert_eq!(
        win(a_true),
        free64,
        "fixture: the state byte of the reclaimed block flipped to FREE"
    );

    // Publish p AGAIN (its start bit survives reclaim — monotone; the FREE
    // byte only matters to the owner). The pending record is armed.
    // SAFETY: deliberate duplicate-free probe; the owner-side verdict on the
    // CORRUPTED byte is the behavior under test.
    assert!(unsafe { try_publish(p) }, "start bit survives the reclaim");

    // Deliberate corruption: p's byte now claims class 512 B, state LIVE —
    // geometrically valid (offset-aligned, end <= bump) but spanning the
    // issued starts at off+64/128/192 inside the open interval.
    // SAFETY: single-byte write into the lease-owned segment metadata; the
    // decisive reclaim runs before any other access to that byte.
    unsafe { (base as *mut u8).byte_add(a_true).write_volatile(c512 + 1) };
    println!("ARMED");
    // Decisive call: clean owner fatals on the overlap; the mutant accepts.
    let drained = heap.dbg_drain_sidecar_ingress();
    println!("SURVIVED (drained {drained})");
    // SAFETY: unreachable on the clean owner (fatal above).
    drop(lease);
}

/// M-sb (round D1′ fix 2b): a pending record whose start granule has NO
/// start bit must fatal BEFORE the quiet FREE/MAG no-op returns. The byte
/// is driven to FREE legitimately (publish + drain), the record is
/// re-armed (pending + start), and then the start bit alone is unset
/// through the reviewed doc-hidden `dbg_unset_start_bit_for_test` hook —
/// the drain then cuts a FREE-byte record with start bit 0.
fn child_free_record_start_bit_0() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let a = first_block_of_fresh_small_segment(heap);
    let off_a = (a.addr() & (SEGMENT - 1)) as u32;

    // SAFETY: a is live and transferred exactly once (T4 pattern).
    assert!(unsafe { try_publish(a) });
    assert_eq!(
        heap.dbg_drain_sidecar_ingress(),
        1,
        "reclaimed once: byte is FREE"
    );
    // SAFETY: deliberate duplicate-free probe (stale-record shape).
    assert!(unsafe { try_publish(a) }, "start bit survives the reclaim");

    // Corrupt: unset ONLY the start bit — pending stays armed, byte FREE.
    let pin = RouteDirectory::global().lookup(a).expect("route");
    let sidecar = pin.dbg_small_sidecar_for_test().expect("small sidecar");
    sidecar.dbg_unset_start_bit_for_test(off_a);
    assert_eq!(
        RouteRegistration::start_state_at_global_address_for_test(a.addr()),
        Some((false, true)),
        "fixture: pending armed, start bit unset"
    );
    println!("ARMED");
    // Decisive call: the drain cuts the FREE-byte record with start bit 0.
    // Clean owner: fatal BEFORE the quiet FREE reject. The reordering mutant
    // (start-bit check after the FREE/MAG returns) or a deleted fatal lets
    // the child survive → RED.
    let drained = heap.dbg_drain_sidecar_ingress();
    println!("SURVIVED (drained {drained})");
    // SAFETY: unreachable on the clean owner (fatal above).
    drop(lease);
}

/// M-k (round D1′ fix 2): a pending record whose start granule has NO
/// start bit must fatal even when the record's state byte is LIVE. The
/// record is armed on the live route itself (issue_for_test + a producer
/// publish of the live block — pending armed, byte `encode(c, LIVE)`), and
/// then the start bit alone is unset through the reviewed doc-hidden
/// `dbg_unset_start_bit_for_test` hook — the drain then cuts a LIVE-byte
/// record with start bit 0.
fn child_live_record_start_bit_0() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let a = first_block_of_fresh_small_segment(heap);
    let off_a = (a.addr() & (SEGMENT - 1)) as u32;

    // SAFETY: a is live and transferred exactly once (T4 pattern); the
    // pending record is deliberately NOT drained here — its LIVE byte is
    // the point of this case (M-sb covers the FREE-byte shape).
    assert!(unsafe { try_publish(a) });
    let pin = RouteDirectory::global().lookup(a).expect("route");
    assert!(
        pin.pending_for_test(a),
        "fixture: pending armed on the live block"
    );

    // Corrupt: unset ONLY the start bit — pending stays armed, byte LIVE.
    let sidecar = pin.dbg_small_sidecar_for_test().expect("small sidecar");
    sidecar.dbg_unset_start_bit_for_test(off_a);
    assert_eq!(
        RouteRegistration::start_state_at_global_address_for_test(a.addr()),
        Some((false, true)),
        "fixture: pending armed, start bit unset, byte LIVE"
    );
    println!("ARMED");
    // Decisive call: the drain cuts the LIVE-byte record with start bit 0.
    // Clean owner: fatal ("published record has start bit 0") BEFORE any
    // reclaim work. A deleted fatal lets the child survive → RED.
    let drained = heap.dbg_drain_sidecar_ingress();
    println!("SURVIVED (drained {drained})");
    // SAFETY: unreachable on the clean owner (fatal above).
    drop(lease);
}

/// Parent entry for the abort-expecting cases.
#[test]
fn ocmut_child() {
    let Some(case) = std::env::var_os(CASE_ENV) else {
        // Invoked as an ordinary test (no case selected): nothing to do.
        return;
    };
    match case.to_str().expect("utf8 case") {
        "carve_clear_muta" => child_carve_clear_muta(),
        "zero_state_byte_md" => child_zero_state_byte_md(),
        "overlap_claim_ml" => child_overlap_claim_ml(),
        "free_record_start_bit_0" => child_free_record_start_bit_0(),
        "live_record_start_bit_0" => child_live_record_start_bit_0(),
        other => panic!("unknown ocmut case {other}"),
    }
}

/// M-a — see the module docs / `child_carve_clear_muta`.
/// The M-a child SURVIVES on the clean tree (the carve clears the bit, so no
/// byte-0 reclaim happens) — the parent asserts full success.
fn child_must_survive(case: &str) {
    let output = run_child(case);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("ARMED") && output.status.success(),
        "{case}: child must survive on the clean tree: {stdout} {stderr}"
    );
    assert!(stdout.contains("SURVIVED"), "{case}: {stdout} {stderr}");
}

/// M-a — see the module docs / `child_carve_clear_muta`.
#[test]
fn muta_carve_clear_drops_stale_pending_at_recarve() {
    let _guard = serialize();
    child_must_survive("carve_clear_muta");
}

/// M-d — the clean owner must ABORT (fatal on the zero byte at a start);
/// a quiet counter no-op (the mutant) lets the child survive → RED.
#[test]
fn mutd_reclaim_zero_state_byte_at_start_is_fatal() {
    let _guard = serialize();
    child_must_abort("zero_state_byte_md");
}

/// M-l — the clean owner must ABORT (fatal on the overlapping claim); the
/// mutant accepts the corrupted record → RED.
#[test]
fn mutl_reclaim_overlap_of_issued_start_is_fatal() {
    let _guard = serialize();
    child_must_abort("overlap_claim_ml");
}

/// M-sb — the clean owner must ABORT (fatal on start bit 0 BEFORE the
/// quiet FREE reject); the reordering or fatal-removal mutant lets the
/// child survive → RED.
#[test]
fn mutsb_reclaim_start_bit_0_before_quiet_reject_is_fatal() {
    let _guard = serialize();
    child_must_abort("free_record_start_bit_0");
}

/// M-k — the clean owner must ABORT (fatal on start bit 0 of a LIVE-byte
/// record); a deleted fatal lets the child survive → RED.
#[test]
fn mutk_reclaim_live_record_start_bit_0_is_fatal() {
    let _guard = serialize();
    child_must_abort("live_record_start_bit_0");
}

/// M-c′ — the retain-reset clear range is EXACTLY `[payload_start,
/// old_bump)`: stale pending bits at the low, middle AND high granules of
/// that range must all be cleared (any narrowed upper bound leaves the
/// middle/high bits → RED). A "range starts at 0" mutation is equivalent
/// (no legitimate bit ever exists below payload_start) and deliberately not
/// the target here — see the module docs.
#[test]
fn mutc_retain_reset_range_is_exact_low_middle_high() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = l(BIG);

    // Six 64 KiB blocks spanning a wide [payload_start, old_bump) range in
    // ONE fresh Small segment (T4 pattern: the block at which the reserved
    // total grows is the fresh segment's first carve).
    let reserved_before = sefer_alloc::AllocCore::dbg_segments_reserved_total();
    let mut blocks: Vec<*mut u8> = Vec::new();
    for _ in 0..20_000 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        let grew = sefer_alloc::AllocCore::dbg_segments_reserved_total() > reserved_before;
        if grew {
            blocks.push(p);
            break;
        }
    }
    assert!(!blocks.is_empty(), "fixture: a fresh Small segment formed");
    let base = blocks[0].addr() & !(SEGMENT - 1);
    let mut attempts = 0;
    while blocks.len() < 6 {
        attempts += 1;
        assert!(attempts <= 20_000, "fixture: fresh segment never filled");
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        if p.addr() & !(SEGMENT - 1) != base {
            continue; // served elsewhere; the fresh segment fills later
        }
        blocks.push(p);
    }
    assert_eq!(blocks.len(), 6, "fixture: six blocks in the fresh segment");

    let low = blocks[0];
    let mid = blocks[3];
    let high = blocks[5];
    for b in [low, mid, high] {
        // SAFETY: each block is live and transferred exactly once; the
        // reset below is the consumer of these records.
        assert!(unsafe { try_publish(b) }, "stale bit planted");
    }
    let pin = RouteDirectory::global().lookup(low).expect("route");
    assert!(pin.pending_for_test(low) && pin.pending_for_test(mid) && pin.pending_for_test(high));

    // SAFETY: quiescent test hook (T4 pattern).
    assert!(unsafe { heap.dbg_force_decommit_retain_for(low) });
    for (name, b) in [("low", low), ("middle", mid), ("high", high)] {
        assert!(
            !pin.pending_for_test(b),
            "M-c′ RED: retain-reset left the stale PENDING bit at the {name} \
             granule — the clear range's upper bound is below old_bump"
        );
        assert!(
            !unsafe { try_publish(b) },
            "retain-reset must clear the START bit at the {name} granule too"
        );
    }

    // Control: the segment re-carves from its reset bump cursor and the
    // fresh block publishes and reclaims exactly once.
    let q = heap.alloc(layout);
    assert!(!q.is_null());
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    // SAFETY: q is live and transferred exactly once here.
    assert!(unsafe { try_publish(q) });
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    // SAFETY: retired above.
    drop(lease);
}

/// M-e — a reclaimed record must land at the head of ITS OWN class bin:
/// re-issuing the SAME Layout returns the SAME address, and the direct
/// `owner_state_for_test` bin-head probe agrees; a foreign class gets
/// nothing of it.
#[test]
fn mute_reclaim_bins_into_the_state_byte_class() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    let p = heap.alloc(l(64));
    assert!(!p.is_null());
    let q = heap.alloc(l(64));
    assert!(!q.is_null());
    let base = p.addr() & !(SEGMENT - 1);
    let off_p = (p.addr() - base) as u32;
    let c64 = heap.dbg_class_for(l(64)).expect("64 B class");
    let c16 = heap.dbg_class_for(l(16)).expect("16 B class");

    // SAFETY: p is live and transferred exactly once here.
    assert!(unsafe { try_publish(p) });
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1, "reclaimed once");

    // Direct bin oracle: p must be the head of the 64 B bin, of NO other.
    let (_, _, head64) = unsafe {
        sefer_alloc::registry::segment_route::SmallSidecar::owner_state_for_test(p.addr(), c64)
    }
    .expect("owned segment");
    assert_eq!(
        head64, off_p,
        "M-e RED: the reclaimed block is not at the head of its own class bin"
    );
    if c16 != c64 {
        let (_, _, head16) = unsafe {
            sefer_alloc::registry::segment_route::SmallSidecar::owner_state_for_test(p.addr(), c16)
        }
        .expect("owned segment");
        assert_ne!(
            head16, off_p,
            "M-e RED: the reclaimed block leaked into a foreign class bin"
        );
    }

    // The reclaimed block committed FREE in its own class.
    assert_eq!(
        heap.dbg_state_byte_state_for(p),
        Some(2),
        "the reclaim committed the byte to FREE"
    );
    // SAFETY: retired above.
    drop(lease);
    let _ = q;
}

/// M-f — the producer payload filter fires BEFORE the start-bit gate: a
/// METADATA granule of a Primordial-kind route
/// (`[SMALL_META_END, PRIMORDIAL_META_END)`) whose start bit was set by the
/// owner hook must not publish and must plant no pending bit. Removing the
/// payload filter (M-f) lets the hook-set start bit through → RED.
#[test]
fn mutf_publish_metadata_offset_is_rejected_before_the_start_gate() {
    let _guard = serialize();

    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).expect("reservation");
    let root = reservation.as_ptr();
    let directory = RouteDirectory::new();
    let registration = directory
        .register(root, SEGMENT, root, 17, RouteKind::Primordial)
        .expect("primordial registration");
    let sidecar = registration.small_sidecar().expect("small sidecar");

    let meta_lo = SegmentLayout::SMALL_META_END; // 4 KiB-aligned, 16-aligned
    assert!(
        meta_lo < SegmentLayout::PRIMORDIAL_META_END,
        "fixture: a granule strictly between the two metadata ends exists"
    );
    assert_eq!(meta_lo % 16, 0);

    // The owner hook sets the START bit — under the mutant (payload filter
    // removed) this is exactly the second gate the publication would sail
    // through.
    sidecar.issue_for_test(meta_lo as u32);
    // SAFETY: the pin identifies the fixture's own hook-issued granule; the
    // publication's verdict is the behavior under test.
    let pin = directory.lookup(root).expect("route pin");
    assert!(
        !unsafe { pin.publish_small_borrowed(meta_lo as u32) },
        "M-f RED: a metadata granule published — the payload filter is gone"
    );
    assert!(
        !pin.pending_for_test(root),
        "M-f RED: the rejected metadata publication planted a pending bit"
    );

    // Control: the same sidecar accepts a payload granule through the very
    // same gates (the start bit for it is hook-set as well).
    let payload_off = SegmentLayout::PRIMORDIAL_META_END;
    assert_eq!(payload_off % 16, 0);
    sidecar.issue_for_test(payload_off as u32);
    // SAFETY: as above.
    assert!(
        unsafe { pin.publish_small_borrowed(payload_off as u32) },
        "control: a legitimate payload granule must publish"
    );
    assert!(
        pin.pending_for_test(root.with_addr(root.addr() + payload_off)),
        "control: the control publication planted its pending bit"
    );
}
