// Guard against the Linux/Android doc-drift class (audit M1, 2026-08-18
// publication-readiness audit, task #1105): contract statements that name
// ONLY "Linux" for a mechanism whose cfg/backend actually covers BOTH
// `target_os = "linux"` AND `target_os = "android"`.
//
// The concrete incident: `reserve_aligned_huge`'s rustdoc said the 2 MiB
// multiple requirement applies "except on Linux with huge-pages enabled"
// while the check that actually runs (`src/os/unix.rs`, `unix_reserve`'s
// huge guard) is gated `#[cfg(all(any(target_os = "linux",
// target_os = "android"), feature = "huge-pages"))]` — so an Android
// caller was rejected with `invalid_argument()` by a contract the
// published docs promised applied only on Linux. Android is not exercised
// in CI, so no test can catch this drift; only a static guard can.
//
// RULE (per WINDOW — a paragraph, list item, or table row, with wrapped
// continuation lines joined; see WINDOW RULE below):
//   A WINDOW is a VIOLATION if it mentions the word "linux" AND one of the
//   PAIR-GATED mechanism markers below, WITHOUT any Android satisfier
//   ANYWHERE IN THE SAME WINDOW. Mechanisms whose cfg is the Linux/Android pair:
//   MAP_HUGETLB, MAP_HUGE_<size> (e.g. MAP_HUGE_2MB), LINUX_HUGE_PAGE_SIZE,
//   HUGE_SUPPORTED, MADV_HUGEPAGE, MADV_FREE (the lazy-decommit advice
//   routed by `madv_free_advice`), "hugetlb", "huge page(s)".
//   Android satisfiers: the word "android", or the phrase "linux kernel"
//   ("the Linux kernel reserves pool pages..." is kernel-family prose —
//   Android IS the Linux kernel, so such a sentence does not forget it).
//
// WINDOW RULE (task #1121, undoing #1114's over-widening while keeping its
// hard-wrap fix): a window is one SEMANTIC UNIT of documentation, built as a
// maximal run of WRAPPED CONTINUATION lines —
//   - a line that is blank (a `///` with nothing after it in .rs files, an
//     empty line in README.md/Cargo.toml) ENDS the current window;
//   - a line that STARTS a new structural item — a Markdown table row (a
//     line beginning with `|`, in README.md) or a list item (`- `, `* `,
//     `+ `, `1. `, ...) — STARTS a new window;
//   - every other non-blank line is a continuation of the previous line's
//     sentence and is JOINED into the current window (this is #1114's
//     hard-wrap fix: a Linux-only claim wrapped across two source lines is
//     still one window).
// #1114 had widened the window to the WHOLE doc block / whole run of
// contiguous non-blank lines. Task #1121's review proved that loses the
// guard's founding case: reintroducing the M1 defect text verbatim in
// reserve_aligned_huge.rs passed, because "Linux/Android `MAP_HUGETLB`" 20
// lines earlier in the same block satisfied the search; and a whole 17-row
// README table was ONE window, so any single row mentioning Android
// ("guaranteed on Linux/Android/Windows") forgave a Linux-only drift in
// every other row. The unit a contract claim lives in is the
// paragraph/list-item/table-row, so that is the window.
//
// SECOND RULE (stale-arm phrasing, both directions pinned):
//   Any occurrence of `covered by the same target_os = "linux" arm` (or a
//   backticked variant) is a VIOLATION unconditionally: task #944/U-2
//   widened every such arm to `any(target_os = "linux",
//   target_os = "android")`, so describing the arm as the bare
//   `target_os = "linux"` arm is stale by construction. This pins the
//   inverse drift direction too: prose claiming which OSes an arm covers
//   must not contradict the arm's actual shape. This rule is evaluated over
//   each window.
//
// PREPROCESSING (before the "linux" word test, to avoid false triggers):
//   - Rust target triples (`i686-unknown-linux-gnu`,
//     `armv7-unknown-linux-musleabihf`, ...) are stripped: "linux" between
//     hyphens is part of a triple, not an OS statement.
//   - Path-like tokens with >= 2 slashes ENDING IN A FILE EXTENSION
//     (`include/uapi/linux/mman.h`, `docs/perf/OPEN_ITEMS.md`) are
//     stripped for the same reason. URLs are NOT left to this rule — they
//     are stripped WHOLE by the host-anchored URL rule below, not by
//     PATH_LIKE (task #1128 narrowed this from the
//     original "any token with >= 2 slashes": that form erased the trigger
//     word itself in prose like `Linux/glibc/musl` — a slash-separated OS
//     enumeration is not a path — so a true drift written that shape passed
//     green ("Linux/glibc/musl `MADV_FREE`" was a MISS). Requiring a
//     file-extension suffix (`\S*/\S*/\S*\.<ext>`) keeps every real path in
//     the scanned corpus stripped (`include/uapi/linux/mman.h` in
//     os/unix.rs is the one linux-bearing example) while OS enumerations
//     survive to the `\blinux\b` test.
//     CORRECTION (task #1130, F4): #1128's commit body also claimed "an
//     EXTENSIONLESS path would no longer be stripped. None exists in the
//     scanned surface today — verified, zero such tokens." That
//     verification claim was FALSE: dozens of tokens in the scanned
//     surface changed stripping behaviour between the old and new
//     PATH_LIKE (~51 per the review that found this, counted over joined
//     windows; a per-line recount here counts ~70),
//     including extensionless URLs. The real mechanism is subtler than
//     "extensionless path": against a URL like
//     `https://github.com/torvalds/linux` the new regex matches only the
//     HOST (`https://github.com` — its `.com` supplies the required
//     extension-like suffix and terminates the match there) and leaves the
//     linux-bearing PATH TAIL (`/torvalds/linux`) behind, so a doc line
//     citing a Linux repo URL next to a huge-page mention false-positives
//     (demonstrated: exit 1 on the new guard, exit 0 on the pre-#1128
//     guard). Loud, not a silent miss — #1128's reasoning that a false
//     positive is allowlistable was sound; only its "verified, zero such
//     tokens" claim and the header's "GitHub URLs are stripped" sentence
//     were false. Fixed by stripping URLs whole: `URL_TOKEN`
//     (`(?:https?:)?\/\/\S+`) runs as its own preprocessing step BEFORE
//     TARGET_TRIPLE/PATH_LIKE, so a URL's host AND path are removed
//     together. Measured against the corpus at landing: the 23 URL tokens
//     in the scanned surface contain no `linux` word and no Android
//     satisfier (the one android-bearing URL,
//     `android.googlesource.com/...` in os/unix.rs:814, sits in a `//`
//     code comment outside the `///` scan surface), so the rule changes
//     no current finding or allowlist entry.
//   - Constant NAMES (`LINUX_HUGE_PAGE_SIZE`) never trigger the "linux"
//     word test: underscores are word characters, so \blinux\b does not
//     match inside the identifier. A block/paragraph whose only "Linux" is the
//     constant name is provenance, not an OS enumeration.
//
// SCAN SURFACE: identical to scripts/vmem-doc-drift-guard.mjs (task
// #1078): every .rs file under crates/aligned-vmem/src/** recursively
// (rustdoc /// and //! blocks only), plus crates/aligned-vmem/Cargo.toml
// and crates/aligned-vmem/README.md (every non-blank line, split into
// windows per the WINDOW RULE above). Deliberately NOT scanned: tests/,
// benches/, examples/ (internal prose, never rendered as crate documentation)
// and CHANGELOG.md (a historical record of what was true at each version —
// re-qualifying past entries against current behavior would falsify that record).
//
// KNOWN-DRIFT ALLOWLIST: blocks/paragraphs that DO violate the rule today
// but live in files outside the fixing task's scope. Each entry names the
// file, a regex matching the offending block/paragraph, and a reason. The
// allowlist is SELF-CLEANING: an entry that no longer matches anything is
// itself a FAILURE ("stale allowlist entry") so the list cannot silently rot —
// when the drift is fixed, the guard forces the entry's removal. Entries are
// printed in the OK output so the debt stays visible, never silent.
//
// NOTE (honest accounting of task #1114's window widening, corrected by
// task #1122's review): #1114 deleted five allowlist entries claiming the
// wider window now found an Android satisfier in the same paragraph. That
// was true for only TWO of the five (reservation.rs's "Huge-page
// reservation: decommit never works" example block and unix.rs's
// now-falsified-premise quote — both genuinely carry an Android satisfier
// in the same paragraph). The other three (Cargo.toml `(Linux
// `MADV_HUGEPAGE`)`, decommit.rs `On Linux, `MADV_DONTNEED`/`MADV_FREE`...`,
// huge.rs `On Linux, `madvise`...`) were deleted because of a second bug:
// the allowlist bound at most ONE entry per violation (`find`), and #1114's
// wider window had MERGED two previously-separate violations in the same
// file into one window, so the second entry matched nothing and was
// reported "stale" while its drift was neither fixed nor reworded. Those
// three entries are restored below. Of the two entries #1114 ADDED as
// "newly surfaced drift", both were artifacts of the over-wide window, not
// drift: `decommit_reclaims_and_zeroes`'s "Linux" sits in the
// `MADV_DONTNEED` bullet (deliberately not a pair marker) while the
// `MADV_FREE` that tripped the marker sits in the BSD bullet describing
// BSD, and `from_raw_parts`'s "Linux" is a page-size remark in the
// `reservation_len` contract ~40 lines from the huge-page discussion. Both
// are removed. Net honest accounting of #1114's trade: 14 real-drift
// entries -> 12 real + 0 artifacts (9 kept + 3 wrongly deleted then
// restored), not "14 -> 11"; plus one explicitly-labeled FALSE-POSITIVE
// suppression entry (the decommit.rs Darwin-caveat paragraph) that the
// re-narrowed window surfaced and the `find`-binding bug had hidden.
//
// KNOWN LIMITATIONS (same posture as vmem-doc-drift-guard.mjs):
//   1. Per-block/paragraph heuristics cannot fully decide English semantics.
//      The marker list covers the mechanisms whose cfg IS the pair today; a
//      future mechanism needs its marker added here.
//   2. The decommit-reclaim enumeration family ("guaranteed on
//      Linux/Windows") is NOT policed: eager decommit's backend runs on
//      ALL unix (not the Linux/Android pair), so its prose is a
//      kernel-semantics claim rather than a pair-cfg contract. (The M1
//      round still fixed the two such sentences in lib.rs/README.md by
//      hand; decommit.rs/reservation.rs siblings remain reported drift.)
//   3. Inverse-direction coverage ("says Linux and Android where the code
//      is Linux-only") is only pinned by the stale-arm rule; a general
//      inverse check would require parsing cfg expressions out of code,
//      which prose cannot be reliably correlated with block-by-block.
//   4. The Android satisfier is WORD PRESENCE ANYWHERE IN THE BLOCK/PARAGRAPH,
//      so a block whose NORMATIVE claim is Linux-only still passes if it names
//      Android incidentally — e.g. by quoting the cfg it describes.
//      "except on Linux with `huge-pages` enabled (the check's cfg is
//      `any(target_os = "linux", target_os = "android")`)" is the original
//      M1 defect's wording plus a cfg quote, and this guard reports it
//      GREEN. Found by the orchestrator's own counterfactual during the
//      task #1105 merge: a first attempt at reverting the fixed sentence
//      left the cfg parenthetical in place and the guard did NOT fire;
//      removing it (the true pre-fix text) made the guard FAIL at
//      reserve_aligned_huge.rs:9 as designed. Narrowing this would mean
//      deciding which clause of a sentence carries the normative claim —
//      the same English-semantics wall as limitation 1 — so it is recorded
//      rather than fixed. Practical consequence: this guard catches a
//      DROPPED Android mention, not a DEMOTED one.
//   5. The rule window is the PARAGRAPH / LIST ITEM / TABLE ROW: all
//      sentences inside one window are joined, so a Linux-only claim whose
//      satisfier sits in a DIFFERENT sentence of the same window is caught
//      (the cross-sentence blind spot #1114 closed, kept), but a claim split
//      across a window boundary is not: each window must itself contain
//      both the "linux" word and a pair marker for the rule to fire, so a
//      Linux-only sentence in one paragraph and the mechanism marker in a
//      NEIGHBORING paragraph of the same doc block are not correlated. This
//      is the deliberate price of #1121's re-narrowing: #1114's whole-block
//      window caught that shape but ALSO forgave the guard's own founding
//      case (a single Android mention anywhere in a 40-line block or a
//      17-row table satisfied the search), which is the worse failure.
//   6. Within a window, only WRAPPED CONTINUATION lines are joined: a line
//      that does not start a new `|` table row or list item continues the
//      previous line. Table rows and list items ARE separate windows
//      (fixed in task #1121 — #1114's whole-paragraph join had made the
//      entire 17-row README API table one window, so one row's Android
//      mention forgave every other row). A multi-line table CELL whose
//      continuation lines do not begin with `|` would be joined with the
//      following row's window; no such cell exists in the scanned files
//      today.
//   7. A Linux-only claim split across TWO ITEMS OF ONE LIST is NOT caught
//      (task #1128, F4): `- Huge pages are requested with `MAP_HUGETLB`.``
//      followed by `- That request path is taken on Linux.` is two windows
//      under the LIST_MARKER rule — the marker item has no "linux" word,
//      the Linux item has no pair marker — so neither window fires. The
//      pre-#1121 whole-block window DID catch this shape (verified on the
//      `51816b4^` guard), and rustdoc lists are this crate's dominant doc
//      structure, so this is a real loss. It is accepted rather than fixed
//      because the only automatic repair — re-widening the window toward
//      the whole doc block — is exactly what #1121 reverted: a report-only
//      "advisory" block-level pass was prototyped against the real corpus
//      and produced exactly 2 findings, BOTH of them the known #1114
//      whole-block artifacts the NOTE above already documents as not-drift
//      (`decommit_reclaims_and_zeroes`' Linux/MADV_DONTNEED bullets and
//      `from_raw_parts`' page-size remark) — zero new signal, two standing
//      false positives a maintainer must dismiss every run, i.e. the
//      false-positive pressure that caused the narrowing, reintroduced in
//      diluted form. Recorded as this limitation instead. This also closes
//      a gap in limitation 5's framing: #1121's commit body priced the
//      narrowing in neighbouring PARAGRAPHS only and never named this
//      one-list shape.
//
// Usage (from repo root):
//   node scripts/vmem-linux-android-pairing-guard.mjs
//
// Counterfactuals (all executed against the fixed guard, task #1121/#1122,
// in a scratch copy of the tree — see docs/reviews for the transcript):
//   1. Reintroducing the pre-fix wording — `except on Linux with
//      `huge-pages` enabled` — in src/api/reserve_aligned_huge.rs FAILS
//      naming that block (it PASSED under #1114's whole-block window).
//   2. Dropping "Android" from only the `decommit_lazy` row of the README
//      API table FAILS naming that row (one row is one window).
//   3. A Linux-only sentence hard-wrapped across two README prose lines
//      still FAILS (wrapped continuations are joined — #1114's fix kept).
//   4. Marker in sentence 1, "Linux" in sentence 2 of the same .rs doc
//      paragraph still FAILS (sentences within a window are joined).
//   5. Re-adding a restored allowlist entry whose drift shares a window
//      with another entry's drift is NOT reported stale (all matching
//      entries bind, not just the first).
//   6. Clean tree: exit 0 with the full allowlist listed.
//   7. (task #1128, F8) A drift written as a slash-separated OS enumeration
//      — `Cheaper lazy reclaim — Linux/glibc/musl `MADV_FREE`` — FAILS
//      (PATH_LIKE no longer erases the "Linux" word: it requires a file
//      extension, so `Linux/glibc/musl` is not treated as a path). The
//      pre-#1128 guard passed this shape green.
//   8. (task #1129) Appending a realistic
//      `[target."cfg(target_os = \"android\")".dependencies]` section to
//      crates/aligned-vmem/Cargo.toml KEEPS the guard green with all 13
//      allowlist entries active (before #1129 the whole file was ONE
//      window, so that one `android` mention satisfied every paragraph in
//      the file and flipped the two live Cargo.toml entries to "stale" —
//      exit 1 inviting their deletion).
//   9. (task #1130, F4) A doc line citing `https://github.com/torvalds/linux`
//      next to a huge-page mention does NOT fire (URL_TOKEN strips the URL
//      whole; pre-fix the host-only PATH_LIKE match left `/torvalds/linux`
//      behind and false-positived — exit 1 on the #1128 guard).

