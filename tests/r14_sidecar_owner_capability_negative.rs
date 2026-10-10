//! R14-01 negative compile-fail harness (R13 convention: process-local
//! rustc, `--error-format=json`, exactly one error with an exact code,
//! rendered-content substrings, primary span inside the fixture's own
//! main.rs).
//! This harness invokes RUSTC (or rustc) directly. Candidates are collected
//! from the test binary's `deps` directory and probed deterministically.
//! Freshness uses each candidate's Cargo dep-info `.d`: only Rust sources
//! listed there can stale an rlib. Package-manifest mtime and cfg-inactive
//! source edits are not treated as evidence of staleness; missing/unreadable
//! dep-info is not evidence either, so the candidate stays for the positive
//! probe. This does not prove Cargo linked that candidate into this test.
//!
//! The positive probe classifies candidates as compatible, or skips only
//! exact, reviewed diagnostics: feature-incompatible registry/dependency
//! errors, E0461 for this crate's foreign target, and the exact E0514 message
//! for `sefer_alloc` itself. Unknown diagnostics and incompatible dependency
//! compiler artifacts remain hard failures.
//!
//! An all-foreign set may skip only when `rustc --print cfg` differs from the
//! test binary's target in architecture, OS, or a known target environment.
//! Equal or unrecognized target configurations fail closed, so a native green
//! test cannot silently skip all R14 checks. No workflow output parsing,
//! architecture-directory heuristic, or filesystem marker is used.
//!
//! Each negative fixture is run against every compatible candidate in its
//! own output directory and must produce exactly one expected coded error,
//! expected rendered text, and a primary span in its own `main.rs`.
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
use std::time::SystemTime;

fn rustc_command() -> std::ffi::OsString {
    std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())
}

/// True for rustc's own "this rlib is built for another target" verdict.
fn is_foreign_target_diagnostic(code: Option<&str>, message: &str) -> bool {
    code == Some("E0461")
        && message.starts_with("couldn't find crate `sefer_alloc` with expected target triple ")
}

fn is_incompatible_rustc_diagnostic(code: Option<&str>, message: &str) -> bool {
    code == Some("E0514")
        && message == "found crate `sefer_alloc` compiled by an incompatible version of rustc"
}

fn cfg_value<'a>(cfg: &'a str, key: &str) -> Option<&'a str> {
    cfg.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        if name == key {
            value.strip_prefix('"')?.strip_suffix('"')
        } else {
            None
        }
    })
}

fn current_target_env() -> Option<&'static str> {
    if cfg!(target_env = "gnu") {
        Some("gnu")
    } else if cfg!(target_env = "musl") {
        Some("musl")
    } else if cfg!(target_env = "msvc") {
        Some("msvc")
    } else if cfg!(target_env = "uclibc") {
        Some("uclibc")
    } else if cfg!(target_env = "newlib") {
        Some("newlib")
    } else if cfg!(target_env = "sgx") {
        Some("sgx")
    } else if cfg!(target_env = "ohos") {
        Some("ohos")
    } else if cfg!(target_env = "p1") {
        Some("p1")
    } else if cfg!(target_env = "p2") {
        Some("p2")
    } else if cfg!(target_os = "macos") {
        Some("")
    } else {
        None
    }
}

fn host_cfg_matches_target(host_cfg: &str, arch: &str, os: &str, env: &str) -> Option<bool> {
    Some(
        cfg_value(host_cfg, "target_arch")? == arch
            && cfg_value(host_cfg, "target_os")? == os
            && cfg_value(host_cfg, "target_env")? == env,
    )
}

fn all_foreign_skip_allowed(
    candidate_count: usize,
    foreign_count: usize,
    compatible_count: usize,
    host_matches_target: Option<bool>,
) -> bool {
    candidate_count > 0
        && foreign_count == candidate_count
        && compatible_count == 0
        && host_matches_target == Some(false)
}

