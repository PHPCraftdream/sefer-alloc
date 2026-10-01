/// Unrecoverable descriptor-protocol violation. Never called under a shard
/// lock; writes a static slice only (no allocation) and aborts.
pub(super) fn fatal(msg: &'static [u8]) -> ! {
    use std::io::Write;
    let _ = std::io::stderr().write_all(msg);
    std::process::abort()
}
