//! `numa-shim` — dependency-free NUMA detection and placement.
//!
//! **Key selling point:** zero C library dependencies.
//! - Linux: `mbind(2)` via raw `syscall(2)` (no libnuma, no hwloc).
//! - Linux node detection: reads `/sys/devices/system/node/nodeN/cpumap` directly
//!   via `open`/`read`/`close` from the C runtime (always present in glibc/musl).
//! - Windows: `VirtualAllocExNuma` for NUMA-preferred reservations;
//!   `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx` for detection.
//! - macOS / miri: detection reports "unavailable"; the reservation API
//!   returns `Err(UnsupportedPlatform)` (no silent no-ops — task #1306).
//!
//! This is rare in the Rust ecosystem — typical NUMA crates bind to `libnuma` or
//! `hwloc`, pulling in heavy C dependencies. `numa-shim` has **zero non-system
//! dependencies** in its default configuration.
//!
//! ## Usage
//!
//! ```text
//! use numa_shim::{current_node, NO_NODE};
//!
//! match current_node() {
//!     Some(node) => println!("Running on NUMA node {node}"),
//!     None       => println!("NUMA unavailable or single-node host"),
//! }
//! ```
//!
//! Runnable form: `tests/smoke.rs`.
//!
//! ## Safety
//!
//! The public API is safe to call from `#![forbid(unsafe_code)]` consumers —
//! the crate has NO `pub unsafe fn`. `unsafe` is confined to the per-OS
//! `mod platform` blocks plus a small set of crate-root Linux mbind FFI
//! helpers (`mbind_preferred_linux`, `libc_mbind`, and the
//! `extern "C" { fn syscall(...) }` declaration), each with `// SAFETY:` proof
//! comments. task #1277 (review N7): the old claim that unsafe was "confined to
//! platform modules" was false — those crate-root helpers sit outside every
//! `mod platform`. The `bind_range` byte-range API (previously the single
//! `pub unsafe fn`) was removed in task #1306 as it was confirmed broken
//! (unaligned `addr` → silent EINVAL; mbind default flags affect only FUTURE
//! faults, not already-touched pages).
//!
//! ## Feature flags
//!
//! | Flag | Effect |
//! |------|--------|
//! | `vmem-integration` | Enables `reserve_preferred_on_node`, which uses the `aligned-vmem` crate for the reservation step. Windows path uses `VirtualAllocExNuma`; Linux reserves then calls `mbind`. |
//!
//! ## Platform matrix
//!
//! | Platform | [`current_node`] | [`reserve_preferred_on_node`] (feature) |
//! |----------|-----------------|------------------------------------------|
//! | Linux x86_64/aarch64 (non-miri) | sched_getcpu + sysfs cpumap | mmap then mbind (complete span, before first touch) |
//! | Linux other arch (non-miri) | sched_getcpu + sysfs cpumap | `UnsupportedArchitecture` error |
//! | Windows (non-miri) | `GetCurrentProcessorNumberEx` | `VirtualAllocExNuma` |
//! | macOS | `None` | `UnsupportedPlatform` error |
//! | miri | `None` | `UnsupportedPlatform` error |
//! | other | `None` | `UnsupportedPlatform` error |

// This crate intentionally contains unsafe OS FFI code.
// The public API is safe — all unsafe lives in the per-OS `mod platform`
// blocks plus a small set of crate-root Linux mbind FFI helpers
// (`mbind_preferred_linux`, `libc_mbind`, and the
// `extern "C" { fn syscall(...) }` declaration they call through), each
// documented with // SAFETY: proof comments. task #1277 (review N7): the
// old "confined to platform modules" claim was false — those crate-root
// helpers sit outside every `mod platform`. The `bind_range` byte-range
// API (previously the single `pub unsafe fn`) was removed in task #1306
// as it was confirmed broken (unaligned `addr` → silent EINVAL; mbind
// default flags affect only FUTURE faults, not already-touched pages).
#![allow(unsafe_code)]
#![deny(missing_docs)]

/// Sentinel value meaning "no NUMA node / feature disabled / unsupported
/// platform". This constant is useful when interfacing with APIs that return
/// a raw `u32` node index and need a "not available" sentinel.
///
/// [`current_node`] returns `None` instead of this sentinel; `NO_NODE` is
/// provided for interop with code that uses the sentinel pattern.
///
/// As of task #1306, `NO_NODE` is no longer accepted by the reservation API
/// (`reserve_preferred_on_node`). It is now used only for detection-side interop;
/// `current_node()` returns `Option<u32>` for the no-preference case.
pub const NO_NODE: u32 = u32::MAX;

/// A NUMA node identifier for the reservation/policy API.
///
/// The [`NO_NODE`] sentinel is unrepresentable: [`NodeId::new`] rejects it
/// (task #1309).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// Construct a `NodeId` from a raw `u32`, rejecting ONLY the
    /// [`NO_NODE`] sentinel (`u32::MAX`).
    ///
    /// Returns `None` for exactly one input: [`NO_NODE`], the one value that
    /// is invalid on EVERY platform — the "no node" state this type exists
    /// to keep out of the reservation API (task #1309, finding F4 of the
    /// fifteenth independent review). Every other `u32` constructs,
    /// including ids a particular platform cannot address: node EXISTENCE
    /// is platform- and runtime-dependent (Linux's single-`u64` nodemask
    /// addresses nodes 0..=63 only; Windows forwards any id to the OS), so
    /// that validation stays where it already lives — the fallible
    /// [`reserve_preferred_on_node`] checks
    /// ([`ReserveNumaError::InvalidNode`]/[`ReserveNumaError::Os`]), not
    /// construction time. Do not read more validation into this constructor
    /// than the single sentinel comparison it performs.
    ///
    /// # Ergonomic path from detection
    ///
    /// [`current_node`] remaps the sentinel to `None` (its `Some(n)` arm can
    /// never carry `NO_NODE`), so the composition cannot actually fail:
    ///
    /// ```text
    /// match numa_shim::current_node() {
    ///     // current_node() never yields the NO_NODE sentinel in its Some
    ///     // arm, so NodeId::new(n) here is always Some(_).
    ///     Some(n) => numa_shim::reserve_preferred_on_node(
    ///         size,
    ///         align,
    ///         NodeId::new(n).expect("never the NO_NODE sentinel"),
    ///     ),
    ///     None => aligned_vmem::reserve_aligned(size, align),
    /// }
    /// ```
    ///
    /// Composed directly, the two `Option`s flatten:
    /// `current_node().and_then(NodeId::new)` is an `Option<NodeId>`.
    ///
    /// No `new_unchecked`/`unsafe` constructor exists — no path needs to
    /// bypass the one comparison.
    // `Option` is already `#[must_use]`; a bare `#[must_use]` here would
    // trip clippy::double_must_use under this repo's -D warnings gate.
    pub const fn new(id: u32) -> Option<Self> {
        if id == NO_NODE {
            None
        } else {
            Some(Self(id))
        }
    }

    /// Return the raw node id wrapped by this `NodeId`.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// The failure cause of a NUMA-preferred reservation attempt.
#[non_exhaustive]
#[derive(Debug)]
pub enum ReserveNumaError {
    /// The platform provides no NUMA API (macOS, miri, other unsupported OS).
    UnsupportedPlatform,
    /// Linux architecture without a known `SYS_MBIND` syscall number.
    UnsupportedArchitecture,
    /// `size`/`align` violated the reservation contract (zero size, align not
    /// a power of two >= page size, size not a page multiple, or size+align
    /// overflow). Carried as one variant because the underlying validator
    /// cannot distinguish which parameter was at fault.
    InvalidArguments,
    /// The node id cannot be addressed by this platform's nodemask — the
    /// documented Linux implementation limit: a single `u64` nodemask
    /// addresses nodes 0..=63 only.
    InvalidNode,
    /// The OS refused an operation; the io::Error was captured immediately
    /// at the failing syscall, before any cleanup FFI could overwrite errno.
    Os(std::io::Error),
}

impl core::fmt::Display for ReserveNumaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                f.write_str("NUMA-preferred reservation is unsupported on this platform")
            }
            Self::UnsupportedArchitecture => {
                f.write_str("Linux architecture without a known SYS_MBIND syscall number")
            }
            Self::InvalidArguments => {
                f.write_str("invalid arguments (reservation contract violation)")
            }
            Self::InvalidNode => {
                f.write_str("NUMA node id cannot be addressed by this platform's nodemask")
            }
            Self::Os(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ReserveNumaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Os(e) => Some(e),
            _ => None,
        }
    }
}

/// Re-exported so callers of [`reserve_preferred_on_node`] can name the return
/// type as `numa_shim::Reservation` without adding a direct `aligned-vmem`
/// dependency of their own. This re-export makes the intentional semver
/// coupling between the two sibling crates visible in `numa-shim`'s own
/// public API (item 46, `docs/CORRECTNESS_OPEN_ITEMS.md`) rather than
/// leaving it implicit — see [`reserve_preferred_on_node`]'s own doc section
/// on the coupling for the full rationale.
#[cfg(feature = "vmem-integration")]
pub use aligned_vmem::Reservation;

/// Test-only mock state replacing platform NUMA syscalls.  Records every
/// invocation into a thread-local buffer so unit tests can assert the
/// wrapping logic is correct on any target (including macOS and miri,
/// where real NUMA syscalls are absent).
///
/// Enabled by the build-time cfg flag `numa_shim_mock` (`RUSTFLAGS="--cfg numa_shim_mock"`).
/// When enabled, the public NUMA functions dispatch into this module instead of
/// the platform implementations.
///
/// The recording log is capped at [`mock::CALLS_CAP`] entries (task
/// #726/#778) — see [`mock::drain`]'s own doc for what that means for a
/// caller driving more calls than the cap without an intervening drain.
///
/// # Why a `--cfg` flag, not a Cargo feature (task #1288, item 42)
///
/// This module used to be gated on a `mock` Cargo feature. That was a hazard
/// because Cargo unifies features across a build's WHOLE dependency graph, so
/// any one target enabling `numa-shim/mock` silently replaced the real syscalls
/// for every consumer sharing that graph (this repo's own root crate demonstrated
/// it: `numa-aware-mock` forwarded to `numa-shim/mock`). Converted 2026-08-23
/// (task #1288) mirroring aligned-vmem's task #962. The cfg still applies
/// build-graph-wide once set — what changed is WHO can set it: only the top-level
/// build invoker via an explicit RUSTFLAGS/build-script choice, never a
/// transitive dependency via Cargo's additive feature-unification, and never
/// `--all-features`/docs.rs/`cargo add` by accident. The flag is declared in this
/// crate's `[lints.rust unexpected_cfgs]` in Cargo.toml so it produces no
/// unexpected-cfg warnings. Removing the feature is semver-breaking against
/// published 0.1.0 (see CHANGELOG "Removed").
#[cfg(numa_shim_mock)]
pub mod mock {
    use crate::NodeResolution;
    use core::cell::RefCell;

