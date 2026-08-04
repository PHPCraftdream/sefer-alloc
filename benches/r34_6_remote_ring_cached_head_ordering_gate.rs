//! R34-6 (task #525) — cost gate for promoting `cached_head`'s ordering
//! from `Relaxed` to `Acquire`/`Release` in `RemoteFreeRing::full_check`.
//!
//! ## Background (finding F-1, release-stabilization audit)
//!
//! The F10 shadow-head fast path (`full_check`, `remote_free_ring.rs:1004`)
//! replaced every push's pre-F10 `head.load(Acquire)` with a
//! `cached_head.load(Relaxed)` on the producer's own cache line. That is a
//! *value-domain* win (proven: `cached_head <= head` always, so the fast
//! path can only under-estimate occupancy). But it removed the only
//! happens-before edge that ordered the consumer's `slot.store(EMPTY)`
//! before a producer's `slot.store(offset)` into a recycled slot. The
//! promotion restores that edge ON `cached_head` itself (Acquire load +
//! Release store), which is already on the producer's own line — no new
//! cross-core traffic; the cost is fence *strength*, not a fence
//! instruction.
//!
//! ## What is measured
//!
//! The push-heavy ns/op of a raw `RemoteFreeRing::push` → `full_check`
//! cycle, with the fast path taken ≥95% of the time (verified by the
//! `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW` path-activation oracle below). This
//! is the exact path the promotion touches: `full_check`'s fast-path
//! `cached_head.load` (Relaxed→Acquire) and slow-path
//! `cached_head.store` (Relaxed→Release).
//!
//! ## A/B method
//!
//! The ordering is baked into the source, not a Cargo feature, so the A/B
//! is done by building this SAME bench against two source trees — current
//! (Relaxed) vs promoted (Acquire/Release) — and comparing criterion's mean
//! ns/iter. This mirrors the established "build two worktrees" pattern for
//! non-feature-gated before/after gates in this project (see
//! `docs/perf/R32_11_REMOTE_RING_SHADOW_HEAD_GATE.md`).
//!
//! ## Entry point
//!
//! `RemoteFreeRing::push` (the raw ring, NOT `HeapCore`/`AllocCore`) — the
//! promotion lives in `full_check`, which `push`/`try_push_uncounted` call
//! directly. Measuring the raw ring isolates the ordering cost from
//! magazine/tier/registry overhead that would mask a sub-nanosecond delta.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "internals"
))]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::hint::black_box;
use std::sync::atomic::Ordering::Relaxed;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

use sefer_alloc::alloc_core::remote_free_ring::{
    RemoteFreeRing, DBG_RING_PUSH_SHADOW_FAST, DBG_RING_PUSH_SHADOW_SLOW, FOOTPRINT, RING_CAP,
};

// Pin that the batch stays well under capacity so every push takes the
// shadow fast path (the promotion's cost lives on that load).
const _: () = assert!(
    PUSHES_PER_ITER < RING_CAP,
    "PUSHES_PER_ITER must stay under RING_CAP so the fast path always fires"
);

/// Pushes per timed iteration. Kept well under `RING_CAP` (256) so every
/// push takes the shadow fast path (`t.wrapping_sub(cached_head) < RING_CAP`
/// with `cached_head` stale-low at 0): the promotion's cost lives on that
/// load, so a bench that takes the slow path would measure the wrong thing.
const PUSHES_PER_ITER: usize = 128;

/// A buffer of `FOOTPRINT` bytes, 4-byte aligned, for a standalone ring.
fn ring_buffer() -> Box<[u8]> {
    let buf: Vec<u8> = vec![0u8; FOOTPRINT];
    assert!(
        (buf.as_ptr() as usize).is_multiple_of(core::mem::align_of::<u32>()),
        "ring buffer must be 4-byte aligned"
    );
    buf.into_boxed_slice()
}

/// Hard-assert the fast path fires ≥95% of the time (path-activation oracle,
/// CLAUDE.md R30-8 rule). Without this, a bench that silently took the slow
/// path would measure `head.load(Acquire)` instead of `cached_head.load`.
/// Runs ONCE at bench-group setup (not in the timed region).
#[cfg(feature = "bench-internals")]
fn assert_fast_path_dominates(ring: &RemoteFreeRing) {
    let fast_before = DBG_RING_PUSH_SHADOW_FAST.load(Relaxed);
    let slow_before = DBG_RING_PUSH_SHADOW_SLOW.load(Relaxed);
    for i in 0..PUSHES_PER_ITER as u32 {
        let _ = ring.push((i + 1) * 16);
    }
    ring.drain(|_| {});
    let fast_after = DBG_RING_PUSH_SHADOW_FAST.load(Relaxed);
    let slow_after = DBG_RING_PUSH_SHADOW_SLOW.load(Relaxed);
    let fast = fast_after - fast_before;
    let slow = slow_after - slow_before;
    let total = fast + slow;
    assert!(total > 0, "oracle: at least one push must have happened");
    let pct = fast * 100 / total;
    assert!(
        pct >= 95,
        "oracle: fast path must fire >=95% of pushes, got {fast}/{total} ({pct}%)"
    );
}

/// Bench the push-heavy fast-path cost: PUSHES_PER_ITER pushes then one drain.
/// The drain keeps the ring balanced (never overflows); the timed cost is
/// dominated by the PUSHES_PER_ITER `full_check` fast-path loads.
fn bench_push_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("r34_6_cached_head_ordering");
    group.sample_size(50);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    let buf = ring_buffer();
    let base = buf.as_ptr() as *mut u8;
    // SAFETY: `base` is a FOOTPRINT-sized, 4-byte-aligned, exclusively-owned
    // boxed buffer, live for the whole bench (see `ring_buffer()`).
    let ring = unsafe {
        RemoteFreeRing::init_test_buffer(base);
        RemoteFreeRing::over_test_buffer(base)
    };

    #[cfg(feature = "bench-internals")]
    assert_fast_path_dominates(&ring);

    group.bench_function("push_heavy_fast_path", |b| {
        let mut off: u32 = 16;
        b.iter(|| {
            for _ in 0..PUSHES_PER_ITER {
                // Push a valid offset (never RING_SLOT_EMPTY = u32::MAX).
                off = off.wrapping_add(16);
                if off >= u32::MAX - 32 {
                    off = 16;
                }
                let _ = ring.push(black_box(off));
            }
            // Drain to keep the ring balanced across iterations. The drain
            // cost is constant across A/B (the promotion only touches
            // `full_check`), so it cancels out in the comparison.
            ring.drain(|_reclaimed| {});
        });
    });

    group.finish();
}

criterion_group!(benches, bench_push_heavy);
criterion_main!(benches);
