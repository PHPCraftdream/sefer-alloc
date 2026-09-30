# Source review round 11: active remediation

Independent XS source baseline:
`2cbf515aa39b33da2c2b3eb616af05aace5f5e69`.
Report `docs/reviews/2026-09-30-232702-src-review-xs-sol-round-11.md` is
committed separately as `d7205ff42359b7e49d72f72ea21ac09097f3de99` (docs-only).
Review counts: P0=0/P1=0/P2=1/P3=3/P4=3. No dynamic result or release GO
belongs to that source-only review.

## Current queue

- P2-1, OPEN: experimental remote queue's negative Relaxed hint can skip
  queued reusable capacity before a full return. Parent inspected the actual
  insert/hint/queue path. HS fixes a synchronized empty-free-list fallback
  and tests the weak negative-hint case; Acquire on the hint alone is not
  accepted as a fix.
- P3-1, OPEN: every Small refill currently scans all live Large candidates;
  current active-index improvement removed historical NULL candidates, not
  the K*L clean-live-route work. XXS evaluates a bounded hot policy or a sound
  pending-only protocol, retaining full cold cuts and terminal-last-access.
- P3-2, OPEN design/optimization: current Small sidecar's 288 KiB footprint
  and zeroing cost are factual and documented, not a leak or safety defect.
  XXS evaluates compact/adaptive storage without foreign allocation, lost
  class information or pin-lifetime/provenance changes. A sample <=64 KiB
  budget from the review is not automatically a universal product contract;
  justify any budget and state worst-case-versus-common-case tradeoffs.
- P3-3, OPEN: cold claim scans all prior LIVE states before fresh bump,
  yielding triangular first-touch probes. XXS evaluates a fresh/hint path or
  exact claimable index while preserving failed-materialization retry,
  authoritative slot CAS, saturation invalidation and maintenance exclusion.
- P4-1/2/3: sorted-route update cost, legacy RCU pointer-table copying and
  documented thread-not-region shard binding. Retained as lower-priority
  review observations; no unsafe counterexample or measured benefit inferred.

Only isolated worktrees are active. No new implementation is accepted yet.
After personally reading outputs, run non-vacuous focused checks, repair
failures immediately, commit each accepted task, remove its worktree, then
independent XS12 if confirmed P0-P3 remain. No push or version changes.

R10 validation has a separate manifest and does not certify future R11 fixes.
Full installed-global-allocator Miri and external platform/performance gates
remain explicit limitations. Do not use host OOM or missing execution as a
standalone code finding or a successful test result.
