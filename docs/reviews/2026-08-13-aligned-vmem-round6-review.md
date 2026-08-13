# `aligned-vmem` — round-6 readonly review (post-first-push, post-macOS-CI-discovery)

**Scope:** `crates/vmem/` in full (`src/lib.rs`, `src/error.rs`, `src/mock.rs`,
`src/fault_injection.rs`, `tests/*.rs`, `benches/vmem_bench.rs`,
`examples/v20_849_unix_exact_reserve_hit_rate.rs`, `Cargo.toml`, `README.md`),
the `.github/workflows/ci.yml` rows that touch this crate,
`scripts/vmem-doc-drift-guard.mjs`, and the two open-items indexes' entries for
this crate.

**Review type:** READ-ONLY. No file in the repository was modified by this
review (this document is written untracked, per the campaign's convention for
review reports). Everything below was executed on this host or read from the
real GitHub Actions API — no claim in this document rests on a prior round's
report.

**Base revision:** local `main` @ `9c777bc` ("fix(vmem): CI red on real macOS…").
`git fetch && git log origin/main..HEAD --oneline | wc -l` → **0** — nothing
unpushed; `origin/main` and local `main` are the same commit.

**Finding prefix:** `S` (sixth round). Prior prefixes in use and deliberately
not reused: `V`/`W`/`P` (rounds 1–2), `F` (round 3), `R`/`CR` (round 4 + its
closing review), `Q`/`QC` (round 5 + its closing review).

**Date:** 2026-08-13.

---

## Verdict up front

The round-5 closing review's own question — *"did round 5's own fixes introduce
the next round's findings?"* — has an unambiguous answer this round, and it is
not the usual one. Round 5's fixes are clean; the finding-generating commit is
the one that came **after** the round, `9c777bc`, the mitigation for the first
genuinely CI-discovered defect in this whole campaign.

Three things about `9c777bc` are true simultaneously:

1. It is the **right call** — restoring CI green by not asserting a guarantee
   that does not hold, rather than by weakening real Linux/Windows coverage, is
   exactly correct, and filing item 48 instead of rushing a `MAP_FIXED` re-map
   is the right sequencing.
2. Its mitigation is **incomplete in a way that matters for a crate that is
   about to be published**: it updated 3 sites and left at least 4 public
   statements of the now-known-false guarantee standing — including
   `recommit()`'s entire rustdoc, `decommit()`'s own opening sentence, the
   crate-root module doc, and every word of the README, which is the crates.io
   landing page (**S1**, **S5**).
3. Its stated **root cause is asserted, not established** — the CI evidence
   (`left: 119, right: 0`) is byte-identically consistent with a second
   hypothesis (the `madvise` call itself failing), and this crate discards
   `madvise`'s return value by documented design, so no evidence that
   distinguishes them exists anywhere (**S2**). The spec says the stated cause
   is almost certainly the right one; the *evidence* does not say so.

And the campaign's first lesson (R1: "the CI never actually ran the real
backend") has now recurred at a **third** depth. R1 was "the backend wasn't
run." This round's `9c777bc` was "the backend ran, but its behavior was never
verified." **S2/S4/S6** are the next layer down: *even now that it runs, the
crate has no oracle that can tell a working Darwin decommit from a completely
inert one* — the zero-fill assertion was the only effect-observing assertion in
the entire crate on any platform, and it is now `#[cfg]`'d off on the one
platform where the effect is in doubt.

One finding is not about `9c777bc` at all and is the one I would rank second:
**S3** — the macOS `MADV_DONTNEED` gap was **not** "previously undiscovered."
It is documented in this repository, in production code comments and two design
docs, since Round 9 — and, most pointedly, in `.github/workflows/ci.yml:810`,
three lines above the step that went red for exactly that reason. One of those
design docs even cites *`crates/vmem/src/lib.rs`'s decommit note* as its source
for the fact — a note that did not exist until `9c777bc` created it, 2026-08-13.

Performance: **still null**, sixth round running, and I checked freshly rather
than assuming. One idea (**S12**) is genuinely not covered by
`docs/perf/ALIGNED_VMEM_VIRTUALALLOC2_VA_OPTIMIZATION_OPPORTUNITY.md` and is the
only one that applies to a platform actually in CI — filed as INFO, unmeasured,
with an explicit "this is not a claim of a win."

