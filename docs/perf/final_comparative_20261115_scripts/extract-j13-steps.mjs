// Extracts the verbatim `run: |` blocks of the registered J13 study (build+receipt, measurement) from the CI sources.
// Usage: node extract-j13-steps.mjs <repo-root> <out-dir>
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';
const [root, out] = process.argv.slice(2);
mkdirSync(out, { recursive: true });
function block(file, stepName) {
  const lines = readFileSync(join(root, file), 'utf8').split('\n');
  const at = lines.findIndex(l => l.includes(`- name: ${stepName}`));
  if (at < 0) throw new Error(`step not found: ${stepName}`);
  const runAt = lines.findIndex((l, i) => i > at && /^\s+run: \|\s*$/.test(l));
  const indent = lines[runAt].match(/^\s*/)[0].length;
  const body = [];
  for (let i = runAt + 1; i < lines.length; i++) {
    const l = lines[i];
    if (l.trim() !== '' && l.match(/^\s*/)[0].length <= indent) break;
    body.push(l.length >= indent + 2 ? l.slice(indent + 2) : '');
  }
  while (body.length && body[body.length - 1] === '') body.pop();
  return body.join('\n') + '\n';
}
const build = block('.github/actions/ci-j13b-judge-preparation/action.yml', 'G1 limited-study single CI build and provenance receipt (not measurement admission)');
const measure = block('.github/workflows/ci.yml', 'Manual J13 measurement (owner-approved protocol; never automatic)');
writeFileSync(join(out, 'j13-build.sh'), '#!/usr/bin/env bash\n' + build);
writeFileSync(join(out, 'j13-measure.sh'), '#!/usr/bin/env bash\n' + measure);
console.log(`build: ${build.split('\n').length} lines, measure: ${measure.split('\n').length} lines`);
