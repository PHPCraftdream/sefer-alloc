//! R2-08 (independent src review round 2, task #2010) — `GlobalAlloc` must
//! not unwind on a legitimately reachable path: the DIRECT-trait-call surface.
//!
//! ## The defect
//!
//! `HeapRegistry::claim_with_config`'s re-claim branch (N2 / task #95) used
//! to signal a config conflict — a recycled, already-materialised registry
//! slot re-claimed through a `SeferAlloc` instance carrying a DIFFERENT
//! `LargeCacheConfig` — with a `debug_assert!`, i.e. a real `panic!` in every
//! `debug_assertions` build. That branch is the cold TLS bind path behind
//! `SeferAlloc::current_heap()`, which `GlobalAlloc::alloc` / `alloc_zeroed`
//! / `realloc` call directly. Reaching it needs no invalid `Layout` and no
//! caller-contract violation — only two `SeferAlloc` instances with different
//! configs sharing the process-global registry, which `with_config`'s own
//! doc describes (first-materialisation-wins, counted in
//! `AllocStats::config_conflicts`).
//!
//! On a DIRECT trait call (`GlobalAlloc::alloc(&instance, layout)`, generic
//! `A: GlobalAlloc` code, `&dyn GlobalAlloc`) the std `__rust_alloc*` shims —
//! `#[rustc_nounwind]`, which the pre-fix module doc relied on to turn an
//! escaping panic into an abort — are not on the stack at all, so the panic
//! unwound straight out of `GlobalAlloc::alloc`. The trait's safety contract
//! forbids that unconditionally (it is UB).
//!
//! ## The fix
//!
//! The `debug_assert!` is removed: the conflict is signalled ONLY by the
//! always-compiled `CONFIG_CONFLICTS` counter (`SeferAlloc::stats()
//! .config_conflicts`), with the slot's existing config winning — exactly the
//! release behaviour, now in every build profile. The `ConflictRollback`
//! guard, which existed solely to restore the slot during that assert's
//! unwind, is gone with it.
//!
//! ## What this file proves (debug profile is the load-bearing one)
//!
//! Under a plain `cargo test` (`debug_assertions` ON — the profile in which
//! the pre-fix code panics) each test drives a REAL config conflict and
//! asserts, via `catch_unwind`, that the call returned normally, returned
//! usable memory, and that the conflict was still counted (so the test is
//! not vacuous: the conflict branch genuinely ran). Pre-fix every one of the
//! direct-call tests fails with `Err` from `catch_unwind` carrying the
//! "sefer-alloc: config conflict on recycled heap slot" payload.
//!
//! This binary deliberately installs NO `#[global_allocator]` (System stays
//! the global allocator), so a spawned thread's FIRST `SeferAlloc` call is
//! guaranteed to be the direct trait call under test — std's own per-thread
//! allocations cannot bind a registry slot first. The `#[global_allocator]`
//! surface is covered separately by
//! `tests/regression_r2_08_global_allocator_path_no_unwind.rs`.

#![cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]

use core::alloc::{GlobalAlloc, Layout};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, MutexGuard};

use sefer_alloc::{LargeCacheConfig, SeferAlloc};

/// Two configs differing in a resolved field (`budget_bytes`), so
/// `live_config_matches` reports a genuine mismatch (same pair as
/// `tests/regression_r4_3_config_conflict.rs`).
const CONFIG_A: LargeCacheConfig = LargeCacheConfig::new().budget_bytes(64 * 1024 * 1024);
const CONFIG_B: LargeCacheConfig = LargeCacheConfig::new().budget_bytes(128 * 1024 * 1024);

static ALLOC_A: SeferAlloc = SeferAlloc::with_config(CONFIG_A);
static ALLOC_B: SeferAlloc = SeferAlloc::with_config(CONFIG_B);

/// The registry and `CONFIG_CONFLICTS` are process-global and every test
/// relies on LIFO `free_slots` reuse landing on the slot it just primed, so
/// the tests are serialized. Poison-tolerant: a failing test must not turn
/// every later test into a spurious poison failure.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn conflicts() -> u64 {
    SeferAlloc::new().stats().config_conflicts
}

const LAYOUT: Layout = match Layout::from_size_align(64, 8) {
    Ok(l) => l,
    Err(_) => panic!("static layout"),
};

/// Bind a registry slot with `CONFIG_A` on a fresh thread and let the thread
/// exit, so its `AbandonGuard` recycles the slot onto the top of the LIFO
/// free stack with `CONFIG_A` baked in. Returns one block allocated from
/// that slot's heap and deliberately left live (as a `usize`), for the
/// `realloc` case; the other cases ignore it.
fn prime_slot_with_config_a() -> usize {
    std::thread::spawn(|| {
        // SAFETY: valid non-zero-size layout; the block is intentionally left
        // live (returned to the caller), never freed twice.
        let p = unsafe { ALLOC_A.alloc(LAYOUT) };
        assert!(!p.is_null(), "priming allocation failed");
        p as usize
    })
    .join()
    .expect("priming thread panicked")
}

