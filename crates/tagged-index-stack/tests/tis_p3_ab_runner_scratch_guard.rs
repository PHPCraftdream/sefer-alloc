//! Negative tests pinning the run-17 review P1-1 fix in
//! `crates/tagged-index-stack/scripts/tis_p3_ab_runner.mjs`.
//!
//! Review:
//! `docs/reviews/2026-09-03-164740-tagged-index-stack-review-Sol-codex-run-17.md`,
//! P1-1 — the runner's former `--out-dir` option resolved its value against
//! the repo root with no containment check, and `freshDir()` recursively
//! deleted the resolved directory on every wallclock/codegen run, so
//! `--out-dir .` deleted the entire repository.
//!
//! The fix under test REMOVED `--out-dir` and pinned the scratch root:
//! every case below must be rejected with the runner's `FATAL` diagnostics
//! (non-zero exit) BEFORE any filesystem mutation — the repo copy, the
//! dedicated scratch root, and every canary probe must survive.
//!
//! More precisely: every `--out-dir`/`--target` case below must be rejected
//! with the runner's `FATAL` diagnostics (non-zero exit) BEFORE any
//! filesystem mutation — the repo copy, the dedicated scratch root, and every
//! canary probe must survive; the junction-redirect case instead requires a
//! SUCCESSFUL run with the victim untouched.
//!
//! Run-18 review additions: this suite also pins the P1-2/P2-2 fixes — one
//! test plants a junction at the old scratch-root path and asserts the
//! victim's canary survives a real `--mode build-check` run (it MUST fail
//! against the pre-fix runner), and the suite's own temp dirs are now
//! exclusive-create nonces (PID + sub-second time + counter, `create_dir`
//! retry on `AlreadyExists`) so concurrent test processes cannot collide or
//! adopt a pre-planted path.
//!
//! Run-20 review additions (P3-3): this suite also pins the runner's
//! scratch-root LIFECYCLE contract (commit `24944ee`): `fail()` throws
//! `RunnerFatalError` instead of calling `process.exit()`, so the top-level
//! `try`/`catch`/`finally` removes the invocation's own `mkdtemp` root on
//! EVERY exit path — success, fatal error, unexpected exception — with
//! `--keep-scratch` as the deliberate opt-out. The three lifecycle tests at
//! the bottom of this file snapshot the `tis_p3_ab-*` entries under the
//! SKELETON's `target/` around one real run and assert the exact delta: a
//! successful `--mode build-check` must leave none; a deterministic fatal
//! error raised AFTER `mkdtemp` (a deliberately broken copy of `src/imp.rs`
//! in the skeleton — the run dies at build-check's post-`mkdtemp` `cargo
//! build` step, which a FATAL-message oracle proves per run) must also
//! leave none; and the same failure with `--keep-scratch` must leave
//! exactly one root, which the test then removes itself. Against the
//! pre-`24944ee` runner, the failure-path oracle fails by leaking and the
//! `--keep-scratch` oracle fails at argument parsing — each test's doc
//! comment names its specific counterfactual.
//!
//! Every case runs against a DISPOSABLE SKELETON COPY of the repo built in
//! the system temp dir (never against the real repo tree), so this suite is
//! also the counterfactual vehicle: checking out the pre-fix runner and
//! re-running it makes these tests fail by DELETING THROUGH the rejected
//! inputs (each skeleton lives under its own private temp parent, so even
//! the pre-fix runner's widest blast radius `--out-dir ..` can only destroy
//! that one test's skeleton). The skeleton includes a best-effort `git
//! init`/`add`/`commit` because the pre-fix runner's header step
//! (`capture-measurement-identity.mjs`) needs it to get past identity
//! capture and reach the destructive path; the fixed runner rejects in
//! argument parsing, long before any git call, so the suite also passes on
//! machines where git cannot create the skeleton repo.
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
/// unconditional `remove_dir_all` is safe because, after the exclusive-nonce
/// fix below, the guard provably wraps a directory this process exclusively
/// created itself (first-to-create-wins).
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

