//! R20 PRF-04 claim-path gate cells (Ir gate support; NO counter reads).
//!
//! Entry point: `HeapRegistry::dbg_claim_lease()` — the `internals`-gated safe
//! observer whose body is exactly `claim_lease()` (`claim_impl(HeapCore::new,
//! |_| {})`), the call `tls_heap::bind_slow_tagged` makes on a thread's first
//! allocating call. `HeapLease::drop` is the thread-exit release (LIVE -> FREE
//! plus `reuse_hint`). PRF-04 changes only code inside that claim/release
//! path, so this is the layer the decision applies to.
//!
//! Why not spawned threads through `GlobalAlloc`: under the default iai
//! toggle callgrind records 0 Ir in worker-thread output files (verified; see
//! the preregistration), so a thread-based cell would measure spawn/join and
//! miss the claim. Every family here runs on the collected thread.
//!
//! - start: one claim from an empty registry (bootstrap + first EMPTY bump);
//!   the direct construction cell.
//! - cold: `claims` leases are claimed and forgotten (slots stay LIVE), so
//!   every claim is an EMPTY-first-claim bump with a fresh `HeapCore::new`.
//! - recycled: one warmup claim+drop leaves one FREE slot with `reuse_hint`
//!   on it; each measured iteration claims it back and releases it again.
//! - wave: `PRF04_WAVE_SLOTS` claims are released together (many FREE slots,
//!   `reuse_hint` on the last), then `claims` leases are claimed and
//!   forgotten. Main serves one claim by hint and bumps for the rest (the FREE
//!   slots stay stranded until the registry is saturated); candidate B serves
//!   the rest from its FREE-slot hint set. The paths differ by design.
//!
//! iai-callgrind runs each `#[library_benchmark]` in a fresh process, so the
//! registry always starts empty.

use sefer_alloc::registry::HeapRegistry;
use std::hint::black_box;

/// Claim count for the N cell.
pub const PRF04_N: usize = 16;
/// Claim count for the 2N cell.
pub const PRF04_2N: usize = 32;
/// Claim count for the 4N cell.
pub const PRF04_4N: usize = 64;
/// Warmup claim+drop pairs of the recycled family (identical in N/2N/4N, so
/// it cancels in the slope).
pub const PRF04_WARMUP_K: usize = 1;
/// FREE slots the wave family leaves behind (covers the largest measured
/// claim count and one full 64-bit word of the free-slot hint set).
pub const PRF04_WAVE_SLOTS: usize = 64;

/// `claims` EMPTY-first-claim bumps; every lease is forgotten so its slot
/// stays LIVE and can never be reclaimed.
pub fn cold_first_claim(claims: usize) {
    for _ in 0..claims {
        let lease = HeapRegistry::dbg_claim_lease().expect("PRF-04 cold claim");
        black_box(&lease);
        core::mem::forget(lease);
    }
}

/// The construction cell: bootstrap plus one first claim.
pub fn start_first_claim() {
    cold_first_claim(1);
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

/// Wave warmup: claim `PRF04_WAVE_SLOTS` leases, then release them all, in
/// claim order. The same allocation pattern precedes every wave cell.
pub fn wave_warmup() {
    let mut held = Vec::with_capacity(PRF04_WAVE_SLOTS);
    for _ in 0..PRF04_WAVE_SLOTS {
        held.push(HeapRegistry::dbg_claim_lease().expect("PRF-04 wave warmup claim"));
    }
    drop(held);
}

/// `claims` leases claimed after the wave; each is forgotten so the next
/// claim cannot see it as FREE.
pub fn wave_measured(claims: usize) {
    for _ in 0..claims {
        let lease = HeapRegistry::dbg_claim_lease().expect("PRF-04 wave claim");
        black_box(&lease);
        core::mem::forget(lease);
    }
}

/// Full wave cell body: warmup, then `claims` measured claims.
pub fn wave_first_claim(claims: usize) {
    wave_warmup();
    wave_measured(claims);
}
