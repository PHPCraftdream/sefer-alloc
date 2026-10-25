//! gp_kernel_probe — SINGLE-KERNEL, SINGLE-ARM no-timer driver for callgrind.
//!
//! One arm (sefer or mimalloc), one kernel, `portion_body` only — NO timers
//! anywhere (no `tsc_now`, no `qpc`, no `run_portion`); callgrind counts Ir
//! and cache events itself. Seeds and window are JUDGE-EXACT via the shared
//! core (`judge_warmup_seed` / `judge_block_seed` / `judge_warmup_kernel`),
//! so the probe's window is the judge's window 1:1:
//!
//! ```text
//! warmup: WARMUP_PORTIONS(=32) portions per judge_warmup_kernel call
//!         (seed wr = seed + w*PORTION_SEED_G, both arms simulated);
//!         --warmup N = N whole judge warmup cycles (N*32 portions)
//! portion_seed(block b) = judge_block_seed(seed, ki, b)
//!                        = (seed + b*0x1000_0000_0000_0001).rotate_left(ki)
//! seed2 = portion_seed.wrapping_mul(PORTION_SEED_G) | 1
//! ```
//!
//! where `ki` is the kernel's index in `ALL_KERNELS` — exactly the judge's
//! formula.
//!
//! Activation oracle (abort with exit code 1): a `RingState` mutated in
//! place cumulatively, `expected_block_calls` per block, each element of the
//! pair compared with the actual shim delta of the corresponding portion;
//! foreign shim slots must stay 0. NO wall-clock measurements exist here.
//!
//! `--oracle-break` (TEST-ONLY, negative test of the oracle itself): after
//! the successful block-0.. checks, at block `ORACLE_BREAK_BLOCK` the
//! portion-0 check is INVERTED (die when `got == expected`), so a correct
//! run fails with exit code 1 by design. Never used in real/raw runs.
//!
//! Output protocol (stdout): ONE identity header JSON line first, then one
//! JSON line per measured portion, then one summary JSON line.
use std::io::Write;

include!("_shared/b4_interleave_core.rs");

/// The harness allocator — System with counting (as in the judge).
#[global_allocator]
static HARNESS: HarnessAlloc = HarnessAlloc;

const DEFAULT_SEED: u64 = 0xB411_0001;
/// Default `--warmup`: ONE judge-exact warmup cycle (WARMUP_PORTIONS
/// portions per kernel, as the judge does).
const DEFAULT_WARMUP: u64 = 1;
/// Block whose portion-0 oracle check is inverted under `--oracle-break`
/// (TEST-ONLY negative test; must be < window blocks).
const ORACLE_BREAK_BLOCK: u64 = 3;

fn parse_u64_any(s: &str, what: &str) -> u64 {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        let hex = hex.replace('_', "");
        u64::from_str_radix(&hex, 16).unwrap_or_else(|e| panic!("bad {what} `{s}`: {e}"))
    } else {
        let s = s.replace('_', "");
        s.parse()
            .unwrap_or_else(|e| panic!("bad {what} `{s}`: {e}"))
    }
}

fn need(it: &mut impl Iterator<Item = String>, flag: &str) -> String {
    it.next().unwrap_or_else(|| panic!("{flag} needs a value"))
}

fn die(msg: &str) -> ! {
    eprintln!("[oracle] FAIL: {msg}");
    // Explicit non-zero exit: the runner treats anything != 0 as oracle fail.
    std::process::exit(1);
}

struct Args {
    arm: String,
    kernel: Kernel,
    warmup: u64,
    portions: u64,
    seed: u64,
    oracle_break: bool,
}

