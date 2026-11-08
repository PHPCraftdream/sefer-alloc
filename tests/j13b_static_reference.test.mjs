import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { linkSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, unlinkSync, writeFileSync } from 'node:fs';
import { dirname, posix, resolve, win32 } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { applyStaticReference, isStrictChild, STATIC_REFERENCE_SOURCES,
  staticReferenceOverlay } from '../scripts/j13b_static_reference.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const hash = text => createHash('sha256').update(text).digest('hex');
const originals = () => Object.fromEntries(Object.keys(STATIC_REFERENCE_SOURCES)
  .map(path => [path, readFileSync(resolve(ROOT, path), 'utf8')]));

function withCopy(run) {
  mkdirSync(resolve(ROOT, '.rush'), { recursive: true });
  const directory = mkdtempSync(resolve(ROOT, '.rush/j13b-static-test-'));
  try {
    const sources = originals();
    for (const [path, text] of Object.entries(sources)) {
      mkdirSync(dirname(resolve(directory, path)), { recursive: true });
      writeFileSync(resolve(directory, path), text);
    }
    return run(directory, sources);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

test('confinement rejects same path, siblings, traversal and cross-drive Windows paths', () => {
  for (const paths of [win32, posix]) {
    const base = paths === win32 ? 'D:\\repo\\.rush' : '/repo/.rush';
    assert.equal(isStrictChild(base, paths.join(base, 'copy'), paths), true);
    for (const destination of [base, paths.dirname(base), paths.join(base, '..', 'src'), `${base}-other`]) {
      assert.equal(isStrictChild(base, destination, paths), false);
    }
  }
  assert.equal(isStrictChild('D:\\repo\\.rush', 'E:\\repo\\.rush\\copy', win32), false);
  assert.equal(isStrictChild('D:\\repo\\.rush', '\\\\host\\share\\copy', win32), false);
});

test('overlay accepts only the exact reviewed whole-file sources', () => {
  const sources = originals();
  for (const [path, expected] of Object.entries(STATIC_REFERENCE_SOURCES)) assert.equal(hash(sources[path]), expected);
  for (const path of Object.keys(sources)) {
    assert.throws(() => staticReferenceOverlay({ ...sources, [path]: `${sources[path]}\n` }), /original source hash mismatch/);
    const missing = { ...sources };
    delete missing[path];
    assert.throws(() => staticReferenceOverlay(missing), /source set differs/);
  }
  assert.throws(() => staticReferenceOverlay({ ...sources, extra: 'unexpected' }), /source set differs/);
});

test('overlay changes only Always control selection and estimator admission, preserving all other source', () => {
  const sources = originals();
  const result = staticReferenceOverlay(sources);
  const [fresh, runtime] = Object.keys(STATIC_REFERENCE_SOURCES);
  const expectedFresh = sources[fresh].replace(
    'if tuple.mode == LargeThpMode::Auto {\n        let (selected, _ordinal)',
    'if matches!(tuple.mode, LargeThpMode::Always | LargeThpMode::Auto) {\n        let (selected, _ordinal)');
  assert.notEqual(result.output[fresh], sources[fresh]);
  assert.equal(result.output[fresh], expectedFresh);
  const admissionStart = sources[runtime].indexOf('        let applied = LEARNERS[group].try_observe(');
  const admissionEnd = sources[runtime].indexOf('\n    }', admissionStart);
  assert.ok(admissionStart >= 0 && admissionEnd > admissionStart);
  assert.equal(result.output[runtime], sources[runtime].slice(0, admissionStart)
    + '        // Benchmark-only static reference: validate/sample, but never update an estimator.\n        let _ = observation;'
    + sources[runtime].slice(admissionEnd));
  assert.equal(result.manifest.schema, 'j13b.static-reference-overlay.v1');
  assert.match(result.manifest.overlay_sha256, /^[a-f0-9]{64}$/);
  for (const entry of result.manifest.files) {
    assert.equal(entry.original_sha256, hash(sources[entry.path]));
    assert.equal(entry.result_sha256, hash(result.output[entry.path]));
    assert.notEqual(entry.original_sha256, entry.result_sha256);
  }
  assert.deepEqual(staticReferenceOverlay(sources), result);
});

test('application changes only a private CI copy and records resulting file hashes', () => withCopy((directory, sources) => {
  const manifest = applyStaticReference(directory);
  for (const entry of manifest.files) {
    assert.equal(hash(readFileSync(resolve(directory, entry.path))), entry.result_sha256);
    assert.equal(readFileSync(resolve(ROOT, entry.path), 'utf8'), sources[entry.path]);
  }
  assert.deepEqual(JSON.parse(readFileSync(resolve(directory, 'j13b-static-reference-overlay.json'), 'utf8')), manifest);
  assert.throws(() => applyStaticReference(directory), /original source hash mismatch/);
}));

test('invalid second source and preexisting receipt refuse before either original source is written', () => withCopy((directory, sources) => {
  const [fresh, runtime] = Object.keys(sources);
  writeFileSync(resolve(directory, runtime), `${sources[runtime]}\n`);
  assert.throws(() => applyStaticReference(directory), /original source hash mismatch/);
  assert.equal(readFileSync(resolve(directory, fresh), 'utf8'), sources[fresh]);
  writeFileSync(resolve(directory, runtime), sources[runtime]);
  writeFileSync(resolve(directory, 'j13b-static-reference-overlay.json'), 'existing receipt');
  assert.throws(() => applyStaticReference(directory), /EEXIST/);
  for (const [path, text] of Object.entries(sources)) assert.equal(readFileSync(resolve(directory, path), 'utf8'), text);
}));

test('production checkout, shared .rush root and source links are refused', () => withCopy(directory => {
  assert.throws(() => applyStaticReference(ROOT), /separate in-tree/);
  assert.throws(() => applyStaticReference(resolve(ROOT, '.rush')), /separate in-tree/);
  const [path] = Object.keys(STATIC_REFERENCE_SOURCES);
  const destination = resolve(directory, path);
  unlinkSync(destination);
  symlinkSync(resolve(ROOT, path), destination);
  assert.throws(() => applyStaticReference(directory), /private regular file/);
  unlinkSync(destination);
  linkSync(resolve(ROOT, path), destination);
  assert.throws(() => applyStaticReference(directory), /private regular file/);
}));
