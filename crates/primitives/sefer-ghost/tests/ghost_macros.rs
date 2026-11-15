//! Crate-local self-test for the ghost macros and the shadow table:
//! a `ghost_assert!` invoked through the crate boundary expands to the
//! crate's own `fail`/`record` (the `$crate` paths), events land in the
//! shadow table, and `reset` clears it. Ghost builds only:
//! `RUSTFLAGS="--cfg sefer_ghost" cargo test -p sefer-ghost`.
#![cfg(any(loom, miri, kani, sefer_ghost))]

use sefer_ghost::ghost::{count, overflowed, reset};
use sefer_ghost::ghost::{ghost_assert, ghost_event};

#[test]
fn macros_record_and_reset_across_the_crate_boundary() {
    reset();
    assert_eq!(count("ghost/probe", 1), 0);
    ghost_event!("ghost/probe", 1, 2);
    ghost_assert!("ghost/probe", true, "never fires");
    assert_eq!(
        count("ghost/probe", 1),
        2,
        "ghost_event! recorded via $crate"
    );
    ghost_assert!("ghost/probe", 1 + 1 == 2);
    let r = std::panic::catch_unwind(|| ghost_assert!("ghost/assert", false));
    assert!(r.is_err(), "a false ghost_assert! must panic");
    assert_eq!(
        count("ghost/probe", 1),
        2,
        "a passing assert records nothing"
    );
    assert_eq!(overflowed(), 0);
    reset();
    assert_eq!(count("ghost/probe", 1), 0, "reset clears the table");
}
