#!/usr/bin/env node
// Plan §3 "Test inventory" gate for the P0.2 pilot merge (sefer-alloc).
//
// Purpose: prove, WITHOUT cargo, that the move of the 58 flat
// tests/regression_<name>.rs files into the single flat target
// tests/regression_merged.rs (modules tests/regression_merged/<name>.rs)
// is LOSSLESS: every `#[test]` fn, every `proptest!` property and every
// `#[test_case]` (documented approximation) survives with the same name
// and the same cfg context, and no collateral target changed.
//
// Usage:
//   node scripts/test-inventory.mjs [--ref <git-ref>]
//        [--before-dir <dir>] [--after-dir <dir>]
//        [--merged-target regression_merged] [--csv <out.csv>]
//
// Sources:
//   - "before" side: git ref via `git show <ref>:<path>` /
//     `git ls-tree --name-only <ref> tests/` / `git show <ref>:Cargo.toml`
//     when --ref is given, else a plain directory (--before-dir, for tests).
//   - "after" side: the working tree by default, else --after-dir.
//
// Targets: flat tests/*.rs are auto-discovered targets (cargo autotests on),
// minus any file covered by a `[[test]]` table with `test = false`.
// `[[test]]` tables with a `path` also count (minimal TOML parse of
// name/path/test/harness/required-features — enough to know which flat
// files are NOT targets).
//
// Inventory per target file:
//   - comments (`//`, `/* */`) and string/char literals stripped first;
//   - file-level inner `#![cfg(...)]` / `#![cfg_attr(...)]` attributes
//     before the first real item become the FILE cfg list (order kept);
//   - `#[test]` fns anywhere (incl. nested `mod` blocks, arbitrary depth);
//     attributes between `#[test]` and `fn` collected; the per-test cfg
//     context = enclosing mod cfg attrs (ordered) + fn cfg attrs (ordered);
//   - `proptest! { fn name(...) ... }` properties count as one test each;
//   - each `#[test_case(...)]` occurrence counts as one test named by the
//     following fn (approximation; zero expected in this repo).
//
// Classification:
//   moved      = flat target before, absent flat after, declared as a module
//                of the merged target after;
//   unchanged  = flat target on both sides;
//   ADDED (red)     = new flat target after, not before, not the merged one;
//   ORPHAN (red)    = .rs file directly under tests/<merged>/ not declared
//                     exactly once by the merged target.
//
// Equality proof per moved stem: multiset of (full path = stem::innerpath,
// normalized cfg context) before vs after — sets equal, each full path
// exactly once per side. Cfg normalization: all whitespace removed
// (stacked file cfg ORDER and item-cfg ORDER remain significant).
//
// Output: Markdown table (stem | tests before | tests after | status) over
// moved stems, then one summary line and a PASS/FAIL verdict; exit 0 iff
// all counters zero / diffs clean. In-script asserts enforce the
// derived-not-typed rule: lost+kept == B, lost+kept+added == A, and the
// per-stem counts sum to the totals.
//
// Determinism: sorted output, no timestamps, writes nothing unless --csv
// is given (then only that file).

