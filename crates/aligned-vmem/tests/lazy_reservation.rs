//! `LazyReservation` — the tracked lazy handle (item 66 / finding R7-2).
//!
//! These tests pin the watermark contract. Two design notes matter for reading
//! them:
//!
//! 1. **Every platform arm asserts something.** `lazy_commit_is_honored()` is
//!    `false` on Unix, miri and the mock backend, where the whole span is
//!    committed up front. A test that simply returned early on those platforms
//!    would be vacuous exactly where most CI runs — so each such test asserts
//!    the OTHER half of the contract instead of skipping.
//! 2. **Nothing is ever written past `committed_len()`.** On the one platform
//!    where the watermark is load-bearing (real Windows) such a write does not
//!    fail a test, it kills the process. The tests write strictly below the
//!    watermark, which is also what makes them discriminating: if
//!    `ensure_committed` did not actually commit, `everything_below_the_watermark_is_writable`
//!    faults instead of passing.

#![cfg(feature = "lazy-commit")]

use aligned_vmem::{lazy_commit_is_honored, page_size, LazyReservation};

/// 4 MiB — a multiple of every page size this crate supports (4 KiB through
/// 64 KiB), so the arithmetic below is page-exact on every host.
const SPAN: usize = 4 * 1024 * 1024;

fn fresh() -> LazyReservation {
    aligned_vmem::reserve_aligned_lazy(SPAN, SPAN, page_size()).expect("lazy reserve 4 MiB")
}

/// The watermark records what the BACKEND committed, not what was REQUESTED.
///
/// This is the whole reason the type derives its initial value from
/// `lazy_commit_is_honored()` rather than from the caller's `initial_commit`:
/// the two differ on every platform except real Windows. Both arms are pinned,
/// so a regression that hard-codes either answer fails on one of them.
#[test]
fn initial_watermark_is_the_platform_truth_not_the_request() {
    let r = fresh();
    if lazy_commit_is_honored() {
        assert_eq!(
            r.committed_len(),
            page_size(),
            "backend honors initial_commit: watermark must be exactly the requested prefix"
        );
        assert!(
            r.committed_len() < r.len(),
            "a genuinely lazy reservation must leave a tail uncommitted"
        );
    } else {
        assert_eq!(
            r.committed_len(),
            r.len(),
            "backend ignores initial_commit and commits the whole span: the watermark \
             must say so rather than repeating the request"
        );
    }
}

#[test]
fn geometry_is_forwarded_unchanged() {
    let r = fresh();
    assert_eq!(r.len(), SPAN);
    assert_eq!(r.align(), SPAN);
    assert!(!r.is_empty());
    assert!(!r.as_ptr().is_null());
    assert_eq!(r.as_reservation().len(), SPAN);
}

/// `ensure_committed` is idempotent and monotone — the property that removes
/// the caller's "did I already commit this?" bookkeeping.
#[test]
fn ensure_committed_is_idempotent_and_monotone() {
    let mut r = fresh();
    let ps = page_size();
    let target = 8 * ps;

    r.ensure_committed(target).expect("first grow");
    let after_first = r.committed_len();
    assert!(after_first >= target);

    r.ensure_committed(target).expect("second call is a no-op");
    assert_eq!(
        r.committed_len(),
        after_first,
        "idempotent: watermark unmoved"
    );

    r.ensure_committed(target / 2)
        .expect("asking for less than the watermark is fine");
    assert_eq!(
        r.committed_len(),
        after_first,
        "monotone: ensure_committed never lowers the watermark"
    );
}

/// A non-page-aligned request commits the page CONTAINING the last byte.
#[test]
fn ensure_committed_rounds_up_to_a_page() {
    let mut r = fresh();
    let ps = page_size();
    let start = r.committed_len();

    if start == r.len() {
        // Whole span already committed: there is nothing above the watermark to
        // round into. The contract still has to hold at the boundary — exactly
        // the span is fine, one byte past it is rejected.
        r.ensure_committed(r.len()).expect("exactly the span");
        assert!(
            r.ensure_committed(r.len() + 1).is_err(),
            "one byte past the span must be rejected, not rounded"
        );
        return;
    }

    r.ensure_committed(start + 1)
        .expect("one byte past the watermark");
    assert_eq!(
        r.committed_len(),
        start + ps,
        "rounding is UP to a whole page, so the byte asked for is committed"
    );
}

