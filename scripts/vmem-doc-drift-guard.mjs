// Guard against the doc-comment drift class that has recurred 5 times:
// unconditional "over-reserve size + align" / "trim" statements that
// don't mention the actual conditional behavior (Unix exact-size fast
// path, Windows align <= 64 KiB fast path). Originally added by R6 / task
// #871 (implementing what W5 / task #854 asked for two rounds earlier); see
// task #878/Q8 for the full history and the per-sentence rewrite that
// closes CR2.
//
// Heuristic (per-sentence, not per-block): a sentence in a rustdoc comment
// (`.rs`) or on any non-blank line (Cargo.toml/README.md) mentioning
// "over-reserv" or "trim" is a VIOLATION if EITHER:
//   (a) it contains "unconditional" (an outright conviction — the sentence
//       itself says the behavior has no conditions), OR
//   (b) it contains NO scope word indicating path-specific or conditional
//       behavior in that same sentence (if/when/unless/may/miss/fast-path/
//       slow-path/fallback/<=/>=/etc)
//
// This is intentionally not a blunt "this string must never appear" check:
// "over-reserve" and "trim" legitimately appear in correctly-conditional
// sentences (e.g. reserve_aligned's own rustdoc at lib.rs:741-749, which
// correctly describes the conditional fast-path vs slow-path behavior).
//
// KNOWN LIMITATION:
//   1. In lib.rs, this guard scans only ///  /  //! rustdoc comments, not
//      regular // implementation comments. The dispatch-condition drift
//      class (Q2) lives in // comments and is a separate tooling problem
//      - this guard's per-sentence predicate is not suited to it.
//   2. The SCOPE list is a heuristic that will require point additions as
//      the text evolves. It was derived from actual historical drifts and
//      verified against the current tree, but is not exhaustive.
//
// Usage (from repo root):
//   node scripts/vmem-doc-drift-guard.mjs

import { REPO_ROOT, run } from './lib.mjs';
import { readFileSync } from 'fs';

async function main() {
  const vmemDir = `${REPO_ROOT}/crates/aligned-vmem`;

  const filePaths = [
    `${vmemDir}/src/lib.rs`,
    `${vmemDir}/Cargo.toml`,
    `${vmemDir}/README.md`,
  ];

  let violations = [];

  for (const filePath of filePaths) {
    const content = readFileSync(filePath, 'utf-8');
    const lines = content.split('\n');
    const relativePath = filePath.replace(`${REPO_ROOT}/`, '');
    const isRsFile = filePath.endsWith('.rs');

    if (isRsFile) {
      let currentDoc = [];

      for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();

        if (trimmed.startsWith('///') || trimmed.startsWith('//!')) {
          currentDoc.push({ lineNum: i + 1, content: trimmed });
        } else {
          // End of a doc comment - check it
          if (currentDoc.length > 0) {
            checkDocComment(currentDoc, violations, relativePath, true);
            currentDoc = [];
          }
        }
      }

      // Check the last doc comment if the file ends with one
      if (currentDoc.length > 0) {
        checkDocComment(currentDoc, violations, relativePath, true);
      }
    } else {
      // Non-.rs files (Cargo.toml, README.md) have no `///`/`//!` doc-comment
      // marker, so there is no reliable "doc block" boundary the way there is
      // in .rs files. Both known drift sites here (Cargo.toml's `description`
      // string, README.md's feature table row) are single physical lines, so
      // scanning every non-blank line independently is sufficient and keeps
      // reported line numbers exact. (round-5/Q8: an earlier version of this
      // widening read these two files' content but never actually scanned it —
      // doc-block accumulation was gated on `///`/`//!`, which TOML/Markdown
      // never contain, so Cargo.toml/README.md were read and silently ignored.
      // Verified via an injected counterfactual: an unconditional over-reserve
      // sentence spliced into Cargo.toml's `description` passed clean under
      // that version.)
      for (let i = 0; i < lines.length; i++) {
        const trimmed = lines[i].trim();
        if (!trimmed) continue;
        checkDocComment([{ lineNum: i + 1, content: trimmed }], violations, relativePath, false);
      }
    }
  }

  if (violations.length > 0) {
    console.log('\n[vmem-doc-drift-guard] FAIL: Found sentences with "over-reserv" or "trim" but without qualifying context:');
    violations.forEach(v => {
      console.log(`\n  ${v.path}:${v.lineNum}`);
      console.log(`  ${v.sentence}`);
      console.log(`  Missing scope or contains "unconditional"`);
    });
    process.exit(1);
  }

  console.log('[vmem-doc-drift-guard] OK: no unconditional over-reserve/trim statements found');
}

