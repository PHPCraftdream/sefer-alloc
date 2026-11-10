// CI-only clean working-source capture; no Cargo or native executable invocation.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { constants, closeSync, fstatSync, lstatSync, openSync, readSync, realpathSync } from 'node:fs';
import { resolve, relative, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { sourceInventory } from './source-inventory.mjs';

const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const same = (a, b) => a.dev === b.dev && a.ino === b.ino && a.size === b.size
  && a.mtimeMs === b.mtimeMs && a.ctimeMs === b.ctimeMs && a.mode === b.mode && a.nlink === b.nlink;

export function trackedSource(outputDirectory, expectedCommit) {
  assert.equal(process.platform, 'linux', 'source collector requires controlled native Linux CI');
  assert(!['GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE'].some(key => process.env[key]), 'Git identity override');
  assert.match(expectedCommit, /^[a-f0-9]{40}$/, 'expected commit identity');
  const base = realpathSync(root);
  const output = realpathSync(outputDirectory);
  const localOutput = relative(base, output);
  assert(localOutput.startsWith(`.rush${sep}j13b-builder-`) && !localOutput.split(sep).includes('..'),
    'builder output must be an isolated in-tree .rush/j13b-builder-* directory');
  const git = args => execFileSync('git', ['-C', base, ...args], {
    timeout: 30000, maxBuffer: 32 * 1024 * 1024, windowsHide: true,
    env: { ...process.env, LC_ALL: 'C', GIT_OPTIONAL_LOCKS: '0' },
  });
  const decode = bytes => new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  function verifyClean() {
    assert.equal(decode(git(['rev-parse', 'HEAD'])).trim(), expectedCommit, 'checkout commit changed');
    git(['diff', '--quiet', '--no-ext-diff', '--no-textconv', '--']);
    git(['diff', '--cached', '--quiet', '--no-ext-diff', '--no-textconv', '--']);
    for (const args of [['ls-files', '--others', '--exclude-standard', '-z'],
      ['ls-files', '--others', '--ignored', '--exclude-standard', '-z']]) {
      for (const path of decode(git(args)).split('\0').filter(Boolean)) {
        assert(path.startsWith('target/') || path.startsWith(`${localOutput}/`), `untracked source/input: ${path}`);
      }
    }
  }
  verifyClean();
  const inventory = decode(git(['ls-files', '--stage', '-z']));
  const files = inventory.split('\0').filter(Boolean).map(line => {
    const match = /^([0-7]{6}) ([a-f0-9]{40}) ([0-3])\t([\s\S]+)$/.exec(line);
    assert(match && match[3] === '0', 'malformed or conflicting Git index entry');
    return { path: match[4], mode: match[1], bytes: Buffer.alloc(0) };
  });
  sourceInventory(files); // Validate all names/modes before resolving or opening a source path.
  let total = 0;
  for (const file of files) {
    const path = resolve(base, file.path);
    assert.equal(realpathSync(path), path, `source symlink/escape: ${file.path}`);
    const before = lstatSync(path);
    assert(before.isFile() && before.nlink === 1 && before.size <= 64 * 1024 * 1024,
      'source must be a private bounded regular file');
    assert.equal((before.mode & 0o111) !== 0, file.mode === '100755', 'source executable mode differs from Git');
    total += before.size;
    assert(Number.isSafeInteger(total) && total <= 512 * 1024 * 1024, 'source total byte bound');
    const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    try {
      assert(same(before, fstatSync(fd)), 'source changed before open');
      const bytes = Buffer.alloc(before.size + 1);
      let used = 0;
      while (used < bytes.length) {
        const count = readSync(fd, bytes, used, bytes.length - used, null);
        if (count === 0) break;
        used += count;
      }
      assert.equal(used, before.size, 'source size changed while reading');
      assert(same(before, fstatSync(fd)) && same(before, lstatSync(path)), 'source changed while reading');
      file.bytes = bytes.subarray(0, used);
    } finally { closeSync(fd); }
  }
  assert.equal(decode(git(['ls-files', '--stage', '-z'])), inventory, 'Git source inventory changed');
  verifyClean();
  return sourceInventory(files);
}
