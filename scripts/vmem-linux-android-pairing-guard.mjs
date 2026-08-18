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
// RULE (per doc block / paragraph):
//   A doc block (for .rs files: a contiguous /// or //! block; for README.md
//   / Cargo.toml: a paragraph = maximal run of non-blank lines) is a
//   VIOLATION if it mentions the word "linux" AND one of the PAIR-GATED
//   mechanism markers below, WITHOUT any Android satisfier ANYWHERE IN THE
//   SAME BLOCK/PARAGRAPH. Mechanisms whose cfg is the Linux/Android pair:
//   MAP_HUGETLB, MAP_HUGE_<size> (e.g. MAP_HUGE_2MB), LINUX_HUGE_PAGE_SIZE,
//   HUGE_SUPPORTED, MADV_HUGEPAGE, MADV_FREE (the lazy-decommit advice
//   routed by `madv_free_advice`), "hugetlb", "huge page(s)".
//   Android satisfiers: the word "android", or the phrase "linux kernel"
//   ("the Linux kernel reserves pool pages..." is kernel-family prose —
//   Android IS the Linux kernel, so such a sentence does not forget it).
//
// SECOND RULE (stale-arm phrasing, both directions pinned):
//   Any occurrence of `covered by the same target_os = "linux" arm` (or a
//   backticked variant) is a VIOLATION unconditionally: task #944/U-2
//   widened every such arm to `any(target_os = "linux",
//   target_os = "android")`, so describing the arm as the bare
//   `target_os = "linux"` arm is stale by construction. This pins the
//   inverse drift direction too: prose claiming which OSes an arm covers
//   must not contradict the arm's actual shape. This rule is evaluated over
//   the whole doc block/paragraph.
//
// PREPROCESSING (before the "linux" word test, to avoid false triggers):
//   - Rust target triples (`i686-unknown-linux-gnu`,
//     `armv7-unknown-linux-musleabihf`, ...) are stripped: "linux" between
//     hyphens is part of a triple, not an OS statement.
//   - Path-like tokens with >= 2 slashes (`include/uapi/linux/mman.h`) are
//     stripped for the same reason.
//   - Constant NAMES (`LINUX_HUGE_PAGE_SIZE`) never trigger the "linux"
//     word test: underscores are word characters, so \blinux\b does not
//     match inside the identifier. A block/paragraph whose only "Linux" is the
//     constant name is provenance, not an OS enumeration.
//
// SCAN SURFACE: identical to scripts/vmem-doc-drift-guard.mjs (task
// #1078): every .rs file under crates/aligned-vmem/src/** recursively
// (rustdoc /// and //! blocks only), plus crates/aligned-vmem/Cargo.toml
// and crates/aligned-vmem/README.md (every non-blank line, joined into
// paragraphs by blank-line separation). Deliberately NOT scanned: tests/,
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
// NOTE: The paragraph-join widening in task #1114 made some entries stale
// because the wider window now includes an Android satisfier elsewhere in the
// paragraph (e.g., a sentence that mentions Android later in the same doc block
// forgives an earlier Linux-only claim). This is a semantic change, not a fix:
// the drift is still real at the sentence level, but the guard's window is
// intentionally coarser to catch the wrapped-line and cross-sentence blind spots
// that shipped (task #1105/#1114). See KNOWN LIMITATIONS item 5.
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
//   5. The rule window is the WHOLE DOC BLOCK / PARAGRAPH, so a Linux-only
//      claim in one sentence is forgiven if ANY OTHER sentence in the same
//      block mentions Android (the window is now coarser, deliberately — the
//      wrapped-line and cross-sentence blind spots it closes were the actual
//      shipped defect shape, task #1105/#1114). This is an intentional
//      tradeoff: the defect class that shipped was hard-wrapped text and
//      multi-sentence prose where "Linux" and the pair marker were split,
//      which per-sentence scanning missed entirely.
//   6. Paragraph joining for README.md/Cargo.toml is by BLANK-LINE SEPARATION:
//      a hard-wrapped paragraph is one window, but prose split by a blank line
//      or a table/list boundary is still separate windows. This matches the
//      actual defect shape (hard-wrapped prose in README.md) without
//      over-aggregating unrelated content.
//
// Usage (from repo root):
//   node scripts/vmem-linux-android-pairing-guard.mjs
//
// Counterfactual (recorded from the guard's own development, task #1105):
//   reintroducing the pre-fix wording — `except on Linux with huge-pages
//   enabled` — in src/api/reserve_aligned_huge.rs makes this guard FAIL
//   naming that block; the fixed wording (`except on Linux AND Android`)
//   passes.

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

