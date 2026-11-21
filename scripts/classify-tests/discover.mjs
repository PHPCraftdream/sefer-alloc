// Cargo autotest discovery and non-target inventory from Git-selected paths.
import { posix, sep } from 'node:path';
import { packageValue } from './toml.mjs';

const toPosix = (p) => p.split(sep).join('/');

/** Extract explicit [[test]] targets from a parsed TOML document. */
export function explicitTestTargets(doc) {
  const tables = doc.sections.filter((s) =>
    s.kind === 'array' && s.path.length === 1 && s.path[0] === 'test');
  return tables.map((t) => {
    const v = t.values;
    const name = v.name;
    if (typeof name !== 'string' || name === '') {
      throw new Error(`[[test]] table without a name near path ${t.path.join('.')}`);
    }
    const relPath = typeof v.path === 'string' ? toPosix(v.path) : `tests/${name}.rs`;
    return {
      name,
      path: relPath,
      declared_explicit: true,
      harness: v.harness === undefined ? true : v.harness === true,
      test_enabled: v.test === undefined ? true : v.test === true,
      required_features: Array.isArray(v['required-features']) ? v['required-features'].map(String) : [],
    };
  });
}

/** Classify repo-relative POSIX file paths below testsDirName; the caller validates explicit paths. */
export function discoverTargets(testFiles, manifest, testsDirName = 'tests') {
  const explicit = explicitTestTargets(manifest);
  const autotests = manifest.autotests ?? packageValue(manifest, 'autotests', true);
  const targets = [...explicit];
  const nonTargets = [];
  const claimedPaths = new Set(explicit.map((t) => t.path));
  const targetNames = new Set(explicit.map((t) => t.name));
  const fixtures = new Set();
  const entryDirs = new Set();

  for (const path of new Set(testFiles)) {
    const parts = path.slice(testsDirName.length + 1).split('/');
    const file = parts.at(-1);
    let parent = testsDirName;
    for (const dir of parts.slice(0, -1)) {
      parent += `/${dir}`;
      fixtures.add(parent);
    }

    const directoryEntry = parts.length === 2 && file === 'main.rs';
    if (directoryEntry) entryDirs.add(parent);
    if (claimedPaths.has(path)) continue;

    let kind = 'other-file';
    if (directoryEntry || (parts.length === 1 && file.endsWith('.rs'))) {
      const name = directoryEntry ? parts[0] : file.slice(0, -3);
      if (targetNames.has(name)) throw new Error(`duplicate target name: ${name}`);
      targetNames.add(name);
      if (autotests !== false) {
        targets.push({ name, path, declared_explicit: false,
          harness: true, test_enabled: true, required_features: [] });
        continue;
      }
    } else if (file.endsWith('.rs')) kind = 'support-module';
    else if (file.endsWith('.mjs')) kind = 'mjs-script';
    else if (file.endsWith('.proptest-regressions')) kind = 'proptest-regressions';
    nonTargets.push({ path, kind });
  }

  for (const path of fixtures) {
    if (!entryDirs.has(path)) nonTargets.push({ path, kind: 'directory-fixture' });
  }

  targets.sort((a, b) => a.path.localeCompare(b.path));
  nonTargets.sort((a, b) => a.path.localeCompare(b.path));

  const names = new Set();
  const paths = new Set();
  for (const t of targets) {
    if (names.has(t.name)) throw new Error(`duplicate target name: ${t.name}`);
    if (paths.has(t.path)) throw new Error(`duplicate target path: ${t.path}`);
    names.add(t.name); paths.add(t.path);
  }
  return { targets, nonTargets, explicitCount: explicit.length };
}

export { posix };