/// On failure the watermark must not move, so the handle keeps describing what
/// is actually committed and a retry is safe.
#[test]
fn ensure_committed_past_the_span_is_rejected_and_leaves_the_watermark() {
    let mut r = fresh();
    let before = r.committed_len();

    assert!(
        r.ensure_committed(r.len() + 1).is_err(),
        "beyond the usable span is a contract violation"
    );
    assert_eq!(
        r.committed_len(),
        before,
        "a rejected request must not advance the watermark"
    );

    // And the handle is still usable afterwards.
    r.ensure_committed(before)
        .expect("still usable after a rejection");
}

/// The functional property the watermark exists to guarantee.
///
/// Discriminating by construction: on real Windows, if `ensure_committed` did
/// not actually commit the range, this write faults rather than failing.
#[test]
fn everything_below_the_watermark_is_writable() {
    let mut r = fresh();
    let ps = page_size();

    r.ensure_committed(4 * ps).expect("grow to four pages");
    let n = r.committed_len();
    assert!(n >= 4 * ps);

    // SAFETY: `[0, committed_len())` is committed per this type's contract, and
    // the handle owns the span exclusively.
    unsafe {
        r.as_ptr().write_bytes(0xA5, n);
        assert_eq!(*r.as_ptr(), 0xA5, "first byte");
        assert_eq!(
            *r.as_ptr().add(n - 1),
            0xA5,
            "last byte below the watermark"
        );
    }
}

/// Shrinking rounds UP, so a page holding bytes the caller asked to KEEP is
/// never released.
#[test]
fn shrink_rounds_up_so_kept_bytes_survive() {
    let mut r = fresh();
    let ps = page_size();

    r.ensure_committed(8 * ps).expect("grow to eight pages");
    let high = r.committed_len();

    r.shrink_committed(4 * ps + 1);
    assert_eq!(
        r.committed_len(),
        5 * ps,
        "byte 4*ps must stay committed, so its whole page is kept"
    );
    assert!(r.committed_len() < high, "the watermark actually dropped");

    // SAFETY: the kept prefix is still below the watermark.
    unsafe {
        r.as_ptr().write_bytes(0x5A, 4 * ps + 1);
        assert_eq!(*r.as_ptr().add(4 * ps), 0x5A, "the byte we asked to keep");
    }
}

#[test]
fn shrink_at_or_above_the_watermark_is_a_no_op() {
    let mut r = fresh();
    let before = r.committed_len();

    r.shrink_committed(before);
    assert_eq!(r.committed_len(), before, "shrinking to the watermark");

    r.shrink_committed(before + 1);
    assert_eq!(r.committed_len(), before, "shrinking above the watermark");
}

/// Growing again after a shrink must work — the commit primitive is idempotent
/// on the one platform where the range was genuinely decommitted.
#[test]
fn grow_after_shrink_recommits() {
    let mut r = fresh();
    let ps = page_size();

    r.ensure_committed(8 * ps).expect("grow");
    r.shrink_committed(2 * ps);
    assert_eq!(r.committed_len(), 2 * ps);

    r.ensure_committed(6 * ps)
        .expect("grow again over the released range");
    assert_eq!(r.committed_len(), 6 * ps);

    // SAFETY: below the watermark again.
    unsafe {
        r.as_ptr().write_bytes(0x33, 6 * ps);
        assert_eq!(*r.as_ptr().add(6 * ps - 1), 0x33);
    }
}

/// The explicit door out preserves the span exactly; the resulting handle owns
/// the reservation and releases it on drop as usual.
#[test]
fn into_reservation_preserves_the_span() {
    let r = fresh();
    let (ptr, len, align) = (r.as_ptr(), r.len(), r.align());

    let res = r.into_reservation();
    assert_eq!(res.as_ptr(), ptr, "same base");
    assert_eq!(res.len(), len, "same usable length");
    assert_eq!(res.align(), align, "same alignment");
    assert!(
        !res.is_huge(),
        "the lazy constructors never request huge pages"
    );
}
