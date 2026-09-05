//! Root integration tests for the runner's containment and scratch-lifecycle
//! contracts.
//!
//! Seven oracles cover rejected output/target paths, a planted scratch-root
//! redirect, successful cleanup, fatal cleanup, ordinary-error cleanup, and
//! the explicit `--keep-scratch` opt-out. Every run uses a disposable
//! skeleton, so a containment regression can only damage that test's copy.
//! Counterfactuals are explicit: rejected paths must fail before mutation;
//! a planted redirect must not reach its victim; post-creation failures must
//! clean their root; and `--keep-scratch` must retain exactly one owned root.
//!
//! Skips (with a message, not a failure) when `node` is not on PATH: the
//! runner is a Node script, and CI's cargo-test runners ship Node (per the
//! ci.yml build-check step's own comment). Directory-symlink coverage skips
//! analogously where the OS refuses to create one.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Directory removed recursively on drop, including when an assertion panics
/// mid-test (keeps counterfactual runs from littering the temp dir). The
/// guard only wraps a directory this process created exclusively.
struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
        let _ = fs::remove_file(&self.0);
    }
}

impl DirGuard {
    fn new(path: PathBuf) -> Self {
        DirGuard(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

fn next_uid() -> u32 {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Creates a private temp directory. Exclusive creation prevents a concurrent
/// test or planted symlink from becoming a directory guard's target.
fn exclusive_temp_dir(label: &str) -> DirGuard {
    loop {
        let subsec_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "tis_runner_guard_{}_{}_{}_{}",
            std::process::id(),
            subsec_nanos,
            next_uid(),
            label
        ));
        match fs::create_dir(&dir) {
            Ok(()) => return DirGuard::new(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => panic!("create exclusive temp dir {}: {e}", dir.display()),
        }
    }
}

fn node_available() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

fn copy_file(src: &Path, dst: &Path) {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).expect("create skeleton parent dir");
    }
    fs::copy(src, dst)
        .unwrap_or_else(|e| panic!("copy {} -> {}: {e}", src.display(), dst.display()));
}

/// Builds a disposable skeleton repo in the temp dir and returns guards for
/// its private parent and the runner copy inside it. The skeleton mirrors the
/// repository layout the runner derives from its own location.
fn build_repo_copy(label: &str) -> (DirGuard, DirGuard, PathBuf) {
    let repo_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crate_dir = repo_dir.join("crates/tagged-index-stack");
    let parent = exclusive_temp_dir(label);
    let root = parent.path().join("repo");
    fs::create_dir_all(&root).expect("create skeleton repo root");

    // Dummy workspace manifest: the repo-intact probes below check this file.
    fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("write dummy Cargo.toml");

    // Copy the runner and its materialization inputs from the repository tree.
    let scripts = crate_dir.join("scripts");
    copy_file(
        &scripts.join("tis_p3_ab_runner.mjs"),
        &root.join("crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs"),
    );
    for tmpl in [
        "codegen_wrapper.rs.tmpl",
        "harness_bin.rs",
        "scratch_Cargo.toml.tmpl",
    ] {
        copy_file(
            &scripts.join("tis_p3_ab").join(tmpl),
            &root
                .join("crates/tagged-index-stack/scripts/tis_p3_ab")
                .join(tmpl),
        );
    }
    copy_file(
        &crate_dir.join("src/lib.rs"),
        &root.join("crates/tagged-index-stack/src/lib.rs"),
    );
    copy_file(
        &crate_dir.join("src/imp.rs"),
        &root.join("crates/tagged-index-stack/src/imp.rs"),
    );
    copy_file(
        &repo_dir.join("scripts/capture-measurement-identity.mjs"),
        &root.join("scripts/capture-measurement-identity.mjs"),
    );
    copy_file(
        &repo_dir.join("scripts/lib.mjs"),
        &root.join("scripts/lib.mjs"),
    );

    // Disposable-copy git state ONLY (never the shared workspace repo): the
    // identity capture needs a commit for `rev-parse HEAD` + `write-tree`.
    let _ = Command::new("git").arg("init").current_dir(&root).output();
    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(&root)
        .output();
    let _ = Command::new("git")
        .args([
            "-c",
            "user.name=tis-runner-guard",
            "-c",
            "user.email=guard@example.invalid",
            "commit",
            "-m",
            "skeleton",
        ])
        .current_dir(&root)
        .output();

    let runner = root.join("crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs");
    (parent, DirGuard::new(root.clone()), runner)
}

/// The rejection is mode-independent (argument parsing), so every case uses
/// the cheapest mode; the target value is charset-valid but never reached.
fn run_codegen(runner: &Path, extra: &[&str]) -> Output {
    Command::new("node")
        .arg(runner)
        .args(["--mode", "codegen", "--target", "x86_64-unknown-linux-gnu"])
        .args(extra)
        .output()
        .expect("spawn node for the runner copy")
}

/// Run the runner copy in `--mode build-check`: the cheapest mode that
/// actually REACHES the scratch machinery (the `--out-dir`/`--target`
/// rejection cases above die in argument parsing, before any filesystem
/// access), and the one mode whose scratch writes nothing outside `target/`
/// — no docs/perf artifacts, no identity capture, no git needed.
fn run_build_check(runner: &Path) -> Output {
    run_build_check_with(runner, &[])
}

/// [`run_build_check`] with extra CLI arguments (e.g. `--keep-scratch` for
/// the lifecycle oracles at the bottom of this file).
fn run_build_check_with(runner: &Path, extra: &[&str]) -> Output {
    Command::new("node")
        .arg(runner)
        .args(["--mode", "build-check"])
        .args(extra)
        .output()
        .expect("spawn node for the runner copy")
}

fn run_build_check_unexpected_error(runner: &Path) -> Output {
    Command::new("node")
        .arg(runner)
        .args(["--mode", "build-check"])
        .env("TIS_P3_AB_TEST_UNEXPECTED_AFTER_MKDTEMP", "1")
        .output()
        .expect("spawn node for the unexpected-error lifecycle oracle")
}

fn assert_fatal(out: &Output, what: &str) {
    assert!(
        !out.status.success(),
        "runner accepted {what}; rejected paths must fail before mutation"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("tis_p3_ab_runner: FATAL"),
        "runner rejected {what} but without its FATAL diagnostics; stderr:\n{stderr}"
    );
}

fn assert_repo_intact(root: &Path, runner: &Path, what: &str) {
    assert!(
        root.join("Cargo.toml").is_file(),
        "skeleton repo root Cargo.toml was deleted while rejecting {what}"
    );
    assert!(
        runner.is_file(),
        "the runner script itself was deleted while rejecting {what}"
    );
}

/// One rejected `--out-dir` case: label, value, and survival oracle.
type OutDirCase<'a> = (&'a str, String, Box<dyn Fn()>);

