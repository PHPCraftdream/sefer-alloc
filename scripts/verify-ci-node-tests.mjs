// Verify every Node test file is selected by exactly one CI command.
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const defaultRoot = resolve(fileURLToPath(new URL('..', import.meta.url)));
const testNamePattern = /^[A-Za-z0-9_.-]+\.test\.mjs$/;

function ciFiles(repoRoot) {
  const github = join(repoRoot, '.github');
  const files = [];
  const workflows = join(github, 'workflows');
  for (const name of readdirSync(workflows).sort()) {
    if (name.endsWith('.yml')) files.push(join(workflows, name));
  }
  const actions = join(github, 'actions');
  for (const name of readdirSync(actions).sort()) {
    const file = join(actions, name, 'action.yml');
    try {
      if (statSync(file).isFile()) files.push(file);
    } catch (error) {
      if (error.code !== 'ENOENT' && error.code !== 'ENOTDIR') throw error;
    }
  }
  return files;
}

function expandAlternations(fragment) {
  let at = 0;
  const parseAlternatives = (nested = false) => {
    const options = [];
    let current = [''];
    while (at < fragment.length) {
      const char = fragment[at++];
      if (char === '|' && nested) {
        options.push(...current);
        current = [''];
      } else if (char === ')' && !nested) {
        throw new Error('unexpected ) in grep exclusion');
      } else if (char === ')' && nested) {
        options.push(...current);
        return options;
      } else if (char === '(') {
        const choices = parseAlternatives(true);
        current = current.flatMap((prefix) => choices.map((choice) => prefix + choice));
      } else if (char === '\\') {
        if (at >= fragment.length) throw new Error('trailing escape in grep exclusion');
        const literal = fragment[at++];
        current = current.map((prefix) => prefix + literal);
      } else {
        if (!/[A-Za-z0-9_.-]/.test(char)) throw new Error(`unsupported grep exclusion character: ${char}`);
        current = current.map((prefix) => prefix + char);
      }
    }
    if (nested) throw new Error('unclosed group in grep exclusion');
    options.push(...current);
    return options;
  };
  return parseAlternatives();
}

function excludedTestNames(line) {
  const grep = line.match(/\bgrep\s+-(\S+)/);
  if (!grep || !grep[1].includes('E') || !grep[1].includes('v')) return [];
  const arg = line.slice(grep.index + grep[0].length).trimStart();
  const quoted = arg.match(/^(['"])(.*?)\1/);
  if (!quoted) throw new Error(`grep -Ev must use a quoted exclusion pattern: ${line.trim()}`);
  const pattern = quoted[2];
  const prefix = '^tests/';
  const suffix = String.raw`\.test\.mjs$`;
  if (!pattern.startsWith(prefix) || !pattern.endsWith(suffix)) {
    throw new Error(`unsupported grep -Ev exclusion pattern: ${pattern}`);
  }
  return expandAlternations(pattern.slice(prefix.length, -suffix.length)).map((name) => `${name}.test.mjs`);
}

function rel(repoRoot, file) {
  return file.slice(repoRoot.length + 1).replaceAll('\\', '/');
}

export function computeCoverage(repoRoot = defaultRoot) {
  repoRoot = resolve(repoRoot);
  const testsDir = join(repoRoot, 'tests');
  const tests = readdirSync(testsDir).filter((name) => testNamePattern.test(name)).sort();
  const testSet = new Set(tests);
  const steps = [];
  const errors = [];
  const coverage = new Map(tests.map((name) => [name, []]));

  for (const file of ciFiles(repoRoot)) {
    const source = rel(repoRoot, file);
    const lines = readFileSync(file, 'utf8').split(/\r?\n/);
    lines.forEach((line, index) => {
      if (line.includes('*.test.mjs')) {
        let excluded;
        try {
          excluded = excludedTestNames(line);
        } catch (error) {
          errors.push(`${source}:${index + 1}: ${error.message}`);
          return;
        }
        for (const name of excluded) {
          if (!testSet.has(name)) errors.push(`${source}:${index + 1}: exclusion names nonexistent test tests/${name}`);
        }
        const omitted = new Set(excluded);
        const covered = tests.filter((name) => !omitted.has(name));
        const step = { source, line: index + 1, kind: 'glob', covered };
        steps.push(step);
        for (const name of covered) coverage.get(name).push(step);
        for (const name of excluded) {
          if (!testSet.has(name)) continue;
          if (!steps.some((other) => other !== step && other.covered.includes(name))) {
            // A later explicit step may cover this exclusion; validate after scanning all files.
          }
        }
      } else {
        const refs = [...line.matchAll(/\btests\/([A-Za-z0-9_.-]+\.test\.mjs)\b/g)].map((match) => match[1]);
        const missing = refs.filter((name) => !testSet.has(name));
        for (const name of missing) errors.push(`${source}:${index + 1}: explicit reference names nonexistent test tests/${name}`);
        const present = refs.filter((name) => testSet.has(name));
        if (present.length) {
          const step = { source, line: index + 1, kind: 'explicit', covered: present };
          steps.push(step);
          for (const name of present) coverage.get(name).push(step);
        }
      }
    });
  }

  for (const step of steps.filter((item) => item.kind === 'glob')) {
    const line = readFileSync(join(repoRoot, step.source), 'utf8').split(/\r?\n/)[step.line - 1];
    for (const name of excludedTestNames(line)) {
      if (testSet.has(name) && !steps.some((other) => other !== step && other.covered.includes(name))) {
        errors.push(`${step.source}:${step.line}: exclusion removes tests/${name}, which is not covered by another CI step`);
      }
    }
  }

  for (const name of tests) {
    const count = coverage.get(name).length;
    if (count === 0) errors.push(`tests/${name} runs 0 times in CI`);
    else if (count > 1) {
      const places = coverage.get(name).map((step) => `${step.source}:${step.line}`).join(', ');
      errors.push(`tests/${name} runs ${count} times in CI (${places})`);
    }
  }
  if (errors.length) throw new Error(errors.join('\n'));
  return {
    tests,
    steps: steps.map(({ source, line, kind, covered }) => ({ source, line, kind, covered: [...covered].sort() })),
    coverage: Object.fromEntries(tests.map((name) => [name, coverage.get(name).length])),
  };
}

export function main() {
  try {
    const result = computeCoverage();
    console.log(`[verify-ci-node-tests] ${result.tests.length} test files run exactly once`);
    for (const step of result.steps) {
      console.log(`  ${step.source}:${step.line} (${step.kind}): ${step.covered.join(', ')}`);
    }
  } catch (error) {
    console.error(`[verify-ci-node-tests] ${error.message}`);
    process.exitCode = 1;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
