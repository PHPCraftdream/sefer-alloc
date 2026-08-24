//! Reverse-index construction/lookup tests for `numa_shim::cpumap` (task #1310).
//!
//! This module exercises the pure reverse-index logic directly on every host
//! (same cross-platform pattern as `tests/cpumap_parser.rs` — the crate is
//! developed/tested on Windows here; no real `/sys` reads). These tests cover
//! the review findings F5 and F10 from the fifteenth audit
//! (`docs/reviews/2026-08-24-170047-numa-shim-publication-audit-Sol-codex.md`):
//! the reverse index replaces the per-node raw-text cache that had O(nodes ×
//! bytes) lookup cost and a 1024-byte per-node buffer that silently dropped
//! nodes with wide global CPU-ID space.

use numa_shim::cpumap::{parse_contains_cpu, ReverseIndex, MAX_INDEXED_CPUS};

/// Build a sysfs-shaped cpumap text for `cpus`: word_count covers the highest
/// cpu; most-significant word FIRST; 8 lowercase hex digits per word,
/// comma-separated; trailing newline — mirrors `cpumap_print_bitmask_to_buf`.
///
/// Word for word w covers CPUs w*32..w*32+31; leftmost token is the highest word.
fn cpumap_text(cpus: &[u32]) -> Vec<u8> {
    if cpus.is_empty() {
        return b"00000000\n".to_vec();
    }

    let max_cpu = *cpus.iter().max().unwrap();
    let word_count = (max_cpu / 32) + 1;

    let mut words = vec![0u32; word_count as usize];
    for &cpu in cpus {
        let word_idx = (cpu / 32) as usize;
        let bit = cpu % 32;
        words[word_idx] |= 1u32 << bit;
    }

    // Emit most-significant word FIRST
    let mut result = Vec::new();
    for (i, &word) in words.iter().enumerate().rev() {
        if i != words.len() - 1 {
            result.push(b',');
        }
        result.extend(format!("{:08x}", word).as_bytes());
    }
    result.push(b'\n');
    result
}

/// The exact F5 scenario: sparse global CPU IDs spanning multiple nodes.
///
/// Node 0 = [0,1,2,3, 4096,4097,4098,4099]
/// Node 1 = [100, 200]
/// Node 2 = [8191]
///
/// This test verifies:
/// 1. The OLD design's 1024-byte per-node buffer would have silently DROPPED
///    node 0 because its text length > 1024 bytes (the file is a global cpumask
///    so its width tracks global CPU-ID space, not per-node CPU count).
/// 2. The text contains a comma (multi-word, MSB-first order exercised).
/// 3. All lookups return the correct node IDs.
#[test]
fn multi_node_sparse_global_ids_map_correctly() {
    let mut index = ReverseIndex::new();

    // Node 0: CPUs 0-3 and 4096-4099 (sparse global IDs)
    let node0_text = cpumap_text(&[0, 1, 2, 3, 4096, 4097, 4098, 4099]);
    assert!(
        node0_text.len() > 1024,
        "F5 defect: OLD 1024-byte buffer would DROP this node"
    );
    assert!(
        node0_text.contains(&b','),
        "multi-word mask must contain comma"
    );

    // Node 1: CPUs 100 and 200
    let node1_text = cpumap_text(&[100, 200]);

    // Node 2: CPU 8191 (at capacity boundary)
    let node2_text = cpumap_text(&[8191]);

    // Index in ascending node order (mirroring the real topology() caller)
    assert!(
        index.index_node(0, &node0_text),
        "node 0 should index successfully"
    );
    assert!(
        index.index_node(1, &node1_text),
        "node 1 should index successfully"
    );
    assert!(
        index.index_node(2, &node2_text),
        "node 2 should index successfully"
    );

    // Verify lookups for node 0
    assert_eq!(index.lookup(0), Some(0), "CPU 0 -> node 0");
    assert_eq!(index.lookup(3), Some(0), "CPU 3 -> node 0");
    assert_eq!(index.lookup(4096), Some(0), "CPU 4096 -> node 0");
    assert_eq!(index.lookup(4099), Some(0), "CPU 4099 -> node 0");
    assert_eq!(index.lookup(4095), None, "CPU 4095 -> unmapped");
    assert_eq!(index.lookup(4100), None, "CPU 4100 -> unmapped");

    // Verify lookups for node 1
    assert_eq!(index.lookup(100), Some(1), "CPU 100 -> node 1");
    assert_eq!(index.lookup(200), Some(1), "CPU 200 -> node 1");
    assert_eq!(index.lookup(150), None, "CPU 150 -> unmapped");

    // Verify lookups for node 2
    assert_eq!(index.lookup(8191), Some(2), "CPU 8191 -> node 2");
    assert_eq!(index.lookup(8190), None, "CPU 8190 -> unmapped");
}

