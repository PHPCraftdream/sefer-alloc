//! oxx R2-05 (`docs/reviews/2026-09-28-154558-src-review-oxx-round-2.md`):
//! every raw `aligned_vmem::reserve_aligned(` / `reserve_aligned_lazy(` call
//! (including typed `try_` variants)
//! on a segment-reservation path must count BOTH outcomes into the process-
//! wide counter pair documented on `SEGMENTS_RESERVE_FAILED_TOTAL`
//! (`src/alloc_core/platform/os.rs`): success into
//! `SEGMENTS_RESERVED_TOTAL`, OS refusal into `SEGMENTS_RESERVE_FAILED_TOTAL`.
//!
//! Before the fix, `reserve_small_segment_impl`
//! (`src/alloc_core/small/alloc_core_small/reserve.rs`) called
//! `aligned_vmem::reserve_aligned_lazy(...)` directly (twice: the initial
//! reserve and the pool-drain-and-retry arm) under `small-segment-lazy-commit`
//! without `numa-aware`, incrementing only `SEGMENTS_RESERVED_TOTAL` via
//! `.inspect(..)` on success — the `None` (OS refusal) arm returned straight
//! through `?` without ever touching `SEGMENTS_RESERVE_FAILED_TOTAL`. That
//! breaks the counter's own documented oracle ("zero `_FAILED_TOTAL` growth
//! ⇒ blame the allocator, not the machine") for ordinary small-segment
//! reservations in that one feature combination (`--all-features` and
//! `numa-aware` builds are unaffected — NUMA-routed reservations go through
//! `numa.rs`'s already-accounted path instead).
//!
//! The fix moved both call sites behind `os::Segment::reserve_small_lazy` —
//! a new accounting seam mirroring the pre-existing `Segment::reserve_lazy`
//! (the primordial segment's sibling) — which increments both counters
//! exactly like every other `Segment` constructor in that file.
//!
//! This is a source-text tripwire in the style of `tests/src_file_size_cap.rs`
//! / `tests/no_stale_doc_references.rs`: there is no reservation-failure
//! fault injector for this call (only commit-failure injection exists), so a
//! behavioral test cannot exercise the `None` arm directly. Instead this
//! scans `src/**/*.rs` (never `crates/`, which is `aligned_vmem` itself —
//! the primitive being called, not a caller) for every raw
//! `reserve_aligned(`/`reserve_aligned_lazy(` call and requires each one to
//! be inside an explicitly reviewed, justified wrapper function (the
//! `ALLOWLIST` below) whose body demonstrably increments
//! `SEGMENTS_RESERVE_FAILED_TOTAL` — except the one call site that is
//! legitimately NOT a segment reservation at all (`AccountedSidecar::reserve`,
//! sidecar VM, tracked by a different counter family) and is allowlisted
//! with `requires_failed_counter: false` instead.
//!
//! **Counterfactual proof (see task notes / commit message for the exact
//! command output):** reverting `reserve.rs` and `os.rs` to their pre-fix
//! content makes `no_unaccounted_reserve_aligned_calls` fail, naming
//! `reserve.rs`'s two raw call sites (function `reserve_small_segment_impl`,
//! not on the allowlist) as offenders. On the fixed tree the test is green.

use std::fs;
use std::path::{Path, PathBuf};

/// One entry: `reserve_aligned{,_lazy}(` is allowed to appear inside the
/// function named `func` in the source file whose path ends with `file`.
/// `requires_failed_counter`: if true, that SAME function's body must also
/// contain `SEGMENTS_RESERVE_FAILED_TOTAL` (the accounting is verified, not
/// just trusted); if false, the entry is an explicit, justified exemption
/// (documented in `note`) from the segment-reservation accounting contract
/// entirely.
struct Allow {
    file: &'static str,
    func: &'static str,
    requires_failed_counter: bool,
    note: &'static str,
}

