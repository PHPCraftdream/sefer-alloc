// R2-01 (independent src review round 2, task #2003) — compile-fail bait for
// the FIXED lifetime bound on `AllocCore::segment_bases`. This crate exists
// SOLELY as compile-fail bait, built by
// `tests/regression_r2_01_segment_bases_lifetime.rs` as a child process;
// never part of the sefer-alloc workspace or its own target set (see this
// fixture's own `Cargo.toml`).
//
// Before task #2003, `AllocCore::segment_bases`/`SegmentTable::bases`/
// `HeapCore::segment_bases` returned `impl Iterator<Item = *mut u8>` with no
// lifetime bound. Under edition 2021's return-position-`impl-Trait` elision
// rules this did NOT capture the elided `&self` lifetime (the underlying
// closure only closes over `Copy` data), so a safe caller could return/hold
// the iterator past `AllocCore`'s own `drop` and still call `next()` on it —
// a real use-after-free reachable from 100% safe code (confirmed
// empirically: this exact function body compiled cleanly before the fix).
// Task #2003 added `+ '_` at all three levels, closing it.
//
// `probe` returns the iterator out of the function that owns the local
// `AllocCore` — this MUST fail to compile with E0597 ("`core` does not live
// long enough") after the fix, and did NOT fail before it.

fn probe() -> impl Iterator<Item = *mut u8> {
    let core = sefer_alloc::AllocCore::new().unwrap();
    core.segment_bases()
}

fn main() {
    let mut it = probe();
    let _ = it.next();
}
