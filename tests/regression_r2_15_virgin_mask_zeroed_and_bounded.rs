//! R2-15: regression coverage for stale `u16` virgin-mask bits and oversized
//! output slices. Calls the internals-only safe forwarder; exact carve
//! assertions are skipped under Miri, where virgin marking is intentionally
//! disabled. A deterministic partial/OOM refill cannot be induced here.
//!
//! Oversized refills no longer panic (R6-T14): the magazine assert was
//! converted to a `fatal` abort, so the two oversized-length cases run in a
//! spawned child process (see `con01_exit_stranded_lock.rs` for the pattern)
//! and the parent asserts the ABORT exit signature. The child trigger tests
//! are inert in a normal `cargo test` run (env-var gate).

#![cfg(all(
    feature = "internals",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "virgin-zero-skip"
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// Poison value for the caller's mask: every bit set, so ANY stale bit
/// surviving a call is visible in the exact-equality asserts below.
const POISON: u16 = 0xFFFF;

fn fresh_core() -> AllocCore {
    AllocCore::new().expect("AllocCore::new (primordial reservation) failed")
}

/// A small class (96 B / align 8 — small in every size-class table this
/// crate ships, including `medium-classes`) on a FRESH core, so the test's
/// refills are deterministic: a fresh core's first refill for the class is
/// one all-virgin `carve_batch` run, and the only blocks that can ever sit
/// on its freelist are the ones this test itself freed.
fn pick_class() -> usize {
    SegmentLayout::class_for(96, 8)
        .expect("96 B / align 8 is a small class in every size-class table")
}

#[test]
fn zero_len_refill_zeroes_even_a_poisoned_mask() {
    let mut core = fresh_core();
    let c = pick_class();
    let mut out: [*mut u8; 0] = [];
    let mut mask: u16 = POISON;
    let filled = core.refill_class_bump_virgin(c, &mut out, &mut mask);
    assert_eq!(filled, 0);
    assert_eq!(
        mask, 0,
        "a zero-length refill must leave the mask fully zeroed — a poisoned \
         input mask must not survive (stale-bits defect, boundary case len=0)"
    );
}

#[test]
fn recycled_then_fresh_mixed_refill_mask_is_exact() {
    if cfg!(miri) {
        // carve-path bit-set withheld under miri by design (module doc)
        return;
    }
    let mut core = fresh_core();
    let c = pick_class();
    let layout = Layout::from_size_align(96, 8).unwrap();

    // Stage 1: carve 4 virgin blocks (fresh core → one all-virgin run),
    // proving the carve bits DO get set on this configuration, then dirty
    // and free them → 4 recycled blocks on the class freelist. Freed blocks
    // are NEVER virgin (dispatch conjunct false in the refill doc).
    let mut stage1 = vec![core::ptr::null_mut::<u8>(); 4];
    let mut scratch: u16 = 0;
    let got1 = core.refill_class_bump_virgin(c, &mut stage1, &mut scratch);
    assert_eq!(got1, 4);
    assert_eq!(
        scratch, 0b1111,
        "4 carved blocks from a fresh virgin segment must promise exactly bits 0..4"
    );
    for &p in &stage1 {
        // SAFETY (R6-MS-1/2): `p` was returned by the refill above, is live
        // here, and writing 8 bytes stays within its block (block_size >= 96).
        unsafe { core::ptr::write_bytes(p, 0xAA, 8) };
        // SAFETY (R6-MS-1/2): matching layout, live block, freed exactly once.
        unsafe { core.dealloc(p, layout) };
    }

    // Stage 2: 16-slot refill = drain the 4 recycled + carve 12 fresh. The
    // recycled prefix (slots 0..4) must stay CLEAR, the carved suffix
    // (slots 4..16) set, and the poisoned input must be gone everywhere.
    let mut out = vec![core::ptr::null_mut::<u8>(); 16];
    let mut mask: u16 = POISON;
    let filled = core.refill_class_bump_virgin(c, &mut out, &mut mask);
    assert_eq!(filled, 16);
    assert_eq!(
        mask, 0b1111_1111_1111_0000,
        "recycled prefix must be clear, carved suffix set, and no stale poison \
         bit may survive (stale-bits defect on mask reuse)"
    );
    // Distinct, non-null pointers in the promised prefix order.
    assert!(out.iter().all(|&p| !p.is_null()));
    assert_eq!(
        out.iter()
            .map(|&p| p as usize)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        16,
        "refill must not alias slots"
    );
}

