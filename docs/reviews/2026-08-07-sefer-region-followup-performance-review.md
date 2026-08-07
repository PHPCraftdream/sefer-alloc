# `sefer-region` — follow-up performance review (read-only)

**Date:** 2026-08-07
**Scope:** `crates/region` (crate `sefer-region`) — genuinely NEW performance ground beyond the
2026-08-07 performance review, the round-closing review (GO-WITH-FIXES), and the nine landed fix
commits. The four angles investigated: (1) real multi-threaded contention on `SyncRegion` —
the deliberately-deferred task #673; (2) `get_cloned`'s clone-under-lock cost for expensive-`Clone`
payloads; (3) `reserve`/`with_capacity` growth-policy pathology; (4) per-`T` monomorphization
codegen size.
**Explicitly NOT re-investigated (settled this round):** wrapper overhead vs raw slotmap (zero,
measured), bench-harness cold-path labeling (fixed, #665), tombstone-iteration cost (measured,
#671), `sync/churn`'s noisy absolute figure (documented caveat, #684).
**Mode:** read-only. No repository file was modified except this report; `git status --porcelain`
was identical before and after (this file plus the pre-existing untracked round-closing review).
No commit was made. Task #673 remains `pending` — this report supplies the measurement it asked
for but does not land a committed harness.
**Measured source identity:** `main` @ `aa24f844dc0b8df46ca928f643055d2f28fc03aa` (clean tree —
`aa24f84` is also the last commit touching `crates/region`), consumed as an unmodified
path-dependency, so the commit SHA alone is the immutable identity (no uncommitted delta existed).
**Evidence base:** five throwaway probes built and run in a scratch cargo project under `%TEMP%`
(path-dependency on `crates/region`, release profile, deleted after use — the same protocol the
round-closing review used). Verbatim output is inlined below; probe design is specified in the
appendix precisely enough to rebuild each one. Host: Windows 10 Pro 10.0.19045, **16 logical
CPUs**, rustc 1.97.0 (2026-07-07), `opt-level = 3`, no LTO, default `std::sync::RwLock`
(SRWLock-backed on this platform). Single noisy dev host — same caveat register as the README
table; 3 runs per scenario, medians quoted, full per-run output inlined.

---

## Verdict

**Task #673's deferred question has a real, non-obvious answer.** The one-shot `SyncRegion` read
API does not merely "cost what RwLock contention costs" — it is **anti-scalable**: on this host,
aggregate `get_cloned` throughput *drops ~4×* going from 1 reader thread to 8 (28.5 → 7.0 Mops/s
median), while per-op latency rises ~32× (34.7 → 1,112 ns). This is not a code defect and no lock
change is recommended (§1's verdict deliberately re-confirms the prior review's keep-the-`RwLock`
decision) — but the *shape* of the result (negative scaling, not flat scaling) plus the measured
~30× mitigation (batching reads under one `read()` guard restores flat ~200 Mops/s at every
thread count) is exactly the guidance the crate's docs currently do not give, and it is cheap to
give. A second, mechanism-level finding: `get_cloned` executes `T::clone` **while holding the
read lock** (a language-level temporary-lifetime fact, verified by measurement), so a 4 MiB
payload turns a writer's worst-case latency from ~14 µs into ~1.8 ms — the doc that says "Prefer
this over `read`" needs one costing sentence. The other two angles came back clean: no
`reserve` growth pathology exists, and monomorphization costs ~1.7 KB of code per distinct `T` —
both recorded as no-action confirmations so the omissions are decisions, not oversights.

| # | Finding | Kind | Impact |
|---|---|---|---|
| 1 | One-shot `SyncRegion` reads are anti-scalable under contention (4× aggregate throughput LOSS at 8 readers); guard-batching restores ~30×; writers stall up to ~220× under 7 readers | new measurement (task #673) + doc guidance owed | **Medium-high** |
| 2 | `get_cloned` runs `T::clone` under the read lock; measured worst-case writer stall ≈ the full clone duration (~1.8 ms for 4 MiB) | mechanism + measurement; doc caveat owed | Medium |
| 3 | `reserve(1)`-per-insert: no pathology (≈ plain insert); `with_capacity` upfront ~2× faster per insert | no-action confirmation | Low |
| 4 | Monomorphization: ~1.7 KB of code per additional distinct `T` (full `Region`+`SyncRegion` surface) | no-action confirmation | Low |

---

## 1. MEDIUM-HIGH — the contended measurement task #673 deferred: one-shot reads anti-scale, writers get crushed, and guard-batching is a measured ~30× mitigation

### What was measured

Three scenarios against a warm `SyncRegion<u64>` pre-populated with 1,024 entries, threads
started on a `Barrier`, 3 full runs (probe source: appendix A1):

- **read-scaling** — N ∈ {1,2,4,8} reader threads, each doing 1,000,000 one-shot
  `get_cloned` calls round-robin over the 1,024 handles (decorrelated start offsets);
- **write-scaling** — N ∈ {1,2,4,8} writer threads, each churning (remove + reinsert) its own
  private handle 300,000 times through the one-shot `insert`/`remove` API;
- **mixed** — 1 writer churning 300,000 cycles while N ∈ {1,3,7} readers loop `get_cloned`
  until the writer finishes (stop-flag window, so both sides are measured under the same
  contention).

### Results (median of 3 runs; full verbatim output in appendix B1)

| Scenario | n=1 | n=2 | n=4 | n=8 |
|---|---|---|---|---|
| one-shot read, per-thread ns/op | 34.7 | 147.1 | 522.8 | 1,112.0 |
| one-shot read, **aggregate Mops/s** | **28.5** | **13.3** | **7.4** | **7.0** |
| one-shot write churn, per-thread ns/cycle | 68.7 | 385.0 | 2,116.5 | 4,259.0 |
| one-shot write churn, aggregate Mcycles/s | 14.3 | 5.1 | 1.9 | 1.9 |
| guard-batched read (64 gets/`read()` guard), per-thread ns/get | 4.6 | 10.3 | 17.0 | 34.7 |
| guard-batched read, **aggregate Mops/s** | **195.9** | **184.5** | **205.9** | **207.8** |

| Mixed (1 writer + N readers) | readers=1 | readers=3 | readers=7 |
|---|---|---|---|
| writer ns/cycle (median; uncontended baseline 68.7) | 286 | 2,694 | 15,150 |
| reader ns/op (median) | 420 | 724 | 1,332 |

Derived ratios (numerator/denominator named per the repo's reporting convention):

- **Aggregate read throughput at 8 readers vs 1 reader** (6.95 / 28.50 Mops/s, medians): the
  system does **4.1× LESS total work** with 8 threads than with 1. Adding readers makes the
  whole system slower, not just each reader.
- **Per-op read latency at 8 readers vs 1** (1,112.0 / 34.7 ns): **32×**.
- **Guard-batched vs one-shot aggregate at n=8** (207.8 / 6.95 Mops/s): **~30×** — and batched
  scaling is flat (195.9 → 207.8 Mops/s across 1→8 threads; the extra threads neither help nor
  hurt, which is the correct expectation for a shared-read workload whose true bottleneck is
  the lock word, not the data).
- **Writer churn under 7 readers vs uncontended** (15,150 / 68.7 ns median): **~220×**. This
  axis is also the noisiest — across the 3 runs the readers=7 writer figure spanned 4,729 to
  17,668 ns/cycle (69×–257×), i.e. SRWLock's writer-vs-reader arbitration under sustained read
  pressure is not just slow but *erratic* on this host. Treat the 220× as an order-of-magnitude
  anchor, same register as the README's `sync/churn` note.

### Mechanism (why negative scaling, not flat)

`get_cloned`'s critical section is ~4–5 ns of work (`st/get_hit` ≈ 4.4–5.0 ns per the README
table), but every one-shot call performs an atomic RMW on the *single shared lock word* to
acquire and release the read lock (`sync_region.rs:64-66` → `RwLock::read`). With multiple
readers, that cache line ping-pongs between cores, and the coherence traffic — not the lookup —
becomes the entire cost. This is inherent to any centralized reader-writer lock guarding a
nanosecond-scale critical section; it is NOT a defect in `SyncRegion`'s code and NOT fixable by
swapping in `parking_lot` (same single lock word; the prior review's §3 rejection of that
dependency stands — this measurement now supplies the previously-missing contended evidence for
it). The batched arm proves the diagnosis: amortizing one acquire/release over 64 gets removes
~98% of the lock-word traffic and the cliff disappears entirely.

### Platform caveat

These absolute numbers are Windows/SRWLock-specific. Linux's futex-based `RwLock` will have
different constants and different writer-arbitration behavior, but the *direction* (per-op
read-lock acquisition anti-scales when the critical section is nanoseconds) is
architecture-level (shared-cache-line RMW), not OS-level. The ~30× batching mitigation ratio
will vary; its sign will not.

### Recommended actions (doc + optional harness; no code change)

1. **Doc guidance, ~4 sentences, two sites.** `SyncRegion`'s struct doc
   (`crates/region/src/sync_region.rs:16-19`) currently frames the one-shot methods purely as
   ergonomic alternatives ("use `read`/`write` for multi-operation transactions"). Add the
   performance half of that truth: under multi-threaded read contention the one-shot methods
   anti-scale (each call pays a shared-cache-line lock acquisition that dominates the ~5 ns
   lookup — measured 4× aggregate throughput loss from 1→8 readers on a 16-CPU host), and
   batching reads under one `read()` guard is the intended contended usage (measured ~30×
   aggregate at 8 threads). Mirror one sentence + the small table into README
   §"Performance". This is the cheapest possible closure of the "ships blind" gap the prior
   review named, and it converts task #673 from "unmeasured decision gate" into published
   numbers a consumer can budget against.
2. **Optional: land the probe as a committed example** (`examples/` or a new bench id) so the
   number is re-measurable per the repo's evidence conventions, and close #673 with it. The
   probe is ~150 lines, uses only `std` + the crate's public API, and appendix A1 specifies it
   fully. If instead #673 is closed doc-only, cite this report's inlined output as the
   evidence and say the harness was deliberately not committed (this report's scratch protocol
   matches the round-closing review's precedent).
3. **Do NOT change the lock.** Re-affirmed with contended data this time: sharding or
   `parking_lot` would chase constants on an axis whose real fix (guard batching / `Arc`
   payloads / different design for read-hot paths) is already available to consumers through
   the existing API surface.

---

## 2. MEDIUM — `get_cloned` executes `T::clone` while holding the read lock; a writer's worst-case latency becomes O(clone duration)

### Mechanism (deterministic, then verified by measurement)

`crates/region/src/sync_region.rs:143`:

```text
self.read().get(handle).cloned()
```

The `RwLockReadGuard` returned by `self.read()` is a temporary in the middle of a method chain,
so it lives until the end of the full expression — meaning `Option::<&T>::cloned`, i.e. the
entire `T::clone`, runs **inside the read-locked window**. For `T = u64` that is invisible
(§1's 34.7 ns includes it). For an expensive-`Clone` `T` it is not: every `get_cloned` extends
the read-side lock hold by the full clone duration, and any writer arriving during that window
blocks for it (readers don't block each other, so this is specifically a write-latency tax).

### Measurement (probe source: appendix A3; full output: appendix B3)

`SyncRegion<Vec<u8>>` holding one 4 MiB value (standalone clone cost on this host: **~2,016 µs**).
One writer thread churns a *different, tiny* (8-byte) entry 20,000 cycles, recording mean and
max per-cycle latency; one reader thread loops `get_cloned` on the 4 MiB entry:

| Writer churn latency | mean ns | max ns |
|---|---|---|
| writer alone (3 runs) | 210 / 230 / 212 | 3,400 / 14,000 / 3,700 |
| vs concurrent 4 MiB `get_cloned` reader | 821 / 814 / 786 | **1,547,000 / 1,537,900 / 1,760,900** |

The max writer stall (~1.5–1.8 ms) is the same order as the standalone clone (~2.0 ms) — the
writer's worst case is, as predicted, "arrive just after a clone started, wait for all of it."
A consumer storing, say, decoded images or large config snapshots in a `SyncRegion` and calling
`get_cloned` from reader threads has silently given every writer a multi-millisecond tail.

### Recommended actions (doc only)

1. `get_cloned`'s doc (`sync_region.rs:134-138`) currently says "Prefer this over
   [`read`](Self::read) when you only need a by-value copy and don't want to hold the guard
   across other work" — which is precisely backwards for expensive-`Clone` `T`: `get_cloned`
   *does* hold the guard across the clone; that is unavoidable given the borrow (`read()`
   followed by a manual clone has the identical window). Add: the clone runs under the read
   lock, so for expensive-`Clone` payloads every `get_cloned` extends the lock hold by the full
   clone and delays writers by up to that much (cite the 4 MiB → ~1.8 ms max-writer-stall
   measurement); for such payloads store `Arc<T>` (or `Arc<Payload>` fields) so the clone is a
   refcount bump. One or two sentences.
2. No code change. An API that clones outside the lock cannot exist for `&T`-borrowed data
   (the value could be removed the moment the guard drops), and `Arc` payloads already solve
   it consumer-side with zero crate surface.

---

## 3. LOW — `reserve`/`with_capacity` growth policy: no pathology; one usable observation (no action owed)

Probe (appendix A2; full output B2): three insert loops of N = 1,000,000 `u64` values each —
plain `insert`, `reserve(1)` before every insert, and `with_capacity(N)` upfront — plus the
capacity trajectory under `reserve(1)`-per-insert at small n.

- **`reserve(1)` before every insert ≈ plain insert** (21.0–21.7 vs 18.6–24.4 ns/insert across
  runs — indistinguishable on this host). `slotmap::SlotMap::reserve` delegates to
  `Vec::reserve`, whose growth is amortized (doubling), so repeated tiny reserves do NOT
  degrade into quadratic reallocation. The measured capacity trajectory confirms textbook
  doubling: 3 → 7 → 15 → 31 → … → 16,383 (Vec capacity 4→8→…, minus slotmap's sentinel slot —
  consistent with the captrack findings already in README §"Capacity growth"). The hypothesis
  named in this review's brief ("does slotmap's growth policy make many small reserves
  meaningfully worse?") is answered: **no**.
- **`with_capacity(N)` upfront is ~2× faster per insert** than growth-as-you-go (9.7–12.3 vs
  18.6–24.4 ns/insert) — the reallocation + element-move traffic of doubling roughly doubles
  the per-insert cost of a bulk fill. The README already recommends `with_capacity` implicitly
  via the capacity section; this number could accompany it, but it is standard-Vec folklore and
  the table already has a cold-vs-warm story. Optional at best.

**No action owed.** Recorded so the negative result is a decision, not an unchecked assumption.

## 4. LOW — monomorphization cost per distinct `T`: ~1.7 KB; no codegen-size concern (no action owed)

The brief asked whether `Region<T>`/`SyncRegion<T>`'s one-instantiation-per-`T` (each carrying
its own copy of slotmap's insert/remove/iter machinery) deserves a doc caveat for consumers with
many distinct `T`s. Measured (probe: appendix A4): two release binaries, identical except one
drives the full public surface (`insert`/`get`/`get_mut`/`contains`/`iter`/`iter_mut`/`reserve`/
`remove`/`clear`/`len`/`is_empty`/`capacity` + `SyncRegion`'s one-shots) for **1** distinct `T`
vs **32** distinct `T`s (newtype structs of 9–40 bytes):

- mono1: 119,808 B; mono32: 173,056 B → Δ = 53,248 B over 31 extra instantiations ≈
  **1,718 B per additional distinct `T`** for the entire two-type surface.

At ~1.7 KB/type, even an unusually generic-heavy consumer with 100 distinct `T`s pays ~170 KB —
noise next to the rest of any real binary. Constants scale mildly with `T`'s size/drop-glue
complexity; order of magnitude is what matters. **No doc caveat warranted; no action owed.**

---

## Summary

Task #673's deferred contended measurement turns out to have real content: `SyncRegion`'s
one-shot read API is **anti-scalable** (4× aggregate throughput LOSS from 1→8 readers; per-op
latency ×32; writers stalled up to ~220× under 7 readers) while **guard-batching under one
`read()` restores flat scaling at ~30× the one-shot aggregate** — inherent RwLock physics, no
code change recommended, but a ~4-sentence doc addition (and optionally a committed probe to
close #673 per the repo's evidence conventions) is owed before a concurrent consumer learns
this in production. Second finding, same file: `get_cloned` runs `T::clone` inside the read
lock — measured worst-case writer stall ≈ the full clone duration (~1.8 ms for a 4 MiB payload)
— and its current doc recommends it in exactly the scenario where that bites; one costing
sentence plus an `Arc<T>` pointer fixes it. The two remaining angles (repeated-small-`reserve`
pathology, per-`T` codegen size) were measured and came back clean; recorded as no-action
confirmations. Nothing in `src/` needs a code change — consistent with every prior review of
this crate.

## Open questions for the maintainer

- **Q1** — Close #673 doc-only (citing this report's inlined numbers) or land the ~150-line
  probe as a committed example/bench first, per the evidence conventions? The latter is small
  and makes the README-facing numbers reproducible from the repo.
- **Q2** — Should the §1 contended table (or a 3-row summary of it) go into README
  §"Performance" next to the existing uncontended rows, given the same single-noisy-host caveat
  already framing that table? My recommendation: yes for the read-scaling and batched rows;
  the mixed-workload writer numbers are too erratic to publish as more than one qualitative
  sentence.
- **Q3** — §2's `get_cloned` caveat lands in the same `sync_region.rs` doc block the
  poisoning-policy text lives in — fold both §1 and §2 doc edits into one `docs(config)`-style
  commit?

---

## Appendix A — probe designs (sufficient to rebuild; scratch project deleted per protocol)

All probes: scratch cargo project under `%TEMP%`, `[dependencies] sefer-region = { path =
"D:/dev/rust/sefer-alloc/crates/region" }`, release profile (opt-level 3, no LTO), rustc 1.97.0.

### A1 — `contended.rs` (§1)

Constants: `POP = 1024`, `READ_OPS = 1_000_000` per reader, `WRITE_OPS = 300_000` per writer.
All threads `std::thread::scope`-spawned, aligned on a `std::sync::Barrier`, each timing its own
window with `Instant`; every value passes through `std::hint::black_box`.

- *read-scaling(n):* fresh `SyncRegion<u64>` + `Vec<Handle<u64>>` of `POP` inserts. Each of `n`
  threads: `idx = (idx + 1) % POP` starting from `t * 37`, then
  `black_box(sr.get_cloned(black_box(handles[idx])))`, `READ_OPS` times. Reports per-thread
  ns/op (mean/min/max) and aggregate Mops/s = `n * READ_OPS / wall`.
- *write-scaling(n):* fresh region pre-padded with `POP` entries; each thread inserts one
  private handle, then loops `let v = sr.remove(h).unwrap(); h = sr.insert(v);` `WRITE_OPS`
  times. Reports per-thread ns/cycle and aggregate Mcycles/s.
- *mixed(n_readers):* same warm region + handle vec; readers loop one-shot `get_cloned` until an
  `AtomicBool` stop flag (set by the writer on completion), accumulating op counts and elapsed
  ns into `AtomicU64`s; the writer churns `WRITE_OPS` cycles as above. Reports writer ns/cycle
  and readers' aggregate ns/op (Σns / Σops).

### A2 — `reserve_probe.rs` (§3)

N = 1,000,000 `u64` inserts per arm, three arms per run (fresh `Region` each): plain `insert`;
`r.reserve(1)` immediately before every insert; `Region::with_capacity(N)` then insert. Reports
ns/insert. Plus a 10,000-insert loop with `reserve(1)`-per-insert recording every change of
`capacity()` (the growth trajectory).

### A3 — `clone_stall.rs` (§2)

`SyncRegion<Vec<u8>>` holding one `vec![0xAB; 4 * 1024 * 1024]` entry plus one 8-byte entry.
Baseline: 100 standalone clones of the 4 MiB vec, mean µs. Then `writer_latency(with_reader)`:
optional reader thread loops `black_box(sr.get_cloned(big))`; writer churns the small entry
20,000 cycles, recording overall mean ns/cycle and the single worst cycle (per-cycle `Instant`
pair). Run 3× in each mode.

### A4 — `mono1.rs` / `mono32.rs` (§4)

Identical bins except for the number of distinct payload types (1 vs 32 `#[derive(Clone)]`
newtype structs `T{i}(u64, [u8; i+1])`, generated). Each type gets one `drive_t{i}()` calling
the full `Region` + `SyncRegion` public surface (all 12 `Region` methods incl. both iterators,
plus `SyncRegion`'s `with_capacity`/`insert`/`contains`/`get_cloned`/`len`/`is_empty`/`remove`/
`clear`) with `black_box` on every result; `main` calls all drivers. Compared: on-disk `.exe`
size of the two release binaries.

## Appendix B — verbatim probe output (complete, untruncated)

### B1 — `contended.exe`, 3 runs

```text
== run 1 ==
read_scaling n=1: per-thread ns/op mean=34.39 (min=34.39 max=34.39)  aggregate=28.80 Mops/s
read_scaling n=2: per-thread ns/op mean=147.09 (min=144.22 max=149.96)  aggregate=13.30 Mops/s
read_scaling n=4: per-thread ns/op mean=493.51 (min=485.85 max=502.24)  aggregate=7.95 Mops/s
read_scaling n=8: per-thread ns/op mean=1111.98 (min=1066.63 max=1150.67)  aggregate=6.95 Mops/s
write_scaling n=1: per-thread ns/cycle mean=73.35 (min=73.35 max=73.35)  aggregate=13.37 Mcycles/s
write_scaling n=2: per-thread ns/cycle mean=316.69 (min=316.26 max=317.12)  aggregate=6.28 Mcycles/s
write_scaling n=4: per-thread ns/cycle mean=2116.51 (min=2097.18 max=2126.15)  aggregate=1.88 Mcycles/s
write_scaling n=8: per-thread ns/cycle mean=4078.04 (min=3922.19 max=4171.43)  aggregate=1.92 Mcycles/s
mixed readers=1: writer ns/cycle=561.50  reader ns/op=419.51  reader total ops=401509
mixed readers=3: writer ns/cycle=2693.86  reader ns/op=693.84  reader total ops=3494207
mixed readers=7: writer ns/cycle=17667.62  reader ns/op=1194.47  reader total ops=31061474
== run 2 ==
read_scaling n=1: per-thread ns/op mean=36.45 (min=36.45 max=36.45)  aggregate=27.14 Mops/s
read_scaling n=2: per-thread ns/op mean=158.35 (min=158.03 max=158.67)  aggregate=12.57 Mops/s
read_scaling n=4: per-thread ns/op mean=522.84 (min=505.51 max=537.19)  aggregate=7.43 Mops/s
read_scaling n=8: per-thread ns/op mean=1096.05 (min=994.97 max=1129.92)  aggregate=7.07 Mops/s
write_scaling n=1: per-thread ns/cycle mean=68.68 (min=68.68 max=68.68)  aggregate=14.31 Mcycles/s
write_scaling n=2: per-thread ns/cycle mean=655.55 (min=650.19 max=660.91)  aggregate=3.02 Mcycles/s
write_scaling n=4: per-thread ns/cycle mean=2358.52 (min=2311.34 max=2378.88)  aggregate=1.68 Mcycles/s
write_scaling n=8: per-thread ns/cycle mean=5152.60 (min=4907.55 max=5259.11)  aggregate=1.52 Mcycles/s
mixed readers=1: writer ns/cycle=281.64  reader ns/op=540.69  reader total ops=156252
mixed readers=3: writer ns/cycle=1260.24  reader ns/op=1147.52  reader total ops=988232
mixed readers=7: writer ns/cycle=4728.68  reader ns/op=1421.66  reader total ops=6984712
== run 3 ==
read_scaling n=1: per-thread ns/op mean=34.67 (min=34.67 max=34.67)  aggregate=28.50 Mops/s
read_scaling n=2: per-thread ns/op mean=135.88 (min=135.34 max=136.42)  aggregate=14.62 Mops/s
read_scaling n=4: per-thread ns/op mean=576.43 (min=570.50 max=581.20)  aggregate=6.87 Mops/s
read_scaling n=8: per-thread ns/op mean=1292.18 (min=1264.47 max=1313.76)  aggregate=6.08 Mops/s
write_scaling n=1: per-thread ns/cycle mean=62.87 (min=62.87 max=62.87)  aggregate=15.60 Mcycles/s
write_scaling n=2: per-thread ns/cycle mean=384.97 (min=381.68 max=388.27)  aggregate=5.13 Mcycles/s
write_scaling n=4: per-thread ns/cycle mean=2002.89 (min=1979.15 max=2026.46)  aggregate=1.97 Mcycles/s
write_scaling n=8: per-thread ns/cycle mean=4258.95 (min=3997.74 max=4318.03)  aggregate=1.85 Mcycles/s
mixed readers=1: writer ns/cycle=286.12  reader ns/op=204.93  reader total ops=418823
mixed readers=3: writer ns/cycle=2818.79  reader ns/op=723.91  reader total ops=3504342
mixed readers=7: writer ns/cycle=15149.90  reader ns/op=1332.25  reader total ops=23880380
```

### B1b — `batched_read.exe` (batch = 64 gets per `read()` guard), 3 runs

```text
== run 0 ==
batched_read(batch=64) n=1: per-thread ns/get mean=4.63  aggregate=195.91 Mops/s
batched_read(batch=64) n=2: per-thread ns/get mean=10.33  aggregate=184.52 Mops/s
batched_read(batch=64) n=4: per-thread ns/get mean=16.87  aggregate=206.37 Mops/s
batched_read(batch=64) n=8: per-thread ns/get mean=34.65  aggregate=207.83 Mops/s
== run 1 ==
batched_read(batch=64) n=1: per-thread ns/get mean=3.67  aggregate=257.39 Mops/s
batched_read(batch=64) n=2: per-thread ns/get mean=10.97  aggregate=171.10 Mops/s
batched_read(batch=64) n=4: per-thread ns/get mean=17.02  aggregate=200.76 Mops/s
batched_read(batch=64) n=8: per-thread ns/get mean=35.64  aggregate=205.46 Mops/s
== run 2 ==
batched_read(batch=64) n=1: per-thread ns/get mean=5.19  aggregate=179.28 Mops/s
batched_read(batch=64) n=2: per-thread ns/get mean=9.87  aggregate=192.74 Mops/s
batched_read(batch=64) n=4: per-thread ns/get mean=17.16  aggregate=205.88 Mops/s
batched_read(batch=64) n=8: per-thread ns/get mean=33.81  aggregate=211.04 Mops/s
```

### B2 — `reserve_probe.exe`, 3 runs

```text
== run 0 ==
plain insert (amortized growth): 24.35 ns/insert
reserve(1) before every insert: 21.74 ns/insert
with_capacity(N) upfront: 9.68 ns/insert
capacity trajectory under reserve(1)+insert: [(0, 3), (3, 7), (7, 15), (15, 31), (31, 63), (63, 127), (127, 255), (255, 511), (511, 1023), (1023, 2047), (2047, 4095), (4095, 8191), (8191, 16383)]
== run 1 ==
plain insert (amortized growth): 20.27 ns/insert
reserve(1) before every insert: 21.24 ns/insert
with_capacity(N) upfront: 10.50 ns/insert
capacity trajectory under reserve(1)+insert: [(0, 3), (3, 7), (7, 15), (15, 31), (31, 63), (63, 127), (127, 255), (255, 511), (511, 1023), (1023, 2047), (2047, 4095), (4095, 8191), (8191, 16383)]
== run 2 ==
plain insert (amortized growth): 18.60 ns/insert
reserve(1) before every insert: 21.05 ns/insert
with_capacity(N) upfront: 12.29 ns/insert
capacity trajectory under reserve(1)+insert: [(0, 3), (3, 7), (7, 15), (15, 31), (31, 63), (63, 127), (127, 255), (255, 511), (511, 1023), (1023, 2047), (2047, 4095), (4095, 8191), (8191, 16383)]
```

### B3 — `clone_stall.exe`

```text
standalone 4 MiB Vec clone: 2015.8 us
run 0: writer alone mean=210 ns max=3400 ns | vs 4MiB-get_cloned reader mean=821 ns max=1547000 ns
run 1: writer alone mean=230 ns max=14000 ns | vs 4MiB-get_cloned reader mean=814 ns max=1537900 ns
run 2: writer alone mean=212 ns max=3700 ns | vs 4MiB-get_cloned reader mean=786 ns max=1760900 ns
```

### B4 — monomorphization binaries

```text
119808  mono1.exe   (1 distinct T)
173056  mono32.exe  (32 distinct T)
Δ = 53248 B / 31 extra instantiations ≈ 1718 B per distinct T
```