/// Exclusive temp-dir creation (run-18 review P2-2): the old names
/// (`tis_p1_1_guard_{counter}_{label}`) were predictable across processes —
/// a counter starting at zero, no PID, no time — and `create_dir_all`
/// silently accepted a pre-existing directory or symlink at that name, so a
/// concurrent `cargo test` process (or a pre-planted path) could collide and
/// `DirGuard` would then recursively delete a tree this process never owned —
/// the same "predictable path + unconditional recursive delete" hazard class
/// this suite polices in the runner itself. The nonce combines PID, a
/// sub-second wall-clock component and the process-local counter (kept: it
/// disambiguates sequential tests within one run); `create_dir` (NOT
/// `create_dir_all`) makes creation exclusive, and `AlreadyExists` means
/// "pick a new nonce and retry", never "adopt the pre-existing path".
fn exclusive_temp_dir(label: &str) -> DirGuard {
    loop {
        let subsec_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "tis_p1_1_guard_{}_{}_{}_{}",
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

/// Same as [`copy_file`], but a missing SOURCE is not fatal: `real_repo`
/// (see below) only resolves to a real workspace root when this test runs
/// from a git checkout. When it runs against the PACKAGED `.crate`
/// (`tagged-index-stack package gates` CI job, which extracts the `.crate`
/// to a standalone temp dir and runs the suite from there), `CARGO_MANIFEST_DIR`
/// IS the package root — there is no enclosing workspace two levels up, so
/// `real_repo` resolves to an unrelated ancestor directory and these
/// workspace-only files genuinely do not exist. That is fine: the fixed
/// runner rejects every case this suite pins during argument parsing, long
/// before it would ever read `capture-measurement-identity.mjs`, so the
/// skeleton does not need that file to be present for the FIXED runner's
/// behavior under test — only the (not-CI-exercised) pre-fix counterfactual
/// needs it, and that is run by hand against a real git checkout.
fn copy_file_if_present(src: &Path, dst: &Path) {
    if !src.is_file() {
        return;
    }
    copy_file(src, dst);
}

/// Builds a disposable skeleton repo in the temp dir and returns guards for
/// its private parent and the runner copy inside it. The skeleton mirrors the
/// layout the runner derives from its own location (repo root three levels
/// above `crates/tagged-index-stack/scripts/`), plus the identity-capture
/// script pair it invokes before any measurement mode — best-effort: see
/// [`copy_file_if_present`].
fn build_repo_copy(label: &str) -> (DirGuard, DirGuard, PathBuf) {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let real_repo = crate_dir
        .ancestors()
        .nth(2)
        .expect("manifest dir sits two levels below the repo root");
    let parent = exclusive_temp_dir(label);
    let root = parent.path().join("repo");
    fs::create_dir_all(&root).expect("create skeleton repo root");

    // Dummy workspace manifest: the repo-intact probes below check this file.
    fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("write dummy Cargo.toml");

    // The runner under test plus its materialization inputs, copied from the
    // REAL tree so the suite always pins the shipped files.
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
    copy_file_if_present(
        &real_repo.join("scripts/capture-measurement-identity.mjs"),
        &root.join("scripts/capture-measurement-identity.mjs"),
    );
    copy_file_if_present(
        &real_repo.join("scripts/lib.mjs"),
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
            "user.name=tis-p1-1-guard",
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

fn assert_fatal(out: &Output, what: &str) {
    assert!(
        !out.status.success(),
        "runner accepted {what} with exit 0 — the P1-1 guard regressed"
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

/// One rejected `--out-dir` case: assertion label, the value, and the
/// value's own survival oracle (run after the shared fatal/intact asserts).
type OutDirCase<'a> = (&'a str, String, Box<dyn Fn()>);

/// NONOPT-2 (run-21 review): the runner's `--out-dir` handling is
/// value-independent — `parseArgs`'s `case '--out-dir'` arm calls `fail()`
/// without ever reading the value (`tis_p3_ab_runner.mjs:189-194`) — so the
/// six former per-value tests (`out_dir_dot_is_rejected` through
/// `out_dir_symlink_escape_is_rejected_and_canary_survives`) all drove the
/// identical statement, each behind its own full skeleton build (five file
/// copies plus `git init`/`add`/`commit`). This single test loops all six
/// rejected values over ONE skeleton and re-pins the invariant per value:
/// FATAL rejection, repo intact. The case-specific survival oracles (victim
/// canary, sibling-not-created, symlink + its target's canary) are kept PER
/// VALUE rather than hoisted: each value gets its own runner invocation
/// anyway, and a future regression that starts reading the value should
/// name which value class leaked. The symlink case still skips itself where
/// the OS refuses to create one — with the same message the CI skip needle
/// greps for (`ci.yml` test-windows row) — while the other five values stay
/// pinned on such a machine.
#[test]
fn out_dir_rejection_is_value_independent() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("out_dir_values");
    let root_str = root_guard.path().to_string_lossy().to_string();

    // Fixtures for the values that point OUTSIDE the skeleton, planted the
    // same way the six former tests planted them.
    let victim = exclusive_temp_dir("victim");
    fs::write(victim.path().join("canary.txt"), "unrelated to the repo")
        .expect("write victim canary");
    let victim_str = victim.path().to_string_lossy().to_string();

    let sibling = DirGuard::new(parent.path().join(format!("sibling_{}", next_uid())));
    let sibling_str = sibling.path().to_string_lossy().to_string();

    let real = exclusive_temp_dir("real");
    fs::write(real.path().join("canary.txt"), "behind the symlink")
        .expect("write symlink-target canary");
    // run-19 review P2-1: the symlink destination must NOT already exist —
    // `symlink`/`symlink_dir`/`mklink /J` all fail if the target path exists,
    // and `exclusive_temp_dir("link")` creates its directory before returning,
    // so a derived, never-created child of the guarded parent is used instead.
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

/// Run-18 review P1-2: the OLD fixed scratch root `<repo>/target/tis_p3_ab`
/// was a predictable, reused path, and `freshDir()`'s containment check was
/// purely lexical — a directory junction/symlink planted at that exact path
/// made `<scratch>/build-check` look "inside" the root while `rmSync`
/// resolved the reparse point and destroyed the REAL external directory.
/// This test plants exactly that redirect in a disposable skeleton (the
/// skeleton's `target/tis_p3_ab` → an outside "victim" dir holding a canary
/// at `victim/build-check/canary.txt` — precisely the child path the
/// build-check mode clears), runs the REAL runner in build-check mode, and
/// asserts the runner succeeds AND the canary survives. It MUST fail against
/// the pre-fix runner (the canary is deleted through the junction) and pass
/// once the scratch root is a fresh, unpredictable, exclusively-created
/// `mkdtemp` sibling that could not have been pre-planted.
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
        "runner failed outright against a junction at the old scratch-root path \
         (expected: succeed via a fresh mkdtemp sibling); stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        victim.join("build-check").join("canary.txt").is_file(),
        "the victim directory behind the <target>/tis_p3_ab junction was deleted — \
         the runner still follows a reparse point planted at the scratch root (run-18 P1-2)"
    );
}

// ── Run-20 review P3-3: scratch-root lifecycle oracles ─────────────────────
//
// The tests above pin CONTAINMENT (what the runner must refuse to delete);
// the three tests below pin the CLEANUP LIFECYCLE (what the runner must not
// leave behind). Each snapshots the `tis_p3_ab-*` entries under the
// skeleton's `target/` immediately before and after one real runner
// invocation and asserts the exact delta.

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
/// toolchain) — strictly after `makeScratchRoot()`, which is exactly the
/// ordering the lifecycle oracles test.
fn break_skeleton_imp_rs(skeleton_root: &Path) {
    let imp = skeleton_root.join("crates/tagged-index-stack/src/imp.rs");
    let mut src = fs::read_to_string(&imp).expect("read skeleton imp.rs copy");
    src.push_str("\n__tis_p3_3_scratch_guard_deliberate_syntax_error__\n");
    fs::write(&imp, src).expect("append deliberate syntax error to skeleton imp.rs copy");
}

/// For the failure-path lifecycle oracles: the run must be fatal AND the
/// FATAL must come from build-check's post-`mkdtemp` `cargo build` step —
/// not from argument parsing or any pre-scratch validation. Without this
/// mechanism oracle, a "no new scratch root" assertion could pass vacuously
/// (a run that dies before `mkdtemp` also leaves nothing behind).
fn assert_fatal_from_post_mkdtemp_cargo_build(out: &Output) {
    assert!(
        !out.status.success(),
        "runner exited 0 against a deliberately broken imp.rs — the controlled \
         post-mkdtemp failure did not happen"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("tis_p3_ab_runner: FATAL"),
        "broken-source run failed without the runner's FATAL diagnostics; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("cargo build failed"),
        "FATAL did not come from the post-mkdtemp cargo-build step — a pre-scratch death \
         would make the no-leak assertion vacuous; stderr:\n{stderr}"
    );
}

/// Run-20 review P3-3, oracle 1: a SUCCESSFUL `--mode build-check` run must
/// leave no new `tis_p3_ab-*` scratch root under the repo's `target/` — the
/// top-level `finally` cleans up on the success path too. Counterfactual: a
/// refactor that drops the cleanup entirely, skips it on the success path,
/// or defaults `--keep-scratch` to on fails here by leaving exactly the root
/// the run created. (The pre-`24944ee` runner PASSES this oracle: its
/// success-only cleanup still ran on success — the failure-path oracles
/// below are what catch that revert.)
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
        "a successful --mode build-check left new tis_p3_ab-* scratch root(s) under \
         <repo>/target/ — the top-level finally's cleanup regressed on the success path \
         (run-20 review P3-3)"
    );
    drop(parent);
}

/// Run-20 review P3-3, oracle 2: a fatal error raised AFTER the `mkdtemp`
/// scratch root already exists must still leave no new `tis_p3_ab-*` root —
/// exactly the leak commit `24944ee` fixed (the pre-fix `fail()` called
/// `process.exit()`, which terminates on the spot and skips every `finally`,
/// so ANY expected build/oracle failure leaked the whole scratch tree under
/// `target/`). Counterfactual: reverting `fail()` to `process.exit(1)`, or
/// moving cleanup back to the success path only, fails here by leaving
/// exactly the root the failed run created. The failure itself is injected
/// deterministically AFTER `mkdtemp` (see [`break_skeleton_imp_rs`]) and its
/// post-`mkdtemp` origin is proven per-run by
/// [`assert_fatal_from_post_mkdtemp_cargo_build`].
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
        "a fatal error raised after scratch-root creation left new tis_p3_ab-* root(s) \
         under <repo>/target/ — cleanup no longer runs on the fatal path (the 24944ee \
         leak regression; run-20 review P3-3)"
    );
    drop(parent);
}

/// Run-20 review P3-3, oracle 3: the same controlled post-`mkdtemp` failure
/// WITH `--keep-scratch` must leave exactly ONE new `tis_p3_ab-*` root, and
/// it must be THIS invocation's own — proven by the presence of build-check
/// mode's fixed `build-check` child inside it. The test then removes the
/// kept root itself (it lives inside this test's exclusive skeleton, and was
/// created by the runner process this test spawned), so a `--keep-scratch`
/// run never litters any real `target/` across test runs. Counterfactuals:
/// removing the `--keep-scratch` option (the pre-`24944ee` CLI shape) fails
/// here at argument parsing — no root is created at all; making the cleanup
/// ignore the flag and remove anyway fails here with zero new roots.
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
        "--keep-scratch must leave exactly ONE new tis_p3_ab-* root for this invocation \
         (the skeleton starts with none: {before:?}); got {after:?} — the flag was \
         rejected at argument parsing (no root at all) or cleanup ran despite it \
         (run-20 review P3-3)"
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
