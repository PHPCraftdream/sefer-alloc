//! R2-08 (independent src review round 2, task #2010) — the
//! `#[global_allocator]` surface of the config-conflict no-unwind fix.
//!
//! Companion to `tests/regression_r2_08_globalalloc_no_unwind.rs` (the
//! direct-trait-call surface; see that file for the full defect write-up).
//! There, a config-conflict bind's `debug_assert!` unwound straight out of
//! `GlobalAlloc::alloc`. HERE the same bind is reached through the real
//! `#[global_allocator]` entry (`__rust_alloc` → `SeferAlloc::alloc` →
//! `current_heap()` → `claim_with_config`). The std `__rust_alloc` shim is
//! `#[rustc_nounwind]`, which the pre-fix module doc read as "an escaping
//! panic aborts the process". Observed pre-fix instead (rustc 1.97.0,
//! x86_64-pc-windows-msvc, debug): the panic unwound straight through
//! `__rust_alloc` and `alloc::alloc::Global::alloc_impl` up to the spawned
//! thread's boundary — an unwind through frames the compiler treats as
//! non-unwinding, not a guaranteed abort.
//!
//! Since the pre-fix outcome may be an abort OR an unwind depending on
//! toolchain/target, the scenario runs in a child process (this same test
//! binary, re-executed with an env marker and an exact test filter), which
//! observes both uniformly. The parent asserts the child exited successfully
//! and reported a non-zero config-conflict delta (the conflict branch
//! genuinely ran on the global path — not a vacuous pass).
//!
//! Scenario (child): mint a registry slot materialised with `CONFIG_B` via
//! `HeapRegistry::claim_with_config` and recycle it LAST, so it is the top of
//! the LIFO free stack; then spawn a thread. That thread's first allocation —
//! whichever std- or user-level allocation comes first — goes through the
//! `#[global_allocator]` (`CONFIG_G`), binds, pops the `CONFIG_B` slot and
//! takes the conflict branch. Pre-fix (debug): panic (the child fails).
//! Post-fix: counted, first-wins, returns normally.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals"
))]

use std::process::Command;

use sefer_alloc::registry::{config_conflicts_total, HeapRegistry};
use sefer_alloc::{LargeCacheConfig, SeferAlloc};

const CONFIG_G: LargeCacheConfig = LargeCacheConfig::new().budget_bytes(64 * 1024 * 1024);
const CONFIG_B: LargeCacheConfig = LargeCacheConfig::new().budget_bytes(128 * 1024 * 1024);

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG_G);

const CHILD_ENV: &str = "SEFER_R2_08_GLOBAL_PATH_CHILD";
const TEST_NAME: &str = "global_allocator_bind_on_config_conflict_does_not_panic";
const MARKER: &str = "R2_08_CHILD conflicts_delta=";

/// Child-side scenario. Runs only under `CHILD_ENV`.
fn child_scenario() {
    // 1. Obtain a heap materialised with CONFIG_B: a claim that did NOT bump
    //    the conflict counter either minted a fresh slot with CONFIG_B or
    //    re-claimed one already carrying it. Conflicting (non-B) re-claims
    //    are held and recycled first, so the B slot ends up on top.
    let mut held = Vec::new();
    let b_heap = loop {
        assert!(held.len() < 64, "could not obtain a CONFIG_B slot");
        let before = config_conflicts_total();
        let h = HeapRegistry::claim_with_config(CONFIG_B);
        assert!(!h.is_null(), "registry claim returned null");
        if config_conflicts_total() == before {
            break h;
        }
        held.push(h);
    };
    for h in held {
        // SAFETY: each `h` came from `claim_with_config` and is recycled once.
        unsafe { HeapRegistry::recycle(h) };
    }
    // SAFETY: from `claim_with_config`, recycled once — now the LIFO top.
    unsafe { HeapRegistry::recycle(b_heap) };

    // 2. A fresh thread's first global allocation binds through GLOBAL
    //    (CONFIG_G) and re-claims the CONFIG_B slot: a conflict on the
    //    `#[global_allocator]` path.
    let before = config_conflicts_total();
    let sum = std::thread::spawn(|| {
        let v = vec![7u8; 4096];
        v.iter().map(|&b| u64::from(b)).sum::<u64>()
    })
    .join()
    .expect("worker thread panicked");
    let delta = config_conflicts_total() - before;
    assert_eq!(sum, 7 * 4096);
    println!("{MARKER}{delta}");
}

#[test]
fn global_allocator_bind_on_config_conflict_does_not_panic() {
    if std::env::var_os(CHILD_ENV).is_some() {
        child_scenario();
        return;
    }

    let exe = std::env::current_exe().expect("current_exe");
    let out = Command::new(exe)
        .args(["--exact", TEST_NAME, "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .expect("spawn child test process");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        out.status.success(),
        "R2-08 regression: the child process did not exit cleanly ({:?}) — a \
         config-conflict bind on the #[global_allocator] path panicked (it \
         unwound through the #[rustc_nounwind] shim, or aborted).\n--- child stdout ---\n\
         {stdout}\n--- child stderr ---\n{stderr}",
        out.status
    );
    let delta: u64 = stdout
        .lines()
        // libtest may print the marker on the same line as `test <name> ...`.
        .find_map(|l| l.split_once(MARKER).map(|(_, v)| v))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
        .unwrap_or_else(|| {
            panic!("child did not report {MARKER}<n>\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}")
        });
    assert!(
        delta >= 1,
        "the child's global-path bind never hit the config-conflict branch \
         (delta = {delta}) — the scenario would be vacuous"
    );
}