**Publish readiness (task #658):** the macOS gap does not block publishing, but
**S5** should land first. Shipping 0.2.0 with a README whose headline feature
table says "Return page-granular physical backing to the OS" and mentions none
of the crate's three known platform divergences — Windows access-violation,
huge-page no-op, macOS zero-fill/RSS — is a documentation gap that gets much
more expensive to fix once external consumers exist.

---

## What was verified green — every command below was executed on this host

```
$ git fetch && git log origin/main..HEAD --oneline | wc -l
0                                          # nothing unpushed; HEAD == origin/main == 9c777bc

$ gh run list --commit 9c777bcb1a5d97a39ed3c2c391fffe3f3031d6e5
completed  success  CI                  main  push  31678423595  31m21s
completed  success  Kani verification   main  push  31678423571  40s

$ gh run view 31676133649 --json conclusion,headSha        # the run item 48 cites
{"conclusion":"failure","headSha":"e60e46a25b2a14cdfefb3cd09cd070d9f6cf895e", ...}
$ gh run view 31676133649 --json jobs -q '.jobs[]|select(.conclusion=="failure")|.name'
test macos (production)                    # the ONLY failing job — item 48's citation is exact

$ gh api repos/PHPCraftdream/sefer-alloc/actions/jobs/94378091599/logs | grep -i image
Image: macos-26-arm64                      # => 16 KiB pages, stable-aarch64-apple-darwin,
                                           #    rustc 1.97.1 (see S6)

$ cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast
fault_injection 5 / huge_pages 1 / lazy_commit 11 / min_page 2 / mock 0 / smoke 20 /
vmemerror_io_bridge 3 / doc-tests 0   => 42 passed, 0 failed        (Windows host)

$ git log --oneline -S "advisory" -- crates/vmem/src/lib.rs
9c777bc                                    # the ONLY commit — see S3
```

CI is green on the landing SHA (`9c777bc`), confirmed from the remote, not
assumed. Item 48's evidence citation (run `31676133649`, job
`test macos (production)`) is **verified accurate** — the run exists, its head
SHA is `e60e46a` as claimed, and macOS was the only failing job in it, which is
what makes the "Linux and Windows both passed the identical assertion" claim in
the commit message true as well.

---

## Category 1 — residue from `9c777bc` (angle 6): what the fix missed

### S1 — MEDIUM-HIGH — the macOS caveat landed on 3 sites; the two *most-read* public statements of the now-known-false guarantee were not among them, and one of them (`recommit()`'s entire rustdoc) is the single sentence a consumer of this API is most likely to read

`9c777bc` updated: `decommit()`'s rustdoc (a new paragraph at `lib.rs:995-1004`),
`recommit_pages_impl`'s code comment (`lib.rs:2057-2071`), and the test's `#[cfg]`
(`smoke.rs:184`). Four public statements of the same guarantee were left
untouched:

| # | Site | Text (verbatim) |
|---|---|---|
| a | `lib.rs:1064-1067` — **`recommit()`'s whole rustdoc** | "Recommit pages `[base + start, base + end)` previously passed to [`decommit`]. On Windows this re-commits physical pages (`VirtualAlloc(MEM_COMMIT)`); **on Unix re-access is implicit so this is a no-op.**" |
| b | `lib.rs:954-957` — **`decommit()`'s own opening sentence** | "…return their physical backing to the OS… **Re-access after decommit produces fresh zero-filled pages (after [`recommit`] on Windows; implicitly on Unix).**" |
| c | `lib.rs:32-34` — **the crate-root module doc** | "…plus page-granularity decommit/recommit so you can **return physical memory to the OS** while keeping the address-space reservation." |
| d | `lib.rs:1029-1031` — `decommit_lazy()`'s rustdoc | "**Unlike [`decommit`]**, on Linux the pages are NOT necessarily zeroed on next access…" — which reads as a positive assurance that `decommit` *is* zeroed on Unix. |

(b) is the sharpest: `decommit()`'s **first paragraph** states the guarantee
unconditionally, and the correction sits ~40 lines below it, after the `# Safety`
section, the Windows-divergence paragraph, and the huge-page paragraph. A
rustdoc summary line is what appears in the module index and in IDE hover; a
reader who never scrolls past `# Safety` sees only the false version. (a) is
worse still — `recommit()`'s rustdoc has **no** caveat anywhere in it, and
`recommit` is precisely the function a consumer calls when they are about to
rely on the pages being fresh.

**Failure scenario (concrete).** A consumer builds a page-recycling arena on
this crate: on region reuse it calls `decommit(base, lo, hi)` then
`recommit(base, lo, hi)` and hands the range to a `calloc`-shaped caller
*without* re-zeroing, justified by `decommit()`'s summary line and `recommit()`'s
"re-access is implicit" — both of which are what the docs say today. On Linux
and Windows this is correct. On macOS the caller receives the previous tenant's
bytes: a silent information-disclosure bug that no test in the consumer's suite
will catch unless it runs on Darwin *and* asserts on content. This is not a
hypothetical consumer shape — it is the exact shape `sefer-alloc`'s own
`virgin-zero-skip` design docs analyse at length (see S3), and the exact reason
they had to prove the dangerous path unreachable.

**Fix:** the shortest honest version is one cross-reference sentence in each of
(a)–(d) pointing at `decommit`'s macOS paragraph, plus rewording (b) to
"…(after [`recommit`] on Windows; implicitly on Linux — **see the macOS caveat
below**)". Do not delete the existing paragraph; it is well written where it is.

### S2 — MEDIUM — item 48's stated root cause is **asserted, not established**: the CI evidence is byte-identically consistent with a second hypothesis, and this crate structurally discards the one signal that would separate them

Item 48 and `9c777bc`'s message both state the root cause as: *`MADV_DONTNEED`
on Darwin is advisory-only for anonymous memory.* The entire evidence base is
one assertion failure: `left: 119, right: 0`.

Two hypotheses produce that byte-identical evidence:

- **H1 (stated):** `madvise(addr, len, 4)` succeeds and Darwin merely
  deactivates the pages without freeing them → contents survive.
- **H2 (never excluded):** the `madvise` call **fails** (returns `-1`) — e.g.
  the advice constant is wrong for this target, or the range is rejected — so
  nothing whatsoever happens → contents survive.

