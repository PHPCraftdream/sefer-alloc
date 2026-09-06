//! Shared child-cargo mechanics for the root compile-fail driver. The test
//! count drifts as hazards are added; re-derive it with
//! `grep -c '^#\[test\]' tests/tagged_index_stack_compile_fail.rs`.
//!
//! Every compile-fail test used to duplicate the same ~55 lines of
//! boilerplate: manifest-path resolution, the out-of-process `cargo build`,
//! and the diagnostic context string.
//! This module is that boilerplate, stated once. The assertion logic —
//! which error codes and message substrings each fixture must produce —
//! stays in the individual tests in `tests/tagged_index_stack_compile_fail.rs`.
//!
//! The fixture crates stay under the package's repository-only test tree; the
//! root driver and this helper live outside the published crate. A checkout
//! that reaches this helper must therefore contain every fixture; a missing
//! manifest is an immediate test failure.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[derive(Debug)]
pub struct CargoSpanText {
    pub text: String,
    pub highlight_start: u32,
    pub highlight_end: u32,
}

#[derive(Debug)]
pub struct CargoDiagnosticSpan {
    pub file_name: String,
    pub line_start: u32,
    pub line_end: u32,
    pub column_start: u32,
    pub column_end: u32,
    pub is_primary: bool,
    pub text: Vec<CargoSpanText>,
}

impl CargoDiagnosticSpan {
    #[must_use]
    pub fn highlighted_text(&self) -> Option<String> {
        if self.text.len() != 1 || self.line_start != self.line_end {
            return None;
        }
        let source = &self.text[0].text;
        let start = usize::try_from(self.text[0].highlight_start)
            .ok()?
            .checked_sub(1)?;
        let end = usize::try_from(self.text[0].highlight_end)
            .ok()?
            .checked_sub(1)?;
        source.get(start..end).map(str::to_owned)
    }
}

#[derive(Debug)]
pub struct CargoErrorDiagnostic {
    pub code: Option<String>,
    pub code_is_null: bool,
    pub message: String,
    pub rendered: String,
    pub spans: Vec<CargoDiagnosticSpan>,
}

/// Resolves the fixture manifest
/// `crates/tagged-index-stack/tests/compile_fail/<fixture_dir>/Cargo.toml`
/// (public so callers can build their failure context from the same path).
pub fn fixture_manifest(fixture_dir: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("crates")
        .join("tagged-index-stack")
        .join("tests")
        .join("compile_fail")
        .join(fixture_dir)
        .join("Cargo.toml")
}

/// Builds the named compile-fail fixture in a child cargo process and
/// returns its output.
///
/// `rustflags` selects the child `RUSTFLAGS` handling: `Some(f)` SETS
/// `RUSTFLAGS=f` (the loom-cfg fixture is the inverse case — the `--cfg
/// loom` configuration is the whole point); `None` REMOVES `RUSTFLAGS` so
/// an inherited `--cfg loom` cannot make a fixture fail for the WRONG
/// reason. Both cases `env_remove("CARGO_ENCODED_RUSTFLAGS")`: cargo
/// prefers the encoded variable over `RUSTFLAGS`, so a merely-empty-or-
/// inherited encoded value silently cancels either the strip (nothing to
/// cancel, but hygiene) or the override (an empty
/// `CARGO_ENCODED_RUSTFLAGS=""` was observed to suppress the RUSTFLAGS
/// override entirely while developing the loom-cfg test).
///
/// The child target dir is `CARGO_TARGET_TMPDIR/<fixture_dir>` — cached
/// across runs; target/tmp is gitignored.
pub fn build_fixture(fixture_dir: &str, rustflags: Option<&str>) -> Output {
    let manifest = fixture_manifest(fixture_dir);
    assert!(
        manifest.is_file(),
        "compile-fail fixture missing from checkout: {}",
        manifest.display()
    );

    let child_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(fixture_dir);

    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(&cargo);
    command
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target);
    match rustflags {
        // The cfg-is-the-point case (loom fixture): SET the override.
        Some(flags) => {
            command.env("RUSTFLAGS", flags);
        }
        // The default case: STRIP the flags so an inherited `--cfg loom`
        // cannot mask the real failure.
        None => {
            command.env_remove("RUSTFLAGS");
        }
    }
    // Either way, the encoded variant must go or it silently wins over
    // `RUSTFLAGS` (see above).
    command.env_remove("CARGO_ENCODED_RUSTFLAGS");
    // CI's workflow-level CARGO_TERM_COLOR=always is inherited all the way
    // down to this child build; force plain-text rustc diagnostics so the
    // substring assertions in the callers match the same text in CI as
    // locally (the same color-sensitive diagnostic class as CI
    // with --color=never).
    command.env("CARGO_TERM_COLOR", "never");
    command
        .output()
        .expect("failed to spawn cargo for the compile-fail fixture")
}

