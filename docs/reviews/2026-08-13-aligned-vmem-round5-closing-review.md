# `aligned-vmem` — round-5 CLOSING review (verification of the Q1–Q8 remediation)

**Date:** 2026-08-13
**Scope:** verification of the five remediation tasks (#875–#879, letters A–E) that closed
`docs/reviews/2026-08-13-aligned-vmem-round5-review.md`'s findings Q1–Q8 (Q9 required no action),
plus the two defects the orchestrating agent personally found and fixed during zero-trust review on
top of the `/crush` delegates' own commits, plus the one merge conflict that resolution produced.
Every file in the round's diff (`git diff 7c6e4be..HEAD --stat`: 7 files, +226/−128) and the code
each of those changes makes a claim about.
**Reviewed tree:** local `main` @ `0ff30c5` (the task #879 merge). `git status --short` at session
start showed exactly two untracked entries, both pre-existing and neither in this crate:
`docs/checkpoints/2026-08-13-0130.md` and `docs/reviews/2026-08-13-aligned-vmem-round5-review.md`
(the round-5 review itself — see QC9).
`origin/main` = `8804fc91c1c0019c63afa605e9729a2f2475f576`; **confirmed by `git fetch`:
`git log origin/main..HEAD --oneline | wc -l` = 29** — the 17 unpushed commits round 5 inherited
plus this round's 12. **Nothing in rounds 4 or 5 has ever run in real CI.**
**Toolchain:** `cargo`/`rustc` stable as installed on this host; Windows 10 Pro x86_64, 4 KiB page.
**Nature:** read-only. Nothing in the repository was modified other than the creation of this
document. No `git add` / `git commit` / `git push` / version bump. `git status --porcelain` after
all experiments is byte-identical to what it was at session start. Every command quoted below was
actually run on this host; every `file:line` citation was read in the current tree before being
written down.

**Three executed experiment suites, all run OUTSIDE the repository.** (1) A drift-guard sandbox
under `%TEMP%` holding `scripts/{lib.mjs,vmem-doc-drift-guard.mjs}` plus
`crates/vmem/{src/lib.rs,Cargo.toml,README.md}` — the guard resolves `REPO_ROOT` from its own file
location (`scripts/lib.mjs:14`), so a copied tree is a faithful sandbox; the delegate's
pre-orchestrator-fix guard (`git show 917cde9:scripts/vmem-doc-drift-guard.mjs`) was copied in
alongside the current one so every injection could be run against BOTH versions as a controlled
A/B. (2) A scratch `aligned-vmem` crate under `%TEMP%` (`src/`, `tests/`, `Cargo.toml` with
`[dev-dependencies]` onward stripped and `[workspace]` appended, `CARGO_TARGET_DIR` redirected) for
Q1's counterfactual. (3) A from-scratch `CARGO_TARGET_DIR` for one clippy row, so at least one
clippy result in the matrix below is a real recompile rather than a cache hit. All three temp trees
were deleted afterwards (verified). Everything reported from them is a real observed exit
code / panic location, not a prediction.

**Relationship to the prior five rounds.** This pass does not re-report V1–V21, W1–W16 + P-A/P-B/P-C,
F1–F11, R1–R13, CR1–CR10 or Q1–Q9. It verifies Q1–Q8's remediation and reports only what is new. To
stay unambiguous against the `V`-, `W`-, `P`-, `F`-, `R`-, `CR`- and `Q`-series, this pass's own
findings are numbered **QC1…QC9** (round-5 closing series).

---

## Verdict up front

**Q1, Q2, Q3, Q5, Q7 and Q8 are genuinely closed and I proved each by execution rather than by
reading. All three of the orchestrating agent's personally-found defects hold. There are no
conflict markers anywhere. The full verification matrix is green, including one from-scratch (not
cached) clippy recompile.**

**The two findings that matter are both in the round's own new residue — the campaign pattern at its
sixth consecutive occurrence:**

* **QC1 (MEDIUM, publish-relevant).** Q6's whole purpose was to replace a *false premise* with an
  honest one. It replaced it with a **different false premise, and this one points the decision the
  wrong way.** `Cargo.toml:77-80` and `src/mock.rs:38-40` now say "0.1.0 is already on crates.io, so
  removing `mock` as a Cargo feature is already a breaking change". **`mock` has never been
  published.** `git ls-tree --name-only 4ec1516^ crates/vmem/src/` returns exactly one file,
  `lib.rs`; `git show 4ec1516^:crates/vmem/Cargo.toml`'s `[features]` block contains exactly one
  entry, `alloc-lazy-commit`. `4ec1516` is the single commit that bumped `version` 0.1.0 → 0.2.0
  **and** introduced the `mock` feature, `src/mock.rs` and `Call` — and 0.2.0 is still unpublished
  (task #658). So removing `mock` right now costs nothing at all; the free window closes at
  `cargo publish`, which is literally the next planned step. CR9's open maintainer decision is
  currently documented with a cost that does not exist.
* **QC2 (MEDIUM).** The orchestrating agent's personal fix to task D — making the
  `Cargo.toml`/`README.md` scope widening actually functional — **is real, and I confirmed it is
  load-bearing on a real historical drift**, not just on a synthetic probe: the verbatim original
  `(over-reserve + trim)` description (`4a59c2b`, W5's own target) is CAUGHT by the current guard
  and PASSES CLEAN under the delegate's pre-fix version. But the widening is **still structurally
  blind to the one README site it was widened for.** README.md's API table row for
  `reserve_aligned` — the site W5 named — contains a Rust signature, and the bare `->` return arrow
  satisfies the `SCOPE` regex's bare `>` alternative. Controlled A/B, only the arrow differing:
  a row reading `` `reserve_aligned(size, align) -> bool` … (over-reserve + trim) `` **PASSES**;
  the same prose in a row with no arrow is **CAUGHT**. Every row of that table has an arrow. This
  is CR2's own defect class — a "qualifier" the drift sentence supplies for free — reproduced in
  the file scope added one commit after CR2 was closed for `.rs` files. Round 5's Q8 claimed the
  widening "closes the sixth-recurrence hole"; measured, it closes it for `lib.rs` and for one of
  the two historical `Cargo.toml` shapes, and not at all for `README.md`.

**Everything else is small.** QC3 is Q4's own fix writing a *fresh* instance of the exact
half-stated dispatch condition Q2 was fixing in the same round. QC4 and QC5 are two more
"the artifact describing the verification disagrees with the verification" instances, both created
by task D's rewrite. QC6–QC9 are citation/process INFO items.

**Publish posture (task #658).** Nothing here is a soundness blocker or a breaking change. **QC1 is
the one that should be settled before `cargo publish`** — not because the text is cosmetically
wrong, but because it is the premise a maintainer will read when deciding CR9, it ships verbatim
inside the `.crate` tarball, and it is wrong in the direction that makes the cheap option look
expensive.

---

## What was verified green — every command below was executed on this host

| command | result |
|---|---|
| `cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --no-fail-fast` | **green**, exit 0 — lib 0, `fault_injection` **5**, `huge_pages` 1, `lazy_commit` **11**, `min_page` 2, `mock` **0**, `smoke` 19, `vmemerror_io_bridge` 3, doctests 0; 0 failed |
| `cargo test -p aligned-vmem --all-features --no-fail-fast` | **green**, exit 0 — lib 0, `fault_injection` **0**, `huge_pages` 1, `lazy_commit` **11**, `min_page` 2, `mock` **9**, `smoke` 19, `vmemerror_io_bridge` 3, doctests 0; 0 failed |
| `cargo clippy -p aligned-vmem --all-targets -- -D warnings` (default row) | **green**, exit 0 (cache hit) |
| `cargo clippy -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets -- -D warnings` (the row Q3 added) | **green**, exit 0 — re-run a second time with a **fresh `CARGO_TARGET_DIR`, i.e. a genuine from-scratch compile**, still green |
| `cargo clippy -p aligned-vmem --all-features --all-targets -- -D warnings` | **green**, exit 0 (cache hit) |
| `cargo fmt -p aligned-vmem --check` | **green**, no output, exit 0 |
| `node scripts/vmem-doc-drift-guard.mjs` | **OK**, exit 0 — and, unlike round 4's identical line, this "OK" is now worth something for `.rs` files; see QC2 for what it is still not worth for `README.md` |
| `node scripts/verify-commit-prefixes.mjs` | **PASS** (5 warnings, all "docs prefix touching `src/`" on genuinely doc-comment-only commits — checked, benign) |
| `grep -rn '<<<<<<<\|>>>>>>>\|=======' crates/vmem/ scripts/vmem-doc-drift-guard.mjs docs/CORRECTNESS_OPEN_ITEMS.md scripts/check-all.mjs .github/workflows/ci.yml` | **no conflict markers** (the only hits are `// ===` banner comments in `lib.rs` and `console.log` separators in `check-all.mjs`) |
| exactly **3** `cargo clippy -p aligned-vmem` invocations in `ci.yml`'s `aligned-vmem-gates` | confirmed — `:154` (default), `:158` (everything-except-`mock`, new), `:161` (`--all-features`) |
| `git fetch && git rev-parse origin/main` / `git log origin/main..HEAD --oneline \| wc -l` | `8804fc9` / **29** |

**Read the two test rows against each other — that pair is still the R1/R2 close, and it still
holds.** `fault_injection` 5 → 0 and `mock` 0 → 9 between the two feature sets, on real Windows.

**Honesty note on the clippy rows.** Two of the three completed in under a second, i.e. served from
`cargo`'s cache against this unmodified tree. To avoid repeating round 4's caveat unqualified, I
re-ran the row that Q3 *added* — the one with no prior history on this tree — against a fresh
`CARGO_TARGET_DIR`, forcing a real compile of `bench-scale-tool` and `aligned-vmem`. It is green as
a genuine recompile, not only as a cache entry.

---

# Findings — new this pass

## QC1 — MEDIUM, publish-relevant — Q6 replaced a false premise with a *different* false premise, and the new one inverts the decision it informs: `mock` was introduced in the still-unpublished 0.2.0, so removing it as a Cargo feature is **not** "already a breaking change" — the free window CR9's deferral rests on is still open, right now

`crates/vmem/Cargo.toml:75-84` (specifically `:77-80`), `crates/vmem/src/mock.rs:36-40` and `:54-56`,
against `git ls-tree --name-only 4ec1516^ crates/vmem/src/`, `git show 4ec1516^:crates/vmem/Cargo.toml`,
`crates/vmem/Cargo.toml:3` (`version = "0.2.0"`) and task #658's own title (*"aligned-vmem — publish
0.2.0 (local already bumped, **crates.io still shows 0.1.0**)"*).

**What CR9 found and what Q6 was asked to do.** CR9 flagged that the `mock` feature-unification
hazard's deferral argument rested on *"this crate has not yet had its first `crates.io` publish
(task #659)"* — false on both halves (0.1.0 is published; #659 is another crate). Q6 extended it
with two more sites in `src/mock.rs` and prescribed, verbatim: *"restate both premises honestly
('0.1.0 is already on crates.io, so removing `mock` as a Cargo feature is already a breaking change;
the deferral now rests on the absence of real external consumers, not on the absence of a
publish')."* Task #879 applied that prescription literally, in all three sites.

**Why the prescription itself is wrong.** It silently assumes `mock` was part of 0.1.0. It was not.
Three independent checks, all run here:

```
$ git log --oneline --diff-filter=A -- crates/vmem/src/mock.rs
4ec1516 feat(vmem): aligned-vmem 0.2 — OS page_size, try_* Result API, lazy-commit/MADV_FREE/
        huge-pages, mock+fault-injection, leak_zeroed_pages

$ git ls-tree --name-only 4ec1516^ crates/vmem/src/
crates/vmem/src/lib.rs                      # ← the entire 0.1.0-era src/ tree

$ git show 4ec1516^:crates/vmem/Cargo.toml | grep -nE '^version|^[a-z-]+ = \['
3:version = "0.1.0"
23:alloc-lazy-commit = []                    # ← the entire 0.1.0-era [features] block
```

`4ec1516` is a single commit that bumps `version` 0.1.0 → 0.2.0 **and** creates `src/mock.rs`, the
`mock = []` feature, and the `Call` enum. `git log --oneline -S'version = "0.2.0"' --
crates/vmem/Cargo.toml` returns that one commit and no other, so no 0.2.x has ever been cut from a
tree that lacked `mock` either.

**Consequences, in the three sites, in order of decision-relevance.**

1. **`Cargo.toml:77-80`** — *"was evaluated and DEFERRED: 0.1.0 is already on crates.io, so removing
   `mock` as a Cargo feature is already a breaking change; the deferral now rests on the absence of
   real external consumers, not on the absence of a publish."* Removing a feature that has never
   appeared in any published version is not a breaking change against anything; no `Cargo.toml`
   anywhere can name `aligned-vmem/mock` and resolve today. The `--cfg` conversion CR9 defers is
   **still free**, and stops being free the moment task #658 runs `cargo publish`. This paragraph
   ships verbatim inside the `.crate` tarball.
2. **`src/mock.rs:36-40`** — the module doc repeats the same sentence, so the crate's own source
   carries it too.
3. **`src/mock.rs:54-56`** — *"Decided now, before 0.2.0 (0.1.0 is already on crates.io, so not
   adding this now would mean adding it later after 0.1.0 is already shipped, which would itself be
   a breaking change)."* `Call` did not exist in 0.1.0, so "after 0.1.0 is already shipped" is the
   wrong milestone for a `#[non_exhaustive]` decision about it; the boundary is 0.2.0's publish. The
   *conclusion* (add it now) is right; the reasoning printed under it is not. Note that the
   pre-Q6 text — "before this crate's first publish" — was wrong about the crate but arguably right
   about **this type**, so this specific edit traded a defensible-if-imprecise sentence for a
   precisely-wrong one.

**And the index/source polarity has flipped again.** Q6's own argument for filing a separate finding
was that `docs/CORRECTNESS_OPEN_ITEMS.md` "already has the right number" and the source was wrong.
The moved narrative (now `:2298-2306`) still reads *"neither crate has real external consumers
**before its first publish** (`aligned-vmem`: task #658 …)"* — which, for the `mock` feature
specifically, is the accurate framing, because task #658's publish IS the first publish of the
version that introduces `mock`. So the durable index is right and the source is wrong, again — the
same disagreement Q6 opened, with the same polarity, after the fix intended to close it.

**Failure scenario, concrete.** The maintainer picks up task #658, reads CR9's still-open decision,
opens `crates/vmem/Cargo.toml:77`, and reads that the `--cfg` conversion's cheap window has already
closed — "removing `mock` as a Cargo feature is already a breaking change". They therefore decline
it (correctly, given that premise) and publish 0.2.0. At that instant the window *actually* closes,
for the first time, and the hazard is locked in for the whole 0.2.x line — with the decision record
saying it was already locked in beforehand, which is the one thing that would have made the
decision reversible.

**Fix (text only; the design decision stays CR9's).** In all three sites, state the real boundary:
`mock` (and `Call`, and `src/mock.rs`) are **new in 0.2.0 and have never been published**; removing
`mock` as a Cargo feature, or converting it to a `--cfg`, is free **until 0.2.0 ships** and breaking
afterwards; the deferral therefore rests on the mechanical cost of the conversion plus the absence
of external consumers, not on any already-incurred breaking-change cost. Worth landing in whatever
commit settles CR9 — or, if CR9 stays open past the publish, before the publish, because after it
the sentence becomes true by accident and stops being checkable.

## QC2 — MEDIUM — the personally-fixed `Cargo.toml`/`README.md` scope widening is real and load-bearing (verified against a genuine historical drift), but it is structurally blind to the one README site it was widened for: the API table's bare `->` return arrow satisfies the `SCOPE` regex's `>` alternative

`scripts/vmem-doc-drift-guard.mjs:164` (the `SCOPE` regex), `:76-94` (the non-`.rs` per-line branch),
`crates/vmem/README.md:40` (the site), against `docs/reviews/2026-08-13-aligned-vmem-round5-review.md`'s
Q8 claim that widening the file list "closes the sixth-recurrence hole".

**First, the part that holds, because it deserves to be recorded as verified rather than assumed.**
The orchestrating agent's personal fix (`aafb35d`) — a separate per-line scan path for non-`.rs`
files, because the delegate's version accumulated doc blocks only on `///`/`//!` prefixes that TOML
and Markdown never contain — **is load-bearing on a real historical drift, not merely on a synthetic
probe.** Executed A/B, current guard vs. `git show 917cde9:scripts/vmem-doc-drift-guard.mjs`:

| injected into the sandbox | current guard | delegate's pre-fix guard |
|---|---|---|
| `Cargo.toml` description = **verbatim `4a59c2b` original**: `…(over-reserve + trim), commit/decommit pages…` (W5's own target) | **CAUGHT**, exit 1, `crates/vmem/Cargo.toml:7` | **PASSED CLEAN**, exit 0 |
| `Cargo.toml` description with `…unconditionally over-reserves size+align…` spliced in (the prompt's own prescribed counterfactual) | **CAUGHT**, exit 1 | **PASSED CLEAN**, exit 0 |
| `README.md` with an unqualified over-reserve sentence as its own line | **CAUGHT**, exit 1, `crates/vmem/README.md:37` | **PASSED CLEAN**, exit 0 |
| pristine sandbox (unmodified real files) | **OK**, exit 0 | **OK**, exit 0 |

**Now the part that does not hold.** The `SCOPE` regex (`:164`) accepts a bare `<` or `>` anywhere in
the sentence as proof that the sentence is conditional. `README.md:37-49` is an API table whose left
column is a Rust signature. Executed, with the *only* difference between the two rows being the
return arrow:

| README row injected (same drift prose in all three) | result |
|---|---|
| `` \| `reserve_aligned(size, align) -> Option<Reservation>` \| Reserve … (over-reserve + trim). \| `` — **verbatim W5-era shape** | **PASSED CLEAN** |
| `` \| `reserve_aligned(size, align) -> bool` \| Reserve a span (over-reserve + trim). \| `` — generics removed, arrow kept | **PASSED CLEAN** |
| `` \| `reserve_aligned(size, align)` \| Reserve a span (over-reserve + trim). \| `` — arrow removed | **CAUGHT** |

The arrow alone is the rescuer. Every one of the API table's twelve rows contains `->`
(`README.md:40-49` plus the two above it), so **any over-reserve drift written into that table is
invisible to this guard by construction** — including the verbatim W5-era row
`(exact-size mmap fast path on Unix, over-reserve on Windows)`, which I also injected and which
passes clean.

**The same rescuer effect has one real `Cargo.toml` instance too.** The verbatim `0370b17` (W5-era,
round-3 F4's target) description passes clean, because its *Unix* clause supplies `fast path` and
`miss` to a sentence whose *Windows* clause is unconditional — and TOML puts the entire
`description` value on one physical line, so the guard's own sentence splitter merges the drift into
the surrounding qualified prose. Dropping just the Unix clause makes the same sentence CAUGHT,
confirming the mechanism. So of the two documented historical `Cargo.toml` shapes, the widening
catches one (`4a59c2b`) and not the other (`0370b17`).

**Why this is MEDIUM and not a re-report of CR2.** CR2 was "the guard cannot catch either real
historical drift, in the only file it reads". That is genuinely closed — see the Q8 verification
below, where both of CR2's decisive cases now fail the guard. QC2 is narrower and newer: the
*extension* added in the same commit that closed CR2 is non-discriminating on its primary target
file, and the round-5 review's own text asserts the opposite ("closes the sixth-recurrence hole").
It is the same worst-of-three-states shape CR2 named — `npm run check` prints a green
`vmem-doc-drift-guard` line, and for `README.md` that green cannot go red.

**Failure scenario, concrete.** Round 6 or 7 edits `README.md:40` — the single most-drifted sentence
in this crate's history, five recurrences across five rounds — and simplifies it back to
"over-reserve on Windows" while tidying the table. `npm run check` is green. The guard's header and
`check-all.mjs`'s wiring both advertise `README.md` as in scope. The seventh recurrence ships to
crates.io in the README rendered on the crate's own landing page.

**Fix.** Two options, either sufficient: (a) strip inline-code spans (`` `…` ``) from a line before
applying `SCOPE`, so a signature cannot qualify prose — this is exactly right for Markdown tables
and costs nothing on `.rs` doc comments; or (b) drop bare `<`/`>` from `SCOPE` and keep only `<=`
and `>=`, which are the forms that actually appear in this crate's real conditional statements
(`align <= 64 KiB`, `align > 64 KiB` would need `>` kept — so (a) is the cleaner one). Re-run the
three injections above as the acceptance test; they are the whole test suite this guard needs and
they take a minute.

## QC3 — LOW — task #878's own Q4 fix writes a **fresh** instance of the half-stated Windows dispatch condition that task #876 was fixing for Q2 in the same round; `lib.rs:1525` is a second site Q2's own prescribed sweep found and left

`crates/vmem/src/lib.rs:616-618` (Q4's new text) and `:1525` (the two-call path's inline comment),
against `:1480` (the real condition), `:1524-1527` (`over = size + align`), and `:1683`
(`try_reserve_aligned_lazy`'s call, which is the `commit_len != size` caller).

Q4's fix reads:

```rust
/// base (this crate over-reserves `size + align` and keeps the full mapping
/// whenever the exact-size fast path misses, or on Windows when
/// `align > 64 KiB`, which is exactly that shape).
```

The real Windows condition for taking the over-reserving two-call path is the negation of
`:1480`'s `if align <= WIN_ALLOCATION_GRANULARITY && commit_len == size` — i.e. `align > 64 KiB`
**or** `commit_len != size`. The second disjunct is not hypothetical: `win_reserve_commit` has three
callers (`:1426`, `:1683`, `:1702`), and `:1683` is `try_reserve_aligned_lazy`, which passes
`initial_commit`. So `reserve_aligned_lazy(size, 4 KiB, PAGE)` on Windows over-reserves
`size + align` and keeps the full mapping — the exact shape the sentence is describing — under an
`align` the sentence explicitly excludes.

This is precisely the defect class **Q2 filed and task #876 fixed 430 lines earlier in the same
file, in the same round**: `lib.rs:182-185` now correctly reads "the fast path for `align <= 64 KiB`
**on a full-span commit (`commit_len == size`)** … the traditional path for larger alignments **or a
partial initial commit**". Task #878 wrote the omission back in at `:617-618`, and the two commits
were merged twenty minutes apart.

**Second site, from Q2's own prescription.** Q2 closed with *"then grep the crate for the sixth
instance rather than waiting for round 6 — `grep -rn '64 KiB' crates/vmem/ src/alloc_core/` is the
whole search space and takes a minute."* I ran that grep. It returns `lib.rs:1525`:

```rust
// Two-call path for align > 64 KiB (original behavior preserved).
```

sitting as the first line of the `else` branch of `:1480` — i.e. the branch that is also taken for
`commit_len != size`. Task #876's commit did not touch it. (It is the weakest instance in the family,
because the `if` condition is two lines above it in the same screen; I file it only because it is
the literal answer to the sweep Q2 asked for and the sweep evidently did not produce it.)

**Checked and explicitly NOT the same defect,** so round 6 does not re-derive them: `:425`, `:520`,
`:1321` and `:1695-1697` all state `align <= 64 KiB` as the sole fast-path condition for
**huge pages**, and there it is complete — `reserve_aligned_huge_raw` (`:1702`) calls
`win_reserve_commit(size, align, size, MEM_LARGE_PAGES)`, so `commit_len == size` holds by
construction on every huge path. Likewise `:1612`'s "the two-call path (align > 64 KiB) is currently
unreachable in practice for MEM_LARGE_PAGES" is scoped to `MEM_LARGE_PAGES` and is therefore
accurate. And `Cargo.toml:7` / `README.md:40` / `lib.rs:29` / `:773` are all scoped to
`reserve_aligned`, where `commit_len == size` always holds — Q2 already reasoned through those and
declined to file them, correctly.

**Failure scenario, concrete.** A `numa-shim`-style cross-crate adopter (the exact consumer
`from_raw_parts`'s rationale paragraph is written for) reads this paragraph to work out when the
usable base can differ from the reservation base, concludes that a `align <= 64 KiB` Windows
reservation is always `base == region`, and writes an adoption path that assumes
`base == reservation_ptr` for small alignments. A lazily-committed reservation handed to it then has
`base != region` and the assumption is silently false. The `unsafe` contract itself
(`:620-640`) is not violated — it names all five values explicitly — so this is a LOW, not a
soundness item.

**Fix.** `:617-618` → "…whenever the exact-size fast path misses, or on Windows when `align > 64 KiB`
**or the initial commit is partial (`commit_len != size`)**…". `:1525` → "Two-call path
(`align > 64 KiB` or a partial initial commit)".

## QC4 — LOW — `scripts/check-all.mjs` still describes the guard's **deleted** OR-qualifier predicate — the exact predicate CR2 condemned — as what the pre-push gate checks

`scripts/check-all.mjs:237-243`, against `scripts/vmem-doc-drift-guard.mjs:162-164` (the predicate
that actually runs).

```js
    // R6 (task #871; the guard W5/task #854 asked for two rounds ago):
    // grep-based guard against the doc-comment drift class
    // that has recurred 5 times (unconditional "over-reserve size + align" /
    // "trim" statements without qualifying context). Heuristic: every doc
    // comment mentioning "over-reserv" or "trim" must also mention "align",
    // "conditional", or "Windows" to indicate the statement is conditional.
    // See scripts/vmem-doc-drift-guard.mjs's header for the full history.
```

Every operative clause of that description was deleted by task #878. It is not "grep-based" (it is a
per-sentence tokenizing predicate); it is not per-"doc comment" (it is per-sentence, and per-line for
non-`.rs` files); and `align`/`conditional`/`Windows` are no longer qualifiers at all — the
replacement `SCOPE` list is `if|when|unless|may|miss|fast-path|slow-path|fallback|<=|>=|<|>|only|
either|paths?|rather than|no longer|instead`, and `unconditional` now *convicts* rather than
qualifying. The CR8 fix and Q5's fix both edited this file's neighbourhood; neither task touched the
body, and task #878 (which rewrote what it describes) did not either.

The pointer on the last line is correct and does mitigate this, which is why it is LOW rather than
higher: a reader who follows it lands on the true description. A reader who does not — and this text
is the one printed next to the gate's own name in `npm run check`'s step list — carries away the
predicate CR2 spent a round proving does not work.

**Failure scenario, concrete.** Round 6 adds a doc sentence, sees the guard go red, opens
`check-all.mjs` to understand why, reads "must also mention 'align', 'conditional', or 'Windows'",
adds the word "Windows" to the sentence — and the guard stays red, because that is no longer a
qualifier. Best case they lose ten minutes; worst case they conclude the guard is broken and add an
exemption.

**Fix.** Replace the four heuristic lines with one: `// Heuristic: every SENTENCE mentioning
"over-reserv"/"trim" must contain a scope word in that same sentence; "unconditional" is an
outright failure. See the guard's own header.`

## QC5 — LOW — the rewritten guard's own header states a heuristic that contradicts the predicate it implements: it lists "contains `unconditional`" as a way to **satisfy** the requirement, while the code treats it as an outright conviction

`scripts/vmem-doc-drift-guard.mjs:9-14` against `:162-174`.

```js
// Heuristic (per-sentence, not per-block): every sentence in rustdoc
// comments mentioning "over-reserv" or "trim" must ALSO, in the same
// sentence, either:
//   (a) be clearly conditional (contains "unconditional" as a HARD_FAIL), OR
//   (b) contain a scope word indicating path-specific or conditional
//       behavior (if/when/unless/may/miss/fast-path/slow-path/fallback/etc)
```

The implemented predicate is `violation ⟺ TRIGGER && (HARD_FAIL || !SCOPE)`. Clause (a) is written
in the grammatical position of a *satisfying* condition — "must ALSO … either (a) … OR (b) …" — and
its content, "be clearly conditional (contains `unconditional` …)", is self-contradictory on its
face: containing the word `unconditional` is the definition of *not* being conditional, and in the
code it is the one thing that convicts a sentence regardless of everything else. Only the
parenthetical "as a HARD_FAIL" hints at the truth, and it hints at the opposite of the sentence
containing it.

Secondary: the same paragraph scopes the heuristic to "rustdoc comments", but the guard now also
scans every non-blank line of `Cargo.toml` and `README.md`, which contain no rustdoc at all. That
half is disclosed — KNOWN LIMITATION 1 (`:22-27`) mentions the per-file branch — but the heuristic
statement above it was not updated to match.

**Failure scenario, concrete.** This is the file a round-6 reviewer opens first, because QC2 above
gives them a reason to. They read (a), conclude that adding "unconditionally" to a sentence is one
of the two sanctioned ways to satisfy the guard, and write a doc comment accordingly — which is the
single fastest way to produce a seventh recurrence of the exact sentence family this guard exists
for.

**Fix.** Rewrite as: *"a sentence containing `over-reserv`/`trim` is a violation if it contains
`unconditional`, or if it contains no scope word (if/when/unless/…) in that same sentence"*, and
change "in rustdoc comments" to "in rustdoc comments (`.rs`) or on any non-blank line
(`Cargo.toml`/`README.md`)".

## QC6 — INFO — Q6's `Cargo.toml` edit shifted `mock = []` by one line and silently invalidated `ci.yml`'s `crates/vmem/Cargo.toml:55-84` citation — the citation round 4's closing review had explicitly verified as exact; the OPEN_ITEMS narrative task #879 moved carries a stale line range too

`.github/workflows/ci.yml:781` and `:814` (both read `see crates/vmem/Cargo.toml:55-84`),
`docs/CORRECTNESS_OPEN_ITEMS.md:2307-2310`, against the current `crates/vmem/Cargo.toml`.

Round 4's closing review recorded: *"`ci.yml`'s new comments point at `crates/vmem/Cargo.toml:55-84`;
I checked the range — `:55` is the first line of the `mock` feature block and `:84` is `mock = []`.
The citation is exact."* Task #879's Q6 restatement is net +1 line inside that block
(`git show 7c6e4be:crates/vmem/Cargo.toml | sed -n '84p'` → `mock = []`; current `sed -n '84p;85p'`
→ `# consumers and the hazard is reported for real.` / `mock = []`). The cited range now stops one
line short of the declaration it was chosen to end on, in two places.

Second site, weaker and partly pre-existing: the closure narrative task #879 moved into "Recently
resolved" carries `**Evidence:** crates/vmem/Cargo.toml:60-81 … both state the deferral explicitly:
"Revisit if/when this crate gains external consumers and the hazard is reported for real."` That
sentence now lives at `:83-84`, outside the cited `:60-81`. It was already outside before this round
(it was at `:82-83`), so this is inherited drift the move propagated rather than created — but the
move was the natural moment to re-check it, and per CLAUDE.md's own "update the card in the SAME
commit" convention this is the commit that owned that check.

**Failure scenario, concrete.** Not a functional one — it is a citation, and both are close enough to
find by eye. It is filed because this campaign has now had four separate findings (CR4, CR8, Q5,
QC6) whose entire content is "a durable artifact cites something that does not resolve", and because
a line-range citation into a file the same commit is editing is the one class that is mechanically
avoidable: cite the feature name (`the mock feature block in crates/vmem/Cargo.toml`), not the
lines.

**Fix.** Both `ci.yml` sites → `see the "CARGO FEATURE-UNIFICATION HAZARD" comment on
crates/vmem/Cargo.toml's mock feature`. The OPEN_ITEMS Evidence line → drop the line range, keep the
quoted sentence (which is the actual evidence and is stable).

## QC7 — INFO — `aligned-vmem-gates`'s own header comment still says "Task #846 proved **two** clippy rows are required", with three now present, and the third row is not mentioned anywhere in the header

`.github/workflows/ci.yml:137-144` against `:154`, `:158`, `:161`.

Q3's fix is correct and complete at the step level — the new row exists, carries its own accurate
`R1/Q3:` comment, and I verified it green as a from-scratch compile. But the job's header block,
which is the part a reader auditing coverage reads first, still enumerates the required rows as
two: *"Task #846 proved two clippy rows are required: --all-features (covers all optional features)
and default (caught the inverted `#[cfg_attr]` dead_code bug that #846 fixed)."*

This is the mildest possible member of the CR1 class (a CI comment describing a step set that does
not match the steps). It is genuinely a historical statement about task #846, so it is not false as
written — which is exactly why it is INFO. The cost is that the one sentence summarising why this
job has the rows it has now omits the row that exists for the subtlest reason of the three.

**Fix.** Append one clause: *"…and, since Q3/task #877, a third: an explicit everything-except-`mock`
list, because `--all-features` turns on `mock`, which REPLACES the backend."*

## QC8 — INFO — Q1's *secondary* consequence, which the round-5 review filed explicitly as "a coverage gap in its own right", was not addressed and is still open: no test asserts anything about `reservation_len()` on the Windows single-call fast path

`crates/vmem/tests/lazy_commit.rs:89-93` (the new assertion, which covers the **two-call** path only),
against `crates/vmem/src/lib.rs:492-503` (the `reservation_len()` caveat) and
`grep -rn 'reservation_len' crates/vmem/tests/`.

Q1's fix is closed and verified (see below). Its second half is not. The grep returns exactly one
live behavioural assertion on `reservation_len()` — the one Q1 added, in
`lazy_reserve_small_align_still_reserves_full_span`, which by construction takes the **two-call**
path (`commit_len != size`). Everything else is a `Debug`-string containment check (`smoke.rs:41`),
two `from_raw_parts` negative tests, and two comments explaining why the value is deliberately *not*
asserted (`smoke.rs:73`, `mock.rs:60`). So the one path whose documentation states outright that
`reservation_len()` is **not** the true reservation size — the Windows `align <= 64 KiB`,
`commit_len == size` single call, where `:1521` returns `commit_len` while Windows has actually
reserved a 64 KiB-granular region — still has no assertion anywhere.

Not attributed to task #875: Q1's prescribed fix was one assertion and that is what landed. Filed so
round 6 inherits it without re-reading Q1's body, since the round-5 review itself asked for it to be
"stated separately".

**Fix.** One assertion in `smoke.rs`: reserve `4 KiB / 4 KiB` eagerly and assert
`r.reservation_len() == r.len()` on Windows (documenting that this is the *reported*, not the *true*,
reservation size, per `:492-503`). Two lines, and it pins the one contract in this crate that is
deliberately surprising.

## QC9 — INFO — round 5's own tasks (#875–#879) have no CHANGELOG entry as of `HEAD`, and both round-5 review documents are untracked — the third consecutive occurrence of the same shape (round-3 F11 item 3 → CR10 → this)

`grep -nE '#87[5-9]' CHANGELOG.md` → no match; the newest `aligned-vmem` entry is
`CHANGELOG.md:330` (*"round-4 follow-up (2026-08-13, tasks #867-874)"*). `git status --porcelain` →
`?? docs/reviews/2026-08-13-aligned-vmem-round5-review.md` (plus an unrelated checkpoint file), and
this document will make it two.

Stated with the same caveat CR10 carried, because it is the same observation: a round is not closed
until its post-work lands, and this closing review *is* that post-work, so the entry may well be
written in the same pass that reads this. The shape is worth naming out loud anyway, because CR10
made exactly this note about round 4 and the mechanism recurred unchanged one round later.

One thing to fold in when the entry is written: round 4's `[A]` card in
`docs/CORRECTNESS_OPEN_ITEMS.md:61-78` tracks "recurrences of the missing-CHANGELOG-entry class" as
its Current-number. If #875–879's entry lands promptly, record that the recurrence was caught before
it aged; if it does not, the card's number moves again.

---

## Q1–Q8: closed / not closed, verified one by one

* **Q1 (MEDIUM, vacuous regression assertion) — CLOSED, and I proved non-vacuity by re-running the
  full counterfactual, not by reading the diff.** `tests/lazy_commit.rs:88` now reads
  `assert_eq!(r.len(), size, "len() echoes the requested size")` (the misleading message corrected
  exactly as Q1 asked) and `:89-93` adds the two-sided oracle. On a scratch copy under `%TEMP%`, with
  `&& commit_len == size` deleted from `lib.rs:1480` — reproducing bug #848 — the suite now fails
  **at line 89**:
  ```
  thread 'lazy_reserve_small_align_still_reserves_full_span' panicked at tests\lazy_commit.rs:89:5:
  the OS reservation must cover the full requested span (got 4096)
  test result: FAILED. 10 passed; 1 failed
  ```
  With the guard restored, `11 passed; 0 failed`. Round 5's finding was that the failure landed at
  `:105` (the `commit_range` assertion) rather than at the assertion whose message names the
  invariant; that is now `:89`, the assertion that both names *and* checks it. Its secondary half is
  QC8.
* **Q2 (MEDIUM, half-stated dispatch condition) — CLOSED at both cited sites.** `lib.rs:181-186` and
  `Cargo.toml:103-107` now both carry the `commit_len == size` half and the "or a partial initial
  commit" half, and they agree with `:1480` and with the two counter rustdocs at `:224` / `:239`.
  The prescribed sweep produced QC3's second site and did not prevent QC3's first.
* **Q3 (LOW-MEDIUM, missing clippy row) — CLOSED.** Exactly three `cargo clippy -p aligned-vmem`
  invocations exist in `aligned-vmem-gates` (`:154`, `:158`, `:161`), matching what task C added, and
  the new one carries an accurate `R1/Q3` comment. Verified green as a **from-scratch recompile**
  against a fresh `CARGO_TARGET_DIR`, not a cache hit. Its job-header residue is QC7.
* **Q4 (LOW, sixth over-reserve drift) — CLOSED as to the unconditional phrasing; the replacement
  introduces QC3.** `lib.rs:616-618` is no longer an unqualified statement and the guard no longer
  flags it (it was the guard's single true positive on the pre-round-5 tree).
* **Q5 (LOW, guard-header mis-citation) — CLOSED, and the merge conflict was resolved coherently.**
  `scripts/vmem-doc-drift-guard.mjs:4-7` now reads *"Originally added by R6 / task #871 (implementing
  what W5 / task #854 asked for two rounds earlier); see task #878/Q8 for the full history and the
  per-sentence rewrite that closes CR2"* — both citations present, correctly paired, one coherent
  sentence. No conflict markers anywhere in the repository. The call-site description in
  `check-all.mjs` is QC4.
* **Q6 (LOW, publish-relevant false premise) — the task-number half is closed; the premise half is
  QC1.** `#659` is gone from all three sites. The replacement premise is wrong, in the
  decision-inverting direction.
* **Q7 (INFO, OPEN_ITEMS structural slip) — CLOSED, per CLAUDE.md's R34-24 prescription.** Item 42 in
  the `[T]` tier (`:1876-1885`) is now a stub ending *"**Closed** — see 'Recently resolved' section
  for the full closure narrative"*, and the ~45-line narrative sits in "Recently resolved" as item 3
  (`:2277-2320`). I checked the renumbering it forced (old 3→4, old 4→5): no collision, no skip, and
  nothing in the repository cites those positions by number. Its Evidence line's stale range is QC6.
* **Q8 (INFO, CR2 closure) — CR2 IS CLOSED for `.rs` files, and I re-ran CR2's own decisive cases to
  say so.** Injecting the **verbatim round-3 F4 sentence** into `reserve_aligned`'s rustdoc and the
  **verbatim round-4 R6 sentence** into the module `//!` doc — the two inputs the old guard passed
  clean on, which was the whole of CR2 — both now exit 1 (`lib.rs:762` and `lib.rs:1`). The pristine
  tree is clean. The file-scope extension is QC2.
* **Q9 (INFO) — no action required, none taken; not re-derived here.**

## Checked and explicitly NOT findings

* **The three personally-fixed defects all hold.** (1) The guard has two genuinely distinct code
  paths (`isRsFile` branch `:55-75`, else branch `:76-94`) and the else branch is functional —
  proved by the A/B table in QC2, where three injections into `Cargo.toml`/`README.md` are caught by
  the current guard and pass clean under `917cde9`'s. (2) `Cargo.toml:75-84` is one coherent
  sentence with no stray `##` and no truncation; the parallel text in `src/mock.rs:36-40` matches it
  (both are wrong on the facts — QC1 — but they are grammatically complete and mutually consistent,
  which is what the personal fix was for). (3) The merged header reads coherently and cites both
  origins; `grep -rn '<<<<<<<\|>>>>>>>\|======='` over `crates/vmem/`, the guard, the correctness
  index, `check-all.mjs` and `ci.yml` returns only `// ===` banner comments and `console.log`
  separators.
* **No out-of-scope edits.** `git diff 7c6e4be..HEAD --stat` is 7 files — `ci.yml`,
  `crates/vmem/{Cargo.toml,src/lib.rs,src/mock.rs,tests/lazy_commit.rs}`,
  `docs/CORRECTNESS_OPEN_ITEMS.md`, `scripts/vmem-doc-drift-guard.mjs` — every one traceable to a
  specific Q-finding. No TODO, no placeholder, no commented-out code, no stray debugging artifact.
* **No code change at all, in the runtime sense.** The round's only `src/` edits are doc comments
  (`lib.rs` ×2 blocks, `mock.rs` ×2 blocks); `git diff` shows no change to any executable statement,
  no new `pub fn`, no new `unsafe`, no new `dbg_*` hook, nothing matching CLAUDE.md's benchmark-hook
  rule. `node scripts/verify-commit-prefixes.mjs` passes; its five warnings are all
  "`docs(...)` prefix touched `src/`" on commits I confirmed are doc-comment-only.
* **Performance: null, for the sixth round running.** No algorithm, no feature composition, no
  default changed. `production`'s composition in the root `Cargo.toml` is untouched by this round.
* **The huge-page `align <= 64 KiB` statements are complete, not half-stated** (`:425`, `:520`,
  `:1321`, `:1695-1697`), because `reserve_aligned_huge_raw` passes `commit_len == size` by
  construction; `:1612`'s `MEM_LARGE_PAGES`-scoped claim likewise. Recorded so round 6 does not
  re-derive what QC3's grep already settled.
* **`Cargo.toml`'s comment re-wrapping after Q2/Q6 is ragged but harmless** (`:107` and `:112-113`
  are visibly shorter/longer than their neighbours after in-place edits). No line in the file exceeds
  100 characters except `description` at `:7` (482 chars, intrinsic to TOML). Cosmetic; not filed.
* **The `Recently resolved` renumbering task #879 performed is sound** — no duplicate numbers, no
  broken cross-reference; the section's pre-existing non-monotonic numbering (1,2,…,5,30,31,…) is
  older than this round and unaffected.
* **CR9 remains correctly open** as the maintainer's design decision (`--cfg` vs Cargo feature),
  jointly with `numa-shim` per the moved item's own "Revisit condition (both crates jointly)". QC1
  changes the *premise* that decision is documented with; it does not settle the decision, and I am
  not proposing one.

---

## Recommended order

1. **QC1** — three text sites, and it should land before `cargo publish` (task #658), not after.
   It is the only finding here that changes what a maintainer would decide.
2. **QC2** — one regex change in `scripts/vmem-doc-drift-guard.mjs` (strip inline-code spans before
   applying `SCOPE`), with the three README injections above as its acceptance test. A guard whose
   green light cannot go red on its primary target file is the state CR2 spent a round establishing
   is the worst of the three.
3. **QC3, QC4, QC5** — three doc/comment corrections, batchable in one pass, all in files this round
   already edited.
4. **QC6, QC7, QC8** — two citation fixes, one CI comment clause, one assertion.
5. **QC9 + push.** Write the #875–879 CHANGELOG entry, commit both round-5 review documents, then —
   the standing precondition now three rounds old — **push.** `origin/main` is 29 commits behind and
   none of rounds 4 or 5 has ever run in CI. Per CLAUDE.md's own "Then confirm CI went green — do not
   assume it", the push and the landing-SHA confirmation are the real next gate; every green claim in
   this document and in rounds 1–5 remains a local claim on one Windows 10 host with a 4 KiB page.

Nothing here is a breaking change. Nothing here reopens a V-, W-, P-, F-, R-, CR- or Q-series
finding: QC1 is *downstream of* Q6/CR9, QC2 of Q8/CR2, QC3 of Q2/Q4, QC4/QC5 of Q5/Q8 — but each is
a distinct site or a distinct claim that the corresponding finding did not make.

## On "did round 5's own fixes introduce the next round's findings?" — an honest answer

**Yes, four of them, and the two that matter were introduced by the two tasks that were most
carefully reviewed.** QC1 is task E's, QC2/QC4/QC5 are task D's — the two tasks that received a
personal zero-trust fix on top of the delegate's work. That is not an argument against the
zero-trust pass; both personal fixes were correct and I verified both are load-bearing. It is an
argument that the pass ends one level too early: it checked that the *delegate's mechanism* did what
the commit message said, and did not re-ask whether the *finding's own prescription* was true. Q6
prescribed the sentence "0.1.0 is already on crates.io, so removing `mock` as a Cargo feature is
already a breaking change"; task E applied it faithfully; nobody ran `git ls-tree 4ec1516^` to check
whether `mock` was in 0.1.0. Q8 prescribed widening the file list and asserted it "closes the
sixth-recurrence hole"; task D widened it, the orchestrator caught that the widening did not run at
all and fixed that — and nobody then re-asked whether, now that it ran, it *caught* anything at the
site it was widened for.

**The technique with remaining yield is unchanged from round 5's own closing note, one notch
sharper.** Round 5 said: execute the artifact, do not read it. Every finding above came from that,
and the three that came from executing an artifact *against the specific input it was built for* —
QC2's README row, QC1's `git ls-tree`, Q1's counterfactual — are the ones that produced real
information. The sharpening for round 6: **when a review prescribes a fix, the prescription is a
claim, not a receipt, exactly as an agent's "tests passed" is.** Q6's prescribed replacement text
was quoted verbatim into three files without anyone testing the sentence. Q8's "closes the
sixth-recurrence hole" was quoted into a commit message without anyone injecting a README drift.
Both would have taken under five minutes to falsify, and both were falsified here in under five
minutes.

**On whether a sixth round is padding.** The source is unchanged for the sixth round running —
this round's entire `src/` diff is four doc-comment blocks — and I found nothing new in it, as
round 5 predicted for round 5 and was right about. But every round including this one has found
genuine residue in the *verification and documentation layer*, and the rate is not declining: round
4 found 10, round 5 found 9, this pass found 9 in a 226-line diff. A round 6 that reads `lib.rs`
again would be padding. A round 6 that takes each of QC1–QC9's fixes and, before accepting it, runs
the one command that would falsify it, would not.
