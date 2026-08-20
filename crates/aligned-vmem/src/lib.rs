//! `aligned-vmem` — cross-platform **aligned anonymous virtual memory**.
//!
//! Reserve a span of `size` bytes whose base is aligned to an arbitrary
//! power-of-two `align`, commit/decommit its pages, and release it — directly
//! through the OS (`mmap`/`munmap`/`madvise` on Unix, `VirtualAlloc`/
//! `VirtualFree` on Windows), with **no file-mapping machinery** and **no
//! dependencies**. Under [miri](https://github.com/rust-lang/miri) it falls
//! back to `std::alloc` so consumers stay miri-testable. A consumer that
//! installs itself as `#[global_allocator]` cannot use this crate under miri,
//! because the miri backend routes allocations through the global allocator and
//! would create a reentrancy hazard (the same class of issue `numa-shim` hit
//! in #777).
//!
//! This is the OS aperture extracted from
//! [`sefer-alloc`](https://crates.io/crates/sefer-alloc). It is the one crate
//! whose *entire purpose* is the `unsafe` OS calls — every `unsafe` block
//! carries a `// SAFETY:` proof, and a safe API is exposed on top.
//!
//! # Why not `region` / `memmap2` / `mmap-rs`?
//!
//! Those crates are oriented around **file mappings** and **page-protection**.
//! `aligned-vmem` does one different thing: hand you an *anonymous* span whose
//! **base is aligned to a power of two you choose** (e.g. 2 MiB / 4 MiB for an
//! allocator's segments). On 32-bit Unix, first tries an ordinary exact-size
//! `mmap` and checks whether the kernel happened to place it at an
//! `align`-aligned address (fast path; hit rate depends on the OS's placement
//! heuristics, not on any hint this crate passes); on a miss (wrong
//! alignment), over-reserves `size + align` bytes and keeps the full mapping.
//! On 64-bit Unix, the exact-size fast path is compiled out entirely (see the
//! module-level "bench-internals" section below and [`reserve_aligned`]'s own
//! rustdoc), with ONE exception: on Linux AND Android, with the `huge-pages`
//! feature on, a request for `align == LINUX_HUGE_PAGE_SIZE` (2 MiB) huge pages
//! takes an exact-size `MAP_HUGETLB` attempt first, which when it succeeds
//! reserves exactly `size`. The exception's gate is
//! `any(target_os = "linux", target_os = "android")` + `feature = "huge-pages"` —
//! it is NOT keyed on pointer width, which is why it survives on 64-bit; and it
//! covers Android too, so do not describe it as Linux-only. When that exception
//! does not apply, a 64-bit Unix reservation over-reserves `size + align` bytes
//! in one `mmap` call. On Windows, uses one syscall (fast path
//! for `align <= 64 KiB`, over-reserving nothing — base == region) or two
//! syscalls (over-reserving `size + align` and keeping the full mapping). The
//! `Reservation::reservation_ptr` / `reservation_len` fields expose the full
//! reservation; `Reservation::as_ptr` / `len` expose the aligned usable span,
//! plus page-granularity decommit/recommit so you can hint the OS to return
//! physical memory while keeping the address-space reservation (on Linux,
//! Android, and Windows this is guaranteed to return physical backing; on the
//! Darwin family
//! — macOS/iOS/tvOS/watchOS — and the BSDs, this reclaim is advisory-only and
//! provides no zero-fill guarantee, see [`decommit`]'s Darwin caveat). If you are building an
//! allocator, an arena, or a slab and need "give me a 4 MiB-aligned 4 MiB
//! span", this is the small focused tool.
//!
//! # Fallible vs infallible API (0.2)
//!
//! Every reservation/commit entry point has two forms:
//! - the historical infallible form returning `Option`/`bool`
//!   ([`reserve_aligned`], [`recommit`], …), and
//! - a `try_*` form returning [`Result<_, VmemError>`] whose error carries the
//!   OS `errno` / `GetLastError` cause ([`try_reserve_aligned`],
//!   [`try_recommit`], …).
//!
//! For most of these pairs the infallible form forwards to the `try_*` form
//! and discards the cause, so both stay in perfect lockstep. **The decommit
//! family is the exception, in the OPPOSITE direction:** [`decommit`] and
//! [`try_decommit`] are siblings, not a forward/wrap pair — both call the
//! same per-OS backend directly (each discarding or keeping its own copy of
//! the outcome), rather than one calling the other. `decommit`'s contract is
//! deliberately silent on OS-level outcome (best-effort by nature; see
//! [`decommit`]'s own rustdoc) — it discards the backend's answer.
//! [`try_decommit`]'s outer `Result` still reports range-contract validity
//! only (unchanged) — but since task #1180 its `Ok` payload is a
//! [`DecommitOutcome`] (`Skipped` / `Advised` / `Refused`), which DOES
//! observe what the SELECTED BACKEND did with a well-formed, non-empty
//! range: whether a call was issued at all, and if so, whether it was
//! accepted or refused. **`Advised` names what the call did, not
//! necessarily a real OS syscall** — under the native backend it means the
//! kernel accepted a real `madvise(2)`/`VirtualFree` call; under the
//! `aligned_vmem_mock` cfg or miri, no syscall runs at all and `Advised` is
//! the simulated backend's own unconditional answer (see
//! [`DecommitOutcome::Advised`]'s own doc for the full three-way split).
//! Before task #1180 this was a bare `Ok(())`, indistinguishable from every
//! other well-formed outcome.
//!
//! # Example
//!
//! ```text
//! use aligned_vmem::{reserve_aligned, release};
//!
//! // Reserve 4 MiB aligned to 4 MiB.
//! let span = 4 * 1024 * 1024;
//! let r = reserve_aligned(span, span).expect("OOM");
//! let base = r.as_ptr();
//! assert_eq!(base.addr() % span, 0); // base is `span`-aligned
//!
//! // SAFETY: `base` is valid for `r.len()` bytes; we own it exclusively.
//! unsafe { base.write(0xAB); assert_eq!(base.read(), 0xAB); }
//!
//! // RAII release on drop, or take the parts for manual self-hosted release:
//! let (raw, raw_len, raw_align) = r.into_parts();
//! // SAFETY: the triple came from `into_parts` and is released exactly once.
//! unsafe { release(raw, raw_len, raw_align) };
//! ```
//!
//! Runnable form: `tests/smoke.rs`.
//!
//! # Alignment contract
//!
//! `align` must be a power of two and at least [`PAGE`]. `size` must be a
//! non-zero multiple of [`PAGE`] (so decommit ranges land on page boundaries).
//! Violations return `None` / `Err(VmemError::invalid_argument())` rather than
//! panicking.
//!
//! # Page size ([`page_size`])
//!
//! [`PAGE`] (4 KiB) is the crate's *minimum decommit granularity* — the
//! validation constant. [`page_size`] returns the **actual OS page size**
//! queried once via `sysconf(_SC_PAGESIZE)` (Unix) / `GetSystemInfo` (Windows).
//! On Apple Silicon macOS this is 16 KiB; callers computing decommit offsets
//! must round to `page_size()`, not `PAGE`. The crate's own validation of
//! both range endpoints against `page_size()` is the load-bearing guard: do
//! not rely on the OS to reject a misaligned range — Linux `madvise(2)`
//! rejects only a misaligned ADDRESS and rounds a misaligned LENGTH **up**
//! past the requested range, and Windows `VirtualFree(MEM_DECOMMIT)` rejects
//! nothing (it widens the range in both directions to whole pages). In the
//! never-observed case where the one-time OS query fails, the crate fails
//! closed rather than guessing — see [`page_size`]'s "If the one-time OS
//! query fails" paragraph and [`try_page_size`].

