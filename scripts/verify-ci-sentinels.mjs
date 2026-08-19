// Static guard over every `grep -F "test <NAME> ... ok" "$RUNNER_TEMP/<log>"`
// postcondition in .github/workflows/ci.yml (task #1150).
//
// THE PROBLEM (freshly demonstrated twice in this campaign): these
// postconditions are hand-written STRINGS that must stay byte-identical to
// libtest's own output format, and that format depends on attributes on the
// Rust `fn` the sentinel names:
//   - plain `#[test]`               -> `test <NAME> ... ok`
//   - `#[should_panic]` + `#[test]` -> `test <NAME> - should panic ... ok`
//   - `#[ignore]`                   -> the test never runs, so NEITHER form
//                                      can ever appear in the log; the
//                                      sentinel is permanently dead
// Nothing enforces that a sentinel's form matches its target `fn`'s real
// attributes. Task #1146 (commit `77efd3a`) found and fixed exactly this:
// six sentinels named `#[should_panic]` tests in the plain form, so two loom
// jobs went red on every push even though every test in them passed — the
// mismatch was silent from task #1110 (when the sentinels were introduced)
// until #1146 found it by hand. An attribute can be added to any of the 24
// sentinel-tracked tests tomorrow and ci.yml gets no automatic signal; this
// script is that signal.
//
// WHAT THIS SCRIPT DOES — a pure text scan, no cargo invocation:
//   1. Parse ci.yml. For every `- run: |` / `- run: <cmd>` step, find any
//      `cargo test ... --test <target>` invocations (zero, one, or several
//      `--test` flags in one step — the loom jobs run up to 7 targets in one
//      `cargo test` call) and any `grep -F "test <NAME>[ - should panic] ...
//      ok" "$RUNNER_TEMP/<log>"` lines in the SAME step (sentinel and
//      invocation are always co-located in one step, per the task brief).
//      A step with zero `--test` flags but a `-p <crate>` scope runs that
//      crate's WHOLE test suite (`cargo test -p <crate>` / `cargo test -p
//      <crate> --all-features`, etc.) — the mock-debug rows are this shape.
//   2. For each sentinel in a step, resolve which target file it names:
//      - if the step named exactly one `--test <target>`, or the sentinel's
//        bare name is found in exactly one of the step's several `--test`
//        targets, use that target unambiguously;
//      - if the step named NO `--test` (whole-crate/whole-package), search
//        every `tests/*.rs` file under that step's `-p <crate>` scope (or
//        the root `tests/` dir if no `-p` is given) for `fn <name>`.
//      A sentinel name may be module-qualified (`mod::fn_name`, e.g.
//      `forced_page::lazy_initial_commit_call_sites_survive_a_forced_64k_page`)
//      — one level of `mod <name> { ... }` resolution is applied: the bare
//      fn name is searched for, and if found inside a `mod <modname> { ... }`
//      block, `<modname>::<fn_name>` must equal the sentinel's full name.
//   3. Read the real attributes immediately preceding the resolved `fn`
//      (walking upward through attribute lines / blank lines, stopping at
//      the first non-attribute, non-blank line). Compare against the
//      sentinel string:
//      - `#[should_panic]` present  <=>  sentinel contains ` - should panic`
//      - `#[ignore]` present        =>   ALWAYS a mismatch (the class #1146
//        did not need to handle because no sentinel today targets an
//        ignored test — flagged distinctly so a future ignore added to a
//        tracked test reads as its own defect class, not a should_panic
//        miscategorization)
//   4. Exit 1 with a precise message (file:line, the two attribute states,
//      what the sentinel says vs. what the source says) on ANY mismatch or
//      unresolved sentinel (a sentinel this script cannot resolve to a real
//      `fn` is ALSO a failure — a silently-unverifiable sentinel is exactly
//      as dangerous as a wrong one).
//
// WHAT THIS DOES NOT COVER (see docs/CORRECTNESS_OPEN_ITEMS.md's new card
// for the durable record — the "does and does NOT cover" split the task
// brief asked for):
//   - Sentinels are matched to `fn` definitions by NAME only. A test that
//     is renamed on BOTH sides at once (the `fn` and its sentinel string
//     both changed to a new, still-consistent name) is invisible to this
//     script — it only catches attribute/sentinel-FORM drift, not "does
//     this sentinel still name a test that exists" (a renamed-away-from
//     name would simply fail to resolve and be caught by point 4 above,
//     but a coincidentally-still-resolvable stale name would not).
//   - Does not verify the `$RUNNER_TEMP/<log>` filename inside a step is
//     unique/consistent, or that the `--test`/`-p` argv this script parses
//     is byte-identical to what actually runs in CI (that is a parse of
//     the YAML step text, not an execution).
//   - Does not run cargo, so it cannot catch a sentinel whose test NAME is
//     correct and attributes match, but whose test BODY no longer compiles
//     under the step's stated feature set (a separate, already-covered
//     concern — that is what running CI/`npm run check` actually catches).
//   - Only understands the `#[should_panic]` / `#[ignore]` attribute pair.
//     A future libtest output change (e.g. a new `#[test]` variant with its
//     own suffix) is not modeled and would need a new branch here.

