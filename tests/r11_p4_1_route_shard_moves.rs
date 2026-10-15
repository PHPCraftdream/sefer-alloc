#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]
//! R11 P4-1 witness: pointer cells moved by one-shard route churn, counted
//! (not timed). Page-aligned Large roots share one shard; the directory only
//! uses their addresses, never their bytes.

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};

const SEGMENT: usize = 4 * 1024 * 1024;
const BASE_SEGMENT: usize = 1 << 16;

fn shard_keys(n: usize) -> Vec<usize> {
    (BASE_SEGMENT..)
        .map(|v| v * SEGMENT)
        .filter(|&k| RouteDirectory::shard_index_for_test(k) == 0)
        .take(n)
        .collect()
}

/// Returns (register moves, remove moves) for descending register and
/// ascending removal of `n` same-shard Large routes.
fn wave(n: usize) -> (u64, u64) {
    let directory = RouteDirectory::new();
    let keys = shard_keys(n);
    let mut routes = Vec::with_capacity(n);
    for &key in keys.iter().rev() {
        let root = core::ptr::without_provenance_mut::<u8>(key);
        routes.push(
            directory
                .register(root, SEGMENT, root, 1, RouteKind::Large)
                .unwrap(),
        );
    }
    let register_moves = directory.moved_pointer_cells_for_test();
    for &key in &keys {
        let root = core::ptr::without_provenance_mut::<u8>(key);
        assert!(directory.lookup(root).is_some());
    }
    routes.reverse();
    for route in routes {
        drop(route);
    }
    let total = directory.moved_pointer_cells_for_test();
    (register_moves, total - register_moves)
}

/// Moved cells per update must stay bounded by a block-sized constant
/// (quadratic shifting is n/2 per update: 128 at n=256, 512 at n=1024), and
/// quadrupling n must not grow the total anywhere near 16x.
#[test]
fn one_shard_reverse_wave_moves_subquadratic_cells() {
    const MAX_PER_UPDATE: u64 = 96;
    let mut rows = Vec::new();
    for n in [64u64, 256, 1024] {
        let (reg, rem) = wave(n as usize);
        println!(
            "n={n} register_moves={reg} ({}/update) remove_moves={rem} ({}/update) quadratic n(n-1)/2={}",
            reg / n,
            rem / n,
            n * (n - 1) / 2
        );
        assert!(
            reg <= n * MAX_PER_UPDATE,
            "n={n}: register {reg} moves / {n} updates"
        );
        assert!(
            rem <= n * MAX_PER_UPDATE,
            "n={n}: remove {rem} moves / {n} updates"
        );
        rows.push((reg, rem));
    }
    assert!(
        rows[2].0 < rows[1].0 * 8,
        "register growth 256->1024: {} / {}",
        rows[2].0,
        rows[1].0
    );
    assert!(
        rows[2].1 < rows[1].1 * 8,
        "remove growth 256->1024: {} / {}",
        rows[2].1,
        rows[1].1
    );
}
