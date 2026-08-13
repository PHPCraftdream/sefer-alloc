# `aligned-vmem` — round-10 CLOSING review (verification of the H1/M1/M2/L1 remediation, and an answer to "what else are we obligated to fix?")

**Date:** 2026-08-13

**Scope:** verification of the four remediation tasks (#909 H1, #910 M1, #911 M2, #912 L1) that
closed `docs/reviews/2026-08-13-aligned-vmem-independent-review.md`, the four `--no-ff` merges
that landed them, the merge-conflict resolution that renumbered `docs/CORRECTNESS_OPEN_ITEMS.md`
item 52 → 53, and the round's CHANGELOG entry. Plus — as the round brief explicitly commissioned
— a **fresh, targeted pass hunting the specific failure-mode shape that let H1 hide for nine
rounds**: every hard-coded constant and every assumed platform fact used as the basis of a
safety-critical calculation, audited for "has anyone ever verified this constant's own premise,
or has everyone only verified the arithmetic built on top of it?" (§4).

**Reviewed tree:** local `main` @ `00868d1469966f8d36e10b252dc4e5953c99ab34`, identical to
`origin/main` (`git rev-parse origin/main` → same SHA). Round span: `a1d75ec..HEAD`, **9 commits**
(4 task commits + 4 merges + the docs/CHANGELOG commit), `git diff a1d75ec..HEAD --stat` →
6 files, **+316 / −51**. `git status --porcelain` is **empty** — no untracked residue, which is
the first round in this campaign where that is true at closing-review time (contrast round 9's
V2C7).

**Toolchain / host:** `rustc 1.97.0` (2d8144b78 2026-07-07), stable-`x86_64-pc-windows-msvc`;
Windows 10 Pro, 4 KiB page. **No Linux host, no Darwin host, no hugetlb-configured host, no
32-bit target installed** — every Linux/Darwin/HugeTLB claim below is reasoned from spec and from
code read in the current tree, plus the `x86_64-unknown-linux-gnu` cross-compile row (item 51).
Target-`cfg` claims (§H2C1, §H2C7) were established by running `rustc --print cfg --target <t>`
for each named target, which needs no installed std and is quoted verbatim where used.

**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / `git push`, no branch/worktree/ref mutation. Every
`file:line` citation below was read in the current tree before being written down; every
before/after comparison was made by reading the cited line at BOTH the pre-round base
(`git show a1d75ec:<path>`) or the relevant worktree tip and at `HEAD`.

**Finding prefix:** `H2C` (round-10 closing — "the H1/M1/M2/L1 round's closing review"). Prior
prefixes deliberately not reused: `V`/`W`/`P` (rounds 1–2), `F` (3), `R`/`CR` (4 + closing),
`Q`/`QC` (5 + closing), `S`/`SC` (6 + closing), `T`/`TC` (7 + closing), `U`/`UC` (8 + closing),
`V2`/`V2C` (9 + closing), `H1`/`M1`/`M2`/`L1` (the round-10 independent review itself).

---

## Verdict up front

**All four fixes landed, all four landed on the intended content and mechanism, and the highest-
stakes one (H1) is correct — including the constant the delegate self-corrected, which I
re-derived from scratch rather than trusting the correction.** The full matrix is green here,
re-executed rather than taken on trust: **44/49 tests**, four clippy rows, `fmt --check`, the
doc-drift guard, `cargo bench --no-run`, a clean conflict-marker sweep, and — a check no prior
round ran — **the only in-repo downstream consumer of the API M1 tightened (`numa-shim`, 33
tests) still passes**, with its argument proven to satisfy the new assertion by construction.

**But round 10 did reproduce the campaign's signature pattern — twice, and one of them
mechanically.** `docs/CORRECTNESS_OPEN_ITEMS.md` item 52's own evidence citation
(`lib.rs` "lines 2418-2447") was **correct in the worktree it was written in and stale the moment
it was merged**, because the H1 merge inserted 28 net lines above it — the exact "durable citation
outside the diff that the diff invalidates" class that round 9's closing review argued is the one
thing a diff-scoped review structurally cannot catch (H2C8). And M1's fix replaced one false
completeness claim with a **broader** false completeness claim (H2C3). Neither is a code defect;
both are the campaign's most frequent finding class recurring inside the round that was supposed
to be different.

**One finding is publish-blocking and it is a genuine regression introduced by round 10**
(H2C1, MEDIUM): M2's blanket `compile_error!` makes `aligned-vmem` 0.2.0 **fail to build** on
`i686-unknown-linux-gnu` (Rust **Tier 1**) and `armv7-unknown-linux-gnueabihf` (Tier 2), both of
which build 0.1.0 today — while `Cargo.toml`'s crates.io `description` still opens with
"Cross-platform", the README still publishes no supported-target list, and the CHANGELOG carries
no BREAKING note. The independent review's own M2 remedy required the restriction be encoded
"in `cfg`/compile-time guards **and publication docs**"; only the first half shipped.

**§5 is the point of this document.** It answers, directly, the question that commissioned the
round: *"We already said there's nothing left to find, and then this whole batch got found. What
else are we obligated to fix?"*

---

## 1. Remediation verification table

| Finding | Sev | Task / merge | Landed? | Mechanism independently re-traced? | Verdict |
|---|---|---|---|---|---|
| **H1** — Linux HugeTLB leak on non-2 MiB default huge-page size | HIGH | #909 / `837de37` → merge `c3850e7` | Yes | Yes — see §2 | **CORRECT AND COMPLETE** for both the exact-size and the over-reserve paths. Two undocumented side effects: H2C4, H2C5. |
| **M1** — `from_raw_parts` accepted forbidden `reservation_len` | MED | #910 / `22a1d74` → merge `da643f2` | Yes | Yes — see §3 | **CORRECT.** No legitimate caller broken (proven, not assumed). New source comment overclaims: H2C3. |
| **M2** — `mmap` `off_t: i64` ABI shape on 32-bit Unix | MED | #911 / `b1185fb` → merge `806cb30` | Yes | Yes | **PARTIAL.** The `cfg` half landed; the publication-docs half did not, and the chosen form is a hard build break on two real targets: **H2C1**. Evidence citation stale: **H2C8**. |
| **L1** — bench recorded OOM/no-op as valid fast samples | LOW | #912 / `6f62513` → merge `3beb34a` | Yes | Yes | **CORRECT.** All four workloads + the inline 1 MiB arm now panic with a named-operation message; the `-> bool`/`black_box(ok)` laundering is fully removed. |
| **L2** — `decommit_lazy_roundtrip` vacuous w.r.t. lazy decommit | LOW | *(none — confirmed already-tracked)* | n/a | n/a | Correctly triaged as item 48's S4 remainder, no task opened. Still open. |
| Item 52/53 merge-conflict resolution | — | `c3850e7` conflict resolution | Yes | Yes | **Both items' content preserved in full, no drop, no conflict-marker residue.** Three defects in the *result*: **H2C2** (tier placement + indentation), **H2C8** (stale citation). |

**Re-executed matrix (all on this host, at `00868d1`):**

| Command | Result |
|---|---|
| `cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast` | **44 passed, 0 failed** (summed across all 7 test binaries + doctests) |
| `cargo test -p aligned-vmem --all-features --no-fail-fast` | **49 passed, 0 failed** |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` | clean |
| `cargo clippy -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets -- -D warnings` | clean |
| `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` | clean |
| `cargo clippy -p aligned-vmem --target x86_64-unknown-linux-gnu --features "…" --all-targets -- -D warnings` (item 51 row) | clean |
| `cargo fmt -p aligned-vmem --check` | clean |
| `node scripts/vmem-doc-drift-guard.mjs` | `OK: no unconditional over-reserve/trim statements found` |
| `cargo bench -p aligned-vmem --no-run` | clean |
| `cargo test -p numa-shim --all-features --no-fail-fast` *(downstream consumer of the M1-tightened API — not part of any prior round's matrix)* | **33 passed, 0 failed** |
| conflict-marker sweep over `crates/vmem/`, `docs/CORRECTNESS_OPEN_ITEMS.md`, `CHANGELOG.md` | none |

The "44/49" claim in the round's CHANGELOG is **confirmed exactly**, and the +2 over round 9's
42/47 is exactly M1's two new negative tests (`from_raw_parts_rejects_zero_reservation_len_immediately`,
`from_raw_parts_rejects_non_page_multiple_reservation_len_immediately`), both observed in the
run output as `should panic … ok`.

**CI:** `gh run list --commit 00868d1469966f8d36e10b252dc4e5953c99ab34` — `Kani verification`
**completed / success**; the main `CI` workflow was still `in_progress` at the time of writing
(run `31739534515`). Task #913 owns confirming it. Nothing in this review depends on that result;
the local Linux cross-compile row is the substitute signal for the `cfg(unix)` half.

---

## 2. H1 — re-traced independently

**The constant.** `MAP_HUGE_2MB: i32 = 21 << 26` (`crates/vmem/src/lib.rs:2341`). I re-derived
this without reference to the delegate's reasoning. Linux's `include/uapi/linux/mman.h` defines
`MAP_HUGE_SHIFT 26` and encodes the size field as **log2 of the page size in bytes**:
`MAP_HUGE_64KB = 16 << SHIFT`, `MAP_HUGE_2MB = 21 << SHIFT`, `MAP_HUGE_1GB = 30 << SHIFT`.
2 MiB = 2²¹ ⇒ **21** is correct. `21 << 26 = 1 409 286 144 = 0x5400_0000`, which fits `i32`
(< 2³¹) with no overflow, and `MAP_HUGETLB | MAP_HUGE_2MB = 0x0004_0000 | 0x5400_0000 =
0x5404_0000`. **The delegate's self-correction is itself correct.** One point the constant's own
doc does not make, and which is worth recording because its immediate sibling `MAP_HUGETLB`
carries the opposite caveat: `MAP_HUGE_SHIFT` lives in the **generic** `uapi/linux/mman.h`, not in
any `arch/*/include/uapi/asm/mman.h`, so unlike `MAP_HUGETLB` (wrong on MIPS/Alpha/PA-RISC/Xtensa,
documented in-source by task #893) `MAP_HUGE_2MB` is **architecture-independent**. Silence is the
right answer here; I record the derivation so a future round does not have to redo it.

**The mechanism, on both paths.** `unix_reserve` (`:2119-2126`) rejects any huge request whose
`size` or `align` is not a multiple of `LINUX_HUGE_PAGE_SIZE`. Given that:

- **Exact path** (`try_reserve_aligned_exact`): maps exactly `size`; `size` is a 2 MiB multiple and
  the kernel now maps 2 MiB pages by explicit request, so no rounding occurs and
  `reservation_len = size` is the true mapping length. The miss-path `munmap(ptr, size)` is
  huge-aligned. ✔
- **Over-reserve path**: `over = size + align`, both 2 MiB multiples ⇒ `over` is a 2 MiB multiple;
  the single whole-mapping `munmap(region_ptr, over)` in `release_reservation` is huge-aligned. ✔
- **Ordinary-page fallback** (`libc_mmap(over, false)` at `:2020-2023`) maps a 2 MiB-multiple length
  with ordinary pages — `munmap` of that length is trivially page-aligned. ✔

The leak H1 described required `reservation_len` to under-report the kernel's rounded-up length.
After the fix the crate's request and its bookkeeping are pinned to the **same** value by
construction, which is what makes this a premise fix rather than another arithmetic fix. The public
`reserve_aligned_huge` rustdoc's existing claim (`:1439-1442`, "`size` and `align` must BOTH
additionally be multiples of the Linux huge-page size (2 MiB)") was a statement about a *system
default* before this round and is a statement about *what the crate requests* after it — H1 made a
pre-existing public doc line true. That is worth naming as the positive result.

**No regression on the 2 MiB-default host:** explicitly requesting 2 MiB where 2 MiB is already the
default is a no-op at the kernel; behaviour is byte-identical. **On a 1 GiB-default host** the fix
converts a silent leak into a clean fall-back to ordinary pages with `is_huge() == false`, which the
public rustdoc already licenses ("the reservation transparently falls back to ordinary pages"). See
H2C4 for the one case where that fall-back is a *loss*.

---

## 3. M1 — re-traced independently, including the "does this break a real caller?" question

The new predicate is `reservation_len != 0 && reservation_len.is_multiple_of(PAGE)` alongside the
pre-existing `align.is_power_of_two() && align >= PAGE && Layout::from_size_align(...).is_ok()`
(`:759-767`). The independent review's claim that `Layout::from_size_align` accepts both violations
is correct: `Layout` permits zero-size layouts and does **not** require size to be a multiple of
align.

**The regression question the round did not ask.** `from_raw_parts` is an `unsafe fn` on the public
surface, so tightening it can break a real caller. There is exactly one in this repository:
`crates/numa/src/lib.rs:1137`, `Reservation::from_raw_parts(base, size, raw as *mut u8, over, align)`.
`reserve_aligned_numa` rejects `size == 0`, `!size.is_multiple_of(PAGE)`, and
`!align.is_power_of_two() || align < PAGE` at `:1059` *before* computing `over = size + align`
(`:1061`) — so `over` is a non-zero `PAGE` multiple **by construction**, and the new assertion is
satisfied for every input that reaches it. Confirmed empirically: `cargo test -p numa-shim
--all-features` → 33 passed. **No legitimate caller is broken**, and this is proven from the
caller's own guard rather than inferred from a green test run.

The counterfactual claim in the task commit (removing the predicates makes both new tests fail) is
consistent with the tests' shape — each wraps the construction in `catch_unwind`, releases the real
reservation, then `resume_unwind`s the payload into `#[should_panic(expected = …)]`, so without the
predicate there is no payload and `panic_info.unwrap()` fails. The `expected =` strings are
substrings of the new assertion message and both were observed passing. Not vacuous.

---

## 4. NEW findings

Severity is rated on the crate's own scale: HIGH = can lose or corrupt memory / leak an entire
mapping on a normal configuration; MEDIUM = breaks a documented guarantee or a real consumer's
build; LOW = wrong or incomplete claim with bounded blast radius; INFO = record-only.

### H2C1 — MEDIUM — round 10 narrowed the SUPPORTED target surface without narrowing the ADVERTISED one; 0.2.0 now hard-fails to build on a Tier-1 target that 0.1.0 builds

**Where:** `crates/vmem/src/lib.rs:2466-2477` (the new `compile_error!`); `crates/vmem/Cargo.toml`
(`description`); `crates/vmem/README.md` (§"Platform caveats"); `CHANGELOG.md:477`.

M2's fix is `#[cfg(all(unix, not(miri), target_pointer_width = "32"))] compile_error!(...)`.
Verified by `rustc --print cfg`:

- `i686-unknown-linux-gnu` → `unix`, `target_pointer_width="32"` — **Rust Tier 1**.
- `armv7-unknown-linux-gnueabihf` → `unix`, `target_pointer_width="32"` — **Tier 2 with host tools**
  (Raspberry-Pi-class targets).

Both compiled `aligned-vmem` 0.1.0 (nothing in the pre-round tree was `target_pointer_width`-gated;
`MAP_ANON`/`MAP_HUGETLB`/`_SC_PAGESIZE` are all correct for 32-bit Linux). They now fail at
`compile_error!`. Meanwhile:

- `Cargo.toml`'s crates.io `description` still begins **"Cross-platform aligned anonymous virtual
  memory…"** and names only "Unix"/"Windows".
- `README.md` publishes **no supported-target list at all** — its only platform section is
  "Platform caveats", which is about `decommit` semantics, not target support.
- The CHANGELOG bullet describes the change accurately but carries **no BREAKING / platform-support
  tag**, and the round's summary text presents M2 as pure hardening.

The independent review's M2 remedy was explicit that both halves are required: *"If the project
intentionally supports only the enumerated 64-bit Unix targets, that restriction must be encoded in
`cfg`/compile-time guards **and publication docs**; the current code and 'Unix' marketing are
broader."* Only the first half landed. **Round 7's T6 already asked for the same artifact** — *"the
crate does not publish a supported-target list anywhere (`README.md`, `Cargo.toml`)"* — and task
#893 chose the in-source-comment option instead. The list still does not exist, and round 10 has now
made its absence load-bearing rather than cosmetic.

**Failure scenario (concrete, no exotic configuration):** a downstream crate depending on
`aligned-vmem = "0.2"` runs `cargo build --target armv7-unknown-linux-gnueabihf` in its own CI. The
build fails inside a transitive dependency with a message about `off_t`, on a target the crate's
crates.io page advertises as supported. There is no `docs.rs` page, README line, or CHANGELOG entry
that would have told them in advance.

**Two acceptable fixes; both are small.**
1. *(preferred — keeps the targets working)* Replace the blanket guard with the per-target `off_t`
   alias the independent review actually recommended first: `type OffT = i64;` everywhere except
   `all(target_pointer_width = "32", any(target_os = "linux", target_os = "android"))` → `i32`
   (32-bit BSD/Darwin keep a 64-bit `off_t`, so they stay `i64`). This is the honest ABI fix; the
   `compile_error!` then narrows to "any 32-bit Unix we have not classified."
2. *(acceptable — keeps the guard)* Keep the `compile_error!` and add, in the same commit: a
   "Supported targets" section to `README.md`, a 64-bit-Unix qualifier to `Cargo.toml`'s
   `description`, and a `**BREAKING (platform support)**` line to the round-10 CHANGELOG entry.

Doing neither before publishing 0.2.0 (task #658) ships an undisclosed platform-support regression.

### H2C2 — LOW — items 52 and 53 are `Status: CLOSED` but sit in the `[T] Tracked, not yet actioned` tier with their full closure narratives inline; item 52's card is also mis-indented

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:2148-2158` (items 52/53), vs the `### [T]` tier heading at
`:86` and `## Recently resolved` at `:2162`.

CLAUDE.md's R34-24 rule is explicit: *"A closed item that still sits in an active tier (`[A]`, `[D]`,
`[L]`, `[T]`) with no Status-card update is a structural defect — the round that closes it MUST
update the card to `Status: CLOSED` **and move the narrative** in the SAME commit"*, with the main
index keeping only a one-line pointer in "Recently resolved". Round 10 did the first half (both
cards say `CLOSED`) and not the second: both items sit inside `[T]`, above the `---` at `:2160`, with
their full multi-hundred-word closure narratives inline, and **neither appears in "Recently
resolved"**. A fresh round performing CLAUDE.md's mandatory "Round start: check BOTH open-items
indexes" read will encounter two closed items in the not-yet-actioned tier.

Second, mechanical, defect from the same merge: **item 52's four card bullets are indented 3 spaces**
(`:2149-2152`) where item 53's and every other item's are indented 4 (`:2155-2158`). For a `52. `
list marker CommonMark's content column is 4, so a 3-space-indented bullet is not nested inside item
52 — it starts a sibling list. The rendered index will show item 52's Status/Current/Next/Evidence
card detached from item 52.

Fix: move both narratives to "Recently resolved" leaving one-line pointers, and normalise item 52's
indentation to 4 spaces. ~10 minutes, in the file the next round is required to read first.

### H2C3 — LOW — M1's fix replaced one false completeness claim with a strictly broader false completeness claim

**Where:** `crates/vmem/src/lib.rs:750-756` (the new comment) vs the `# Safety` contract at
`:693-707`.

The defect M1 was filed for was, in the independent review's own words, that *"the constructor and
its comments promise to convert this exact documented-contract misuse into an immediate,
attributable failure and do not do so"* — the old comment claimed the `Layout` check "covers both
halves of the documented contract." The replacement comment now says the checks *"together …
**cover all documented contract violations** immediately at the call site."*

That is false, and it is a wider claim than the one it replaced. Of the eight predicates the
`# Safety` section documents, **four are checked** (`base` non-null, `reservation` non-null, `align`
power-of-two `>= PAGE`, `reservation_len` non-zero `PAGE`-multiple + `Layout`-valid) and **four are
not** — of which three are cheaply checkable with the values already in hand:

| Documented invariant | Checked? |
|---|---|
| `len` is a **non-zero multiple of `PAGE`** (`:700`) | **No** — trivially checkable |
| `base` is **aligned to `align`** (`:699`) | **No** — `base.addr() % align == 0` is one expression |
| `reservation_len >= len + (base - reservation)` (`:705`) | **No** — checkable with the five arguments |
| `base` valid for `len` bytes; reservation live/exclusive; released exactly once | No — genuinely uncheckable |

No unsoundness follows: this is an `unsafe fn`, so violating the contract is already UB and the
crate's own callers all satisfy it. The finding is that **the campaign's signature defect — a
comment asserting more completeness than the code delivers — was reproduced by the very fix that
was filed to remove an instance of it**, one round later, in the same function. The minimal fix is
to weaken the sentence to name what is and is not validated; the better fix is to add the three
cheap predicates and then the sentence becomes true.

### H2C4 — LOW — H1's fix silently removes a case that previously worked *correctly*, and this is recorded nowhere

**Where:** `crates/vmem/src/lib.rs:2476` (`flags |= MAP_HUGETLB | MAP_HUGE_2MB`); undocumented in
item 53, `CHANGELOG.md:475`, and `reserve_aligned_huge`'s rustdoc (`:1423-1462`).

H1's failure sequence required `reservation_len` to *not* be a multiple of the kernel's actual huge
page size. The complementary case — where it **is** — worked correctly before the fix and does not
work now. Concretely, on a `default_hugepagesz=1G` host with a provisioned 1 GiB pool and an empty
2 MiB pool:

- **Before:** `reserve_aligned_huge(1 GiB, 1 GiB)` passed the 2 MiB-multiple guard (1 GiB is a
  multiple of 2 MiB), `mmap(MAP_HUGETLB)` allocated a 1 GiB huge page, `reservation_len = 1 GiB`,
  and `munmap(ptr, 1 GiB)` **succeeded** — length was an exact multiple of the underlying huge page.
  The caller got a genuine 1 GiB huge page with `is_huge() == true` and no leak.
- **After:** the same call requests 2 MiB pages explicitly, the 2 MiB pool is empty, `mmap` fails,
  and the crate falls back to ordinary pages with `is_huge() == false`.

This is the right trade (fail-closed beats a leak, and the general case cannot be made safe without
either querying `Hugepagesize:` or recording the kernel-rounded length), but it is a **functional
narrowing**: the crate can no longer obtain huge pages of any size other than 2 MiB, on any host.
Item 53, the CHANGELOG bullet and the public `reserve_aligned_huge` rustdoc all describe the fix as
fail-closed hardening and none of them says this. A one-sentence note on each is the whole fix.

### H2C5 — INFO — H1's fix is silently kernel-version-dependent, and nothing detects when it is inert

**Where:** `crates/vmem/src/lib.rs:2329-2341` (the constant's doc).

The `MAP_HUGE_*` size encoding was introduced in **Linux 3.8** (2013). On older kernels the bits
above `MAP_HUGE_SHIFT` were not interpreted by the `MAP_HUGETLB` path; a kernel that ignores them
silently reverts `libc_mmap` to "use the system default huge-page size" — i.e. **H1's exact bug
returns, with no diagnostic**, on a kernel inside Rust's own nominal minimum-supported range.
Practically irrelevant (a 2026 crate on a pre-2013 kernel), but the constant's doc already carries a
careful REASONED-FROM-SPEC caveat and this is the one caveat it omits. Record-only; one sentence.

### H2C6 — LOW — the closest surviving structural analogue of H1: `WIN_ALLOCATION_GRANULARITY`'s premise is never checked at the point that depends on it

**Where:** `crates/vmem/src/lib.rs:1893` (the constant), `:1612` (the fast-path condition),
`:1655-1662` (the fast path's return), `:426-440` (the only guard).

This is the finding the §4 hunt was commissioned to look for, and it is the one entry in the whole
constant inventory (§5.2) that has H1's shape intact.

`WIN_ALLOCATION_GRANULARITY = 65536` is the **sole basis** for the Windows single-call fast path's
alignment contract: `if align <= WIN_ALLOCATION_GRANULARITY && commit_len == size`, take one
`VirtualAlloc(NULL, …, MEM_RESERVE | MEM_COMMIT, …)` and return its result as `base` — with **no
check that the returned base is actually `align`-aligned** (`:1655-1662` returns
`Ok((base, base, commit_len, …))` directly). The premise is the source comment at `:1587-1589`:
*"`VirtualAlloc(NULL, ...)` already returns a base aligned to `WIN_ALLOCATION_GRANULARITY` (64 KiB
on all supported Windows targets), so the alignment contract is satisfied by construction."*

The only thing in the crate that compares that constant against what the OS actually reports is a
`debug_assert!` at `:427-433` — and it is doubly inert for this purpose, as the code's own NOTE
immediately below it (`:435-440`) admits: it **compiles out of `--release`**, and it lives in
`query_os_page_size()`, *"a cold path (decommit/decommit_lazy) … It does NOT fire on the Windows
single-call reservation fast path, which uses `WIN_ALLOCATION_GRANULARITY` directly."* So the
constant's own premise is verified in exactly zero of the code paths that depend on it, in exactly
zero release builds.

**Why this is LOW and not MEDIUM/HIGH, stated honestly:** unlike H1's premise, this one is *true* on
every shipping Windows — `dwAllocationGranularity` is 64 KiB on every Windows NT release, on x86,
x64 and ARM64, and under Wine. Microsoft documents the value as **queryable**, not as fixed (which
is why `GetSystemInfo` reports it at all), but no counterexample exists today. The consequence if it
were ever wrong is nevertheless severe and silent: `reserve_aligned` would return a base violating
its documented alignment guarantee with no error, which for a downstream allocator doing
mask-based segment lookup is a memory-safety bug in *their* code.

**The asymmetry is the argument for fixing it.** Round 8's U1 (task #897) faced the *identical*
question on the Unix side — "can we skip the returned-address alignment check because the platform
guarantees it?" — and answered no, deliberately, making `try_reserve_aligned_exact`'s check
unconditional and a real runtime check rather than a `debug_assert!`, with a comment that spells out
the reasoning (`:2136-2152`): *"Deliberately a real runtime check, not a `debug_assert!`: release
builds are exactly where an unverified constant would matter (CLAUDE.md's R26-4 rule)."* The Windows
fast path is the same question, decided the opposite way, with no record of the decision being made.
**Fix:** one line in `win_reserve_commit`'s fast path — `if base.addr() % align != 0 { release and
fall through to the two-call path }` (or, minimally, the same unconditional check U1 chose). Cost is
one AND and one branch on a path already dominated by a syscall.

### H2C7 — INFO — the OS axis of M2's own question is still answered with a bare `E0425`; round 10 established the precedent that it should not be

**Where:** `crates/vmem/src/lib.rs:2302` / `:2306-2317` (the two `MAP_ANON` definitions).

`MAP_ANON` is defined for `target_os = "linux"` and for the eight Darwin/BSD targets, and for
nothing else. Verified by `rustc --print cfg`: `aarch64-linux-android`, `x86_64-unknown-illumos` and
`x86_64-pc-solaris` all set `unix` and **`target_pointer_width="64"`** — so M2's new guard does not
fire for them — and none matches either `MAP_ANON` `cfg`, so `libc_mmap` fails to compile with a
bare `error[E0425]: cannot find value MAP_ANON in this scope`. (Android is a Tier-2 target with real
users.) The catch-all `_SC_PAGESIZE = 30` for "Linux and most other unices" is wrong for
illumos/Solaris (11) and bionic, but is **unreachable** on those targets precisely because
compilation fails first — the two gaps cover each other, exactly as round 6 recorded.

This is **not new** — round 6 found it and round 7's T6 cited it — and it was correctly triaged
then as fail-closed and no-action. What is new is that **round 10 changed the precedent**: for the
32-bit axis, the campaign has now decided that "unsupported target" deserves an explicit
`compile_error!` naming the reason and the unblocking procedure. The OS axis, which is the *more
likely* one to be hit, still gets an unattributable `E0425`. If H2C1 is fixed via option 2 (publish
a supported-target list), extending the same `compile_error!` to unenumerated `cfg(unix)` OSes is
the natural same-commit companion and costs three lines.

### H2C8 — LOW — item 52's evidence citation was correct in its own worktree and stale on arrival in `main`, caused mechanically by the parallel-worktree merge order

**Where:** `docs/CORRECTNESS_OPEN_ITEMS.md:2152` — *"Evidence: task #911 commit,
`crates/vmem/src/lib.rs` **lines 2418-2447** (updated doc comment + new `compile_error!` + existing
extern block)."*

Verified at three points:

- `git show b1185fb:crates/vmem/src/lib.rs | grep -n "task #719: \`offset\`'s type hardcodes"` →
  **2418**. The citation was **correct** when written, in the `vmem-ir-c` worktree.
- `git show a1d75ec:crates/vmem/src/lib.rs | grep -n …` → **2418**. Correct at the round's base too.
- Current `HEAD`: the same comment begins at **2446** and its `compile_error!` closes at **2477**.
  Line **2418** at `HEAD` is `    ))]` inside `_SC_PAGESIZE`'s macOS `cfg` arm; line **2447** is the
  second line of the `task #719` comment. The citation now points at unrelated code, off by 28.

The cause is purely mechanical: `vmem-ir-a` (H1) and `vmem-ir-c` (M2) both branched from `a1d75ec`
and edited the same region of `lib.rs`; H1's merge landed **after** M2's (`806cb30` then `c3850e7`)
and inserted 28 net lines *above* M2's block. The item-52-vs-53 numbering collision was noticed and
resolved by hand; this second, invisible collision was not, because it is not a textual conflict —
git merged both hunks cleanly and only the *meaning* of a number in a third file broke.

**This matters far beyond the one stale number.** It is the campaign's single most frequent finding
class (F11, T5, TC4, U2, U3, U8, V2-2, V2C1, V2C3 — now H2C8), it is precisely the class round 9's
closing review argued *"a diff-scoped review structurally cannot catch, since the defect and the
commit that causes it are never in the same file region"*, and round 10 produced a fresh instance
**by construction** from its own delegation pattern. Round 9's closing review recommended building
the ~50-line citation resolver **before** adopting the pivot; round 10 adopted the pivot and did not
build it. That recommendation is now 2-for-2 as a prediction. See §5.4.

---

## 5. THE ANSWER — "we said there was nothing left to find, and then this batch got found. What else are we obligated to fix?"

### 5.1 Was round 9's PIVOT wrong? — No. Its *recommendation* was right; its *stated reason* was wrong, and the difference is the whole lesson.

Round 9's closing review (§"Evaluating the round-9 PIVOT recommendation") argued three things.
Reading them back against what actually happened:

| Round 9 claimed | Falsified by round 10? |
|---|---|
| **(a)** *"Three consecutive rounds of full re-reading have now found nothing in code that pre-dates the previous round's own remediation."* | **No — still true, and round 10 does not test it.** Round 10 did not re-run the established methodology. Nothing about H1 shows a tenth full read by the same reader would have found it; the point is that four separate passes by that reader had already looked directly at those lines and did not. |
| **(b)** *"End the scheduled full-crate reads"* and try something else. | **No — VINDICATED.** H1 exists in the record *because* the pivot was adopted. Had round 10 been a tenth full read, H1 would still be undiscovered. Round 10 is evidence **for** the pivot, not against it. |
| **(c)** the implicit premise underneath: *the remaining unknowns are blocked on infrastructure (CI runners), not on reading the code again.* | **YES — this is the part that was falsified.** H1 needed no runner, no CI row, no new tooling. It needed one reader who had not already agreed with the conclusion. |

So the user's framing — *"we said there's nothing left to find"* — is a half-step stronger than what
was actually said. The campaign said **"there is nothing left for *this reader* to find."** That was
a true statement about the instrument, and it was then treated as a statement about the code.

**The transferable lesson, stated as a rule:** *the convergence of a review process is evidence
about the reviewer, not about the artifact.* Nine rounds of one reader agreeing with itself carries
almost no marginal information after round three, because each round inherits the previous rounds'
**conclusions** as priors. That inheritance is exactly the mechanism that produced H1's nine-round
survival: task #714 established "the default huge page size is 2 MiB"; the round-1 rust-intel audit,
the round-1 closing review and the round-4 closing review each re-derived the *arithmetic over* that
constant (varying `align`, varying `size`, checking `over = size + align`'s huge-alignment) and each
concluded "correct" — because within the inherited premise it **was** correct. Nobody re-opened the
premise because it was already marked settled by a review they trusted.

The actionable form is therefore **not** "keep reading forever." It is:

> **A premise established once and re-confirmed N times by the same review lineage has been verified
> once, not N+1 times. Re-confirmation by a reviewer that inherited the premise is not independent
> evidence.**

That is a rule the campaign can apply without any new infrastructure, and it is the single most
valuable thing round 10 produced — worth more than the four fixes.

### 5.2 Is "no known HIGH/CRITICAL findings" credible for 0.2.0? — Yes as a statement about *knowledge*, no as a statement about the *code*. Here is the falsifiable version.

Commissioned by the round brief: a full sweep of every hard-coded constant and assumed platform
fact used as the basis of a safety-critical calculation (alignment, size, syscall argument), each
asked "**has the constant's own premise ever been checked, or only the arithmetic on top of it?**"
Inventory taken from `grep -nE '^\s*(pub )?const |^\s*static ' crates/vmem/src/lib.rs` plus the two
hand-written `extern` blocks and the `SYSTEM_INFO` mirror — i.e. exhaustive, not a sample.

| Constant / assumed platform fact | Premise it encodes | Premise ever checked against the OS? | Blast radius if wrong | Verdict |
|---|---|---|---|---|
| `PAGE = 1 << 12`, `MIN_PAGE` (`:157`,`:164`) | "no supported platform pages smaller than 4 KiB"; PAGE is a *minimum*, not the page size | **Yes, structurally.** `page_size()` queries the OS and its guard rejects `queried < PAGE` (`:400-406`); `try_reserve_aligned_exact`'s alignment check is unconditional and does **not** trust `page_size()` (task #897) | — | **SOUND — and the model.** This is what "premise verified, not just arithmetic" looks like in this crate. |
| `_SC_PAGESIZE` table: 29 Darwin / 47 FreeBSD+DragonFly / 28 NetBSD+OpenBSD / 30 else (`:2412-2444`) | each OS's own `sysconf` name-table index | **Partly.** Darwin (29) and Linux (30) are exercised by real CI rows. The **four BSD values never have been** — REASONED-FROM-SPEC, already tracked as **item 43** | wrong `page_size()` → poisoned decommit rounding | **KNOWN GAP, honestly labelled.** Bounded by the `page_size()` guard **and** by #897 removing the one place the value was load-bearing for alignment. |
| `MAP_ANON` `0x20` / `0x1000` (`:2302`,`:2317`) | `asm-generic` vs BSD values | Linux value exercised by real Linux CI; BSD values not. Arch-dependence (MIPS/Alpha/PA-RISC/Xtensa) documented in-source by #893 | wrong ⇒ `EBADF` ⇒ every reserve fails | Fails closed both ways. OS-axis gap = **H2C7**. |
| `MAP_HUGETLB = 0x40000` (`:2325`) | `asm-generic` value | **No** — no hugetlb host anywhere in CI | reserve fails / falls back | Same tracked gap H1 lived in; fail-closed. |
| `MAP_HUGE_2MB = 21 << 26` (`:2341`) | `MAP_HUGE_SHIFT = 26`; field = log2(bytes) | **Re-derived independently in this review (§2): correct, and architecture-independent.** Not empirically exercised | fix inert ⇒ H1 returns | Correct as written; kernel-version caveat = **H2C5**. |
| `LINUX_HUGE_PAGE_SIZE = 2 MiB` (`:2370`) | **was** "the system default is 2 MiB" (**FALSE**) → **now** "the crate explicitly requests 2 MiB" (**true by construction**) | Now correct *by construction* rather than by assumption | *(was: entire pinned mapping leaked)* | **FIXED — this round. The premise was replaced, not re-verified.** |
| `MADV_DONTNEED = 4` (all unix, `:2377`) | value 4 on Linux, Darwin and all four BSDs | Linux + Darwin exercised in CI. Alpha's divergent `6` is unreachable (no Rust target) | decommit no-ops | **SOUND.** |
| `MADV_FREE = 8` (Linux) / `MADV_FREE_REUSABLE = 7` (macOS/iOS) (`:2382`,`:2385`) | the Linux/Darwin split is real — Darwin's `8` is `MADV_FREE_REUSE`, a *different* call, and the code correctly does not use it | Darwin arm exercised by the real macOS CI row | wrong advice ⇒ no reclaim | **SOUND.** tvOS/watchOS deliberately fall through to `MADV_DONTNEED`. |
| `MADV_HUGEPAGE = 14` (`:2388`) | `asm-generic` value | No | no THP hint; return discarded | Zero blast radius. |
| `MAP_FAILED = usize::MAX` (`:2372`) | `(void *) -1` | Universal | — | **SOUND.** |
| Windows `MEM_COMMIT/RESERVE/DECOMMIT/RELEASE/LARGE_PAGES`, `PAGE_READWRITE` (`:1889-1904`) | `winnt.h` values | Exercised by the real Windows CI row and by this host every round | — | **SOUND.** |
| `SystemInfo` mirror of `SYSTEM_INFO` (`:1841-1866`) | field order/widths of a hand-written `#[repr(C)]` struct | **Re-derived in this review** against the Win32 layout (`WORD,WORD,DWORD,LPVOID,LPVOID,DWORD_PTR,DWORD,DWORD,DWORD,WORD,WORD`): matches on both 32- and 64-bit. `dw_page_size` read is exercised | garbage `page_size()` | **SOUND.** |
| `extern "system"` for `VirtualAlloc`/`VirtualFree`/`GetSystemInfo` (`:1840`) | stdcall on 32-bit Windows, C on 64-bit | Correct **by construction** — this is the one hand-written FFI block in the crate that gets the calling convention right, and notably it is *not* the bug M2 found on the Unix side | stack corruption on i686 | **SOUND** (checked because it is the natural sibling of M2). |
| `off_t == i64` for all `cfg(unix)` (`:2446-2477`) | POSIX `off_t` width | Was an unverified premise; round 10 **restricted the targets** instead of verifying it | ABI shape mismatch | Narrowed, not verified — and the narrowing is undisclosed: **H2C1**. |
| `WIN_ALLOCATION_GRANULARITY = 65536` (`:1893`) | "`VirtualAlloc(NULL, …)` always returns a ≥ 64 KiB-aligned base" — the **sole** basis of the single-call fast path's alignment contract | **NO — not at the point of use.** Only guard is a `debug_assert!` that compiles out of release **and** sits in a function the fast path never calls (the source says so) | silently misaligned `base` returned from `reserve_aligned` | **H2C6 — the one surviving entry with H1's exact shape.** |
| "the `cfg(unix)` surface == the enumerated OS list" (implicit) | that no other `cfg(unix)` target reaches this code | Fails closed via `E0425`; known since round 6 | build failure | **H2C7.** |

**So the credible statement for 0.2.0 is not "no known HIGH/CRITICAL findings."** It is:

> One HIGH was found by the first pass that used a different framing, and it was a *premise* defect,
> not an arithmetic defect. A subsequent exhaustive premise sweep (this document, §5.2) found
> **fifteen** constants/platform facts, of which **one** (`WIN_ALLOCATION_GRANULARITY`) still has
> H1's exact unverified-at-point-of-use shape (H2C6, LOW — the premise is true on every shipping
> Windows), **three** are REASONED-FROM-SPEC but structurally fail-closed and already tracked
> (the four BSD `_SC_PAGESIZE` values as item 43; `MAP_ANON`/`MAP_HUGETLB` on untested OSes and
> architectures), and the rest are either exercised by real CI rows or re-derived from primary
> sources in this review. **No further HIGH-severity defect was found by this sweep.**

That is falsifiable and auditable. "No known HIGH/CRITICAL" is neither, and after this round it
would be the same category of statement that let H1 survive.

### 5.3 Did round 10 reproduce the campaign's "round N's fix creates round N+1's finding" pattern? — Yes, twice; once mechanically.

- **H2C3** — M1's fix, filed *because* a comment overclaimed what was validated, ships a comment that
  overclaims what is validated, more broadly than before.
- **H2C8** — the M2 task wrote a correct line citation which the H1 merge silently invalidated. Not a
  judgement error at all: a structural consequence of merging two same-region worktrees, which is
  exactly the delegation pattern this campaign uses every round.
- **H2C1** — the strongest instance: M2's fix is itself the round's only *user-visible regression*.

Round 10's remediation is nonetheless the campaign's **best** on the merits: four correct fixes, one
of them a real HIGH, plus a merge conflict handled without dropping content. The pattern recurring
is not an argument against the round; it is an argument that the pattern is **structural** and needs
a mechanism, not more care. Which is exactly §5.4's first item.

### 5.4 What are you actually OBLIGATED to fix? — prioritised, with honest reasoning

**Tier 0 — before publishing 0.2.0 (task #658). These are obligations, not improvements.**

1. **H2C1 — resolve the 32-bit-Unix build break.** ~30 min. The `off_t` type-alias route (option 1)
   is preferred because it keeps Tier-1 `i686-unknown-linux-gnu` working *and* fixes the real ABI
   concern; the docs route (option 2) is acceptable if narrowing is deliberate. Doing neither ships
   an undisclosed platform-support regression under a "Cross-platform" description. **This is the
   only item on this list that can break a stranger's build.**
2. **Settle the `mock` feature-unification question.** *Not a finding of mine — it is in the
   independent review's own null-results section ("a documented non-additive feature-unification
   hazard that is especially worth settling before first publish") and in `Cargo.toml`'s own comment,
   which states the conversion to a `--cfg` flag "is therefore still free today and stays free only
   until 0.2.0 ships (task #658)."* It has been deferred every round. It is the one item on this
   entire list whose cost is **strictly increasing with time** and becomes permanent the moment
   0.2.0 is published: `mock` replaces the syscall backend, Cargo unifies features graph-wide, and
   after publication removing or converting it is a breaking change. Decide it — either convert to
   `--cfg`, or write down explicitly that the hazard is accepted for 0.2.0 and why.
3. **H2C2 + H2C8 — index hygiene.** ~15 min. Move items 52/53's narratives to "Recently resolved"
   with one-line pointers, normalise item 52's indentation, and replace its `lines 2418-2447`
   citation with symbol names (the fix round 8's U3 already established as this campaign's house
   style for exactly this reason). Cheap, and it is the file CLAUDE.md requires the *next* round to
   read first.

**Tier 1 — the honest remaining risk, in expected-value order.**

4. **Do another genuinely independent, fresh-context review — and aim it at PREMISES.** This is the
   highest-expected-value action available and costs one delegation. The track record is now a real
   (if small) number: **8 rounds of the established methodology produced 1 pre-existing code defect
   (U1, round 8); 1 independent pass produced 1 HIGH + 2 MEDIUM + 1 LOW.** That is roughly an
   order-of-magnitude difference in yield per unit effort, and the *mechanism* is understood (§5.1),
   not luck. Make the next one different again rather than a repeat: hand it §5.2's table and ask it
   to attack the rows marked not-verified, plus one axis round 10 never touched — `Drop`/unwind
   ordering and panic-safety across the `Reservation` lifecycle, and the `mock`/`fault-injection`
   feature-interaction surface. **Do not give it this campaign's conclusions.** That was the active
   ingredient.
5. **Build the ~50-line citation-resolver script.** Round 9's closing review made this a *precondition*
   of the pivot; round 10 adopted the pivot without it and produced H2C8 by construction in its very
   first round. The recommendation is 2-for-2 as a prediction (it would have caught U2/U3/U8 in
   round 8, V2-2 in round 9, V2C1/V2C3 at round 9's close, and H2C8 here). It is the only proposed
   mechanism that catches the campaign's most frequent defect class **at all** now that scheduled
   full reads have ended, and it is cheap enough for the per-PR path. **This is the single highest-
   value infrastructure item, and it is cheaper than every alternative on this list.**
6. **H2C6 — add the returned-base alignment check to `win_reserve_commit`'s fast path.** One line.
   It closes the last constant in the crate that still has H1's shape, and it makes the Windows
   fast path consistent with the decision round 8's U1 already made, deliberately and in writing,
   for the Unix one.
7. **H2C3, H2C4, H2C5, H2C7 — the doc/comment truth bundle.** All four are "make a claim match the
   code" edits; H2C3 optionally becomes a three-predicate code fix. Half an hour together, and they
   are the exact class of defect this campaign exists to prevent.
8. **Linux `bench-internals` real-backend CI row.** Unchanged priority from rounds 8 and 9 — still
   the highest-leverage *coverage* item (four beneficiaries, plus the Linux half of item 43).

**Tier 2 — worth doing, but do not let it block the publish, and do not over-value it.**

9. **A hugetlb-configured CI runner to empirically test H1's fix.** Honest assessment: **lower
   priority than its severity suggests.** H1's fix is fail-closed *by construction* — the residual
   risk it would retire is "huge pages are silently unavailable on some host", not "a mapping
   leaks." A ~90 %-of-the-value substitute needs **no special runner**: a Linux CI step that reads
   `/proc/meminfo`'s `Hugepagesize:` and `/sys/kernel/mm/hugepages/`, and asserts
   `reserve_aligned_huge(2 MiB, 2 MiB).is_huge()` agrees with what is actually provisioned,
   skipping cleanly when no pool exists. That also gives H2C4 an oracle.
10. **Item 41's crate-local miri step.** Unchanged: still blocked on the intentional-leak runner
    policy, still record-only.

**And, explicitly, what you are NOT obligated to do: another full-crate read using the established
methodology.** Round 9's evidence for stopping is unrefuted, and round 10 does not contradict it —
round 10 is what happens when you follow that recommendation, not a reason to reverse it. The
correct reading of this round is *"the pivot worked; the reason we gave for it was wrong; keep
pivoting, and stop treating any single reviewer's convergence as evidence about the code."*

---

## 6. Null results (checked, nothing found)

- **H1 regression sweep.** Behaviour on a 2 MiB-default host is byte-identical (explicitly
  requesting the default is a kernel no-op). The over-reserve path, the ordinary-page fallback, and
  the exact-path miss `munmap` were each re-derived for huge-alignment (§2) — all conformant.
  `granted_huge` bookkeeping (`HUGE_SUPPORTED && huge`) is unaffected. The only behaviour delta is
  H2C4, which is a documented-fallback outcome.
- **M1 regression sweep.** The sole in-repo consumer (`numa-shim`) proven to satisfy the new
  assertion by construction, plus 33 passing tests. No `from_raw_parts` call anywhere else in the
  workspace outside `crates/vmem/tests/`. The new assertion checks against `PAGE` (4 KiB minimum),
  not `page_size()`, so it stays correct on 16 KiB-page hosts.
- **L1 sweep.** All five call sites converted; no `-> bool` / `black_box(ok)` laundering remains in
  `benches/vmem_bench.rs`. `cargo bench --no-run` clean. Benches are compile-only in CI, so the
  panic-on-failure change carries no CI risk, as the task claimed.
- **Merge integrity.** `git diff a1d75ec..HEAD --stat` touches exactly the six files the five
  commits touch — no stray edit rode along on any of the four merges. No conflict-marker residue in
  `crates/vmem/`, `docs/CORRECTNESS_OPEN_ITEMS.md` or `CHANGELOG.md`. Items 52 and 53 both retain
  their full narratives and complete four-field cards; **nothing was dropped by the renumbering**.
- **CHANGELOG accuracy.** Every SHA cited in the round-10 entry (`c3850e7`, `da643f2`, `806cb30`,
  `3beb34a`) resolves to the merge it names; the "44/49" figure is exact; the `0x54000000`
  arithmetic is correct. The one omission is the platform-support narrowing (H2C1) and the
  huge-page capability narrowing (H2C4).
- **Windows FFI.** `extern "system"` (not `extern "C"`) — the 32-bit-Windows stdcall analogue of M2
  does **not** exist. `SystemInfo`'s field layout re-derived and matches on both pointer widths.
- **`_SC_PAGESIZE` catch-all.** The `30` fallback is unreachable on every non-Linux OS because
  `MAP_ANON` fails to compile there first (H2C7); on Linux, `30` is correct. No live hazard.
- **Round-10 CHANGELOG entry exists** — the first round in a five-round streak (6, 7, 8, 9 all
  needed their closing review to catch it) where the round's own closing task wrote it. Index item 1
  should record that the streak broke *favourably* here.
