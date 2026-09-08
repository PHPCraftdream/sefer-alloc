//! Exploratory measurement for the 3 P4 performance follow-ups deferred by
//! Sol-codex review rounds 2-4 (per-byte fill/verify cost, O(M^2) overlap
//! scan, `BoxedStrategy` heap-allocation overhead). Not a shipped
//! benchmark — a one-off probe to decide whether any of the three is worth
//! a structural fix, per this project's measurement-before-restructuring
//! discipline. Requires `--features proptest` for the P4-5 section.
//!
//! Scope of the overlap-scan verdict: it is a NO-GO for the regimes actually
//! measured below — K=2048 (the `arbitrary` front-end's per-`OpStream`
//! decode-attempt ceiling) and K~200 (the native integration tests' own
//! stream length) — NOT a claim that those K values bound the crate. The
//! driver's real cost is O(M + M*K + B), worst case O(M^2 + B), where M is
//! the op count, K the peak live count, and B the total oracle byte work
//! (fill + verify passes over block contents): every block-creating op is
//! compared against every currently-live block. `op_strategy`'s `len_range`
//! is caller-supplied and `Config` permits a frequent (not merely rare)
//! large arm, so a hand-built or long generated stream can reach a far
//! larger peak live count than either measured regime; the linear scan
//! stays because no measured regime approached a cost that would pay for an
//! ordered interval index.
//!
//! `pattern_byte` is duplicated here verbatim from `src/drive.rs` (it is
//! private) — same "independent copy" rationale `tests/oracle_negative.rs`
//! already uses.
//!
//! # Three separately labeled scenarios (P3-2)
//!
//! The previous single `draw:` label was wrong: its one measurement window
//! opened before `new_tree` and then ALWAYS ran a full `while tree.simplify()`
//! walk before dropping, so it measured construction PLUS a mandatory
//! simplify-only walk, not a plain successful draw. The published
//! 848,124/915,594 B figures came from that combined protocol. The
//! distinction is not cosmetic: proptest's `TupleUnion` initializes
//! previously-unselected variants while simplifying, so on the boxed arm the
//! full-simplify regime pays extra size-`Box` allocations that a successful
//! case (which never triggers shrinking) never creates. Arm ordering by total
//! or peak memory is therefore NOT necessarily the same in the two regimes.
//! This probe now reports three separately labeled scenarios per arm:
//!
//! - `S1 successful-draw (new_tree -> current -> drop; NO shrinking)` — the
//!   successful property case.
//! - `S2 simplify-only walk (while simplify(); no per-step current(), no
//!   complicate)` — the old microbenchmark, kept because it isolates
//!   TupleUnion's lazy-branch materialization mechanics; at 64 seeds it is
//!   the apples-to-apples corrected number for the old (mislabeled)
//!   protocol.
//! - `S3 shrink-protocol (REAL TestRunner::run_one walk; CUSTOM iteration
//!   budget)` — the shrink scenario itself, driven by proptest's own
//!   `TestRunner::run_one` under an explicitly constructed `Config`
//!   (review P3-1 fix: the earlier hand-rolled loop diverged from the real
//!   runner in three ways — it began every iteration with an unconditional
//!   simplify instead of re-evaluating the restored candidate after a
//!   successful complicate, it did not end the search when complicate
//!   returned false, and it retained the last accepted `Vec<Op>` across the
//!   next candidate's construction, an extra live buffer the real runner
//!   never holds). Its `max_shrink_iters` is an explicitly CUSTOM 65,536 —
//!   NOT proptest's default, which (resolved proptest 1.11.0,
//!   `config.rs:573-579`) resolves the `u32::MAX` sentinel at
//!   `config.rs:177` to `cases * 4` = 256 * 4 = 1024 at default cases; at
//!   1024 evaluations a 200-op stream's walk is cut off before its natural
//!   end (~15.5k evaluations) and the complicate backoff would never fire.
//!   The per-step materialization is a substantial part of the Vec<Op>
//!   alloc+copy cost of real shrinking, so S3 runs on fewer seeds (8 vs 64).
//!
//! # Counter accounting convention (P3-1)
//!
//! `CountingAlloc` separates ATTEMPTED-call counters (`ALLOC_CALLS` for
//! `alloc()`, `REALLOC_CALLS` for `realloc()`, incremented regardless of
//! outcome) from byte accounting for SUCCESSFUL ranges (`LIVE_BYTES`,
//! `TOTAL_BYTES`, `PEAK_LIVE`), which update ONLY after a non-null backend
//! result. On a successful realloc, the OLD size is REPLACED by the NEW size
//! in the live state (one CAS loop); on a null realloc the live state is left
//! completely untouched (per the `GlobalAlloc` contract the old allocation
//! stays valid). `TOTAL_BYTES` counts the cumulative FULL requested size of
//! every SUCCESSFUL call — a successful realloc counts the full new request,
//! not the growth delta. Every measurement window reports its
//! `REALLOC_CALLS` delta so a reader can tell whether the window could have
//! been affected by the pre-fix accounting bug.
//!
//! # Marginal allocs/op numerator convention (review P4-2)
//!
//! The regression test's counting wrapper
//! (`tests/size_strategy_avoids_boxed_value_trees.rs`) does NOT override
//! `GlobalAlloc::realloc`. The trait's default implementation (checked in
//! this toolchain's `core/src/alloc/global.rs`) allocates the new block via
//! `self.alloc(...)` and deallocates the old one only on success — so each
//! realloc event there is EXACTLY one call of the wrapper's own `alloc`,
//! i.e. one increment of its single `ALLOC_CALLS` counter. This probe's
//! realloc-honest counters instead keep `REALLOC_CALLS` separate from
//! `ALLOC_CALLS`, so an alloc-only numerator differs from the test's by
//! exactly the realloc events. Every marginal allocs/op figure is therefore
//! printed under all THREE numerators — attempted `ALLOC_CALLS` only,
//! attempted `REALLOC_CALLS` only, and their SUM — and the SUM is the one
//! comparable to the test's counting. The previously published enum-side
//! 0.015 -> 0.000 change is this definitional exclusion of realloc events
//! from the alloc-only numerator, not run-to-run drift.
//!
//! # Honest build/target markers (P4-3)
//!
//! The output never claims the binary is a release build from inside the
//! binary: `cfg!(debug_assertions) == false` does not prove an optimized
//! profile, and the example runs fine without `--release`. The header prints
//! the `debug_assertions` state as the build claim plus the verbatim
//! canonical command (whose arguments are the invoker's business). The
//! dependency-version line is labeled a RUNTIME Cargo.lock snapshot, NOT
//! build identity: it reads whatever lockfile is reachable from the
//! process's cwd, which can be a different project, or a different lockfile
//! state, than the one this binary was built from (the crate's own
//! `env!("CARGO_PKG_VERSION")` IS build identity). A candidate lockfile is
//! accepted only if it supplies BOTH wanted packages — versions are never
//! merged across files. (True build identity for dependency versions would
//! require binding the resolved graph to the artifact at build time, e.g. a
//! build script emitting the versions into `env!`-read constants; this
//! example deliberately has no build script.)

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use globalalloc_model::{drive, Config, Op};

