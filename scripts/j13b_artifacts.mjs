import { createHash } from 'node:crypto';
import { closeSync, fstatSync, lstatSync, openSync, readSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import { ARMS } from './j13b_schema.mjs';

const HASH = /^[a-f0-9]{64}$/;
const COMMIT = /^[a-f0-9]{40}$/;
const CANDIDATE_ARMS = ['d', 'n', 'b', 'a', 'ab'];
const ARTIFACT_FIELDS = [
  'path', 'variant', 'source_commit', 'source_sha256', 'binary_sha256', 'rustc', 'features',
  'original_source_sha256', 'overlay_sha256',
];

function object(value, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw Error(`${label} must be an object`);
}

function exactKeys(value, keys, label) {
  object(value, label);
  const actual = Object.keys(value);
  if (actual.length !== keys.length || keys.some(key => !Object.hasOwn(value, key))) {
    throw Error(`${label} fields differ from contract`);
  }
}

function nonempty(value, label) {
  if (typeof value !== 'string' || !value.trim() || value.includes('\0')) throw Error(`${label} missing or invalid`);
}

// Validate before resolving paths or spawning any child; return an owned immutable copy.
export function validateArtifactManifest(input) {
  exactKeys(input, ['schema', 'run_id', 'artifacts', 'arms'], 'artifact manifest');
  if (input.schema !== 'j13b.artifacts.v1') throw Error('unsupported artifact manifest schema');
  nonempty(input.run_id, 'run_id');
  exactKeys(input.arms, ARMS, 'arm routing');
  object(input.artifacts, 'artifacts');
  const ids = Object.keys(input.artifacts);
  if (ids.length !== 3 || ids.some(id => !/^[a-z][a-z0-9-]{0,63}$/.test(id))) {
    throw Error('exactly three named artifacts required');
  }
  const artifacts = Object.create(null);
  for (const id of ids) {
    const artifact = input.artifacts[id];
    exactKeys(artifact, ARTIFACT_FIELDS, `artifact ${id}`);
    nonempty(artifact.path, 'artifact path');
    nonempty(artifact.rustc, 'artifact rustc');
    nonempty(artifact.features, 'artifact features');
    if (typeof artifact.source_commit !== 'string' || !COMMIT.test(artifact.source_commit)) throw Error('artifact source_commit invalid');
    for (const key of ['source_sha256', 'binary_sha256']) {
      if (typeof artifact[key] !== 'string' || !HASH.test(artifact[key])) throw Error(`artifact ${key} invalid`);
    }
    if (!['candidate', 'baseline', 'static-control'].includes(artifact.variant)) throw Error('artifact variant invalid');
    if (artifact.variant === 'static-control') {
      for (const key of ['original_source_sha256', 'overlay_sha256']) {
        if (typeof artifact[key] !== 'string' || !HASH.test(artifact[key])) throw Error(`static-control ${key} invalid`);
      }
      if (artifact.original_source_sha256 === artifact.source_sha256) throw Error('static-control source must include overlay');
    } else if (artifact.original_source_sha256 !== null || artifact.overlay_sha256 !== null) {
      throw Error('non-overlay artifact must not claim overlay provenance');
    }
    artifacts[id] = Object.freeze({ ...artifact });
  }
  const arms = Object.create(null);
  for (const arm of ARMS) {
    const id = input.arms[arm];
    if (typeof id !== 'string' || !Object.hasOwn(artifacts, id)) throw Error(`artifact missing for arm ${arm}`);
    arms[arm] = id;
  }
  const candidate = arms.d;
  if (!CANDIDATE_ARMS.every(arm => arms[arm] === candidate)
    || artifacts[candidate].variant !== 'candidate') throw Error('candidate arms must share candidate artifact');
  const baseline = arms.base;
  if (arms.base2 !== baseline || arms.mi !== baseline || artifacts[baseline].variant !== 'baseline') {
    throw Error('base/base2/mi must share historical baseline artifact');
  }
  if (artifacts[arms.f].variant !== 'static-control') throw Error('f requires static-control artifact');
  const reference = artifacts[arms.f];
  if (reference.binary_sha256 === artifacts[candidate].binary_sha256) throw Error('static-control must have a distinct executable hash');
  if (reference.original_source_sha256 !== artifacts[candidate].source_sha256
    || reference.source_commit !== artifacts[candidate].source_commit
    || reference.rustc !== artifacts[candidate].rustc
    || reference.features !== artifacts[candidate].features) {
    throw Error('static-control must overlay the identical candidate source/toolchain/features');
  }
  // One path cannot claim multiple executable identities or variants.
  if (new Set(ids.map(id => artifacts[id].path)).size !== ids.length) throw Error('artifact paths must be distinct');
  return Object.freeze({ schema: input.schema, run_id: input.run_id, artifacts: Object.freeze(artifacts), arms: Object.freeze(arms) });
}

export function readArtifactManifest(path) {
  const fd = openSync(path, 'r');
  try {
    if (!fstatSync(fd).isFile()) throw Error('artifact manifest must be a regular file');
    const buffer = Buffer.alloc(64 * 1024 + 1);
    let used = 0;
    while (used < buffer.length) {
      const count = readSync(fd, buffer, used, buffer.length - used, null);
      if (count === 0) break;
      used += count;
    }
    if (used > 64 * 1024) throw Error('artifact manifest exceeds 64 KiB');
    const text = new TextDecoder('utf-8', { fatal: true }).decode(buffer.subarray(0, used));
    return validateArtifactManifest(JSON.parse(text));
  } finally {
    closeSync(fd);
  }
}

export function bindArtifactJobs(manifest, jobs, directory) {
  const checked = validateArtifactManifest(manifest);
  const paths = Object.values(checked.artifacts).map(artifact => resolve(directory, artifact.path));
  if (new Set(paths).size !== paths.length) throw Error('resolved artifact paths must be distinct');
  return jobs.map(job => {
    const id = checked.arms[job.arm];
    if (!id) throw Error('job arm is absent from artifact routing');
    const artifact = checked.artifacts[id];
    return Object.freeze({ ...job, executable: resolve(directory, artifact.path), artifact,
      artifact_id: id, run_id: checked.run_id });
  });
}

export function hashExecutable(path) {
  if (!isAbsolute(path)) throw Error('executable path must be absolute');
  if (!lstatSync(path).isFile()) throw Error('executable must be a regular non-symlink file');
  const fd = openSync(path, 'r');
  try {
    const before = fstatSync(fd);
    if (!before.isFile()) throw Error('executable must be a regular file');
    const hash = createHash('sha256');
    const buffer = Buffer.allocUnsafe(64 * 1024);
    let count;
    while ((count = readSync(fd, buffer, 0, buffer.length, null)) > 0) hash.update(buffer.subarray(0, count));
    const after = fstatSync(fd);
    if (before.size !== after.size || before.mtimeMs !== after.mtimeMs || before.ctimeMs !== after.ctimeMs) {
      throw Error('executable changed while hashing');
    }
    return hash.digest('hex');
  } finally {
    closeSync(fd);
  }
}

export function verifyJobArtifact(job) {
  if (!job.artifact || !job.artifact_id || !job.run_id) throw Error('job lacks immutable artifact binding');
  if (hashExecutable(job.executable) !== job.artifact.binary_sha256) throw Error('executable SHA256 mismatch');
}

export function verifyArtifactRecord(record, job) {
  const identity = record.identity;
  if (!identity || identity.run_id !== job.run_id || identity.artifact_id !== job.artifact_id) {
    throw Error('child artifact/run identity mismatch');
  }
  for (const key of ['source_sha256', 'binary_sha256', 'rustc', 'features', 'variant', 'source_commit',
    'original_source_sha256', 'overlay_sha256']) {
    if (identity[key] !== job.artifact[key]) throw Error(`child artifact ${key} mismatch`);
  }
}
