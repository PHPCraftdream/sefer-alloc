// Runs every row in `scripts/check-matrix.mjs`'s `PER_PR_ROWS`, in order,
// fail-fast. This is the ONE runner shared by both consumers of that
// manifest:
//   - scripts/check-all.mjs (local `npm run check` pre-push gate) invokes
//     this as one of its steps.
//   - `.github/workflows/ci.yml`'s `check-matrix` job invokes this script
//     directly as its single step.
//
// Why a script-invoked-in-a-loop, not YAML-generated matrix rows (R30-5,
// task #454): `PER_PR_ROWS` mixes three different `cargo` subcommands
// (clippy / check / test) and a `--bench`-scoped row alongside whole-crate
// rows — encoding that shape as a GitHub Actions `strategy: matrix:` would
// need either a same-shaped JSON `include:` list (itself hand-maintained
// again, defeating the point) or a `fromJSON()` step that reads this file's
// exports at workflow-parse time (Actions cannot `import` an ES module
// directly; it would need a separate "dump the manifest to JSON" step run
// BEFORE the matrix is evaluated, adding a job-dependency layer for four
// small rows). A single job whose one step is `node
// scripts/run-check-matrix.mjs` is simpler, keeps the fail-fast `npm run
// check` semantics identical between local and CI, and the manifest module is
// still the single source of truth either way — this is the "OR add a CI
// step that runs a script which reads the manifest and executes each check
// in a loop" option the task brief names explicitly.
//
// Usage:
//   node scripts/run-check-matrix.mjs
//   node scripts/run-check-matrix.mjs --kind check --kind test   # filter to
//                                                                 # one or
//                                                                 # more kinds
//   npm run check:matrix
//
// `--kind <clippy|check|test>` (repeatable) restricts the run to rows of the
// named kind(s); with no `--kind` flag, every row in `PER_PR_ROWS` runs. This
// exists so `.github/workflows/ci.yml`'s `check-matrix` job can run only the
// NON-clippy rows (its sibling `clippy` job already runs every clippy row as
// its own named per-step Actions-UI entry — see that job's header comment)
// without a second hand-maintained list of which rows to skip: the filter is
// by `kind`, read straight from the manifest, so a future non-clippy
// `PER_PR_ROWS` entry is picked up automatically with no ci.yml edit.

import { REPO_ROOT, run } from './lib.mjs';
import { PER_PR_ROWS, rowToCargoArgs, rowLabel } from './check-matrix.mjs';

const rawArgs = process.argv.slice(2);
const kindFilter = new Set();
for (let i = 0; i < rawArgs.length; i++) {
  if (rawArgs[i] === '--kind') {
    const val = rawArgs[i + 1];
    if (!val) {
      console.error('[check-matrix] --kind requires a value (clippy|check|test)');
      process.exit(2);
    }
    kindFilter.add(val);
    i++;
  } else {
    console.error(`[check-matrix] unrecognized argument: ${rawArgs[i]}`);
    process.exit(2);
  }
}
const rows = kindFilter.size
  ? PER_PR_ROWS.filter((r) => kindFilter.has(r.kind))
  : PER_PR_ROWS;

console.log(`[check-matrix] repo: ${REPO_ROOT}`);
if (kindFilter.size) {
  console.log(`[check-matrix] kind filter: ${[...kindFilter].join(', ')}`);
}
console.log(`[check-matrix] running ${rows.length} row(s) — fails fast\n`);

let allOk = true;
for (const row of rows) {
  const args = rowToCargoArgs(row);
  console.log(`\n============================================================`);
  console.log(`  [${row.id}] ${rowLabel(row)}`);
  console.log(`  ${row.note}`);
  console.log(`============================================================`);
  const { code } = await run('cargo', args, { cwd: REPO_ROOT });
  if (code !== 0) {
    console.log(`\n[check-matrix] FAIL at row: ${row.id} (exit ${code})`);
    allOk = false;
    break;
  }
  console.log(`\n[check-matrix] OK: ${row.id}`);
}

console.log(`\n============================================================`);
console.log(
  allOk ? '[check-matrix] ALL GREEN' : '[check-matrix] FAILED',
);
console.log(`============================================================`);
process.exit(allOk ? 0 : 1);
