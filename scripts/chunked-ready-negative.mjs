// CI-only counterfactual: реальный ready Acquire заменяется Relaxed, затем восстановление.
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

assert.equal(process.env.GITHUB_ACTIONS, 'true', 'negative control исполняется только GitHub CI');
const root = fileURLToPath(new URL('../', import.meta.url));
const source = new URL('../src/alloc_core/segment/remote_bitmap/sidecar_bitmap/chunked_bank.rs', import.meta.url);
const original = readFileSync(source, 'utf8');
const needle = 'self.ready[chunk / 64].load(Ordering::Acquire)';
assert.equal(original.split(needle).length, 2, 'ровно одна load-gate точка мутации');
const mutant = original.replace(needle, 'self.ready[chunk / 64].load(Ordering::Relaxed)');
let result;
try {
  writeFileSync(source, mutant);
  result = spawnSync('cargo', ['test', '--locked', '--features',
    'alloc-core alloc-xthread tagged-index-stack/loom', '--test', 'loom_chunked_sidecar_actual',
    'ready_acquire_protects_observer_without_allocation_handoff', '--', '--exact'], {
    cwd: root, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024,
    env: { ...process.env, RUSTFLAGS: '--cfg loom' },
  });
} finally {
  writeFileSync(source, original);
  assert.equal(readFileSync(source, 'utf8'), original, 'source восстановлен побайтно');
}
if (result.error) throw result.error;
const output = (result.stdout ?? '') + (result.stderr ?? '');
process.stdout.write(output);
assert.equal(result.signal, null, 'ожидается assertion Loom, не crash/timeout');
assert.notEqual(result.status, 0, 'Relaxed mutant обязан быть RED');
assert.match(output, /ready_acquire_protects_observer_without_allocation_handoff.*FAILED/);
assert.match(output, /Causality violation/);
assert.match(output, /test result: FAILED/);
console.log('[chunked] ready-relaxed rejected by tracked storage');