import { REPO_ROOT } from './lib.mjs';
import { readFileSync, readdirSync } from 'fs';
import { join, relative } from 'path';

// Mechanisms whose cfg is `any(target_os = "linux", target_os = "android")`.
// MADV_DONTNEED is deliberately absent: it is the all-unix fallback advice,
// not pair-gated. MAP_HUGE_SHIFT-style encoding constants are absent too
// (`\d+[KMG]B` requires a size suffix), so kernel-version-history sentences
// about `MAP_HUGE_*` bit encoding do not false-trigger.
const PAIR_MARKERS =
  /\bMAP_HUGETLB\b|\bMAP_HUGE_\d+[KMG]B\b|\bLINUX_HUGE_PAGE_SIZE\b|\bHUGE_SUPPORTED\b|\bMADV_HUGEPAGE\b|\bMADV_FREE\b|\bhugetlb\b|\bhuge[- ]pages?\b/i;
const ANDROID_SATISFIERS = /android|linux[- ]kernel/i;
const LINUX_WORD = /\blinux\b/i;
// Task #944/U-2 widened every arm that used to be bare
// `target_os = "linux"` to `any(target_os = "linux", target_os = "android")`
// for the huge-page mechanisms; describing such an arm as the bare
// linux arm is stale regardless of context.
const STALE_ARM =
  /covered by the same\s+`?target_os = "linux"`?\s+arm/;

