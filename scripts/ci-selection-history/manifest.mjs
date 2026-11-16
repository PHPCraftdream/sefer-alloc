// Cargo-free TOML-subset parsing for the P1.5 selection-potential model.
// Only the table shapes actually present in this workspace's manifests are
// supported: [section] headers, `key = value` where value is a quoted string,
// a (possibly multiline) array of strings/inline tables, or an inline table.
// Comments (`#`) are stripped outside quotes. No external dependencies.

function stripComment(line) {
  let inS = false, inD = false, out = '';
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === "'" && !inD) inS = !inS;
    else if (c === '"' && !inS) inD = !inD;
    else if (c === '#' && !inS && !inD) break;
    out += c;
  }
  return out;
}

function balanced(s) {
  let inS = false, inD = false, depth = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "'" && !inD) inS = !inS;
    else if (c === '"' && !inS) inD = !inD;
    else if (!inS && !inD) {
      if (c === '[' || c === '{') depth++;
      if (c === ']' || c === '}') depth--;
    }
  }
  return depth === 0 && !inS && !inD;
}

function splitTop(s, sep) {
  const parts = [];
  let inS = false, inD = false, depth = 0, cur = '';
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "'" && !inD) { inS = !inS; cur += c; continue; }
    if (c === '"' && !inS) { inD = !inD; cur += c; continue; }
    if (!inS && !inD) {
      if (c === '[' || c === '{') depth++;
      if (c === ']' || c === '}') depth--;
      if (c === sep && depth === 0) { parts.push(cur); cur = ''; continue; }
    }
    cur += c;
  }
  if (cur.trim()) parts.push(cur);
  return parts;
}

function unquote(s) {
  const t = s.trim();
  if ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'"))) return t.slice(1, -1);
  return t;
}

/** value -> string | string[] | {k: v} */
export function parseValue(v) {
  const t = v.trim();
  if (t.startsWith('{')) {
    const out = {};
    for (const part of splitTop(t.slice(1, -1), ',')) {
      const m = /^\s*([A-Za-z0-9_-]+)\s*=\s*([\s\S]*)$/.exec(part);
      if (m) out[m[1]] = parseValue(m[2]);
    }
    return out;
  }
  if (t.startsWith('[')) return splitTop(t.slice(1, -1), ',').map(unquote).filter(s => s.length);
  if (t === 'true') return true;
  if (t === 'false') return false;
  return unquote(t);
}

/** text -> Map<header, Array<{key, value}>>; header kept verbatim between the brackets. */
export function parseTables(text) {
  const tables = new Map();
  let cur = null, buf = null, bufKey = null;
  const finish = () => {
    if (bufKey && cur) cur.push({ key: bufKey, value: parseValue(buf) });
    buf = null; bufKey = null;
  };
  for (const raw of text.split(/\r?\n/)) {
    if (buf !== null) {
      buf += stripComment(raw) + '\n';
      if (balanced(buf)) finish();
      continue;
    }
    const h = /^\s*\[([^\][{]+)\]\s*$/.exec(stripComment(raw));
    if (h) { finish(); cur = []; tables.set(h[1].trim(), cur); continue; }
    if (/^\s*\[\[/.test(raw)) { finish(); cur = null; continue; } // [[array-of-table]]: not needed
    if (!cur) continue;
    const kv = /^\s*([A-Za-z0-9_-]+)\s*=\s*([\s\S]*)$/.exec(stripComment(raw));
    if (kv) {
      if (balanced(kv[2])) cur.push({ key: kv[1], value: parseValue(kv[2]) });
      else { buf = kv[2] + '\n'; bufKey = kv[1]; }
    }
  }
  finish();
  return tables;
}

const get = (tables, name) => tables.get(name) ?? [];
const depEntries = (rows) => {
  const out = new Map();
  for (const { key, value } of rows) {
    if (typeof value === 'string') out.set(key, { path: null, optional: false });
    else if (value && typeof value === 'object') out.set(key, { path: typeof value.path === 'string' ? value.path : null, optional: value.optional === true });
  }
  return out;
};

/** Root (workspace + package) manifest. */
export function parseRootManifest(text) {
  const tables = parseTables(text);
  const members = (get(tables, 'workspace').find(r => r.key === 'members')?.value ?? []);
  const packageName = get(tables, 'package').find(r => r.key === 'name')?.value ?? null;
  const features = new Map();
  for (const { key, value } of get(tables, 'features')) features.set(key, Array.isArray(value) ? value : [value]);
  const deps = depEntries(get(tables, 'dependencies'));
  const devDeps = depEntries(get(tables, 'dev-dependencies'));
  const cfgDeps = new Map();
  for (const [header, rows] of tables) {
    if (!header.endsWith('.dependencies') || !header.includes('cfg(')) continue;
    if (/loom|kani/.test(header)) for (const [k] of depEntries(rows)) cfgDeps.set(k, header);
  }
  return { packageName, members: members.filter(m => typeof m === 'string'), features, deps, devDeps, cfgDeps };
}

/** A workspace member manifest: package name + its [dependencies] edges. */
export function parseMemberManifest(text) {
  const tables = parseTables(text);
  const name = get(tables, 'package').find(r => r.key === 'name')?.value ?? null;
  const deps = depEntries(get(tables, 'dependencies'));
  return { name, deps };
}

/**
 * feature-token -> set of dependency crates enabled, computed FROM the
 * [features] table: `dep:x` enables x; `x/feat` enables x (when x is a
 * declared dependency); `x?/feat` does NOT itself enable x; plain feature
 * names recurse.
 */
export function closureDeps(tokens, features, depNames) {
  const deps = new Set(), seen = new Set();
  const queue = [...tokens];
  while (queue.length) {
    const tok = queue.shift();
    if (!tok || seen.has(tok)) continue;
    seen.add(tok);
    if (tok.startsWith('dep:')) { deps.add(tok.slice(4)); continue; }
    const slash = tok.indexOf('/');
    if (slash > 0) {
      const base = tok.slice(0, slash);
      if (base.endsWith('?')) continue; // weak forwarding: does not enable the dep
      if (depNames.has(base)) deps.add(base);
      else queue.push(base);
      continue;
    }
    for (const e of features.get(tok) ?? []) queue.push(e);
  }
  return deps;
}

/** All features whose closure enables `dep` (feature name -> dep edge table). */
export function featuresEnabling(dep, features, depNames) {
  const out = new Set();
  for (const f of features.keys()) {
    if (closureDeps([f], features, depNames).has(dep)) out.add(f);
  }
  return out;
}