#[test]
fn fully_recycled_refill_mask_is_all_clear() {
    if cfg!(miri) {
        return;
    }
    let mut core = fresh_core();
    let c = pick_class();
    let layout = Layout::from_size_align(96, 8).unwrap();

    // 16 fresh-carved virgin blocks → full mask (len=16 boundary: exactly
    // the whole u16 promised).
    let mut out = vec![core::ptr::null_mut::<u8>(); 16];
    let mut scratch: u16 = 0;
    let got = core.refill_class_bump_virgin(c, &mut out, &mut scratch);
    assert_eq!(got, 16);
    assert_eq!(
        scratch,
        u16::MAX,
        "16 carved blocks from a fresh segment: all 16 bits promised (len=16 boundary)"
    );

    // Dirty + free ALL 16 → refill drains only recycled blocks. The whole
    // mask must come back ZEROED even with a poisoned input: no carve runs
    // happen, so pre-fix the poison would have survived verbatim.
    for &p in &out {
        // SAFETY (R6-MS-1/2): live refill-issued block; 8 bytes fit.
        unsafe { core::ptr::write_bytes(p, 0x5A, 8) };
        // SAFETY (R6-MS-1/2): matching layout, freed exactly once.
        unsafe { core.dealloc(p, layout) };
    }
    let mut out2 = vec![core::ptr::null_mut::<u8>(); 16];
    let mut mask: u16 = POISON;
    let got2 = core.refill_class_bump_virgin(c, &mut out2, &mut mask);
    assert_eq!(got2, 16);
    assert_eq!(
        mask, 0,
        "every slot came from the freelist (recycled = never virgin); a poisoned \
         input mask must not survive the refill"
    );
}

/// Env var gating the oversized-refill child: without it the trigger tests
/// return immediately, keeping a normal `cargo test` run alive.
const ABORT_CHILD: &str = "SEFER_R2_15_ABORT_CHILD";

fn spawn_abort_child(child_test: &str) -> std::process::ExitStatus {
    let exe = std::env::current_exe().expect("test executable");
    std::process::Command::new(&exe)
        .args(["--exact", child_test, "--nocapture", "--test-threads", "1"])
        .env(ABORT_CHILD, "1")
        .status()
        .expect("spawn own child for the oversized-refill abort probe")
}

fn assert_abort_signature(status: std::process::ExitStatus) {
    // Windows: UCRT abort exit 3, or fastfail 0xC0000409 (-1073740791).
    // Unix: SIGABRT (signal 6).
    let aborted = cfg!(windows) && matches!(status.code(), Some(3) | Some(-1073740791))
        || cfg!(unix) && format!("{status}").contains("signal: 6");
    assert!(
        aborted,
        "the oversized refill must ABORT process-wide (fatal, R6-T14), not \
         panic and not succeed; got {status}. A panic in the child would make \
         libtest exit 101 cleanly, turning this assert red against the \
         pre-fix behaviour"
    );
}

#[test]
fn out_len_17_aborts_process_wide() {
    if cfg!(miri) {
        return;
    }
    let status = spawn_abort_child("out_len_17_aborts_child");
    assert_abort_signature(status);
}

#[test]
fn out_len_32_aborts_process_wide() {
    if cfg!(miri) {
        return;
    }
    let status = spawn_abort_child("out_len_32_aborts_child");
    assert_abort_signature(status);
}

#[test]
fn out_len_17_aborts_child() {
    if std::env::var(ABORT_CHILD).is_err() {
        return; // inert in a normal `cargo test` run
    }
    let mut core = fresh_core();
    let c = pick_class();
    let mut out = vec![core::ptr::null_mut::<u8>(); 17];
    let mut mask: u16 = 0;
    let _ = core.refill_class_bump_virgin(c, &mut out, &mut mask);
}

#[test]
fn out_len_32_aborts_child() {
    if std::env::var(ABORT_CHILD).is_err() {
        return; // inert in a normal `cargo test` run
    }
    let mut core = fresh_core();
    let c = pick_class();
    let mut out = vec![core::ptr::null_mut::<u8>(); 32];
    let mut mask: u16 = 0;
    let _ = core.refill_class_bump_virgin(c, &mut out, &mut mask);
}
