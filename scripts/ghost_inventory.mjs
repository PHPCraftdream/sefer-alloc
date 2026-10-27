// Ghost-checks G0 inventory (docs/GHOST_CHECKS_PLAN.md §2, §8 G0): every atomic operation site in
// `src/**` and `crates/*/src/**`, with its enclosing item path, operation, orderings, receiver and
// cfg context, classified by docs/verification/ghost-rules.json `scopes`. A site no scope claims is
// a gap; `--check` is red on any gap, on a scope that claims nothing and on a site claimed twice.
// Text scanner, not a parser: comments and literals are blanked first, braces give the item path.
// G1's `crates/ghost-lint` (syn) replaces it as the authority; until then this is the G0 metric.
//
//   node scripts/ghost_inventory.mjs            # summary table
//   node scripts/ghost_inventory.mjs --check    # red on gaps / dead scopes / double claims
//   node scripts/ghost_inventory.mjs --csv FILE # per-site CSV (no line numbers: stable keys)
//   node scripts/ghost_inventory.mjs --gaps     # list unclaimed sites
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';
import { writeReport } from './ghost_inventory_report.mjs';
import { selftest } from './ghost_inventory_selftest.mjs';

const OPS = ['compare_exchange_weak', 'compare_exchange', 'fetch_update', 'fetch_add', 'fetch_sub', 'fetch_or',
  'fetch_and', 'fetch_xor', 'fetch_nand', 'fetch_max', 'fetch_min', 'swap', 'store', 'load'];
const ORDERINGS = ['SeqCst', 'AcqRel', 'Acquire', 'Release', 'Relaxed'];
const CATALOG_REL = 'docs/verification/ghost-rules.json';

// Blank comments, string/char literals with spaces (newlines kept), so offsets and lines survive.
export function blank(src) {
  const out = src.split('');
  const n = src.length;
  let i = 0;
  const wipe = (a, b) => { for (let k = a; k < b; k++) if (out[k] !== '\n') out[k] = ' '; };
  while (i < n) {
    const c = src[i], d = src[i + 1];
    if (c === '/' && d === '/') { const e = src.indexOf('\n', i); const end = e < 0 ? n : e; wipe(i, end); i = end; continue; }
    if (c === '/' && d === '*') {
      let depth = 1, k = i + 2;
      while (k < n && depth > 0) {
        if (src[k] === '/' && src[k + 1] === '*') { depth++; k += 2; } else if (src[k] === '*' && src[k + 1] === '/') { depth--; k += 2; } else k++;
      }
      wipe(i, k); i = k; continue;
    }
    const raw = /^b?r(#*)"/.exec(src.slice(i, i + 40));
    if (raw && (i === 0 || !/[A-Za-z0-9_]/.test(src[i - 1]))) {
      const close = '"' + raw[1];
      const e = src.indexOf(close, i + raw[0].length);
      const end = e < 0 ? n : e + close.length;
      wipe(i, end); i = end; continue;
    }
    if (c === '"' || (c === 'b' && d === '"' && (i === 0 || !/[A-Za-z0-9_]/.test(src[i - 1])))) {
      let k = c === 'b' ? i + 2 : i + 1;
      while (k < n && src[k] !== '"') k += src[k] === '\\' ? 2 : 1;
      wipe(i, k + 1); i = k + 1; continue;
    }
    if (c === '\'') {
      const m = /^'(\\(x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]+\}|.)|[^\\'\n])'/.exec(src.slice(i, i + 12));
      if (m) { wipe(i, i + m[0].length); i += m[0].length; continue; }
    }
    i++;
  }
  return out.join('');
}

// Matching close paren for the '(' at `open`; -1 if unbalanced.
function closeParen(text, open) {
  let depth = 0;
  for (let k = open; k < text.length; k++) {
    if (text[k] === '(') depth++;
    else if (text[k] === ')') { depth--; if (depth === 0) return k; }
  }
  return -1;
}