/// Rejected `--out-dir` values must fail before the runner reads or mutates
/// them. Each value keeps its own canary or containment oracle.
#[test]
fn out_dir_rejection_is_value_independent() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("out_dir_values");
    let root_str = root_guard.path().to_string_lossy().to_string();

    // Fixtures for the values that point OUTSIDE the skeleton, planted the
    // Plant the external values beside the disposable skeleton.
    let victim = exclusive_temp_dir("victim");
    fs::write(victim.path().join("canary.txt"), "unrelated to the repo")
        .expect("write victim canary");
    let victim_str = victim.path().to_string_lossy().to_string();

    let sibling = DirGuard::new(parent.path().join(format!("sibling_{}", next_uid())));
    let sibling_str = sibling.path().to_string_lossy().to_string();

    let real = exclusive_temp_dir("real");
    fs::write(real.path().join("canary.txt"), "behind the symlink")
        .expect("write symlink-target canary");
    // Directory-link creation requires a free destination, so use a fresh
    // child of the guarded parent rather than an already-created temp dir.
    let link = parent.path().join("link");
    assert!(
        !link.exists(),
        "make_dir_symlink requires a free destination path"
    );
    let symlink_planted = make_dir_symlink(&link, real.path());
    let link_str = link.to_string_lossy().to_string();
    if symlink_planted {
        let metadata = fs::symlink_metadata(&link)
            .unwrap_or_else(|e| panic!("symlink_metadata on {}: {e}", link.display()));
        assert!(
            metadata.file_type().is_symlink(),
            "fixture: {} is not a symlink/junction after make_dir_symlink",
            link.display()
        );
    } else {
        eprintln!("skipping: directory symlinks/junctions unavailable in this environment");
    }

    // (assertion label, the rejected value, the value's own survival oracle)
    let mut cases: Vec<OutDirCase<'_>> = Vec::new();
    cases.push(("--out-dir .", ".".to_string(), Box::new(|| {})));
    cases.push(("--out-dir ..", "..".to_string(), Box::new(|| {})));
    cases.push((
        "--out-dir <repo root as absolute path>",
        root_str,
        Box::new(|| {}),
    ));
    cases.push((
        "--out-dir <absolute temp dir unrelated to the repo>",
        victim_str,
        Box::new(move || {
            assert!(
                victim.path().join("canary.txt").is_file(),
                "the absolute out-dir's canary file was deleted — the runner still clears a user-supplied directory"
            );
        }),
    ));
    cases.push((
        "--out-dir <sibling directory of the repo>",
        sibling_str,
        Box::new(move || {
            assert!(
                !sibling.path().exists(),
                "the runner created the sibling directory it was supposed to reject"
            );
        }),
    ));
    if symlink_planted {
        cases.push((
            "--out-dir <symlink pointing outside the scratch root>",
            link_str,
            Box::new(move || {
                assert!(
                    link.exists(),
                    "the symlink itself was deleted by the runner while rejecting its --out-dir"
                );
                assert!(
                    real.path().join("canary.txt").is_file(),
                    "the symlink target's canary file was deleted by the runner while rejecting its --out-dir"
                );
            }),
        ));
    }

    for (what, value, post) in &cases {
        assert_fatal(&run_codegen(&runner, &["--out-dir", value.as_str()]), what);
        assert_repo_intact(root_guard.path(), &runner, what);
        post();
    }
    drop(parent);
}

