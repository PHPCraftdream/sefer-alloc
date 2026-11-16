//! The one sanctioned process terminator (`fatal(msg)`): writes a best-effort
//! static diagnostic and terminates without unwinding.
//!
//! Moved verbatim from `sefer-alloc`'s `alloc_core::platform::fatal` by plan
//! step P1.4 (docs/plans/2026-11-15-workspace-restructure-plan.md); the root
//! crate reaches it through its `alloc_core::fatal` conduit, so all call-site
//! paths are unchanged.
///
/// Writes a best-effort static diagnostic and terminates without unwinding.
#[cold]
#[inline(never)]
pub fn fatal(msg: &'static [u8]) -> ! {
    use std::io::Write;
    let _ = std::io::stderr().write_all(msg);
    std::process::abort()
}
