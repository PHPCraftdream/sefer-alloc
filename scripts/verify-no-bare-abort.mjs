// R6-T15 / R6-T14 wave-5 lexical guard (review R6-CQ-28, R6-CQ-29/MS-2/SEC-14).
//
// ## The two rules
//
//   Rule 1 (NO-BARE-ABORT): every `std::process::abort` occurrence under
//   `src/**/*.rs` must live in `src/alloc_core/platform/fatal.rs` (the ONE
//   sanctioned terminator). R6-T15 converted the bare-abort sites to
//   `platform::fatal` so every termination carries a diagnostic message; the
//   allowlist mechanism is RETAINED but currently EMPTY, so any bare abort
//   outside fatal.rs fails this gate.
//
//   Rule 2 (NO-PANIC-CONSTRUCTS): the selected GlobalAlloc-path files (the
//   R6-T14 set) must contain no panic-capable construct (`panic!`,
//   `unreachable!`, `assert!`/`assert_eq!`/`assert_ne!`, `.expect(`,
//   `.unwrap(`). This is the cheap text-level complement of the cfg-aware
//   scan in tests/no_panic_doc_accuracy.rs, not a replacement for it.
//
// ## Allowlists
//
// Both allowlist mechanisms exist so a future reviewed exception has a place
// to live with a printed reason: a matching allowlisted hit is printed as
// `ALLOWED (reason)` and does NOT fail the run. ABORT_ALLOWLIST is empty
// since the PR #49 / PR #50 merges (R6-T15 complete). PANIC_ALLOWLIST is
// EMPTY; any new entry needs a reviewed reason (why the construct is safe
// and which task reviewed it) or it does not belong here.
//
// ## Self-test first
//
// A guard that never fired is unproven. The embedded negative-fixture
// self-test runs FIRST on EVERY invocation (and ONLY the self-test under
// `--self-test`), through the exact detector the real scan uses. Any
// fixture disagreement exits 1 with "FAIL - fix the guard, not the
// fixtures".
//
// ## Comment/string stripping (heuristic, documented)
//
// `//`-to-EOL is stripped only when the `//` sits outside a string literal
// (quote parity tracked per line, `\"` escapes honored); `/* ... */` block
// comments are stripped by a small state machine. Doc comments (`///`,
// `//!`) start with `//` and strip naturally. KNOWN LIMITATION: a `//`
// inside a string literal can confuse the stripper (it may blank the rest
// of that line, potentially hiding a real hit); the repo's fatal messages
// contain no `//`, and the heavier cfg-aware test remains the authority.
// Raw strings (`r#"..."#`) are NOT modeled — another reason this guard is
// the cheap complement, not the oracle.
//
// Pure node text scan, no cargo, no external deps.

import { readdirSync, readFileSync } from 'node:fs';
import { join, sep } from 'node:path';

import { fileURLToPath } from 'node:url';

const REPO_ROOT = fileURLToPath(new URL('..', import.meta.url));

// ---------------------------------------------------------------------------
// Rule 1 configuration
// ---------------------------------------------------------------------------

const ABORT_EXEMPT_FILE = 'src/alloc_core/platform/fatal.rs';

// Empty since the PR #49 / PR #50 merges: every remaining bare-abort file was
// converted (R6-T15 complete). Do NOT re-add an entry without a task and a
// named owner — a bare `process::abort` outside fatal.rs must fail this gate.
const ABORT_ALLOWLIST = [];

const ABORT_RE = /std::process::abort|process\s*::\s*abort/g;

// ---------------------------------------------------------------------------
// Rule 2 configuration
// ---------------------------------------------------------------------------

const PANIC_SCAN_FILES = [
  'src/alloc_core/large/alloc_core_large_cache.rs',
  'src/alloc_core/segment/segment_header/terminal_words.rs',
  'src/alloc_core/small/alloc_core_small_magazine.rs',
  'src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs',
  'src/registry/bootstrap/registry.rs',
  'src/alloc_core/platform/numa.rs',
  'src/global/exact_object/exact_shard.rs',
];

// (file, needle, reason) with the same print-ALLOWED semantics as Rule 1.
// Currently EMPTY: entries need a reviewed reason (why the construct cannot
// panic at runtime and which task reviewed that proof).
const PANIC_ALLOWLIST = [];

