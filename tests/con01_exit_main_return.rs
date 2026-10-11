//! Real CRT/main-return exit path, using the same bounded parent and TLS oracle.
#[path = "con01_exit_stranded_lock.rs"]
#[allow(dead_code)] // The shared libtest selectors are not this binary's entry.
mod oracle;

fn main() {
    if std::env::var_os("SEFER_CON01_CHILD").is_some() {
        oracle::prepare_main_return();
        // This is the actual return from main: no process::exit wrapper.
        return;
    }
    assert!(
        oracle::run_scenario("main_return", None, false),
        "expected bounded CON-01 main return within 20s with shard unroutable +1"
    );
}
