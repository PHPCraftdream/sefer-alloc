import { createHash } from 'node:crypto';
import { closeSync, lstatSync, openSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const FRESH = 'src/alloc_core/alloc_core/mem/advise_fresh_large.rs';
const RUNTIME = 'src/alloc_core/config/policy/large_thp_runtime/thp_runtime.rs';
const ADMISSION = `        let applied = LEARNERS[group].try_observe(&Self::current().gradient, observation);
        if applied {
            note_group_counter(&LEARNING_APPLIED, group, 1);
        } else {
            note_group_counter(&LEARNING_DROPPED, group, 1);
        }`;

// Whole-file hashes pin the reviewed seams, not just a coincidental text anchor.
export const STATIC_REFERENCE_SOURCES = Object.freeze({
  [FRESH]: '84bf2f9c070271187c4bb07f3bdc552ce7d0f2a686bc62ffc28ee299b93d41c4',
  [RUNTIME]: 'a2476ebd40189187dd8ea5a6a0966861eb90f6a8eea3712eb4cd2403cc75dc89',
});

function replaceOnce(source, before, after) {
  if (source.indexOf(before) < 0 || source.indexOf(before) !== source.lastIndexOf(before)) {
    throw Error('static reference source anchor is absent or ambiguous');
  }
  return source.replace(before, after);
}

export function staticReferenceOverlay(sources) {
  const paths = Object.keys(STATIC_REFERENCE_SOURCES);
  if (Object.keys(sources).length !== paths.length) throw Error('static reference source set differs');
  for (const path of paths) {
    if (typeof sources[path] !== 'string' || sha256(sources[path]) !== STATIC_REFERENCE_SOURCES[path]) {
      throw Error(`static reference original source hash mismatch: ${path}`);
    }
  }
  const output = {
    [FRESH]: replaceOnce(sources[FRESH],
      '    if tuple.mode == LargeThpMode::Auto {\n        let (selected, _ordinal)',
      '    if matches!(tuple.mode, LargeThpMode::Always | LargeThpMode::Auto) {\n        let (selected, _ordinal)'),
    [RUNTIME]: replaceOnce(sources[RUNTIME], ADMISSION,
      '        // Benchmark-only static reference: validate/sample, but never update an estimator.\n        let _ = observation;'),
  };
  const files = paths.map(path => ({ path, original_sha256: sha256(sources[path]), result_sha256: sha256(output[path]) }));
  // This digest identifies the exact two-file transformation, not a complete source tree or binary.
  const overlay_sha256 = sha256(JSON.stringify(paths.map(path => ({ path, before: sources[path], after: output[path] }))));
  return { output, manifest: { schema: 'j13b.static-reference-overlay.v1', overlay_sha256, files } };
}

export function isStrictChild(base, destination, paths = { relative, isAbsolute, sep }) {
  const local = paths.relative(base, destination);
  return local !== '' && local !== '..' && !local.startsWith(`..${paths.sep}`) && !paths.isAbsolute(local);
}

export function applyStaticReference(directory) {
  const destination = realpathSync(directory);
  const permitted = realpathSync(resolve(ROOT, '.rush'));
  if (!isStrictChild(realpathSync(ROOT), permitted) || !isStrictChild(permitted, destination)) {
    throw Error('static reference destination must be a separate in-tree .rush source copy');
  }
  const sources = {};
  for (const path of Object.keys(STATIC_REFERENCE_SOURCES)) {
    const requested = resolve(destination, path);
    const file = realpathSync(requested);
    const stat = lstatSync(requested);
    if (!isStrictChild(destination, file) || file !== requested || !stat.isFile() || stat.nlink !== 1) {
      throw Error('static reference seam must be a private regular file inside the source copy');
    }
    sources[path] = readFileSync(file, 'utf8');
  }
  const result = staticReferenceOverlay(sources);
  // Trusted, non-concurrently-mutated CI copy; this is not an adversarial filesystem sandbox.
  const receipt = openSync(resolve(destination, 'j13b-static-reference-overlay.json'), 'wx');
  try {
    for (const [path, text] of Object.entries(result.output)) writeFileSync(resolve(destination, path), text, 'utf8');
    writeFileSync(receipt, `${JSON.stringify(result.manifest, null, 2)}\n`);
    return result.manifest;
  } finally {
    closeSync(receipt);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [directory, ...extra] = process.argv.slice(2);
  if (!directory || extra.length) {
    console.error('usage: node scripts/j13b_static_reference.mjs <in-tree-source-copy>');
    process.exitCode = 2;
  } else {
    try {
      console.log(JSON.stringify(applyStaticReference(directory)));
    } catch (error) {
      console.error(`[j13b_static_reference] refused: ${error.message}`);
      process.exitCode = 1;
    }
  }
}