// `debug_assert!(` must not match `assert!(` (char before must be a
// non-word char); `.unwrap(` must not match `.unwrap_or(`/`.unwrap_or_else(`
// etc. (`(` required right after `unwrap`).
const PANIC_NEEDLES = [
  ['panic!(', /(^|[^A-Za-z0-9_])panic!\(/],
  ['unreachable!(', /(^|[^A-Za-z0-9_])unreachable!\(/],
  ['assert!(', /(^|[^A-Za-z0-9_])assert!\(/],
  ['assert_eq!(', /(^|[^A-Za-z0-9_])assert_eq!\(/],
  ['assert_ne!(', /(^|[^A-Za-z0-9_])assert_ne!\(/],
  ['.expect(', /\.expect\(/],
  ['.unwrap(', /\.unwrap\(/],
];

// ---------------------------------------------------------------------------
// Shared detector
// ---------------------------------------------------------------------------

// Strip comments (heuristically, string-aware for `//`; see header
// limitations). Returns the same-length text with comment spans blanked so
// reported column offsets stay meaningful-ish and line numbers are exact.
function stripComments(text) {
  const out = text.split('');
  let i = 0;
  let inBlock = false;
  let blockDepth = 0; // rust block comments nest
  while (i < text.length) {
    if (inBlock) {
      if (text[i] === '/' && text[i + 1] === '*') {
        blockDepth += 1;
        i += 2;
      } else if (text[i] === '*' && text[i + 1] === '/') {
        blockDepth -= 1;
        i += 2;
        if (blockDepth === 0) inBlock = false;
      } else {
        if (text[i] === '\n') i += 1;
        else out[i] = ' ';
        i += 1;
      }
      continue;
    }
    if (text[i] === '/' && text[i + 1] === '*') {
      inBlock = true;
      blockDepth = 1;
      out[i] = ' ';
      out[i + 1] = ' ';
      i += 2;
      continue;
    }
    if (text[i] === '/' && text[i + 1] === '/') {
      // Strip to EOL only if the `//` is outside a string literal on this
      // line: track quote parity from line start, honoring `\"` escapes.
      let lineStart = i;
      while (lineStart > 0 && text[lineStart - 1] !== '\n') lineStart -= 1;
      let inString = false;
      for (let j = lineStart; j < i; j += 1) {
        if (text[j] === '\\' && text[j + 1] !== undefined) {
          j += 1;
        } else if (text[j] === '"') {
          inString = !inString;
        }
      }
      if (!inString) {
        while (i < text.length && text[i] !== '\n') {
          if (out[i] !== '\n') out[i] = ' ';
          i += 1;
        }
        continue;
      }
    }
    i += 1;
  }
  return out.join('');
}

function* scanLine(text) {
  let line = 1;
  for (const ch of text) {
    if (ch === '\n') line += 1;
    yield { ch, line };
  }
}

// `const _: () = { ... }` items are COMPILE-TIME layout/invariant asserts:
// they fail the build, they cannot panic or unwind at runtime, and the repo
// deliberately uses this pattern (offset asserts, Sync proofs). Constructs
// inside such a block are a structurally different class and are SKIPPED
// (brace-counting skip mode, quote-aware) — this is NOT a PANIC_ALLOWLIST
// entry, and PANIC_ALLOWLIST stays empty.
// One entry per rule-2 construct hit: { rule, snippet, line }.
function panicHits(content) {
  const stripped = stripComments(content);
  const hits = [];
  const lines = stripped.split('\n');
  let constSkipDepth = 0;
  for (let idx = 0; idx < lines.length; idx += 1) {
    const lineText = lines[idx];
    if (constSkipDepth === 0 && /const\s+_:\s*\(\)\s*=/.test(lineText)) {
      constSkipDepth = braceDelta(lineText);
      if (constSkipDepth <= 0) constSkipDepth = 0;
      continue;
    }
    if (constSkipDepth > 0) {
      constSkipDepth += braceDelta(lineText);
      if (constSkipDepth <= 0) constSkipDepth = 0;
      continue;
    }
    for (const [label, re] of PANIC_NEEDLES) {
      const m = re.exec(lineText);
      if (m !== null) {
        hits.push({
          rule: `NO-PANIC-CONSTRUCTS ${label}`,
          line: idx + 1,
          snippet: lineText.trim().slice(0, 80),
        });
      }
    }
  }
  return hits;
}

// Net brace delta of one line, honoring `"` string parity and `\"` escapes
// (same heuristic class as stripComments; see header limitations).
function braceDelta(lineText) {
  let delta = 0;
  let inString = false;
  for (let i = 0; i < lineText.length; i += 1) {
    const ch = lineText[i];
    if (inString) {
      if (ch === '\\') i += 1;
      else if (ch === '"') inString = false;
    } else if (ch === '"') {
      inString = true;
    } else if (ch === '{') {
      delta += 1;
    } else if (ch === '}') {
      delta -= 1;
    }
  }
  return delta;
}

// One entry per rule-1 hit, allowlist-applied: each element is
// { line, snippet, allowed: boolean, reason?: string }.
function abortHits(path, content) {
  const stripped = stripComments(content);
  const hits = [];
  const lines = stripped.split('\n');
  for (let idx = 0; idx < lines.length; idx += 1) {
    ABORT_RE.lastIndex = 0;
    const m = ABORT_RE.exec(lines[idx]);
    if (m !== null) {
      const allow = ABORT_ALLOWLIST.find(([f]) => f === path);
      const exempt = path === ABORT_EXEMPT_FILE;
      hits.push({
        line: idx + 1,
        snippet: lines[idx].trim().slice(0, 80),
        allowed: exempt || allow !== undefined,
        reason: exempt
          ? 'sanctioned terminator'
          : allow === undefined
            ? undefined
            : allow[1],
      });
    }
  }
  return hits;
}

function* walkRs(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) yield* walkRs(full);
    else if (entry.isFile() && entry.name.endsWith('.rs')) yield full;
  }
}

function toRepoRel(full) {
  return full.startsWith(REPO_ROOT) ? full.slice(REPO_ROOT.length) : full;
}

// ---------------------------------------------------------------------------
// Self-test (embedded negative fixtures, run through the SAME detectors)
// ---------------------------------------------------------------------------

const FIXTURES = [
  // MUST-FLAG (expect at least one hit)
  {
    path: 'fake.rs',
    kind: 'abort',
    content: 'fn f() {\n    std::process::abort();\n}\n',
    expect: 'flag',
    why: 'bare std::process::abort outside fatal.rs must be flagged',
  },
  {
    path: 'src/registry/bootstrap/registry.rs',
    kind: 'panic',
    content: 'fn f() {\n    x.expect("boom");\n}\n',
    expect: 'flag',
    why: '.expect( in a selected file must be flagged',
  },
  {
    path: 'src/alloc_core/large/alloc_core_large_cache.rs',
    kind: 'panic',
    content: 'fn f(x: bool) {\n    assert!(x);\n}\n',
    expect: 'flag',
    why: 'assert!( in a selected file must be flagged',
  },
  {
    path: 'src/alloc_core/platform/numa.rs',
    kind: 'panic',
    content: 'fn f() {\n    unreachable!("x");\n}\n',
    expect: 'flag',
    why: 'unreachable!( in a selected file must be flagged',
  },
  {
    path: 'src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs',
    kind: 'panic',
    content: 'const _: () = {\n    assert!(1 == 1);\n};\n',
    expect: 'pass',
    why: 'compile-time const _: () assert blocks are skipped (cannot panic at runtime)',
  },
  {
    path: 'src/alloc_core/small/alloc_core_small/alloc_core_small_impl.rs',
    kind: 'panic',
    content: 'fn f(x: bool) {\n    assert!(x);\n}\n',
    expect: 'flag',
    why: 'the same assert OUTSIDE a const _: () block must still be flagged',
  },
  // MUST-PASS (expect zero hits)
  {
    path: 'fake.rs',
    kind: 'abort',
    content: 'fn f() {\n    fatal(b"fake: invariant\\n");\n}\n',
    expect: 'pass',
    why: 'fatal() is the sanctioned terminator, not an abort hit',
  },
  {
    path: 'src/alloc_core/platform/fatal.rs',
    kind: 'abort',
    content: 'pub fn fatal(msg: &[u8]) -> ! {\n    std::process::abort()\n}\n',
    expect: 'allowed',
    why: 'fatal.rs is the sanctioned abort site (exempt, printed ALLOWED)',
  },
  {
    path: 'src/alloc_core/alloc_core/sidecar_drain.rs',
    kind: 'abort',
    content: 'fn f() {\n    std::process::abort();\n}\n',
    expect: 'flag',
    why: 'former open-PR allowlisted path must now flag (allowlist is empty)',
  },
  {
    path: 'src/alloc_core/small/alloc_core_small_magazine.rs',
    kind: 'panic',
    content: 'fn f(cond: bool) {\n    debug_assert!(cond);\n}\n',
    expect: 'pass',
    why: 'debug_assert!( is an excluded construct (word boundary)',
  },
  {
    path: 'src/registry/bootstrap/registry.rs',
    kind: 'panic',
    content: 'fn f() -> u8 {\n    x.unwrap_or_else(f)\n}\n',
    expect: 'pass',
    why: '.unwrap_or_else( must not match .unwrap(',
  },
  {
    path: 'fake.rs',
    kind: 'abort',
    content: '// std::process::abort();\nfn f() {}\n',
    expect: 'pass',
    why: 'commented-out abort text must be comment-stripped',
  },
];

function runSelfTest() {
  let failures = 0;
  for (const fx of FIXTURES) {
    let flagged;
    if (fx.kind === 'abort') {
      const hits = abortHits(fx.path, fx.content);
      if (fx.expect === 'flag') {
        flagged = hits.some((h) => !h.allowed);
      } else if (fx.expect === 'allowed') {
        flagged = hits.length > 0 && hits.every((h) => h.allowed);
      } else {
        flagged = hits.length > 0;
      }
    } else {
      flagged = panicHits(fx.content).length > 0;
    }
    const ok = fx.expect === 'pass' ? !flagged : flagged;
    if (!ok) {
      failures += 1;
      console.log(
        `SELF-TEST FAIL: ${fx.path} [${fx.kind}] expected ${fx.expect}, ` +
          `got ${flagged ? 'flag' : 'pass'} -- ${fx.why}`,
      );
    } else {
      console.log(`SELF-TEST ok: ${fx.why}`);
    }
  }
  console.log(
    failures === 0
      ? `SELF-TEST: ${FIXTURES.length}/${FIXTURES.length} fixtures agree`
      : `SELF-TEST: ${failures} disagreement(s)`,
  );
  return failures === 0;
}

// ---------------------------------------------------------------------------
// Real scan
// ---------------------------------------------------------------------------

function realScan() {
  let violations = 0;
  let allowed = 0;
  const srcDir = join(REPO_ROOT, 'src');
  for (const full of walkRs(srcDir)) {
    const rel = toRepoRel(full).split(sep).join('/');
    const content = readFileSync(full, 'utf8');
    for (const hit of abortHits(rel, content)) {
      if (hit.allowed) {
        allowed += 1;
        console.log(`${rel}:${hit.line}: ALLOWED (reason) ${hit.reason}`);
      } else {
        violations += 1;
        console.log(
          `${rel}:${hit.line}: NO-BARE-ABORT (convert to platform::fatal, R6-T15) ${hit.snippet}`,
        );
      }
    }
  }
  for (const rel of PANIC_SCAN_FILES) {
    const content = readFileSync(join(REPO_ROOT, rel), 'utf8');
    for (const hit of panicHits(content)) {
      const allow = PANIC_ALLOWLIST.find(
        ([f, needle]) => f === rel && hit.snippet.includes(needle),
      );
      if (allow !== undefined) {
        allowed += 1;
        console.log(
          `${rel}:${hit.line}: ALLOWED (reason) ${allow[2]} ${hit.snippet}`,
        );
      } else {
        violations += 1;
        console.log(
          `${rel}:${hit.line}: ${hit.rule} (R6-T14: convert to platform::fatal) ${hit.snippet}`,
        );
      }
    }
  }
  console.log(
    `\nverify-no-bare-abort: ${violations} violation(s), ${allowed} allowed ` +
      `(self-test ${FIXTURES.length} fixtures OK)`,
  );
  return violations === 0;
}

const selfTestOnly = process.argv.includes('--self-test');
const selfTestOk = runSelfTest();
if (!selfTestOk) {
  console.log('FAIL - fix the guard, not the fixtures');
  process.exit(1);
}
if (selfTestOnly) {
  console.log('verify-no-bare-abort: self-test only, scan skipped');
  process.exit(0);
}
process.exit(realScan() ? 0 : 1);
