//! `Reservation::try_advise_huge_pages_range` — range-limited transparent
//! huge-page advice (task #53, J13b VM-part). The whole-map TH-1 sibling
//! stays in `api/reserve.rs`; this file is the focused home for the
//! range-limited form.

use crate::error::VmemError;
use crate::page_size::{page_size_or_poison, PAGE_SIZE_QUERY_FAILED};
use crate::Reservation;

impl Reservation {
    /// Range-limited huge-page advice: Linux/Android `madvise(2)` +
    /// `MADV_HUGEPAGE` over `[offset, offset + length)` of the usable span
    /// ([`as_ptr`](Reservation::as_ptr) .. +[`len`](Reservation::len)).
    /// Advisory only: `Ok(length)` means the kernel accepted the hint for
    /// exactly this range — never that any huge page exists or will back it
    /// (THP `never`, no aligned extent, no free huge page). The mapping and
    /// its contents do not change.
    ///
    /// Validation runs on EVERY platform before any backend decision, in
    /// this order:
    /// 1. a poisoned page-size query yields
    ///    [`VmemError::os_refusal_unknown_code`] (fail-closed; mirrors
    ///    `Reservation::try_decommit`);
    /// 2. `offset.checked_add(length)` overflow and
    ///    `offset + length > len()` yield
    ///    [`VmemError::invalid_argument`] — never clamped, never widened;
    /// 3. both endpoints must be page-size aligned in ABSOLUTE address
    ///    terms (`as_ptr().addr() + offset`, and `length` itself) so the
    ///    kernel receives exactly the requested bytes — `madvise(2)` would
    ///    round a misaligned length up. The huge-page size is
    ///    arch/kernel-config dependent, so validation is deliberately at
    ///    `page_size()`; the CALLER owns the policy granularity (full
    ///    2-MiB-aligned ranges on x86_64 Linux/Android);
    /// 4. a well-formed `length == 0` is `Ok(0)` on every platform — a
    ///    no-op that consults no backend.
    ///
    /// # Errors
    ///
    /// - [`VmemError::invalid_argument`] for the range violations in 2–3
    ///   above;
    /// - [`VmemError::os_refusal_unknown_code`] for the poisoned page-size
    ///   query, on platforms without the backend (non-Linux/Android,
    ///   miri, `aligned_vmem_mock`), and under an armed `fault-injection`
    ///   hook. All of these are PRE-syscall: nothing was touched, zero
    ///   bytes advised, the reservation unchanged. They share the one
    ///   no-code sentinel and are deliberately NOT distinguishable from
    ///   each other (`is_invalid_argument() == false`,
    ///   `os_code() == None` for the whole class);
    /// - [`VmemError::last_os_error`] when the kernel refused the
    ///   `madvise` call (captured immediately; no intervening FFI).
    ///
    /// **Budget semantics of an OS refusal.** `madvise(2)` is not
    /// documented as all-or-nothing: an error return does NOT certify that
    /// the hint applied to zero bytes. A conservative caller treats the
    /// effect as UNKNOWN — neither credits nor debits budget for the range
    /// until the state is re-derived or the reservation is released. Only
    /// the pre-syscall class above certifies "nothing touched".
    pub fn try_advise_huge_pages_range(
        &self,
        offset: usize,
        length: usize,
    ) -> Result<usize, VmemError> {
        let ps = page_size_or_poison();
        if ps == PAGE_SIZE_QUERY_FAILED {
            return Err(VmemError::os_refusal_unknown_code());
        }
        let Some(end) = offset.checked_add(length) else {
            return Err(VmemError::invalid_argument());
        };
        if end > self.len() {
            return Err(VmemError::invalid_argument());
        }
        let Some(start_abs) = self.as_ptr().addr().checked_add(offset) else {
            return Err(VmemError::invalid_argument());
        };
        if !start_abs.is_multiple_of(ps) || !length.is_multiple_of(ps) {
            return Err(VmemError::invalid_argument());
        }
        if length == 0 {
            return Ok(0);
        }
        #[cfg(all(
            any(target_os = "linux", target_os = "android"),
            not(miri),
            not(aligned_vmem_mock)
        ))]
        {
            #[cfg(feature = "fault-injection")]
            if crate::fault_injection::should_fail_advise_huge_range() {
                return Err(VmemError::os_refusal_unknown_code());
            }
            const MADV_HUGEPAGE: i32 = 14;
            let ptr = self.as_ptr();
            // SAFETY: `offset <= self.len()` was validated above, so
            // `ptr.add(offset)` stays inside this reservation's live,
            // exclusively owned usable span.
            let start = unsafe { ptr.add(offset) };
            // SAFETY: `[start, start + length)` lies within the live,
            // exclusively owned, page-aligned mapping (validated above);
            // MADV_HUGEPAGE only sets a VMA hint and changes no page
            // contents.
            if unsafe { crate::os::madvise(start.cast(), length, MADV_HUGEPAGE) } == 0 {
                Ok(length)
            } else {
                Err(VmemError::last_os_error())
            }
        }
        #[cfg(not(all(
            any(target_os = "linux", target_os = "android"),
            not(miri),
            not(aligned_vmem_mock)
        )))]
        {
            Err(VmemError::os_refusal_unknown_code())
        }
    }
}
