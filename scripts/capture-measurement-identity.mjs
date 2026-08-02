// Pre-measurement immutable-source-identity capture — the missing piece
// CLAUDE.md's R29-6 rule (see "Phased delivery" section, "A perf-gate
// report measuring an uncommitted tree must record an IMMUTABLE source
// identity...") names but never automated: every gate report so far that
// measured an uncommitted tree computed its identity AFTER the fact, from a
// stated recipe applied to a working tree that had since moved on (R29-13's
// own citation is the concrete failure mode: a 63-hex "sha256" that could
// not be reproduced from its own stated recipe — see CLAUDE.md's rule (e)
// item 7 and R30-4/task #453 finding e). This script exists so a measurement
// run can capture form (2) of the R29-6 rule's four sanctioned forms — a git
// TREE OBJECT SHA via `git write-tree` — in the seconds immediately BEFORE
// the measurement binaries are built/run, not reconstructed afterward from
// memory of what the tree "should" have looked like.
//
// Why `git write-tree` (form 2) rather than a temp commit (form 1) or a
// patch hash (form 3) as the DEFAULT here: `write-tree` needs no commit
// object, no branch, no cleanup, and — critically for the shared-workspace
// git-safety rule this project's agents operate under — touches NO ref, NO
// HEAD, NO working tree state. It only requires the index to already reflect
// the files a caller cares about (this script does not run `git add`; that
// is the caller's job, exactly like a real `git write-tree` invocation
// always has been — see the --stage-and-restore note below for why a
// convenience flag doing that automatically was rejected). A caller wanting
// form 1/3/4 instead can compose them from this script's own primitives
// (see --patch-hash below) or shell out directly; this script does not try
// to be every form in one flag surface.
//
// TREE SHA IS THE AUTHORITATIVE FORM (task #493, R32-1). Every report this
// session has actually cited and treated as authoritative is `treeSha`, not
// `patchSha256` — the patch hash is SECONDARY / best-effort, produced only
// on request via --patch-hash. Round 32's independent review (P2-3, filed
// OPEN_ITEMS.md item 36) found the two forms could describe DIFFERENT
// content: `treeSha` comes from `git write-tree` (the INDEX at capture
// time), while the old `patchSha256` came from `git diff HEAD` (the WORKING
// TREE at capture time, unstaged included) — if the working tree has an
// unstaged edit not yet `git add`ed, the two snapshots diverge silently.
// Confirmed concretely while fixing this: `git diff HEAD` on this repo's own
// working tree at fix time included an unstaged edit to
// `benches/r32_0_virgin_zero_skip_cost_side_gate.rs` that `git diff <headSha>
// <treeSha>` (diffing the two committed/tree objects directly) does NOT —
// proof the old code's two fields were not guaranteed to agree.
//
// Fix: `patchSha256` (when requested) is now computed via
// `git diff <headSha> <treeSha>` — a diff between two tree-ish OBJECTS, not
// a diff against the live working tree — so it is BY CONSTRUCTION the exact
// patch that reproduces `treeSha`'s content from `headSha`, never a
// second, independently-drifting snapshot. The script also now SAVES this
// patch to disk (`docs/perf/_raw_identity_<treeSha-prefix>.patch`) so the
// printed `patchReproduceCommand` names a file that actually exists, instead
// of instructing `git apply <saved-patch>` against nothing (the old P2-3
// finding's second half). Saving under `docs/perf/_raw_*` reuses this
// project's already-established "raw artifact, scratch by default, `git add
// -f` only when a report cites it as evidence" convention (see CLAUDE.md's
// raw-log-policy bullet) rather than inventing a new artifact location.
//
// Usage:
//   node scripts/capture-measurement-identity.mjs
//     Stages nothing; reports the write-tree SHA for the CURRENT INDEX as-is
//     (i.e. whatever `git add` state already exists — same semantics as a
//     bare `git write-tree`). This is the right default for a caller who has
//     already staged exactly the files it's about to measure.
//
//   node scripts/capture-measurement-identity.mjs --patch-hash
//     ALSO computes form (3), now DERIVED FROM treeSha (see above): sha256 of
//     `git diff <headSha> <treeSha>` — the exact patch that reproduces the
//     SAME snapshot `treeSha` already names, saved to
//     `docs/perf/_raw_identity_<treeSha-prefix>.patch`.
//
//   node scripts/capture-measurement-identity.mjs --json
//     Machine-readable output only (no human-readable banner) — for a
//     measurement script to capture into its own provenance JSON
//     programmatically. Combine with --patch-hash to get both fields. THIS
//     is the intended flow for any NEW report/derive-script going forward
//     (closes P2-10, OPEN_ITEMS.md item 36): a `scripts/r*_derive_report_data.mjs`
//     script should consume this command's `--json` output (e.g. via
//     `execFileSync('node', ['scripts/capture-measurement-identity.mjs',
//     '--json'])` + `JSON.parse`, or by piping a previously captured JSON
//     file in) for its `baseCommit`/`sourceTreeSha` fields, rather than
//     hand-typing a provenance header — the pattern
//     `scripts/r31_10_derive_cost_report_data.mjs` and
//     `scripts/r32_0_derive_report_data.mjs` both used (hardcoded
//     `baseCommit`/`sourceTreeSha` string literals) is exactly the defect
//     this note exists to head off in NEW scripts; those two existing files
//     are not retrofitted by this task (out of scope — see task #493's own
//     scope-discipline instruction), but no NEW derive script has an excuse
//     to repeat the pattern now that this usage note says so explicitly.
//
// Output (human mode) is deliberately copy-pasteable directly into a gate
// report's "Base revision measured" line: it prints the exact
// `git show <tree-sha>:<path>` recovery command form the R29-6 rule expects
// a citation to support — VERIFIED working (P2-2 fix, OPEN_ITEMS.md item 36):
// the OLD form (`git show <tree>: -- <path>`) silently ignores the `--
// <path>` pathspec and prints the ROOT TREE LISTING instead, exiting 0 (a
// worse failure mode than an error — looks like it worked). Confirmed by
// running both forms against a real tree SHA from this repo before and
// after this fix. The corrected form omits `--` entirely:
// `git show <tree-sha>:<path>`.
//
// Deliberately NOT included: a `--stage-and-restore` convenience flag that
// runs `git add -A` before `write-tree` and `git reset` after. This project's
// git-safety rule (see the "Git safety — shared workspace" contract every
// agent operates under) forbids exactly the mutating commands
// (`git add -A` touches the index; a subsequent `git reset` to "restore" it
// is itself a mutating op with cleanup-ordering risk if the script is
// killed mid-way) that convenience would need — a caller in a shared
// workspace must stage deliberately, with its own tool calls, so a
// concurrent agent's unstaged edits are never swept into a tree SHA that
// isn't actually what THIS measurement run measured.

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const PERF_DIR = resolve(REPO_ROOT, 'docs/perf');

