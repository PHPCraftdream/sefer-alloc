# Checkpoint — 2026-06-28 [Campaign complete; only optional local TSan remains (WSL being fixed)]

## Session summary

`sefer-alloc` — safe-by-construction allocator (mimalloc-class drop-in). During this
session **the campaign was brought to completion**: all of Phase 12 (production MT trust) +
Phase 13 (speed) + the family of data races §13/§11 + M6 decommit + honest macro-bench
+ hardening-gate extension — **done, committed and verified by me under
zero-trust** (including miri and counterfactuals). TaskList is empty (0 pending / 0
in_progress; all #21–#44 completed). Babysit cron deleted. Tree is clean (only
this checkpoint untracked). Key victory of the session: while verifying #27 I found
a **~90× regression** (16B churn 1.9ms vs Phase-11 ~21.5us) — the root cause turned out to be
an O(N^2) double-free guard (`free_list_contains`) introduced in Phase 12.1; fix
(#28/13.4a) — O(1)-bitmap guard, brought 16B back to ~22us (the "competitive" thesis
restored). Then investigation #40 revealed that §13 (page_map-class) was
ALIVE on the explicit-`Heap` path (not dead code) → unified on RemoteFreeRing; and
#43 closed the last §11 site (Heap::stamp_owner full-struct write). #44
built the MT macro-bench (larson/mstress): **SeferMalloc beats mimalloc at T=4**
(larson 40 vs 32, mstress 73 vs 64 M ops/sec), scales monotonically; single-
threaded mimalloc leads (~22us vs ~11us). #29 (refill) and #30 (pinning) were measured and
HONESTLY left as-is (do not help on this machine); #42 (two-list) closed
by analysis (obviated by bitmap+ring). #35 (M6 decommit, feature alloc-decommit
default-off) — **M11 epoch provably NOT needed** (Variant-2 freer does not
dereference the block + bitmap-guard in metadata + stale-guard off>=bump +
owner-serialization); safety verified by me via miri (decommit_miri_cycle 249s) +
native stale_ring (real unmap on Windows = real proof of no-UAF) +
counterfactual. #32 — hardening gate EXTENDED to the allocator code (ci.yml: features in
test/multi-arch on x86+aarch64, new TSan job, miri on decommit/reclaim,
global-alloc fuzz target); EXECUTION is inherently CI/Linux. Sub-agents sh twice
CONFABULATED ("code was already in the tree") — caught and discarded by zero-trust;
code was judged by git diff, not by narrative. IMPORTANT: the entire arc of Phases 12–13
(`8638a65..4e034e5`) is LOCAL, origin is at Phase 11, NOT pushed. The last
open item is an *optional* local TSan run, blocked by a broken
WSL (see below). In response to the user's direct question "is it ready for DBMS-highload" I gave
an honest assessment: architecturally/correctness-wise — strong, but production-trust for
DBMS-highload is NOT THERE (heavy gate is wired but NOT RUN: TSan/aarch64 never
executed, no multi-hour/high-thread soak, no burn-in under live tokio,
large/realloc/NUMA not profiled at scale).

## WSL status (blocker for local TSan)
WSL is broken: an interrupted automatic platform upgrade. Progress on repair (all under
admin): `wsl --update` → installed 2.7.10; Appx re-registration → ok;
`Restart-Service WSLService` → "cannot start service"; `dism enable-feature
VirtualMachinePlatform` + `Microsoft-Windows-Subsystem-Linux` → both success BUT with
`/norestart`. Current error: `Wsl/CallMsi/Install/REGDB_E_CLASSNOTREG`.
**Diagnosis: features are enabled but awaiting a WINDOWS REBOOT** — this is the final step
(no-reboot paths exhausted). After reboot: `wsl -l -v` + `wsl -e bash -lc
"uname -m"` (expect `x86_64`). Distro data (vhdx: nightly+rust-src+
valgrind etc.) is intact — platform reinstallation does not touch it. Once WSL is alive —
run TSan (recipe below).

