// Exercise the actual Ubuntu setup scripts without network or package installation.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

assert.equal(process.platform, 'linux', 'Ubuntu setup fixtures run only in Linux CI');
for (const [action, packages] of [
  ['ci-ghost-zero-cost', ['valgrind', 'binutils']],
  ['ci-test-workspace', ['gcc-multilib']],
]) {
  const source = readFileSync(new URL(`../.github/actions/${action}/action.yml`, import.meta.url), 'utf8');
  const block = source.match(/    - run: \|\n(        \. \/etc\/os-release\n(?:        .*\n)+)/)?.[1];
  assert.ok(block, `${action}: actual APT block required`);
  const script = block.split('\n').map(line => line.slice(8)).join('\n');
  const dir = mkdtempSync(join(tmpdir(), 'ci-ubuntu-apt-'));
  try {
    const trace = join(dir, 'trace');
    const result = spawnSync('bash', ['-e'], {
      input: 'sudo() { printf "%s\\0" "$@" >> "$APT_TRACE"; printf "\\n" >> "$APT_TRACE"; }\n' + script,
      env: { ...process.env, RUNNER_TEMP: dir, APT_TRACE: trace }, encoding: 'utf8',
    });
    assert.equal(result.status, 0, `${action}: ${result.error ?? result.stderr}`);
    const os = readFileSync('/etc/os-release', 'utf8');
    const codename = os.match(/^VERSION_CODENAME=["']?([^"'\n]+)/m)?.[1];
    assert.ok(codename);
    const list = join(dir, 'ubuntu-ci.list');
    const signed = '[signed-by=/usr/share/keyrings/ubuntu-archive-keyring.gpg]';
    assert.equal(readFileSync(list, 'utf8'), [
      ...['', '-updates', '-backports'].map(suffix =>
        `deb ${signed} https://archive.ubuntu.com/ubuntu ${codename}${suffix} main restricted universe multiverse`),
      `deb ${signed} https://security.ubuntu.com/ubuntu ${codename}-security main restricted universe multiverse`,
      '',
    ].join('\n'));
    const commands = readFileSync(trace, 'utf8').trimEnd().split('\n').map(line => line.split('\0').slice(0, -1));
    const options = ['apt-get', '-o', `Dir::Etc::sourcelist=${list}`, '-o', 'Dir::Etc::sourceparts=-'];
    const quiet = action === 'ci-ghost-zero-cost' ? ['-q'] : [];
    assert.deepEqual(commands, [
      [...options, 'update', ...quiet],
      [...options, 'install', '-y', ...quiet, ...packages],
    ], `${action}: both commands must use isolated sources and unchanged packages`);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
console.log('[ci-ubuntu-apt-sources] actual scripts preserve official signed sources, command isolation and packages');
