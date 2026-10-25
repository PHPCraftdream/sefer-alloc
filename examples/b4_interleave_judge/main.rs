//! B4 J1′ native interleave judge — MAIN BINARY.
//!
//! One process, both arms: `#[global_allocator]` is the counting
//! `HarnessAlloc` (System underneath, zero allocations inside the window —
//! per-block oracle). Arms: `static SEFER: SeferAlloc` (production feature
//! set) and `static MI: mimalloc::MiMalloc`, reached only through
//! identically-shaped `#[inline(never)]` shims (the `__rust_alloc` face
//! shape of an installed allocator). Kernels are generic over a black-boxed
//! shim table (`examples/_shared/b4_interleave_core.rs`), so the load's
//! machine code is one shared `#[inline(never)]` function per kernel for
//! both arms.
//!
//! Protocol per block: 4 portions in ABBA or BAAB order (the SAME portion
//! seed for both arms; second portion seed = `seed * PORTION_SEED_G | 1`),
//! `ln(t_B / t_A)` per block from the pair sums, aggregated by
//! `scripts/b4_interleave_table.mjs` (which takes THREE raw files: A/A
//! sefer, A/A mimalloc, A/B, and emits ONE table + ONE CSV). State (live
//! blocks) persists between portions — the judge measures the steady state,
//! each arm warmed up before the window. Every live block is explicitly
//! torn down (freed with its OWN layout) before process exit.
//!
//! ## Block ORDER is exact by construction
//! The random coin is GONE. For every kernel a list of exactly N/2 `ABBA`
//! and N/2 `BAAB` entries is built and shuffled Fisher–Yates style with a
//! recorded seed (`--order-seed`, written into the raw header as
//! `order_seed`). Every raw therefore has the STRICT balance
//! ABBA = BAAB = N/2 per kernel — no tolerance anywhere downstream.
//!
//! ## Modes
//! - `ab`        — sefer vs mimalloc (the actual judge).
//! - `aa_sefer`  — sefer×sefer A/A calibration (two identical-shaped shim
//!   sets against the SAME static; pure harness-symmetry control).
//! - `aa_mi`     — mimalloc×mimalloc A/A calibration.
//!
//! ## CLI
//! ```text
//! --mode ab|aa_sefer|aa_mi     default: ab
//! --blocks N                   default 20000 (dry-run: 24); MUST be even
//! --seed S                     default 0xB411_0001; recorded into raw (hex)
//! --order-seed S               default 0x0B4D_5EED; recorded into raw (hex)
//! --out PATH                   raw JSONL path
//! --head-sha HEX               REQUIRED: 40-hex `git rev-parse HEAD` output
//! --source-diff-sha HEX        REQUIRED: sha256 of the bytes of `git diff`
//! --features F                 REQUIRED: feature set the binary was built
//!                              with (recorded verbatim into the header)
//! --profile release|bench-user REQUIRED: cargo profile (validated strictly)
//! --rustc "rustc 1.x.y ..."    REQUIRED: `rustc --version` output
//! --os-version NN.N.NNNN       REQUIRED: Windows version (major.minor.build,
//!                              parsed from `cmd /c ver` by the runner) or
//!                              the literal `unknown` on non-Windows
//! --dry-run                    few blocks, raw-format + generator smoke test
//! ```
//!
//! The FIRST line of the raw JSONL is a JSON identity header written by the
//! judge itself (mode, blocks, seed, order seed, HEAD sha, CPU brand string,
//! OS/arch, source-diff sha, sha256 of its own exe, features, profile,
//! rustc version, invariant TSC, QPC frequency, RT_MIN/RT_MAX and all
//! portion constants). The generator takes identity ONLY from this header
//! and asserts every field matches across the three raws (including blocks).
//!
//! Pre-registered A/A threshold (fixed BEFORE phase 2, see the report):
//! |median A/A| ≤ 1 % AND 95 % CI half-width ≤ 1 % per kernel; a kernel that
//! fails A/A in EITHER mode is not admitted, and the A/B verdict is issued
//! only for admitted kernels.

include!("../_shared/b4_interleave_core.rs");

/// The harness allocator — System with a zero-in-window oracle counter.
#[global_allocator]
static HARNESS: HarnessAlloc = HarnessAlloc;

use std::io::Write;

/// Timed runner for the shared warmup loop (discards the timings; behavior
/// identical to the previous inline `run_portion` calls).
fn run_judge_portion(table: &'static ShimTable, kernel: Kernel, seed: u64) {
    let _ = run_portion(table, kernel, seed);
}

