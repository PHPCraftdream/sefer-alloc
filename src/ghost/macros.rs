//! `ghost_assert!(rule, cond[, fmt..])` and `ghost_event!(rule, key, delta)`.
//!
//! - loom / miri / `--cfg sefer_ghost`: the assertion panics through
//!   [`super::fail`] naming the rule; the event adds `delta` to the shadow-table
//!   counter of `(rule, key)` for the harness to check at the end of a run.
//! - kani: the assertion is a `kani::assert` proof obligation; events are not
//!   recorded (kani harnesses check local values only).
//! - production: both expand to nothing; arguments are not evaluated.
//!
//! `rule` is a catalog rule id (`docs/verification/ghost-rules.json`); ghost-lint
//! rule R6 finds call sites by it, R9 forbids writes inside the arguments.

// Unused until the first production call site (G2) outside the internals probe.
#![allow(unused_macros)]

#[cfg(all(any(loom, miri, sefer_ghost), not(kani)))]
macro_rules! ghost_assert {
    ($rule:literal, $cond:expr $(,)?) => {
        if !($cond) {
            $crate::ghost::fail($rule, format_args!("{}", stringify!($cond)))
        }
    };
    ($rule:literal, $cond:expr, $($arg:tt)+) => {
        if !($cond) {
            $crate::ghost::fail($rule, format_args!($($arg)+))
        }
    };
}

#[cfg(kani)]
macro_rules! ghost_assert {
    ($rule:literal, $cond:expr $(, $($arg:tt)*)?) => {
        kani::assert($cond, concat!("ghost check ", $rule))
    };
}

#[cfg(not(any(loom, miri, kani, sefer_ghost)))]
macro_rules! ghost_assert {
    ($($t:tt)*) => {};
}

#[cfg(all(any(loom, miri, sefer_ghost), not(kani)))]
macro_rules! ghost_event {
    ($rule:literal, $key:expr, $delta:expr $(,)?) => {
        $crate::ghost::record($rule, ($key) as u64, ($delta) as i64)
    };
}

#[cfg(any(kani, not(any(loom, miri, sefer_ghost))))]
macro_rules! ghost_event {
    ($($t:tt)*) => {};
}

#[allow(unused_imports)]
pub(crate) use {ghost_assert, ghost_event};
