//! `proc-probe` — the `RESULT key=value` stdout protocol every fresh-process
//! judge shares, plus (under the default `std` feature) a re-export of
//! `proc-memstat`'s single-read memory `snapshot`.
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
//! `<key>` is `[a-z0-9_]+` (ASCII only). `<value>` is one or more characters
//! the reference runner's ECMAScript parser treats as non-whitespace — i.e.
//! outside ECMAScript `\s` (its WhiteSpace + LineTerminator sets plus the
//! Space_Separator category), *not* outside Rust's `char::is_whitespace`;
//! the two whitespace models disagree at the boundary (U+FEFF is `\s`-whitespace
//! but not Rust-whitespace, U+0085 the reverse), so values involving those code
//! points parse differently between a Rust-side model and the real runner.
//! Probes should keep keys and values ASCII, where the two models agree
//! exactly; `tests/protocol.rs`'s `parser_contract_matches_node_ecmascript_regex`
//! verifies this model against the real ECMAScript regex (via node) and pins
//! those divergences explicitly. This crate
//! is the emitting half — one function per value shape (`emit`, `emit_u64`,
//! `emit_i64`, `emit_f64`, `emit_ns`) so a probe never hand-rolls the
//! `println!("RESULT ...")` string and the library side cannot drift the
//! format the runner parses against. The shape is a CALLER precondition,
//! though: the family is unchecked — a key/value violating it (a newline
//! inside a value, an empty key) is a caller logic error that nothing here
//! validates, sanitizes, or truncates (see `emit`'s docs). The `emit*` family
//! and the `proc-memstat` re-export exist
//! only under the default `std` feature; a `no_std` consumer built with
//! `default-features = false` gets `RESULT_PREFIX` alone and supplies its own
//! sink (that build has zero dependencies).
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

// The emit family writes to stdout, which needs `std`, and the `proc-memstat`
// re-export is itself an std-only crate — so both are gated on the `std`
// feature (which is what pulls in the optional dependency). The protocol
// *shape* (and any future formatting helpers) is `no_std`-clean: with
// `default-features = false` this crate builds with zero dependencies, and a
// downstream `no_std` probe that builds its own sink can still use the
// `RESULT_PREFIX` constant/format.
#[cfg(feature = "std")]
extern crate std;

/// Re-export of the single-read memory snapshot from [`proc_memstat`] — a
/// probe gets "measure + report" from this one crate.
///
/// A probe almost always wants to *measure* memory and then *report* it. This
/// crate re-exports [`proc_memstat::snapshot`] and [`proc_memstat::MemStat`],
/// so a probe binary depends on **one** crate for both halves:
///
/// ```text
/// let m = proc_probe::snapshot();          // measure (bytes, one read)
/// proc_probe::emit_u64("rss_kib", m.rss / 1024);        // report
/// // Name the metric you actually read. `commit_charge` and `virtual_size`
/// // are different quantities and only one of them exists on a given OS —
/// // see `proc_memstat::MemStat`. Emitting one under the other's name is
/// // how a retained address space gets reported as retained commit.
/// if let Some(kib) = m.commit_charge.map(|b| b / 1024) {
///     proc_probe::emit_u64("commit_charge_kib", kib);
/// }
/// if let Some(kib) = m.virtual_size.map(|b| b / 1024) {
///     proc_probe::emit_u64("virtual_size_kib", kib);
/// }
/// ```
///
/// This example is prose, not a doctest (this crate ships none). The closest
/// exercised surfaces in `tests/protocol.rs` are
/// `real_stdout_emit_family_exact_bytes` — the `emit*` family's REAL stdout
/// bytes, captured from a freshly re-exec'd child against independently
/// hardcoded expectations — and `snapshot_re_export_matches_proc_memstat`,
/// which proves the re-export is the same `snapshot()`. No test currently
/// replays this exact measure-then-report flow end-to-end: the two optional
/// `commit_charge`/`virtual_size` branches above are documented here, not
/// executed there.
#[cfg(feature = "std")]
pub use proc_memstat::{snapshot, MemStat};

/// Re-export of the fallible measurement form — [`proc_memstat::try_snapshot`]
/// and its error type [`proc_memstat::SnapshotError`].
///
/// [`proc_memstat::snapshot`] is best-effort by documented contract: on any
/// read failure it returns an all-zero `MemStat` (`rss: 0`, `None` for the
/// optional fields). A memory-comparison judge reading `RESULT rss_kib=0`
/// cannot distinguish "the process freed everything" from "the reading
/// failed" — for exactly that paired-A/B case, take the `Result` and emit a
/// hard failure signal instead of a well-formed zero:
///
/// ```text
/// match proc_probe::try_snapshot() {
///     Ok(m) => proc_probe::emit_u64("rss_kib", m.rss / 1024),
///     Err(e) => {
///         eprintln!("proc-probe: memory measurement failed: {e}");
///         std::process::exit(3); // this probe's failure signal to its runner
///     }
/// }
/// ```
#[cfg(feature = "std")]
pub use proc_memstat::{try_snapshot, SnapshotError};

