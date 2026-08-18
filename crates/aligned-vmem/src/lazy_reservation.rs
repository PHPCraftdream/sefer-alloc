use crate::error::VmemError;
use crate::page_size::page_size;
use crate::Reservation;

use super::lazy_commit_is_honored::lazy_commit_is_honored;

/// A [`Reservation`] that also tracks how much of itself is committed.
///
/// The tracked counterpart to the raw commit primitives. A plain `Reservation`
/// describes GEOMETRY — where the span is, how long, how aligned — but virtual
/// memory also has STATE per page (reserved / committed), and a bare
/// `Reservation` holds none of it. Every caller of the lazy path therefore had
/// to invent the same bookkeeping. This type holds it instead.
///
/// # What is tracked, and what deliberately is not
///
/// Exactly one number: a **watermark**. `[0, committed_len())` is committed;
/// `[committed_len(), len())` is not. Arbitrary committed/uncommitted HOLES are
/// not representable, and that is a drawn boundary rather than an oversight —
/// the dominant lazy-reservation shape is grow-only, a watermark covers it in
/// one `usize`, and anything more general is a per-page bitmap, which is an
/// allocator's job and not this crate's. A caller that genuinely needs holes
/// already has its own metadata plane and should take
/// [`into_reservation`](Self::into_reservation) and drive the raw primitives.
///
/// # Why the mutating methods take `&mut self`
///
/// Not bookkeeping hygiene — this is the crate finally stating a requirement it
/// always had. A watermark is inherently racy: two threads committing
/// concurrently must serialise, and under the raw primitives nothing ever said
/// so. `&mut self` makes the compiler ask for the synchronisation that was
/// always necessary, instead of leaving it to be discovered in production.
///
/// # The watermark is a guarantee, not a prohibition
///
/// `committed_len()` is what this crate GUARANTEES writable. Where decommit is
/// advisory rather than reclaiming (see
/// [`Reservation::decommit_reclaims_and_zeroes`]) memory past the watermark may
/// still be resident and writable after
/// [`shrink_committed`](Self::shrink_committed). Relying on that is exactly the
/// non-portable assumption this type exists to prevent — treat the watermark as
/// the contract.
///
/// # It can still be bypassed, and that is honest
///
/// [`as_ptr`](Self::as_ptr) hands out the raw pointer; it must, or you could not
/// write to the memory. Passing that pointer to the raw commit primitives
/// changes OS state behind the watermark's back and the watermark goes stale.
/// No API over raw memory can prevent that. What changes is the DEFAULT: the
/// tracked path is what you get without asking, and the bypass now requires
/// deliberately reaching for a differently-named function.
///
/// After task #1104 there are exactly two doors out, and both are
/// deliberate: the raw pointer above — every USE of which is already
/// `unsafe` — and [`into_reservation`](Self::into_reservation), which
/// consumes this handle, so the watermark cannot outlive its own
/// tracking. A borrowed `&Reservation` was briefly offered as a read-only
/// convenience and removed: every OS-state mutator on `Reservation` takes
/// `&self`, so any such borrow is a 100%-safe bypass of the watermark
/// (publication audit finding H1, task #1104). The read-only queries
/// callers actually need — [`len`](Self::len), [`as_ptr`](Self::as_ptr),
/// [`align`](Self::align) — are proxied directly on this type; anything
/// else lives behind [`into_reservation`](Self::into_reservation) on
/// purpose.
///
/// A `LazyReservation` is never huge-page backed — the lazy constructors always
/// request ordinary pages — so there is no `is_huge()` here. It would be a
/// constant `false` dressed up as a question.
#[cfg(feature = "lazy-commit")]
#[cfg_attr(docsrs, doc(cfg(feature = "lazy-commit")))]
pub struct LazyReservation {
    inner: Reservation,
    /// Invariant: `committed <= inner.len()`, and `committed` is always a
    /// multiple of the runtime `page_size()`. Both mutators round to page
    /// granularity, and `inner.len()` is itself a page multiple because
    /// `validate_initial_commit` rejects anything else.
    committed: usize,
}