#[cfg(unix)]
fn make_dir_symlink(link: &Path, target: &Path) -> bool {
    std::os::unix::fs::symlink(target, link).is_ok()
}

#[cfg(windows)]
fn make_dir_symlink(link: &Path, target: &Path) -> bool {
    if std::os::windows::fs::symlink_dir(target, link).is_ok() {
        return true;
    }
    // No Dev Mode: fall back to a directory junction (no privilege needed).
    Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .map(|o| o.status.success() && link.exists())
        .unwrap_or(false)
}

#[test]
fn target_dot_and_dotdot_are_rejected_and_scratch_canary_survives() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("targets");
    let scratch = root_guard.path().join("target").join("tis_p3_ab");
    fs::create_dir_all(&scratch).expect("create dedicated scratch root in skeleton");
    fs::write(scratch.join("canary.txt"), "scratch canary").expect("write scratch canary");

    assert_fatal(&run_codegen(&runner, &["--target", "."]), "--target .");
    assert_repo_intact(root_guard.path(), &runner, "--target .");
    assert!(
        scratch.join("canary.txt").is_file(),
        "--target . cleared the dedicated scratch root (canary gone)"
    );

    assert_fatal(&run_codegen(&runner, &["--target", ".."]), "--target ..");
    assert_repo_intact(root_guard.path(), &runner, "--target ..");
    assert!(
        scratch.join("canary.txt").is_file(),
        "--target .. cleared at or above the dedicated scratch root (canary gone)"
    );
    drop(parent);
}

