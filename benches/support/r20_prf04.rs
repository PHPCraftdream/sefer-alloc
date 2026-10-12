//! R20 PRF-04 claim-path gate cells (Ir gate support; NO counter reads).
//!
//! Entry point: `HeapRegistry::dbg_claim_lease()` — the `internals`-gated safe
//! observer whose body is exactly `claim_lease()` (`claim_impl(HeapCore::new,
//! |_| {})`), the call `tls_heap::bind_slow_tagged` makes on a thread's first
//! allocating call. `HeapLease::drop` is the thread-exit release (LIVE -> FREE
//! plus `reuse_hint`). PRF-04 changes only code inside that claim/release
//! path, so this is the layer the decision applies to.
//!
//! Why not spawned threads through `GlobalAlloc`: a smoke run showed that
//! callgrind (iai-callgrind) records 0 Ir in worker-thread output files; only
//! the thread that enters the benchmark function is collected. A thread-based
//! cell would therefore measure spawn/join cost and miss the claim entirely.
//! Both families here run on the thread that is collected, deterministically.
//!
//! - cold: `claims` leases are claimed and forgotten (slots stay LIVE), so
//!   every claim is an EMPTY-first-claim bump with a fresh `HeapCore::new`.
//! - recycled: one warmup claim+drop materialises a slot and leaves it FREE
//!   with `reuse_hint` pointing at it; each measured claim reclaims it through
//!   the hint and drops it again (claim + release both inside the slope).
//!
//! iai-callgrind runs each `#[library_benchmark]` in a fresh process, so the
//! registry always starts empty.

use sefer_alloc::registry::HeapRegistry;
use std::hint::black_box;

/// Claim count for the N cell.
pub const PRF04_N: usize = 16;
/// Claim count for the 2N cell.
pub const PRF04_2N: usize = 32;
/// Claim count for the 4N (linearity) cell.
pub const PRF04_4N: usize = 64;
/// Warmup claim+drop pairs of the recycled family (identical in N/2N/4N, so
/// it cancels in the Ir(2N) - Ir(N) slope).
pub const PRF04_WARMUP_K: usize = 1;

/// `claims` EMPTY-first-claim bumps; every lease is forgotten so its slot
/// stays LIVE and can never be reclaimed.
pub fn cold_first_claim(claims: usize) {
    for _ in 0..claims {
        let lease = HeapRegistry::dbg_claim_lease().expect("PRF-04 cold claim");
        black_box(&lease);
        core::mem::forget(lease);
    }
}

/// Warmup: `PRF04_WARMUP_K` claim+drop pairs leave an initialised FREE slot
/// with `reuse_hint` pointing at it.
pub fn recycled_warmup() {
    for _ in 0..PRF04_WARMUP_K {
        drop(HeapRegistry::dbg_claim_lease().expect("PRF-04 warmup claim"));
    }
}

/// `claims` reuse-hint reclaims, each followed by its release.
pub fn recycled_measured(claims: usize) {
    for _ in 0..claims {
        let lease = HeapRegistry::dbg_claim_lease().expect("PRF-04 recycled claim");
        black_box(&lease);
        drop(lease);
    }
}

/// Full recycled cell body: warmup, then `claims` measured reclaims.
pub fn recycled_first_claim(claims: usize) {
    recycled_warmup();
    recycled_measured(claims);
}