    /// Maximum number of calls `CALLS` retains before `record()` stops
    /// pushing.
    ///
    /// task #726 (rust-intel audit §B14): under the documented
    /// sefer-alloc-as-global `numa-aware-mock` scenario (this module's own
    /// R11-5 note on `record`), every allocation calls `current_node()` →
    /// `record()` → `Vec::push` with nothing ever draining the log in that
    /// scenario — an unbounded insert-only Vec growing linearly with
    /// allocation count per thread. Once `CALLS` holds this many entries,
    /// `record()` stops pushing (oldest entries are kept, matching a
    /// call-log's usual "what happened first" debugging value) rather than
    /// growing forever; direct mock tests that `drain()` promptly never
    /// approach this cap.
    ///
    /// task #778 (round-closing review, F7): made `pub` so a downstream
    /// test driving a large number of mocked calls can assert against this
    /// exact value instead of hardcoding a mirror of it (as
    /// `tests/mock_dispatch.rs`'s own `calls_log_is_capped_not_unbounded`
    /// now does).
    pub const CALLS_CAP: usize = 4096;

    /// One recorded invocation of a public NUMA function.
    #[non_exhaustive]
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum MockCall {
        /// `current_node()` was called; the inner value is the RAW
        /// pre-remap slot value, which is not necessarily what the function
        /// returned: when the slot holds `NO_NODE`, the record carries that
        /// raw sentinel even though `current_node()` remaps it to `None` for
        /// its caller (record-then-remap order, matching the real dispatch;
        /// deliberately asserted by `tests/mock_dispatch.rs`'s
        /// `current_node_scripted_no_node_yields_none`).
        ///
        /// task #778 (round-closing review, F13): unlike [`ReservePreferredOnNode`]
        /// below, this tuple variant deliberately does NOT carry
        /// `#[non_exhaustive]` -- `current_node()`'s signature is
        /// `fn() -> Option<u32>`, a single scalar return with no plausible
        /// second field to grow into (unlike `reserve_preferred_on_node`,
        /// which takes multiple arguments a future API revision could
        /// add to). Marking it would force `tests/mock_dispatch.rs`'s two
        /// `assert_eq!(calls, vec![MockCall::CurrentNode(n)])` equality-
        /// oracle sites into weaker `matches!` form for no real growth path
        /// this shape needs to reserve. This variant's single-field layout
        /// is considered frozen.
        ///
        /// [`ReservePreferredOnNode`]: MockCall::ReservePreferredOnNode
        CurrentNode(u32),
        /// `current_node_resolution()` was called; the inner value is what
        /// was returned.
        ///
        /// task #1277 (review N6): `current_node_resolution()` previously
        /// did not record at all, contradicting this module's "records
        /// every invocation" contract; this variant closes that gap. Like
        /// [`CurrentNode`], this single-field tuple variant deliberately
        /// carries no field-level `#[non_exhaustive]` (same reasoning as
        /// task #778/F13's note on `CurrentNode`): one value with no
        /// plausible second field to grow into, keeping equality-oracle
        /// `assert_eq!` sites possible.
        ///
        /// [`CurrentNode`]: MockCall::CurrentNode
        CurrentNodeResolution(NodeResolution),
        /// `reserve_preferred_on_node(size, align, node)` was called; `node`
        /// is the raw id from the `NodeId` (recorded BEFORE validation — unlike
        /// the old `BindRange` which recorded only past its short-circuit —
        /// so error paths like `InvalidNode` are observable in the log too;
        /// task #1306).
        #[non_exhaustive]
        ReservePreferredOnNode {
            /// Requested reservation size in bytes.
            size: usize,
            /// Required alignment in bytes.
            align: usize,
            /// Raw NUMA node id wrapped by the `NodeId`.
            node: u32,
        },
        /// The mock's simulated policy-installation stage ran for a reservation
        /// that had ALREADY succeeded (mirrors the real Linux backend's post-
        /// reservation `mbind(2)`).
        ///
        /// `reservation_len` is `Reservation::reservation_len()` at policy time —
        /// the complete OS span the real backend mbinds (not the aligned usable
        /// subrange). `succeeded == false` only when a scripted failure fired
        /// via `set_policy_failure`.
        ///
        /// task #1311 (F6).
        #[non_exhaustive]
        InstallPolicy {
            /// Raw NUMA node id the policy was applied to.
            node: u32,
            /// Complete OS reservation length at policy time.
            reservation_len: usize,
            /// Whether the simulated policy installation succeeded.
            succeeded: bool,
        },
        /// The mock RELEASED a just-made reservation because the policy stage
        /// failed.
        ///
        /// This record is pushed strictly AFTER the `Drop` of the reservation ran,
        /// so its presence is the observable proof that the two-stage cleanup
        /// contract executed. Exactly one such record per failed call is the
        /// "released exactly once" postcondition.
        ///
        /// task #1311 (F6).
        PolicyFailureRelease {
            /// Raw NUMA node id of the released reservation.
            node: u32,
        },
    }

    std::thread_local! {
        /// Calls recorded since the last `drain()`.
        ///
        /// task #726 (rust-intel audit §A3): was `pub`, committing this
        /// thread-local's internal representation (`RefCell<Vec<MockCall>>`)
        /// to the crate's semver surface even though the intended API is the
        /// encapsulating pair [`drain`]/[`set_current_node`] — no code
        /// anywhere in this workspace (including this crate's own tests)
        /// touched `CALLS`/`CURRENT_NODE_SLOT` directly. Narrowed to
        /// `pub(crate)`; external consumers keep `drain()`/`set_current_node()`
        /// as the only surface.
        pub(crate) static CALLS: RefCell<Vec<MockCall>> = const { RefCell::new(Vec::new()) };
        /// Value returned by `current_node()` under the mock.  Default 0.
        pub(crate) static CURRENT_NODE_SLOT: RefCell<u32> = const { RefCell::new(0) };
        /// Scripted policy-installation failure for a specific node id.
        ///
        /// Holds `Some((node, err))` when a test has armed a failure for
        /// calls with that exact node. Consumed by the first matching call
        /// (`take_policy_failure_for`).
        ///
        /// Internal state encapsulated by `set_policy_failure`/`clear_policy_failure`/
        /// `take_policy_failure_for` — mirrors the convention documented for
        /// `CALLS`/`CURRENT_NODE_SLOT` above (task #726): `pub(crate)` internals
        /// behind encapsulating functions, not part of the crate's semver surface.
        pub(crate) static POLICY_FAILURE_SLOT: RefCell<Option<(u32, std::io::Error)>> = const { RefCell::new(None) };
    }

    /// Drain every recorded call since the last drain (or test start).
    ///
    /// task #778 (round-closing review, F7): truthful only up to
    /// [`CALLS_CAP`] entries — past that, `record()` has already stopped
    /// pushing (see `CALLS_CAP`'s own doc), so a caller that drives more
    /// than `CALLS_CAP` calls without an intervening `drain()` gets a
    /// silently truncated (oldest-first) prefix here, not the full set.
    pub fn drain() -> Vec<MockCall> {
        CALLS.with(|c| c.borrow_mut().drain(..).collect())
    }

    /// Set the value returned by subsequent `current_node()` calls, until
    /// changed by a later `set_current_node` call.
    pub fn set_current_node(node: u32) {
        CURRENT_NODE_SLOT.with(|c| *c.borrow_mut() = node);
    }

    /// Internal: read the scripted current_node value.
    pub(crate) fn current_node_slot() -> u32 {
        CURRENT_NODE_SLOT.with(|c| *c.borrow())
    }

    /// Script a simulated policy-installation failure for calls with the exact
    /// node id `node`.
    ///
    /// The scripted error surfaces as `ReserveNumaError::Os(err)` — the same
    /// variant the real Linux backend returns when `mbind(2)` fails after a
    /// successful reservation.
    ///
    /// # One-shot semantics
    ///
    /// The failure is consumed by the FIRST matching `reserve_preferred_on_node`
    /// call with this exact node id. Subsequent calls with the same node succeed
    /// (unless re-armed).
    ///
    /// # Node-scoped semantics
    ///
    /// A call with a different node id is unaffected. Test hygiene requires
    /// calling `clear_policy_failure()` after each test.
    ///
    /// task #1311 (F6).
    pub fn set_policy_failure(node: u32, err: std::io::Error) {
        POLICY_FAILURE_SLOT.with(|c| *c.borrow_mut() = Some((node, err)));
    }

    /// Reset the scripted policy-installation failure.
    ///
    /// Test hygiene: call this at the start or end of each test that uses
    /// `set_policy_failure` to avoid leaking state across tests.
    ///
    /// task #1311 (F6).
    pub fn clear_policy_failure() {
        POLICY_FAILURE_SLOT.with(|c| *c.borrow_mut() = None);
    }

    /// Internal: consume the scripted policy-installation failure for `node`.
    ///
    /// Returns `Some(err)` if a failure is armed for this exact node, consuming
    /// it. Returns `None` if no failure is armed or the armed failure is for a
    /// different node.
    ///
    /// # Reentrancy safety
    ///
    /// Uses `try_with`/`try_borrow_mut` like `record()`: on borrow failure,
    /// returns `None` rather than panicking. This is defensive — the mock never
    /// allocates inside the guard, but the pattern matches the established
    /// reentrancy discipline.
    ///
    /// task #1311 (F6).
    pub(crate) fn take_policy_failure_for(node: u32) -> Option<std::io::Error> {
        POLICY_FAILURE_SLOT
            .try_with(|c| {
                if let Ok(mut b) = c.try_borrow_mut() {
                    b.take().and_then(|(armed_node, err)| {
                        if armed_node == node {
                            Some(err)
                        } else {
                            // Different node: put it back and report none
                            *b = Some((armed_node, err));
                            None
                        }
                    })
                } else {
                    None
                }
            })
            .unwrap_or(None)
    }

    /// Internal: record a call.
    ///
    /// R11-5: reentrancy-safe. The `Vec::push` inside the borrow guard
    /// allocates via the global allocator; if the global allocator IS
    /// sefer-alloc under `numa-aware-mock` (which requires BOTH the feature
    /// AND `--cfg numa_shim_mock`), that allocation re-enters `current_node()` → `record()`, which would
    /// deadlock on a plain `borrow_mut()` (already borrowed). `try_with` +
    /// `try_borrow_mut` silently drops the recording on re-entry — the
    /// RETURNED value (from `current_node_slot`) is unaffected; only the
    /// call-log entry for the re-entrant call is lost, which is acceptable
    /// because tests that inspect the call log never run under a
    /// sefer-alloc-as-global scenario.
    pub(crate) fn record(call: MockCall) {
        let _ = CALLS.try_with(|c| {
            if let Ok(mut b) = c.try_borrow_mut() {
                // task #726 (rust-intel audit §B14): cap the log so an
                // unbounded numa-aware-mock allocation scenario (see this
                // fn's own R11-5 note above) cannot grow this Vec forever.
                if b.len() < CALLS_CAP {
                    b.push(call);
                }
            }
        });
    }
}

