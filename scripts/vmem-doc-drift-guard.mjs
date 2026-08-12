// Guard against the doc-comment drift class that has recurred 5 times:
// unconditional "over-reserve size + align" / "trim" statements that
// don't mention the actual conditional behavior (Unix exact-size fast
// path, Windows align <= 64 KiB fast path). See task #854/R6 for the full
// history.
//
// Heuristic: every rustdoc comment mentioning "over-reserv" or "trim" must
// also, within the same doc comment, mention one of "align", "conditional",
// or "Windows" to indicate the statement is conditional or path-specific.
//
// This is intentionally not a blunt "this string must never appear" check:
// "over-reserve" and "trim" legitimately appear in correctly-conditional
// sentences (e.g. reserve_aligned's own rustdoc at lib.rs:741-749, which
// correctly describes the conditional fast-path vs slow-path behavior).
//
// KNOWN LIMITATION: checkDocComment() joins an entire *contiguous* run of
// `///`/`//!` lines into one block before testing it, so a qualifying word
// ANYWHERE in a large block (e.g. the crate's top-of-file `//!` module doc,
// which is one continuous ~70-line block) satisfies the check for every
// sentence in that block -- including an unrelated, genuinely unconditional
// one. This guard therefore does not catch a drift reintroduced inside a
// large multi-paragraph block that also happens to say "align"/"Windows"/
// "conditional" somewhere else in the same block. It reliably catches drift
// in the SHORT, single-purpose doc comments (individual method/function
// docs) that were the site of 4 of the 5 historical recurrences; the module
// top-doc (the 5th, R6's own fix) is the weak spot. A per-sentence (not
// per-block) rewrite would close this, but was not attempted here -- flagged
// for a future pass rather than risking a rushed rewrite of the heuristic.
//
// Usage (from repo root):
//   node scripts/vmem-doc-drift-guard.mjs

import { REPO_ROOT, run } from './lib.mjs';
import { readFileSync } from 'fs';

async function main() {
  const vmemDir = `${REPO_ROOT}/crates/vmem`;

  // Read the entire lib.rs file to properly group doc comments
  const libRsPath = `${vmemDir}/src/lib.rs`;
  const content = readFileSync(libRsPath, 'utf-8');
  const lines = content.split('\n');

  let violations = [];
  let currentDoc = [];
  let currentDocStartLine = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const trimmed = line.trim();

    // Check if this is a rustdoc comment line
    if (trimmed.startsWith('///') || trimmed.startsWith('//!')) {
      if (currentDoc.length === 0) {
        currentDocStartLine = i + 1; // 1-indexed line number
      }
      currentDoc.push({ lineNum: i + 1, content: trimmed });
    } else {
      // End of a doc comment - check it
      if (currentDoc.length > 0) {
        checkDocComment(currentDoc, violations);
        currentDoc = [];
      }
    }
  }

  // Check the last doc comment if the file ends with one
  if (currentDoc.length > 0) {
    checkDocComment(currentDoc, violations);
  }

  if (violations.length > 0) {
    console.log('\n[vmem-doc-drift-guard] FAIL: Found doc comments with "over-reserv" or "trim" but without qualifying context:');
    violations.forEach(v => {
      console.log(`\n  crates/vmem/src/lib.rs:${v.lineNum}`);
      console.log(`  ${v.line}`);
      console.log(`  Missing one of: "align", "conditional", "Windows"`);
    });
    process.exit(1);
  }

  console.log('[vmem-doc-drift-guard] OK: no unconditional over-reserve/trim statements found');
}

function checkDocComment(docLines, violations) {
  const docText = docLines.map(l => l.content).join(' ');
  const hasTrigger = /over-reserv|trim/.test(docText);
  // "align"/"Windows" stay plain substring matches (so "alignment",
  // "aligned", "Windows'" etc. still count). Only "conditional" is
  // \b-anchored: "unconditionally" contains "conditional" as a bare
  // substring, and the historical drift sentence this guard exists to
  // catch is worded exactly "unconditionally over-reserves" -- an
  // unanchored match would wrongly treat that as already-qualified.
  const hasQualifier = /align|\bconditional\b|Windows/.test(docText);

  if (hasTrigger && !hasQualifier) {
    // Find the specific line with the trigger
    const triggerLine = docLines.find(l => /over-reserv|trim/.test(l.content));
    if (triggerLine) {
      violations.push({
        lineNum: triggerLine.lineNum,
        line: triggerLine.content.trim(),
      });
    }
  }
}

main().catch(err => {
  console.error(`[vmem-doc-drift-guard] ERROR: ${err}`);
  process.exit(1);
});