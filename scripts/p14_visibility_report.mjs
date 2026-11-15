#!/usr/bin/env node
// P1.4 visibility/extraction report generator (cargo-free; derived, not typed).
// Compares the moved sources at `origin/main` (git show) with the current tree
// and prints markdown tables:
//   (a) visibility widening (pub(crate)/private -> pub) per moved symbol, with
//       the R25-1 class flag (safe fn taking a raw-pointer parameter),
//   (b) moved / stayed symbol counts, cross-checked against the symbol lists of
//       scripts/p14_extraction_inventory.mjs (parsed from its source text; the
//       inventory script itself is NOT imported: it has top-level side effects
//       and its asserts describe the pre-move tree),
//   (c) cfg gates of moved symbols (asserted identical origin -> new),
//   (d) #[inline]/#[inline(always)]/#[cold] before/after (exactly one addition:
//       `#[inline]` on segment_base_of),
//   (e) feature forwarding table root -> sefer-platform.
//
// Usage:
//   node scripts/p14_visibility_report.mjs          # markdown to stdout
//   node scripts/p14_visibility_report.mjs --write  # rewrite the section between
//     <!-- BEGIN generated --> / <!-- END generated --> in
//     docs/perf/P1_4_SEFER_PLATFORM_EXTRACTION.md
// Exit 1 on any failed assertion.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const ROOT = join(import.meta.dirname, '..');
const CRATE = 'crates/platform/sefer-platform';
const REPORT = 'docs/perf/P1_4_SEFER_PLATFORM_EXTRACTION.md';
const BEGIN = '<!-- BEGIN generated -->';
const END = '<!-- END generated -->';

const readWork = (p) => readFileSync(join(ROOT, p), 'utf8').replace(/\r\n/g, '\n');
const readOrigin = (p) =>
  execFileSync('git', ['show', `origin/main:${p}`], { cwd: ROOT, encoding: 'utf8', maxBuffer: 1 << 26 }).replace(/\r\n/g, '\n');
const esc = (s) => String(s).replace(/\|/g, '\\|');
const norm = (s) => s.replace(/\s+/g, ' ').trim();

// moved source (origin path) -> new file in the crate
const MOVES = [
  { from: 'src/alloc_core/platform/os.rs', to: `${CRATE}/src/os.rs` },
  { from: 'src/alloc_core/platform/fatal.rs', to: `${CRATE}/src/fatal.rs` },
  { from: 'src/global/process_exit.rs', to: `${CRATE}/src/process_exit.rs` },
];

// ── item scanner ──
function attrSpans(ls) {
  const spans = [];
  for (let i = 0; i < ls.length; i++) {
    if (!/^\s*#!?\[/.test(ls[i])) continue;
    let depth = 0, j = i, text = '';
    for (; j < ls.length; j++) {
      text += ls[j].trim() + ' ';
      for (const ch of ls[j]) { if (ch === '[') depth++; else if (ch === ']') depth--; }
      if (depth <= 0) break;
    }
    spans.push({ start: i, end: j, text: norm(text) });
    i = j;
  }
  return spans;
}

const ITEM_RE = /^(\s*)(pub(?:\([^)]*\))?\s+)?((?:(?:const|unsafe|async)\s+)*)(fn|static|struct|const)\s+(?:mut\s+)?([A-Za-z_]\w*)/;

