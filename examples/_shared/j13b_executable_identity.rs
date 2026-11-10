//! Actual running-image digest under a trusted Linux host and GNU hash utility.
#![forbid(unsafe_code)]
use std::io;

/// Hash a handle opened by this process, not /proc/self/exe resolved inside the
/// hash child and not a mutable executable pathname. This is not host attestation.
pub(crate) fn executable_sha256() -> io::Result<String> {
    #[cfg(all(target_os = "linux", not(miri)))]
    {
        use std::fs::File;
        use std::os::unix::fs::MetadataExt;
        use std::process::{Command, Stdio};
        let file = File::open("/proc/self/exe")?;
        let before = file.metadata()?;
        if !before.is_file() || before.len() == 0 || before.len() > 256 * 1024 * 1024 {
            return Err(io::Error::other("running executable size/type unsupported"));
        }
        let output = Command::new("/usr/bin/sha256sum")
            .env("LC_ALL", "C")
            .stdin(Stdio::from(file.try_clone()?))
            .output()?;
        let after = file.metadata()?;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.len() != after.len()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
        {
            return Err(io::Error::other("running executable changed while hashing"));
        }
        let bytes = output.stdout;
        if !output.status.success()
            || bytes.len() != 68
            || &bytes[64..] != b"  -\n"
            || !bytes[..64]
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            return Err(io::Error::other(
                "running executable digest unavailable or malformed",
            ));
        }
        String::from_utf8(bytes[..64].to_vec()).map_err(io::Error::other)
    }
    #[cfg(any(not(target_os = "linux"), miri))]
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "running executable identity requires native Linux",
    ))
}
