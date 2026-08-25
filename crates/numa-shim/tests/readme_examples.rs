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
//! ## task #1341: the `src/lib.rs` doc snippets (twentieth review F1)
//!
//! The twentieth independent review
//! (`docs/reviews/2026-08-25-021741-numa-shim-publication-audit-run-17-Sol-codex.md`,
//! finding F1 — its single blocking P2) found that two public rustdoc
//! examples in `src/lib.rs` did not compile, invisible to every gate
//! because this repo's no-doctest convention keeps them in ` ```text `
//! fences rustdoc never type-checks: `NodeId::new`'s "Ergonomic path
//! from detection" match mixed a `Result` arm with an `Option` arm, and
//! `reserve_preferred_on_node`'s "Best-effort fallback" snippet called
//! `Result::or_else` with a closure returning `Option`. Both snippets
//! are now type-correct in the source, and this file remains the
//! compile oracle: the tests below carry each corrected snippet as real
//! compiled-and-run Rust, so a future type error in a ` ```text `
//! example fails CI here instead of hiding behind the fence (the same
//! guard this file established for the README example in task #1340).
//! The crate-level `//! ## Usage` snippet — whose unused `NO_NODE`
//! import was F1's third sub-finding — is carried here too: cheap
//! insurance, since its only compile risk was exactly that unused
//! import, and it pins the snippet's import list against `-D warnings`.
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
//! As of task #1341 the same caveat extends to this file's
//! `node_id_new_ergonomic_path_snippet_compiles_and_runs` (it too
//! `.expect`s the preferred reservation on the `Some` arm); the other
//! two #1341 tests cannot hit it — the usage snippet performs no
//! reservation, and the best-effort snippet's `.ok()` routes an
//! `Err(UnsupportedArchitecture)` into the fallback reservation, which
//! is exactly the path that snippet exists to demonstrate.

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

/// Runs the crate-level `//! ## Usage` snippet from `src/lib.rs` —
/// task #1341 (twentieth review F1): the snippet's `use` line imported
/// `NO_NODE`, which the body never names — a warning-level defect
/// (hard error under a downstream `-D warnings` build) that a
/// ` ```text ` fence can never catch, plus a `None`-arm message that
/// mis-stated `None` as covering single-node hosts (a correctly
/// resolved single-node host returns `Some(0)` since task #1308).
/// Statements are the corrected snippet verbatim, modulo rustfmt's
/// canonical formatting inside this fn.
#[test]
fn crate_usage_snippet_compiles_and_runs() {
    // === BEGIN: src/lib.rs crate-doc "## Usage" snippet ===
    use numa_shim::current_node;

    match current_node() {
        Some(node) => println!("Running on NUMA node {node}"),
        None => println!("NUMA topology unavailable (detection failed or unsupported platform)"),
    }
    // === END: src/lib.rs crate-doc "## Usage" snippet ===
}

/// Runs `NodeId::new`'s "# Ergonomic path from detection" snippet from
/// `src/lib.rs` — task #1341 (twentieth review F1): the snippet's match
/// had one arm returning `reserve_preferred_on_node(...)`'s
/// `Result<Reservation, ReserveNumaError>` and the other returning
/// `aligned_vmem::reserve_aligned(...)`'s `Option<Reservation>` —
/// different types, so the match as documented could never compile.
/// Both arms now yield `Reservation` via `.expect`, the resolution the
/// README's `vmem-integration` example (oracled by the test above)
/// already established. Statements are the corrected snippet verbatim;
/// the snippet's placeholder `size`/`align` are bound to concrete
/// values the same way the README example binds them.
#[test]
fn node_id_new_ergonomic_path_snippet_compiles_and_runs() {
    // === BEGIN: src/lib.rs NodeId::new "# Ergonomic path from detection" snippet ===
    use aligned_vmem::{page_size, PAGE};
    use numa_shim::NodeId;

    let ps = page_size();
    let size = ps * 16;
    let align = PAGE.max(ps);
    let r = match numa_shim::current_node() {
        // current_node() never yields the NO_NODE sentinel in its Some
        // arm, so NodeId::new(n) here is always Some(_).
        Some(n) => numa_shim::reserve_preferred_on_node(
            size,
            align,
            NodeId::new(n).expect("never the NO_NODE sentinel"),
        )
        .expect("NUMA-preferred reservation failed"),
        None => aligned_vmem::reserve_aligned(size, align).expect("OOM"),
    };
    drop(r);
    // === END: src/lib.rs NodeId::new "# Ergonomic path from detection" snippet ===
}

/// Runs `reserve_preferred_on_node`'s "## Best-effort fallback" snippet
/// from `src/lib.rs` — task #1341 (twentieth review F1): the snippet
/// called `Result::or_else` with a closure returning
/// `Option<Reservation>` (`aligned_vmem::reserve_aligned`), which cannot
/// type-check — `Result::or_else`'s closure must return the same
/// `Result` type. The corrected form composes `Result::ok()` (narrowing
/// to `Option`, deliberately discarding the specific error — that is
/// what "best-effort" means here) with `Option::or_else`. `NodeId::new(0)`
/// stands in for the snippet's `node` placeholder: 0 always constructs
/// (only `NO_NODE` is rejected), and on every backend this suite runs
/// under the chain yields a reservation — real Linux/Windows and the
/// mock succeed on node 0 directly, while a real unsupported platform
/// (macOS) takes the `.ok()`-to-`None` fallback path the snippet exists
/// to demonstrate.
#[test]
fn best_effort_fallback_snippet_compiles_and_runs() {
    // === BEGIN: src/lib.rs reserve_preferred_on_node "## Best-effort fallback" snippet ===
    use aligned_vmem::{page_size, PAGE};
    use numa_shim::{reserve_preferred_on_node, NodeId};

    let ps = page_size();
    let size = ps * 16;
    let align = PAGE.max(ps);
    let node = NodeId::new(0).expect("0 is never the NO_NODE sentinel");
    let r = reserve_preferred_on_node(size, align, node)
        .ok()
        .or_else(|| aligned_vmem::reserve_aligned(size, align))
        .expect("both the NUMA-preferred and the fallback reservation failed");
    drop(r);
    // === END: src/lib.rs reserve_preferred_on_node "## Best-effort fallback" snippet ===
}
