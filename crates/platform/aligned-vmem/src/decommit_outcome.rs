use crate::error::VmemError;

/// The observed result of one [`try_decommit`](crate::try_decommit) /
/// [`Reservation::try_decommit`](crate::Reservation::try_decommit) call —
/// **task #1180 (PUB-R2 phase 2)**, replacing the pre-#1180
/// `Result<(), VmemError>` return, which reported only whether the
/// CALLER'S ARGUMENTS were valid, never what the OS actually did (or was
/// even asked to do).
///
/// The outer `Result<DecommitOutcome, VmemError>` still reports a caller
/// contract violation exactly as before (`Err(VmemError::invalid_argument())`
/// for a malformed range, `Err(VmemError::os_refusal_unknown_code())` if the
/// one-time OS page-size query failed) — see
/// [`try_decommit`](crate::try_decommit)'s own `# Errors` section. What is
/// new is the `Ok` payload: three variants that distinguish "no backend
/// call was made" from "the backend call was made and refused" from "the
/// SELECTED BACKEND accepted the request", where the pre-#1180 signature
/// collapsed all three into the same `Ok(())`. That acceptance does NOT
/// by itself imply that a real OS syscall ran — under the
/// `aligned_vmem_mock` cfg or miri no syscall runs at all, and `Advised`
/// is the simulated backend's own unconditional answer (see
/// [`DecommitOutcome::Advised`]'s own doc for the per-backend meaning).
///
/// **None of the three variants is a claim about physical memory having
/// actually been reclaimed.** Decommit is best-effort by nature (see
/// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes)
/// for which platforms guarantee reclaim+zero-fill at all) — this type
/// answers "what did this call do", not "what did it accomplish".
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecommitOutcome {
    /// No backend call was made — a Rust-level skip, decided before any
    /// syscall. Two sources, both a well-formed range:
    ///
    /// - **An empty range** (`start == end`), on either
    ///   [`try_decommit`](crate::try_decommit) or
    ///   [`Reservation::try_decommit`](crate::Reservation::try_decommit) — a
    ///   deliberate no-op, checked before any huge-page eligibility question
    ///   even applies.
    /// - **A well-formed, in-span, non-empty range on a huge-page
    ///   reservation** ([`Reservation::is_huge`](crate::Reservation::is_huge)
    ///   == `true`) that does not take the Linux/Android kernel >= 5.18
    ///   huge-aligned real-backend path — see
    ///   [`Reservation::decommit`](crate::Reservation::decommit)'s "Huge-page
    ///   granularity" doc for the exact eligibility split (Windows: always;
    ///   Linux/Android: only a range that is NOT huge-page-size-aligned at
    ///   both endpoints, or when the `huge-pages` feature is off). Only
    ///   [`Reservation::try_decommit`](crate::Reservation::try_decommit) has
    ///   an [`is_huge()`](crate::Reservation::is_huge) to consult, so this
    ///   second source is exclusive to it — the free
    ///   [`try_decommit`](crate::try_decommit) function has no such
    ///   eligibility check and, for a non-empty range, always forwards to the
    ///   backend.
    Skipped,
    /// **The SELECTED BACKEND accepted the request.** What that means
    /// depends on which backend is actually compiled in — this variant does
    /// NOT by itself imply that a real OS syscall ran:
    ///
    /// - **Native backend** (no `aligned_vmem_mock` cfg, not miri): a real
    ///   syscall was made and the kernel/OS accepted it — Linux
    ///   `madvise(2)` returned `0`, or Windows `VirtualFree(MEM_DECOMMIT)`
    ///   returned nonzero (success).
    /// - **`aligned_vmem_mock` cfg** (`RUSTFLAGS="--cfg aligned_vmem_mock"`):
    ///   no syscall runs at all — the mock backend records the call into its
    ///   call log and unconditionally reports `Advised`, without touching
    ///   the OS (see the `crate::mock` module doc). This is a deliberate
    ///   simulation, not an OS acceptance.
    /// - **miri**: the backend is a no-op that always "succeeds" — miri
    ///   models no RSS, so there is no real syscall to refuse.
    ///
    /// **Never a claim that physical pages were actually returned to the
    /// OS**, even on the native backend — let alone that a subsequent access
    /// re-faults zeroed memory. That gap between "the kernel accepted the
    /// advice" and "the kernel acted on the advice as this crate's docs
    /// describe" is exactly what
    /// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes)
    /// answers — it already reports `false` under `aligned_vmem_mock` and
    /// under miri (in addition to Darwin/BSD), precisely because "the
    /// selected backend accepted the request" and "the OS actually
    /// reclaimed physical memory" are two different questions, and that
    /// query is the one that distinguishes them; `Advised` is not a second,
    /// competing channel for the same distinction and must not be read as
    /// one. On Darwin/the four BSDs specifically (the native backend, not
    /// mock/miri), `MADV_DONTNEED` is well known to return `0` while the
    /// pages stay resident (advisory semantics) — `Advised` there is
    /// expected and unremarkable, not a stronger signal than the platform
    /// actually gives.
    ///
    /// **No separate `Simulated` variant, by design (task #1212).** This
    /// type is already `#[non_exhaustive]`, so adding a variant later would
    /// NOT be a semver break — every external `match` on `DecommitOutcome`
    /// already requires a wildcard arm. A `Simulated` variant was
    /// considered and deferred (not rejected outright) because the
    /// mock/miri-vs-real distinction it would carry is already expressed by
    /// the capability-query family:
    /// [`Reservation::decommit_reclaims_and_zeroes`](crate::Reservation::decommit_reclaims_and_zeroes)
    /// and, for the commit side,
    /// [`lazy_commit_is_honored`](crate::lazy_commit_is_honored) both
    /// already answer `false` under `aligned_vmem_mock`/miri specifically
    /// BECAUSE those cfgs substitute simulation for the real OS call — see
    /// each query's own doc for its exclusion list. A third `Simulated`
    /// enum variant would duplicate a distinction the crate already exposes
    /// as a queryable bool; revisit if a caller need emerges that the
    /// existing query family cannot serve (e.g. wanting to branch on
    /// simulated-vs-real from a single `DecommitOutcome` value with no
    /// second call).
    Advised,
    /// The backend call was made and the kernel/OS **refused** it — Linux/Android
    /// `madvise(2)` returned `-1` (e.g. `EINVAL` on a pre-5.18 kernel
    /// receiving a HugeTLB range, or any other kernel-side rejection), or
    /// Windows `VirtualFree(MEM_DECOMMIT)` returned zero (failure, e.g.
    /// `GetLastError()` on a large-page region). Carries
    /// [`VmemError::last_os_error`] captured immediately after the failing
    /// call, same capture-timing contract as every other OS-refusal error in
    /// this crate.
    ///
    /// **One optional fault-injection second source of this payload** (task
    /// #1219): with the `fault-injection` feature enabled AND
    /// [`fault_injection::arm_fail_next_decommit`](crate::fault_injection::arm_fail_next_decommit)
    /// armed, the syscall is replaced by a simulated failure and the payload
    /// is [`VmemError::os_refusal_unknown_code`] instead — no syscall ran, so
    /// there is no `last_os_error` to capture (the same task-#713 rule the
    /// commit-side seam follows). `fault-injection` is a public, process-global,
    /// opt-in Cargo feature (see its own `Cargo.toml` doc comment) — "test-only"
    /// understates it, since any downstream consumer that enables it can arm
    /// this path in a production build too. A build without that feature
    /// enabled can only reach the real-backend path described above.
    Refused(VmemError),
}

impl DecommitOutcome {
    /// `true` for [`DecommitOutcome::Skipped`].
    #[must_use]
    #[inline]
    pub const fn is_skipped(&self) -> bool {
        matches!(self, DecommitOutcome::Skipped)
    }

    /// `true` for [`DecommitOutcome::Advised`].
    #[must_use]
    #[inline]
    pub const fn is_advised(&self) -> bool {
        matches!(self, DecommitOutcome::Advised)
    }

    /// `true` for [`DecommitOutcome::Refused`].
    #[must_use]
    #[inline]
    pub const fn is_refused(&self) -> bool {
        matches!(self, DecommitOutcome::Refused(_))
    }
}
