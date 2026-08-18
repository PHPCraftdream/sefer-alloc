# Profiling next steps — deferred plan (filed 2026-08-18)

**Status: DEFERRED by the owner.** Nothing here has been measured. This file
records *what to run and why*, so the decision does not have to be
re-derived. It contains **no** measured numbers and therefore owes no raw
logs or summary CSV under CLAUDE.md's raw-log policy — it is a plan, not a
gate report. The moment any arm below is actually run, its output becomes a
gate report and inherits the full evidence obligations (raw logs, summary
CSV, immutable source identity, path-activation oracle).

Tracked as **item 55** in `docs/perf/OPEN_ITEMS.md`.

---

## 0. What triggered this

Two wall-clock gaps against mimalloc, both reproduced on a QUIET machine
(`npm run bench:table`, 2026-08-18, second run; the first run was taken while
a full `npm run check` was compiling and is void — its own control-arm drift
guard fired at `mimalloc median -25.20%, System median -27.70%`):

| Bench | Size | SeferAlloc | mimalloc | ratio |
|---|---|---:|---:|---:|
| `bench_direct_alloc` (warm bulk burst) | 16 B | 36.7 ns/pair | 11.2 ns/pair | **3.28× slower** |
| `bench_global_alloc_churn_with_teardown` | 1024 B | 99.0 ns/pair | 43.9 ns/pair | **2.25× slower** |

Everything else is at or ahead of parity — notably `bench_churn_alloc` at
1024 B is **8.46× FASTER** than mimalloc, and `segment_decommit_cycle` is
3.39× faster.

**A correction that must not be lost.** On the loaded first run the teardown
gap read 1.40× and mimalloc's 256 B→1024 B jump (×2.17) looked nearly
identical to ours (×2.29), which supported a conclusion that the jump is
mostly shared physics and our pool-cap residual is small. On the quiet
machine that is FALSE: ours is ×3.22 vs mimalloc's ×1.97, and the ratio
widens 1.37× → 2.25×. **Pool-cap exhaustion is the dominant term, not a
residual.** The load was flattering us.

Corroborating signal: with every control arm ~25-30 % faster on the quiet
machine, exactly one id moved the other way —
`global_alloc_churn_with_teardown/SeferAlloc/1024B`, **+13.8 %**. Coherent
reading: syscall-bound work barely benefits from an idle CPU while
user-space work does, so the proportion shifted. That is independent support
for "here we pay the kernel, not our own logic".

---

## 1. The two gaps need DIFFERENT tools — this is the main point of this file

| Question | Right tool | Why the obvious tool is wrong |
|---|---|---|
| 16 B fast path, 3.28× | **`npm run iai`** (callgrind), already wired | User-space, instruction-bound. Deterministic instruction counts beat a sampling profiler outright — zero noise, no sample-count problem |
| 1024 B teardown, 2.25× | **`decommit_calls` counter**, then ETW if needed | Kernel-bound. A user-space flamegraph shows one fat `NtFreeVirtualMemory` frame and nothing actionable — the actionable quantity is HOW MANY TIMES we enter it, which is a count, not a profile |
| "what is hot at all" | flamegraph, but only with `--profile-time` | Without it ~84 % of the graph is criterion's own statistics (see §3) |

---

## 2. Step 1 (cheapest, do first) — `decommit_calls` per size

`benches/global_alloc.rs` already snapshots `sefer.stats()` around each
size's rotated three-arm block and prints the delta to **stderr**:

```
global_alloc_churn_with_teardown/{size}B: decommit_calls delta = N, segments_released_total delta = M
```

mimalloc/System arms never touch Sefer's statics, so the delta isolates our
own kernel traffic at that size. `npm run bench:table` does NOT parse these
lines — they must be read from the raw `cargo bench` stderr.

**The bench's own documented rule:** 16/64/256 B must read **0**; 1024 B
nonzero is the known cap-exceeded case.

| Outcome | Meaning |
|---|---|
| 16/64/256 = 0, 1024 ≫ 0 | Model confirmed: the pool is intact and the 2.25× IS cap exhaustion. The "raise the cap" lever aims at a real target |
| any of 16/64/256 ≠ 0 | **The pool REGRESSED.** The canary fired — that is a bug, and it outranks this whole performance discussion |
| 1024 = 0 too | The diagnosis is wrong: we pay somewhere else (re-reserve, commit, page faults) and the cap lever is useless |

`segments_released_total` is not decoration: it separates *decommit* (pages
returned, address reservation kept) from *release* (segment handed back,
re-reserve required later). Different costs, different fixes.

**Caveat, so the number is not over-read:** the delta is taken around the
WHOLE arm while criterion runs many iterations inside. It answers "did we go
to the kernel and how much", not "what did one call cost".

Do NOT confuse these with the **173/367** figures in the `pool_cap_sweep`
doc comment — those belong to `working_set_cycle`, a differently-shaped
bench, and say nothing about the teardown canary's zero-rule.

---

## 3. Step 2 — flamegraph, if a broad picture is wanted

A full report already exists: **`docs/PROFILE_FLAMEGRAPHS.md`** (2026-06-28,
34 KB) with §0 prerequisites, exact reproduction commands, and a §5
prioritised candidate list. Its recipe stands; **its findings are stale** —
the pool (Mechanism-2), the large cache and much else landed after it.
Historical proof the pipeline pays off: its §5 produced OPT-E (empty
large-segment cache) and OPT-F (in-place small→small realloc), both now
described in `docs/ARCHITECTURE.md` as existing machinery.