fn rustc_host_cfg() -> String {
    let output = std::process::Command::new(rustc_command())
        .arg("--print")
        .arg("cfg")
        .output()
        .expect("failed to query rustc's default target cfg");
    assert!(
        output.status.success(),
        "rustc --print cfg failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("rustc --print cfg must be UTF-8")
}

fn rustc_host_matches_test_target(host_cfg: &str) -> Option<bool> {
    host_cfg_matches_target(
        host_cfg,
        std::env::consts::ARCH,
        std::env::consts::OS,
        current_target_env()?,
    )
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

#[test]
fn incompatible_rustc_diagnostic_matches_only_sefer_alloc_e0514() {
    let message = "found crate `sefer_alloc` compiled by an incompatible version of rustc";
    assert!(is_incompatible_rustc_diagnostic(Some("E0514"), message));
    assert!(!is_incompatible_rustc_diagnostic(
        Some("E0514"),
        "found crate `core` compiled by an incompatible version of rustc"
    ));
    assert!(!is_incompatible_rustc_diagnostic(Some("E0460"), message));
}

#[test]
fn all_foreign_candidates_skip_only_for_a_different_target() {
    assert!(all_foreign_skip_allowed(2, 2, 0, Some(false)));
    assert!(!all_foreign_skip_allowed(2, 2, 0, Some(true)));
    assert!(!all_foreign_skip_allowed(2, 1, 0, Some(false)));
    assert!(!all_foreign_skip_allowed(0, 0, 0, Some(false)));
    assert!(!all_foreign_skip_allowed(2, 2, 0, None));
    assert_eq!(
        host_cfg_matches_target(
            "target_arch=\"aarch64\"\ntarget_os=\"macos\"\ntarget_env=\"\"\n",
            "aarch64",
            "macos",
            "",
        ),
        Some(true)
    );
    assert_eq!(
        host_cfg_matches_target(
            "target_arch=\"x86_64\"\ntarget_os=\"linux\"\ntarget_env=\"gnu\"\n",
            "aarch64",
            "linux",
            "gnu",
        ),
        Some(false)
    );
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

/// Exclude only artifacts known to predate a readable source timestamp.
fn candidate_is_stale(
    candidate_mtime: Option<SystemTime>,
    newest_source: Option<SystemTime>,
) -> bool {
    matches!((candidate_mtime, newest_source), (Some(candidate), Some(source)) if candidate < source)
}

fn readable_modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// Missing timestamps add no staleness evidence; only dep-info-listed Rust
/// sources are compared, not manifest mtimes or cfg-inactive files.
fn newest_source_modified(manifest: &Path, dependencies: &[PathBuf]) -> Option<SystemTime> {
    let mut newest = None;
    for dependency in dependencies {
        if dependency.extension().is_some_and(|ext| ext == "rs") {
            let path = if dependency.is_absolute() {
                dependency.clone()
            } else {
                manifest.join(dependency)
            };
            newest = newest.max(readable_modified(&path));
        }
    }
    newest
}

fn dep_info_path(candidate: &Path) -> Option<PathBuf> {
    let file_name = candidate.file_name()?.to_str()?;
    let stem = file_name.strip_suffix(".rlib")?;
    let dep_stem = stem.strip_prefix("lib").unwrap_or(stem);
    Some(candidate.parent()?.join(format!("{dep_stem}.d")))
}

fn dep_info_dependencies(contents: &str) -> Vec<PathBuf> {
    let mut dependencies = Vec::new();
    for line in contents.lines() {
        let Some((_, raw_dependencies)) = line.split_once(": ") else {
            continue;
        };
        let mut word = String::new();
        let mut chars = raw_dependencies.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' && chars.peek().is_some_and(|next| next.is_whitespace()) {
                word.push(chars.next().expect("peeked dep-info escape"));
            } else if ch.is_whitespace() {
                if !word.is_empty() {
                    dependencies.push(PathBuf::from(std::mem::take(&mut word)));
                }
            } else {
                word.push(ch);
            }
        }
        if !word.is_empty() {
            dependencies.push(PathBuf::from(word));
        }
    }
    dependencies
}

fn candidate_dep_info_dependencies(candidate: &Path) -> Option<Vec<PathBuf>> {
    let path = dep_info_path(candidate)?;
    let contents = std::fs::read_to_string(path).ok()?;
    Some(dep_info_dependencies(&contents))
}

fn dep_info_freshness(candidate: &Path, manifest: &Path) -> Option<SystemTime> {
    let dependencies = candidate_dep_info_dependencies(candidate)?;
    newest_source_modified(manifest, &dependencies)
}

#[test]
fn candidate_freshness_keeps_equal_newer_and_unknown_timestamps() {
    use std::time::{Duration, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("r14_freshness_{}_{unique}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let result = std::panic::catch_unwind(|| {
        let source_time = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let source = directory.join("source.rs");
        std::fs::File::create(&source)
            .unwrap()
            .set_modified(source_time)
            .unwrap();
        let newest = readable_modified(&source);
        assert_eq!(newest, Some(source_time));
        for (name, time, stale) in [
            ("old.rlib", source_time - Duration::from_secs(60), true),
            ("equal.rlib", source_time, false),
            ("new.rlib", source_time + Duration::from_secs(60), false),
        ] {
            let path = directory.join(name);
            std::fs::File::create(&path)
                .unwrap()
                .set_modified(time)
                .unwrap();
            let modified = readable_modified(&path);
            assert_eq!(modified, Some(time));
            assert_eq!(candidate_is_stale(modified, newest), stale, "{name}");
            assert!(!candidate_is_stale(modified, None), "{name}");
        }
        let unknown = readable_modified(&directory.join("absent.rlib"));
        assert_eq!(unknown, None);
        assert!(!candidate_is_stale(unknown, newest));
        assert!(!candidate_is_stale(unknown, None));
    });
    std::fs::remove_dir_all(&directory).unwrap();
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

#[test]
fn dep_info_parser_preserves_path_separators_and_escaped_spaces() {
    let paths = dep_info_dependencies(
        r"C:\target\deps\sefer_alloc.d: src\lib.rs src\module\ with\ space.rs",
    );
    assert_eq!(
        paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        vec![
            "src\\lib.rs".to_owned(),
            "src\\module with space.rs".to_owned(),
        ]
    );
}

#[test]
fn dep_info_freshness_ignores_cfg_inactive_sources() {
    use std::time::{Duration, UNIX_EPOCH};

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let manifest = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("r14_dep_info_{unique}"));
    let src = manifest.join("src");
    std::fs::create_dir_all(&src).unwrap();
    let result = std::panic::catch_unwind(|| {
        let active_time = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let candidate_time = UNIX_EPOCH + Duration::from_secs(1_700_000_100);
        let inactive_time = UNIX_EPOCH + Duration::from_secs(1_700_000_200);
        let active = src.join("active.rs");
        let inactive = src.join("cfg_inactive.rs");
        for (path, time) in [(&active, active_time), (&inactive, inactive_time)] {
            std::fs::File::create(path)
                .unwrap()
                .set_modified(time)
                .unwrap();
        }
        let deps = manifest.join("deps");
        std::fs::create_dir_all(&deps).unwrap();
        let candidate = deps.join("libsefer_alloc-a1b2.rlib");
        std::fs::File::create(&candidate)
            .unwrap()
            .set_modified(candidate_time)
            .unwrap();
        std::fs::write(
            deps.join("sefer_alloc-a1b2.d"),
            "sefer_alloc-a1b2.rlib: src/active.rs\n",
        )
        .unwrap();
        let newest = dep_info_freshness(&candidate, &manifest);
        assert_eq!(newest, Some(active_time));
        assert!(
            !candidate_is_stale(readable_modified(&candidate), newest),
            "a newer cfg-inactive source must not stale this candidate"
        );
        assert!(
            candidate_is_stale(Some(active_time - Duration::from_secs(1)), newest),
            "an older candidate must still be rejected when an active source changed"
        );
    });
    std::fs::remove_dir_all(&manifest).unwrap();
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// Source-fresh or unknown-mtime rlibs under deps, sorted by path.
fn candidate_rlibs(deps: &Path, tag: &str) -> Vec<PathBuf> {
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
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut stale_skipped = 0usize;
    candidates.retain(|candidate| {
        if candidate_is_stale(
            readable_modified(candidate),
            dep_info_freshness(candidate, manifest),
        ) {
            stale_skipped += 1;
            eprintln!(
                "R14 {tag}: skip stale-source candidate={}",
                candidate.display()
            );
            false
        } else {
            true
        }
    });
    eprintln!("R14 {tag}: {stale_skipped} stale-source candidates skipped");
    assert!(
        !candidates.is_empty(),
        "R14 {tag}: current build not found under {} after active-input freshness filtering; \
         rebuild the crate with alloc-global + internals features",
        deps.display()
    );
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

/// Filters candidate rlibs through a positive probe. Skip only the narrowly
/// recognized feature-incompatibility diagnostics, the exact E0514 for this
/// crate, or rustc's E0461 verdict for the sefer_alloc candidate. An all-
/// foreign set skips only for a test target different from rustc's default;
/// empty sets and all-feature-incompatible sets hard fail.
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
                is_incompatible_rustc_diagnostic(
                    error.pointer("/code/code").and_then(as_str),
                    error.get("message").and_then(as_str).unwrap_or_default(),
                )
            })
        {
            eprintln!(
                "R14 {tag}: skip rustc-incompatible candidate={}",
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
        let host_cfg = rustc_host_cfg();
        let host_matches_target = rustc_host_matches_test_target(&host_cfg);
        assert!(
            all_foreign_skip_allowed(
                candidates.len(),
                foreign,
                compatible.len(),
                host_matches_target
            ),
            "R14 {tag}: refusing the all-foreign skip: candidate target does not match \
             rustc, but the test target is the same as rustc's default or cannot \
             be identified safely. host cfg:\n{host_cfg}"
        );
        eprintln!(
            "R14 {tag}: skip, all {} candidate rlibs are foreign to this cross target; \
             native-target runs cannot take this skip",
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
    let variants = compatible_variants(
        "prepare",
        &probe_fixture(),
        &candidate_rlibs(&deps_dir(), "prepare"),
    );
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
    let variants = compatible_variants(
        "issue",
        &probe_fixture(),
        &candidate_rlibs(&deps_dir(), "issue"),
    );
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
    let variants = compatible_variants(
        "sync",
        &probe_fixture(),
        &candidate_rlibs(&deps_dir(), "sync"),
    );
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
