//! Bounded native host observations; top-level THP state is not complete host-policy attestation.
#![forbid(unsafe_code)]
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostFacts {
    pub arch: String,
    pub kernel: String,
    pub cpu: String,
    pub boot_id: String,
    pub thp_top_level_mode: String,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn clean(value: &str, maximum: usize) -> io::Result<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(invalid("host fact missing or malformed"));
    }
    Ok(value.to_owned())
}

impl HostFacts {
    /// Pure fixture parser; observed and compiled architecture inputs must come
    /// from the native collector before this can be treated as observed facts.
    pub(crate) fn parse(
        cpu: &str,
        kernel: &str,
        boot: &str,
        thp: &str,
        arch: &str,
        compiled_arch: &str,
    ) -> io::Result<Self> {
        if cpu.len() > 16384 || kernel.len() > 4096 || boot.len() > 64 || thp.len() > 256 {
            return Err(invalid("host fact input exceeds bound"));
        }
        let arch = clean(arch, 64)?;
        if !matches!(arch.as_str(), "x86_64" | "aarch64") || arch != compiled_arch {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native host/compiled architecture mismatch",
            ));
        }
        let cpu = cpu
            .split_inclusive('\n')
            .filter(|line| line.ends_with('\n'))
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(key, _)| matches!(key.trim(), "model name" | "Hardware"))
                    .map(|(_, value)| value)
            })
            .ok_or_else(|| invalid("CPU model missing from bounded prefix"))?;
        let cpu = clean(cpu, 512)?;
        let kernel = clean(kernel, 512)?;
        let boot_id = clean(boot, 36)?;
        if boot_id.len() != 36
            || !boot_id.bytes().enumerate().all(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    byte == b'-'
                } else {
                    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
                }
            })
        {
            return Err(invalid("boot identity malformed"));
        }
        let tokens: Vec<_> = thp.split_whitespace().collect();
        let mut active = None;
        let mut modes = std::collections::BTreeSet::new();
        for token in &tokens {
            let mode = if let Some(mode) = token
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
            {
                if active.replace(mode).is_some() {
                    return Err(invalid("multiple active THP modes"));
                }
                mode
            } else {
                token
            };
            if !matches!(mode, "always" | "madvise" | "never") || !modes.insert(mode) {
                return Err(invalid("THP enabled record malformed"));
            }
        }
        if tokens.len() != 3 || modes.len() != 3 {
            return Err(invalid("THP enabled record incomplete"));
        }
        let thp_top_level_mode = active
            .ok_or_else(|| invalid("active THP mode missing"))?
            .to_owned();
        Ok(Self {
            arch,
            kernel,
            cpu,
            boot_id,
            thp_top_level_mode,
        })
    }

    pub(crate) fn observe() -> io::Result<Self> {
        #[cfg(all(target_os = "linux", not(miri)))]
        {
            use std::fs::File;
            use std::io::Read;
            fn read(path: &str, maximum: u64, prefix: bool) -> io::Result<String> {
                let mut bytes = Vec::new();
                File::open(path)?
                    .take(maximum + 1)
                    .read_to_end(&mut bytes)?;
                let maximum = usize::try_from(maximum).map_err(io::Error::other)?;
                if bytes.len() > maximum {
                    if !prefix {
                        return Err(invalid("host fact exceeds bound"));
                    }
                    // Truncate bytes at a complete line, never at a UTF-8 offset.
                    let end = bytes[..maximum]
                        .iter()
                        .rposition(|byte| *byte == b'\n')
                        .ok_or_else(|| invalid("CPU prefix lacks a complete line"))?;
                    bytes.truncate(end + 1);
                }
                String::from_utf8(bytes).map_err(io::Error::other)
            }
            let machine = std::process::Command::new("/usr/bin/uname")
                .arg("-m")
                .env("LC_ALL", "C")
                .output()?;
            if !machine.status.success() || machine.stdout.len() > 64 {
                return Err(invalid("host architecture unavailable"));
            }
            let machine = String::from_utf8(machine.stdout).map_err(io::Error::other)?;
            Self::parse(
                &read("/proc/cpuinfo", 16384, true)?,
                &read("/proc/sys/kernel/osrelease", 4096, false)?,
                &read("/proc/sys/kernel/random/boot_id", 64, false)?,
                &read("/sys/kernel/mm/transparent_hugepage/enabled", 256, false)?,
                machine.trim(),
                std::env::consts::ARCH,
            )
        }
        #[cfg(any(not(target_os = "linux"), miri))]
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "host identity requires native Linux",
        ))
    }

    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "host_id": serde_json::json!([self.arch, self.kernel, self.cpu, self.boot_id]).to_string(),
            "arch": self.arch, "kernel": self.kernel, "cpu": self.cpu, "boot_id": self.boot_id,
            "thp_top_level_mode": self.thp_top_level_mode,
            "thp_policy_scope": "top-level enabled only; per-size/defrag policy not authenticated",
        })
    }
}