#![allow(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]
// Under `mock` the real platform syscalls (decommit/recommit/commit_range) are
// bypassed by the recording backend, so their per-OS `*_impl` helpers become
// legitimately unused. This used to be a crate-wide `allow(dead_code)`, which
// made the whole crate structurally unable to report ANY unused item under
// `--all-features` (task #646/F8). Narrowed to per-item
// `#[cfg_attr(aligned_vmem_mock, allow(dead_code))]` on exactly the helpers
// confirmed (by building `RUSTFLAGS="--cfg aligned_vmem_mock" cargo build
// --features lazy-commit,huge-pages,fault-injection` on Windows, Unix
// (`--target x86_64-unknown-linux-gnu`) and miri (`--cfg miri`)) to go dead
// under `mock` alone: the per-OS `decommit_pages_impl` / `recommit_pages_impl`
// / `commit_range_impl` / `reserve_aligned_lazy_raw` trio-plus-one on each
// platform, plus the Windows-only `winapi_virtual_decommit` +
// `MEM_DECOMMIT` and the Unix-only `libc_madvise` + `madv_free_advice` +
// `MADV_DONTNEED` + `MADV_FREE` (all only reachable from the real decommit
// path, which `mock` bypasses).
// `fault_injection` carries two hooks with one call site each (task #1219
// added the second). The COMMIT-side hook (`should_fail_commit`) is consulted
// only from `try_commit_range`, which is itself gated on `lazy-commit`: a
// caller who enables `fault-injection` without `lazy-commit` gets a
// compiled-but-unreachable hook (harmless — the feature is additive and
// test-only); suppress dead-code only in that specific combination, on the
// single item it affects. The DECOMMIT-side hook (`should_fail_decommit`) is
// consulted only from `dispatch_try_decommit`, which is NOT feature-gated
// (decommit is core API), so its only orphaning combination is the mock cfg —
// see its own `#[cfg_attr(aligned_vmem_mock, allow(dead_code))]`.
//
// Structural alternative considered and deferred for a future major release:
// reorganize the three backends as separate `#[cfg]`-selected private modules
// (`os_windows` / `os_unix` / `os_miri`) with one shared private signature,
// allowing `mock` to be a fourth module selected by the same `#[cfg]` mechanism.
// That would eliminate every `#[cfg_attr(aligned_vmem_mock, allow(dead_code))]`
// attribute, but is a larger refactor than this crate's 0.2.0 release should
// carry. The current partial-replacement shape (mock replaces decommit/recommit/
// commit_range but not reserve/release) is explicitly chosen.
//
// Module layout (task #1055 / R7-10 / perf item 54): this file used to be one
// 4656-line monolith. It is now the crate's re-export surface only — every
// item lives in a module named after it (or, where the crate itself already
// documents two functions as one feature in two forms — an infallible/`try_*`
// pair, or a family of per-platform bench-internals counters — grouped into
// one file per that established pairing, not atomized further).

