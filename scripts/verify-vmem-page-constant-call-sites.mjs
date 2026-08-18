// task #1076 — static-analysis guard against passing the COMPILE-TIME page
// constant (the 4 KiB `PAGE`) into argument positions that the aligned-vmem API
// validates against the RUNTIME `page_size()`.
//
// ## Background
//
// Three escapes of the same bug class happened in one campaign:
//   - task #1067: `crates/aligned-vmem/tests/mock.rs` recorded mock calls with
//     `PAGE`-sized offsets;
//   - task #1074: `src/alloc_core/bootstrap.rs`-era production code (plus 5
//     sibling sites) flowed meta-end sums built from `PAGE` into
//     page_size()-validated positions;
//   - task #1075: `crates/aligned-vmem/tests/smoke.rs` x4 `reserve_aligned_lazy`
//     calls passed bare `PAGE` as `initial_commit` in positive
//     (success-expecting) contexts, since fixed to `ps`.
//
// None of them could be caught by a test on this Windows dev host: 4 KiB pages
// mean `PAGE == page_size()`, so every `PAGE`-multiple is also a
// `page_size()`-multiple and the guards under test accept it. The failure only
// exists counterfactually, on 16 KiB (Apple Silicon macOS CI) or 64 KiB-page
// hosts. That is the exact structural blindness task #1059 addressed for the
// cfg(unix) gap: a local host where the discriminating input cannot exist.
// This guard replaces the missing host with constant folding: it evaluates
// argument expressions at scanned call sites and FAILS when a folded value is
// not a multiple of 65536.
//
// ## The 64 KiB bar
//
// 64 KiB is the safety bar because it covers every runtime page size the crate
// acknowledges (4/16/64 KiB): any value that is a multiple of 64 KiB is a
// multiple of all three, so it passes `page_size()`-based validation on every
// host. A `PAGE`-multiple that is not a 64 KiB multiple (e.g. bare `PAGE`,
// `2 * PAGE`) is exactly the value class that silently works here and breaks
// on 16 KiB-page CI. Zero counts as safe (empty ranges are legal no-ops).
//
// ## What is scanned (derived from validate_initial_commit in
// crates/aligned-vmem/src/api/internal.rs and the range validators in
// src/api/{commit_range,decommit,decommit_lazy,recommit}.rs)
//
// FREE-FUNCTION forms — name possibly preceded by lowercase module paths
// (`vmem::`, `aligned_vmem::`); a preceding Uppercase qualifier (`Call::`,
// `Self::`, `SegmentLayout::`) means an enum-variant/mock-record constructor
// and the site is skipped entirely:
//   - reserve_aligned_lazy / try_reserve_aligned_lazy (size, align,
//     initial_commit): scan args 1 and 3. Arg 2 (align) is validated against
//     the compile-time PAGE only, so it is NOT scanned.
//   - commit_range / try_commit_range / decommit / try_decommit /
//     decommit_lazy / try_decommit_lazy / recommit / try_recommit
//     (base, start, end): scan args 2 and 3. Arg 1 is a base pointer.
//
// METHOD forms — a `.` immediately before the name (Reservation / safe
// wrapper receivers), same eight range names as above with (start, end):
// scan args 1 and 2. NOTE: a `.decommit(`-style method is ASSUMED to be a
// Reservation-family receiver; if some unrelated type ever grows a method
// with one of these names and constant args, the assumption needs revisiting.
//
// ## Suppressions
//
//   S1  marker `pageguard:allow` on the flagged argument's line, the call's
//       first line, or either of the 2 lines above it (in the ORIGINAL text —
//       the marker lives in a comment, which blanking would otherwise hide).
//   S2a some scanned arg folds to a value that is not a multiple of 4096 —
//       the call violates even the compile-time floor, fails on EVERY host,
//       and would fail loudly in any test run (not the silent-until-16KiB-CI
//       class this guard hunts).
//   S2b both scanned range args fold and start > end (inverted range) — same
//       "already rejected everywhere" reasoning. Never applied to the reserve
//       family.
//   S3  negative-assertion context: (a) `!call(` prefix, (b) `.is_none(`/
//       `.is_err`/`.unwrap_err(`/`.err(` within 64 chars after the close
//       paren (stopping at the first `;` or `{`), or (c) `let NAME = <call>`
//       followed within 300 chars by `NAME.is_none(`/`NAME.is_err`/`!NAME`/
//       `NAME.unwrap_err(`.
//
// ## KNOWN BLIND SPOTS (do not treat a green run as proof of absence)
//
//   - Values flowing through local variables or cross-file consts are
//     invisible: `crates/aligned-vmem/tests/lazy_commit.rs`'s
//     `windows_lazy_reserve_saves_commit_charge` binds `let initial = PAGE;`
//     and passes `initial` — valid only because the test is `#[cfg(windows)]`-
//     gated where runtime pages are 4 KiB, and this guard cannot see it.
//   - Task #1074's production escape flowed meta-end sums through a helper
//     function before they reached the validated positions; this purely
//     syntactic guard would NOT have caught it.
//   - `#[should_panic]` and `matches!(x, Err(_))` negative forms are not
//     recognized (none exist in-tree today).
//   - String/char-literal blanking is heuristic (raw-string corner cases; a
//     char literal containing `"` is handled, exotic escapes may not be).
//   - On a hypothetical >64 KiB-page host even `16 * PAGE` fails at runtime
//     but passes this guard — the bar is 64 KiB, not "any page size".
//
// ## Self-test
//
// Before scanning the real tree, the detector runs over embedded fixture
// Rust sources through the SAME scanSource(path, source) code path the file
// walker uses. A guard that never fired is unproven; the fixtures are the
// firing proof, and they run FIRST, always.
//
// Usage:
//   node scripts/verify-vmem-page-constant-call-sites.mjs
//   npm run check   (wired in alongside the other verify-* guards)

