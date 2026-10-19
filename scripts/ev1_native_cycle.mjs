import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const TARGET = 'D:/dev/rust/sefer-alloc/.rush/cargo-target-C';
const SCRATCH = join(ROOT, '.rush/native-mutation');
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const args = process.argv.slice(2);
if (args.length !== 1 || !['R4c-16', 'R5b-19'].includes(args[0])) {
  throw Error('usage: node scripts/ev1_native_cycle.mjs R4c-16|R5b-19');
}
const id = args[0];
const recipePath = join(ROOT, 'docs/evidence/reverify-recipes', `${id}.json`);
const recipe = JSON.parse(readFileSync(recipePath, 'utf8'));
const mutant = recipe.mutant;
if (!mutant || !mutant.file || !mutant.find || !mutant.replace) throw Error('missing mutant recipe');
const sourcePath = resolve(ROOT, mutant.file);
if (!sourcePath.startsWith(`${ROOT}/`) && !sourcePath.startsWith(`${ROOT}\\`)) throw Error('unsafe source path');
const original = readFileSync(sourcePath);
const normalized = original.toString('utf8').replace(/\r\n?/g, '\n');
const find = mutant.find.replace(/\r\n?/g, '\n');
const count = normalized.split(find).length - 1;
if (count !== 1) throw Error(`mutation fragment count ${count}`);
const mutated = Buffer.from(normalized.replace(find, mutant.replace.replace(/\r\n?/g, '\n')));
mkdirSync(SCRATCH, { recursive: true });
const logPath = join(SCRATCH, `${id}.json`);
const run = (label, argv) => {
  const command = ['test', ...argv];
  const result = spawnSync('cargo', command, {
    cwd: ROOT,
    encoding: 'utf8',
    timeout: 600_000,
    env: { ...process.env, CARGO_TARGET_DIR: TARGET },
  });
  const record = {
    label, command: ['cargo', ...command], exit: result.status,
    signal: result.signal, error: result.error?.message || null,
    stdout: result.stdout || '', stderr: result.stderr || '', timedOut: result.error?.code === 'ETIMEDOUT',
  };
  writeFileSync(logPath, `${JSON.stringify(record, null, 2)}\n`, { flag: 'a' });
  console.log(JSON.stringify({ id, label, exit: record.exit, signal: record.signal, error: record.error,
    timedOut: record.timedOut, stdout: record.stdout, stderr: record.stderr }));
  return record;
};
const argsFor = (step, baseline = false) => {
  const args = step.argv.slice(2);
  const sep = args.indexOf('--');
  if (sep >= 0) args.splice(sep);
  const noRun = args.indexOf('--no-run');
  if (noRun >= 0) args.splice(noRun, 1);
  if (!baseline && id === 'R4c-16') args.push('--', '--nocapture');
  return args;
};
let mutation = null;
try {
  writeFileSync(sourcePath, mutated);
  const steps = mutant.steps;
  for (const step of steps) {
    if (step.argv.includes('--no-run')) {
      mutation = run(`mutant:${step.name}`, argsFor(step));
      if (mutation.exit !== 0) break;
      continue;
    }
    mutation = run(`mutant:${step.name}`, argsFor(step));
    break;
  }
} finally {
  writeFileSync(sourcePath, original);
}
const restoredHash = hash(readFileSync(sourcePath));
if (restoredHash !== hash(original)) throw Error(`restore SHA mismatch ${restoredHash}`);
const baseline = run('restored-baseline', argsFor(recipe.steps[0], true));
const finalRecord = { id, originalSha256: hash(original), restoredSha256: restoredHash,
  mutantExit: mutation?.exit ?? null, baselineExit: baseline.exit };
writeFileSync(logPath, `${JSON.stringify(finalRecord, null, 2)}\n`, { flag: 'a' });
console.log(JSON.stringify(finalRecord));
if (mutation?.exit === 0 || mutation?.exit === null || baseline.exit !== 0 || /running 0 tests/.test(mutation?.stdout || '') || /running 0 tests/.test(baseline.stdout || '')) process.exitCode = 1;