/// The line prefix every emitted metric carries. A runner keys off exactly this
/// token, so it is exposed as a constant rather than hard-coded per call site.
pub const RESULT_PREFIX: &str = "RESULT";

/// Emit one `RESULT <key>=<value>` line to stdout.
///
/// This is the string-valued primitive; the numeric helpers ([`emit_u64`],
/// [`emit_i64`], [`emit_f64`], [`emit_ns`]) format their argument and delegate
/// here in spirit. `key` should match `[a-z0-9_]+` (the shape runners parse);
/// `value` is one or more characters outside the runner's ECMAScript `\s`
/// definition of whitespace (see the protocol notes above); prefer ASCII so the
/// Rust-side and ECMAScript whitespace models agree exactly.
///
/// # Unchecked contract
///
/// Neither `key` nor `value` is validated, sanitized, or truncated — the
/// shape above is a precondition, and violating it is a logic error in the
/// caller, not a failure this function reports. Concretely: a `value` with an
/// embedded newline prints more than one stdout line, and the extra lines are
/// indistinguishable from real metrics to a line-based runner; an empty `key`
/// or empty `value` prints a line no runner accepts.
///
/// # Panics
///
/// Panics if writing to stdout fails — the documented behavior of
/// `std::println!`, which this function wraps.
#[cfg(feature = "std")]
pub fn emit(key: &str, value: &str) {
    std::println!("{RESULT_PREFIX} {key}={value}");
}

/// Emit an unsigned-integer metric: `RESULT <key>=<value>`.
///
/// The value half is well-formed by construction (`u64`'s `Display` is digits
/// only); `key` is unchecked exactly like [`emit`]'s — the `[a-z0-9_]+` shape
/// is a precondition, and violating it is a caller logic error, not validated
/// here.
///
/// # Panics
///
/// Panics if writing to stdout fails — the documented behavior of
/// `std::println!`, which this function wraps.
#[cfg(feature = "std")]
pub fn emit_u64(key: &str, value: u64) {
    std::println!("{RESULT_PREFIX} {key}={value}");
}

/// Emit a signed-integer metric: `RESULT <key>=<value>` (e.g. a delta that may
/// be negative).
///
/// The value half is well-formed by construction (`i64`'s `Display` is a sign
/// plus digits only); `key` is unchecked exactly like [`emit`]'s — the
/// `[a-z0-9_]+` shape is a precondition, and violating it is a caller logic
/// error, not validated here.
///
/// # Panics
///
/// Panics if writing to stdout fails — the documented behavior of
/// `std::println!`, which this function wraps.
#[cfg(feature = "std")]
pub fn emit_i64(key: &str, value: i64) {
    std::println!("{RESULT_PREFIX} {key}={value}");
}

/// Emit a floating-point metric: `RESULT <key>=<value>`.
///
/// The value half goes through the default `f64` `Display`, which never emits
/// whitespace, so it stays a single parseable token; it is not separately
/// validated (nothing to validate — that is `Display`'s by-construction
/// shape). `key` is unchecked exactly like [`emit`]'s — the `[a-z0-9_]+`
/// shape is a precondition, and violating it is a caller logic error, not
/// validated here.
///
/// # Panics
///
/// Panics if writing to stdout fails — the documented behavior of
/// `std::println!`, which this function wraps.
#[cfg(feature = "std")]
pub fn emit_f64(key: &str, value: f64) {
    std::println!("{RESULT_PREFIX} {key}={value}");
}

/// Emit a nanosecond duration as a plain integer metric: `RESULT <key>=<value>`.
///
/// A convenience for the common `Instant::elapsed().as_nanos()` (a `u128`)
/// shape: the value is taken and printed at full `u128` width — no narrowing
/// step that could silently wrap (a manual `as u64` at the call site would
/// wrap for durations beyond roughly 584 years in nanoseconds) and no
/// hand-rolled `RESULT` formatting at the call site. `key` is unchecked
/// exactly like [`emit`]'s — the `[a-z0-9_]+` shape is a precondition, and
/// violating it is a caller logic error, not validated here.
///
/// # Panics
///
/// Panics if writing to stdout fails — the documented behavior of
/// `std::println!`, which this function wraps.
#[cfg(feature = "std")]
pub fn emit_ns(key: &str, ns: u128) {
    std::println!("{RESULT_PREFIX} {key}={ns}");
}
