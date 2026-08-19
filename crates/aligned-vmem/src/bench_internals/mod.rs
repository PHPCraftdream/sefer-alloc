//! `bench-internals`: path-activation counters (task #504, F11 step 1).
//!
//! Five independent questions, one instrument family each:
//!
//! - Unix: on 32-bit, `unix_reserve` tries an EXACT-size `mmap` first
//!   (`try_reserve_aligned_exact`) and only falls through to the over-reserve
//!   path on a miss (wrong alignment), keeping the full `size + align` mapping.
//!   The fast path costs 1 syscall on a hit (mmap) vs 3 on a miss (mmap +
//!   munmap + mmap for the over-reserve). Expected cost = `p*1 + (1-p)*3 =
//!   3 - 2p` syscalls vs a flat 1 without the fast path. On 64-bit, the fast
//!   path is disabled entirely — `unix_reserve` always over-reserves (1 syscall)
//!   because the expected cost exceeds 1 for every hit rate p < 1 (the break-even
//!   is 100%), and address-space economy (the fast path's only benefit) is not
//!   a concern on 64-bit. On 32-bit, the fast path remains enabled for VA economy,
//!   despite the same syscall-cost disadvantage, because 32-bit address space is
//!   scarce. At real hit rates (34.4%-56.7%, see `reserve_aligned`'s own
//!   rustdoc "Cost on Unix fast-path miss" note) this would be 87%-131% MORE
//!   syscall traffic on 64-bit if the fast path were kept.
//!   `UNIX_EXACT_RESERVE_HITS`/`_ATTEMPTS` settle the real hit rate.
//! - Windows: `win_reserve_commit` issues reserve+commit in either
//!   one syscall (the fast path for `align <= WIN_ALLOCATION_GRANULARITY` on a full-span commit
//!   (`commit_len == size`), or `align <= GetLargePageMinimum()` for large-page requests,
//!   over-reserving nothing — base == region)
//!   or two syscalls (all other cases — `align > WIN_ALLOCATION_GRANULARITY` for ordinary
//!   requests, `align > GetLargePageMinimum()` for large-page requests, or a partial initial
//!   commit (`commit_len != size`) — over-reserving `size + align` only when
//!   `align > WIN_ALLOCATION_GRANULARITY` or the fast-reserve sub-path's own alignment check
//!   misses; Windows cannot partially release a `MEM_RESERVE` region).
//!   `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS` and `WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`
//!   count each path separately for parity/comparison against the Unix
//!   hit-rate story.
//! - macOS decommit oracle (round-6, task #882): `libc_madvise` discards
//!   `madvise`'s return value by design (task #719), so nothing distinguished
//!   "the syscall succeeded but Darwin's semantics didn't reclaim the pages"
//!   from "the syscall itself failed" for item 48's root-cause question.
//!   `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` settle it with a real
//!   number — see those statics' own docs.
//! - Windows decommit failure path (round-3): `VirtualFree(MEM_DECOMMIT)`
//!   failure/failure tracking distinguishes "the syscall was attempted but
//!   failed" from "it was never attempted at all". `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS`/`_FAILURES`
//!   settle this, and `UNIX_MUNMAP_ATTEMPTS`/`UNIX_MUNMAP_FAILURES` provides
//!   the Unix counterpart.
//! - Release attempt/success oracle (task #1189, coverage gap C2): a
//!   failures-only counter cannot distinguish "release ran and succeeded"
//!   from "the release call site was removed and never ran" -- both read as
//!   zero failures. `UNIX_MUNMAP_ATTEMPTS` and
//!   `WINDOWS_VIRTUALFREE_RELEASE_ATTEMPTS` (added alongside the pre-existing
//!   `UNIX_MUNMAP_FAILURES`/`WINDOWS_VIRTUALFREE_RELEASE_FAILURES`) close
//!   that gap for the `Reservation::Drop`/`release`/`try_release` path,
//!   mirroring the decommit-side attempts/failures pairs already above.
//! - Windows large-page failure taxonomy (R4-5/R5-4): two distinct failure modes
//!   that both incur syscall cost but are semantically separate.
//!   `WINDOWS_LARGE_PAGE_RETRY_FAILURES` counts ONLY the case where the initial
//!   large-page attempt failed AND the ordinary-page retry ALSO failed (both
//!   returned NULL). `WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES` counts the case
//!   where a fast-path attempt that requested large pages returned a base
//!   address that was not aligned to the requested `align`, forcing a
//!   `VirtualFree` and fallthrough to the two-call path. This is EXPECTED
//!   and NOT a malfunction when `WIN_ALLOCATION_GRANULARITY < align <=
//!   GetLargePageMinimum()`: `VirtualAlloc(NULL, ...)` guarantees alignment
//!   only to the 64 KiB granularity, not to arbitrary larger alignments. The
//!   counter grows on ordinary Windows machines without `SeLockMemoryPrivilege`
//!   because the fast path accepts such alignments when `extra_commit_flags !=
//!   0` (see the `fast_path_align_threshold` logic). A nonzero value is
//!   diagnostic ONLY when it appears for alignments at or below
//!   `WIN_ALLOCATION_GRANULARITY` (which the kernel SHOULD honor), or when the
//!   caller DOES have `SeLockMemoryPrivilege` (where large pages are actually
//!   granted and SHOULD respect the alignment guarantee).
//!   `WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES` counts the case where the
//!   initial large-page attempt failed but the retry with ordinary pages succeeded,
//!   closing the observability gap where `WINDOWS_RESERVE_COMMIT_SINGLE_CALLS`
//!   cannot distinguish between "single-call with large pages succeeded" and
//!   "single-call failed with large pages, retry with plain pages succeeded"
//!   (both increment the same counter). Added by R7-3.
//!
//! `AtomicU64` storage, increments gated on `bench-internals` so a plain build
//! carries zero extra instructions (storage itself is also gated, not compiled
//! without the feature). Relaxed — diagnostic only, no ordering obligation.

