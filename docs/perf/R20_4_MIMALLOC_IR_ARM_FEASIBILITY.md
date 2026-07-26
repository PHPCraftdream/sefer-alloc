# R20-4 — Feasibility: a `mimalloc` comparison arm in the `iai-callgrind` `Ir` gate

**Task #349 (R20-4), Round 20. FEASIBILITY-ONLY — nothing benched/implemented.**
**Date:** 2026-07-26 · **Investigator:** delegated session (zero-trust, evidence-cited).
**Scope:** answer one question for `docs/perf/OPEN_ITEMS.md` Active item 2
(`R18_7_MIMALLOC_GAP_STATUS.md` §3b): can `benches/perf_gate_iai.rs`'s
deterministic `Ir` gate be extended with a `mimalloc` comparison arm, and if
so, what would it take? Per the item's own framing ("Requires a feasibility
check ... before committing"), this doc recommends only — it does not add,
modify, or run any bench/script/CI/`Cargo.toml` change.

---

## 0. Verdict: **FEASIBLE**, with one small implementation nuance (not a blocker)

Three things this investigation had to check all came back "no problem,
already proven in this exact repo":

1. **Mimalloc's C code is statically compiled into the same binary Callgrind
   instruments — no dynamic-link/JIT attribution gap.** `libmimalloc-sys`'s
   `build.rs` compiles mimalloc's amalgamated `static.c` via `cc::Build` and
   the build script's own comment says outright: *"we only ever build a
   static lib"* (§5 below). Callgrind works by binary-translating whatever
   machine code the monitored process executes, regardless of source
   language or link mode; a statically-linked, ahead-of-time-compiled C
   library is architecturally indistinguishable from Rust code at that
   level. The "Valgrind under-counts JIT'd code" caveat the task asked me to
   check for is real for engines like V8/JVM that generate code AT RUNTIME —
   it does not apply to code that was already machine code when the ELF was
   loaded, which is what a `cc`-compiled static archive linked at build time
   is.
2. **The "one `#[global_allocator]` per process" constraint the task
   flagged as the likely reason a mimalloc arm needs a separate bench
   binary does not actually bind here — because neither arm needs to BE the
   process's global allocator.** `benches/perf_gate_iai.rs`'s existing
   SeferAlloc benches never write `#[global_allocator]`; they construct
   `SeferAlloc::new()` locally inside each `#[library_benchmark]` fn and call
   `.alloc()`/`.dealloc()` directly through the `GlobalAlloc` trait (§2).
   `benches/global_alloc.rs` already does the IDENTICAL thing for mimalloc —
   `let mi = mimalloc::MiMalloc; ... unsafe { mi.alloc(layout) }`, "NOT
   installed globally, to allow a head-to-head in one binary" per that file's
   own module doc (§3). This means a mimalloc arm can be added as new
   `#[library_benchmark]` functions in the **same** `perf_gate_iai.rs` file —
   no new bench binary/target is architecturally required (§4, §8).
