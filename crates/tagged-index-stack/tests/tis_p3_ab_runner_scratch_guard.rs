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
    Command::new("node")
        .arg(runner)
        .args(["--mode", "build-check"])
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

#[test]
fn out_dir_dot_is_rejected() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("dot");
    assert_fatal(&run_codegen(&runner, &["--out-dir", "."]), "--out-dir .");
    assert_repo_intact(root_guard.path(), &runner, "--out-dir .");
    drop(parent);
}

#[test]
fn out_dir_dotdot_is_rejected() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("dotdot");
    assert_fatal(&run_codegen(&runner, &["--out-dir", ".."]), "--out-dir ..");
    assert_repo_intact(root_guard.path(), &runner, "--out-dir ..");
    drop(parent);
}

#[test]
fn out_dir_absolute_repo_root_is_rejected() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("absroot");
    let root_str = root_guard.path().to_string_lossy().to_string();
    assert_fatal(
        &run_codegen(&runner, &["--out-dir", &root_str]),
        "--out-dir <repo root as absolute path>",
    );
    assert_repo_intact(
        root_guard.path(),
        &runner,
        "--out-dir <repo root as absolute path>",
    );
    drop(parent);
}

#[test]
fn out_dir_absolute_temp_victim_is_rejected_and_canary_survives() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("victim");
    let victim = exclusive_temp_dir("victim");
    fs::write(victim.path().join("canary.txt"), "unrelated to the repo")
        .expect("write victim canary");
    let victim_str = victim.path().to_string_lossy().to_string();
    assert_fatal(
        &run_codegen(&runner, &["--out-dir", &victim_str]),
        "--out-dir <absolute temp dir unrelated to the repo>",
    );
    assert_repo_intact(
        root_guard.path(),
        &runner,
        "--out-dir <absolute temp dir unrelated to the repo>",
    );
    assert!(
        victim.path().join("canary.txt").is_file(),
        "the absolute out-dir's canary file was deleted — the runner still clears a user-supplied directory"
    );
    drop(parent);
}

#[test]
fn out_dir_repo_sibling_is_rejected_and_not_created() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("sibling");
    let sibling = DirGuard::new(parent.path().join(format!("sibling_{}", next_uid())));
    let sibling_str = sibling.path().to_string_lossy().to_string();
    assert_fatal(
        &run_codegen(&runner, &["--out-dir", &sibling_str]),
        "--out-dir <sibling directory of the repo>",
    );
    assert_repo_intact(
        root_guard.path(),
        &runner,
        "--out-dir <sibling directory of the repo>",
    );
    assert!(
        !sibling.path().exists(),
        "the runner created the sibling directory it was supposed to reject"
    );
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
fn out_dir_symlink_escape_is_rejected_and_canary_survives() {
    if !node_available() {
        eprintln!("skipping: node not on PATH");
        return;
    }
    let (parent, root_guard, runner) = build_repo_copy("symlink");
    let real = exclusive_temp_dir("real");
    fs::write(real.path().join("canary.txt"), "behind the symlink")
        .expect("write symlink-target canary");
    let link = exclusive_temp_dir("link");
    if !make_dir_symlink(link.path(), real.path()) {
        eprintln!("skipping: directory symlinks/junctions unavailable in this environment");
        drop(parent);
        return;
    }
    let link_str = link.path().to_string_lossy().to_string();
    assert_fatal(
        &run_codegen(&runner, &["--out-dir", &link_str]),
        "--out-dir <symlink pointing outside the scratch root>",
    );
    assert_repo_intact(
        root_guard.path(),
        &runner,
        "--out-dir <symlink pointing outside the scratch root>",
    );
    assert!(
        link.path().exists(),
        "the symlink itself was deleted by the runner while rejecting its --out-dir"
    );
    assert!(
        real.path().join("canary.txt").is_file(),
        "the symlink target's canary file was deleted by the runner while rejecting its --out-dir"
    );
    drop(parent);
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
