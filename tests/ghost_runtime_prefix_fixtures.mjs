// Cargo-free policy fixtures; they never build or execute the native probe.
import assert from 'node:assert/strict';
import { declaredRuntimePrefix } from '../scripts/ghost_runtime_prefix.mjs';

export function checkRuntimePrefixCases() {
  const runtime = (sha) => ({
    sha,
    subjects: ['perf(opt-in): change explicit policy'],
    changed: new Map([['src/policy.rs', ['let share = 1;']]]),
  });
  const choose = (candidates, options = {}) => declaredRuntimePrefix(candidates, { head: 'head', ...options });
  const newest = runtime('newest');
  const older = runtime('older');
  assert.equal(choose([newest, older]).head, 'newest');
  assert.equal(choose([runtime('head'), older]).head, 'older', 'the head cannot exempt its own suffix');
  assert.equal(choose([runtime('head')]), null);
  assert.equal(choose([]), null);
  const bad = [
    { ...newest, subjects: ['bench: diagnostics only'] },
    { ...newest, subjects: [...newest.subjects, 'test(verification): change ordering'] },
    { ...newest, changed: new Map([['src/policy.rs', ['#[cfg(miri)]']]]) },
    { ...newest, changed: new Map([['Cargo.lock', ['changed']]]) },
    { ...newest, changed: new Map([['src/ghost/mod.rs', ['changed']]]) },
  ];
  for (const candidate of bad) {
    assert.equal(choose([candidate]), null, 'an inadmissible prefix must retain the original proof');
    assert.equal(choose([candidate, older]).head, 'older', 'all changes after the admitted prefix remain in the proof');
  }
  function* forbiddenRead() { throw new Error('dirty trees must not search for an exemption'); }
  assert.equal(choose(forbiddenRead(), { dirty: [' M src/policy.rs'] }), null);
  const admitted = choose([older]);
  assert(admitted.reason.includes('declares a runtime change'));
  assert.deepEqual(admitted.subjects, older.subjects);
  assert.deepEqual(admitted.paths, ['src/policy.rs']);
  return 4 + bad.length * 2 + 4;
}