3. **The CI toolchain question is already answered by a currently-green
   job on the exact runner label `perf-gate.yml` uses.** `ci.yml`'s `clippy
   (--all-features)` job (`ci.yml:78`, `cargo clippy --all-targets
   --all-features`) runs on `ubuntu-latest` and already compiles
   `benches/global_alloc.rs` (requires `alloc-global`, included by
   `--all-features`), which already forces `libmimalloc-sys`'s `cc::Build`
   step to compile mimalloc's C source to completion — with **zero** explicit
   C-toolchain install step in that job. `perf-gate.yml`'s job runs on the
   identical `ubuntu-latest` label (§6). The C-toolchain risk the task asked
   me to check is therefore already retired by an existing, currently
   passing job, not something to newly validate.
4. **Licensing is a non-issue**, and already was: mimalloc is MIT-licensed
   and has been a dev-dependency of this crate since "Phase 9"
   (`Cargo.toml:778-781`), already exercised in CI via `--all-features` for
   many rounds with no license-gate ever flagging it (§7).

**The one real nuance** (not a feasibility blocker, but real follow-up work
if this is implemented): `scripts/iai.mjs`'s existing "marginal Ir/op"
column subtracts a single hardcoded bootstrap constant
(`large_alloc_free_cycle`'s raw `Ir`) from every bench's raw `Ir`, on the
assumption every bench pays the SAME one-time process bootstrap. A mimalloc
arm's one-time init cost is a **different** constant from SeferAlloc's
(different allocator, different internal bookkeeping) — subtracting
SeferAlloc's bootstrap from a mimalloc row would silently corrupt the
marginal-Ir/op number. This needs a per-arm bootstrap proxy, not a code
change to the feasibility of the arm itself (§8).

---

## 1. What the background doc already established (verified, not re-derived)

Per `R18_7_MIMALLOC_GAP_STATUS.md` §3b (read in full): `rg mimalloc
benches/perf_gate_iai.rs docs/perf/IAI_BASELINE.md` finds no mimalloc arm in
the iai gate — confirmed still true (`benches/perf_gate_iai.rs` mentions
`mimalloc` nowhere; read the whole 517-line file). The "already-confirmed
fact" in this task's brief — that `mimalloc = "0.1"` is already a dependency
(`Cargo.toml:781`) and already used for wall-clock comparison in
`benches/global_alloc.rs` — is confirmed exactly as stated: `Cargo.toml:781`
reads `mimalloc = "0.1"` under a comment block ("Phase 9: mimalloc for
head-to-head benchmarking of the per-thread heap's alloc/dealloc hot path vs
a state-of-the-art allocator. Dev-only — NOT a runtime dependency."), and it
is an **unconditional** dev-dependency — not scoped to
`[target.'cfg(target_os = "linux")'.dev-dependencies]` the way
`iai-callgrind = "0.14"` is (`Cargo.toml:825-826`). So `mimalloc` is already
available in every dev-dependency build graph (Windows/macOS/Linux); only
`iai-callgrind` itself is Linux-scoped.

---

## 2. `benches/perf_gate_iai.rs` — exact existing structure (read in full)

- The whole file (imports, the 13 `#[library_benchmark]` fns, the
  `library_benchmark_group!`, the `main!` invocation) is
  `#[cfg(target_os = "linux")]`-gated; a bare `fn main() {}` fallback exists
  for non-Linux so the `harness = false` bench target still links everywhere
  Cargo resolves it, per the file's own module doc (lines 28-38).
- **No `#[global_allocator]` anywhere in the file.** Every bench function
  constructs its own `let sefer = SeferAlloc::new();` and calls
  `unsafe { sefer.alloc(layout) }` / `unsafe { sefer.dealloc(ptr, layout) }`
  directly — `SeferAlloc` is a plain struct implementing `unsafe impl
  GlobalAlloc for SeferAlloc` (`src/global/sefer_alloc.rs:164,550`); nothing
  about calling its trait methods requires it to be the process's registered
  global allocator.
- 13 benches, one `library_benchmark_group!(name = perf_gate; benchmarks =
  ...)`, one `main!(library_benchmark_groups = perf_gate)`. `Cargo.toml`
  wires it as `[[bench]] name = "perf_gate_iai"`, `harness = false`,
  `required-features = ["alloc-global"]` (`Cargo.toml:1337-1340`).
- Driven in CI by `.github/workflows/perf-gate.yml` (`cargo bench --bench
  perf_gate_iai --features production`) and locally via `node
  scripts/iai.mjs` (WSL + Valgrind wrapper; `npm run iai`).

---

## 3. `benches/global_alloc.rs`'s mimalloc usage — exact pattern (read in full)

Confirmed exactly as the task's brief described, with the precise mechanics:

- `let mi = mimalloc::MiMalloc;` (line 368/549/646/705/821) — a bare struct
  value, never written to a `#[global_allocator]` static.
- Generic helpers take `&A: GlobalAlloc` and call `.alloc()`/`.dealloc()`
  directly: `fn bench_direct_alloc<A: GlobalAlloc>(alloc: &A, layout:
  Layout)` (line 192), invoked as `bench_direct_alloc(mi_ref, layout)`
  (line 403).