// Item path at every '{': a stack of { kind, name, cfg } opened by `mod`/`impl`/`trait`/`fn`/`macro_rules!`.
// The header runs to the first '{' or ';' outside ()[]<> (so `-> [u64; 4]` does not end it); cfg text
// comes from the original source at the same offsets (literals are blanked in `text`).
function scopes(text, src) {
  const opens = [];
  const re = /\b(mod|impl|trait|fn|macro_rules!)(?![A-Za-z0-9_])/g;
  let m;
  while ((m = re.exec(text))) {
    if (m.index > 0 && /[A-Za-z0-9_]/.test(text[m.index - 1])) continue;
    let k = m.index + m[0].length, depth = 0;
    for (; k < text.length; k++) {
      const ch = text[k];
      if (ch === '(' || ch === '[') depth++;
      else if (ch === ')' || ch === ']') depth--;
      else if (ch === '<' && m[1] !== 'fn') depth++;
      else if (ch === '>' && m[1] !== 'fn' && text[k - 1] !== '-' && text[k - 1] !== '=') depth--;
      else if (depth <= 0 && (ch === '{' || ch === ';')) break;
    }
    if (text[k] !== '{') continue;
    const kind = m[1];
    let name = text.slice(m.index + m[0].length, k).trim();
    if (kind !== 'impl' && kind !== 'trait') name = (/^([A-Za-z_][A-Za-z0-9_]*)/.exec(name) || [, '?'])[1];
    else name = name.replace(/\s+/g, ' ').replace(/\bwhere\b.*$/, '').trim();
    const before = src.slice(Math.max(0, m.index - 400), m.index);
    const attr = /#\[cfg\(((?:[^\[\]]|\[[^\]]*\])*)\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?(?:(?:const|unsafe|async|default|extern\s+"C")\s+)*$/.exec(before);
    opens.push({ at: k, kind, name, cfg: attr ? attr[1].replace(/\s+/g, ' ').trim() : '' });
  }
  return opens.sort((a, b) => a.at - b.at);
}

// Enclosing item stack for an offset: replay braces, keeping the scope entries whose '{' is open.
function pathAt(text, opens, offset, memo) {
  const stack = [];
  let oi = 0;
  for (let k = 0; k < offset; k++) {
    const ch = text[k];
    if (ch === '{') {
      if (oi < opens.length && opens[oi].at === k) { stack.push(opens[oi]); oi++; } else stack.push(null);
    } else if (ch === '}') stack.pop();
    while (oi < opens.length && opens[oi].at < k) oi++;
  }
  void memo;
  return stack.filter(Boolean);
}

