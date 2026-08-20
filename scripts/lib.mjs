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

// Task #1232 (heartbeat constants): a child may print NOTHING for its whole
// run — libtest emits `test <name> ... ok` only when a test FINISHES, so a
// binary with one long test (the gate's ~130 s alloc_core_differential step)
// is silent for over two minutes while the gate's fast steps each finish in
// well under a second. A human watching the gate cannot tell "working" from
// "hung", and the gate was abandoned mid-run four times in one session for
// exactly that reason. 10 s to the FIRST heartbeat sits far above every
// chatty step's inter-output gap (cargo prints per-crate lines continuously
// while building) and far below a watching human's patience; 10 s between
// repeats bounds the 130 s silent step to ~13 lines locally. Both are
// deliberately the same number so the line's "no output for Ns" reading
// stays simple to predict.
const HEARTBEAT_SILENCE_MS = 10_000;
const HEARTBEAT_EVERY_MS = 10_000;
const HEARTBEAT_TICK_MS = 1_000;

/**
 * Run a command, streaming its combined output to this process's stdout AND
 * capturing it for post-run scanning. Resolves to { code, out }.
 *
 * ## Heartbeat (task #1232)
 *
 * While the child has produced NO output for HEARTBEAT_SILENCE_MS, a
 * `[heartbeat] ...` line is written to THIS process's stdout every
 * HEARTBEAT_EVERY_MS, saying the step is still running, how long it has run,
 * and how long it has been silent — so a long silent step reads as alive
 * instead of hung.
 *
 * The heartbeat goes to `process.stdout` ONLY and is NEVER appended to `out`:
 * `out` is scanned by verdict() (test result: FAILED, ^error[, ^error:, and
 * caller-supplied markers like TSan's `WARNING: ThreadSanitizer`/`data
 * race` and ASan's marker list) and directly by callers (check-all.mjs's
 * expectWork `Checking|Compiling|Documenting <crate>` and expectTest
 * `test <name> ... ok` scans, staleArtifactDiagnosis's `panicked at` /
 * `kind: NotFound` pair, and argv-roundtrip-test.mjs's JSON.parse(out),
 * which requires `out` to be EXACTLY the child's own JSON). The wording
 * below ("still running", "elapsed", "no output for") deliberately contains
 * none of those markers either — belt and braces, so even a future refactor
 * that (incorrectly) routed the heartbeat through the tee would not flip a
 * passing step to FAIL.
 *
 * Enabled by default ONLY when this process's stdout is a TTY
 * (`process.stdout.isTTY`): in CI or under redirection the heartbeat would
 * be one line every 10 s for a ~25-minute gate (~150 lines) in a log nobody
 * watches live — noise with no local-interactive benefit. SEFER_HEARTBEAT=1
 * opts IN (a watched CI run, demonstrations), =0 opts OUT; an explicit
 * `opts.heartbeat` boolean beats the env var for a single call. The
 * heartbeat is also disabled when the child's own stdio is not observable
 * (`child.stdout`/`child.stderr` missing, e.g. stdio:'inherit' — no current
 * caller): unable to see the child's output, "no output" would be a claim
 * we cannot make.
 */
