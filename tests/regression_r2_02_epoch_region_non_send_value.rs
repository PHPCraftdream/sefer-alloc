//! R2-02 (independent src review round 2, task #2004) — automated oracle for
//! the compile-fail fixture at
//! `tests/compile_fail/r2_02_epoch_region_non_send_value/`.
//!
//! Same out-of-process child-cargo technique as
//! `tests/regression_r2_01_segment_bases_lifetime.rs` (see that file's own
//! module doc for the full rationale — a compile-FAIL property cannot be
//! asserted by a normal `#[test]`).
//!
//! This oracle proves the FIX, not the bug: before task #2004 added `T: Send
//! + 'static` to `EpochRegion::insert`/`remove` (and the underlying
//! `AtomicSlot::install`/`try_evict_at`), the fixture's exact body compiled
//! and ran cleanly (confirmed manually during that task) — a real
//! use-after-free/destructor-on-wrong-thread hazard reachable from safe code
//! via `crossbeam-epoch`'s global deferred-reclamation collector.
//!
//! This test pins the POST-fix state: the fixture must fail with
//! specifically E0277 ("cannot be sent between threads safely"), not
//! succeed and not fail for some unrelated reason.

use std::path::PathBuf;
use std::process::Command;

fn fixture_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compile_fail")
        .join("r2_02_epoch_region_non_send_value")
        .join("Cargo.toml")
}

#[test]
fn epoch_region_insert_of_non_send_value_must_not_compile() {
    let manifest = fixture_manifest();
    assert!(
        manifest.is_file(),
        "compile-fail fixture missing from checkout: {}",
        manifest.display()
    );

    let child_target =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("r2_02_fixture");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(&cargo)
        .args(["build", "--offline", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &child_target)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("failed to spawn `cargo build` for the R2-02 fixture");

    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "the R2-02 fixture COMPILED — EpochRegion::insert/remove no longer \
         require T: Send (the R2-02/task #2004 fix regressed). stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("E0277"),
        "the R2-02 fixture failed to compile, but NOT with the expected \
         E0277 (Send not satisfied) — it may be failing for an unrelated \
         reason. stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("cannot be sent between threads safely"),
        "expected the specific \"cannot be sent between threads safely\" \
         message tied to Rc<i32>'s missing Send impl. stderr:\n{stderr}"
    );
}