import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const SCRIPT = 'verify-vmem-page-constant-call-sites';
const BAR = 65536; // 64 KiB — see "The 64 KiB bar" above
const FLOOR = 4096; // compile-time PAGE floor — S2a's threshold
const MARKER = 'pageguard:allow';
const SKIP_DIRS = new Set(['target', '.git', 'docs', 'node_modules']);

// Free-function forms: name -> scanned 1-based argument indexes.
const FREE_FN_ARGS = {
  reserve_aligned_lazy: [1, 3],
  try_reserve_aligned_lazy: [1, 3],
  commit_range: [2, 3],
  try_commit_range: [2, 3],
  decommit: [2, 3],
  try_decommit: [2, 3],
  decommit_lazy: [2, 3],
  try_decommit_lazy: [2, 3],
  recommit: [2, 3],
  try_recommit: [2, 3],
};
// Names for which a `.`-prefixed (method) form is the Reservation API.
const METHOD_NAMES = new Set([
  'commit_range',
  'try_commit_range',
  'decommit',
  'try_decommit',
  'decommit_lazy',
  'try_decommit_lazy',
  'recommit',
  'try_recommit',
]);

// ─────────────────────────────────────────────────────────────────────────────
// Blanker: replace comments and string literals with spaces, preserving every
// offset and newline so line/column numbers stay exact. Delimiters are blanked
// too (not just interiors) — that keeps `PAGE /* why */ 2` foldable and kills
// matches like `"...per decommit()/decommit_lazy()..."` completely. Char
// literals are left alone (they cannot contain a call), but their extent is
// skipped so a `'"'` does not open a phantom string.
// ─────────────────────────────────────────────────────────────────────────────