/// Run `first_call` as the FIRST `SeferAlloc` operation on a fresh thread —
/// so it takes the cold bind path through `ALLOC_B` (CONFIG_B) and re-claims
/// the CONFIG_A slot primed just before — and assert it neither unwinds nor
/// skips the conflict signal.
fn assert_first_call_does_not_unwind(entry: &str, first_call: fn(usize)) {
    let leaked = prime_slot_with_config_a();
    let before = conflicts();
    let outcome = std::thread::spawn(move || {
        catch_unwind(AssertUnwindSafe(|| first_call(leaked))).map_err(|payload| {
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_else(|| "<non-string panic payload>".to_string())
        })
    })
    .join()
    .expect("worker thread panicked outside catch_unwind");
    let after = conflicts();

    if let Err(msg) = outcome {
        panic!(
            "R2-08 regression: a direct `GlobalAlloc::{entry}` call UNWOUND out of \
             the allocator on a config-conflict bind (UB per the GlobalAlloc \
             contract — no #[rustc_nounwind] shim on a direct call). Panic \
             payload: {msg:?}"
        );
    }
    assert_eq!(
        after - before,
        1,
        "the {entry} bind did not take the config-conflict branch exactly once \
         (before={before}, after={after}) — the scenario is vacuous if the \
         conflict never happened"
    );
}

#[test]
fn direct_alloc_on_config_conflict_bind_does_not_unwind() {
    let _g = serial();
    assert_first_call_does_not_unwind("alloc", |_| {
        // SAFETY: valid layout; the block is freed once, below, on the same
        // thread that allocated it.
        unsafe {
            let p = ALLOC_B.alloc(LAYOUT);
            assert!(!p.is_null(), "alloc returned null on the conflict bind");
            p.write_bytes(0xA5, LAYOUT.size());
            ALLOC_B.dealloc(p, LAYOUT);
        }
    });
}

#[test]
fn direct_alloc_zeroed_on_config_conflict_bind_does_not_unwind() {
    let _g = serial();
    assert_first_call_does_not_unwind("alloc_zeroed", |_| {
        // SAFETY: valid layout; the block is read within bounds and freed
        // once, on the same thread that allocated it.
        unsafe {
            let p = ALLOC_B.alloc_zeroed(LAYOUT);
            assert!(
                !p.is_null(),
                "alloc_zeroed returned null on the conflict bind"
            );
            let bytes = core::slice::from_raw_parts(p, LAYOUT.size());
            assert!(
                bytes.iter().all(|&b| b == 0),
                "alloc_zeroed block not zeroed"
            );
            ALLOC_B.dealloc(p, LAYOUT);
        }
    });
}

#[test]
fn direct_realloc_on_config_conflict_bind_does_not_unwind() {
    let _g = serial();
    assert_first_call_does_not_unwind("realloc", |leaked| {
        let p = leaked as *mut u8;
        // SAFETY: `p` is a live `LAYOUT` block from the primed slot's heap
        // (every `SeferAlloc` instance serves one process-global registry,
        // so it is "currently allocated via this allocator"); the bind below
        // re-claims that very slot (LIFO), so the realloc is own-thread. The
        // result is freed once, with its new layout.
        unsafe {
            p.write_bytes(0x5A, LAYOUT.size());
            let new_size = 256;
            let q = ALLOC_B.realloc(p, LAYOUT, new_size);
            assert!(!q.is_null(), "realloc returned null on the conflict bind");
            let kept = core::slice::from_raw_parts(q, LAYOUT.size());
            assert!(
                kept.iter().all(|&b| b == 0x5A),
                "realloc lost the old contents"
            );
            ALLOC_B.dealloc(
                q,
                Layout::from_size_align_unchecked(new_size, LAYOUT.align()),
            );
        }
    });
}

/// The registry-level leg (the established `regression_r4_3_config_conflict`
/// driving pattern), now STRICT: the mismatched re-claim must return
/// normally in every profile — pre-fix it panicked in debug builds, which
/// the older test tolerated. Also pins first-wins: the returned heap is the
/// same slot, and it stays reclaimable (no leak).
#[cfg(feature = "internals")]
#[test]
fn registry_claim_with_config_conflict_returns_normally() {
    use sefer_alloc::registry::HeapRegistry;

    let _g = serial();
    let heap_a = HeapRegistry::claim_with_config(CONFIG_A);
    assert!(!heap_a.is_null());
    // SAFETY: `heap_a` was just returned by `claim_with_config`.
    let slot_idx = unsafe { (*heap_a).id() };
    // SAFETY: returned by `claim_with_config`, not yet recycled.
    unsafe { HeapRegistry::recycle(heap_a) };

    let before = conflicts();
    let heap_b = match catch_unwind(|| HeapRegistry::claim_with_config(CONFIG_B)) {
        Ok(h) => h,
        Err(_) => panic!(
            "R2-08 regression: claim_with_config panicked on a config conflict \
             (the cold bind path behind every GlobalAlloc method)"
        ),
    };
    let after = conflicts();
    assert_eq!(after - before, 1, "conflict not counted exactly once");
    assert!(!heap_b.is_null(), "conflicting re-claim returned null");
    // SAFETY: `heap_b` was just returned by `claim_with_config`.
    assert_eq!(
        unsafe { (*heap_b).id() },
        slot_idx,
        "first-wins: the conflicting re-claim must reuse the recycled slot"
    );
    // SAFETY: returned by `claim_with_config`, not yet recycled.
    unsafe { HeapRegistry::recycle(heap_b) };

    // Still reclaimable as the same slot — the conflict leaked nothing.
    let heap_c = HeapRegistry::claim_with_config(CONFIG_A);
    assert!(!heap_c.is_null());
    // SAFETY: `heap_c` was just returned by `claim_with_config`.
    assert_eq!(
        unsafe { (*heap_c).id() },
        slot_idx,
        "slot leaked after conflict"
    );
    // SAFETY: returned by `claim_with_config`, not yet recycled.
    unsafe { HeapRegistry::recycle(heap_c) };
}

