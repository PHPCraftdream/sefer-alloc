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
// Usage:
//   node scripts/capture-measurement-identity.mjs
//     Stages nothing; reports the write-tree SHA for the CURRENT INDEX as-is
//     (i.e. whatever `git add` state already exists — same semantics as a
//     bare `git write-tree`). This is the right default for a caller who has
//     already staged exactly the files it's about to measure.
//
//   node scripts/capture-measurement-identity.mjs --patch-hash
//     ALSO computes form (3): sha256 of `git diff` against HEAD (working
//     tree vs HEAD, unstaged+staged combined via `git diff HEAD`) — useful
//     as a second, independent identity when the caller wants both a tree
//     SHA and a reproducible-by-reapplying-the-patch hash in the same
//     capture.
//
//   node scripts/capture-measurement-identity.mjs --json
//     Machine-readable output only (no human-readable banner) — for a
//     measurement script to capture into its own provenance JSON
//     programmatically. Combine with --patch-hash to get both fields.
//
// Output (human mode) is deliberately copy-pasteable directly into a gate
// report's "Base revision measured" line: it prints the exact
// `git show <tree-sha>:<path>` recovery command form the R29-6 rule expects
// a citation to support.
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
import { REPO_ROOT } from './lib.mjs';

function git(args) {
  const r = spawnSync('git', args, { cwd: REPO_ROOT, encoding: 'utf8' });
  if (r.status !== 0) {
    throw new Error(`git ${args.join(' ')} failed (exit ${r.status}): ${r.stderr.trim()}`);
  }
  return r.stdout.trim();
}

function capture({ patchHash }) {
  const headSha = git(['rev-parse', 'HEAD']);
  const treeSha = git(['write-tree']);
  const dirty = git(['status', '--porcelain']).length > 0;
  const capturedAt = new Date().toISOString();

  const result = {
    capturedAt,
    headSha,
    treeSha,
    dirty,
    recoverCommand: `git show ${treeSha}: -- <path>  # or: git archive ${treeSha} | tar -x`,
  };

  if (patchHash) {
    // Form (3): sha256 of `git diff HEAD` — staged + unstaged combined,
    // matching the R29-6 rule's own worked example
    // ("sha256(git diff -- ...)"). Reproducible later by re-applying this
    // exact diff to headSha and re-hashing.
    const diffText = git(['diff', 'HEAD']);
    result.patchSha256 = createHash('sha256').update(diffText, 'utf8').digest('hex');
    result.patchReproduceCommand =
      `git checkout ${headSha} -- . && git apply <saved-patch> && ` +
      `git diff HEAD | sha256sum  # must equal ${result.patchSha256}`;
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
  console.log(`  tree object SHA:    ${result.treeSha}`);
  console.log(`  working tree dirty: ${result.dirty}`);
  if (patchHash) {
    console.log(`  patch sha256:       ${result.patchSha256}`);
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
      `  Patch identity: sha256 \`${result.patchSha256}\` — reproduce via ${result.patchReproduceCommand}`,
    );
  }
}