/// Outcome of a NUMA-node determination attempt for the calling thread.
///
/// This enum provides finer-grained status information than the simpler
/// `Option<u32>` returned by [`current_node`], exposing WHY a node could
/// not be determined rather than just that it could not.
///
/// As of task #1308, [`current_node`] itself fails closed — it returns `None`
/// for every non-`Resolved` outcome — so the distinction this enum exposes is
/// diagnostic ("WHY detection failed") not a way to recover a node-0 answer.
/// [`current_node`] remains the recommended function for most callers; use
/// `current_node_resolution()` for diagnostic logging / warnings that NUMA
/// hints may not be effective.
///
/// See task #1266, audit finding F4 for background, and task #1308 for the
/// fail-closed origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NodeResolution {
    /// The calling thread's CPU was genuinely resolved to this NUMA node
    /// via the platform topology.
    ///
    /// This variant is returned on Linux when the CPU index from
    /// `sched_getcpu(2)` was found in one of the cached sysfs
    /// `/sys/devices/system/node/nodeN/cpumap` files, on Windows when
    /// `GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx` succeed,
    /// or under the `numa_shim_mock` cfg when the scripted node is not
    /// [`NO_NODE`]. Note that `Resolved(0)` can legitimately indicate a
    /// genuinely single-node system.
    ///
    /// Deliberately carries no field-level `#[non_exhaustive]` (see task
    /// #778/F13 for the precedent this follows): this is a single scalar
    /// field (the resolved node ID) with no plausible second field to grow
    /// into, so marking it would force callers into weaker `matches!`
    /// patterns for no real growth path this shape needs to reserve. The
    /// enum-level `#[non_exhaustive]` above still protects against future
    /// *variants*.
    Resolved(u32),

    /// Linux only: the CPU index was obtained, but no cached sysfs cpumap
    /// contains it.
    ///
    /// This occurs when:
    /// - The real topology was unreadable (e.g., sysfs permissions or
    ///   I/O errors during the first-call cache population).
    /// - The CPU lives on a NUMA node >= 64 — the implementation scans
    ///   only nodes 0..63 because [`reserve_preferred_on_node`] enforces
    ///   a single-`u64` nodemask limit (see the `InvalidNode` error in that
    ///   function's documentation).
    /// - The kernel has no NUMA sysfs at all (single-node system where
    ///   the `/sys/devices/system/node/` directory is absent).
    ///
    /// [`current_node`] returns `None` for this variant as well (task #1308
    /// — it previously collapsed it into `Some(0)`). This variant exists
    /// to distinguish "the platform HAS a NUMA API and detection ran, but
    /// this specific CPU could not be resolved" from [`NodeResolution::Unavailable`]
    /// ("the platform has no NUMA API / the OS call itself failed") — a real,
    /// useful distinction for diagnostic/logging callers even though both map
    /// to `None` in `current_node()`.
    TopologyUnavailable,

    /// The platform provides no NUMA API, or the OS API failed.
    ///
    /// This is returned on:
    /// - macOS (no public NUMA API).
    /// - miri (no real OS topology).
    /// - Unsupported platforms (e.g., FreeBSD, other Unix).
    /// - Linux when `sched_getcpu(2)` fails (returns -1).
    /// - Windows when `GetNumaProcessorNodeEx` fails or returns the
    ///   `MAXUSHORT` sentinel.
    /// - Under the `numa_shim_mock` cfg when the scripted node is [`NO_NODE`].
    ///
    /// [`current_node`] returns `None` for this case.
    Unavailable,
}

/// Return the NUMA-node resolution status for the calling thread.
///
/// This is an **additive** alternative to [`current_node`] that exposes the
/// internal outcome of the node-determination logic on Linux. Both functions
/// now fail closed for non-`Resolved` outcomes (task #1308); this function's
/// added value is the granular "WHY" for diagnostics, not recovering a
/// `Some(0)` that `current_node()` no longer produces.
///
/// The mapping to [`current_node`] is:
///
/// | `current_node_resolution()` | `current_node()` |
/// |-----------------------------|------------------|
/// | `Resolved(n)` | `Some(n)` |
/// | `TopologyUnavailable` | `None` |
/// | `Unavailable` | `None` |
///
/// This function has the same first-call cost on Linux as `current_node()`
/// (up to 64 `open`/`read`/`close` syscalls to populate the topology cache);
/// subsequent calls are pure in-memory operations.
#[must_use]
pub fn current_node_resolution() -> NodeResolution {
    #[cfg(numa_shim_mock)]
    {
        let n = mock::current_node_slot();
        let resolution = if n == NO_NODE {
            NodeResolution::Unavailable
        } else {
            NodeResolution::Resolved(n)
        };
        // task #1277 (review N6): this arm used to deliberately skip
        // recording, with a note claiming recording "would break existing
        // tests' expectations" — it would not: no existing test inspects
        // the log after calling this function (`tests/mock_dispatch.rs`
        // never calls it; `tests/node_resolution.rs` never drains after
        // it). Skipping contradicted the `mock` module's documented
        // "records every invocation" contract, so this call now records
        // like every other public NUMA function. The recorded value is
        // the RESOLVED outcome returned to the caller — intentionally a
        // DIFFERENT convention from `CurrentNode`'s raw pre-remap slot
        // recording (task #1283, review E3: this comment previously and
        // wrongly claimed the two conventions mirrored): `NodeResolution`
        // is itself the semantically meaningful output this function
        // exists to expose, so the resolved outcome is what test
        // assertions want to compare against.
        mock::record(mock::MockCall::CurrentNodeResolution(resolution));
        resolution
    }
    #[cfg(not(numa_shim_mock))]
    {
        platform::current_node_resolution_impl()
    }
}

/// Return the NUMA node id of the calling thread, or `None` if not
/// determinable.
///
/// Returns `Some(n)` only when the calling thread's CPU was genuinely
/// resolved to node `n` via the platform topology.
///
/// Returns `None` when:
/// - The platform provides no NUMA API (macOS, miri, unsupported OS).
/// - The OS API call itself failed.
/// - (Linux, changed in task #1308) The topology could not resolve the
///   calling thread's CPU to any node — including sysfs being entirely
///   absent (single-node kernel with no NUMA support compiled in), a CPU
///   whose real node is >= 64, or any sysfs read/permission failure.
///
/// On Linux, `Some(0)` now occurs ONLY for a CPU genuinely resolved to node
/// 0 — it is NOT returned for absent sysfs or any other undetermined case.
/// For the granular reason WHY detection failed, use
/// [`current_node_resolution()`].
///
/// Historical note: before task #1308, every undetermined case (unreadable
/// sysfs, a CPU on a node >= 64, a kernel with no NUMA sysfs at all) collapsed
/// into the same `Some(0)` as a genuinely node-0-resolved CPU — tasks
/// #722/#725 documented that collapse, and task #1308 made the mapping
/// fail-closed (finding F1 of the fifteenth independent review).
///
/// **First-call cost on Linux** (task #778, round-closing review, F12): the
/// VERY FIRST call to this function on a real Linux host performs up to 64
/// `open`/`read`/`close` syscall triples (one per candidate NUMA node) to
/// populate a process-lifetime topology cache; every subsequent call is a
/// pure in-memory reverse-index lookup with no syscalls at all. For a crate
/// whose selling point is "zero dependencies, `forbid(unsafe_code)`-friendly
/// for consumers," this first-call cost is a contract-level fact a caller on a
/// latency-sensitive cold path should know about — most callers should call
/// this once early (e.g. at startup) rather than assuming every call is
/// equally cheap.
#[must_use]
pub fn current_node() -> Option<u32> {
    #[cfg(numa_shim_mock)]
    {
        let n = mock::current_node_slot();
        mock::record(mock::MockCall::CurrentNode(n));
        // task #722 (rust-intel audit §F2): this used to unconditionally
        // wrap the scripted slot in `Some`, so `set_current_node(NO_NODE)`
        // (`u32::MAX`) produced `Some(NO_NODE)` -- violating this function's
        // own documented "returns `Option`, never the sentinel" guarantee,
        // and making every consumer's `None` branch impossible to exercise
        // under `numa_shim_mock`, the very cfg that exists so CI can assert this
        // wrapping logic. Mirrored the real dispatch's remapping below.
        if n == NO_NODE {
            None
        } else {
            Some(n)
        }
    }
    #[cfg(not(numa_shim_mock))]
    {
        let raw = platform::current_node_impl();
        if raw == NO_NODE {
            None
        } else {
            Some(raw)
        }
    }
}

/// Reserve `size` bytes of anonymous virtual memory with a NUMA preference for
/// `node`, aligned to `align`.
///
/// Requires the `vmem-integration` feature.
///
/// Installs a NUMA preference at RESERVATION time, before the first page fault —
/// the only point where a NUMA preference can be installed before any page is
/// touched (mbind default flags affect only future faults; an already-touched
/// object cannot be retroactively placed — that is why the old `bind_range` was
/// removed, task #1306). `MPOL_PREFERRED` remains a SOFT preference even then:
/// the kernel may fall back under memory pressure, so success means "policy
/// installed," not "physical placement guaranteed."
///
/// ## Per-platform behavior
///
/// ```text
/// // Linux x86_64/aarch64:
/// try_reserve_aligned then mbind(MPOL_PREFERRED) on the COMPLETE
/// underlying OS reservation span (reservation_ptr()/reservation_len(),
/// NOT as_ptr()/len() — policy lifetime aligns with mapping lifetime,
/// no VMA splitting around alignment slack).
///
/// // Linux other arch:
/// Err(UnsupportedArchitecture).
///
/// // Windows:
/// VirtualAllocExNuma at reservation time.
///
/// // macOS / miri / other:
/// Err(UnsupportedPlatform).
/// ```
///
/// On Linux, node ids >= 64 are rejected with `InvalidNode` (single-`u64`
/// nodemask limit; `mbind(2)` itself supports `MAX_NUMNODES` — documented
/// implementation limit, not a kernel limit). Windows forwards any id to the OS
/// and reports its refusal as `Os`.
///
/// ## Best-effort fallback
///
/// This function has NO silent fallback to a no-preference reservation — callers
/// wanting plain memory should call `aligned_vmem::reserve_aligned` directly.
/// Best-effort belongs at the call site:
///
/// ```text
/// reserve_preferred_on_node(size, align, node)
///     .or_else(|_| aligned_vmem::reserve_aligned(size, align))
/// ```
///
/// If the NUMA policy fails AFTER a successful reservation, the reservation is
/// RELEASED (dropped) and the error returned — never a reservation with a
/// half-installed policy, never `Ok` with the preference silently absent
/// (task #1306).
///
/// ## Errors
///
/// - `InvalidArguments`: `size`/`align` violate the reservation contract (zero
///   size, align not a power of two >= page size, size not a page multiple,
///   or size+align overflow).
/// - `InvalidNode`: Linux node id >= 64 (single-u64 nodemask limit).
/// - `Os`: the OS refused the operation; the io::Error was captured immediately
///   at the failing syscall, before any cleanup FFI could overwrite errno.
/// - `UnsupportedPlatform`: the platform provides no NUMA API (macOS, miri, other).
/// - `UnsupportedArchitecture`: Linux architecture without a known `SYS_MBIND`.
///
/// # Semver coupling with `aligned-vmem`
///
/// This function's return type is `aligned-vmem`'s own [`Reservation`]
/// (re-exported here as [`numa_shim::Reservation`](crate::Reservation)), not
/// a `numa-shim`-owned wrapper. That is an intentional, accepted coupling
/// (item 46, `docs/CORRECTNESS_OPEN_ITEMS.md`), not an oversight: `numa-shim`
/// and `aligned-vmem` are sibling crates in this workspace, released
/// together, so a semver-major bump in `aligned-vmem`'s `Reservation` shape
/// forces a coordinated `numa-shim` bump in the same release — a cost
/// already paid by the shared release process, not an extra one. The
/// alternative (a `numa-shim`-owned newtype) was considered and rejected:
/// wrapping `Reservation` would need either full API forwarding (permanent
/// boilerplate that drifts on every `aligned-vmem` change) or an
/// `into_inner()` escape hatch that re-exposes the same type in a public
/// signature anyway, reproducing the coupling it was meant to remove.
#[cfg(feature = "vmem-integration")]
pub fn reserve_preferred_on_node(
    size: usize,
    align: usize,
    node: NodeId,
) -> Result<aligned_vmem::Reservation, ReserveNumaError> {
    #[cfg(numa_shim_mock)]
    {
        mock::record(mock::MockCall::ReservePreferredOnNode {
            size,
            align,
            node: node.get(),
        });
        // task #1306: mirrors the real Linux backend's documented single-u64
        // nodemask limit (nodes 0..=63) so the InvalidNode error path is
        // assertable under the mock on EVERY host, not only Linux.
        //
        // task #1311 (F6, doc-honesty): the mock approximates the REAL LINUX
        // backend's contract here. Real Windows FORWARDS any node id (including >= 64)
        // to the OS and reports the refusal as `Os`; real macOS returns
        // `UnsupportedPlatform` unconditionally BEFORE this check. The mock has
        // no per-platform simulation mode, and making it platform-faithful would
        // break its run-anywhere purpose (mock tests on Windows hosts assert the
        // Linux-shaped `InvalidNode`).
        if node.get() >= 64 {
            return Err(ReserveNumaError::InvalidNode);
        }
        // Mirror the real backends' error mapping instead of the old
        // collapsed `Option`: contract violations are `InvalidArguments`,
        // OS refusals are `Os` — so mock-mode tests can assert the
        // distinction the old `reserve_on_node -> Option` API collapsed.
        let r = match aligned_vmem::try_reserve_aligned(size, align) {
            Ok(r) => r,
            Err(e) => {
                return Err(if e.is_invalid_argument() {
                    ReserveNumaError::InvalidArguments
                } else {
                    ReserveNumaError::Os(std::io::Error::from(e))
                })
            }
        };
        let reservation_len = r.reservation_len();

        // task #1311 (F6): two-stage reserve-then-policy, mirroring the real
        // Linux backend. Check for a scripted policy failure for this node.
        match mock::take_policy_failure_for(node.get()) {
            Some(err) => {
                // task #1311 (F6): mirror the real Linux backend's post-mbind
                // failure path — release the just-made reservation, then return
                // the error. The ORIGINAL error is returned untouched: the mock
                // twin of the real backend's capture-errno-IMMEDIATELY-before-
                // cleanup contract (see the Linux impl's comment in
                // `platform::reserve_preferred_on_node_impl`).
                mock::record(mock::MockCall::InstallPolicy {
                    node: node.get(),
                    reservation_len,
                    succeeded: false,
                });
                drop(r);
                // Record-after-drop ordering is load-bearing: the release record
                // can only be pushed after the reservation's Drop ran, proving
                // the cleanup executed.
                mock::record(mock::MockCall::PolicyFailureRelease { node: node.get() });
                Err(ReserveNumaError::Os(err))
            }
            None => {
                // Policy succeeded: record the install and return the reservation.
                mock::record(mock::MockCall::InstallPolicy {
                    node: node.get(),
                    reservation_len,
                    succeeded: true,
                });
                Ok(r)
            }
        }
    }
    #[cfg(not(numa_shim_mock))]
    {
        platform::reserve_preferred_on_node_impl(size, align, node)
    }
}