import fs from 'node:fs';
import path from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const CI_YML = path.join(REPO_ROOT, '.github', 'workflows', 'ci.yml');

/** Map of workspace member crate name -> its tests/ dir, relative to REPO_ROOT. */
const CRATE_TEST_DIRS = {
  'aligned-vmem': 'crates/aligned-vmem/tests',
  'numa-shim': 'crates/numa-shim/tests',
  'malloc-bench-rs': 'crates/malloc-bench-rs/tests',
  'sefer-region': 'crates/sefer-region/tests',
  'racy-ptr-cell': 'crates/racy-ptr-cell/tests',
  'size-classes': 'crates/size-classes/tests',
  'globalalloc-model': 'crates/globalalloc-model/tests',
  'tagged-index-stack': 'crates/tagged-index-stack/tests',
  'proc-memstat': 'crates/proc-memstat/tests',
  'proc-probe': 'crates/proc-probe/tests',
};

/**
 * Split ci.yml into "step blocks": each block is the text of one `- run:`
 * step (inline `- run: cmd` or block-scalar `- run: |` ... indented lines),
 * tagged with its 1-based start line number. A sentinel and the cargo
 * invocation it checks are always co-located in the SAME step (verified
 * against every current sentinel site) — restricting the search to
 * one-step windows is what keeps target resolution unambiguous instead of
 * accidentally matching a `--test` flag from a neighboring, unrelated step.
 */
function splitIntoRunSteps(lines) {
  const steps = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    // `run:` may open a step either as `- run: <cmd>` / `- run: |` (the
    // step's ONLY key) or, when a step also carries a `name:` on its own
    // `- name: ...` line above, as a bare `run: <cmd>` / `run: |` line at
    // the SAME indentation as that step's other keys (`shell:`, `env:`,
    // ...) — e.g. the loom-misc job's `- name: loom_thread_free` /
    // `        run: |` pair. Both shapes are matched identically here; only
    // the leading `- ` is optional.
    const inlineMatch = line.match(/^(\s*)(?:-\s+)?run:\s+(.+)$/);
    const blockMatch = line.match(/^(\s*)(?:-\s+)?run:\s*\|-?\s*$/);
    if (blockMatch) {
      const indent = blockMatch[1].length;
      const bodyLines = [];
      const startLine = i + 1;
      let j = i + 1;
      while (j < lines.length) {
        const l = lines[j];
        if (l.trim() === '') {
          bodyLines.push(l);
          j++;
          continue;
        }
        const thisIndent = l.match(/^(\s*)/)[1].length;
        if (thisIndent <= indent) break;
        bodyLines.push(l);
        j++;
      }
      steps.push({ startLine, text: bodyLines.join('\n') });
      i = j;
      continue;
    }
    if (inlineMatch) {
      steps.push({ startLine: i + 1, text: inlineMatch[2] });
      i++;
      continue;
    }
    i++;
  }
  return steps;
}

/** Extract every `--test <target>` token from a step's text, in order. */
function extractTestTargets(stepText) {
  const targets = [];
  const re = /--test\s+(\S+)/g;
  let m;
  while ((m = re.exec(stepText))) targets.push(m[1]);
  return targets;
}

/** Extract the `-p <crate>` package scope, if any (first occurrence). */
function extractPackageScope(stepText) {
  const m = stepText.match(/-p\s+(\S+)/);
  return m ? m[1] : null;
}

/**
 * Extract every `grep -F "test <sentinel-body>" "$RUNNER_TEMP/<log>"` line
 * from a step's text, in the order they appear (this order matches the
 * `--test` target declaration order for multi-target steps — verified
 * against every current loom step).
 */
function extractSentinels(stepText) {
  const sentinels = [];
  const re = /grep -F "test (.+?)"\s+"\$RUNNER_TEMP\/[^"]+"/g;
  let m;
  while ((m = re.exec(stepText))) {
    const body = m[1];
    const shouldPanic = body.endsWith(' - should panic ... ok');
    const plain = body.endsWith(' ... ok');
    if (!shouldPanic && !plain) {
      sentinels.push({ raw: body, name: null, shouldPanic: null, malformed: true });
      continue;
    }
    const name = shouldPanic
      ? body.slice(0, -' - should panic ... ok'.length)
      : body.slice(0, -' ... ok'.length);
    sentinels.push({ raw: body, name, shouldPanic, malformed: false });
  }
  return sentinels;
}

