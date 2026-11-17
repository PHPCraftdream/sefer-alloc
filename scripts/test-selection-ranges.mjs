#!/usr/bin/env node
// Read-only Git evidence capture for the historical conditional-range metric.
// `--capture` writes only compact repo-relative graph evidence into ranges.json.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const INPUTS = join(ROOT, 'docs/perf/test_selection_inputs');
const RANGES = join(INPUTS, 'ranges.json');
const SHA = /^[0-9a-f]{40}$/;
function git(args, allowFailure = false) {
  const r = spawnSync('git', args, { cwd: ROOT, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  if (r.status !== 0 && !allowFailure) throw new Error(`git ${args.join(' ')} failed: ${r.stderr}`);
  return r.status === 0 ? r.stdout.trim() : null;
}
function parents(sha) {
  const line = git(['show', '-s', '--format=%P', sha]);
  return line ? line.split(' ') : [];
}
function capture() {
  const runs = JSON.parse(readFileSync(join(INPUTS, 'runs.json'), 'utf8')).runs;
  let historyRef = null;
  for (const ref of ['origin/main', 'main']) {
    if (git(['rev-parse', '--verify', `${ref}^{commit}`], true) !== null) { historyRef = ref; break; }
  }
  if (!historyRef) throw new Error('neither origin/main nor main is an available commit ref');
  const firstParentHistory = git(['log', historyRef, '--first-parent', '--format=%H %P'])
    .split('\n').filter(Boolean).map((line) => line.split(' '));
  const firstParentCommits = new Set(firstParentHistory.map((row) => row[0]));
  const firstParentMerges = firstParentHistory.filter((row) => row.length === 3);
  const records = runs.map((run) => {
    const id = String(run.id), head = run.head_sha;
    if (!SHA.test(head) || git(['cat-file', '-e', `${head}^{commit}`], true) === null)
      throw new Error(`run ${id}: head commit is missing or invalid`);
    const headParents = parents(head);
    let base = null, method = null, evidence = null;
    if (run.event === 'push' && run.head_branch === 'main' && headParents.length === 2
      && firstParentCommits.has(head)) {
      base = headParents[0];
      method = 'push-merge-parent1';
      evidence = { headParents };
    } else if (run.event === 'pull_request') {
      const merge = firstParentMerges.find((row) => row[2] === head);
      if (merge) {
        const mergeParents = merge.slice(1);
        const mergeBase = git(['merge-base', head, mergeParents[0]], true);
        if (mergeBase && SHA.test(mergeBase)) {
          base = mergeBase;
          method = 'pr-merge-base';
          evidence = { merge: merge[0], mergeParents, mergeBase };
        }
      }
    }
    if (!base) return { id, head, status: 'unresolved', method: null, base: null, evidence: null, files: [] };
    if (!SHA.test(base) || git(['cat-file', '-e', `${base}^{commit}`], true) === null)
      throw new Error(`run ${id}: reconstructed base is missing`);
    const raw = git(['diff', '--name-only', base, head]);
    const files = [...new Set(raw.split('\n').filter(Boolean).map((p) => p.replace(/\\/g, '/')))].sort();
    if (files.some((p) => p.startsWith('/') || p.split('/').includes('..') || p.includes('\n') || p.includes('\r')))
      throw new Error(`run ${id}: non-repository-relative file path`);
    return { id, head, status: 'graph-reconstructed', method, base, evidence, files };
  });
  return { historyRef, records };
}

const args = process.argv.slice(2);
if (args.length === 1 && args[0] === '--capture') {
  const { historyRef, records } = capture();
  const lines = [
    '{',
    ' "schema": 1,',
    ' "historyRef": ' + JSON.stringify(historyRef) + ',',
    ' "note": "Graph-reconstructed conditional ranges; GitHub webhook event base is not retained, so these are not exact event-base proofs.",',
    ' "runs": [',
    ...records.map((record, i) => '  ' + JSON.stringify(record) + (i + 1 === records.length ? '' : ',')),
    ' ]',
    '}',
    '',
  ];
  writeFileSync(RANGES, lines.join('\n'));
  console.log(`captured ${records.length} runs from ${historyRef}: ${records.filter((r) => r.status === 'graph-reconstructed').length} graph-reconstructed, ${records.filter((r) => r.status === 'unresolved').length} unresolved`);
} else {
  console.error('usage: node scripts/test-selection-ranges.mjs --capture');
  process.exit(2);
}
