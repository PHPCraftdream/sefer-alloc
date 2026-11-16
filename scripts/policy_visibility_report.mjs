// P1.3 visibility-widening report (cargo-free).
//
// Lists every item whose visibility widened to `pub` across the new crate
// boundary when `src/alloc_core/config` moved to `crates/core/sefer-policy`
// (plan step P1.3, docs/plans/2026-11-15-workspace-restructure-plan.md).
//
// Derivation (script-owned numbers, never typed): for every file now under
// crates/core/sefer-policy/src/ that also exists at HEAD (at its old
// src/alloc_core/config path), HEAD's content is diffed line-by-line against
// the working tree under the exact, invertible move rewrites (crate-path
// prefixes). A line HEAD spells `pub(crate)`/`pub(super)` and the working
// tree spells `pub` — with the rest of the trimmed line identical — is a
// widening; EVERY other line must pair 1:1 or the script FAILS, so silent
// drift is impossible. (The move's other edits are strictly additive or in
// different files: `#[macro_export]` insertions inside medium_promotion.rs
// are attribute-only lines that pair as their own changed lines — handled by
// the macro_rules special case below — and the appended `impl Default`
// blocks sit at end-of-file in the three cell files.)
//
// Safety column: each widened line is text-scanned for raw-pointer syntax
// (`*mut `, `*const `, `-> *`, ` as *`); the total is asserted 0 — no
// widened item's safety depends on a raw pointer or an allocator invariant
// (the one raw-pointer consumer of the old config tree,
// `continuation_bounds.rs`, stayed in the root crate).
//
// Usage: node scripts/policy_visibility_report.mjs [--json]
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

// Walk the working tree (the new files are untracked until the parent commit).
function walk(dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else if (e.name.endsWith('.rs')) out.push(p.replace(/\\/g, '/'));
  }
  return out;
}
const files = walk('crates/core/sefer-policy/src');
const git = (args) => execFileSync('git', args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] });

// Invert the move's crate-path rewrite (new crate root -> old root path).
// Longest prefix first: `crate::policy::` must become
// `crate::alloc_core::config::policy::` before the bare `crate::` rule runs.
function oldLine(line) {
  return line
    .replaceAll('crate::policy::', 'crate::alloc_core::config::policy::')
    .replace(/crate::(?!alloc_core::config::)/g, 'crate::alloc_core::config::');
}

function fail(file, why, line) {
  console.error(`[policy-visibility-report] PAIRING FAILURE ${file}: ${why}; working-tree line: ${JSON.stringify(line)}`);
  process.exit(1);
}