function blankRust(src) {
  const out = src.split('');
  const n = src.length;
  const blank = (a, b) => {
    for (let k = a; k < b && k < n; k++) if (out[k] !== '\n') out[k] = ' ';
  };

  let i = 0;
  while (i < n) {
    const c = src[i];

    if (c === '/' && src[i + 1] === '/') {
      // `//`, `///`, `//!` — to end of line (newline preserved).
      let j = i + 2;
      while (j < n && src[j] !== '\n') j++;
      blank(i, j);
      i = j;
    } else if (c === '/' && src[i + 1] === '*') {
      // Rust block comments NEST.
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        if (src[j] === '/' && src[j + 1] === '*') {
          depth++;
          j += 2;
        } else if (src[j] === '*' && src[j + 1] === '/') {
          depth--;
          j += 2;
        } else {
          j++;
        }
      }
      blank(i, j);
      i = j;
    } else if (c === 'r' && (src[i + 1] === '"' || src[i + 1] === '#')) {
      // Raw string r"..." / r#"..."# / r##"..."## (any # count). An `r#ident`
      // raw identifier fails the `"` check and falls through untouched.
      let j = i + 1;
      let hashes = 0;
      while (j < n && src[j] === '#') {
        hashes++;
        j++;
      }
      if (j < n && src[j] === '"') {
        const hashes2 = '#'.repeat(hashes); // closing is `"` + the same # count
        let k = j + 1;
        let end = -1;
        while (k < n) {
          if (src[k] === '"' && src.startsWith(hashes2, k + 1)) {
            end = k + 1 + hashes;
            break;
          }
          k++;
        }
        const stop = end === -1 ? n : end;
        blank(i, stop);
        i = stop;
      } else {
        i++;
      }
    } else if (c === '"') {
      // Ordinary (and byte `b"..."`) string: the leading `b` was already
      // consumed as a plain char; escapes respected.
      let j = i + 1;
      while (j < n) {
        if (src[j] === '\\') j += 2;
        else if (src[j] === '"') {
          j++;
          break;
        } else j++;
      }
      blank(i, j);
      i = j;
    } else if (c === "'") {
      // Char literal ('x', '\n', '\'', '\u{1F}', '\x41')? Skip its extent.
      // A lifetime ('a) fails this and is left in place, as intended.
      const m = /^'(\\(?:['"\\0nrt]|x[0-9a-fA-F]{2}|u\{[0-9a-fA-F]{1,6}\})|[^\\'])'/.exec(
        src.slice(i),
      );
      i += m ? m[0].length : 1;
    } else {
      i++;
    }
  }
  return out.join('');
}

// ─────────────────────────────────────────────────────────────────────────────
// Constant folder: plain integer arithmetic over per-file `const NAME: usize`
// bindings plus the PAGE builtin (4096). A file-local `const PAGE` is honored
// when its definition itself folds (e.g. page.rs's literal `4096`); when it
// does NOT fold it is an alias of the standard constant (`vmem::PAGE`,
// `aligned_vmem::PAGE`), not a different value, so PAGE falls back to 4096 —
// otherwise the src/alloc_core/os.rs shape (a pure `vmem::PAGE` alias) would
// silently disable scanning of every PAGE argument in that file.
// Anything else — unresolvable identifiers, `::`, `.`, calls — is "unevaluable"
// and the argument is skipped (it is a runtime value or cross-file constant).
// Plain JS numbers, not BigInt: the values here are small; anything that
// leaves the safe-integer range is unevaluable by construction.
// ─────────────────────────────────────────────────────────────────────────────

class Unevaluable extends Error {}

const CONST_DEF_RE =
  /(?:\bpub(?:\s*\([^)]*\))?\s+)?\bconst\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*usize\b\s*=\s*([^;]*);/g;
const NUM_RE =
  /^(0x[0-9a-fA-F_]+|0b[01_]+|0o[0-7_]+|[0-9][0-9_]*)(u8|u16|u32|u64|usize|i8|i16|i32|i64|isize)?/;
const IDENT_RE = /^[A-Za-z_][A-Za-z0-9_]*/;

function tokenizeExpr(text) {
  const toks = [];
  let i = 0;
  outer: while (i < text.length) {
    const c = text[i];
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') {
      i++;
      continue;
    }
    const rest = text.slice(i);
    let m = NUM_RE.exec(rest);
    if (m) {
      const digits = m[1].replace(/_/g, '');
      const radix = digits.startsWith('0x')
        ? 16
        : digits.startsWith('0b')
          ? 2
          : digits.startsWith('0o')
            ? 8
            : 10;
      toks.push({ k: 'num', v: parseInt(radix === 10 ? digits : digits.slice(2), radix) });
      i += m[0].length;
      continue;
    }
    m = IDENT_RE.exec(rest);
    if (m) {
      toks.push({ k: 'id', v: m[0] });
      i += m[0].length;
      continue;
    }
    for (const op of ['<<', '>>']) {
      if (rest.startsWith(op)) {
        toks.push({ k: 'op', v: op });
        i += 2;
        continue outer;
      }
    }
    if ('+-*/%()'.includes(c)) {
      toks.push({ k: 'op', v: c });
      i++;
      continue;
    }
    throw new Unevaluable(`stray token ${JSON.stringify(rest.slice(0, 8))}`);
  }
  return toks;
}

