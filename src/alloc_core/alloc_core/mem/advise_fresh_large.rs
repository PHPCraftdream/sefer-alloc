//! `advise_fresh_large` — range-limited huge-page advice for a fresh Large
//! mapping, with strict budget accounting. Safe code only, no allocation.

use crate::alloc_core::os::Segment;
use crate::alloc_core::policy::large_thp_runtime::{ThpAdvice, ThpAdviceOutcome, ThpRuntime};

/// Advise a prefix of the freshly reserved `segment` and return the budget
/// charge the caller must RECORD in the mapping's header (0 = nothing held).
///
/// Must run while the caller still owns the live `Segment`, once per fresh
/// reservation, never for a cache hit or an in-place growth. The tuple is read
/// once. The advised range always starts at the usable root (offset 0) and
/// spans `given <= usable_span` bytes, so it never widens past the span; the
/// vmem wrapper additionally rejects any range beyond the mapping.
///
/// Budget 0 advises without touching the ledger and returns 0. Otherwise the
/// charge is reserved BEFORE the advice and:
/// - accepted: the full reservation stays charged and is returned;
/// - nothing touched (validation, injected or unsupported failure): the
///   reservation is given back at once and 0 is returned;
/// - an OS error with a code (partial effect unknown): the full reservation is
///   held and returned for the terminal release; it is not counted as advised.
pub(crate) fn advise_fresh_large(
    segment: &Segment,
    request_size: usize,
    usable_span: usize,
) -> usize {
    if !cfg!(any(target_os = "linux", target_os = "android")) {
        return 0;
    }
    let tuple = ThpRuntime::current();
    if !ThpAdvice::eligible(tuple.mode, tuple.min_bytes, request_size) {
        return 0;
    }
    let share = ThpAdvice::share_for(&tuple, request_size);
    let want = ThpAdvice::extent(usable_span, share, None, aligned_vmem::page_size());
    if want == 0 {
        return 0;
    }
    let budgeted = tuple.budget_bytes != 0;
    let given = if budgeted {
        let reserved = ThpRuntime::ledger().try_reserve(want, tuple.budget_bytes);
        if reserved == 0 {
            return 0;
        }
        ThpRuntime::note_charged(reserved);
        reserved
    } else {
        want
    };
    match ThpAdvice::classify(segment.try_advise_huge_pages_range(0, given)) {
        ThpAdviceOutcome::Accepted(n) => {
            ThpRuntime::note_advised(n);
            if budgeted {
                given
            } else {
                0
            }
        }
        ThpAdviceOutcome::PreSyscall => {
            if budgeted {
                ThpRuntime::note_returned(given);
                ThpRuntime::ledger().give_back(given);
            }
            0
        }
        ThpAdviceOutcome::UnknownPartial => {
            if budgeted {
                given
            } else {
                0
            }
        }
    }
}
