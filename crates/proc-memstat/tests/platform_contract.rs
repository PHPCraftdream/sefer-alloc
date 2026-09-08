//! The per-platform CONTRACT of `snapshot()` — the axes review P3-3 showed
//! the relation-only checks in `tests/monotonicity.rs` cannot see.
//!
//! `after > before`, `peak >= rss`, and `peak_after >= peak_before` all
//! survive (a) a backend that reports the raw KiB figures of
//! `/proc/self/status` where the crate documents BYTES, and (b) a backend
//! that has quietly stopped producing `peak_rss` at all — `(None, None)`
//! compares equal to itself. This file pins, per platform:
//!
//! 1. AVAILABILITY — which counters MUST be `Some`, which MUST be `None`
//!    (the crate-doc platform matrix), and which figure
//!    `charged_or_reserved_bytes()` must return;
//! 2. UNITS — on Linux, that the kB figures of `/proc/self/status` are
//!    scaled to bytes. The `* 1024` lives in the backend, not in the parser
//!    that `tests/status_parse.rs` already covers with exact-value oracles —
//!    so this file parses the SAME `/proc/self/status` independently and
//!    holds the backend's output against the documented `kB * 1024`;
//! 3. NEGATIVE CONTROLS — every oracle here is fed the exact value shapes of
//!    the historical defects (dead peak counter, dropped `* 1024`, a
//!    `* 1000` slip, crossed backends) and must REJECT them, on every host,
//!    under miri too — so the live assertions cannot silently be vacuous.
//!
//! Read-only: nothing here moves process memory, so — unlike
//! `tests/monotonicity.rs` — no fresh-process isolation is needed.
//!
//! The parser reaches the test via `#[path]` — the sanctioned pattern (see
//! `tests/status_parse.rs`); the review explicitly ruled out widening the
//! crate's public API to expose the private parser.

use proc_memstat::{snapshot, MemStat};

// Only the Linux live test calls the parser; gating the module the same way
// keeps it from being dead code on Windows/macOS/stub hosts.
#[cfg(all(target_os = "linux", not(miri)))]
#[path = "../src/status_parse.rs"]
mod status_parse;

// ---------------------------------------------------------------------------
// Contract oracles — pure functions of a `MemStat` (plus, for the unit
// oracle, the raw kB figure), so the negative controls below can feed them
// fabricated defect shapes on ANY host.
// ---------------------------------------------------------------------------

/// Linux must report `rss > 0`, `virtual_size` (`VmSize`) and `peak_rss`
/// (`VmHWM`) present, `commit_charge` ABSENT — `/proc/self/status` carries
/// no commit counter and `VmSize` is address space, a different quantity —
/// and `charged_or_reserved_bytes()` == `virtual_size`.
fn expect_linux_shape(m: &MemStat) {
    assert!(m.rss > 0, "Linux rss must be non-zero, got {}", m.rss);
    let vs = m
        .virtual_size
        .expect("Linux must report virtual_size (VmSize)");
    assert!(
        m.commit_charge.is_none(),
        "Linux must leave commit_charge None (no commit counter in \
         /proc/self/status); got {:?}",
        m.commit_charge
    );
    let peak = m.peak_rss.expect(
        "Linux must report peak_rss (VmHWM) — a permanently-None peak is a \
         broken backend, not a stub platform",
    );
    assert!(
        peak >= m.rss,
        "peak_rss ({peak}) must be >= live rss ({})",
        m.rss
    );
    assert_eq!(
        m.charged_or_reserved_bytes(),
        vs,
        "charged_or_reserved_bytes must return virtual_size on Linux"
    );
}

/// Windows must report `rss > 0`, `commit_charge` (`PagefileUsage`) and
/// `peak_rss` (`PeakWorkingSetSize`) present, `virtual_size` ABSENT
/// (`PROCESS_MEMORY_COUNTERS` carries no such field), and
/// `charged_or_reserved_bytes()` == `commit_charge`.
fn expect_windows_shape(m: &MemStat) {
    assert!(m.rss > 0, "Windows rss must be non-zero, got {}", m.rss);
    assert!(
        m.virtual_size.is_none(),
        "Windows must leave virtual_size None (PROCESS_MEMORY_COUNTERS has no \
         virtual-size field); got {:?}",
        m.virtual_size
    );
    let cc = m
        .commit_charge
        .expect("Windows must report commit_charge (PagefileUsage)");
    let peak = m.peak_rss.expect(
        "Windows must report peak_rss (PeakWorkingSetSize) — a \
         permanently-None peak is a broken backend, not a stub platform",
    );
    assert!(
        peak >= m.rss,
        "peak_rss ({peak}) must be >= live rss ({})",
        m.rss
    );
    assert_eq!(
        m.charged_or_reserved_bytes(),
        cc,
        "charged_or_reserved_bytes must return commit_charge on Windows"
    );
}

