// Miri execution from miri-matrix.mjs; manual CI checked by
// verify-miri-target-wiring.mjs. Help/dry-run/self-test never spawn Cargo.
import { MIRI_TARGETS } from './miri-matrix.mjs';
import { spawn } from 'node:child_process';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const label = (r) => `${r.job}/${r.test} ${r.args.join(' ')} [${r.model}/${r.borrows}]`;
const features = (r) => Array.isArray(r.features) ? r.features.join(' ') : r.features;
function flags(r) {
  return [r.model === 'strict' ? '-Zmiri-strict-provenance -Zmiri-disable-isolation'
    : '-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5',
  ...(r.borrows === 'tb' ? ['-Zmiri-tree-borrows'] : []),
  ...(r.test === 'miri_global_box_acceptance' ? ['-Zmiri-report-progress=10000000'] : [])].join(' ');
}
function argv(r) {
  return ['+nightly', 'miri', 'test', '--locked',
    ...(r.pkg ? ['-p', r.pkg] : []), ...(r.noDefaultFeatures ? ['--no-default-features'] : []),
    ...(r.allFeatures ? ['--all-features'] : []), ...(features(r) ? ['--features', features(r)] : []),
    ...(r.test === 'miri_global_box_acceptance' ? ['-j', '1'] : []), '--test', r.test, ...r.args];
}
// CI uses grep -E (POSIX ERE), not JS regexes. Only translate the POSIX
// whitespace class actually used by the pinned frame expression; fail closed
// on other classes rather than silently changing their meaning.
function ere(pattern, multiline = false) {
  const translated = pattern.replaceAll('[[:space:]]', '[\\t\\r\\n\\v\\f ]');
  if (translated.includes('[:')) throw new Error(`unsupported POSIX class: ${pattern}`);
  return new RegExp(translated, multiline ? 'm' : '');
}
function expectedRed(r, code, out) {
  // CI rejects COMPLETE on either stream even if the process later fails.
  if (out.includes(r.marker) || code === 0) return { ok: false, verdict: `FAIL: close card ${r.card} (witness green)` };
  if (!Number.isInteger(code) || code < 0) return { ok: false, verdict: 'FAIL: new defect (process did not exit normally)' };
  if (!ere(r.redMessage).test(out)) return { ok: false, verdict: 'FAIL: new defect (unexpected UB message)' };
  // grep -A2 frame0 + sed 2p/3p: site and caller must be the two
  // immediately following lines, never unrelated matches elsewhere in log.
  const lines = out.split(/\r?\n/);
  const frame = lines.findIndex((s) => ere(r.redSite.frame0).test(s));
  if (frame < 0 || !ere(r.redSite.file).test(lines[frame + 1] ?? '')
    || !ere(r.redSite.caller).test(lines[frame + 2] ?? '')) {
    return { ok: false, verdict: 'FAIL: new defect (unexpected UB site/caller)' };
  }
  return { ok: true, verdict: 'EXPECTED-RED-OK' };
}
function green(r, code, out) {
  const ran = r.test === 'miri_global_box_acceptance' ? out.includes(r.marker) : /running [1-9]\d* test/.test(out);
  const exact = r.args.indexOf('--exact');
  const completed = exact < 0 || out.includes(`test ${r.args[exact + 1]} ... ok`);
  const ok = code === 0 && ran && completed && (!r.marker || out.includes(r.marker))
    && (r.test === 'miri_global_box_acceptance' || /test result: ok/.test(out))
    && !/^error(?:\[|:)|test result: FAILED/m.test(out);
  return { ok, verdict: ok ? 'PASS' : 'FAIL: no selected invariant completed or process/test error' };
}
function execute(r) {
  return new Promise((resolveResult, reject) => {
    const child = spawn('cargo', argv(r), { cwd: root, shell: false,
      env: { ...process.env, CARGO_BUILD_JOBS: '3', CARGO_TARGET_DIR: resolve(root, 'target'),
        RUSTC_WRAPPER: '', MIRIFLAGS: flags(r) } });
    // Full combined log: unlike a tail buffer, a late error cannot erase an
    // earlier green marker and turn an EXPECTED-RED failure into success.
    let out = '';
    for (const stream of [child.stdout, child.stderr]) stream.on('data', (chunk) => {
      const text = chunk.toString(); out += text; process.stdout.write(text);
    });
    child.on('error', reject);
    child.on('close', (code, signal) => resolveResult({ code: signal ? null : code, out }));
  });
}
function selfTest() {
  let count = 0;
  const redRows = MIRI_TARGETS.filter((r) => r.expect === 'EXPECTED-RED');
  assert.equal(redRows.length, 4, 'must test all four pinned EXPECTED-RED variants');
  for (const r of redRows) {
    const message = r.borrows === 'sb'
      ? 'Undefined Behavior: not granting access to tag <123> weakly protected'
      : 'Undefined Behavior: write access through <123> is forbidden';
    const frame = '    0: sefer_alloc::alloc_core::platform::node::Node::write_next';
    const site = '       at src/alloc_core/platform/node.rs:42:5';
    const caller = '    1: sefer_alloc::reclaim_sidecar_record';
    const good = `${message}\n${frame}\n${site}\n${caller}\n`;
    const cases = [
      ['expected', 1, good, true, 'EXPECTED-RED-OK'],
      ['green', 0, r.marker, false, `close card ${r.card}`],
      ['zero without marker', 0, good, false, `close card ${r.card}`],
      ['marker then UB', 1, `${r.marker}\n${good}`, false, `close card ${r.card}`],
      ['wrong UB', 1, good.replace(message, 'Undefined Behavior: unrelated UB'), false, 'new defect'],
      ['wrong frame', 1, good.replace(frame, '    0: unrelated_function'), false, 'new defect'],
      ['wrong file', 1, good.replace(site, '       at other.rs:1'), false, 'new defect'],
      ['wrong caller', 1, good.replace(caller, '    1: unrelated_function'), false, 'new defect'],
      ['site elsewhere', 1, `${message}\n${frame}\nwrong\nwrong\n${site}\n${caller}`, false, 'new defect'],
      ['signal', null, good, false, 'new defect'],
      ['empty', 1, '', false, 'new defect'],
    ];
    for (const [name, code, out, ok, text] of cases) {
      const result = expectedRed(r, code, out);
      assert.equal(result.ok, ok, `${label(r)}: ${name}`);
      assert.ok(result.verdict.includes(text), `${name}: ${result.verdict}`); count++;
    }
    // Both pinned allowed caller alternatives must also work.
    for (const alternative of ['flush_run', 'dealloc_small']) {
      assert.ok(expectedRed(r, 1, good.replace('reclaim_sidecar_record', alternative)).ok); count++;
    }
  }
  assert.equal(green({ test: 'ordinary', args: [] }, 0, 'running 0 tests\ntest result: ok').ok, false); count++;
  assert.equal(green({ test: 'ordinary', args: [] }, 0, 'running 1 test').ok, false); count++;
  assert.equal(green({ test: 'ordinary', args: [] }, 0, 'running 1 test\ntest result: ok').ok, true); count++;
  console.log(`[miri] self-test: PASS (${count} verdict fixtures; 4 EXPECTED-RED variants)`);
}
async function main() {
  const opts = { names: [] };
  const args = process.argv.slice(2);
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === '--job') {
      opts.job = args[++i];
      if (!opts.job || opts.job.startsWith('--')) throw new Error('--job requires a job name');
    } else if (['--help', '--dry-run', '--self-test', '--plain', '--tree-borrows'].includes(a)) opts[a] = true;
    else if (a.startsWith('-')) throw new Error(`unknown option ${a}`);
    else opts.names.push(a);
  }
  if (opts['--help']) {
    console.log('Usage: node scripts/miri.mjs [--job JOB] [--plain] [--tree-borrows] [--dry-run] [TEST ...]\n'
      + '       node scripts/miri.mjs --self-test\n'
      + 'Default: all GREEN/EXPECTED-RED manifest rows, including SB and TB.\n'
      + '--plain/--tree-borrows filter rows; they never override manifest flags.\n'
      + 'WAIVED rows are reported and skipped, not executed.\n'
      + `Jobs: ${[...new Set(MIRI_TARGETS.map((r) => r.job))].join(', ')}`);
    return;
  }
  if (opts['--self-test']) { selfTest(); return; }
  if (opts.job && !MIRI_TARGETS.some((r) => r.job === opts.job)) throw new Error(`unknown job ${opts.job}`);
  for (const test of opts.names) if (!MIRI_TARGETS.some((r) => r.test === test)) throw new Error(`unknown test ${test}`);
  const selected = MIRI_TARGETS.filter((r) => (!opts.job || r.job === opts.job)
    && (!opts.names.length || opts.names.includes(r.test)) && (!opts['--plain'] || r.model === 'plain')
    && (!opts['--tree-borrows'] || r.borrows === 'tb'));
  if (!selected.length) throw new Error('0 entries selected');
  let ok = true, executed = 0;
  for (const r of selected) {
    if (r.expect === 'WAIVED') { console.log(`[miri] WAIVED ${label(r)}: card ${r.card}: ${r.reason}`); continue; }
    if (!['GREEN', 'EXPECTED-RED'].includes(r.expect)) throw new Error(`invalid expectation ${r.expect}`);
    executed++;
    console.log(`[miri] ${r.expect} ${label(r)}\n[miri] MIRIFLAGS=${flags(r)}\n[miri] cargo ${argv(r).map((s) => /\s/.test(s) ? JSON.stringify(s) : s).join(' ')}`);
    if (opts['--dry-run']) continue;
    const result = await execute(r);
    const v = r.expect === 'EXPECTED-RED' ? expectedRed(r, result.code, result.out) : green(r, result.code, result.out);
    console.log(`[miri] ${label(r)}: ${v.verdict}`); ok = v.ok && ok;
  }
  if (!executed) throw new Error('0 executable entries selected (WAIVED only)');
  console.log(`[miri] ${opts['--dry-run'] ? 'dry-run' : 'overall'}: ${opts['--dry-run'] ? 'OK' : ok ? 'PASS' : 'FAIL'} (${executed} executable, ${selected.length - executed} waived)`);
  if (!ok) process.exitCode = 1;
}
try { await main(); } catch (error) { console.error(`[miri] FAIL: ${error.message}`); process.exitCode = 1; }