// ---------------------------------------------------------------------------
// Linux cpumap parsing and reverse index (task #721, #1310): extracted from
// the Linux-only `platform` module below into a target-INDEPENDENT module.
// This module provides pure byte-slice parsing helpers and a boot-time
// reverse index (cpu -> node) with no syscalls and no OS dependency
// whatsoever -- gating them inside `#[cfg(target_os = "linux")]` was an
// accident of code organization, not a genuine platform requirement, and it
// meant the crate's own most intricate parsing logic (the
// most-significant-word-first cpumap bitmask format) could ONLY be exercised
// on a real Linux host. This crate's own `numa_shim_mock` cfg bypasses the
// whole `platform` module rather than exercising it, so before this change
// there was no way to run these functions on ANY host this project's CI or
// this session actually has. Moving them here (still `#[doc(hidden)]`, not
// part of this crate's public API — see the established "doc-hidden test-only
// forwarders" pattern in `CLAUDE.md`) lets `tests/cpumap_parser.rs` and
// `tests/cpumap_reverse_index.rs` exercise the real parsing logic and reverse
// index construction directly, on every target, closing the round-closing
// audit's §D1a finding for this half of its "zero behavioral oracles" claim.
// ---------------------------------------------------------------------------
/// Test-oracle-only module: sysfs cpumap parsing helpers and reverse index.
///
/// `#[doc(hidden)]` and **exempt from this crate's SemVer guarantees**
/// (task #1289, following the `serde::__private` convention): everything in
/// this module — signatures, names, existence — may change or be removed
/// in ANY release, including patch releases, without a deprecation period.
/// Do not depend on it from code outside this crate's own `tests/`;
/// `cargo-semver-checks` likewise excludes `#[doc(hidden)]` items from its
/// public-API model.
#[doc(hidden)]
pub mod cpumap {
    /// Maximum number of CPUs that can be indexed in the reverse index.
    ///
    /// A Linux node cpumap file is the GLOBAL cpumask `cpumask_of_node(node) &
    /// cpu_online_mask`; bit indices are global logical CPU IDs, all `< nr_cpu_ids
    /// <= NR_CPUS` (kernel config). The kernel's per-arch `NR_CPUS` ceiling:
    /// x86_64 caps at 8192 (arch/x86/Kconfig `range 2 8192`), arm64 at 4096;
    /// other Linux archs are at or below these. This crate's platform matrix
    /// supports Linux x86_64/aarch64, so 8192 entries (1 byte each, 8 KiB)
    /// cover every possible set bit on any supported kernel. A CPU ID >= 8192
    /// stays unmapped and degrades exactly like the old oversized-file case:
    /// not found → `None` → `TopologyUnavailable`.
    ///
    /// This constant deliberately bounds GLOBAL CPU-ID space, correcting the
    /// old `NODE_CPUMAP_BUF_LEN` comment's wrong per-node reasoning (task
    /// #1310, review finding F5). The old comment claimed a 1024-byte buffer
    /// covers "~3640 CPUs on a SINGLE node" — FALSE: on many-node/sparse-ID
    /// systems ALL nodes' cpumaps can simultaneously exceed a small buffer
    /// because the width tracks global ID space, not per-node CPU count.
    pub const MAX_INDEXED_CPUS: usize = 8192;

    /// Sentinel value in the reverse index meaning "no node mapped".
    ///
    /// Node values are 0..=63, so 255 (`u8::MAX`) is unambiguous as the unmapped
    /// sentinel.
    pub const CPU_UNMAPPED: u8 = u8::MAX;

    /// Parse a Linux cpumap and invoke `on_cpu` for every set bit.
    ///
    /// Format: comma-separated hex 32-bit words, most-significant word first,
    /// optional trailing newline. Example: `"00000000,00000003\n"` means CPUs 0
    /// and 1 are in this node.
    ///
    /// This is the SINGLE format interpreter used by both `parse_contains_cpu`
    /// and the reverse-index build (task #1310): there is no second divergent
    /// parsing path.
    ///
    /// Returns `true` on full success, `false` on ANY malformed input (fail-closed).
    /// A malformed token ANYWHERE in the text causes failure — real sysfs never
    /// produces malformed tokens.
    pub fn parse_each_set_cpu(data: &[u8], mut on_cpu: impl FnMut(u32)) -> bool {
        let data = trim_end(data);
        let word_count = data.iter().filter(|&&b| b == b',').count() + 1;
        // Iterate words from MSB to LSB (leftmost word covers highest CPU indices).
        for w in 0..word_count {
            let left_index = word_count - 1 - w;
            let word_str = match nth_token(data, left_index, b',') {
                Some(s) => s,
                None => return false,
            };
            let val = match parse_hex_u32(word_str) {
                Some(v) => v,
                None => return false,
            };
            // Iterate each bit in the word (LSB first = lower CPU IDs).
            for bit in 0..32 {
                if (val >> bit) & 1 == 1 {
                    on_cpu((w * 32 + bit) as u32);
                }
            }
        }
        true
    }

    /// Write `/sys/devices/system/node/nodeN/cpumap\0` into `buf` and return
    /// the nul-terminated slice. Avoids heap allocation.
    pub fn format_sysfs_path(buf: &mut [u8; 64], node: u32) -> &[u8] {
        const PREFIX: &[u8] = b"/sys/devices/system/node/node";
        const SUFFIX: &[u8] = b"/cpumap\0";
        let mut pos = 0usize;
        for &b in PREFIX {
            buf[pos] = b;
            pos += 1;
        }
        // task #727 (rust-intel audit §B7): `tmp` sized `[u8; 10]` -- the
        // maximum decimal digit count for ANY `u32` (`u32::MAX` =
        // 4294967295, 10 digits) -- rather than the previous `[u8; 4]`,
        // which panicked (`tmp[digits]` out of bounds) for `node >= 10000`.
        // Unreachable today (the only caller, `topology()` below, iterates
        // `0u32..64`), but this is a `#[doc(hidden)] pub` function reachable
        // by `tests/cpumap_parser.rs` with an arbitrary `node`, and the old
        // doc comment ("up to 3 digits for node < 1000") already disagreed
        // with the old buffer size (4 bytes = up to 3 digits + none-needed
        // slack, not 4 full digits) -- sizing for the real signature (`u32`)
        // removes the latent panic instead of just re-stating the caller's
        // unstated bound.
        let mut tmp = [0u8; 10];
        let mut n = node;
        let mut digits = 0usize;
        if n == 0 {
            tmp[0] = b'0';
            digits = 1;
        } else {
            while n > 0 {
                tmp[digits] = b'0' + (n % 10) as u8;
                n /= 10;
                digits += 1;
            }
            // Written in reverse; fix ordering.
            tmp[..digits].reverse();
        }
        for &d in tmp.iter().take(digits) {
            buf[pos] = d;
            pos += 1;
        }
        for &b in SUFFIX {
            buf[pos] = b;
            pos += 1;
        }
        &buf[..pos]
    }

    /// Parse a Linux cpumap bitmask string and test whether `cpu_idx` is set.
    ///
    /// Format: comma-separated hex 32-bit words, most-significant first,
    /// optional trailing newline. Example: `"00000000,00000003\n"` means
    /// CPUs 0 and 1 are in this node.
    ///
    /// Now layered on the single `parse_each_set_cpu` interpreter (task #1310).
    /// One behavioral nuance: a malformed token ANYWHERE in the text fails the
    /// probe (previously only the target word's token was validated); both are
    /// fail-closed `false`, and real sysfs never produces malformed tokens.
    pub fn parse_contains_cpu(data: &[u8], cpu_idx: u32) -> bool {
        let mut found = false;
        let ok = parse_each_set_cpu(data, |b| {
            if b == cpu_idx {
                found = true;
            }
        });
        ok && found
    }

    /// Trim trailing `\n`/`\r`/` ` bytes.
    pub fn trim_end(data: &[u8]) -> &[u8] {
        let mut end = data.len();
        while end > 0 && (data[end - 1] == b'\n' || data[end - 1] == b'\r' || data[end - 1] == b' ')
        {
            end -= 1;
        }
        &data[..end]
    }

    /// Return the `n`-th token (0-indexed) delimited by `sep`.
    pub fn nth_token(data: &[u8], n: usize, sep: u8) -> Option<&[u8]> {
        let mut idx = 0usize;
        let mut start = 0usize;
        for (i, &b) in data.iter().enumerate() {
            if b == sep {
                if idx == n {
                    return Some(&data[start..i]);
                }
                idx += 1;
                start = i + 1;
            }
        }
        // Last token (no trailing separator).
        if idx == n {
            Some(&data[start..])
        } else {
            None
        }
    }