fn parse_args() -> Args {
    let mut arm = String::new();
    let mut kernel_name = String::new();
    let mut warmup = DEFAULT_WARMUP;
    let mut portions: Option<u64> = None;
    let mut seed = DEFAULT_SEED;
    let mut oracle_break = false;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--arm" => arm = need(&mut it, "--arm"),
            "--kernel" => kernel_name = need(&mut it, "--kernel"),
            // Whole judge warmup CYCLES (N * WARMUP_PORTIONS portions).
            "--warmup" => warmup = need(&mut it, "--warmup").parse().expect("--warmup"),
            "--portions" => {
                portions = Some(need(&mut it, "--portions").parse().expect("--portions"))
            }
            "--seed" => seed = parse_u64_any(&need(&mut it, "--seed"), "--seed"),
            // TEST-ONLY: inverts the oracle check at ORACLE_BREAK_BLOCK so a
            // CORRECT run fails (negative test of the oracle itself).
            "--oracle-break" => oracle_break = true,
            other => panic!("unknown arg {other}"),
        }
    }
    if !matches!(arm.as_str(), "sefer" | "mi") {
        panic!("--arm must be `sefer` or `mi`");
    }
    let kernel = Kernel::parse(&kernel_name)
        .unwrap_or_else(|| panic!("unknown kernel `{kernel_name}` (see Kernel::name)"));
    let portions = portions.unwrap_or_else(|| panic!("--portions is required"));
    // A block = 2 portions; the window must be whole blocks.
    assert!(
        portions.is_multiple_of(2),
        "--portions must be even (blocks of 2 portions)"
    );
    assert!(warmup >= 1, "--warmup must be >= 1 (judge warmup cycles)");
    if oracle_break {
        assert!(
            portions / 2 > ORACLE_BREAK_BLOCK,
            "--oracle-break needs more than {ORACLE_BREAK_BLOCK} window blocks"
        );
    }
    Args {
        arm,
        kernel,
        warmup,
        portions,
        seed,
        oracle_break,
    }
}

