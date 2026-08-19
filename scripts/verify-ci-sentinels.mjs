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
// Task #1162 added a SECOND sentinel shape this script recognizes: a
// "marker sentinel" is a `grep -F "<literal>" "$RUNNER_TEMP/<log>"` line
// whose literal does NOT start with `test ` — a deliberate stdout marker a
// test prints itself (e.g. `println!("[oracle] ARMED: ...")`) to make an
// otherwise output-indistinguishable branch (armed vs. unarmed) observable
// in the CI log. See `extractLiteralMarkers` below; it is resolved against
// a LIVE (not commented-out) `println!("...")` occurrence that sits AFTER a
// preceding `assert!`/`assert_eq!`/`assert_ne!`/`debug_assert*!` call inside
// SOME enclosing `fn` in a candidate source file (see
// `resolveMarkerLocation`, task #1167/F4) — not against a
// `#[should_panic]`/`#[ignore]` attribute pair (a marker doesn't name a
// `fn`, so that check does not apply to it). Deleting the `println!`,
// commenting it out, or moving it ahead of the assertion it exists to gate
// on WITHIN ITS OWN FUNCTION all now surface as a resolution failure. This
// is a real, but bounded, fix — a marker carries no `fn` name (unlike a
// `test <name> ... ok` sentinel), so the resolver cannot tell "some
// enclosing fn with a preceding assert" apart from "the ONE specific fn
// this marker is meant to gate on": a print relocated into a DIFFERENT
// test function that also has its own preceding assert still resolves.
// See `resolveMarkerLocation`'s own doc comment for the precise, verified
// (not assumed) boundary of what this catches and what it does not.
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
//   5. Task #1176/M3: for every `cargo test ...` invocation in a step that
//      `tee`s its output to a `$RUNNER_TEMP/<log>` file, if that invocation
//      passes `-- ... --nocapture ...` WITHOUT also passing `--exact <name>`
//      (single-test isolation), AND the same step greps a `test ... ok`
//      result-line sentinel out of that SAME log file — FAIL. This is a
//      static guard against a regression of task #1166's own fix (see
//      `checkNocaptureRegression`'s doc comment for the full rationale): a
//      `--nocapture` multi-test run's unsynchronized worker-thread
//      `println!`s can land mid-write of another test's own result line and
//      corrupt the exact string `grep -F` depends on.
//   6. Task #1183: YAML-side liveness. A `grep -F ... "$RUNNER_TEMP/..."`
//      line counts as a sentinel/marker ONLY if it sits on a LIVE line —
//      no `#` comment open at word start before the match
//      (`grepMatchIsCommented`). A full-form grep on a commented line is
//      its own error class and does NOT count toward checkedCount: before
//      #1183 the extractors matched blind to comment status, so
//      commenting out a postcondition kept the count (and the guard)
//      green — fail-OPEN, the third side of the same "text present but
//      not live code" triangle #1176/M2 closed on the .rs side. Latent
//      at fix time: every prose comment in ci.yml mentioning `grep -F`
//      lacked the full `"$RUNNER_TEMP/<log>"` form and never matched
//      (re-derive via `grep -n 'grep -F' .github/workflows/ci.yml`);
//      one edit in that form would have activated the hole.
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
//   - Does NOT model `#[cfg(...)]` on the resolved `fn` against the step's
//     `runs-on` (F13, task #1157). The attribute walk (see step 3
//     above) reads only `#[should_panic]`/`#[ignore]`; it does not evaluate
//     a `#[cfg(target_os = "...")]`/`#[cfg(windows)]`/`#[cfg(unix)]`/
//     `#[cfg(any(...))]`/`#[cfg(all(...))]`/`#[cfg(not(...))]` predicate on
//     the same `fn` against the OS the enclosing ci.yml job's `runs-on:`
//     actually runs on. A sentinel naming a `#[cfg(windows)]` test inside a
//     step whose job is `runs-on: ubuntu-latest` would resolve to a real
//     `fn`, match attributes, and exit 0 here — then never appear in that
//     job's real log at all, because the `#[cfg]` excludes the whole `fn`
//     from compilation on that platform. This is the same "green and dead"
//     class this script exists to prevent, just gated on platform instead
//     of on attribute form.
//     Two fixes were weighed: (a) parse `#[cfg(...)]` on the resolved `fn`
//     and evaluate it against a `runs-on` -> target-os mapping, which is the
//     real fix but needs a small cfg-expression evaluator (`any`/`all`/
//     `not`, `target_os = "..."`, `windows`, `unix`) plus that mapping,
//     each an independent place to be subtly wrong; (b) document the class
//     here and leave it unhandled. (b) was chosen for now: a census at the
//     time of this finding found 230+ platform-predicate `#[cfg(...)]`
//     sites across this repo's `tests/` trees (`grep -rnE
//     "cfg\((any\(|all\(|not\(|target_os|windows|unix)" crates/*/tests/*.rs
//     tests/*.rs | grep -v feature | wc -l`), so a correct evaluator is real
//     surface, not a two-line patch, and every sentinel-tracked test today
//     is either unconditional or (the one real case: the hugetlb-page test
//     naming `#[cfg(any(target_os = "linux", target_os = "android"))]` in
//     `crates/aligned-vmem/tests/huge_pages.rs`, run from an
//     `ubuntu-latest` job) already platform-consistent — the class is
//     currently LATENT, not live, so the cost of a possibly-subtly-wrong
//     evaluator was judged higher than the value of closing a blind spot
//     with zero current instances. This is a documented gap, not a fixed
//     one — re-evaluate (a) if a sentinel is ever added for a `#[cfg]`-gated
//     test whose gate does not obviously match its step's `runs-on`.
//   - The `#` liveness test (point 6) models a comment opening at line
//     start or after whitespace — the shell "word beginning with #"
//     form. A `#` directly after a shell control operator with no
//     intervening space (`;#`, `&&#`) also opens a real comment but is
//     not modeled (no such token exists in ci.yml today, verified by
//     grep), and a `#` inside a quoted string earlier on the same line
//     as a live grep is a FALSE REJECT — a loud FAIL, not a silent pass
//     (fail-closed; the same trade the .rs-side `//` checks make for
//     string literals). Also note `checkNocaptureRegression` (point 5)
//     still matches its sentinel regexes without liveness, so a
//     commented-out `test ... ok` line can trip IT too — a fail-closed
//     false positive that is subsumed: any step holding such a line
//     already fails via point 6's own dead-sentinel error plus the
//     floor drop (the dead line no longer counts toward checkedCount).

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
    //
    // Block-scalar header (F12, task #1157): YAML's block scalar
    // header is a BLOCK indicator (`|` literal or `>` folded), optionally
    // followed by a chomping indicator (`-` strip / `+` keep) and/or an
    // explicit indentation indicator (a single digit 1-9), and the two
    // optional indicators may appear in EITHER order (`|2-` and `|-2` are
    // both legal per the YAML spec). The prior regex (`\|-?`) matched only
    // the literal indicator with an optional trailing `-` — it silently
    // treated any `run: >...` (folded scalar) line as an INLINE command
    // whose "body" is the literal indicator text itself (e.g. the two
    // characters `>-`), discarding the real multi-line command entirely.
    // That was harmless while no `grep -F` sentinel line lived inside a
    // folded-scalar step (verified for all 9 such steps in ci.yml at the
    // time this was fixed — see the F12 finding), but a sentinel later
    // moved into, or added inside, a `run: >-` step would become invisible
    // to this script with exit 0. This regex now accepts both indicators
    // and both indicator orders.
    const inlineMatch = line.match(/^(\s*)(?:-\s+)?run:\s+(.+)$/);
    const blockMatch = line.match(/^(\s*)(?:-\s+)?run:\s*[|>](?:[1-9][+-]?|[+-]?[1-9]?)\s*$/);
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
 * Task #1183: the YAML/shell-side liveness test — the THIRD side of the
 * "text present but not live code" triangle. The two .rs-side checks in
 * `resolveMarkerLocation` (`println!` liveness, assert liveness) reject
 * dead text via `//`; this one rejects dead text via `#` — inside a
 * `run: |` block scalar the step text is SHELL, where a `#` at word
 * start (line start or after whitespace) opens a comment, and a fully
 * commented `# grep -F ...` line is dead either way. Before this check,
 * `extractSentinels`/`extractLiteralMarkers` matched their regexes blind
 * to comment status, so commenting out a postcondition left
 * `checkedCount` unchanged and the guard green (fail-OPEN) while CI no
 * longer checked anything — disabling the very postcondition this guard
 * exists to verify passed silently. Returns true when a `#` comment is
 * open at word start at or before `matchIndex` on `matchIndex`'s own
 * line (the line the grep match starts on governs the whole command,
 * including any `\`-continued tail).
 *
 * Approximation, deliberately: a `#` directly after a shell control
 * operator with no intervening space (`;#`, `&&#`) also opens a real
 * shell comment but is NOT modeled — no such token exists in ci.yml
 * today (verified: `grep -nE ';#|&#' .github/workflows/ci.yml` is
 * empty). A `#` inside a quoted string earlier on the same line as a
 * live grep is a FALSE REJECT: a loud FAIL, not a silent pass —
 * fail-closed, the same trade the .rs-side `//` checks already make for
 * string literals (see the note inside `resolveMarkerLocation`); no
 * such line exists today either.
 */
function grepMatchIsCommented(stepText, matchIndex) {
  const lineStart = stepText.lastIndexOf('\n', matchIndex - 1) + 1;
  const prefix = stepText.slice(lineStart, matchIndex);
  return /(^|\s)#/.test(prefix);
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
    if (grepMatchIsCommented(stepText, m.index)) {
      // Task #1183: a full-form grep on a commented line is dead text —
      // surface it as its own failure, never as a counted sentinel.
      sentinels.push({ raw: body, dead: true });
      continue;
    }
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
 * Task #1162: extract every `grep -F "<literal>" "$RUNNER_TEMP/<log>"` line
 * whose literal does NOT start with `test ` (that shape is a libtest
 * result-line sentinel, handled by extractSentinels above — a "marker
 * sentinel" is a distinct, deliberate stdout marker a test prints itself,
 * e.g. `println!("[oracle] ARMED: ...")`, used to make an otherwise
 * indistinguishable armed/unarmed outcome observable in CI's log). Unlike a
 * `test <name> ... ok` sentinel, a marker sentinel is not resolved against a
 * `#[should_panic]`/`#[ignore]` attribute pair (it does not name a `fn` at
 * all) — it is instead resolved against a literal, LIVE (not commented-out)
 * `println!("<same text>")` occurrence that sits AFTER at least one
 * `assert!`/`assert_eq!`/`assert_ne!`/`debug_assert*!` call inside the SAME
 * enclosing `fn` (see `resolveMarkerLocation` below — task #1167/F4). This
 * exists specifically so a marker sentinel counts toward
 * `checkedCount`/`MIN_SENTINEL_COUNT` (deleting the `println!` that emits
 * it, or the CI line that greps for it, both surface as a floor/count
 * change) without widening the `fn`-attribute matching logic above to cover
 * a case it was never designed for.
 */
function extractLiteralMarkers(stepText) {
  const markers = [];
  const re = /grep -F "([^"]+)"\s+"\$RUNNER_TEMP\/[^"]+"/g;
  let m;
  while ((m = re.exec(stepText))) {
    const body = m[1];
    if (body.startsWith('test ')) continue; // handled by extractSentinels
    if (grepMatchIsCommented(stepText, m.index)) {
      markers.push({ raw: body, dead: true });
      continue;
    }
    markers.push({ raw: body });
  }
  return markers;
}

/**
 * Task #1176/M3: static guard against a REGRESSION of task #1166's own fix.
 *
 * THE GAP: `6ba1f31` (#1166) correctly diagnosed and fixed a real corruption
 * bug — running the FULL multi-test `aligned-vmem-hugetlb-real` invocation
 * with `-- --nocapture` let two worker threads' unsynchronized `println!`s
 * land mid-write of another test's own `"test {name} ... "` / outcome-word /
 * newline triple, corrupting `grep -F "test <name> ... ok"` sentinel lines
 * (measured 11/400 runs locally; see that step's own comment in ci.yml). The
 * fix — split the marker-printing tests out into their own single-test
 * `--exact <name> -- --nocapture` invocations, run the shared multi-test
 * invocation WITHOUT `--nocapture` — is correct, but nothing in this script
 * (or anywhere else) stops a future edit from putting `-- --nocapture` back
 * onto a multi-test invocation whose output a `grep -F "test ... ok"`
 * sentinel depends on. The only thing currently preventing that regression
 * is a comment in ci.yml (see the "Task #1166 (F1)" note above the
 * `aligned-vmem-hugetlb-real` step) — comments do not fail CI.
 *
 * WHAT THIS CHECKS: within one step's text, a `cargo test ...` invocation is
 * paired with the `$RUNNER_TEMP/<log>.log` file it `tee`s its output to (the
 * existing, already-established convention every sentinel-bearing invocation
 * in this repo already follows — see the module header's "sentinel and the
 * cargo invocation it checks are always co-located" note for the analogous
 * per-STEP version of this same convention, here narrowed to per-INVOCATION
 * since one step can hold several `cargo test` calls, e.g. the
 * `aligned-vmem-hugetlb-real` step's three). For each such
 * invocation-to-logfile pairing:
 *   1. Determine whether the invocation passes `--nocapture` after a `--`
 *      libtest-args separator, ANYWHERE in the invocation's own text
 *      (spanning its `\`-continued lines, matching how this script already
 *      treats a step's `\`-joined block-scalar body as one text blob
 *      elsewhere — see `splitIntoRunSteps`).
 *   2. Determine whether the SAME invocation also passes `--exact <name>`
 *      — the established safe form (verified live in this file today: both
 *      `aligned-vmem-hugetlb-real`'s marker sub-invocations and the CI-wide
 *      `numa_alloc`/`numa_segment_id`/`numa_seam` steps use `--nocapture`
 *      freely, but the numa steps carry NO `grep -F "test ... ok"` sentinel
 *      at all in the same step, so they are unaffected either way; only a
 *      `--nocapture` invocation that ALSO backs a `test ... ok` sentinel is
 *      in scope here — see point 3).
 *   3. If `--nocapture` is present, `--exact` is NOT present, AND the same
 *      step's text contains at least one `grep -F "test ... ok"       "$RUNNER_TEMP/<same log file>"` line (i.e. a
 *      libtest-result-line sentinel depends on this exact invocation's
 *      output) — FAIL. This is deliberately narrower than "no `--nocapture`
 *      without `--exact` anywhere in ci.yml": a `--nocapture` invocation
 *      with NO `test ... ok` sentinel riding on its log file (like the numa
 *      steps) cannot reproduce #1166's corruption class, because nothing
 *      greps a byte-exact result line out of that log.
 *
 * WHY THE `--exact` TEST, NOT "only one `--test` target": the two SAFE
 * invocations verified live in ci.yml today (`vmem-hugetlb-real-marker1`/
 * `marker2`) each still declare a SINGLE `--test decommit_capability`
 * target, but that target file holds many `#[test]` fns — `--exact <name>`
 * is what actually narrows execution down to the one test whose print can
 * no longer race any OTHER test's own result-line write, which is the
 * actual precondition #1166's counterfactual needed ("no other test running
 * concurrently to interleave with", per that step's own comment). A
 * `--test`-count check alone would not have verified the real safety
 * property and would have OK'd a single-target-but-multi-fn invocation that
 * is exactly as unsafe as the multi-target one #1166 fixed.
 *
 * NOT COVERED (documented gap, not oversold as fixed):
 *   - Does not verify #1166's own quantitative claim (11/400 corruption
 *     rate) still holds; it is a structural/textual guard against the
 *     SHAPE of the regression (`--nocapture` reintroduced onto a
 *     multi-test invocation a result-line sentinel depends on), not a
 *     re-run of the probabilistic counterfactual itself.
 *   - Does not catch a step that greps a `test ... ok` sentinel against a
 *     DIFFERENT step's or job's log file (this repo's convention is
 *     strictly one step per `tee` target, verified against every current
 *     site — see the module header), nor a `cargo test` invocation whose
 *     output is not `tee`'d to a `$RUNNER_TEMP/*.log` file at all (in which
 *     case no sentinel could depend on it in the first place, so the
 *     regression class this guard targets cannot arise from it).
 *   - Does not evaluate `--test-threads=1`: the ci.yml comment this
 *     function guards against explicitly notes `--test-threads=1` does NOT
 *     fix the corruption (it makes a PRINTING test's own line corruption
 *     deterministic instead of rare, not safe) — so this guard correctly
 *     does not treat `--test-threads=1` as an escape hatch; only `--exact`
 *     (single-test isolation) is accepted as safe.
 *   - The `-- ` libtest-args separator is located with `text.search(/\s--\s/)`
 *     (`\s` matches a newline, so a `\`-continued ` -- \` on its own line is
 *     found correctly — verified live against this exact shape) — but this
 *     is still a textual heuristic, not a real argv tokenizer. A `--`
 *     appearing as part of a QUOTED string argument earlier in the same
 *     invocation (e.g. inside a `--features "..."` value) could in
 *     principle confuse `dashDashIdx`; not observed anywhere in this
 *     repo's ci.yml today (verified: every `--features "..."` value in a
 *     step containing a `test ... ok` sentinel is a plain space-separated
 *     feature list, no embedded ` -- `), and no different in kind from the
 *     text-scan tradeoff already accepted throughout this file (e.g.
 *     `extractTestTargets`/`extractPackageScope` above).
 *   - `--exact` is accepted as sufficient proof of single-test isolation
 *     without checking that the test NAME it names is the same one whose
 *     `println!` a marker sentinel resolves against, or that only ONE
 *     `--exact` flag is present — a hypothetical `--exact a --exact b`
 *     (cargo actually only honors the last one) is not specially detected;
 *     this mirrors the same "form, not full semantics" scope every other
 *     check in this file already accepts, and no such multi-`--exact`
 *     invocation exists in this repo's ci.yml today.
 */
function extractCargoTestInvocations(stepText) {
  const invocations = [];
  // Split on each `cargo test` occurrence; the text from one `cargo test` up
  // to (but not including) the next is that invocation's own text. This
  // mirrors `--test`/`-p` extraction elsewhere in this file, which already
  // operates on whole-step text without a real shell/YAML parser.
  const starts = [];
  const startRe = /\bcargo test\b/g;
  let sm;
  while ((sm = startRe.exec(stepText))) starts.push(sm.index);
  for (let i = 0; i < starts.length; i++) {
    const from = starts[i];
    const to = i + 1 < starts.length ? starts[i + 1] : stepText.length;
    const text = stepText.slice(from, to);
    const teeMatch = text.match(/tee\s+"\$RUNNER_TEMP\/([^"]+)"/);
    if (!teeMatch) continue; // no log file -> no sentinel can depend on this invocation
    // libtest args live after a ` -- ` separator (space-bounded, so this
    // does not match `--nocapture`/`--exact` themselves, only the
    // separator token itself).
    const dashDashIdx = text.search(/\s--\s/);
    const libtestArgs = dashDashIdx === -1 ? '' : text.slice(dashDashIdx);
    invocations.push({
      logFile: teeMatch[1],
      hasNocapture: /--nocapture\b/.test(libtestArgs),
      hasExact: /--exact\b/.test(libtestArgs),
    });
  }
  return invocations;
}

/**
 * Task #1176/M3: the check itself — see `extractCargoTestInvocations`'s doc
 * comment immediately above for the full design rationale. Returns an array
 * of error strings (empty if the step is clean).
 */
function checkNocaptureRegression(stepText, startLine) {
  const errors = [];
  const invocations = extractCargoTestInvocations(stepText);
  for (const inv of invocations) {
    if (!inv.hasNocapture || inv.hasExact) continue;
    const sentinelRe = new RegExp(
      `grep -F "test (.+?) \\.\\.\\. ok"\\s+"\\$RUNNER_TEMP/${inv.logFile.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}"`,
    );
    const shouldPanicRe = new RegExp(
      `grep -F "test (.+?) - should panic \\.\\.\\. ok"\\s+"\\$RUNNER_TEMP/${inv.logFile.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}"`,
    );
    if (sentinelRe.test(stepText) || shouldPanicRe.test(stepText)) {
      errors.push(
        `step starting at ci.yml:${startLine}: a \`cargo test\` invocation piping to ` +
          `"$RUNNER_TEMP/${inv.logFile}" passes \`--nocapture\` without \`--exact\` (single-test ` +
          `isolation), while this step also greps a \`test ... ok\` result-line sentinel out of ` +
          `that same log file. This is the exact regression task #1166 fixed and documented: an ` +
          `unsynchronized worker-thread \`println!\` under \`--nocapture\` can land mid-write of ` +
          `ANOTHER test's own "test {name} ... " / outcome-word / newline triple, corrupting the ` +
          `sentinel line \`grep -F\` depends on (measured 11/400 runs — see that step's own comment ` +
          `in ci.yml). Either drop \`--nocapture\` from this invocation (libtest captures a passing ` +
          `test's stdout by default) or restrict it to one test with \`--exact <name>\`.`,
      );
    }
  }
  return errors;
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

/** Escape a literal string for safe embedding inside a RegExp source. */
function escapeRegExp(literal) {
  return literal.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Task #1167 (F4, closing a gap review @ox found in the original task #1162
 * resolver): resolve a marker sentinel's literal to a LIVE `println!(...)`
 * call site that sits after at least one `assert!`/`assert_eq!`/
 * `assert_ne!`/`debug_assert*!` invocation inside the SAME enclosing `fn`.
 *
 * THE GAP THIS CLOSES: the original resolver (`src.includes(...)`, task
 * #1162) only checked that the literal text appeared ANYWHERE in one of the
 * candidate files — with no check that the occurrence was live code (a
 * commented-out `// println!("...")` still satisfies `String.includes`) and
 * no check that the print actually followed the assertion it is meant to
 * make observable. Commenting out the `println!`, or moving it above the
 * `is_huge()`/kernel-acceptance assert it exists to gate on (or into an
 * unrelated test entirely), left the guard AND the CI `grep -F` both green
 * while the oracle itself was fully disarmed — the exact "green and dead"
 * defect class task #1162 was written to prevent in the first place.
 *
 * WHAT THIS CHECKS (deliberately NOT a general Rust parser — see the
 * module-level scope note above the marker resolver's header comment for
 * why a full evaluator was rejected for the analogous `#[cfg]` case; the
 * same cost/benefit applies here):
 *   1. The literal must appear as a LIVE (non-commented) `println!("<lit>")`
 *      or `println!("<lit>");` occurrence — a line whose content up to the
 *      match is not a `//` line comment is treated as live. This does not
 *      attempt to strip `/* ... *\/` block comments or macro-generated
 *      code; see the "NOT COVERED" note below.
 *   2. The occurrence must be textually inside a `fn ... { ... }` body
 *      (found by walking outward via brace-depth tracking from the matched
 *      line to the nearest enclosing top-level `fn`), and at least one LIVE
 *      (non-commented) `assert!`/`assert_eq!`/`assert_ne!`/`debug_assert!`/
 *      `debug_assert_eq!`/`debug_assert_ne!` invocation must appear on an
 *      EARLIER line inside that same function body. This pins the print's
 *      ordering relative to the assertion it is meant to gate on, without
 *      needing to parse statement boundaries or control flow. "Live" here
 *      uses the identical check as point 1's `println!` liveness test
 *      (content up to the match must not open a `//` line comment) — task
 *      #1176/M2 found the ORIGINAL version of this function applied that
 *      liveness check ONLY to the `println!` side (point 1) and never to
 *      the `assert!` side: `assertRe.test(l)` alone, with no check that `l`
 *      itself wasn't `// assert!(...)`. Measured against this file's own
 *      `assertRe` (task #1184): `.test()` on a fully commented
 *      `// assert!(reservation.is_huge(), "...")` line returns `true` —
 *      dead text matched exactly like live code. So commenting out the
 *      ENTIRE `is_huge()` assert this marker exists to gate on (see the
 *      target test's own doc comment) set `sawAssertBeforeMarker` TRUE
 *      from the dead line, the marker still resolved, and the resolver
 *      reported OK / exit 0 with the oracle's gating assert disarmed —
 *      the exact "green and dead" class above, and the counterfactual
 *      commit `6c5dbbe` records ("comment out the assert! it is ordered
 *      after -> OK/0 <- the gap I left"). The gap only changes an outcome
 *      when the commented line is the one satisfying the
 *      "preceding assert" requirement: with a second, still-LIVE assert
 *      earlier in the same function, the marker resolves via that live
 *      line both before and after the fix, which is correct behavior, not
 *      a miss (see the non-exclusive-assert bullet below). Fixed by
 *      requiring the SAME liveness test as `println!`'s — take the
 *      LEFTMOST assert match on the line and require nothing before it
 *      opens a `//` comment, so dead text no longer counts as "seen": in
 *      the single-assert case `sawAssertBeforeMarker` correctly stays
 *      FALSE, the marker fails to resolve, and the guard goes RED, per
 *      the "CAUGHT as of task #1176/M2" bullet below.
 *
 * NOT COVERED (documented gap, same spirit as the `#[cfg]` gap noted
 * above — verified by an actual counterfactual during this task, not
 * assumed): this is a textual/line-order heuristic scoped to "some
 * enclosing fn with some preceding LIVE assert", NOT "the ONE specific fn
 * this marker's ci.yml comment/doc says it belongs to" — a marker
 * sentinel, by construction (see extractLiteralMarkers above), carries no
 * fn name to check against, so nothing here can tell that a print resolved
 * inside the WRONG function. Several concrete cases, all checked:
 *   - relocating the print ABOVE the assert it exists to gate on, in its
 *     OWN function: CAUGHT (no preceding assert found at that position).
 *   - commenting out the print entirely, or deleting it: CAUGHT (point 1's
 *     liveness check / no occurrence at all).
 *   - commenting out the ONE assert the print exists to gate on, in a
 *     function where that assert was the print's only preceding assertion:
 *     CAUGHT as of task #1176/M2 — the assert-liveness fix above means a
 *     commented-out `// assert!(...)` no longer counts as "seen"; if that
 *     was the function's only assert, `sawAssertBeforeMarker` correctly
 *     stays false and the marker fails to resolve.
 *   - relocating the print into a DIFFERENT, unrelated `#[test] fn` in the
 *     same candidate file that also happens to contain a preceding LIVE
 *     `assert!`: NOT CAUGHT — the resolver finds that occurrence, sees an
 *     earlier live assert in ITS enclosing fn, and reports it resolved.
 *     This is a real, confirmed residual gap of the marker-sentinel design
 *     itself (not just this resolver): closing it would require the marker
 *     sentinel to carry the name of the test function it belongs to (a
 *     format change to how markers are declared in ci.yml, out of scope
 *     for a task confined to this one file), or a source-level convention
 *     making marker text unique to its owning function (which this repo's
 *     `[oracle] ARMED: ...` markers already mostly are, informally, but
 *     nothing enforces it). It would also NOT catch a `println!` moved
 *     into an `if false { ... }` branch still physically below the assert
 *     in the same function, or one moved behind an early `return`/
 *     `continue` that never reaches it on the path the assert also sits
 *     on — both need real control-flow analysis of Rust source, out of
 *     scope for a text-scanning CI guard (the same trade-off already made
 *     for `#[cfg]` predicates above).
 *   - a commented-out assert in a function that ALSO has a second, still-
 *     live, DIFFERENT assert earlier than the marker: NOT specially
 *     handled and does not need to be — `sawAssertBeforeMarker` is a
 *     disjunction over every live assert line seen while walking upward
 *     (see the loop below), so the marker still correctly resolves as
 *     "gated on an assert" (a true statement: it IS still gated on the
 *     surviving live one). This resolver was never designed to pin the
 *     print to one SPECIFIC assert, only to "at least one live assert
 *     precedes it" — commenting out a NON-exclusive assert is not a
 *     disarming edit and is correctly not flagged.
 * What this DOES close is the three defects now verified fixed: commenting
 * the print out, relocating it ahead of the assert it is meant to gate on
 * within its own function, and commenting out the (sole) assert it gates
 * on — all three now fail loudly instead of passing silently. It does NOT
 * close the cross-function relocation case; that residual gap is
 * intentionally not oversold as fixed here.
 */
function resolveMarkerLocation(candidateFiles, literal) {
  const escaped = escapeRegExp(literal);
  const liveRe = new RegExp(`println!\\("${escaped}"\\)`);
  const assertRe = /\b(?:debug_)?assert(?:_eq|_ne)?!\s*\(/;
  const fnOpenRe = /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+\w+\s*[(<][\s\S]*?\{?\s*$/;

  for (const file of candidateFiles) {
    if (!fs.existsSync(file)) continue;
    const src = fs.readFileSync(file, 'utf8');
    const lines = src.split(/\r?\n/);

    for (let idx = 0; idx < lines.length; idx++) {
      const line = lines[idx];
      const m = liveRe.exec(line);
      if (m === null) continue;
      // Liveness: nothing before the match on this line may open a `//` line
      // comment. Checking only `line.trimStart().startsWith('//')` would miss
      // a trailing-comment form (`foo(); // println!("<lit>")`), which is
      // exactly the "text present but not live code" case this resolver
      // exists to reject — the doc comment above promised "content up to the
      // match", so the check has to actually be that (task #1167, zero-trust
      // review of this file's own first revision). A `//` inside an earlier
      // string literal on the same line would be a false reject, i.e. a loud
      // FAIL, not a silent pass — fail-closed, and not observed in this repo.
      if (line.slice(0, m.index).includes('//')) continue;

      // Found a live occurrence. Walk upward to find the nearest enclosing
      // top-level `fn` header (brace-depth 0 relative to this line), and
      // check for a preceding assert inside that same function.
      let depth = 0;
      let fnLine = -1;
      let sawAssertBeforeMarker = false;
      for (let k = idx - 1; k >= 0; k--) {
        const l = lines[k];
        const closes = (l.match(/\}/g) || []).length;
        const opens = (l.match(/\{/g) || []).length;
        // Walking upward: a `}` on an earlier line closes a nested block
        // relative to our marker line, so it INCREASES the depth we must
        // unwind before we're back at the marker's own enclosing scope.
        depth += closes - opens;
        if (depth < 0) {
          // We've walked past the `{` that opens the marker's own
          // enclosing block. Check whether that line is a `fn` header.
          if (fnOpenRe.test(l) && /\bfn\s+\w+/.test(l)) {
            fnLine = k;
            break;
          }
          // Not a fn header at this depth (e.g. an `if`/`match` arm) —
          // keep walking outward one more level.
          depth = 0;
          continue;
        }
        // Task #1176/M2: the assert side needs the SAME liveness check as
        // the println! side above (point 1 of the doc comment) — a
        // commented-out `// assert!(...)` must not count as "seen". Find
        // the LEFTMOST assert-invocation match on this line and require
        // nothing before it opens a `//` line comment; a `.test()` alone
        // (the original, pre-fix form) matches a fully commented-out line
        // just as readily as a live one.
        const am = assertRe.exec(l);
        if (am !== null && !l.slice(0, am.index).includes('//')) {
          sawAssertBeforeMarker = true;
        }
      }

      if (fnLine === -1) continue; // could not locate an enclosing fn; try next occurrence
      if (!sawAssertBeforeMarker) continue; // live, but not proven to gate on an assert; try next occurrence

      return { file, line: idx + 1, fnLine: fnLine + 1 };
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
    errors.push(...checkNocaptureRegression(step.text, step.startLine));

    const markers = extractLiteralMarkers(step.text);
    if (markers.length > 0) {
      const testTargets = extractTestTargets(step.text);
      const pkgScope = extractPackageScope(step.text);
      const dirs = pkgScope && CRATE_TEST_DIRS[pkgScope] ? [CRATE_TEST_DIRS[pkgScope]] : ['tests'];
      const candidateFiles =
        testTargets.length > 0
          ? testTargets.map((t) => dirs.map((d) => path.join(REPO_ROOT, d, `${t}.rs`))).flat()
          : pkgScope && CRATE_TEST_DIRS[pkgScope]
            ? listTestFiles(CRATE_TEST_DIRS[pkgScope])
            : listTestFiles('tests');

      for (const marker of markers) {
        if (marker.dead) {
          errors.push(
            `step starting at ci.yml:${step.startLine}: marker sentinel "${marker.raw}" is COMMENTED OUT in ` +
              `the step text — its line opens a \`#\` comment, so CI never executes this grep, yet the line ` +
              `still carries the full \`grep -F "<literal>" "$RUNNER_TEMP/..."\` shape. Before task #1183 such ` +
              `a line was counted as checked and the guard stayed green (fail-OPEN) while the oracle it names ` +
              `was disabled. Either restore the line (delete the leading \`#\`) or remove it entirely and ` +
              `lower MIN_SENTINEL_COUNT in the same commit, exactly as for a deleted marker.`,
          );
          continue;
        }
        checkedCount++;
        const resolved = resolveMarkerLocation(candidateFiles, marker.raw);
        if (!resolved) {
          errors.push(
            `step starting at ci.yml:${step.startLine}: marker sentinel "${marker.raw}" was not found ` +
              `as a LIVE \`println!("${marker.raw}")\` occurring after a preceding \`assert!\`/\`assert_eq!\`/` +
              `\`assert_ne!\`/\`debug_assert*!\` inside the same enclosing \`fn\`, in any candidate file ` +
              `(${candidateFiles.length > 0 ? candidateFiles.map((f) => path.relative(REPO_ROOT, f)).join(', ') : '<no candidates>'}). ` +
              `An unresolvable marker sentinel is a failure: either the print was deleted, commented out, ` +
              `or moved ahead of (or outside) the assertion it exists to gate on — any of which disarms the ` +
              `oracle this marker is supposed to prove executed.`,
          );
        }
      }
    }

    const sentinels = extractSentinels(step.text);
    if (sentinels.length === 0) continue;

    const testTargets = extractTestTargets(step.text);
    const pkgScope = extractPackageScope(step.text);

    for (const sentinel of sentinels) {
      if (sentinel.dead) {
        errors.push(
          `step starting at ci.yml:${step.startLine}: sentinel "test ${sentinel.raw}" is COMMENTED OUT in ` +
            `the step text — its line opens a \`#\` comment, so CI never executes this grep, yet the line ` +
            `still carries the full \`grep -F "test ... ok" "$RUNNER_TEMP/..."\` shape. Before task #1183 ` +
            `such a line was counted as checked and the guard stayed green (fail-OPEN) while the ` +
            `postcondition it names was disabled — the third side of the liveness triangle the two .rs-side ` +
            `checks in resolveMarkerLocation already close. Either restore the line (delete the leading ` +
            `\`#\`) or remove it entirely and lower MIN_SENTINEL_COUNT in the same commit, exactly as for a ` +
            `deleted sentinel.`,
        );
        continue;
      }
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

// F12 (task #1157) minimum-count floor.
//
// The script's own header already names the mitigation for "a future parser
// blind spot silently drops sentinels from the count": the count is
// printed on every run. That mitigation alone depends on a human noticing a
// SMALLER number scroll past — exactly the failure mode this campaign has
// hit repeatedly (a stale count carried across commits unnoticed). A hard
// assertion is strictly stronger: it turns a silent drop into a RED exit
// code, which is what a CI gate actually needs.
//
// Two designs were weighed:
//   (a) pin the EXACT current count (37, as of task #1152) and require every
//       task that adds a sentinel to bump this literal in the same commit;
//   (b) pin a FLOOR below the current count and only assert the count did
//       not drop below it.
// (a) is loud on every legitimate change (a new sentinel bumps the literal,
// so a forgotten bump fails CI even though nothing regressed) but a floor
// set too low never fires and degenerates back into "just a printed
// number" for any drop that still clears the floor. (b) is the direction
// chosen here, but note it is NOT a get-out-of-loud-updates-free choice:
// the floor is set to the CURRENT exact count, not an arbitrary lower
// number. This keeps the assertion's failure mode identical to (a)'s for
// the case that actually matters here (a silent parser regression that
// drops sentinels below the current count) while ALSO catching the case
// (a) does not — the parser regression this finding describes does not
// reduce the SOURCE sentinel count in ci.yml at all, only the count this
// script manages to SEE, so a bare "did the source count change" diff
// would not have caught it; only re-deriving the checked count against a
// known floor does. A legitimate new sentinel added in a future task is
// still expected to bump `MIN_SENTINEL_COUNT` upward in the same commit
// (exactly as (a) would require) — the only difference from (a) is that a
// REMOVED sentinel (count still >= the old floor) does not spuriously fail,
// which (a)'s exact-equality pin would.
// Raised 33 -> 37 by task #1152 (F1): the `aligned-vmem-hugetlb-real` job
// gained 4 new sentinels (3 for the newly-added `decommit_capability`/
// `reservation_decommit_contract` targets it now actually runs, 1 for the
// new `ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback`
// path-activation oracle).
// Raised 37 -> 38 by task #1162 (F/arming): the same job gained one MARKER
// sentinel (`extractLiteralMarkers`, not a `test <name> ... ok` sentinel) —
// `[oracle] ARMED: real MAP_HUGETLB grant confirmed`, a `println!` that only
// executes past `ci_hugetlb_real_pool_oracle_refuses_ordinary_page_fallback`'s
// hard assert, making the armed outcome observable in the log itself instead
// of merely inferable from the presence of an env var in workflow text.
//
// COUPLING (task #1161): this literal and docs/CORRECTNESS_OPEN_ITEMS.md's
// item 87 "Current-number-or-verdict" card are THE SAME NUMBER living in two
// places, and they have already drifted apart once — `dad4d7a` (task #1152)
// raised this literal 33 -> 37 in the same commit that added the 4 sentinels
// (correctly, per the header comment above), but did not touch item 87's
// card, which kept reporting 33 for two more commits until task #1161 found
// and fixed it. A commit that changes this literal MUST also update item
// 87's Current-number-or-verdict card in the SAME commit — the WARNING below
// is the automated half of enforcing that; the manual half is: whoever
// raises MIN_SENTINEL_COUNT is the one person guaranteed to be looking at
// this exact file at this exact moment, so this comment is the cheapest
// place to say it. (Task #1162's own 37 -> 38 bump above and the matching
// item-87 card update landed in the SAME commit as this comment, as a live
// demonstration of the coupling task #1161 documents, not just a rule
// stated in the abstract.)
//
// Task #1164: 38 -> 40. Closed item 59a's kernel-response gap
// (`ci_hugetlb_real_pool_kernel_actually_accepts_eligible_madvise` in
// `crates/aligned-vmem/tests/decommit_capability.rs`), adding TWO sentinel
// lines to the `aligned-vmem-hugetlb-real` job in the SAME commit: one
// `test <name> ... ok` line (the standard shape) plus one new
// `[oracle] ARMED: kernel accepted ...` literal marker (the
// `extractLiteralMarkers` shape task #1162 introduced) — the same coupling
// this comment already documents, demonstrated a third time.
const MIN_SENTINEL_COUNT = 40;

const { checkedCount, errors } = verifyCiSentinels();
if (errors.length > 0) {
  console.log(`[verify-ci-sentinels] FAIL — ${errors.length} of ${checkedCount} sentinel(s) mismatched:\n`);
  for (const e of errors) console.log(`  - ${e}\n`);
  process.exit(1);
}
if (checkedCount < MIN_SENTINEL_COUNT) {
  console.log(
    `[verify-ci-sentinels] FAIL — only ${checkedCount} sentinel(s) checked, below the floor of ` +
      `${MIN_SENTINEL_COUNT}. Every individually-checked sentinel matched its \`fn\`'s real ` +
      `attributes, but the TOTAL COUNT dropped below the known-good floor — this is exactly the ` +
      `signature of a parser blind spot silently skipping a step shape (e.g. a \`run: >-\`/other ` +
      `block-scalar variant the step-splitter does not recognize) rather than of sentinels having ` +
      `been legitimately removed from ci.yml. If sentinels were genuinely and intentionally removed ` +
      `this run, lower MIN_SENTINEL_COUNT to the new checked count in the same commit; otherwise, ` +
      `re-examine splitIntoRunSteps() for a step shape it is silently failing to split.`,
  );
  process.exit(1);
}

// F-drift (task #1161): make a FORGOTTEN floor bump LOUD instead of silent.
//
// `checkedCount > MIN_SENTINEL_COUNT` (e.g. today's 37 checked >= a floor
// left at, say, 33 after a new sentinel was added but the floor wasn't
// bumped) is a SILENT pass under the check above — the floor only re-arms
// when a human remembers to raise it by hand. That is exactly the failure
// mode that produced this task: `dad4d7a` DID remember to bump the literal,
// but nothing forced a matching update to item 87's card in
// docs/CORRECTNESS_OPEN_ITEMS.md, so the card silently drifted for two
// commits with a fully green guard the entire time.
//
// This is a WARNING, not a FAIL, by deliberate choice — weighed against a
// hard failure and decided against it, for a reason that does not
// contradict the exact-pin-vs-floor reasoning above (this is a NEW axis, not
// a re-litigation of that choice): a hard failure here would turn EVERY
// legitimate sentinel addition into a two-step edit enforced by CI (bump the
// literal in the same commit, or the build goes red) — which is annoying but
// literally impossible to forget, since the red exit code cannot be missed.
// A warning can be ignored, which is a real weakness. It was chosen anyway
// because: (1) the header's own (b)-floor rationale already commits to "a
// legitimate new sentinel added in a future task is still expected to bump
// MIN_SENTINEL_COUNT upward in the same commit" — a hard failure here would
// just be that same existing expectation finally enforced, but this script
// has no way to also verify the SECOND, doc-side half of the coupling (that
// item 87's card was updated) — hard-failing on the count alone would only
// guarantee half the coupling stays honest, and could give a false sense
// that the whole coupling is enforced when it is not; (2) `checkedCount >
// MIN_SENTINEL_COUNT` is not on its own evidence of a forgotten bump — a
// human might legitimately choose to leave a small headroom in the floor
// (though today's convention, per the header comment, is to pin it exactly)
// — so a hard failure here risks being wrong about intent in a way the
// below-floor case (job of the FAIL branch above) is not: a COUNT DROP is
// unambiguously either a real removal (then lower the floor) or a parser
// regression (then fix the parser) — there is no third legitimate reading.
// A count RISE has a legitimate benign reading (deliberate headroom) that a
// drop does not, which is why this is a WARNING and the drop case above is
// a FAIL. If this warning is ever seen to go unactioned across multiple
// rounds in practice, escalate it to a FAIL — the option is deliberately
// left open, not foreclosed by this choice.
if (checkedCount > MIN_SENTINEL_COUNT) {
  console.log(
    `[verify-ci-sentinels] WARNING — ${checkedCount} sentinel(s) checked, above the pinned floor of ` +
      `${MIN_SENTINEL_COUNT} (delta +${checkedCount - MIN_SENTINEL_COUNT}). This is not a failure, but ` +
      `it usually means a sentinel was added without bumping MIN_SENTINEL_COUNT to match in the same ` +
      `commit. Raise MIN_SENTINEL_COUNT in this file to ${checkedCount} AND update item 87's ` +
      `"Current-number-or-verdict" card in docs/CORRECTNESS_OPEN_ITEMS.md in the SAME commit — those ` +
      `two numbers plus this floor are one fact recorded in two places, and they have already drifted ` +
      `apart once (task #1152's dad4d7a bumped this floor 33 -> 37 but left item 87's card at 33 for ` +
      `two more commits, found only by task #1161).`,
  );
}

console.log(`[verify-ci-sentinels] OK — ${checkedCount} sentinel(s) checked against their real \`fn\` attributes, all match.`);
process.exit(0);
