// Rust source lexer for the module-dependency-graph tool
// (scripts/module-graph/ — the parse stage lives in parse.mjs).
//
// `lexRust(source)` blanks every comment and every string-ish literal and
// returns `{ code, comments }`:
//
//   - `code` is a string of EXACTLY the same length as `source` in which
//     every character belonging to a comment, string literal, byte string,
//     C string, raw string or char literal is replaced by a space — except
//     `"\n"`, which is preserved exactly where it was. Line numbers and
//     indexes are therefore valid against `code` and the original source
//     interchangeably: the parse stage scans `code` for structure, then
//     reads attribute text (whose quoted strings only exist in `raw`) at
//     the same indexes.
//
//   - `comments` is `[{ kind, text, line }]`: `kind` is `"doc"` for `///`,
//     `//!`, `/** … */` (and `/*! … */`) and `"code"` for ordinary comments;
//     `text` is the body without the marker slashes (one leading space
//     stripped, trailing whitespace trimmed); `line` is the 1-based line
//     where the comment starts, computed against the original source.
//
// Lexing assumptions (each covered by `--selftest`):
//   - Block comments nest, per Rust: `/* a /* b */ c */` is one comment.
//   - `///` is a doc comment only when it is exactly three slashes; four or
//     more slashes are an ordinary comment. `//!`, `/** … */`, `/*! … */`
//     are doc comments.
//   - At `'` the quote opens a char literal iff the text matches
//     `'(\\.|[^\\'])'` (one escaped char, or one non-quote/non-backslash
//     char, then a closing quote) or the unicode-escape form `'\u{…}'`;
//     otherwise it starts a lifetime — nothing is blanked and scanning
//     resumes after the quote character.
//   - The string prefixes `r"…"`, `r#"…"#`, `b"…"`, `br#"…"#`, `c"…"`,
//     `cr#"…"#`, `b'…'` are recognised only when the prefix character is
//     not preceded by an identifier character (so `mur"x"` lexes as the
//     identifier `mur` plus an ordinary string).
//
// `lineOf(source, index)` returns the 1-based line of a source index; it
// builds the newline index lazily per invocation (no cross-call state, so
// the function stays pure) and binary-searches it.
//
// Usage: `node scripts/module-graph/lexer.mjs --selftest` runs the in-memory
// self-test. Importing the module prints nothing.

import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

/** ASCII/Unicode identifier-continue test (sufficient for this codebase). */
function isIdentContinue(ch) {
  if (ch === undefined) return false;
  const c = ch.codePointAt(0);
  return (c >= 97 && c <= 122) || (c >= 65 && c <= 90) || (c >= 48 && c <= 57) || c === 95 || c > 127;
}

/** Blank chars[from, to) with spaces, preserving `\n` so lines stay put. */
function blankChars(chars, from, to) {
  const end = Math.min(to, chars.length);
  for (let k = Math.max(from, 0); k < end; k++) {
    if (chars[k] !== "\n") chars[k] = " ";
  }
}

/**
 * Blank a plain `"…"` string starting at the opening quote at index `q`,
 * honouring backslash escapes (which include `\"`, `\\`, `\n` and the
 * backslash-newline continuation). Returns the index just past the string.
 */
function scanQuoted(source, chars, q) {
  const n = source.length;
  let j = q + 1;
  while (j < n) {
    const ch = source[j];
    if (ch === "\\") {
      j += 2;
      continue;
    }
    if (ch === "\"") {
      j += 1;
      break;
    }
    j += 1;
  }
  if (j > n) j = n;
  blankChars(chars, q, j);
  return j;
}

/**
 * Blank a raw string: opening quote at `quote` terminated by `"` followed by
 * exactly `hashes` `#` characters. Returns the index just past the string.
 */
function scanRawString(source, chars, start, quote, hashes) {
  const n = source.length;
  let end = n;
  let j = quote + 1;
  while (j < n) {
    if (source[j] === "\"") {
      let k = j + 1;
      while (k < n && source[k] === "#" && k - (j + 1) < hashes) k += 1;
      if (k - (j + 1) === hashes) {
        end = j + 1 + hashes;
        break;
      }
    }
    j += 1;
  }
  blankChars(chars, start, end);
  return end;
}

/**
 * At a `r`/`b`/`c` character (never preceded by an identifier char), try to
 * consume a raw/byte/C string (or byte char). Returns the index just past the
 * literal, or -1 when the characters do not form one.
 */
function scanPrefixedLiteral(source, chars, i) {
  const c = source[i];
  let raw = false;
  let p = i + 1;
  if (c === "r") {
    raw = true;
  } else if (c === "b" || c === "c") {
    if (source[p] === "r") {
      raw = true;
      p += 1;
    }
  }
  if (raw) {
    let hashes = 0;
    while (source[p] === "#") {
      hashes += 1;
      p += 1;
    }
    if (source[p] !== "\"") return -1;
    return scanRawString(source, chars, i, p, hashes);
  }
  if (source[p] === "\"") return scanQuoted(source, chars, p);
  if (c === "b" && source[p] === "'" && source[p + 2] === "'") {
    blankChars(chars, i, p + 3);
    return p + 3;
  }
  return -1;
}

