use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const MAX_STDERR_BYTES: u64 = 64 * 1024;

pub(super) enum ChildRun {
    Completed { status: ExitStatus, stderr: Vec<u8> },
    BudgetExceeded { status: ExitStatus, stderr: Vec<u8> },
}

struct OwnedChild(Option<Child>);

impl OwnedChild {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn probe(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let status = self.0.as_mut().expect("owned child").try_wait()?;
        if status.is_some() {
            self.0 = None;
        }
        Ok(status)
    }

    fn kill_and_reap(&mut self) -> Result<(ExitStatus, bool), String> {
        let child = self.0.as_mut().expect("owned child");
        if let Err(kill_error) = child.kill() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.0 = None;
                    return Ok((status, false));
                }
                Ok(None) => {
                    return Err(format!(
                        "child kill failed and child remains live: {kill_error}"
                    ));
                }
                Err(probe_error) => {
                    return Err(format!(
                        "child kill failed ({kill_error}); status probe failed: {probe_error}"
                    ));
                }
            }
        }
        let status = self
            .0
            .as_mut()
            .expect("owned child")
            .wait()
            .map_err(|error| format!("child reap failed after kill: {error}"))?;
        self.0 = None;
        Ok((status, true))
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let Some(child) = self.0.as_mut() else {
            return;
        };
        if let Err(error) = child.kill() {
            eprintln!("child cleanup kill failed: {error}");
        }
        if let Err(error) = child.wait() {
            eprintln!("child cleanup reap failed: {error}");
        } else {
            self.0 = None;
        }
    }
}

fn read_stderr(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|error| format!("open captured stderr: {error}"))?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_STDERR_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read captured stderr: {error}"))?;
    if bytes.len() as u64 > MAX_STDERR_BYTES {
        return Err(format!(
            "captured stderr exceeds {} bytes",
            MAX_STDERR_BYTES
        ));
    }
    Ok(bytes)
}

// A fixed deadline gate observes the owned direct launcher, not inherited stream
// EOF. Fast children intentionally consume the full budget; std process and file
// calls are not guaranteed to honor an absolute OS deadline.
pub(super) fn run_child_bounded(
    command: &mut Command,
    dir: &Path,
    budget: Duration,
    capture_stderr: bool,
) -> Result<ChildRun, String> {
    let stderr_path = dir.join("stderr");
    if capture_stderr {
        let stderr_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr_path)
            .map_err(|error| format!("create fresh stderr capture: {error}"))?;
        command.stderr(Stdio::from(stderr_file));
    } else {
        command.stderr(Stdio::null());
    }
    command.stdin(Stdio::null()).stdout(Stdio::null());

    let child = command
        .spawn()
        .map_err(|error| format!("spawn child: {error}"))?;
    let mut child = OwnedChild::new(child);

    let (tx, rx) = mpsc::channel::<()>();
    let gate = rx.recv_timeout(budget);
    drop(tx);
    match gate {
        Err(mpsc::RecvTimeoutError::Timeout) => {}
        Ok(()) => return Err("child deadline gate unexpectedly received a message".to_string()),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            return Err("child deadline gate sender disconnected unexpectedly".to_string())
        }
    }

    if let Some(status) = child
        .probe()
        .map_err(|error| format!("child status probe failed: {error}"))?
    {
        let stderr = if capture_stderr {
            read_stderr(&stderr_path)?
        } else {
            Vec::new()
        };
        return Ok(ChildRun::Completed { status, stderr });
    }

    let (status, killed) = child.kill_and_reap()?;
    let stderr = if capture_stderr {
        read_stderr(&stderr_path)?
    } else {
        Vec::new()
    };
    if killed {
        Ok(ChildRun::BudgetExceeded { status, stderr })
    } else {
        Ok(ChildRun::Completed { status, stderr })
    }
}
