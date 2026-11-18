// Runs rows from `scripts/check-matrix.mjs`'s `PER_PR_ROWS` manifest, shared
// by both consumers: `scripts/check-all.mjs` (local `npm run check` gate) and
// `.github/workflows/ci.yml`'s `check-matrix` job, whose composite action
// invokes this runner once per shard with `--shard <i>/<n>`. With no shard
// flags, all rows still run. `--list` is a dry-run listing rows (used in
// tests only — never by production CI). `--keep-going` aggregates failures
// and implies `--no-fail-fast` for test rows; `--free-between-rows` deletes
// per-row artifacts between rows to free disk while preserving reusable rlibs.
//
// `--kind <clippy|check|test>` (repeatable) restricts the run to rows of the
// named kind(s); with no `--kind` flag, every row in `PER_PR_ROWS` runs. This
// exists so `.github/workflows/ci.yml`'s `check-matrix` job can run only the
// NON-clippy rows (its sibling `clippy` job already runs every clippy row as
// its own named per-step Actions-UI entry — see that job's header comment)
// without a second hand-maintained list of which rows to skip: the filter is
// by `kind`, read straight from the manifest, so a future non-clippy
// `PER_PR_ROWS` entry is picked up automatically with no ci.yml edit.

import { existsSync, readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { REPO_ROOT, run } from './lib.mjs';
import { PER_PR_ROWS, cargoArgv, parseShardSpec, rowLabel, selectShardRows } from './check-matrix.mjs';

const rawArgs = process.argv.slice(2);
const kindFilter = new Set();
let keepGoing = false;
let freeBetweenRows = false;
let listOnly = false;
let shardSpec = null;
for (let i = 0; i < rawArgs.length; i++) {
  if (rawArgs[i] === '--keep-going') {
    keepGoing = true;
  } else if (rawArgs[i] === '--free-between-rows') {
    freeBetweenRows = true;
  } else if (rawArgs[i] === '--list') {
    listOnly = true;
  } else if (rawArgs[i] === '--shard') {
    const val = rawArgs[i + 1];
    if (!val) {
      console.error('[check-matrix] --shard requires a value (<index>/<total>, 1-based)');
      process.exit(2);
    }
    shardSpec = val;
    i++;
  } else if (rawArgs[i] === '--kind') {
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
// `--shard` distributes the shardable (check/test) rows with the manifest's
// deterministic LPT; because a shard is a horizontal slice of ALL non-clippy
// rows, it must not be combined with a partial `--kind` view (and clippy rows
// are never sharded at all — R31-11). Unsharded `--kind` filters (including
// `--kind clippy`) keep their original pre-shard semantics untouched.
if (shardSpec && kindFilter.size > 0) {
  const ok = kindFilter.size === 2 && kindFilter.has('check') && kindFilter.has('test');
  if (!ok) {
    console.error('[check-matrix] --shard requires no --kind filters or exactly --kind check --kind test');
    process.exit(2);
  }
}
if (shardSpec) {
  try {
    parseShardSpec(shardSpec);
  } catch (err) {
    console.error(`[check-matrix] ${err.message}`);
    process.exit(2);
  }
}
const rows = shardSpec
  ? selectShardRows(shardSpec)
  : kindFilter.size
    ? PER_PR_ROWS.filter((r) => kindFilter.has(r.kind))
    : PER_PR_ROWS;

/**
 * Delete the extension-less files (the executables) under target/debug/{deps,examples} and the incremental
 * cache. Cargo 1.100's build-dir layout v2 keeps each unit in build/<package>/<hash>/out instead of deps/, so
 * this package's unit dirs are swept too (it has no build script, so none of them is an OUT_DIR).
 */
function freeBuildArtefacts() {
  const debug = join(REPO_ROOT, 'target', 'debug');
  rmSync(join(debug, 'incremental'), { recursive: true, force: true });
  let files = 0;
  const ownUnits = join(debug, 'build', 'sefer-alloc');
  const unitOuts = existsSync(ownUnits) ? readdirSync(ownUnits).map((hash) => join('build', 'sefer-alloc', hash, 'out')) : [];
  for (const sub of ['deps', 'examples', ...unitOuts]) {
    const dir = join(debug, sub);
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir)) {
      if (name.includes('.')) continue; // .rlib / .rmeta / .d / .so keep the dependency cache
      rmSync(join(dir, name), { force: true });
      files++;
    }
  }
  console.log(`[check-matrix] freed build artefacts: ${files} executable(s) and the incremental cache`);
}

if (listOnly) {
  // CI contract: stdout is a single JSON array of {id, argv, env} — argv
  // WITHOUT the leading 'cargo', env always an object ({} when the row has
  // none). No banners, on stdout OR stderr: a test worker parses stdout.
  console.log(
    JSON.stringify(
      rows.map((row) => ({
        id: row.id,
        argv: cargoArgv(row, { keepGoing }),
        env: row.env ?? {},
      })),
      null,
      2,
    ),
  );
  process.exit(0);
}
console.log(`[check-matrix] repo: ${REPO_ROOT}`);
if (shardSpec) {
  const { index, total } = parseShardSpec(shardSpec);
  console.log(`[check-matrix] shard ${index}/${total}`);
}
if (kindFilter.size) {
  console.log(`[check-matrix] kind filter: ${[...kindFilter].join(', ')}`);
}
console.log(
  `[check-matrix] running ${rows.length} row(s) — ${keepGoing ? 'keeps going, reports every failure' : 'fails fast'}\n`,
);

let allOk = true;
const failedRows = [];
for (const row of rows) {
  const args = cargoArgv(row, { keepGoing });
  console.log(`\n============================================================`);
  console.log(`  [${row.id}] ${rowLabel(row)}`);
  console.log(`  ${row.note}`);
  console.log(`============================================================`);
  // R1-08: `row.env` (e.g. `{ RUSTFLAGS: '-D warnings' }` for a `check`-kind
  // row that has no `-- -D warnings` passthrough of its own) is MERGED over
  // the inherited environment, never replacing it wholesale — mirrors
  // check-all.mjs's identical `step.env` handling and the same rationale
  // (spawn's `env` option replaces PATH entirely if not merged).
  const { code } = await run('cargo', args, {
    cwd: REPO_ROOT,
    ...(row.env ? { env: { ...process.env, ...row.env } } : {}),
  });
  if (code !== 0) {
    console.log(`\n[check-matrix] FAIL at row: ${row.id} (exit ${code})`);
    allOk = false;
    failedRows.push(row.id);
    if (!keepGoing) break;
  }
  console.log(`\n[check-matrix] OK: ${row.id}`);
  if (freeBetweenRows) freeBuildArtefacts();
}

console.log(`\n============================================================`);
console.log(
  allOk
    ? '[check-matrix] ALL GREEN'
    : `[check-matrix] FAILED: ${failedRows.join(', ')}`,
);
console.log(`============================================================`);
process.exit(allOk ? 0 : 1);