/// A planted link at the scratch-root location must not redirect
/// cleanup into an external victim. The runner must use a fresh private root,
/// complete build-check, and preserve the victim canary.
#[test]
fn scratch_root_junction_redirect_leaves_victim_canary_intact() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("scratch_junction");
    let victim = parent.path().join("victim");
    fs::create_dir_all(victim.join("build-check")).expect("create victim build-check dir");
    fs::write(
        victim.join("build-check").join("canary.txt"),
        "behind the scratch-root junction",
    )
    .expect("write victim canary");

    let skeleton_target = root_guard.path().join("target");
    fs::create_dir_all(&skeleton_target).expect("create skeleton target dir");
    if !make_dir_symlink(&skeleton_target.join("tis_p3_ab"), &victim) {
        eprintln!("skipping: directory symlinks/junctions unavailable in this environment");
        drop(parent);
        return;
    }

    let out = run_build_check(&runner);
    assert!(
        out.status.success(),
        "runner failed against a planted scratch-root redirect; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        victim.join("build-check").join("canary.txt").is_file(),
        "runner followed a planted scratch-root redirect and deleted the victim"
    );
}

// The first four tests pin containment; the three tests below pin cleanup.
// Each compares the scratch-root set before and after one runner invocation.

/// Sorted list of the `tis_p3_ab-` prefixed entries directly under the
/// skeleton's `target/` — exactly the per-invocation `mkdtemp` scratch roots
/// the runner creates (`tis_p3_ab-<random>`). A missing `target/` directory
/// means "no roots yet" (a fresh skeleton's state); any other read error is
/// a fixture failure, not an empty snapshot.
fn scratch_roots_under(target_dir: &Path) -> Vec<PathBuf> {
    match fs::read_dir(target_dir) {
        Ok(entries) => {
            let mut roots: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("tis_p3_ab-"))
                })
                .collect();
            roots.sort();
            roots
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(e) => panic!("snapshot scratch roots under {}: {e}", target_dir.display()),
    }
}

/// Appends a guaranteed top-level syntax error to the SKELETON's
/// `src/imp.rs` copy — the controlled failure injection for the lifecycle
/// oracles. The appended line cannot create or duplicate a template anchor
/// (the runner's `verifyAllAnchorsOnce` only counts fixed multi-line
/// snippets), so the runner sails past source verification, creates its
/// `mkdtemp` scratch root, materializes the scratch tree, and only then
/// fails deterministically at `cargo build` (a parse error, on every
/// toolchain) — strictly after scratch-root creation, which makes the cleanup
/// assertions non-vacuous.
fn break_skeleton_imp_rs(skeleton_root: &Path) {
    let imp = skeleton_root.join("crates/tagged-index-stack/src/imp.rs");
    let mut src = fs::read_to_string(&imp).expect("read skeleton imp.rs copy");
    src.push_str("\n__tis_scratch_guard_deliberate_syntax_error__\n");
    fs::write(&imp, src).expect("append deliberate syntax error to skeleton imp.rs copy");
}

/// Failure-path lifecycle runs must fail at build-check after scratch-root
/// creation; a pre-scratch failure would make the no-leak assertion vacuous.
fn assert_fatal_from_post_mkdtemp_cargo_build(out: &Output) {
    assert!(
        !out.status.success(),
        "runner exited 0 against a deliberately broken imp.rs — the controlled \
         post-mkdtemp failure did not happen"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("tis_p3_ab_runner: FATAL"),
        "broken-source run lacked the runner's FATAL diagnostics; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("cargo build failed"),
        "FATAL did not come from post-creation cargo build; no-leak oracle is vacuous; \
         stderr:\n{stderr}"
    );
}

