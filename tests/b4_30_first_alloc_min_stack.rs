//! B4-30 first-allocation stack budget test (fix8: differential self-measurement).
//!
//! The contract is a FIXED per-(OS, profile) byte budget declared in the
//! constants below. Every run of this test MEASURES on the CURRENT toolchain
//! (fresh child processes: an eater recursion, physical depth from addresses,
//! a dynamic per-frame stride measured in the child, free-coordinate
//! bracket/bisect, overflow classification by signature):
//!
//! - a CONTROL boundary: how much free stack (deep_addr - stack_low) an
//!   eat-only (no allocation) eater still has at its deepest usable stop;
//! - a SCENARIO boundary: the same for an eater whose body performs the
//!   first-alloc scenario (small / large / foreign-free);
//! - `need = fs - fc + stride` — the DIFFERENTIAL requirement of the
//!   scenario over the control, on the same thread geometry. The platform
//!   baseline (std's spawned-thread stack guarantee, guard page, bottom
//!   page) and the control's own frames are NOT part of `need`.
//!
//! Each boundary is the result of REPEATS fully independent bracket+bisect
//! searches (no seed carry-over); the worst (max free) is taken. The budget
//! assert runs only when `SEFER_B4_30_ENFORCE=1`; without it the test
//! measures and prints the markers WITHOUT enforcing (whole-suite CI rows
//! that are not part of the budget evidence must not fail on unmeasured
//! configurations). The evidence CSV the budgets were derived from is
//! `docs/perf/b4_30_probes/b4_30_fix7_budget_evidence.csv` (derived from the
//! committed raw run logs `docs/perf/_raw_b4_30_fix7_*.log` by
//! `scripts/b4_30_fix7_evidence.mjs`).
//!
//! Degenerate boundaries: if a SCENARIO bracket reaches the bottom of the
//! free coordinate (the scenario completes even on the deepest reachable
//! frame), the boundary is a valid `boundary=floor` result and `need` is the
//! resulting upper estimate. If the CONTROL bracket floors, the control
//! requirement is indistinguishable from zero in this coordinate, so the
//! differential is replaced by the sound absolute upper estimate
//! `need = fs + stride` (marker `control=floor`); this keeps the gate
//! conservative instead of asserting a differential the measurement cannot
//! support. NOTE (fix8 honesty): NEITHER floor branch was ever OBSERVED in
//! the fix7 evidence — all 24 rows are `boundary=bisect` and
//! `control=bisect` (linux release fc = 63 B, fs = 159 B); both branches are
//! model-argued, not demonstrated.
//!
//! Measured platforms: windows/linux. On any other target the file compiles
//! to nothing; on windows/linux with a non-x86_64 arch the tests print
//! `[b4-30] SKIP <reason>` and pass — a deliberate, marked skip rather than
//! an unmarked vacuous pass, though libtest CAPTURES the printed marker
//! (it is visible only with `--nocapture` or in failure output dumps, so the
//! passing CI log shows a plain `test ... ok`). A dedicated marker test
//! (`tests/b4_30_skip_marker.rs`) prints `[b4-30] SKIP macos` on macOS.
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    not(miri),
    any(windows, target_os = "linux")
))]
use sefer_alloc::{SeferAlloc, SegmentLayout};
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicBool;
use std::{
    alloc::{GlobalAlloc, Layout},
    hint::black_box,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
};
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();
static ENTRY: AtomicUsize = AtomicUsize::new(0);
static DEEP: AtomicUsize = AtomicUsize::new(0);
static PREV: AtomicUsize = AtomicUsize::new(0);
static STRIDE_ACT: AtomicUsize = AtomicUsize::new(0);
static LOW: AtomicUsize = AtomicUsize::new(0);
static HIGH: AtomicUsize = AtomicUsize::new(0);
static GUARD: AtomicUsize = AtomicUsize::new(0);
static RESULT: AtomicUsize = AtomicUsize::new(0);
static PTR: AtomicUsize = AtomicUsize::new(0);
static SIZE: AtomicUsize = AtomicUsize::new(0);
static CLAIM0: AtomicUsize = AtomicUsize::new(0);
static CLAIM1: AtomicUsize = AtomicUsize::new(0);
static CLAIM2: AtomicUsize = AtomicUsize::new(0);
static FB0: AtomicUsize = AtomicUsize::new(0);
static FB1: AtomicUsize = AtomicUsize::new(0);
#[cfg(feature = "bench-internals")]
static SRC0: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];
#[cfg(feature = "bench-internals")]
static SRC1: [AtomicUsize; 4] = [const { AtomicUsize::new(0) }; 4];
#[cfg(target_os = "linux")]
static READY: AtomicBool = AtomicBool::new(false);
const STACK: usize = 2 * 1024 * 1024;
const SC: &str = "SEFER_B4_30_SCENARIO";
const D: &str = "SEFER_B4_30_P";
const F: &str = "SEFER_B4_30_FREE";
const ENFORCE: &str = "SEFER_B4_30_ENFORCE";