// One site per call that carries a memory ordering: an atomic method from OPS (orderings literal or
// passed through a parameter), a `fence`/`compiler_fence`, or any other call whose top-level
// arguments name an ordering — a wrapper such as `HeapSlot::cas_state(.., Ordering::Release, ..)`,
// whose call sites are where the protocol's orderings actually live.
export function scanFile(rel, src) {
  const text = blank(src);
  const opens = scopes(text, src);
  const sites = [];
  const re = /(\.|::|\b)([a-z_][a-z0-9_]*)\s*(::<[^>]*>)?\s*\(/g;
  const ordRe = new RegExp(`\\b(${ORDERINGS.join('|')})\\b`, 'g');
  let m;
  while ((m = re.exec(text))) {
    const name = m[2];
    const head = m.index + m[1].length;
    if (/\bfn\s+$/.test(text.slice(Math.max(0, head - 8), head))) continue;
    if (['if', 'while', 'match', 'for', 'return', 'loop', 'in', 'as', 'move'].includes(name)) continue;
    const open = text.indexOf('(', head + name.length);
    const close = closeParen(text, open);
    if (close < 0) continue;
    const args = text.slice(open + 1, close);
    let top = '', depth = 0;
    for (const ch of args) { if (ch === '(' || ch === '[' || ch === '{') depth++; else if (ch === ')' || ch === ']' || ch === '}') depth--; else if (depth === 0) top += ch; }
    const ords = [...top.matchAll(ordRe)].map((x) => x[1]);
    const isOp = m[1] === '.' && OPS.includes(name);
    const isFence = name === 'fence' || name === 'compiler_fence';
    const orderingParam = isOp && /\b(order|ordering|ord|success|failure|set_order|fetch_order)\b/.test(top);
    if (ords.length === 0 && !orderingParam) continue;
    if (!isOp && !isFence && ords.length === 0) continue;
    let r = head;
    if (m[1] !== '') {
      r = m.index;
      while (r > 0 && /\s/.test(text[r - 1])) r--;
    }
    let st = r;
    depth = 0;
    if (m[1] !== '') {
      while (st > 0) {
        const ch = text[st - 1];
        if (ch === ')' || ch === ']') depth++;
        else if (ch === '(' || ch === '[') { if (depth === 0) break; depth--; }
        else if (depth === 0 && !/[A-Za-z0-9_.:&*]/.test(ch)) break;
        st--;
      }
    }
    const receiver = text.slice(st, r).replace(/\s+/g, '').replace(/^[&*]+/, '');
    const stack = pathAt(text, opens, m.index);
    const line = text.slice(0, m.index).split('\n').length;
    sites.push({
      file: rel, line, op: name, kind: isOp ? 'atomic' : isFence ? 'fence' : 'wrapper',
      orderings: ords.length ? ords.join('/') : 'param', receiver,
      item: stack.map((x) => (x.kind === 'fn' ? x.name : x.kind === 'mod' ? `mod ${x.name}` : `${x.kind} ${x.name}`)).join('::'),
      fn: (stack.filter((x) => x.kind === 'fn').pop() || { name: '' }).name,
      cfg: stack.map((x) => x.cfg).filter(Boolean).join(' & '),
    });
    re.lastIndex = open + 1;
  }
  return sites;
}

function walk(dir, out) {
  for (const e of readdirSync(join(REPO_ROOT, dir))) {
    const rel = `${dir}/${e}`;
    if (statSync(join(REPO_ROOT, rel)).isDirectory()) walk(rel, out);
    else if (e.endsWith('.rs')) out.push(rel);
  }
  return out;
}

export function sourceFiles() {
  const files = walk('src', []);
  for (const c of readdirSync(join(REPO_ROOT, 'crates'))) {
    try { if (statSync(join(REPO_ROOT, 'crates', c, 'src')).isDirectory()) walk(`crates/${c}/src`, files); } catch { /* no src */ }
  }
  return files.sort();
}

const globRe = (g) => new RegExp('^' + g.replace(/[.+^$()|[\]\\]/g, '\\$&').replace(/\*\*/g, '\u0000').replace(/\*/g, '[^/]*').replace(/\u0000/g, '.*') + '$');

// A scope claims a site when every present selector matches: file glob, item/fn/receiver regex, op, ordering.
export function claims(scope, site) {
  if (!scope.files.some((g) => globRe(g).test(site.file))) return false;
  if (scope.exclude && scope.exclude.some((g) => globRe(g).test(site.file))) return false;
  if (scope.fn && !new RegExp(scope.fn).test(site.fn)) return false;
  if (scope.fn_not && new RegExp(scope.fn_not).test(site.fn)) return false;
  if (scope.receiver_not && new RegExp(scope.receiver_not).test(site.receiver)) return false;
  if (scope.kind && scope.kind !== site.kind) return false;
  if (scope.item && !new RegExp(scope.item).test(site.item)) return false;
  if (scope.receiver && !new RegExp(scope.receiver).test(site.receiver)) return false;
  if (scope.ops && !scope.ops.includes(site.op)) return false;
  if (scope.orderings && !site.orderings.split('/').every((o) => scope.orderings.includes(o))) return false;
  if (scope.cfg && !new RegExp(scope.cfg).test(site.cfg)) return false;
  return true;
}

export function classify(catalog, sites) {
  const perScope = new Map(catalog.scopes.map((s) => [s.id, 0]));
  const gaps = [], doubles = [];
  for (const site of sites) {
    const inTestFn = catalog.test_fn && new RegExp(catalog.test_fn).test(site.fn);
    const hit = catalog.scopes.filter((s) => (inTestFn ? s.id === catalog.test_fn_scope : true) && claims(s, site));
    if (hit.length === 0) gaps.push(site);
    else if (hit.length > 1) doubles.push({ site, ids: hit.map((h) => h.id) });
    else { site.scope = hit[0].id; site.class = hit[0].class; site.protocol = hit[0].protocol || ''; perScope.set(hit[0].id, perScope.get(hit[0].id) + 1); }
  }
  const dead = [...perScope].filter(([, n]) => n === 0).map(([id]) => id);
  return { perScope, gaps, doubles, dead };
}

// Protocol-level G0 checks: every prod path resolves (file exists; `::fn` names a fn in it), every
// cited loom test / pin exists, every registry row exists, every S-class protocol has a decision, every
// scope's protocol is declared, and every protocol is either claimed by a scope or says why it has no
// atomic sites of its own (`atomics` field).
export function validateProtocols(catalog, perScope, readRel, existsRel, registryIds) {
  const errors = [];
  const ids = new Set(catalog.protocols.map((p) => p.id));
  if (ids.size !== catalog.protocols.length) errors.push('duplicate protocol id');
  const sitesOf = new Map([...ids].map((id) => [id, 0]));
  for (const sc of catalog.scopes) {
    if (sc.protocol && !ids.has(sc.protocol)) errors.push(`scope ${sc.id}: unknown protocol ${sc.protocol}`);
    if (sc.protocol) sitesOf.set(sc.protocol, sitesOf.get(sc.protocol) + perScope.get(sc.id));
    if (!sc.reason) errors.push(`scope ${sc.id}: no reason`);
  }
  let paths = 0;
  for (const p of catalog.protocols) {
    for (const ref of p.prod) {
      const [file, fn] = ref.split('::');
      paths++;
      if (!existsRel(file)) { errors.push(`${p.id}: missing file ${file}`); continue; }
      if (fn && !new RegExp(`\\bfn\\s+${fn}\\b`).test(readRel(file))) errors.push(`${p.id}: no fn ${fn} in ${file}`);
    }
    for (const t of [...p.loom, ...p.pins]) if (!existsRel(t)) errors.push(`${p.id}: missing test ${t}`);
    for (const r of p.registry) if (!registryIds.has(r)) errors.push(`${p.id}: registry row ${r} not in docs/evidence/registry.csv`);
    if (!p.decision || !p.reason) errors.push(`${p.id}: decision and reason required`);
    if (p.class !== 'none' && p.invariants.length === 0) errors.push(`${p.id}: no invariant`);
    if (p.decision !== 'archive' && sitesOf.get(p.id) === 0 && !p.atomics) errors.push(`${p.id}: no scope claims a site and no atomics note`);
  }
  return { errors, sitesOf, paths };
}

function main() {
  const argv = process.argv.slice(2);
  if (argv.includes('--selftest') || argv.includes('--check')) console.log('[ghost-inventory] self-test OK (' + selftest({ scanFile, classify }) + ' cases)');
  const catalog = JSON.parse(readFileSync(join(REPO_ROOT, CATALOG_REL), 'utf8'));
  const ids = catalog.scopes.map((s) => s.id);
  assert.equal(new Set(ids).size, ids.length, 'duplicate scope id in catalog');
  const files = sourceFiles();
  const sites = files.flatMap((f) => scanFile(f, readFileSync(join(REPO_ROOT, f), 'utf8')));
  const { perScope, gaps, doubles, dead } = classify(catalog, sites);
  const byClass = {};
  for (const s of sites) if (s.class) byClass[s.class] = (byClass[s.class] || 0) + 1;
  const claimed = sites.length - gaps.length - doubles.length;
  assert.equal(Object.values(byClass).reduce((a, b) => a + b, 0), claimed, 'class totals must sum to claimed sites');
  const registryIds = new Set(readFileSync(join(REPO_ROOT, 'docs/evidence/registry.csv'), 'utf8').split('\n').slice(1).map((l) => l.split(',')[0]).filter(Boolean));
  const existsRel = (f) => { try { return statSync(join(REPO_ROOT, f)).isFile(); } catch { return false; } };
  const readRel = (f) => readFileSync(join(REPO_ROOT, f), 'utf8');
  const pv = validateProtocols(catalog, perScope, readRel, existsRel, registryIds);
  const nFiles = new Set(sites.map((s) => s.file)).size;
  console.log(`# ghost inventory: ${sites.length} ordering sites in ${nFiles} files; ${catalog.protocols.length} protocols; ${ids.length} scopes`);
  console.log('| class | sites |\n|---|---:|');
  for (const [k, v] of Object.entries(byClass).sort()) console.log(`| ${k} | ${v} |`);
  console.log(`| **claimed** | ${claimed} |
| unclaimed (gaps) | ${gaps.length} |
| claimed twice | ${doubles.length} |`);
  console.log(`dead scopes: ${dead.length}; protocol errors: ${pv.errors.length} (prod paths checked: ${pv.paths})`);
  if (argv.includes('--scopes')) for (const [id, n] of perScope) console.log(`${String(n).padStart(4)} ${id}`);
  if (argv.includes('--gaps')) for (const g of gaps) console.log(`GAP ${g.file}:${g.line} ${g.item} ${g.receiver}.${g.op}(${g.orderings}) cfg=${g.cfg}`);
  for (const d of doubles) console.log(`DOUBLE ${d.site.file}:${d.site.line} ${d.site.receiver}.${d.site.op} → ${d.ids.join(', ')}`);
  for (const id of dead) console.log(`DEAD ${id}`);
  for (const e of pv.errors) console.log(`PROTOCOL ${e}`);
  const esc = (v) => (/[",\n]/.test(String(v)) ? `"${String(v).replace(/"/g, '""')}"` : String(v));
  const ci = argv.indexOf('--csv');
  if (ci >= 0) {
    const rows = sites.map((x) => [x.file, x.item, x.receiver, x.op, x.orderings, x.cfg, x.class || 'GAP', x.protocol || '', x.scope || ''].map(esc).join(','));
    writeFileSync(argv[ci + 1], ['file,item,receiver,op,orderings,cfg,class,protocol,scope', ...rows].join('\n') + '\n');
  }
  const ri = argv.indexOf('--report');
  if (ri >= 0) writeReport(argv[ri + 1], { catalog, sites, byClass, claimed, gaps, doubles, dead, pv, nFiles, perScope });
  if (argv.includes('--check') && (gaps.length || doubles.length || dead.length || pv.errors.length)) {
    console.error(`[ghost-inventory] RED: ${gaps.length} gap(s), ${doubles.length} double claim(s), ${dead.length} dead scope(s), ${pv.errors.length} protocol error(s)`);
    process.exit(1);
  }
}

if (import.meta.url === `file://${process.argv[1].replace(/\\/g, '/').replace(/^\/?/, '/')}` || process.argv[1].endsWith('ghost_inventory.mjs')) main();