pub mod error;
pub use error::VmemError;

mod decommit_outcome;
pub use decommit_outcome::DecommitOutcome;

#[cfg(aligned_vmem_mock)]
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_mock)))]
pub mod mock;

#[cfg(feature = "fault-injection")]
#[cfg_attr(docsrs, doc(cfg(feature = "fault-injection")))]
pub mod fault_injection;

#[cfg(aligned_vmem_page_size_override)]
pub mod page_size_override;
#[cfg(aligned_vmem_page_size_override)]
pub mod page_size_query_override;

mod min_page;
mod page;
mod page_size;
mod try_page_size;
#[cfg(feature = "bench-internals")]
mod validate_page_size;

pub use min_page::MIN_PAGE;
pub use page::PAGE;
pub use page_size::page_size;
pub use try_page_size::try_page_size;
#[cfg(feature = "bench-internals")]
pub use validate_page_size::validate_page_size;

#[cfg(feature = "bench-internals")]
mod bench_internals;
#[cfg(feature = "bench-internals")]
pub use bench_internals::{
    huge_decommit_attempts, reset_bench_internals_counters, unix_exact_reserve_attempts,
    unix_exact_reserve_hits, unix_madvise_attempts, unix_madvise_successes, unix_munmap_attempts,
    unix_munmap_failures, windows_large_page_alignment_failures,
    windows_large_page_plain_fallback_successes, windows_large_page_retry_failures,
    windows_reserve_commit_calls, windows_reserve_commit_single_calls,
    windows_reserve_commit_two_call_pairs, windows_virtualfree_decommit_attempts,
    windows_virtualfree_decommit_failures, windows_virtualfree_release_attempts,
    windows_virtualfree_release_failures,
};

mod reservation;
pub use reservation::Reservation;

#[cfg(feature = "lazy-commit")]
mod lazy_commit_is_honored;
#[cfg(feature = "lazy-commit")]
mod lazy_reservation;
#[cfg(feature = "lazy-commit")]
pub use lazy_commit_is_honored::lazy_commit_is_honored;
#[cfg(feature = "lazy-commit")]
pub use lazy_reservation::LazyReservation;

mod reservation_parts;
pub use reservation_parts::ReservationParts;

mod reservation_full_parts;
pub use reservation_full_parts::ReservationFullParts;

mod api;
#[cfg(feature = "lazy-commit")]
pub use api::{commit_range, reserve_aligned_lazy, try_commit_range, try_reserve_aligned_lazy};
pub use api::{
    decommit, decommit_lazy, leak_zeroed_pages, recommit, release, release_parts, reserve_aligned,
    try_decommit, try_recommit, try_reserve_aligned,
};
#[cfg(feature = "huge-pages")]
pub use api::{reserve_aligned_huge, try_reserve_aligned_huge};

mod os;
