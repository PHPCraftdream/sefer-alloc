//! R14-01 negative compile-fail harness (R13 convention: process-local
//! rustc, `--error-format=json`, exactly one error with an exact code,
//! rendered-content substrings, primary span inside the fixture's own
//! main.rs).
//! This harness invokes RUSTC (or rustc) directly. A runtime gate reads its
//! `host:` from one `rustc -vV` per negative check and normalizes both path
//! separators; CACHEDIR.TAG or .rustc_info.json above `<profile>/deps` marks
//! a native target root; otherwise that directory's basename is the target.
//! Native and explicit host targets run on every
//! architecture. Missing/nonrunning rustc and foreign targets explicitly skip
//! (foreign skips print both triples); malformed successful output fails.
//! Native roots missing both markers are treated as foreign by basename.
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
//! - E0463: prefix "can't find crate for `" and suffix
//!   " which `sefer_alloc` depends on".
//! - E0603: "module `registry` is private".
//!
//! Only the exact count-matched uncoded abort summary is allowed;
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

#[derive(Debug, PartialEq, Eq)]
enum RuntimeGate {
    Permit { host: String, target: String },
    ForeignTarget { host: String, target: String },
    MissingRustc,
}

fn runtime_gate(
    executable: &str,
    rustc_stdout: Option<&str>,
    is_target_dir_root: &dyn Fn(&str) -> bool,
) -> Result<RuntimeGate, String> {
    let Some(stdout) = rustc_stdout else {
        return Ok(RuntimeGate::MissingRustc);
    };
    let hosts: Vec<_> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("host:"))
        .collect();
    if hosts.len() != 1 || hosts[0].split_whitespace().count() != 1 {
        return Err(format!("malformed successful rustc -vV output: {stdout:?}"));
    }
    let host = hosts[0].trim();
    let normalized = executable.replace('\\', "/");
    let components: Vec<_> = normalized.split('/').collect();
    if components.len() < 4
        || components[components.len() - 2] != "deps"
        || components[components.len() - 3].is_empty()
        || components[components.len() - 4].is_empty()
        || components.last() == Some(&"")
    {
        return Err(format!(
            "unsupported test executable layout: {executable:?}"
        ));
    }
    let above_profile = components[components.len() - 4];
    let directory = components[..components.len() - 3].join("/");
    let target = if is_target_dir_root(&directory) {
        host
    } else {
        above_profile
    };
    let host = host.to_owned();
    let target = target.to_owned();
    Ok(if host == target {
        RuntimeGate::Permit { host, target }
    } else {
        RuntimeGate::ForeignTarget { host, target }
    })
}

fn rustc_command() -> std::ffi::OsString {
    std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into())
}

fn permit_fixture(tag: &str) -> bool {
    let executable = std::env::current_exe().expect("current_exe is resolvable");
    let output = std::process::Command::new(rustc_command())
        .arg("-vV")
        .output();
    let stdout = match &output {
        Ok(output) if output.status.success() => Some(
            std::str::from_utf8(&output.stdout).expect("successful rustc -vV stdout must be UTF-8"),
        ),
        _ => None,
    };
    let is_target_dir_root = |dir: &str| {
        let is_root = Path::new(dir).join("CACHEDIR.TAG").exists()
            || Path::new(dir).join(".rustc_info.json").exists();
        eprintln!("R14 {tag}: target directory={dir} is_target_dir_root={is_root}");
        is_root
    };
    match runtime_gate(&executable.to_string_lossy(), stdout, &is_target_dir_root)
        .unwrap_or_else(|error| panic!("{tag}: {error}"))
    {
        RuntimeGate::Permit { host, target } => {
            eprintln!("R14 {tag}: permit host={host} target={target}");
            true
        }
        RuntimeGate::ForeignTarget { host, target } => {
            eprintln!("R14 {tag}: skip foreign target host={host} target={target}");
            false
        }
        RuntimeGate::MissingRustc => {
            eprintln!("R14 {tag}: skip missing/nonrunning rustc: {output:?}");
            false
        }
    }
}

