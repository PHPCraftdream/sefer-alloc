//! `advise_fresh_large` — range-limited huge-page advice for a fresh Large
//! mapping, with strict budget accounting. Safe code only, no allocation.

use crate::alloc_core::config::policy::large_thp::{LargeThpMode, LargeThpRequestGroup};
use crate::alloc_core::os::Segment;
use crate::alloc_core::policy::large_thp_runtime::{ThpAdvice, ThpAdviceOutcome, ThpRuntime};

/// Advise a prefix of the freshly reserved `segment` and return the budget
/// charge the caller must RECORD in the mapping's header (0 = nothing held),
/// together with a control-observation token (0 = not a control observation).
///
/// Must run while the caller still owns the live `Segment`, once per fresh
/// reservation, never for a cache hit or an in-place growth. The tuple is read
/// once. The advised range always starts at the usable root (offset 0) and
/// spans `given <= usable_span` bytes, so it never widens past the span; the
/// vmem wrapper additionally rejects any range beyond the mapping.
///
/// Budget 0 advises without touching the ledger and returns a zero charge.
/// Otherwise the charge is reserved BEFORE the advice and:
/// - accepted: the full reservation stays charged and is returned;
/// - nothing touched (validation, injected or unsupported failure): the
///   reservation is given back at once and a zero charge is returned;
/// - an OS error with a code (partial effect unknown): the full reservation is
///   held and returned for the terminal release; it is not counted as advised.
///
/// Control tokens encode request groups by index: `1..=6` means a successful
/// no-THP control for groups `0..=5`; `9..=14` means the corresponding control
/// attempt failed. Zero means no control observation.
///
/// Premise ordering (fresh-premise proof): the mapping comes straight from
/// the OS reservation and has never been faulted, and
/// `try_disable_transparent_huge_pages` runs BEFORE the header write, any
/// terminal-word initialization, and any payload access. No huge backing can
/// therefore exist at the moment the policy is applied, and no payload or
/// header page faults before it. A failed operation consequently yields an
/// UNKNOWN token, which is never sampled.
pub(crate) fn advise_fresh_large(
    segment: &Segment,
    request_size: usize,
    usable_span: usize,
) -> (usize, u64) {
    if !cfg!(any(target_os = "linux", target_os = "android")) {
        return (0, 0);
    }
    let tuple = ThpRuntime::current();
    if !ThpAdvice::eligible(tuple.mode, tuple.min_bytes, request_size) {
        return (0, 0);
    }
    if tuple.mode == LargeThpMode::Auto {
        let (selected, _ordinal) = ThpRuntime::next_control(request_size);
        if selected {
            let group_index = match LargeThpRequestGroup::of_bytes(request_size as u64) {
                LargeThpRequestGroup::Under1MiB => 0,
                LargeThpRequestGroup::MiB1To8 => 1,
                LargeThpRequestGroup::MiB8To32 => 2,
                LargeThpRequestGroup::MiB32To128 => 3,
                LargeThpRequestGroup::MiB128To512 => 4,
                LargeThpRequestGroup::Over512MiB => 5,
            };
            let success_token = group_index + 1;
            if segment.try_disable_transparent_huge_pages().is_ok() {
                return (0, success_token);
            }
            return (0, group_index + 9);
        }
    }
    let share = if tuple.mode == LargeThpMode::Auto {
        ThpRuntime::learned_share(request_size, &tuple.gradient)
    } else {
        ThpAdvice::share_for(&tuple, request_size)
    };
    let want = ThpAdvice::extent(usable_span, share, None, aligned_vmem::page_size());
    if want == 0 {
        return (0, 0);
    }
    let budgeted = tuple.budget_bytes != 0;
    let given = if budgeted {
        let reserved = ThpRuntime::ledger().try_reserve(want, tuple.budget_bytes);
        if reserved == 0 {
            return (0, 0);
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
                (given, 0)
            } else {
                (0, 0)
            }
        }
        ThpAdviceOutcome::PreSyscall => {
            if budgeted {
                ThpRuntime::note_returned(given);
                ThpRuntime::ledger().give_back(given);
            }
            (0, 0)
        }
        ThpAdviceOutcome::UnknownPartial => {
            if budgeted {
                (given, 0)
            } else {
                (0, 0)
            }
        }
    }
}
