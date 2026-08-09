# `aligned-vmem` round-closing review (read-only, end-to-end)

**Date:** 2026-08-09
**Reviewed range:** `6b18834^..97bc726` (the range the task named) **plus** `56d0764`, which the
task listed as part of the round but which is **outside** that range — verified:
`git merge-base --is-ancestor 56d0764 6b18834^` succeeds, and
`git log --oneline 6b18834^..97bc726 | grep -c 56d0764` → `0`. The loom-hoist commit landed at
08:39, seven minutes before `6b18834` (08:46). It was reviewed separately here.
**Scope:** the 11-commit `aligned-vmem` `rust-intel` remediation round — `6b18834` (#699),
`54089fa` (#712), `131355a` + `d6b72b1` (#713), `2e7f4f5` (#714), `e5f6700` (#715), `81ecfe3`
(#716), `94aef18` (#717), `b8b70fb` (#718), `55e71b0` (#719), the CHANGELOG/checkpoint commit
`97bc726` (#746), and the out-of-range `56d0764` (#711).
**Mode:** read-only. No repository file was modified except this report. `git status --porcelain`
was empty before and after every probe. Two throwaway probes were built and run outside the
repository (a standalone `rustc` program in `%TEMP%` modelling `should_fail_commit`, and a
workspace-detached scratch copy of `crates/vmem` for counterfactual reversions), both deleted;
one temporary `crates/vmem/examples/mockerr.rs` probe was created, run, and removed inside the
same shell invocation (`git status --porcelain` confirmed clean immediately after). Verbatim
probe output is inlined below.

**Bottom line:** the round's **shipped source changes are correct**. Every one of the four
counterfactuals that can be mechanically constructed reproduces exactly as claimed, independently
re-run here rather than trusted (§2). #712's contract-violation split, #713's immediate-capture
rewrite, #714's per-OS `_SC_PAGESIZE` table, #717's `.addr()`/`.with_addr()` derivation, #718's
`fetch_update` + `Release`/`Acquire` pair, and #719's `from_raw_parts` assert are all individually
sound, and I found **no memory-safety defect and no new `unsafe` surface** anywhere in the round.

The round's **evidence and reporting layer is where the real problems are**, and there is one
finding severe enough to name up front. **F1 (HIGH): #718's headline regression test is
structurally incapable of failing against the pre-fix racy code — not "unlikely to on real
hardware", which is what the test's own honesty note, the commit message, the CHANGELOG and the
checkpoint all say.** Because the test arms exactly as many failures as it makes calls, the
observed failure count is invariant under the bug. I proved this two ways: with the race window
artificially widened, the pre-fix implementation lost 294 decrements and the shipped assertion
still passed; and changing one constant (arm half the calls) makes the pre-fix implementation fail
the assertion **5 runs out of 5 on this machine with no artificial delay at all** — directly
refuting the shipped claim that "no amount of thread/round count fixes this on real hardware."
The round therefore published a wrong root-cause diagnosis in four places, and that wrong diagnosis
is what foreclosed the one-line fix that works.

Four further findings are MEDIUM: an incomplete #713 (F2, the `mock` backend still mints a
fabricated `code 0` for a purely simulated failure — reproduced), an undocumented public-contract
narrowing plus a real observable behaviour change on Linux from #714 (F3), and two CHANGELOG
accuracy defects (F4 blanket-counterfactual overclaim, F5 a fabricated rationale for #711).

---

## 0. Current-state green check (re-run personally, not trusted from commit messages)

| Command | Result |
| --- | --- |
| `cargo test -p aligned-vmem --all-features` | **32 passed**, 0 failed — but see the split below; `fault_injection.rs` runs **0 tests** here (by design, `#![cfg(not(feature = "mock"))]`) |
| ↳ per-binary under `--all-features` | lib 0 · `fault_injection` **0** · `huge_pages` 1 · `lazy_commit` 10 · `mock` 6 · `smoke` 15 |
| `cargo test -p aligned-vmem --features "fault-injection lazy-commit" --no-fail-fast` (the step #699 added) | **30 passed**, 0 failed — `fault_injection` **5**, `huge_pages` 0, `lazy_commit` 10, `mock` 0, `smoke` 15 |
| `cargo test -p aligned-vmem --no-fail-fast` (CI's default-features step) | **15 passed** (`smoke` only) |
| `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` | clean |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` (default features) | clean |
| `cargo fmt -p aligned-vmem -- --check` | clean (exit 0) |
| `cargo check -p aligned-vmem` / `--features bench-internals` / `--features fault-injection` (no `lazy-commit`) | all clean — the module doc's dead-code carve-out for `fault-injection`-without-`lazy-commit` holds |
| `cargo doc -p aligned-vmem --no-deps --features "lazy-commit huge-pages fault-injection"` (the **exact docs.rs feature set**) | **5 warnings** — all introduced by `b8b70fb` (#718). See **F6** |
| **0 doctests** in every configuration | CLAUDE.md's no-doctest rule holds |

The numeric claims in the commit messages check out against the current tree with **one exception**
(ci.yml's "4 passed", now 5 — **F10**).

---

## 1. Commit-by-commit: does each diff match its own message?

All eleven commits were read line by line (`git show`) against their messages. **Eleven of eleven
diffs do what their messages say.** Specific spot-verifications that carried real risk of
divergence:

- **`56d0764` (#711)** genuinely adds `[workspace.dependencies] loom = "0.7"` and converts all
  three consumers (root `[target.'cfg(loom)'.dev-dependencies]`, `racy-ptr-cell`,
  `tagged-index-stack`) to `loom = { workspace = true }`. The commit message is accurate. The
  **CHANGELOG's retelling of it is not** — **F5**.
- **`6b18834` (#699)** adds exactly the one `cargo test -p aligned-vmem --features
  "fault-injection lazy-commit"` step (`ci.yml:765`) and rewrites the misleading comment above it.
  Independently confirmed both halves: `--all-features` really does compile `fault_injection.rs` to
  0 tests, and the new step really does run 5 (§0). Comment count is stale — **F10**.
- **`54089fa` (#712)** genuinely splits `start == end` (→ `Ok(())`) from `start > end` /
  misaligned (→ `Err(invalid_argument())`) in **both** `try_recommit` (`lib.rs:670-675`) and
  `try_commit_range` (`:748-753`), and genuinely deletes the two test assertions that pinned the
  old permissive clamp. Counterfactual reproduced in both files — §2.1.
- **`131355a` (#713)** converts every raw helper on all **three** platform blocks (Windows, Unix,
  **and miri** — the CHANGELOG says only "Windows and Unix") to `Result<_, VmemError>`, and every
  `last_os_error()` call site is now adjacent to its own failing syscall with no intervening FFI.
  I traced each of the five capture points (`lib.rs:1005`, `:1063`, `:1069`, `:1329`, `:1333`,
  `:1409`) and each is genuinely immediate. The two internal fit-computation paths (`:1031`,
  `:1356`) correctly return `invalid_argument()` and correctly do **not** read errno. The
  `Option<u32>` representation change is real (`error.rs:32`). Counterfactual reproduced — §2.2.
  Incomplete for the `mock` backend — **F2**.
- **`d6b72b1` (#713 side-discovery)** is a docs-only addition of `CORRECTNESS_OPEN_ITEMS.md` item
  41, correctly formatted with the Status/Evidence/Next-trigger current-state card CLAUDE.md
  requires. Went stale two commits later — **F11**.
- **`2e7f4f5` (#714)** genuinely lands the per-OS `_SC_PAGESIZE` table (`lib.rs:1596-1629`), the
  `queried >= PAGE` tightening (`:247`), the `LINUX_HUGE_PAGE_SIZE` constant (`:1555`) and the
  hugetlb guard (`:1303-1309`), and creates `tests/huge_pages.rs`. I checked the pre-image: the old
  arm was `any(macos, ios)` → 29, everything else → 30, so the fix also silently corrects
  **`tvos`/`watchos`** (both Darwin, both previously getting the Linux 30, both in this crate's own
  `MAP_ANON` cfg list) — six wrong targets, not the four the message and CHANGELOG claim. That is
  an *under*-claim, noted for completeness. The guard's user-facing consequences are **F3**.
- **`e5f6700` (#715)** adds `#[non_exhaustive]` to all 8 struct-like `Call` variants and the
  unification-hazard doc in three places (`Cargo.toml:60-81`, `mock.rs:25-38`, `README.md:57-61`).
  The `..` addition in `tests/mock.rs:29` confirms the marker is load-bearing, exactly as claimed.
- **`81ecfe3` (#716)** adds the `#[cfg(not(miri))]` gate (`tests/lazy_commit.rs:287-295`) and the
  `fail_next_reserve_injects_through_huge_path` test. Counterfactual reproduced — §2.3.
- **`94aef18` (#717)** replaces the two `as usize` → `as *mut u8` round-trips with
  `.addr()`/`.with_addr()` in `win_reserve_commit` (`lib.rs:1016`, `:1037`) and `unix_reserve`
  (`:1342`, `:1362`, `:1372`), and switches the Windows `VirtualAlloc` call sites to
  `base.as_ptr().cast()`. Provenance soundness independently checked — §2.5. One sibling site left
  behind — **F8**.
- **`b8b70fb` (#718)** genuinely lands `Release` on `arm_fail_at`'s target store
  (`fault_injection.rs:93`), `Acquire` on `should_fail_commit`'s target load (`:124`), and
  `fetch_update` with a lazy `.then(|| next - 1)` closure (`:110-118`). Both fixes are correct
  (§2.4). Its test is **F1**; its new doc links are **F6**.
- **`55e71b0` (#719)** lands all 7 items. Counterfactual for item 4 reproduced — §2.6. Item 4's
  fix is partial — **F7**.
- **`97bc726` (#746)** is docs-only (CHANGELOG + two checkpoints). Its content has four accuracy
  problems — **F1**, **F3**, **F4**, **F5**.

**Out-of-scope edits, TODO/placeholder code, half-wired features: none found.**
`git diff --name-only 6b18834^..97bc726` touches exactly 11 crate files plus `ci.yml`,
`CHANGELOG.md`, `docs/CORRECTNESS_OPEN_ITEMS.md` and two checkpoints — every one of which the round
explicitly called for. `crates/vmem/Cargo.toml`'s entire +23-line diff is comment lines (verified:
`git diff ... -- crates/vmem/Cargo.toml | grep '^+' | grep -v '^+#'` returns only the `+++` header).
**No version bump. No new dependency. `Cargo.lock` untouched by the whole range.**
`grep -rn "TODO\|FIXME\|XXX\|unimplemented!\|todo!\|HACK\|dbg!" crates/vmem/` returns nothing.

---

## 2. Were the fixes' counterfactuals real? (zero-trust re-verification)

Run in a workspace-detached copy of `crates/vmem` under `%TEMP%` (a `[workspace]` stanza appended
to its `Cargo.toml`), deleted after use. Baseline before any reversion: 5 + 1 + 10 + 15 = 31 tests
green.

### 2.1 `#712` — contract violation vs. success sentinel — **CONFIRMED, both files**

Reverted both `try_recommit` and `try_commit_range` to the original single
`if start >= end || misaligned { return Ok(()); }` clamp (2 textual occurrences, both replaced):

```text
thread 'commit_range_rejects_contract_violating_offsets' panicked at tests\lazy_commit.rs:132:9:
test result: FAILED. 9 passed; 1 failed

thread 'recommit_rejects_contract_violating_offsets' panicked at tests\smoke.rs:142:9:
test result: FAILED. 14 passed; 1 failed
```

Both new tests die, at the misaligned-start assertion, for the right reason.

### 2.2 `#713` — `VmemError` three-way classification — **CONFIRMED**

Changed `os_refusal_unknown_code()` to store `Some(0)` (reintroducing the pre-fix ambiguity):

```text
thread 'vmem_error_kinds_are_distinguishable' panicked at tests\smoke.rs:325:5:
assertion `left == right` failed: an OS refusal with no known code must report os_code() == None,
DISTINCT from invalid_argument() (also None) via is_invalid_argument()
  left: Some(0)
 right: None
test result: FAILED. 14 passed; 1 failed
```

### 2.3 `#716` — mock records `ReserveHuge` — **CONFIRMED**

Changed `try_reserve_aligned_huge`'s two `mock::record` sites to `Call::Reserve`:

```text
must record Call::ReserveHuge, not Call::Reserve: Reserve { size: 2097152, align: 2097152 }
test result: FAILED. 5 passed; 1 failed
```

### 2.4 `#718` — the ordering fixes are right; the test is not a regression test — **see F1**

Both *fixes* are correct on reading, and I checked each against the specific hazard it names:

- **`Release`/`Acquire` pairing.** `arm_fail_at` (`fault_injection.rs:92-94`) writes
  `FAIL_AT_COUNTER = 0` (`Relaxed`, the payload) then `FAIL_AT_TARGET = k` (`Release`, the flag);
  `should_fail_commit` (`:124-126`) reads `FAIL_AT_TARGET` (`Acquire`) and only then touches
  `FAIL_AT_COUNTER`. That is a correctly-paired payload/flag publish, and the gating order in the
  reader really does make `FAIL_AT_COUNTER` a payload rather than an independently-read variable.
  Correct. `arm_fail_next`'s remaining `Relaxed` store (`:73`) is fine — `FAIL_NEXT` publishes no
  payload.
- **`fetch_update`'s closure.** `(next > 0).then(|| next - 1)` is pure, total, side-effect-free and
  idempotent — exactly what `fetch_update`'s retry-loop contract requires, since the closure may be
  invoked repeatedly on CAS failure. The `then` vs `then_some` distinction the commit calls out is
  real and correctly reasoned (`then_some`'s argument is evaluated eagerly, so `next - 1` would
  underflow-panic at `next == 0`). `.is_ok()` correctly means "fired". Correct.

The **test**, however, is a different matter — **F1**.

### 2.5 `#717` — provenance soundness, checked rather than assumed — **SOUND**

Both derivations are genuinely within the originating allocation's bounds:

- **Windows** (`lib.rs:1016-1037`): `region_ptr` comes from `VirtualAlloc(NULL, over, MEM_RESERVE,
  …)`, so its provenance covers `[region_addr, region_addr + over)`. `base_addr` is
  `align_up_addr(region_addr, align)` and reaches `with_addr` **only** through the `fits` closure,
  which requires `base_addr + size <= region_addr + over` with both additions `checked_add`. So
  `base_addr ∈ [region_addr, region_addr + over)` and `region_ptr.with_addr(base_addr)` is a valid
  derived pointer. The two `VirtualAlloc(MEM_COMMIT)` calls now go through `base.as_ptr().cast()`,
  so the committed sub-range also carries real provenance.
- **Unix** (`lib.rs:1342-1372`): identical structure; `region_ptr` is the `mmap` return covering
  `over` bytes, `base_addr` and `tail_start` both come out of the same checked `fits` closure, and
  `tail_start <= region_end` is proven there. `region_ptr.with_addr(tail_start)` for the tail
  `munmap` is in-bounds. Sound.
- **No behaviour change**, as claimed: `with_addr` lowers identically to the old cast on every
  supported target.

One sibling site was not converted — **F8**.

### 2.6 `#719` item 4 — `from_raw_parts`'s `assert!` — **CONFIRMED, and complete for `align`**

Deleted the `assert!` (`lib.rs:453-456`):

```text
note: test did not panic as expected at tests\smoke.rs:252:4
test result: FAILED. 14 passed; 1 failed
```

**Are there other internal construction paths that could still reach `Drop` with an invalid
`(align, reservation_len)` pair?** I enumerated every `Reservation { … }` literal in the crate.
There are exactly four construction sites: `try_reserve_aligned` (`:523`),
`try_reserve_aligned_lazy` (`:853`), `try_reserve_aligned_huge` (`:901`), and `from_raw_parts`
(`:457`). All three public entry points validate `align.is_power_of_two() && align >= PAGE` and
`size` non-zero page-multiple *before* calling their raw helper, and the `reservation_len` they
store is either `size` (miri, Unix) or `size + align` (Windows) — always a valid `Layout` pairing.
So for the `align` half, **the assert closes the only unvalidated path.** For the
`reservation_len` half, it does not — **F7**.

---

## 3. Findings

### F1 — HIGH — `#718`'s regression test cannot fail against the pre-fix racy code *by construction*, and the shipped explanation for why blames the wrong cause — foreclosing the one-line change that does work

**`crates/vmem/tests/fault_injection.rs:184-254`**, specifically the oracle at `:218-222` and
`:249-253`:

```rust
const THREADS: usize = 32;
const ROUNDS:  u32   = 200;
const TOTAL:   u32   = THREADS as u32 * ROUNDS;   // 6400
arm_fail_next(TOTAL);                              // armed == number of calls
…
assert_eq!(failures, TOTAL, …);
```

**The invariance argument.** A call fires iff it observed `FAIL_NEXT > 0`. Under the *fixed*
(`fetch_update`) implementation the counter decrements exactly once per fire, so exactly `TOTAL`
of the `TOTAL` calls fire. Under the *racy* (load-then-store) implementation a torn store can
**lose** a decrement, and can even *raise* the counter (thread A reads 5; B reads 5 and stores 4;
C reads 4 and stores 3; A stores 4) — so the counter is pointwise **≥** its correct trajectory at
every instant. It follows that every call that fires under the fix also fires under the bug, and
none fails to. With `armed == calls`, `failures == TOTAL` holds under **both** implementations.
No scheduler, no model checker, and no thread/round count can change that; the assertion is
one-sided in the direction the bug never moves.

**Empirically confirmed, twice.** A standalone `rustc -O` program modelling both implementations
under the same 32-thread / 200-round `Barrier` design, run outside the repository:

```text
-- shipped oracle shape: armed == calls --
racy (pre-#718, forced window)  armed=6400 calls=6400 failures=6400 residual_counter=294 shipped_oracle=PASS
fixed (fetch_update)            armed=6400 calls=6400 failures=6400 residual_counter=0   shipped_oracle=PASS
-- alternative oracle: armed == calls/2 --
racy (pre-#718, forced window)  armed=3200 calls=6400 failures=3318 residual_counter=0   shipped_oracle=FAIL
fixed (fetch_update)            armed=3200 calls=6400 failures=3200 residual_counter=0   shipped_oracle=PASS
```

The first row is the decisive one: the racy implementation **lost 294 decrements** — the residual
counter proves the race fired hundreds of times — and the shipped assertion still passed.

**And the stated reason is empirically false.** The test's own doc (`:164-176`), `b8b70fb`'s
message, `CHANGELOG.md:243` and `docs/checkpoints/aligned-vmem-round-complete.md:32-42` all say
the same thing: *"No amount of thread/round count fixes this on real hardware without either a
model checker (`loom` …) or injecting an artificial delay into the very code path under test."*
Removing the artificial delay entirely and arming half the calls — otherwise the identical
32-thread/200-round barrier design — reproduces the bug **5 runs out of 5** on this machine:

```text
racy   armed=3200 calls=6400 failures=3222 shipped_oracle=FAIL
fixed  armed=3200 calls=6400 failures=3200 shipped_oracle=PASS
racy   armed=3200 calls=6400 failures=3221 shipped_oracle=FAIL
racy   armed=3200 calls=6400 failures=3215 shipped_oracle=FAIL
racy   armed=3200 calls=6400 failures=3223 shipped_oracle=FAIL
racy   armed=3200 calls=6400 failures=3220 shipped_oracle=FAIL
```

**Consequences, stated precisely.** The *fix* is correct and I am not disputing it — `fetch_update`
is atomic by construction, as the commit says. What is wrong is:

1. The round's most prominent "verification honesty" note gives a **wrong root cause** for a real
   observation. The "10 runs, zero failures against genuinely racy code" evidence is not evidence
   of a narrow timing window; it is a tautology of the oracle's shape, and would have read exactly
   the same on a 1000-core machine or under `loom`.
2. That wrong diagnosis is *load-bearing*: it is what justified shipping a non-regression test and
   declaring the matter closed. Had the oracle's one-sidedness been named instead, the fix (change
   `arm_fail_next(TOTAL)` to `arm_fail_next(TOTAL / 2)` and assert `failures == TOTAL / 2`) is one
   constant, needs no `loom`, needs no artificial delay, and is demonstrably effective on this
   exact hardware.
3. The claim propagated verbatim into four artifacts, one of which (`CHANGELOG.md`) is the
   project's published record.

This is the same failure class the `tagged-index-stack` closing review's F1 and the R30-8 rule in
CLAUDE.md both target: a test whose *stated* limitation is honest but whose *stated mechanism* is
wrong, so nobody re-examines whether the limitation was necessary.

**Suggested close (for a follow-up task, not applied here):** change `arm_fail_next(TOTAL)` →
`arm_fail_next(TOTAL / 2)` and `assert_eq!(failures, TOTAL / 2, …)`; rewrite the doc note to say
the oracle is now two-sided and *why* the previous shape could not fail (the ≥-monotonicity
argument above), replacing the thread-jitter explanation.

### F2 — MEDIUM — `#713`'s "a simulated failure must not mint a fabricated OS code" fix skipped the `mock` backend, which still reports the exact `code 0` ambiguity `error.rs`'s own new doc says is closed

**`crates/vmem/src/mock.rs:183`** (`take_reserve_fault`) and **`:196`** (`take_commit_fault`), both
`Some(VmemError::last_os_error())`.

`131355a`'s message says it *"Also fixed the one non-reservation site sharing the same defect
class"* — `try_commit_range`'s `fault-injection` branch, correctly changed to
`os_refusal_unknown_code()` with the reasoning at `lib.rs:773-779`: *"this is a SIMULATED failure —
no real syscall ran, so `VmemError::last_os_error()` would read whatever `errno`/`GetLastError`
happens to be lying around from unrelated prior code."* That reasoning applies **verbatim** to
`mock`'s two takers, which are reached from `try_reserve_aligned` (`:513`),
`try_reserve_aligned_lazy` (`:825`), `try_reserve_aligned_huge` (`:892`), `try_recommit` (`:683`)
and `try_commit_range` (`:761`). There are three sites in this class, not one.

**Reproduced, not inferred.** A throwaway example (`crates/vmem/examples/mockerr.rs`, created, run
and removed in one shell invocation; `git status --porcelain` empty immediately after):

```text
mock simulated reserve fault -> VmemError { os_code: Some(0) } | os_code=Some(0)
    | is_invalid_argument=false | Display=OS virtual-memory error (code 0)
```

That is precisely the outcome `error.rs:22-27`'s round-added doc advertises as eliminated: *"an
earlier version of this type stored the raw code as a bare `u32` defaulting to `0` when
unavailable, making 'no OS code available' indistinguishable from a genuine `code 0` /
`ERROR_SUCCESS` … Storing `Option<u32>` closes that gap at the type level."* The type-level gap is
closed; the `mock` backend re-opens it behaviourally on every simulated fault.

Rated MEDIUM rather than higher because `mock` is a test-only, off-by-default feature and no
in-repo consumer inspects `os_code()` (I grepped `src/` and `crates/numa/src/`: no
`os_code()`/`is_invalid_argument()` consumer exists). But a `mock` user writing exactly the
OOM-path test this feature exists for will see a fabricated `ERROR_SUCCESS`.

**Suggested close:** `Some(VmemError::os_refusal_unknown_code())` at both sites, mirroring
`lib.rs:779` verbatim; check `tests/mock.rs` for any assertion on the error's code first (there is
none today — all five mock tests assert on `is_none()`/`is_some()`/call-log shape).

### F3 — MEDIUM — `#714`'s hugetlb guard narrows `reserve_aligned_huge`'s public contract on Linux with **zero** user-facing documentation, and changes observable behaviour on a previously-valid success path

**`crates/vmem/src/lib.rs:1303-1309`** (the guard) versus **`:866-908`** (the two public entry
points' rustdoc) and **`README.md:54`**.

The guard rejects any Linux `huge` request whose `size` **or** `align` is not a multiple of 2 MiB.
`try_reserve_aligned_huge`'s own validator (`:888`) still checks only
`align.is_power_of_two() && align >= PAGE && size % PAGE == 0`, so the stricter requirement is
enforced two layers down, invisibly. Meanwhile the public doc still says, unchanged by this round:

- `:878` — *"Base/align/size contract is identical to [`reserve_aligned`]."* **False on Linux
  with `huge-pages` enabled.**
- `:873-875` — *"The request is **best-effort**: if the OS refuses large pages (none configured, no
  privilege), the reservation transparently falls back to ordinary pages, so this never fails purely
  because huge pages are unavailable."* The trailing hedge *"it fails only on … a contract
  violation"* refers to the contract the previous sentence just declared identical to
  `reserve_aligned`'s — so the doc is self-referentially wrong, not merely incomplete.
- `README.md:54` — *"`huge-pages` (`reserve_aligned_huge` — `MAP_HUGETLB` / `MEM_LARGE_PAGES`,
  best-effort with fallback)"*, no mention of a 2 MiB requirement.

**The behaviour change is real, not theoretical.** Take `reserve_aligned_huge(64 * 1024, 64 * 1024)`
on Linux. Pre-`2e7f4f5`: `try_reserve_aligned_exact` attempts `MAP_HUGETLB`; on the overwhelmingly
common host with no hugepages configured that fails, `unix_reserve` falls through to the
over-reserve path, its `huge` mmap also fails, and the `if huge { libc_mmap(over, false) }` retry
(`:1325`) succeeds — the caller gets a working ordinary-page reservation, exactly as documented.
Post-`2e7f4f5`: `Err(VmemError::invalid_argument())` / `None`, before any syscall. The leak the
guard prevents only ever occurred when the hugetlb mmap **succeeded**; the guard also rejects every
case where it would have failed and fallen back correctly. The rejection is defensible (it is what
makes the trim provably conformant, as the commit argues), but it is an over-rejection and it is
undocumented.

This also contradicts the round's own header claim at **`CHANGELOG.md:234`**: *"no shipping
algorithm or production default changed observable behavior on any valid-input success path."*
A 64 KiB huge request was valid input under the shipped contract and was a success path.

I could not execute this — no Linux host in this session — so it is read from the code, exactly the
same evidentiary basis `2e7f4f5` itself declares ("REASONED-FROM-SPEC, NOT empirically verified").
The control-flow reading is unambiguous, though: the guard is the first statement in `unix_reserve`
and unconditionally returns.

**Suggested close:** add the Linux-only requirement to `reserve_aligned_huge`'s and
`try_reserve_aligned_huge`'s rustdoc and to `README.md:54`; correct the "identical to
`reserve_aligned`" sentence; and either soften `CHANGELOG.md:234`'s "any valid-input success path"
or carve out #714 explicitly.

### F4 — MEDIUM — "each verified via a genuine zero-trust counterfactual" is contradicted by the round's own commit messages for at least four of the ten tasks

**`CHANGELOG.md:234`**: *"All 10 tasks (`#699`, `#711` above, `#712`-`#719`) landed as individual
commits, **each verified via a genuine zero-trust counterfactual** before commit (temporarily
reverting the fix, confirming the associated test fails for the right reason, then restoring it —
confirmed via `git diff` showing zero net change)."* Repeated at
**`docs/checkpoints/aligned-vmem-round-complete.md:18-23`**.

Four of the ten have no such counterfactual, and three of them say so themselves:

| Task | Why there is no counterfactual |
| --- | --- |
| `#711` | A Cargo manifest hoist. Nothing to revert-and-fail; its own commit cites re-runs, not a counterfactual. |
| `#714` | Its four hugetlb regression tests are `#[cfg(target_os = "linux")]`; `2e7f4f5`'s own message states they were "compile-checked clean on the Linux cross-target, **NOT executed anywhere in this session**". A test that has never run cannot have been counterfactually failed. |
| `#717` | `94aef18`'s own message: *"no counterfactual test exists that would fail before this fix and pass after"* — stated plainly, and correctly. |
| `#718` | Its own test doc and commit message say the test *"does NOT reliably fail against the pre-fix racy implementation"*. Per **F1** it cannot fail at all. |

Five of the remaining six do have real counterfactuals and all five that I could construct
reproduce (§2). The individual commit messages are each honest about what they actually did; it is
only the CHANGELOG's and checkpoint's blanket generalisation that overstates. Same class as the
`racy-ptr-cell` closing review's F13, but asserted more strongly here ("each", plus a specific
`git diff` verification protocol that could not have been performed for #711/#717).

### F5 — MEDIUM — the CHANGELOG's `#711` bullet states a reason for the change that is factually false: `aligned-vmem` has no `loom` dependency and never gained one

**`CHANGELOG.md:230`**, two claims:

1. *"`loom = "0.7"` was pinned independently in three manifests (`crates/racy-ptr-cell/Cargo.toml`,
   `crates/tagged-index-stack/Cargo.toml`, **and the workspace root once `aligned-vmem` needed it
   too**)"*
2. *"**Unblocks the aligned-vmem round below, which needed `loom` for a new dev-dependency** without
   re-pinning a fourth independent copy."*

Both are wrong, and `56d0764`'s own commit message contradicts them: the third manifest was *"root
Cargo.toml's `[target.'cfg(loom)'.dev-dependencies]`"*, which has existed independently for many
rounds and has nothing to do with `aligned-vmem`. And `aligned-vmem` never acquired a `loom`
dependency:

```text
$ grep -rn "loom" crates/vmem/
crates/vmem/Cargo.toml:74:# feature, matching this repo's own `cfg(loom)`/`cfg(kani)` precedent — cfg
crates/vmem/tests/fault_injection.rs:173:/// hardware without either a model checker (`loom`, not currently wired into
```

Two prose mentions, one of which explicitly says loom is *"not currently wired into
`aligned-vmem`"*. `crates/vmem/Cargo.toml`'s `[dependencies]` section (`:109`) is the last line of
the file and is empty; there is no `[dev-dependencies]` at all; `Cargo.lock` is untouched by the
whole round. `#711` was a genuine, well-motivated drift fix — it simply had no causal connection to
this round beyond sequencing, and inventing one puts a fabricated dependency relationship into the
project's published changelog.

**Suggested close:** replace both clauses with `56d0764`'s own accurate wording (three manifests =
root's `[target.'cfg(loom)'.dev-dependencies]` + the two loom-consuming crates; the round ordering
was a TaskList `blockedBy` sequencing choice, not a technical unblock).

### F6 — LOW-MEDIUM — `#718` introduced 5 rustdoc warnings in exactly the feature set docs.rs builds, so the published page will carry 5 unresolved links

`[package.metadata.docs.rs] features = ["lazy-commit", "huge-pages", "fault-injection"]`
(`crates/vmem/Cargo.toml:27`). Building precisely that:

```text
warning: public documentation for `fault_injection` links to private item `should_fail_commit`
  --> crates\vmem\src\fault_injection.rs:41:7
warning: public documentation for `fault_injection` links to private item `FAIL_NEXT`
  --> crates\vmem\src\fault_injection.rs:41:68
warning: public documentation for `arm_fail_at` links to private item `should_fail_commit`
  --> crates\vmem\src\fault_injection.rs:85:33
warning: public documentation for `arm_fail_at` links to private item `FAIL_AT_COUNTER`
  --> crates\vmem\src\fault_injection.rs:86:7
warning: public documentation for `arm_fail_at` links to private item `FAIL_AT_TARGET`
  --> crates\vmem\src\fault_injection.rs:86:48
warning: `aligned-vmem` (lib doc) generated 5 warnings
```

All five sit on lines `b8b70fb` added (the module doc's new `Release`/`Acquire` explanation and
`arm_fail_at`'s new payload/flag paragraph); the pre-`#718` file had one such link and it was on a
*private* item, which rustdoc does not warn about. `#715`'s verification matrix did include
`cargo doc --no-deps (0 warnings)` — accurate at that commit — but `#718` landed three commits
later and its own matrix (test / clippy / fmt / cross-check / miri) does not include `cargo doc`.
On docs.rs the five links render as inert code spans and the build log shows the warnings; the
crate's first-publish task (#658) is still open, so this is cheap to catch now.

**Suggested close:** demote the five private-item references from intra-doc links to plain code
spans (``` `FAIL_AT_TARGET` ```), or move the explanation onto the private items themselves. Add
`cargo doc -p aligned-vmem --no-deps --features "lazy-commit huge-pages fault-injection"` to this
crate's pre-publish check list.

### F7 — LOW-MEDIUM — `#719` item 4's `assert!` closes only one of the two inputs its own stated hazard depends on

**`crates/vmem/src/lib.rs:453-456`** validates `align` only. The hazard it names is the
`Layout::from_size_align(reservation_len, align).expect("release: invalid layout")` at **`:1766`**,
inside the miri backend's `release_reservation`, reachable from `Drop::drop` (`:473`).
`Layout::from_size_align` returns `Err` on **either** a non-power-of-two `align` **or** a `size`
that overflows `isize::MAX` when rounded up to `align`. The `from_raw_parts` `# Safety` contract
(`:411-415`) constrains `reservation_len` too ("a non-zero multiple of [`PAGE`]"), and that half is
not checked.

So `unsafe { Reservation::from_raw_parts(b, PAGE, r, usize::MAX, PAGE) }` still constructs
successfully and still panics inside `Drop` under the miri backend — exactly the
"silently-deferred hazard" the commit says it converted into "a loud, attributable failure at the
actual point of misuse". The fix is a genuine improvement (it closes the more likely misuse and it
is counterfactually proven, §2.6); it just does not fully close its own stated hazard, and neither
the code comment (`:437-452`) nor `CHANGELOG.md:244` notes the remaining half.

**Suggested close:** extend the assert to
`Layout::from_size_align(reservation_len, align).is_ok()` — or simply add
`reservation_len != 0 && reservation_len.is_multiple_of(PAGE) && reservation_len.checked_add(align - 1).is_some()`
— and add a matching `should_panic` case. Either way the whole `Drop`-reachable panic disappears
rather than shrinking.

### F8 — LOW — `#717` left the third native address-computation site untouched: the Unix **fast path**, which is the most-taken reservation path in the crate

**`crates/vmem/src/lib.rs:1413`** (`let region_addr = region_ptr as usize;`), **`:1416`**
(`libc_munmap(region_ptr as *mut u8, size)`), **`:1422`**
(`NonNull::new_unchecked(region_ptr as *mut u8)`), and **`:1682`**
(`if (p as usize) == MAP_FAILED` in `libc_mmap`).

This is *not* the provenance-losing shape `#717` fixed — the returned `base` at `:1422` is derived
by a pointer-to-pointer cast, which preserves provenance, so no pointer is manufactured out of an
integer. But `ptr as usize` **is** an exposing cast (`expose_provenance` in strict-provenance
terms): it marks the whole allocation as exposed, which is precisely the "exposed-address `as
usize`" phrase the README guarantee at `:95-96` uses. `try_reserve_aligned_exact` is the 1-syscall
fast path every Unix reservation tries first, so on Unix this executes on essentially every call —
while `unix_reserve`'s slow path, which `#717` did clean, only runs on an alignment miss.

`.addr()` is a drop-in for `:1413` and `:1682` with no other change required. Rated LOW because
there is no unsoundness and no behaviour difference on any current target; flagged for the same
reason the `racy-ptr-cell` closing review flagged its F2 — a discipline the round declared and
enforced in one of two sibling functions is worth finishing while it is fresh, and it is a
prerequisite for ever running this crate under `-Zmiri-strict-provenance`.

### F9 — LOW — the README's "Alignment contract" section is stale in two ways this round created

**`README.md:87-88`**: *"Violations return `None` / **are no-ops** — **never a panic**, so this is
safe to call from inside a `GlobalAlloc::alloc` body."*

- After `#712`, a `recommit`/`commit_range` offset violation is **not** a no-op — it returns
  `false` / `Err(invalid_argument())`. That is the whole point of the fix, and it is the sentence a
  downstream reader would consult.
- After `#719`, `Reservation::from_raw_parts` **does** panic (`lib.rs:453`). The blanket "never a
  panic" claim now has an exception, and it sits in the section that sells the crate's
  `GlobalAlloc`-safety.

Adjacent, and worth naming in the same fix: `#712` deliberately left `decommit`/`decommit_lazy`
(`lib.rs:584`, `:618`) with the silent-skip-on-violation shape it rejected for their siblings —
a defensible scoping call (the commit says so explicitly, and their `()` return carries no
write-permitting sentinel), but the resulting asymmetry appears in **neither** function's rustdoc.
A caller who learns from `recommit`'s new doc that violations are rejected will reasonably assume
`decommit` behaves the same way.

### F10 — LOW — the CI step `#699` added says "4 passed"; it runs 5

**`.github/workflows/ci.yml:762-764`**: *"All 4 of this file's tests had therefore never executed
in ANY CI configuration. Fixed with a dedicated step using the file's actual required feature
combination (verified locally: 4 passed)."* Accurate when `6b18834` landed; `b8b70fb` (#718) added
`fail_next_is_atomic_under_concurrent_callers` five commits later without revisiting the comment.
Verified — the step now runs 5:

```text
     Running tests\fault_injection.rs
running 5 tests
test result: ok. 5 passed; 0 failed
```

Trivial to fix, but it is the kind of hardcoded count CLAUDE.md's own "never a hardcoded count"
guidance for the `unsafe` inventory exists to avoid.

### F11 — LOW — `CORRECTNESS_OPEN_ITEMS.md` item 41 was not updated when `#716`, two commits later, closed one of its two named blockers

**`docs/CORRECTNESS_OPEN_ITEMS.md:1795`** (filed by `d6b72b1`) lists two miri incompatibilities as
its Evidence. Item **#2** is `tests/lazy_commit.rs`'s uninitialized read, presented as live and
cross-referenced to task #716. `81ecfe3` (#716) then **fixed** it (`#[cfg(not(miri))]` at
`tests/lazy_commit.rs:287-295`) and did not touch item 41. `git log --oneline -3 --
docs/CORRECTNESS_OPEN_ITEMS.md` confirms `d6b72b1` is still the newest touch.

CLAUDE.md's standing rule is explicit — *"OPEN_ITEMS indexes are CURRENT-STATE, not archives … a
closed / null / rejected item must NOT look active due to a stale header … the round that closes it
MUST update the card … in the SAME commit."* The item as a whole is legitimately still open (the
CI step and the intentional-leak blocker remain), so this is a card-accuracy defect rather than a
stale-open-item defect; the "Next trigger" line happens to be phrased forward-compatibly. Still,
a reader opening item 41 next round will re-investigate a closed sub-item.

### F12 — LOW — `tests/huge_pages.rs`'s module doc under-claims its own CI coverage: the Linux runner it says would be needed already exists and already runs these tests

**`crates/vmem/tests/huge_pages.rs:15-19`**: *"this file exists so a Linux CI runner (**were one
added** — see `docs/CORRECTNESS_OPEN_ITEMS.md` item 41 for the adjacent miri-CI gap) would actually
exercise this rejection logic."*

A Linux runner already runs this crate's full suite: `ci.yml`'s `test-workspace` job is
`runs-on: ubuntu-latest` (`:700`) and executes `cargo test -p aligned-vmem --all-features`
(`:750`), which compiles `huge_pages.rs` (the file is gated only on `feature = "huge-pages"`) and
runs all four `#[cfg(target_os = "linux")]` tests. I traced each one's expected outcome on a
GitHub runner with no hugepages configured and all four pass by construction (the two rejection
tests hit the new guard; `..._accepts_huge_page_aligned_request` deliberately tolerates an OS
refusal; the type-level test is unconditional). So `#714`'s Linux-only regression coverage is
genuinely live in CI from the next push — better than the round claims for itself — and the
parenthetical should be corrected so a future reader does not conclude it is dead weight.

The narrower true statement, which is what the honesty note presumably meant, is that no
**hugetlb-configured** host runs them, so the `MAP_HUGETLB`-success branch stays unexercised. That
is worth stating precisely rather than as "were one added".

### F13 — LOW — the round flagged four deferrals in commit messages and filed only one of them in an index

Only `#713`'s side-discovery reached `docs/CORRECTNESS_OPEN_ITEMS.md` (item 41, correctly and
well-formatted). Four other explicitly-deferred or explicitly-unverified items live only in commit
bodies, where they do not survive a session boundary:

- `#714` — the BSD `_SC_PAGESIZE` values (47 / 28) and the hugetlb-alignment reasoning are
  **REASONED-FROM-SPEC, never executed** on any of the six affected targets. An OS-conformance
  constant asserted from a header file is exactly the kind of claim that should carry a
  "verify when a BSD runner exists" trigger.
- `#715` — the `--cfg`-flag conversion, deferred with an explicit *"Revisit if/when this crate
  gains real external consumers"*, and explicitly promised as *"one consistent policy"* binding
  `numa-shim`'s identical §C10 finding in the very next round.
- `#719` item 6 — the `off_t` width, deferred *"until this crate gains a real 32-bit Unix target"*.
- `#718` — the admitted non-regression test (which **F1** shows is worse than admitted).

CLAUDE.md: *"When a gate report / commit / review newly flags an open item, add it to the
appropriate index in the same commit"*, with `docs/CORRECTNESS_OPEN_ITEMS.md` scoped to
"correctness bugs, flaky tests, and CI-coverage gaps flagged from ANY source (commit messages, code
comments, reviews)". Mitigating, and stated for fairness: this is a sweep-wide pattern (the
`racy-ptr-cell` round's F5 named the same thing), and the `#715` deferral in particular is
load-bearing for the *next* crate in the sweep, so it is the one most likely to be silently lost.

