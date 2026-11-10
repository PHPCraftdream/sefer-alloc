// Pure bounded Cargo JSON decoding; identity comes from the reviewed caller/toolchain.
import assert from 'node:assert/strict';
import { isAbsolute } from 'node:path';

export function compilerArtifact(bytes, expected) {
  assert(Buffer.isBuffer(bytes) && bytes.length > 0 && bytes.length <= 16 * 1024 * 1024,
    'Cargo message byte bound');
  assert(expected && typeof expected.packageId === 'string' && expected.packageId.length > 0,
    'root package identity missing');
  assert(isAbsolute(expected.sourcePath) && isAbsolute(expected.executablePath), 'artifact paths must be absolute');
  const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  const lines = text.split('\n').filter(line => line.trim());
  assert(lines.length > 0 && lines.length <= 50000, 'Cargo message count');
  const messages = lines.map(line => {
    assert(Buffer.byteLength(line) <= 256 * 1024, 'Cargo message line bound');
    const message = JSON.parse(line);
    assert(message && !Array.isArray(message) && typeof message.reason === 'string', 'Cargo message shape');
    return message;
  });
  const finished = messages.filter(message => message.reason === 'build-finished');
  assert(finished.length === 1 && finished[0].success === true && messages.at(-1) === finished[0],
    'Cargo build must finish successfully exactly once');
  const artifacts = messages.filter(message => message.reason === 'compiler-artifact'
    && message.package_id === expected.packageId && message.target?.name === 'g1_j13b_judge'
    && Array.isArray(message.target.kind) && message.target.kind.length === 1 && message.target.kind[0] === 'example');
  assert.equal(artifacts.length, 1, 'exactly one intended root example artifact required');
  const artifact = artifacts[0];
  assert.equal(artifact.target.src_path, expected.sourcePath, 'wrong example source path');
  assert.equal(artifact.executable, expected.executablePath, 'wrong example executable path');
  assert(Array.isArray(artifact.filenames) && artifact.filenames.includes(expected.executablePath),
    'executable must belong to the compiler artifact');
  assert(artifact.profile?.test === false && artifact.profile?.debug_assertions === false
    && artifact.profile?.opt_level === '3', 'expected controlled release profile');
  assert(Array.isArray(artifact.target.crate_types) && artifact.target.crate_types.length === 1
    && artifact.target.crate_types[0] === 'bin', 'expected executable crate type');
  assert(Array.isArray(artifact.features) && artifact.features.length > 0 && artifact.features.length <= 512,
    'resolved feature count');
  assert(artifact.features.every(feature => typeof feature === 'string' && /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/.test(feature)),
    'resolved feature names');
  assert.equal(new Set(artifact.features).size, artifact.features.length, 'duplicate resolved feature');
  assert(artifact.features.includes('production'), 'required production feature missing');
  return Object.freeze({ package_id: artifact.package_id, source_path: artifact.target.src_path,
    executable: artifact.executable, features: Object.freeze([...artifact.features].sort()),
    profile: Object.freeze({ test: false, debug_assertions: false, opt_level: '3' }) });
}
