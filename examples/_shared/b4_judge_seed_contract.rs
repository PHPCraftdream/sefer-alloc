// Shared include!-module: the judge seed/warmup contract.
// Part of `b4_interleave_core.rs` (include!-ed in place; no logic beyond
// the contract itself). Included from `examples/b4_interleave_judge/main.rs`,
// `examples/gp_kernel_probe.rs` and `tests/b4_interleave_judge_correctness.rs`
// via the core.

/// Warmup portions per (arm, kernel) before the window (the judge's
/// steady-state warmup contract; the probe mirrors it exactly).
#[allow(dead_code)] // shared context: not every include! site reads the const
pub const WARMUP_PORTIONS: u64 = 32;

/// Judge-exact warmup seed for warmup-portion `w` (the judge's
/// `seed.wrapping_add(w.wrapping_mul(PORTION_SEED_G))`).
#[allow(dead_code)] // shared context: not every include! site calls every fn
#[must_use]
pub fn judge_warmup_seed(seed: u64, w: u64) -> u64 {
    seed.wrapping_add(w.wrapping_mul(PORTION_SEED_G))
}

/// Judge-exact block portion seed: `ki` is the kernel's index in
/// `ALL_KERNELS` (`seed + block_no*0x1000_0000_0000_0001` rotated left by
/// `ki`).
#[allow(dead_code)] // shared context: not every include! site calls every fn
#[must_use]
pub fn judge_block_seed(seed: u64, ki: u32, block_no: u64) -> u64 {
    seed.wrapping_add(block_no.wrapping_mul(0x1000_0000_0000_0001))
        .rotate_left(ki)
}

/// Judge-exact warmup loop for ONE kernel (semantics of the judge's warmup
/// block, with the runner injected): for each of `WARMUP_PORTIONS` warmup
/// portions run BOTH arms with seed [`judge_warmup_seed`] and step each
/// arm's occupancy simulation in lockstep — bit-exact with the judge, whose
/// `run` is the timed `run_portion` (the probe passes a no-timer runner on
/// `portion_body`).
pub fn judge_warmup_kernel(
    a: &'static ShimTable,
    b: &'static ShimTable,
    k: Kernel,
    seed: u64,
    st_a: &mut RingState,
    st_b: &mut RingState,
    run: fn(&'static ShimTable, Kernel, u64),
) {
    for w in 0..WARMUP_PORTIONS {
        let ws = judge_warmup_seed(seed, w);
        run(a, k, ws);
        let _ = simulate_portion_calls(st_a, k, ws);
        run(b, k, ws);
        let _ = simulate_portion_calls(st_b, k, ws);
    }
}
