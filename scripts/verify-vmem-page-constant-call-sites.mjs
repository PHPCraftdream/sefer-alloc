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
// ## Production provenance rule (task #1080)
//
// The fold-only flow above has a structural blind spot in production: every
// `src/**` call site passes VARIABLES (`initial_commit`, `start_offset`, ...)
// into the validated positions, so nothing ever folds and the guard raised
// ZERO candidates from `src/` — a green run said nothing about production.
// Task #1074's real escape (raw `meta_end + LAZY_FIRST_CHUNK` sums) was
// invisible to it for exactly this reason.
//
// For files under `src/` only, every scanned argument at every scanned call
// site must additionally carry PROVENANCE, independently of (and in addition
// to) the fold flow, which keeps working unchanged over the whole repo:
//   - strict   — argument 3 of the free-function reserve family
//                (`reserve_aligned_lazy` / `try_reserve_aligned_lazy`): the
//                task #1074 `initial_commit` position. Must fold, be an
//                approved terminal, or RESOLVE to one; a raw expression
//                here FAILS.
//   - pagefree — every other scanned position (reserve arg 1, range-family
//                free args 2,3, method args 1,2): same resolution, but an
//                opaque PAGE-free expression is ACCEPTED (an
//                invariant-carried value); only a textual `PAGE` token fails
//                (word-boundary match, so `MAX_REALISTIC_PAGE_SIZE` does not
//                trip it) — the task #1077 class.
//
// Resolution recursions, depth budget 4: a bare lowercase identifier that is
// a PARAMETER of its enclosing fn resolves through ALL textual callers of
// that fn across the src/ index (every actual at the parameter's position
// must qualify); any other bare identifier resolves through ALL its same-
// file `let` bindings (over-approximation is deliberate — one bad same-named
// local flags; Rust has no same-scope shadowing across fns, so this is
// conservative in the safe direction). Approved terminal forms (both modes):
// a qualified `lazy_initial_commit(..)` call (the task #1074 rounding
// helper, any arguments), `..page_size()` itself, or
// `align_up(x, ..page_size())` (last argument checked).
//
// Prod findings reuse suppressions S1 (marker) and S3 (negative context),
// and are deduplicated against fold findings by (line, arg index).
//
// The ONE in-tree marker is `src/alloc_core/os.rs`'s `reserve_capacity_exact`
// call (see the comment block above it): both arguments are value-proven
// runtime-page multiples but form-opaque to the walker — `reserved_len` is
// `usable.saturating_mul(4).min(16 * SEGMENT).max(usable)` (integer ×4 /
// min / max over page-multiples preserves page-multiplicity), and
// `initial_commit` is `usable`, whose cfg-active `exact-span-large` arm is
// `align_up(needed, aligned_vmem::page_size())` and whose feature-OFF arm is
// a whole-SEGMENT multiple (4 MiB is a multiple of every supported runtime
// page ≤ 64 KiB); the guard reads text without cfg-evaluation, so both arms
// are checked. Runtime-pinned by tests/large_reserved_capacity.rs (task
// #1077's 64 KiB-multiple boundary assertion) and by
// `validate_initial_commit` itself. The runtime complement to this whole
// rule is tests/lazy_initial_commit_forced_page.rs (forced-page regression).
//
// ## KNOWN BLIND SPOTS (do not treat a green run as proof of absence)
//
//   - Method-call RHS opacity: a PAGE-free method or qualified call (e.g.
//     `meta.committed_payload_end_of()`, `SegLayout::small_meta_end()`) is
//     accepted without walking its body — unless it is reached via a
//     param/binding chain that leads back to a raw `PAGE` token or (in
//     strict mode) to a non-approved raw expression.
//   - cfg-blindness is DELIBERATE: the guard reads text without cfg
//     evaluation, so cfg'd-OUT arms are checked too (both arms of
//     `alloc_core_large.rs`'s `usable` are qualified — see the os.rs marker
//     for the one site where this matters).
//   - `crates/**` remains fold-only: the production provenance rule covers
//     `src/` only, so a `let initial = PAGE;` in a non-src/ test (e.g.
//     `crates/aligned-vmem/tests/lazy_commit.rs`'s `#[cfg(windows)]`-gated
//     `windows_lazy_reserve_saves_commit_charge`, valid only because
//     runtime pages are 4 KiB there) is still invisible.
//   - Unresolvable identifiers (struct fields, captured variables) are
//     FLAGGED rather than waved through — conservative in the safe
//     direction.
//   - `#[should_panic]` and `matches!(x, Err(_))` negative forms are not
//     recognized (none exist in-tree today).
//   - String/char-literal blanking is heuristic (raw-string corner cases; a
//     char literal containing `"` is handled, exotic escapes may not be).
//   - On a hypothetical >64 KiB-page host even `16 * PAGE` fails at runtime
//     but passes this guard — the bar is 64 KiB, not "any page size".
//
// ## Summary-line counter semantics (task #1083)
//
// The fold flow and the production-provenance flow keep SEPARATE counter
// buckets, and each set is a true partition:
//   - fold flow: candidates = S1 + S2a + S2b + S3 + findings — every
//     candidate arg is attributed to exactly one bucket, the FIRST
//     suppression stage that fires (S1 filters per candidate; S2a/S2b/S3
//     take the whole surviving set and stop the pipeline for that call).
//   - prod flow: prodChecked = prodQualified + prodS1 + prodS3 +
//     prodDeduped + prodFindings.
// main() ASSERTS both identities before printing them, so a future counter
// drift fails the run loudly instead of printing a self-contradictory
// summary (the pre-#1083 bug: prod-flow S1/S3 events shared the fold
// counters, printing "53 suppressed" over 52 candidate arg(s) raised).
//
// ## Self-test
//
// Before scanning the real tree, the detector runs over embedded fixture
// Rust sources through the SAME scanSource(path, source, prodCtx) code path
// the file walker uses. A guard that never fired is unproven; the fixtures are
// the firing proof, and they run FIRST, always. Prod-rule fixtures (F13+,
// P13+) use `src/__selftest__/<id>.rs` paths so the production provenance rule
// engages, and their caller-search index is built from ALL prod-fixture
// sources, so the cross-file fixtures (F15a/b, P15a/b, P18a/b) resolve each
// other.
//
// Usage:
//   node scripts/verify-vmem-page-constant-call-sites.mjs
//   npm run check   (wired in alongside the other verify-* guards)

