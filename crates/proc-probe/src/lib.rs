//! `proc-probe` — the `RESULT key=value` stdout protocol every fresh-process
//! judge shares, plus a re-export of [`proc_memstat`]'s same-instant memory
//! [`snapshot`].
//!
//! # The protocol
//!
//! A *probe* is a tiny binary that a *runner* launches N times as fresh OS
//! processes, parsing one machine-readable line per metric out of each run's
//! stdout. The line shape is deliberately trivial so a runner can `grep`/parse
//! it robustly regardless of surrounding log noise:
//!
//! ```text
//! RESULT <key>=<value>
//! ```
//!
//! `<key>` is `[a-z0-9_]+`; `<value>` is any non-whitespace token. This crate
//! is the emitting half — one function per value shape ([`emit`], [`emit_u64`],
//! [`emit_i64`], [`emit_f64`], [`emit_ns`]) so a probe never hand-rolls the
//! `println!("RESULT ...")` string and never drifts the format the runner
//! parses against.
//!
//! # Measure + report in one dependency
//!
//! A probe almost always wants to *measure* memory and then *report* it. This
//! crate re-exports [`proc_memstat::snapshot`] and [`proc_memstat::MemStat`],
//! so a probe binary depends on **one** crate for both halves:
//!
//! ```text
//! let m = proc_probe::snapshot();          // measure (bytes, same instant)
//! proc_probe::emit_u64("rss_kib", m.rss / 1024);        // report
//! proc_probe::emit_u64("commit_kib", m.commit / 1024);
//! ```
//!
//! (Runnable form of the examples lives in `tests/protocol.rs` — this crate
//! ships no doctests.)
//!
//! # Why a separate crate from `proc-memstat`
//!
//! `proc-memstat` is the pure *measurement* library (its whole reason to exist
//! is the OS FFI that reads memory counters). This crate is the *reporting*
//! convention layered on top — a probe's single "measure + emit" dependency —
//! and it holds **no** `unsafe` of its own (`#![forbid(unsafe_code)]`); all the
//! FFI stays confined to `proc-memstat`.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![no_std]

// The emit family writes to stdout, which needs `std`. The protocol *shape*
// (and any future formatting helpers) is `no_std`-clean; only the sink pulls in
// `std`, so downstream `no_std` probes that build their own sink can still use
// this crate's constants/format without the emit functions.
#[cfg(feature = "std")]
extern crate std;

/// Re-export of the same-instant memory snapshot from [`proc_memstat`] — a
/// probe gets "measure + report" from this one crate.
pub use proc_memstat::{snapshot, MemStat};

/// The line prefix every emitted metric carries. A runner keys off exactly this
/// token, so it is exposed as a constant rather than hard-coded per call site.
pub const RESULT_PREFIX: &str = "RESULT";

/// Emit one `RESULT <key>=<value>` line to stdout.
///
/// This is the string-valued primitive; the numeric helpers ([`emit_u64`],
/// [`emit_i64`], [`emit_f64`], [`emit_ns`]) format their argument and delegate
/// here in spirit. `key` should match `[a-z0-9_]+` (the shape runners parse);
/// `value` is any token without whitespace.
#[cfg(feature = "std")]
pub fn emit(key: &str, value: &str) {
    std::println!("{RESULT_PREFIX} {key}={value}");
}

/// Emit an unsigned-integer metric: `RESULT <key>=<value>`.
#[cfg(feature = "std")]
pub fn emit_u64(key: &str, value: u64) {
    std::println!("{RESULT_PREFIX} {key}={value}");
}

/// Emit a signed-integer metric: `RESULT <key>=<value>` (e.g. a delta that may
/// be negative).
#[cfg(feature = "std")]
pub fn emit_i64(key: &str, value: i64) {
    std::println!("{RESULT_PREFIX} {key}={value}");
}

/// Emit a floating-point metric: `RESULT <key>=<value>`.
///
/// Uses the default `f64` `Display`, which never emits whitespace, so the value
/// stays a single parseable token.
#[cfg(feature = "std")]
pub fn emit_f64(key: &str, value: f64) {
    std::println!("{RESULT_PREFIX} {key}={value}");
}

/// Emit a nanosecond duration as a plain integer metric: `RESULT <key>=<value>`.
///
/// A convenience for the common `Instant::elapsed().as_nanos()` (a `u128`)
/// shape — kept as its own function so a probe timing a region does not repeat
/// the cast at every call site.
#[cfg(feature = "std")]
pub fn emit_ns(key: &str, ns: u128) {
    std::println!("{RESULT_PREFIX} {key}={ns}");
}