const checked = (v) => {
  if (!Number.isSafeInteger(v)) throw new Unevaluable('out of safe range');
  return v;
};

/** Fold `text` to a number, or return null if it is unevaluable.
 * `defs` maps const NAME -> raw EXPR text for THIS file (folded lazily,
 * cycle- and depth-guarded). PAGE resolves to the file's own const when that
 * definition folds, else to the 4096 builtin (a non-folding file-local PAGE
 * is an alias of the standard constant, not a different value). */
function foldExpr(text, defs) {
  const fold = (expr, depth, seen) => {
    const toks = tokenizeExpr(expr);
    if (toks.length === 0) throw new Unevaluable('empty');
    let p = 0;
    const isOp = (v) => toks[p] !== undefined && toks[p].k === 'op' && toks[p].v === v;
    const expect = (v) => {
      if (!isOp(v)) throw new Unevaluable(`expected ${v}`);
      p++;
    };

    const resolve = (name) => {
      if (depth > 16 || seen.has(name)) throw new Unevaluable(`cycle/depth at ${name}`);
      if (name === 'PAGE') {
        if (defs.has('PAGE')) {
          try {
            return fold(defs.get('PAGE'), depth + 1, new Set(seen).add('PAGE'));
          } catch (e) {
            if (e instanceof Unevaluable) return 4096; // alias like `vmem::PAGE`
            throw e;
          }
        }
        return 4096;
      }
      if (defs.has(name)) return fold(defs.get(name), depth + 1, new Set(seen).add(name));
      throw new Unevaluable(`unresolved ${name}`);
    };

    // precedence: + -  <  * / %  <  << >>  <  unary -  <  primary
    const additive = () => {
      let v = multiplicative();
      while (isOp('+') || isOp('-')) {
        const op = toks[p++].v;
        const r = multiplicative();
        v = checked(op === '+' ? v + r : v - r);
      }
      return v;
    };
    const multiplicative = () => {
      let v = shift();
      while (isOp('*') || isOp('/') || isOp('%')) {
        const op = toks[p++].v;
        const r = shift();
        if ((op === '/' || op === '%') && r === 0) throw new Unevaluable('div by zero');
        v = checked(op === '*' ? v * r : op === '/' ? Math.trunc(v / r) : v % r);
      }
      return v;
    };
    const shift = () => {
      let v = unary();
      while (isOp('<<') || isOp('>>')) {
        const op = toks[p++].v;
        const r = unary();
        if (r < 0 || r > 53) throw new Unevaluable('shift range');
        v = checked(op === '<<' ? v * 2 ** r : Math.trunc(v / 2 ** r));
      }
      return v;
    };
    const unary = () => {
      if (isOp('-')) {
        p++;
        return checked(-unary());
      }
      if (isOp('+')) {
        p++;
        return unary();
      }
      return primary();
    };
    const primary = () => {
      const t = toks[p];
      if (t === undefined) throw new Unevaluable('eof');
      if (t.k === 'num') {
        p++;
        return checked(t.v);
      }
      if (t.k === 'id') {
        p++;
        return checked(resolve(t.v));
      }
      if (isOp('(')) {
        p++;
        const v = additive();
        expect(')');
        return v;
      }
      throw new Unevaluable('bad primary');
    };

    const v = additive();
    if (p !== toks.length) throw new Unevaluable('trailing tokens');
    return v;
  };

  try {
    return fold(text, 0, new Set());
  } catch (e) {
    if (e instanceof Unevaluable) return null;
    throw e;
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Call-site scanning
// ─────────────────────────────────────────────────────────────────────────────

/** Classify what precedes the name at `nameStart` in blanked text:
 * 'method' (a `.`), 'skip' (Uppercase:: qualifier — mock/enum constructor),
 * or 'free'. */
function classifySite(blanked, nameStart) {
  let i = nameStart - 1;
  while (i >= 0 && /\s/.test(blanked[i])) i--;
  if (i < 0) return 'free';
  const prev = blanked[i];
  if (prev === '.') return 'method';
  if (prev === ':' && blanked[i - 1] === ':') {
    let j = i - 2;
    while (j >= 0 && /\s/.test(blanked[j])) j--;
    const end = j;
    while (j >= 0 && /[A-Za-z0-9_]/.test(blanked[j])) j--;
    const qualifier = blanked.slice(j + 1, end + 1);
    if (qualifier && qualifier[0] === qualifier[0].toUpperCase()) return 'skip';
    return 'free'; // vmem::, aligned_vmem::, crate::api:: …
  }
  return 'free';
}

/** From the `(` at `openParen`, scan to the matching `)` (one depth counter
 * over ()[]{}) and split top-level commas. Returns null if unbalanced. */
function extractArgs(blanked, openParen) {
  let depth = 0;
  let close = -1;
  for (let k = openParen; k < blanked.length; k++) {
    const ch = blanked[k];
    if (ch === '(' || ch === '[' || ch === '{') depth++;
    else if (ch === ')' || ch === ']' || ch === '}') {
      depth--;
      if (depth === 0) {
        close = k;
        break;
      }
    }
  }
  if (close === -1) return null;
  const args = [];
  let rel = 0;
  let start = openParen + 1;
  for (let k = openParen + 1; k < close; k++) {
    const ch = blanked[k];
    if (ch === '(' || ch === '[' || ch === '{') rel++;
    else if (ch === ')' || ch === ']' || ch === '}') rel--;
    else if (ch === ',' && rel === 0) {
      args.push({ text: blanked.slice(start, k).trim(), start });
      start = k + 1;
    }
  }
  args.push({ text: blanked.slice(start, close).trim(), start });
  return { args, closeParen: close };
}

/** S3: is this call in a context that EXPECTS rejection? */
function negativeContext(blanked, nameStart, closeParen) {
  // (a) `!call(` — but not `!=` / `!==`. An `unsafe {` prefix ends with `{`
  // and correctly does not match.
  const win = blanked.slice(Math.max(0, nameStart - 8), nameStart).trimEnd();
  if (win.endsWith('!') && !win.endsWith('!=') && !win.endsWith('!==')) return 'S3a';
  // (b) `.is_none(`/`.is_err*`/`.unwrap_err(`/`.err(` right after the call —
  // window of 64 chars, stopping at the first `;` or `{` (the window must
  // cross newlines: `try_commit_range(...)\n    .unwrap_err()` is this shape),
  // then skipping whitespace and `}`/`)` (the `}` of a closing `unsafe {`).
  const w64 = blanked.slice(closeParen + 1, closeParen + 65);
  const stop = w64.search(/[;{]/);
  const head = (stop === -1 ? w64 : w64.slice(0, stop)).replace(/^[\s})]+/, '');
  if (
    head.startsWith('.is_none(') ||
    head.startsWith('.is_err') ||
    head.startsWith('.unwrap_err(') ||
    head.startsWith('.err(')
  ) {
    return 'S3b';
  }
  // (c) deferred: `let NAME = <call>` (an optional simple receiver like `r.`
  // before the call name is tolerated — `let result = r.try_recommit(..)`)
  // with a negative assertion on NAME within 300 chars after the call.
  const lm = /let\s+([A-Za-z_]\w*)\s*=\s*[\w.\s]*$/.exec(blanked.slice(0, nameStart));
  if (lm) {
    const bind = lm[1];
    const w300 = blanked.slice(closeParen + 1, closeParen + 1 + 300);
    const pats = [
      new RegExp(`\\b${bind}\\s*\\.\\s*is_none\\(`),
      new RegExp(`\\b${bind}\\s*\\.\\s*is_err`),
      new RegExp(`!\\s*${bind}\\b`),
      new RegExp(`\\b${bind}\\s*\\.\\s*unwrap_err\\(`),
    ];
    if (pats.some((r) => r.test(w300))) return 'S3c';
  }
  return null;
}

/** Scan one Rust source (fixture or real file) through the exact same path.
 * Returns { findings, stats }; `path` is only used for reporting. */
function scanSource(path, source) {
  const blanked = blankRust(source);
  const originalLines = source.split('\n'); // S1 reads the ORIGINAL text
  const lineStarts = [0];
  for (let i = 0; i < blanked.length; i++) if (blanked[i] === '\n') lineStarts.push(i + 1);
  const lineOf = (off) => {
    let lo = 0;
    let hi = lineStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (lineStarts[mid] <= off) lo = mid;
      else hi = mid - 1;
    }
    return lo + 1;
  };

  // Per-file const bindings (blanked text: commented-out consts cannot match).
  const defs = new Map();
  for (const m of blanked.matchAll(CONST_DEF_RE)) defs.set(m[1], m[2].trim());

  const stats = { calls: 0, candidates: 0, s1: 0, s2a: 0, s2b: 0, s3: 0 };
  const findings = [];

  for (const name of Object.keys(FREE_FN_ARGS)) {
    const re = new RegExp(`\\b${name}\\s*\\(`, 'g');
    for (const m of blanked.matchAll(re)) {
      stats.calls++;
      const nameStart = m.index;
      const openParen = nameStart + m[0].length - 1;
      const form = classifySite(blanked, nameStart);
      if (form === 'skip') continue;
      let argIndexes;
      if (form === 'method') {
        if (!METHOD_NAMES.has(name)) continue; // `.reserve_aligned_lazy(` — not our API
        argIndexes = [1, 2];
      } else {
        argIndexes = FREE_FN_ARGS[name];
      }
      const call = extractArgs(blanked, openParen);
      if (!call) continue;
      const callLine = lineOf(nameStart);

      // Fold every scanned arg; collect the ones that fold to a non-64KiB
      // multiple as candidate findings.
      const folded = new Map();
      for (const idx of argIndexes) {
        const a = call.args[idx - 1];
        folded.set(idx, a ? foldExpr(a.text, defs) : null);
      }
      const candidates = [];
      for (const idx of argIndexes) {
        const v = folded.get(idx);
        if (v === null || v === undefined || v % BAR === 0) continue;
        candidates.push({ idx, text: call.args[idx - 1].text, value: v, argLine: lineOf(call.args[idx - 1].start) });
      }
      if (candidates.length === 0) continue;
      stats.candidates += candidates.length;

      // S1 — per candidate, original text: the flagged arg's line, the call's
      // first line, or either of the 2 lines above the call's first line.
      const survivors = [];
      for (const c of candidates) {
        const lines = new Set([callLine, callLine - 1, callLine - 2, c.argLine]);
        const marked = [...lines].some((ln) => ln >= 1 && (originalLines[ln - 1] ?? '').includes(MARKER));
        if (marked) stats.s1++;
        else survivors.push(c);
      }
      if (survivors.length === 0) continue;

      // S2a — any scanned arg folds to a non-4096-multiple (for the reserve
      // family the scanned set IS args 1 and 3): rejected on every host.
      const s2a = [...folded.values()].some((v) => v !== null && v !== undefined && v % FLOOR !== 0);
      if (s2a) {
        stats.s2a += survivors.length;
        continue;
      }
      // S2b — inverted range (never for the reserve family): both scanned
      // range args fold and start > end.
      const isReserve = name.startsWith('reserve_aligned_lazy') || name.startsWith('try_reserve_aligned_lazy');
      if (!isReserve) {
        const [i1, i2] = argIndexes;
        const v1 = folded.get(i1);
        const v2 = folded.get(i2);
        if (v1 !== null && v1 !== undefined && v2 !== null && v2 !== undefined && v1 > v2) {
          stats.s2b += survivors.length;
          continue;
        }
      }
      // S3 — negative-assertion context.
      const s3 = negativeContext(blanked, nameStart, call.closeParen);
      if (s3) {
        stats.s3 += survivors.length;
        continue;
      }

      for (const c of survivors) {
        findings.push({ path, line: callLine, name, argIdx: c.idx, argText: c.text, value: c.value });
      }
    }
  }
  return { findings, stats };
}

