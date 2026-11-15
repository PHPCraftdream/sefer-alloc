//! Test-only injection of the RAW OS page-size query result (build-time cfg
//! `aligned_vmem_page_size_override` — the same cfg as
//! [`crate::page_size_override`], because both are one seam: page-size
//! injection for tests).
//!
//! [`crate::page_size_override::set_page_size_override`] injects into the
//! CACHE — it can only express "the page size is this valid value". It
//! structurally cannot express the one state the crate's fail-closed handling
//! exists for: **the OS query itself failing** (`sysconf` returning `-1`, or
//! a garbage answer). Before this module, tests/page_size_override.rs's own
//! docs recorded that "the natural run-both-ways counterfactual cannot be
//! produced on a 4 KiB host through the public API by any means: there is no
//! injection point for the OS page-size query" — task #1085's evidence run
//! had to hand-patch `query_os_page_size` in a scratch tree. This module IS
//! that injection point, committed: it sits underneath
//! `crate::page_size::query_os_page_size`, so an armed simulated raw answer
//! flows through the SAME validation and caching the real answer would.
//!
//! # Why this is a safe `fn`
//!
//! Same argument shape as `set_page_size_override`, one level down: the
//! setter takes no pointers and touches no allocator metadata, and its
//! acceptance rule keeps every armed simulation on the SAFE side —
//! - an INVALID raw answer (0, non-power-of-two, below `PAGE`) is accepted:
//!   downstream it poisons the cache and every page-granular validator fails
//!   CLOSED, which is strictly safer than any real host's behavior;
//! - a VALID raw answer is accepted only when it is `>=` the machine's real
//!   OS page size (queried fresh, bypassing this override): the simulation
//!   can make validation stricter (a larger simulated page), never looser.
//!   A valid-but-below-real simulation is REJECTED — it would loosen every
//!   page-multiple validator below reality, the exact task-#1085/M1 hazard
//!   the cache seam's floor already forecloses.
//!
//! # Restoration contract
//!
//! Process-global, like the cache seam. Tests MUST disarm with `None` AND
//! clear the cache (`set_page_size_override(None)`) when done — a `Drop`
//! guard doing both is the recommended shape. Arming this override does not
//! itself clear the cache: the simulation takes effect at the next COLD
//! query, so arm it, then clear the cache, then call `page_size()`.
//!
//! Zero cost when the cfg is off: the entire module is compiled out, and
//! `query_os_page_size`'s consultation of it disappears with it.

use core::sync::atomic::{AtomicUsize, Ordering};

use super::page_size::{query_os_page_size_real, validate_page_size_impl};

/// Encoded armed state: `0` = disarmed; otherwise `simulated_raw_answer + 1`.
/// The `+1` shift exists so a simulated raw answer of `0` (the canonical
/// "query failed" shape) is representable distinctly from "disarmed";
/// `usize::MAX` is consequently not encodable and is rejected by the setter
/// (it is an invalid answer anyway — use `0` to simulate failure).
static QUERY_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// The armed simulated raw query result, if any. Consulted by
/// `crate::page_size::query_os_page_size` before the real per-OS query.
pub(crate) fn armed_query_result() -> Option<usize> {
    let encoded = QUERY_OVERRIDE.load(Ordering::Relaxed);
    if encoded == 0 {
        None
    } else {
        Some(encoded - 1)
    }
}

/// Arm (`Some`) or disarm (`None`) a simulated RAW OS page-size query result,
/// returning whether the request took effect.
///
/// - `Some(raw)`: accepted when `raw` is either an INVALID page size
///   (simulating a failed query — the crate poisons the cache and fails
///   page-granular operations closed) or a VALID page size not smaller than
///   the machine's real OS page (simulating a larger-page host — strictly
///   stricter validation). `Some(usize::MAX)` and a valid-but-below-real
///   `raw` are rejected: the function returns `false` and nothing is stored.
/// - `None`: disarms. Always returns `true`.
///
/// The simulation is read at the next COLD query only — pair it with
/// [`crate::page_size_override::set_page_size_override`]`(None)` to clear the
/// cache, and disarm + clear again when done (`Drop` guard recommended).
#[cfg_attr(docsrs, doc(cfg(aligned_vmem_page_size_override)))]
pub fn set_page_size_query_override(new: Option<usize>) -> bool {
    match new {
        Some(raw) => {
            if raw == usize::MAX {
                // Not encodable (see `QUERY_OVERRIDE`), and meaningless: an
                // invalid answer is better simulated by `0`.
                return false;
            }
            let raw_is_valid_page = validate_page_size_impl(raw) == raw;
            if raw_is_valid_page {
                // A valid simulation must not loosen validation below the
                // real machine page — same floor rule, same reasoning, as
                // `set_page_size_override` (task #1085/M1). The real query
                // deliberately bypasses this override; if the REAL query
                // itself is unusable, no valid simulation can be floored,
                // so refuse it (only failure simulations are then possible).
                let real_raw = query_os_page_size_real();
                let real_is_valid = validate_page_size_impl(real_raw) == real_raw;
                if !(real_is_valid && raw >= real_raw) {
                    return false;
                }
            }
            QUERY_OVERRIDE.store(raw + 1, Ordering::Relaxed);
            true
        }
        None => {
            QUERY_OVERRIDE.store(0, Ordering::Relaxed);
            true
        }
    }
}