#[cfg(feature = "bench-internals")]
mod huge;
#[cfg(feature = "bench-internals")]
mod reset;
#[cfg(feature = "bench-internals")]
mod unix;
#[cfg(feature = "bench-internals")]
mod windows;

#[cfg(feature = "bench-internals")]
pub use huge::huge_decommit_attempts;
#[cfg(feature = "bench-internals")]
pub(crate) use huge::HUGE_DECOMMIT_ATTEMPTS;
#[cfg(feature = "bench-internals")]
pub use reset::reset_bench_internals_counters;
#[cfg(feature = "bench-internals")]
pub use unix::{
    unix_exact_reserve_attempts, unix_exact_reserve_hits, unix_madvise_attempts,
    unix_madvise_successes, unix_munmap_attempts, unix_munmap_failures,
};
#[cfg(all(
    feature = "bench-internals",
    unix,
    not(miri),
    any(
        all(
            any(target_os = "linux", target_os = "android"),
            feature = "huge-pages"
        ),
        target_pointer_width = "32"
    )
))]
pub(crate) use unix::{UNIX_EXACT_RESERVE_ATTEMPTS, UNIX_EXACT_RESERVE_HITS};
#[cfg(all(feature = "bench-internals", unix, not(miri)))]
pub(crate) use unix::{
    UNIX_MADVISE_ATTEMPTS, UNIX_MADVISE_SUCCESSES, UNIX_MUNMAP_ATTEMPTS, UNIX_MUNMAP_FAILURES,
};
#[cfg(feature = "bench-internals")]
pub use windows::{
    windows_large_page_alignment_failures, windows_large_page_plain_fallback_successes,
    windows_large_page_retry_failures, windows_reserve_commit_calls,
    windows_reserve_commit_single_calls, windows_reserve_commit_two_call_pairs,
    windows_virtualfree_decommit_attempts, windows_virtualfree_decommit_failures,
    windows_virtualfree_release_attempts, windows_virtualfree_release_failures,
};
#[cfg(all(feature = "bench-internals", windows, not(miri)))]
pub(crate) use windows::{
    WINDOWS_LARGE_PAGE_ALIGNMENT_FAILURES, WINDOWS_LARGE_PAGE_PLAIN_FALLBACK_SUCCESSES,
    WINDOWS_LARGE_PAGE_RETRY_FAILURES, WINDOWS_RESERVE_COMMIT_SINGLE_CALLS,
    WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS, WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS,
    WINDOWS_VIRTUALFREE_DECOMMIT_FAILURES, WINDOWS_VIRTUALFREE_RELEASE_ATTEMPTS,
    WINDOWS_VIRTUALFREE_RELEASE_FAILURES,
};