const ALLOWLIST: &[Allow] = &[
    Allow {
        file: "src/alloc_core/platform/os.rs",
        func: "reserve",
        requires_failed_counter: true,
        note: "Segment::reserve — eager whole-SEGMENT reservation, accounts both outcomes",
    },
    Allow {
        file: "src/alloc_core/platform/os.rs",
        func: "reserve_exact",
        requires_failed_counter: true,
        note: "Segment::reserve_exact (exact-span-large) — accounts both outcomes",
    },
    Allow {
        file: "src/alloc_core/platform/os.rs",
        func: "reserve_capacity_exact",
        requires_failed_counter: true,
        note: "Segment::reserve_capacity_exact (large-reserved-capacity) — accounts both outcomes",
    },
    Allow {
        file: "src/alloc_core/platform/os.rs",
        func: "reserve_lazy",
        requires_failed_counter: true,
        note: "Segment::reserve_lazy (primordial-lazy-commit) — accounts both outcomes",
    },
    Allow {
        file: "src/alloc_core/platform/os.rs",
        func: "reserve_small_lazy",
        requires_failed_counter: true,
        note: "oxx R2-05 fix: Segment::reserve_small_lazy (small-segment-lazy-commit) — accounts both outcomes",
    },
    Allow {
        file: "src/alloc_core/platform/os.rs",
        func: "reserve_biased",
        requires_failed_counter: true,
        note: "Segment::reserve_biased excludes invalid arguments and later useful-window commit failure; the lazy constructor cannot distinguish initial commit from reserve failure",
    },
    Allow {
        file: "src/alloc_core/platform/os.rs",
        func: "reserve_lazy_for_measurement",
        requires_failed_counter: true,
        note: "Segment::reserve_lazy_for_measurement (bench-internals) — accounts both outcomes",
    },
    Allow {
        file: "src/alloc_core/platform/numa.rs",
        func: "reserve_aligned_on_node",
        requires_failed_counter: true,
        note: "numa::reserve_aligned_on_node — both fallback-to-plain-reservation arms account",
    },
    Allow {
        file: "src/alloc_core/platform/sidecar.rs",
        func: "reserve",
        requires_failed_counter: false,
        note: "AccountedSidecar::reserve is NOT a segment reservation (SegmentDirectory/large-cache \
               sidecar VM) — SEGMENTS_RESERVE_FAILED_TOTAL's own doc restricts it to \
               \"a segment-reservation path\"; this path is tracked instead by \
               sidecar_stats::record_sidecar_reservation, a separate counter family",
    },
];

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

/// For every line, the name of the innermost `fn` whose body (the span
/// between its opening `{` and matching closing `}`) contains that line —
/// `None` if the line sits outside any function body. Brace-depth tracking
/// over raw chars: exact for this crate's rustfmt'd sources, which never put
/// a `{`/`}` inside a string/char literal on a line this scanner needs to
/// resolve.
fn fn_per_line(text: &str) -> (Vec<&str>, Vec<Option<String>>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut fn_at_line: Vec<Option<String>> = vec![None; lines.len()];
    let mut stack: Vec<(String, i64)> = Vec::new();
    let mut depth: i64 = 0;
    let mut pending_name: Option<String> = None;
    for (i, line) in lines.iter().enumerate() {
        if !line.trim_start().starts_with("//") {
            if let Some(idx) = line.find("fn ") {
                let boundary_ok = line[..idx]
                    .chars()
                    .last()
                    .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
                if boundary_ok {
                    let name: String = line[idx + 3..]
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        pending_name = Some(name);
                    }
                }
            }
        }
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    if let Some(name) = pending_name.take() {
                        stack.push((name, depth));
                    }
                }
                '}' => {
                    if stack.last().is_some_and(|&(_, d)| d == depth) {
                        stack.pop();
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        fn_at_line[i] = stack.last().map(|(n, _)| n.clone());
    }
    (lines, fn_at_line)
}

