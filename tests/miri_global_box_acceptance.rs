//! Explicit, one-scenario-per-process installed-Sefer Miri acceptance.
//! Cargo runs this target only when selected by name; no libtest is linked.

#[path = "support/r8_global_box_witness.rs"]
mod witness;

use sefer_alloc::SeferAlloc;
use std::ffi::OsStr;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

fn main() {
    let mut args = std::env::args_os();
    let _program = args.next();
    let selected = args.next();
    if args.next().is_some() {
        std::process::exit(2);
    }
    match selected.as_deref() {
        Some(name) if name == OsStr::new("narrow") => {
            let completed_rounds = witness::narrow_two_rounds(&GLOBAL);
            assert_eq!(completed_rounds, 2);
            println!("[miri_global_box_acceptance] COMPLETE narrow_two_rounds");
        }
        #[cfg(miri)]
        Some(name) if name == OsStr::new("paused") => {
            let completed_pauses = witness::paused_terminal_owner_retirement(&GLOBAL);
            assert_eq!(completed_pauses, 1);
            println!("[miri_global_box_acceptance] COMPLETE paused_terminal_owner_retirement");
        }
        _ => std::process::exit(2),
    }
}
