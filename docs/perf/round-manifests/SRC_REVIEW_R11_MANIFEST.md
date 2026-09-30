# Source review round 11: active remediation

Independent XS source baseline:
`2cbf515aa39b33da2c2b3eb616af05aace5f5e69`.
Report `docs/reviews/2026-09-30-232702-src-review-xs-sol-round-11.md` is
committed separately as `d7205ff42359b7e49d72f72ea21ac09097f3de99` (docs-only).
Review counts: P0=0/P1=0/P2=1/P3=3/P4=3. No dynamic result or release GO
belongs to that source-only review.

## Current queue

- P2-1, accepted in `692b9a935cfec28a4a99fb0ea68cfde234f520a9`: experimental remote queue's negative Relaxed hint could skip
  queued reusable capacity before a full return. Parent inspected the actual
  insert/hint/queue path. The empty-free-list fallback now inspects the queue
  under its mutex before Err, keeping the flag advisory. Parent native
  capacity/epoch/MAX/queue-buffer/doc checks passed45 cases; four reduced Loom
  queue/hint/completion cases passed, including an expected old-algorithm
  counterexample, task `a08334a5-a0da-4310-96c6-2eecf62731f3`. The model
  covers queue semantics, not epoch memory reclamation. Final hooks are gated
  by both internals and bench-internals; final capacity/doc/cap checks36,
  targeted clippy and strict all-feature docs passed,
  `39846b51-00eb-4910-b9c0-09bf83650e59`. No new unsafe site or Miri claim.
- P3-1, OPEN: every Small refill currently scans all live Large candidates;
  current active-index improvement removed historical NULL candidates, not
  the K*L clean-live-route work. Accepted XXS advice chooses separate numeric
  bounded hot checks for Small refill and one full cold rescue on zero refill,
  retaining Large-request full cuts, strict trim and terminal-last-access.
  HS implementation is still unaccepted.
- P3-2, OPEN design/optimization: current Small sidecar's 288 KiB footprint
  and zeroing cost are factual and documented, not a leak or safety defect.
  Accepted XXS advice chooses a uniform/mixed-leaf prototype without foreign allocation, lost
  class information or pin-lifetime/provenance changes. A sample <=64 KiB
  budget from the review is not automatically a universal product contract;
  justify any budget and state worst-case-versus-common-case tradeoffs. The
  candidate's41KiB baseline plus mixed leaves can exceed the old288KiB in
  adversarial all-mixed cases. Spill allocation needs fallible preflight before
  any issue/freelist/bitmap/bump/credit/output mutation, not an abort-on-OOM.
  No production promotion or RSS/CPU win is asserted before actual acceptance.
- P3-3, OPEN: cold claim scans all prior LIVE states before fresh bump,
  yielding triangular first-touch probes. Accepted advice chooses reuse-hint
  first, capped fresh bump while capacity remains, and full scan only at cap
  or explicit cold materialization-OOM recovery. Preserve failed-materialization
  retry, authoritative slot CAS, saturation invalidation/epoch exhaustion and
  maintenance exclusion; temporary older-FREE deferral is a stated policy,
  not strict global recycled-first selection. HS implementation unaccepted.
- P4-1/2/3: sorted-route update cost, legacy RCU pointer-table copying and
  documented thread-not-region shard binding. Retained as lower-priority
  review observations; no unsafe counterexample or measured benefit inferred.

P2 model wiring accepted separately in
`f371c5d03ef7864c18d74f5d37bd686dc5bf0ac9`; source inventory300tests/9rootLoom,
CI static sentinel floor/card headline/next-trigger105. Advice committed
as `efaf828691da02a5d014de867dd80149b4a62229`,
`docs/reviews/2026-10-01-src-r11-production-cost-advice-xxs-sol.md`, read fully.
Production implementations are in three separate, disjoint HS worktrees.

## Transition obligations for production integration

| Mechanism | Prepare / live state | Retirement / failure obligation |
|---|---|---|
| Small issue | Prepare every needed leaf/spill before freelist, bitmap, bump, live-credit or output change; class write precedes handoff | OOM preserves ownership/retry or valid partial batch; pending cut consumes exact credit; unlink closes admission, last pin frees exact System layouts |
| Large publication/hot check | Issued credit keeps route registered; producer terminal CAS is last reservation access; numeric cursor advances before reclaim | Bounded Small prelude may defer; full rescue after zero refill avoids false OOM; strict/cold/large-request cuts remain full |
| Registry claim | Hint is a candidate; capped count creates numeric EMPTY; only successful state CAS grants metadata writer, initialized flag gates dereference | Constructor/chunk OOM advertises retry and cold recovery excludes failed candidate; FREE publication invalidates saturation; stable full negative alone certifies saturation |

Count/storage/probe bounds are not latency/RSS claims; preserve independent
census, cold recovery, negative controls, producer pin and pointer provenance.
After personally reading outputs, run non-vacuous focused checks, repair
failures immediately, commit each accepted task, remove its worktree, then
independent XS12 if confirmed P0-P3 remain. No push or version changes.

R10 validation has a separate manifest and does not certify future R11 fixes.
Full installed-global-allocator Miri and external platform/performance gates
remain explicit limitations. Do not use host OOM or missing execution as a
standalone code finding or a successful test result.