const DEFAULT_BLOCKS: u64 = 20_000;
const DRY_RUN_BLOCKS: u64 = 24;
const DEFAULT_ORDER_SEED: u64 = 0x0B4D_5EED;
// Warmup portions per (arm, kernel): the value lives in the shared core
// (`WARMUP_PORTIONS`, include!-ed above) so the probe can mirror it exactly.

// ── Minimal SHA-256 (FIPS 180-4), for the self-hash of the judge exe. ──────
mod sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    pub fn hex(data: &[u8]) -> String {
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];
        let bitlen = (data.len() as u64).wrapping_mul(8);
        let mut msg = data.to_vec();
        msg.push(0x80);
        while msg.len() % 64 != 56 {
            msg.push(0);
        }
        msg.extend_from_slice(&bitlen.to_be_bytes());
        for chunk in msg.chunks(64) {
            let mut w = [0u32; 64];
            for (i, word) in chunk.chunks(4).enumerate() {
                w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(s1)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(w[i - 2]);
            }
            let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
                (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let t1 = hh
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let t2 = s0.wrapping_add(maj);
                hh = g;
                g = f;
                f = e;
                e = d.wrapping_add(t1);
                d = c;
                c = b;
                b = a;
                a = t1.wrapping_add(t2);
            }
            h[0] = h[0].wrapping_add(a);
            h[1] = h[1].wrapping_add(b);
            h[2] = h[2].wrapping_add(c);
            h[3] = h[3].wrapping_add(d);
            h[4] = h[4].wrapping_add(e);
            h[5] = h[5].wrapping_add(f);
            h[6] = h[6].wrapping_add(g);
            h[7] = h[7].wrapping_add(hh);
        }
        let mut out = String::with_capacity(64);
        for word in h {
            out.push_str(&format!("{word:08x}"));
        }
        out
    }
}

/// CPU brand string: cpuid leaves 0x80000002..=0x80000004 concatenated.
fn cpu_brand() -> String {
    let mut s = String::new();
    for leaf in 0x8000_0002u32..=0x8000_0004 {
        let r = __cpuid_count(leaf, 0);
        for reg in [r.eax, r.ebx, r.ecx, r.edx] {
            for b in reg.to_le_bytes() {
                if b.is_ascii_graphic() || b == b' ' {
                    s.push(b as char);
                }
            }
        }
    }
    s.trim().to_string()
}

/// OS identification: `std::env::consts::OS` + arch. The numeric Windows
/// version comes separately as `--os-version` (parsed from `cmd /c ver` by
/// the runner; `unknown` on non-Windows) — a richer in-judge winver would
/// need FFI for no extra discriminative power on one machine.
fn os_string() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Exact block orders per kernel: N/2 ABBA + N/2 BAAB, Fisher–Yates shuffled
/// with the recorded order seed (strict balance by construction).
fn build_orders(blocks: u64, order_seed: u64) -> Vec<Vec<bool>> {
    let n = blocks as usize;
    let mut rng = XorShift64::new(order_seed);
    ALL_KERNELS
        .into_iter()
        .map(|_| {
            let mut v: Vec<bool> = (0..n).map(|i| i < n / 2).collect();
            for i in (1..v.len()).rev() {
                let j = (rng.below((i + 1) as u64) as usize).min(i);
                v.swap(i, j);
            }
            v
        })
        .collect()
}

fn parse_u64_any(s: &str, what: &str) -> u64 {
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).unwrap_or_else(|e| panic!("bad {what} `{s}`: {e}"))
    } else {
        s.parse()
            .unwrap_or_else(|e| panic!("bad {what} `{s}`: {e}"))
    }
}