/// Steady-state leg (checked separately from the cold config-conflict bind,
/// as the finding's acceptance asks): extreme-but-VALID layouts — alignments
/// up to `1 << 62`, sizes at segment boundaries and up to the `isize::MAX`
/// `Layout` ceiling — through all four `GlobalAlloc` methods on an ALREADY
/// BOUND heap must return (memory or null), never unwind. Under a debug
/// build this also catches any size/alignment arithmetic that would trip an
/// overflow check. Sizes are chosen so every request either is small or
/// cannot be satisfied (no large commit / zero-fill load on the machine).
///
/// Isolation from the cold path: runs on a fresh thread whose heap is bound
/// by a warm-up call through `ALLOC_A` onto a slot primed with `CONFIG_A`
/// (no conflict), and asserts the conflict counter does not move during the
/// sweep — so this leg passes or fails on steady-state behaviour alone,
/// independent of the config-conflict fix the other tests pin.
#[test]
fn extreme_valid_layouts_never_unwind_steady_state() {
    let _g = serial();
    let _ = prime_slot_with_config_a();
    let unwound = std::thread::spawn(|| {
        // Warm-up: bind this thread's heap (cold path, conflict-free).
        // SAFETY: valid layout; freed once, same thread.
        unsafe {
            let p = ALLOC_A.alloc(LAYOUT);
            assert!(!p.is_null());
            ALLOC_A.dealloc(p, LAYOUT);
        }
        let before = conflicts();
        let unwound = steady_state_sweep();
        assert_eq!(
            conflicts(),
            before,
            "the sweep re-entered the cold bind path — not a steady-state check"
        );
        unwound
    })
    .join()
    .expect("steady-state worker panicked outside catch_unwind");
    assert!(
        unwound.is_empty(),
        "GlobalAlloc calls unwound: {unwound:#?}"
    );
}

fn steady_state_sweep() -> Vec<String> {
    const SEG: usize = 1 << 22;
    let max = isize::MAX as usize;
    let aligns = [1usize, 8, 4096, 1 << 16, SEG, SEG << 1, 1 << 30, 1 << 62];
    let sizes = [
        1usize,
        7,
        4095,
        4096,
        1 << 20,
        SEG - 1,
        SEG,
        SEG + 1,
        1 << 47,
        1 << 60,
        max / 2 + 1,
        max,
    ];

    let mut unwound = Vec::new();
    for &align in &aligns {
        for &size in &sizes {
            let size = size.min(max - (align - 1));
            let Ok(layout) = Layout::from_size_align(size, align) else {
                continue;
            };
            for zeroed in [false, true] {
                let r = catch_unwind(AssertUnwindSafe(|| {
                    // SAFETY: non-zero-size valid layout; a non-null result
                    // is freed once with the same layout.
                    unsafe {
                        let p = if zeroed {
                            ALLOC_A.alloc_zeroed(layout)
                        } else {
                            ALLOC_A.alloc(layout)
                        };
                        if !p.is_null() {
                            ALLOC_A.dealloc(p, layout);
                        }
                    }
                }));
                if r.is_err() {
                    unwound.push(format!("alloc(zeroed={zeroed}) size={size} align={align}"));
                }
            }
            // realloc a small block of the same alignment up to `size`.
            let small = Layout::from_size_align(16, align.min(SEG)).unwrap();
            if Layout::from_size_align(size, small.align()).is_ok() {
                let r = catch_unwind(AssertUnwindSafe(|| {
                    // SAFETY: `p` is live with layout `small`; `size` rounded
                    // to its alignment fits `isize` (checked above). Exactly
                    // one of `p` / `q` is freed, with its own layout.
                    unsafe {
                        let p = ALLOC_A.alloc(small);
                        if p.is_null() {
                            return;
                        }
                        let q = ALLOC_A.realloc(p, small, size);
                        if q.is_null() {
                            ALLOC_A.dealloc(p, small);
                        } else {
                            ALLOC_A
                                .dealloc(q, Layout::from_size_align_unchecked(size, small.align()));
                        }
                    }
                }));
                if r.is_err() {
                    unwound.push(format!("realloc 16->{size} align={}", small.align()));
                }
            }
        }
    }
    unwound
}
