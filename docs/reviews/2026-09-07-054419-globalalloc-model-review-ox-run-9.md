# globalalloc-model — independent read-only review, run 9 (one-shot confirmation)

**Verdict: CLEAN — the P0–P3 finding count is ZERO. 0 P0, 0 P1, 0 P2, 0 P3,
3 P4 (all optional, none blocking).** Round 8's single P3 (the unsafe
inventory understating the crate's reasons to hold `unsafe`) is **confirmed
correctly closed**, and all three companion P4s round 8 recommended folding
into the same commit are **confirmed correctly done**.

**This is the stop condition for the entire 9-round review-fix cycle.**

Reviewed at `4085a74` (`main`), read-only, narrow scope exactly as briefed —
this is the one-shot confirmation run 8 itself recommended, not a ninth full
pass. Read: `docs/reviews/2026-09-07-052921-globalalloc-model-review-ox-run-8.md`
in full, the complete diff of `4085a74`, `crates/globalalloc-model/src/lib.rs`
in full, `README.md`'s §"Where `unsafe` lives (the complete list)" section in
full, `crates/globalalloc-model/src/double_free_ok.rs` in full, the edited
regions of `tests/double_free_no_op.rs` and `tests/oracle_negative.rs`, plus
`src/config.rs:78-96` and the `# Safety` / `// SAFETY:` inventory of
`src/drive.rs` and `src/raw_allocator.rs`. Round 7's five fixes were **not**
re-verified (run 8 did that exhaustively) and no fresh 6-axis audit was run.
No `cargo`/build/test/lint command was run — same discipline as runs 1–8.

**P0–P3 trend: 47 → 14 → 10 → 5 → 2 → 1 → 5 → 1 → 0.**

---

## 1. P3-1 — confirmed closed, both halves, with no new inaccuracy

**`README.md:599`** now reads "two documented reasons: (a) the `unsafe trait
RawAllocator` (its impls must return valid pointers for the requested
layout), every impl + call carrying `// SAFETY:`; (b) `DoubleFreeOk::new`,
the unforgeable consent token for the M2 double-free oracle, an `unsafe fn`
declaration carrying its own `# Safety` contract". Both reasons named; the
"single documented reason" phrasing that round 8 showed was false is gone;
the row contains no `|`, so the table still renders.

**`crates/globalalloc-model/src/lib.rs:104-112`** now opens "This crate holds
`unsafe` for two reasons," numbers them (1)/(2), and — this is the better of
the two resolutions round 8 offered — does **not** merely soften "Every such
site carries a `// SAFETY:` note" but *rescopes* it: the sentence now sits at
the end of reason (1), covering only the pointer sites, and reason (2)
explicitly states that `DoubleFreeOk::new` carries "its own `# Safety` doc
rather than a `// SAFETY:` note". Both forms are therefore stated, and
neither claim over-reaches the other's territory.

**Neither edit over-claims.** I checked each factual assertion the new text
makes:

- "with no raw pointer involved" — correct: `pub const unsafe fn new() -> Self`
  takes no arguments and returns `Self(())` (`src/double_free_ok.rs:29-31`).
- "carrying its own `# Safety` doc" — correct (`:22`).
- "(a) … every impl + call carrying `// SAFETY:`" — holds. Every `unsafe {}`
  block in `src/drive.rs` (lines 103, 114, 146, 175, 259, 297, 317, 349, 357,
  376, 387, 417, 440, 446, 465, 484) has a preceding `// SAFETY:`, all four
  `unsafe fn` helpers additionally carry `# Safety` docs (`:99, :108, :141,
  :163`), and all four blanket-impl methods carry `// SAFETY:`
  (`src/raw_allocator.rs:141, 146, 151, 156`) under an impl-level `# Safety`
  note (`:120`).

**"Two reasons, exactly" is still complete, not newly incomplete.** I
re-enumerated every `unsafe` item in the whole crate (not just `src/`):
`src/drive.rs:101/111/143/165` (four pointer helpers),
`src/raw_allocator.rs:82/87/96/105/117` (trait + four methods) and
`:139/140/145/150/155` (blanket impl + four methods) are all reason (a);
`src/double_free_ok.rs:29` is the sole reason (b). The only other `unsafe`
items in the crate are `unsafe impl RawAllocator` test fixtures in
`tests/double_free_no_op.rs:35-51` and `tests/oracle_negative.rs:249-321`,
which are instances of reason (a) in separate test-crate targets — the same
shape the `tagged-index-stack` row already accounts for, and outside the
tier-1 seam grep by construction. `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]'
crates/globalalloc-model/` still returns exactly one line, `src/lib.rs:113`,
so the row still describes exactly one tier-1 seam. There is no third reason
and no un-inventoried item.