    /// Parse a hex string (no `0x` prefix) as `u32`. Returns `None` on error,
    /// including a token longer than 8 hex digits (would silently overflow
    /// `u32`; see task #727 below).
    pub fn parse_hex_u32(s: &[u8]) -> Option<u32> {
        if s.is_empty() {
            return None;
        }
        // task #727 (rust-intel audit §B26): previously absent, so a token
        // longer than 8 hex digits silently WRAPPED (`wrapping_shl` drops
        // the most-significant nibbles) instead of failing like every other
        // malformed input this parser rejects (empty token, invalid digit).
        // Real sysfs cpumap words are fixed 8 hex chars, so this had no live
        // impact, but a silently-wrong value for oversized input is
        // inconsistent with the rest of this parser's fail-closed behavior.
        if s.len() > 8 {
            return None;
        }
        let mut val: u32 = 0;
        for &b in s {
            let digit = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return None,
            };
            val = val.wrapping_shl(4) | digit as u32;
        }
        Some(val)
    }

    /// Fixed-size reverse index mapping CPU IDs to node IDs.
    ///
    /// This is the replacement for the per-node raw-text cache (task #1310,
    /// review findings F5+F10). The old design cached `[[u8; 1024]; 64]` (~64.5
    /// KiB) of raw cpumap text and re-parsed up to ~64 KiB per lookup (O(nodes
    /// × bytes)). This design parses each node's cpumap exactly once at init and
    /// stores a compact 8 KiB array (`[u8; 8192]`) for O(1) lookup.
    ///
    /// Built once inside the `OnceLock` topology initializer; allocation-free
    /// (static storage only). CPU IDs >= `MAX_INDEXED_CPUS` stay unmapped and
    /// degrade exactly like the old oversized-file case (unmapped → `None` →
    /// `TopologyUnavailable`).
    ///
    /// First-mapping-wins semantics for overlapping masks: when `index_node`
    /// processes multiple nodes that both claim the same CPU, the first node
    /// processed wins. The real caller scans nodes in ascending order (0..63),
    /// so this reproduces the old ascending-scan's lowest-node-wins behavior.
    pub struct ReverseIndex {
        map: [u8; MAX_INDEXED_CPUS],
    }

    impl Default for ReverseIndex {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ReverseIndex {
        /// Create a new empty reverse index (all entries unmapped).
        pub const fn new() -> Self {
            Self {
                map: [CPU_UNMAPPED; MAX_INDEXED_CPUS],
            }
        }

        /// Index a node's cpumap text into this reverse index.
        ///
        /// Returns `false` WITHOUT modifying anything if:
        /// - `node > 63` (defensive; the real caller scans 0..64)
        /// - The text is malformed (any token fails hex parsing)
        ///
        /// Implementation: two-stage dry-run then actual write (init-time only,
        /// so the double parse is acceptable). Stage 1 validates the entire
        /// text; stage 2 writes the mapping for each CPU where currently unmapped
        /// (first-mapping-wins). CPUs >= `MAX_INDEXED_CPUS` are silently skipped
        /// (documented degradation).
        pub fn index_node(&mut self, node: u32, data: &[u8]) -> bool {
            if node > 63 {
                return false;
            }
            // Stage 1: dry-run validation (fail-closed per node).
            if !parse_each_set_cpu(data, |_| {}) {
                return false;
            }
            // Stage 2: actually index, first-mapping-wins.
            parse_each_set_cpu(data, |cpu| {
                if (cpu as usize) < MAX_INDEXED_CPUS {
                    let entry = &mut self.map[cpu as usize];
                    if *entry == CPU_UNMAPPED {
                        *entry = node as u8;
                    }
                }
                // CPUs >= MAX_INDEXED_CPUS: silently skipped, same as
                // old buffer-too-small case.
            });
            true
        }

        /// Look up the node for a CPU ID.
        ///
        /// Returns `Some(node_id)` if the CPU is indexed, `None` otherwise.
        /// O(1) array probe after init.
        pub fn lookup(&self, cpu: u32) -> Option<u32> {
            let entry = self.map.get(cpu as usize)?;
            if *entry == CPU_UNMAPPED {
                None
            } else {
                Some(*entry as u32)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Linux-only test-only forwarders (sanctioned pattern per CLAUDE.md)
// ---------------------------------------------------------------------------
/// Test-oracle-only module: Linux-only test forwarders.
///
/// `#[doc(hidden)]` and **exempt from this crate's SemVer guarantees**
/// (task #1289, following the `serde::__private` convention): everything in
/// this module — signatures, names, existence — may change or be removed
/// in ANY release, including patch releases, without a deprecation period.
/// Do not depend on it from code outside this crate's own `tests/`;
/// `cargo-semver-checks` likewise excludes `#[doc(hidden)]` items from its
/// public-API model.
#[cfg(all(target_os = "linux", not(miri), not(numa_shim_mock)))]
#[doc(hidden)]
pub mod linux {
    use super::NodeResolution;

    /// Test-only forwarder: map a CPU index to a `NodeResolution` without
    /// calling `sched_getcpu(2)`.
    ///
    /// This is the same mapping logic used by `current_node_resolution()`,
    /// but with a manually-specified CPU index instead of calling
    /// `sched_getcpu(2)`. It is gated on `not(numa_shim_mock)` because the
    /// real platform implementation is not used when the `numa_shim_mock` cfg is
    /// set.
    ///
    /// This function is safe to call with arbitrarily large CPU indices
    /// (e.g., 1_000_000) — `cpu_to_numa_node_checked` returns `None` when the
    /// CPU is not found in any cached cpumap (including when the CPU index
    /// exceeds the cached topology's word count), so this will return
    /// `NodeResolution::TopologyUnavailable` rather than panicking.
    pub fn dbg_node_resolution_for_cpu(cpu: u32) -> NodeResolution {
        // No unsafe here: `platform` is defined below under the same cfg
        // (`target_os = "linux" && not(miri)`), so `cpu_to_numa_node_checked`
        // is available; it's `pub(crate)`, so this sibling module (both live
        // directly under the crate root) can call it.
        match super::platform::cpu_to_numa_node_checked(cpu) {
            Some(n) => NodeResolution::Resolved(n),
            None => NodeResolution::TopologyUnavailable,
        }
    }

    /// Test-only forwarder: `current_node()`-equivalent `Option<u32>` mapping
    /// for a manually-specified CPU index, without calling `sched_getcpu(2)`.
    ///
    /// Mirrors `current_node()`'s wrapper around `current_node_impl()`: the
    /// raw node from `cpu_to_numa_node` — which returns `NO_NODE` when the
    /// topology cannot resolve the CPU (task #1308) — maps to `None`; any
    /// genuinely resolved node maps to `Some(n)`. Counterfactual oracle for
    /// the fail-closed fix: before task #1308, `cpu_to_numa_node` substituted
    /// `0` for lookup failure, so an unmapped CPU produced `Some(0)` here —
    /// indistinguishable from a genuinely resolved node 0.
    ///
    /// Safe to call with arbitrarily large CPU indices (e.g., 1_000_000):
    /// returns `None` rather than panicking.
    pub fn dbg_current_node_for_cpu(cpu: u32) -> Option<u32> {
        let raw = super::platform::cpu_to_numa_node(cpu);
        if raw == super::NO_NODE {
            None
        } else {
            Some(raw)
        }
    }
}

// ---------------------------------------------------------------------------
// Per-platform implementations
// ---------------------------------------------------------------------------

// ---- Linux (real hardware, not miri) --------------------------------------
#[cfg(all(target_os = "linux", not(miri)))]
// Under `mock`, the public API dispatches to the recording mock instead of
// these platform impls, so every symbol here is (expectedly) unused. `mock`
// exists precisely to bypass the real syscalls; the platform code still must
// compile. Suppress dead-code only in that combination.
#[cfg_attr(numa_shim_mock, allow(dead_code))]
mod platform {
    #[cfg(all(
        feature = "vmem-integration",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    use super::mbind_preferred_linux;
    #[cfg(feature = "vmem-integration")]
    use super::{NodeId, ReserveNumaError};
    use super::{NodeResolution, NO_NODE};

    pub(super) fn current_node_impl() -> u32 {
        // SAFETY: `sched_getcpu` is a POSIX function that returns the CPU index
        // of the calling thread, or -1 on error. No pointer arguments.
        let cpu = unsafe { libc_sched_getcpu() };
        if cpu < 0 {
            return NO_NODE;
        }
        cpu_to_numa_node(cpu as u32)
    }

    pub(super) fn current_node_resolution_impl() -> NodeResolution {
        // SAFETY: `sched_getcpu` is a POSIX function that returns the CPU index
        // of the calling thread, or -1 on error. No pointer arguments.
        let cpu = unsafe { libc_sched_getcpu() };
        if cpu < 0 {
            return NodeResolution::Unavailable;
        }
        match cpu_to_numa_node_checked(cpu as u32) {
            Some(n) => NodeResolution::Resolved(n),
            None => NodeResolution::TopologyUnavailable,
        }
    }

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_preferred_on_node_impl(
        size: usize,
        align: usize,
        node: NodeId,
    ) -> Result<aligned_vmem::Reservation, ReserveNumaError> {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        {
            let raw_node = node.get();
            if raw_node >= 64 {
                // task #722/#1306: single-u64 nodemask limit, now an explicit error
                // instead of the old silent no-op.
                return Err(ReserveNumaError::InvalidNode);
            }
            let r = aligned_vmem::try_reserve_aligned(size, align).map_err(|e| {
                if e.is_invalid_argument() {
                    ReserveNumaError::InvalidArguments
                } else {
                    ReserveNumaError::Os(std::io::Error::from(e))
                }
            })?;
            // Apply the policy to the COMPLETE OS reservation span (task #1306):
            // reservation_ptr()/reservation_len(), not as_ptr()/len().
            // SAFETY: `r` is a fresh live OS reservation we own; mbind only sets
            // kernel page-policy metadata, never payload bytes.
            let rc = unsafe {
                mbind_preferred_linux(r.reservation_ptr(), r.reservation_len(), raw_node)
            };
            if rc == -1 {
                // Capture errno IMMEDIATELY — before the cleanup drop below runs
                // munmap and overwrites it (task #1306, the errno-timing contract).
                let err = std::io::Error::last_os_error();
                drop(r);
                return Err(ReserveNumaError::Os(err));
            }
            Ok(r)
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            let _ = (size, align, node);
            Err(ReserveNumaError::UnsupportedArchitecture)
        }
    }

    /// Scratch buffer size for reading a single node's cpumap file during init.
    ///
    /// This is an init-time SCRATCH buffer for reading ONE node's cpumap text
    /// at a time inside the `OnceLock` initializer. The raw text is no longer
    /// cached per-node (task #1310 replaced the per-node raw-text cache with
    /// the reverse index).
    ///
    /// A complete cpumap for the full indexable global CPU-ID space is
    /// `ceil(MAX_INDEXED_CPUS / 32) = 256` words at 9 bytes per word (8 hex
    /// chars plus one separator; the final separator is the trailing
    /// newline) = 2304 bytes. 4096 (one page, task #720's original size)
    /// holds the complete text for any system within `MAX_INDEXED_CPUS`
    /// with headroom.
    ///
    /// A file WIDER than this buffer implies > ~455 words > 14560 global CPU
    /// IDs — beyond every supported kernel's `NR_CPUS`. Such a file is treated
    /// as a read failure (node not indexed), preserving task #720 §C4's
    /// fail-closed no-silent-truncation rule.
    ///
    /// NOTE: This bound is on GLOBAL CPU-ID space (the file is a global
    /// cpumask), NOT per-node CPU count — explicitly correcting the old
    /// comment's wrong "3640 CPUs on a SINGLE node" justification (review
    /// finding F5, task #1310).
    const CPUMAP_READ_BUF_LEN: usize = 4096;

    /// Boot-static cpu→node topology, parsed via sysfs ONCE and cached for
    /// the life of the process (task #723, rust-intel audit §E5: each
    /// `current_node()` call previously re-derived the mapping via up to 64
    /// open/read/close syscall triples -- expensive on an ALLOCATION path,
    /// since `current_node()` is re-entered from sefer-alloc's `numa-aware`
    /// feature). CPU-hotplug changes after the first call are NOT
    /// reflected -- acceptable for an `MPOL_PREFERRED` hint, itself already
    /// a soft, best-effort preference the kernel can override under memory
    /// pressure.
    ///
    /// task #778 (round-closing review, F12): the caveat above covers
    /// hotplug AFTER the first call; it does not cover the narrower window
    /// this cache also opens: the initializer below reads 64 sysfs files
    /// SEQUENTIALLY, so a hotplug event landing mid-scan can freeze a TORN
    /// snapshot for the rest of the process's lifetime (a CPU that existed
    /// only after the scan passed its node's file now permanently resolves
    /// as undetermined — `None` from `current_node()`, task #1308's fail-closed
    /// mapping; previously it silently fell back to the `Some(0)` single-node
    /// answer). Still acceptable for the same reason as the broader hotplug
    /// caveat above (a soft `MPOL_PREFERRED` hint), but worth naming as its own
    /// distinct property rather than folding it into the "after the first call"
    /// wording, which reads as covering only post-scan hotplug.
    ///
    /// task #777 (rust-intel audit round-closing review, finding F1, HIGH):
    /// task #723's original design cached `Vec<Vec<u8>>` -- ~65 heap
    /// allocations inside the `OnceLock::get_or_init` initializer.
    /// `current_node()` is reachable from `AllocCore::alloc` (via
    /// `current_node_cached` on a cache miss, inside `reserve_small_segment`
    /// / `alloc_large_slow`), and the parent `sefer-alloc` crate's own `M5`
    /// invariant declares that entire path allocation-free/reentrancy-free
    /// specifically so it never re-enters the global allocator. Under a real
    /// `#[global_allocator] = SeferAlloc` + `numa-aware` deployment on
    /// Linux, the FIRST allocation needing a NUMA lookup would have
    /// triggered heap allocation, which re-enters `GlobalAlloc::alloc`,
    /// which re-enters `current_node()`, which re-enters
    /// `OnceLock::get_or_init` on the SAME cell mid-initialization --
    /// documented by `std::sync::OnceLock` as "an error to reentrantly
    /// initialize the cell from `f`... current implementation deadlocks".
    /// Fixed by making the cache allocation-free: the reverse index
    /// (`crate::cpumap::ReverseIndex`) uses only fixed-size static storage
    /// (`[u8; MAX_INDEXED_CPUS]`, 8 KiB), and the scratch buffer below lives
    /// on the stack inside the initializer, so populating the topology touches
    /// no `Vec`/`Box`/heap at all, and the reentrancy hazard is structurally
    /// removed rather than guarded against.
    static TOPOLOGY: std::sync::OnceLock<crate::cpumap::ReverseIndex> = std::sync::OnceLock::new();

    fn topology() -> &'static crate::cpumap::ReverseIndex {
        TOPOLOGY.get_or_init(|| {
            let mut index = crate::cpumap::ReverseIndex::new();
            let mut buf = [0u8; CPUMAP_READ_BUF_LEN];
            for node in 0u32..64 {
                let mut path = [0u8; 64];
                let path_str = crate::cpumap::format_sysfs_path(&mut path, node);
                if let Some(n) = read_cpumap_into(path_str, &mut buf) {
                    index.index_node(node, &buf[..n]);
                }
            }
            index
        })
    }

    /// Map a CPU index to its NUMA node using the cached boot-static
    /// topology (`topology()` above). Pure in-memory reverse-index lookup after
    /// the topology's one-time syscall-driven populate.
    ///
    /// Returns `None` when sysfs NUMA topology files are absent, the CPU
    /// is not found in any cached cpumap (including when the CPU's real node
    /// is >= 64 and thus not in the 0..63 scan range), the topology
    /// cache could not be populated, or the CPU ID is at or beyond the
    /// reverse index's `MAX_INDEXED_CPUS` capacity (task #1310: same
    /// degradation as the old oversized-file case).
    pub(crate) fn cpu_to_numa_node_checked(cpu_idx: u32) -> Option<u32> {
        topology().lookup(cpu_idx)
    }

    /// Map a CPU index to its NUMA node using the cached boot-static
    /// topology (`topology()` above). Pure in-memory reverse-index lookup after
    /// the topology's one-time syscall-driven populate.
    ///
    /// Returns [`NO_NODE`] when sysfs NUMA topology files are absent, the CPU
    /// is not found in any cached cpumap (including when the CPU's real node
    /// is >= 64 and thus not in the 0..63 scan range), the topology
    /// cache could not be populated, or the CPU ID is at or beyond the
    /// reverse index's `MAX_INDEXED_CPUS` capacity (task #1310: same
    /// degradation as the old oversized-file case). This is exactly the set of
    /// conditions under which `cpu_to_numa_node_checked` returns `None`, and
    /// `current_node()` maps this sentinel to `None` (task #1308 — it previously
    /// substituted `0`, making an undeterminable node indistinguishable from a
    /// genuinely resolved node 0).
    pub(crate) fn cpu_to_numa_node(cpu_idx: u32) -> u32 {
        cpu_to_numa_node_checked(cpu_idx).unwrap_or(NO_NODE)
    }

    /// Open the cpumap file at `path` and read its complete contents into
    /// the caller-supplied fixed buffer `out`, returning the byte count.
    ///
    /// The cpumap file format: `"00000000,00000001\n"` — comma-separated
    /// hex 32-bit words, most-significant word first; each word covers 32 CPUs.
    /// Parsing itself is delegated to `crate::cpumap` (task #721) -- a
    /// target-independent module so the parser can be exercised by
    /// `tests/cpumap_parser.rs` on every target, not just real Linux.
    ///
    /// Returns `None` on open/read failure, or if the file is wider than
    /// `out` (task #720, rust-intel audit §C4: a truncated read must never
    /// be silently treated as complete -- the most-significant-word-first
    /// `word_count`/`left_index` arithmetic would misalign on a prefix and
    /// return a WRONG node rather than failing loudly). The caller supplies
    /// an init-time stack scratch buffer (`CPUMAP_READ_BUF_LEN`), so this
    /// function remains allocation-free (task #777: the original heap `Vec`
    /// destination created a reentrancy hazard; fixed-size storage removes it).
    fn read_cpumap_into(path: &[u8], out: &mut [u8]) -> Option<usize> {
        // SAFETY: `path` is a valid nul-terminated C string constructed above.
        // `open` is a POSIX syscall; we check for -1 on error.
        let fd = unsafe { libc_open(path.as_ptr() as *const core::ffi::c_char, 0) };
        if fd < 0 {
            return None;
        }
        let mut total = 0usize;
        loop {
            if total >= out.len() {
                // SAFETY: `fd` was opened by us and must be closed exactly once.
                unsafe { libc_close(fd) };
                return None;
            }
            // SAFETY: `out[total..]` is a valid writable sub-slice of `out`
            // (length `out.len() - total > 0`, checked above);
            // `fd` was returned by the successful `open` call above and not
            // yet closed.
            let n = unsafe {
                libc_read(
                    fd,
                    out[total..].as_mut_ptr() as *mut core::ffi::c_void,
                    out.len() - total,
                )
            };
            if n < 0 {
                // SAFETY: same as above.
                unsafe { libc_close(fd) };
                return None;
            }
            if n == 0 {
                break; // EOF: `out[..total]` holds the complete file.
            }
            total += n as usize;
        }
        // SAFETY: `fd` was opened by us and must be closed exactly once.
        unsafe { libc_close(fd) };
        if total == 0 {
            return None;
        }
        Some(total)
    }

    // -- Raw Linux FFI (no libc crate dependency) ----------------------------

    extern "C" {
        fn sched_getcpu() -> core::ffi::c_int;
        fn open(path: *const core::ffi::c_char, flags: core::ffi::c_int, ...) -> core::ffi::c_int;
        fn read(
            fd: core::ffi::c_int,
            buf: *mut core::ffi::c_void,
            count: usize,
        ) -> core::ffi::c_long;
        fn close(fd: core::ffi::c_int) -> core::ffi::c_int;
    }

    // Thin private wrappers so every call site has its own // SAFETY: comment.
    unsafe fn libc_sched_getcpu() -> core::ffi::c_int {
        // SAFETY: no pointer args; returns current CPU index or -1.
        sched_getcpu()
    }
    unsafe fn libc_open(
        path: *const core::ffi::c_char,
        flags: core::ffi::c_int,
    ) -> core::ffi::c_int {
        // SAFETY: caller must supply a valid nul-terminated path.
        open(path, flags)
    }
    unsafe fn libc_read(
        fd: core::ffi::c_int,
        buf: *mut core::ffi::c_void,
        count: usize,
    ) -> core::ffi::c_long {
        // SAFETY: caller must supply a valid fd and a writable buffer of `count` bytes.
        read(fd, buf, count)
    }
    unsafe fn libc_close(fd: core::ffi::c_int) {
        // SAFETY: caller must supply a valid, open fd that is closed exactly once.
        let _ = close(fd);
    }
}

// ---------------------------------------------------------------------------
// Linux mbind: factored out of `platform` so reserve_preferred_on_node_impl
// (under vmem-integration) can call it.
// ---------------------------------------------------------------------------

/// Install an `MPOL_PREFERRED` policy over `[base, base+len)` favoring NUMA
/// node `node` via `mbind(2)`, returning the syscall result.
///
/// Uses `syscall(SYS_MBIND, …)` — avoids a hard dependency on `libnuma`.
/// The caller is responsible for checking the return value and capturing
/// errno on -1 (task #1306).
#[cfg(all(
    target_os = "linux",
    not(miri),
    feature = "vmem-integration",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
// Reached only from the platform module, which is itself unused under `mock`.
#[cfg_attr(numa_shim_mock, allow(dead_code))]
unsafe fn mbind_preferred_linux(base: *mut u8, len: usize, node: u32) -> i64 {
    // 64-bit nodemask with bit `node` set.
    let nodemask: u64 = 1u64 << node;
    // task #697 (rust-intel audit §F1): `maxnode` is NOT simply "number of
    // bits in the mask" -- the kernel's `get_nodes()` (mm/mempolicy.c)
    // decrements `maxnode` internally before computing which bits are
    // addressable, so `maxnode = 64` only covers bits 0..62, silently
    // dropping bit 63. `libnuma` compensates for this exact
    // kernel quirk by always passing bitmask-size + 1; mirrored here.
    // task #1306: callers now validate node < 64, so this no longer
    // silently drops bit 63 — the caller returns an explicit InvalidNode
    // error instead.
    let maxnode: u64 = 65;
    // SAFETY: `base` is the start of a live OS reservation we own (caller's contract).
    // `mbind` only sets kernel page-policy metadata; it never accesses payload
    // bytes. Return value IS checked by the caller; errno is captured immediately on -1.
    libc_mbind(
        base as *mut core::ffi::c_void,
        len as u64,
        MPOL_PREFERRED,
        &nodemask as *const u64,
        maxnode,
        0,
    )
}

/// `MPOL_PREFERRED`: soft preferred-node policy; kernel falls back on pressure.
#[cfg(all(target_os = "linux", not(miri), feature = "vmem-integration"))]
#[cfg_attr(numa_shim_mock, allow(dead_code))]
const MPOL_PREFERRED: i32 = 1;

/// Syscall number for `mbind(2)` on x86_64.
#[cfg(all(
    target_os = "linux",
    not(miri),
    feature = "vmem-integration",
    target_arch = "x86_64"
))]
#[cfg_attr(numa_shim_mock, allow(dead_code))]
const SYS_MBIND: i64 = 237;