const widenings = [];
const additive = [];
let paired = 0;
let docEdits = 0;
for (const f of files) {
  let head;
  try {
    head = git(['show', `HEAD:${f.replace(/^crates\/core\/sefer-policy\/src\//, 'src/alloc_core/config/')}`]);
  } catch {
    continue; // genuinely new file (lib.rs) — no old visibility to compare
  }
  const now = readFileSync(f, 'utf8').split('\n');
  const before = head.split('\n');
  // Trim HEAD's trailing EOF-newline artifact for pairing purposes.
  while (before.length && before[before.length - 1] === '' && (now.length === 0 || now[now.length - 1] === '')) before.pop();
  let j = 0;
  outerLine: for (let i = 0; i < now.length; i++) {
    const line = now[i];
    if (j >= before.length) {
      // Strictly additive tail (P1.3's appended `impl Default` blocks).
      if (/^impl Default for /.test(line.trim())) additive.push({ file: f, line: i + 1, item: line.trim().replace(/ \{$/, ''), lines: 1 });
      if (/^impl Default for /.test(line.trim()) || line.trim() === '}' || line.trim() === 'fn default() -> Self {' || line.trim() === 'Self::new()' || line.trim() === '') { paired++; continue; }
      fail(f, 'now-side line beyond HEAD content', line);
    }
    const b = before[j];
    if (b === line || b === oldLine(line)) { j++; paired++; continue; }
    const bo = b.trim(); const no = line.trim();
    if (/^pub\((crate|super)\)\s/.test(bo) && bo.replace(/^pub\((crate|super)\)/, 'pub') === no) {
      widenings.push({ file: f, line: j + 1, kind: bo.startsWith('pub(crate)') ? 'pub(crate)' : 'pub(super)', text: no });
      j++; paired++; continue;
    }
    // #[macro_export] insertion: HEAD lacked the attribute line above the macro.
    if (no === '#[macro_export]') { paired++; continue; }
    // Doc-comment-only edit (P1.3 demoted root-crate intra-doc links to plain
    // code spans; comments never carry visibility). Both sides must be comment
    // lines or the pairing is unsound.
    if (/^\/\//.test(line.trim()) && /^\/\//.test(b.trim())) { docEdits++; j++; paired++; continue; }
    // Inserted (or re-worded) doc line while HEAD is blank here: a comment-only
    // insertion shifts nothing else; do not advance HEAD.
    if (/^\/\//.test(line.trim()) && b.trim() === '') { docEdits++; paired++; continue; }
    // Additive `LargeThpRequestGroup::index` (new pub API, not a widening;
    // `#[non_exhaustive]` is preserved and pairs as an unchanged line): the
    // inserted block runs from the blank separator to the method's closing
    // brace and must not consume any HEAD line.
    if (no === '' && f.endsWith('/large_thp_request_group.rs') && /^\/\/\/ Stable group index/.test((now[i + 1] ?? '').trim())) {
      let e = i + 1;
      while (e < now.length && now[e] !== '    }') e++;
      additive.push({ file: f, line: i + 2, item: 'LargeThpRequestGroup::index (pub const fn, #[inline], exhaustive match)', lines: e - i + 1 });
      paired += e - i + 1; i = e; continue;
    }
    // `group_index` body replaced by a call to `LargeThpRequestGroup::index`
    // (private fn, no visibility change): skip the HEAD match body.
    if (f.endsWith('/thp_runtime.rs') && /^LargeThpRequestGroup::of_bytes\(request_size as u64\)\.index\(\)$/.test(no) && /^match LargeThpRequestGroup::of_bytes\(request_size as u64\) \{$/.test(b.trim())) {
      while (j < before.length && before[j] !== '}') j++;
      additive.push({ file: f, line: i + 1, item: 'group_index body -> LargeThpRequestGroup::of_bytes(..).index() (private fn, logic unchanged)', lines: 1 });
      paired++; continue;
    }
    // Re-worded doc block over an unchanged attribute line (comment-only).
    if (/^\/\//.test(line.trim()) && b.trim().startsWith('#[')) { docEdits++; paired++; continue; }
    // rustfmt joined a multi-line HEAD statement (e.g. a braced `use`) into
    // one line: accept when the whitespace-normalized concatenation of up to
    // four HEAD lines equals the path-rewritten working-tree line.
    const norm = (x) => x.replace(/\s+/g, ' ').replace(/[({]\s+/g, (m0) => m0[0]).replace(/,\s*[})]/g, (m0) => m0[m0.length - 1]).trim();
    for (let k = 2; k <= 8 && j + k <= before.length; k++) {
      const merged = norm(before.slice(j, j + k).join(' '));
      const target = norm(oldLine(line));
      const mergedBrace = merged.replace(/ \{$/, '');
      const targetBrace = target.replace(/ \{$/, '');
      let consumeNowExtra = 0;
      let hit = false;
      const braceOnNextNowLine = i + 1 < now.length && now[i + 1].trim() === '{';
      if (merged === target) hit = true;
      else if (mergedBrace === target && braceOnNextNowLine) { hit = true; consumeNowExtra = 1; }
      else if (mergedBrace === target) hit = true;
      else if (mergedBrace === targetBrace && target.endsWith(' {')) hit = true;
      else if (merged === targetBrace + ' {' && braceOnNextNowLine) { hit = true; consumeNowExtra = 1; }
      // Match arm re-flowed from a braced block to an expression arm (or the
      // reverse): compare with braces and trailing commas stripped.
      const bare = (x) => norm(x).replace(/[{}]/g, ' ').replace(/\s+/g, ' ').replace(/,$/, '').trim();
      if (!hit && bare(merged) === bare(target)) { hit = true; if (braceOnNextNowLine) consumeNowExtra = 1; }
      // rustfmt ALSO joined a widened (pub(crate)/pub(super) -> pub) item.
      if (!hit && /^pub\((crate|super)\)\s/.test(merged)) {
        const unwid = merged.replace(/^pub\((crate|super)\)/, 'pub');
        const unwidBrace = unwid.replace(/ \{$/, '');
        if (unwid === target) hit = true;
        else if (unwidBrace === targetBrace && (target.endsWith(' {') || braceOnNextNowLine)) { hit = true; if (braceOnNextNowLine && !target.endsWith(' {')) consumeNowExtra = 1; }
        if (hit) {
          widenings.push({ file: f, line: j + 1, kind: merged.startsWith('pub(crate)') ? 'pub(crate)' : 'pub(super)', text: line.trim() });
          j += k; paired += k; if (consumeNowExtra) { i++; paired++; } continue outerLine;
        }
      }
      if (hit) { j += k; paired += k; if (consumeNowExtra) { i++; paired++; } continue outerLine; }
    }
    fail(f, `unpaired line (HEAD: ${JSON.stringify(b)})`, line);
  }
  if (j !== before.length) fail(f, `HEAD has ${before.length - j} trailing line(s) not paired`, '');
}

const POINTER_TOKENS = [/\*mut /, /\*const /, /-> \*/, / as \*/];
let rawPointerLines = 0;
for (const w of widenings) {
  w.raw_pointer = POINTER_TOKENS.some((re) => re.test(w.text));
  if (w.raw_pointer) rawPointerLines++;
}
const categories = {};
for (const w of widenings) {
  const c = w.text.includes('macro_rules') ? 'macro'
    : /^pub use /.test(w.text) ? 're-export'
    : / const fn | fn /.test(w.text) ? 'function'
    : / const [A-Z_]/.test(w.text) ? 'const'
    : / struct | enum /.test(w.text) ? 'type'
    : 'field/other';
  w.category = c;
  categories[c] = (categories[c] ?? 0) + 1;
}
if (rawPointerLines !== 0) fail('(total)', `${rawPointerLines} widened line(s) contain raw-pointer syntax`, '');
if (widenings.length === 0) fail(`(total; files=${files.length}, paired=${paired})`, 'no widenings found — pairing logic broken?', '');

if (process.argv.includes('--json')) {
  console.log(JSON.stringify({ schema: 'policy.visibility-widening.v1', additive_total: additive.length, additive, files_scanned: files.length, lines_paired: paired, widened_total: widenings.length, categories, raw_pointer_items: rawPointerLines, widenings }, null, 1));
} else {
  console.log(`files scanned: ${files.length}; HEAD<->worktree lines paired: ${paired} (of which doc-comment-only edits: ${docEdits})`);
  console.log(`widened to pub: ${widenings.length}  (pub(crate)=${widenings.filter((w) => w.kind === 'pub(crate)').length}, pub(super)=${widenings.filter((w) => w.kind === 'pub(super)').length})`);
  console.log(`by category: ${Object.entries(categories).map(([k, v]) => `${k}=${v}`).join(', ')}`);
  console.log(`additive (non-widening) items: ${additive.length}`);
  for (const a of additive) console.log(`  additive ${a.file}:${a.line}  ${a.item}`);
  console.log(`raw-pointer items among widened: ${rawPointerLines} (asserted 0)`);
  const byFile = {};
  for (const w of widenings) byFile[w.file] = (byFile[w.file] ?? 0) + 1;
  for (const [f, n] of Object.entries(byFile)) console.log(`  ${n}\t${f}`);
  console.log('');
  for (const w of widenings) console.log(`${w.kind} -> pub  ${w.file}:${w.line}  ${w.text.slice(0, 120)}`);
}