/// CPUs beyond capacity degrade gracefully.
///
/// Node 0 text = [10, 8192, 9000]
/// - CPU 10 is within capacity and should map to node 0
/// - CPU 8192 and 9000 are beyond MAX_INDEXED_CPUS and should return None
///
/// None is exactly what `cpu_to_numa_node_checked` returns for unmapped CPUs,
/// which `current_node_resolution()` maps to `FellBackToZero` and `current_node()`
/// to `Some(0)` — the same silent fallback the old design produced for oversized
/// files (degradation semantics preserved).
#[test]
fn cpu_beyond_capacity_degrades_like_old_buffer_too_small() {
    let mut index = ReverseIndex::new();
    let text = cpumap_text(&[10, 8192, 9000]);

    assert!(
        index.index_node(0, &text),
        "index_node returns true even if some CPUs beyond capacity"
    );

    assert_eq!(index.lookup(10), Some(0), "CPU 10 -> node 0");
    assert_eq!(
        index.lookup(8192),
        None,
        "CPU 8192 -> beyond capacity -> None"
    );
    assert_eq!(
        index.lookup(9000),
        None,
        "CPU 9000 -> beyond capacity -> None"
    );
    assert_eq!(
        index.lookup(u32::MAX),
        None,
        "CPU u32::MAX -> beyond capacity -> None"
    );
}

/// Capacity boundary and alignment checks.
///
/// Tests:
/// 1. Fresh index: lookup(0) -> None (the no-sysfs single-node case stays unmapped)
/// 2. Node holding [8191]: MAX_INDEXED_CPUS-1 -> Some, MAX_INDEXED_CPUS -> None
/// 3. MAX_INDEXED_CPUS % 32 == 0 (word-aligned capacity)
#[test]
fn capacity_boundary_and_alignment() {
    // Fresh index: nothing mapped
    let index = ReverseIndex::new();
    assert_eq!(
        index.lookup(0),
        None,
        "fresh index: CPU 0 unmapped (no-sysfs case)"
    );

    // Index with CPU 8191 (last valid CPU)
    let mut index = ReverseIndex::new();
    let text = cpumap_text(&[8191]);
    assert!(index.index_node(0, &text));

    assert_eq!(
        index.lookup(MAX_INDEXED_CPUS as u32 - 1),
        Some(0),
        "CPU 8191 (MAX_INDEXED_CPUS-1) -> Some"
    );
    assert_eq!(
        index.lookup(MAX_INDEXED_CPUS as u32),
        None,
        "CPU 8192 (MAX_INDEXED_CPUS) -> None (beyond capacity)"
    );

    assert_eq!(MAX_INDEXED_CPUS % 32, 0, "capacity must be word-aligned");
}

/// Overlapping masks: lowest node wins when indexing in ascending order.
///
/// CPU 5 appears in both node 2's and node 5's texts. When indexing in
/// ascending order (0, 1, 2, 3, 4, 5...), node 2 wins.
///
/// In a SEPARATE index, indexing node 5 then node 2 still gives Some(5),
/// documenting the first-mapping-wins insertion semantics. The real caller's
/// ascending order makes the two coincide.
#[test]
fn overlapping_masks_lowest_node_wins() {
    // Test 1: ascending-order indexing -> node 2 wins for CPU 5
    let mut index = ReverseIndex::new();

    // Node 2 claims CPU 5
    let node2_text = cpumap_text(&[5]);
    assert!(index.index_node(2, &node2_text));

    // Node 5 also claims CPU 5 (but node 2 already indexed it)
    let node5_text = cpumap_text(&[5]);
    assert!(index.index_node(5, &node5_text));

    assert_eq!(
        index.lookup(5),
        Some(2),
        "CPU 5 -> node 2 (lowest node wins in ascending order)"
    );

    // Test 2: reverse-order indexing in a fresh index -> node 5 wins
    // This documents first-mapping-wins semantics
    let mut index2 = ReverseIndex::new();

    // Node 5 indexed first
    assert!(index2.index_node(5, &node5_text));

    // Node 2 indexed second (but CPU 5 already mapped to node 5)
    assert!(index2.index_node(2, &node2_text));

    assert_eq!(
        index2.lookup(5),
        Some(5),
        "CPU 5 -> node 5 (first mapping wins when indexed out of order)"
    );
}

