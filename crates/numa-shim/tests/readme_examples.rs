//! Compile-and-run oracle for the README's `vmem-integration` example.
//!
//! task #1340 (finding P3-4 of the nineteenth independent review,
//! `docs/reviews/2026-08-25-022026-numa-shim-publication-audit-oh.md`):
//! the README's code examples were compiled by nothing — no
//! `include_str!` into the crate, no `tests/readme_*.rs`, no doc-drift
//! guard (unlike the sibling `aligned-vmem` crate's
//! `scripts/vmem-doc-drift-guard.mjs`) — and the `vmem-integration`
//! example, this crate's most-copied artifact, had already been broken
//! once before (F6/task #1268: `aligned_vmem::PAGE` was not reachable
//! from a downstream consumer) with nothing added to prevent recurrence.
//!
//! This file carries the example as real compiled Rust and RUNS it. The
//! run is cheap and process-local on every backend the suite executes
//! under: two 16-page reservations, each dropped before the next is
//! made, via either the NUMA-preferred path (real Linux x86_64/aarch64,
//! real Windows, or the mock's recorded dispatch) or the example's
//! documented `None` fallback (`aligned_vmem::reserve_aligned`).
//! Compilation alone is already the guard: building this test proves
//! every item the snippet names — `numa_shim::{current_node,
//! reserve_preferred_on_node, NodeId}` and `aligned_vmem::{page_size,
//! PAGE}` plus the `aligned_vmem::reserve_aligned` path — is reachable
//! exactly as written by a downstream consumer, which is what broke in
//! task #1268.
//!
//! Transcribing the example for this oracle caught two latent
//! warning-level defects in the snippet itself, both fixed in the README
//! in the same task (each would have been a hard error for a downstream
//! consumer building with `-D warnings` — exactly what the absent
//! compile check had been hiding): the `use` line imported
//! `ReserveNumaError`, which the snippet never names, and the first
//! `let r = ...` binding was shadowed without ever being read (rustc's
//! `unused_variables` fires per binding; verified with a standalone
//! `rustc --edition 2021 -D warnings` probe before writing this). The
//! example now ends each of its two demonstration blocks with an
//! explicit `drop(r);`.
//!
//! ## Compilation gating
//!
//! The whole file is `#![cfg(feature = "vmem-integration")]`-gated (the
//! `tests/policy_oracle_linux.rs` pattern): without the feature there is
//! no `reserve_preferred_on_node` to demonstrate and the example cannot
//! compile. Unlike that file there is no platform/arch clause — every
//! backend the test suite runs on either resolves a node (real or mock)
//! and performs a preferred reservation, or takes the documented `None`
//! fallback branch. The one combination that would panic inside the
//! example is a REAL Linux backend on a non-x86_64/aarch64 arch with
//! resolvable topology (`Err(UnsupportedArchitecture)` reaches the
//! example's `.expect`); no CI row builds that combination for this
//! crate (the same situation `tests/smoke.rs` lived in before task
//! #1337 made its own predicates arch-aware), and it is named here so it
//! stays a known property rather than a surprise.

#![cfg(feature = "vmem-integration")]

/// Runs the README's `vmem-integration` example — same statements and
/// comments as the README's snippet, modulo rustfmt's canonical
/// indentation inside this fn. Editing the README example without
/// updating this copy silently stops guarding it: this copy, not the
/// README, is what the compiler checks.
#[test]
fn readme_vmem_integration_example_compiles_and_runs() {
    // === BEGIN: README.md "### `vmem-integration`" example ===
    use aligned_vmem::{page_size, PAGE};
    use numa_shim::{current_node, reserve_preferred_on_node, NodeId};

    // Reserve fresh memory with a NUMA preference installed BEFORE the first
    // page fault — the only point where a preference can be in place before
    // any page is touched (still SOFT: "installed," not "placement
    // guaranteed"). `NodeId::new` rejects only the `NO_NODE` sentinel
    // (u32::MAX): an id the platform cannot address still constructs and
    // surfaces as `Err(ReserveNumaError::InvalidNode)` (Linux nodemask
    // limit) or `Err(ReserveNumaError::Os(..))` (Windows forwards any id to
    // the OS). `None` from `current_node()` means undetermined topology ->
    // no NUMA preference (task #1308; `Some(0)` only ever means
    // genuinely-resolved node 0).
    let ps = page_size();
    let r = match current_node() {
        Some(node) => {
            // current_node() remaps the sentinel to None, so `node` here is
            // never NO_NODE and NodeId::new cannot fail.
            reserve_preferred_on_node(
                ps * 16,
                PAGE.max(ps),
                NodeId::new(node).expect("never NO_NODE"),
            )
            .expect("NUMA-preferred reservation failed")
        }
        None => {
            // No NUMA preference — plain aligned reservation.
            aligned_vmem::reserve_aligned(ps * 16, PAGE.max(ps)).expect("OOM")
        }
    };
    drop(r);

    // Best-effort fallback with more detailed error handling:
    let r = match current_node() {
        Some(node) => {
            // As above: `node` is never the sentinel here.
            match reserve_preferred_on_node(
                ps * 16,
                PAGE.max(ps),
                NodeId::new(node).expect("never NO_NODE"),
            ) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "NUMA preference on node {} failed ({}); using an unbound reservation",
                        node, e
                    );
                    aligned_vmem::reserve_aligned(ps * 16, PAGE.max(ps)).expect("OOM")
                }
            }
        }
        None => aligned_vmem::reserve_aligned(ps * 16, PAGE.max(ps)).expect("OOM"),
    };
    drop(r);
    // === END: README.md "### `vmem-integration`" example ===
}
