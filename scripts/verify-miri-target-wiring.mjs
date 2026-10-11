// Static VER-03 wiring gate. No Cargo/Miri execution and no repository writes.
// Manifest contract: MIRI_TARGETS entries use job/test/pkg/features/model/borrows/
// args/expect/marker/redMessage/redSite/card; WAIVED also requires reason.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => readFileSync(join(root, p), 'utf8');
const failures = [];
const fail = (s) => failures.push(s);
const tokens = (s) => (Array.isArray(s) ? s : String(s ?? '').split(/[\s,]+/)).filter(Boolean);
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const featureKey = (s) => [...new Set(tokens(s))].sort();
const label = (r) => `${r.pkg || 'root'}/${r.test}${r.args?.length ? ` ${r.args.join(' ')}` : ''} [${r.model}/${r.borrows}]`;

// Remove comments without mistaking quoted strings for comments; nested Rust
// block comments are supported. Newlines survive for diagnostics and YAML.
function uncomment(text, language) {
  let out = '', i = 0, depth = 0, quote = '';
  while (i < text.length) {
    const c = text[i], n = text[i + 1];
    if (depth) {
      if (c === '/' && n === '*') { depth++; i += 2; }
      else if (c === '*' && n === '/') { depth--; i += 2; }
      else { out += c === '\n' ? '\n' : ' '; i++; }
    } else if (quote) {
      out += c; i++;
      if (c === '\\' && quote !== "'") { out += text[i++] ?? ''; }
      else if (c === quote) quote = '';
    } else if ((language === 'yaml' && c === '#') || (language !== 'yaml' && c === '/' && n === '/')) {
      while (i < text.length && text[i] !== '\n') i++;
    } else if (language !== 'yaml' && c === '/' && n === '*') { depth++; i += 2; out += ' '; }
    else { out += c; i++; if (c === '"' || (language !== 'rust' && c === "'")) quote = c; }
  }
  if (depth) throw new Error('unclosed block comment');
  return out;
}
function cfgs(text, innerOnly = false) {
  const result = [];
  const re = innerOnly ? /^\s*#!\s*\[\s*cfg\s*\(/gm : /^\s*#!?\s*\[\s*cfg\s*\(/gm;
  for (let m; (m = re.exec(text));) {
    // Ignore attribute-looking lines inside multiline Rust string fixtures.
    let inString = false;
    for (let i = 0; i < m.index; i++) {
      if (inString && text[i] === '\\') { i++; continue; }
      if (text[i] === '"') inString = !inString;
    }
    if (inString) continue;
    let end = re.lastIndex, depth = 1, quote = false;
    for (; end < text.length && depth; end++) {
      const c = text[end];
      if (c === '"' && text[end - 1] !== '\\') quote = !quote;
      if (!quote) { if (c === '(') depth++; if (c === ')') depth--; }
    }
    if (depth) throw new Error('unclosed cfg');
    result.push(text.slice(re.lastIndex, end - 1).trim()); re.lastIndex = end;
  }
  return result;
}
// Adapted from evalCfg in scripts/verify-evidence-registry.mjs (245-297).
// Same three-valued semantics; miri/test are true, loom/kani false here.
// Parse to an AST first so positive miri under nested not() is inventoried.
function parseCfg(expr) {
  const ts = expr.match(/"[^"\n]*"|[A-Za-z0-9_\-]+|[()=,]/g) ?? [];
  if (ts.join('') !== expr.replace(/\s+/g, '').replace(/"[^"\n]*"/g, (s) => s.replace(/\s+/g, ''))) throw new Error(`bad cfg tokens: ${expr}`);
  let p = 0;
  function atom() {
    const name = ts[p++];
    if (!name || /^[()=,]$/.test(name)) throw new Error(`bad cfg atom: ${expr}`);
    if (['all', 'any', 'not'].includes(name)) {
      if (ts[p++] !== '(') throw new Error(`bad cfg call: ${expr}`);
      const args = [];
      while (ts[p] !== ')') { args.push(atom()); if (ts[p] !== ',') break; p++; }
      if (ts[p++] !== ')' || (name === 'not' && args.length !== 1)) throw new Error(`bad cfg arity: ${expr}`);
      return { name, args };
    }
    let value;
    if (ts[p] === '=') { p++; value = ts[p++]; if (!/^".*"$/.test(value ?? '')) throw new Error(`bad cfg value: ${expr}`); }
    return { name, value: value?.slice(1, -1) };
  }
  const ast = atom(); if (p !== ts.length) throw new Error(`cfg trailing tokens: ${expr}`); return ast;
}
function evalCfg(ast, feats) {
  const { name, args, value } = ast;
  if (name === 'feature') return feats.has(value);
  if (name === 'not') { const a = evalCfg(args[0], feats); return a === null ? null : !a; }
  if (name === 'all' || name === 'any') {
    const a = args.map((x) => evalCfg(x, feats));
    return name === 'all' ? (a.includes(false) ? false : a.includes(null) ? null : true)
      : (a.includes(true) ? true : a.includes(null) ? null : false);
  }
  if (['miri', 'test'].includes(name) && value === undefined) return true;
  if (['loom', 'kani'].includes(name) && value === undefined) return false;
  return null; // target predicates need an actual target, not the host OS.
}
function positiveMiri(ast, positive = true) {
  if (ast.name === 'miri' && ast.value === undefined) return positive;
  return (ast.args ?? []).some((a) => positiveMiri(a, ast.name === 'not' ? !positive : positive));
}
const graphs = new Map();
function closure(r) {
  const path = r.pkg ? `crates/${r.pkg}/Cargo.toml` : 'Cargo.toml';
  if (!graphs.has(path)) {
    const text = read(path).replace(/^\s*#.*$/gm, '');
    const section = text.match(/^\[features\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1] ?? '';
    const graph = new Map();
    for (const m of section.matchAll(/^([\w-]+)\s*=\s*\[([^\]]*)\]/gm)) graph.set(m[1], [...m[2].matchAll(/"([^"]*)"/g)].map((x) => x[1]));
    graphs.set(path, graph);
  }
  const graph = graphs.get(path), set = new Set();
  const pending = [...tokens(r.features), ...(r.noDefaultFeatures ? [] : ['default'])];
  if (r.allFeatures) pending.push(...graph.keys());
  while (pending.length) {
    const f = pending.pop(); if (set.has(f)) continue; set.add(f);
    if (!graph.has(f) && f !== 'default' && !f.includes('/') && !f.startsWith('dep:')) fail(`${label(r)}: unknown Cargo feature ${f}`);
    for (const d of graph.get(f) ?? []) if (!d.includes('/') && !d.startsWith('dep:')) pending.push(d);
  }
  return set;
}
function shellTokens(s) {
  const re = /"([^"\n]*)"|'([^'\n]*)'|([^\s]+)/g;
  return [...s.matchAll(re)].map((m) => m[1] ?? m[2] ?? m[3]);
}
function ciRows() {
  const text = uncomment(read('.github/workflows/ci.yml'), 'yaml');
  const jobs = [...text.matchAll(/^  ([\w-]+):\s*$/gm)], rows = [];
  for (let j = 0; j < jobs.length; j++) {
    const job = jobs[j][1], body = text.slice(jobs[j].index, jobs[j + 1]?.index ?? text.length);
    if (!job.startsWith('miri-')) continue;
    const stepsAt = body.indexOf('    steps:');
    if (stepsAt < 0) throw new Error(`${job}: unsupported steps`);
    const inherited = body.slice(0, stepsAt).match(/MIRIFLAGS:\s*"([^"]*)"/)?.[1] ?? '';
    const steps = [...body.matchAll(/^      - /gm)];
    for (let k = 0; k < steps.length; k++) {
      const step = body.slice(steps[k].index, steps[k + 1]?.index ?? body.length);
      const flags = step.match(/MIRIFLAGS:\s*"([^"]*)"/)?.[1] ?? inherited;
      const run = step.match(/^\s*(?:- )?run:\s*([^\n]*)\n?/m);
      if (!run) continue;
      let script = run[1];
      if (/^[|>][-+]?\s*$/.test(script)) script = step.slice(run.index + run[0].length).split('\n').filter((s) => /^          /.test(s)).join('\n');
      script = script.replace(/\\\s*\n/g, ' ');
      // Folded YAML commands have continuation --test lines, literal scripts do not.
      if (run[1].startsWith('>')) script = script.replace(/\n/g, ' ');
      for (const command of script.matchAll(/cargo\s+(?:\+\S+\s+)?miri\s+test\s+([^\n]*?)(?=\s+2>&1|\s+\||\n|$)/g)) {
        const argv = shellTokens(command[1]), tests = [], features = [];
        let pkg, args = [], noDefaultFeatures = false, allFeatures = false;
        for (let i = 0; i < argv.length; i++) {
          const a = argv[i];
          if (a === '--') { args = ['--', ...argv.slice(i + 1)]; break; }
          if (a === '--test') tests.push(argv[++i]);
          else if (a === '--features' || a === '-F') features.push(...tokens(argv[++i]));
          else if (a === '-p' || a === '--package') pkg = argv[++i];
          else if (a === '--no-default-features') noDefaultFeatures = true;
          else if (a === '--all-features') allFeatures = true;
          else if (a === '--locked') { /* lockfile policy does not alter cfg/wiring identity */ }
          else throw new Error(`${job}: unsupported Cargo argument ${a}`);
        }
        for (const test of tests) rows.push({ job, test, pkg, features, args, noDefaultFeatures, allFeatures,
          model: flags.includes('-Zmiri-strict-provenance') ? 'strict' : 'plain',
          borrows: flags.includes('-Zmiri-tree-borrows') ? 'tb' : 'sb', flags, script });
      }
    }
  }
  if (!rows.length) throw new Error('no CI Miri commands parsed'); return rows;
}
function identity(a, b) {
  return a.job === b.job && a.test === b.test && (a.pkg || '') === (b.pkg || '')
    && a.model === b.model && a.borrows === b.borrows && same(a.args ?? [], b.args ?? [])
    && same(featureKey(a.features), featureKey(b.features))
    && !!a.noDefaultFeatures === !!b.noDefaultFeatures && !!a.allFeatures === !!b.allFeatures;
}
try {
  const ci = ciRows(), manifestPath = join(root, 'scripts/miri-matrix.mjs');
  if (process.argv.length > 2) throw new Error('no arguments are supported');
  if (!existsSync(manifestPath)) throw new Error('missing Miri manifest');
  const rows = (await import(pathToFileURL(manifestPath).href)).MIRI_TARGETS;
  if (!Array.isArray(rows) || !rows.length) throw new Error('MIRI_TARGETS must be a nonempty array');
  console.log('[verify-miri-target-wiring] mode: manifest MIRI_TARGETS');
  const inventory = readdirSync(join(root, 'tests')).filter((n) => n.endsWith('.rs')).filter((n) => {
    const text = uncomment(read(`tests/${n}`), 'rust');
    return /miri/.test(n) || cfgs(text, true).some((s) => positiveMiri(parseCfg(s)));
  }).map((n) => n.slice(0, -3)).sort();
  if (!inventory.length) fail('empty Miri inventory');
  for (const test of inventory) {
    const local = rows.some((r) => r.test === test && !r.pkg);
    const wired = ci.some((r) => r.test === test && !r.pkg);
    if (!local) fail(`inventory ${test}: missing manifest`);
    if (!wired && !local && !rows.some((r) => r.test === test && !r.pkg && String(r.expect).toLowerCase() === 'waived')) fail(`inventory ${test}: missing CI`);
  }
  const seen = new Set();
  for (const r of rows) {
    const expectation = String(r.expect).toLowerCase();
    if (!['green', 'expected-red', 'waived'].includes(expectation)) { fail(`${label(r)}: invalid expectation`); continue; }
    if (!/^[\w-]+$/.test(r.test ?? '') || (r.pkg && !/^[\w-]+$/.test(r.pkg))) throw new Error('unsafe target/package path');
    if (!['strict', 'plain'].includes(r.model) || !['sb', 'tb'].includes(r.borrows) || !Array.isArray(r.args)) fail(`${label(r)}: invalid model/borrows/args`);
    const key = JSON.stringify([r.job, r.pkg, r.test, featureKey(r.features), r.model, r.borrows, r.args]);
    if (seen.has(key)) fail(`${label(r)}: duplicate entry`); seen.add(key);
    if (expectation === 'waived') {
      if (!r.reason?.trim() || !r.card) fail(`${label(r)}: WAIVED requires reason and card`);
      continue;
    }
    const path = r.pkg ? `crates/${r.pkg}/tests/${r.test}.rs` : `tests/${r.test}.rs`;
    if (!existsSync(join(root, path))) { fail(`${label(r)}: missing test file`); continue; }
    const feats = closure(r);
    for (const expr of cfgs(uncomment(read(path), 'rust'), true)) {
      if (evalCfg(parseCfg(expr), feats) === false) fail(`${label(r)}: unsatisfiable cfg(${expr}) (transitive Cargo features; N1/pass-by-absence)`);
    }
    const matches = ci.filter((c) => identity(r, c));
    if (!matches.length) fail(`${label(r)}: no exact CI wiring (job/features/model/borrows/args)`);
    for (const c of matches) {
      if (!c.flags.includes('-Zmiri-disable-isolation') || (r.model === 'plain' && !c.flags.includes('-Zmiri-preemption-rate=0.5'))) fail(`${label(r)}: incompatible MIRIFLAGS`);
      if (expectation === 'expected-red') {
        // Use uncommented executable step text: another step or a YAML comment
        // cannot satisfy the manifest's literal pin.
        const literals = [r.marker, r.redMessage, r.redSite?.frame0, r.redSite?.file, r.redSite?.caller];
        if (!r.card || literals.some((s) => typeof s !== 'string' || !s)) fail(`${label(r)}: EXPECTED-RED requires card/marker/message/frame0/file/caller`);
        for (const s of literals.filter(Boolean)) if (!c.script.includes(s)) fail(`${label(r)}: expected-red literal absent in same CI step: ${s}`);
        if (!/test\s+"\$status"\s+-ne\s+0/.test(c.script) || !/if grep -F/.test(c.script) || !/exit 1/.test(c.script)) fail(`${label(r)}: expected-red exit/green-rejection guard absent`);
      } else if (/test\s+"\$status"\s+-ne\s+0/.test(c.script)) fail(`${label(r)}: GREEN locally but EXPECTED-RED in CI (card 164)`);
      if (r.marker && !c.script.includes(r.marker)) fail(`${label(r)}: marker absent in same CI step`);
    }
  }
  for (const c of ci) if (!rows.some((r) => String(r.expect).toLowerCase() !== 'waived' && identity(r, c))) fail(`CI ${label(c)} job=${c.job}: missing exact manifest entry`);
  console.log(`[verify-miri-target-wiring] inventory=${inventory.length}, entries=${rows.length}, CI entries=${ci.length}`);
  for (const s of failures) console.log(`[verify-miri-target-wiring] RED: ${s}`);
  console.log(`[verify-miri-target-wiring] ${failures.length ? `FAIL: ${failures.length} wiring defects` : 'OK'}`);
  process.exitCode = failures.length ? 1 : 0;
} catch (error) {
  console.error(`[verify-miri-target-wiring] FAIL: ${error.message}`);
  process.exitCode = 1;
}
