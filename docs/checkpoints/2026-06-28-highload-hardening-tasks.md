# Checkpoint — 2026-06-28 [Created highload-hardening block: 13 tasks #45–#57]

## Session summary

`sefer-alloc` — safe-by-construction allocator (mimalloc-class drop-in). The campaign
of Phases 12–13 was completed in the previous session (all #21–#44 closed, arc `8638a65..4e034e5`
local, origin@Phase 11, NOT pushed; tree clean). This session: restored
context via `/resume` campaign-complete, then at the user's request
recalled the honest assessment "is the allocator ready for DBMS-highload" (answer: architecturally
strong, but production-trust is NOT THERE — heavy gate wired but NOT RUN). Discussed NUMA
(what it is; can it be emulated on a single socket — yes, QEMU `-numa`/`numa=fake`
verifies CORRECTNESS but NOT latency gain, for real numbers you need 2-socket
hardware; in GitHub CI — only functionally via QEMU on top of a runner, there is no real
NUMA on hosted runners). At the user's request, created the **highload-hardening
block — 13 tasks #45–#57**, grouped by the nature of their blockers:
red-circle prerequisite #45 (restore WSL via reboot); penguin Linux/WSL runs #46–#50,#55,#56
(blockedBy #45); window non-WSL harnesses #51–#54 (code first, can run on Windows);
globe #57 push+CI (only on explicit "push"). Tasks NOT started — only created.
Sub-agents NOT invoked (user did not give explicit go-ahead for execution in
this session — the last question: run the window group in parallel while WSL is being fixed, or
wait for WSL and start with TSan). WSL is still broken (REGDB_E_CLASSNOTREG, dism features
awaiting Windows reboot).

## Active goal
none (the previous goal "solve the remaining tasks" was completed; a new one was not armed).
Babysit cron NOT installed in this session.

## TaskList
### in_progress
- (none)
### pending
- #45 Restore WSL (reboot Windows) + confirm uname=x86_64
- #46 TSan cross-thread (blockedBy #45)
- #47 TSan decommit path (blockedBy #45)
- #48 aarch64 weak-memory qemu-user (blockedBy #45)
- #49 Valgrind helgrind/drd/memcheck (blockedBy #45)
- #50 feature matrix + focused miri on Linux (blockedBy #45)
- #51 Soak harness 32/64/128 threads x hours (NOT blocked)
- #52 tokio burn-in as #[global_allocator] (NOT blocked)
- #53 RSS + ring-overflow probe (NOT blocked)
- #54 large/huge + realloc profile (NOT blocked)
- #55 NUMA path + QEMU -numa (blockedBy #45)
- #56 region_ops.rs arbitrary-idiom fix (blockedBy #45)
- #57 Push the Phases 12–13 arc + first green CI (NOT blocked in the graph; gate = explicit "push")
### recently completed
- (the entire campaign #21–#44 — see checkpoint 2026-06-28-campaign-complete.md)

## Decisions
- **Highload-hardening block split into 13 leaf tasks** (not an umbrella) — by the nature of
  the blocker (WSL / non-WSL / explicit permission), so that /resume and parallel branches
  work. Rejected: a single generalized "bring to DBMS-ready" task.
- **Linux runs (TSan/qemu/valgrind/miri-Linux) are blocked by #45 (WSL reboot)** —
  cannot run on Windows; CI is an alternative path, but local iteration is faster.
- **Harnesses #51–#54 are NOT blocked by WSL** — OS semaphores are cross-platform, can run
  on Windows (16 cores), larger Linux/hardware — later.
- **NUMA remains optional** — emulation (QEMU -numa) provides correctness cheaply,
  but latency gains require 2-socket hardware; numbers are needed only for the multi-socket
  DBMS scenario.
- **#57 push — only on explicit word** (the standing prohibition on push without asking is in effect).

## Open questions
- **Where to start execution:** run the window group (#51–#54,#56-code) in parallel while
  the user reboots WSL, OR wait for WSL and start with TSan #46? (awaiting answer)
- **WSL reboot** — needed from the user (#45); after: `wsl -l -v` + `uname -m`.
- **Push** (#57) — only on explicit request.
- **Agent execution** — the user previously gave the go-ahead for "using sh agents"
  for the previous block; for THIS block there is NO explicit go-ahead for sub-agents yet.

## Repo state
```
?? docs/checkpoints/2026-06-26-1230.md
?? docs/checkpoints/2026-06-28-campaign-complete.md
(+ this file will be untracked)
```
```
4e034e5 ci(phase32): extend hardening gate to the allocator — aarch64 + TSan + miri + global-alloc fuzz
92f3288 bench(phase13.6): heap==core pinning — measured, does not help here, kept opt-in (#30)
c49c0f2 feat(phase35): M6 decommit — return empty segments to the OS, M11-free (alloc-decommit)
81fec54 perf(phase13.5): keep REFILL_BATCH=31 — measurement rules out larger (task #29)
465e3ba bench(phase13.7): MT macro-bench (larson + mstress) + honest MALLOC_BENCH refresh
```
(origin/main @ Phase 11; `8638a65..4e034e5` — local, not pushed)
