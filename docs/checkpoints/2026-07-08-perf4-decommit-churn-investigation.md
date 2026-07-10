# Checkpoint — 2026-07-08 00:00 [perf4-decommit-churn-investigation]

## Session summary

This session continued the sefer-alloc PERF-3 arc's aftermath and general
session/CI hygiene, then pivoted to a new investigation (PERF-4) triggered by
an external signal from a *different* repo. Chronologically: (1) verified and
committed the PERF-3-Ф2/Ф3/Ф4/Ф5 phases (already landed before this
checkpoint's visible window, commits 7d5bada/f13ec4b/3e097be/154d1fa) — Ф5
was an honest NO-GO verdict (the `alloc-runfreelist` run-encoded-freelist
feature regressed every one of the 11 iai benches; feature stays off/opt-in,
source kept for a possible future rework). (2) Ran and showed comparative
wall-clock tables vs mimalloc/System (`cargo bench --bench global_alloc`).
(3) User asked to automate table generation → built `scripts/bench-table.mjs`
(`npm run bench:table`) for canonical, unit-consistent comparison tables, and
`scripts/check-all.mjs` (`npm run check`) as a single pre-push gate (fmt +
clippy across the full CI feature matrix + tests + iai). (4) Pushed 17+
commits; **CI was red** on the first push — found and fixed three real,
pre-existing issues: (a) rustfmt/clippy drift accumulated across the PERF-3
phases (commit `d9767fe`), (b) two `ci.yml` jobs (`loom_thread_free`,
`thread-sanitizer`) still referencing a Cargo feature (`alloc`) and two test
files deleted by the earlier Heap-removal task #204 (commit `ad1d533`), (c) a
brand-new RUSTSEC advisory (`RUSTSEC-2026-0204`, crossbeam-epoch) unrelated to
this session's own changes, added to `deny.toml`'s ignore list with a
carefully-verified rationale (commit `e1ff1e9`). CI went fully green after
all three fixes. (5) User asked to update CHANGELOG.md and README.md —
delegated both to `/crush` (tasks #214/#215), personally verified both diffs
against real commit hashes and a fresh `npm run bench:table` run, committed
as `e4c3be9`. (6) **A genuinely unexplained diff appeared** in
`src/alloc_core/alloc_core.rs` + `src/registry/heap_core.rs` mid-session — a
sound, small realloc-path optimization (eliminates a duplicate `contains_base`
probe by threading an already-proven segment `base` through
`try_realloc_inplace_known_base`) that neither doc-crush-session should have
touched (both were src/-forbidden) and that a full-text grep of the *entire*
local crush session-history database (`.crush/crush.db`) found zero trace of
— origin genuinely could not be established. Verified it independently
(builds, clippy clean, fmt clean, full `production` test suite green 120/120,
`npm run iai` shows 10/11 benches byte-identical and `realloc_grow` slightly
*improved*, −0.12% Ir) before the user explicitly said to commit it anyway
(commit `a664707`, with an honest "provenance unknown, verified independently"
note in the message). (7) Pushed again; final CI run (`28865776369`) is fully
green. (8) User then shared a report from a *different* repo
(`D:\dev\rust\shamir-db\docs\perf\bench-sweep-allocator-comparison-2026-07-07.md`)
— an independent 47-target mixed-microbench sweep (opt-level=1, NOT release)
comparing sefer-alloc 0.2.1 (crates.io) vs 0.3.0 (local path) vs mimalloc vs
System. Finding: **0.3.0 is ~15–18% slower than 0.2.1 on this sweep** (82.1–
82.2s vs 69.5–71.2s wall-clock totals; System itself is fastest here at
63.6–67.5s, which is expected — this workload shape, "many short-lived small
segments cycling quickly" under a single thread, is not sefer-alloc's target
regime). The report's own tentative hypothesis: 0.3.0 added `alloc-decommit` +
`fastbin` to the `production` feature bundle (0.2.1 didn't have decommit),
plausibly expensive on this specific churn shape. I explained what "the
judge" (`npm run iai` / iai-callgrind, deterministic instruction-count
measurement via WSL+Valgrind, used as the GO/NO-GO arbiter for every PERF
experiment this whole arc) means, since the user asked directly. I then
created two new tasks (#216 measurement-only, #217 conditional fix) to
investigate the decommit-churn hypothesis using sefer-alloc's own judge
*before* attempting any fix — mirroring the PERF-2/PERF-3 honest-experiment
methodology used all session. Task #216 was marked `in_progress` but **no
actual crush session has been launched yet** — the user interrupted right
after `TaskUpdate` to ask the judge-clarification question, then invoked
`/checkpoint`. So the investigation is queued but not started.

## Active goal

None (no `/goal` Stop hook is in force this session).

## TaskList

### in_progress
- #216 PERF-4-1: подтвердить/опровергнуть гипотезу decommit-churn на паттерне "много коротких сегментов"  (blockedBy: none)

### pending
- #217 PERF-4-2: закрыть decommit-churn регрессию (если подтверждена в PERF-4-1)  (blockedBy: #216)

### recently completed
(most recent 10, from this and the immediately preceding session — TaskList only currently shows #216/#217 as live; the following completed earlier this session and were cleared/not re-listed by TaskList but are recorded here for continuity from conversation context)
- #215 DOC-2: актуализировать README.md — освежить бенчмарк-таблицы свежими числами
- #214 DOC-1: обновить CHANGELOG.md — добавить записи за SEC-1..6, PERF-1..3, dev-скрипты, CI-фиксы
- #212 PERF-3-Ф5: честный ledger цены/выгоды + go/no-go гейт (NO-GO verdict)
- #211 PERF-3-Ф4: швы жизненного цикла (decommit-reset очищает RunStack)
- #210 PERF-3-Ф3: реконструкция на drain (сердце изменения) + adversarial-аудит
- #209 PERF-3-Ф2: детекция contiguous-run на стороне flush (pack)
- #208 PERF-3-Ф1: RunStack storage + Layout
- #207 PERF-3: run-encoded freelist для recycle-пути (umbrella, honest-rejected at Ф5)
- #204 Удалить публичный тип Heap/with_heap (task predating this session's visible window, referenced repeatedly as the source of the ci.yml drift found this session)
- #198–#203 SEC-1..6 security/compliance remediation

## Decisions

- **PERF-3 (`alloc-runfreelist`) stays off/opt-in, source kept, not reverted.** Ф5's honest-reject found the feature regresses every iai bench (+23–31% on its own cold/recycle targets); root cause is the flush-side implementation *augmenting* the classic linked-list build rather than *diverting* from it. Keeping the code (zero production cost, fully reviewed/tested) preserves the option for a future "PERF-3.5" rework of just the flush-side algorithm.
- **Chose to build `scripts/bench-table.mjs` + `scripts/check-all.mjs`** over continuing to hand-assemble comparison tables ad hoc — a prior ad-hoc table once read as a spurious "20ns→40ns regression" that was actually a µs-per-batch vs ns-per-op unit mixup; a red CI push (rustfmt/clippy/stale-ci-refs) that `npm run check` would have caught locally motivated the second script.
- **Kept the unexplained realloc-path optimization diff, committed with an honest "origin unknown" note**, rather than discarding it — chosen after independently verifying it builds, passes clippy/fmt, passes the full test suite, and shows a small genuine iai improvement (`realloc_grow` −0.12% Ir, no other bench regressed). User made the final call after I presented full investigation findings (exhaustive local crush-session-DB grep found zero authorship trace).
- **New RUSTSEC-2026-0204 (crossbeam-epoch) ignored in `deny.toml` rather than bumped** — `cargo update -p crossbeam-epoch` would be a dependency-version decision requiring explicit request per project convention; verified the crate's own code never calls the vulnerable `fmt::Display` path on an epoch pointer before adding the ignore.
- **PERF-4 investigation ordering: measure before fixing** (task #216 before #217, #217 explicitly conditional on #216's finding) — same discipline as PERF-2/PERF-3, to avoid fixing a hypothesis that turns out not to be the real cause.

## Open questions

- Is the decommit-churn hypothesis from the shamir-db report actually correct? Not yet measured on sefer-alloc's own judge — this is exactly what task #216 exists to answer, and no session has been launched for it yet.
- If #216 confirms the hypothesis, what's the right fix shape for #217 (decommit hysteresis/delay threshold vs a configurable knob)? Not yet designed — deferred until #216's numbers are in.
- The user separately mentioned wanting to eventually hook this shamir-db bench into a flamegraph for a more visual look at what the code is doing — explicitly deferred ("это потом", "пока я просто хочу победить на этом бенче тоже"). Not started, not scheduled.
- Whether "winning" #217 will be validated against sefer-alloc's own synthetic judge only, or also re-run against the actual `D:\dev\rust\shamir-db` sweep (`cargo bench-tool sweep`) for real-world confirmation — task #217's description says "по возможности", i.e. best-effort, not mandatory. `cargo bench-tool` custom subcommand was NOT found installed in this shell environment when checked (`error: no such command: bench-tool`) — accessing it may require locating/building the shamir-db workspace's own tool first.

## Repo state

```
?? docs/security/
```

(`docs/security/` has been untracked/unstaged the entire session — pre-existing, never part of any commit this session; not investigated further as it's out of scope for every task touched.)

```
a664707 perf(realloc): thread proven segment base through the in-place fast path
e4c3be9 docs: sync CHANGELOG.md and README.md with today's session (SEC/PERF/CI)
e1ff1e9 fix(ci): add RUSTSEC-2026-0204 (crossbeam-epoch) to deny.toml ignore list
29087c5 feat(dev): npm run check — single pre-push gate (scripts/check-all.mjs)
ad1d533 fix(ci): remove references to test files/features deleted in task #204
```

All of the above are pushed to `origin/main`; the final CI run
(`28865776369`) after the last push is fully green (all jobs `✓`, one
`NUMA on real Linux kernel` job shows `-` / skipped, which is expected —
env-guarded, not a failure).