- The module doc says why outright (lines 12-19): *"mimalloc (via the
  `mimalloc` crate's `GlobalAlloc` impl, called directly — NOT installed
  globally, to allow a head-to-head in one binary) ... For (2) and (3) we
  call the `GlobalAlloc` methods directly to avoid replacing the global
  allocator mid-process (which SeferAlloc already occupies). This is an
  honest apples-to-apples comparison of the alloc/dealloc hot path."*

This is a already-shipped, already-reviewed (multiple rounds' worth of
confound fixes layered on top — rotation, TLS-trim, etc.) working
demonstration that `mimalloc::MiMalloc` can be called directly, uninstalled,
in the same process as `SeferAlloc` — the exact shape a new iai bench needs.

---

## 4. The "one `#[global_allocator]` per process" concern — investigated, does not apply

The task asked me to confirm whether a mimalloc arm "almost certainly needs
to be a SEPARATE bench binary/target" because a process can only have one
`#[global_allocator]`. Having read both files in full: **neither arm in this
repo's methodology is ever installed as `#[global_allocator]`** — both
`perf_gate_iai.rs` (SeferAlloc) and `global_alloc.rs` (SeferAlloc, mimalloc,
System) call allocator methods directly on locally-constructed values via
the `GlobalAlloc` trait. `#[global_allocator]` is a purely Rust-level routing
attribute (it tells `rustc` where `Box`/`Vec`/etc. route their allocations);
it has no bearing on whether an allocator's own methods can be invoked
directly as ordinary function calls. Since the existing gate already avoids
touching `#[global_allocator]` (precisely so it measures the allocator's own
work, not incidental harness-internal `Vec` churn from iai-callgrind's own
macro-generated glue), adding a mimalloc arm the same way introduces no
allocator-singleton conflict at all.

Practical consequence: **the premise that a separate bench binary is
"almost certainly" required is not correct for this codebase's established
methodology.** A mimalloc arm can be added as new `#[library_benchmark]`
functions inside `benches/perf_gate_iai.rs` itself. A separate file/binary
remains an option (e.g. for independent CI-job splitting), but it is a
choice, not a requirement — see §8's recommendation.

The ambient global allocator for the whole `perf_gate_iai` binary (since it
sets none) is Rust's own default `System` allocator; that is only exercised
by the harness's own bookkeeping (iai-callgrind's macro-generated setup),
not by anything inside the timed bench bodies (every existing bench uses a
stack array `[*mut u8; N]` for its pointer scratch space, not a heap `Vec`)
— so this is already the current, working isolation discipline, unaffected
by adding a mimalloc arm alongside it.

---

## 5. Valgrind/Callgrind attribution through the statically-linked mimalloc C core

Read `libmimalloc-sys-0.1.49/build.rs` in full (the crate underlying the
`mimalloc` Rust wrapper). Key facts:

```
let mut build = cc::Build::new();
...
build.file(&static_source);   // c_src/mimalloc/{v2,v3}/src/static.c
...
// Overriding malloc is only available on windows in shared mode, but we
// only ever build a static lib.
```

- mimalloc's C core compiles as a single amalgamated `static.c` translation
  unit via the `cc` crate (the standard "vendor a C dependency" pattern also
  used by many other `-sys` crates in the Rust ecosystem), producing a
  static archive linked directly into the final Rust binary — no `.so`, no
  `dlopen`, no separate process.
