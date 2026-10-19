import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import {
  appendFileSync, closeSync, fstatSync, mkdirSync, openSync,
  readFileSync, readSync, rmSync, writeFileSync,
} from 'node:fs';
import { join, resolve } from 'node:path';
import { spawn } from 'node:child_process';
import { evaluateStep } from '../scripts/r20_evidence_reverify_support.mjs';

const ROOT = resolve(import.meta.dirname, '..');
const SCRATCH = join(ROOT, `.rush/evidence-reverify-runner-selftest-${process.pid}-${Date.now()}`);
const hash = (value) => createHash('sha256').update(value).digest('hex');

function run(step, id) {
  return new Promise((resolveRun) => {
    const log = join(SCRATCH, `${id}.log`);
    const fd = openSync(log, 'a+');
    const start = fstatSync(fd).size;
    let error = null;
    let child;
    try {
      child = spawn(step.argv[0], step.argv.slice(1), { cwd: ROOT, stdio: ['ignore', fd, fd] });
    } catch (cause) {
      closeSync(fd);
      resolveRun({ exit: null, signal: null, error: cause.message, output: '' });
      return;
    }
    child.on('error', (cause) => { error = cause.message; });
    child.on('close', (exit, signal) => {
      const end = fstatSync(fd).size;
      closeSync(fd);
      const buf = Buffer.alloc(end - start);
      const input = openSync(log, 'r');
      let offset = 0;
      while (offset < buf.length) offset += readSync(input, buf, offset, buf.length - offset, start + offset);
      closeSync(input);
      resolveRun({ exit, signal, error, output: buf.toString('utf8') });
    });
  });
}

async function main() {
  rmSync(SCRATCH, { recursive: true, force: true });
  mkdirSync(SCRATCH, { recursive: true });
  try {
    const signalExpect = { mutant: 'signal', stdoutRegex: ['running 1 test',
      "process (?:did not|didn't) exit successfully[^\n]*(STATUS_ACCESS_VIOLATION|0xc0000005|signal: (?:11|7|6), SIG(SEGV|BUS|ABRT))"] };
    const crash = await run({ argv: [process.execPath, '-e',
      'process.stdout.write("running 1 test\\n"); process.stderr.write("error: test failed\\nCaused by: process didn\'t exit successfully: (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)\\n"); process.exit(3221225477)'] }, 'signal');
    assert.equal(crash.output.includes('running 1 test'), true);
    assert.equal(crash.output.includes("process didn't exit successfully"), true);

assert.equal(evaluateStep(signalExpect, { ...crash, exit: 101, cargoBuildPassed: true },
  crash.output).passed, true);

    // Linux cargo crash form (exit 101, cargo build witness, SIGSEGV receipt).
    const linuxCrashOutput = 'running 1 test\nerror: test failed\nCaused by: ' +
      "process didn't exit successfully: `cargo test --test loom_fixture` (signal: 11, SIGSEGV: invalid memory reference)";
    assert.equal(evaluateStep(signalExpect, { exit: 101, signal: null, error: null,
      cargoBuildPassed: true }, linuxCrashOutput).passed, true,
      'Linux cargo signal: 11 SIGSEGV receipt must pass with build witness');
    assert.equal(evaluateStep(signalExpect, { exit: 101, signal: null, error: null,
      cargoBuildPassed: false }, linuxCrashOutput).passed, false,
      'Linux crash receipt without build witness must not pass');
    const linuxAbortOutput = "running 1 test\nCaused by: process didn't exit successfully: `cargo test` (signal: 6, SIGABRT: abort)";
    assert.equal(evaluateStep(signalExpect, { exit: 101, signal: null, error: null,
      cargoBuildPassed: true }, linuxAbortOutput).passed, true);

    const earlyCargoFailure = { exit: 101, signal: null, error: null,
      output: 'error: could not compile `mutant`\n' };
    assert.equal(evaluateStep(signalExpect, earlyCargoFailure, earlyCargoFailure.output).passed, false);
    const cargoWrapper = { exit: 101, signal: null, error: null, cargoBuildPassed: true,
      output: 'running 1 test\nassertion failed' };

    assert.equal(evaluateStep(signalExpect, cargoWrapper, cargoWrapper.output).passed, false,
      'Cargo wrapper 101 alone is not process signal receipt');

    const nativeExitOutput = 'running 1 test\nerror: test failed\nCaused by: process didn\'t exit successfully: x (exit code: 0xc0000005, STATUS_ACCESS_VIOLATION)';
    assert.equal(evaluateStep(signalExpect, { exit: 3221225477, signal: null, error: null,
      output: nativeExitOutput }, nativeExitOutput).passed, true);
    assert.equal(evaluateStep(signalExpect, { exit: -1073741819, signal: null, error: null,
      output: nativeExitOutput }, nativeExitOutput).passed, true);

    for (const failure of [
      { exit: 124, signal: null, error: null, output: 'running 1 test runtime-crash-marker' },
      { exit: null, signal: 'SIGTERM', error: null, output: 'running 1 test runtime-crash-marker' },
      { exit: null, signal: 'SIGKILL', error: null, output: 'running 1 test runtime-crash-marker' },
      { exit: null, signal: null, error: 'spawn ENOENT', output: 'running 1 test runtime-crash-marker' },
    ]) assert.equal(evaluateStep(signalExpect, failure, failure.output).passed, false);

    const nonzeroExpect = { exitNonZero: true, stdoutRegex: ['assertion failed'] };
    const assertion = await run({ argv: [process.execPath, '-e',
      'process.stdout.write("assertion failed\\n"); process.exit(101)'] }, 'nonzero');
    assert.equal(assertion.exit, 101);
    assert.equal(assertion.output.includes('assertion failed'), true);
    assert.equal(evaluateStep(nonzeroExpect,
      { exit: assertion.exit, signal: null, error: null }, assertion.output).passed, true,
      'nonzero witness must pass evaluateStep');
    assert.equal(evaluateStep(nonzeroExpect,
      { exit: 0, signal: null, error: null }, assertion.output).passed, false,
      'exit 0 must fail an exitNonZero expectation');
    assert.notEqual(hash(assertion.output), hash(''));

  } finally {
    rmSync(SCRATCH, { recursive: true, force: true });
  }
}

await main();
console.log('PASS evidence runner signal/nonzero/failure selftest');
