import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const digest = value => typeof value === 'string' && value.length === 64 && /^[a-f0-9]+$/.test(value);
const record = value => value !== null && typeof value === 'object' &&
  [Object.prototype, null].includes(Object.getPrototypeOf(value));
const strings = value => Array.isArray(value) && value.every(item => typeof item === 'string');
const sourcePath = value => /^(?:[\w.-]+\/)*[\w.-]+\.rs$/.test(value) &&
  !value.split('/').some(part => part === '.' || part === '..');

function canonical(value, seen = new Set()) {
  if (value === null || ['string', 'boolean'].includes(typeof value) || Number.isFinite(value)) {
    return JSON.stringify(value);
  }
  assert((Array.isArray(value) || record(value)) && !seen.has(value), 'malformed selection JSON');
  seen.add(value);
  const entries = Object.getOwnPropertyDescriptors(value);
  const keys = Reflect.ownKeys(entries);
  const encode = key => {
    const entry = Object.hasOwn(entries, key) && entries[key];
    assert(entry?.enumerable && Object.hasOwn(entry, 'value'), 'malformed selection JSON property');
    return canonical(entry.value, seen);
  };
  let result;
  if (Array.isArray(value)) {
    assert(Object.getPrototypeOf(value) === Array.prototype && keys.length === value.length + 1,
      'malformed selection JSON array');
    result = `[${Array.from({ length: value.length }, (_, i) => encode(i)).join(',')}]`;
  } else {
    assert(keys.every(key => typeof key === 'string'), 'malformed selection JSON key');
    result = `{${keys.sort().map(key => `${JSON.stringify(key)}:${encode(key)}`).join(',')}}`;
  }
  seen.delete(value);
  return result;
}

function compactFields(selection) {
  assert(record(selection), 'malformed selection artifact');
  const fields = Object.getOwnPropertyDescriptors(selection);
  delete fields.integrity;
  return Object.defineProperties(Object.create(null), fields);
}

function receiptFor(selection, sourceSha256) {
  const compactSha256 = sha256(canonical({ selection, sourceSha256 }));
  const { mode, reason, total, tests, mapShas, crateOnly } = selection;
  assert(['all', 'subset'].includes(mode) && typeof reason === 'string' && reason.trim() &&
    Number.isSafeInteger(total) && total >= 0, 'malformed selection artifact');
  assert(tests === undefined ? mode === 'all' : strings(tests) && tests.every(sourcePath) &&
    new Set(tests).size === tests.length && tests.length <= total, 'malformed selection tests');
  assert(mapShas === undefined || strings(mapShas), 'malformed selection mapShas');
  assert(crateOnly === undefined || (record(crateOnly) && Object.hasOwn(crateOnly, 'crates') &&
    strings(crateOnly.crates)), 'malformed selection crateOnly');
  return { sourceSha256, compactSha256, testCount: tests?.length ?? 0 };
}

export function sealSelection(selection, raw) {
  assert(raw instanceof Uint8Array, 'original selection bytes required');
  const compact = compactFields(selection);
  return { ...compact, integrity: receiptFor(compact, sha256(raw)) };
}

export function assertSelectionIntegrity(selection) {
  const compact = compactFields(selection);
  const entry = Object.getOwnPropertyDescriptor(selection, 'integrity');
  assert(entry?.enumerable && record(entry.value), 'missing or malformed selection integrity receipt');
  canonical(entry.value);
  const { sourceSha256, compactSha256, testCount } = entry.value;
  assert(Reflect.ownKeys(entry.value).length === 3 &&
    ['sourceSha256', 'compactSha256', 'testCount'].every(key => Object.hasOwn(entry.value, key)) &&
    digest(sourceSha256) && digest(compactSha256), 'malformed selection integrity receipt');
  const expected = receiptFor(compact, sourceSha256);
  assert.equal(testCount, expected.testCount, 'selection integrity test count mismatch');
  assert.equal(compactSha256, expected.compactSha256, 'selection integrity digest mismatch');
}