/// A successful build-check must leave no new scratch root. A cleanup
/// regression leaves the root created by this invocation.
#[test]
fn build_check_success_leaves_no_scratch_root() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("lifecycle_ok");
    let skeleton_target = root_guard.path().join("target");
    let before = scratch_roots_under(&skeleton_target);
    assert!(
        before.is_empty(),
        "fixture: a fresh skeleton must have no scratch roots yet: {before:?}"
    );
    let out = run_build_check(&runner);
    assert!(
        out.status.success(),
        "build-check must succeed against an unbroken skeleton for this oracle to mean \
         anything; stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("build-check mode OK"),
        "run exited 0 but never printed build-check's success line (mechanism oracle); \
         stdout:\n{stdout}"
    );
    let after = scratch_roots_under(&skeleton_target);
    assert_eq!(
        before, after,
        "successful build-check left a scratch root under <repo>/target"
    );
    drop(parent);
}

/// A fatal build-check error after scratch-root creation must still leave no
/// root. The controlled syntax error and post-creation diagnostic prove that
/// the cleanup assertion does not pass before scratch creation.
#[test]
fn build_check_fatal_failure_leaves_no_scratch_root() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("lifecycle_fail");
    break_skeleton_imp_rs(root_guard.path());
    let skeleton_target = root_guard.path().join("target");
    let before = scratch_roots_under(&skeleton_target);
    assert!(
        before.is_empty(),
        "fixture: a fresh skeleton must have no scratch roots yet: {before:?}"
    );
    let out = run_build_check(&runner);
    assert_fatal_from_post_mkdtemp_cargo_build(&out);
    let after = scratch_roots_under(&skeleton_target);
    assert_eq!(
        before, after,
        "fatal post-creation error left a scratch root under <repo>/target"
    );
    drop(parent);
}

/// An ordinary post-creation error must also clean the already-created root;
/// the environment hook makes a pre-scratch rejection impossible here.
#[test]
fn unexpected_post_mkdtemp_error_leaves_no_scratch_root() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("lifecycle_unexpected");
    let target = root_guard.path().join("target");
    let before = scratch_roots_under(&target);
    assert!(
        before.is_empty(),
        "fixture: unexpected-error skeleton has roots: {before:?}"
    );
    let out = run_build_check_unexpected_error(&runner);
    assert!(
        !out.status.success(),
        "ordinary post-mkdtemp Error unexpectedly returned success"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("deliberate post-mkdtemp ordinary Error"),
        "failure did not come from the ordinary Error hook: {stderr}"
    );
    assert_eq!(
        before,
        scratch_roots_under(&target),
        "ordinary Error after mkdtemp leaked a scratch root"
    );
    drop(parent);
}

/// With `--keep-scratch`, the same controlled failure must leave exactly one
/// owned root containing this invocation's build-check child. The test removes
/// that root after observing it.
#[test]
fn keep_scratch_fatal_failure_keeps_exactly_one_owned_root() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("lifecycle_keep");
    break_skeleton_imp_rs(root_guard.path());
    let skeleton_target = root_guard.path().join("target");
    let before = scratch_roots_under(&skeleton_target);
    assert!(
        before.is_empty(),
        "fixture: a fresh skeleton must have no scratch roots yet: {before:?}"
    );
    let out = run_build_check_with(&runner, &["--keep-scratch"]);
    assert_fatal_from_post_mkdtemp_cargo_build(&out);
    let after = scratch_roots_under(&skeleton_target);
    assert_eq!(
        after.len(),
        1,
        "--keep-scratch must leave exactly one owned scratch root; before={before:?}, after={after:?}"
    );
    let kept = &after[0];
    assert!(
        kept.join("build-check").is_dir(),
        "the kept root {} does not contain build-check mode's fixed child — it is not \
         this invocation's scratch root",
        kept.display()
    );
    // Same contract the oracle pins, honored by the test itself: a
    // --keep-scratch caller cleans its kept root up deliberately. Safe
    // because the path was discovered inside this test's own exclusive
    // skeleton and positively identified above.
    fs::remove_dir_all(kept).unwrap_or_else(|e| {
        panic!(
            "remove the kept --keep-scratch root {}: {e}",
            kept.display()
        )
    });
    assert!(
        !kept.exists(),
        "the kept --keep-scratch root survived its own explicit removal"
    );
    drop(parent);
}