### F14 — INFORMATIONAL — `#718`'s test asserts a concurrency guarantee the crate does not actually document

**`crates/vmem/tests/fault_injection.rs:201-206`**, justifying `unsafe impl Sync for SendPtr`:
*"every thread only calls `commit_range` on an already-committed range, which the crate documents
as idempotent **and safe to call concurrently**."* `commit_range`'s `# Safety` (`lib.rs:725-731`)
documents the range/liveness contract and idempotence ("or already committed — recommitting is
harmless on Windows"); it says nothing about thread-safety. The claim is true in practice
(`VirtualAlloc(MEM_COMMIT)` is thread-safe, and the Unix/miri paths are no-ops), but the SAFETY
comment cites a documented guarantee that does not exist. Either the doc should gain the guarantee
(it is worth having — a `GlobalAlloc` consumer will want it) or the comment should justify it from
the platform semantics directly.

### F15 — INFORMATIONAL — a third concurrency hazard remains in the same 3-atomic protocol the round declared closed

**`crates/vmem/src/fault_injection.rs:130-131`** — `should_fail_commit`'s one-shot self-disarm
writes `FAIL_AT_TARGET = 0` and `FAIL_AT_COUNTER = 0` with `Relaxed`. If thread A fires the
one-shot while thread B concurrently calls `arm_fail_at(k)` (`FAIL_AT_COUNTER = 0` Relaxed, then
`FAIL_AT_TARGET = k` Release), A's disarm can land after B's arm, silently cancelling a
freshly-armed hook with no signal. Symmetrically, A's `FAIL_AT_COUNTER = 0` races with other
threads' in-flight `fetch_add`.

This is pre-existing, is genuinely out of `#718`'s stated scope (which named exactly two hazards),
and matters only for a consumer arming from multiple threads. It is worth recording because the
commit subject and `CHANGELOG.md:243` both say the round "closed **two** real data-race hazards in
`fault_injection`'s atomics", which reads as an exhaustive audit of a 3-atomic, ~40-line module. A
one-line scope note ("a concurrent `arm_fail_at` racing the one-shot disarm remains unhandled; the
hooks assume a single arming thread") would make the claim precise.

---

## 4. Category-by-category verdicts on the questions asked

### 4.1 Does every diff do what its message claims? — **yes, 11 of 11**

§1. No commit misdescribes its own diff. The divergences are all in the *aggregate* reporting layer
(CHANGELOG/checkpoint), not in the per-commit record — with the single exception of `#713`'s "the
one non-reservation site" (**F2**) and `#718`'s race-window diagnosis (**F1**).

### 4.2 Are the "zero-trust counterfactual" claims real? — **five of six real; the blanket claim is not**

Four constructed and re-run here, all reproducing exactly (§2.1-2.3, §2.6); a fifth (`#715`'s
E0638 compile-failure check) is structurally verifiable from the `tests/mock.rs:29` `..` addition
in the same diff. `#711`, `#714`, `#717` and `#718` have none — **F4**, and for `#718` specifically
**F1**.

### 4.3 Is `#717`'s strict-provenance usage sound? — **yes, both sites**

§2.5. `region_ptr`'s provenance covers the full `over`-byte allocation in both functions; both
`base_addr` and `tail_start` are proven in-bounds by the same `checked_add`-guarded `fits` closure
that gates reaching `with_addr` at all; the Windows `VirtualAlloc(MEM_COMMIT)` calls now derive
from `base` rather than a bare integer. `.with_addr()` is the documented API for this exact shape.
The one incompleteness is the untouched fast path — **F8**.

### 4.4 Is `#718`'s `Release`/`Acquire` pairing correct, and is `fetch_update`'s closure legal? — **yes to both**

§2.4. The store side (`:93`, `Release`) and load side (`:124`, `Acquire`) are correctly paired, and
the reader genuinely gates its payload read on the flag, which is what makes it a payload/flag pair
rather than two independent reads. The closure `(next > 0).then(|| next - 1)` is pure, total and
idempotent — legal under `fetch_update`'s retry semantics — and the eager-vs-lazy distinction the
commit calls out is real and correctly resolved.

**And on the specific question of whether the test's honesty note undermines the fix's
credibility:** the *fix's* credibility is untouched — it rests on `fetch_update`'s
atomic-by-construction semantics, which is correct. But the note is **not** the reasonable,
honestly-scoped limitation it presents itself as. It is an honest observation with a wrong
explanation, and the wrong explanation is what makes it read as an unavoidable limitation rather
than a fixable oracle-design mistake. That distinction is the whole of **F1**.

### 4.5 Does `#719`'s `from_raw_parts` assert genuinely close the described hazard? — **for `align`, yes and completely; for the hazard as a whole, no**

§2.6 enumerates all four `Reservation` construction sites and confirms the three public entry points
cannot produce an invalid `align`. But `Layout::from_size_align` fails on `reservation_len` too, and
that half is unguarded — **F7**.

### 4.6 Are the doc-only fixes factually accurate? — **yes, all four**

- **`#715` / `#719` `off_t`** (`lib.rs:1631-1645`): accurate. `off_t` really is 64-bit on
  x86_64 Linux/FreeBSD/NetBSD/macOS and on Windows-irrelevant paths; `mmap` really is called with a
  literal `0` offset at the sole call site (`:1680`), so no value can truncate; and the residual is
  correctly characterised as ABI *shape* only. The deferral rationale is sound.
- **`#717` / `#719` `from_raw_parts`-is-not-the-inverse-of-`into_parts`** (`:386-402`): correct and
  well-argued. `into_parts` returns `(*mut u8, usize, usize)` = `(reservation, reservation_len,
  align)` (`:369-373`), discarding `base` and `len`; `from_raw_parts` needs all five; and `release`
  (`:542`) does take exactly that 3-tuple. The identification of `release` as the true complement is
  right.
- **`#719` `let _ =` discards** (`:1692-1702`, `:1712-1719`): accurate. `munmap` failure really does
  leave the mapping valid (leak, not unsafety), `madvise` failure really does leave the pages
  exactly as valid as before, and both public entry points really do have infallible signatures
  with no channel to surface an error.
- **`#719` `DecommitKind` dead-code narrowing** (`:1822-1832`): verified in both directions —
  `cargo check -p aligned-vmem` and `--features mock` are both clean with zero `dead_code`
  warnings.

The doc problems this round has are all in text it **did not** update (**F3**, **F9**) or text it
newly wrote outside the crate (**F4**, **F5**), not in the four doc fixes it set out to make.

### 4.7 Any dangling TODO, debug code, out-of-scope edit, or a new safe `pub fn` of the CLAUDE.md-prohibited shape? — **none**

- `grep -rn "TODO\|FIXME\|XXX\|unimplemented!\|todo!\|HACK\|dbg!" crates/vmem/` → nothing.
- Every public function in this crate that accepts a raw pointer is already `unsafe fn`
  (`from_raw_parts`, `release`, `decommit`, `decommit_lazy`, `recommit`, `try_recommit`,
  `commit_range`, `try_commit_range`), each with a `# Safety` section. The round added no new
  `pub fn` at all — it added an `assert!` inside an existing `unsafe fn`.
- The crate's three `bench-internals` accessors (`unix_exact_reserve_attempts`,
  `unix_exact_reserve_hits`, `windows_reserve_commit_calls`) are safe `pub fn`s but take no
  arguments and touch no allocator metadata; CLAUDE.md's benchmark-hook rule (1) does not engage,
  and rule (2)'s gating requirement is satisfied — they are gated on `bench-internals`, which is not
  in any bundle.
- `#![allow(unsafe_code)]` inventory unchanged: exactly one tier-1 module-level allow
  (`src/lib.rs:75`), zero tier-2 item-level allows. No new `unsafe fn`, no new `unsafe impl` in
  `src/` (the round's two new `unsafe impl`s are in `tests/fault_injection.rs:204`/`:206`, both with
  `// SAFETY:` comments — see **F14** on one of them).

### 4.8 CHANGELOG cross-check against the real diff

- **All 11 commit SHAs are correct** and cited in landing order:
  `56d0764`, `6b18834`, `54089fa`, `131355a`, `d6b72b1`, `2e7f4f5`, `e5f6700`, `81ecfe3`,
  `94aef18`, `b8b70fb`, `55e71b0`. Each resolves and each matches the task it is attributed to.
- Each bullet's substantive technical description matches its diff. Defects, all named above:
  **F4** (the blanket counterfactual claim, `:234`), **F5** (`#711`'s fabricated rationale,
  `:230`), **F3** (the "no valid-input success path" header claim, `:234`), **F1**'s wrong
  diagnosis reproduced verbatim (`:243`), and **F7**'s half-closed hazard described as closed
  (`:244`).
- Two small imprecisions not worth their own findings: `:238` says the raw helpers changed on
  "Windows and Unix" (the miri block changed too), and `:239` says "all four BSDs" (six targets
  were wrong — `tvos`/`watchos` were also on the Linux value; the round fixed them silently).
- **Commit prefixes** — `fix(perf)` ×8, `build` ×1, `docs` ×2. Correct under R30-12: no `perf(...)`
  appears, every `fix(perf)` genuinely changed shipping or opt-in code for correctness with no
  speedup claimed, and the two doc-only commits are `docs`. One arguable case: `2e7f4f5` (#714)
  changed observable Linux behaviour on a previously-succeeding path (**F3**), which nudges it
  toward `perf(opt-in)` territory under the taxonomy's own "a non-default feature's CODE changed"
  clause — `huge-pages` is exactly such a feature. Not worth retagging (the taxonomy is explicitly
  non-retroactive), but it is the reason **F3**'s header-claim correction matters.

---

## 5. Categories with nothing to report

Stated explicitly rather than omitted:

- **Out-of-scope edits:** none. Every hunk is inside `crates/vmem/`, `ci.yml`, `CHANGELOG.md`,
  `docs/CORRECTNESS_OPEN_ITEMS.md`, or the two checkpoints the round's own post-work tasks called
  for.
- **Version bumps / dependency changes:** none. `crates/vmem/Cargo.toml`'s `version = "0.2.0"` is
  untouched; its whole +23-line diff is comments; `Cargo.lock` is untouched by the entire range;
  `#711` moved a version pin without changing it.
- **New `unsafe` surface in `src/`:** none. Same inventory before and after.
- **SAFETY-tag substance:** every `unsafe` block in `src/lib.rs` carries a `// SAFETY:` naming a
  real invariant, including the one `#719` added (`:1444-1447`). The two `unsafe impl`s the round
  added in `tests/` both carry justifications; one of them cites a guarantee the crate does not
  document (**F14**), but neither is filler.
- **Clippy / fmt:** clean in both configurations re-run in §0, `-D warnings`, `--all-targets`.
- **Doctests:** zero, in every configuration. CLAUDE.md's no-doctest rule holds; the crate's
  illustrative examples use ` ```text ` fences (`lib.rs:40-56`, `mock.rs:15-21`) with the runnable
  forms correctly pointed at `tests/smoke.rs` and `tests/mock.rs`.
- **Feature isolation:** `--features fault-injection` *without* `lazy-commit` compiles clean, so the
  crate-level dead-code carve-out at `lib.rs:96-99` is still correctly scoped; `bench-internals`
  alone and default-features-only both compile clean.
- **Test vacuity:** none of the round's new tests is vacuous. `from_raw_parts_accepts_a_valid_reservation`
  genuinely writes and reads through the adopted span; `vmem_error_kinds_are_distinguishable`
  asserts all three kinds pairwise *and* the `Display` string; `recommit_/commit_range_rejects_…`
  each assert three distinct violation shapes plus the fallible form's error kind. The single
  exception is `#718`'s concurrency test, which is not vacuous in the ordinary sense (it does assert
  something true and would catch a deadlock or a gross miscount) but is one-sided against the bug it
  names — **F1**.
- **Root-repo consumers:** `src/alloc_core/os.rs`'s `commit_pages`/`recommit_pages` still compile
  and the root suite is unaffected; no in-repo code consumes `VmemError::os_code()` or
  `is_invalid_argument()` (grepped `src/` and `crates/numa/src/`), so `#713`'s representation change
  has no downstream reach.

---

## 6. Suggested follow-up tasks

| Priority | Finding | One-line action |
| --- | --- | --- |
| P1 | F1 | Change `fail_next_is_atomic_under_concurrent_callers` to `arm_fail_next(TOTAL / 2)` + `assert_eq!(failures, TOTAL / 2)` (reproduces the pre-fix bug 5/5 on real hardware, no delay, no loom), and rewrite the test doc + `CHANGELOG.md:243` + the checkpoint to replace the thread-jitter explanation with the oracle's ≥-monotonic one-sidedness. |
| P1 | F3 | Document the Linux-only 2 MiB `size`/`align` requirement on `reserve_aligned_huge`/`try_reserve_aligned_huge` and `README.md:54`; fix the "contract is identical to `reserve_aligned`" sentence; correct or carve out `CHANGELOG.md:234`'s "any valid-input success path". |
| P2 | F2 | `mock::take_reserve_fault`/`take_commit_fault` → `os_refusal_unknown_code()`, mirroring `lib.rs:779`; reproduced fabricated `code 0` output is in §F2. |
| P2 | F4 + F5 | Correct `CHANGELOG.md:230` (`#711`'s three manifests + no aligned-vmem loom dependency) and `:234` ("each verified via a genuine zero-trust counterfactual" → name the five that were, and say plainly that #711/#714/#717/#718 were not). |
| P2 | F6 | Demote the 5 private intra-doc links in `fault_injection.rs:41`/`:85-86` to plain code spans; add `cargo doc --features "lazy-commit huge-pages fault-injection"` to this crate's pre-publish gate (#658). |
| P3 | F7 | Extend `from_raw_parts`'s assert to cover `reservation_len` (e.g. `Layout::from_size_align(reservation_len, align).is_ok()`), with a matching `should_panic` case. |
| P3 | F8 | Apply `#717`'s own fix to `try_reserve_aligned_exact` (`lib.rs:1413`) and `libc_mmap` (`:1682`) — `.addr()` in both — so the Unix fast path matches the slow path's discipline. |
| P3 | F9 + F12 | Reword `README.md:87-88` ("are no-ops" / "never a panic"); note the `decommit`-vs-`recommit` violation-handling asymmetry in both rustdocs; correct `tests/huge_pages.rs:15-19` — the Linux runner exists, only a *hugetlb-configured* one does not. |
| P4 | F13 | File the four commit-message-only deferrals into `docs/CORRECTNESS_OPEN_ITEMS.md` — especially `#715`'s `--cfg`-flag decision, which the round promises will bind `numa-shim`'s identical §C10 finding in the very next crate. |
| P4 | F10 + F11 + F14 + F15 | Housekeeping: `ci.yml:764` "4 passed" → 5; update item 41's Evidence now that `#716` closed its sub-item 2; either document `commit_range`'s concurrent-call guarantee or re-justify the test's `Sync` impl from platform semantics; add a scope note that `arm_fail_at`'s one-shot disarm still races a concurrent arming. |

---

## 7. Verdict

**The round's code is safe to consider genuinely closed. Its reporting is not.**

Every shipped source change in `crates/vmem/src/` is correct, the test suite is green in every
configuration I ran, clippy and fmt are clean, and there is no memory-safety defect, no new
`unsafe` surface, no out-of-scope edit, and no TODO anywhere in the round. All four mechanically
constructible counterfactuals reproduce. `#712`, `#713`, `#716`, `#717` and `#719` are, on the
merits, closed.

What is not closed is the *evidence* for `#718` and the *documentation* for `#714`. **F1** is the
one finding that would block me from signing off the round as-is: the round's flagship concurrency
fix shipped with a test that cannot fail against the bug, accompanied by a confidently-stated wrong
explanation for why — repeated in four places, including the published CHANGELOG — and that
explanation is exactly what stopped anyone from making the test work. It is fixable by changing one
constant. **F3** is the one finding with user-visible consequences: a public API's contract silently
narrowed on Linux, with the old contract still printed in the rustdoc a docs.rs reader will see.

Both are cheap. With **F1**, **F2**, **F3**, **F5** and **F6** closed, this round is in as good a
state as the `tagged-index-stack` and `racy-ptr-cell` rounds were after their own closing reviews'
follow-ups (#771-772, #773-774) landed — and `aligned-vmem` 0.2.0 (task #658) would be ready to
publish.
