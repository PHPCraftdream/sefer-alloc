import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

function parse(text) {
  const entries = [];
  let current;
  let total = 0;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/\x1b\[[0-9;]*m/g, '');
    const running = /running `(.+)` on (\S+) \((\d+)\/(\d+)\)/.exec(line);
    if (running) {
      current = {
        command: running[1], package: running[2], index: Number(running[3]),
        total: Number(running[4]), error: '', warnings: 0,
      };
      entries.push(current);
      total = Math.max(total, current.total);
      continue;
    }
    if (/\bFinished\s|^error:.*(?:failed commands|commands failed|failed to run|failed to execute)/.test(line)) {
      current = undefined;
    }
    if (!current) continue;
    const diagnostic = /(?:^|\s)((?:error(?:\[[^\]]+\])?|warning(?:\[[^\]]+\])?):\s.*)/.exec(line);
    if (!diagnostic) continue;
    if (diagnostic[1].startsWith('warning')) {
      if (!/generated \d+ warnings?/.test(diagnostic[1])) current.warnings++;
    }
    else if (!current.error) current.error = diagnostic[1];
  }
  return { total, attempted: entries.length, failed: entries.filter((e) => e.error).length, entries };
}

function cell(value) {
  return String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;').replaceAll('|', '&#124;').replaceAll('`', '&#96;');
}

function markdown(result, label) {
  const lines = [
    `## Feature sweep: ${cell(label)}`,
    '',
    `Total: ${result.total}; attempted: ${result.attempted}; failed: ${result.failed}; warnings: ${result.entries.reduce((n, e) => n + e.warnings, 0)}.`,
    '',
    '| Package | Position | Failed command | First error | Warnings |',
    '| --- | --- | --- | --- | --- |',
  ];
  for (const e of result.entries.filter((e) => e.error)) {
    lines.push(`| ${cell(e.package)} | ${e.index}/${e.total} | ${cell(e.command)} | ${cell(e.error)} | ${e.warnings} |`);
  }
  lines.push('', '### Warnings by combination', '', '| Package | Position | Command | Warnings |', '| --- | --- | --- | --- |');
  for (const e of result.entries.filter((e) => e.warnings)) {
    lines.push(`| ${cell(e.package)} | ${e.index}/${e.total} | ${cell(e.command)} | ${e.warnings} |`);
  }
  return lines.join('\n') + '\n';
}

function selfTest() {
  const observed = [
    '##[group]running `cargo check --no-default-features --features lazy-commit-fault-injection,medium-classes-wide` on sefer-alloc (204/390)',
    '    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.59s',
    '##[group]running `cargo check --no-default-features --features numa-aware` on sefer-alloc (205/390)',
    'error[E0433]: cannot find type `SegmentMeta` in this scope',
    'error: could not compile `sefer-alloc` (lib) due to 1 previous error',
  ].join('\n');
  const sparse = parse(observed);
  assert.deepEqual([sparse.total, sparse.attempted, sparse.failed], [390, 2, 1]);
  assert.equal(sparse.entries[1].command, 'cargo check --no-default-features --features numa-aware');
  assert.equal(sparse.entries[1].index, 205);
  assert.equal(sparse.entries[1].error, 'error[E0433]: cannot find type `SegmentMeta` in this scope');
  const multiple = parse(observed + '\n' + [
    'info: running `cargo check --locked --features another` on sefer-alloc (206/390)',
    'warning: first warning',
    'warning[unused_imports]: second warning',
    'error: synthetic second failure',
    'info: running `cargo check --locked --features production` on sefer-alloc (390/390)',
    'warning: successful combination warning',
    '    Finished `dev` profile [unoptimized + debuginfo] target(s)',
    'error: failed to run 2 commands',
    'error: final recap must not belong to production',
  ].join('\n'));
  assert.deepEqual([multiple.total, multiple.attempted, multiple.failed], [390, 4, 2]);
  assert.deepEqual(multiple.entries.map((e) => e.warnings), [0, 0, 2, 1]);
  assert.equal(multiple.entries[3].error, '');
  const output = markdown(multiple, 'fixture');
  assert.match(output, /Total: 390; attempted: 4; failed: 2; warnings: 3/);
  assert.match(output, /206\/390.*synthetic second failure.*2/);
  assert.equal(parse('').attempted, 0);
  assert.equal(parse('error: setup failed').failed, 0);
  assert.equal(cell('a|<b>`'), 'a&#124;&lt;b&gt;&#96;');
  assert.equal(parse(observed.replaceAll('\n', '\r\n').replace('error[E0433]', '\x1b[31merror[E0433]\x1b[0m')).failed, 1);
  const warningFixture = [
    'info: running `cargo check` on x (1/1)',
    'warning: `x` (lib) generated 1 warning',
  ].join('\n');
  assert.equal(parse(warningFixture).entries[0].warnings, 0);
  assert.equal(parse(warningFixture + '\n' + [
    'warning: `x` (manifest) deprecated setting',
    'warning: unused manifest key: package.example',
  ].join('\n')).entries[0].warnings, 2);
  console.log('powerset-summary self-test: 15 assertions passed');
}

const args = process.argv.slice(2);
if (args.length === 1 && args[0] === '--self-test') {
  selfTest();
} else if (args.length === 1 && !args[0].startsWith('--')) {
  console.log(markdown(parse(readFileSync(args[0], 'utf8')), args[0]));
} else {
  console.error('usage: node scripts/powerset-summary.mjs <log> | --self-test');
  process.exitCode = 2;
}
