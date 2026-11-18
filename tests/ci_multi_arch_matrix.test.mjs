// Task #196B: coverage proof for the multi-arch matrix job. Strict bounded text
// extraction of the exact observed ci.yml layout via an anchored global block
// regex; every non-comment line between the exact header and the exact steps
// prefix must be a flat include row, and steps must be exact fixed shapes.
// Anything else fails closed with a named diagnostic. Zero external deps.
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { effectiveWorkflow } from '../scripts/ci/effective-workflow.mjs';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const JOB_RE = /^  multi-arch:\n([\s\S]*?)(?=^  [\w-]+:|(?![\s\S]))/gm;
const NATIVE_T = 'x86_64-unknown-linux-gnu';
const PREFIX = 'aarch64-unknown-linux-gnu';
const NATIVE_GUARD = '${{ !matrix.use_cross }}';
const CROSS_GUARD = '${{ matrix.use_cross }}';
const CROSS_RUN = 'cross test --target ${{ matrix.target }} --features "${{ matrix.features }}"';
const LABEL_FOR = { experimental: '(experimental)', 'alloc-global alloc-xthread internals': '(cross-thread)',
  'alloc-global alloc-xthread alloc-decommit internals': '(cross-thread-decommit)', 'production internals': '(production)' };
const CROSS_FEATURES = Object.keys(LABEL_FOR);

const HEADER = [
  '  multi-arch:',
  "    needs: changes",
  "    if: needs.changes.outputs.os == 'true'",
  '    name: test (${{ matrix.label }})',
  '    runs-on: ubuntu-latest',
  '    strategy:',
  '      fail-fast: false',
  '      matrix:',
  '        include:',
];
const STEPS_PREFIX = [
  '    steps:',
  '      - uses: actions/checkout@v5',
  '      - uses: dtolnay/rust-toolchain@stable',
  '        with:',
  '          targets: ${{ matrix.target }}',
  '      - name: Install cross',
  '        if: ${{ matrix.use_cross }}',
  '        uses: taiki-e/install-action@v2',
  '        with:',
  '          tool: cross',
];
const EXPECTED = [
  `${NATIVE_T} / native / experimental`,
  `${NATIVE_T} / native / alloc-global alloc-xthread internals`,
  `${NATIVE_T} / native / alloc-global alloc-xthread alloc-decommit internals`,
  ...CROSS_FEATURES.map((f) => `${PREFIX} / cross / ${f}`),
];
const fail = (msg) => { throw new Error(msg); };
const atLine = (ls, l) => { const at = ls.findLastIndex((x) => x === l); assert.ok(at !== -1, 'mutant line exists: ' + l); return at; };

/** Extract the exact header and steps prefixes; strict-parse the include rows. */
function parseBlock(block) {
  const ls = block.split('\n').filter((l) => l.trim() !== '' && !l.trimStart().startsWith('#'));
  if (ls.length < HEADER.length + STEPS_PREFIX.length) fail('multi-arch block is shorter than the fixed shape');
  for (let i = 0; i < HEADER.length; i++) if (ls[i] !== HEADER[i]) fail(`header line ${i} must be ${JSON.stringify(HEADER[i])}, got ${JSON.stringify(ls[i])}`);
  const stepsAt = ls.indexOf(STEPS_PREFIX[0]);
  if (stepsAt === -1 || stepsAt + STEPS_PREFIX.length > ls.length) fail('exact steps: block header not found');
  for (let j = 0; j < STEPS_PREFIX.length; j++) if (ls[stepsAt + j] !== STEPS_PREFIX[j]) fail(`steps prefix line ${j} must be ${JSON.stringify(STEPS_PREFIX[j])}, got ${JSON.stringify(ls[stepsAt + j])}`);
  const rows = [];
  for (const l of ls.slice(HEADER.length, stepsAt)) {
    let m = l.match(/^          - target: ([\w-]+)$/);
    if (m) { rows.push({ target: m[1] }); continue; }
    m = l.match(/^            ([\w-]+): (.+)$/);
    if (!m) fail(`include lines must be "10sp - target: V" or 12sp key: value, got ${JSON.stringify(l)}`);
    if (!['label', 'features', 'use_cross', 'target'].includes(m[1])) fail(`unknown include key: ${m[1]}`);
    const row = rows.at(-1);
    if (!row) fail('continuation before any include row');
    if (Object.hasOwn(row, m[1])) fail(`duplicate include key: ${m[1]}`);
    row[m[1]] = m[2];
  }
  const steps = [];
  for (let i = stepsAt + STEPS_PREFIX.length; i < ls.length; i += 3) {
    const trio = ls.slice(i, i + 3);
    if (trio.length !== 3) fail('each remaining test step must be exactly 3 lines');
    const name = trio[0].match(/^      - name: (.+)$/), iff = trio[1].match(/^        if: (.+)$/), run = trio[2].match(/^        run: (.+)$/);
    if (!name || !iff || !run) fail(`step must be (- name, if, run) trio, got ${JSON.stringify(trio)}`);
    steps.push({ name: name[1], if: iff[1], run: run[1] });
  }
  return { rows, steps };
}