#[cfg(feature = "lazy-commit")]
impl LazyReservation {
    /// Wrap a freshly-created lazy reservation, recording what the backend
    /// ACTUALLY committed rather than what was requested.
    ///
    /// The two differ on every platform except real Windows — see
    /// [`lazy_commit_is_honored`]. Deriving the watermark from that one query,
    /// rather than from the caller's `initial_commit`, is what makes the
    /// watermark and the query incapable of disagreeing.
    pub(crate) fn new(inner: Reservation, initial_commit: usize) -> Self {
        let committed = if lazy_commit_is_honored() {
            initial_commit
        } else {
            inner.len()
        };
        Self { inner, committed }
    }

    /// Bytes from the base that are committed and writable.
    ///
    /// Where [`lazy_commit_is_honored`] is `false` this equals
    /// [`len`](Self::len) from the moment of creation.
    #[must_use]
    pub const fn committed_len(&self) -> usize {
        self.committed
    }

    /// Ensure at least `len` bytes from the base are committed.
    ///
    /// **Idempotent and monotone** — the point of the whole type. Call it before
    /// every write without remembering what you already committed: a call
    /// asking for no more than the current watermark issues no syscall and
    /// returns `Ok(())`.
    ///
    /// `len` need NOT be page-aligned; it is rounded UP to the runtime page size
    /// internally, so asking for one byte past the watermark commits the page
    /// containing it.
    ///
    /// # Errors
    ///
    /// [`VmemError::invalid_argument`] if `len > len()`; otherwise the OS cause
    /// when the commit genuinely fails (commit-charge exhaustion / OOM).
    ///
    /// **On failure the watermark is left unchanged**, so the handle still
    /// describes exactly what is committed and a retry is safe.
    pub fn ensure_committed(&mut self, len: usize) -> Result<(), VmemError> {
        if len <= self.committed {
            return Ok(());
        }
        if len > self.inner.len() {
            return Err(VmemError::invalid_argument());
        }
        // `len <= inner.len()` and `inner.len()` is a page multiple, so rounding
        // up cannot escape the span.
        let new_end = len.next_multiple_of(page_size());
        debug_assert!(new_end <= self.inner.len());
        self.inner.try_commit_range(self.committed, new_end)?;
        self.committed = new_end;
        Ok(())
    }

    /// Lower the watermark to `len`, asking the OS to release
    /// `[new watermark, old watermark)`.
    ///
    /// `len` is rounded UP to the runtime page size, so a page containing bytes
    /// you asked to KEEP is never released. Asking for at least the current
    /// watermark is a no-op.
    ///
    /// What the OS does with the released range is platform-dependent — see
    /// [`Reservation::decommit`]'s platform matrix. The watermark drops
    /// regardless, because it is this crate's guarantee and not a claim about
    /// residency.
    pub fn shrink_committed(&mut self, len: usize) {
        if len >= self.committed {
            return;
        }
        let new_end = len.next_multiple_of(page_size());
        if new_end >= self.committed {
            return;
        }
        self.inner.decommit(new_end, self.committed);
        self.committed = new_end;
    }

    /// Give up tracking and take the plain [`Reservation`].
    ///
    /// The explicit door out, for a caller keeping its own commit state. The
    /// motivating case is an allocator whose watermark must live in its own
    /// metadata, reachable from a bare pointer on a hot path where no handle is
    /// in scope. After this call the crate tracks nothing and the raw
    /// primitives are yours to drive.
    #[must_use]
    pub fn into_reservation(self) -> Reservation {
        self.inner
    }

    /// Base pointer of the usable span. See [`Reservation::as_ptr`].
    #[must_use]
    pub fn as_ptr(&self) -> *mut u8 {
        self.inner.as_ptr()
    }

    /// Usable length of the span — committed and uncommitted together.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the usable span is empty. Always `false` for a reservation this
    /// crate produced (a zero-size request is rejected); present because clippy
    /// asks for it alongside `len`.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    /// Alignment the span was reserved with. See [`Reservation::align`].
    #[must_use]
    pub const fn align(&self) -> usize {
        self.inner.align()
    }
}
