// Minimal TOML-subset parser for the sefer-alloc root manifest.
//
// Purpose: enough TOML to read `[package]` (autotests) and `[[test]]`,
// `[[bench]]`, `[[example]]` tables (keys name/path/harness/test/
// required-features) from Cargo.toml WITHOUT invoking cargo. It is a generic
// parser (not a Cargo.toml mock): synthetic manifests in test temp dirs parse
// too.
//
// Supported subset:
//   - `[a.b.c]` and `[[a.b]]` table headers; dotted parts may be quoted
//     ('...' or "..."), e.g. [target.'cfg(target_os = "windows")'.dev-dependencies]
//   - key = value where value is a basic string ("..."), literal string
//     ('...'), boolean, integer/float, or array of those
//   - arrays may span multiple lines; `#` comments (full-line and trailing,
//     outside strings) are ignored
//
// Usage:
//   import { parseToml } from './classify-tests/toml.mjs';
//   const doc = parseToml(fs.readFileSync('Cargo.toml', 'utf8'));
//   doc.sections // array of { kind: 'table'|'array', path: [...], values: {...} }
//
// Derives: nothing beyond the parse itself.
// Asserts: throws on unbalanced brackets/quotes so a bad manifest fails
// loudly instead of producing a silently truncated document.

/** Split a header's dotted path, honouring quotes. Throws on malformed input. */
function splitHeaderPath(header) {
  const parts = [];
  let cur = '';
  let quote = null;
  for (let i = 0; i < header.length; i++) {
    const ch = header[i];
    if (quote) {
      if (ch === quote) quote = null; else cur += ch;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
    } else if (ch === '.') {
      if (!cur) throw new Error(`malformed TOML header: [${header}]`);
      parts.push(cur); cur = '';
    } else {
      cur += ch;
    }
  }
  if (quote) throw new Error(`unterminated quote in TOML header: [${header}]`);
  if (!cur) throw new Error(`malformed TOML header: [${header}]`);
  parts.push(cur);
  return parts;
}

/** Strip a `#` comment outside of quotes. Returns the comment-free text. */
function stripComment(line) {
  let quote = null;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (quote) {
      if (ch === '\\') i++; // skip escaped char inside basic strings
      else if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") {
      quote = ch;
    } else if (ch === '#') {
      return line.slice(0, i);
    }
  }
  return line;
}

/** Bracket/brace depth of `text`, counting only outside quotes. */
function bracketDepth(text) {
  let depth = 0;
  let quote = null;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i];
    if (quote) {
      if (ch === '\\') i++;
      else if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") quote = ch;
    else if (ch === '[' || ch === '{') depth++;
    else if (ch === ']' || ch === '}') depth--;
  }
  return depth;
}

/** Split an array body on top-level commas (outside quotes and brackets). */
function splitArrayItems(body) {
  const items = [];
  let cur = '';
  let depth = 0;
  let quote = null;
  for (let i = 0; i < body.length; i++) {
    const ch = body[i];
    if (quote) {
      cur += ch;
      if (ch === '\\') { cur += body[i + 1] ?? ''; i++; }
      else if (ch === quote) quote = null;
    } else if (ch === '"' || ch === "'") { quote = ch; cur += ch; }
    else if (ch === '[' || ch === '{') { depth++; cur += ch; }
    else if (ch === ']' || ch === '}') { depth--; cur += ch; }
    else if (ch === ',' && depth === 0) { items.push(cur); cur = ''; }
    else cur += ch;
  }
  if (cur.trim() !== '') items.push(cur);
  return items;
}

/** Parse a single scalar (string / bool / number) or nested array value. */
function parseValue(text) {
  const t = text.trim();
  if (t === '') throw new Error('empty TOML value');
  if (t.startsWith('[')) {
    if (bracketDepth(t) !== 0) throw new Error(`unbalanced array value: ${t}`);
    return splitArrayItems(t.slice(1, -1)).map(parseValue);
  }
  if ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'"))) {
    const inner = t.slice(1, -1);
    return t.startsWith('"') ? unescapeBasic(inner) : inner;
  }
  if (t === 'true') return true;
  if (t === 'false') return false;
  const num = Number(t);
  if (Number.isFinite(num) && /^[-+]?[0-9_]+(\.[0-9]+)?([eE][-+]?[0-9]+)?$/.test(t.replaceAll('_', ''))) {
    return num;
  }
  // Dates and other exotic values are not needed here; keep them verbatim.
  return t;
}

function unescapeBasic(inner) {
  return inner.replaceAll('\\"', '"').replaceAll('\\\\', '\\').replaceAll('\\n', '\n').replaceAll('\\t', '\t');
}

/** Parse a document into a flat list of sections. */
export function parseToml(text) {
  const lines = text.split(/\r?\n/);
  const sections = [];
  let current = null; // { kind, path, values }
  let pending = null; // { key, parts } while a multi-line array is open

  const finishPending = () => {
    if (!pending) return;
    const full = pending.parts.join(' ');
    current.values[pending.key] = parseValue(full);
    pending = null;
  };

  for (const raw of lines) {
    if (pending) {
      pending.parts.push(stripComment(raw));
      if (bracketDepth(pending.parts.join(' ')) === 0) finishPending();
      continue;
    }
    const line = stripComment(raw).trim();
    if (line === '') continue;
    if (line.startsWith('[[') && line.endsWith(']]')) {
      finishPending();
      current = { kind: 'array', path: splitHeaderPath(line.slice(2, -2)), values: {} };
      sections.push(current);
      continue;
    }
    if (line.startsWith('[') && line.endsWith(']')) {
      finishPending();
      current = { kind: 'table', path: splitHeaderPath(line.slice(1, -1)), values: {} };
      sections.push(current);
      continue;
    }
    const eq = line.indexOf('=');
    if (eq === -1) throw new Error(`unparsable TOML line: ${raw}`);
    const key = line.slice(0, eq).trim().replaceAll(/^"|"$/g, '');
    const valueText = line.slice(eq + 1);
    if (!current) { current = { kind: 'table', path: [], values: {} }; sections.push(current); }
    if (bracketDepth(valueText) === 0) {
      current.values[key] = parseValue(valueText);
    } else {
      pending = { key, parts: [valueText] };
    }
  }
  finishPending();
  return { sections };
}

/** Convenience: value of the first `[package]` table key. */
export function packageValue(doc, key, fallback) {
  const pkg = doc.sections.find((s) => s.kind === 'table' && s.path.length === 1 && s.path[0] === 'package');
  if (!pkg) return fallback;
  const v = pkg.values[key];
  return v === undefined ? fallback : v;
}