function scan(src) {
  const ls = src.split('\n');
  const spans = attrSpans(ls);
  const endAt = new Map(spans.map((s) => [s.end, s]));
  const items = [];
  let ctx = null;
  for (let i = 0; i < ls.length; i++) {
    const l = ls[i];
    if (/^\S.*\{\s*$/.test(l)) ctx = l.trim();
    else if (/^\}/.test(l)) ctx = null;
    const m = ITEM_RE.exec(l);
    if (!m || m[5] === '_') continue;
    const indent = m[1].length;
    if (indent > 4 || (indent === 4 && !(ctx && ctx.startsWith('impl ')))) continue;
    if (indent === 0 && /^\s*(const|static)\s/.test(l) === false && !/^(pub|fn|unsafe|const|static|struct)/.test(l)) continue;
    const owner = indent === 4 ? ctx.replace(/^impl\s+(?:<[^>]*>\s*)?(?:\S+\s+for\s+)?(\w+).*$/, '$1') : null;
    const vis = m[2] ? m[2].trim() : 'private';
    const mods = m[3];
    // attributes and docs above
    const attrs = []; const docs = [];
    for (let j = i - 1; j >= 0;) {
      if (endAt.has(j)) { const s = endAt.get(j); attrs.unshift(s.text); j = s.start - 1; }
      else if (/^\s*\/\/\//.test(ls[j])) { docs.unshift(ls[j].trim()); j--; }
      else if (/^\s*\/\/(?!\/)/.test(ls[j])) j--; // plain `//` comment between docs and item
      else break;
    }
    // signature (fn only)
    let sig = '';
    if (m[4] === 'fn') {
      for (let j = i; j < ls.length; j++) { sig += ls[j].trim() + ' '; if (/[{;]\s*$/.test(ls[j])) break; }
    }
    // body (fn only): text from the signature's opening brace to its match
    let body = '';
    if (m[4] === 'fn') {
      let depth = 0, started = false;
      scanBody: for (let j = i; j < ls.length; j++) {
        for (const ch of ls[j]) {
          if (ch === '{') { depth++; started = true; }
          else if (ch === '}') depth--;
          if (started) body += ch;
          if (started && depth === 0) break scanBody;
        }
        if (started) body += '\n';
      }
    }
    items.push({
      body, key: owner ? `${owner}::${m[5]}` : m[5], name: m[5], owner, kind: m[4], vis,
      isUnsafe: /\bunsafe\b/.test(mods), isConst: /\bconst\b/.test(mods), attrs, docs: docs.join('\n'), sig, line: i + 1,
    });
  }
  return items;
}

function params(sig) {
  const a = sig.indexOf('(');
  if (a < 0) return '';
  let d = 0;
  for (let k = a; k < sig.length; k++) {
    if (sig[k] === '(') d++;
    else if (sig[k] === ')' && --d === 0) return sig.slice(a + 1, k);
  }
  return '';
}

const isPub = (it) => it.vis === 'pub';
const isVisible = (it) => it.vis !== 'private'; // pub / pub(crate): the inventory's "symbols"
const gateOf = (it) => it.attrs.filter((a) => /^#\[cfg(_attr)?\(/.test(a)).join(' ');
const perfOf = (it) => it.attrs.filter((a) => /^#\[(inline|cold)/.test(a));
const byKey = (items) => new Map(items.map((i) => [i.key, i]));

// ── load ──
const files = MOVES.map((mv) => ({ ...mv, old: scan(readOrigin(mv.from)), now: scan(readWork(mv.to)) }));
const rootOs = scan(readWork('src/alloc_core/platform/os.rs'));
const osFile = files[0];

// inventory lists (parsed from source text)
const invSrc = readWork('scripts/p14_extraction_inventory.mjs');
const quoted = (decl) => {
  const m = new RegExp(`const ${decl} = \\[([\\s\\S]*?)\\]`).exec(invSrc);
  assert(m, `inventory: ${decl} not found`);
  return [...m[1].matchAll(/'([^']+)'/g)].map((x) => x[1]);
};
const OS_STAY = quoted('OS_STAY');
const OS_MOVE = new Set([...quoted('OS_MOVE_CONSTRUCTORS'), ...[...invSrc.matchAll(/const OS_MOVE = \[([\s\S]*?)\]/g)].flatMap((m) => [...m[1].matchAll(/'([^']+)'/g)].map((x) => x[1]))]);
const invFiles = [...invSrc.matchAll(/\{ path: '([^']+)', decision: '(\w+)', reason: '((?:[^'\\]|\\.)*)' \}/g)]
  .map((m) => ({ path: m[1], decision: m[2], reason: m[3].replace(/\\'/g, "'") }));
assert(invFiles.length === 8, `inventory FILES expected 8, got ${invFiles.length}`);

// ── relations: pure move ──
const out = [];
const emit = (s = '') => out.push(s);

// P1.4 / R25-1: a moved safe fn taking a raw-pointer parameter is the one
// permitted change: it becomes `pub unsafe fn` (with `# Safety`), and the root
// keeps a real pub(crate) SAFE wrapper with the pre-extraction signature.
// Address-arithmetic-only fns (no deref/read/write/unsafe in the body) stay safe.
const hasRawPtrParam = (it) => /\*(mut|const)\b/.test(params(it.sig));
const DEREF_RE = /\bunsafe\b|\.(?:read|write|read_volatile|write_volatile|as_ref|as_mut|offset_from|copy_\w+)\s*\(|(?:^|[^\w)\]\s])\s*\*\s*[\w(]|(?:^|[=(,;{]\s*)\*\s*[\w(]/;
const ARITH_ONLY_ALLOWLIST = ['segment_base_of_ptr']; // pure `map_addr` rounding, provenance-preserving
const flipped = new Set();
for (const f of files) {
  const o = byKey(f.old), n = byKey(f.now);
  for (const [k, it] of o) {
    const nw = n.get(k);
    if (nw && it.kind === 'fn' && !it.isUnsafe && nw.isUnsafe) flipped.add(k);
  }
}
const stripUnsafe = (x) => x.replace(/\bunsafe\s+/, '');
for (const f of files) {
  const o = byKey(f.old), n = byKey(f.now);
  const isOs = f === osFile;
  for (const [k, it] of o) {
    if (isOs && OS_STAY.includes(it.name)) {
      assert(!n.has(k), `${k} must NOT move (directory trio)`);
      assert(rootOs.some((r) => r.name === it.name), `${k} must remain in root os.rs`);
      continue;
    }
    assert(n.has(k), `${f.from}: ${k} missing in ${f.to}`);
    assert.equal(n.get(k).kind, it.kind, `${k}: kind changed`);
    if (!flipped.has(k)) assert.equal(n.get(k).isUnsafe, it.isUnsafe, `${k}: unsafe-ness changed`);
    assert.equal(n.get(k).isConst, it.isConst, `${k}: const-ness changed`);
    // rustfmt-insensitive: drop whitespace around punctuation and a trailing comma before `)`.
    const bare = (x) => norm(x.sig).replace(/^pub(\([^)]*\))?\s+/, "").replace(/,\s*\)/g, ")").replace(/\(\s+/g, "(").replace(/\s+\)/g, ")");
    assert.equal(flipped.has(k) ? stripUnsafe(bare(n.get(k))) : bare(n.get(k)), bare(it), `${k}: signature changed`);
    assert.equal(norm(gateOf(n.get(k))), norm(gateOf(it)), `${k}: cfg gate changed`);
  }
  for (const k of n.keys()) assert(o.has(k), `${f.to}: ${k} is new (extraction must be a pure move)`);
}
// every flipped fn: raw-ptr param, `# Safety` doc, pub unsafe in the crate, and a
// real root pub(crate) safe wrapper with the origin signature, gate and attrs
// whose body forwards to sefer_platform::os::<name>.
const rootByName = new Map(rootOs.map((r) => [r.name, r]));
for (const k of flipped) {
  const oldIt = osFile.old.find((i) => i.key === k), nowIt = osFile.now.find((i) => i.key === k);
  assert(oldIt && nowIt, `${k}: flipped fn must come from os.rs`);
  assert(isPub(nowIt) && nowIt.isUnsafe, `${k}: must be pub unsafe fn`);
  assert(hasRawPtrParam(nowIt), `${k}: flipped but takes no raw pointer`);
  assert(/^\s*\/\/\/ # Safety\s*$/m.test(nowIt.docs), `${k}: missing # Safety doc`);
  assert(!nowIt.attrs.some((a) => a.includes('not_unsafe_ptr_arg_deref')), `${k}: stale allow(not_unsafe_ptr_arg_deref)`);
  const w = rootByName.get(nowIt.name);
  assert(w, `${k}: root wrapper missing`);
  assert.equal(w.vis, 'pub(crate)', `${k}: wrapper visibility`);
  assert(!w.isUnsafe, `${k}: wrapper must be safe`);
  const bare = (x) => norm(x.sig).replace(/^pub(\([^)]*\))?\s+/, '');
  // origin spells the error type via the module alias `vmem` (= `aligned_vmem`); same type
  const canon = (x) => bare(x).replace(/\baligned_vmem::/g, 'vmem::');
  assert.equal(canon(w), canon(oldIt), `${k}: wrapper signature differs from origin`);
  assert.equal(norm(gateOf(w)), norm(gateOf(oldIt)), `${k}: wrapper cfg gate differs from origin`);
  assert.deepEqual(w.attrs, oldIt.attrs, `${k}: wrapper attrs differ from origin`);
  assert(norm(w.body).includes(`unsafe { sefer_platform::os::${w.name}(`), `${k}: wrapper must forward to sefer_platform::os`);
}
const WRAPPED = [...flipped].map((k) => k.split('::').pop());
for (const r of rootOs) if (r.vis !== 'private') assert(OS_STAY.includes(r.name) || WRAPPED.includes(r.name), `root os.rs keeps unexpected symbol ${r.name}`);
assert.equal(rootOs.filter((r) => r.vis !== 'private').length, OS_STAY.length + WRAPPED.length, 'root os.rs keeps exactly the directory trio plus the safe wrappers');

// moved os.rs visible symbols == inventory OS_MOVE (minus names that are private at origin)
const osMovedNames = new Set(osFile.old.filter((i) => isVisible(i) && !OS_STAY.includes(i.name)).map((i) => i.name));
for (const nm of osMovedNames) assert(OS_MOVE.has(nm), `inventory OS_MOVE lacks ${nm}`);
for (const nm of OS_MOVE) assert(osMovedNames.has(nm) || osFile.old.some((i) => i.name === nm), `inventory OS_MOVE names ${nm} absent at origin`);

// ── (a) widening ──
const widened = [];
for (const f of files) {
  const o = byKey(f.old);
  for (const it of f.now) {
    const prev = o.get(it.key);
    if (isPub(it) && prev && !isPub(prev)) widened.push({ file: f, now: it, prev });
  }
}
for (const w of widened) assert(w.prev.vis === 'pub(crate)' || w.prev.vis === 'private', `${w.now.key}: unexpected old visibility ${w.prev.vis}`);
// everything visible at origin is pub now (no pub(crate) left in the crate)
for (const f of files) for (const it of f.now) assert(it.vis !== 'pub(crate)', `${f.to}: ${it.key} still pub(crate)`);

const classify = (it) => {
  if (it.kind !== 'fn') return it.kind === 'struct' ? 'type' : it.kind;
  if (it.isUnsafe) return 'unsafe fn';
  if (!hasRawPtrParam(it)) return 'safe fn';
  return DEREF_RE.test(it.body) ? 'safe fn + raw ptr param (R25-1 class)' : 'safe, address arithmetic only';
};
const safetyNote = (it) => {
  const c = classify(it);
  if (c === 'unsafe fn') return it.docs.includes('# Safety') ? 'unsafe fn, `# Safety` doc present' : 'unsafe fn, NO `# Safety` doc';
  if (c === 'safe, address arithmetic only') return 'takes raw ptr: yes; no deref/read/write in the body (derived from the source); does not dereference';
  if (!c.includes('raw ptr')) return '';
  const allow = it.attrs.some((a) => a.includes('not_unsafe_ptr_arg_deref'));
  const doc = it.docs.includes('# Safety');
  return `was already pub: no; takes raw ptr: yes; \`# Safety\`: ${doc ? 'yes' : 'no'}; allow(clippy::not_unsafe_ptr_arg_deref): ${allow ? 'yes' : 'no'}`;
};

emit('### (a) Visibility widening (origin/main `pub(crate)`/private -> `pub` in sefer-platform)');
emit();
emit('| source @ origin/main | new file | symbol | origin vis | class | raw-ptr / safety note |');
emit('|---|---|---|---|---|---|');
for (const w of widened) {
  emit(`| \`${w.file.from}\` | \`${w.file.to}\` | \`${w.now.key}\` | \`${w.prev.vis}\` | ${classify(w.now)} | ${esc(safetyNote(w.now))} |`);
}
emit();
const classCount = {};
for (const w of widened) classCount[classify(w.now)] = (classCount[classify(w.now)] ?? 0) + 1;
emit('| class | widened symbols |');
emit('|---|---|');
for (const [k, v] of Object.entries(classCount).sort()) emit(`| ${k} | ${v} |`);
emit(`| **total widened** | **${widened.length}** |`);
emit();
// R25-1 gate: no safe pub fn with a raw-pointer parameter may dereference it.
const r251 = files.flatMap((f) => f.now).filter((it) => isPub(it) && classify(it) === 'safe fn + raw ptr param (R25-1 class)');
assert.deepEqual(r251.map((i) => i.key), [], 'R25-1: safe pub fn with raw-ptr param still present');
const arith = files.flatMap((f) => f.now).filter((it) => isPub(it) && classify(it) === 'safe, address arithmetic only');
assert.deepEqual(arith.map((i) => i.key).sort(), [...ARITH_ONLY_ALLOWLIST].sort(), 'address-arithmetic-only allowlist');
assert(flipped.size > 0, 'expected the R25-1-class functions to be flipped to unsafe fn');
for (const it of files.flatMap((f) => f.now)) if (isPub(it) && it.kind === 'fn' && it.isUnsafe) assert(/^\s*\/\/\/ # Safety\s*$/m.test(it.docs), `${it.key}: pub unsafe fn without # Safety`);

// ── (b) counts ──
const visOld = (f) => f.old.filter(isVisible);
const originOsVisible = visOld(osFile).length;
const movedOs = visOld(osFile).filter((i) => !OS_STAY.includes(i.name)).length;
const movedAll = files.reduce((s, f) => s + visOld(f).filter((i) => !(f === osFile && OS_STAY.includes(i.name))).length, 0);
const nowPubAll = files.reduce((s, f) => s + f.now.filter(isPub).length, 0);
assert.equal(originOsVisible, movedOs + OS_STAY.length, 'os.rs: moved + stayed = origin total');
assert.equal(nowPubAll, movedAll, 'crate pub symbols == moved symbols');
assert.equal(movedOs, osFile.now.filter(isPub).length, 'os pub count == moved os.rs symbols');

emit('### (b) Symbol counts (visible = `pub`/`pub(crate)` items)');
emit();
emit('| source @ origin/main | visible at origin | moved to sefer-platform | stayed in root |');
emit('|---|---|---|---|');
for (const f of files) {
  const tot = visOld(f).length;
  const stay = f === osFile ? OS_STAY.length : 0;
  emit(`| \`${f.from}\` | ${tot} | ${tot - stay} | ${stay} |`);
}
emit(`| **total** | **${files.reduce((s, f) => s + visOld(f).length, 0)}** | **${movedAll}** | **${OS_STAY.length}** |`);
emit();
emit(`Stayed in root os.rs (directory trio, asserted equal to the inventory's OS_STAY): ${OS_STAY.map((s) => `\`${s}\``).join(', ')}.`);
emit();
emit(`Root os.rs additionally keeps real \`pub(crate)\` safe wrappers with the pre-extraction signatures for the moved raw-pointer functions now \`pub unsafe fn\` in sefer-platform (asserted: signature, cfg gate and attrs equal to origin/main; body forwards to \`sefer_platform::os\`): ${WRAPPED.map((s) => `\`${s}\``).join(', ')}.`);
emit();

// decisions table from inventory + root deps at origin
const invTxt = new Map(invFiles.map((f) => [f.path, f]));
const platformSelf = (path, dep) =>
  (path.includes('platform') && dep.startsWith('alloc_core::platform')) || (path.startsWith('src/global/process_exit') && dep.startsWith('global::process_exit'));
const rootDeps = (path) => {
  const deps = new Map();
  readOrigin(path).split('\n').forEach((l) => {
    if (l.trim().startsWith('//')) return;
    for (const m of l.matchAll(/crate::([A-Za-z0-9_:]+)/g)) {
      const p = m[1].split('::').slice(0, 3).join('::');
      if (!platformSelf(path, p)) deps.set(p, true);
    }
  });
  return [...deps.keys()];
};
const moveSet = new Set(MOVES.map((m) => m.from));
for (const f of invFiles) {
  if (f.decision === 'STAY') {
    assert.equal(readOrigin(f.path), readWork(f.path), `${f.path}: STAY file changed`);
  } else assert(moveSet.has(f.path), `${f.path}: ${f.decision} but not in MOVES`);
}
assert.deepEqual(invFiles.filter((f) => f.decision !== 'STAY').map((f) => f.path).sort(), [...moveSet].sort(), 'inventory EXTRACT/SPLIT set == MOVES');
emit('### (b2) File decisions and root dependencies (from `scripts/p14_extraction_inventory.mjs`)');
emit();
emit('| file | decision | root `crate::` deps (origin/main) | reason |');
emit('|---|---|---|---|');
for (const f of invFiles) {
  const d = rootDeps(f.path);
  emit(`| \`${f.path}\` | ${f.decision} | ${d.length ? d.map((x) => `\`${x}\``).join(', ') : 'none (leaf)'} | ${esc(f.reason)} |`);
}
emit();
const cnt = (d) => invFiles.filter((f) => f.decision === d).length;
emit(`Decision counts: EXTRACT ${cnt('EXTRACT')}, SPLIT ${cnt('SPLIT')}, STAY ${cnt('STAY')} (of ${invTxt.size} candidate files).`);
emit();
assert.deepEqual(rootDeps('src/global/process_exit.rs'), [], 'process_exit.rs: no root deps (no ghost bridge)');
assert.deepEqual(rootDeps('src/alloc_core/platform/fatal.rs'), [], 'fatal.rs: leaf');

// ── (c) cfg gates ──
emit('### (c) cfg gates of moved symbols (identical at origin/main and in sefer-platform: asserted)');
emit();
emit('| symbol | new file:line | gate |');
emit('|---|---|---|');
let gated = 0;
for (const f of files) {
  for (const it of f.now) {
    const g = gateOf(it);
    if (!g) continue;
    gated++;
    emit(`| \`${it.key}\` | \`${f.to.slice(CRATE.length + 1)}:${it.line}\` | \`${esc(g)}\` |`);
  }
}
emit();
emit(`Gated symbols: ${gated} of ${files.reduce((s, f) => s + f.now.length, 0)} scanned items.`);
emit();
// the moved code carries no hand-rolled build cfg (loom/kani/mock)
for (const f of files) assert(!/cfg\([^)]*(loom|kani|mock)/.test(readWork(f.to)), `${f.to}: unexpected hand-rolled cfg`);
// moved code is gated only on known feature names / windows
const crateFeatures = parseFeatures(readWork(`${CRATE}/Cargo.toml`));
for (const f of files) for (const it of f.now) {
  for (const m of gateOf(it).matchAll(/feature = "([^"]+)"/g)) assert(crateFeatures.has(m[1]), `${it.key}: gate on unknown crate feature ${m[1]}`);
}

// ── (d) inline / cold ──
emit('### (d) `#[inline]` / `#[inline(always)]` / `#[cold]` on moved symbols');
emit();
emit('| symbol | origin/main | sefer-platform | change |');
emit('|---|---|---|---|');
const changes = [];
for (const f of files) {
  const o = byKey(f.old);
  for (const it of f.now) {
    const before = perfOf(o.get(it.key)), after = perfOf(it);
    if (!before.length && !after.length) continue;
    const added = after.filter((a) => !before.includes(a)), removed = before.filter((a) => !after.includes(a));
    if (added.length || removed.length) changes.push({ key: it.key, added, removed });
    emit(`| \`${it.key}\` | ${before.map((a) => `\`${a}\``).join(' ') || '—'} | ${after.map((a) => `\`${a}\``).join(' ') || '—'} | ${added.length ? `+ ${added.join(' ')}` : removed.length ? `- ${removed.join(' ')}` : 'unchanged'} |`);
  }
}
emit();
assert.equal(changes.length, 1, `exactly one inline/cold change expected, got ${JSON.stringify(changes)}`);
assert.equal(changes[0].key, 'segment_base_of');
assert.deepEqual(changes[0].added, ['#[inline]']);
assert.deepEqual(changes[0].removed, []);
emit(`Changes relative to origin/main: ${changes.length} (\`${changes[0].key}\`: added ${changes[0].added.map((a) => `\`${a}\``).join(', ')}).`);
emit();

// ── (e) feature forwarding ──
function parseFeatures(toml) {
  const m = /^\[features\]\s*$([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(toml);
  assert(m, 'no [features] section');
  const clean = m[1].split('\n').map((l) => {
    let q = false, o = '';
    for (const ch of l) { if (ch === '"') q = !q; if (ch === '#' && !q) break; o += ch; }
    return o;
  }).join('\n');
  const feats = new Map();
  for (const x of clean.matchAll(/^([A-Za-z0-9_-]+)\s*=\s*\[([\s\S]*?)\]/gm)) feats.set(x[1], [...x[2].matchAll(/"([^"]+)"/g)].map((y) => y[1]));
  return feats;
}
const rootToml = readWork('Cargo.toml');
const rootFeatures = parseFeatures(rootToml);
emit('### (e) Feature forwarding: root `sefer-alloc` -> `sefer-platform`');
emit();
emit('| feature | root forwards | crate-side enables |');
emit('|---|---|---|');
for (const [name, deps] of crateFeatures) {
  const rf = rootFeatures.get(name);
  assert(rf, `crate feature ${name} does not exist in the root`);
  const fwd = rf.find((x) => x === `sefer-platform/${name}` || x === `sefer-platform?/${name}`);
  assert(fwd, `root feature ${name} does not forward sefer-platform/${name}`);
  emit(`| \`${name}\` | \`${fwd}\` | ${deps.length ? deps.map((d) => `\`${d}\``).join(', ') : '— (empty toggle)'} |`);
}
emit();
assert((rootFeatures.get('alloc-core') ?? []).includes('dep:sefer-platform'), 'root alloc-core must activate dep:sefer-platform');
assert(/^sefer-platform = \{ workspace = true, optional = true \}/m.test(rootToml), 'root optional dependency on sefer-platform');
assert(/"crates\/platform\/sefer-platform"/.test(/^members = \[.*\]/m.exec(rootToml)[0]), 'workspace member');
emit(`Crate features: ${crateFeatures.size}; all forwarded by the root. Optional dependency: root \`alloc-core\` -> \`dep:sefer-platform\`.`);
emit();

// ── output ──
const md = out.join('\n').trimEnd() + '\n';
if (process.argv.includes('--write')) {
  const cur = readWork(REPORT);
  const a = cur.indexOf(BEGIN), b = cur.indexOf(END);
  assert(a >= 0 && b > a, `${REPORT}: markers missing`);
  const next = `${cur.slice(0, a + BEGIN.length)}\n\n${md}\n${cur.slice(b)}`;
  writeFileSync(join(ROOT, REPORT), next);
  console.log(`updated ${REPORT} (${md.split('\n').length} generated lines)`);
} else {
  process.stdout.write(md);
}