**No second stale inventory statement exists elsewhere.** The crate's own
`README.md` and `CHANGELOG.md` — the two documents an external auditor reads
on crates.io — make no "single reason" unsafe-inventory claim at all, so the
root `README.md` row and `src/lib.rs`'s header were the complete set of sites
needing this edit.

## 2. The three companion P4s — each confirmed correctly done

1. **Round 8 P4 #1 (stale `double_free: true`)** —
   `tests/double_free_no_op.rs:7` now reads "a valid `double_free` consumer".
   Correct, and it is the last one: the only surviving `double_free: true`
   in tracked files is `crates/globalalloc-model/CHANGELOG.md:80`, which is a
   *deliberate historical* reference inside the "Changed (breaking …)" entry
   describing the `bool` shape that was replaced ("a plain public `bool` let
   100%-safe code write `Config { double_free: true, … }`") — correct there,
   not stale.
2. **Round 8 P4 #2 (dead clippy allow)** — the attribute is gone; the
   valuable "No `Default` impl on purpose" comment is kept and now carries a
   one-line justification for the removal. Two independent derivations agree
   the allow was inert (run 8's read of clippy's early return for
   `sig.header.is_unsafe()`, and the commit's own clean
   `clippy --all-features --all-targets -D warnings`), so removing it cannot
   have unmasked a real lint. As a bonus this also closes **half of round 8's
   P4 #11**: `DoubleFreeOk::new` gained a `/// Construct the token.` summary
   line, so it no longer renders with an empty description in the type's
   method list on docs.rs.
3. **Round 8 P4 #6 (unresolvable citation)** —
   `tests/oracle_negative.rs:919-923` no longer points at nonexistent commit
   probe notes. The load-bearing claim is now self-contained ("each failure
   mode is re-derivable directly from `validate_align`'s and the surrounding
   clamp's arithmetic in `drive.rs`"), which is true — run 8 re-derived all
   four arithmetically — with run 8 cited only as corroboration.

## 3. P4 (optional, non-blocking)

1. **`README.md:599` and `src/lib.rs:110`** — both call `DoubleFreeOk::new`
   "the unforgeable consent token"; strictly it is the token's *constructor*
   (`DoubleFreeOk` is the token). The operative facts — which item holds
   `unsafe`, that it is an `unsafe fn` declaration, that it carries a
   `# Safety` doc — are all exactly right, so this is shorthand, not an
   inaccuracy worth a re-edit before the tag.
2. **`tests/oracle_negative.rs:922`** cites "review run 8" without a path.
   `docs/reviews/2026-09-07-052921-globalalloc-model-review-ox-run-8.md` does
   exist (unlike the citation it replaced), but a reader inside the packaged
   `.crate` cannot resolve the name. Harmless now that the citation is
   corroborative rather than load-bearing.
3. **Round 8's remaining P4s #3, #4, #5, #7, #8, #9, #10 and the README-rewrap
   half of #11 are unaddressed** — correctly so: the commit picked up exactly
   the three round 8 named as the natural companions and did not scope-creep.
   #3 (`align: 1 << 63` is a hard `arithmetic_overflow` error on a 32-bit
   `usize`) is the most substantive of the residue and is worth a
   `#[cfg(target_pointer_width = "64")]` gate whenever this crate's tests are
   next touched; nothing in CI compiles this crate's tests for a 32-bit
   target today, so it is red nowhere.

---

## Summary

| Severity | Count |
|---|---|
| P0 | 0 |
| P1 | 0 |
| P2 | 0 |
| P3 | 0 |
| P4 | 3 (all optional; 2 are cosmetic, 1 is a scope note) |

**P0–P3 count is ZERO. The stop condition for this review-fix cycle is met.**

**Round 8's P3-1: confirmed closed.** The two-line inventory correction landed
correctly in both required documents, states both reasons accurately, and
introduces no new over-claim; the "two reasons, exactly" framing is verified
complete against a fresh enumeration of every `unsafe` item in the crate,
including its test targets. The three companion P4 fixes are each correct,
and the second of them incidentally closed half of a fourth. Nothing found
here blocks the 0.1.0 tag on correctness, soundness, or documentation-accuracy
grounds.

**Recommendation: tag 0.1.0 and publish.** No run 10.