export function run(cmd, args, opts = {}) {
  if (opts.shell === true) {
    throw new Error(
      'run(): shell:true is forbidden — see scripts/lib.mjs\'s run() doc comment ' +
        'for why; if you genuinely need shell syntax, that is a deliberate, ' +
        'separate decision this project has not needed yet',
    );
  }
  return new Promise((res, rej) => {
    // `opts` defaults to `{}` (no `shell`), so `spawn` hands `args` straight
    // to the OS process-creation call as a real argv array — no shell
    // re-parses them, so a multi-word argument (e.g. `--features 'production
    // alloc-stats bench-internals'`) survives as ONE argv element with zero
    // quoting. Callers must NOT pass `shell: true`: Node 22+ (DEP0190) made
    // that path concatenate argv RAW and silently split multi-word args on
    // whitespace, and hand-rolled cross-shell quoting is the exact fragility
    // this `shell: false` default exists to avoid. See
    // scripts/argv-roundtrip-test.mjs for the regression test.
    const child = spawn(cmd, args, opts);
    let out = '';
    let lastOutputAt = Date.now();
    const startedAt = lastOutputAt;
    const tee = (buf) => {
      const s = buf.toString();
      out += s;
      lastOutputAt = Date.now();
      process.stdout.write(s);
    };
    child.stdout?.on('data', tee);
    child.stderr?.on('data', tee);

    // Heartbeat (task #1232) — see run()'s doc comment above for the full
    // design: stdout-only (never `out`), TTY-gated with env/opt overrides,
    // and only when the child's output is observable at all.
    const heartbeatWanted =
      typeof opts.heartbeat === 'boolean'
        ? opts.heartbeat
        : process.env.SEFER_HEARTBEAT === '1'
          ? true
          : process.env.SEFER_HEARTBEAT === '0'
            ? false
            : process.stdout.isTTY === true;
    const rawLabel = [cmd, ...args].join(' ');
    const label = rawLabel.length > 72 ? `${rawLabel.slice(0, 69)}...` : rawLabel;
    let beat = null;
    if (heartbeatWanted && child.stdout && child.stderr) {
      let lastBeatAt = 0;
      beat = setInterval(() => {
        const now = Date.now();
        if (
          now - lastOutputAt >= HEARTBEAT_SILENCE_MS &&
          now - lastBeatAt >= HEARTBEAT_EVERY_MS
        ) {
          lastBeatAt = now;
          const line =
            `[heartbeat] ${label} — still running, ` +
            `${Math.floor((now - startedAt) / 1000)}s elapsed, ` +
            `no output for ${Math.floor((now - lastOutputAt) / 1000)}s\n`;
          // Straight to process.stdout, NEVER through `tee` (see the doc
          // comment: `out` is verdict()/caller-scanned). If the child's last
          // chunk did not end in a newline, start a fresh line first so the
          // heartbeat cannot visually split the child's own output; the
          // newline is display-only and likewise never enters `out`.
          process.stdout.write(out.length > 0 && !out.endsWith('\n') ? `\n${line}` : line);
        }
      }, HEARTBEAT_TICK_MS);
    }
    // Cleared on BOTH settle paths — an uncleared interval keeps the Node
    // event loop (and therefore the whole gate) alive forever, which is
    // strictly worse than the silence this heartbeat exists to cure.
    const stopHeartbeat = () => {
      if (beat !== null) {
        clearInterval(beat);
        beat = null;
      }
    };
    child.on('error', (e) => {
      stopHeartbeat();
      rej(e);
    });
    child.on('close', (code) => {
      stopHeartbeat();
      res({ code: code ?? 1, out });
    });
  });
}

/**
 * Task #1232 (half two): recognize STATUS_DLL_INIT_FAILED. link.exe died
 * with 0xC0000142 twice in one session (sccache `Cache errors 0` both times,
 * so not cache corruption) — an environmental Windows loader failure under
 * memory pressure or antivirus interference, NOT a defect in the code being
 * linked. Returns a one-line diagnostic for exactly that exit code, `null`
 * for every other — advisory only, printed IN ADDITION to a step's own
 * failure, never instead of it (the same contract as
 * staleArtifactDiagnosis).
 *
 * How the code is checked: verified on the dev host (Node v24.12.0) that a
 * child exiting with that NTSTATUS surfaces in run()'s `close` as
 * 3221225794 UNSIGNED with signal null — including when the dying process
 * passes the SIGNED form (-1073741502) to ExitProcess — so an exact
 * equality on the unsigned value is the right check, the negative form is
 * deliberately NOT also matched (it cannot occur there), and the signal
 * field is not involved. This verifies the surfacing MECHANISM (an
 * ExitProcess with that DWORD), not the link.exe failure itself, which is
 * environmental and not reproducible on demand.
 *
 * What this deliberately does NOT do: retry. A retry that hides a real,
 * reproducible failure is strictly worse than a loud stop — the "guard made
 * cleverer and consequently claiming properties it does not have" class of
 * #1113/#1121/#1126/#1131. The caller still fails the step and stops the
 * gate; a human re-runs.
 */
export function dllInitFailedDiagnosis(code) {
  const STATUS_DLL_INIT_FAILED = 3221225794; // 0xC0000142, unsigned
  if (code !== STATUS_DLL_INIT_FAILED) return null;
  return (
    `exit code ${code} (0xC0000142, STATUS_DLL_INIT_FAILED) — a known ` +
    'ENVIRONMENTAL failure class on Windows: link.exe\'s process started but a ' +
    'DLL\'s loader initialization failed, typically under memory pressure or ' +
    'antivirus interference. This is NOT evidence of a defect in the code ' +
    'being linked (sccache reported \'Cache errors 0\' when last observed). No ' +
    'automatic retry is performed — the step FAILS and the gate stops here. ' +
    'Re-run the gate yourself; if it reproduces, suspect the environment (free ' +
    'memory, antivirus scanning the toolchain or the shared CARGO_TARGET_DIR), ' +
    'not your diff. Advisory only, printed in addition to the step\'s own ' +
    'failure — this diagnosis cannot PROVE the failure was environmental, and ' +
    'a genuinely reproducible DLL-init defect in a build script would print ' +
    'the same code.'
  );
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