function validate(t) {
  const { rows, steps } = parseBlock(jobBlock(t ?? text()));
  for (const r of rows) {
    if (!['true', 'false'].includes(r.use_cross)) fail(`use_cross must be a Boolean ("true"/"false"), got ${JSON.stringify(r.use_cross)}`);
    const native = r.use_cross === 'false';
    const allowed = native ? ['target', 'label', 'use_cross'] : ['target', 'label', 'features', 'use_cross'];
    const keys = Object.keys(r).sort();
    if (keys.join(',') !== allowed.sort().join(',')) fail(`include row keys must be exactly [${allowed}] for ${r.label ?? r.target}, got [${keys}]`);
    if (native) {
      if (r.target !== NATIVE_T) fail(`native target must be ${NATIVE_T}`);
      if (r.label !== NATIVE_T) fail('native label must be the exact target');
    } else if (r.target !== PREFIX) fail(`cross target must be ${PREFIX}`);
  }
  const crossRows = rows.filter((r) => r.use_cross === 'true'), nativeRows = rows.filter((r) => r.use_cross === 'false');
  if (nativeRows.length !== 1) fail(`exactly 1 native include row, got ${nativeRows.length}`);
  const crossSteps = steps.filter((s) => s.if === CROSS_GUARD), nativeSteps = steps.filter((s) => s.if === NATIVE_GUARD);
  if (steps.some((s) => s.if !== CROSS_GUARD && s.if !== NATIVE_GUARD)) fail('every test step guard must be the exact use_cross guard');
  if (crossSteps.length !== 1) fail(`exactly 1 cross step, got ${crossSteps.length}`);
  if (nativeSteps.length !== 3) fail(`exactly 3 native steps, got ${nativeSteps.length}`);
  if (crossSteps[0].run !== CROSS_RUN) fail(`cross step run must be exactly ${CROSS_RUN}`);
  const NATIVE_RE = /^cargo test --target \$\{\{ matrix\.target \}\} --features (?:"([\w -]+)"|([\w-]+))$/;
  if (nativeSteps.some((s) => !NATIVE_RE.test(s.run))) fail('every native step must be an anchored cargo-test --features command');
  const seen = new Map();
  for (const m of nativeSteps.map((s) => s.run.match(NATIVE_RE))) seen.set(`${NATIVE_T} / native / ${m[1] ?? m[2]}`, 1);
  for (const r of crossRows) seen.set(`${r.target} / cross / ${r.features}`, (seen.get(`${r.target} / cross / ${r.features}`) ?? 0) + 1);
  const problems = [];
  for (const t of EXPECTED) {
    if (!seen.has(t)) problems.push(`missing feature tuple: ${t}`);
    else if (seen.get(t) > 1) problems.push(`duplicate feature tuple: ${t}`);
    seen.delete(t);
  }
  for (const t of seen.keys()) problems.push(`unexpected feature tuple: ${t}`);
  if (problems.length) fail(problems.join('; '));
  const labels = crossRows.map((r) => r.label);
  if (new Set(labels).size !== labels.length) fail('duplicate aarch64 job labels');
  for (const r of crossRows) if (r.label !== `${PREFIX} ${LABEL_FOR[r.features]}`) fail(`label\u2194features mapping mismatch for features ${JSON.stringify(r.features)}: ${JSON.stringify(r.label)}`);
}
// Validate a full workflow text; returns the captured block (header kept).
function jobBlock(text) {
  const m = [...text.matchAll(JOB_RE)];
  if (m.length !== 1) fail(`exactly one multi-arch block expected, found ${m.length}`);
  return m[0][0];
}