function git(args) {
  const r = spawnSync('git', args, { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (r.status !== 0) {
    throw new Error(`git ${args.join(' ')} failed (exit ${r.status}): ${r.stderr.trim()}`);
  }
  return r.stdout;
}

function capture({ patchHash }) {
  const headSha = git(['rev-parse', 'HEAD']).trim();
  const treeSha = git(['write-tree']).trim();
  const dirty = git(['status', '--porcelain']).trim().length > 0;
  const capturedAt = new Date().toISOString();

  const result = {
    capturedAt,
    headSha,
    treeSha,
    dirty,
    // Corrected form (P2-2, OPEN_ITEMS.md item 36): NO `-- ` before the
    // path — `git show <tree>: -- <path>` silently ignores the pathspec and
    // prints the root tree listing instead, exiting 0. Verified working:
    // `git show <treeSha>:<path>` (no `--`) prints the file's blob content.
    recoverCommand: `git show ${treeSha}:<path>  # or: git archive ${treeSha} | tar -x`,
  };

  if (patchHash) {
    // Form (3), now DERIVED FROM treeSha rather than an independent
    // snapshot (P2-3 fix, OPEN_ITEMS.md item 36): diff the two tree-ish
    // OBJECTS (headSha commit vs. treeSha tree) directly, rather than
    // `git diff HEAD` (which reads the LIVE working tree — a third,
    // potentially-divergent snapshot if anything is unstaged since
    // write-tree ran). This guarantees patchSha256 describes exactly the
    // same content treeSha names, never a separately-drifting one.
    const diffText = git(['diff', headSha, treeSha]);
    result.patchSha256 = createHash('sha256').update(diffText, 'utf8').digest('hex');

    // Actually save the patch this hash was computed from (P2-3 second half,
    // OPEN_ITEMS.md item 36): the old script printed a
    // `git apply <saved-patch>` reproduce command without ever writing
    // `<saved-patch>` anywhere. Scratch by default under docs/perf/_raw_* —
    // same "raw artifact, git add -f only when a report cites it" convention
    // CLAUDE.md's raw-log-policy bullet already establishes for other
    // _raw_*.log/_raw_*.patch measurement artifacts.
    const patchFileName = `_raw_identity_${treeSha.slice(0, 12)}.patch`;
    const patchPath = resolve(PERF_DIR, patchFileName);
    writeFileSync(patchPath, diffText);
    result.patchFile = `docs/perf/${patchFileName}`;
    result.patchReproduceCommand =
      `git checkout ${headSha} -- . && git apply ${result.patchFile} && ` +
      `git write-tree  # must equal ${treeSha}`;
  }

  return result;
}

const args = process.argv.slice(2);
const jsonOnly = args.includes('--json');
const patchHash = args.includes('--patch-hash');

let result;
try {
  result = capture({ patchHash });
} catch (err) {
  console.error(`[capture-measurement-identity] ${err.message}`);
  process.exit(1);
}

if (jsonOnly) {
  console.log(JSON.stringify(result, null, 2));
} else {
  console.log('[capture-measurement-identity] immutable source identity captured');
  console.log(`  captured at (UTC):  ${result.capturedAt}`);
  console.log(`  HEAD commit SHA:    ${result.headSha}`);
  console.log(`  tree object SHA:    ${result.treeSha}  (AUTHORITATIVE — cite this one)`);
  console.log(`  working tree dirty: ${result.dirty}`);
  if (patchHash) {
    console.log(`  patch sha256:       ${result.patchSha256}  (secondary/best-effort, derived FROM tree SHA)`);
    console.log(`  patch file:         ${result.patchFile}`);
  }
  console.log('');
  console.log('  Copy this into the report\'s "Base revision measured" line, e.g.:');
  console.log(
    `  **Base revision measured:** \`main\` @ \`${result.headSha}\` + working tree, tree SHA ` +
      `\`${result.treeSha}\` (captured ${result.capturedAt}, BEFORE this run's measurement binaries ` +
      `were built) — recover via \`${result.recoverCommand}\`.`,
  );
  if (patchHash) {
    console.log(
      `  Secondary patch identity: sha256 \`${result.patchSha256}\` (saved to \`${result.patchFile}\`) ` +
        `— reproduce via ${result.patchReproduceCommand}`,
    );
  }
}
