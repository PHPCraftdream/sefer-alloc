// Synthetic cargo-free format fixtures only: no filesystem, Cargo, compiler or binary execution.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { sourceInventory } from '../scripts/j13b/source-inventory.mjs';
import { compilerArtifact } from '../scripts/j13b/compiler-artifact.mjs';

let checks = 0;
function check(run) { run(); checks++; }
const entry = (path = 'a', mode = '100644', text = 'x') => ({ path, mode, bytes: Buffer.from(text) });
const one = sourceInventory([entry()]);
check(() => {
  const bytes = Buffer.concat([Buffer.from('sefer-j13b-tracked-source-v1\0'),
    Buffer.from('0000000000000001610000000000000006313030363434000000000000000178', 'hex')]);
  assert.equal(one.source_sha256, createHash('sha256').update(bytes).digest('hex'));
  assert.equal(one.recipe, 'j13b.tracked-source.v1');
  assert.equal(one.total_bytes, 1);
  assert(Object.isFrozen(one.files) && Object.isFrozen(one.files[0]));
});
check(() => assert.deepEqual(sourceInventory([entry('b'), entry('a')]), sourceInventory([entry('a'), entry('b')])));
for (const changed of [entry('b'), entry('a', '100755'), entry('a', '100644', 'y'), entry('a', '100644', '')]) {
  check(() => assert.notEqual(sourceInventory([changed]).source_sha256, one.source_sha256));
}
check(() => assert.notEqual(sourceInventory([entry('a', '100644', 'bc')]).source_sha256,
  sourceInventory([entry('ab', '100644', 'c')]).source_sha256, 'length framing prevents concatenation collisions'));
check(() => {
  const input = entry(); const observed = sourceInventory([input]); input.bytes[0] = 0;
  assert.equal(observed.files[0].sha256, one.files[0].sha256);
});
for (const path of ['', '/absolute', '../escape', 'a/../b', 'a//b', './a', 'a/', '.git/config', 'a/.GIT/config',
  'a\\b', 'a\0b', 'a\nb', 'C:stream', '\ud800']) {
  check(() => assert.throws(() => sourceInventory([entry(path)])));
}
for (const mode of ['120000', '160000', '100600', '', 100644]) {
  check(() => assert.throws(() => sourceInventory([entry('a', mode)])));
}
check(() => assert.throws(() => sourceInventory([entry(), entry()]), /duplicate/));
check(() => assert.throws(() => sourceInventory([]), /count/));
check(() => assert.throws(() => sourceInventory(new Array(200001)), /count/));
check(() => assert.throws(() => sourceInventory([{ ...entry(), stage: 1 }]), /fields/));
check(() => assert.throws(() => sourceInventory([{ ...entry(), bytes: 'not owned bytes' }]), /byte bound/));

const expected = { packageId: 'path+file:///synthetic-root#sefer-alloc@0.3.0',
  sourcePath: resolve('synthetic-fixture', 'examples/g1_j13b_judge.rs'),
  executablePath: resolve('synthetic-output', 'release/examples/g1_j13b_judge') };
function artifact() {
  return { reason: 'compiler-artifact', package_id: expected.packageId,
    target: { name: 'g1_j13b_judge', kind: ['example'], crate_types: ['bin'], src_path: expected.sourcePath },
    executable: expected.executablePath, filenames: [expected.executablePath],
    profile: { test: false, debug_assertions: false, opt_level: '3' },
    features: ['production', 'internals', 'bench-internals'] };
}
const finish = { reason: 'build-finished', success: true };
const encode = messages => Buffer.from(messages.map(message => JSON.stringify(message)).join('\n') + '\n');
check(() => {
  const observed = compilerArtifact(encode([artifact(), finish]), expected);
  assert.deepEqual(observed.features, ['bench-internals', 'internals', 'production']);
  assert(Object.isFrozen(observed.features) && Object.isFrozen(observed.profile));
  assert.equal(observed.executable, expected.executablePath);
});
check(() => {
  const dependency = { ...artifact(), package_id: 'unrelated-dependency' };
  assert.equal(compilerArtifact(encode([dependency, artifact(), finish]), expected).package_id, expected.packageId);
});
const mutations = [
  a => { a.package_id = 'wrong-root'; },
  a => { a.target.name = 'different-example'; },
  a => { a.target.kind = ['lib']; },
  a => { a.target.kind.push('bin'); },
  a => { a.target.crate_types = ['rlib']; },
  a => { a.target.src_path = resolve('untracked-source.rs'); },
  a => { a.executable = resolve('stale-executable'); },
  a => { a.filenames = []; },
  a => { a.profile.test = true; },
  a => { a.profile.debug_assertions = true; },
  a => { a.profile.opt_level = '0'; },
  a => { a.features = ['production', 'production']; },
  a => { a.features = ['internals']; },
  a => { a.features = ['production', 'bad feature']; },
  a => { a.features = Array.from({ length: 513 }, (_, i) => `feature-${i}`); },
];
for (const mutate of mutations) {
  check(() => { const changed = artifact(); mutate(changed); assert.throws(() => compilerArtifact(encode([changed, finish]), expected)); });
}
for (const messages of [[], [artifact()], [finish], [artifact(), artifact(), finish],
  [artifact(), { ...finish, success: false }], [artifact(), finish, finish],
  [artifact(), finish, { reason: 'compiler-message' }], [null], [[]], [{ reason: 1 }]]) {
  check(() => assert.throws(() => compilerArtifact(encode(messages), expected)));
}
for (const bytes of [Buffer.from('{'), Buffer.from([0xff]), Buffer.alloc(0)]) {
  check(() => assert.throws(() => compilerArtifact(bytes, expected)));
}
check(() => assert.throws(() => compilerArtifact(encode([artifact(), finish]), { ...expected, executablePath: 'relative' })));
console.log(`[j13b-builder-fixtures] ${checks} synthetic source/artifact format checks passed; no compiler/native execution`);
