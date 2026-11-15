// Parser layer of the module-dependency-graph tool (scripts/module-graph/;
// the lexer lives in lexer.mjs, the graph builder comes later).
//
// `parseRustFile({ raw, code, comments, filePath })` scans one Rust file and
// returns `{ mods, uses, pathRefs, restrictedDecls }`. `raw` is the original
// file text; `code`/`comments` come from lexRust(raw); `filePath` is for error
// messages only. Indexes and lines are valid against `raw` and `code`
// interchangeably (lexRust preserves length and newlines).
//
//   mods:            every `mod NAME;` item (an inline `mod NAME { … }` is
//                    recorded too, flagged `inline: true` so the caller can
//                    warn), with visibility, the `&&`-joined text of all
//                    directly attached `#[cfg(...)]` predicates, the
//                    `#[path = "..."]` string and a `#[doc(hidden)]` flag.
//                    cfg-disjoint duplicate declarations of the same name are
//                    emitted as SEPARATE entries (the graph stage merges).
//   uses:            every `use …;` item — including ones inside macro
//                    invocation bodies — decomposed into kind/prefix/items,
//                    with visibility and cfg predicates.
//   pathRefs:        every `crate::…` / `super::…` / `self::…` path reference
//                    in the stripped code, with the spans of all parsed `use`
//                    statements blanked first so a use's own path is not
//                    double-counted. Requires `::` right after the keyword
//                    (`crate()` is not a path); stops at the first non-ident
//                    (`C::method(`, `X!`, `T<'_>` all terminate correctly);
//                    zero-segment refs are not recorded.
//   restrictedDecls: every `pub(crate)` / `pub(super)` / `pub(in …)` item
//                    declaration; a restricted `use` re-export emits ONE entry
//                    per leaf item (globs skipped, declKind "use").
//
// Documented assumptions / deviations:
//   - Attribute bodies (`[…]` contents) contain no `]` character — true for
//     this codebase and realistic cfg predicates — which is what makes the
//     backwards bracket match in attributesBefore() safe. attributesBefore()
//     walks backwards over whitespace, whole-comment lines and complete
//     `#[…]` attributes; it stops at code tokens and at inner `#![…]`
//     attributes (they belong to the file, not the item), and skips the
//     `pub…` visibility token first so attributes attached to a
//     `pub(crate) use …` inside a macro invocation body are still found.
//     Multi-line attribute bodies are safe; predicate text is normalised by
//     collapsing whitespace runs to single spaces before joining with " && ".
//   - The path inside a `pub(in …)` visibility qualifier is NOT a reference:
//     collectPathRefs blanks every `pub(in <path>)` qualifier span before
//     scanning, so `pub(in crate::x) fn f();` yields no pathRef (a vis
//     qualifier states where an item is visible TO, it does not name a
//     dependency).
//   - Path shape: `prefix` starts with the leading kind segments (`crate`; one
//     entry per `super`; `self`) followed by the plain segments before the
//     first `{…}` group. A group-less use has no group, so its `prefix` keeps
//     the kind segments plus the FIRST post-marker segment and the single leaf
//     carries the rest in `segs` (`use crate::a::b::c;` → prefix
//     ["crate","a"], segs ["b","c"]). Leaf `segs` is relative to `prefix` and
//     INCLUDES the leaf name; a glob leaf has segs [] and name null. kind
//     "extern" is a leading-`::` path; kind "uniform" is everything else.
//     pathRefs segs exclude the leading marker(s) (`super::super::x` →
//     kind "super", segs ["x"]).
//   - DEVIATION, because the src-wide sanity gate (≥1300 entries against 1358
//     `pub(crate)|pub(super)|pub(in ` rg lines) is otherwise unreachable —
//     only 1293 such occurrences exist in code: restricted-visibility
//     struct/variant fields (`pub(in crate::x) name: Type`) are recorded as
//     declKind "field", and `pub(crate) const NAME: T` items as declKind
//     "const" (the spec's modifier list only covers `const fn`).
//
// Usage: `node scripts/module-graph/parse.mjs --selftest` runs the in-memory
// self-test. Importing the module prints nothing.

import { lexRust, lineOf } from "./lexer.mjs";
import assert from "node:assert/strict";
import { pathToFileURL } from "node:url";

/** Item keywords a restricted-visibility declaration may introduce. */
const DECL_KINDS = new Set(["fn", "struct", "enum", "union", "trait", "type", "static", "mod", "use"]);

