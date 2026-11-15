// Test-target discovery implementing Cargo's autotests rules, plus a
// non-target inventory of tests/, from a parsed manifest + directory walk.
//
// Purpose: derive every test target of the root crate (name, path,
// declared_explicit, harness, test_enabled, required_features) and every
// NON-target entry under tests/ (kind), without invoking cargo.
//
// Rules implemented (mirroring Cargo autotests):
//   - `autotests` defaults to true; if false, only explicit [[test]] tables.
//   - flat tests/<stem>.rs files are auto-discovered as target `<stem>`,
//     unless an explicit [[test]] table already claims that path.
//   - tests/<dir>/main.rs is target `<dir>` ONLY at depth 1 (direct child of
//     tests/). Deeper nested main.rs files (e.g. trybuild-style
//     tests/compile_fail/<case>/src/main.rs) are NOT targets: they are
//     inventoried recursively as non-targets.
//   - anything else under tests/ subdirectories is NOT a target.
//
// Derives: targets[], nonTargets[] (both sorted by path), counts.
// Asserts: no duplicate target names or paths; every explicit [[test]] table
// resolves to a path; explicit paths override only one auto-discovery entry.

import { readdirSync, statSync, existsSync } from 'node:fs';
import { join, posix, sep } from 'node:path';
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

/** Walk tests/ recursively; returns { targets, nonTargets }. */
export function discoverTargets(rootDir, manifest, testsDirName = 'tests') {
  const explicit = explicitTestTargets(manifest);
  // `autotests` lives in [package]; accept BOTH a raw manifest object with a
  // top-level `autotests` boolean (synthetic callers/tests) and a parsed
  // parseToml() doc where it sits in sections. packageValue() handles both
  // (a raw object without `sections` falls back to the given default, so we
  // check the raw boolean first).
  const autotests = (manifest && manifest.autotests !== undefined)
    ? manifest.autotests
    : packageValue(manifest, 'autotests', true);
  const testsDir = join(rootDir, testsDirName);
  const targets = [...explicit.map((t) => ({ ...t }))];
  const nonTargets = [];

  const claimedPaths = new Set(explicit.map((t) => t.path));
  const byName = new Map(targets.map((t) => [t.name, t]));

  const walk = (dir, rel) => {
    const entries = readdirSync(dir, { withFileTypes: true })
      .sort((a, b) => a.name.localeCompare(b.name));
    for (const ent of entries) {
      const childRel = rel === '' ? ent.name : `${rel}/${ent.name}`;
      const abs = join(dir, ent.name);
      if (ent.isDirectory()) {
        const isTopLevel = rel === '';
        const mainAbs = join(abs, 'main.rs');
        if (isTopLevel && existsSync(mainAbs) && statSync(mainAbs).isFile()) {
          // tests/<dir>/main.rs is target <dir> — depth 1 ONLY.
          const p = toPosix(`${testsDirName}/${childRel}/main.rs`);
          if (!claimedPaths.has(p)) {
            if (byName.has(ent.name)) throw new Error(`duplicate target name: ${ent.name}`);
            byName.set(ent.name, { name: ent.name, path: p, declared_explicit: false,
              harness: true, test_enabled: true, required_features: [] });
            targets.push(byName.get(ent.name));
            claimedPaths.add(p);
          }
          // Deeper entries under a target dir are not inventoried (Cargo
          // treats the whole dir as one target).
        } else {
          // Deeper dirs are fixtures / support trees, not target roots.
          nonTargets.push({ path: toPosix(`${testsDirName}/${childRel}`), kind: 'directory-fixture' });
          walk(abs, childRel);
        }
        continue;
      }
      if (rel === '' && ent.isFile() && ent.name.endsWith('.rs')) { // Flat auto-discovery candidate (depth 1 only).
        const p = toPosix(`${testsDirName}/${ent.name}`);
        if (claimedPaths.has(p)) continue; // explicit table owns this file
        const stem = ent.name.slice(0, -3);
        if (byName.has(stem)) throw new Error(`duplicate target name: ${stem}`);
        const t = { name: stem, path: p, declared_explicit: false,
          harness: true, test_enabled: true, required_features: [] };
        byName.set(stem, t);
        targets.push(t);
        continue;
      }
      // Non-target entries.
      let kind;
      if (ent.isFile() && ent.name.endsWith('.rs')) kind = 'support-module';
      else if (ent.isFile() && ent.name.endsWith('.mjs')) kind = 'mjs-script';
      else if (ent.isFile() && ent.name.endsWith('.proptest-regressions')) kind = 'proptest-regressions';
      else kind = 'other-file';
      nonTargets.push({ path: toPosix(`${testsDirName}/${childRel}`), kind });
    }
  };

  walk(testsDir, '');

  if (autotests === false) {
    // Only explicit tables are targets; drop auto-discovered entries and
    // reclassify their files as support-modules of unknown status.
    for (let i = targets.length - 1; i >= 0; i--) {
      if (!targets[i].declared_explicit) {
        nonTargets.push({ path: targets[i].path, kind: 'other-file' });
        targets.splice(i, 1);
      }
    }
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