/// Builds a fixture with Cargo's machine-readable diagnostics.
pub fn build_fixture_with_json(fixture_dir: &str, rustflags: Option<&str>) -> Output {
    let manifest = fixture_manifest(fixture_dir);
    assert!(
        manifest.is_file(),
        "compile-fail fixture missing from checkout: {}",
        manifest.display()
    );

    let child_target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(fixture_dir);
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(&cargo);
    command
        .args([
            "build",
            "--offline",
            "--message-format=json",
            "--manifest-path",
        ])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", child_target);
    match rustflags {
        Some(flags) => {
            command.env("RUSTFLAGS", flags);
        }
        None => {
            command.env_remove("RUSTFLAGS");
        }
    }
    command
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to spawn cargo for the compile-fail fixture")
}

/// Parses every non-empty Cargo JSON line; malformed output fails loudly.
pub fn cargo_error_diagnostics(output: &Output) -> Vec<CargoErrorDiagnostic> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("Cargo JSON output contained invalid JSON")
        })
        .filter_map(|record| {
            let reason = record
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .expect("Cargo JSON record missing string `reason`");
            if reason != "compiler-message" {
                return None;
            }
            let message = record
                .get("message")
                .and_then(serde_json::Value::as_object)
                .expect("Cargo compiler-message missing `message` object");
            let level = message
                .get("level")
                .and_then(serde_json::Value::as_str)
                .expect("Cargo diagnostic missing string `level`");
            if level != "error" {
                return None;
            }
            let code_value = message
                .get("code")
                .expect("Cargo diagnostic missing `code`");
            let code_is_null = code_value.is_null();
            let code = if code_is_null {
                None
            } else {
                Some(
                    code_value
                        .get("code")
                        .and_then(serde_json::Value::as_str)
                        .expect("Cargo diagnostic code missing string `code`")
                        .to_owned(),
                )
            };
            let spans = message
                .get("spans")
                .and_then(serde_json::Value::as_array)
                .expect("Cargo diagnostic missing `spans` array")
                .iter()
                .map(parse_diagnostic_span)
                .collect();
            Some(CargoErrorDiagnostic {
                code,
                code_is_null,
                message: message
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .expect("Cargo diagnostic missing string `message`")
                    .to_owned(),
                rendered: message
                    .get("rendered")
                    .and_then(serde_json::Value::as_str)
                    .expect("Cargo diagnostic missing string `rendered`")
                    .to_owned(),
                spans,
            })
        })
        .collect()
}

fn parse_diagnostic_span(span: &serde_json::Value) -> CargoDiagnosticSpan {
    let object = span
        .as_object()
        .expect("Cargo diagnostic span was not an object");
    CargoDiagnosticSpan {
        file_name: required_string(object, "file_name", "span"),
        line_start: required_u32(object, "line_start", "span"),
        line_end: required_u32(object, "line_end", "span"),
        column_start: required_u32(object, "column_start", "span"),
        column_end: required_u32(object, "column_end", "span"),
        is_primary: object
            .get("is_primary")
            .and_then(serde_json::Value::as_bool)
            .expect("Cargo diagnostic span missing boolean `is_primary`"),
        text: object
            .get("text")
            .and_then(serde_json::Value::as_array)
            .expect("Cargo diagnostic span missing `text` array")
            .iter()
            .map(|line| {
                let line = line
                    .as_object()
                    .expect("Cargo diagnostic span text was not an object");
                CargoSpanText {
                    text: required_string(line, "text", "span text"),
                    highlight_start: required_u32(line, "highlight_start", "span text"),
                    highlight_end: required_u32(line, "highlight_end", "span text"),
                }
            })
            .collect(),
    }
}

fn required_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    context: &str,
) -> String {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("Cargo {context} missing string `{key}`"))
}

fn required_u32(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    context: &str,
) -> u32 {
    let value = object
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or_else(|| panic!("Cargo {context} missing integer `{key}`"));
    u32::try_from(value).unwrap_or_else(|_| panic!("Cargo {context} `{key}` overflows u32"))
}

/// The shared failure-context string every assertion message in
/// `tests/tagged_index_stack_compile_fail.rs` embeds: fixture path, exit
/// status, and both output streams.
pub fn failure_context(manifest: &Path, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    format!(
        "fixture: {}\nstatus: {:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        manifest.display(),
        output.status.code()
    )
}
