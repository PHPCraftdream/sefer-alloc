#!/usr/bin/env node
// Regression test for the DEP0190 argv-preservation fix in scripts/lib.mjs's
// `run()`. Proves that a command-line argument containing BOTH whitespace AND
// shell-special characters survives `run()` as ONE argv element, byte-for-byte
// — the exact failure mode Node 22+'s `shell: true` change (DEP0190)
// introduced (silent whitespace-splitting of multi-word args) and that
// `run()`'s `shell: false` default exists to make structurally impossible.
//
// Run directly:
//   node scripts/argv-roundtrip-test.mjs
//
// Exits 0 on pass, 1 on failure. No npm deps (matches scripts/'s own
// "there is no npm dependency graph" rule — see lib.mjs's header).
//
// WHY THIS IS A REAL REGRESSION TEST (counterfactual): the child echoes its
// own `process.argv` back as JSON; the parent checks each tricky argument is
// present VERBATIM as a single array element. If `run()` ever regressed to
// `shell: true` (with or without hand-rolled quoting), a value like
// `'production alloc-stats bench-internals'` would be split into three
// separate argv elements by the shell and `argv.includes(arg)` would fail —
// the test would go red. So a green run here is positive evidence the
// shell:false path genuinely preserves argv, not just a hope that it does.

import { run } from './lib.mjs';

// `node -e` prints its own argv as JSON. We pass ONE "tricky" argument per
// launch and check it appears in the child's argv array verbatim. (The child's
// argv also contains execPath + '[eval]'; JSON.stringify round-trips those
// faithfully, and `includes` ignores them.)
const ECHO = 'process.stdout.write(JSON.stringify(process.argv))';

const TRICKY = [
  // The real-world case from check-all.mjs — a multi-word `--features` value
  // that DEP0190's shell:true used to split into three stray positionals.
  'production alloc-stats bench-internals',
  // Apostrophe + double quotes + spaces — stresses every shell's quoting rules.
  'it\'s a "test"',
  // A tab is whitespace too (`/\s/` matches it); shells also split on it.
  'a\tb\tc',
  // `&` and `|` — the two characters that, if a shell were ever in the path,
  // would be read as backgrounding / piping. Highest-signal proof run() bypasses
  // any shell: under shell:true these would never arrive as one argv token.
  'a & b | c',
  // `%` — Windows cmd.exe's variable-expansion char; highest-signal for this
  // project's primary dev platform (its own git status shows Windows paths).
  // Under a cmd.exe shell `a%PATH%b` would expand to the literal PATH value;
  // under shell:false it roundtrips verbatim.
  'a%PATH%b',
  // Parentheses — another shell-metacharacter class (subshelling).
  '(grouped)',
  // Unicode — a non-ASCII string, to prove UTF-8 argv bytes survive spawn()
  // unmangled (this project's own winToWsl / path code reasons about non-ASCII).
  'café — naïve 日本語',
  // Boring baseline — a single token must still roundtrip (guards against an
  // over-corrective fix that mangles simple args).
  'plain',
];

let failures = 0;
for (const arg of TRICKY) {
  const { code, out } = await run('node', ['-e', ECHO, arg], {});
  if (code !== 0) {
    console.error(`FAIL: 'node -e ... ${JSON.stringify(arg)}' exited ${code}`);
    failures++;
    continue;
  }
  let argv;
  try {
    argv = JSON.parse(out);
  } catch (e) {
    console.error(`FAIL: child did not emit valid JSON argv for ${JSON.stringify(arg)} (raw: ${JSON.stringify(out)})`);
    failures++;
    continue;
  }
  if (!Array.isArray(argv) || !argv.includes(arg)) {
    console.error(`FAIL: ${JSON.stringify(arg)} did not survive as a single argv element`);
    console.error(`  child argv: ${JSON.stringify(argv)}`);
    failures++;
  } else {
    console.log(`ok: ${JSON.stringify(arg)}`);
  }
}

if (failures) {
  console.error(`\n${failures} argument(s) failed to roundtrip — run() is splitting or mangling argv.`);
  process.exit(1);
}
console.log(`\nall ${TRICKY.length} tricky arguments roundtripped as single argv elements (shell:false argv preservation confirmed).`);
process.exit(0);