/**
 * At a `'` character: blank a char literal (`'x'`, `'\n'`, `'\u{…}'`) or skip
 * a lifetime quote (`'a`, `'static`). Returns the index to resume scanning at.
 */
function scanQuoteOrLifetime(source, chars, i) {
  const c1 = source[i + 1];
  const c2 = source[i + 2];
  const c3 = source[i + 3];
  if (c1 === "\\") {
    // A lifetime can never start with an escape, so escapes are char literals.
    if (c2 === "u" && c3 === "{") {
      const brace = source.indexOf("}", i + 3);
      if (brace > 0 && source[brace + 1] === "'") {
        blankChars(chars, i, brace + 2);
        return brace + 2;
      }
      return i + 1; // malformed escape: resume after the quote, blank nothing
    }
    if (c3 === "'") {
      blankChars(chars, i, i + 4);
      return i + 4;
    }
    const close = source.indexOf("'", i + 2);
    if (close > 0) {
      blankChars(chars, i, close + 1);
      return close + 1;
    }
    return i + 1;
  }
  if (c1 !== undefined && c1 !== "'" && c2 === "'") {
    blankChars(chars, i, i + 3);
    return i + 3;
  }
  return i + 1; // lifetime: keep the quote, continue after it
}

/**
 * Lex Rust source. See the file header for the returned shape and the
 * lexing assumptions. Pure: no module state, no side effects.
 *
 * @param {string} source full text of a .rs file
 * @returns {{ code: string, comments: Array<{kind: string, text: string, line: number}> }}
 */
export function lexRust(source) {
  const n = source.length;
  const chars = source.split("");
  const comments = [];
  let i = 0;
  while (i < n) {
    const c = source[i];
    if (c === "/" && source[i + 1] === "/") {
      let k = i;
      while (source[k] === "/") k += 1;
      const slashes = k - i;
      const after = source[k];
      const isDoc = slashes === 3 || (slashes === 2 && after === "!");
      let eol = source.indexOf("\n", k);
      if (eol < 0) eol = n;
      let text = source.slice(k, eol);
      if (slashes === 2 && after === "!") text = text.replace(/^!/, "");
      if (text.startsWith(" ")) text = text.slice(1);
      comments.push({
        kind: isDoc ? "doc" : "code",
        text: text.replace(/\r$/, "").trimEnd(),
        line: lineOf(source, i),
      });
      blankChars(chars, i, eol);
      i = eol;
      continue;
    }
    if (c === "/" && source[i + 1] === "*") {
      const isDoc = (source[i + 2] === "*" && source[i + 3] !== "/") || source[i + 2] === "!";
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        if (source[j] === "/" && source[j + 1] === "*") {
          depth += 1;
          j += 2;
        } else if (source[j] === "*" && source[j + 1] === "/") {
          depth -= 1;
          j += 2;
        } else {
          j += 1;
        }
      }
      const close = depth === 0 ? j - 2 : n;
      const bodyStart = isDoc ? i + 3 : i + 2; // skip the extra * / ! doc marker
      let text = source.slice(bodyStart, Math.max(close, bodyStart));
      if (text.startsWith(" ")) text = text.slice(1);
      comments.push({ kind: isDoc ? "doc" : "code", text: text.trimEnd(), line: lineOf(source, i) });
      blankChars(chars, i, j);
      i = j;
      continue;
    }
    if (c === "\"") {
      i = scanQuoted(source, chars, i);
      continue;
    }
    if (c === "'") {
      i = scanQuoteOrLifetime(source, chars, i);
      continue;
    }
    if ((c === "r" || c === "b" || c === "c") && (i === 0 || !isIdentContinue(source[i - 1]))) {
      const end = scanPrefixedLiteral(source, chars, i);
      if (end >= 0) {
        i = end;
        continue;
      }
    }
    i += 1;
  }
  return { code: chars.join(""), comments };
}

/**
 * 1-based line number of `index` in `source`. Builds the newline index once
 * per invocation and binary-searches it, so repeated calls stay cheap while
 * the function remains pure (no cache shared across calls).
 *
 * @param {string} source
 * @param {number} index
 * @returns {number}
 */
export function lineOf(source, index) {
  const newlines = [];
  for (let k = 0; k < source.length; k++) {
    if (source.charCodeAt(k) === 10) newlines.push(k);
  }
  let lo = 0;
  let hi = newlines.length;
  const at = Math.max(0, Math.min(index, source.length));
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (newlines[mid] < at) lo = mid + 1;
    else hi = mid;
  }
  return lo + 1;
}

// ── self-test (only when run directly: node scripts/module-graph/lexer.mjs --selftest)