/// macOS must report `rss > 0`, `virtual_size` and `peak_rss`
/// (`resident_size_max`) present, `commit_charge` ABSENT
/// (`MACH_TASK_BASIC_INFO` carries no commit counter), and
/// `charged_or_reserved_bytes()` == `virtual_size`.
fn expect_macos_shape(m: &MemStat) {
    assert!(m.rss > 0, "macOS rss must be non-zero, got {}", m.rss);
    let vs = m.virtual_size.expect("macOS must report virtual_size");
    assert!(
        m.commit_charge.is_none(),
        "macOS must leave commit_charge None (MACH_TASK_BASIC_INFO carries no \
         commit counter); got {:?}",
        m.commit_charge
    );
    let peak = m.peak_rss.expect(
        "macOS must report peak_rss (resident_size_max) — a permanently-None \
         peak is a broken backend, not a stub platform",
    );
    assert!(
        peak >= m.rss,
        "peak_rss ({peak}) must be >= live rss ({})",
        m.rss
    );
    assert_eq!(
        m.charged_or_reserved_bytes(),
        vs,
        "charged_or_reserved_bytes must return virtual_size on macOS"
    );
}

/// One Linux field must be `kb * 1024` — BYTES, not the raw kB figure
/// `/proc/self/status` reports. Three legs, each catching what the previous
/// one lets through:
///
/// 1. PAGE GRANULARITY: every `Vm*` figure is derived from page counts
///    (`PAGE_SHIFT >= 12` on every mainline target), so the kB value is a
///    multiple of >= 4 and correctly scaled bytes are a multiple of 4096.
///    This is the leg that catches a subtle `* 1000` slip, which the band
///    below (±25%) is too wide for (a `* 1000` value is only 2.4% off).
/// 2. STRICTLY ABOVE the raw kB figure: a DROPPED `* 1024` reports the kB
///    number itself. Two adjacent reads of a quiet process see the same
///    value, and RSS cannot shrink ~1024-fold between them — so this leg
///    catches the drop even when the kB figure is 4096-aligned by luck.
/// 3. JITTER BAND around `kb * 1024`: the live caller snapshots and parses
///    at slightly different instants, so exact equality cannot be demanded —
///    but a 1024x scale error is orders of magnitude outside any drift band.
fn expect_byte_scaled(actual_bytes: u64, kb: u64, field: &str) {
    assert_eq!(
        actual_bytes % 4096,
        0,
        "{field}: scaled-to-bytes must be page-granular (multiple of 4096); \
         got {actual_bytes}"
    );
    assert!(
        actual_bytes > kb,
        "{field}: byte value ({actual_bytes}) must strictly exceed the raw kB \
         figure ({kb}) — a value AT the kB figure means the `* 1024` scale \
         step is gone"
    );
    let expected = kb
        .checked_mul(1024)
        .expect("kB figure small enough to scale without overflow");
    assert!(
        actual_bytes >= (expected / 4) * 3 && actual_bytes <= (expected / 4) * 5,
        "{field}: byte value ({actual_bytes}) must sit within ±25% of \
         kB*1024 = {expected} (read jitter); it is a 1024x scale error away"
    );
}

/// Assert `f` PANICS — the negative-control harness. Without this check the
/// live assertions above could themselves be vacuous (an oracle that accepts
/// everything "passes" every live snapshot).
fn rejects_what(f: impl FnOnce() + std::panic::UnwindSafe, what: &str) {
    assert!(
        std::panic::catch_unwind(f).is_err(),
        "contract oracle ACCEPTED {what} — the live assertions would be vacuous"
    );
}

// ---------------------------------------------------------------------------
// Negative controls — run on EVERY host (Windows/macOS/Linux/miri/stub):
// they feed the oracles the exact value shapes of the defects review P3-3
// demonstrated and require rejection.
// ---------------------------------------------------------------------------

#[test]
fn shape_oracles_reject_a_dead_peak_counter() {
    // The P3-3 defect: a backend that stopped producing peak_rss. `(None,
    // None)` compared equal to itself before; it must now fail everywhere.
    rejects_what(
        || {
            expect_linux_shape(&MemStat {
                rss: 8192,
                virtual_size: Some(1 << 20),
                commit_charge: None,
                peak_rss: None,
            })
        },
        "Linux-shaped values with peak_rss: None",
    );
    rejects_what(
        || {
            expect_windows_shape(&MemStat {
                rss: 8192,
                virtual_size: None,
                commit_charge: Some(1 << 20),
                peak_rss: None,
            })
        },
        "Windows-shaped values with peak_rss: None",
    );
    rejects_what(
        || {
            expect_macos_shape(&MemStat {
                rss: 8192,
                virtual_size: Some(1 << 20),
                commit_charge: None,
                peak_rss: None,
            })
        },
        "macOS-shaped values with peak_rss: None",
    );
}

