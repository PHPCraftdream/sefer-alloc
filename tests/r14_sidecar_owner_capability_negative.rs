//! R14-01 negative compile-fail harness (R13 convention: process-local
//! rustc, `--error-format=json`, exactly one error with an exact code,
//! rendered-content substrings, primary span inside the fixture's own
//! main.rs).
//! This harness invokes RUSTC (or rustc) directly. There is no architecture,
//! layout or marker gate: a candidate rlib built for another target than
//! rustc's default (the `cross test` rows) makes the positive probe fail with
//! E0461 ("couldn't find crate `sefer_alloc` with expected target triple ..."),
//! which is the compiler's own verdict that the candidate is foreign. Such
//! candidates are skipped explicitly, and when every candidate is foreign the
//! test prints a skip line and returns. With a usable current native artifact,
//! the probe compiles and the checks run (native arm64 included). Directory-layout and
//! marker inference was tried and dropped: cargo writes CACHEDIR.TAG into each
//! `<triple>` directory, and `.rustc_info.json` is not guaranteed in a root.
//!
//! Fixtures compile against compatible built artifacts; these candidates
//! are not proven to be the rlib linked into the current test build.
//! Artifact selection is
//! deterministic: every `libsefer_alloc-*.rlib` / `libsefer_alloc.rlib` under
//! the deps directory (the parent of `current_exe()`, which cargo places
//! under `<profile>/deps/`) is a candidate, sorted by path. A positive probe
//! fixture (`tests/compile_fail/r14_positive_probe`) decides per candidate
//! whether the build is internals-compatible: success → COMPATIBLE, failure
//! with only these restricted coded messages → skipped:
//! - E0432: "unresolved import `sefer_alloc::registry`".
//! - E0433: "cannot find `registry` in `sefer_alloc`" or
//!   "could not find `registry` in `sefer_alloc`".
//! - E0460: prefix "found possibly newer version of crate `" and suffix
//!   "` which `sefer_alloc` depends on".
//! - E0461: prefix "couldn't find crate `sefer_alloc` with expected target triple ".
//! - E0463: prefix "can't find crate for `" and suffix
//!   " which `sefer_alloc` depends on".
//! - E0603: "module `registry` is private".
//!
//! Only exit 1 with nonempty coded errors all matching the foreign verdict
//! counts a candidate as foreign. Only a nonempty all-foreign candidate set
//! with no compatible candidates skips the test; empty sets and all-feature-
//! incompatible sets hard fail. Only the exact count-matched uncoded abort
//! summary is allowed;
//! anything else → hard harness failure. Each negative fixture is then run
//! against EVERY compatible variant, each with its own out-dir, and must
//! yield exactly one coded error of the expected code with the expected
//! rendered substrings and a primary span exactly in the fixture's own file.
//!
//! Expected outcomes:
//! - `small_sidecar_shared_prepare_not_callable` → exactly one E0624
//!   (private `prepare` through a shared `&SmallSidecar`),
//! - `small_sidecar_shared_issue_not_callable` → exactly one E0624
//!   (private `issue` through a shared `&SmallSidecar`),
//! - `route_registration_owner_not_sync` → exactly one E0277
//!   (`RouteRegistration` is `!Sync` via `PhantomData<Cell<()>>`).

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::path::{Path, PathBuf};

fn rustc_command() -> std::ffi::OsString {
    std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())
}

/// True for rustc's own "this rlib is built for another target" verdict.
fn is_foreign_target_diagnostic(code: Option<&str>, message: &str) -> bool {
    code == Some("E0461")
        && message.starts_with("couldn't find crate `sefer_alloc` with expected target triple ")
}

#[test]
fn foreign_target_diagnostic_matches_the_cross_ci_message() {
    let ci =
        "couldn't find crate `sefer_alloc` with expected target triple x86_64-unknown-linux-gnu";
    assert!(is_foreign_target_diagnostic(Some("E0461"), ci));
    for code in ["E0463", "E0599", "E0624"] {
        assert!(!is_foreign_target_diagnostic(Some(code), ci));
    }
    for message in [
        "couldn't find crate `other` with expected target triple x86_64-unknown-linux-gnu",
        "couldn't find crate `sefer_alloc_extra` with expected target triple x86_64-unknown-linux-gnu",
        "couldn't find crate `sefer_alloc`",
        "couldn't find crate `sefer_alloc` with unexpected target triple x86_64-unknown-linux-gnu",
    ] {
        assert!(!is_foreign_target_diagnostic(Some("E0461"), message));
    }
    assert!(!is_foreign_target_diagnostic(None, ci));
    assert!(!is_foreign_target_diagnostic(
        Some("E0461"),
        "found possibly newer version of crate `x`"
    ));
}

