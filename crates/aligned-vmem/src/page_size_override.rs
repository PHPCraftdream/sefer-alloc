//! Test-only page-size injection (build-time cfg
//! `aligned_vmem_page_size_override`).
//!
//! Purpose: let a test inject a simulated *runtime* page size so that call
//! paths validated against [`crate::page_size`] can be exercised on hosts
//! whose real page is smaller (task #1080). The motivating bug class — tasks
//! #1074/#1077, values rounded to the compile-time `PAGE` constant instead
//! of the runtime `page_size()` — was invisible on 4 KiB-page hosts by
//! construction: there the two values coincide, so only forcing a larger
//! simulated page (e.g. 64 KiB) makes such a call site fail loudly on any
//! host.
//!
//! # Why this is a safe `fn`
//!
//! [`set_page_size_override`] takes no pointers and touches no allocator
//! metadata, so it cannot introduce UB by itself — and the acceptance rule
//! keeps any stored override on the SAFE side of the real page size: a
//! value is stored only if it is a power of two, at least `PAGE`, and NOT
//! SMALLER than the machine's real OS page size (a fresh,
//! cache-bypassing query — task #1085). Every validator in this crate
//! (`validate_initial_commit`, the commit/decommit range validators)
//! compares against `page_size()`; a stored override is therefore always
//! `>=` the real page, and for powers of two "larger" implies "multiple":
//! every byte range that passes validation under the override is also a
//! multiple of the real page. The override can only make validation
//! STRICTER — it can never accept what the real page size would reject.
//! The OS calls that do pass validation remain legal: a 64 KiB multiple is
//! also a multiple of every smaller real page, so `VirtualAlloc`/`mmap`
//! accept the ranges unchanged. Misaligned ranges are rejected
//! (reserve/commit) or silently skipped (decommit) — fail-closed
//! degradation, never UB.
//!
//! Residual assumption, inherited rather than override-specific: the floor
//! is the OS query validated by the same rule [`crate::page_size`] uses, so
//! if the OS query itself fails and falls back to `PAGE` on a host whose
//! true hardware page is larger, the floor degrades to `PAGE` — exactly
//! the degraded assumption the crate's own `page_size()` already makes
//! with no override armed. Before task #1085 this section claimed the
//! stricter-only property WITHOUT the real-page floor, which was false
//! whenever a caller forced a page smaller than the real one (e.g.
//! `Some(4096)` on a 16 KiB-page host): validators would accept 4 KiB
//! multiples and the OS rounds a decommit LENGTH up to the real page,
//! silently discarding live data outside the requested range.
//!
//! # Reachability
//!
//! This module is compiled only under the `aligned_vmem_page_size_override`
//! build-time cfg (task #1080), deliberately NOT a Cargo feature — the exact
//! feature-unification hazard task #962 /
//! docs/CORRECTNESS_OPEN_ITEMS.md item 42 closed for `mock`: a feature would
//! be reachable transitively from any downstream crate's feature resolution,
//! while a cfg flag is passed only explicitly per-build via
//! `RUSTFLAGS="--cfg aligned_vmem_page_size_override"` (wired in
//! `scripts/check-all.mjs` and the `test-windows` CI job); declared in this
//! crate's `[lints.rust unexpected_cfgs]` check-cfg list so it produces no
//! unexpected-cfg warnings.
//!
//! # Restoration contract
//!
//! The override is process-global. Tests MUST pass `None` when done (a `Drop`
//! guard is the recommended shape) so the next [`crate::page_size`] call
//! re-queries the real OS page size.
//!
//! Zero cost when the cfg is off: this entire module is compiled out
//! (`#[cfg(aligned_vmem_page_size_override)]` on the `mod` declaration in
//! `lib.rs`), so the production path is byte-identical with the flag absent.

use core::sync::atomic::Ordering;

use super::page_size::{query_os_page_size, validate_page_size_impl, PAGE_SIZE_CACHE};

/// The REAL OS page size right now, as a fresh validated query that ignores
/// [`PAGE_SIZE_CACHE`] entirely (task #1085).
///
/// Unlike [`crate::page_size`], this never reads (or writes) the cache, so
/// while an override is armed it still reports the true OS value. The
/// acceptance floor below must compare against the machine's REAL page, not
/// the cached (possibly overridden) one: comparing against the cache would
/// both falsely reject legal downshifts to a still-legal page and fail to
/// pin the invariant that actually matters — the effective page size is
/// always >= the real one, so every page-multiple validator stays at least
/// as strict as the no-override behavior.
fn real_os_page_size_fresh() -> usize {
    validate_page_size_impl(query_os_page_size())
}

/// Set (`Some`) or clear (`None`) the process-global page-size override seen
/// by [`crate::page_size`], returning whether the request took effect.
///
/// - `Some(ps)`: `ps` is validated with the SAME rule [`crate::page_size`]
///   applies to OS queries (`validate_page_size_impl`: at least `PAGE` and a
///   power of two), PLUS the task-#1085 real-page floor: `ps` must not be
///   SMALLER than the machine's real OS page size (queried fresh, bypassing
///   the override cache). An override below the real page would loosen every
///   page-multiple validator below reality — the OS rounds decommit lengths
///   up to the real page, silently discarding live data outside the requested
///   range (the exact hazard on 16/64 KiB-page hosts such as macOS ARM64 and
///   aarch64-64k Linux). An override EQUAL to the real page is accepted (a
///   harmless no-op). Unlike the OS-query path there is NO silent fallback
///   to `PAGE` — an invalid or below-real `ps` is REJECTED: the function
///   returns `false` and leaves the cache untouched. On success `ps` is
///   stored and `true` is returned.
/// - `None`: stores `0` (the "not yet queried" sentinel), so the next
///   [`crate::page_size`] call re-queries the real OS. Always returns `true`.
///
/// Process-global and unordered (`Relaxed`, matching [`crate::page_size`]'s
/// own cache accesses): arm it before the code under test runs, restore with
/// `None` afterwards — a `Drop` guard is the recommended shape.
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_page_size_override)))]
pub fn set_page_size_override(new: Option<usize>) -> bool {
    match new {
        Some(ps) => {
            // Reuse the OS-query validator so the override can never accept a
            // value the real query path would have rejected. The impl returns
            // its input only when the input is valid, and returns PAGE
            // otherwise — so "result == input" IS the acceptance test, with
            // one benign overlap: the valid value PAGE itself maps to PAGE.
            let validated = validate_page_size_impl(ps);
            // Task #1085 (finding M1): the override must also never fall
            // BELOW the machine's real page size (see
            // `real_os_page_size_fresh` for why the query bypasses the
            // cache).
            if validated == ps && ps >= real_os_page_size_fresh() {
                PAGE_SIZE_CACHE.store(ps, Ordering::Relaxed);
                true
            } else {
                false
            }
        }
        None => {
            // 0 is the "not yet queried" sentinel: the next `page_size()`
            // call re-queries the real OS and repopulates the cache.
            PAGE_SIZE_CACHE.store(0, Ordering::Relaxed);
            true
        }
    }
}
