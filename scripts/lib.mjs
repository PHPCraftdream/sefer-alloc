// Shared helpers for the hardening-sweep runner scripts (tsan / loom / miri).
//
// These wrap the fiddly invocations we run before a release push so they are
// reproducible and don't re-learn the traps each time — most importantly the
// TSan-via-WSL path (RUSTC_WRAPPER inheritance, a separate Linux target dir,
// -Zbuild-std). Node is used only as a portable process launcher; there is no
// npm dependency graph (no node_modules).

import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

/** Absolute Windows path to the repo root (parent of scripts/). */
export const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/**
 * Convert a Windows path (`D:\dev\rust\sefer-alloc`) to its WSL mount path
 * (`/mnt/d/dev/rust/sefer-alloc`). We do this in JS rather than shelling out to
 * `wslpath` because `wslpath` needs careful backslash escaping through two
 * shell layers and silently mangles the path if it is wrong.
 */
export function winToWsl(winPath) {
  const m = /^([A-Za-z]):(.*)$/.exec(winPath);
  if (!m) throw new Error(`not an absolute Windows path: ${winPath}`);
  const drive = m[1].toLowerCase();
  const rest = m[2].replace(/\\/g, '/');
  return `/mnt/${drive}${rest}`;
}

/**
 * Run a command, streaming its combined output to this process's stdout AND
 * capturing it for post-run scanning. Resolves to { code, out }.
 */
export function run(cmd, args, opts = {}) {
  return new Promise((res, rej) => {
    const child = spawn(cmd, args, { ...opts });
    let out = '';
    const tee = (buf) => {
      const s = buf.toString();
      out += s;
      process.stdout.write(s);
    };
    child.stdout?.on('data', tee);
    child.stderr?.on('data', tee);
    child.on('error', rej);
    child.on('close', (code) => res({ code: code ?? 1, out }));
  });
}

/**
 * Scan cargo-test output and decide pass/fail. Fails on any `test result:
 * FAILED`, any `error[`/`error:` compile error, any explicit extra markers
 * (e.g. TSan's `ThreadSanitizer`/`data race`), or a non-zero process code with
 * no `test result: ok` at all. Prints a one-line verdict and returns a boolean.
 */
export function verdict(label, code, out, extraFailMarkers = []) {
  const failed = /test result: FAILED/.test(out);
  const compileErr = /^error(\[|:)/m.test(out);
  const extra = extraFailMarkers.filter((m) => out.includes(m));
  const anyOk = /test result: ok/.test(out);
  const ok =
    !failed && !compileErr && extra.length === 0 && (code === 0 || anyOk);
  if (ok) {
    console.log(`\n[${label}] PASS`);
  } else {
    console.log(
      `\n[${label}] FAIL` +
        (failed ? ' (test failure)' : '') +
        (compileErr ? ' (compile error)' : '') +
        (extra.length ? ` (markers: ${extra.join(', ')})` : '') +
        (!anyOk && code !== 0 ? ` (exit ${code}, no test ran)` : ''),
    );
  }
  return ok;
}
