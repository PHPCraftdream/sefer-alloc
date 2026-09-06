//! Size-distribution knobs for the op-stream generators.

/// Size-distribution knobs for the op-stream generators.
///
/// Field consumers, exactly: `drive` reads only `double_free`; the proptest
/// front-end (`op_strategy`) reads all five generator knobs (`small_max`,
/// `large_max`, `small_weight`, `large_weight`, `max_align`); the arbitrary
/// front-end reads `small_max`, `large_max`, `small_weight`, `large_weight`,
/// `max_align` via `OpStream::arbitrary_with_config`; plain
/// `Arbitrary for OpStream` delegates to that constructor with
/// `Config::default()`'s values, and so does any front-end with its own
/// shaping needs (the in-tree `global_alloc_ops` fuzz target passes an
/// explicit `Config` restoring its historical 2 MiB reach).
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Upper bound (inclusive) of the small size arm.
    ///
    /// Degenerate value `0` is NOT a precondition violation: both front-ends
    /// clamp it identically to a small arm of exactly size 1.
    pub small_max: usize,
    /// Upper bound (inclusive) of the large size arm.
    ///
    /// Degenerate value `<= small_max` is NOT a precondition violation: both
    /// front-ends clamp it identically to a large arm of exactly
    /// `small_max + 1`.
    pub large_max: usize,
    /// Relative weight of the small arm in the generators' size choice.
    pub small_weight: u32,
    /// Relative weight of the large arm in the generators' size choice.
    ///
    /// Precondition (checked by [`Config::validate`], which both front-ends
    /// call): `small_weight + large_weight >= 1` — an all-zero weight sum
    /// would make the generators' weighted pick undefined, and each
    /// front-end reinterprets it differently.
    pub large_weight: u32,
    /// Maximum alignment (a power of two) offered by the align strategy.
    ///
    /// Precondition (checked by [`Config::validate`], which both front-ends
    /// call): a non-zero power of two `<= isize::MAX`. Anything else is not
    /// merely clamped — each front-end silently reinterprets it DIFFERENTLY
    /// (e.g. `max_align: 3000` caps aligns at 2048 in the proptest
    /// front-end but at 8 in the arbitrary one, and `usize::MAX`, the
    /// natural spelling of "no limit", pins every generated align to 1).
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

impl Config {
    /// Reject, with a panic naming the offending field, a config whose
    /// generator preconditions (see the field docs) do not hold. Both
    /// front-ends call this up front, so a degenerate config is rejected
    /// identically — before any op is generated — instead of being silently
    /// reinterpreted differently by each generator.
    ///
    /// Deliberately NOT checked here (both front-ends already clamp these
    /// identically by construction, so they are degenerate behavior, not a
    /// disagreement): `small_max == 0` and `large_max <= small_max`.
    ///
    /// # Panics
    ///
    /// - `max_align` is zero, not a power of two, or greater than
    ///   `isize::MAX` (an align beyond that can never be admitted by
    ///   `Layout::from_size_align`, which is also the ceiling `drive`
    ///   clamps sizes to).
    /// - `small_weight` and `large_weight` are both zero (the weighted
    ///   pick over the two arms would be undefined).
    pub fn validate(&self) {
        assert!(
            self.max_align.is_power_of_two() && self.max_align <= isize::MAX as usize,
            "Config::max_align must be a non-zero power of two <= isize::MAX, got {}",
            self.max_align
        );
        assert!(
            self.small_weight.saturating_add(self.large_weight) > 0,
            "Config::small_weight and Config::large_weight must not both be zero, \
             got {} and {}",
            self.small_weight,
            self.large_weight
        );
    }
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