fn is_hex40(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

struct Args {
    mode: Mode,
    blocks: u64,
    seed: u64,
    order_seed: u64,
    out: String,
    dry_run: bool,
    head_sha: String,
    source_diff_sha: String,
    features: String,
    profile: String,
    rustc: String,
    os_version: String,
}

fn need(it: &mut impl Iterator<Item = String>, flag: &str) -> String {
    it.next().unwrap_or_else(|| panic!("{flag} needs a value"))
}

fn parse_args() -> Args {
    let mut mode = Mode::Ab;
    let mut blocks = DEFAULT_BLOCKS;
    let mut seed: u64 = 0xB411_0001;
    let mut order_seed: u64 = DEFAULT_ORDER_SEED;
    let mut out = String::from("docs/perf/_raw_b4_interleave.jsonl");
    let mut dry_run = false;
    let mut head_sha = String::new();
    let mut source_diff_sha = String::new();
    let mut features = String::new();
    let mut profile = String::new();
    let mut rustc = String::new();
    let mut os_version = String::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--mode" => {
                mode =
                    Mode::parse(&need(&mut it, "--mode")).unwrap_or_else(|| panic!("unknown mode"))
            }
            "--blocks" => blocks = need(&mut it, "--blocks").parse().expect("blocks"),
            "--seed" => seed = parse_u64_any(&need(&mut it, "--seed"), "--seed"),
            "--order-seed" => {
                order_seed = parse_u64_any(&need(&mut it, "--order-seed"), "--order-seed")
            }
            "--out" => out = need(&mut it, "--out"),
            "--head-sha" => head_sha = need(&mut it, "--head-sha"),
            "--source-diff-sha" => source_diff_sha = need(&mut it, "--source-diff-sha"),
            "--features" => features = need(&mut it, "--features"),
            "--profile" => profile = need(&mut it, "--profile"),
            "--rustc" => rustc = need(&mut it, "--rustc"),
            "--os-version" => os_version = need(&mut it, "--os-version"),
            "--dry-run" => dry_run = true,
            other => panic!("unknown arg {other}"),
        }
    }
    if dry_run {
        blocks = DRY_RUN_BLOCKS;
    }
    // REQUIRED identity arguments — a raw without full identity is invalid.
    assert!(
        is_hex40(&head_sha),
        "--head-sha must be a 40-char lowercase git sha (got `{head_sha}`)"
    );
    assert_eq!(
        source_diff_sha.len(),
        64,
        "--source-diff-sha must be a sha256 hex string"
    );
    assert!(
        !features.is_empty() && !profile.is_empty() && !rustc.is_empty(),
        "--features, --profile and --rustc are REQUIRED (raw identity header)"
    );
    // STRICT profile validation: only the two pre-registered profiles exist.
    assert!(
        profile == "release" || profile == "bench-user",
        "--profile must be `release` or `bench-user` (got `{profile}`)"
    );
    assert!(
        blocks.is_multiple_of(2),
        "--blocks must be EVEN (strict ABBA=BAAB=N/2 balance per kernel), got {blocks}"
    );
    // OS version: the runner parses `cmd /c ver` into `NN.N.NNNN` (Windows)
    // or passes the literal `unknown` (non-Windows; documented choice).
    let os_version_ok = os_version == "unknown" || {
        let p: Vec<&str> = os_version.split('.').collect();
        p.len() == 3
            && p.iter()
                .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    };
    assert!(
        os_version_ok,
        "--os-version must be `unknown` or `NN.N.NNNN`, got `{os_version}`"
    );
    out = out.replace("<mode>", mode.name());
    Args {
        mode,
        blocks,
        seed,
        order_seed,
        out,
        dry_run,
        head_sha,
        source_diff_sha,
        features,
        profile,
        rustc,
        os_version,
    }
}