function isIdentContinue(ch) {
  if (ch === undefined) return false;
  const c = ch.codePointAt(0);
  return (c >= 97 && c <= 122) || (c >= 65 && c <= 90) || (c >= 48 && c <= 57) || c === 95 || c > 127;
}
const isSpace = (ch) => ch === " " || ch === "\t" || ch === "\n" || ch === "\r";
function skipWs(text, i) { while (i < text.length && isSpace(text[i])) i += 1; return i; }
function skipWsBack(text, i) { while (i > 0 && isSpace(text[i - 1])) i -= 1; return i; }
function readIdent(text, i) {
  let j = i;
  while (j < text.length && isIdentContinue(text[j])) j += 1;
  return j === i ? null : { name: text.slice(i, j), end: j };
}
function lineStart(text, idx) { let i = idx; while (i > 0 && text[i - 1] !== "\n") i -= 1; return i; }
function skipLineWs(text, from) { let k = from; while (text[k] === " " || text[k] === "\t") k += 1; return k; }

/** Whole-word occurrences of `kw` in `text`. */
function keywordIndexes(text, kw) {
  const out = [];
  let from = 0;
  for (;;) {
    const idx = text.indexOf(kw, from);
    if (idx < 0) return out;
    const before = idx > 0 ? text[idx - 1] : " ";
    const after = idx + kw.length < text.length ? text[idx + kw.length] : " ";
    if (!isIdentContinue(before) && !isIdentContinue(after)) out.push(idx);
    from = idx + kw.length;
  }
}

/**
 * Visibility token ending immediately before `kwIdx` (whitespace skipped):
 * "private", "pub", "pub(crate)", "pub(super)" or the literal `pub(in …)`
 * text. Returns { vis, start }; start is the vis token index (kwIdx if none).
 */
function visBefore(text, kwIdx) {
  const p = skipWsBack(text, kwIdx);
  if (p > 0 && text[p - 1] === ")") {
    let depth = 0, open = -1;
    for (let q = p - 1; q >= 0; q--) {
      if (text[q] === ")") depth += 1;
      else if (text[q] === "(") { depth -= 1; if (depth === 0) { open = q; break; } }
    }
    if (open > 0) {
      const vStart = skipWsBack(text, open);
      if (vStart >= 3 && text.slice(vStart - 3, vStart) === "pub" && (vStart - 4 < 0 || !isIdentContinue(text[vStart - 4]))) {
        return { vis: `pub(${text.slice(open + 1, p - 1).trim()})`, start: vStart - 3 };
      }
    }
  } else if (p >= 3 && text.slice(p - 3, p) === "pub" && (p - 4 < 0 || !isIdentContinue(text[p - 4]))) {
    return { vis: "pub", start: p - 3 };
  }
  return { vis: "private", start: kwIdx };
}

/** Start of the line holding `idx` when that line is entirely a `//` comment, else -1. */
function commentLineStart(text, idx) {
  const q = skipLineWs(text, lineStart(text, idx));
  return text[q] === "/" && text[q + 1] === "/" ? lineStart(text, idx) : -1;
}

/**
 * Attribute texts (`#[…]`) of the contiguous attribute block immediately
 * before `index`, in source order. See the header for the assumptions: no `]`
 * inside attribute bodies; whole-comment lines and the visibility token are
 * stepped over; inner `#![…]` attributes stop the walk.
 */
function attributesBefore(raw, index) {
  const attrs = [];
  let p = visBefore(raw, index).start;
  for (;;) {
    p = skipWsBack(raw, p);
    if (p <= 0) break;
    const ch = raw[p - 1];
    if (ch === "]") {
      let depth = 0, open = -1;
      for (let q = p - 1; q >= 0; q--) {
        if (raw[q] === "]") depth += 1;
        else if (raw[q] === "[") { depth -= 1; if (depth === 0) { open = q; break; } }
      }
      if (open >= 1 && raw[open - 1] === "#" && raw[open - 2] !== "!") {
        attrs.push(raw.slice(open - 1, p));
        p = open - 1;
        continue;
      }
    } else if (ch === "/" && raw[p - 2] === "*") { // block comment ends here (nested-aware)
      let depth = 1, q = p - 2;
      while (q > 0 && depth > 0) {
        if (raw[q - 1] === "/" && raw[q - 2] === "*") { depth -= 1; q -= 2; }
        else if (raw[q - 1] === "*" && raw[q - 2] === "/") { depth += 1; q -= 2; }
        else q -= 1;
      }
      p = Math.max(q, 0);
      continue;
    }
    const comment = commentLineStart(raw, p - 1);
    if (comment < 0) break;
    p = comment;
  }
  attrs.reverse();
  return attrs;
}