Nothing in the CI log, the test, or the crate can tell these apart, because
`libc_madvise` (`lib.rs:2331-2341`) discards the return value **by documented
design**: *"the return value is deliberately discarded — `madvise` failing here
means the OS did not reclaim the pages… not a memory-safety concern."* That
rationale is sound for production; it also means the crate has no channel
through which a Darwin-specific mechanism question can ever be answered.

I read the spec: Darwin's `sys/mman.h` does define `MADV_DONTNEED = 4`, and XNU
maps it to `VM_BEHAVIOR_DONTNEED`, which deactivates rather than frees — so H1
is very probably correct. That is a *spec* argument, identical in kind to the
several REASONED-FROM-SPEC claims this crate already labels as such
(`LINUX_HUGE_PAGE_SIZE`, the `_SC_PAGESIZE` table, `off_t`'s width). Item 48
does not label it that way; it presents H1 as confirmed by the CI run, which it
is not.

This matters beyond pedantry because item 48's **Next trigger** — implement
`mmap(MAP_FIXED)` re-mapping — is a real, unsafe-surface-adding change whose
justification rests entirely on H1. If H2 were the true cause, the correct fix
would be a one-token constant change, not a new `MAP_FIXED` code path.

**Failure scenario.** A future round implements the `MAP_FIXED` re-map on the
strength of item 48, adds genuine new unsafe surface (a fixed-address mapping
overlapping a live reservation, with all the aliasing analysis that needs), and
ships it — when the real defect was a failed syscall a one-line `-1` check would
have exposed. Cost: a whole review round spent on the wrong fix.

**Fix (cheap, and it also serves S4):** a `bench-internals`-gated fallible
variant of `libc_madvise` returning `Result<(), VmemError>`, plus a macOS-gated
test asserting the eager decommit's `madvise` returns 0. That single assertion
discriminates H1 from H2, on hardware that CI already has, and it is the
mechanism-activation oracle CLAUDE.md's own R26-4 → R30-8 rule lineage requires
before a verdict may rest on "which mechanism ran."

### S3 — MEDIUM — "previously-undiscovered" is **false at the repository level**: this exact hazard has been documented in-repo since Round 9 — including in the CI file that ran the failing job, three lines above the failing step — and one design doc cites a vmem "decommit note" as its source that did not exist until `9c777bc` created it

`9c777bc`'s message: *"a real, previously-undiscovered functional gap."* Item 48:
*"Discovered 2026-08-13."* Both are wrong about *discovery*; they are right only
about *this crate's own docs and tests*. Verified sites, all pre-existing:

| Site | Text |
|---|---|
| `.github/workflows/ci.yml:810-811` | "*The decommit path in isolation: **MADV_DONTNEED on Darwin is advisory/lazy (no zero-fill guarantee)**; these tests assert correctness holds regardless.*" — in the `test-macos` job itself, immediately above the `-p aligned-vmem` step that went red. |
| `src/alloc_core/alloc_core_small_pool.rs:1002-1021` | Production code comment: "*the exact macOS `MADV_DONTNEED`-is-advisory-and-lazy hazard…*", "*a subsequent recommit is not OS-zero-guaranteed on every backend (macOS/XNU/\*BSD `MADV_DONTNEED` is advisory + lazy, no zero-fill guarantee)*." |
| `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:115-116` | "*— **NOT guaranteed** on decommit-then-recommit on macOS/XNU/\*BSD (`MADV_DONTNEED` is advisory+lazy, no zero-fill — **`crates/vmem/src/lib.rs` §decommit note**)*." |
| `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:358` (§6 rebuttal table, row #2) | The entire safety argument for `virgin-zero-skip` is built on this fact. |
| `docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md:32` | Same fact, restated. |

The R9_5 citation is itself a **mis-citation of exactly the class rounds 4 and 5
kept finding** (CR4, Q5, QC6): it points at a `crates/vmem/src/lib.rs` "decommit
note" as the authority for the macOS non-guarantee.
`git log --oneline -S "advisory" -- crates/vmem/src/lib.rs` returns exactly one
commit — `9c777bc`, dated 2026-08-13. The note R9_5 cited never existed until
this week; the crate's docs asserted the *opposite* the whole time.

**Consequences, in order of importance:**

1. **The extraction lost a known fact.** When this crate was carved out of
   `src/alloc_core/os.rs`, the repo already knew Darwin decommit is not
   zero-guaranteed, and the extracted crate shipped a doc guarantee contradicting
   it. That is the actual root-cause story, and it is more useful to a future
   reader than "CI found it."
2. **A live design doc is mis-cited.** R9_5's macOS argument now points at a note
   that says something narrower than R9_5 needs (it covers `decommit`, not
   `decommit_lazy`, and says nothing about `*BSD`). Whoever revisits
   `virgin-zero-skip` will chase it.
3. **Item 48 should record the pre-existing knowledge** so a future round
   doesn't re-derive it, and so the honest framing ("known repo-wide since R9;
   never propagated into the extracted crate's own docs or tests; CI finally
   made it fail loudly") replaces "newly discovered."

**Re-verified, not assumed:** the root crate's own safety argument still holds
today. `decommit_empty_segment_impl(.., release_follows = false)` — the only
path that decommits a payload while leaving the segment registered, i.e. the
macOS-dangerous state — still has exactly one caller,
`dbg_force_decommit_retain_for` (`alloc_core_small_pool.rs:845-857`), gated
`#[cfg(feature = "internals")]` + `#[cfg(all(feature = "alloc-decommit",
feature = "bench-internals"))]` and `pub unsafe fn`. Not a production caller. So
this is a documentation/attribution finding, **not** a live soundness bug in
`sefer-alloc`.

### S4 — MEDIUM — the fix removed macOS's **only** decommit oracle: after `9c777bc`, no test on any platform can distinguish a working Darwin decommit from a completely inert one

Before `9c777bc`, `smoke.rs`'s zero-fill assertion was the sole assertion in the
whole crate that observed a decommit having *any* effect on the OS. Everything
else asserts only that a subsequent write/read works — which is true whether
`madvise` ran, failed, or was never called. `9c777bc` correctly scoped that
assertion off macOS. The consequence, not stated anywhere in the commit or item
48: **macOS now has zero effect-observing coverage of either decommit variant.**

Specifically:

- `decommit_recommit_roundtrip` (`smoke.rs:149-191`) — the zero-fill assert is
  `#[cfg(not(any(miri, feature = "mock", target_os = "macos")))]`. On macOS the
  test now asserts only `recommit(...) == true`, which is `Ok(())` unconditionally
  on Unix (`recommit_pages_impl` is a bare `Ok(())`). The test is **vacuous** on
  macOS.
- `decommit_lazy_roundtrip` (`smoke.rs:307-324`) — writes, calls `decommit_lazy`,
  recommits, writes again, reads back. Passes identically whether
  `madvise(MADV_FREE_REUSABLE)` succeeded, returned `EINVAL`, or was never
  compiled in. Vacuous on **every** platform, not just macOS.

**Failure scenario.** A future refactor mis-cfgs the Darwin arm of
`madv_free_advice()`, or a constant is renumbered, or someone "simplifies"
`decommit_pages_impl` to skip `madvise` on non-Linux Unix "since it's advisory
anyway." All three CI platforms stay green; the crate silently stops issuing any
reclaim syscall on Darwin; nothing in this repository notices, because the last
assertion that could have noticed was scoped off in `9c777bc`.

**Fix:** the S2 fallible-`madvise` wrapper doubles as this oracle. Minimum
viable: on macOS assert both variants' `madvise` returns 0 (proves the call is
issued and accepted, which is the whole remaining claim on that platform).
Stronger, if a footprint API is acceptable: assert `phys_footprint` drops after
`decommit_lazy` — but the return-code check alone closes the regression hole.

### S7 — LOW — the caveat is scoped `target_os = "macos"`, but the behavior is **Darwin-family-wide**, and this crate's own cfg lists claim `ios`/`tvos`/`watchos` as supported targets

`smoke.rs:184`'s exclusion is `target_os = "macos"` only, and `decommit()`'s new
caveat paragraph says "macOS" throughout. But:

- `lib.rs:2116-2118` — `madv_free_advice()` already treats `macos` and `ios`
  **identically** (both get `MADV_FREE_REUSABLE`), i.e. the crate already models
  them as one XNU family;
- `lib.rs:2134-2148` (`MAP_ANON = 0x1000`) and `lib.rs:2217-2225`
  (`_SC_PAGESIZE = 29`) both enumerate `macos`, `ios`, `tvos`, `watchos` as
  supported;
- XNU's `MADV_DONTNEED` semantics are the same on all four.

So the mitigation asserts a false guarantee on three targets the crate itself
claims to support, and the doc caveat under-scopes the limitation for anyone
building for iOS. Failure scenario: an iOS consumer reads `decommit()`'s rustdoc,
sees a caveat explicitly narrowed to "macOS specifically… for ORDINARY
(non-huge) reservations on macOS", concludes iOS is unaffected, and ships the S1
scenario on the platform where physical-footprint accounting matters most.

**Fix:** widen the caveat's wording to "Darwin (macOS/iOS/tvOS/watchOS)" and the
test's `#[cfg]` to the same `any(...)` list the crate already uses twice.

---

## Category 2 — publish readiness (angle 7)

### S5 — MEDIUM, publish-relevant — the README documents **none** of the crate's three known platform divergences; row 48 of the API table still promises the macOS behavior that item 48 says does not happen

`crates/vmem/README.md` is the crates.io landing page and the GitHub front page.
Its full statement of decommit semantics is one table row (`README.md:48`):

> `decommit(base, start, end)` / `recommit(base, start, end)` (unsafe) | Return
> page-granular physical backing to the OS / re-commit it.

The rustdoc treats **three** divergences as load-bearing enough to spend
paragraphs on. The README mentions none of them:

1. **Windows** — a write to a decommitted range before `recommit` is a hard
   `STATUS_ACCESS_VIOLATION`, not a soft re-fault (`lib.rs:976-982`; this one has
   *already crashed an in-repo consumer*, `docs/CORRECTNESS_OPEN_ITEMS.md` item 6).
2. **Huge pages** — decommit is a silent no-op on any `is_huge()` reservation, on
   both Windows and Linux (`lib.rs:984-993`).
3. **macOS** — no zero-fill, no RSS return (`lib.rs:995-1004`, new this week).

The README *does* spend a full paragraph each on the `huge-pages` alignment
rules, the `mock` feature-unification hazard, and the `recommit`/`commit_range`
contract-violation change — so the omission is not a length-budget decision; it
is a gap.

**Failure scenario.** 0.2.0 publishes. A downstream consumer evaluates the crate
from crates.io, reads "Return page-granular physical backing to the OS", adopts
it for exactly that property, and discovers on their macOS CI (or, worse, in
production RSS graphs) that it does not happen. The rustdoc caveat exists, but a
caveat in the 40th paragraph of one function's docs is not where an evaluation
decision gets made. Post-publish, fixing this needs a version bump and a
"correction" note; pre-publish it is a README edit.

**Fix:** one `## Platform caveats` section listing all three, with links to the
functions' own rustdoc. That is the whole change. Cargo.toml's `description`
(shown in crates.io search results) is acceptable as-is at that length.

---

## Category 3 — platform assumptions that real CI now *could* verify but still doesn't (angle 1)

### S6 — LOW-MEDIUM — `docs/CORRECTNESS_OPEN_ITEMS.md` item 43's macOS half is marked "verified" on the strength of a Windows host, its trigger has now partially fired (a real Darwin runner exists and runs this crate), and **no test in the crate can tell a correct `_SC_PAGESIZE` from a wrong one**

Item 43 tracks the per-OS `_SC_PAGESIZE` table (`lib.rs:2215-2248`) as
REASONED-FROM-SPEC for 4 of 6 targets. Its macOS entry says: *"macOS family = 29
(verified — this session ran on/cross-compiled for a Darwin-adjacent config)."*
That session ran on Windows; "Darwin-adjacent config" is a cross-compile, which
proves compilation, not the constant.

Item 43's own **Next trigger** names the exact weakness: a wrong constant is
*"hard to notice without an explicit assertion against the OS's own reported
value… not just checking the result is a power of two."* That is precisely what
the crate does today:

- `page_size()` (`lib.rs:343-347`) **silently swallows** any implausible answer:
  `if queried >= PAGE && queried.is_power_of_two() { queried } else { PAGE }`.
  A wrong `_SC_PAGESIZE` returning `-1`/`0`/garbage falls back to 4 KiB with no
  signal.
- `page_size_is_a_valid_os_page` (`smoke.rs:278-290`) asserts only
  `is_power_of_two()`, `>= PAGE`, and caching. **The fallback value passes every
  one of those assertions.**

Meanwhile the hardware now exists and is exact: I pulled the macOS job log from
the API — the runner image is **`macos-26-arm64`**, toolchain
`stable-aarch64-apple-darwin`, rustc 1.97.1. On aarch64 Darwin the page size is
architecturally **16 KiB**. So `page_size() == 16384` is a hard,
platform-guaranteed expected value on the runner CI already pays for.

**Failure scenario (today, undetected).** If `_SC_PAGESIZE = 29` were wrong for
Darwin, `page_size()` returns 4096 on the 16 KiB-page runner. Every macOS
`decommit(base, start, end)` whose offsets are 4 KiB-but-not-16 KiB-aligned then
passes the crate's own validation and is handed to `madvise`, which rejects the
*entire* call (all-or-nothing, as `lib.rs:322-325` itself documents) — a 100%
silent decommit failure on macOS, and every test in the suite still passes,
because none of them observes an effect (see S4).

**Fix (3 lines, closes item 43's macOS half on existing hardware):**

```rust
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn apple_silicon_page_size_is_16_kib() { assert_eq!(page_size(), 16 * 1024); }
```

This is also the round's answer to "should item 43 be revisited now that macOS
CI is real?" — **yes, and nobody did when task #867/R1 added the row.** Neither
open-items index was touched by rounds 4–6 for item 43.

### S8 — LOW — `reservation_len()`'s doc names the Windows fast path as "the one path in the crate" that under-reports; on the 16 KiB-page macOS runner, **every** reservation whose `size` is not a 16 KiB multiple under-reports too

`reservation_len()`'s rustdoc (`lib.rs:492-501`) documents exactly one exception:
the Windows single-call fast path, where Windows rounds VA reservations up to
64 KiB internally while the field reports `commit_len`. Round 5's closing review
added a test for it (QC8, `smoke.rs:65-83`) whose comment states: *"This is the
one path in the crate where `reservation_len()` deliberately does NOT report the
true reservation size."*

That claim is false on the macOS CI runner. `mmap` rounds `length` up to the page
size, so on aarch64 Darwin `reserve_aligned(PAGE, PAGE)` maps a full **16 KiB**
page while `reservation_len()` reports **4096**. Identical shape, second
instance, undocumented. (It would also arise on a 64 KiB-page Linux, a
configuration `lib.rs:151` already names.)

**Impact: documentation only.** `release_reservation` → `munmap(ptr, 4096)`; the
kernel rounds the length up and unmaps the whole page, so there is no leak and no
unsoundness — I checked this specifically before filing it as LOW. The defect is
that a reader (or a future round auditing VA accounting) is told there is exactly
one such path when there are at least two, and the newest test in the file
asserts the wrong universal.

### S9 — LOW — on Darwin the crate implements **half** of the `MADV_FREE_REUSABLE` protocol, and the eager/lazy cost ordering its docs describe is inverted there — which also means item 48's "Next trigger" never weighs the cheaper candidate fix

Three connected observations about `decommit_lazy` on Darwin
(`lib.rs:2111-2124`, `lib.rs:2187-2189`):

1. **Half a protocol.** Apple documents `MADV_FREE_REUSABLE` as one half of a
   pair: the range is removed from the process's physical footprint, and the
   application is expected to issue `MADV_FREE_REUSE` before touching it again so
   the pages are added back. This crate issues `MADV_FREE_REUSABLE` and nothing
   ever issues `MADV_FREE_REUSE` — `recommit_pages_impl` is an unconditional
   `Ok(())` on all Unix. Effect (REASONED-FROM-SPEC, **not** verified on
   hardware): physical-footprint accounting drift, not memory unsafety. On iOS,
   where jetsam decisions read that ledger, accounting drift is not purely
   cosmetic.
2. **The documented ordering is inverted on Darwin.** `decommit_lazy`'s rustdoc
   (`lib.rs:1024-1033`) says it is "*cheaper than [`decommit`]*" and that "*the
   kernel takes pages only under pressure*" — Linux `MADV_FREE` semantics. On
   Darwin, `MADV_FREE_REUSABLE` drops the footprint **immediately**, while
   `decommit`'s `MADV_DONTNEED` (per item 48) drops nothing at all. So on macOS
   the "lazy" call is the one that actually returns memory and the "eager" one is
   the no-op — the exact opposite of what both functions' docs describe.
3. **Item 48's Next trigger is therefore incomplete.** It names one candidate
   fix (`mmap(MAP_FIXED | MAP_ANONYMOUS)` re-map) and weighs no alternative.
   Given (2), there is a materially cheaper candidate worth *recording* (not
   adopting sight-unseen): route Darwin's eager `decommit` to
   `MADV_FREE_REUSABLE` and issue `MADV_FREE_REUSE` from `recommit`. **Be
   precise about what that buys:** it would close the *"return physical backing
   to the OS"* half of the promise on Darwin; it would **not** close the
   *"reads as zero"* half, because `MADV_FREE_REUSABLE` preserves contents when
   the pages are re-touched before reclaim. Only re-mapping gives both. A future
   round should weigh "cheap fix, half the promise" against "expensive fix, whole
   promise" — item 48 currently records only the second option's existence.

All of (1)–(3) are spec-reads, flagged as such, and none is verified on Darwin
hardware — which is itself S4's point.

---

## Category 4 — tooling residue

### S10 — LOW — the doc-drift guard's own header justifies stripping inline code spans with a reason that argues for the **opposite**, and the stripping creates a false-positive class that convicts *correctly qualified* sentences

QC2's fix made `SCOPE` strip inline code spans before testing
(`scripts/vmem-doc-drift-guard.mjs:171-180`), so a Rust signature's `-> T` arrow
can no longer "rescue" a README table row. That part is right and closes QC2.

The header comment justifying it says:

> *"TRIGGER and HARD_FAIL stay on the full sentence — this crate's real
> qualifiers (`align <= 64 KiB`) DO legitimately live inside backticks in
> rustdoc, so only SCOPE strips code spans."*

The premise ("real qualifiers live inside backticks") is an argument for **not**
stripping code spans from `SCOPE` — it is stated as if it were the reason for
doing so. And the code's actual behavior matches the premise's warning, not its
conclusion. Executed counterfactual, using the guard's own three regexes verbatim
(`node -e`, no file modified):

```
"Over-reserves `size + align` for `align > 64 KiB`."          -> violation: true
"The Windows backend over-reserves for `align > 64 KiB`."     -> violation: true
"Over-reserves `size + align` when align is large."           -> violation: false
```

The first two are *correctly qualified* sentences — they name the exact
condition — and the guard convicts them, because their only qualifier lives
inside backticks and `SCOPE` deleted it. The third is vaguer and passes.

**Failure scenario.** Round 7 rewrites the Windows dispatch sentence into the
precise backticked form (which is what Q2/QC3 have been pushing these sentences
toward for two rounds), `npm run check` goes red, and the author "fixes" the
guard by making the prose *less* precise — the guard actively steering text away
from the shape the campaign wants. Fails closed, so LOW, but the rationale in the
header will mislead whoever hits it.

**Fix:** either keep code spans for `SCOPE` and instead strip only Rust arrow
tokens (`->`, `=>`) plus generic brackets, or leave the behavior and correct the
header to state the real trade-off ("code spans are stripped because README table
arrows falsely satisfy `<`/`>`; the cost is that a sentence whose ONLY qualifier
is a backticked comparison is a false positive — write at least one unbacked
qualifier word").

### S11 — INFO — `9c777bc` has no CHANGELOG entry; fourth consecutive round with this exact shape

`git log` shows `9c777bc` touching three files: `lib.rs`, `smoke.rs`,
`docs/CORRECTNESS_OPEN_ITEMS.md`. `CHANGELOG.md` is untouched, and no entry
mentions the macOS gap (`grep -n "9c777bc" CHANGELOG.md` → nothing).

This is the same finding as R12 (round 3's tasks), CR10 (round 4's), and QC9
(round 5's) — the last of which explicitly closed itself with *"caught by the
closing review and closed in this same pass rather than aging into round 6."* It
then aged into round 6 anyway, one commit later. Worth noting that the
`docs/CORRECTNESS_OPEN_ITEMS.md` half **was** done correctly and promptly (item
48 was filed in the same commit as the fix, exactly as CLAUDE.md requires) — it
is only the CHANGELOG half that is missing, and this is the campaign's first
CI-discovered defect, which is precisely the kind of entry a reader of the
changelog would most want.

---

## Category 5 — performance (angle 2)

### S12 — INFO (perf opportunity, REASONED-FROM-SPEC, unmeasured, NOT a claim of a win) — the one idea not covered by the existing design note applies to the two platforms that *are* in CI: an aligned, non-`MAP_FIXED` `mmap` **hint** on the exact-path retry

Sixth round, and the honest headline is still **null**: nothing in this crate is
on `sefer-alloc`'s allocation hot path (reservation is a per-segment cold path;
R32-13 measured reserve+commit at ~4.3–4.8% of the Windows segment lifecycle),
`page_size()` is a cached relaxed load, and the `bench-internals` counters
compile out entirely. I re-read every path rather than assuming.

The one thing I could not find covered anywhere:
`docs/perf/ALIGNED_VMEM_VIRTUALALLOC2_VA_OPTIMIZATION_OPPORTUNITY.md` covers
Windows `VirtualAlloc2` and, as a separate note, BSD `MAP_ALIGNED(n)` — the
former on a platform in CI, the latter on four that are not. **Neither covers
Linux or macOS**, where item 46's own measured *hit* rates are 34.4% at 64 KiB /
46.7% at 1 MiB / 56.7% at 4 MiB (i.e. a 43–66% **miss** rate outside the
page-size regime), and where a miss costs **3 syscalls** (`mmap(size)` → `munmap` → `mmap(size + align)`) plus a
permanent `align` bytes of VA held for the reservation's lifetime.

Not considered anywhere: on the miss, before falling back to over-reserve, retry
`mmap(align_up(p, align), size, …)` with a **non-`MAP_FIXED` hint**. Both Linux
and Darwin honour a hint address when the range is free and silently ignore it
otherwise, so the retry is sound by construction (the existing alignment check
already validates the result). Cost on a retry-hit: still 3 syscalls, but the
mapping is exact-size — **no permanent `align`-byte VA overhead**. Cost on a
retry-miss: one extra `mmap`/`munmap` pair before the existing fallback.

The crate is unusually well set up to settle this cheaply: `UNIX_EXACT_RESERVE_ATTEMPTS`
/ `UNIX_EXACT_RESERVE_HITS` already exist, and
`examples/v20_849_unix_exact_reserve_hit_rate.rs` already runs the
30-independent-process methodology item 46 established. A third counter for
"hint retry hit" would produce a real number for the same harness cost.

Explicitly: **this is a spec-read, not a measurement, and not a recommendation
to implement.** Item 46 already says the fast-path question needs bare-metal
Linux remeasurement before any decision; this note just records a mechanism that
measurement should include, since the existing design note's two mechanisms both
apply to platforms other than the ones item 46 measures.

---

## Checked and explicitly NOT findings

Recorded so round 7 does not re-derive them.

- **`recommit`/`commit_range` validate against `PAGE` while `decommit` validates
  against `page_size()`.** Asymmetric, and on the 16 KiB-page macOS runner
  `recommit(base, PAGE, 2*PAGE)` returns `true` for a range `decommit` would
  silently skip. **No reachable failure:** `recommit` is a bare `Ok(())` on all
  Unix, and Windows' page size is 4 KiB on every supported target (x86_64 and
  ARM64 alike; WoA's 64 KiB figure is the *allocation granularity*, already
  handled separately by `WIN_ALLOCATION_GRANULARITY`). The asymmetry is
  deliberate and documented with its rationale in `README.md:100-106`. Not a
  finding today; would become one only on a platform with both >4 KiB pages and
  real commit semantics, which does not exist in the supported set.
- **`MAP_ANON = 0x1000` on Darwin is now genuinely hardware-verified.** A wrong
  value would make `mmap(…, fd = -1)` fail `EBADF`, so every macOS test would
  fail, not just the decommit one. The macOS CI row therefore *does* verify this
  constant — a real (if narrow) win from the first push, worth stating alongside
  S6's negative result for `_SC_PAGESIZE`.
- **Windows two-call path returning `granted_huge = true`.** `win_reserve_commit`
  sets `granted_huge = extra_commit_flags != 0` on the two-call success path
  (`lib.rs:1626`), which would be wrong if `MEM_COMMIT | MEM_LARGE_PAGES` ever
  succeeded on a pre-reserved region. Per MSDN large pages require
  `MEM_RESERVE | MEM_COMMIT | MEM_LARGE_PAGES` in a single call, so the branch is
  unreachable — the code says so (`lib.rs:1621-1625`), and
  `huge_pages.rs:61-62`'s `assert!(!r.is_huge())` passes on the real Windows CI
  row, which is a live (if indirect) check. Documented-but-not-enforced, as the
  comment says; no change warranted.
- **The crate does not compile on `unix` targets outside its `MAP_ANON` cfg list**
  (Android, Solaris/illumos, Haiku): `MAP_ANON` is simply undefined there, so
  `libc_mmap` fails to compile (`E0425`). This is a *hard* failure, not a silent
  wrong-`_SC_PAGESIZE` path — the `_SC_PAGESIZE` fallback comment's "Linux and
  most other unices use 30" is wrong for Android (bionic uses a different value)
  and Solaris, but those targets can never reach it. The two gaps happen to cover
  each other. No action; noted so it is not re-discovered as a soundness issue.
- **`benches/vmem_bench.rs` decommit workloads on a 16 KiB-page host.**
  `RESERVE_SIZE = 64 KiB` and the benches decommit `[0, len)`, both multiples of
  16 KiB, so nothing silently no-ops on Apple Silicon. (Benches are not run in
  CI, but this would have been a real measurement-validity defect if the size had
  been, say, 4 KiB.)
- **`src/error.rs`, `src/mock.rs`, `src/fault_injection.rs`** — read in full, no
  new findings. `fault_injection`'s third, unhandled disarm/re-arm race is
  already disclosed in its own module doc (task #776/F15) with an accurate scope
  statement; `mock`'s partial-backend-replacement shape and feature-unification
  hazard are documented at three sites and are QC1's already-settled decision.
- **Structure / CLAUDE.md conventions** — no inline `#[cfg(test)] mod tests`
  anywhere in `src/`, zero runnable doctests (`cargo test` reports
  `Doc-tests aligned_vmem … 0 passed`; both illustrative examples use ```` ```text ````
  fences), no `mod.rs`, every `unsafe` site carries `// SAFETY:` and every
  `unsafe fn` a `# Safety` section. The four-files-vs-"single-file seam crate"
  point is R13's, already settled.
- **Item 48's CI-run citation** — verified exact against the GitHub API (see
  "What was verified green"). This is the one part of item 48 I tried hardest to
  break and could not.

---

## Categories with nothing to report

- **Memory safety / UB.** No new `unsafe` surface since round 5; no safe `pub fn`
  taking a raw pointer and touching allocator metadata (CLAUDE.md's
  benchmark-hook rule); provenance discipline (`.addr()`/`.with_addr()`) intact
  on both backends' aligned-base derivations.
- **Error contracts.** `VmemError`'s three-way classification, the `io::Error`
  bridge, and the `invalid_argument` vs `os_refusal_unknown_code` distinction are
  all consistent between code, docs, and tests.
- **Semver / API surface.** No public item changed in `9c777bc`. The
  `#[non_exhaustive]` decisions, the `is_empty` deprecation, and the
  `ReservationParts` shape are all round-4/5 settled ground.
- **`lazy-commit` / `fault-injection` / `huge-pages` feature paths.** Re-read;
  nothing beyond what rounds 4–5 already closed.

---

## Recommended order

1. **S1** — add the macOS cross-reference to `recommit()`'s rustdoc,
   `decommit()`'s opening sentence, the module doc, and `decommit_lazy`'s
   contrast sentence. Pure text, no behavior, closes the highest-consequence gap.
2. **S5** — README `## Platform caveats` section (all three divergences).
   **Before publishing 0.2.0**, not after.
3. **S2 + S4 together** — one `bench-internals`-gated fallible `madvise` wrapper
   plus macOS-gated assertions. Settles item 48's root cause *and* restores an
   effect oracle on the platform that just lost one. One task, two findings.
4. **S3** — correct item 48's framing, record the pre-existing repo knowledge and
   the R9_5 mis-citation.
5. **S6** — the 3-line Apple-Silicon page-size assertion; update item 43's card.
6. **S7**, **S9**, **S8** — Darwin-family scoping, the `MADV_FREE_REUSE` /
   inversion note on item 48, the `reservation_len()` doc correction.
7. **S10**, **S11** — guard header, CHANGELOG entry.
8. **S12** — record only; do not implement. Fold into item 46's next-trigger
   text so a bare-metal remeasurement covers it.

---

## On "is round 6 padding?" — an honest answer

Five rounds have combed this crate, so the prior on a sixth finding anything
real was low. What made this round non-vacuous is that the *world changed
between rounds*: the first push finally ran the real backends on real hardware,
and that both produced a genuine defect and shifted what is verifiable.

The findings split cleanly into three groups, and I would defend the first two
as load-bearing and the third as ordinary hygiene:

- **Genuinely new, caused by the newest commit:** S1, S2, S4, S7 — all four are
  "the fix that closed round 5's era opened these," the campaign's signature
  pattern, at its most literal.
- **Genuinely new, revealed by the new hardware:** S3 (the fact was known
  repo-wide; only the crate's own docs were wrong), S6 and S8 (an assumption and
  a doc claim that only a 16 KiB-page runner can falsify — and one now exists).
- **Hygiene:** S9, S10, S11, S12.

The one I would not want lost in the list is **S2**. Every prior round of this
campaign, and three separate CLAUDE.md rules (R26-4, R30-8, and the entry-point
rule), exist because a verdict was published on evidence that could not actually
distinguish the stated mechanism from an alternative. Item 48 is that shape
again: a correct-looking conclusion, a real failure, and no evidence that
separates "Darwin's MADV_DONTNEED is advisory" from "the madvise call failed" —
in a crate that discards the return code by design. The spec says the conclusion
is right. The evidence does not, and item 48 says it does.