// ─────────────────────────────────────────────────────────────────────────────
// Built-in self-test — runs FIRST, always. A must-flag fixture that produces
// zero findings (or a must-pass fixture that produces one) is [BAD] and stops
// the run with exit 1 before the tree is scanned.
//
// F4 deviation note: the task's original F4 text used the literal `4097`, but
// 4097 is not a 4096 multiple, so suppression S2a (exactly as specified)
// drops it — it cannot FLAG. That exact text is kept below as PASS fixture
// P12 (proving S2a catches it), and F4 flags with `12288`: still a bare
// literal non-multiple of the 64 KiB bar in a positive context, but a
// 4096-multiple, so S2a stays out of the way. Same lesson the task text
// itself applied when it corrected F3.
// ─────────────────────────────────────────────────────────────────────────────

const FIXTURES = [
  {
    id: 'F1',
    mustFlag: true,
    purpose: 'reserve_aligned_lazy with PAGE initial_commit (task #1075 shape)',
    source: `const MIB: usize = 1024 * 1024;

fn probe() {
    let r = reserve_aligned_lazy(2 * MIB, 2 * MIB, PAGE).expect("x");
}
`,
  },
  {
    id: 'F2',
    mustFlag: true,
    purpose: 'production shape, lowercase path qualifier, no marker',
    source: `fn probe(base: *mut u8) {
    unsafe { vmem::decommit(base, PAGE, 2 * PAGE); }
}
`,
  },
  {
    id: 'F3',
    mustFlag: true,
    purpose: 'method form, ordered PAGE multiples, positive context',
    source: `fn probe(r: &mut Reservation) {
    r.commit_range(PAGE, 4 * PAGE);
}
`,
  },
  {
    id: 'F4',
    mustFlag: true,
    purpose: 'bare literal non-multiple of 64 KiB (12288, a 4096 multiple) in positive context',
    source: `fn probe(r: &mut Reservation) {
    assert!(r.try_commit_range(0, 12288));
}
`,
  },
  {
    id: 'F5',
    mustFlag: true,
    purpose: 'file-local const PAGE alias (vmem::PAGE) still folds to 4096 — the src/alloc_core/os.rs shape',
    source: `const PAGE: usize = vmem::PAGE;

fn probe(seg: usize) {
    let r = vmem::reserve_aligned_lazy(seg, seg, PAGE);
}
`,
  },
  {
    id: 'P1',
    mustFlag: false,
    purpose: 'the known-safe 64 KiB form (16 * PAGE)',
    source: `fn probe() {
    let r = reserve_aligned_lazy(16 * PAGE, 16 * PAGE, 16 * PAGE);
}
`,
  },
  {
    id: 'P2',
    mustFlag: false,
    purpose: 'negative assertion (.is_none)',
    source: `fn probe(bad: usize) {
    assert!(reserve_aligned_lazy(bad, bad, PAGE).is_none());
}
`,
  },
  {
    id: 'P3',
    mustFlag: false,
    purpose: 'Uppercase-qualified mock constructor, not scanned',
    source: `fn probe() {
    let e = Call::decommit(0x1000, 0, PAGE);
}
`,
  },
  {
    id: 'P4',
    mustFlag: false,
    purpose: 'negation prefix (!call)',
    source: `fn probe(base: *mut u8) {
    assert!(!recommit(base, 1, PAGE));
}
`,
  },
  {
    id: 'P5',
    mustFlag: false,
    purpose: 'deferred negative (S3c) and non-floor arg (S2a)',
    source: `fn probe(r: &mut Reservation, ps: usize) {
    let result = r.try_recommit(1, ps);
    assert!(result.is_err_and(|e| true));
}
`,
  },
  {
    id: 'P6',
    mustFlag: false,
    purpose: 'variables, unevaluable',
    source: `fn probe(base: *mut u8, start_offset: usize, end_offset: usize) {
    unsafe { vmem::commit_range(base, start_offset, end_offset) }
}
`,
  },
  {
    id: 'P7',
    mustFlag: false,
    purpose: 'zeros are safe multiples',
    source: `fn probe(base: *mut u8) {
    assert!(commit_range(base, 0, 0));
}
`,
  },
  {
    id: 'P8',
    mustFlag: false,
    purpose: 'pageguard:allow marker on the line above',
    source: `fn probe(base: *mut u8) {
    // pageguard:allow — deliberate validation-base oracle (task #906)
    aligned_vmem::decommit(base, PAGE, 2 * PAGE);
}
`,
  },
  {
    id: 'P9',
    mustFlag: false,
    purpose: 'immediate .unwrap_err() across a newline (S3b window; both args are 4096-multiples and ordered, so ONLY S3 can suppress)',
    source: `fn probe(base: *mut u8) {
    assert!(
        try_commit_range(base, 2 * PAGE, 4 * PAGE)
            .unwrap_err()
            .is_invalid_argument()
    );
}
`,
  },
  {
    id: 'P10',
    mustFlag: false,
    purpose: 'inverted range (S2b), no marker',
    source: `fn probe(base: *mut u8) {
    decommit(base, PAGE, 0);
}
`,
  },
  {
    id: 'P11',
    mustFlag: false,
    purpose: 'try_ reserve form with a runtime var',
    source: `const MIB: usize = 1024 * 1024;

fn probe(ps: usize) {
    let r = try_reserve_aligned_lazy(4 * MIB, 4 * MIB, ps).expect("x");
}
`,
  },
  {
    id: 'P12',
    mustFlag: false,
    purpose: 'the original F4 literal 4097: non-floor value, S2a suppresses',
    source: `fn probe(r: &mut Reservation) {
    assert!(r.try_commit_range(0, 4097));
}
`,
  },
];