// Split text into sentences, treating headers (# and - at start of line) as sentence boundaries
function splitIntoSentences(text) {
  // First, split on actual sentence terminators (. ! ?)
  let sentences = [];
  let current = '';
  let i = 0;

  while (i < text.length) {
    const char = text[i];

    // Check for sentence terminators
    if (char === '.' || char === '!' || char === '?') {
      current += char;
      i++;

      // Consume whitespace after the terminator
      while (i < text.length && /\s/.test(text[i])) {
        // If we hit a newline followed by # or -, that's a header boundary
        if (text[i] === '\n' && i + 1 < text.length) {
          const nextChar = text[i + 1];
          if (nextChar === '#' || nextChar === '-') {
            i++;
            break; // End of sentence at header boundary
          }
        }
        i++;
      }

      if (current.trim()) {
        sentences.push(current.trim());
      }
      current = '';
    } else {
      current += char;
      i++;
    }
  }

  // Don't forget the last sentence if there is one
  if (current.trim()) {
    sentences.push(current.trim());
  }

  return sentences;
}

function checkDocComment(docLines, violations, filePath, stripDocPrefix) {
  const docText = docLines
    .map(l => (stripDocPrefix ? l.content.slice(3).trim() : l.content))
    .join(' ');
  const sentences = splitIntoSentences(docText);

  const TRIGGER   = /over-reserv|\btrim(s|med|ming)?\b/i;
  const HARD_FAIL = /unconditional/i;
  const SCOPE     = /\bif\b|\bwhen(ever)?\b|\bunless\b|\bmay\b|\bmiss\b|fast[- ]path|slow[- ]path|fall(s|ing)?[- ]?(back|through)|fallback|<=|>=|<|>|\bonly\b|\beither\b|\bpaths?\b|rather than|no longer|instead/i;

  for (const sentence of sentences) {
    const hasTrigger = TRIGGER.test(sentence);

    if (hasTrigger) {
      const hasHardFail = HARD_FAIL.test(sentence);
      // Strip only the specific tokens that falsely satisfy the bare
      // `<`/`>` SCOPE alternative — a Rust signature's `->`/`=>` return
      // arrow and generic angle-bracket pairs like `<Reservation>` — before
      // checking SCOPE (round-5 closing review QC2: README.md's API table
      // was invisible to this guard because every row's `-> T` arrow
      // "rescued" it). TRIGGER and HARD_FAIL stay on the full sentence.
      // Unlike an earlier version of this fix, this does NOT strip whole
      // backtick-wrapped spans: this crate's real qualifiers (e.g.
      // `align <= 64 KiB`) legitimately live inside backticks, and
      // stripping every code span destroyed them too, convicting correctly
      // qualified sentences whose only qualifier happened to be backticked
      // (round-6 review S10). Stripping just the arrow/generic tokens keeps
      // the arrow-rescue closed while leaving backticked qualifiers like
      // `<=`/`>=` intact for SCOPE to see.
      const scopeText = sentence
        .replace(/->/g, ' ')
        .replace(/=>/g, ' ')
        .replace(/<[A-Za-z_][\w:<>, ]*>/g, ' ');
      const hasScope = SCOPE.test(scopeText);

      // Violation iff: TRIGGER && (HARD_FAIL || !SCOPE)
      if (hasHardFail || !hasScope) {
        // Find which line this sentence came from (approximate)
        // For reporting, we use the first line of the doc block
        violations.push({
          path: filePath,
          lineNum: docLines[0].lineNum,
          sentence: sentence,
        });
      }
    }
  }
}

main().catch(err => {
  console.error(`[vmem-doc-drift-guard] ERROR: ${err}`);
  process.exit(1);
});