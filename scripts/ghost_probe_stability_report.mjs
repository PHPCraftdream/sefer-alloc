// Read-only by default: emit the marked Markdown block, then the CSV.
import { readFileSync, writeFileSync } from 'node:fs';

const perf = new URL('../docs/perf/', import.meta.url);
const stem = 'GHOST_ZERO_COST_PROBE_STABILITY_20261121';
const raw = new URL('_raw_ghost_zero_cost_probe_stability_20261121.log', perf);
const report = new URL(`${stem}.md`, perf);
const summary = new URL(`${stem}_summary.csv`, perf);
const begin = '<!-- BEGIN GENERATED GHOST PROBE RESULTS -->';
const end = '<!-- END GENERATED GHOST PROBE RESULTS -->';
const probes = ['old', 'pa', 'pb', 'pc'];
const pairs = ['ctl', 'p22v0', 'p23v0', 'p23v4'];
const sha = 'f6aef10dc301f18683e97b2fc032404cc4efdf2a';
const run = '44928351811';
const limit = 1000n;
const calibration = [['p22v0', 42346, 1000], ['p23v0', 49215, 0], ['p23v4', -332, 0]];
const record = /^PROBE (old|pa|pb|pc) PAIR (ctl|p22v0|p23v0|p23v4) ir_base=([1-9]\d*) ir_head=([1-9]\d*) delta=(0|-?[1-9]\d*) pct=([+-]\d+\.\d{15}) text_equal=(true|false)$/;

function requireThat(ok, message) {
  if (!ok) throw new Error(message);
}

const signed = n => `${n >= 0 ? '+' : ''}${n}`;
const describe = ({ pct, delta, base }) => `${pct}% (${signed(delta)}/${base})`;
const within = ({ delta, base }) => BigInt(Math.abs(delta)) * limit <= BigInt(base);

function parse(text) {
  const rows = new Map();
  for (const [i, line] of text.split(/\r?\n/).entries()) {
    if (line === '' || line.startsWith('#')) continue;
    const match = record.exec(line);
    requireThat(match && match[0] === line, `raw line ${i + 1}: malformed PROBE record`);
    const [, probe, pair, irBase, irHead, loggedDelta, loggedPct, textEqual] = match;
    const key = `${probe}/${pair}`;
    const where = `raw line ${i + 1} (${key})`;
    requireThat(!rows.has(key), `${where}: duplicate PROBE record`);
    const base = Number(irBase), head = Number(irHead);
    requireThat(Number.isSafeInteger(base) && Number.isSafeInteger(head), `${where}: Ir counts must be positive safe integers`);
    const delta = head - base;
    requireThat(String(delta) === loggedDelta, `${where}: delta ${loggedDelta} does not match head − base (${delta})`);
    const pct = `${delta >= 0 ? '+' : ''}${(100 * delta / base).toFixed(15)}`;
    requireThat(pct === loggedPct, `${where}: pct ${loggedPct} does not match recomputed ${pct}`);
    rows.set(key, { probe, pair, base, head, delta, pct, textEqual });
  }
  const keys = probes.flatMap(probe => pairs.map(pair => `${probe}/${pair}`));
  const missing = keys.filter(key => !rows.has(key));
  requireThat(!missing.length, `missing PROBE records: ${missing.join(', ')}`);
  return keys.map(key => rows.get(key));
}

