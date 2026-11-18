const commands = new Map([
  ['test ()', /^cargo test {1,2}--no-fail-fast$/],
  ['test (--features experimental)', /^cargo test --features experimental {1,2}--no-fail-fast$/],
  ['test (--features alloc-core)', /^cargo test --features "alloc-core internals" {1,2}--no-fail-fast$/],
]);
const timestamp = /^\uFEFF?\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z /;
const ansi = /\u001b\[[0-?]*[ -/]*[@-~]/g;
const compileError = /^error: (?:could not compile |failed to (?:compile |run custom build command for |get .+ as a dependency|select a version for |(?:load|parse|read) manifest)|the lock file .+ needs to be updated)/;

function sourcePath(value) {
  const path = value.replaceAll('\\', '/').replace(/^\.\//, '');
  return /^(?:[\w.-]+\/)*[\w.-]+\.rs$/.test(path)
    && !path.split('/').some(part => part === '.' || part === '..') ? path : null;
}

function target(selector) {
  if (selector === '--lib') return { kind: 'lib', name: 'lib' };
  if (selector === '--doc') return { kind: 'doctest', name: 'doc' };
  const named = /^--(test|bin|example|bench) ([\w.-]+)$/.exec(selector);
  if (!named) return null;
  return { kind: named[1] === 'test' ? 'integration' : 'other', name: named[2] };
}

function reduceStep(lines) {
  const receipts = new Map();
  const summary = new Set();
  let active = null, count = null, exit = null;
  let started = false, compileFailed = false, inconsistent = false;
  const unresolved = reason => ({ status: 'unresolved', reason,
    ...(receipts.size ? { failures: [...receipts.values()] } : {}) });

  for (const line of lines) {
    const processExit = /^##\[error\]Process completed with exit code (\d+)\.$/.exec(line);
    if (processExit) {
      exit = Number(processExit[1]);
      break;
    }
    if (/^(?:running \d+ tests?$|test result: (?:FAILED|ok)\.)/.test(line)) {
      started = true;
      continue;
    }
    if (count !== null) {
      if (!line.trim()) continue;
      const item = /^ +([`'])([^`']+)\1$/.exec(line);
      if (!item || !target(item[2]) || summary.has(item[2])) inconsistent = true;
      else summary.add(item[2]);
      continue;
    }
    const aggregate = /^error: (\d+) targets? failed:$/.exec(line);
    if (aggregate) {
      count = Number(aggregate[1]);
      continue;
    }
    const running = /^\s*Running (unittests )?(.+) \([^)]+\)$/.exec(line);
    const doc = /^\s*Doc-tests [\w.-]+$/.test(line);
    if (running || doc) {
      active = doc ? { kind: 'doctest' } : {
        kind: running[1] ? 'unit' : 'integration', path: sourcePath(running[2]),
      };
      started = true;
      continue;
    }
    if (/^error: (?:test|doctest) failed\b/.test(line)) {
      const receipt = /^error: (test|doctest) failed, to rerun pass ([`'])([^`']+)\2$/.exec(line);
      const selector = receipt?.[3];
      const failure = selector && target(selector);
      if (!failure || (receipt[1] === 'doctest') !== (failure.kind === 'doctest')) {
        inconsistent = true;
      } else {
        const expected = failure.kind === 'lib' || failure.kind === 'other' ? 'unit' : failure.kind;
        if (active?.kind !== expected) inconsistent = true;
        if (failure.kind === 'integration') {
          if (active?.kind === expected && active.path) failure.path = active.path;
          else inconsistent = true;
        }
        if (receipts.has(selector)) {
          inconsistent = true;
          const previous = receipts.get(selector);
          if (previous.path && failure.path && previous.path !== failure.path) delete previous.path;
        } else receipts.set(selector, failure);
      }
      active = null;
      started = true;
      continue;
    }
    if (compileError.test(line)) compileFailed = true;
  }

  if (exit !== 101) return unresolved('missing Cargo failure exit');
  if (compileFailed && !started && !inconsistent && count === null) {
    return { status: 'pre-test', reason: 'Cargo compilation or setup failed' };
  }
  if (inconsistent || compileFailed || !receipts.size || count !== receipts.size
      || count !== summary.size || [...receipts.keys()].some(selector => !summary.has(selector))) {
    return unresolved('Cargo failure receipts and summary do not reconcile');
  }
  return { status: 'test-failures', failures: [...receipts.values()] };
}

export function parseCoreLog(log, rows) {
  const steps = new Map();
  let active;
  let position = 0;
  for (const raw of String(log ?? '').split(/\r?\n/)) {
    const line = raw.replace(timestamp, '').replace(ansi, '').trimEnd();
    const command = /^##\[group\]Run (.*)$/.exec(line)?.[1];
    if (command !== undefined || /^##\[(?:start|end)-action\b/.test(line)) {
      active = undefined;
      if (command === undefined) continue;
      for (const [name, pattern] of commands) {
        if (!pattern.test(command)) continue;
        const step = { position: position++, lines: [] };
        if (!steps.has(name)) steps.set(name, []);
        steps.get(name).push(step);
        active = step;
        break;
      }
    } else if (active) active.lines.push(line);
  }

  const misordered = new Set();
  const matched = rows.filter(row => steps.get(row.name)?.length === 1);
  for (const [i, row] of matched.entries()) {
    for (let j = 0; j < i; j++) {
      if (steps.get(matched[j].name)[0].position > steps.get(row.name)[0].position) {
        misordered.add(row.name);
        misordered.add(matched[j].name);
      }
    }
  }
  return rows.map(row => {
    if (row.conclusion !== 'failure') return row;
    const matches = steps.get(row.name);
    if (matches?.length !== 1) return { ...row, evidence: { status: 'unresolved',
      reason: matches ? 'duplicate core test command' : 'missing exact core test command' } };
    const evidence = reduceStep(matches[0].lines);
    return { ...row, evidence: misordered.has(row.name)
      ? { ...evidence, status: 'unresolved', reason: 'core test commands out of order' } : evidence };
  });
}
