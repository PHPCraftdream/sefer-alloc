import { closeSync, fsyncSync, openSync, writeSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { campaignSchedule, childKey, plannedRunCount } from './j13b_campaign.mjs';
import { CELL_MATRIX, validateRuntimeRow } from './j13b_schema.mjs';
import { bindArtifactJobs, readArtifactManifest, verifyArtifactRecord, verifyJobArtifact } from './j13b_artifacts.mjs';

const CELLS = new Map(CELL_MATRIX.map(cell => [cell.id, cell]));
const STRICT_UTF8 = new TextDecoder('utf-8', { fatal: true });

function decodeRecords(bytes, job, cell) {
  if (bytes.length === 0 || bytes[bytes.length - 1] !== 10) throw Error('child output lacks JSONL line terminator');
  let text;
  try {
    text = STRICT_UTF8.decode(bytes);
  } catch {
    throw Error('child stdout is not valid UTF-8; raw bytes preserved');
  }
  const lines = text.slice(0, -1).split(/\r?\n/);
  if (lines.length !== cell.windows.length) {
    throw Error(`expected ${cell.windows.length} window records, received ${lines.length}`);
  }
  const records = lines.map((line, index) => {
    try {
      return JSON.parse(line);
    } catch (error) {
      throw Error(`child record ${index + 1}: invalid JSON: ${error.message}`);
    }
  });
  const windows = records.map(record => record.window);
  if (new Set(windows).size !== windows.length || !cell.windows.every(window => windows.includes(window))) {
    throw Error('child windows differ from the canonical cell windows');
  }
  for (const record of records) {
    const same = record.cell_id === job.cell_id && record.arm === job.arm
      && record.block === job.block && record.repeat === job.repeat;
    if (!same) throw Error('child output identity differs from dispatched tuple');
    if (record.status !== 'sample' || record.workload_executed !== true) throw Error('child unavailable: runtime measurement refused');
  }
  const invocations = new Set(records.map(record => record.identity?.invocation_id));
  if (invocations.size !== 1 || ![...invocations][0]) throw Error('child records must share one invocation_id');
  return { records, invocation: [...invocations][0] };
}

export function executeChildJobs({ jobs, spawn = spawnSync, sink, validate = validateRuntimeRow,
  verifyArtifact = verifyJobArtifact, verifyRecord = verifyArtifactRecord }) {
  const pending = new Set(jobs.map(childKey));
  const invocations = new Set();
  const staged = [];
  for (const job of jobs) {
    const cell = CELLS.get(job.cell_id);
    if (!cell || !cell.arms.includes(job.arm)) throw Error('job not present in explicit cell matrix');
    if (!pending.delete(childKey(job))) throw Error('duplicate scheduled child');
    verifyArtifact(job);
    const { path: artifactPath, ...provenance } = job.artifact ?? {};
    const child = spawn(
      job.executable,
      [job.cell_id, job.arm, String(job.block), String(job.repeat)],
      { encoding: 'buffer', timeout: 120000, maxBuffer: 4 * 1024 * 1024, windowsHide: true,
        env: { ...process.env, SEFER_J13B_ARTIFACT: JSON.stringify({
          ...provenance, run_id: job.run_id, artifact_id: job.artifact_id,
        }) } },
    );
    const bytes = child.stdout;
    if (bytes && bytes.length > 0) sink(bytes);
    verifyArtifact(job);
    if (child.error) throw child.error;
    if (!bytes) throw Error('child stdout capture absent; nothing was preserved');
    if (child.status !== 0) throw Error(`child failed with exit=${child.status}`);
    const { records, invocation } = decodeRecords(bytes, job, cell);
    for (const record of records) verifyRecord(record, job);
    if (invocations.has(invocation)) throw Error('invocation_id reused across children');
    invocations.add(invocation);
    staged.push(...records);
  }
  for (const record of staged) validate(record);
  return staged;
}

function main() {
  const [manifestPath, output, ...extra] = process.argv.slice(2);
  if (!manifestPath || !output || extra.length) {
    console.error('usage: node scripts/j13b_run.mjs <artifacts.json> <raw.jsonl>');
    process.exitCode = 2;
    return;
  }
  let fd;
  try {
    const manifest = readArtifactManifest(resolve(manifestPath));
    const jobs = bindArtifactJobs(manifest, campaignSchedule(), dirname(resolve(manifestPath)));
    if (jobs.length !== plannedRunCount()) throw Error('scheduled child count differs from fixed matrix');
    for (const job of jobs.filter((job, index, all) => all.findIndex(other => other.artifact_id === job.artifact_id) === index)) {
      verifyJobArtifact(job);
    }
    fd = openSync(resolve(output), 'wx');
    executeChildJobs({
      jobs,
      sink: bytes => {
        writeSync(fd, bytes);
        fsyncSync(fd);
      },
    });
  } catch (error) {
    console.error(`[j13b_run] stopped fail-closed; emitted stdout preserved: ${error.message}`);
    process.exitCode = 1;
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
