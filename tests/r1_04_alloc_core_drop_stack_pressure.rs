//! R1-04: `Drop for AllocCore` must not overflow a small-stack thread.
//!
//! Before the fix, `Drop for AllocCore` (`src/alloc_core/alloc_core/lifecycle.rs`)
//! collected every live segment's `(reservation, reservation_len)` into a
//! `[(*mut u8, usize); MAX_SEGMENTS]` stack array (`MAX_SEGMENTS = 4096` ->
//! 65 536 B) before freeing any of them, so the primordial segment (which
//! hosts the `SegmentTable` itself) could be freed last without the walk
//! reading unmapped memory. That array alone is bigger than a 64 KiB thread
//! stack -- a plain `AllocCore::new()` + `drop()` on such a thread overflowed
//! the stack (observed on Windows as `STATUS_STACK_OVERFLOW`, exit code
//! `0xC00000FD`), REGARDLESS of how many segments were actually live (the
//! array was always the same fixed size).
//!
//! A stack overflow is fatal to the whole PROCESS -- it cannot be caught by
//! `JoinHandle::join` on the thread that overflowed. So, like
//! `tests/r3_1_fallback_remote_free.rs`, each scenario here re-execs this test
//! binary as a fresh child PROCESS and the parent test only asserts on the
//! child's exit status: a stack overflow shows up as a failed child exit
//! rather than killing the test harness.
//!
//! Two scenarios:
//! - `bare`: construct-then-drop a single `AllocCore` (just the primordial
//!   segment) -- this alone overflowed the old fixed-size-buffer `Drop`.
//! - `many-segments`: allocate several small AND several large blocks first,
//!   so the segment walk in `drop` has multiple NON-primordial segments to
//!   free (each large allocation lands on its own freshly reserved,
//!   separately registered segment). This additionally exercises the fixed
//!   `Drop`'s free-during-walk ordering: the primordial segment (which hosts
//!   the registry `bases()` reads from) must stay mapped until every other
//!   segment has already been read and freed.

#![cfg(all(feature = "alloc-core", not(miri)))]

use std::process::Command;

const CASE_ENV: &str = "SEFER_R1_04_CHILD_CASE";
const SMALL_STACK_BYTES: usize = 64 * 1024;

fn run_child(case: &str) -> std::process::Output {
    let exe = std::env::current_exe().expect("test executable");
    // Cargo applies its target runner to this test binary, but not to a
    // subprocess it starts. Cross's aarch64 image supplies this runner (same
    // pattern as `tests/regression_r2_08_global_allocator_path_no_unwind.rs`).
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    let mut command =
        if let Ok(runner) = std::env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER") {
            let mut words = runner.split_ascii_whitespace();
            let program = words.next().expect("nonempty aarch64 target runner");
            let mut command = Command::new(program);
            command.args(words).arg(&exe);
            command
        } else {
            Command::new(&exe)
        };
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    let mut command = Command::new(&exe);
    command
        .arg("--exact")
        .arg("r1_04_drop_stack_child")
        .arg("--ignored")
        .arg("--nocapture")
        .env(CASE_ENV, case)
        .output()
        .expect("run r1-04 child")
}

fn assert_child_ok(case: &str, output: &std::process::Output) {
    assert!(
        output.status.success(),
        "case {case}: child did not exit cleanly: status={}\nstdout={}\nstderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn drop_on_64kib_stack_bare() {
    let output = run_child("bare");
    assert_child_ok("bare", &output);
}

#[test]
fn drop_on_64kib_stack_with_many_segments() {
    let output = run_child("many-segments");
    assert_child_ok("many-segments", &output);
}

#[test]
#[ignore = "run only in a fresh subprocess (see run_child)"]
fn r1_04_drop_stack_child() {
    let Ok(case) = std::env::var(CASE_ENV) else {
        return;
    };
    // Warm process-wide one-time state on this (full-size) stack first: under
    // `numa-aware`, the first `AllocCore::new()` builds numa-shim's topology
    // index inside `OnceLock::get_or_init`, which alone overflows a 64 KiB
    // stack in an unoptimized Linux build (`docs/CORRECTNESS_OPEN_ITEMS.md`
    // item 155). This test is about `Drop`'s stack use, not that initializer.
    drop(sefer_alloc::AllocCore::new().expect("warm-up reservation"));
    let handle = std::thread::Builder::new()
        .stack_size(SMALL_STACK_BYTES)
        .spawn(move || build_and_drop(&case))
        .expect("spawn 64 KiB thread");
    handle.join().expect("64 KiB thread must not panic");
}

fn build_and_drop(case: &str) {
    let mut core = sefer_alloc::AllocCore::new().expect("primordial reservation");
    if case == "many-segments" {
        use std::alloc::Layout;
        // A handful of small allocations -- served out of the primordial
        // segment's own payload, exercising the small path alongside the
        // large one below (does not by itself add extra segments).
        let small = Layout::from_size_align(16, 8).expect("small layout");
        for _ in 0..8 {
            let p = core.alloc(small);
            assert!(!p.is_null(), "small alloc failed");
        }
        // Several LARGE allocations, each landing on its own freshly
        // reserved (and separately registered) segment -- this is what
        // gives the drop's segment walk multiple non-primordial entries to
        // free, not just the primordial.
        let large = Layout::from_size_align(4 * 1024 * 1024, 8).expect("large layout");
        for _ in 0..6 {
            let p = core.alloc(large);
            assert!(!p.is_null(), "large alloc failed");
        }
    }
    drop(core);
}