// Task #1129: this rule was previously FALSE for the non-`.rs` path —
// blank lines were filtered out of `units` before `pushWindows` ran, so
// `!unit.content` was never true and the whole of Cargo.toml (164 lines,
// 12 of them blank) was ONE window. One `android` mention anywhere in the
// file then satisfied the entire file: appending a realistic
// `[target."cfg(target_os = \"android\")".dependencies]` section flipped
// the two live #1122-restored Cargo.toml entries to "stale" — whose
// documented remedy is deletion, i.e. the guard invited a repeat of the
// mistake #1114 made and #1122 closed. Fixed by pushing a blank sentinel
// unit for empty lines in the non-`.rs` path so they genuinely end a
// window, exactly as the header always claimed.
// Rust target triples (i686-unknown-linux-gnu, x86_64-pc-solaris, ...)
const TARGET_TRIPLE = /\b[A-Za-z0-9_]+-(?:unknown|pc|apple)-[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)*\b/g;
// Path-ish tokens with at least two slashes ending in a file extension
// (include/uapi/linux/mman.h). NOT bare slash-enumerations (`Linux/glibc/musl`)
// — see the PREPROCESSING note in the header (task #1128, corrected by
// task #1130).
const PATH_LIKE = /\S*\/\S*\/\S*\.[A-Za-z][A-Za-z0-9]*\b/g;
// URLs, stripped WHOLE (host + path) as their own preprocessing step —
// runs BEFORE PATH_LIKE so a URL is never partially stripped down to its
// linux-bearing path tail (task #1130, F4). Scheme-anchored `https?://`,
// or a protocol-relative `//` form.
const URL_TOKEN = /(?:https?:)?\/\/\S+/g;