import { spawnSync } from 'node:child_process';
import { readdirSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';

// ---------------------------------------------------------------- CLI
export function parseArgs(argv) {
  const opts = {
    ref: null, beforeDir: null, afterDir: null,
    mergedTarget: 'regression_merged', csv: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--ref') opts.ref = argv[++i];
    else if (a === '--before-dir') opts.beforeDir = argv[++i];
    else if (a === '--after-dir') opts.afterDir = argv[++i];
    else if (a === '--merged-target') opts.mergedTarget = argv[++i];
    else if (a === '--csv') opts.csv = argv[++i];
    else { opts.unknown = a; return opts; }
  }
  return opts;
}

const repoRoot = resolve(new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'));

function gitShow(ref, path) {
  const r = spawnSync('git', ['show', `${ref}:${path}`], {
    cwd: repoRoot, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
  });
  if (r.status !== 0) throw new Error(`git show ${ref}:${path} failed: ${r.stderr}`);
  return r.stdout;
}
function gitListTests(ref) {
  const r = spawnSync('git', ['ls-tree', '--name-only', ref, 'tests/'], {
    cwd: repoRoot, encoding: 'utf8',
  });
  if (r.status !== 0) throw new Error(`git ls-tree failed: ${r.stderr}`);
  return r.stdout.split(/\r?\n/).filter(Boolean);
}

// ------------------------------------------------- literal/comment stripping
// Replace comments and string/char literals with spaces (structure kept).
export function stripLiterals(src) {
  let out = '';
  let i = 0;
  const n = src.length;
  const blank = (s) => s.replace(/[^\n]/g, ' ');
  while (i < n) {
    const c = src[i];
    const two = src.slice(i, i + 2);
    if (two === '//') {
      const end = src.indexOf('\n', i);
      const stop = end === -1 ? n : end;
      out += blank(src.slice(i, stop));
      i = stop;
    } else if (two === '/*') {
      const end = src.indexOf('*/', i + 2);
      const stop = end === -1 ? n : end + 2;
      out += blank(src.slice(i, stop));
      i = stop;
    } else if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"') {
        if (src[j] === '\\') j++;
        j++;
      }
      const stop = Math.min(j + 1, n);
      // keep the literal verbatim except structural punctuation, so cfg
      // feature names survive but a "#[test] ..." inside a string can
      // never be parsed as syntax
      out += src.slice(i, stop).replace(/[#[\]{}();]/g, ' ');
      i = stop;
    } else if (c === "'") {
      // lifetime or char literal. Lifetimes are `'a` / `'static` (an
      // identifier run with NO closing quote); char literals are anything
      // else up to the closing quote — including brace chars like '{' or
      // '}', whose content MUST be blanked or the stripped code ends up
      // with unbalanced braces (which can send the item scanner into an
      // infinite loop).
      let j = i + 1;
      let isChar = false;
      if (src[j] === '\\') { isChar = true; }
      else if (j < n && src[j] !== "'") {
        if (src[j + 1] === "'") {
          j += 1; isChar = true; // 'x'
        } else if (/[A-Za-z_]/.test(src[j])) {
          // identifier run: lifetime if no closing quote follows it,
          // char literal (e.g. 'a') if it does
          let k = j;
          while (k < n && /[A-Za-z0-9_]/.test(src[k])) k++;
          if (src[k] === "'") { j = k; isChar = true; }
          // else: lifetime, keep the tick as-is
        } else {
          // punctuation char literal like '{': char iff a closing quote
          // exists on the same line
          const nl = src.indexOf('\n', j);
          const q = src.indexOf("'", j);
          if (q !== -1 && (nl === -1 || q < nl)) { j = q; isChar = true; }
          // else: malformed/stray tick, treat as lifetime
        }
      }
      if (isChar) {
        while (j < n && src[j] !== "'") j++;
        // keep the literal verbatim except structural punctuation — a
        // brace/bracket char literal ('{' etc.) must not contribute real
        // braces to the stripped code
        out += src.slice(i, Math.min(j + 1, n)).replace(/[#[\]{}();]/g, ' ');
        i = Math.min(j + 1, n);
      } else {
        out += c; i++; // lifetime tick, keep as-is
      }
    } else {
      out += c; i++;
    }
  }
  return out;
}

// ------------------------------------------------------- balanced scanning
function findMatching(code, openPos, openCh, closeCh) {
  let depth = 0;
  for (let i = openPos; i < code.length; i++) {
    if (code[i] === openCh) depth++;
    else if (code[i] === closeCh) {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}
// skip an item body: to the terminating ';' or past a balanced '{...}'
function skipItem(code, pos) {
  while (pos < code.length && /\s/.test(code[pos])) pos++;
  if (pos >= code.length) return pos;
  if (code[pos] === '{') {
    const c = findMatching(code, pos, '{', '}');
    return c === -1 ? code.length : c + 1;
  }
  if (code[pos] === ';') return pos + 1;
  // walk to ';' or '{' (generics / where-clauses contain no braces here).
  // An UNMATCHED opener (e.g. the `<<` shift in `1 << 16`) must never
  // rewind the cursor — findMatching returning -1 advances by one char.
  let i = pos;
  while (i < code.length && code[i] !== ';' && code[i] !== '{') {
    if (code[i] === '<' || code[i] === '(') {
      const [o, c] = code[i] === '<' ? ['<', '>'] : ['(', ')'];
      const m = findMatching(code, i, o, c);
      i = m === -1 ? i + 1 : m + 1;
    } else i++;
  }
  if (i < code.length && code[i] === '{') {
    const c = findMatching(code, i, '{', '}');
    return c === -1 ? code.length : c + 1;
  }
  return i + 1;
}

function normCfg(s) { return s.replace(/\s+/g, ''); }
function attrInner(code, attrStart) {
  // attrStart at '#' (and optional '!'), returns balanced-bracket content
  const open = code.indexOf('[', attrStart);
  const close = findMatching(code, open, '[', ']');
  return code.slice(open + 1, close);
}
function isCfgAttr(inner) {
  const t = inner.trim();
  return t === 'cfg' || t.startsWith('cfg(') || t.startsWith('cfg_attr(');
}

// ------------------------------------------------------------- inventory
// Returns { fileCfgs: [normalized...], tests: [{path, cfg}], proptestCount,
// testCases: [names], directDefs: n } — directDefs = #[test]/proptest
// properties declared OUTSIDE any module (for the merged-target zero check).
export function extractInventory(srcRaw) {
  const code = stripLiterals(srcRaw);
  const fileCfgs = [];
  const tests = [];
  const testCases = [];
  let directDefs = 0;

  function collectAttrs(pos, end, allowInner) {
    // returns { attrs: [{inner, raw}], end } — consumes attributes
    const attrs = [];
    let p = pos;
    for (;;) {
      while (p < end && /\s/.test(code[p])) p++;
      if (p >= end || code[p] !== '#') break;
      let q = p + 1;
      let inner = false;
      if (code[q] === '!') { inner = true; q++; }
      if (code[q] !== '[') break;
      const close = findMatching(code, q, '[', ']');
      if (close === -1 || close >= end) break;
      const content = code.slice(q + 1, close);
      if (inner) {
        if (allowInner && isCfgAttr(content)) fileCfgs.push(normCfg(content));
      } else {
        attrs.push({ inner: content.trim(), cfg: isCfgAttr(content) });
      }
      p = close + 1;
    }
    return { attrs, end: p };
  }

  function walk(start, end, modPath, modCfgs, insideMod) {
    let p = start;
    const pendingMods = [];
    for (;;) {
      const { attrs, end: afterAttrs } = collectAttrs(p, end, !insideMod);
      p = afterAttrs;
      while (p < end && /\s/.test(code[p])) p++;
      if (p >= end) break;
      const word = /^([A-Za-z_][A-Za-z0-9_]*)/.exec(code.slice(p, end));
      if (!word) { p++; continue; }
      const kw = word[1];
      const after = p + kw.length;

      if (kw === 'mod') {
        let q = after;
        while (q < end && /\s/.test(code[q])) q++;
        const nameM = /^([A-Za-z_][A-Za-z0-9_]*)/.exec(code.slice(q, end));
        if (!nameM) { p = q; continue; }
        const name = nameM[1];
        let r = q + name.length;
        while (r < end && /\s/.test(code[r])) r++;
        if (code[r] === '{') {
          const close = findMatching(code, r, '{', '}');
          const cfgs = [...modCfgs, ...attrs.filter((a) => a.cfg).map((a) => normCfg(a.inner))];
          pendingMods.push({ r: r + 1, close, path: [...modPath, name], cfgs });
          p = close + 1;
        } else {
          p = skipItem(code, r); // `mod x;`
        }
      } else if (kw === 'fn') {
        let q = after;
        while (q < end && /\s/.test(code[q])) q++;
        const nameM = /^([A-Za-z_][A-Za-z0-9_]*)/.exec(code.slice(q, end));
        const name = nameM ? nameM[1] : '?';
        const isTest = attrs.some((a) => a.inner === 'test');
        const cfgs = [...modCfgs, ...attrs.filter((a) => a.cfg).map((a) => normCfg(a.inner))];
        if (isTest) {
          tests.push({ path: [...modPath, name].join('::'), cfg: cfgs });
          if (!insideMod) directDefs++;
        }
        if (attrs.some((a) => a.inner.startsWith('test_case'))) {
          testCases.push([...modPath, name].join('::'));
          if (!insideMod) directDefs++;
        }
        p = skipItem(code, q + name.length);
      } else if (kw === 'proptest' || kw === 'proptest_stateful') {
        let q = after;
        while (q < end && /\s/.test(code[q])) q++;
        if (code[q] !== '!') { p = q; continue; }
        q++;
        while (q < end && /\s/.test(code[q])) q++;
        if (code[q] !== '{') { p = q; continue; }
        const close = findMatching(code, q, '{', '}');
        const cfgs = [...modCfgs, ...attrs.filter((a) => a.cfg).map((a) => normCfg(a.inner))];
        const body = code.slice(q + 1, close);
        const fnRe = /\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/g;
        let m;
        while ((m = fnRe.exec(body)) !== null) {
          tests.push({ path: [...modPath, m[1]].join('::'), cfg: cfgs });
          if (!insideMod) directDefs++;
        }
        p = close + 1;
      } else if (kw === 'impl' || kw === 'trait') {
        p = skipItem(code, p);
      } else if (kw === 'struct' || kw === 'enum' || kw === 'union' ||
                 kw === 'use' || kw === 'const' || kw === 'static' ||
                 kw === 'type' || kw === 'extern' || kw === 'macro_rules' ||
                 kw === 'unsafe' || kw === 'async') {
        // `unsafe { }` / `async fn` handled by generic skip below
        p = skipItem(code, p);
      } else {
        // unknown item keyword (e.g. `pub`): consume it and continue
        p = after;
      }
    }
    // nested modules are walked AFTER the enclosing level's own items, so
    // inventory order follows "enclosing items first, then modules" (the
    // multiset comparison itself is order-insensitive)
    for (const m of pendingMods) walk(m.r, m.close, m.path, m.cfgs, true);
  }

  walk(0, code.length, [], [], false);
  return { fileCfgs, tests, testCases, directDefs };
}

// ------------------------------------------------------------ Cargo.toml
// Minimal [[test]] table parse: name/path/test/harness/required-features.
export function parseTestTables(cargoToml) {
  const tables = [];
  const lines = cargoToml.split(/\r?\n/);
  let cur = null;
  for (const lineRaw of lines) {
    const line = lineRaw.replace(/(^|\s)#.*$/, ''); // strip comments
    const sec = /^\s*\[\[\s*test\s*\]\]\s*$/.exec(line);
    if (sec) { if (cur) tables.push(cur); cur = {}; continue; }
    if (/^\s*\[/.test(line)) { if (cur) tables.push(cur); cur = null; continue; }
    if (!cur) continue;
    const kv = /^\s*(name|path|test|harness|required-features)\s*=\s*(.+?)\s*$/.exec(line);
    if (kv) {
      let v = kv[2];
      if (kv[1] === 'test' || kv[1] === 'harness') cur[kv[1]] = /\btrue\b/.test(v);
      else if (kv[1] === 'required-features') cur[kv[1]] = v;
      else cur[kv[1]] = v.replace(/^"(.*)"$/, '$1');
    }
  }
  if (cur) tables.push(cur);
  return tables;
}
export function autotestsEnabled(cargoToml) {
  const m = /^\s*autotests\s*=\s*(true|false)/m.exec(cargoToml.replace(/(^|\s)#.*$/gm, ''));
  return m ? m[1] === 'true' : true;
}

// --------------------------------------------------------------- sources
function dirListRust(dir, sub) {
  const full = join(dir, sub);
  try { return readdirSync(full).filter((f) => f.endsWith('.rs')).sort(); }
  catch { return []; }
}
function readFileOrThrow(p) { return readFileSync(p, 'utf8'); }

function readSide(opts, which) {
  // returns { files: Map<testsRelPath, content>, cargoToml }
  if (which === 'before' && opts.ref) {
    const files = new Map();
    for (const p of gitListTests(opts.ref)) {
      if (!p.endsWith('.rs')) continue;
      files.set(p, gitShow(opts.ref, p));
    }
    return { files, cargoToml: gitShow(opts.ref, 'Cargo.toml') };
  }
  // 'after' side defaults to the repository working tree (see usage above)
  const root = which === 'before' ? opts.beforeDir : (opts.afterDir || repoRoot);
  if (!root) throw new Error(`${which} side requires --ref or --before-dir`);
  const files = new Map();
  for (const f of dirListRust(root, 'tests')) {
    files.set(`tests/${f}`, readFileOrThrow(join(root, 'tests', f)));
  }
  const mergedDir = join(root, 'tests', opts.mergedTarget);
  try {
    for (const f of readdirSync(mergedDir)) {
      if (f.endsWith('.rs')) files.set(`tests/${opts.mergedTarget}/${f}`, readFileOrThrow(join(mergedDir, f)));
    }
  } catch { /* no merged dir */ }
  let cargoToml = '';
  try { cargoToml = readFileOrThrow(join(root, 'Cargo.toml')); } catch { cargoToml = ''; }
  return { files, cargoToml };
}

// flat .rs files under tests/ that are actual harness targets
export function flatTargets(side, mergedTarget) {
  const auto = autotestsEnabled(side.cargoToml);
  const tables = parseTestTables(side.cargoToml);
  const disabled = new Set();
  const explicit = new Set();
  for (const t of tables) {
    const path = t.path || (t.name ? `tests/${t.name}.rs` : null);
    if (!path) continue;
    if (t.test === false) disabled.add(path);
    else explicit.add(path);
  }
  const targets = new Set();
  for (const p of side.files.keys()) {
    if (!/^tests\/[^/]+\.rs$/.test(p)) continue;
    const stem = p.slice('tests/'.length, -3);
    if (stem === mergedTarget) continue; // handled separately
    if (disabled.has(p)) continue;
    if (!auto && !explicit.has(p)) continue;
    targets.add(stem);
  }
  return targets;
}

// merged target: top-level `#[cfg...]* mod <name>;` declarations
export function parseMergedDecls(src) {
  const code = stripLiterals(src);
  const decls = new Map(); // stem -> [normalized cfgs]
  let p = 0;
  const n = code.length;
  for (;;) {
    // consume doc comments already stripped; collect cfg attrs
    const cfgs = [];
    for (;;) {
      while (p < n && /\s/.test(code[p])) p++;
      if (code[p] !== '#') break;
      let q = p + 1;
      if (code[q] === '!') q++;
      if (code[q] !== '[') break;
      const close = findMatching(code, q, '[', ']');
      const content = code.slice(q + 1, close).trim();
      if (isCfgAttr(content)) cfgs.push(normCfg(content));
      p = close + 1;
    }
    const m = /^mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;/.exec(code.slice(p, p + 400));
    if (!m) {
      // skip whatever this token is and retry (doc-comment placeholders etc.)
      const w = /\S/.exec(code.slice(p));
      if (!w) break;
      p += w.index + 1;
      continue;
    }
    decls.set(m[1], cfgs);
    p += m[0].length;
  }
  return decls;
}

// ------------------------------------------------------------- comparison
function cfgKey(ctx) { return ctx.join(';'); }
function multiset(entries) {
  const m = new Map();
  for (const e of entries) {
    const k = `${e.path}|${cfgKey(e.cfg)}`;
    m.set(k, (m.get(k) || 0) + 1);
  }
  return m;
}
function multisetIntersection(a, b) {
  let n = 0;
  for (const [k, v] of a) n += Math.min(v, b.get(k) || 0);
  return n;
}
function multisetDiff(a, b) {
  // entries in a not covered by b ("lost") and vice versa ("added")
  const lost = []; const added = [];
  const B = new Map(b);
  for (const [k, v] of a) {
    const have = B.get(k) || 0;
    for (let i = 0; i < Math.max(0, v - have); i++) lost.push(k);
    B.set(k, Math.max(0, have - v));
  }
  for (const [k, v] of B) for (let i = 0; i < v; i++) added.push(k);
  return { lost, added };
}

// ------------------------------------------------------------------ main
export function runGate(opts) {
  const mt = opts.mergedTarget;
  const before = readSide(opts, 'before');
  const after = readSide(opts, 'after');

  const beforeTargets = flatTargets(before, mt);
  const afterTargets = flatTargets(after, mt);

  const mergedSrc = after.files.get(`tests/${mt}.rs`);
  if (mergedSrc === undefined) throw new Error(`missing tests/${mt}.rs on the after side`);
  const decls = parseMergedDecls(mergedSrc);

  // merged target dir files must each be declared exactly once
  const mergedDirFiles = [...after.files.keys()]
    .filter((p) => p.startsWith(`tests/${mt}/`) && p.endsWith('.rs'))
    .map((p) => p.slice(`tests/${mt}/`.length, -3));
  const orphans = [];
  for (const f of mergedDirFiles) {
    if (!decls.has(f)) orphans.push(f);
  }
  const declCounts = new Map();
  for (const f of mergedDirFiles) declCounts.set(f, (declCounts.get(f) || 0) + 0);
  for (const stem of decls.keys()) {
    if (!mergedDirFiles.includes(stem)) orphans.push(`${stem} (declared, file missing)`);
  }

  const moved = [];
  const unchanged = [];
  const addedTargets = [];
  for (const stem of [...beforeTargets].sort()) {
    if (afterTargets.has(stem)) unchanged.push(stem);
    else if (decls.has(stem)) moved.push(stem);
    else addedTargets.push(stem); // target disappeared without moving
  }
  for (const stem of [...afterTargets].sort()) {
    if (!beforeTargets.has(stem) && stem !== mt) addedTargets.push(stem);
  }

  const inv = (side, stem) => extractInventory(side.files.get(`tests/${stem}.rs`));
  const invMerged = (stem) => extractInventory(after.files.get(`tests/${mt}/${stem}.rs`));

  // inventories
  const beforeInv = new Map();
  for (const stem of moved) beforeInv.set(stem, inv(before, stem));
  for (const stem of unchanged) beforeInv.set(stem, inv(before, stem));
  const afterInv = new Map();
  for (const stem of moved) afterInv.set(stem, invMerged(stem));
  for (const stem of unchanged) afterInv.set(stem, inv(after, stem));

  const rows = [];
  const problems = { lost: 0, added: 0, dup: 0, cfgmismatch: 0, orphans: orphans.length, newtargets: addedTargets.length };
  const lostNames = [];
  const addedNames = [];

  for (const stem of moved) {
    const b = beforeInv.get(stem);
    const a = afterInv.get(stem);
    // the `mod` decl carries only the gating cfg(...) attrs, not inner
    // cfg_attr(...) allows — compare it against the file's cfg-only entries
    const cfgOnly = (f) => f.fileCfgs.filter((c) => c.startsWith('cfg('));
    const cfgShift = JSON.stringify(b.fileCfgs) !== JSON.stringify(a.fileCfgs) ||
      JSON.stringify(decls.get(stem)) !== JSON.stringify(cfgOnly(b));
    const diff = multisetDiff(multiset(b.tests), multiset(a.tests));
    const bPaths = b.tests.map((t) => t.path);
    const aPaths = a.tests.map((t) => t.path);
    const dup = bPaths.length !== new Set(bPaths).size || aPaths.length !== new Set(aPaths).size;
    let status = 'ok';
    if (diff.lost.length) { status = 'LOST'; problems.lost += diff.lost.length; for (const k of diff.lost) lostNames.push(`${stem}::${k.split('|')[0]}`); }
    if (diff.added.length) { status = status === 'ok' ? 'ADDED-FN' : status + '+ADDED-FN'; problems.added += diff.added.length; for (const k of diff.added) addedNames.push(`${stem}::${k.split('|')[0]}`); }
    if (dup) { status += '+DUP'; problems.dup++; }
    if (cfgShift) { status += '+CFG'; problems.cfgmismatch++; }
    rows.push({ stem, before: b.tests.length, after: a.tests.length, status, diff, cfgShift, dup });
  }

  for (const stem of unchanged) {
    const b = beforeInv.get(stem);
    const a = afterInv.get(stem);
    const diff = multisetDiff(multiset(b.tests), multiset(a.tests));
    if (diff.lost.length || diff.added.length) {
      rows.push({ stem, before: b.tests.length, after: a.tests.length, status: 'COLLATERAL-CHANGED', diff, cfgShift: false, dup: false });
      problems.lost += diff.lost.length;
      problems.added += diff.added.length;
      for (const k of diff.lost) lostNames.push(`${stem}::${k.split('|')[0]}`);
      for (const k of diff.added) addedNames.push(`${stem}::${k.split('|')[0]}`);
    }
  }

  const mergedInv = extractInventory(mergedSrc);
  const mergedDirect = mergedInv.tests.length + mergedInv.testCases.length;

  // totals (derived, then asserted)
  const B = [...beforeInv.values()].reduce((s, i) => s + i.tests.length, 0);
  const A = [...afterInv.values()].reduce((s, i) => s + i.tests.length, 0);
  const kept = multisetIntersection(
    multiset([...beforeInv.values()].flatMap((i) => i.tests)),
    multiset([...afterInv.values()].flatMap((i) => i.tests)),
  );
  const lost = problems.lost;
  const addedFns = problems.added;
  const assert = (cond, msg) => { if (!cond) throw new Error(`assert failed: ${msg}`); };
  // derived-not-typed: kept is the before∩after multiset intersection, so
  // kept+lost==B and kept+added==A hold by construction; additionally, in a
  // clean run (lost==0) the spec identity lost+kept+added==A holds too.
  assert(kept + lost === B, `kept(${kept}) + lost(${lost}) != tests before (${B})`);
  assert(kept + addedFns === A, `kept + added (${kept + addedFns}) != tests after (${A})`);
  if (lost === 0) assert(lost + kept + addedFns === A, 'spec identity lost+kept+added==A');
  // unchanged stems WITHOUT a printed row (COLLATERAL-CHANGED stems already
  // have one) contribute their counts on top of the printed rows
  const rowStems = new Set(rows.map((r) => r.stem));
  const unchangedTests = [...unchanged]
    .filter((stem) => !rowStems.has(stem))
    .reduce((s, stem) => s + beforeInv.get(stem).tests.length, 0);
  const rowSumBefore = rows.reduce((s, r) => s + r.before, 0) + unchangedTests;
  assert(rowSumBefore === B, `printed per-stem before counts (${rowSumBefore}) sum to the before total (${B})`);
  const rowSumAfter = rows.reduce((s, r) => s + r.after, 0) + unchangedTests;
  assert(rowSumAfter === A, `printed per-stem after counts (${rowSumAfter}) sum to the after total (${A})`);

  return {
    mt, moved, unchanged, addedTargets, orphans, decls, mergedDirFiles,
    beforeInv, afterInv, rows, problems, lostNames, addedNames,
    B, A, kept, mergedDirect, mergedInv,
    testCaseHits: [...beforeInv.values()].reduce((s, i) => s + i.testCases.length, 0),
  };
}

export function render(res) {
  const L = [];
  L.push(`# Test inventory gate — merged target \`${res.mt}\``);
  L.push('');
  L.push(`moved stems: ${res.moved.length} | unchanged stems: ${res.unchanged.length} | merged dir files: ${res.mergedDirFiles.length}`);
  L.push('');
  L.push('| stem | tests before | tests after | status |');
  L.push('|---|---|---|---|');
  for (const r of res.rows.sort((a, b) => a.stem.localeCompare(b.stem))) {
    L.push(`| ${r.stem} | ${r.before} | ${r.after} | ${r.status} |`);
  }
  L.push('');
  if (res.addedTargets.length) {
    L.push(`unexpected new flat targets: ${res.addedTargets.join(', ')}`);
    L.push('');
  }
  if (res.orphans.length) {
    L.push(`orphan merged modules: ${res.orphans.join(', ')}`);
    L.push('');
  }
  if (res.lostNames.length) {
    L.push(`lost tests: ${res.lostNames.join(', ')}`);
    L.push('');
  }
  if (res.addedNames.length) {
    L.push(`added tests: ${res.addedNames.join(', ')}`);
    L.push('');
  }
  L.push(`#[test_case] occurrences (documented approximation): ${res.testCaseHits}`);
  const p = res.problems;
  const ok = p.lost === 0 && p.added === 0 && p.dup === 0 && p.cfgmismatch === 0 &&
    p.orphans === 0 && p.newtargets === 0 && res.mergedDirect === 0;
  L.push(`inventory: ${res.B} tests before, ${res.A} tests after, ${p.lost} lost, ${p.added} added, ${p.dup} dup, ${p.cfgmismatch} cfg-mismatch, ${p.orphans} orphans, ${p.newtargets} unexpected new targets`);
  L.push(ok ? 'PASS' : 'FAIL');
  return L.join('\n');
}

export function renderCsv(res) {
  const L = ['stem,tests_before,tests_after,status'];
  for (const r of res.rows.sort((a, b) => a.stem.localeCompare(b.stem))) {
    L.push(`${r.stem},${r.before},${r.after},${r.status}`);
  }
  const p = res.problems;
  L.push(`SUMMARY,${res.B},${res.A},${p.lost} lost,${p.added} added,${p.dup} dup,${p.cfgmismatch} cfg-mismatch,${p.orphans} orphans,${p.newtargets} new-targets,${res.mergedDirect} merged-direct`);
  return L.join('\n') + '\n';
}

export function main(argv) {
  const opts = parseArgs(argv);
  if (opts.unknown || (opts.ref && opts.beforeDir)) {
    process.stderr.write('usage: node scripts/test-inventory.mjs [--ref <git-ref>] [--before-dir <dir>] [--after-dir <dir>] [--merged-target <name>] [--csv <out.csv>]\n');
    process.exit(2);
  }
  if (!opts.ref && !opts.beforeDir) {
    process.stderr.write('error: one of --ref or --before-dir is required\n');
    process.exit(2);
  }
  const res = runGate(opts);
  const out = render(res);
  process.stdout.write(out + '\n');
  if (opts.csv) {
    mkdirSync(resolve(opts.csv, '..'), { recursive: true });
    writeFileSync(opts.csv, renderCsv(res));
    process.stdout.write(`csv written: ${opts.csv}\n`);
  }
  process.exit(/PASS$/.test(out) ? 0 : 1);
}

if (process.argv[1] && process.argv[1].endsWith('test-inventory.mjs')) {
  main(process.argv.slice(2));
}