/// Syscall number for `mbind(2)` on aarch64.
#[cfg(all(
    target_os = "linux",
    not(miri),
    feature = "vmem-integration",
    target_arch = "aarch64"
))]
#[cfg_attr(numa_shim_mock, allow(dead_code))]
const SYS_MBIND: i64 = 235;

// `syscall(2)` from glibc/musl — always present, does not require libnuma.
#[cfg(all(
    target_os = "linux",
    not(miri),
    feature = "vmem-integration",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
extern "C" {
    fn syscall(number: i64, ...) -> i64;
}

#[cfg(all(
    target_os = "linux",
    not(miri),
    feature = "vmem-integration",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[cfg_attr(numa_shim_mock, allow(dead_code))]
unsafe fn libc_mbind(
    addr: *mut core::ffi::c_void,
    len: u64,
    mode: i32,
    nodemask: *const u64,
    maxnode: u64,
    flags: u32,
) -> i64 {
    // SAFETY: SYS_MBIND is the correct syscall number for this architecture.
    // `addr` is a live mapping; `nodemask` points to a valid stack-allocated u64.
    // Return value IS checked by the caller; errno is captured immediately on -1.
    syscall(
        SYS_MBIND,
        addr,
        len as usize,
        mode as i64,
        nodemask,
        maxnode as usize,
        flags as i64,
    )
}