// Known drift outside the M1 fixing task's file scope (task #1105, agent B:
// reserve_aligned_huge.rs / os/unix.rs / lib.rs / README.md only). Each
// entry: file (repo-relative, forward slashes), sentenceRegex, reason.
// When one of these gets fixed, this guard FAILS with "stale allowlist
// entry" until the entry is removed — the list cannot silently rot.
const KNOWN_DRIFT = [
  {
    file: 'crates/aligned-vmem/Cargo.toml',
    sentenceRegex: /Linux `MAP_HUGETLB`, Windows `MEM_LARGE_PAGES`/,
    reason:
      'huge-pages feature comment block repeats the reserve_aligned_huge.rs summary; Cargo.toml was outside task #1105/agent-B file scope — reported to the orchestrator',
  },
  {
    file: 'crates/aligned-vmem/src/api/decommit.rs',
    sentenceRegex: /on both Windows and Linux, decommit \*\*does not work\*\* on huge-page reservations/,
    reason:
      'decommit.rs was outside task #1105/agent-B file scope — reported to the orchestrator (the README and reserve_aligned_huge.rs siblings WERE fixed)',
  },
  {
    file: 'crates/aligned-vmem/src/api/decommit_lazy.rs',
    sentenceRegex: /Linux `MADV_FREE`, macOS\/iOS `MADV_FREE_REUSABLE`/,
    reason:
      'decommit_lazy.rs was outside task #1105/agent-B file scope — reported to the orchestrator (the README sibling row WAS fixed)',
  },
  {
    file: 'crates/aligned-vmem/src/api/internal.rs',
    sentenceRegex: /granted \(Linux `MAP_HUGETLB` \/ Windows `MEM_LARGE_PAGES`\)/,
    reason:
      'internal.rs was outside task #1105/agent-B file scope — reported to the orchestrator',
  },
  {
    file: 'crates/aligned-vmem/src/bench_internals/huge.rs',
    sentenceRegex: /incompatible with huge-page reservations on both Windows and Linux/,
    reason:
      'bench_internals/huge.rs was outside task #1105/agent-B file scope — reported to the orchestrator',
  },
  {
    file: 'crates/aligned-vmem/src/reservation.rs',
    sentenceRegex: /Linux \(`MAP_HUGETLB`\) or Windows \(`MEM_LARGE_PAGES`/,
    reason:
      'reservation.rs was outside task #1105/agent-B file scope — reported to the orchestrator',
  },
  {
    file: 'crates/aligned-vmem/src/reservation.rs',
    sentenceRegex: /from the OS \(Linux `MAP_HUGETLB` or Windows `MEM_LARGE_PAGES`\)/,
    reason:
      'reservation.rs was outside task #1105/agent-B file scope — reported to the orchestrator',
  },
  {
    file: 'crates/aligned-vmem/src/reservation.rs',
    sentenceRegex: /Windows crashes on write before recommit, Linux does not/,
    reason:
      'reservation.rs was outside task #1105/agent-B file scope — reported to the orchestrator',
  },
  {
    file: 'crates/aligned-vmem/src/reservation.rs',
    sentenceRegex: /cheaper than \[`Self::decommit`\] \(Linux `MADV_FREE`/,
    reason:
      'reservation.rs was outside task #1105/agent-B file scope — reported to the orchestrator',
  },
  // Restored by task #1122: wrongly deleted by #1114's window widening.
  // The drift below was never fixed or reworded — these entries went
  // "stale" only because the widened window MERGED two violations in the
  // same file into one and the allowlist bound at most one entry per
  // violation (`find`), leaving the second entry unmatched. They share a
  // window with the entries above and are live, unfixed drift.
  {
    file: 'crates/aligned-vmem/Cargo.toml',
    sentenceRegex: /\(Linux `MADV_HUGEPAGE`\)/,
    reason:
      'restored by task #1122 after #1114 deleted it via violation merging — the drift is live and unfixed (same huge-pages comment block as the MAP_HUGETLB entry above)',
  },
  {
    file: 'crates/aligned-vmem/src/api/decommit.rs',
    sentenceRegex: /On Linux, `MADV_DONTNEED`\/`MADV_FREE` on a `MAP_HUGETLB` mapping is accepted/,
    reason:
      'restored by task #1122 after #1114 deleted it via violation merging — the drift is live and unfixed (same paragraph as the does-not-work entry above)',
  },
  {
    file: 'crates/aligned-vmem/src/bench_internals/huge.rs',
    sentenceRegex: /On Linux, `madvise` on a `MAP_HUGETLB` mapping only works at huge-page granularity/,
    reason:
      'restored by task #1122 after #1114 deleted it via violation merging — the drift is live and unfixed (same doc block as the incompatible entry above)',
  },
  // Suppression (NOT drift): a heuristic false positive that the #1121
  // window re-narrowing surfaced. At HEAD this paragraph was already
  // violating the rule, invisibly — the whole doc block was ONE window, the
  // two decommit.rs entries above bound to that one violation, and this
  // paragraph rode inside it. Per-paragraph windows correctly separate the
  // real drift (the Huge-page incompatibility paragraph) from this Darwin
  // caveat, whose "Linux" is a comparison ("unlike Linux, it does not
  // reliably unmap") and whose marker hit is a cross-reference ("the same
  // shape as the huge-page case above") — no pair-gated contract is stated.
  {
    file: 'crates/aligned-vmem/src/api/decommit.rs',
    sentenceRegex: /Darwin zero-fill gap.*unlike Linux.*huge-page case above/,
    reason:
      'FALSE-POSITIVE SUPPRESSION (task #1122), not drift: "unlike Linux" is a Darwin/BSD comparison and "the huge-page case above" is a cross-reference — the paragraph states no Linux-only pair contract',
  },
];

function main() {
  const vmemDir = `${REPO_ROOT}/crates/aligned-vmem`;

  const rsFiles = listRsFilesRecursive(`${vmemDir}/src`).sort();
  if (rsFiles.length === 0) {
    throw new Error(
      'no .rs files found under crates/aligned-vmem/src — scan surface lost'
    );
  }
  const filePaths = [
    ...rsFiles,
    `${vmemDir}/Cargo.toml`,
    `${vmemDir}/README.md`,
  ];

  const violations = [];

  // Windows are built per the WINDOW RULE in the header: a maximal run of
  // WRAPPED CONTINUATION lines. A blank line ends a window; a line starting
  // a new structural item (Markdown table row / list item) starts a new
  // window; every other non-blank line continues the current one (joined —
  // this is what preserves #1114's hard-wrap fix).
  const LIST_MARKER = /^(?:[-*+]|\d+[.)])\s/;

  /** @type {{path: string, lineNum: number, sentence: string, rule: string}[]} */
  const pushWindows = (units, isMarkdown, path) => {
    let current = [];
    const flush = () => {
      if (current.length > 0) {
        checkDocComment(current, violations, path);
        current = [];
      }
    };
    for (const unit of units) {
      const startsItem =
        (isMarkdown && unit.content.startsWith('|')) || LIST_MARKER.test(unit.content);
      if (!unit.content || startsItem) flush();
      if (unit.content) current.push(unit);
    }
    flush();
  };

  for (const filePath of filePaths) {
    const content = readFileSync(filePath, 'utf-8');
    const lines = content.split('\n');
    const relativePath = relative(REPO_ROOT, filePath).split('\\').join('/');
    const isRsFile = filePath.endsWith('.rs');
    const isMarkdown = filePath.endsWith('.md');

    if (isRsFile) {
      // Rust doc lines: strip the ///|//! prefix first so a blank doc line
      // (`///` with nothing after it) is a window boundary and list-item
      // detection sees the content, not the marker.
      const units = [];
      for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();
        if (trimmed.startsWith('///') || trimmed.startsWith('//!')) {
          units.push({ lineNum: i + 1, content: trimmed.slice(3).trim() });
        } else if (units.length > 0) {
          pushWindows(units.splice(0), false, relativePath);
        }
      }
      pushWindows(units.splice(0), false, relativePath);
    } else {
      // Cargo.toml / README.md have no doc-block marker; every line is a
      // unit, split into windows per the WINDOW RULE (README.md
      // additionally breaks at table-row starts). Blank lines are pushed
      // as EMPTY-CONTENT sentinel units so they genuinely END a window
      // (task #1129): filtering them out here made `!unit.content` in
      // pushWindows unreachable for these files, so the whole of
      // Cargo.toml was ONE window and a single `android` mention anywhere
      // in the file satisfied every paragraph in it — flipping the two
      // live #1122-restored Cargo.toml entries to "stale" on a realistic
      // appended `[target."cfg(target_os = \"android\")".dependencies]`
      // section, i.e. inviting a repeat of #1114's deletion mistake.
      const units = [];
      for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();
        units.push({ lineNum: i + 1, content: trimmed });
      }
      pushWindows(units, isMarkdown, relativePath);
    }
  }

  // Apply the known-drift allowlist: ALL entries matching a violation bind
  // to it (task #1122: `find` bound at most one entry per violation, so when
  // two drift records share a window the second entry matched nothing and
  // was falsely reported stale — that is how #1114 deleted three live,
  // unfixed drift records). Matching violations are withheld from the
  // failure set (but still counted); entries that matched nothing are stale
  // and must be removed.
  const activeKnown = new Set();
  const fresh = [];
  for (const v of violations) {
    const matching = KNOWN_DRIFT.filter(
      e => e.file === v.path && e.sentenceRegex.test(v.sentence)
    );
    if (matching.length > 0) {
      for (const e of matching) activeKnown.add(e);
    } else {
      fresh.push(v);
    }
  }
  const staleEntries = KNOWN_DRIFT.filter(e => !activeKnown.has(e));

  let failed = false;
  if (fresh.length > 0) {
    console.log(
      `\n[vmem-linux-android-pairing-guard] FAIL: blocks/paragraphs naming only Linux for a Linux/Android-pair-gated mechanism (scanned ${rsFiles.length} .rs files under src/ + Cargo.toml + README.md):`
    );
    for (const v of fresh) {
      console.log(`\n  ${v.path}:${v.lineNum}  [${v.rule}]`);
      console.log(`  ${v.sentence}`);
    }
    failed = true;
  }
  if (staleEntries.length > 0) {
    console.log(
      `\n[vmem-linux-android-pairing-guard] FAIL: stale KNOWN_DRIFT allowlist entries (their drift was fixed or reworded, or the guard's window no longer detects it — remove the entry so the debt list stays honest):`
    );
    for (const e of staleEntries) {
      console.log(`\n  ${e.file}  /${e.sentenceRegex.source}/`);
      console.log(`  reason: ${e.reason}`);
    }
    failed = true;
  }
  if (failed) process.exit(1);

  console.log(
    `[vmem-linux-android-pairing-guard] OK: no Linux-only phrasing of Linux/Android-pair-gated mechanisms (scanned ${rsFiles.length} .rs files under src/ + Cargo.toml + README.md; ${activeKnown.size} known-drift allowlist entr${activeKnown.size === 1 ? 'y' : 'ies'} active, listed above/allowed)`
  );
  if (activeKnown.size > 0) {
    for (const e of activeKnown) {
      console.log(`  known drift: ${e.file} — ${e.reason}`);
    }
  }
}