fn main() {
    let args = parse_args();
    let (a, b) = args.mode.tables();
    let invariant = tsc_invariant();
    let cpu = cpu_brand();
    let os = os_string();
    println!(
        "b4_interleave_judge mode={} blocks={} seed={:#x} order_seed={:#x} dry_run={} tsc_invariant={} head_sha={} cpu=\"{cpu}\" os=\"{os}\"",
        args.mode.name(),
        args.blocks,
        args.seed,
        args.order_seed,
        args.dry_run,
        invariant,
        args.head_sha,
    );
    assert!(invariant, "invariant TSC required for tick-based portions");
    assert!(!cpu.is_empty(), "empty CPU brand string");
    #[cfg(windows)]
    let qpc_freq = qpc::freq();
    #[cfg(not(windows))]
    let qpc_freq = 0i64;
    // sha256 of the judge's own exe (self-identity for the raw header).
    let exe_path = std::env::current_exe().expect("current_exe");
    let exe_bytes =
        std::fs::read(&exe_path).unwrap_or_else(|e| panic!("read exe {}: {e}", exe_path.display()));
    let exe_sha = sha256::hex(&exe_bytes);

    let mut out = std::io::BufWriter::new(
        std::fs::File::create(&args.out).unwrap_or_else(|e| panic!("create {}: {e}", args.out)),
    );
    // ── Identity header: FIRST line of the raw JSONL, written by the judge.
    // Seeds are hex strings (the generator parses them as BigInt).
    writeln!(
        out,
        "{{\"header\":true,\"mode\":\"{}\",\"blocks\":{},\"seed\":\"{:#x}\",\"order_seed\":\"{:#x}\",\
         \"head_sha\":\"{}\",\"cpu\":\"{}\",\"os\":\"{}\",\
         \"source_diff_sha256\":\"{}\",\"exe_sha256\":\"{}\",\"features\":\"{}\",\"profile\":\"{}\",\"rustc\":\"{}\",\"os_version\":\"{}\",\
         \"tsc_invariant\":{},\"qpc_freq\":{},\"pair_ops\":{},\"bulk64_n\":{},\"bulk64_passes\":{},\"bulk16k_n\":{},\"bulk16k_passes\":{},\"churn_live\":{},\"churn_ops\":{},\"churn1024_ops\":{},\"vec_elems\":{},\"vec_cycles\":{},\"larson_live\":{},\"larson_ops\":{},\"larson_min\":{},\"larson_max\":{},\"larson_step\":{},\"rt_min\":{},\"rt_max\":{},\"rt_granule\":{},\"portion_seed_g\":\"{:#x}\",\"larson_align\":{},\"vec_align\":{},\"churn_align\":{},\"pair_align\":{},\"warmup_portions\":{},\"kernels\":{}}}",
        args.mode.name(),
        args.blocks,
        args.seed,
        args.order_seed,
        args.head_sha,
        cpu,
        os,
        args.source_diff_sha,
        exe_sha,
        args.features,
        args.profile,
        args.rustc,
        args.os_version,
        invariant,
        qpc_freq,
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
        LARSON_ALIGN,
        VEC_ALIGN,
        CHURN_ALIGN,
        PAIR_ALIGN,
        WARMUP_PORTIONS,
        ALL_KERNELS.len(),
    )
    .expect("write header");

    // Exact block orders per kernel (strict ABBA = BAAB = N/2 by construction).
    let orders = build_orders(args.blocks, args.order_seed);

    // Warmup: BOTH arms to steady state before any measured block. Warmup
    // itself is outside the window and its portions are not recorded. The
    // seed VARIES per warmup portion: a constant seed would replay the same
    // ring indices and leave most churn/larson slots empty after 32
    // portions. The occupancy simulation runs in lockstep with the real
    // kernels (warmup included, per shim set), so the oracle is exact from
    // block 0.
    let mut st_a = RingState {
        churn: [false; CHURN_LIVE],
        larson: [false; LARSON_LIVE],
    };
    let mut st_b = RingState {
        churn: [false; CHURN_LIVE],
        larson: [false; LARSON_LIVE],
    };
    for k in ALL_KERNELS {
        judge_warmup_kernel(a, b, k, args.seed, &mut st_a, &mut st_b, run_judge_portion);
    }

    let mut block_no: u64 = 0;
    let mut window_alloc_fail: u64 = 0;
    let mut foreign_fail: u64 = 0;
    let mut oracle_fail: u64 = 0;
    // Per-kernel median-portion ns accumulators for the dry-run sizing print.
    let mut ns_samples: Vec<Vec<f64>> = vec![Vec::new(); ALL_KERNELS.len()];

    while block_no < args.blocks {
        for (ki, k) in ALL_KERNELS.into_iter().enumerate() {
            let abba = orders[ki][block_no as usize];
            let portion_seed = judge_block_seed(args.seed, ki as u32, block_no);
            let rec = run_block(a, b, k, block_no, portion_seed, abba);
            if rec.harness_allocs != 0 {
                window_alloc_fail += 1;
            }
            if rec.a1_foreign + rec.b1_foreign + rec.b2_foreign + rec.a2_foreign != 0 {
                foreign_fail += 1;
            }
            // Exact activation oracle: per arm the portion sequence is
            // (seed, seed2) regardless of the interleave order; each of the
            // two shim sets has its OWN occupancy state (correct in A/A too).
            let (exp_a1, exp_a2) = expected_block_calls(&mut st_a, k, portion_seed);
            let (exp_b1, exp_b2) = expected_block_calls(&mut st_b, k, portion_seed);
            for (label, got, expected) in [
                ("a1", rec.a1_calls, exp_a1),
                ("b1", rec.b1_calls, exp_b1),
                ("b2", rec.b2_calls, exp_b2),
                ("a2", rec.a2_calls, exp_a2),
            ] {
                if got != expected {
                    oracle_fail += 1;
                    eprintln!(
                        "[oracle] {} block={} {} calls={} expected={}",
                        k.name(),
                        rec.block,
                        label,
                        got,
                        expected
                    );
                }
            }
            if args.dry_run {
                ns_samples[ki].push(rec.a1_ns);
                ns_samples[ki].push(rec.a2_ns);
            }
            writeln!(
                out,
                "{{\"kernel\":\"{}\",\"mode\":\"{}\",\"order\":\"{}\",\"block\":{},\"seed\":\"{:#x}\",\
                 \"a1\":{},\"b1\":{},\"b2\":{},\"a2\":{},\"a1_ns\":{:.1},\"b1_ns\":{:.1},\
                 \"b2_ns\":{:.1},\"a2_ns\":{:.1},\"harness_allocs\":{},\
                 \"a1_calls\":{},\"b1_calls\":{},\"b2_calls\":{},\"a2_calls\":{},\
                 \"a1_exp\":{},\"b1_exp\":{},\"b2_exp\":{},\"a2_exp\":{},\
                 \"a1_foreign\":{},\"b1_foreign\":{},\"b2_foreign\":{},\"a2_foreign\":{}}}",
                rec.kernel,
                args.mode.name(),
                rec.order,
                rec.block,
                rec.seed,
                rec.a1,
                rec.b1,
                rec.b2,
                rec.a2,
                rec.a1_ns,
                rec.b1_ns,
                rec.b2_ns,
                rec.a2_ns,
                rec.harness_allocs,
                rec.a1_calls,
                rec.b1_calls,
                rec.b2_calls,
                rec.a2_calls,
                exp_a1,
                exp_b1,
                exp_b2,
                exp_a2,
                rec.a1_foreign,
                rec.b1_foreign,
                rec.b2_foreign,
                rec.a2_foreign,
            )
            .expect("write raw");
        }
        block_no += 1;
    }
    out.flush().expect("flush raw");

    // ── Activation / window oracles ───────────────────────────────────────
    // 1. Window harness-allocation oracle: every block must have driven ZERO
    //    harness allocations.
    assert_eq!(window_alloc_fail, 0, "harness allocated inside the window");
    // 2. Foreign-window oracle: no portion ever touched a foreign shim set
    //    (run_portion panics on it; this is the counted cross-check).
    assert_eq!(foreign_fail, 0, "foreign shim calls inside portion windows");
    // 3. Activation oracle: 100 % of portions must match the simulated
    //    expectation exactly — NO slack, any mismatch aborts.
    assert_eq!(
        oracle_fail, 0,
        "activation oracle FAILED: {oracle_fail} portions with a wrong shim-call count"
    );

    let total = args.blocks * ALL_KERNELS.len() as u64;
    let (sa, sb) = match args.mode {
        Mode::Ab => (0, 2),
        Mode::AaSefer => (0, 1),
        Mode::AaMi => (2, 3),
    };
    let ca = SHIM_CALLS[sa].load(Ordering::Relaxed);
    let cb = SHIM_CALLS[sb].load(Ordering::Relaxed);
    println!(
        "RESULT blocks={} orders_abba={} orders_baab={} harness_window_allocs=0 foreign_window_calls=0 oracle_mismatches=0 shim_calls_a={} shim_calls_b={}",
        total,
        args.blocks / 2 * ALL_KERNELS.len() as u64,
        args.blocks / 2 * ALL_KERNELS.len() as u64,
        ca,
        cb
    );

    if args.dry_run {
        // Portion-sizing helper: median portion ns per kernel (dry-run only;
        // the chosen portion constants stay the pre-registered ones).
        for (ki, k) in ALL_KERNELS.into_iter().enumerate() {
            let mut s = ns_samples[ki].clone();
            s.retain(|v| *v > 0.0);
            s.sort_by(|x, y| x.partial_cmp(y).unwrap());
            let med = if s.is_empty() { 0.0 } else { s[s.len() / 2] };
            println!("PORTION {} median_ns={:.0}", k.name(), med);
        }
        println!("DRY_RUN_OK out={}", args.out);
    }

    out.flush().expect("flush raw");
    // Explicit teardown: every live block of every arm freed with its OWN
    // recorded layout, before the process exits.
    teardown_arms([&SEFER_A_TABLE, &SEFER_B_TABLE, &MI_A_TABLE, &MI_B_TABLE]);
}