/**
 * Resolve a (possibly `mod::`-qualified) test name to a source location by
 * scanning candidate .rs files. Returns { file, line, hasShouldPanic,
 * hasIgnore } or null if not found. `mod::name` requires the found `fn` to
 * sit inside a `mod <modname> { ... }` block whose name matches — one level
 * of module resolution, per the task brief's `forced_page::...` example.
 */
function resolveTestLocation(candidateFiles, sentinelName) {
  const parts = sentinelName.split('::');
  const bareFn = parts[parts.length - 1];
  const wantMod = parts.length > 1 ? parts[parts.length - 2] : null;

  const fnRe = new RegExp(`^\\s*(?:pub(?:\\([^)]*\\))?\\s+)?(?:async\\s+)?fn\\s+${bareFn}\\s*[(<]`);

  for (const file of candidateFiles) {
    if (!fs.existsSync(file)) continue;
    const src = fs.readFileSync(file, 'utf8');
    const lines = src.split(/\r?\n/);
    // Track a simple brace-depth-based mod stack: push on `mod X {`, pop
    // when that mod's opening brace closes. Good enough for one level of
    // nesting on well-formed Rust source (no need for a full parser here —
    // the only nested case in this repo today is `mod forced_page { ... }`
    // directly containing its `#[test] fn`s, verified by direct read).
    const modStack = [];
    for (let idx = 0; idx < lines.length; idx++) {
      const line = lines[idx];
      const modMatch = line.match(/^\s*(?:#\[[^\]]*\]\s*)*mod\s+(\w+)\s*\{/);
      if (modMatch) {
        modStack.push({ name: modMatch[1], depth: 0 });
      }
      // Track brace depth for stack pop bookkeeping.
      if (modStack.length > 0) {
        const opens = (line.match(/\{/g) || []).length;
        const closes = (line.match(/\}/g) || []).length;
        for (const frame of modStack) frame.depth += opens - closes;
        // A frame whose depth has returned to <= 0 AFTER this line (and this
        // line was not the line that opened it) has closed.
        while (
          modStack.length > 0 &&
          modStack[modStack.length - 1].depth <= 0 &&
          !modMatch
        ) {
          modStack.pop();
        }
      }
      if (fnRe.test(line)) {
        const currentMod = modStack.length > 0 ? modStack[modStack.length - 1].name : null;
        if (wantMod !== null && currentMod !== wantMod) continue;
        if (wantMod === null && currentMod !== null) {
          // Bare name requested but this fn lives inside a mod — only a
          // match if the sentinel truly meant the unqualified name AND no
          // module qualification is required by convention here; be
          // permissive (allow it) since the mod-qualification requirement
          // is driven by the sentinel string itself, not by source shape.
        }
        // Walk upward through attribute / blank lines to find #[should_panic]
        // and #[ignore].
        let hasShouldPanic = false;
        let hasIgnore = false;
        let k = idx - 1;
        while (k >= 0) {
          const attrLine = lines[k].trim();
          if (attrLine === '') {
            k--;
            continue;
          }
          if (attrLine.startsWith('#[') || attrLine.startsWith('//')) {
            if (/^#\[should_panic\b/.test(attrLine)) hasShouldPanic = true;
            if (/^#\[ignore\]/.test(attrLine)) hasIgnore = true;
            k--;
            continue;
          }
          break;
        }
        return { file, line: idx + 1, hasShouldPanic, hasIgnore };
      }
    }
  }
  return null;
}

/** List every .rs file directly under a tests/ directory (non-recursive is enough — this repo's tests/ dirs are flat). */
function listTestFiles(testsDir) {
  const abs = path.join(REPO_ROOT, testsDir);
  if (!fs.existsSync(abs)) return [];
  return fs
    .readdirSync(abs)
    .filter((f) => f.endsWith('.rs'))
    .map((f) => path.join(abs, f));
}

function verifyCiSentinels() {
  const raw = fs.readFileSync(CI_YML, 'utf8');
  const lines = raw.split(/\r?\n/);
  const steps = splitIntoRunSteps(lines);

  const errors = [];
  let checkedCount = 0;

  for (const step of steps) {
    const sentinels = extractSentinels(step.text);
    if (sentinels.length === 0) continue;

    const testTargets = extractTestTargets(step.text);
    const pkgScope = extractPackageScope(step.text);

    for (const sentinel of sentinels) {
      checkedCount++;
      if (sentinel.malformed) {
        errors.push(
          `step starting at ci.yml:${step.startLine}: sentinel "${sentinel.raw}" does not end ` +
            `in " ... ok" or " - should panic ... ok" — cannot classify, treating as a failure ` +
            `(a sentinel this script cannot parse is exactly as dangerous as one it parses wrong).`,
        );
        continue;
      }

      // Build the candidate file list for this sentinel.
      let candidateFiles = [];
      if (testTargets.length > 0) {
        // Prefer the specific --test target(s) whose file actually defines
        // the bare fn name; if only one target is present, use it directly.
        const bareFn = sentinel.name.split('::').pop();
        const dirs = pkgScope && CRATE_TEST_DIRS[pkgScope]
          ? [CRATE_TEST_DIRS[pkgScope]]
          : ['tests'];
        const targetFiles = testTargets
          .map((t) => dirs.map((d) => path.join(REPO_ROOT, d, `${t}.rs`)))
          .flat();
        if (testTargets.length === 1) {
          candidateFiles = targetFiles;
        } else {
          // Multi-target step: find which target file actually defines the
          // bare fn name, to disambiguate without relying on grep order.
          const matches = targetFiles.filter((f) => {
            if (!fs.existsSync(f)) return false;
            const src = fs.readFileSync(f, 'utf8');
            return new RegExp(`\\bfn\\s+${bareFn}\\s*[(<]`).test(src);
          });
          candidateFiles = matches.length > 0 ? matches : targetFiles;
        }
      } else if (pkgScope && CRATE_TEST_DIRS[pkgScope]) {
        // No --test flag: whole-package suite. Search every tests/*.rs file
        // under that crate.
        candidateFiles = listTestFiles(CRATE_TEST_DIRS[pkgScope]);
      } else {
        // No --test flag, no recognized -p scope: fall back to the root
        // tests/ tree (whole-workspace root-crate suite shape).
        candidateFiles = listTestFiles('tests');
      }

      const resolved = resolveTestLocation(candidateFiles, sentinel.name);
      if (!resolved) {
        errors.push(
          `step starting at ci.yml:${step.startLine}: sentinel "test ${sentinel.raw}" names ` +
            `"${sentinel.name}", which could not be resolved to a real \`fn\` in any candidate ` +
            `file (${candidateFiles.length > 0 ? candidateFiles.map((f) => path.relative(REPO_ROOT, f)).join(', ') : '<no candidates — unrecognized -p scope and no --test flag>'}). ` +
            `An unresolvable sentinel is a failure: it cannot be verified to still name a real test.`,
        );
        continue;
      }

      const relFile = path.relative(REPO_ROOT, resolved.file);
      if (resolved.hasIgnore) {
        errors.push(
          `${relFile}:${resolved.line}: "${sentinel.name}" is #[ignore]d — this test NEVER RUNS, ` +
            `so its ci.yml sentinel (step at ci.yml:${step.startLine}) can never match, in either ` +
            `form. This is a distinct defect class from a should_panic/plain mismatch: the sentinel ` +
            `must be removed (or the step's postcondition redesigned) if the test is intentionally ` +
            `ignored, not just re-worded.`,
        );
        continue;
      }
      if (resolved.hasShouldPanic !== sentinel.shouldPanic) {
        errors.push(
          `${relFile}:${resolved.line}: "${sentinel.name}" ${
            resolved.hasShouldPanic ? 'HAS' : 'does NOT have'
          } #[should_panic], but its ci.yml sentinel (step at ci.yml:${step.startLine}) is in the ` +
            `${sentinel.shouldPanic ? '"- should panic ... ok"' : '"... ok"'} form. libtest prints ` +
            `${resolved.hasShouldPanic ? '"test NAME - should panic ... ok"' : '"test NAME ... ok"'} ` +
            `for this test — the sentinel can never match as written. Fix the sentinel string in ` +
            `ci.yml to ${resolved.hasShouldPanic ? 'add' : 'remove'} " - should panic".`,
        );
      }
    }
  }

  return { checkedCount, errors };
}

const { checkedCount, errors } = verifyCiSentinels();
if (errors.length > 0) {
  console.log(`[verify-ci-sentinels] FAIL — ${errors.length} of ${checkedCount} sentinel(s) mismatched:\n`);
  for (const e of errors) console.log(`  - ${e}\n`);
  process.exit(1);
}
console.log(`[verify-ci-sentinels] OK — ${checkedCount} sentinel(s) checked against their real \`fn\` attributes, all match.`);
process.exit(0);