/// Does the contiguous run of lines sharing `fn_at_line[call_idx]`'s name
/// contain `needle`?
fn enclosing_fn_body_contains(
    fn_at_line: &[Option<String>],
    lines: &[&str],
    call_idx: usize,
    needle: &str,
) -> bool {
    let name = fn_at_line[call_idx].as_deref();
    let mut start = call_idx;
    while start > 0 && fn_at_line[start - 1].as_deref() == name {
        start -= 1;
    }
    let mut end = call_idx;
    while end + 1 < fn_at_line.len() && fn_at_line[end + 1].as_deref() == name {
        end += 1;
    }
    lines[start..=end].iter().any(|l| l.contains(needle))
}

#[test]
fn no_unaccounted_reserve_aligned_calls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rs(&root.join("src"), &mut files);
    assert!(
        files.len() > 100,
        "scanner found only {} files — src/ layout changed?",
        files.len()
    );

    let mut offenders: Vec<String> = Vec::new();
    let mut total_call_sites = 0usize;

    for file in &files {
        let text = fs::read_to_string(file).expect("read source");
        let (lines, fn_at_line) = fn_per_line(&text);
        let rel = file
            .strip_prefix(root)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");

        for (i, line) in lines.iter().enumerate() {
            if line.trim_start().starts_with("//") {
                continue; // doc/line comments mentioning the call in prose
            }
            let is_call =
                line.contains("reserve_aligned(") || line.contains("reserve_aligned_lazy(");
            if !is_call {
                continue;
            }
            total_call_sites += 1;

            let func = fn_at_line[i].as_deref();
            let allow = func.and_then(|f| {
                ALLOWLIST
                    .iter()
                    .find(|a| rel.ends_with(a.file) && a.func == f)
            });

            match allow {
                None => {
                    offenders.push(format!(
                        "{rel}:{}: not in an allowlisted accounting wrapper (enclosing fn: {}): {}",
                        i + 1,
                        func.unwrap_or("<none>"),
                        line.trim()
                    ));
                }
                Some(a) if a.requires_failed_counter => {
                    if !enclosing_fn_body_contains(
                        &fn_at_line,
                        &lines,
                        i,
                        "SEGMENTS_RESERVE_FAILED_TOTAL",
                    ) {
                        offenders.push(format!(
                            "{rel}:{}: allowlisted as `{}` ({}) but its body no longer increments \
                             SEGMENTS_RESERVE_FAILED_TOTAL — update the wrapper or this allowlist",
                            i + 1,
                            a.func,
                            a.note
                        ));
                    }
                    if a.func == "reserve_biased"
                        && (!enclosing_fn_body_contains(
                            &fn_at_line,
                            &lines,
                            i,
                            "!error.is_invalid_argument()",
                        ) || !enclosing_fn_body_contains(
                            &fn_at_line,
                            &lines,
                            i,
                            "SEGMENTS_RESERVE_FAILED_TOTAL.fetch_add",
                        ) || !enclosing_fn_body_contains(
                            &fn_at_line,
                            &lines,
                            i,
                            "SEGMENTS_RESERVED_TOTAL.fetch_add",
                        ))
                    {
                        offenders.push(format!(
                            "{rel}:{}: biased reserve must distinguish OS refusal from invalid arguments and count successful reservations independently",
                            i + 1
                        ));
                    }
                }
                Some(_) => {} // explicit non-segment exemption, no counter required
            }
        }
    }

    assert!(
        total_call_sites >= 8,
        "scanner found only {total_call_sites} reserve_aligned{{,_lazy}} call sites in src/ — \
         scan logic likely broken (expected at least os.rs's 6 + numa.rs's 2)"
    );

    assert!(
        offenders.is_empty(),
        "oxx R2-05: raw `reserve_aligned{{,_lazy}}(` call(s) on a segment-reservation path that do \
         not account an OS refusal into SEGMENTS_RESERVE_FAILED_TOTAL (breaks the \"zero \
         _FAILED_TOTAL growth ⇒ blame the allocator\" oracle for pools 143/146) — route them \
         through an existing `os::Segment::reserve*` accounting seam, add a new one, or extend \
         this test's ALLOWLIST with a justification:\n{}",
        offenders.join("\n")
    );
}