/// Canonical fixture path in a rustc-comparable form (`\\?\` prefix stripped,
/// separators normalized to `/`).
fn canonical_display_path(path: &Path) -> String {
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|error| panic!("failed to canonicalize {}: {error}", path.display()));
    let text = canonical.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.replace('\\', "/")
}

fn as_str(value: &serde_json::Value) -> Option<&str> {
    value.as_str()
}

/// The deps directory: cargo places the test binary under
/// `<profile>/deps/`, so `current_exe()`'s parent IS the deps directory for
/// debug, release, and custom target dirs alike.
fn deps_dir() -> PathBuf {
    std::env::current_exe()
        .expect("current_exe is resolvable")
        .parent()
        .unwrap_or_else(|| panic!("test binary has no parent directory"))
        .to_path_buf()
}

/// All `libsefer_alloc` rlib candidates under the deps directory, sorted by
/// path for determinism (no mtime heuristics).
fn candidate_rlibs(deps: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(deps)
        .unwrap_or_else(|error| panic!("failed to read deps dir {}: {error}", deps.display()))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| {
                    panic!(
                        "failed to read deps dir entry in {}: {error}",
                        deps.display()
                    )
                })
                .path()
        })
        .filter(|path| {
            let name = path.file_name().map_or_else(
                || std::borrow::Cow::Borrowed(""),
                |name| name.to_string_lossy(),
            );
            (name.starts_with("libsefer_alloc-") || name == "libsefer_alloc.rlib")
                && name.ends_with(".rlib")
        })
        .collect();
    candidates.sort();
    candidates
}

/// Returns coded primary errors after accounting for every error diagnostic;
/// only one exact, count-matched, null-code rustc abort summary is permitted.
fn error_diagnostics(stderr: &str, context: &str) -> Vec<serde_json::Value> {
    let errors: Vec<serde_json::Value> = stderr
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap_or_else(|error| {
                panic!("invalid rustc JSON line {line:?}: {error}:\n{context}")
            })
        })
        .filter(|diagnostic| diagnostic.get("level").and_then(as_str) == Some("error"))
        .collect();
    let (coded, uncoded): (Vec<_>, Vec<_>) = errors.into_iter().partition(|diagnostic| {
        diagnostic
            .pointer("/code/code")
            .and_then(as_str)
            .is_some_and(|code| !code.is_empty())
    });
    assert!(
        uncoded.len() <= 1,
        "multiple uncoded rustc error diagnostics:\n{context}"
    );
    if let Some(summary) = uncoded.first() {
        let count = coded.len();
        let suffix = if count == 1 { "error" } else { "errors" };
        let expected = format!("aborting due to {count} previous {suffix}");
        assert!(
            count > 0
                && summary.get("code") == Some(&serde_json::Value::Null)
                && summary.get("message").and_then(as_str) == Some(expected.as_str()),
            "uncoded rustc error is not the exact count-matched abort summary:\n{context}"
        );
    }
    coded
}

fn run_rustc(fixture: &Path, candidate: &Path, out_dir: &Path) -> std::process::Output {
    std::fs::create_dir_all(out_dir)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", out_dir.display()));
    eprintln!(
        "R14 fixture execution: {} candidate={}",
        fixture.display(),
        candidate.display()
    );
    std::process::Command::new(rustc_command())
        .arg(fixture)
        .arg("--edition=2021")
        .arg("--emit=metadata")
        .arg("--error-format=json")
        .arg("--extern")
        .arg(format!("sefer_alloc={}", candidate.display()))
        .arg("-L")
        .arg(format!("dependency={}", deps_dir().display()))
        .arg("--out-dir")
        .arg(out_dir)
        .output()
        .expect("failed to spawn rustc for an R14-01 fixture")
}