Three constraints that decide whether a run is worth anything:

1. **The existing recipe is Linux/WSL2 + `perf`, not native Windows.** It
   therefore profiles `mmap`/`madvise`/`munmap`. Our 1024 B gap is
   `VirtualFree(MEM_DECOMMIT)` on Windows — **a WSL2 profile answers a
   different OS's question.** The doc also records two traps: WSL2's
   `/usr/bin/perf` is broken for recording (use
   `/usr/lib/linux-tools/6.8.0-124-generic/perf` directly) and writing
   `perf.data` onto the NTFS `D:` drive fails with `Bad address` (use
   `CARGO_TARGET_DIR=/tmp/...`).
2. **Profiling criterion profiles criterion.** The existing report's own
   DATA QUALITY WARNING: KDE `bridge_producer_consumer_helper` 52.25 %,
   `libm __ieee754_exp_fma` 20.74 %, `libm exp()` 11.56 % — ~84 % of CPU is
   criterion's statistics, while `AllocCore::alloc` is 1.72 % and the whole
   allocator ~3.7 %. **Fix: criterion's `--profile-time N`**, which runs the
   routine for N seconds and skips the analysis phase entirely; it exists
   for exactly this. The 2026-06-28 recipe does not use it. Compounding
   this, our house benchmark profile is deliberately fast
   (`sample_size(10)`, sub-second measurement), which is far too few samples
   for a sampling profiler — `--profile-time` fixes that too.
3. **Debug info is opt-in, by design.** `Cargo.toml` §0 forbids persistent
   `debug` in `[profile.*]` (it slows linking and inflates rlibs for every
   release build) and prescribes passing it per-run:
   `CARGO_PROFILE_BENCH_DEBUG=line-tables-only`. Note also `[profile.bench]`
   carries `lto = "thin"` + `codegen-units = 1`, so inlining collapses
   frames — realistic, but harder to read.

`cargo-flamegraph.exe` IS installed on this host
(`D:\system_artefact\cargo\bin\`). Natively on Windows it drives Blondie/ETW
and needs **Administrator**; that path is **unverified here**. Native
alternatives for syscall-level work: WPR/WPA from the Windows SDK (free,
ETW — the correct tool for kernel time), Intel VTune, Superluminal.

---

## 4. Levers, if step 1 confirms cap exhaustion

Not a menu to pick from casually — each is a production-default change and
therefore owes a full gate with a path-activation oracle and per-arm
isolation, per CLAUDE.md's R26-4 and R30-8 rules.

| Lever | Mechanism | Cost |
|---|---|---|
| **Retention** — raise the segment-pool cap (currently 4) | teardown stops reaching the OS | RSS. **Already litigated:** R25-5 (task #399) concluded "cap 4→8 wins on BOTH latency AND RSS" and reached CHANGELOG and the indexes; R26-1 (task #410) re-measured with subprocess-per-arm isolation and a hard assert on the RESOLVED cap, and the RSS win **did not reproduce** (RSS-neutral). Any revival must repeat that discipline |
| **Laziness** — evacuate via `decommit_lazy` (`MEM_RESET`/`MADV_FREE`) | the call becomes near-free; the kernel takes pages on demand | Semantics. This campaign spent whole tasks on honest contracts here: lazy pages are not zeroed (#1043), `decommit_lazy` is not lazy on BSD (#970), it carries a documented Windows crash footgun (#898). Changing the evacuation path reopens all of it |
| **Amortisation** — tune the existing decay tick to decommit in background batches | spreads the same work off the hot path | tail latency elsewhere, temporarily higher RSS; and the canary bench stops seeing what it guards if done carelessly |
| **Huge pages** (orthogonal) | a 2 MiB page cuts page-table entries to unmap by 512× | Cuts the CONSTANT in O(pages), not the asymptotics. The hugetlb pool is scarce and this campaign already closed two double-consumption defects (#969, #1069) |

**Do not skip the physics check.** Returning pages to the OS cannot be made
cheaper than O(pages) — no allocator escapes it, and mimalloc's own
256 B→1024 B jump (×1.97) is evidence of the same floor. What CAN be made
flat is not the work but its *presence on the hot path*: at 16/64/256 B the
pool already achieves 0 decommits, i.e. the work is not accelerated, it is
not performed.

**Also keep the workload honest.** The teardown bench is a deliberately
adversarial stress (full teardown every iteration). Real workloads do not
behave that way, and in ordinary churn at 1024 B we are already 8.46× faster
than mimalloc precisely because the OS never enters the picture.

---

## 5. Proposed order

1. `decommit_calls` per size on a quiet machine (free, deterministic,
   answers the 1024 B question outright).
2. Only if the 16 B gap is to be attacked: callgrind/iai over the small
   fast path — per-instruction attribution, no noise.
3. flamegraph last, with `--profile-time`, and with §3.1 in mind (a WSL2
   graph is the Linux arm's picture, not Windows').

Nothing above may be run while another heavy job is active — the first
bench-table run of 2026-08-18 was invalidated exactly that way, and a
concurrent gate has already produced `link.exe` exit `0xC0000142` on this
host.