function selftest() {
  let checks = 0;
  const ok = (cond, msg) => {
    checks += 1;
    assert.ok(cond, msg);
  };

  // nested block comments are one comment; code around them survives
  let src = "/* a /* b */ c */ fn keep() {}\n";
  let r = lexRust(src);
  ok(r.code.length === src.length, "code keeps source length");
  ok(r.code.includes("fn keep() {}"), "code after nested comment intact");
  ok(!r.code.includes("/") || !r.code.slice(0, r.code.indexOf("fn")).includes("/"), "comment markers blanked");
  ok(r.comments.length === 1 && r.comments[0].kind === "code", "nested block comment is one code comment");
  ok(r.comments[0].text.includes("/* b */"), "nested inner text kept");
  ok(r.comments[0].line === 1, "block comment line 1");

  // multi-line block comment preserves newlines
  src = "/* line1\nline2 */\nfn x() {}\n";
  r = lexRust(src);
  ok(r.code.split("\n").length === src.split("\n").length, "newlines preserved in blanked comment");
  ok(r.code.includes("fn x() {}"), "code after multi-line comment intact");

  // doc block and inner-doc block
  r = lexRust("/** doc block */\nfn f() {}\n");
  ok(r.comments[0].kind === "doc" && r.comments[0].text === "doc block", "/** is doc with trimmed text");
  r = lexRust("/*! inner doc */\n");
  ok(r.comments[0].kind === "doc", "/*! is doc");

  // raw strings with hashes
  src = "let s = r#\"he \"llo\" \"#;\nfn f() {}\n";
  r = lexRust(src);
  ok(r.code.length === src.length, "raw string same length");
  ok(r.code.includes("fn f() {}"), "code after raw string intact");
  ok(!r.code.includes("\""), "raw string quotes blanked");
  ok(r.comments.length === 0, "raw string is not a comment");
  r = lexRust("let t = r##\"x\"##;\n");
  ok(!r.code.includes("\""), "r##…## blanked");

  // byte / C strings, raw byte string, byte char
  src = "let a = b\"xy\"; let b2 = br#\"q\"#; let c2 = c\"z\"; let d2 = cr#\"w\"#; let e2 = b\"q\";\n";
  r = lexRust(src);
  ok(!r.code.includes("\""), "byte/C/raw-byte strings blanked");
  r = lexRust("let g = b\"q\";\n");
  ok(!r.code.includes("'") && !r.code.includes("\""), "byte char blanked");

  // char literal vs lifetime
  src = "let c = 'a';\nlet r2: &'a str = x;\nstatic S: &'static str = y;\n";
  r = lexRust(src);
  ok(r.code.includes("'a str"), "lifetime 'a kept");
  ok(r.code.includes("'static str"), "lifetime 'static kept");
  const qi = src.indexOf("'a'");
  ok(r.code.slice(qi, qi + 3) === "   ", "char literal 'a' blanked");
  r = lexRust("let e = '\\u{1F600}';\n");
  ok(!r.code.includes("'"), "unicode char literal blanked");
  r = lexRust("let h = '\\'';\n");
  ok(!r.code.includes("'"), "escaped quote char blanked");

  // escaped quotes inside strings, backslash-newline continuation
  src = "let s = \"a \\\" b\";\nfn f() {}\n";
  r = lexRust(src);
  ok(!r.code.includes("\""), "escaped quote string fully blanked");
  src = "let s = \"one \\\ntwo\";\nfn f() {}\n";
  r = lexRust(src);
  ok(r.code.split("\n").length === src.split("\n").length, "string line continuation keeps its newline");
  ok(!r.code.includes("\""), "continued string fully blanked");

  // doc vs ordinary line comments
  src = "/// doc here\n//// four slashes\n//! inner doc\n// plain\nfn f() {}\n";
  r = lexRust(src);
  ok(r.comments.length === 4, "four line comments");
  ok(r.comments[0].kind === "doc" && r.comments[0].text === "doc here" && r.comments[0].line === 1, "/// doc");
  ok(r.comments[1].kind === "code" && r.comments[1].text === "four slashes", "//// is ordinary");
  ok(r.comments[2].kind === "doc" && r.comments[2].text === "inner doc" && r.comments[2].line === 3, "//! doc");
  ok(r.comments[3].kind === "code" && r.comments[3].text === "plain" && r.comments[3].line === 4, "// ordinary");

  // lineOf
  src = "a\nbb\nccc\n";
  ok(lineOf(src, 0) === 1, "lineOf start");
  ok(lineOf(src, 2) === 2, "lineOf second line");
  ok(lineOf(src, 5) === 3, "lineOf third line");
  ok(lineOf(src, 9) === 4, "lineOf last line");
  ok(lineOf(src, 0) === lineOf(src, 0), "lineOf pure per invocation");

  console.log(`lexer selftest: ok (${checks} checks)`);
}

function runDirectly() {
  if (!process.argv[1]) return false;
  try {
    return import.meta.url === pathToFileURL(process.argv[1]).href;
  } catch {
    return false;
  }
}

if (runDirectly() && process.argv.includes("--selftest")) selftest();