fn full_context(fixture: &Path, candidate: &Path, output: &std::process::Output) -> String {
    format!(
        "fixture: {}\nsefer_alloc rlib: {}\nstatus: {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        fixture.display(),
        candidate.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

/// Filters candidate rlibs through a positive probe. Skip only known
/// feature-incompatible registry and dependency-resolution diagnostics, or
/// rustc's E0461 verdict for the sefer_alloc candidate. The latter counts once
/// per candidate only on exit 1 with nonempty, all-foreign coded errors.
/// Return empty only for a nonempty all-foreign set with no compatible rlibs;
/// empty sets, all-feature-incompatible sets and other failures hard fail.
fn compatible_variants(tag: &str, probe: &Path, candidates: &[PathBuf]) -> Vec<PathBuf> {
    let mut compatible = Vec::new();
    let mut foreign = 0usize;
    for (index, candidate) in candidates.iter().enumerate() {
        let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("r14_probe_{}_{tag}_{index}", std::process::id()));
        let output = run_rustc(probe, candidate, &out_dir);
        let context = full_context(probe, candidate, &output);
        let errors = error_diagnostics(&String::from_utf8_lossy(&output.stderr), &context);
        if output.status.code() == Some(0) {
            assert!(
                errors.is_empty(),
                "the positive probe exited 0 but reported errors:\n{}",
                full_context(probe, candidate, &output)
            );
            eprintln!("R14 {tag}: compatible candidate={}", candidate.display());
            compatible.push(candidate.clone());
        } else if output.status.code() == Some(1)
            && !errors.is_empty()
            && errors.iter().all(|error| {
                is_foreign_target_diagnostic(
                    error.pointer("/code/code").and_then(as_str),
                    error.get("message").and_then(as_str).unwrap_or_default(),
                )
            })
        {
            foreign += 1;
            eprintln!(
                "R14 {tag}: skip foreign-target candidate={}",
                candidate.display()
            );
        } else if output.status.code() == Some(1)
            && !errors.is_empty()
            && errors.iter().all(|error| {
                let code = error.pointer("/code/code").and_then(as_str);
                let message = error.get("message").and_then(as_str).unwrap_or_default();
                match code {
                    Some("E0432") => message == "unresolved import `sefer_alloc::registry`",
                    Some("E0433") => {
                        message == "cannot find `registry` in `sefer_alloc`"
                            || message == "could not find `registry` in `sefer_alloc`"
                    }
                    Some("E0463") => {
                        message.starts_with("can't find crate for `")
                            && message.ends_with(" which `sefer_alloc` depends on")
                    }
                    Some("E0460") => {
                        message.starts_with("found possibly newer version of crate `")
                            && message.ends_with("` which `sefer_alloc` depends on")
                    }
                    Some("E0603") => message == "module `registry` is private",
                    _ => false,
                }
            })
        {
            continue; // feature-incompatible build; skip
        } else {
            panic!(
                "positive probe failed incompatibly — the harness cannot \
                 qualify this candidate:\n{}",
                full_context(probe, candidate, &output)
            );
        }
    }
    if compatible.is_empty() && !candidates.is_empty() && foreign == candidates.len() {
        eprintln!(
            "R14 {tag}: skip, all {} candidate rlibs are built for another target than \
             rustc's default (cross test); the checks run with usable native artifacts",
            candidates.len()
        );
        return compatible;
    }
    assert!(
        !compatible.is_empty(),
        "no compatible libsefer_alloc rlib under {} (candidates: {:?}); \
         build the crate with the alloc-global + internals + \
         bench-internals features first",
        deps_dir().display(),
        candidates
    );
    eprintln!(
        "R14 {tag}: {} compatible candidates of {}",
        compatible.len(),
        candidates.len()
    );
    compatible
}

/// Shared tail of every (fixture, variant) run: rustc must fail with exactly
/// one coded error of the exact expected code, the rendered content must
/// carry the expected substrings, and the primary span must sit EXACTLY in
/// the fixture's own main.rs.
fn assert_single_diagnostic(
    fixture: &Path,
    candidate: &Path,
    output: &std::process::Output,
    expected_code: &str,
    expected_substrings: [&str; 2],
) {
    let context = full_context(fixture, candidate, output);
    assert_eq!(
        output.status.code(),
        Some(1),
        "the negative fixture must exit 1; success or a crash is not the \
         expected owner-capability diagnostic:\n{context}"
    );
    let errors = error_diagnostics(&String::from_utf8_lossy(&output.stderr), &context);
    assert_eq!(
        errors.len(),
        1,
        "expected exactly one coded primary error diagnostic, got {}:\n{context}",
        errors.len()
    );
    let diagnostic = &errors[0];
    assert_eq!(
        diagnostic.pointer("/code/code").and_then(as_str),
        Some(expected_code),
        "the fixture must fail with {expected_code}; a different code usually \
         means the picked rlib is stale or missing the internals features:\
         \n{context}"
    );
    let rendered = diagnostic
        .get("rendered")
        .and_then(as_str)
        .unwrap_or_default();
    assert!(
        rendered.contains(expected_substrings[0]) && rendered.contains(expected_substrings[1]),
        "the diagnostic must mention {:?} and {:?}:\n{rendered}\n{context}",
        expected_substrings[0],
        expected_substrings[1]
    );
    let expected_file = canonical_display_path(fixture);
    let primary = diagnostic
        .pointer("/spans")
        .and_then(serde_json::Value::as_array)
        .expect("diagnostic carries a spans array")
        .iter()
        .find(|span| span.get("is_primary").and_then(serde_json::Value::as_bool) == Some(true))
        .unwrap_or_else(|| panic!("no primary span in the diagnostic:\n{context}"));
    let primary_raw = primary
        .get("file_name")
        .and_then(as_str)
        .unwrap_or_default();
    let primary_file = primary_raw
        .strip_prefix(r"\\?\")
        .unwrap_or(primary_raw)
        .replace('\\', "/");
    assert_eq!(
        primary_file, expected_file,
        "the primary span must sit exactly in the fixture's own main.rs:\
         \n{context}"
    );
}

/// Runs one negative fixture against every compatible rlib variant, each
/// with its own out-dir under CARGO_TARGET_TMPDIR (sanitized from the
/// candidate file name) so parallel test binaries never share a write
/// target.
fn assert_fixture_fails(
    fixture: &Path,
    variants: &[PathBuf],
    expected_code: &str,
    expected_substrings: [&str; 2],
) {
    for candidate in variants {
        let sanitized = candidate
            .file_name()
            .map(|name| {
                name.to_string_lossy()
                    .replace(|character: char| !character.is_ascii_alphanumeric(), "_")
            })
            .unwrap_or_else(|| "unknown".to_owned());
        let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!(
            "r14_negative_{}_{sanitized}_{}",
            std::process::id(),
            expected_substrings[0]
        ));
        let output = run_rustc(fixture, candidate, &out_dir);
        assert_single_diagnostic(
            fixture,
            candidate,
            &output,
            expected_code,
            expected_substrings,
        );
    }
}

/// Fixture A: `SmallSidecar::prepare` is sealed to the crate owner — a call
/// through a shared `&SmallSidecar` from `small_sidecar()` must fail with
/// exactly one E0624 naming the private `prepare`.
#[test]
#[cfg(not(miri))]
fn shared_sidecar_prepare_is_not_callable_from_outside_the_crate() {
    let fixture = canonicalize_fixture(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compile_fail/small_sidecar_shared_prepare_not_callable/src/main.rs"),
    );
    assert!(
        fixture.is_file(),
        "compile-fail fixture missing from checkout: {}",
        fixture.display()
    );
    let variants = compatible_variants("prepare", &probe_fixture(), &candidate_rlibs(&deps_dir()));
    if variants.is_empty() {
        return;
    }
    assert_fixture_fails(&fixture, &variants, "E0624", ["prepare", "private"]);
}

/// Fixture B: `SmallSidecar::issue` is independently sealed — unsealing ONLY
/// `prepare` must still fail this regression with exactly one E0624 naming
/// the private `issue`.
#[test]
#[cfg(not(miri))]
fn shared_sidecar_issue_is_not_callable_from_outside_the_crate() {
    let fixture = canonicalize_fixture(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compile_fail/small_sidecar_shared_issue_not_callable/src/main.rs"),
    );
    assert!(
        fixture.is_file(),
        "compile-fail fixture missing from checkout: {}",
        fixture.display()
    );
    let variants = compatible_variants("issue", &probe_fixture(), &candidate_rlibs(&deps_dir()));
    if variants.is_empty() {
        return;
    }
    assert_fixture_fails(&fixture, &variants, "E0624", ["issue", "private"]);
}

/// Fixture C: `RouteRegistration` is `!Sync` (owner-only mutation marker), so
/// a `Sync` requirement on a shared registration reference must fail with
/// exactly one E0277 naming `RouteRegistration` and `Sync`.
#[test]
#[cfg(not(miri))]
fn route_registration_shared_reference_must_not_be_sync() {
    let fixture = canonicalize_fixture(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compile_fail/route_registration_owner_not_sync/src/main.rs"),
    );
    assert!(
        fixture.is_file(),
        "compile-fail fixture missing from checkout: {}",
        fixture.display()
    );
    let variants = compatible_variants("sync", &probe_fixture(), &candidate_rlibs(&deps_dir()));
    if variants.is_empty() {
        return;
    }
    assert_fixture_fails(&fixture, &variants, "E0277", ["RouteRegistration", "Sync"]);
}

/// Canonicalizes a fixture path so rustc's reported `file_name` matches
/// `canonical_display_path` byte-for-byte after normalization.
fn canonicalize_fixture(path: PathBuf) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|error| panic!("failed to canonicalize {}: {error}", path.display()))
}

/// Path of the positive probe fixture used to qualify candidate rlibs.
fn probe_fixture() -> PathBuf {
    let probe = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/compile_fail/r14_positive_probe/src/main.rs");
    assert!(
        probe.is_file(),
        "positive probe fixture missing from checkout: {}",
        probe.display()
    );
    canonicalize_fixture(probe)
}