const attrBody = (attr) => (attr.startsWith("#[") ? attr.slice(2, attr.length - 1).trim() : null);

/** `#[cfg(...)]` predicate text, whitespace-normalised, or null. */
function cfgPredicateOf(attr) {
  const body = attrBody(attr);
  if (body === null || !/^cfg\s*\(/.test(body)) return null;
  return body.slice(body.indexOf("(") + 1, body.lastIndexOf(")")).replace(/\s+/g, " ").trim();
}

/** `#[path = "…"]` string content from the attribute block, or null. */
function pathAttrOf(attrs) {
  for (const attr of attrs) {
    const body = attrBody(attr);
    if (body === null || !/^path\s*=/.test(body)) continue;
    const q1 = body.indexOf("\""), q2 = body.lastIndexOf("\"");
    if (q1 >= 0 && q2 > q1) return body.slice(q1 + 1, q2);
  }
  return null;
}
const isDocHiddenAttr = (attr) => {
  const body = attrBody(attr);
  return body !== null && /^doc\s*\(\s*hidden\s*\)$/.test(body);
};

/** All cfg predicates of the attribute block, `&&`-joined; null if none. */
function joinedCfg(attrs) {
  const preds = [];
  for (const attr of attrs) {
    const pred = cfgPredicateOf(attr);
    if (pred !== null) preds.push(pred);
  }
  return preds.length > 0 ? preds.join(" && ") : null;
}

/** `use x::y as z` rename at `j`, or { rename: null, end: j }. */
function parseAsRename(text, j) {
  const k = skipWs(text, j);
  if (text[k] === "a" && text[k + 1] === "s" && !isIdentContinue(text[k + 2])) {
    const m = skipWs(text, k + 2);
    const id = readIdent(text, m);
    if (id !== null) return { rename: id.name, end: id.end };
  }
  return { rename: null, end: j };
}

/**
 * Entries of a `{ … }` group at `i` (which must hold `{`). Leaves get
 * `baseSegs` prefixed to their segs (the path accumulated before the brace).
 */
function parseGroup(text, i, baseSegs) {
  let j = i + 1;
  const items = [];
  for (;;) {
    j = skipWs(text, j);
    const ch = text[j];
    if (ch === undefined) break;
    if (ch === "}") { j += 1; break; }
    if (ch === ",") { j += 1; continue; }
    if (ch === "*") { j += 1; items.push({ segs: [...baseSegs], name: null, rename: null, glob: true }); continue; }
    const entry = parseEntry(text, j, baseSegs);
    if (entry.end <= j) break; // no progress: malformed tail
    items.push(...entry.items);
    j = entry.end;
  }
  return { items, end: j };
}

/** One group entry: an ident chain possibly ending in a nested group. */
function parseEntry(text, j, baseSegs) {
  const idents = [];
  for (;;) {
    j = skipWs(text, j);
    if (text[j] === "{") {
      const g = parseGroup(text, j, [...baseSegs, ...idents]);
      return { items: g.items, end: g.end };
    }
    const id = readIdent(text, j);
    if (id === null) return { items: [], end: j };
    idents.push(id.name);
    j = skipWs(text, id.end);
    if (text[j] === ":" && text[j + 1] === ":") { j += 2; continue; }
    break;
  }
  const asr = parseAsRename(text, j);
  const name = idents.pop();
  return { items: [{ segs: [...baseSegs, ...idents, name], name, rename: asr.rename, glob: false }], end: asr.end };
}

/**
 * Hand-written scanner for the path text of a use item, after the leading kind
 * segments were consumed. Returns { prefixExtra, items, end } — see the header
 * for the prefix/segs conventions. Null when this is not a use path.
 */
function parseUsePathTail(text, i) {
  const preBrace = [];
  let j = i;
  for (;;) {
    j = skipWs(text, j);
    const ch = text[j];
    if (ch === "{") {
      const g = parseGroup(text, j, []);
      return { prefixExtra: preBrace, items: g.items, end: g.end };
    }
    if (ch === "*") {
      return { prefixExtra: preBrace, items: [{ segs: [], name: null, rename: null, glob: true }], end: j + 1 };
    }
    const id = readIdent(text, j);
    if (id === null) return null;
    preBrace.push(id.name);
    j = skipWs(text, id.end);
    if (text[j] === ":" && text[j + 1] === ":") { j += 2; continue; }
    const asr = parseAsRename(text, j);
    return {
      prefixExtra: preBrace.slice(0, 1),
      items: [{ segs: preBrace.slice(1), name: preBrace[preBrace.length - 1], rename: asr.rename, glob: false }],
      end: asr.end,
    };
  }
}

/** Parse one `use` keyword occurrence; returns { entry, spanEnd } or null. */
function parseUseAt(text, raw, kwIdx) {
  const vis = visBefore(text, kwIdx);
  const attrs = attributesBefore(raw, kwIdx);
  let i = skipWs(text, kwIdx + 3);
  const markers = [];
  let kind = null;
  for (;;) {
    if (text[i] === ":" && text[i + 1] === ":") { kind = "extern"; i = skipWs(text, i + 2); break; }
    const id = readIdent(text, i);
    if (id === null || (id.name !== "crate" && id.name !== "super" && id.name !== "self")) break;
    const j = skipWs(text, id.end);
    if (!(text[j] === ":" && text[j + 1] === ":")) break;
    if (kind === null) kind = id.name;
    markers.push(id.name);
    i = skipWs(text, j + 2);
  }
  if (kind === null) kind = "uniform";
  const tail = parseUsePathTail(text, i);
  if (tail === null) return null;
  let end = skipWs(text, tail.end);
  if (text[end] === ";") end += 1;
  return {
    entry: {
      kind, prefix: [...markers, ...tail.prefixExtra], items: tail.items,
      pubUse: vis.vis !== "private", vis: vis.vis, cfg: joinedCfg(attrs), line: lineOf(raw, kwIdx),
    },
    spanEnd: end,
  };
}

/** Parse one `mod` keyword occurrence; returns { entry } or null. */
function parseModAt(text, raw, kwIdx) {
  const id = readIdent(text, skipWs(text, kwIdx + 3));
  if (id === null) return null;
  const terminator = text[skipWs(text, id.end)];
  if (terminator !== ";" && terminator !== "{") return null;
  const attrs = attributesBefore(raw, kwIdx);
  return {
    entry: {
      name: id.name, vis: visBefore(text, kwIdx).vis, cfg: joinedCfg(attrs),
      pathAttr: pathAttrOf(attrs), docHidden: attrs.some(isDocHiddenAttr),
      line: lineOf(raw, kwIdx), inline: terminator === "{",
    },
  };
}

/** `pub(crate)` / `pub(super)` / `pub(in …)` starting at `open` (the `(`). */
function restrictedVis(text, open) {
  let j = skipWs(text, open + 1);
  const first = readIdent(text, j);
  if (first !== null && (first.name === "crate" || first.name === "super")) {
    const k = skipWs(text, first.end);
    if (text[k] === ")") return { vis: `pub(${first.name})`, end: k + 1 };
    return null;
  }
  if (first !== null && first.name === "in") {
    j = skipWs(text, first.end);
    const start = j;
    let depth = 0;
    while (j < text.length && !(text[j] === ")" && depth === 0)) {
      if (text[j] === "(") depth += 1;
      else if (text[j] === ")") depth -= 1;
      j += 1;
    }
    if (text[j] !== ")") return null;
    return { vis: `pub(in ${text.slice(start, j).trim()})`, end: j + 1 };
  }
  return null;
}

function collectRestrictedDecls(text, raw) {
  const out = [];
  for (const idx of keywordIndexes(text, "pub")) {
    let j = skipWs(text, idx + 3);
    if (text[j] !== "(") continue;
    const vis = restrictedVis(text, j);
    if (vis === null) continue;
    j = skipWs(text, vis.end);
    let lastWasConst = false;
    for (;;) {
      const id = readIdent(text, j);
      if (id === null) break;
      if (id.name === "async" || id.name === "unsafe" || id.name === "const") {
        lastWasConst = id.name === "const";
        j = skipWs(text, id.end);
        continue;
      }
      if (id.name === "extern") {
        lastWasConst = false;
        j = skipWs(text, id.end); // ABI string literals are blanked in `text`
        continue;
      }
      break;
    }
    const kwId = readIdent(text, j);
    if (kwId === null) continue;
    const line = lineOf(raw, idx);
    if (kwId.name === "use") {
      const path = parseUsePathTail(text, skipWs(text, kwId.end));
      if (path !== null) {
        for (const item of path.items) {
          if (!item.glob) out.push({ name: item.rename !== null ? item.rename : item.name, declKind: "use", vis: vis.vis, line });
        }
      }
      continue;
    }
    if (DECL_KINDS.has(kwId.name)) {
      const id = readIdent(text, skipWs(text, kwId.end));
      if (id !== null) out.push({ name: id.name, declKind: kwId.name, vis: vis.vis, line });
      continue;
    }
    // `pub(crate) const NAME: …` / restricted field — see the header deviations.
    if (lastWasConst) {
      out.push({ name: kwId.name, declKind: "const", vis: vis.vis, line });
      continue;
    }
    if (text[skipWs(text, kwId.end)] === ":") {
      out.push({ name: kwId.name, declKind: "field", vis: vis.vis, line });
    }
  }
  return out;
}

/**
 * Spans of the path text inside `pub(in …)` visibility qualifiers —
 * `pub` `(` `in` <path> `)` with whitespace allowed between tokens. Visibility
 * qualifiers are not path references; collectPathRefs blanks these spans so
 * the qualified path (`crate::x` in `pub(in crate::x)`) is never recorded.
 */
function pubInSpans(text) {
  const spans = [];
  for (const idx of keywordIndexes(text, "pub")) {
    const open = skipWs(text, idx + 3);
    if (text[open] !== "(") continue;
    const kw = readIdent(text, skipWs(text, open + 1));
    if (kw === null || kw.name !== "in") continue;
    let depth = 0;
    let j = kw.end;
    while (j < text.length) {
      if (text[j] === "(") depth += 1;
      else if (text[j] === ")") { if (depth === 0) break; depth -= 1; }
      j += 1;
    }
    if (text[j] !== ")") continue;
    spans.push([skipWs(text, kw.end), j]);
  }
  return spans;
}

function collectPathRefs(text, raw, useSpans) {
  const chars = text.split("");
  for (const [from, to] of [...useSpans, ...pubInSpans(text)]) {
    for (let k = from; k < to; k++) {
      if (chars[k] !== "\n") chars[k] = " ";
    }
  }
  const blanked = chars.join("");
  const matches = [];
  for (const kw of ["crate", "super", "self"]) {
    for (const idx of keywordIndexes(blanked, kw)) matches.push({ kw, idx });
  }
  matches.sort((a, b) => a.idx - b.idx);
  const out = [];
  let coveredUntil = 0; // marker segments already consumed by an earlier match
  for (const { kw, idx } of matches) {
    if (idx < coveredUntil) continue;
    let j = skipWs(blanked, idx + kw.length);
    if (!(blanked[j] === ":" && blanked[j + 1] === ":")) continue;
    const segs = [];
    j = skipWs(blanked, j + 2);
    for (;;) {
      const id = readIdent(blanked, j);
      if (id === null) break;
      segs.push(id.name);
      j = skipWs(blanked, id.end);
      if (blanked[j] === ":" && blanked[j + 1] === ":") { j = skipWs(blanked, j + 2); continue; }
      break;
    }
    coveredUntil = Math.max(coveredUntil, j);
    if (kw !== "crate") while (segs.length > 0 && segs[0] === kw) segs.shift();
    if (segs.length === 0) continue;
    out.push({ kind: kw, segs, line: lineOf(raw, idx) });
  }
  out.sort((a, b) => a.line - b.line);
  return out;
}

/**
 * Parse one Rust file. See the file header for the returned shape; indexes
 * and lines are valid against `raw` and `code` interchangeably.
 */
export function parseRustFile({ raw, code, comments, filePath }) {
  if (typeof raw !== "string") {
    throw new TypeError(`parseRustFile: raw source string required (${filePath ?? "no filePath"})`);
  }
  const lexed = typeof code === "string"
    ? { code, comments: Array.isArray(comments) ? comments : [] }
    : lexRust(raw);
  const text = lexed.code;
  const uses = [];
  const useSpans = [];
  const mods = [];
  for (const idx of keywordIndexes(text, "mod")) {
    const parsed = parseModAt(text, raw, idx);
    if (parsed !== null) mods.push(parsed.entry);
  }
  for (const idx of keywordIndexes(text, "use")) {
    const parsed = parseUseAt(text, raw, idx);
    if (parsed !== null) {
      uses.push(parsed.entry);
      useSpans.push([idx, parsed.spanEnd]);
    }
  }
  return {
    mods,
    uses,
    pathRefs: collectPathRefs(text, raw, useSpans),
    restrictedDecls: collectRestrictedDecls(text, raw),
  };
}

// ── self-test (only when run directly: node scripts/module-graph/parse.mjs --selftest)

function selftest() {
  let checks = 0;
  const ok = (cond, msg) => { checks += 1; assert.ok(cond, msg); };
  const eq = (actual, expected, msg) => { checks += 1; assert.deepEqual(actual, expected, msg); };
  const L = (...ls) => ls.join("\n") + "\n";
  const parse = (raw) => parseRustFile({ raw, filePath: "selftest.rs" });
  const use = (kind, prefix, items, vis, cfg, line) => ({ kind, prefix, items, pubUse: vis !== "private", vis, cfg, line });
  const item = (segs, name, rename, glob) => ({ segs, name, rename, glob });
  const decl = (name, declKind, vis, line) => ({ name, declKind, vis, line });
  const mod = (name, vis, cfg, pathAttr, docHidden, line, inline) => ({ name, vis, cfg, pathAttr, docHidden, line, inline: !!inline });

  // [source, mods, uses, pathRefs, restrictedDecls]
  const fixtures = [
    ["cfg-disjoint double mod (lib.rs shape)", L(
      "#[cfg(all(feature = \"alloc-core\", feature = \"internals\"))]",
      "#[doc(hidden)]",
      "pub mod alloc_core;",
      "#[cfg(all(feature = \"alloc-core\", not(feature = \"internals\")))]",
      "pub(crate) mod alloc_core;",
    ), [
      mod("alloc_core", "pub", "all(feature = \"alloc-core\", feature = \"internals\")", null, true, 3),
      mod("alloc_core", "pub(crate)", "all(feature = \"alloc-core\", not(feature = \"internals\"))", null, false, 5),
    ], [], [], [decl("alloc_core", "mod", "pub(crate)", 5)]],
    ["#[path] mod, above other attrs", L(
      "#[cfg(feature = \"a\")]", "#[path = \"p/q.rs\"]", "pub mod moved;",
    ), [mod("moved", "pub", "feature = \"a\"", "p/q.rs", false, 3)], [], [], []],
    ["multi-line cfg_attr is not cfg", L(
      "#[cfg_attr(", "    not(any(feature = \"alloc-stats\", feature = \"internals\")),", "    allow(unused_imports)", ")]", "mod m5;",
    ), [mod("m5", "private", null, null, false, 5)], [], [], []],
    ["multi-line cfg before use", L(
      "#[cfg(all(", "  feature = \"alloc-global\",", "  feature = \"alloc-xthread\"", "))]",
      "pub(crate) use alloc_core::{A, B};",
    ), [], [use("uniform", ["alloc_core"], [item(["A"], "A", null, false), item(["B"], "B", null, false)], "pub(crate)", "all( feature = \"alloc-global\", feature = \"alloc-xthread\" )", 5)], [],
      [decl("A", "use", "pub(crate)", 5), decl("B", "use", "pub(crate)", 5)]],
    ["two cfg attributes &&-join", L("#[cfg(feature = \"a\")]", "#[cfg(feature = \"b\")]", "mod dual;"),
      [mod("dual", "private", "feature = \"a\" && feature = \"b\"", null, false, 3)], [], [], []],
    ["grouped nested use with rename and glob",
      "use crate::a::b::{c::{d, e as f}, g, *};\n", [],
      [use("crate", ["crate", "a", "b"], [
        item(["c", "d"], "d", null, false), item(["c", "e"], "e", "f", false), item(["g"], "g", null, false), item([], null, null, true),
      ], "private", null, 1)], [], []],
    ["pub(in ...) fn and restricted field", L(
      "pub(in crate::x) fn opaque() {}",
      "pub(in super::super) fn remove(&self) -> bool {", "  false", "}",
      "struct S {", "  pub(in crate::x) name: u32,", "  other: u8,", "}",
    ), [], [], [], [
      // pub(in ...) vis paths are blanked before path-ref scanning (not refs)
      decl("opaque", "fn", "pub(in crate::x)", 1), decl("remove", "fn", "pub(in super::super)", 2),
      decl("name", "field", "pub(in crate::x)", 6),
    ]],
    ["pub(in crate::x) fn f(); yields no pathRef", L(
      "pub(in crate::x) fn f();",
      "pub(in crate::alloc_core::segment) struct S;",
    ), [], [], [], [
      decl("f", "fn", "pub(in crate::x)", 1), decl("S", "struct", "pub(in crate::alloc_core::segment)", 2),
    ]],
    ["pub(crate) use a::{B, C as D};", "pub(crate) use a::{B, C as D};\n", [],
      [use("uniform", ["a"], [item(["B"], "B", null, false), item(["C"], "C", "D", false)], "pub(crate)", null, 1)], [],
      [decl("B", "use", "pub(crate)", 1), decl("D", "use", "pub(crate)", 1)]],
    ["use super::super::x;", "use super::super::x;\n", [],
      [use("super", ["super", "super", "x"], [item([], "x", null, false)], "private", null, 1)], [], []],
    ["use self::state::depot::Depot;", "use self::state::depot::Depot;\n", [],
      [use("self", ["self", "state"], [item(["depot", "Depot"], "Depot", null, false)], "private", null, 1)], [], []],
    ["path refs stop correctly; crate() is not a path", L(
      "fn go() {",
      "  crate::a::b::C::method();",
      "  let r: crate::registry::segment_route::RouteScan<'_> = x;",
      "  crate::ghost::ghost_assert!(true);",
      "  crate::alloc_core::B4_DIRECTORY_LOOKUPS.fetch_add(1, y);",
      "  let u = crate();",
      "  super::super::deep();",
      "}",
    ), [], [], [
      { kind: "crate", segs: ["a", "b", "C", "method"], line: 2 },
      { kind: "crate", segs: ["registry", "segment_route", "RouteScan"], line: 3 },
      { kind: "crate", segs: ["ghost", "ghost_assert"], line: 4 },
      { kind: "crate", segs: ["alloc_core", "B4_DIRECTORY_LOOKUPS"], line: 5 },
      { kind: "super", segs: ["deep"], line: 7 },
    ], []],
    ["use inside a macro invocation body", L(
      "medium_promotion_reachable! {",
      "  #[cfg(feature = \"bench-internals\")]",
      "  pub(crate) use alloc_core::{X, Y};",
      "}",
    ), [], [
      use("uniform", ["alloc_core"], [item(["X"], "X", null, false), item(["Y"], "Y", null, false)], "pub(crate)", "feature = \"bench-internals\"", 3),
    ], [], [decl("X", "use", "pub(crate)", 3), decl("Y", "use", "pub(crate)", 3)]],
    ["inline mod flagged; use path not double-counted", L(
      "mod inline {", "  use crate::deep::Thing;", "  fn f() -> crate::deep::Thing {", "    Thing", "  }", "}",
    ), [mod("inline", "private", null, null, false, 1, true)], [
      use("crate", ["crate", "deep"], [item(["Thing"], "Thing", null, false)], "private", null, 2),
    ], [{ kind: "crate", segs: ["deep", "Thing"], line: 3 }], []],
  ];
  for (const [name, src, modsE, usesE, refsE, declsE] of fixtures) {
    const p = parse(src);
    eq(p.mods, modsE, `${name}: mods`);
    eq(p.uses, usesE, `${name}: uses`);
    eq(p.pathRefs, refsE, `${name}: pathRefs`);
    eq(p.restrictedDecls, declsE, `${name}: restrictedDecls`);
  }

  // doc comment containing crate::x::Y — present in comments, NOT in pathRefs
  const fixDoc = "/// See [`crate::x::Y`] for details.\nmod documented;\n";
  const pDoc = parse(fixDoc);
  eq(pDoc.pathRefs, [], "doc comment paths not in pathRefs");
  eq(pDoc.mods, [mod("documented", "private", null, null, false, 2)], "mod after doc comment");
  const lxDoc = lexRust(fixDoc);
  ok(lxDoc.comments.length === 1 && lxDoc.comments[0].kind === "doc", "doc comment captured");
  ok(lxDoc.comments[0].text.includes("crate::x::Y"), "doc comment text keeps the path");

  console.log(`parse selftest: ok (${checks} checks)`);
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