function generate(rows) {
  const get = (probe, pair) => rows.find(row => row.probe === probe && row.pair === pair);
  for (const probe of probes) {
    const ctl = get(probe, 'ctl');
    requireThat(ctl.delta === 0 && ctl.textEqual === 'true',
      `${probe}/ctl: expected delta=0 and text_equal=true; got delta=${ctl.delta} text_equal=${ctl.textEqual}`);
  }
  const calibrated = calibration.map(([pair, target, tolerance]) => {
    const row = get('old', pair);
    const expected = `${signed(target)}${tolerance ? ` ± ${tolerance} (inclusive)` : ' exactly'}`;
    requireThat(row.delta >= target - tolerance && row.delta <= target + tolerance,
      `old/${pair}: calibration expected delta ${expected}; got ${signed(row.delta)}`);
    return `${pair} ${describe(row)} (expected Δ ${expected})`;
  });
  const failures = probe => rows.filter(row => row.probe === probe && !within(row));
  const cells = list => list.map(row => `${row.pair} ${describe(row)}`).join(', ');
  const pa = failures('pa');
  requireThat(!pa.length, `pa exceeds the absolute 0.1% threshold: ${cells(pa)}`);
  const verdict = probe => {
    const misses = failures(probe);
    return misses.length
      ? `**${probe}: rejected.** Outside the absolute 0.1% threshold on ${cells(misses)}.`
      : `**${probe}: qualifies.** All ${pairs.length} pairs meet the absolute 0.1% threshold.`;
  };
  const paCtl = get('pa', 'ctl'), pcCtl = get('pc', 'ctl');
  const comparison = pcCtl.base > paCtl.base ? 'higher' : pcCtl.base < paCtl.base ? 'lower' : 'equal';
  const markdown = [begin, '',
    'Fixed historical measurements: Callgrind instruction counts (Ir), one sample per arm. These do not establish relative runtime performance.', '',
    '| Probe | Pair | Ir base | Ir head | Signed change (Δ/base) | Text equal |',
    '|---|---|---:|---:|---:|---|',
    ...rows.map(row => `| ${row.probe} | ${row.pair} | ${row.base} | ${row.head} | ${describe(row)} | ${row.textEqual} |`), '',
    `**Calibration passes.** old: ${calibrated.join('; ')}. All ${probes.length} ctl pairs have Δ +0 and text_equal=true.`, '',
    'Qualification uses the exact, unrounded ratio: absolute Δ/base ≤ 1/1000 (0.1%).', '',
    `- ${verdict('pa')}`,
    `- ${verdict('pb')}`,
    `- ${verdict('pc')} On ctl, pc observed Ir ${pcCtl.base} vs pa ${paCtl.base} (${comparison}).`, '', end,
  ].join('\n');
  const csv = ['source_sha,run_id,features,cpu_os,samples_per_arm,probe,pair,ir_base,ir_head,delta,pct,text_equal',
    ...rows.map(row => [sha, run, 'production', 'x86_64 Ubuntu 26.04.1 LTS', 1,
      row.probe, row.pair, row.base, row.head, row.delta, row.pct, row.textEqual].join(',')), '',
  ].join('\n');
  return { markdown, csv };
}

function updateBlock(doc, markdown) {
  const start = doc.indexOf(begin), finish = doc.indexOf(end);
  requireThat(start >= 0 && finish > start && start === doc.lastIndexOf(begin) && finish === doc.lastIndexOf(end),
    `${stem}.md: expected exactly one ordered pair of generated-block markers`);
  const newline = doc.includes('\r\n') ? '\r\n' : '\n';
  return doc.slice(0, start) + markdown.replaceAll('\n', newline) + doc.slice(finish + end.length);
}

function main(args) {
  let mode, override;
  const usage = 'usage: node scripts/ghost_probe_stability_report.mjs [--write | --check] [--raw <path>]';
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if ((arg === '--write' || arg === '--check') && !mode) mode = arg;
    else if (arg === '--raw' && !override && args[i + 1] && !args[i + 1].startsWith('--')) override = args[++i];
    else throw new Error(usage);
  }
  requireThat(!(override && mode === '--write'), '--raw cannot be used with --write');
  const { markdown, csv } = generate(parse(readFileSync(override ?? raw, 'utf8')));
  if (!mode) {
    process.stdout.write(`${markdown}\n\n${csv}`);
    return;
  }
  const doc = readFileSync(report, 'utf8');
  const updated = updateBlock(doc, markdown);
  if (mode === '--write') {
    writeFileSync(report, updated);
    writeFileSync(summary, csv);
    console.log('Wrote generated Markdown block and summary CSV');
  } else {
    const savedCsv = readFileSync(summary, 'utf8').replaceAll('\r\n', '\n');
    const stale = [updated !== doc && `${stem}.md`, savedCsv !== csv && `${stem}_summary.csv`].filter(Boolean);
    requireThat(!stale.length, `--check: stale ${stale.join(', ')}; regenerate with --write`);
    console.log('--check ok: generated Markdown block and summary CSV match');
  }
}

try {
  main(process.argv.slice(2));
} catch (error) {
  console.error(`ghost_probe_stability_report: ${error.message}`);
  process.exitCode = 1;
}