// ---------------------------------------------------------------------------
// Declared budgets (bytes of DIFFERENTIAL stack requirement `need = fs - fc +
// stride` of a first-alloc scenario over the eat-only control, worst over
// scenarios, measured on a fresh 2 MiB worker thread).
//
// Rule (fixed): budget = ceil_to_1024(worst_differential)
//                     + max(4096, ceil_to_1024(0.25 * worst_differential)).
// The 1024-granular base absorbs boundary/measurement noise; the reserve is
// at least 4 KiB (the residual uncertainty of the bisect window is <= ~3
// frames <= 1.5 KiB) and 25% wherever that is larger. margin = budget - need
// is a MODEL-level detection bound only (a growth G is caught where G >
// margin); it is NOT a mutant-proven detection claim — see the mutant table
// in B4_30_STACK_CONTRACT.md for what the v1/v4 mutants actually did.
//
// Frame isolation (fix7): every scenario body runs in its OWN
// `#[inline(never)]` frame (`sc_*` below), the control runs in a separate
// `sc_eat_only` frame, and the worker passes `black_box(&mut body)` into the
// eater so the body can neither be devirtualised into `eat` nor share a
// prologue with the allocator's `#[inline(always)]` entry chain
// (`GlobalAlloc::alloc`/`dealloc`, `current_heap`, `HeapCore::alloc`). The
// eat-only control therefore never contains allocator code and the stack
// probe of the scenario prologue cannot be charged to the control (the fix6
// stand defect that made every mutant look like a lattice shift).
//
// Base: worst per-(OS, profile) differential need over scenarios and both
// feature configs, measured by THIS test logic (the pin is the sha256 of
// this file with the `const BUDGET_*` values masked, so constant-only
// rebasing does not invalidate the evidence; asserted against the live file
// by scripts/b4_30_stack_table.mjs) and recomputed by
// scripts/b4_30_stack_table.mjs from the raw run logs, which assert these
// constants against the evidence. The budgets are (os, profile) keyed and
// apply to both feature configs; the assert is ONE-SIDED upper
// (`need <= budget` under SEFER_B4_30_ENFORCE=1); there is no lower bound —
// scenarios legitimately measure far below the worst-case budget.
// ---------------------------------------------------------------------------
const BUDGET_WIN_DEBUG: usize = 87040;
const BUDGET_WIN_RELEASE: usize = 19456;
const BUDGET_LINUX_DEBUG: usize = 86016;
const BUDGET_LINUX_RELEASE: usize = 17408;
/// Confirmation-probe slack ABOVE the measured boundary `fs` (fix9). The
/// runtime confirmation probe runs in a FRESH child process, and on Windows
/// the frame placement of the eater differs between processes: the fix7 raw
/// logs show fs/fc spreads across independent runs of the same cell up to
/// 224 B (win-debug-production fs 89639..89863, stride 224 => boundary moves
/// in whole strides), and the contract records cross-run frame jitter up to
/// ~448 B (mutant v4 win-debug notes). A probe at exactly `fs` can therefore
/// land below the child process's own minimal passing boundary and overflow
/// spuriously (CI flake: job `test windows (production)`, rustc 1.99.1,
/// commit 153112ae, `confirmation probe at measured boundary 34959
/// overflowed`). The probe target is raised by a platform slack constant:
/// windows = 1024 B (>= 1 KiB, ~2.3x the worst observed 448 B jitter);
/// linux = 256 B (0 jitter observed in all 12 wsl raw logs — fs identical
/// across every run of every cell — but kept NONZERO as protection against
/// future nondeterminism). The slack does NOT enter `need` or the budget:
/// the boundary bisect and `need = fs - fc + stride` are untouched, and the
/// probe target stays far inside the budget reserve (>= 4 KiB).
const CONF_SLACK_WINDOWS: usize = 1024;
const CONF_SLACK_LINUX: usize = 256;
/// Fully independent bracket+bisect searches per boundary; the worst (max
/// free at the passing boundary) is taken.
const REPEATS: usize = 3;
/// All measurement phases of all test fns are serialized by this mutex:
/// libtest runs test fns on parallel threads, and concurrent 100k-frame
/// recursions would distort each other's measurements (chosen over
/// --test-threads=1 in the CI argv so the CI step needs no special flags).
static MEASURE: Mutex<()> = Mutex::new(());
/// Shared per-process measurement state, computed once: stride, control
/// boundary (worst of REPEATS independent searches), the worker's initial
/// free (entry - low, bracket anchor) and whether the control bracket
/// reached the floor.
static CONTROL: OnceLock<(usize, usize, usize, bool)> = OnceLock::new();

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
}
fn bounds() -> (usize, usize) {
    #[cfg(windows)]
    {
        let (mut l, mut h) = (0usize, 0usize);
        // SAFETY: l/h are writable local outputs.
        unsafe { GetCurrentThreadStackLimits(&mut l, &mut h) };
        (l, h)
    }
    #[cfg(target_os = "linux")]
    {
        while !READY.load(Ordering::Acquire) {
            std::hint::spin_loop()
        }
        (LOW.load(Ordering::Relaxed), HIGH.load(Ordering::Relaxed))
    }
}
/// Recursion with a LIVE frame marker. The stop window is derived from the
/// stride ACTUALLY observed between consecutive frames (prev - current): if
/// the per-frame size changes on another toolchain, the recursion still
/// lands in `[t, t + stride_actual)` or fails loudly.
#[inline(never)]
fn eat(d: usize, target: Option<(usize, usize)>, body: &mut dyn FnMut()) {
    let frame = [0u8; 1];
    black_box(&frame);
    let p = (&frame as *const [u8; 1]) as usize;
    let prev = PREV.swap(p, Ordering::Relaxed);
    let stride_actual = prev.saturating_sub(p);
    if stride_actual > 0 {
        STRIDE_ACT.store(stride_actual, Ordering::Relaxed);
    }
    let reached = target.is_some_and(|(low, t)| {
        let free = p.saturating_sub(low);
        stride_actual > 0 && free >= t && free.saturating_sub(stride_actual) < t
    });
    if d == 0 || reached {
        DEEP.store(p, Ordering::Relaxed);
        // black_box: forbid devirtualisation/inlining of the scenario body
        // into `eat` — the eater's own stride must not depend on the code
        // generation of the allocator entry chain (fix6 observation: stride
        // collapsed 192/160 -> 64/32 when the pad made inlining unattractive).
        black_box(body)()
    } else {
        eat(d - 1, target, body)
    }
    black_box(&frame);
}
/// One ISOLATED frame per scenario (fix7): the allocator's
/// `#[inline(always)]` entry chain may inline only into the `sc_*` frame of
/// the allocating scenario, never into the control's frame. `scenario` below
/// is a pure dispatcher; `sc_eat_only` is empty and in RELEASE LLVM removes
/// the call entirely (no symbol in the asm), so the control's deepest frame
/// is the dispatcher closure (~48 B): need is understated by a CONSTANT
/// <= 48 B, which does not affect the gate or the mutants.
#[inline(never)]
fn sc_eat_only() {}
#[inline(never)]
fn sc_small() {
    let l = Layout::from_size_align(16, 16).expect("layout");
    // SAFETY: valid allocation, retained until after the recursion.
    let p = unsafe { GLOBAL.alloc(l) };
    assert!(!p.is_null());
    PTR.store(p as usize, Ordering::Relaxed);
    SIZE.store(16, Ordering::Relaxed)
}
#[inline(never)]
fn sc_large() {
    let n = SegmentLayout::SMALL_MAX + 1024;
    let l = Layout::from_size_align(n, 16).expect("layout");
    // SAFETY: valid allocation, retained until after the recursion.
    let p = unsafe { GLOBAL.alloc(l) };
    assert!(!p.is_null());
    PTR.store(p as usize, Ordering::Relaxed);
    SIZE.store(n, Ordering::Relaxed)
}
#[inline(never)]
fn sc_foreign_free(donor: usize) {
    let l = Layout::from_size_align(64, 16).expect("layout");
    // SAFETY: uniquely transferred donor with original layout.
    unsafe { GLOBAL.dealloc(donor as *mut u8, l) }
}
fn scenario(s: &str, donor: usize) {
    match s {
        "eat-only" => sc_eat_only(),
        "small" => sc_small(),
        "large" => sc_large(),
        "foreign-free" => sc_foreign_free(donor),
        _ => panic!("unknown scenario"),
    }
}
// --------------------------------------------------------------------------
// Measurement child. Oracles kept from fix4: bind/claim delta, ctor,
// fallback-lock delta, address geometry, dynamic stride_actual.
// --------------------------------------------------------------------------
#[test]
#[ignore = "measurement child only"]
fn b4_30_probe_child() {
    let name = std::env::var(SC).expect("scenario");
    let free_target =
        std::env::var_os(F).map(|x| x.to_string_lossy().parse::<usize>().expect("free target"));
    let depth: usize = std::env::var(D).expect("depth").parse().expect("integer");
    CLAIM0.store(
        sefer_alloc::registry::heaps_claimed_high_water() as usize,
        Ordering::Relaxed,
    );
    #[cfg(feature = "bench-internals")]
    let src0 = sefer_alloc::registry::HeapRegistry::dbg_registry_claim_sources();
    let donor = if name == "foreign-free" {
        let l = Layout::from_size_align(64, 16).expect("layout");
        // SAFETY: donor ownership passes exclusively to the worker.
        let p = unsafe { GLOBAL.alloc(l) };
        assert!(!p.is_null());
        p as usize
    } else {
        0
    };
    let child = name.clone();
    let worker = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || {
            let marker = 0u8;
            ENTRY.store((&marker as *const u8) as usize, Ordering::Release);
            let (low, high) = bounds();
            LOW.store(low, Ordering::Relaxed);
            HIGH.store(high, Ordering::Relaxed);
            let before = sefer_alloc::global::tls_heap::current_for_trim().is_none();
            CLAIM1.store(
                sefer_alloc::registry::heaps_claimed_high_water() as usize,
                Ordering::Relaxed,
            );
            FB0.store(
                sefer_alloc::global::dbg_fallback_lock_acquisitions() as usize,
                Ordering::Relaxed,
            );
            #[cfg(feature = "bench-internals")]
            for (i, v) in sefer_alloc::registry::HeapRegistry::dbg_registry_claim_sources()
                .into_iter()
                .enumerate()
            {
                SRC0[i].store(v as usize, Ordering::Relaxed)
            }
            let mut body = || scenario(&child, donor);
            PREV.store(0, Ordering::Relaxed);
            STRIDE_ACT.store(0, Ordering::Relaxed);
            if let Some((low, t)) = free_target.map(|t| (low, t)) {
                eat(100_000, Some((low, t)), &mut body);
                assert_ne!(
                    DEEP.load(Ordering::Relaxed),
                    0,
                    "target residue not reached"
                );
            } else {
                eat(depth, None, &mut body)
            }
            let p = PTR.swap(0, Ordering::Relaxed);
            if p != 0 {
                let n = SIZE.swap(0, Ordering::Relaxed);
                let l = Layout::from_size_align(n, 16).expect("layout");
                // SAFETY: exact retained allocation from the scenario.
                unsafe { GLOBAL.dealloc(p as *mut u8, l) }
            }
            let after = sefer_alloc::global::tls_heap::current_for_trim().is_some();
            CLAIM2.store(
                sefer_alloc::registry::heaps_claimed_high_water() as usize,
                Ordering::Relaxed,
            );
            FB1.store(
                sefer_alloc::global::dbg_fallback_lock_acquisitions() as usize,
                Ordering::Relaxed,
            );
            // Oracle: the scenario path must never route through the fallback
            // allocator. Delta != 0 for small/large/foreign-free is a hard
            // failure. eat-only is not checked (no allocation by design).
            if child != "eat-only" {
                assert_eq!(
                    FB1.load(Ordering::Relaxed),
                    FB0.load(Ordering::Relaxed),
                    "fallback lock acquired during scenario: first-bind path not isolated"
                );
            }
            #[cfg(feature = "bench-internals")]
            {
                let source_after =
                    sefer_alloc::registry::HeapRegistry::dbg_registry_claim_sources();
                for (i, v) in source_after.into_iter().enumerate() {
                    SRC1[i].store(v as usize, Ordering::Relaxed)
                }
            }
            RESULT.store(
                (before as usize) | ((after as usize) << 1),
                Ordering::Relaxed,
            )
        })
        .expect("unnamed worker");
    #[cfg(target_os = "linux")]
    {
        while ENTRY.load(Ordering::Acquire) == 0 {
            std::hint::spin_loop()
        }
        let a = ENTRY.load(Ordering::Acquire);
        let maps = std::fs::read_to_string("/proc/self/maps").expect("read maps");
        let mut found = None;
        let mut prev: Option<(usize, usize, String)> = None;
        for line in maps.lines() {
            let mut f = line.split_whitespace();
            let r = f.next().unwrap();
            let perm = f.next().unwrap();
            let (x, y) = r.split_once('-').unwrap();
            let l = usize::from_str_radix(x, 16).unwrap();
            let h = usize::from_str_radix(y, 16).unwrap();
            if perm.starts_with("rw-p") && l <= a && a < h {
                let guard = prev
                    .as_ref()
                    .filter(|(_, end, permissions)| *end == l && permissions.starts_with("---p"))
                    .map(|(start, end, _)| end - start)
                    .unwrap_or(0);
                found = Some((l, h, guard));
                break;
            }
            prev = Some((l, h, perm.to_string()));
        }
        let (l, h, g) = found.expect("worker entry lacks rw-p VMA");
        assert!(
            h - l + g >= STACK,
            "VMA and adjacent guard smaller than requested stack"
        );
        LOW.store(l, Ordering::Relaxed);
        HIGH.store(h, Ordering::Relaxed);
        GUARD.store(g, Ordering::Relaxed);
        READY.store(true, Ordering::Release)
    }
    worker.join().expect("worker panic");
    let result = RESULT.load(Ordering::Relaxed);
    let expected_result = if name == "eat-only" || name == "foreign-free" {
        1
    } else if name == "small" || name == "large" {
        3
    } else {
        unreachable!()
    };
    let claim_delta = CLAIM2
        .load(Ordering::Relaxed)
        .saturating_sub(CLAIM1.load(Ordering::Relaxed));
    let expected_delta = if name == "small" || name == "large" {
        1
    } else {
        0
    };
    assert_eq!(
        claim_delta, expected_delta,
        "registry claim high-water delta"
    );
    assert_eq!(result, expected_result, "binding oracle mismatch");
    let hc = core::mem::size_of::<sefer_alloc::registry::HeapCore>();
    #[cfg(feature = "bench-internals")]
    let sources_flat = core::array::from_fn::<_, 12, _>(|i| {
        if i < 4 {
            SRC0[i].load(Ordering::Relaxed) as u64
        } else if i < 8 {
            SRC1[i - 4].load(Ordering::Relaxed) as u64
        } else {
            src0[i - 8]
        }
    });
    #[cfg(not(feature = "bench-internals"))]
    let sources_flat = [0u64; 12];
    println!(
        "{{\"scenario\":\"{name}\",\"depth\":{depth},\"entry_addr\":{},\"deep_addr\":{},\"stack_low\":{},\"stack_high\":{},\"guard_bytes\":{},\"result\":{result},\"heapcore_size\":{hc},\"stride_actual\":{},\"fallback_lock_before\":{},\"fallback_lock_after\":{},\"claim_before\":{},\"claim_worker_before\":{},\"claim_worker_after\":{},\"claim_delta\":{},\"ctor_observed\":{},\"claim_sources_flat\":{:?}}}",
        ENTRY.load(Ordering::Relaxed),
        DEEP.load(Ordering::Relaxed),
        LOW.load(Ordering::Relaxed),
        HIGH.load(Ordering::Relaxed),
        GUARD.load(Ordering::Relaxed),
        STRIDE_ACT.load(Ordering::Relaxed),
        FB0.load(Ordering::Relaxed),
        FB1.load(Ordering::Relaxed),
        CLAIM0.load(Ordering::Relaxed),
        CLAIM1.load(Ordering::Relaxed),
        CLAIM2.load(Ordering::Relaxed),
        CLAIM2
            .load(Ordering::Relaxed)
            .saturating_sub(CLAIM1.load(Ordering::Relaxed)),
        (name == "small" || name == "large")
            && CLAIM2.load(Ordering::Relaxed) == CLAIM1.load(Ordering::Relaxed) + 1,
        sources_flat
    )
}
// --------------------------------------------------------------------------
// Parent driver: self-measurement against the declared budget.
// --------------------------------------------------------------------------
struct Probe {
    free: usize,     // deep_addr - stack_low at the stop
    consumed: usize, // entry_addr - deep_addr
    stride_act: usize,
}
enum Step {
    Pass(Probe),
    Overflow,
}
/// Overflow classification by signature: Windows STATUS_STACK_OVERFLOW
/// (0xC00000FD), Linux SIGABRT with the libstd "has overflowed its stack"
/// message. Anything else is a loud test failure, never a skip.
fn classify(o: &std::process::Output) -> Step {
    if o.status.success() {
        return Step::Pass(parse_probe(&String::from_utf8_lossy(&o.stdout)));
    }
    #[cfg(windows)]
    if o.status.code() == Some(-1073741571) {
        // 0xC00000FD as i32
        return Step::Overflow;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if o.status.code().is_none()
            && o.status.signal() == Some(6)
            && String::from_utf8_lossy(&o.stderr).contains("has overflowed its stack")
        {
            // SIGABRT: on Linux the overflow abort kills the process, so the
            // status is a signal, not an exit code.
            return Step::Overflow;
        }
    }
    panic!(
        "b4-30 child ended with an unclassified status {:?}: {}",
        o.status.code(),
        String::from_utf8_lossy(&o.stderr)
    )
}
fn parse_probe(stdout: &str) -> Probe {
    let line = stdout
        .lines()
        .find(|x| x.starts_with('{'))
        .expect("child stdout JSON");
    let j: serde_json::Value = serde_json::from_str(line).expect("child JSON parse");
    let entry = j["entry_addr"].as_u64().expect("entry") as usize;
    let deep = j["deep_addr"].as_u64().expect("deep") as usize;
    let low = j["stack_low"].as_u64().expect("low") as usize;
    let high = j["stack_high"].as_u64().expect("high") as usize;
    let stride_act = j["stride_actual"].as_u64().expect("stride") as usize;
    // Geometry oracle (fix4): fixed 2 MiB worker stack, deep below entry,
    // both inside [low, high). high - deep is the physical depth.
    assert_eq!(high - low, STACK, "worker stack size != 2 MiB");
    assert!(
        low < deep && deep < entry && entry < high,
        "address geometry"
    );
    assert!(
        (1..=512).contains(&stride_act),
        "stride_actual {stride_act} outside the measured bound 1..=512"
    );
    Probe {
        free: deep - low,
        consumed: entry - deep,
        stride_act,
    }
}
fn step(s: &str, depth: usize, t: Option<usize>) -> Step {
    let mut cmd = std::process::Command::new(std::env::current_exe().expect("current_exe"));
    cmd.args(["--exact", "b4_30_probe_child", "--ignored", "--nocapture"])
        .env(SC, s)
        .env(D, depth.to_string());
    if let Some(t) = t {
        cmd.env(F, t.to_string());
    }
    let o = cmd.output().expect("spawn child");
    classify(&o)
}
/// Two-point eater calibration (depths 8 and 9, depth mode): slope of
/// consumed = entry - deep is the per-frame stride. Fails loudly if the
/// frame exceeds 512 B (the stop window and budget reserve assume the
/// fix7-observed strides 48/80/192/224 B).
fn calibrate() -> usize {
    let a = match step("eat-only", 8, None) {
        Step::Pass(p) => p,
        Step::Overflow => panic!("calibration depth 8 overflowed"),
    };
    let b = match step("eat-only", 9, None) {
        Step::Pass(p) => p,
        Step::Overflow => panic!("calibration depth 9 overflowed"),
    };
    let slope = b.consumed - a.consumed;
    assert_eq!(
        slope, a.stride_act,
        "two-point stride {slope} != child-observed stride_actual {}",
        a.stride_act
    );
    assert!(
        (1..=512).contains(&slope),
        "eat frame stride {slope} B > 512 B: protocol assumption broken, \
         recalibrate the budgets before trusting this test"
    );
    slope
}
/// One FULL boundary search: MINIMAL passing free target (the boundary R: a
/// target below R demands a deeper stop than the stack allows -> overflow),
/// found by exponential bracket + binary bisect down to a 2-frame window.
/// The anchor is near the TOP of the worker stack (always passing); the
/// expansion walks downward until the first overflow, so a toolchain that
/// shifted the boundary in either direction is followed automatically.
/// Returns the probe at the MINIMAL passing stop (its `free` is the free
/// ACTUALLY observed there, in [t, t + stride)) and whether the bracket
/// reached the FLOOR of the free coordinate (the body completes even on the
/// deepest reachable frame — a valid degenerate boundary, reported to the
/// caller; it is the caller's decision how to treat it).
fn boundary_search(s: &str, free0: usize, stride: usize) -> (Probe, bool) {
    let unit = stride.max(1);
    // Margin covers the Windows child-to-child entry spread (~2 KiB) plus a
    // few frames, so the anchor is reachable in every child.
    let t0 = free0 - (4096 + 8 * unit);
    let (mut hi, mut hi_probe) = match step(s, 0, Some(t0)) {
        Step::Pass(p) => (t0, p),
        Step::Overflow => panic!("top anchor {t0} (free0 {free0}) overflowed"),
    };
    // Exponential expansion downward until the first overflow.
    let mut gap = 8 * unit;
    let mut lo;
    let mut floor = false;
    loop {
        lo = if gap >= hi { hi / 2 } else { hi - gap };
        if lo == 0 {
            floor = true;
            break;
        }
        match step(s, 0, Some(lo)) {
            Step::Overflow => break,
            Step::Pass(p) => {
                hi = lo;
                hi_probe = p;
                gap *= 2;
            }
        }
    }
    while hi - lo > 2 * unit {
        let mid = lo + (hi - lo) / 2;
        match step(s, 0, Some(mid)) {
            Step::Pass(p) => {
                hi = mid;
                hi_probe = p;
            }
            Step::Overflow => lo = mid,
        }
    }
    (hi_probe, floor)
}
/// Shared per-process state: stride from two-point calibration, the eat-only
/// control boundary (worst of REPEATS independent searches), the worker's
/// initial free (entry - low, used as the bracket anchor), and whether the
/// control bracket reached the floor.
fn control() -> (usize, usize, usize, bool) {
    *CONTROL.get_or_init(|| {
        let stride = calibrate();
        let free0 = match step("eat-only", 64, None) {
            Step::Pass(p) => p.free + p.consumed, // (deep-low)+(entry-deep) = entry-low
            Step::Overflow => panic!("depth-64 anchor probe overflowed"),
        };
        let mut fc = 0;
        let mut floor = false;
        for _ in 0..REPEATS {
            let (p, f) = boundary_search("eat-only", free0, stride);
            fc = fc.max(p.free);
            floor |= f;
        }
        (stride, fc, free0, floor)
    })
}
fn budget() -> usize {
    match (std::env::consts::OS, cfg!(debug_assertions)) {
        ("windows", true) => BUDGET_WIN_DEBUG,
        ("windows", false) => BUDGET_WIN_RELEASE,
        ("linux", true) => BUDGET_LINUX_DEBUG,
        ("linux", false) => BUDGET_LINUX_RELEASE,
        _ => unreachable!("file-level cfg admits only windows/linux"),
    }
}
fn skip_if_unmeasurable() -> bool {
    if cfg!(target_arch = "x86_64") {
        return false;
    }
    println!(
        "[b4-30] SKIP measured OS {} but unmeasured non-x86_64 arch (budgets are x86_64 evidence)",
        std::env::consts::OS
    );
    true
}
/// Full measurement of one scenario: REPEATS fully independent boundary
/// searches (worst free), the differential `need = fs - fc + stride` over
/// the control (or the absolute upper estimate when the control floored),
/// the budget assert (only under SEFER_B4_30_ENFORCE=1), a runtime
/// confirmation probe at the measured boundary plus the platform
/// cross-process slack (fix9; the probe does not participate in `need`),
/// then the markers.
fn measure(s: &str) {
    let (stride, fc, free0, control_floor) = control();
    let mut fs = 0;
    let mut floor = false;
    for _ in 0..REPEATS {
        let (p, f) = boundary_search(s, free0, stride);
        fs = fs.max(p.free);
        floor |= f;
    }
    // Differential need; if the control floored, the differential is not
    // measurable (control requirement ~ 0 in this coordinate) and the sound
    // absolute upper estimate fs + stride is used instead.
    let need = if control_floor {
        fs + stride
    } else {
        fs.saturating_sub(fc) + stride
    };
    let b = budget();
    let enforce = std::env::var_os(ENFORCE).is_some_and(|v| v == "1");
    if enforce {
        assert!(
            need <= b,
            "B4-30 first-alloc differential stack need {need} B exceeds the declared \
             budget {b} B (os={} profile={} scenario={s}): the first-alloc path grew \
             past the evidenced worst differential + its reserve",
            std::env::consts::OS,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
        );
        // Enforce-only static-literal markers: a CI step that greps these
        // (instead of the unconditional `scenario=<s> need=` markers) FAILS
        // unless enforcement actually ran — a typo in the env name or a
        // rename of ENFORCE would otherwise silently turn every enforced
        // step into measure-only with green CI.
        match s {
            "small" => println!("[b4-30] enforced scenario=small"),
            "large" => println!("[b4-30] enforced scenario=large"),
            _ => println!("[b4-30] enforced scenario=foreign-free"),
        }
    }
    // Runtime confirmation (fix9): the scenario must still pass in a fresh
    // child process at a target NOT BELOW the measured boundary. The target
    // is `fs + CONF_SLACK_*` (see the constants): the confirming child's
    // frame placement jitters against the measuring process (Windows up to
    // ~448 B observed), so probing at exactly `fs` flakes with a spurious
    // overflow (CI: job `test windows (production)`, rustc 1.99.1, commit
    // 153112ae). Hardness is kept: the probe MUST pass, and because the
    // child stops in `[t, t + stride)` of its own free coordinate, a pass at
    // `t = fs + slack` proves the scenario executed at free >=
    // `t - stride >= fs` (slack >= stride holds for both platforms:
    // 1024 >= 224, 256 >= 192) — i.e. the upper bound on the requirement
    // is still demonstrated by execution, not weakened into a no-op.
    let slack = if cfg!(windows) {
        CONF_SLACK_WINDOWS
    } else {
        CONF_SLACK_LINUX
    };
    let target = fs + slack;
    let cfree = match step(s, 0, Some(target)) {
        Step::Pass(p) => p.free,
        Step::Overflow => {
            panic!("confirmation probe at measured boundary {fs} + slack {slack} overflowed")
        }
    };
    // FAIL both ways: below `target - stride` would mean the measured
    // boundary is overstated (the scenario needs MORE than `fs` in this
    // child); at/above `target + stride` cannot result from a passing stop
    // window but is asserted anyway so the window stays explicit.
    assert!(
        cfree + stride >= target && cfree < target + stride,
        "confirmation free {cfree} outside the window [{}, {}) at target {target} (boundary {fs} + slack {slack})",
        target.saturating_sub(stride),
        target + stride
    );
    // Static-literal scenario markers (CI greps the verbatim prefix; kept as
    // bare println!("<literal>") so scripts/verify-ci-sentinels.mjs resolves
    // them), each printed only AFTER every assert above has passed.
    match s {
        "small" => println!("[b4-30] scenario=small need="),
        "large" => println!("[b4-30] scenario=large need="),
        _ => println!("[b4-30] scenario=foreign-free need="),
    }
    println!(
        "[b4-30] scenario={s} details need={need} budget={b} margin={} fs={fs} fc={fc} stride={stride} boundary={} control={} enforce={}",
        b as i64 - need as i64,
        if floor { "floor" } else { "bisect" },
        if control_floor { "floor" } else { "bisect" },
        if enforce { 1 } else { 0 },
    );
    // Actual compiler version, NOT the Cargo.toml `rust-version` (MSRV)
    // field: CARGO_PKG_RUST_VERSION is the manifest's rust-version, which is
    // not the toolchain identity. Run `rustc -V` at runtime; on failure fall
    // back to the manifest field, explicitly marked as such.
    let rustc_v = match std::process::Command::new("rustc").arg("-V").output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_owned(),
        _ => format!(
            "{} (msrv fallback)",
            option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown")
        ),
    };
    println!(
        "[b4-30] toolchain={} os={} profile={}",
        rustc_v,
        std::env::consts::OS,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
}
#[test]
fn b4_30_small_first_alloc_stack_budget() {
    let _g = MEASURE.lock().unwrap_or_else(|e| e.into_inner());
    if skip_if_unmeasurable() {
        return;
    }
    measure("small")
}
#[test]
fn b4_30_large_first_alloc_stack_budget() {
    let _g = MEASURE.lock().unwrap_or_else(|e| e.into_inner());
    if skip_if_unmeasurable() {
        return;
    }
    measure("large")
}
#[test]
fn b4_30_foreign_free_first_alloc_stack_budget() {
    let _g = MEASURE.lock().unwrap_or_else(|e| e.into_inner());
    if skip_if_unmeasurable() {
        return;
    }
    measure("foreign-free")
}
