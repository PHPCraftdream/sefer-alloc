#[path = "../src/global/process_exit.rs"]
mod process_exit;

#[cfg(windows)]
mod probe {
    use super::process_exit::process_exit_in_progress;
    use std::io::Write;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    struct Witness(&'static str);

    impl Drop for Witness {
        fn drop(&mut self) {
            let observed = process_exit_in_progress();
            let mut stdout = std::io::stdout().lock();
            // A failed write is rejected by the parent's exact-output assertion.
            let _ = writeln!(stdout, "CON01_TLS_DROP mode={} exiting={observed}", self.0);
            let _ = stdout.flush();
        }
    }

    thread_local! {
        static JOINED: Witness = const { Witness("joined") };
        static EXIT: Witness = const { Witness("process_exit") };
        static RETURN: Witness = const { Witness("main_return") };
    }

    pub(super) fn run() {
        if let Some(mode) = std::env::args().nth(1) {
            match mode.as_str() {
                "joined" => {
                    std::thread::spawn(|| JOINED.with(|_| ()))
                        .join()
                        .expect("ordinary thread joined");
                }
                "process_exit" => {
                    EXIT.with(|_| ());
                    std::process::exit(0);
                }
                "main_return" => RETURN.with(|_| ()),
                _ => panic!("unknown probe mode: {mode}"),
            }
            return;
        }

        let exe = std::env::current_exe().expect("probe executable");
        let mut results = Vec::new();
        for (mode, expected) in [
            ("joined", false),
            ("process_exit", true),
            ("main_return", true),
        ] {
            let mut child = Command::new(&exe)
                .arg(mode)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawn detector probe");
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut timed_out = false;
            loop {
                if child.try_wait().expect("probe status").is_some() {
                    break;
                }
                if Instant::now() >= deadline {
                    timed_out = true;
                    child.kill().expect("kill owned probe child");
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            // Each child emits one short line, so neither pipe can fill.
            let output = child.wait_with_output().expect("reap detector probe");
            let stdout = String::from_utf8(output.stdout).expect("UTF-8 probe stdout");
            let stderr = String::from_utf8(output.stderr).expect("UTF-8 probe stderr");
            println!(
                "CON01_CHILD mode={mode} status={} timed_out={timed_out}",
                output.status
            );
            print!("{stdout}");
            eprint!("{stderr}");
            results.push((mode, expected, timed_out, output.status, stdout, stderr));
        }
        for (mode, expected, timed_out, status, stdout, stderr) in results {
            assert!(!timed_out, "{mode}: probe timed out");
            assert_eq!(status.code(), Some(0), "{mode}: child failed");
            assert!(stderr.is_empty(), "{mode}: unexpected stderr: {stderr}");
            assert_eq!(
                stdout.replace("\r\n", "\n"),
                format!("CON01_TLS_DROP mode={mode} exiting={expected}\n"),
                "{mode}: loader-state detector must distinguish thread and process exit"
            );
        }
        println!("CON01_DETECTOR_PROBE_PASS");
    }
}

fn main() {
    #[cfg(windows)]
    probe::run();
    #[cfg(not(windows))]
    {
        assert!(!process_exit::process_exit_in_progress());
        println!("CON01_DETECTOR_PROBE non-Windows constant-false control only");
    }
}
