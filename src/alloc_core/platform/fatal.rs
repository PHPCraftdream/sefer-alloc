/// Writes a best-effort static diagnostic and terminates without unwinding.
#[cold]
#[inline(never)]
pub(crate) fn fatal(msg: &'static [u8]) -> ! {
    use std::io::Write;
    let _ = std::io::stderr().write_all(msg);
    std::process::abort()
}
