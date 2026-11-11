// Shared, side-effect-free `RESULT key=value` parser, extracted verbatim
// from `paired-ab-runner.mjs` (Sol-codex proc-probe review round 3, P3-1).
//
// WHY THIS FILE EXISTS. `crates/proc-probe/tests/protocol.rs`'s Node-verified
// interop test used to reconstruct the runner's regex/flags/trim semantics by
// scanning the runner's SOURCE TEXT (`extract_runner_contract` in that file,
// added in round 2 to fix a naive substring guard). That extractor was
// itself defeatable by static text it never understood — a stale comment
// containing the OLD regex before a changed real line, or a real-line change
// (e.g. `.exec(line.toLowerCase().trim())`) that still ends with `.trim()`
// and so still "passes" a one-bit trim check while actually changing runner
// semantics. Building a more complete JavaScript lexer to close each such
// gap would just add a second, larger source of truth that can still drift
// from the real one.
//
// The fix is to have ONE source of truth: this file holds the actual
// `parseResult` function with NO top-level side effects (importing it runs
// no measurement workflow, unlike `paired-ab-runner.mjs` itself, whose
// top-level code parses `process.argv`, loads config, and unconditionally
// calls `main()` at import time). `paired-ab-runner.mjs` imports this file
// for its own use, and the Rust interop test spawns a `node` child that
// ALSO imports this exact file and calls the exact function — so there is
// nothing left to keep in sync, and no possibility of the interop test
// checking a shape the runner doesn't actually use.
export function parseResult(out) {
  const r = {};
  for (const line of out.split(/\r?\n/)) {
    const m = /^RESULT\s+([a-z0-9_]+)=(\S+)$/.exec(line.trim());
    if (m) r[m[1]] = /^-?\d+$/.test(m[2]) ? Number(m[2]) : m[2];
  }
  return r;
}

export function parseJsonResult(out, metric) {
  const lines = out.split(/\r?\n/);
  if (lines.at(-1) === '') lines.pop();
  if (lines.length !== 1 || !lines[0].trim()) {
    throw new Error('expected exactly one JSON object line');
  }
  let row;
  try { row = JSON.parse(lines[0]); } catch (error) {
    throw new Error(`invalid JSON result line: ${error.message}`);
  }
  if (row === null || typeof row !== 'object' || Array.isArray(row)) {
    throw new Error('JSON result must be an object');
  }
  if (typeof row[metric] !== 'number' || !Number.isFinite(row[metric]) || row[metric] <= 0) {
    throw new Error(`JSON result metric ${metric} must be finite and positive`);
  }
  if ('warm_ns' in row && row.warm_ns !== null) {
    throw new Error('JSON result warm_ns must be null when present');
  }
  return row;
}
