#![cfg(feature = "alloc-core")]

// Compile the actual primitive in this integration crate. These aliases are
// pinned to the production geometry, not independent test constants.
mod alloc_core {
    // Mirrors `alloc_core::state_byte::StateByte::shadow_reject` for the source-included
    // `SidecarBitmap::publish` (a debug-build abort for a rejected misuse; loom models do not take it).
    #[allow(dead_code)]
    pub mod state_byte {
        pub struct StateByte;
        impl StateByte {
            pub fn shadow_reject(_site: &'static [u8]) {}
        }
    }
    // Mirrors `alloc_core::fatal` (`src/alloc_core/platform/fatal.rs`) for the
    // source-included primitives.
    pub mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            use std::io::Write;
            let _ = std::io::stderr().write_all(msg);
            std::process::abort()
        }
    }
    pub mod os {
        pub const SEGMENT: usize = sefer_alloc::SegmentLayout::SEGMENT;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = sefer_alloc::SegmentLayout::MIN_BLOCK;
        pub const SMALL_CLASS_COUNT: usize = sefer_alloc::SegmentLayout::SIZE_CLASS_TABLE.len();
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
}

// Allocation-discovery routing is outside this source-included primitive test.
// Included scanner siblings unused by this test (same pattern as
// `tests/class_leaves_publication_miri.rs`).
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

// Ghost checks in the included production files (docs/GHOST_CHECKS_PLAN.md):
// ghost builds (miri, --cfg sefer_ghost) compile them.
#[allow(dead_code, unused_imports)]
#[path = "../src/ghost/mod.rs"]
mod ghost;

#[path = "r6_sidecar_bitmap/child_process.rs"]
mod child_process;

mod tests {
    use super::child_process::{run_child_bounded, ChildRun};
    use super::remote_bitmap::{BitmapRecord, SidecarBitmap};
    use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
    use std::sync::{Arc, Barrier};

    fn backing() -> (Vec<AtomicU64>, Vec<AtomicU8>) {
        (
            (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
            (0..SidecarBitmap::GRANULES)
                .map(|_| AtomicU8::new(0))
                .collect(),
        )
    }

    const ACK_ENV: &str = "SEFER_R6_SIDECAR_INVALID_CLASS_ACK";
    const NONCE_ENV: &str = "SEFER_R6_SIDECAR_INVALID_CLASS_NONCE";
    const CHILD_TEST: &str = "tests::r6_invalid_class_aborts_without_unwind";
    // The exact production diagnostic fired by the mirrored `alloc_core::fatal`
    // from `bitmap_cut.rs`'s invalid sidecar class encoding check.
    const FATAL_DIAGNOSTIC: &str = "bitmap_cut: invalid sidecar class encoding";
    #[cfg(unix)]
    const SIGABRT: i32 = 6;
    // The fixed gate makes fast children consume this full budget; process and
    // filesystem calls themselves have no absolute OS deadline guarantee.
    const CHILD_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);
    const CHILD: &str = "SEFER_R6_SIDECAR_INVALID_CLASS_CHILD";

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Termination {
        Success,
        Code(i32),
        // Constructed only on unix (signal-based exit statuses).
        #[allow(dead_code)]
        Signal(i32),
    }