// Rust target triples (i686-unknown-linux-gnu, x86_64-pc-solaris, ...)
const TARGET_TRIPLE = /\b[A-Za-z0-9_]+-(?:unknown|pc|apple)-[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)*\b/g;
// Path-ish tokens with at least two slashes (include/uapi/linux/mman.h)
const PATH_LIKE = /\S*\/\S*\/\S*/g;

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
  // Newly surfaced by task #1114 paragraph-join widening:
  {
    file: 'crates/aligned-vmem/src/reservation.rs',
    sentenceRegex: /Returns `true` if the current platform's \*\*ordinary native backend\*\* guarantees.*Linux \(all targets\).*MADV_FREE/,
    reason:
      'newly surfaced by the task #1114 paragraph-join widening — the doc block for `decommit_reclaims_and_zeroes()` mentions Linux and MADV_FREE (a pair-gated mechanism) in different sentences, with no Android satisfier — reported, not yet fixed',
  },
  {
    file: 'crates/aligned-vmem/src/reservation.rs',
    sentenceRegex: /Wrap a pre-existing OS reservation.*64 KiB on some Linux configurations.*huge pages/,
    reason:
      'newly surfaced by the task #1114 paragraph-join widening — the doc block for `from_raw_parts` mentions Linux and "huge pages" (a pair-gated mechanism) in different sentences, with no Android satisfier — reported, not yet fixed',
  },
  // NOTE: The following entries became stale after task #1114's paragraph-join
  // widening because the wider window now includes an Android satisfier elsewhere
  // in the paragraph. This is a semantic change, not a fix — the drift is still
  // real at the sentence level, but the guard's intentionally coarser window now
  // forgives it. Documented in KNOWN LIMITATIONS item 5.
  //
  // - crates/aligned-vmem/Cargo.toml: /\(Linux `MADV_HUGEPAGE`\)/ — stale
  // - crates/aligned-vmem/src/api/decommit.rs: /On Linux, `MADV_DONTNEED`\/`MADV_FREE` on a `MAP_HUGETLB` mapping is accepted/ — stale
  // - crates/aligned-vmem/src/bench_internals/huge.rs: /On Linux, `madvise` on a `MAP_HUGETLB` mapping only works at huge-page granularity/ — stale
  // - crates/aligned-vmem/src/reservation.rs: /Huge-page reservation: decommit never works, even on Linux\/Windows/ — stale
  // - crates/aligned-vmem/src/os/unix.rs: /now-falsified premise that "the default is always 2 MiB on mainstream x86_64\/aarch64 Linux"/ — stale (LEGITIMATE quote, but the wider window now forgives it because the block includes "Android" elsewhere)
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

  /** @type {{path: string, lineNum: number, sentence: string, rule: string}[]} */
  const violations = [];

  for (const filePath of filePaths) {
    const content = readFileSync(filePath, 'utf-8');
    const lines = content.split('\n');
    const relativePath = relative(REPO_ROOT, filePath).split('\\').join('/');
    const isRsFile = filePath.endsWith('.rs');

    if (isRsFile) {
      let currentDoc = [];
      const flush = () => {
        if (currentDoc.length > 0) {
          checkDocComment(currentDoc, violations, relativePath, true);
          currentDoc = [];
        }
      };
      for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();
        if (trimmed.startsWith('///') || trimmed.startsWith('//!')) {
          currentDoc.push({ lineNum: i + 1, content: trimmed });
        } else {
          flush();
        }
      }
      flush();
    } else {
      // Cargo.toml / README.md have no doc-block marker; join contiguous
      // non-blank lines into paragraphs (a paragraph = maximal run of non-blank
      // lines), then check each paragraph as a unit.
      let currentParagraph = [];
      const flushParagraph = () => {
        if (currentParagraph.length > 0) {
          checkDocComment(currentParagraph, violations, relativePath, false);
          currentParagraph = [];
        }
      };
      for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();
        if (!trimmed) {
          flushParagraph();
        } else {
          currentParagraph.push({ lineNum: i + 1, content: trimmed });
        }
      }
      flushParagraph();
    }
  }

  // Apply the known-drift allowlist: matching violations are withheld from
  // the failure set (but still printed); entries that matched nothing are
  // stale and must be removed.
  const activeKnown = new Set();
  const fresh = [];
  for (const v of violations) {
    const entry = KNOWN_DRIFT.find(
      e => e.file === v.path && e.sentenceRegex.test(v.sentence)
    );
    if (entry) {
      activeKnown.add(entry);
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
      `\n[vmem-linux-android-pairing-guard] FAIL: stale KNOWN_DRIFT allowlist entries (their drift was fixed or reworded — remove the entry so the debt list stays honest):`
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

function checkDocComment(docLines, violations, filePath, stripDocPrefix) {
  const docText = docLines
    .map(l => (stripDocPrefix ? l.content.slice(3).trim() : l.content))
    .join(' ');

  // Rule 2: stale bare-`target_os = "linux"` arm description —
  // unconditional; those arms are the pair since task #944/U-2.
  // Evaluated over the whole block/paragraph.
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
  // Evaluated over the whole block/paragraph, not per-sentence.
  const stripped = docText
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