// --- P4-5: a counting global allocator, active for the whole process ---
//
// Contract (review P3-1 fix):
// - ATTEMPTED-call counters: ALLOC_CALLS (alloc()) and REALLOC_CALLS
//   (realloc()) are incremented on EVERY call, before and regardless of the
//   backend result. The published marginal allocs/op figures are reported
//   under THREE numerators — ALLOC_CALLS only, REALLOC_CALLS only, and
//   their SUM (P4-2; see the numerator-convention section in the file
//   header) — all three attempted-call semantics.
// - Byte accounting (TOTAL_BYTES, LIVE_BYTES, PEAK_LIVE) is updated ONLY on
//   a SUCCESSFUL backend call. A null alloc or null realloc leaves
//   live/total/peak completely unchanged; per the GlobalAlloc contract a
//   failed realloc leaves the old allocation intact.
// - A SUCCESSFUL realloc REPLACES the old size with the new size in the live
//   state (CAS loop), and adds the FULL new_size to TOTAL_BYTES. Convention:
//   TOTAL_BYTES counts the cumulative FULL requested size of every
//   successful call; a successful realloc counts the full new request, not
//   the growth delta.
struct CountingAlloc<B = System> {
    // Backend the counting wrapper forwards to: `System` for the
    // `#[global_allocator]` instance below; the self-check substitutes a
    // deterministic-refusal backend (`RefuseMarked`) so its failing steps
    // do not depend on host memory state. B = System monomorphizes to the
    // same forwarding code the non-generic version had.
    inner: B,
}
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl<B: GlobalAlloc> GlobalAlloc for CountingAlloc<B> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        // Forward FIRST; count bytes only on success.
        // SAFETY: forwarding to the inner backend (`System` for the global
        // instance), as before.
        let ptr = unsafe { self.inner.alloc(layout) };
        if !ptr.is_null() {
            TOTAL_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK_LIVE.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // dealloc is infallible: accounting before the forward is fine.
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: forwarding to the inner backend (`System` for the global
        // instance), as before.
        unsafe { self.inner.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Attempted-call counter, like ALLOC_CALLS above.
        REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        // Forward FIRST; per the GlobalAlloc contract a null result leaves
        // the old allocation intact, so live/total/peak must not move.
        // SAFETY: forwarding to the inner backend (`System` for the global
        // instance), as before.
        let out = unsafe { self.inner.realloc(ptr, layout, new_size) };
        if !out.is_null() {
            // Full-new-request convention, documented above and in the
            // printed legend.
            TOTAL_BYTES.fetch_add(new_size, Ordering::Relaxed);
            // Replace old size with new size in the live state: one CAS loop
            // against concurrent updates (other threads may alloc/free).
            let mut prev = LIVE_BYTES.load(Ordering::Relaxed);
            loop {
                let next = prev - layout.size() + new_size;
                match LIVE_BYTES.compare_exchange_weak(
                    prev,
                    next,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        PEAK_LIVE.fetch_max(next, Ordering::Relaxed);
                        break;
                    }
                    Err(observed) => prev = observed,
                }
            }
        }
        out
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc { inner: System };

// --- P2-1 fix: a controlled-failure backend for the self-check's failing
// steps ---
//
// The old failing steps requested near-`isize::MAX`: the realloc one was
// flatly invalid (isize::MAX rounded up to `layout32`'s align 8 is
// MAX + 1, violating `GlobalAlloc::realloc`'s round-up-to-align <= isize::MAX
// unsafe precondition — not an OOM condition), and the alloc one was valid
// only at align 1 but leaned on real OOM, which is not a portable fixture:
// on 32-bit targets isize::MAX ~ 2 GiB sits inside realistically
// satisfiable virtual memory, so "huge must fail" can legitimately succeed
// there (and on any target the failure depends on host memory pressure).
//
// `RefuseMarked` instead refuses exactly one SMALL, VALID request size —
// one any healthy allocator satisfies — so a null result proves the
// controlled refusal, not memory state, on every target. The refusal fires
// INSIDE the backend `CountingAlloc` forwards to, so the null flows
// through its own null-accounting path (attempted-call counters move,
// byte counters must not).
struct RefuseMarked {
    refused_size: usize,
}

// Deliberately small (4 KiB) and distinct from every other size used by
// the self-check (64/128/32), so no non-failing step can hit the mark.
const REFUSED_SIZE: usize = 4096;

unsafe impl GlobalAlloc for RefuseMarked {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == self.refused_size {
            return core::ptr::null_mut();
        }
        // SAFETY: forwarding to the System allocator, as before.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarding to the System allocator, as before.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Refuse here rather than leaning on the trait's DEFAULT `realloc`.
        // Checked against the toolchain source (core/src/alloc/global.rs):
        // the default allocates the new block FIRST and deallocates the old
        // one only `if !new_ptr.is_null()`, so routing the refusal through
        // it would in fact also leave the old block intact — the override is
        // for directness, not because the default would destroy it. Refusing
        // explicitly keeps this backend's behaviour independent of the
        // default's internals, which the self-check's
        // old-block-survives assertion would otherwise silently depend on.
        if new_size == self.refused_size {
            return core::ptr::null_mut();
        }
        // SAFETY: forwarding to the System allocator, as before.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

// --- P3-1 self-check: exercised unconditionally at process start ---
//
// Direct calls via the GlobalAlloc trait on CountingAlloc itself (NOT
// std::alloc::alloc), so the check is independent of the global-allocator
// wiring. Each step hard-asserts the EXACT expected stored LIVE_BYTES; the
// process is single-threaded at this point, so no concurrent counter updates
// can race the asserts.
//
// Failing steps (P2-1 fix) mark a small valid request and let the backend
// refuse it deterministically — see `RefuseMarked` above.
fn count_alloc_self_check() {
    fn live() -> usize {
        LIVE_BYTES.load(Ordering::Relaxed)
    }
    let baseline = live();

    // One counting instance over the refusing backend: every step flows
    // through the SAME counters and the same forwarding/accounting code as
    // the `#[global_allocator]` instance (B = System); only the backend
    // differs, and it differs only on the marked request size.
    let ca = CountingAlloc {
        inner: RefuseMarked {
            refused_size: REFUSED_SIZE,
        },
    };

    // Grow: alloc 64 B, then realloc to 128 B — live must REPLACE, not add.
    let layout64 = Layout::from_size_align(64, 8).unwrap();
    let p64 = unsafe { ca.alloc(layout64) };
    assert!(!p64.is_null(), "self-check alloc failed");
    assert_eq!(live(), baseline + 64, "alloc did not add exactly 64");
    let p128 = unsafe { ca.realloc(p64, layout64, 128) };
    assert!(!p128.is_null(), "self-check grow realloc failed");
    assert_eq!(
        live(),
        baseline + 128,
        "grow realloc did not replace 64 with 128"
    );

    // Shrink: 128 -> 32 B.
    let layout128 = Layout::from_size_align(128, 8).unwrap();
    let layout32 = Layout::from_size_align(32, 8).unwrap();
    let p32 = unsafe { ca.realloc(p128, layout128, 32) };
    assert!(!p32.is_null(), "self-check shrink realloc failed");
    assert_eq!(
        live(),
        baseline + 32,
        "shrink realloc did not replace 128 with 32"
    );

    // Failing alloc (P2-1 fix): a SMALL, VALID request the backend refuses
    // deterministically — not a near-isize::MAX request. Byte accounting
    // must not move: ALLOC_CALLS counted the ATTEMPT, but LIVE_BYTES /
    // TOTAL_BYTES / PEAK_LIVE update only on success.
    let before_total = TOTAL_BYTES.load(Ordering::Relaxed);
    let before_peak = PEAK_LIVE.load(Ordering::Relaxed);
    let refused_layout = Layout::from_size_align(REFUSED_SIZE, 8).unwrap();
    let null = unsafe { ca.alloc(refused_layout) };
    assert!(
        null.is_null(),
        "self-check expected the refused alloc to fail"
    );
    assert_eq!(live(), baseline + 32, "failed alloc changed live bytes");
    assert_eq!(
        TOTAL_BYTES.load(Ordering::Relaxed),
        before_total,
        "failed alloc changed total bytes"
    );
    assert_eq!(
        PEAK_LIVE.load(Ordering::Relaxed),
        before_peak,
        "failed alloc changed peak live"
    );

    // Failing realloc (P2-1 fix): same controlled-failure backend; the
    // refused new_size (4096 B) would succeed on any healthy allocator, so
    // null proves the refusal, not memory pressure. The old 32 B block
    // must remain intact and live; live/total/peak must not move.
    let null = unsafe { ca.realloc(p32, layout32, REFUSED_SIZE) };
    assert!(
        null.is_null(),
        "self-check expected the refused realloc to fail"
    );
    assert_eq!(live(), baseline + 32, "failed realloc changed live bytes");
    assert_eq!(
        TOTAL_BYTES.load(Ordering::Relaxed),
        before_total,
        "failed realloc changed total bytes"
    );
    assert_eq!(
        PEAK_LIVE.load(Ordering::Relaxed),
        before_peak,
        "failed realloc changed peak live"
    );
    // Prove the old block is really still valid: write to it.
    unsafe { core::ptr::write_bytes(p32, 0, 32) };

    // Free: exactly back to the starting baseline.
    unsafe { ca.dealloc(p32, layout32) };
    assert_eq!(live(), baseline, "dealloc did not return live to baseline");

    // The grow step must have raised the process peak at least once.
    assert!(
        PEAK_LIVE.load(Ordering::Relaxed) >= baseline + 128,
        "PEAK_LIVE never observed the grown live value"
    );
    println!("counter self-check: grow/shrink/refused-alloc/refused-realloc/free OK — live returned to baseline (P3-1 fix + P2-1 controlled-failure fixture verified)");
}

// --- P4-3: dependency versions from the nearest Cargo.lock — a RUNTIME
// snapshot, NOT build identity ---
//
// The example's cwd depends on the invoker (crate dir, workspace root, ...),
// so candidate lockfile paths are tried outward. Whatever this finds is a
// property of the RUNTIME environment, not of this binary: a compiled
// example can be launched from a different project, or from this project
// after its lockfile changed, and the versions it was actually built
// against are not re-checked at run time. The printed line therefore says
// "runtime lockfile snapshot, NOT build identity". Binding the resolved
// graph to the artifact AT BUILD TIME would require a build script emitting
// the versions into `env!`-read constants; this example deliberately has no
// build script, so the snapshot label is the honest one.
//
// Merge rule (review P4-3 fix): the per-file scan state is reset for EVERY
// candidate and a file is accepted only if it supplies BOTH wanted
// packages — a proptest version found in one lockfile can never be paired
// with an arbitrary version from another under one file's name.
// Minimal text scan of `[[package]]` / `name = "X"` / `version = "Y"`
// records; graceful fallback when no lockfile is reachable.
fn resolved_front_ends_line() -> String {
    const WANTED: [&str; 2] = ["proptest", "arbitrary"];
    for (depth, path) in [
        "Cargo.lock",
        "../Cargo.lock",
        "../../Cargo.lock",
        "../../../Cargo.lock",
    ]
    .iter()
    .enumerate()
    {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        // Fresh per-file state: never merged across candidate files.
        let mut found: [(Option<String>, Option<String>); 2] =
            core::array::from_fn(|_| (None, None));
        let mut in_package = false;
        let mut name: Option<String> = None;
        let mut version: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if line == "[[package]]" {
                if let (Some(n), Some(v)) = (name.as_deref(), version.as_deref()) {
                    if let Some(slot) = WANTED.iter().position(|w| *w == n) {
                        found[slot] = (Some(n.to_string()), Some(v.to_string()));
                    }
                }
                in_package = true;
                name = None;
                version = None;
                continue;
            }
            if !in_package {
                continue;
            }
            if let Some(rest) = line.strip_prefix("name = \"") {
                name = rest.strip_suffix('"').map(str::to_string);
            } else if let Some(rest) = line.strip_prefix("version = \"") {
                version = rest.strip_suffix('"').map(str::to_string);
            }
        }
        if let (Some(n), Some(v)) = (name.as_deref(), version.as_deref()) {
            if let Some(slot) = WANTED.iter().position(|w| *w == n) {
                found[slot] = (Some(n.to_string()), Some(v.to_string()));
            }
        }
        // Accept ONLY a file that supplies BOTH wanted packages.
        if found.iter().all(|(_, v)| v.is_some()) {
            let source = match depth {
                0 => "Cargo.lock (cwd)",
                1 => "../Cargo.lock",
                2 => "../../Cargo.lock",
                _ => "../../../Cargo.lock",
            };
            // globalalloc-model's own version IS build identity
            // (compile-time `env!`); the two dependency versions are the
            // runtime snapshot the label names.
            return format!(
                "runtime Cargo.lock snapshot, NOT build identity (read from {source}, relative to this process's cwd): proptest {}, arbitrary {}, globalalloc-model {} (the crate version IS this binary's build identity)",
                found[0].1.as_deref().unwrap_or("?"),
                found[1].1.as_deref().unwrap_or("?"),
                env!("CARGO_PKG_VERSION"),
            );
        }
    }
    "runtime Cargo.lock snapshot, NOT build identity: no reachable Cargo.lock supplies both proptest and arbitrary (record versions from the invoking environment)".to_string()
}

fn pattern_byte(fill: u8, offset: usize) -> u8 {
    if offset == 0 {
        return fill;
    }
    let mut x = (fill as u32) ^ (offset as u32).wrapping_mul(0x9E37_79B1);
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x as u8
}

fn pattern_byte_old_additive(fill: u8, offset: usize) -> u8 {
    fill.wrapping_add(offset as u8)
}

fn main() {
    count_alloc_self_check();

    // Honest build/target markers (P4-3). debug_assertions is the ONLY build
    // claim made here; the canonical command is printed verbatim as a
    // command string, not as a claim about this binary's profile.
    let debug_assertions = cfg!(debug_assertions);
    let build_note = if debug_assertions {
        "asserting build — timings not comparable to an optimized run"
    } else {
        "(assertions compiled out)"
    };
    println!(
        "probe identity: globalalloc-model v{}, {}-bit ptr (size_of::<usize>()), arch {}, os {}, debug_assertions={} {}",
        env!("CARGO_PKG_VERSION"),
        core::mem::size_of::<usize>() * 8,
        std::env::consts::ARCH,
        std::env::consts::OS,
        debug_assertions,
        build_note,
    );
    println!(
        "canonical command: cargo run --release -p globalalloc-model --example perf_probe_p4_measurements --features proptest,internals"
    );
    println!("{}", resolved_front_ends_line());

    println!("\n=== P4-3: pattern_byte marginal cost (isolated, not through drive) ===");
    {
        const N: u64 = 200_000_000;
        let start = Instant::now();
        let mut acc: u64 = 0;
        for i in 0..N {
            let fill = (i % 255 + 1) as u8;
            let offset = (i % 65536) as usize;
            acc = acc.wrapping_add(std::hint::black_box(pattern_byte(fill, offset)) as u64);
        }
        let elapsed = start.elapsed();
        println!(
            "  new (hash-mixed, offset-0 raw): {N} calls in {elapsed:?} ({:.3} ns/call) [acc={acc}]",
            elapsed.as_secs_f64() * 1e9 / N as f64
        );

        let start = Instant::now();
        let mut acc: u64 = 0;
        for i in 0..N {
            let fill = (i % 255 + 1) as u8;
            let offset = (i % 65536) as usize;
            acc = acc
                .wrapping_add(std::hint::black_box(pattern_byte_old_additive(fill, offset)) as u64);
        }
        let elapsed = start.elapsed();
        println!(
            "  old (additive):                 {N} calls in {elapsed:?} ({:.3} ns/call) [acc={acc}]",
            elapsed.as_secs_f64() * 1e9 / N as f64
        );
    }

    println!("\n=== P4-3/P4-4: end-to-end drive() cost by regime ===");
    {
        // Regime A: byte-oracle-heavy — few large blocks, minimal live count.
        let large_ops: Vec<Op> = (0..50)
            .map(|_| Op::Alloc {
                size: 128 * 1024,
                align: 8,
            })
            .collect();
        let start = Instant::now();
        drive(&System, Config::default(), &large_ops);
        let elapsed = start.elapsed();
        let total_bytes: usize = 50 * 128 * 1024;
        println!(
            "  Regime A (50 x 128 KiB blocks, K=50 peak live, ~{} MiB touched): {elapsed:?} ({:.3} ns/byte-op-equivalent)",
            total_bytes / 1024 / 1024,
            elapsed.as_secs_f64() * 1e9 / total_bytes as f64
        );

        // Regime B: overlap-scan-heavy — many tiny blocks, high live count.
        // K=2048 is the `arbitrary` front-end's per-`OpStream`
        // decode-attempt ceiling (`MAX_OPS` in src/arbitrary_stream.rs), NOT
        // a limit on the public driver or on `op_strategy` (whose
        // `len_range` is caller-supplied): a hand-built or long generated
        // stream can reach a far larger peak live count, so this is one
        // measured regime, not the crate's most adversarial reachable case.
        let small_ops: Vec<Op> = (0..2048).map(|_| Op::Alloc { size: 8, align: 8 }).collect();
        let start = Instant::now();
        drive(&System, Config::default(), &small_ops);
        let elapsed = start.elapsed();
        println!(
            "  Regime B (2048 x 8 B blocks, K=2048 peak live, O(K^2/2)~2.1M overlap compares): {elapsed:?} ({:.1} ns/op)",
            elapsed.as_secs_f64() * 1e9 / 2048.0
        );

        // Regime B at the native integration tests' own stream length
        // (tests/system_proptest.rs MAX_LEN, default 200) — a test-suite
        // constant, not a Config or driver limit.
        let small_ops_200: Vec<Op> = (0..200).map(|_| Op::Alloc { size: 8, align: 8 }).collect();
        let start = Instant::now();
        drive(&System, Config::default(), &small_ops_200);
        let elapsed = start.elapsed();
        println!(
            "  Regime B at typical K=200: {elapsed:?} ({:.1} ns/op)",
            elapsed.as_secs_f64() * 1e9 / 200.0
        );
    }

    #[cfg(feature = "proptest")]
    {
        #[cfg(feature = "internals")]
        {
            let sizes = globalalloc_model::size_strategy_repr_sizes();
            println!(
                "\n=== P4-5: static sizes (size_of, compile-time) ===\n  SizeStrategy: {} B; SizeValueTree (inline, replaces a 2-word Box<dyn ValueTree> = {} B slot): {} B; Single tree: {} B; WeightedSizeTree: {} B; per-op element tree in the stream VecValueTree: {} B\n  (all size_of values are specific to this target/toolchain/feature set, NOT portable contracts of the types)",
                sizes.size_strategy,
                2 * core::mem::size_of::<usize>(),
                sizes.size_value_tree,
                sizes.single_tree,
                sizes.weighted_tree,
                sizes.op_element_tree
            );
        }
        #[cfg(not(feature = "internals"))]
        {
            println!(
                "\n=== P4-5: static sizes need --features internals (the heap-byte proxies below do not) ==="
            );
        }
        p4_paired_ab::run();
    }
}

#[cfg(feature = "proptest")]
mod p4_paired_ab {
    use super::{ALLOC_CALLS, LIVE_BYTES, PEAK_LIVE, REALLOC_CALLS, TOTAL_BYTES};
    use globalalloc_model::{op_strategy, Config, Op};
    use proptest::prelude::ProptestConfig;
    use proptest::strategy::{BoxedStrategy, Strategy, ValueTree};
    use proptest::test_runner::{
        FileFailurePersistence, RngSeed, TestCaseError, TestError, TestRunner,
    };
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    // Test/example-only: a FAITHFUL boxed counterpart of the crate's private
    // `SizeStrategy` — identical ranges, weights, arm order, prop_map closures
    // and collection::vec shape, differing ONLY in the final `.boxed()`. The
    // old P4-5 baseline used a different generator shape entirely (plain
    // 1..=8 ranges), so its deltas were confounded; this one is not.
    // Deliberately NOT added to src/strategy.rs — production stays unboxed.
    // Duplicated verbatim in tests/size_strategy_avoids_boxed_value_trees.rs.
    fn size_strategy_boxed(config: Config) -> BoxedStrategy<usize> {
        let small = 1usize..=config.small_max.max(1);
        let large = config.small_max.saturating_add(1)
            ..=config.large_max.max(config.small_max.saturating_add(1));
        match (config.small_weight, config.large_weight) {
            (0, _) => large.boxed(),
            (_, 0) => small.boxed(),
            (small_weight, large_weight) => proptest::strategy::TupleUnion::new((
                (small_weight, std::sync::Arc::new(small)),
                (large_weight, std::sync::Arc::new(large)),
            ))
            .boxed(),
        }
    }

    fn op_strategy_boxed(
        config: Config,
        len_range: core::ops::Range<usize>,
    ) -> BoxedStrategy<Vec<Op>> {
        config.validate();
        let alloc = (size_strategy_boxed(config), align_strategy(config))
            .prop_map(|(size, align)| Op::Alloc { size, align });
        let alloc_zeroed = (size_strategy_boxed(config), align_strategy(config))
            .prop_map(|(size, align)| Op::AllocZeroed { size, align });
        let dealloc = proptest::prelude::any::<usize>().prop_map(Op::Dealloc);
        let realloc = (
            proptest::prelude::any::<usize>(),
            size_strategy_boxed(config),
        )
            .prop_map(|(i, new_size)| Op::Realloc { i, new_size });

        let op = proptest::prelude::prop_oneof![alloc, alloc_zeroed, dealloc, realloc];
        proptest::collection::vec(op, len_range).boxed()
    }

    fn align_strategy(config: Config) -> impl Strategy<Value = usize> {
        let max_exp = config.max_align.trailing_zeros();
        (0..=max_exp).prop_map(move |exponent| 1usize << exponent)
    }

    fn runner(
        seed: u64,
        persistence: Option<Box<dyn proptest::test_runner::FailurePersistence>>,
    ) -> TestRunner {
        TestRunner::new(ProptestConfig {
            rng_seed: RngSeed::Fixed(seed),
            failure_persistence: persistence,
            ..ProptestConfig::default()
        })
    }

    fn draw<S: Strategy<Value = Vec<Op>>>(strat: &S, seed: u64) -> Vec<Op> {
        strat
            .new_tree(&mut runner(seed, None))
            .expect("generate a tree")
            .current()
    }

    /// Mean per-draw ATTEMPTED-call counts of one `new_tree()` + `current()`
    /// of a `len`-op stream, over `seeds` fixed seeds: (alloc-only,
    /// realloc-only).
    ///
    /// P4-2: the marginal allocs/op derived from these is reported under
    /// all THREE numerators — alloc-only, realloc-only, and their SUM —
    /// because the regression test's counting wrapper does not override
    /// `GlobalAlloc::realloc`: the trait default (checked in this
    /// toolchain's `core/src/alloc/global.rs`) routes the new block through
    /// the wrapper's own `alloc` (one `ALLOC_CALLS` per realloc event; the
    /// paired dealloc is uncounted), so the test's single counter equals
    /// the SUM, not the alloc-only figure.
    fn mean_call_counts<S: Strategy<Value = Vec<Op>>>(
        build: impl Fn(core::ops::Range<usize>) -> S,
        len: usize,
        seeds: u64,
    ) -> (f64, f64) {
        let strat = build(len..len + 1);
        let mut total_allocs = 0usize;
        let mut total_reallocs = 0usize;
        for seed in 0..seeds {
            let before_allocs = ALLOC_CALLS.load(Ordering::Relaxed);
            let before_reallocs = REALLOC_CALLS.load(Ordering::Relaxed);
            let tree = strat.new_tree(&mut runner(seed, None)).unwrap();
            std::hint::black_box(tree.current());
            total_allocs += ALLOC_CALLS.load(Ordering::Relaxed) - before_allocs;
            total_reallocs += REALLOC_CALLS.load(Ordering::Relaxed) - before_reallocs;
        }
        (
            total_allocs as f64 / seeds as f64,
            total_reallocs as f64 / seeds as f64,
        )
    }

    /// P4-2: one arm's marginal allocs/op under all three numerators. `sum`
    /// is the canonical figure — it matches the regression test's counting;
    /// `alloc_only` isolates the definitional delta.
    #[derive(Clone, Copy)]
    struct Marginals {
        alloc_only: f64,
        realloc_only: f64,
        sum: f64,
    }

    // --- P3-2: three separately labeled scenarios replace the old single,
    // mislabeled "draw" (which was construction + a mandatory full
    // simplify-only walk). See the file-header doc comment for why the
    // distinction matters (TupleUnion initializes previously-unselected
    // variants while simplifying, so the boxed arm pays extra size-Box
    // allocations in the simplify regimes that a successful case never
    // creates).
    #[derive(Clone, Copy)]
    enum Scenario {
        SuccessfulDraw,
        SimplifyOnly,
        ShrinkProtocol,
    }

    impl Scenario {
        fn label(self) -> &'static str {
            match self {
                Scenario::SuccessfulDraw => {
                    "S1 successful-draw (new_tree -> current -> drop; NO shrinking)"
                }
                Scenario::SimplifyOnly => {
                    "S2 simplify-only walk (while simplify(); no per-step current(), no complicate)"
                }
                Scenario::ShrinkProtocol => {
                    "S3 shrink-protocol (REAL TestRunner::run_one accept/reject/complicate walk; CUSTOM iteration budget — not the default runner)"
                }
            }
        }
    }

    struct ScenarioMetrics {
        total_b: usize,
        total_b_post_construct: usize,
        peak_post_construct: usize,
        peak_traj: usize,
        new_ns: f64,
        // P4-4: S1's `current()` materialization, timed as its own column
        // (it used to fall outside every timing column). 0.0 for S2/S3.
        current_ns: f64,
        // S1/S2: the walk (empty for S1). S3: the whole
        // `TestRunner::run_one` shrink call — the per-evaluation
        // current()+predicate work, every transition, the runner's final
        // minimal-value materialization AND the tree drop happen inside it.
        walk_ns: f64,
        drop_ns: f64,
        // new_ns + current_ns + walk_ns + drop_ns: the whole-case total the
        // disjoint columns above sum to (P4-4).
        total_ns: f64,
        // S2: simplify steps. S3: predicate evaluations (the runner's
        // shrink iterations + the initial evaluation).
        steps: usize,
        accepts: usize,
        // S3: PASS evaluations; the real runner makes exactly one
        // complicate attempt after each (runner.rs:870-879) and attempt
        // success is not externally observable, hence "attempts".
        complicate_attempts: usize,
        // S3: whether the walk hit the CUSTOM iteration budget instead of
        // ending naturally (the runner then unwinds to the last failure).
        cap_truncated: bool,
        reallocs: usize,
    }

    // S3's shrink budget: a CUSTOM choice for this probe, NOT proptest's
    // default. In resolved proptest 1.11.0 the raw default is a u32::MAX
    // sentinel (config.rs:177) that `Config::max_shrink_iters()` resolves
    // to `cases * 4` — 256 * 4 = 1024 at default cases (config.rs:573-579);
    // the previous version of this probe mislabeled its 16,384 as that
    // default. At 1024 evaluations a 200-op stream's walk is cut off long
    // before its natural end (~15.5k evaluations), which would leave the
    // complicate backoff unexercised, so S3 pins this explicitly CUSTOM,
    // larger budget; every output line and published claim labels it as
    // custom, and the probe prints the default's RESOLVED value alongside
    // for comparison.
    const S3_MAX_SHRINK_ITERS: u32 = 65_536;

    /// S3's runner: identical to `runner()` except for the explicitly
    /// CUSTOM `max_shrink_iters` (see `S3_MAX_SHRINK_ITERS`). Same seed =>
    /// same drawn tree: `new_tree` does not read `max_shrink_iters`.
    fn runner_s3(seed: u64) -> TestRunner {
        TestRunner::new(ProptestConfig {
            rng_seed: RngSeed::Fixed(seed),
            failure_persistence: None,
            max_shrink_iters: S3_MAX_SHRINK_ITERS,
            ..ProptestConfig::default()
        })
    }

    /// The S3 "property under test": a stream FAILS the property (candidate
    /// accepted) while any op still carries a parameter above its minimum
    /// (size/new_size > 1, align > 1); only the fully-minimal stream passes.
    /// Predicate over ELEMENT VALUES because the strategies under test are
    /// built with `collection::vec(op, STREAM..STREAM + 1)`: proptest's
    /// VecValueTree pins the minimum length at the range start, so candidate
    /// length is constant (200) and only element values ever move — the
    /// earlier length-based predicates (non-empty, `> KEEP_OPS`) could never
    /// cross their boundary and left `complicates` at 0, a mechanism-dead
    /// scenario. As the element walk exhausts toward full minimality, the
    /// predicate flips and the real runner's `complicate` backoff
    /// genuinely fires; the printed accepts / complicate-attempts counts
    /// are the mechanism-activation evidence. `Op::Dealloc`'s index is
    /// ignored (shrinking it to 0 does not make a stream "pass").
    fn property_fails(ops: &[Op]) -> bool {
        ops.iter().any(|op| match op {
            Op::Alloc { size, align } | Op::AllocZeroed { size, align } => *size > 1 || *align > 1,
            Op::Realloc { new_size, .. } => *new_size > 1,
            Op::Dealloc(_) => false,
        })
    }

    fn scenario_metrics<S: Strategy<Value = Vec<Op>>>(
        strat: &S,
        seed: u64,
        scenario: Scenario,
    ) -> ScenarioMetrics {
        // S3 runs under the CUSTOM-limit runner (same seed => same drawn
        // tree); S1/S2 keep the default-config runner, unchanged.
        let mut runner = if matches!(scenario, Scenario::ShrinkProtocol) {
            runner_s3(seed)
        } else {
            runner(seed, None)
        };
        let before_total = TOTAL_BYTES.load(Ordering::Relaxed);
        let start_live = LIVE_BYTES.load(Ordering::Relaxed);
        // Re-base the window-local peak so constant pre-window offsets
        // cancel.
        PEAK_LIVE.store(start_live, Ordering::Relaxed);
        let before_reallocs = REALLOC_CALLS.load(Ordering::Relaxed);

        let t = Instant::now();
        let mut tree = strat.new_tree(&mut runner).expect("generate a tree");
        let new_ns = t.elapsed().as_nanos() as f64;

        // Post-construction boundary. S1's "construction" is new_tree +
        // current(): the successful case materializes its value. P4-4: the
        // materialization is timed as its OWN column — it used to sit
        // between the new_tree and walk timers and never landed in any
        // timing column, so the columns could not be summed into the cost
        // of a full successful case.
        let current_t = Instant::now();
        if matches!(scenario, Scenario::SuccessfulDraw) {
            std::hint::black_box(tree.current());
        }
        let current_ns = if matches!(scenario, Scenario::SuccessfulDraw) {
            current_t.elapsed().as_nanos() as f64
        } else {
            0.0
        };
        let total_b_post_construct = TOTAL_BYTES.load(Ordering::Relaxed) - before_total;
        let peak_post_construct = PEAK_LIVE.load(Ordering::Relaxed) - start_live;

        let mut steps = 0usize;
        let mut accepts = 0usize;
        let mut complicate_attempts = 0usize;
        let mut cap_truncated = false;
        let walk_t = Instant::now();
        let walk_ns;
        let drop_ns;
        match scenario {
            Scenario::SuccessfulDraw => {
                // Body intentionally empty: no shrinking. The walk column is
                // EXACTLY 0 — there is no walk to time, so none is measured
                // (P4-4: the printed columns must sum to the printed total
                // with no hidden remainder, not even timer overhead).
                walk_ns = 0.0;
                let t = Instant::now();
                drop(tree);
                drop_ns = t.elapsed().as_nanos() as f64;
            }
            Scenario::SimplifyOnly => {
                // Isolates TupleUnion's lazy-branch materialization
                // mechanics; the old (mislabeled "draw") microbenchmark.
                while tree.simplify() {
                    steps += 1;
                }
                walk_ns = walk_t.elapsed().as_nanos() as f64;
                let t = Instant::now();
                drop(tree);
                drop_ns = t.elapsed().as_nanos() as f64;
            }
            Scenario::ShrinkProtocol => {
                // P3-1 fix: the REAL shrink protocol. `TestRunner::run_one`
                // (proptest 1.11.0, src/test_runner/runner.rs:700 ->
                // run_one_with_replay:720 -> shrink:761) drives the
                // candidate: the initial evaluation, the accept/reject
                // transitions, the complicate backoff, the iteration cap
                // and the value lifetimes are proptest's own. The previous
                // hand-rolled loop diverged from the real runner in exactly
                // the three ways the review named: it began every iteration
                // with an unconditional simplify (the real runner
                // evaluates case.current() at the TOP of every loop
                // iteration — runner.rs:856-864 — so after a PASS and a
                // successful complicate the RESTORED candidate is
                // re-evaluated before the next transition), it did not end
                // the search when complicate returned false
                // (runner.rs:870-879: `if !case.complicate() { break }`),
                // and it retained the last accepted Vec<Op> in `result`
                // across the next candidate's construction — the real
                // runner holds only the tree and the failure Reason
                // (runner.rs:782 `last_failure`); every candidate value is
                // handed to the predicate BY VALUE (runner.rs:856-864 via
                // call_test:202 `test(case)`).
                //
                // CUSTOM limit: this runner's max_shrink_iters is
                // S3_MAX_SHRINK_ITERS, not the default runner's (the
                // default resolves cases * 4 = 1024; see the constant).
                //
                // Counters observable from OUTSIDE the runner: every
                // predicate invocation = one evaluation = one Vec<Op>
                // materialized from case.current(); a FAIL evaluation is
                // an accepted candidate; every PASS evaluation is followed
                // by exactly one runner-internal complicate attempt
                // (runner.rs:870-879), whose success is not externally
                // observable — hence "attempts".
                let evals = core::cell::Cell::new(0usize);
                let fails = core::cell::Cell::new(0usize);
                let passes = core::cell::Cell::new(0usize);
                let test = |ops: Vec<Op>| {
                    evals.set(evals.get() + 1);
                    std::hint::black_box(&ops);
                    if property_fails(&ops) {
                        fails.set(fails.get() + 1);
                        Err(TestCaseError::fail(
                            "S3 probe: a non-minimal parameter remains",
                        ))
                    } else {
                        passes.set(passes.get() + 1);
                        Ok(())
                    }
                };
                // Mechanism evidence: the initial candidate must reproduce
                // the failure (else no shrink ran at all), and the runner
                // must return the shrunk minimal value —
                // Err(TestError::Fail) carries it, materialized by the
                // runner's own final case.current() (runner.rs:752).
                match runner.run_one(tree, test) {
                    Err(TestError::Fail(_, minimal)) => {
                        std::hint::black_box(&minimal);
                    }
                    Err(TestError::Abort(reason)) => {
                        panic!("S3: run_one aborted unexpectedly: {reason}");
                    }
                    Ok(_) => {
                        panic!(
                            "S3: the initial candidate passed the property; the shrink protocol was not exercised"
                        );
                    }
                }
                walk_ns = walk_t.elapsed().as_nanos() as f64;
                // The tree was consumed by run_one and is dropped INSIDE
                // it (as is the final-value materialization), so there is
                // no separate drop column for S3.
                drop_ns = 0.0;
                steps = evals.get();
                accepts = fails.get();
                complicate_attempts = passes.get();
                // The runner's cap check fires when its shrink-loop
                // iterations reach the limit (runner.rs:802); with the
                // initial evaluation counted too, a cap-truncated walk
                // ends at exactly S3_MAX_SHRINK_ITERS + 1 predicate calls.
                cap_truncated = steps > S3_MAX_SHRINK_ITERS as usize;
            }
        }

        ScenarioMetrics {
            total_b: TOTAL_BYTES.load(Ordering::Relaxed) - before_total,
            total_b_post_construct,
            peak_post_construct,
            peak_traj: PEAK_LIVE.load(Ordering::Relaxed) - start_live,
            new_ns,
            current_ns,
            walk_ns,
            drop_ns,
            total_ns: new_ns + current_ns + walk_ns + drop_ns,
            steps,
            accepts,
            complicate_attempts,
            cap_truncated,
            reallocs: REALLOC_CALLS.load(Ordering::Relaxed) - before_reallocs,
        }
    }

    /// Full-simplify walk; returns (step count, final shrunk value).
    fn shrink_steps<S: Strategy<Value = Vec<Op>>>(strat: &S, seed: u64) -> (usize, Vec<Op>) {
        let mut tree = strat
            .new_tree(&mut runner(seed, None))
            .expect("generate a tree");
        let mut steps = 0;
        while tree.simplify() {
            steps += 1;
        }
        (steps, tree.current())
    }

    /// Per-scenario (S1, S2, S3) (mean total B, mean trajectory peak B).
    type ScenarioMeans = [(f64, f64); 3];

    /// Per-scenario reporter. Returns, per scenario (S1, S2, S3 order),
    /// the (mean total B, mean trajectory peak B) means used for the
    /// boxing-attributable delta lines.
    fn measure_arm<S: Strategy<Value = Vec<Op>>>(
        name: &str,
        strat: &S,
        seeds_s12: u64,
        seeds_s3: u64,
    ) -> ScenarioMeans {
        let scenarios = [
            (Scenario::SuccessfulDraw, seeds_s12),
            (Scenario::SimplifyOnly, seeds_s12),
            (Scenario::ShrinkProtocol, seeds_s3),
        ];
        let mut means = [(0f64, 0f64); 3];
        for (index, (scenario, seeds)) in scenarios.into_iter().enumerate() {
            let mut total = 0u64;
            let mut total_post = 0u64;
            let mut peak_post = 0u64;
            let mut peak_traj = 0u64;
            let mut peak_traj_max = 0usize;
            let mut new_ns = 0f64;
            let mut current_ns = 0f64;
            let mut walk_ns = 0f64;
            let mut drop_ns = 0f64;
            let mut total_ns = 0f64;
            let mut steps_sum = 0u64;
            let mut accepts_sum = 0u64;
            let mut attempts_sum = 0u64;
            let mut cap_truncated_count = 0usize;
            let mut reallocs_sum = 0u64;
            for seed in 0..seeds {
                let m = scenario_metrics(strat, seed, scenario);
                total += m.total_b as u64;
                total_post += m.total_b_post_construct as u64;
                peak_post += m.peak_post_construct as u64;
                peak_traj += m.peak_traj as u64;
                peak_traj_max = peak_traj_max.max(m.peak_traj);
                new_ns += m.new_ns;
                current_ns += m.current_ns;
                walk_ns += m.walk_ns;
                drop_ns += m.drop_ns;
                total_ns += m.total_ns;
                steps_sum += m.steps as u64;
                accepts_sum += m.accepts as u64;
                attempts_sum += m.complicate_attempts as u64;
                cap_truncated_count += usize::from(m.cap_truncated);
                reallocs_sum += m.reallocs as u64;
            }
            let n = seeds as f64;
            let reallocs = reallocs_sum as f64 / n;
            // P4-4: per-scenario timing line. The listed columns are
            // DISJOINT (no work is timed twice) and sum to the whole-case
            // total; S1's current() materialization now has its own column
            // instead of falling between the new_tree and walk timers.
            let timing_line = match scenario {
                Scenario::SuccessfulDraw => format!(
                    "ns/case: new_tree {:.0} + current {:.0} + drop {:.0} = {:.0} (disjoint columns; they sum to the whole successful case)",
                    new_ns / n,
                    current_ns / n,
                    drop_ns / n,
                    total_ns / n
                ),
                Scenario::SimplifyOnly => format!(
                    "ns/draw: new_tree {:.0} + walk {:.0} + drop {:.0} = {:.0} (disjoint columns; they sum to the whole case)",
                    new_ns / n,
                    walk_ns / n,
                    drop_ns / n,
                    total_ns / n
                ),
                Scenario::ShrinkProtocol => format!(
                    "ns/draw: new_tree {:.0} + shrink run_one {:.0} = {:.0} (disjoint columns; per-evaluation current()+predicate work, every transition, the final minimal-value materialization and the tree drop all happen INSIDE run_one — there is no separate drop column)",
                    new_ns / n,
                    walk_ns / n,
                    total_ns / n
                ),
            };
            let tail = if matches!(scenario, Scenario::ShrinkProtocol) {
                format!(
                    ", evals avg {:.1} (predicate calls; each materializes one candidate Vec<Op>), accepts avg {:.1}, complicate attempts avg {:.1}, cap-truncated seeds {cap_truncated_count}/{seeds}",
                    steps_sum as f64 / n,
                    accepts_sum as f64 / n,
                    attempts_sum as f64 / n,
                )
            } else {
                format!(", steps avg {:.1}", steps_sum as f64 / n)
            };
            println!(
                "  {name:14} {}: n={seeds} draws | total {:.0} B (post-construct {:.0} B) | peak post-construct {:.0} B, trajectory {:.0} B (max {peak_traj_max} B) | reallocs in window {reallocs:.1} | {timing_line}{tail}",
                scenario.label(),
                total as f64 / n,
                total_post as f64 / n,
                peak_post as f64 / n,
                peak_traj as f64 / n,
            );
            means[index] = (total as f64 / n, peak_traj as f64 / n);
        }
        means
    }

    pub fn run() {
        const SEEDS: u64 = 64;
        // Per-step current() materialization makes S3 the expensive
        // scenario, so it runs on fewer seeds.
        const SHRINK_SEEDS: u64 = 8;
        const STREAM: usize = 200;

        println!("\n=== P4-5: paired A/B, enum SizeStrategy vs faithful boxed counterpart ===");
        println!("  (S1/S2: {SEEDS} seeds per arm; S3: {SHRINK_SEEDS} seeds per arm — the real TestRunner::run_one shrink under a CUSTOM {S3_MAX_SHRINK_ITERS}-iteration budget; per-evaluation current() materialization makes it the expensive scenario; one {STREAM}-op stream draw per seed; paired identity asserted per seed)");
        // S3 config evidence (requested vs resolved, not just the label):
        // the CUSTOM limit this probe pins, and what the DEFAULT runner
        // would resolve in this very environment (both env-overridable via
        // PROPTEST_* variables — the default line reads the AMBIENT
        // default, it is not a hardcoded 1024).
        let s3_config = ProptestConfig {
            rng_seed: RngSeed::Fixed(0),
            failure_persistence: None,
            max_shrink_iters: S3_MAX_SHRINK_ITERS,
            ..ProptestConfig::default()
        };
        let default_config = ProptestConfig::default();
        println!(
            "  S3 config evidence: CUSTOM max_shrink_iters = {} (resolved read-back via Config::max_shrink_iters()); the DEFAULT runner would resolve {} from cases = {} (u32::MAX sentinel -> cases * 4)",
            s3_config.max_shrink_iters(),
            default_config.max_shrink_iters(),
            default_config.cases,
        );

        // Counter accounting legend (P3-1). Attempted vs successful calls;
        // realloc replace semantics; full-new-request TOTAL_BYTES
        // convention; window-local re-based peaks; P3-1 exposure via the
        // per-window REALLOC_CALLS delta.
        println!("legend: ATTEMPTED calls (ALLOC_CALLS, REALLOC_CALLS) are counted separately from byte accounting; LIVE_BYTES/TOTAL_BYTES/PEAK_LIVE update ONLY on successful backend calls (a null alloc or realloc leaves live unchanged; a successful realloc REPLACES the old size with the new one in live state). TOTAL_BYTES counts the cumulative FULL requested size of every successful call — a successful realloc counts the full new request, not the growth delta. peak = window-local max of LIVE_BYTES, re-based at window entry (start_live subtracted), so constant pre-window offsets cancel. Marginal allocs/op figures are reported under THREE numerators — attempted ALLOC_CALLS only, attempted REALLOC_CALLS only, and their SUM; the SUM is the one matching the regression test's counting wrapper (it does not override realloc, so the default realloc's new-block alloc lands in its single counter — review P4-2).");
        println!("P3-1 exposure: each window reports its REALLOC_CALLS delta. A window with 0 reallocs cannot have been affected by the pre-fix bug (it only fired on realloc) beyond a constant pre-window offset, which the re-basing cancels; a window with reallocs > 0 WAS affected (stored live previously grew by new_size instead of new_size - old_size per realloc, and null reallocs over-counted).");

        let arms: [(&str, Config, bool); 4] = [
            ("enum/default", Config::default(), false),
            ("boxed/default", Config::default(), true),
            (
                "enum/single",
                Config {
                    large_weight: 0,
                    ..Config::default()
                },
                false,
            ),
            (
                "boxed/single",
                Config {
                    large_weight: 0,
                    ..Config::default()
                },
                true,
            ),
        ];

        // Gate: per-seed REAL pairwise comparison — same seed + same Config
        // must yield byte-identical Vec<Op> streams from the enum strategy
        // and its boxed counterpart. Two real pairs per seed (default and
        // single configs); the 4 measurement arms reuse these 2 configs.
        for seed in 0..SEEDS {
            for (name, config) in [
                ("default", Config::default()),
                (
                    "single",
                    Config {
                        large_weight: 0,
                        ..Config::default()
                    },
                ),
            ] {
                let expect = draw(&op_strategy(config, STREAM..STREAM + 1), seed);
                let got = draw(&op_strategy_boxed(config, STREAM..STREAM + 1), seed);
                assert_eq!(
                    expect, got,
                    "paired identity broke ({name} config), seed {seed}"
                );
            }
        }
        println!("  paired identity (Vec<Op> equality, all {SEEDS} seeds x 2 configs): OK");

        // Full-shrink equality on a small seed subset. Correctness gate,
        // not a measurement; uses the S2-style walk.
        for seed in 0..SHRINK_SEEDS {
            let (enum_steps, enum_final) =
                shrink_steps(&op_strategy(Config::default(), STREAM..STREAM + 1), seed);
            let (boxed_steps, boxed_final) = shrink_steps(
                &op_strategy_boxed(Config::default(), STREAM..STREAM + 1),
                seed,
            );
            assert_eq!(enum_steps, boxed_steps, "shrink step count, seed {seed}");
            assert_eq!(enum_final, boxed_final, "shrunk final value, seed {seed}");
        }
        println!(
            "  full-shrink equality (steps + final value, {SHRINK_SEEDS} seeds, default config): OK"
        );

        // stats: (arm name, marginal allocs/op under all three numerators
        // (P4-2; the delta lines below cite the SUM numerator), per-scenario
        // (mean total B, mean trajectory peak B) in S1, S2, S3 order).
        let mut stats: Vec<(&str, Marginals, ScenarioMeans)> = Vec::new();
        for (name, config, boxed) in arms {
            let build = move |len: core::ops::Range<usize>| {
                if boxed {
                    op_strategy_boxed(config, len)
                } else {
                    op_strategy(config, len).boxed()
                }
            };
            let strat = build(STREAM..STREAM + 1);
            // Marginal allocs/op: (mean calls for len=220) - (mean for
            // len=20) divided by 200, reported under all three numerators
            // (P4-2) — attempted-call semantics throughout.
            let c20 = mean_call_counts(build, 20, SEEDS);
            let c220 = mean_call_counts(build, 220, SEEDS);
            let alloc_only = (c220.0 - c20.0) / 200.0;
            let realloc_only = (c220.1 - c20.1) / 200.0;
            let marginal = Marginals {
                alloc_only,
                realloc_only,
                sum: alloc_only + realloc_only,
            };
            println!(
                "  {name:14} marginal allocs/op: alloc-only {alloc_only:6.3} + realloc-only \
                 {realloc_only:6.3} = sum {sum:6.3} (three numerators: attempted ALLOC_CALLS / \
                 REALLOC_CALLS / their sum; the SUM matches the regression test's counting, \
                 whose wrapper does not override realloc)",
                sum = marginal.sum
            );
            let means = measure_arm(name, &strat, SEEDS, SHRINK_SEEDS);
            stats.push((name, marginal, means));
        }

        let scenario_names = ["S1", "S2", "S3"];
        for (boxed_arm, enum_arm) in [
            ("boxed/default", "enum/default"),
            ("boxed/single", "enum/single"),
        ] {
            let find = |key: &str| {
                let (_, marginal, means) = stats.iter().find(|(n, _, _)| *n == key).unwrap();
                (*marginal, *means)
            };
            let (b_marginal, b_means) = find(boxed_arm);
            let (e_marginal, e_means) = find(enum_arm);
            println!(
                "  boxing-attributable delta ({boxed_arm} - {enum_arm}): sum {:+.3} allocs/op \
                 (alloc-only {:+.3}, realloc-only {:+.3}) — the SUM numerator is the one \
                 matching the regression test's counting",
                b_marginal.sum - e_marginal.sum,
                b_marginal.alloc_only - e_marginal.alloc_only,
                b_marginal.realloc_only - e_marginal.realloc_only,
            );
            for (index, scenario_name) in scenario_names.iter().enumerate() {
                println!(
                    "    {scenario_name}: total B {:+.0}, trajectory peak B {:+.0}",
                    b_means[index].0 - e_means[index].0,
                    b_means[index].1 - e_means[index].1,
                );
            }
        }

        // Legacy uncontrolled calibration sensitivity row: NOT a decision
        // number. This replicates the env-unset default
        // (failure_persistence = Some(FileFailurePersistence::SourceParallel)).
        // The extra allocs are NOT file I/O: they come from the per-
        // `LazyValueTree`-init `partial_clone` of `TestRunner` cloning
        // `Config`, which clones the `Box<dyn FailurePersistence>` — one
        // extra heap alloc per non-selected union arm. File I/O only happens
        // when a failure is actually saved (never in this probe); the setting
        // exists to persist failures to the filesystem.
        let strat = op_strategy(Config::default(), STREAM..STREAM + 1);
        let mut total_allocs = 0usize;
        let mut total_reallocs = 0usize;
        for seed in 0..SEEDS {
            let before_allocs = ALLOC_CALLS.load(Ordering::Relaxed);
            let before_reallocs = REALLOC_CALLS.load(Ordering::Relaxed);
            let tree = strat
                .new_tree(&mut runner(
                    seed,
                    Some(Box::new(FileFailurePersistence::SourceParallel(
                        "proptest-regressions",
                    ))),
                ))
                .unwrap();
            std::hint::black_box(tree.current());
            total_allocs += ALLOC_CALLS.load(Ordering::Relaxed) - before_allocs;
            total_reallocs += REALLOC_CALLS.load(Ordering::Relaxed) - before_reallocs;
        }
        println!(
            "  [legacy sensitivity, NOT a decision number] failure_persistence = Some(SourceParallel) (env-unset default): {:.1} alloc + {:.1} realloc = {:.1} sum calls/draw over {SEEDS} seeds (attempted-call numerators) — extra calls from the TestRunner partial_clone cloning Box<dyn FailurePersistence> (setting exists to persist failures to the filesystem; no I/O occurs here)",
            total_allocs as f64 / SEEDS as f64,
            total_reallocs as f64 / SEEDS as f64,
            (total_allocs + total_reallocs) as f64 / SEEDS as f64,
        );
    }
}