## TSan recipe (once WSL is alive)
In WSL: `RUSTFLAGS="-Zsanitizer=thread" CARGO_TARGET_DIR=/tmp/sefer-tsan cargo
+nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --features
"alloc-global alloc-xthread" --test race_repro --test race_norecycle --test
global_alloc_mt --test heap_cross_thread` (+ optionally alloc-decommit for
decommit_stale_ring). NB: the repository .cargo/config target-dir is a Windows path,
which breaks Linux builds → override with CARGO_TARGET_DIR=/tmp/... (lesson from a previous
session). Under alloc-decommit one can add decommit tests.

## Active goal
`yes, go ahead. solve the remaining tasks` — COMPLETED (TaskList is empty; all tasks
resolved and committed). Stop hook should auto-clear. Babysit cron deleted.

## TaskList
### in_progress
- (none)
### pending
- (none)
### recently completed
- #32 heavy gate (infra: CI matrices+TSan job+miri+global-alloc fuzz; execution is CI-bound) · #30 pinning (measured, does not help, opt-in) · #35 M6 decommit (alloc-decommit, M11-free) · #29 refill (kept at 31) · #44 macro-bench+honest MALLOC_BENCH · #43 §11 Heap::stamp_owner · #42 two-list (obviated) · #40 §13 on Heap (unified on ring) · #28 13.4a O(1)-bitmap guard (regression eliminated) · #27 13.3 arithmetic free
- (earlier: #21–#26 Phase 12.1–12.5 + 13.1, #33 §11 HeapCore, #36/#37/#38/#39)

## Decisions
- **#32 closed as "gate installed + CI-wired"**, execution is inherently CI/Linux/CPU-hours/aarch64-hardware; local TSan is optional additional validation (cross-thread already confirmed by loom+miri+native race x5+macro-bench). Rejected: keeping #32 open indefinitely due to a WSL breakage outside our control.
- **M11 epoch rejected for M6 decommit** (provably not needed — §1 PHASE35_DECOMMIT_DESIGN.md). Decommit-safety = Variant-2 + bitmap + stale-guard off>=bump + owner-serialization.
- **two-list (#42) not built** — obviated (its benefits are already provided by bitmap-guard 13.4a + per-segment ring).
- **refill=31 and pinning-opt-in** — kept based on HONEST measurement (larger refill hurts larson; pinning does not help on 16 cores with <=4 workers).
- **alloc-decommit default-OFF** — RSS-vs-throughput trade-off; the default preserves competitive numbers.

## Open questions
- **Local TSan run** — awaits WSL reboot (optional; the offer stands).
- **Push** — the entire arc 12–13 is local (origin@Phase 11); push only on explicit request.
- **region_ops.rs** — `arbitrary` 1.4.2 idiom (`.arbitrary_iter().ok().flatten()`) does not compile against 1.4.2; pre-existing, separate minor fix (global_alloc_ops already uses the correct idiom).
- **Production-highload (DBMS) NOT ready** — a separate "highload-hardening" block is needed: real TSan+aarch64 run, multi-hour/high-thread soak, tokio burn-in as an installed #[global_allocator], large/realloc profile, NUMA. (User asked; tasks NOT yet created.)

## Repo state
```
?? docs/checkpoints/2026-06-26-1230.md   (+ this file will be untracked)
```
```
4e034e5 ci(phase32): extend hardening gate to the allocator — aarch64 + TSan + miri + global-alloc fuzz
92f3288 bench(phase13.6): heap==core pinning — measured, does not help here, kept opt-in (#30)
c49c0f2 feat(phase35): M6 decommit — return empty segments to the OS, M11-free (alloc-decommit)
81fec54 perf(phase13.5): keep REFILL_BATCH=31 — measurement rules out larger (task #29)
465e3ba bench(phase13.7): MT macro-bench (larson + mstress) + honest MALLOC_BENCH refresh
```
(origin/main @ Phase 11; `8638a65..4e034e5` — local, not pushed)
