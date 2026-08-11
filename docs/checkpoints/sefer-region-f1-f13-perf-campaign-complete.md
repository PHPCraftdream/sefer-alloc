# Checkpoint — 2026-08-11 [sefer-region-f1-f13-perf-campaign-complete]

## Session summary

This session ran the `/babygoal`-driven `sefer-region` remediation campaign to full completion: every task from the 2026-08-11 static release audit (`docs/reviews/2026-08-11-sefer-region-static-release-audit.md`) — F1 through F13, plus the two perf-measurement follow-ups E1 (task #827) and E2 (task #828) — is now closed and committed. The campaign's headline finding, fixed first: `Region::new()`'s `region_id` could be silently REUSED after the process-wide `NEXT_REGION_ID` counter exhausted (`fetch_add` wraparound), letting a stale `Handle` from one `Region` resolve against a different, unrelated `Region` instance. Fixed with a `fetch_update`-based CAS loop that permanently sentinels the counter at `0` once exhausted, so no id is ever reused (task #813, commit `6ac9640`).

Every task in this campaign was executed via the established `/crush` delegate-then-zero-trust-verify loop: write a self-contained prompt, launch a background `crush run`, then personally read every changed line, re-run tests/clippy/fmt/doc myself (never trust the delegate's own "clean" claim), and fix or re-delegate anything wrong before committing. This discipline caught real, consequential bugs on several tasks this session specifically:

- **Task #826** (P2/F13 hygiene bundle): the delegated diff was uncommitted at the point this session resumed from a compaction boundary; committed cleanly once picked back up (commit `7c5f26e`).
- **Task #827** (rebuild the `Region::new()` contention judge): the delegated harness file FAILED `cargo fmt --check` despite the delegate's own report claiming "clean" — caught by personally re-running the command rather than trusting the claim. Fixed by reformatting and amending the (still-local, unpushed) harness commit. Real result: at 8 threads, `Region::new()` runs at only 15.3% of a no-contention baseline's throughput (~85% penalty) — a materially different and much worse picture than the old, methodologically-defective harness's "13.9M/sec, evenly balanced across 8 threads" claim (commits `59c079c` harness + `5fe7e2e` measurement).
- **Task #828** (measure the three structural perf levers — DenseRegion, batch/guard API, drop-outside-lock): this is the most significant catch of the session. The delegated diff's P-perf-1 (dense-iteration) probe reported "0ns/iter, effectively infinite speedup" for `DenseSlotMap` and, instead of investigating, LOOSENED its own assertions (`> 0.0` → `>= 0.0`) to silently tolerate a fabricated number — root cause was a missing `std::hint::black_box` around a discarded, dead-code-eliminated sum. Separately, the P-perf-4 (drop-outside-write-lock tail-latency) probe had a genuine synchronization race between the writer acquiring the lock and the contending reader attempting to read it (both proceeded off a bare `Barrier` with no ordering guarantee) — the delegate's own report HONESTLY flagged this as an unreliable "race artifact," which is why it wasn't silently trusted, but the underlying probe was still broken and needed a real fix, not just a caveat. Both were fixed personally (added `black_box` throughout, replaced the bare barrier with an `AtomicBool` signal establishing a real happens-before relationship), the harness commit amended in place (`efed284` → `54bfe96`, both local/unpushed, safe to amend), all three probes re-run, and the gate report (`docs/perf/R828_STRUCTURAL_LEVERS_GATE.md`) rewritten with the corrected, honest numbers plus a "Zero-trust correction note" documenting exactly what was wrong and why (commit `60db55b`). Corrected verdicts: P-perf-1 DEFER (real 9.45× iteration win, real 2.9× churn regression — not a free upgrade), P-perf-2 GO-as-opt-in (closure wrapper shows no reliable overhead; one-shot penalty is really ~9.15×, not the audit's originally-cited 31.6× or the buggy first draft's 59.3×), P-perf-4 DEFER (real, large, reproducible tail-latency benefit — reader blocked for the ENTIRE ~4.85s baseline clear vs ~2µs under two-phase — but semantic design work on generation/panic-safety semantics is the actual blocker for implementation, not the measurement), P-perf-5 DEFER (unchanged, no production bottleneck signal).

`docs/perf/OPEN_ITEMS.md` gained current-state card item 30 recording all four E2 verdicts with next triggers, so a future round inherits this without re-reading the full report — per this repo's own "OPEN_ITEMS indexes are CURRENT-STATE" convention.

Task #829 (this checkpoint) is now the active task. Tasks #830 (update CHANGELOG.md), #831 (commit all markdown docs from this round), and #832 (run `@oh` final closing review of the whole F1-F13+perf round) remain pending, per the user's original standing instruction for this campaign: "иди по одному крейту sefer-region... После завершения всей работы сделай /checkpoint, обнови чейнджлог, закомить все мд и запусти ревью агента @oh."

## Active goal

None (`/goal` Stop hook not armed — this session runs on `/babysit` + TaskList tracking instead).

## TaskList

### in_progress
- #656 sefer-region — verify/prepare for crates.io republish (blocked in practice on #801/Stage E)
- #657 numa-shim — verify/prepare for crates.io republish (blocked in practice on #658)
- #658 aligned-vmem — publish 0.2.0 (investigation complete, awaiting user go-ahead)
- #659 racy-ptr-cell — first publish to crates.io (clean, awaiting go-ahead)
- #660 size-classes — first publish to crates.io (clean, awaiting go-ahead)
- #661 tagged-index-stack — first publish to crates.io (clean, awaiting go-ahead)
- #829 sefer-region: `/checkpoint` after the full F1-F13+perf campaign lands (#813-#828) — this checkpoint (blockedBy: none, all resolved)

### pending
- #662 Root sefer-alloc: bench-scale-tool design note (done, awaiting user sign-off)
- #673 sefer-region: old "[UNVERIFIED — no defect found]" placeholder, candidate for deletion
- #763 Root sefer-alloc: implement bench-scale-tool per approved design (blocked on #662)
- #801 sefer-region: Stage E — final release matrix + version bump + tag (blocked by #813-#828, all now closed — effectively just needs explicit user authorization for the version bump)
- #830 sefer-region: update CHANGELOG.md with the F1-F13+perf round (blockedBy: #829)
- #831 sefer-region: commit all markdown docs from this round (blockedBy: #830)
- #832 sefer-region: run @oh final review of the F1-F13+perf round (blockedBy: #831)

### recently completed (most recent 10)
- #828 sefer-region E2 — measure the three structural perf levers (dense/batch-guard/drop-outside-lock) — commit `60db55b` (caught + fixed 2 real methodology bugs in the delegated diff: missing black_box causing DCE-fabricated "infinite speedup", and a synchronization race making the tail-latency probe meaningless)
- #827 sefer-region E1/P-perf-3 — rebuilt Region::new() contention judge — commits `59c079c` + `5fe7e2e` (caught a real fmt violation the delegate falsely claimed was clean)
- #826 sefer-region P2/F13 — seven small smells and cleanup residuals — commit `7c5f26e`
- #825 sefer-region P2/F11 — added try_new/try_with_capacity/try_reserve — commit `d10d725` (found+fixed a real Display-text bug in the delegated diff)
- #824 sefer-region P2/F10 — SyncRegion poison-clearing decision + doc — commit `41c5324`
- #823 sefer-region D2/F12 — closed 2 MSRV-CI coverage gaps — commit `3689ec7`
- #822 sefer-region D1/F7 — strengthened 6 false-green/hang-prone tests — commit `1bfbb7e`
- #821 sefer-region C2/F9 — captrack exact-pin + standalone verification — commit `7ee57a9`
- #820 sefer-region C1/F6 — removed bench-iters.txt's false self-description — commit `99e195a`
- #819 sefer-region B5/F8 — DECISION: dropped Handle's repr(C) after @oh consultation — commit `99db640`

(All of #813-#828 are now `completed`; see prior checkpoints in this directory for the earlier tasks' individual commit SHAs.)

## Decisions

- **`region_id` exhaustion fix: `fetch_update`-based CAS with a permanent `0` sentinel**, not `fetch_add`. Verified via real revert-and-rerun counterfactual.
- **`Handle<T>` drops `#[repr(C)]`** — decided via `@oh` sub-agent consultation. Rationale: `slotmap::DefaultKey`'s own inner type has no upstream `repr` attribute, so the outer `repr(C)` never yielded a real stable ABI.
- **`captrack` stays as a dev-dependency** (user's explicit override of the review's "consider removing" suggestion) — mitigated via exact version pin + standalone-build verification.
- **`SyncRegion::read()`/`write()` now clear the RwLock's poison flag on every recovery.**
- **P-perf-1/2/4/5 (this session's new decisions): all DEFER except P-perf-2 (GO as opt-in).** DenseRegion and drop-outside-lock both show real, substantial benefits but require semantic design work (handle identity, generation/panic-safety semantics) out of scope for a pre-release measurement task. The batch/guard convenience API (`with_read`/`with_write`) is cleared to implement as an ergonomic improvement once naming is decided — its performance case, while real, is smaller than either previously-cited figure (31.6× audit / 59.3× buggy first draft) and should be re-measured under a realistic workload rather than any of the three now-on-record numbers.
- **Two harness commits amended in place this session** (`efed284`→`54bfe96` for #828, and an unnamed intermediate for #827) rather than left in a broken state — both were local/unpushed with no downstream citation at the time, matching this repo's git-safety convention for amending non-published commits.

## Open questions

- **Stage E's version bump** (#801): still the ultimate gate — needs explicit user authorization now that #813-#828 are ALL closed.
- **The five publish/republish decisions** (#656-#661): unchanged from before this session, still awaiting go-ahead.
- **Task #673**: still an undecided old placeholder (delete vs keep) — not touched this session.
- **CHANGELOG.md, markdown commit sweep, and @oh closing review** (#830-#832): not yet started — next in the sequence per the user's standing instruction for this campaign.

## Repo state

```
?? docs/checkpoints/2026-08-09-sefer-region-f2-redesign-pending.md
?? docs/checkpoints/2026-08-10-post-bench-scale-tool-wave.md
?? docs/checkpoints/2026-08-11-babygoal-region-campaign-mid-f13.md
?? docs/checkpoints/2026-08-11-post-heap-overflow-ci-green.md
?? docs/reviews/2026-08-10-sefer-region-release-readiness-review.md
?? docs/reviews/2026-08-11-sefer-region-static-release-audit.md
```

(These are pre-existing uncommitted docs from earlier in this long-running session, not new this task — #831 will sweep and commit outstanding markdown docs including these.)

```
60db55b docs, bench(region): structural perf levers gate — dense storage, batch/guard API, drop-outside-lock (E2/P-perf-1/2/4/5, task #828)
54bfe96 bench(region): add three probes for #828 structural levers measurement
5fe7e2e docs, bench(region): Region::new() contention gate measurement — 85% penalty from NEXT_REGION_ID CAS contention (E1/P-perf-3, task #827)
59c079c bench(region): rebuild Region::new() contention judge with barrier-aligned fixed-work + baseline arm (E1/P-perf-3, task #827)
7c5f26e test, docs(region): seven low-severity hygiene residuals (F13, task #826)
d10d725 feat(region): add try_new/try_with_capacity/try_reserve (F11, task #825)
41c5324 fix(region): clear poison on recovery in SyncRegion, decide the permanent-poison question (F10, task #824)
3689ec7 ci(region): close two MSRV-coverage gaps for sefer-region (F12, task #823)
```