#[test]
fn runtime_gate_synthetic_controls() {
    for separator in ["/", "\\"] {
        for host in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let stdout = format!("rustc test\nhost: {host}\nrelease: test\n");
            for (directory, profile, binary) in [
                ("/tmp/target", "debug", "test"),
                ("/tmp/x86_64-native-cache", "debug", "test"),
                ("/tmp/native-target-cache", "custom", "test"),
                ("C:/build/native-target-dir", "release", "test.exe"),
            ] {
                let path = format!("{directory}/{profile}/deps/{binary}");
                assert_eq!(
                    runtime_gate(&path.replace('/', separator), Some(&stdout), &|dir| {
                        assert_eq!(dir, directory);
                        true
                    }),
                    Ok(RuntimeGate::Permit {
                        host: host.into(),
                        target: host.into()
                    })
                );
            }
            let directory = format!("C:/build/target/{host}");
            let path = format!("{directory}/debug/deps/test.exe");
            assert_eq!(
                runtime_gate(&path.replace('/', separator), Some(&stdout), &|dir| {
                    assert_eq!(dir, directory);
                    false
                }),
                Ok(RuntimeGate::Permit {
                    host: host.into(),
                    target: host.into()
                })
            );
            for target in [
                "arm-linux-androideabi",
                "thumbv7em-none-eabihf",
                "aarch64-apple-ios",
                "wasm32-unknown-unknown",
                "x86_64-unknown-none",
                "aarch64-unknown-linux-gnu",
                "riscv64gc-unknown-linux-gnu",
                "armv7-unknown-linux-gnueabihf",
                "x86_64-unknown-linux-gnu",
            ] {
                let directory = format!("/tmp/target/{target}");
                let path = format!("{directory}/release/deps/test");
                assert_eq!(
                    runtime_gate(&path.replace('/', separator), Some(&stdout), &|dir| {
                        assert_eq!(dir, directory);
                        false
                    }),
                    Ok(RuntimeGate::ForeignTarget {
                        host: host.into(),
                        target: target.into()
                    })
                );
            }
            for (directory, profile, binary) in [
                ("/tmp/target", "debug", "test"),
                ("/tmp/x86_64-native-cache", "debug", "test"),
                ("/tmp/native-target-cache", "custom", "test"),
                ("C:/build/native-target-dir", "release", "test.exe"),
            ] {
                let path = format!("{directory}/{profile}/deps/{binary}");
                // Without either marker, a native directory is conservatively foreign.
                assert_eq!(
                    runtime_gate(&path.replace('/', separator), Some(&stdout), &|dir| {
                        assert_eq!(dir, directory);
                        false
                    }),
                    Ok(RuntimeGate::ForeignTarget {
                        host: host.into(),
                        target: directory.rsplit('/').next().unwrap().into()
                    })
                );
            }
        }
        assert_eq!(
            runtime_gate(
                &"C:/build/target/debug/deps/test.exe".replace('/', separator),
                None,
                &|_| panic!("missing rustc must not query the directory")
            ),
            Ok(RuntimeGate::MissingRustc)
        );
    }
    // Any single nonempty host string is accepted, without a triple whitelist.
    assert_eq!(
        runtime_gate(
            "/tmp/target/debug/deps/test",
            Some("host: nonsense"),
            &|_| true
        ),
        Ok(RuntimeGate::Permit {
            host: "nonsense".into(),
            target: "nonsense".into()
        })
    );
    for stdout in [
        "",
        "release: test",
        "host:",
        "host:   ",
        "host: two strings",
        "host: x86_64-pc-windows-msvc\nhost: aarch64-apple-darwin",
    ] {
        assert!(
            runtime_gate("/tmp/target/debug/deps/test", Some(stdout), &|_| {
                panic!("malformed rustc output must not query the directory")
            })
            .is_err()
        );
    }
    for path in [
        "/tmp/test",
        "/debug/deps/test",
        "/tmp/target//deps/test",
        "/tmp/target/debug/deps/",
    ] {
        assert!(
            runtime_gate(path, Some("host: aarch64-apple-darwin"), &|_| {
                panic!("invalid layout must not query the directory")
            })
            .is_err()
        );
    }
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
/// feature-incompatible registry and dependency-resolution diagnostics; all
/// other failures remain hard harness errors.
fn compatible_variants(tag: &str, probe: &Path, candidates: &[PathBuf]) -> Vec<PathBuf> {
    let mut compatible = Vec::new();
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
    if !permit_fixture("prepare") {
        return;
    }
    let fixture = canonicalize_fixture(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compile_fail/small_sidecar_shared_prepare_not_callable/src/main.rs"),
    );
    assert!(
        fixture.is_file(),
        "compile-fail fixture missing from checkout: {}",
        fixture.display()
    );
    assert_fixture_fails(
        &fixture,
        &compatible_variants("prepare", &probe_fixture(), &candidate_rlibs(&deps_dir())),
        "E0624",
        ["prepare", "private"],
    );
}

/// Fixture B: `SmallSidecar::issue` is independently sealed — unsealing ONLY
/// `prepare` must still fail this regression with exactly one E0624 naming
/// the private `issue`.
#[test]
#[cfg(not(miri))]
fn shared_sidecar_issue_is_not_callable_from_outside_the_crate() {
    if !permit_fixture("issue") {
        return;
    }
    let fixture = canonicalize_fixture(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compile_fail/small_sidecar_shared_issue_not_callable/src/main.rs"),
    );
    assert!(
        fixture.is_file(),
        "compile-fail fixture missing from checkout: {}",
        fixture.display()
    );
    assert_fixture_fails(
        &fixture,
        &compatible_variants("issue", &probe_fixture(), &candidate_rlibs(&deps_dir())),
        "E0624",
        ["issue", "private"],
    );
}

/// Fixture C: `RouteRegistration` is `!Sync` (owner-only mutation marker), so
/// a `Sync` requirement on a shared registration reference must fail with
/// exactly one E0277 naming `RouteRegistration` and `Sync`.
#[test]
#[cfg(not(miri))]
fn route_registration_shared_reference_must_not_be_sync() {
    if !permit_fixture("sync") {
        return;
    }
    let fixture = canonicalize_fixture(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compile_fail/route_registration_owner_not_sync/src/main.rs"),
    );
    assert!(
        fixture.is_file(),
        "compile-fail fixture missing from checkout: {}",
        fixture.display()
    );
    assert_fixture_fails(
        &fixture,
        &compatible_variants("sync", &probe_fixture(), &candidate_rlibs(&deps_dir())),
        "E0277",
        ["RouteRegistration", "Sync"],
    );
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
