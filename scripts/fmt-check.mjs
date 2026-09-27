// Cross-platform `cargo fmt --all -- --check` replacement.
//
// Root cause (task: fmt-os206): on Windows, `cargo fmt --all -- --check`
// collects EVERY workspace target's entry-point file (lib.rs / each test /
// each bench / each example — one file per Cargo target, not one per source
// file; rustfmt itself walks `mod` declarations from there) and invokes ONE
// rustfmt process with all of them as argv. This repo's workspace has 496
// such targets (`cargo metadata --no-deps` — one member is the root
// `sefer-alloc` package alone; 496 targets total including its 10
// `crates/*` siblings), whose absolute paths alone sum to ~42 KB — already
// over Windows' CreateProcess argv limit (~32,767 chars) BEFORE cargo adds
// its own flags/quoting. Reproduced empirically: `cargo fmt -p sefer-alloc
// -- --check` (the ROOT PACKAGE ALONE, not `--all`) already fails with
// "The filename or extension is too long. (os error 206)" — so per-package
// `cargo fmt -p <pkg>` looping is not sufficient; the root package's own
// target count is already over budget. Linux CI is unaffected: argv limits
// there are far larger (typically MBs via execve), which is why this went
// unnoticed.
//
// Fix: on win32 only, bypass `cargo fmt`'s own argv-collection entirely —
// gather every workspace target's src_path + edition from `cargo metadata`,
// chunk them under a safe argv budget, and invoke `rustfmt --check
// --edition <edition> <files...>` directly per chunk (relative paths, run
// from REPO_ROOT, to keep each argv small). This is coverage-EQUIVALENT to
// `cargo fmt --all -- --check`: every workspace member and every target
// kind cargo-fmt itself would visit (lib/bin/test/bench/example — this
// workspace has no `bin`/`custom-build` targets today, verified via `cargo
// metadata`) is included, and rustfmt.toml (none exists in this repo today)
// would be auto-discovered identically either way, since rustfmt walks up
// from each input file's own directory regardless of how it was invoked.
//
// On every other platform this script is a thin passthrough to plain
// `cargo fmt --all -- --check` — non-Windows behavior is unchanged.

import { execFileSync } from 'node:child_process';

import { run, REPO_ROOT } from './lib.mjs';

// Conservative: well under the ~32,767-char Windows CreateProcess limit,
// leaving headroom for the `rustfmt.exe` path itself, `--check --edition
// 2021` and per-arg quoting/escaping overhead cmd/CreateProcess may add.
const ARGV_BUDGET_CHARS = 20_000;

function chunk(paths) {
  const chunks = [];
  let current = [];
  let currentLen = 0;
  for (const p of paths) {
    const added = p.length + 1; // +1 for the joining space
    if (current.length > 0 && currentLen + added > ARGV_BUDGET_CHARS) {
      chunks.push(current);
      current = [];
      currentLen = 0;
    }
    current.push(p);
    currentLen += added;
  }
  if (current.length > 0) chunks.push(current);
  return chunks;
}

async function runWindows() {
  // Deliberately NOT run() here: run() tees the child's stdout straight to
  // this process's own stdout as it streams (see lib.mjs) — appropriate for
  // a step whose output IS the useful signal, but `cargo metadata`'s output
  // is one giant single-line JSON blob with no value to a human watching
  // the gate. execFileSync captures it quietly instead.
  let metaOut;
  try {
    metaOut = execFileSync(
      'cargo',
      ['metadata', '--no-deps', '--format-version', '1'],
      { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
    );
  } catch (e) {
    console.log(`[fmt-check] cargo metadata failed — cannot enumerate targets: ${e.message}`);
    return 1;
  }
  let metadata;
  try {
    metadata = JSON.parse(metaOut);
  } catch (e) {
    console.log(`[fmt-check] failed to parse cargo metadata output: ${e.message}`);
    return 1;
  }

  // path.relative-equivalent without importing 'node:path' twice for one
  // call: REPO_ROOT is a plain absolute Windows path (no trailing slash),
  // so a straight prefix strip is exact for every target — all workspace
  // targets live under REPO_ROOT by construction (there is no path-based
  // dependency outside the repo).
  const toRelative = (absPath) => {
    if (absPath.toLowerCase().startsWith(REPO_ROOT.toLowerCase() + '\\')) {
      return absPath.slice(REPO_ROOT.length + 1);
    }
    return absPath; // fallback: use the absolute path as-is (still correct, just longer)
  };

  // Group by edition — rustfmt takes ONE `--edition` per invocation. Every
  // target in this workspace is edition 2021 today (verified via `cargo
  // metadata`), but grouping (rather than assuming) keeps this correct if a
  // future crate pins a different edition.
  const byEdition = new Map();
  const seen = new Set();
  for (const pkg of metadata.packages) {
    for (const target of pkg.targets) {
      const rel = toRelative(target.src_path);
      if (seen.has(rel)) continue; // a shared src_path across target kinds would double-check the same file
      seen.add(rel);
      const edition = target.edition ?? '2021';
      if (!byEdition.has(edition)) byEdition.set(edition, []);
      byEdition.get(edition).push(rel);
    }
  }

  const totalFiles = [...byEdition.values()].reduce((n, arr) => n + arr.length, 0);
  console.log(
    `[fmt-check] windows argv-length workaround: checking ${totalFiles} target file(s) ` +
      `across ${byEdition.size} edition group(s), directly via rustfmt in argv-budgeted chunks ` +
      `(equivalent coverage to 'cargo fmt --all -- --check')`,
  );

  let allOk = true;
  for (const [edition, files] of byEdition) {
    files.sort();
    const chunks = chunk(files);
    console.log(`[fmt-check] edition ${edition}: ${files.length} file(s) in ${chunks.length} chunk(s)`);
    for (const [i, fileChunk] of chunks.entries()) {
      console.log(`[fmt-check] chunk ${i + 1}/${chunks.length} (${fileChunk.length} files)`);
      const { code } = await run(
        'rustfmt',
        ['--check', '--edition', edition, ...fileChunk],
        { cwd: REPO_ROOT },
      );
      if (code !== 0) allOk = false;
    }
  }
  return allOk ? 0 : 1;
}

async function main() {
  if (process.platform !== 'win32') {
    // Non-Windows: unchanged — the plain command CI already runs.
    const { code } = await run('cargo', ['fmt', '--all', '--', '--check'], { cwd: REPO_ROOT });
    return code;
  }
  return runWindows();
}

main().then((code) => process.exit(code));