// ---------------------------------------------------------------------------
// Windows platform module
// ---------------------------------------------------------------------------
#[cfg(all(windows, not(miri)))]
// Under `mock`, the public API dispatches to the recording mock instead of
// these platform impls, so every symbol here is (expectedly) unused. `mock`
// exists precisely to bypass the real syscalls; the platform code still must
// compile. Suppress dead-code only in that combination.
#[cfg_attr(numa_shim_mock, allow(dead_code))]
mod platform {
    #[cfg(feature = "vmem-integration")]
    use super::{NodeId, ReserveNumaError};
    use super::{NodeResolution, NO_NODE};

    pub(super) fn current_node_impl() -> u32 {
        let mut proc_num = ProcessorNumber {
            group: 0,
            number: 0,
            reserved: 0,
        };
        // SAFETY: `proc_num` is a valid zeroed `PROCESSOR_NUMBER`; this API
        // fills it in and never fails (documented to always succeed).
        unsafe { GetCurrentProcessorNumberEx(&mut proc_num) };

        let mut node: u16 = 0;
        // task #722 (rust-intel audit §F1): corrected -- the previous
        // comment here said this API "returns 0 on single-node or error",
        // conflating the BOOL return (`ok`) with the OUT-parameter
        // (`node`). The actual contract: `ok == 0` (Win32 `FALSE`) means the
        // call FAILED outright; `node == 0` on a SUCCESSFUL call is the
        // genuine single-node-system answer, not an error signal. Separately
        // (and NOT previously handled at all): Microsoft's own docs for
        // `GetNumaProcessorNodeEx` state the OUT node number is set to
        // `MAXUSHORT` (`u16::MAX`) when the given processor does not exist,
        // while the call STILL reports success -- that sentinel is checked
        // below, after the `ok` check.
        // SAFETY: `proc_num` was filled by `GetCurrentProcessorNumberEx`
        // above and is a valid `PROCESSOR_NUMBER`; `node` is a valid `u16`
        // out-pointer.
        let ok = unsafe { GetNumaProcessorNodeEx(&proc_num, &mut node) };
        if ok == 0 || node == u16::MAX {
            return NO_NODE;
        }
        node as u32
    }

    pub(super) fn current_node_resolution_impl() -> NodeResolution {
        let mut proc_num = ProcessorNumber {
            group: 0,
            number: 0,
            reserved: 0,
        };
        // SAFETY: `proc_num` is a valid zeroed `PROCESSOR_NUMBER`; this API
        // fills it in and never fails (documented to always succeed).
        unsafe { GetCurrentProcessorNumberEx(&mut proc_num) };

        let mut node: u16 = 0;
        // SAFETY: `proc_num` was filled by `GetCurrentProcessorNumberEx`
        // above and is a valid `PROCESSOR_NUMBER`; `node` is a valid `u16`
        // out-pointer.
        let ok = unsafe { GetNumaProcessorNodeEx(&proc_num, &mut node) };
        if ok == 0 || node == u16::MAX {
            return NodeResolution::Unavailable;
        }
        NodeResolution::Resolved(node as u32)
    }

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_preferred_on_node_impl(
        size: usize,
        align: usize,
        node: NodeId,
    ) -> Result<aligned_vmem::Reservation, ReserveNumaError> {
        reserve_aligned_numa(size, align, node.get())
    }

    /// Reserve `size` bytes aligned to `align` with a NUMA preference for `node`
    /// via `VirtualAllocExNuma` directly. This is the **only** way to attach a
    /// NUMA preference to memory on Windows — there is no post-reservation
    /// equivalent to Linux `mbind(2)`.
    ///
    /// Strategy (mirrors `aligned-vmem`'s own Windows reservation,
    /// `win_reserve_commit` in `crates/aligned-vmem/src/lib.rs`): over-reserve
    /// `size + align` bytes as ADDRESS SPACE ONLY (`MEM_RESERVE`, no
    /// `MEM_COMMIT`), find the aligned chunk inside, then commit only the
    /// caller-requested `size` bytes at that aligned sub-range (`MEM_COMMIT`,
    /// still via `VirtualAllocExNuma` for API-site uniformity, though the
    /// NUMA preference is already fixed by the reserve call above — see the
    /// task #778/F2 note below). The WHOLE `over`-byte reservation is then
    /// adopted into an `aligned_vmem::Reservation` via
    /// [`aligned_vmem::Reservation::from_raw_parts`]; its `Drop` / release
    /// path will `VirtualFree(MEM_RELEASE)` the entire span exactly once.
    ///
    /// task #724 (rust-intel audit): the previous version committed the
    /// FULL `over = size + align` bytes in one `MEM_RESERVE | MEM_COMMIT`
    /// call -- up to double the commit-charge of the byte range the caller
    /// actually asked for and can use (e.g. `align == size` commits `2 *
    /// size`), silently contradicting this function's own doc claim that it
    /// "mirrors aligned-vmem's own Windows reservation" (aligned-vmem's
    /// `win_reserve_commit` has always reserved `over` but committed only
    /// `commit_len <= size`). Fixed to the same two-call reserve-then-commit
    /// shape.
    ///
    /// task #778 (round-closing review, F2, MEDIUM): the mechanism note
    /// above and this function's two `// SAFETY:` comments originally stated
    /// the `node` argument "has no effect" on the `MEM_RESERVE` call and
    /// "takes effect" on the `MEM_COMMIT` call — the EXACT INVERSE of
    /// Microsoft's documented `VirtualAllocExNuma` contract. Per the
    /// Win32 API reference, `nndPreferred` is "used only when allocating a
    /// NEW VA region (either committed or reserved)... ignored when the API
    /// is used to commit pages in a region that already exists" — so `node`
    /// takes effect on the `MEM_RESERVE` call (a new VA region) and is
    /// IGNORED on the `MEM_COMMIT` call (into the region the reserve call
    /// already created). Separately, no `VirtualAllocExNuma` call "actually
    /// allocates physical pages" at all — per the same reference, physical
    /// pages are allocated ON DEMAND at first touch, regardless of which
    /// call reserved/committed the range. The net shipped behavior was
    /// still correct (the preference IS recorded, by the reserve call, and
    /// the commit charge IS halved) — but purely because `node` happened to
    /// be passed on the reserve call too, which the ORIGINAL comments framed
    /// as a harmless no-op kept only "for API uniformity." A reader who
    /// trusted that framing would have every reason to drop `node` from the
    /// (documented-as-inert) reserve call and keep it only on the
    /// (documented-as-load-bearing) commit call — silently disabling Windows
    /// NUMA binding entirely, with no error from either call. Comments
    /// corrected to state the true mechanism.
    ///
    /// Returns `Err(ReserveNumaError::InvalidArguments)` on contract violation
    /// (`align` not a power of two `>= PAGE`, `size` zero or not a multiple of
    /// `PAGE`, or `size + align` overflow). Returns `Err(ReserveNumaError::Os(..))`
    /// with the GetLastError-captured io::Error when the OS refuses the reservation
    /// or the commit (captured immediately at the failing call, before cleanup).
    /// Returns `Ok` on success.
    #[cfg(feature = "vmem-integration")]
    fn reserve_aligned_numa(
        size: usize,
        align: usize,
        node: u32,
    ) -> Result<aligned_vmem::Reservation, ReserveNumaError> {
        use aligned_vmem::PAGE;
        if size == 0 || !align.is_power_of_two() || align < PAGE || !size.is_multiple_of(PAGE) {
            return Err(ReserveNumaError::InvalidArguments);
        }
        let over = size
            .checked_add(align)
            .ok_or(ReserveNumaError::InvalidArguments)?;

        // SAFETY: `VirtualAllocExNuma(GetCurrentProcess(), NULL, over,
        // MEM_RESERVE, PAGE_READWRITE, node)` reserves (but does not commit)
        // `over` bytes of address space, returning the base or NULL on
        // refusal. task #778 (F2): `node` IS load-bearing on this call --
        // per Microsoft's documented `nndPreferred` contract, the NUMA
        // preference is recorded when allocating a NEW VA region (reserved
        // or committed), which this call is; it is the ONLY call in this
        // function where `node` has any effect (see the corrected mechanism
        // note on this function's own rustdoc above).
        let raw = unsafe {
            VirtualAllocExNuma(
                GetCurrentProcess(),
                core::ptr::null_mut(),
                over,
                MEM_RESERVE,
                PAGE_READWRITE,
                node,
            )
        };
        if raw.is_null() {
            // Capture GetLastError IMMEDIATELY — the reservation was refused,
            // so there is nothing to release; no other call has had a chance
            // to overwrite it yet (task #1306, the errno-timing contract).
            let err = std::io::Error::last_os_error();
            return Err(ReserveNumaError::Os(err));
        }
        let raw_u = raw as usize;
        // Checked alignment arithmetic: `raw_u` is a Win32-returned base (page-
        // aligned, so overflow needs an allocation near the top of the address
        // space), but do not rely on that silently wrapping if it ever happens —
        // release the reservation and return InvalidArguments (syscall SUCCEEDED,
        // so there is no OS error to capture; this is an argument-domain overflow).
        let Some(rounded) = raw_u.checked_add(align - 1) else {
            // SAFETY: `raw` came from the MEM_RESERVE above and was never
            // handed out; releasing before returning InvalidArguments cannot double-free.
            unsafe { VirtualFree(raw, 0, MEM_RELEASE) };
            return Err(ReserveNumaError::InvalidArguments);
        };
        let base_u = rounded & !(align - 1);
        let base = base_u as *mut u8;

        // SAFETY: `VirtualAllocExNuma(.., base, size, MEM_COMMIT, ..,
        // node)` commits exactly the caller-requested `size` bytes at the
        // aligned sub-range within the just-reserved `over`-byte region
        // (`base + size <= raw + over` by construction: `base <= raw +
        // align - 1` rounds down to `raw + align`, and `over = size +
        // align`). task #778 (F2): `node` has NO effect on this call --
        // per Microsoft's documented `nndPreferred` contract, the NUMA
        // preference is ignored when committing pages into a region that
        // already exists (the `MEM_RESERVE` call above already created it);
        // passed through for API-site uniformity only, not because it does
        // anything here. Physical pages are not allocated by EITHER call --
        // Windows allocates them on demand at first touch, regardless of
        // which call reserved/committed the range. NULL indicates commit-
        // charge exhaustion; the reservation is released and Os error returned.
        let committed = unsafe {
            VirtualAllocExNuma(
                GetCurrentProcess(),
                base.cast(),
                size,
                MEM_COMMIT,
                PAGE_READWRITE,
                node,
            )
        };
        if committed.is_null() {
            // Commit failed — capture GetLastError IMMEDIATELY (task #1306, the
            // errno-timing contract), BEFORE the VirtualFree cleanup below
            // overwrites it. Then release the reservation and return the
            // captured error.
            let err = std::io::Error::last_os_error();
            // Release the reservation. Returning Os(err) is still correct even if
            // this release itself fails: the caller never received an owning
            // handle, so handing one out now would risk a double-release. The
            // release's own failure is unreportable through this Result signature
            // (we're already returning the commit error; there's no room to carry
            // a second error from cleanup). Silent by choice, matching this file's
            // other unrecoverable cleanup paths (task #1275 N5).
            //
            // SAFETY: `raw` was returned by the `MEM_RESERVE` call above and
            // has not been handed to any caller yet; releasing before
            // returning `Os(err)` cannot double-free.
            let _ = unsafe { VirtualFree(raw, 0, MEM_RELEASE) };
            return Err(ReserveNumaError::Os(err));
        }

        // Win32 contract: committing into an already-reserved region returns the
        // base address of that region subrange, i.e. exactly `base`. task
        // #1304 (P2): this was a `debug_assert_eq!` — compiled to nothing in
        // release builds, so a contract violation there would have proceeded
        // to `from_raw_parts` with a mismatched `base`, constructing a
        // `Reservation` whose bookkeeping does not match what was actually
        // committed. Checked unconditionally now: on mismatch, fail closed —
        // release the reservation (the commit succeeded, so there is no OS
        // error to capture) and return a contract-violation error.
        if committed.cast::<u8>() != base {
            // SAFETY: `raw` was returned by the `MEM_RESERVE` call above and
            // has not been handed to any caller yet; releasing before
            // returning the error cannot double-free.
            let _ = unsafe { VirtualFree(raw, 0, MEM_RELEASE) };
            return Err(ReserveNumaError::Os(std::io::Error::other(
                "VirtualAllocExNuma MEM_COMMIT returned an unexpected base — Win32 contract violation (task #1304)"
            )));
        }

        // SAFETY of from_raw_parts:
        // - `base` is non-null, valid for `size` bytes (it's inside the
        //   `over`-byte reservation since `align <= over - size`), aligned
        //   to `align` (by construction above), and its `size`-byte range
        //   was just committed above. Win32 contract: `committed == base` for
        //   commit into an already-reserved region, checked unconditionally
        //   above (task #1304: a mismatch releases the reservation and
        //   returns the error before this call is reached).
        // - `raw` is the start of the OS reservation, non-null.
        // - `over = size + align` is the full reservation length, multiple of PAGE.
        // - `align` was just used to align `base` — same value.
        // - The reservation will be released exactly once when the returned
        //   handle's `Drop` fires (or via `release` after `into_parts`).
        // - The reservation was created with `MEM_RESERVE` and the `size`-byte
        //   sub-range separately committed with `MEM_COMMIT` →
        //   `VirtualFree(MEM_RELEASE)` on the WHOLE `over`-byte span will
        //   accept it (matches aligned-vmem's own `win_reserve_commit` shape).
        let r = unsafe {
            aligned_vmem::Reservation::from_raw_parts(
                base,
                size,
                raw as *mut u8,
                over,
                align,
                false, // ordinary VirtualAllocExNuma pages -- MEM_LARGE_PAGES is never requested here
            )
        };
        Ok(r)
    }