function runSelfTest() {
  let bad = 0;
  for (const fx of FIXTURES) {
    const { findings } = scanSource(`__selftest__/${fx.id}.rs`, fx.source);
    const ok = fx.mustFlag ? findings.length >= 1 : findings.length === 0;
    if (!ok) bad++;
    console.log(`  [${ok ? 'ok' : 'BAD'}] ${fx.id} — ${fx.purpose}${ok ? '' : ` (got ${findings.length} finding(s))`}`);
  }
  return bad;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tree walk + main
// ─────────────────────────────────────────────────────────────────────────────

function walkRs(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name)) continue;
      walkRs(full, out);
    } else if (entry.isFile() && entry.name.endsWith('.rs')) {
      out.push(full);
    }
  }
  return out;
}

function main() {
  console.log(`[${SCRIPT}] repo: ${REPO_ROOT}`);
  console.log(`[${SCRIPT}] phase 1/2: fixture self-test (a guard that never fired is unproven)\n`);
  const bad = runSelfTest();
  if (bad > 0) {
    console.log(`\n[${SCRIPT}] FAIL — ${bad} self-test fixture(s) disagreed with the detector; fix the guard, not the fixtures.`);
    process.exit(1);
  }

  console.log(`\n[${SCRIPT}] phase 2/2: scanning the real tree from the repo root\n`);
  const totals = { calls: 0, candidates: 0, s1: 0, s2a: 0, s2b: 0, s3: 0 };
  const findings = [];
  const files = walkRs(REPO_ROOT);
  for (const file of files) {
    const rel = relative(REPO_ROOT, file).split('\\').join('/');
    const r = scanSource(rel, readFileSync(file, 'utf8'));
    findings.push(...r.findings);
    for (const k of Object.keys(totals)) totals[k] += r.stats[k];
  }

  for (const f of findings) {
    console.log(
      `  FAIL ${f.path}:${f.line} — ${f.name}(...) arg ${f.argIdx} '${f.argText}' = ${f.value} is not a multiple of 65536 (64 KiB); ` +
        `page_size()-validated positions reject it on 16/64 KiB-page hosts (e.g. macOS ARM64 CI)`,
    );
  }

  const suppressed = totals.s1 + totals.s2a + totals.s2b + totals.s3;
  console.log(
    `\n[${SCRIPT}] scanned ${files.length} file(s), examined ${totals.calls} call site(s); ` +
      `${totals.candidates} candidate arg(s) raised: ${suppressed} suppressed ` +
      `(S1(marker)=${totals.s1} S2a(non-floor)=${totals.s2a} S2b(inverted)=${totals.s2b} ` +
      `S3(negative-ctx)=${totals.s3}), ${findings.length} finding(s).`,
  );

  if (findings.length > 0) {
    console.log(
      `\n[${SCRIPT}] FAIL — fix the call (use page_size()/ps-derived values, or a 64 KiB multiple), ` +
        `or mark the deliberate site with a \`// pageguard:allow\` comment on the call line.`,
    );
    process.exit(1);
  }
  console.log(`\n[${SCRIPT}] ALL GREEN`);
  process.exit(0);
}

main();