import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { REPO_ROOT } from './lib.mjs';

const SCRIPT = 'verify-vmem-page-constant-call-sites';
const BAR = 65536; // 64 KiB — see "The 64 KiB bar" above
const FLOOR = 4096; // compile-time PAGE floor — S2a's threshold
const MARKER = 'pageguard:allow';

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

// ─────────────────────────────────────────────────────────────────────────────
// Production provenance rule (task #1080) — see the header section. Active
// only for files whose repo-relative path starts with `src/`. `qualify`
// decides whether ONE scanned argument carries acceptable provenance;
// `mode` is 'strict' (reserve arg 3) or 'pagefree' (every other position).
// ─────────────────────────────────────────────────────────────────────────────

const PROD_PREFIX = 'src/';
const PROD_DEPTH_BUDGET = 4;
const RE_LAZY_INITIAL_COMMIT = /^(?:[A-Za-z_][A-Za-z0-9_]*::)*lazy_initial_commit\s*\(/;
const RE_PAGE_SIZE_CALL = /^(?:[A-Za-z_][A-Za-z0-9_]*::)*page_size\s*\(\s*\)$/;
const RE_ALIGN_UP_CALL = /^(?:[A-Za-z_][A-Za-z0-9_]*::)*align_up\s*\(/;
const RE_LOWER_IDENT = /^[a-z_][A-Za-z0-9_]*$/;
const RE_PAGE_TOKEN = /\bPAGE\b/; // `_` is a word char: MAX_REALISTIC_PAGE_SIZE does NOT match
const RE_FN_DEF = /\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/g;

/** Per-file `const NAME: usize` defs of a blanked source (shared by the
 * fold flow's per-file map and the cross-file prod index). */
function collectDefs(blanked) {
  const defs = new Map();
  for (const m of blanked.matchAll(CONST_DEF_RE)) defs.set(m[1], m[2].trim());
  return defs;
}

/** The last `fn NAME(` in `blanked` starting before `offset`, with its
 * parameter names (leading `mut`/`&`/`&mut` stripped; non-identifier
 * patterns like `self` skipped). Null when no fn encloses the offset. */
function findEnclosingFn(blanked, offset) {
  let best = null;
  for (const m of blanked.matchAll(RE_FN_DEF)) {
    if (m.index >= offset) break;
    best = m;
  }
  if (!best) return null;
  const pl = extractArgs(blanked, best.index + best[0].length - 1);
  const params = [];
  if (pl) {
    pl.args.forEach((p) => {
      const t = p.text.replace(/^(?:&\s*mut\s+|&\s*|mut\s+)/, '').trim();
      const id = /^[A-Za-z_][A-Za-z0-9_]*/.exec(t);
      // `self` is the receiver, not a positional parameter: a method call
      // `x.foo(a, b)` does not pass it inside the parentheses, so counting
      // it would shift every later parameter's position by one.
      if (!id || id[0] === 'self') return;
      params.push({ name: id[0], pos: params.length + 1 });
    });
  }
  return { name: best[1], params };
}

/** ALL `let (mut )?IDENT (: <type>)? = <RHS>;` bindings of `ident` in
 * `blanked`. The RHS is captured by scanning forward from the `=` while
 * tracking ()/[]/{} depth and stopping at the first depth-0 `;`, so
 * multi-line (and block) RHS forms are captured whole. */
function letBindingsOf(blanked, ident) {
  const out = [];
  const re = new RegExp(`\\blet\\s+(?:mut\\s+)?${ident}\\b`, 'g');
  for (const m of blanked.matchAll(re)) {
    let i = m.index + m[0].length;
    while (i < blanked.length && /\s/.test(blanked[i])) i++;
    if (blanked[i] === ':') {
      // optional `: <type>` — scan to the `=` that ends it
      const eq = blanked.indexOf('=', i);
      if (eq === -1) continue;
      i = eq;
    }
    if (blanked[i] !== '=' || blanked[i + 1] === '=') continue;
    let depth = 0;
    let end = -1;
    for (let j = i + 1; j < blanked.length; j++) {
      const ch = blanked[j];
      if (ch === '(' || ch === '[' || ch === '{') depth++;
      else if (ch === ')' || ch === ']' || ch === '}') {
        depth--;
        if (depth < 0) break;
      } else if (ch === ';' && depth === 0) {
        end = j;
        break;
      }
    }
    if (end === -1) continue;
    let s = i + 1;
    let e = end;
    while (s < e && /\s/.test(blanked[s])) s++;
    while (e > s && /\s/.test(blanked[e - 1])) e--;
    if (e > s) out.push({ rhs: blanked.slice(s, e), rhsStart: s });
  }
  return out;
}

/** ALL textual call sites of `fnName` across the cross-file index, excluding
 * the definitions themselves (a match whose preceding non-space token is
 * `fn`). Uppercase-qualified callers (`Segment::reserve_lazy(..)`) ARE
 * callers here — the skip-form rule is about the scanned vmem API, not
 * about wrapper functions. */
function callerSitesOf(files, fnName) {
  const sites = [];
  const re = new RegExp(`\\b${fnName}\\s*\\(`, 'g');
  for (const fe of files) {
    for (const m of fe.blanked.matchAll(re)) {
      let i = m.index - 1;
      while (i >= 0 && /\s/.test(fe.blanked[i])) i--;
      let j = i;
      while (j >= 0 && /[A-Za-z0-9_]/.test(fe.blanked[j])) j--;
      if (fe.blanked.slice(j + 1, i + 1) === 'fn') continue; // the definition
      const call = extractArgs(fe.blanked, m.index + m[0].length - 1);
      if (!call) continue;
      sites.push({ ctx: { rel: fe.rel, blanked: fe.blanked, defs: fe.defs, files }, args: call.args });
    }
  }
  return sites;
}

/** Provenance decision for one scanned argument (task #1080). `offset` is
 * the argument's start offset in `fileCtx.blanked` — the enclosing-fn
 * search needs it. Returns {ok, reason?}. */
function qualify(exprText, fileCtx, depth, mode, offset) {
  if (depth > PROD_DEPTH_BUDGET) return { ok: false, reason: 'unresolved within depth budget' };
  const t = exprText.trim();
  // Folded = auditable; the existing 64 KiB-bar candidate flow owns folded
  // values (its own suppressions and findings apply).
  if (foldExpr(t, fileCtx.defs) !== null) return { ok: true };
  // Approved terminal forms.
  if (RE_LAZY_INITIAL_COMMIT.test(t)) return { ok: true };
  if (RE_PAGE_SIZE_CALL.test(t)) return { ok: true };
  const au = RE_ALIGN_UP_CALL.exec(t);
  if (au) {
    const inner = extractArgs(t, au[0].length - 1);
    if (inner && inner.args.length > 0 && RE_PAGE_SIZE_CALL.test(inner.args[inner.args.length - 1].text)) {
      return { ok: true };
    }
  }
  // Bare lowercase identifier → parameter mode or binding mode.
  if (RE_LOWER_IDENT.test(t)) {
    const fn = findEnclosingFn(fileCtx.blanked, offset);
    if (!fn) return { ok: false, reason: 'no enclosing fn' };
    const param = fn.params.find((p) => p.name === t);
    if (param) {
      const sites = callerSitesOf(fileCtx.files, fn.name);
      if (sites.length === 0) {
        return { ok: false, reason: `parameter ${t} has no textual caller` };
      }
      for (const site of sites) {
        const actual = site.args[param.pos - 1];
        if (!actual) {
          return { ok: false, reason: `caller of ${fn.name} supplies no argument at parameter position ${param.pos}` };
        }
        const r = qualify(actual.text, site.ctx, depth + 1, mode, actual.start);
        if (!r.ok) return r;
      }
      return { ok: true };
    }
    const binds = letBindingsOf(fileCtx.blanked, t);
    if (binds.length === 0) {
      return { ok: false, reason: `identifier ${t} has no binding and is not a parameter` };
    }
    for (const b of binds) {
      const r = qualify(b.rhs, fileCtx, depth + 1, mode, b.rhsStart);
      if (!r.ok) return r;
    }
    return { ok: true };
  }
  // Opaque non-identifier expression that did not fold.
  if (mode === 'strict') {
    return { ok: false, reason: 'raw expression in the initial_commit position is not an approved rounding form' };
  }
  if (RE_PAGE_TOKEN.test(t)) {
    return { ok: false, reason: 'compile-time PAGE token in a page_size()-validated position (task #1077 class)' };
  }
  return { ok: true }; // invariant-carried value — opaque but PAGE-free
}

/** Scan one Rust source (fixture or real file) through the exact same path.
 * Returns { findings, prodFindings, stats }; `path` is only used for
 * reporting. `prodCtx` ({ files: [{ rel, blanked, defs }] }) is the
 * cross-file index for the production provenance rule; when absent for a
 * `src/` path, a single-file index is built from the source itself. */
function scanSource(path, source, prodCtx) {
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
  const defs = collectDefs(blanked);

  const stats = {
    calls: 0, candidates: 0,
    // Fold-flow buckets (a true partition of `candidates`, task #1083):
    s1: 0, s2a: 0, s2b: 0, s3: 0,
    // Prod-flow buckets (a true partition of `prodChecked`, task #1083) —
    // kept SEPARATE from the fold buckets so prod-flow suppression events
    // can never leak into the fold sentence's sum again.
    prodChecked: 0, prodQualified: 0, prodS1: 0, prodS3: 0, prodDeduped: 0, prodFindings: 0,
  };
  const findings = [];
  const prodFindings = [];
  const prodPending = [];

  // Production provenance rule (task #1080): `src/` files only. The scanned
  // file must be able to see itself in the index (same-file wrapper callers).
  const isProd = path.startsWith(PROD_PREFIX);
  let selfCtx = null;
  if (isProd) {
    const files = prodCtx ? prodCtx.files.slice() : [];
    if (!files.some((f) => f.rel === path)) files.push({ rel: path, blanked, defs });
    selfCtx = { rel: path, blanked, defs, files };
  }

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

      // Production provenance rule (task #1080) — src/ files only; EVERY
      // scanned argument at EVERY scanned (non-skip) call site, independently
      // of the fold-candidate flow below (which keeps running unchanged).
      if (isProd) {
        const isReserveFree =
          form === 'free' && (name === 'reserve_aligned_lazy' || name === 'try_reserve_aligned_lazy');
        for (const idx of argIndexes) {
          const a = call.args[idx - 1];
          if (!a) continue;
          stats.prodChecked++;
          const mode = isReserveFree && idx === 3 ? 'strict' : 'pagefree';
          const q = qualify(a.text, selfCtx, 0, mode, a.start);
          if (q.ok) {
            stats.prodQualified++;
            continue;
          }
          // S1 — same original-text marker window as the fold flow; counted
          // in the PROD bucket (task #1083): a prod-flow suppression is not
          // a fold candidate and must not enter the fold sentence's sum.
          const argLine = lineOf(a.start);
          const lines = new Set([callLine, callLine - 1, callLine - 2, argLine]);
          const marked = [...lines].some((ln) => ln >= 1 && (originalLines[ln - 1] ?? '').includes(MARKER));
          if (marked) {
            stats.prodS1++;
            continue;
          }
          // S3 — negative-assertion context, same as the fold flow (PROD bucket).
          if (negativeContext(blanked, nameStart, call.closeParen)) {
            stats.prodS3++;
            continue;
          }
          prodPending.push({ line: callLine, argIdx: idx, path, name, argText: a.text, reason: q.reason });
        }
      }

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

  // Deduplicate the prod findings against the fold flow by (line, argIdx):
  // if the fold flow already emitted an unsuppressed finding for the same
  // argument, the prod rule must not double-report it.
  for (const p of prodPending) {
    if (findings.some((f) => f.line === p.line && f.argIdx === p.argIdx)) {
      // Already reported by the fold flow for the same (line, arg) — its own
      // bucket in the prod partition (task #1083).
      stats.prodDeduped++;
      continue;
    }
    prodFindings.push(p);
    stats.prodFindings++;
  }
  return { findings, prodFindings, stats };
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
    id: 'F13',
    mustFlag: true,
    prod: true,
    purpose: 'task #1080/#1074 revert shape — raw meta_end + LAZY_FIRST_CHUNK binding flows into initial_commit (strict, binding mode)',
    source: `fn probe() {
    let meta_end = 73728;
    let initial_commit = meta_end + LAZY_FIRST_CHUNK;
    let r = vmem::reserve_aligned_lazy(4194304, 4194304, initial_commit).expect("x");
}
`,
  },
  {
    id: 'F14',
    mustFlag: true,
    prod: true,
    purpose: 'task #1077 shape — align_up(.., os::PAGE) binding flows into a range position (pagefree; arg 2 folds to a 64 KiB multiple so ONLY the prod rule fires)',
    source: `fn probe(base: *mut u8, required_end: usize) {
    let new_span_usable = align_up(required_end, os::PAGE).min(4194304);
    vmem::commit_range(base, 65536, new_span_usable);
}
`,
  },
  {
    id: 'F15a',
    mustFlag: true,
    prod: true,
    purpose: 'task #1080 param→caller chain — the wrapper boundary arg resolves through ANOTHER fixture file\'s raw-sum binding (strict)',
    source: `pub(crate) fn reserve_lazy(initial_commit: usize) -> Option<Reservation> {
    let reservation = vmem::reserve_aligned_lazy(4194304, 4194304, initial_commit)?.into_reservation();
    Some(reservation)
}
`,
  },
  {
    id: 'F15b',
    mustFlag: false,
    prod: true,
    purpose: "F15a's sole textual caller — no scanned API call of its own; exists so the cross-file index resolves F15a",
    source: `fn probe() {
    let initial_commit = 192512 + LAZY_FIRST_CHUNK;
    let r = reserve_lazy(initial_commit).expect("x");
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
  {
    id: 'P13',
    mustFlag: false,
    prod: true,
    purpose: 'current production shape (alloc_core_small.rs) — identifier whose binding calls the approved lazy_initial_commit helper',
    source: `fn probe() {
    let meta_end = SegLayout::small_meta_end();
    let initial_commit = SegLayout::lazy_initial_commit(meta_end, aligned_vmem::page_size());
    let r = aligned_vmem::reserve_aligned_lazy(4194304, 4194304, initial_commit).expect("x");
}
`,
  },
  {
    id: 'P14',
    mustFlag: false,
    prod: true,
    purpose: 'align_up(x, aligned_vmem::page_size()) directly in the initial_commit position (approved terminal)',
    source: `fn probe(needed: usize) {
    let r = vmem::reserve_aligned_lazy(4194304, 4194304, align_up(needed, aligned_vmem::page_size())).expect("x");
}
`,
  },
  {
    id: 'P15a',
    mustFlag: false,
    prod: true,
    purpose: 'os.rs wrapper shape — range args are parameters resolved through a cross-file caller with PAGE-free bindings',
    source: `fn commit_pages(base: *mut u8, start_offset: usize, end_offset: usize) -> bool {
    unsafe { vmem::commit_range(base, start_offset, end_offset) }
}
`,
  },
  {
    id: 'P15b',
    mustFlag: false,
    prod: true,
    purpose: "P15a's sole textual caller — frontier identifiers bound to a method call and a PAGE-free align_up chain",
    source: `fn probe(base: *mut u8, meta: &Meta, carve_end: usize) {
    let frontier = meta.committed_payload_end_of();
    let new_frontier = align_up(carve_end, 262144).min(4194304);
    commit_pages(base, frontier, new_frontier);
}
`,
  },
  {
    id: 'P16',
    mustFlag: false,
    prod: true,
    purpose: 'the PAGE inside MAX_REALISTIC_PAGE_SIZE must NOT match the word-boundary PAGE token check (pagefree opaque-accept)',
    source: `fn probe(base: *mut u8) {
    let span = 65536;
    let required_end = span + 1;
    vmem::commit_range(base, span, align_up(required_end, os::MAX_REALISTIC_PAGE_SIZE));
}
`,
  },
  {
    id: 'P17',
    mustFlag: false,
    prod: true,
    purpose: 'otherwise-flagging src shape suppressed by a pageguard:allow marker on the line above (S1, prod flow)',
    source: `fn probe() {
    let initial_commit = 73728 + LAZY_FIRST_CHUNK;
    // pageguard:allow — deliberate raw-sum boundary shape (fixture P17)
    let r = vmem::reserve_aligned_lazy(4194304, 4194304, initial_commit).expect("x");
}
`,
  },
  {
    id: 'P18a',
    mustFlag: false,
    prod: true,
    purpose: 'os.rs:331 shape — wrapper whose initial_commit param is fed aligned_vmem::page_size() by its sole cross-file caller',
    source: `pub(crate) fn reserve_lazy_for_measurement(initial_commit: usize) -> Option<Reservation> {
    let reservation = vmem::reserve_aligned_lazy(4194304, 4194304, initial_commit)?.into_reservation();
    Some(reservation)
}
`,
  },
  {
    id: 'P18b',
    mustFlag: false,
    prod: true,
    purpose: "P18a's sole textual caller — passes the approved page_size() terminal",
    source: `fn probe() {
    let seg = reserve_lazy_for_measurement(aligned_vmem::page_size())?;
    let _ = seg;
}
`,
  },
];

function runSelfTest() {
  // Prod-rule fixtures engage the production provenance rule via their
  // `src/__selftest__/...` paths; the caller-search index is built from ALL
  // prod-fixture sources so the cross-file fixtures resolve each other.
  const prodCtx = {
    files: FIXTURES.filter((fx) => fx.prod).map((fx) => {
      const blanked = blankRust(fx.source);
      return { rel: `src/__selftest__/${fx.id}.rs`, blanked, defs: collectDefs(blanked) };
    }),
  };
  let bad = 0;
  for (const fx of FIXTURES) {
    const path = `${fx.prod ? 'src/' : ''}__selftest__/${fx.id}.rs`;
    const { findings, prodFindings } = scanSource(path, fx.source, fx.prod ? prodCtx : undefined);
    const total = findings.length + prodFindings.length;
    const ok = fx.mustFlag ? total >= 1 : total === 0;
    if (!ok) bad++;
    console.log(`  [${ok ? 'ok' : 'BAD'}] ${fx.id} — ${fx.purpose}${ok ? '' : ` (got ${total} finding(s))`}`);
  }
  return bad;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tree walk + main
// ─────────────────────────────────────────────────────────────────────────────

// task #1088 (L7) + OH3 (finding I3): the scan set is the TRACKED tree PLUS
// untracked-but-not-gitignored files (`git ls-files --cached` ∪ `git
// ls-files --others --exclude-standard`), not whatever happens to sit on
// this host. Two blind-spot eras preceded this union:
//   - pre-#1088: the walker (readdirSync + a hand-maintained SKIP_DIRS list)
//     consulted no .gitignore, so gitignored scratch copies (a stale
//     `tmp/sefer_backup.rs` etc.) were scanned alongside the real sources —
//     a stale copy could flip the guard RED (or green) with long-fixed call
//     sites, and the summary's "scanned N file(s)" count was host-dependent,
//     NOT reproducible from a clean clone.
//   - #1088/L7 (tracked-only) fixed that but opened the opposite hole: a
//     brand-new source file was invisible until `git add`, and `npm run
//     check` is the PRE-PUSH gate — "new file, not yet staged" is exactly
//     the state a developer is in mid-task, so the guard could go green on
//     a tree containing a fresh violating call site (OH3 finding I3).
// The union closes both at once: gitignored scratch stays excluded
// (clean-clone reproducibility — on a fresh checkout the untracked set is
// empty and the count reduces to the tracked count by construction), while
// an untracked-but-not-ignored .rs file — by this repo's convention a file
// that WILL be committed, since it is not ignored — is scanned from the
// moment it is written. Coverage for it therefore starts at file creation,
// not at `git add`. ls-files output is sorted; the concatenated union is
// re-sorted, so the scan order stays deterministic. Precedent for a guard
// shelling out to git: scripts/verify-commit-prefixes.mjs.
function scanSetRsFiles() {
  const run = (args) => {
    let out;
    try {
      out = execFileSync('git', args, { cwd: REPO_ROOT, encoding: 'utf8' });
    } catch (err) {
      // Fail loudly, never silently fall back to a filesystem walk — a silent
      // fallback would resurrect exactly the host-dependence this function
      // exists to remove.
      throw new Error(
        `[${SCRIPT}] git ${args.join(' ')} failed (${err.message}); the scan set MUST come from git's index/untracked query`,
      );
    }
    return out.split(/\r?\n/).filter(Boolean);
  };
  const tracked = run(['ls-files', '--cached', '--', '*.rs']);
  const untracked = run(['ls-files', '--others', '--exclude-standard', '--', '*.rs']);
  // --cached and --others are disjoint by git's semantics ("others" = NOT in
  // the index); assert it so the printed tracked/untracked split can never
  // double-count a file.
  const seen = new Set(tracked);
  const dup = untracked.filter((f) => seen.has(f));
  if (dup.length > 0) {
    throw new Error(
      `[${SCRIPT}] scan-set invariant violated: ${dup.length} file(s) appeared in BOTH the tracked and untracked lists (${dup[0]} ...) — git semantics guarantee disjointness; fix the query, not this assert.`,
    );
  }
  return { tracked, untracked, union: [...tracked, ...untracked].sort() };
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
  const totals = {
    calls: 0, candidates: 0, s1: 0, s2a: 0, s2b: 0, s3: 0,
    prodChecked: 0, prodQualified: 0, prodS1: 0, prodS3: 0, prodDeduped: 0, prodFindings: 0,
  };
  const findings = [];
  const prodFindings = [];
  // Cross-file index for the production provenance rule (task #1080), built
  // ONCE from the root crate's production tree — the SAME scan set (tracked ∪
  // untracked-not-ignored, OH3/I3) as the main scan below, filtered to src/,
  // so the two can never disagree about what the production tree is.
  const { tracked, untracked, union } = scanSetRsFiles();
  const prodCtx = {
    files: union
      .filter((rel) => rel.startsWith('src/'))
      .map((rel) => {
        const blanked = blankRust(readFileSync(join(REPO_ROOT, rel), 'utf8'));
        return { rel, blanked, defs: collectDefs(blanked) };
      }),
  };
  // Count of files actually read+scanned. In a clean tree this equals
  // tracked.length + untracked.length (on a clean CHECKOUT the untracked
  // query returns nothing, so it equals `git ls-files -- '*.rs' | wc -l`);
  // a file missing from the working tree (deleted without `git rm`) is
  // skipped loudly below and excluded from the count.
  let scanned = 0;
  for (const rel of union) {
    let source;
    try {
      source = readFileSync(join(REPO_ROOT, rel), 'utf8');
    } catch {
      console.log(`[${SCRIPT}] WARNING: tracked file missing on disk, skipped: ${rel}`);
      continue;
    }
    scanned++;
    const r = scanSource(rel, source, rel.startsWith('src/') ? prodCtx : undefined);
    findings.push(...r.findings);
    prodFindings.push(...r.prodFindings);
    for (const k of Object.keys(totals)) totals[k] += r.stats[k];
  }

  for (const f of findings) {
    console.log(
      `  FAIL ${f.path}:${f.line} — ${f.name}(...) arg ${f.argIdx} '${f.argText}' = ${f.value} is not a multiple of 65536 (64 KiB); ` +
        `page_size()-validated positions reject it on 16/64 KiB-page hosts (e.g. macOS ARM64 CI)`,
    );
  }
  for (const f of prodFindings) {
    console.log(
      `  FAIL ${f.path}:${f.line} — ${f.name}(...) arg ${f.argIdx} '${f.argText}' [production provenance] ${f.reason}`,
    );
  }

  // Task #1083 — assert the printed arithmetic BEFORE printing it. Each fold
  // candidate is attributed to exactly one bucket (S1/S2a/S2b/S3/finding) and
  // each prod-checked arg to exactly one bucket (qualified/S1/S3/deduped/
  // finding). A counter drift must fail the run here instead of printing a
  // self-contradictory summary (the pre-#1083 bug printed "53 suppressed"
  // over 52 raised because prod-flow S1/S3 events shared the fold counters).
  const suppressed = totals.s1 + totals.s2a + totals.s2b + totals.s3;
  const foldAccounted = suppressed + findings.length;
  if (foldAccounted !== totals.candidates) {
    throw new Error(
      `[${SCRIPT}] counter invariant violated: ${totals.candidates} candidate arg(s) raised, ` +
        `but the fold buckets account for ${foldAccounted} (${suppressed} suppressed + ` +
        `${findings.length} finding(s)) — every candidate must land in exactly one of ` +
        `S1/S2a/S2b/S3/finding; the scanSource() fold counters have drifted. ` +
        `Fix the counters, not this assert.`,
    );
  }
  const prodSuppressed = totals.prodS1 + totals.prodS3;
  const prodAccounted =
    totals.prodQualified + prodSuppressed + totals.prodDeduped + totals.prodFindings;
  if (prodAccounted !== totals.prodChecked) {
    throw new Error(
      `[${SCRIPT}] counter invariant violated: ${totals.prodChecked} prod arg(s) checked, ` +
        `but the prod buckets account for ${prodAccounted} (${totals.prodQualified} qualified + ` +
        `${prodSuppressed} suppressed + ${totals.prodDeduped} deduped + ` +
        `${totals.prodFindings} finding(s)) — every checked arg must land in exactly one of ` +
        `qualified/S1/S3/deduped/finding; the scanSource() prod counters have drifted. ` +
        `Fix the counters, not this assert.`,
    );
  }
  console.log(
    `\n[${SCRIPT}] scanned ${scanned} file(s) (${tracked.length} tracked + ${untracked.length} untracked-not-ignored .rs; ` +
      `scan set = tracked ∪ untracked-but-not-gitignored, see scanSetRsFiles) — examined ${totals.calls} call site(s); ` +
      `${totals.candidates} candidate arg(s) raised: ${suppressed} suppressed ` +
      `(S1(marker)=${totals.s1} S2a(non-floor)=${totals.s2a} S2b(inverted)=${totals.s2b} ` +
      `S3(negative-ctx)=${totals.s3}), ${findings.length} finding(s); ` +
      `production provenance rule (src/ only): ${totals.prodChecked} arg(s) checked — ` +
      `${totals.prodQualified} qualified, ${prodSuppressed} suppressed ` +
      `(S1(marker)=${totals.prodS1} S3(negative-ctx)=${totals.prodS3}), ` +
      `${totals.prodDeduped} deduped into fold finding(s), ${totals.prodFindings} prod finding(s).`,
  );

  if (findings.length + prodFindings.length > 0) {
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