/// Existing bench-internals counter reads, ONLY under their feature gates.
/// The five registry counters: `sefer_alloc::registry::heap_registry::*`
/// (`internals` gates the pub mod path, the counters `bench-internals`).
/// The two S1 refill observers: `sefer_alloc::alloc_core::AllocCore::*`
/// (the `alloc_core` pub mod path needs `alloc-core` + `internals`, the
/// methods themselves are `bench-internals`-gated; `alloc-core` is implied
/// by `production`). Read-only: no counters are created or reset here.
#[cfg(all(feature = "internals", feature = "bench-internals"))]
fn sefer_events_snapshot() -> [(&'static str, u64); 7] {
    [
        (
            "tcache_hits_total",
            sefer_alloc::registry::heap_registry::tcache_hits_total(),
        ),
        (
            "large_cache_hits_total",
            sefer_alloc::registry::heap_registry::large_cache_hits_total(),
        ),
        ("tcache_and_large_cache_hits_total", {
            let (t, l) = sefer_alloc::registry::heap_registry::tcache_and_large_cache_hits_total();
            t.saturating_add(l)
        }),
        (
            "config_conflicts_total",
            sefer_alloc::registry::heap_registry::config_conflicts_total(),
        ),
        (
            "heaps_claimed_high_water",
            u64::from(sefer_alloc::registry::heap_registry::heaps_claimed_high_water()),
        ),
        (
            "dbg_s1_refill_events",
            sefer_alloc::alloc_core::AllocCore::dbg_s1_refill_events(),
        ),
        (
            "dbg_s1_mag_byte_writes",
            sefer_alloc::alloc_core::AllocCore::dbg_s1_mag_byte_writes(),
        ),
    ]
}
#[cfg(not(all(feature = "internals", feature = "bench-internals")))]
fn sefer_events_snapshot() -> [(&'static str, u64); 0] {
    []
}

/// No-timer runner for the shared judge warmup loop (the probe's arm of
/// `judge_warmup_kernel`): `portion_body` only, timings discarded.
fn run_nt(table: &'static ShimTable, kernel: Kernel, seed: u64) {
    let _ = portion_body(table, kernel, seed);
}

fn main() {
    let args = parse_args();
    let table: &'static ShimTable = if args.arm == "sefer" {
        &SEFER_A_TABLE
    } else {
        &MI_A_TABLE
    };
    let slot = table.call_slot;
    let window_blocks = args.portions / 2;
    let ki = ALL_KERNELS
        .iter()
        .position(|k| k.name() == args.kernel.name())
        .expect("kernel is in ALL_KERNELS") as u32;
    let events_on = cfg!(all(feature = "internals", feature = "bench-internals"));
    let events_names: &[&str] = &sefer_events_snapshot()
        .iter()
        .map(|(n, _)| *n)
        .collect::<Vec<_>>();

    let out = std::io::stdout();
    let mut w = out.lock();
    // ── Identity header: ALWAYS the first stdout line. ────────────────────
    writeln!(
        w,
        "{{\"probe\":\"gp_kernel_probe\",\"arm\":\"{}\",\"kernel\":\"{}\",\"warmup_cycles\":{},\
         \"warmup_portions\":{},\"portions\":{},\"seed\":\"{:#x}\",\
         \"seed_formula\":\"judge_warmup_seed/judge_block_seed\",\
         \"kernel_index_ki\":{},\"warmup_portions_const\":{},\
         \"block_seed_formula\":\"(seed + block_no*0x1000000000000001).rotate_left(ki)\",\
         \"seed2_formula\":\"portion_seed.wrapping_mul(0x9E3779B97F4A7C15) | 1\",\
         \"oracle_break\":{},\"rustc\":\"{}\",\"events_features_on\":{},\"events\":{},\
         \"pair_ops\":{},\"bulk64_n\":{},\"bulk64_passes\":{},\"bulk16k_n\":{},\
         \"bulk16k_passes\":{},\"churn_live\":{},\"churn_ops\":{},\"churn1024_ops\":{},\
         \"vec_elems\":{},\"vec_cycles\":{},\"larson_live\":{},\"larson_ops\":{},\
         \"larson_min\":{},\"larson_max\":{},\"larson_step\":{},\"rt_min\":{},\"rt_max\":{},\
         \"rt_granule\":{},\"portion_seed_g\":\"{:#x}\",\"timers\":\"none\"}}",
        args.arm,
        args.kernel.name(),
        args.warmup,
        args.warmup * WARMUP_PORTIONS,
        args.portions,
        args.seed,
        ki,
        WARMUP_PORTIONS,
        args.oracle_break,
        option_env!("GP_RUSTC").unwrap_or("unknown"),
        events_on,
        serde_lite_json(events_names),
        PAIR_OPS,
        BULK64_N,
        BULK64_PASSES,
        BULK16K_N,
        BULK16K_PASSES,
        CHURN_LIVE,
        CHURN_OPS,
        CHURN1024_OPS,
        VEC_ELEMS,
        VEC_CYCLES,
        LARSON_LIVE,
        LARSON_OPS,
        LARSON_MIN,
        LARSON_MAX,
        LARSON_STEP,
        RT_MIN,
        RT_MAX,
        RT_GRANULE,
        PORTION_SEED_G,
    )
    .expect("write header");
    w.flush().expect("flush header");

    let mut st = RingState {
        churn: [false; CHURN_LIVE],
        larson: [false; LARSON_LIVE],
    };
    let mut warmup_calls: u64 = 0;
    let mut window_calls: u64 = 0;
    let ev_before = sefer_events_snapshot();

    // Judge-exact warmup: whole judge_warmup_kernel cycles. Each cycle runs
    // WARMUP_PORTIONS portions on BOTH shim sets and steps the occupancy
    // simulation in lockstep (so the oracle is exact from block 0). Both
    // probe arms ARE the same shim set; the second RingState only receives
    // the idempotent duplicate occupancy marks of the same seeds.
    let mut st_b = RingState {
        churn: [false; CHURN_LIVE],
        larson: [false; LARSON_LIVE],
    };
    for _ in 0..args.warmup {
        let before = SHIM_CALLS[slot].load(Ordering::Relaxed);
        judge_warmup_kernel(
            table,
            table,
            args.kernel,
            args.seed,
            &mut st,
            &mut st_b,
            run_nt,
        );
        warmup_calls += SHIM_CALLS[slot].load(Ordering::Relaxed) - before;
    }

    for b in 0..window_blocks {
        // The JUDGE-EXACT portion-seed formula (kernel-index based).
        let portion_seed = judge_block_seed(args.seed, ki, b);
        let seed2 = portion_seed.wrapping_mul(PORTION_SEED_G) | 1;

        let (d1, _) = portion_body(table, args.kernel, portion_seed);
        let got1 = d1[slot];
        let f1 = foreign_delta(&d1, slot);
        let (d2, _) = portion_body(table, args.kernel, seed2);
        let got2 = d2[slot];
        let f2 = foreign_delta(&d2, slot);

        // Activation oracle, block-exact like the judge: the pair (ps, ps2).
        let (exp1, exp2) = expected_block_calls(&mut st, args.kernel, portion_seed);
        if f1 != 0 {
            die(&format!("block={b} portion=0 foreign={f1} (must be 0)"));
        }
        if f2 != 0 {
            die(&format!("block={b} portion=1 foreign={f2} (must be 0)"));
        }
        // TEST-ONLY negative test (--oracle-break): at ORACLE_BREAK_BLOCK the
        // portion-0 check is INVERTED — a CORRECT run (got == expected) fails
        // with exit code 1 by design; real/raw runs never pass this flag.
        if args.oracle_break && b == ORACLE_BREAK_BLOCK {
            if got1 == exp1 {
                die(&format!(
                    "oracle-break (test-only inversion) block={b} portion=0 \
                     calls={got1} expected={exp1} — correct run inverted-fails by design"
                ));
            }
            window_calls += got1 + got2;
            continue;
        }
        if got1 != exp1 {
            die(&format!("block={b} portion=0 calls={got1} expected={exp1}"));
        }
        if got2 != exp2 {
            die(&format!("block={b} portion=1 calls={got2} expected={exp2}"));
        }

        window_calls += got1 + got2;
        let mut w = std::io::stdout().lock();
        writeln!(
            w,
            "{{\"i\":{},\"shim_calls\":{},\"exp\":{},\"foreign\":0}}",
            b * 2,
            got1,
            exp1
        )
        .expect("write portion");
        writeln!(
            w,
            "{{\"i\":{},\"shim_calls\":{},\"exp\":{},\"foreign\":0}}",
            b * 2 + 1,
            got2,
            exp2
        )
        .expect("write portion");
        w.flush().expect("flush portion");
    }

    // Summary: window + warmup shim totals; bench-internals deltas when the
    // counters are compiled in (events features on), else `events: null`.
    let ev_after = sefer_events_snapshot();
    let mut w = std::io::stdout().lock();
    if ev_before.is_empty() {
        writeln!(
            w,
            "{{\"summary\":true,\"window_shim_calls\":{},\"warmup_shim_calls\":{},\
             \"window_portions\":{},\"events\":null}}",
            window_calls, warmup_calls, args.portions,
        )
        .expect("write summary");
    } else {
        write!(w, "{{\"summary\":true,\"window_shim_calls\":{},\"warmup_shim_calls\":{},\"window_portions\":{},\"events\":{{", window_calls, warmup_calls, args.portions).expect("write summary");
        let mut first = true;
        for ((n, b), (_, a)) in ev_before.iter().zip(ev_after.iter()) {
            if !first {
                write!(w, ",").expect("write summary");
            }
            first = false;
            write!(w, "\"{}\":{}", n, a.wrapping_sub(*b)).expect("write summary");
        }
        writeln!(w, "}}}}").expect("write summary");
    }
    w.flush().expect("flush summary");

    // Explicit teardown of every live block through its own shim table
    // (same contract as the judge; after the summary so shim totals above
    // cover only the kernel portions).
    teardown_arms([table, table, table, table]);
}

/// Tiny JSON string-array writer (no serde dependency allowed).
fn serde_lite_json(names: &[&str]) -> String {
    let mut s = String::from("[");
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(n);
        s.push('"');
    }
    s.push(']');
    s
}
