//! `try_snapshot` must distinguish a failed reading from a small process
//! (review P4-1).
//!
//! The gap this closes: `snapshot()` reports zeros both when the process
//! genuinely holds almost nothing AND when the platform read failed, so a
//! before/after pair whose second read failed is indistinguishable from a
//! complete release of memory. `try_snapshot` says which happened.

use proc_memstat::{snapshot, try_snapshot, SnapshotError};

/// On every platform this test can run on, a real reading must be available —
/// so the fallible call must SUCCEED here, not merely "not panic".
///
/// Gated to the supported targets: on the stub the contract is the opposite
/// one, asserted below.
#[cfg(all(any(target_os = "linux", windows, target_os = "macos"), not(miri)))]
#[test]
fn a_supported_platform_returns_ok_with_a_real_rss() {
    let m = try_snapshot().expect("a supported platform must produce a reading");
    assert!(
        m.rss > 0,
        "a live process must have a non-zero rss; got {m:?}"
    );
}

/// The stub target must report WHY there is no reading rather than returning
/// zeros that look like a real measurement.
#[cfg(any(miri, not(any(target_os = "linux", windows, target_os = "macos"))))]
#[test]
fn the_stub_target_reports_unsupported_rather_than_zeros() {
    assert_eq!(try_snapshot(), Err(SnapshotError::Unsupported));
}

/// `snapshot()` stays the best-effort wrapper: the `Ok` reading's SHAPE when
/// there is one, and the documented all-zero fallback when there is not.
///
/// This is the compatibility guarantee the rest of the workspace relies on —
/// adding the fallible entry point must not have changed what `snapshot()`
/// returns.
///
/// **This test is an AVAILABILITY check only, and cannot prove the
/// Err→default causation its subject once claimed** (review round 2,
/// P3-3b): `snapshot()` and `try_snapshot()` here are two independent live
/// OS calls, so one succeeding does not prove the other failed, and the
/// `Err(_)` arm's assertion runs only when it runs. The causation is proven
/// where it can be:
///
/// - on stub targets, where `try_snapshot()` cannot succeed at all,
///   `snapshot()` returning exactly `MemStat::default()` through the real
///   path is deterministic — asserted in `tests/platform_contract.rs`;
/// - per-injected-result at the conversion seam, in
///   `tests/status_convert.rs` (`Err(_) → SnapshotError::Os`,
///   `Ok(malformed) → SnapshotError::Malformed`), where the public wrapper
///   is literally `try_snapshot().unwrap_or_default()` (`src/lib.rs`,
///   `fn snapshot`).
///
/// Compares field PRESENCE and not the byte values on purpose. An earlier
/// revision asserted `snapshot() == m` and was flaky within minutes: the two
/// calls are two independent readings of a live process, so its rss moves
/// between them. Which fields exist is a property of the backend and does not
/// move.
#[test]
fn snapshot_is_try_snapshot_with_the_documented_fallback() {
    let best_effort = snapshot();
    match try_snapshot() {
        Ok(m) => {
            assert_eq!(
                (
                    best_effort.virtual_size.is_some(),
                    best_effort.commit_charge.is_some(),
                    best_effort.peak_rss.is_some()
                ),
                (
                    m.virtual_size.is_some(),
                    m.commit_charge.is_some(),
                    m.peak_rss.is_some()
                ),
                "snapshot() must expose the same fields as the Ok reading"
            );
            assert!(
                best_effort.rss > 0,
                "snapshot() must not report the all-zero fallback when a \
                 reading is available; got {best_effort:?}"
            );
        }
        Err(_) => assert_eq!(
            best_effort,
            Default::default(),
            "snapshot() must fall back to all-zeros on error"
        ),
    }
}

/// The error type is usable as an error: it carries a distinct, non-empty
/// message per variant.
///
/// Worth pinning because `#[non_exhaustive]` means variants can be added, and
/// a new one silently sharing another's text would make a failure report
/// ambiguous exactly when it matters.
#[test]
fn every_error_variant_has_its_own_message() {
    let variants = [
        SnapshotError::Unsupported,
        SnapshotError::Os,
        SnapshotError::Malformed,
    ];
    let msgs: Vec<String> = variants.iter().map(|e| e.to_string()).collect();
    for (e, m) in variants.iter().zip(&msgs) {
        assert!(!m.is_empty(), "{e:?} must render a non-empty message");
    }
    for i in 0..msgs.len() {
        for j in (i + 1)..msgs.len() {
            assert_ne!(
                msgs[i], msgs[j],
                "{:?} and {:?} must not share a message",
                variants[i], variants[j]
            );
        }
    }
    // And it really is an `Error`, so `?` and `Box<dyn Error>` work.
    fn assert_is_error<E: std::error::Error>(_: &E) {}
    assert_is_error(&SnapshotError::Os);
}