#[test]
fn shape_oracles_reject_crossed_backends() {
    // Windows' commit figure surfacing on Linux (and vice versa) is exactly
    // the quantity confusion the commit/virtual split exists to prevent.
    rejects_what(
        || {
            expect_linux_shape(&MemStat {
                rss: 8192,
                virtual_size: Some(1 << 20),
                commit_charge: Some(1 << 20),
                peak_rss: Some(1 << 21),
            })
        },
        "commit_charge present on Linux",
    );
    rejects_what(
        || {
            expect_windows_shape(&MemStat {
                rss: 8192,
                virtual_size: Some(1 << 20),
                commit_charge: None,
                peak_rss: Some(1 << 21),
            })
        },
        "commit_charge absent on Windows",
    );
    rejects_what(
        || {
            expect_macos_shape(&MemStat {
                rss: 8192,
                virtual_size: Some(1 << 20),
                commit_charge: Some(1 << 20),
                peak_rss: Some(1 << 21),
            })
        },
        "commit_charge present on macOS",
    );
}

#[test]
fn byte_scale_oracle_rejects_kib_reported_as_bytes() {
    // A DROPPED `* 1024`, with the kB figure chosen 4096-aligned so leg 1
    // (granularity) alone cannot catch it — leg 2 (strictly-above) must:
    // 4096 is NOT > 4096.
    rejects_what(
        || expect_byte_scaled(4096, 4096, "VmRSS"),
        "a raw kB figure (4096 kB) reported as bytes",
    );
    // A `* 1000` slip, which legs 2-3 tolerate (2.4% off) — leg 1 must:
    // 12_345_000 % 4096 == 3_752 != 0.
    rejects_what(
        || expect_byte_scaled(12_345_000, 12_345, "VmRSS"),
        "a x1000 scale slip",
    );
}

#[test]
fn byte_scale_oracle_accepts_correct_scaling() {
    // Positive control: the oracle must accept exactly what the contract
    // demands — otherwise the live Linux assertion could never pass at all.
    expect_byte_scaled(4096 * 1024, 4096, "VmRSS");
}

// ---------------------------------------------------------------------------
// Live contract tests — one per platform, cfg'd to where each backend runs.
// ---------------------------------------------------------------------------

/// Stub target (miri / unknown OS): the documented contract is HONEST ZEROS —
/// a positive contract, not an excuse: the stub must not fabricate figures.
#[cfg(any(miri, not(any(target_os = "linux", windows, target_os = "macos"))))]
#[test]
fn stub_snapshot_reports_honest_zeros() {
    let m = snapshot();
    assert_eq!(
        m,
        MemStat {
            rss: 0,
            virtual_size: None,
            commit_charge: None,
            peak_rss: None,
        },
        "stub must report honest zeros, got {m:?}"
    );
    assert_eq!(m.charged_or_reserved_bytes(), 0);
}

#[cfg(all(target_os = "linux", not(miri)))]
#[test]
fn linux_snapshot_reports_the_documented_counters() {
    expect_linux_shape(&snapshot());
}

/// The KiB-for-bytes oracle, live: an independent parse of the SAME file the
/// backend reads, held against the backend's output for all three `Vm*`
/// fields. Dropping the backend's `* 1024` reports KiB and fails this.
#[cfg(all(target_os = "linux", not(miri)))]
#[test]
fn linux_snapshot_scales_kib_fields_to_bytes() {
    let m = snapshot();
    expect_linux_shape(&m);
    let status = std::fs::read("/proc/self/status").expect("read /proc/self/status");
    let kb = |field: &[u8]| -> u64 {
        status_parse::read_kib_field(&status, field)
            .unwrap_or_else(|| panic!("{field:?} missing from /proc/self/status"))
    };
    expect_byte_scaled(m.rss, kb(b"VmRSS:"), "VmRSS");
    expect_byte_scaled(
        m.virtual_size.expect("virtual_size present on Linux"),
        kb(b"VmSize:"),
        "VmSize",
    );
    expect_byte_scaled(
        m.peak_rss.expect("peak_rss present on Linux"),
        kb(b"VmHWM:"),
        "VmHWM",
    );
}

#[cfg(all(windows, not(miri)))]
#[test]
fn windows_snapshot_reports_the_documented_counters() {
    expect_windows_shape(&snapshot());
}

#[cfg(all(target_os = "macos", not(miri)))]
#[test]
fn macos_snapshot_reports_the_documented_counters() {
    expect_macos_shape(&snapshot());
}