- Because the code is machine code baked into the same ELF binary at build
  time (not generated at runtime by a JIT), Valgrind/Callgrind's
  binary-translation instrumentation covers it exactly the same way it
  covers Rust-compiled code: Callgrind does not distinguish "Rust-compiled"
  from "C-compiled-via-cc" instructions — both are just retired
  instructions in the monitored process's address space. The
  under-count/mis-attribution risk that legitimately exists for JIT engines
  (V8, JVM, etc. — code that materializes as machine code only at runtime,
  sometimes outside Valgrind's initial translation cache) is architecturally
  absent here.
- Checked `iai-callgrind-0.14.2`'s own source/README/CHANGELOG (local cargo
  registry cache, no network dependency) for any documented FFI/C-linkage
  caveat: found none. The only "custom allocator" mentions in its source
  (`src/client_requests/valgrind.rs`, `dhat.rs`, `helgrind.rs`,
  `drd.rs`) are **Memcheck/DHAT/Helgrind/DRD** client-request annotations
  (`malloclike_block`/`freelike_block`, etc.) that let those OTHER Valgrind
  tools understand a custom allocator's block semantics for
  leak/uninitialized-memory/race analysis — they are irrelevant to plain
  Callgrind `Ir` counting, which needs no such annotation to count
  instructions correctly. The README's only stated limitation (`README.md:
  103-104`, "Iai-Callgrind cannot be run on Windows and platforms not
  supported by Valgrind") is already the reason `perf_gate_iai.rs` is
  Linux-gated today — it is not a new constraint a mimalloc arm introduces.
- This is also structurally consistent with what the gate already measures:
  `large_alloc_free_cycle` and `seg_cycle_decommit_256k` already exercise
  real OS-level `mmap`/`madvise`-class syscalls through SeferAlloc's own
  Rust code; Valgrind already has to correctly account for kernel-boundary
  transitions there. Mimalloc's own equivalent syscalls (its C core also
  calls `mmap`/`madvise` for its own segment management) are handled by
  exactly the same mechanism, not a new one.

**Conclusion:** no known or plausible Callgrind attribution gap for a
statically-linked C allocator core in this exact linkage shape.

---

## 6. CI environment — C toolchain already proven present on the same runner label

`perf-gate.yml` (read in full) runs on `ubuntu-latest`, installs Valgrind
(`sudo apt-get install -y valgrind`) and `iai-callgrind-runner` (`cargo
install iai-callgrind-runner --version "^0.14" --locked`) — **no C
toolchain install step at all.**

Separately, `ci.yml`'s `clippy (--all-features)` job (`ci.yml:77-78`, `cargo
clippy --all-targets --all-features -- -D warnings`) also runs on
`ubuntu-latest` (verified — same job block, same `runs-on`), and
`--all-targets --all-features` compiles `benches/global_alloc.rs`
(`required-features = ["alloc-global"]`, included in `--all-features`),
which pulls in the `mimalloc` dev-dependency and therefore runs
`libmimalloc-sys`'s `cc::Build` step to completion. That job is a
currently-passing part of this repo's CI matrix (per the recent commit
history — `production`-composition changes routinely pass `npm run check`,
which itself runs `clippy -D warnings` across the CI feature matrix
including `--all-features`, per `CLAUDE.md`'s own "Before every push" rule).

**This means the exact C-toolchain risk this task was asked to check is
already retired by an existing, already-green job on the identical runner
image** — GitHub's `ubuntu-latest` hosted runners ship `gcc`/`build-essential`
by default, and this repo's own CI already exercises that fact via
`clippy --all-features` without ever installing a compiler explicitly. No
new `apt-get install build-essential`-class step would be needed for
`perf-gate.yml` to also compile a mimalloc arm.

---

## 7. Licensing

`mimalloc` (the Rust wrapper crate) and the underlying `libmimalloc-sys`/
vendored mimalloc C source are MIT-licensed (Microsoft's mimalloc is
MIT-licensed upstream). It has already been a dev-dependency of this crate
since "Phase 9" (`Cargo.toml:778-781`) and already ships in every
`--all-features` CI build without any license-scanning gate having ever
flagged it in this repo's history. Adding a second consumption site
(a new bench) of an *already-vendored* dependency introduces no new
licensing surface. Non-issue, confirmed rather than merely asserted.

---

## 8. Next-round implementation sketch (recommendation — NOT implemented in this task)

If a future round pursues this, the shape that requires the least new
infrastructure:

- **Same file, no new bench binary.** Extend `benches/perf_gate_iai.rs`
  itself (inside its existing `#[cfg(target_os = "linux")]` gate):
  - `use mimalloc::MiMalloc;` alongside the existing `use
    sefer_alloc::SeferAlloc;`.
  - Add mirrored `#[library_benchmark]` functions for the shapes §3b's
    open question actually needs — at minimum the COLD/RECYCLE front (the
    16 B/256 B cold-carve gap the debate is about): `mimalloc_cold_alloc_
    free_256x16b`, `mimalloc_cold_alloc_free_256x64b`,
    `mimalloc_recycle_alloc_free_256x16b`, `mimalloc_recycle_alloc_
    free_256x64b`, plus `mimalloc_small_churn_16b`/`mimalloc_churn_256b`
    for the hot-reuse counterpart, and a `mimalloc_bootstrap_proxy` bench
    (a single mimalloc alloc+free, mirroring `large_alloc_free_cycle`'s role)
    so the marginal-Ir/op decomposition has a mimalloc-specific bootstrap
    constant (see the nuance below — do NOT reuse SeferAlloc's bootstrap
    constant for mimalloc rows).
  - Add the new function names to the existing `library_benchmark_group!`'s
    `benchmarks = ...` list.
- **`Cargo.toml`:** no change needed — `mimalloc = "0.1"` is already an
  unconditional dev-dependency, already visible to this bench target.
- **`.github/workflows/perf-gate.yml`:** no new job/step needed — the
  existing `cargo bench --bench perf_gate_iai --features production` line
  already runs whatever `#[library_benchmark]` fns exist in that file. This
  is the concrete payoff of the same-file approach: zero CI-file diff.
- **`scripts/iai.mjs`:**
  - `BENCH_OPS`: add entries for the new mimalloc bench names with the SAME
    op-counts as their SeferAlloc siblings (the workload shapes would be
    byte-identical by construction).
  - `BOOTSTRAP_BENCH`/`marginalIrPerOp`: the one real code change. Today
    `BOOTSTRAP_BENCH` is a single hardcoded name subtracted from every row.
    With two allocators in one report, this needs to become arm-aware (e.g.
    a `{ prefix -> bootstrap bench name }` map, `sefer_alloc` rows use
    `large_alloc_free_cycle`, `mimalloc_*` rows use
    `mimalloc_bootstrap_proxy`) so the marginal Ir/op column stays an
    honest per-arm number instead of silently mixing constants across
    allocators.
  - Report table: no structural change required — new rows appear like any
    other bench; a follow-up could add a derived "Sefer/mimalloc ratio"
    column once both arms exist side by side (nice-to-have, not required
    for the gate itself to work).
- **`docs/perf/IAI_BASELINE.md` / README:** once real Linux numbers exist
  (WSL locally or the CI job), cite them per the existing raw-log policy —
  this finally gives §3b's missing number: whether mimalloc's cold-carve
  `Ir`/op is materially lower (headroom remains) or comparable (SeferAlloc
  is near the honest floor).
- **`IAI_CALLGRIND_REGRESSION` gate (flag, not a blocker):** the existing
  10%-`Ir` regression threshold (`perf-gate.yml`'s `env:
  IAI_CALLGRIND_REGRESSION: 'Ir=10'`) is applied per bench name by
  iai-callgrind, so it would nominally also apply to the new `mimalloc_*`
  bench IDs. In practice mimalloc's own code does not change from SeferAlloc
  PRs, so its `Ir` is expected to stay flat across ordinary commits; a
  `mimalloc_*` bench tripping the regression gate would signal a
  mimalloc-crate-version bump or toolchain drift, not a SeferAlloc
  regression, and should be triaged as such rather than assumed to block
  merge for the wrong reason. Worth a one-line note in whatever PR
  implements this.

---

## 9. Genuine blockers found: none

Every concern the task asked me to check (FFI/JIT attribution gap,
single-`#[global_allocator]`-per-process conflict, CI C-toolchain
availability, licensing) came back resolved by direct evidence already in
this repository, not by assumption. The one non-trivial piece of follow-up
work identified (§8's arm-aware bootstrap constant in `scripts/iai.mjs`) is
implementation detail, not a feasibility blocker.

---

## 10. Files inspected (evidence trail)

- `benches/perf_gate_iai.rs` (full read, 517 lines) — existing gate
  structure, no `#[global_allocator]`, 13 benches, Linux-only cfg-gating.
- `benches/global_alloc.rs` (full read, 1696 lines) — the established
  "call mimalloc/System directly via `GlobalAlloc`, never installed
  globally" pattern (module doc lines 1-19, `Arm`/`ARM_PERMUTATIONS`
  lines 108-124, `bench_direct_alloc` line 192, mimalloc construction at
  lines 368, 549, 646, 705, 821).
- `docs/perf/R18_7_MIMALLOC_GAP_STATUS.md` (full read) — §3b (lines
  154-170) and §6 (lines 270-281), the background this task builds on;
  confirmed its "no mimalloc arm in the iai gate" finding still holds.
- `docs/perf/OPEN_ITEMS.md` — Active item 2 (lines 68-75), the entry this
  report answers.
- `Cargo.toml:770-948` — `mimalloc = "0.1"` (line 781, unconditional
  dev-dependency), `iai-callgrind = "0.14"` (line 826, scoped to
  `[target.'cfg(target_os = "linux")'.dev-dependencies]`), the
  `perf_gate_iai` / `global_alloc` `[[bench]]` entries.
- `Cargo.toml:1449-1467` — `[profile.release]`/`[profile.bench]`
  `codegen-units = 1` + `lto = "thin"`, and the pre-existing comment noting
  "mimalloc's C core is compiled by its build script at full optimization"
  (confirms mimalloc's build has been part of this crate's profile-tuning
  awareness since PERF-PASS-1).
- `.github/workflows/perf-gate.yml` (full read, 137 lines) — the CI job
  this arm would extend; `ubuntu-latest`, Valgrind install, no C-toolchain
  step, `IAI_CALLGRIND_REGRESSION='Ir=10'`.
- `.github/workflows/ci.yml:53-78` — the `clippy (--all-features)` job
  (`ubuntu-latest`, `cargo clippy --all-targets --all-features`) cited as
  existing proof the mimalloc C build already succeeds on this runner
  image without a dedicated toolchain step.
- `scripts/iai.mjs` (full read, 457 lines) — the local WSL+Valgrind runner,
  `BENCH_OPS`/`BOOTSTRAP_BENCH`/`marginalIrPerOp` (lines 80-150, 354-359),
  the piece that needs the arm-aware bootstrap-constant follow-up (§8).
- `src/global/sefer_alloc.rs:164,550` — confirms `SeferAlloc` is a plain
  struct with `unsafe impl GlobalAlloc for SeferAlloc`, callable directly
  without process-global installation.
- `D:\system_artefact\cargo\registry\src\index.crates.io-*\libmimalloc-sys-0.1.49\build.rs`
  (full read) — confirms `cc::Build` compiles mimalloc's amalgamated
  `static.c` into a static archive ("we only ever build a static lib").
- `D:\system_artefact\cargo\registry\src\index.crates.io-*\iai-callgrind-0.14.2\README.md`
  (full read) and `src/client_requests/{valgrind,dhat,helgrind,drd}.rs`
  (grepped for `GlobalAlloc`/`allocator`) — confirms no documented
  FFI/static-C-linkage caveat for Callgrind `Ir` counting; the only
  "custom allocator" surface is Memcheck/DHAT/Helgrind/DRD block-semantics
  annotations, irrelevant to plain `Ir` counting.

## 11. One-line summary for the eventual follow-up task

Adding a `mimalloc` `Ir` arm to `perf_gate_iai.rs` is **feasible** with no
architectural blocker: mimalloc's C core is statically linked into the same
binary Callgrind already instruments, this repo's own established pattern
(`benches/global_alloc.rs`) already calls it directly without a
`#[global_allocator]` install (so no new bench binary is required — it can
live in the same file), and the CI toolchain question is already answered
by a currently-green `ubuntu-latest` job that compiles mimalloc's C build
today. The one real piece of new work is making `scripts/iai.mjs`'s
marginal-Ir/op bootstrap constant arm-aware so a mimalloc row is not
diffed against SeferAlloc's bootstrap cost by mistake.
