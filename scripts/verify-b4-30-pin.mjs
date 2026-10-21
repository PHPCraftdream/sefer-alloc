#!/usr/bin/env node
// B4-30 fix8 (P2-4): cargo-free CI gate for the measurement-logic PIN.
//
// The fix7 evidence (docs/perf/b4_30_probes/b4_30_fix7_budget_evidence.csv,
// budgets in tests/b4_30_first_alloc_min_stack.rs) was measured with ONE
// revision of the test's measurement logic. Until fix8 nothing enforced the
// pin: only the generator/producer asserted it on a manual run, so a PR
// could change REPEATS, the stride window, the bracket anchor, or the
// budget() mapping and stay green against budgets derived from different
// logic. This script closes that gate: it recomputes the NORMALIZED+MASKED
// sha of the LIVE test file and requires it to be accepted by the pin
// registry in scripts/b4_30_fix7_evidence.mjs (the measured pin, or one of
// its explicitly listed logicEquivalents successors — revisions proven
// EQUIVALENT FOR NEED, e.g. formatting-only or, since fix9, the
// confirmation-probe slack change that does not participate in need).
//
// Normalization + masking + the comment-dropping safety argument are
// IMPORTED from scripts/b4_30_fix7_evidence.mjs (normalizedMaskedTestSha /
// MASKED_TEST_SHA_PINS) so the gate and the evidence can never disagree.
// Summary of that argument (full text in the producer header):
//   - CRLF -> LF, whole-line `//`-comments (incl. `///`, `//!`) and blank
//     lines dropped; every other line is hashed verbatim;
//   - a logic change must occupy at least one non-comment, non-blank line,
//     and every such line is hashed => any logic change (REPEATS, stride
//     window, anchors, `87040 * 2`, budget() mapping, …) changes the hash;
//   - `#[...]` attributes start with `#` and survive; only trailing-comment
//     text after code is unhashed, and the code part of those lines stays;
//   - `///`/`//!` doc comments are rustdoc prose and never generate code in
//     an integration-test target, so dropping them only permits comment-only
//     edits (the intended property: P3 comment fixes need no re-pin).
// Read-only: no file writes, no cargo, no git. Exit 0 = pin holds, 1 = pin
// violated (or the file is unreadable).
import { readFileSync } from "node:fs";
import {
  TEST,
  MASKED_TEST_SHA_PINS,
  normalizedMaskedTestSha,
} from "./b4_30_fix7_evidence.mjs";

let source;
try {
  source = readFileSync(TEST, "utf8");
} catch (e) {
  console.error(`verify-b4-30-pin: cannot read ${TEST}: ${e.message}`);
  process.exit(1);
}
const masked = normalizedMaskedTestSha(source);
if (!MASKED_TEST_SHA_PINS.accepts(masked)) {
  console.error(
    `verify-b4-30-pin: normalized masked sha of ${TEST} = ${masked} is NOT accepted\n` +
      `  pinned (measured at 5940f78f): ${MASKED_TEST_SHA_PINS.pin}\n` +
      `  logic-equivalent successors: ${JSON.stringify(MASKED_TEST_SHA_PINS.logicEquivalents)}\n` +
      `  The measurement logic changed — re-measure all 8 cells, refresh the raw logs and evidence, then update the pin.`,
  );
  process.exit(1);
}
const via =
  masked === MASKED_TEST_SHA_PINS.pin
    ? "measured pin"
    : `logic-equivalent successor (${MASKED_TEST_SHA_PINS.logicEquivalents[masked]})`;
console.log(`verify-b4-30-pin: OK — ${TEST} matches the ${via} (${masked})`);
