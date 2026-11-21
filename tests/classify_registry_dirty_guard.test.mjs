import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

test('CLI --identity: tracked registry-only changes require allow-dirty', () => {
  // Substitute only Git status: the real CLI consumes porcelain without writing an index.
  const statusMarker = 'DIRTY_GUARD_GIT_STATUS_ARGV:';
  const statusArgs = ['status', '--porcelain', '-z', '--untracked-files=normal'];
  const run = (records, allow = '') => {
    const preload = `
      import childProcess from 'node:child_process';
      import { syncBuiltinESMExports } from 'node:module';
      const spawn = childProcess.spawnSync;
      childProcess.spawnSync = (cmd, args, options) => {
        if (cmd !== 'git' || args[0] !== 'status') return spawn(cmd, args, options);
        process.stderr.write(${JSON.stringify(statusMarker)} + JSON.stringify(args) + '\\n');
        if (!args.includes('-z')) return { status: 1, stdout: '', stderr: 'mock git status requires -z' };
        return { status: 0, stdout: ${JSON.stringify(records.join('\0') + '\0')} };
      };
      syncBuiltinESMExports();`;
    const result = spawnSync(process.execPath, ['--import', `data:text/javascript,${encodeURIComponent(preload)}`,
      'scripts/classify-tests.mjs', '--identity'], {
      cwd: repoRoot, encoding: 'utf8',
      env: { ...process.env, CLASSIFY_TESTS_ALLOW_DIRTY: allow, GIT_OPTIONAL_LOCKS: '0' },
    });
    assert.ifError(result.error);
    const calls = result.stderr.split(/\r?\n/).filter(line => line.startsWith(statusMarker));
    assert.ok(calls.length > 0, `expected an intercepted git status call\n${result.stderr}`);
    for (const call of calls) {
      assert.deepEqual(JSON.parse(call.slice(statusMarker.length)), statusArgs, 'git status argv');
    }
    return result;
  };
  const registry = 'crates/core/sefer-registry/src/file -> test.rs';
  const other = 'crates/core/sefer-segment/src/file.rs';
  for (const [records, diagnostic] of [
    [[` M ${registry}`], ` M ${registry}`],
    [[` D ${registry}`], ` D ${registry}`],
    [[`R  ${registry}`, other], `R  ${registry}`],
    [[`R  ${other}`, registry], `R  ${registry} -> ${other}`],
  ]) {
    const r = run(records);
    assert.equal(r.status, 1, r.stderr);
    assert.equal(r.stdout, '');
    assert.ok(r.stderr.replaceAll('\r\n', '\n').includes(
      `Error: tracked classification inputs are dirty:\n${diagnostic}\n`), r.stderr);
  }
  for (const r of [run([` M ${registry}`], '1'), run([
    `?? ${registry}`, ` M ${other}`, ' M crates/core/sefer-alloc-core/src/lib.rs',
  ])]) {
    assert.equal(r.status, 0, r.stderr);
    assert.match(r.stdout, /^tracked inputs clean: yes$/m);
  }
});
