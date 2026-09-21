//! Task #1995 census probe: how often/expensive is the "bind-less cold arm"
//! (a thread whose TLS bind cannot succeed — registry exhaustion, or TLS
//! teardown mid-allocation — repeating the full claim attempt on EVERY
//! `alloc` call, per the original review's code-reading) actually likely to
//! be in a real deployment? This probe measures case (a) — registry
//! exhaustion — directly; case (b) is argued structurally in the companion
//! report (`docs/perf/TASK1995_BIND_LESS_COLD_ARM_CENSUS.md`), not measured
//! here (forcing genuine TLS-destructor-ordering mid-allocation is not
//! reliably reproducible from a portable probe).
//!
//! **Entry point under test:** `SeferAlloc`'s real `#[global_allocator]`
//! path (ordinary `Vec` allocations below), not a bare `AllocCore`/
//! `HeapCore` call — the cold arm under study lives entirely in
//! `global::tls_heap`'s `current_for_alloc`/`bind_slow_tagged` resolution,
//! upstream of both.
//!
//! **Mechanism to reach exhaustion cheaply.** `HeapRegistry::claim()` is a
//! raw registry operation independent of the calling thread's own TLS
//! binding (it never touches `LOCAL`) — so this ONE thread can claim all
//! `MAX_HEAPS` slots itself, never recycling any, and thereby exhaust the
//! registry without spawning any other threads. This is far cheaper and
//! faster than a real many-thread reproduction (which would need
//! `MAX_HEAPS + 1` = 4097 simultaneously-alive, never-exiting OS threads).
//!
//! **Path-activation oracle.** [`sefer_alloc::global::dbg_fallback_lock_acquisitions`]
//! (an existing counter, no new instrumentation added by this probe) counts
//! every successful fallback-spinlock acquisition process-wide. Once this
//! thread's own registry slot claim is impossible (all `MAX_HEAPS` slots
//! LIVE, `free_slots` empty), every one of its `alloc` calls must route
//! through the fallback and take that lock — so the counter's delta over N
//! calls proves (not assumes) that each call independently redoes the work,
//! rather than caching a "known-hopeless" result after the first.
//!
//! Build/run:
//! `cargo run --release --example task1995_bind_less_cold_arm_census --features "production internals"`

use std::time::Instant;

use sefer_alloc::global::dbg_fallback_lock_acquisitions;
use sefer_alloc::registry::bootstrap::MAX_HEAPS;
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Allocations per measured phase. Each iteration performs one `alloc` +
/// one matching `dealloc` (a `Vec<u8>` drop), so `N` calls should move the
/// lock-acquisition oracle by some multiple of `N` in the exhausted phase
/// (both alloc and dealloc route through the fallback once this thread has
/// no own-thread heap) and by exactly `0` in the warm phase.
const N: u64 = 2_000;

fn one_alloc_dealloc_round() {
    // A `Vec<u8>` forces a real heap allocation (not inlined/elided) and
    // its `Drop` frees it before the next iteration — this exercises both
    // `alloc` and `dealloc` on the SAME allocator instance per round.
    let v: Vec<u8> = Vec::with_capacity(64);
    core::hint::black_box(&v);
}

fn main() {
    // ---- Warm-path baseline (this thread's OWN, freshly claimed slot) ----
    // One allocation up front claims this thread's own registry slot via
    // the ordinary (non-exhausted) path, so the timed baseline below never
    // pays a first-claim cost.
    one_alloc_dealloc_round();
    let warm_lock_before = dbg_fallback_lock_acquisitions();
    let warm_start = Instant::now();
    for _ in 0..N {
        one_alloc_dealloc_round();
    }
    let warm_elapsed = warm_start.elapsed();
    let warm_lock_delta = dbg_fallback_lock_acquisitions() - warm_lock_before;
    println!(
        "phase=warm_own_slot n={N} elapsed_ns_total={} ns_per_round={:.1} fallback_lock_delta={}",
        warm_elapsed.as_nanos(),
        warm_elapsed.as_nanos() as f64 / N as f64,
        warm_lock_delta
    );

    // ---- Fill the registry from THIS SAME thread, via the raw registry
    // op, bypassing this thread's already-bound TLS entirely. A SEPARATE
    // OS thread is used for the exhausted-phase measurement below so its
    // TLS starts genuinely unbound (this thread's own `LOCAL` is already
    // populated by the warm-path baseline above and cannot be un-bound
    // without real thread exit). ----
    let fill_start = Instant::now();
    // `MAX_HEAPS - 1`: this thread's own warm-path claim above already
    // holds one of the `MAX_HEAPS` slots, so filling the remaining
    // `MAX_HEAPS - 1` completes the exhaustion for any OTHER thread.
    let to_fill = MAX_HEAPS - 1;
    let mut claimed = Vec::with_capacity(to_fill);
    for _ in 0..to_fill {
        let ptr = HeapRegistry::claim();
        assert!(
            !ptr.is_null(),
            "registry claim failed before intended exhaustion"
        );
        claimed.push(ptr);
    }
    let fill_elapsed = fill_start.elapsed();
    println!(
        "phase=fill_registry slots_filled={to_fill} elapsed_ms={:.3}",
        fill_elapsed.as_secs_f64() * 1000.0
    );

    // ---- Exhausted-arm measurement, on a fresh thread (genuinely unbound
    // TLS) ----
    let handle = std::thread::spawn(move || {
        let before = dbg_fallback_lock_acquisitions();
        let start = Instant::now();
        for _ in 0..N {
            one_alloc_dealloc_round();
        }
        let elapsed = start.elapsed();
        let after = dbg_fallback_lock_acquisitions();
        (elapsed, after - before)
    });
    let (exhausted_elapsed, exhausted_lock_delta) = handle.join().expect("worker thread panicked");
    println!(
        "phase=exhausted_bind_less n={N} elapsed_ns_total={} ns_per_round={:.1} fallback_lock_delta={}",
        exhausted_elapsed.as_nanos(),
        exhausted_elapsed.as_nanos() as f64 / N as f64,
        exhausted_lock_delta
    );

    // Path-activation assertion: the exhausted thread's lock-acquisition
    // delta must be strictly greater than the warm thread's (which should
    // be exactly 0 — a thread with its own claimed slot never touches the
    // fallback at all) — proving the exhausted arm genuinely routed through
    // the fallback on these calls, not an assumption.
    assert_eq!(
        warm_lock_delta, 0,
        "warm-path baseline unexpectedly touched the fallback lock"
    );
    assert!(
        exhausted_lock_delta > 0,
        "exhausted-arm probe never touched the fallback lock — the census's own oracle failed to activate"
    );

    let ratio = exhausted_elapsed.as_nanos() as f64 / warm_elapsed.as_nanos() as f64;
    println!("summary=exhausted_vs_warm_ns_ratio value={ratio:.2}");

    // Keep every claimed pointer alive for the whole run (never recycled),
    // matching the "these slots are LIVE forever" registry-exhaustion
    // scenario this probe measures.
    core::hint::black_box(&claimed);
}
