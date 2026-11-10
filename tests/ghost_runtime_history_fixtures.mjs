// Cargo-free graph fixtures: no native build or probe, and no tolerance/skip changes.
import assert from 'node:assert/strict';
import { runtimeHistory } from '../scripts/ghost_runtime_history.mjs';

export function checkRuntimeHistoryCases() {
  const base = 'main-base', head = 'landing';
  const records = {
    'rev-list --parents -n 1 landing': 'landing main-base side',
    'merge-base --all main-base side': 'common',
    'rev-parse --verify landing^{tree}': 'source-tree',
    'rev-parse --verify side^{tree}': 'source-tree',
    'rev-parse --verify main-base^{tree}': 'base-tree',
    'rev-parse --verify common^{tree}': 'base-tree',
  };
  const read = overrides => args => {
    const key = args.join(' ');
    assert(Object.hasOwn({ ...records, ...overrides }, key), `unexpected graph query: ${key}`);
    return { ...records, ...overrides }[key];
  };
  const bridged = runtimeHistory({ base, head }, read({}));
  assert.equal(bridged.base, 'common');
  assert.equal(bridged.head, 'side');
  assert.equal(bridged.bridge.landing, head);
  assert.equal(bridged.bridge.first_parent, base);
  const mutants = [
    ['rev-list --parents -n 1 landing', 'landing main-base'],
    ['rev-list --parents -n 1 landing', 'landing main-base side third'],
    ['rev-list --parents -n 1 landing', 'landing other-base side'],
    ['rev-list --parents -n 1 landing', 'wrong main-base side'],
    ['merge-base --all main-base side', ''],
    ['merge-base --all main-base side', 'common other-common'],
    ['rev-parse --verify landing^{tree}', 'conflict-resolution-tree'],
    ['rev-parse --verify side^{tree}', 'other-side-tree'],
    ['rev-parse --verify main-base^{tree}', 'new-main-source'],
    ['rev-parse --verify common^{tree}', 'different-common-source'],
    ['rev-parse --verify landing^{tree}', ''],
    ['rev-parse --verify main-base^{tree}', ''],
  ];
  for (const [key, value] of mutants) {
    assert.deepEqual(runtimeHistory({ base, head }, read({ [key]: value })), { base, head },
      `${key}: a non-identical or ambiguous merge must retain the original proof`);
  }
  return 4 + mutants.length;
}