    /// Mirrors `PROCESSOR_NUMBER` from the Windows SDK.
    #[repr(C)]
    struct ProcessorNumber {
        group: u16,
        number: u8,
        reserved: u8,
    }

    // task #726 (rust-intel audit §B25): `ProcessorNumber` is passed by
    // pointer to `GetCurrentProcessorNumberEx`/`GetNumaProcessorNodeEx` --
    // the hand-written mirror currently matches the real `PROCESSOR_NUMBER`
    // layout (size 4, align 2, offsets 0/2/3), but nothing pinned that
    // before this assertion, so a future field edit (reordering, adding a
    // field) would silently corrupt the out-parameter write these two FFI
    // calls make into it, with no compile-time signal.
    const _: () = {
        assert!(core::mem::size_of::<ProcessorNumber>() == 4);
        assert!(core::mem::align_of::<ProcessorNumber>() == 2);
        assert!(core::mem::offset_of!(ProcessorNumber, group) == 0);
        assert!(core::mem::offset_of!(ProcessorNumber, number) == 2);
        assert!(core::mem::offset_of!(ProcessorNumber, reserved) == 3);
    };

    extern "system" {
        fn GetCurrentProcessorNumberEx(proc_number: *mut ProcessorNumber);
        fn GetNumaProcessorNodeEx(processor: *const ProcessorNumber, node_number: *mut u16) -> i32;
    }

    // `VirtualAllocExNuma` is the load-bearing call: it is the ONLY way to
    // attach a NUMA preference to a reservation on Windows (`VirtualAlloc`
    // chooses the node by kernel heuristic; there is no `mbind`-equivalent
    // for post-reservation policy installation). Declared locally to avoid pulling
    // `windows-sys` / `winapi` just for one syscall.
    #[cfg(feature = "vmem-integration")]
    extern "system" {
        // task #778 (round-closing review, F9): moved here from the
        // always-compiled extern block above -- its only two call sites
        // (`reserve_aligned_numa`) are already `vmem-integration`-gated, so
        // leaving it in the unconditional block made `cargo clippy
        // --all-targets -- -D warnings` fail on this crate's DEFAULT
        // feature set (what `cargo add numa-shim` produces) with "function
        // `GetCurrentProcess` is never used" -- every downstream Windows
        // consumer's default build saw this warning, and no CI job for this
        // crate runs clippy at all to catch it either way.
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn VirtualAllocExNuma(
            h_process: *mut core::ffi::c_void,
            lp_address: *mut core::ffi::c_void,
            dw_size: usize,
            fl_allocation_type: u32,
            fl_protect: u32,
            nnd_preferred: u32,
        ) -> *mut core::ffi::c_void;
        // task #724: needed to release a reservation whose commit step
        // failed, before any `aligned_vmem::Reservation` (whose own `Drop`
        // would otherwise own that release) has been constructed.
        fn VirtualFree(
            lp_address: *mut core::ffi::c_void,
            dw_size: usize,
            dw_free_type: u32,
        ) -> i32;
    }

    #[cfg(feature = "vmem-integration")]
    const MEM_RESERVE: u32 = 0x0000_2000;
    #[cfg(feature = "vmem-integration")]
    const MEM_COMMIT: u32 = 0x0000_1000;
    #[cfg(feature = "vmem-integration")]
    const MEM_RELEASE: u32 = 0x0000_8000;
    #[cfg(feature = "vmem-integration")]
    const PAGE_READWRITE: u32 = 0x04;
}

// ---- macOS stub -----------------------------------------------------------
// `not(miri)` is required here (matching the other three sibling
// `mod platform` blocks above -- Linux, Windows, and the generic
// fallback -- task #778/F11: line numbers drift with every edit to this
// file, so this is described by role instead of a citation that goes
// stale silently): without it, this block and the separate
// `#[cfg(miri)] mod platform` block below (any-OS-under-miri stub) BOTH
// satisfy their cfg simultaneously when running miri on macOS
// (`target_os = "macos"` is true AND `miri` is true), causing `mod platform`
// to be defined twice (E0428). No CI job caught this because every miri job
// runs on `ubuntu-latest` and the macOS job (`numa-shim-macos`) runs plain
// `cargo test`, never miri — the two conditions never crossed until an
// explicit macOS+miri CI job was added. If you ever touch this block or the
// `#[cfg(miri)]` block below, keep them mutually exclusive.
#[cfg(all(target_os = "macos", not(miri)))]
#[cfg_attr(numa_shim_mock, allow(dead_code))]
mod platform {
    #[cfg(feature = "vmem-integration")]
    use super::{NodeId, ReserveNumaError};
    use super::{NodeResolution, NO_NODE};

    /// macOS has no public NUMA API. Always returns `NO_NODE`.
    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    pub(super) fn current_node_resolution_impl() -> NodeResolution {
        NodeResolution::Unavailable
    }

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_preferred_on_node_impl(
        size: usize,
        align: usize,
        node: NodeId,
    ) -> Result<aligned_vmem::Reservation, ReserveNumaError> {
        let _ = (size, align, node);
        Err(ReserveNumaError::UnsupportedPlatform)
    }
}

// ---- miri stub (any OS under miri) ----------------------------------------
#[cfg(miri)]
#[cfg_attr(numa_shim_mock, allow(dead_code))]
mod platform {
    #[cfg(feature = "vmem-integration")]
    use super::{NodeId, ReserveNumaError};
    use super::{NodeResolution, NO_NODE};

    /// Under miri NUMA detection is not meaningful. Always returns `NO_NODE`.
    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    pub(super) fn current_node_resolution_impl() -> NodeResolution {
        NodeResolution::Unavailable
    }

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_preferred_on_node_impl(
        size: usize,
        align: usize,
        node: NodeId,
    ) -> Result<aligned_vmem::Reservation, ReserveNumaError> {
        let _ = (size, align, node);
        Err(ReserveNumaError::UnsupportedPlatform)
    }
}

// ---- Fallback: unsupported platform (e.g. FreeBSD, other Unix) ------------
#[cfg(not(any(target_os = "linux", windows, target_os = "macos", miri,)))]
#[cfg_attr(numa_shim_mock, allow(dead_code))]
mod platform {
    #[cfg(feature = "vmem-integration")]
    use super::{NodeId, ReserveNumaError};
    use super::{NodeResolution, NO_NODE};

    /// Unsupported platform: always returns `NO_NODE`.
    pub(super) fn current_node_impl() -> u32 {
        NO_NODE
    }

    pub(super) fn current_node_resolution_impl() -> NodeResolution {
        NodeResolution::Unavailable
    }

    #[cfg(feature = "vmem-integration")]
    pub(super) fn reserve_preferred_on_node_impl(
        size: usize,
        align: usize,
        node: NodeId,
    ) -> Result<aligned_vmem::Reservation, ReserveNumaError> {
        let _ = (size, align, node);
        Err(ReserveNumaError::UnsupportedPlatform)
    }
}