/// Malformed text is fail-closed per node.
///
/// Tests:
/// 1. `index_node(0, b"0000000g,00000001\n")` returns false AND lookup(0) -> None
///    (no partial commit: the valid rightmost word's set bit was NOT indexed)
/// 2. Empty input b"" and b"\n" -> false, nothing indexed
/// 3. Direct probe test for Fix 1: `parse_contains_cpu(b"0000000g,00000001\n", 0)`
///    must be false (malformed token anywhere fails the probe — the exact case
///    the pre-fix implementation got wrong; this pins the `ok && found` fix)
#[test]
fn malformed_text_is_fail_closed_per_node() {
    let mut index = ReverseIndex::new();

    // Test 1: malformed token in leftmost word
    // The rightmost word "00000001" has CPU 0 set, but the leftmost word "0000000g"
    // has an invalid hex digit 'g', so the entire parse should fail.
    let malformed = b"0000000g,00000001\n";
    assert!(
        !index.index_node(0, malformed),
        "malformed text should return false"
    );
    assert_eq!(
        index.lookup(0),
        None,
        "CPU 0 should NOT be indexed (partial commit rejected)"
    );

    // Test 2: empty inputs
    assert!(!index.index_node(1, b""), "empty input should return false");
    assert!(
        !index.index_node(2, b"\n"),
        "only newline should return false"
    );
    assert_eq!(index.lookup(1), None, "CPU 1 unmapped after empty input");
    assert_eq!(
        index.lookup(2),
        None,
        "CPU 2 unmapped after newline-only input"
    );

    // Test 3: direct probe test for Fix 1
    // Before the fix, this would return true because the closure set found=true
    // on the valid rightmost word before the malformed leftmost word aborted.
    // After the fix, this returns false because ok && found evaluates to false.
    assert!(
        !parse_contains_cpu(b"0000000g,00000001\n", 0),
        "malformed token anywhere must fail the probe (Fix 1: ok && found)"
    );
}

/// Non-divergence oracle: parse_contains_cpu agrees with reverse index within capacity.
///
/// For each node text from test 1 (with its node id), verify that for all tested
/// CPUs, `parse_contains_cpu(&text, c) == (index.lookup(c) == Some(node))`.
///
/// This ensures the reverse index doesn't diverge from the single interpreter
/// for all CPUs within MAX_INDEXED_CPUS.
#[test]
fn probe_agrees_with_reverse_index_within_capacity() {
    let mut index = ReverseIndex::new();

    // Build the same topology as test 1
    let node0_text = cpumap_text(&[0, 1, 2, 3, 4096, 4097, 4098, 4099]);
    let node1_text = cpumap_text(&[100, 200]);
    let node2_text = cpumap_text(&[8191]);

    index.index_node(0, &node0_text);
    index.index_node(1, &node1_text);
    index.index_node(2, &node2_text);

    // Test CPU 0..=5
    for cpu in 0..=5 {
        let in_node0 = parse_contains_cpu(&node0_text, cpu);
        let index_result = index.lookup(cpu) == Some(0);
        assert_eq!(
            in_node0, index_result,
            "CPU {}: probe and index must agree",
            cpu
        );
    }

    // Test CPU 99..=101
    for cpu in 99..=101 {
        let in_node1 = parse_contains_cpu(&node1_text, cpu);
        let index_result = index.lookup(cpu) == Some(1);
        assert_eq!(
            in_node1, index_result,
            "CPU {}: probe and index must agree",
            cpu
        );
    }

    // Test CPU 199..=201
    for cpu in 199..=201 {
        let in_node1 = parse_contains_cpu(&node1_text, cpu);
        let index_result = index.lookup(cpu) == Some(1);
        assert_eq!(
            in_node1, index_result,
            "CPU {}: probe and index must agree",
            cpu
        );
    }

    // Test CPU 4095..=4100
    for cpu in 4095..=4100 {
        let in_node0 = parse_contains_cpu(&node0_text, cpu);
        let index_result = index.lookup(cpu) == Some(0);
        assert_eq!(
            in_node0, index_result,
            "CPU {}: probe and index must agree",
            cpu
        );
    }

    // Test CPU 8190..=8191 (at capacity boundary)
    for cpu in 8190..=8191 {
        let in_node2 = parse_contains_cpu(&node2_text, cpu);
        let index_result = index.lookup(cpu) == Some(2);
        assert_eq!(
            in_node2, index_result,
            "CPU {}: probe and index must agree",
            cpu
        );
    }
}