function checkDocComment(docLines, violations, filePath) {
  const docText = docLines
    .map(l => l.content)
    .join(' ');

  // Rule 2: stale bare-`target_os = "linux"` arm description —
  // unconditional; those arms are the pair since task #944/U-2.
  // Evaluated over the whole window.
  if (STALE_ARM.test(docText)) {
    violations.push({
      path: filePath,
      lineNum: docLines[0].lineNum,
      sentence: docText,
      rule: 'stale-arm',
    });
    // Don't return early: a block can violate both rules.
  }

  // Rule 1: Linux word (post-strip) + pair marker, no Android satisfier.
  // Evaluated over the whole window, not per-sentence.
  const stripped = docText
    .replace(URL_TOKEN, ' ')
    .replace(TARGET_TRIPLE, ' ')
    .replace(PATH_LIKE, ' ');
  if (
    LINUX_WORD.test(stripped) &&
    PAIR_MARKERS.test(docText) &&
    !ANDROID_SATISFIERS.test(stripped)
  ) {
    violations.push({
      path: filePath,
      lineNum: docLines[0].lineNum,
      sentence: docText,
      rule: 'linux-only-pair-mechanism',
    });
  }
}

// Recursively list every .rs file under dir (same rationale as
// vmem-doc-drift-guard.mjs: the a4b8e50 modules-per-file split moved doc
// text into new subdirectories once already — a flat readdir is how a
// false green happens).
function listRsFilesRecursive(dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listRsFilesRecursive(full));
    } else if (entry.isFile() && entry.name.endsWith('.rs')) {
      out.push(full);
    }
  }
  return out;
}

main();