const text = () => effectiveWorkflow(ROOT).text.replace(/\r\n/g, '\n');
const mutant = (edit) => { const ls = text().split('\n'); edit(ls); return ls.join('\n'); };
const labelLine = (s) => `            label: aarch64-unknown-linux-gnu ${s}`;
const rowStart = (ls, s) => atLine(ls, labelLine(s)) - 1;

test('real CI workflow multi-arch matrix matches the origin/main baseline', () => { validate(); });

// Table-driven RED counterfactuals: each mutates the REAL workflow text and
// must be rejected with its named diagnostic.
const tup = (f, mo = 'cross', t = PREFIX) => `${t} / ${mo} / ${f}`;
const MUTANTS = [
  ['dropped include row', (ls) => ls.splice(rowStart(ls, '(production)'), 4), new RegExp(`missing feature tuple: ${tup('production internals')}`)],
  ['duplicated suite', (ls) => { const at = rowStart(ls, '(experimental)'); ls.splice(at + 4, 0, ...ls.slice(at, at + 4)); }, new RegExp(`duplicate feature tuple: ${tup('experimental')}`)],
  ['wrong native features command', (ls) => { const at = atLine(ls, '        run: cargo test --target ${{ matrix.target }} --features experimental'); ls[at] = `        run: cargo test --target \${{ matrix.target }} --features "experimental wrong-extra"`; },
    new RegExp(`missing feature tuple: ${tup('experimental', 'native', NATIVE_T)}[\\s\\S]*unexpected feature tuple: ${tup('experimental wrong-extra', 'native', NATIVE_T)}`)],
  ['broken cross guard', (ls) => { const at = atLine(ls, '        run: ' + CROSS_RUN); ls[at - 1] = `        if: \${{ !matrix.use_cross }}`; }, /exactly 1 cross step/],
  ['removed aarch64 label', (ls) => ls.splice(atLine(ls, labelLine('(experimental)')), 1), /include row keys must be exactly/],
  ['duplicated aarch64 label', (ls) => { ls[atLine(ls, labelLine('(cross-thread-decommit)'))] = labelLine('(cross-thread)'); }, /duplicate aarch64 job labels/],
  ['malformed include nesting', (ls) => { const at = atLine(ls, '          - target: aarch64-unknown-linux-gnu'); ls[at] = '         ' + ls[at]; }, /include lines must be/],
  ['duplicate include key', (ls) => ls.splice(atLine(ls, '            features: experimental'), 0, '            features: experimental'), /duplicate include key: features/],
  ['unknown include key', (ls) => ls.splice(atLine(ls, labelLine('(production)')) + 1, 0, '            extra: true'), /unknown include key: extra/],
  ['repeated matrix block', (ls) => { const at = atLine(ls, HEADER[6]); ls[at] = HEADER[7]; ls.splice(at + 1, 0, HEADER[8]); }, /header line 6 must be/],
];
for (const [name, edit, re] of MUTANTS) {
  test(`RED counterfactual (${name}) is rejected with its named diagnostic`, () => {
    assert.throws(() => validate(mutant(edit)), (e) => e instanceof Error && re.test(e.message), name);
  });
}