    fn termination_of(status: &std::process::ExitStatus) -> Termination {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                return Termination::Signal(signal);
            }
        }
        if let Some(code) = status.code() {
            return Termination::Code(code);
        }
        Termination::Success
    }

    // Only the documented allocator abort (`std::process::abort` in the mirrored
    // `alloc_core::fatal`, fired from the production `BitmapCut::pop`
    // invalid-encoding check) is the expected termination; every other outcome
    // fails closed.
    fn expected_abort_termination(t: Termination) -> Result<(), String> {
        match t {
            Termination::Success => Err("returned normally".to_string()),
            Termination::Code(101) => Err("Rust panic unwind (exit 101)".to_string()),
            // Exit 2 means the runner never exec'd the child: the raw execve of
            // an unrunnable child image fell back to running the ELF as a shell
            // script (ENOEXEC), and the shell rejected it.
            Termination::Code(2) => {
                Err("shell-as-script ENOEXEC fallback for an unrunnable child image".to_string())
            }
            #[cfg(unix)]
            Termination::Signal(signal) if signal == SIGABRT => Ok(()),
            // Windows: `process::abort` on MSVC raises the fast-fail interrupt
            // (STATUS_STACK_BUFFER_OVERRUN == -1073740791); the CRT abort path
            // exits with code 3. Both are accepted abort signatures.
            #[cfg(windows)]
            Termination::Code(code) if code == 3 || code == -1073740791 => Ok(()),
            other => Err(format!("unexpected termination {other:?}")),
        }
    }

    // Only triples this repository's CI actually configures runners for; other
    // targets use the native direct exec contract.
    fn runner_env_var_name() -> Option<&'static str> {
        if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            Some("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_RUNNER")
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER")
        } else {
            None
        }
    }

    // Whitespace split only: no shell, no quote handling. Cargo runner values
    // used here are a plain command plus arguments.
    fn split_runner(runner: &str) -> Result<Vec<String>, String> {
        let parts: Vec<String> = runner.split_whitespace().map(str::to_string).collect();
        if parts.is_empty() {
            Err("configured target runner is blank".to_string())
        } else {
            Ok(parts)
        }
    }

    fn child_runner() -> Result<Option<Vec<String>>, String> {
        match runner_env_var_name() {
            None => Ok(None),
            Some(name) => match std::env::var_os(name) {
                None => Ok(None),
                Some(raw) => {
                    if raw.is_empty() {
                        Err(format!("configured target runner {name} is blank"))
                    } else {
                        split_runner(&raw.to_string_lossy()).map(Some)
                    }
                }
            },
        }
    }

    // Naming mints a unique label; exclusivity comes from create_dir below, not
    // from unpredictability. A clock failure only degrades the label, never the
    // ownership guarantee.
    fn mint_nonce() -> String {
        static ACK_COUNTER: core::sync::atomic::AtomicU64 = AtomicU64::new(0);
        let counter = ACK_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        format!("{}-{}-{}", std::process::id(), nanos, counter)
    }

    // Exclusive parent-owned launch reservation: create_dir fails when the path
    // already exists, so a successful reservation proves THIS launch owns a fresh
    // empty directory; the ack can only appear inside it during this launch.
    fn reserve_ack_dir(nonce: &str) -> std::io::Result<std::path::PathBuf> {
        let dir = std::env::temp_dir().join(format!("sefer-r6-sidecar-invalid-class-{nonce}"));
        std::fs::create_dir(&dir)?;
        Ok(dir)
    }

    // Hygiene only: correctness relies on the exclusive reservation plus
    // create_new/verify_ack, never on cleanup success.
    struct AckDirGuard(std::path::PathBuf);

    impl Drop for AckDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // The parent passes the nonce to the child so both sides can build the same
    // exact expected content.
    fn expected_ack_content(mode: &str, nonce: &str) -> String {
        format!("{mode}\n{nonce}\n")
    }

    // Fail-closed creation: an existing file makes the open fail, and the child
    // then panics (exit 101), which the parent rejects.
    fn write_fresh_ack(path: &std::path::Path, content: &str) -> std::io::Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(content.as_bytes())
    }

    fn verify_ack(path: &std::path::Path, expected: &str) -> Result<(), String> {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                if content == expected {
                    Ok(())
                } else {
                    Err(format!(
                        "ack content mismatch: expected {expected:?}, found {content:?}"
                    ))
                }
            }
            Err(error) => Err(format!("ack unreadable or missing: {error}")),
        }
    }

    fn validate_fatal_diagnostic(stderr: &str) -> Result<(), String> {
        if stderr
            .lines()
            .any(|line| line.trim_end_matches('\r') == FATAL_DIAGNOSTIC)
        {
            Ok(())
        } else {
            Err(format!(
                "child stderr must contain the exact production diagnostic line {FATAL_DIAGNOSTIC:?}"
            ))
        }
    }

    // Assembled acceptance gate: termination, activation, and diagnostic must all
    // accept; exercised by the parent loop and unit-tested directly.
    fn acceptance_verdict(
        termination: Result<(), String>,
        activation: Result<(), String>,
        diagnostic: Result<(), String>,
    ) -> Result<(), String> {
        termination?;
        activation?;
        diagnostic?;
        Ok(())
    }

    // The platform's accepted CRT exit 3 on Windows; fast-fail is checked separately.
    fn platform_abort_termination() -> Termination {
        #[cfg(unix)]
        {
            Termination::Signal(SIGABRT)
        }
        #[cfg(windows)]
        {
            Termination::Code(3)
        }
    }

    #[test]
    fn r18_concurrent_publications_survive_empty_cuts_and_resume() {
        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        assert!(map.issue(1024, 3));
        assert!(map.issue(1040, 4));
        let mut first = map.scan_from(2048, 1).unwrap();
        let mut records = Vec::new();
        std::thread::scope(|scope| {
            let map_ref = &map;
            let a = scope.spawn(move || assert!(map_ref.publish(1024)));
            let b = scope.spawn(move || assert!(map_ref.publish(1040)));
            let mut cut = first.next_cut().unwrap();
            while let Some(record) = cut.pop() {
                records.push(record);
            }
            assert!(first.next_cut().is_none());
            a.join().unwrap();
            b.join().unwrap();
        });
        let mut later = map.scan_from(2048, 1).unwrap();
        let mut cut = later.next_cut().unwrap();
        while let Some(record) = cut.pop() {
            records.push(record);
        }
        records.sort_by_key(|record| record.offset);
        assert_eq!(
            records,
            [
                BitmapRecord {
                    offset: 1024,
                    class: 3
                },
                BitmapRecord {
                    offset: 1040,
                    class: 4
                },
            ],
            "publication lost or duplicated across cuts"
        );
        assert!(later.next_cut().is_none());
        assert!(map
            .scan_from(2048, 1)
            .unwrap()
            .next_cut()
            .unwrap()
            .is_empty());
        assert!(map.issue(1024, 5));
        assert!(map.publish(1024));
        let mut valid = map.scan_from(2048, 1).unwrap().next_cut().unwrap();
        assert_eq!(valid.pop().unwrap().class, 5, "nonempty control must drain");
        assert!(valid.pop().is_none());
    }

    #[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
    #[test]
    fn r18_candidate_skips_empty_exchange_and_drains_exact_record() {
        use super::remote_bitmap::BitmapScan;

        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        BitmapScan::measure_scans(true, false);
        let mut empty = map.scan(1024).unwrap();
        assert!(empty.next_cut().unwrap().is_empty());
        assert!(empty.next_cut().is_none());
        // An unconditional exchange would report [1, 1, 1, 0].
        assert_eq!(&BitmapScan::measure_scans(true, false)[..4], &[1, 1, 0, 0]);

        assert!(map.issue(64, 3));
        assert!(map.publish(64));
        let mut scan = map.scan(1024).unwrap();
        let mut cut = scan.next_cut().unwrap();
        assert_eq!(
            cut.pop(),
            Some(BitmapRecord {
                offset: 64,
                class: 3
            })
        );
        assert!(cut.pop().is_none());
        assert!(scan.next_cut().is_none());
        assert_eq!(&BitmapScan::measure_scans(false, true)[..4], &[1, 0, 1, 1]);
        assert!(map.scan(1024).unwrap().next_cut().unwrap().is_empty());
        assert_eq!(BitmapScan::measure_scans(false, true), [0; 5]);
    }

    #[test]
    fn r6_geometry_and_fixed_high_water() {
        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        assert_eq!(SidecarBitmap::WORDS, 4096);
        assert_eq!(SidecarBitmap::GRANULES, 262_144);
        assert!(!map.issue(1, 0));
        assert!(!map.publish(4 * 1024 * 1024));
        assert!(!map.issue(16, u8::MAX));
        assert!(map.issue(1024, 0));
        assert!(map.issue(4 * 1024 * 1024 - 16, 1));
        assert!(map.publish(1024));
        assert!(map.publish(4 * 1024 * 1024 - 16));
        let mut scan = map.scan(2048).unwrap();
        let mut records = Vec::new();
        let mut cuts = 0;
        while let Some(mut cut) = scan.next_cut() {
            cuts += 1;
            while let Some(record) = cut.pop() {
                records.push(record);
            }
        }
        assert_eq!(cuts, 2);
        assert_eq!(
            records,
            [BitmapRecord {
                offset: 1024,
                class: 0
            }]
        );
        let mut full = map.scan(4 * 1024 * 1024).unwrap();
        let mut final_record = None;
        let mut visits = 0;
        while let Some(mut cut) = full.next_cut() {
            visits += 1;
            if let Some(record) = cut.pop() {
                final_record = Some(record);
            }
        }
        assert_eq!(visits, 4096);
        assert_eq!(
            final_record,
            Some(BitmapRecord {
                offset: 4 * 1024 * 1024 - 16,
                class: 1
            })
        );
    }

    #[test]
    fn r6_paused_before_and_after_terminal_publish() {
        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        assert!(map.issue(16, 3));
        let mut before = map.scan(1024).unwrap();
        assert!(before.next_cut().unwrap().is_empty());
        let entered = Arc::new(Barrier::new(2));
        let resume = Arc::new(Barrier::new(2));
        std::thread::scope(|scope| {
            let entered_producer = Arc::clone(&entered);
            let resume_producer = Arc::clone(&resume);
            let map_ref = &map;
            scope.spawn(move || {
                entered_producer.wait();
                resume_producer.wait();
                assert!(map_ref.publish(16));
            });
            entered.wait();
            let cut = map.scan(1024).unwrap().next_cut().unwrap();
            assert!(cut.is_empty());
            resume.wait();
        });
        assert!(map.issue(32, 4));
        let published = Arc::new(Barrier::new(2));
        let return_now = Arc::new(Barrier::new(2));
        std::thread::scope(|scope| {
            let published_producer = Arc::clone(&published);
            let return_producer = Arc::clone(&return_now);
            let map_ref = &map;
            scope.spawn(move || {
                assert!(map_ref.publish(32));
                published_producer.wait();
                return_producer.wait();
            });
            published.wait();
            let mut cut = map.scan(1024).unwrap().next_cut().unwrap();
            assert_eq!(
                cut.pop().unwrap(),
                BitmapRecord {
                    offset: 16,
                    class: 3
                }
            );
            assert_eq!(
                cut.pop().unwrap(),
                BitmapRecord {
                    offset: 32,
                    class: 4
                }
            );
            assert!(cut.pop().is_none());
            return_now.wait();
        });
    }

    #[test]
    fn r6_reuse_bit_and_republish_class() {
        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        let offset = 64;
        assert!(map.issue(offset, 2));
        assert!(map.publish(offset));
        let mut first = map.scan(1024).unwrap().next_cut().unwrap();
        assert_eq!(first.pop().unwrap().class, 2);
        assert!(first.pop().is_none());
        assert!(map.issue(offset, 5));
        assert!(map.publish(offset));
        let mut second = map.scan(1024).unwrap().next_cut().unwrap();
        assert_eq!(second.pop().unwrap().class, 5);
        assert!(second.pop().is_none());
    }

    #[test]
    fn r6_invalid_class_aborts_without_unwind() {
        if let Some(mode) = std::env::var_os(CHILD) {
            let mode = mode.to_string_lossy().into_owned();
            let (words, classes) = backing();
            let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
            match mode.as_str() {
                "pre:255" => classes[1].store(u8::MAX, Ordering::Relaxed),
                // The class is cleared after a valid publication: the owner's own
                // check still rejects the record.
                "post:0" => {
                    classes[1].store(1, Ordering::Relaxed);
                    assert!(map.publish(16));
                    classes[1].store(0, Ordering::Relaxed);
                }
                "hold:park" | "return:normal" => {}
                other => panic!("unknown child mode {other}"),
            }
            if mode == "pre:255" {
                assert!(map.publish(16));
            }
            // Written after the invalid class state is fully prepared and
            // immediately before the scan whose `BitmapCut::pop` triggers the
            // invalid-encoding `fatal` (abort), so an ack with matching content
            // proves the child actually entered and set up the invalid-class
            // Rust path.
            let ack = std::env::var_os(ACK_ENV)
                .unwrap_or_else(|| panic!("child mode requires {ACK_ENV} from its parent"));
            let nonce = std::env::var_os(NONCE_ENV)
                .unwrap_or_else(|| panic!("child mode requires {NONCE_ENV} from its parent"));
            let ack = std::path::PathBuf::from(ack);
            let nonce = nonce.to_string_lossy().into_owned();
            write_fresh_ack(&ack, &expected_ack_content(&mode, &nonce))
                .expect("write fresh child activation ack");
            if mode == "hold:park" {
                // There are no child output streams: parent routes both to null.
                loop {
                    std::thread::park();
                }
            }
            if mode == "return:normal" {
                return;
            }
            let mut cut = map.scan(1024).unwrap().next_cut().unwrap();
            let _ = cut.pop();
            return;
        }

        // A zero class at publication time is dropped by the producer (R12-01):
        // nothing reaches the owner. In-process: it needs no child.
        let (words, classes) = backing();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();
        classes[1].store(0, Ordering::Relaxed);
        assert!(!map.publish(16), "zero class must be dropped at publish");

        let executable = std::env::current_exe().unwrap();
        let runner = child_runner()
            .unwrap_or_else(|error| panic!("invalid-class child runner misconfiguration: {error}"));
        for mode in ["pre:255", "post:0"] {
            let nonce = mint_nonce();
            let dir = reserve_ack_dir(&nonce).unwrap_or_else(|error| {
                panic!("could not reserve exclusive invalid-class launch directory: {error}")
            });
            let _ack_guard = AckDirGuard(dir.clone());
            let ack = dir.join("ack");
            assert!(
                !ack.exists(),
                "reserved launch directory must start without an ack"
            );
            let mut command = invalid_class_child_command(&executable, runner.as_deref());
            command
                .env(CHILD, mode)
                .env(ACK_ENV, &ack)
                .env(NONCE_ENV, &nonce);
            let outcome = run_child_bounded(&mut command, &dir, CHILD_BUDGET, true)
                .unwrap_or_else(|error| {
                    panic!(
                        "failed supervising invalid class {mode} child through runner {:?}: {error}",
                        runner.as_deref(),
                    )
                });
            let termination_result = match outcome {
                ChildRun::Completed { status, stderr } => {
                    let termination = termination_of(&status);
                    let stderr_text = String::from_utf8_lossy(&stderr).into_owned();
                    let termination = expected_abort_termination(termination);
                    let activation = verify_ack(&ack, &expected_ack_content(mode, &nonce));
                    let diagnostic = validate_fatal_diagnostic(&stderr_text);
                    acceptance_verdict(termination, activation, diagnostic).map_err(|rejection| {
                        format!(
                            "invalid class {mode} child rejected through runner {:?}: \
                             {rejection}; child stderr:\n{stderr_text}",
                            runner.as_deref(),
                        )
                    })
                }
                ChildRun::BudgetExceeded { status, stderr } => {
                    panic!(
                        "invalid class {mode} child exceeded its budget; killed and reaped as \
                         {status:?}; stderr: {}",
                        String::from_utf8_lossy(&stderr)
                    )
                }
            };
            if let Err(rejection) = termination_result {
                panic!("{rejection}");
            }
        }
    }

    // Builds argv per transport: the runner program plus its arguments, then the
    // child executable, then the test filters. The native transport must NOT get
    // the executable again as an extra positional libtest filter.
    fn invalid_class_child_command(
        executable: &std::path::Path,
        runner: Option<&[String]>,
    ) -> std::process::Command {
        let mut command = match runner {
            Some(runner_args) => {
                let mut command = std::process::Command::new(&runner_args[0]);
                command.args(&runner_args[1..]);
                command.arg(executable);
                command
            }
            None => std::process::Command::new(executable),
        };
        command.args(["--exact", CHILD_TEST, "--nocapture"]);
        command
    }

    fn command_args(command: &std::process::Command) -> Vec<String> {
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn r6_invalid_class_child_command_native_argv_has_no_extra_filter() {
        assert_eq!(CHILD_TEST, "tests::r6_invalid_class_aborts_without_unwind");
        let exe = std::path::Path::new("/fake/target/debug/deps/r6_sidecar_bitmap-abc123");
        let native = invalid_class_child_command(exe, None);
        assert_eq!(native.get_program(), exe.as_os_str());
        assert_eq!(
            command_args(&native),
            vec!["--exact", CHILD_TEST, "--nocapture"]
        );
    }

    #[test]
    fn r6_invalid_class_child_command_runner_argv_prepends_transport() {
        let exe = std::path::Path::new("/fake/target/debug/deps/r6_sidecar_bitmap-abc123");
        let runner = vec!["/linux-runner".to_string(), "aarch64".to_string()];
        let via_runner = invalid_class_child_command(exe, Some(&runner));
        assert_eq!(via_runner.get_program().to_string_lossy(), "/linux-runner");
        assert_eq!(
            command_args(&via_runner),
            vec![
                "aarch64".to_string(),
                "/fake/target/debug/deps/r6_sidecar_bitmap-abc123".to_string(),
                "--exact".to_string(),
                CHILD_TEST.to_string(),
                "--nocapture".to_string(),
            ]
        );
    }

    #[test]
    fn r6_invalid_class_ack_verification_rejects_missing_stale_and_mismatched() {
        let nonce = mint_nonce();
        let dir = reserve_ack_dir(&nonce)
            .unwrap_or_else(|error| panic!("could not reserve launch directory: {error}"));
        let _ack_guard = AckDirGuard(dir.clone());
        let ack = dir.join("ack");
        let expected = expected_ack_content("pre:255", &nonce);
        // (a) Never-created path: unreadable or missing must be rejected.
        assert!(verify_ack(&ack, &expected).is_err());
        // (b) A stale pre-existing file with different content must be rejected.
        std::fs::write(&ack, "stale\n").expect("write stale ack selftest file");
        assert!(verify_ack(&ack, &expected).is_err());
        // (c) The exact expected content is accepted.
        std::fs::write(&ack, &expected).expect("write matching ack selftest file");
        assert_eq!(verify_ack(&ack, &expected), Ok(()));
    }

    #[test]
    fn r6_invalid_class_fatal_diagnostic_requires_production_site() {
        assert_eq!(
            validate_fatal_diagnostic(
                "running 1 test\nbefore bitmap_cut: invalid sidecar class encoding after\nok",
            ),
            Err(format!(
                "child stderr must contain the exact production diagnostic line {FATAL_DIAGNOSTIC:?}"
            )),
        );
        // Exact whole line among surrounding runner lines is accepted.
        assert_eq!(
            validate_fatal_diagnostic(
                "running 1 test\nbitmap_cut: invalid sidecar class encoding\nok"
            ),
            Ok(())
        );
        // CRLF line endings are tolerated.
        assert_eq!(
            validate_fatal_diagnostic("bitmap_cut: invalid sidecar class encoding\r\nok\r\n"),
            Ok(())
        );
        // Extra text on the same line is rejected.
        assert!(
            validate_fatal_diagnostic("bitmap_cut: invalid sidecar class encoding (extra)")
                .is_err()
        );
        assert!(validate_fatal_diagnostic(&format!("\"{FATAL_DIAGNOSTIC}\"\n")).is_err());
        assert!(
            validate_fatal_diagnostic("bitmap_scan: sidecar scan iterator exhausted early")
                .is_err()
        );
        assert!(validate_fatal_diagnostic("").is_err());
    }

    #[test]
    fn r6_invalid_class_acceptance_gates() {
        let nonce = mint_nonce();
        let dir = reserve_ack_dir(&nonce)
            .unwrap_or_else(|error| panic!("could not reserve launch directory: {error}"));
        let _ack_guard = AckDirGuard(dir.clone());
        let ack = dir.join("ack");
        let missing_ack = dir.join("missing-ack");
        let expected = expected_ack_content("pre:255", &nonce);
        write_fresh_ack(&ack, &expected).expect("write valid acceptance-gate activation");
        let activation = verify_ack(&ack, &expected);
        let diagnostic = validate_fatal_diagnostic(FATAL_DIAGNOSTIC);

        // Missing activation is rejected against the real verification gate.
        assert!(acceptance_verdict(
            expected_abort_termination(platform_abort_termination()),
            verify_ack(&missing_ack, &expected),
            diagnostic.clone(),
        )
        .is_err());
        // A real unrelated diagnostic is rejected with an otherwise valid pair.
        assert!(acceptance_verdict(
            expected_abort_termination(platform_abort_termination()),
            activation.clone(),
            validate_fatal_diagnostic("unrelated diagnostic"),
        )
        .is_err());
        for rejected in [
            Termination::Success,
            Termination::Code(0),
            Termination::Code(2),
            Termination::Code(101),
        ] {
            assert!(
                acceptance_verdict(
                    expected_abort_termination(rejected),
                    activation.clone(),
                    diagnostic.clone(),
                )
                .is_err(),
                "accepted termination {rejected:?}"
            );
        }
        assert_eq!(
            acceptance_verdict(
                expected_abort_termination(platform_abort_termination()),
                verify_ack(&ack, &expected),
                validate_fatal_diagnostic(FATAL_DIAGNOSTIC),
            ),
            Ok(())
        );
    }

    #[test]
    fn r6_invalid_class_ack_creation_is_exclusive() {
        let nonce = mint_nonce();
        let dir = reserve_ack_dir(&nonce)
            .unwrap_or_else(|error| panic!("could not reserve launch directory: {error}"));
        let _ack_guard = AckDirGuard(dir.clone());
        let ack = dir.join("ack");
        let content = expected_ack_content("pre:255", &nonce);
        assert!(write_fresh_ack(&ack, &content).is_ok());
        // create_new refuses an existing resource.
        assert!(write_fresh_ack(&ack, &content).is_err());
        // The exclusive reservation refuses a second claim on the same nonce.
        assert!(reserve_ack_dir(&nonce).is_err());
        assert_eq!(verify_ack(&ack, &content), Ok(()));
    }

    #[test]
    fn r6_invalid_class_budget_supervises_live_child_without_output_streams() {
        let executable = std::env::current_exe().unwrap();
        let runner = child_runner()
            .unwrap_or_else(|error| panic!("invalid-class child runner misconfiguration: {error}"));
        let nonce = mint_nonce();
        let dir = reserve_ack_dir(&nonce)
            .unwrap_or_else(|error| panic!("could not reserve launch directory: {error}"));
        let _ack_guard = AckDirGuard(dir.clone());
        let ack = dir.join("ack");
        let expected = expected_ack_content("hold:park", &nonce);
        let mut command = invalid_class_child_command(&executable, runner.as_deref());
        command
            .env(CHILD, "hold:park")
            .env(ACK_ENV, &ack)
            .env(NONCE_ENV, &nonce);
        // This counterfactual has no output streams at all (both are null), so
        // completion cannot depend on closing streams or inherited pipe EOF.
        let outcome = run_child_bounded(
            &mut command,
            &dir,
            std::time::Duration::from_secs(10),
            false,
        )
        .unwrap_or_else(|error| panic!("hold:park child supervision failed: {error}"));
        match outcome {
            ChildRun::BudgetExceeded { status, .. } => {
                assert!(
                    !status.success(),
                    "budget-killed child must not report success: {status:?}"
                );
                assert_eq!(
                    verify_ack(&ack, &expected),
                    Ok(()),
                    "child must activate before parking"
                );
            }
            ChildRun::Completed { .. } => panic!("live child must not complete before the budget"),
        }
    }

    #[test]
    fn r6_invalid_class_normal_child_return_is_rejected_by_real_gates() {
        let executable = std::env::current_exe().unwrap();
        let runner = child_runner()
            .unwrap_or_else(|error| panic!("invalid-class child runner misconfiguration: {error}"));
        let nonce = mint_nonce();
        let dir = reserve_ack_dir(&nonce)
            .unwrap_or_else(|error| panic!("could not reserve launch directory: {error}"));
        let _ack_guard = AckDirGuard(dir.clone());
        let ack = dir.join("ack");
        let expected = expected_ack_content("return:normal", &nonce);
        let mut command = invalid_class_child_command(&executable, runner.as_deref());
        command
            .env(CHILD, "return:normal")
            .env(ACK_ENV, &ack)
            .env(NONCE_ENV, &nonce);
        let outcome =
            run_child_bounded(&mut command, &dir, std::time::Duration::from_secs(10), true)
                .unwrap_or_else(|error| panic!("normal child supervision failed: {error}"));
        let ChildRun::Completed { status, stderr } = outcome else {
            panic!("normal-return child exceeded its budget")
        };
        assert!(
            status.success(),
            "normal-return control must return successfully: {status:?}"
        );
        let stderr_text = String::from_utf8_lossy(&stderr);
        assert_eq!(
            verify_ack(&ack, &expected),
            Ok(()),
            "normal-return control must activate"
        );
        assert!(
            validate_fatal_diagnostic(&stderr_text).is_err(),
            "normal-return control must not emit allocator fatal"
        );
        assert!(acceptance_verdict(
            expected_abort_termination(termination_of(&status)),
            verify_ack(&ack, &expected),
            validate_fatal_diagnostic(&stderr_text),
        )
        .is_err());
    }

    #[test]
    fn r6_invalid_class_bad_launcher_is_rejected_by_real_gates() {
        let executable = std::env::current_exe().unwrap();
        let runner = child_runner()
            .unwrap_or_else(|error| panic!("invalid-class child runner misconfiguration: {error}"));
        let nonce = mint_nonce();
        let dir = reserve_ack_dir(&nonce)
            .unwrap_or_else(|error| panic!("could not reserve launch directory: {error}"));
        let _ack_guard = AckDirGuard(dir.clone());
        let missing_ack = dir.join("missing-ack");
        let mut command = invalid_class_child_command(&executable, runner.as_deref());
        command
            .arg("--sefer-task58-invalid-launcher-argument")
            .env_remove(CHILD)
            .env_remove(ACK_ENV)
            .env_remove(NONCE_ENV);
        let outcome =
            run_child_bounded(&mut command, &dir, std::time::Duration::from_secs(10), true)
                .unwrap_or_else(|error| panic!("bad launcher supervision failed: {error}"));
        let ChildRun::Completed { status, stderr } = outcome else {
            panic!("bad launcher exceeded its budget")
        };
        let stderr_text = String::from_utf8_lossy(&stderr);
        assert!(
            !status.success(),
            "bad launcher must fail: {status:?}; stderr: {stderr_text}"
        );
        assert!(
            expected_abort_termination(termination_of(&status)).is_err(),
            "bad launcher must not have an expected abort signature: {status:?}"
        );
        assert!(verify_ack(&missing_ack, &expected_ack_content("pre:255", &nonce)).is_err());
        assert!(validate_fatal_diagnostic(&stderr_text).is_err());
        assert!(
            stderr_text.contains("sefer-task58-invalid-launcher-argument"),
            "libtest must report the unrecognized launcher argument; stderr: {stderr_text}"
        );
        let verdict = acceptance_verdict(
            expected_abort_termination(termination_of(&status)),
            verify_ack(&missing_ack, &expected_ack_content("pre:255", &nonce)),
            validate_fatal_diagnostic(&stderr_text),
        );
        assert!(
            verdict.is_err(),
            "bad launcher unexpectedly passed acceptance gates; stderr: {stderr_text}"
        );
    }

    #[test]
    fn r6_invalid_class_runner_split_rejects_blank_launcher() {
        assert_eq!(
            split_runner(""),
            Err("configured target runner is blank".to_string())
        );
        assert_eq!(
            split_runner("   "),
            Err("configured target runner is blank".to_string())
        );
    }

    #[test]
    fn r6_invalid_class_runner_split_keeps_runner_arguments() {
        assert_eq!(
            split_runner("/linux-runner aarch64"),
            Ok(vec!["/linux-runner".to_string(), "aarch64".to_string()])
        );
    }

    #[cfg(unix)]
    #[test]
    fn r6_invalid_class_termination_rejects_everything_but_sigabrt() {
        for rejected in [
            Termination::Success,
            Termination::Code(0),
            Termination::Code(2),
            // Exit 3 on unix is a plain exit code, not an abort signature.
            Termination::Code(3),
            // Rust panic unwinds exit 101.
            Termination::Code(101),
            Termination::Signal(9),
        ] {
            assert!(
                expected_abort_termination(rejected).is_err(),
                "accepted {rejected:?}"
            );
        }
        assert_eq!(
            expected_abort_termination(Termination::Signal(SIGABRT)),
            Ok(())
        );
    }

    #[cfg(windows)]
    #[test]
    fn r6_invalid_class_termination_rejects_everything_but_abort_codes() {
        for rejected in [
            Termination::Success,
            Termination::Code(0),
            Termination::Code(1),
            Termination::Code(2),
            // Rust panic unwinds exit 101.
            Termination::Code(101),
            Termination::Code(7),
        ] {
            assert!(
                expected_abort_termination(rejected).is_err(),
                "accepted {rejected:?}"
            );
        }
        assert_eq!(expected_abort_termination(Termination::Code(3)), Ok(()));
        assert_eq!(
            expected_abort_termination(Termination::Code(-1073740791)),
            Ok(())
        );
    }
}
