//! Size-distribution knobs for the op-stream generators.

/// Size-distribution knobs for the op-stream generators.
///
/// Field consumers, exactly: `drive` reads only `double_free`; the proptest
/// front-end (`op_strategy`) reads all five generator knobs (`small_max`,
/// `large_max`, `small_weight`, `large_weight`, `max_align`); the arbitrary
/// front-end reads `small_max`, `large_max`, `small_weight`, `large_weight`,
/// `max_align` via `OpStream::arbitrary_with_config`, while plain
/// `Arbitrary for OpStream` (what `fuzz_target!` drives) uses
/// `Config::default()`'s values.
///
/// The defaults span a small hot-path range plus a rare, capped large arm — the
/// shape every in-tree copy used. Tune per allocator-under-test (e.g. set
/// `large_max` above the allocator's small-class ceiling to exercise its
/// dedicated-large path).
///
/// Exhaustiveness note: `Config` (like `Op`) is deliberately an exhaustive
/// public type with public fields so `Config { double_free: true,
/// ..Config::default() }` stays ergonomic; adding a field or variant is
/// therefore a breaking change and bumps the minor version under 0.x.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// Upper bound (inclusive) of the small size arm.
    pub small_max: usize,
    /// Upper bound (inclusive) of the large size arm.
    pub large_max: usize,
    /// Relative weight of the small arm in the proptest size strategy.
    pub small_weight: u32,
    /// Relative weight of the large arm in the proptest size strategy.
    pub large_weight: u32,
    /// Maximum alignment (a power of two) offered by the align strategy.
    ///
    /// Precondition: a non-zero power of two (the align strategy
    /// debug-asserts it); both front-ends clamp generated aligns to it.
    pub max_align: usize,
    /// Whether to exercise the **M2 double-free-is-no-op oracle**: after each
    /// `Dealloc`, free the SAME pointer a second time.
    ///
    /// This is a *stronger-than-`GlobalAlloc`* guarantee — a real system malloc
    /// treats a double-free as undefined behaviour (heap corruption), so this is
    /// **off by default**. Turn it ON only for an allocator whose contract is
    /// that a redundant `dealloc` of an already-freed pointer is a safe no-op
    /// (e.g. sefer's `AllocCore`). The oracle cannot itself observe "no-op"
    /// directly; its guard is that the allocator must not corrupt — a later
    /// op/teardown that would then touch corrupted state is what catches it.
    pub double_free: bool,
}

impl Default for Config {
    fn default() -> Self {
        // Matches the historical `heap_differential` shape: 9:1 small:large,
        // small <= 4 KiB, large capped at 128 KiB, aligns up to 4096. The M2
        // double-free is OFF by default (unsafe against a real malloc); consumers
        // whose allocator tolerates it opt in via `double_free`.
        Config {
            small_max: 4096,
            large_max: 128 * 1024,
            small_weight: 9,
            large_weight: 1,
            max_align: 4096,
            double_free: false,
        }
    }
}
