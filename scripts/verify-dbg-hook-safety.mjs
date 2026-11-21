// Inventory includes public const dbg functions, suffix test hooks, injections,
// and the explicitly named deliberate root forwarders below.
// Executable security gate migrated from the R30-2 reviewed hook tripwire.
// Every crate-public dbg_* hook, regardless of signature shape, must be a
// genuinely bench-internals-gated safe hook, a reviewed pure observer, an
// individually justified safe mutator, or an exhaustively reviewed unsafe hook.
// These are the ONE authoritative reviewed tables. Surviving invariant reasons
// are preserved verbatim; only explicitly retired definitions were pruned.
// Unsafe measurement hooks require their real gate and # Safety contract.
// R6-T38: crate-public safe fns with raw-pointer parameters are inventoried
// regardless of name.
// Internals/bench-internals-gated unsafe raw-pointer fns are inventoried too and must carry # Safety.
// No source-copy test, auto-pruning, wildcard allowlist or cfg_attr exemption.
// Run: node scripts/verify-dbg-hook-safety.mjs

import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { basename, dirname, join, relative } from 'node:path';
import { pathToFileURL } from 'node:url';
import { REPO_ROOT } from './lib.mjs';
import { PURE_OBSERVERS, SAFE_MUTATORS, REVIEWED_SURFACE } from './dbg-hook-reviewed-surface.mjs';
// Reviewed-surface tables live in ./dbg-hook-reviewed-surface.mjs (split for
// structure limits); the contract exports below are unchanged.
export { REVIEWED_SURFACE };


// Caller-owned unsafe boundaries, exhaustively accounted for.
const CURRENT_RESERVATION_HOOK = "src/global/sefer_alloc/diag.rs::dbg_current_reservation_for_test";
const UNSAFE_HOOKS = [
  "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_stamp_segment_id",
  "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/alloc_core_core_diag/header_diag.rs::dbg_stamp_kind_byte",
  "crates/core/sefer-registry/src/registry/heap_core/diag/queries.rs::dbg_stamp_kind_byte",
  "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_unregister",
  "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/alloc_core_core_diag/table_diag.rs::dbg_recycle",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_diag.rs::dbg_drain_freelist_batch",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_diag.rs::dbg_state_bytes_for",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_diag.rs::dbg_corrupt_freelist_head_next",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_diag.rs::dbg_payload_start_for",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_decommit_payload",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_recommit_payload",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_release",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_win_commit_only",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_pool/decomp_hooks.rs::dbg_decomp_win_release_only",
  "crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_pool/decommit.rs::dbg_force_decommit_retain_for",
  "src/global/sefer_alloc/global_alloc.rs::dbg_dealloc_while_fallback_lock_held",
  "src/global/tls_heap.rs::dbg_restore_local_for_test",
  "crates/core/sefer-registry/src/registry/bootstrap/registry.rs::dbg_slot_preset_generation",
  "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs::dbg_dealloc_own_thread_with_base",
  "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs::dbg_flush_class_only",
  "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs::dbg_clear_magazine_on_hit",
  "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_decommit_payload",
  "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_recommit_payload",
  "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_release",
  "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_win_commit_only",
  "crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs::dbg_decomp_win_release_only",
  "crates/core/sefer-alloc-core/src/alloc_core/alloc_core/lifecycle_parts/sidecar_test_hooks.rs::dbg_publish_small_sidecar_free",
  "crates/core/sefer-registry/src/registry/heap_core_xthread/sidecar_drain.rs::dbg_publish_small_sidecar_free",
  "crates/core/sefer-registry/src/registry/heap_core_xthread/sidecar_drain.rs::dbg_reclaim_sidecar_record_for_test",
  CURRENT_RESERVATION_HOOK
];
const SIDECAR_UNSAFE_HOOKS = new Set(UNSAFE_HOOKS.filter(id => /::dbg_(?:publish_small_sidecar_free|reclaim_sidecar_record_for_test)$/.test(id)));
const BENCH_UNSAFE_HOOKS = new Set(UNSAFE_HOOKS.filter(id => /::dbg_(?:decomp_|dealloc_own_thread_with_base|flush_class_only|clear_magazine_on_hit|force_decommit_retain_for|restore_local_for_test|dealloc_while_fallback_lock_held|publish_small_sidecar_free|reclaim_sidecar_record_for_test)/.test(id)));
BENCH_UNSAFE_HOOKS.add(CURRENT_RESERVATION_HOOK);
// Registered explicitly via its REVIEWED_SURFACE row (kind 'unsafe', gates
// internals + bench-internals); UNSAFE_HOOKS and REVIEWED_SURFACE are mutually
// exclusive (unique gate), so the row carries the registration and the add
// below carries the bench-gate requirement.
BENCH_UNSAFE_HOOKS.add("crates/core/sefer-segment/src/segment/segment_route/route_state/pin.rs::dbg_publish_cached_for_test");
const ROOT_FORWARDERS = [
  'crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_magazine.rs::refill_class',
  'crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_magazine.rs::refill_class_bump',
  'crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_magazine.rs::refill_class_bump_virgin',
  'crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_magazine.rs::flush_class',
  'crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs::small_decommit_start',
  'crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs::primordial_decommit_start',
  'crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs::small_lazy_initial_commit',
  'crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs::primordial_lazy_initial_commit',
  // Pass-9d selfCheck: REVIEWED_SURFACE rows whose fn names carry no
  // dbg_/_for_test/inject marker. Without this registration their synthesized
  // selfCheck fixtures hit the production-unsafe-API skip (ungated unsafe
  // raw-pointer fn), so the required-gate mutant never fired. Registry rows
  // verified against the real definitions (internals cfg / test-only harness).
  'crates/core/sefer-segment/src/segment/segment_header/segment_header_impl/segment_header_views.rs::set_segment_id_at',
  'crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs::register',
  'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::insert',
  'crates/core/sefer-segment/src/segment/segment_table/impl_group/harness.rs::insert_root_for_key',
  // R25-1 pass 10: doc-hidden pub unsafe constructor converted from a safe
  // observer row; ungated (always compiled), so it needs this named
  // registration to stay inside the inventory instead of the
  // production-unsafe-API skip.
  'crates/core/sefer-segment/src/segment/bitmap/state_byte.rs::new',
  // Keep the production pool-link writers and their safety contracts inventoried.
  'crates/core/sefer-segment/src/segment/segment_header/segment_header_impl/segment_header_meta_fields.rs::set_pool_prev',
  'crates/core/sefer-segment/src/segment/segment_header/segment_header_impl/segment_header_meta_fields.rs::set_pool_next',
];

export function rustMask(text) {
  const out = text.split('');
  const blank = (a, b) => { for (let j = a; j < b; j++) if (out[j] !== '\n' && out[j] !== '\r') out[j] = ' '; };
  for (let i = 0; i < text.length;) {
    const start = i;
    if (text.startsWith('//', i)) { const end = text.indexOf('\n', i); i = end < 0 ? text.length : end; blank(start, i); }
    else if (text.startsWith('/*', i)) {
      let depth = 1; i += 2;
      while (i < text.length && depth) { if (text.startsWith('/*', i)) { depth++; i += 2; } else if (text.startsWith('*/', i)) { depth--; i += 2; } else i++; }
      if (depth) throw new Error('unterminated Rust block comment');
      blank(start, i);
    } else {
      const raw = /^(?:b)?r(#{0,255})"/.exec(text.slice(i));
      if (raw) { const end = text.indexOf(`"${raw[1]}`, i + raw[0].length); if (end < 0) throw new Error('unterminated Rust raw string'); i = end + 1 + raw[1].length; blank(start, i); }
      else if (text[i] === '"') { i++; while (i < text.length) { if (text[i] === '\\') i += 2; else if (text[i++] === '"') break; } blank(start, i); }
      else { const char = /^'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|[^\r\n])|[^'\\\r\n])'/.exec(text.slice(i)); if (char) { i += char[0].length; blank(start, i); } else i++; }
    }
  }
  return out.join('');
}

// The policy deliberately accepts bare bench-internals and all(...), never
// not(...), any(...) or cfg_attr. Unknown/malformed predicates fail closed.
function parseCfg(inner) {
  const tokens = inner.match(/"(?:\\.|[^"\\])*"|[A-Za-z_][\w-]*|[(),=]|\S/g) ?? [];
  let pos = 0;
  function term() {
    const name = tokens[pos++];
    if (!/^[A-Za-z_][\w-]*$/.test(name ?? '')) throw new Error('invalid cfg identifier');
    if (tokens[pos] === '=') { pos++; const value = tokens[pos++]; if (!value?.startsWith('"')) throw new Error('invalid cfg value'); return { name, value: JSON.parse(value) }; }
    if (tokens[pos] !== '(') return { name };
    pos++; const children = [];
    while (tokens[pos] !== ')') { children.push(term()); if (tokens[pos] !== ',') break; pos++; }
    if (tokens[pos++] !== ')') throw new Error('unterminated cfg');
    if (name === 'not' && children.length !== 1) throw new Error('invalid not arity');
    return { name, children };
  }
  try { const result = term(); if (pos !== tokens.length) return null; return result; } catch { return null; }
}
// Resolve root Cargo implications rather than hardcoding a second feature
// convention: alloc-global already requires alloc-xthread in the manifest.
const featureSection = /^\[features\]\s*\n([\s\S]*?)(?=^\[|(?![\s\S]))/m.exec(readFileSync(join(REPO_ROOT, 'Cargo.toml'), 'utf8'))?.[1];
if (featureSection === undefined) throw new Error('missing root Cargo feature definitions');
const featureGraph = new Map([...featureSection.matchAll(/^\s*([\w-]+)\s*=\s*\[([\s\S]*?)\]/gm)].map(match => [match[1], [...match[2].matchAll(/"([^"]+)"/g)].map(value => value[1])]));
function featureImplies(from, required, seen = new Set()) {
  if (from === required) return true;
  if (seen.has(from)) return false;
  return (featureGraph.get(from) ?? []).some(child => featureGraph.has(child) && featureImplies(child, required, new Set([...seen, from])));
}
function expressionRequires(expr, feature) {
  if (feature === 'loom' || feature === 'r18_sidecar_scan_bench') return !!expr && ((expr.name === feature && !expr.children) || (expr.name === 'all' && expr.children.some(child => expressionRequires(child, feature))));
  if (feature === 'test-model') return !!expr && ((['loom', 'tagged_index_stack_test'].includes(expr.name) && !expr.children) || (expr.name === 'any' && expr.children.length > 0 && expr.children.every(child => expressionRequires(child, feature))) || (expr.name === 'all' && expr.children.some(child => expressionRequires(child, feature))));
  return !!expr && ((expr.name === 'feature' && featureImplies(expr.value, feature)) || (expr.name === 'all' && expr.children.some(child => expressionRequires(child, feature))));
}
function cfgInner(attr) { return /^#!?\[\s*cfg\s*\(([\s\S]*)\)\s*\]$/.exec(attr)?.[1]; }
export function attributesRequire(attrs, feature) { return attrs.some(attr => { const inner = cfgInner(attr); return inner !== undefined && expressionRequires(parseCfg(inner), feature); }); }
function invalidBenchAttribute(attr) {
  if (!/^#!?\[\s*cfg(?:_attr)?\s*\(/.test(attr) || !/feature\s*=\s*"bench-internals"/.test(attr)) return false;
  const inner = cfgInner(attr);
  return inner === undefined || !expressionRequires(parseCfg(inner), 'bench-internals');
}
function hasRawPointerParams(mask, from) {
  let angle = 0, start = -1, depth = 0;
  for (let i = from; i < mask.length; i++) {
    const ch = mask[i];
    if (start < 0) {
      if (ch === '<') angle++;
      else if (ch === '>' && mask[i - 1] !== '-') angle = Math.max(0, angle - 1);
      else if (ch === '(' && angle === 0) start = i;
      else if (ch === '{' || ch === ';') return false;
    } else {
      if (ch === '(') depth++;
      else if (ch === ')') { if (depth === 0) return /\*\s*(?:mut|const)\b/.test(mask.slice(start, i + 1)); depth--; }
    }
  }
  return false;
}
function safetyDocs(text, offset) {
  const lines = text.slice(0, offset).split(/\r?\n/);
  let first = lines.length - 1;
  // Walk back over blank/comment lines and attributes so docs placed before
  // an attribute are attributed. Stop at block openers: docs above an
  // `impl X {` line belong to the impl, not to a fn inside it.
  while (first >= 0 && (!lines[first].trim() || /^\s*\/\//.test(lines[first]) || /^\s*#!?\[/.test(lines[first]))) first--;
  return lines.slice(first + 1).join('\n');
}
export function scanFile(id, text) {
  const mask = rustMask(text);
  // Rust's canonical order is const unsafe; also inventory unsafe const
  // defensively so a malformed declaration cannot silently hide a hook.
  const token = /#(!?)\s*\[|\bpub\s+(?:(const)\s+)?(unsafe\s+)?(?:const\s+)?fn\s+(\w+)\b|\b(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*([;{])|[{};]/g;
  const globals = [], scopes = [], hooks = [], modules = [], moduleScopes = [];
  let pending = [], pendingStart = null;
  for (let match; (match = token.exec(mask));) {
    const inherited = () => [...globals, ...scopes.flat(), ...pending];
    if (match[0].startsWith('#')) {
      let end = token.lastIndex, depth = 1;
      while (end < mask.length && depth) { if (mask[end] === '[') depth++; else if (mask[end] === ']') depth--; end++; }
      if (depth) throw new Error(`${id}: unterminated attribute`);
      const attr = text.slice(match.index, end);
      if (match[1] === '!' && scopes.length === 0) globals.push(attr); else { pending.push(attr); pendingStart ??= match.index; }
      token.lastIndex = end;
    } else if (match[4]) {
      const name = match[4];
      const impls = [...mask.slice(0, match.index).matchAll(/\bimpl(?:\s*<[^{}]*?>)?\s+(\w+)[^{]*\{/g)];
      const lastImpl = impls.at(-1);
      const implTail = lastImpl ? mask.slice(lastImpl.index, match.index) : '';
      const owner = lastImpl && [...implTail].reduce((n, ch) => n + (ch === '{' ? 1 : ch === '}' ? -1 : 0), 0) > 0 ? lastImpl[1] : null;
      const hookId = `${id}::${id === 'crates/primitives/tagged-index-stack/src/imp.rs' && owner ? `${owner}::` : ''}${name}`;
      const named = /^dbg_|_for_tests?$|inject/.test(name) || ROOT_FORWARDERS.includes(hookId);
      // R6-T38 signature rule: safe crate-public fns with raw-pointer
      // parameters are inventoried regardless of name; gated unsafe fns with
      // pointer params are inventoried too (scoped in verifyHooks).
      // Safe AND unsafe fns are collected; verifyHooks scopes gated unsafe ones.
      const pointerParams = hasRawPointerParams(mask, token.lastIndex);
      if (named || pointerParams)
        hooks.push({ id: hookId, name, owner, file: id, unsafe: !!match[3], named, pointerParams, attrs: inherited(), docs: safetyDocs(text, pendingStart ?? match.index) });
    } else if (match[5]) {
      if (match[6] === '{') { scopes.push(pending); moduleScopes.push(match[5]); }
      else modules.push({ name: match[5], attrs: inherited(), inline: moduleScopes.filter(Boolean) });
      pending = []; pendingStart = null;
    } else if (match[0] === '{') { scopes.push(pending); moduleScopes.push(null); pending = []; pendingStart = null; }
    else if (match[0] === '}') { scopes.pop(); moduleScopes.pop(); pending = []; pendingStart = null; }
    else { pending = []; pendingStart = null; }
  }
  return { globals, hooks, modules };
}
export function listRust(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry => { const path = join(dir, entry.name); return entry.isDirectory() ? listRust(path) : entry.isFile() && entry.name.endsWith('.rs') ? [path] : []; });
}
export function idFor(path) { return relative(REPO_ROOT, path).split('\\').join('/'); }
export function moduleTarget(parent, module, files, roots = new Set()) {
  const explicit = module.attrs.map(attr => /^#\[\s*path\s*=\s*"([^"\n]+)"\s*\]$/.exec(attr)?.[1]).find(Boolean);
  const dir = dirname(parent);
  const stem = basename(parent, '.rs');
  const base = roots.has(parent) || ['mod', 'lib', 'main'].includes(stem) ? dir : join(dir, stem);
  const inline = module.inline ?? [];
  if (explicit) { const id = join(inline.length ? join(base, ...inline) : dir, explicit).split('\\').join('/'); return files.has(id) ? id : null; }
  const moduleBase = join(base, ...inline);
  return [join(moduleBase, `${module.name}.rs`), join(moduleBase, module.name, 'mod.rs')].map(path => path.split('\\').join('/')).find(path => files.has(path)) ?? null;
}
function unique(ids, label, errors) {
  const result = new Set();
  for (const id of ids) { if (result.has(id)) errors.push(`duplicate ${label}: ${id}`); result.add(id); }
  return result;
}
export function effectiveFileGates(files, roots = new Set()) {
  const incoming = new Map();
  for (const [parent, file] of files) for (const module of file.modules) {
    const target = moduleTarget(parent, module, files, roots);
    if (target) { if (!incoming.has(target)) incoming.set(target, []); incoming.get(target).push({ parent, attrs: module.attrs }); }
  }
  function requires(file, feature, seen = new Set()) {
    if (seen.has(file)) return false;
    if (attributesRequire(files.get(file).globals, feature)) return true;
    // A crate root is also compiled independently of any module declaration.
    if (roots.has(file)) return false;
    const edges = incoming.get(file);
    return !!edges?.length && edges.every(edge => attributesRequire(edge.attrs, feature) || requires(edge.parent, feature, new Set([...seen, file])));
  }
  return requires;
}
export function verifyHooks(files, policy = {}) {
  const errors = [];
  const expectedSafe = unique(policy.safe ?? [...PURE_OBSERVERS, ...SAFE_MUTATORS.map(([id]) => id), ...REVIEWED_SURFACE.filter(row => row.kind !== 'unsafe').map(row => row.id)], 'reviewed safe hook', errors);
  for (const [id, reason] of SAFE_MUTATORS) if (!reason.trim()) errors.push(`missing invariant justification: ${id}`);
  const expectedUnsafe = unique(policy.unsafe ?? [...UNSAFE_HOOKS, ...REVIEWED_SURFACE.filter(row => row.kind === 'unsafe').map(row => row.id)], 'reviewed unsafe hook', errors);
  for (const id of expectedUnsafe) if (expectedSafe.has(id)) errors.push(`safe/unsafe classification overlaps: ${id}`);
  unique((policy.surface ?? REVIEWED_SURFACE).map(row => row.id), 'reviewed surface row', errors);
  for (const row of policy.surface ?? REVIEWED_SURFACE) if (!row.reason?.trim()) errors.push(`missing invariant justification: ${row.id}`);
  const requiresFile = effectiveFileGates(files);
  const foundSafe = new Set(), foundUnsafe = new Set();
  let gated = 0;
  for (const file of files.values()) for (const hook of file.hooks) {
    const requires = feature => attributesRequire(hook.attrs, feature) || requiresFile(hook.file, feature);
    if (!hook.named && !hook.pointerParams) continue;
    // Outside internals/bench-internals a raw-pointer unsafe fn is the
    // documented production unsafe API (tier policy), not this inventory.
    if (!hook.named && hook.unsafe && !requires('internals') && !requires('bench-internals')) continue;
    const row = (policy.surface ?? REVIEWED_SURFACE).find(row => row.id === hook.id);
    if (row) for (const feature of row.gates) if (!requires(feature)) errors.push(`reviewed hook missing ${feature} gate: ${hook.id}`);
    for (const attr of hook.attrs) if (invalidBenchAttribute(attr)) errors.push(`non-gating bench cfg/cfg_attr: ${hook.id}: ${attr}`);
    if (hook.id === 'crates/core/sefer-registry/src/fallback.rs::dbg_fallback_lock_spin_transition') for (const feature of ['internals', 'bench-internals']) if (!requires(feature)) errors.push(`pure transition missing ${feature} gate: ${hook.id}`);
    if (hook.unsafe) {
      foundUnsafe.add(hook.id);
      if (!/^\s*\/\/\/\s*# Safety\s*$/m.test(hook.docs)) errors.push(`unsafe hook lacks its # Safety contract: ${hook.id}`);
      if (BENCH_UNSAFE_HOOKS.has(hook.id) && !requires('bench-internals')) errors.push(`measurement-only unsafe hook is not bench-internals-gated: ${hook.id}`);
      if (hook.id === CURRENT_RESERVATION_HOOK && !requires('internals')) errors.push(`reservation observer is not internals-gated: ${hook.id}`);
      if (SIDECAR_UNSAFE_HOOKS.has(hook.id)) for (const feature of ['alloc-global', 'alloc-xthread', 'internals', 'bench-internals']) if (!requires(feature)) errors.push(`terminal producer hook missing ${feature} gate: ${hook.id}`);
    } else if (requires('bench-internals')) {
      gated++;
      if (expectedSafe.has(hook.id)) foundSafe.add(hook.id);
      else if (row || !/^dbg_/.test(hook.name)) foundSafe.add(hook.id);
    } else foundSafe.add(hook.id);
  }
  for (const [found, expected, label] of [[foundSafe, expectedSafe, 'safe ungated'], [foundUnsafe, expectedUnsafe, 'unsafe']]) {
    for (const id of found) if (!expected.has(id)) errors.push(`unreviewed ${label} hook: ${id}`);
    for (const id of expected) if (!found.has(id)) errors.push(`stale ${label} allowlist entry (review required, never auto-pruned): ${id}`);
  }
  return { errors, safe: foundSafe.size, unsafe: foundUnsafe.size, gated };
}
// Hand-authored oracle: declarations, owners, classifications and required gates
// below are literal expectations, not derived from REVIEWED_SURFACE rows.
function handAuthoredCheck() {
  const check = (file, source, expected) => {
    const parsed = scanFile(file, source);
    assert.deepEqual(parsed.hooks.map(h => [h.name, h.owner, h.unsafe]), expected);
    // A partial fixture necessarily leaves unrelated allowlist entries stale.
    const errors = verifyHooks(new Map([[file, parsed]])).errors.filter(e => !e.startsWith('stale '));
    assert.deepEqual(errors, []);
    return parsed;
  };
  const sidecar = 'crates/core/sefer-segment/src/segment/segment_route/route_state/small_sidecar.rs';
  const ownerSource = `impl SmallSidecar {
    /// # Safety
    /// Caller exclusively owns the live mapped reservation.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub unsafe fn owner_state_for_test(address: usize, class: usize) -> Option<(usize, u32, u32)> { None }
  }`;
  check(sidecar, ownerSource, [['owner_state_for_test', 'SmallSidecar', true]]);
  const sidecarErrors = source => verifyHooks(new Map([[sidecar, scanFile(sidecar, source)]])).errors;
  assert(sidecarErrors(ownerSource.replace('pub unsafe fn', 'pub fn')).includes(`unreviewed safe ungated hook: ${sidecar}::owner_state_for_test`));
  assert(sidecarErrors(ownerSource.replace('/// # Safety', '/// Contract')).includes(`unsafe hook lacks its # Safety contract: ${sidecar}::owner_state_for_test`));
  assert(sidecarErrors(ownerSource.replace('all(feature = "internals", feature = "bench-internals")', 'feature = "internals"')).includes(`reviewed hook missing bench-internals gate: ${sidecar}::owner_state_for_test`));
  assert(sidecarErrors(ownerSource.replace('all(feature = "internals", feature = "bench-internals")', 'feature = "bench-internals"')).includes(`reviewed hook missing internals gate: ${sidecar}::owner_state_for_test`));

  const chunks = 'crates/core/sefer-registry/src/registry/bootstrap/ensure.rs';
  check(chunks, '#[cfg(feature = "internals")]\npub const fn dbg_num_chunks() -> usize { 4 }', [['dbg_num_chunks', null, false]]);
  assert(verifyHooks(new Map([[chunks, scanFile(chunks, 'pub const fn dbg_num_chunks() -> usize { 4 }')]])).errors.includes(`reviewed hook missing internals gate: ${chunks}::dbg_num_chunks`));
  const probes = 'crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs';
  check(probes, 'impl HeapCore { #[cfg(feature = "internals")] pub const fn dbg_promotion_compiled() -> bool { false } }', [['dbg_promotion_compiled', 'HeapCore', false]]);
  assert(verifyHooks(new Map([[probes, scanFile(probes, 'impl HeapCore { pub const fn dbg_promotion_compiled() -> bool { false } }')]])).errors.includes(`reviewed hook missing internals gate: ${probes}::dbg_promotion_compiled`));

  const magazine = 'crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_magazine.rs';
  const magazineSource = `#[cfg(feature = "internals")]
  impl AllocCore {
    pub fn refill_class(&mut self, class_idx: usize, want: usize, out: &mut [*mut u8]) -> usize { 0 }
    pub fn refill_class_bump(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize { 0 }
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin", feature = "virgin-zero-skip"))]
    pub fn refill_class_bump_virgin(&mut self, class_idx: usize, out: &mut [*mut u8], virgin_out: &mut u16) -> usize { 0 }
    /// # Safety
    /// Caller owns each live block exactly once.
    pub unsafe fn flush_class(&mut self, class_idx: usize, blocks: &[*mut u8]) {}
  }`;
  check(magazine, magazineSource, [['refill_class', 'AllocCore', false], ['refill_class_bump', 'AllocCore', false], ['refill_class_bump_virgin', 'AllocCore', false], ['flush_class', 'AllocCore', true]]);
  assert.deepEqual(verifyHooks(new Map([[magazine, scanFile(magazine, magazineSource.replace('#[cfg(feature = "internals")]', ''))]])).errors.filter(e => e.startsWith('reviewed hook missing ')), [
    `reviewed hook missing internals gate: ${magazine}::refill_class`,
    `reviewed hook missing internals gate: ${magazine}::refill_class_bump`,
    `reviewed hook missing internals gate: ${magazine}::refill_class_bump_virgin`,
    `reviewed hook missing internals gate: ${magazine}::flush_class`,
  ]);

  const layout = 'crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs';
  const layoutSource = `#[cfg(feature = "internals")]
  impl SegmentLayout {
    pub fn small_decommit_start() -> usize { 0 }
    pub fn primordial_decommit_start() -> usize { 0 }
    #[cfg(any(feature = "primordial-lazy-commit", feature = "small-segment-lazy-commit"))]
    pub fn small_lazy_initial_commit(page_size: usize) -> usize { page_size }
    #[cfg(any(feature = "primordial-lazy-commit", feature = "small-segment-lazy-commit"))]
    pub fn primordial_lazy_initial_commit(page_size: usize) -> usize { page_size }
  }`;
  check(layout, layoutSource, [['small_decommit_start', 'SegmentLayout', false], ['primordial_decommit_start', 'SegmentLayout', false], ['small_lazy_initial_commit', 'SegmentLayout', false], ['primordial_lazy_initial_commit', 'SegmentLayout', false]]);
  assert.deepEqual(verifyHooks(new Map([[layout, scanFile(layout, layoutSource.replace('#[cfg(feature = "internals")]', ''))]])).errors.filter(e => e.startsWith('reviewed hook missing ')), [
    `reviewed hook missing internals gate: ${layout}::small_decommit_start`,
    `reviewed hook missing internals gate: ${layout}::primordial_decommit_start`,
    `reviewed hook missing internals gate: ${layout}::small_lazy_initial_commit`,
    `reviewed hook missing internals gate: ${layout}::primordial_lazy_initial_commit`,
  ]);

  const epoch = 'crates/containers/sefer-concurrent/src/epoch/epoch_region.rs';
  check(epoch, `#[cfg(feature = "internals")] impl EpochRegion<T> {
    pub fn _set_slot_generation_for_tests(&mut self, index: u32, generation: u32) {}
    pub fn _remote_free_queue_buffer_identity_for_tests(&self) -> (usize, usize, usize) { (0, 0, 0) }
  }`, [['_set_slot_generation_for_tests', 'EpochRegion', false], ['_remote_free_queue_buffer_identity_for_tests', 'EpochRegion', false]]);
  assert.deepEqual(verifyHooks(new Map([[epoch, scanFile(epoch, `impl EpochRegion<T> {
    pub fn _set_slot_generation_for_tests(&mut self, index: u32, generation: u32) {}
    pub fn _remote_free_queue_buffer_identity_for_tests(&self) -> (usize, usize, usize) { (0, 0, 0) }
  }`)]] )).errors.filter(e => e.startsWith('reviewed hook missing ')), [
    `reviewed hook missing internals gate: ${epoch}::_set_slot_generation_for_tests`,
    `reviewed hook missing internals gate: ${epoch}::_remote_free_queue_buffer_identity_for_tests`,
  ]);
  const sharded = 'crates/containers/sefer-concurrent/src/sharded/sharded_region.rs';
  check(sharded, `#[cfg(feature = "internals")] impl ShardedRegion<T> {
    pub fn _reset_my_shard_binding_for_tests() {}
    pub fn _remote_free_queue_buffer_identity_for_tests(&self, shard: u16) -> Option<(usize, usize, usize)> { None }
    #[cfg(feature = "bench-internals")]
    pub fn _set_remote_free_hint_for_tests(&self, shard: u16, pending: bool) -> bool { false }
  }`, [['_reset_my_shard_binding_for_tests', 'ShardedRegion', false], ['_remote_free_queue_buffer_identity_for_tests', 'ShardedRegion', false], ['_set_remote_free_hint_for_tests', 'ShardedRegion', false]]);
  assert(verifyHooks(new Map([[sharded, scanFile(sharded, '#[cfg(feature = "internals")] impl ShardedRegion<T> { pub fn _set_remote_free_hint_for_tests(&self, shard: u16, pending: bool) -> bool { false } }')]])).errors.includes(`reviewed hook missing bench-internals gate: ${sharded}::_set_remote_free_hint_for_tests`));

  assert.deepEqual(verifyHooks(new Map([[sharded, scanFile(sharded, `impl ShardedRegion<T> {
    pub fn _reset_my_shard_binding_for_tests() {}
    pub fn _remote_free_queue_buffer_identity_for_tests(&self, shard: u16) -> Option<(usize, usize, usize)> { None }
    #[cfg(feature = "bench-internals")]
    pub fn _set_remote_free_hint_for_tests(&self, shard: u16, pending: bool) -> bool { false }
  }`)]] )).errors.filter(e => e.startsWith('reviewed hook missing ')), [
    `reviewed hook missing internals gate: ${sharded}::_reset_my_shard_binding_for_tests`,
    `reviewed hook missing internals gate: ${sharded}::_remote_free_queue_buffer_identity_for_tests`,
    `reviewed hook missing internals gate: ${sharded}::_set_remote_free_hint_for_tests`,
  ]);
  // Actual bootstrap module/file relationship, with no item-level gate.
  const parent = 'crates/core/sefer-registry/src/registry/bootstrap/mod.rs';
  const files = new Map([
    [parent, scanFile(parent, '#[cfg(feature = "internals")] mod ensure;')],
    [chunks, scanFile(chunks, 'pub const fn dbg_num_chunks() -> usize { 4 }')],
  ]);
  assert.deepEqual(verifyHooks(files).errors.filter(e => !e.startsWith('stale ')), []);
  files.set(parent, scanFile(parent, '#[cfg(any(feature = "internals", feature = "production"))] mod ensure;'));
  assert(verifyHooks(files).errors.includes(`reviewed hook missing internals gate: ${chunks}::dbg_num_chunks`));
  files.set(parent, scanFile(parent, '#[cfg(feature = "internals")] mod ensure;'));
  files.set('crates/core/sefer-registry/src/registry/bootstrap/alternate.rs', scanFile('crates/core/sefer-registry/src/registry/bootstrap/alternate.rs', '#[path = "ensure.rs"] mod ensure;'));
  assert(verifyHooks(files).errors.includes(`reviewed hook missing internals gate: ${chunks}::dbg_num_chunks`));

  // const unsafe is used by globalalloc-model; unsafe const is inventoried
  // defensively, not asserted to be a valid Rust qualifier order.
  for (const declaration of ['pub const unsafe fn', 'pub unsafe const fn']) {
    const parsed = scanFile('fixture.rs', `impl Core {\n/// # Safety\n/// Caller owns the allocation.\n${declaration} dbg_const_unsafe() {} }`);
    assert.deepEqual(parsed.hooks.map(h => [h.name, h.unsafe]), [['dbg_const_unsafe', true]]);
    assert.deepEqual(verifyHooks(new Map([['fixture.rs', parsed]]), { safe: [], unsafe: ['fixture.rs::dbg_const_unsafe'], surface: [] }).errors, []);
  }
}
function selfCheck() {
  handAuthoredCheck();
  signatureCheck();
  const fixturePolicy = { safe: [], unsafe: [], surface: [] };
  for (const name of ['dbg_const', 'value_for_test', 'value_for_tests', 'inject_fault']) {
    const file = scanFile('fixture.rs', `impl Core { pub const fn ${name}() {} }`);
    assert.equal(file.hooks.length, 1);
    assert(verifyHooks(new Map([['fixture.rs', file]]), fixturePolicy).errors.includes(`unreviewed safe ungated hook: fixture.rs::${name}`));
  }
  // Structural coverage only: these generated cases share the policy's rows
  // and therefore cannot serve as an independent classification/gating oracle.
  for (const row of REVIEWED_SURFACE) {
    const split = row.id.lastIndexOf('::'), name = row.id.slice(split + 2);
    const qualified = row.id.startsWith('crates/primitives/tagged-index-stack/src/imp.rs::');
    const file = qualified ? 'crates/primitives/tagged-index-stack/src/imp.rs' : row.id.slice(0, split);
    const owner = qualified && row.id.slice(file.length + 2).includes('::') ? row.id.slice(file.length + 2, split) : 'Core';
    const namedHook = /^dbg_|_for_tests?$|inject/.test(name) || ROOT_FORWARDERS.includes(`${file}::${name}`);
    const params = namedHook ? '' : 'p: *mut u8';
    const docs = row.kind === 'unsafe' ? '/// # Safety\n/// Caller owns the mapped allocation.\n' : '';
    const declaration = `${docs}pub ${row.kind === 'unsafe' ? 'unsafe ' : ''}fn ${name}(${params}) {}`;
    const skeleton = gates => `${gates}\nimpl ${owner} {\n${declaration}\n}`;
    // r18_sidecar_scan_bench is a bare cfg (see Cargo.toml check-cfg), like loom —
    // never a cargo feature, so the synthesized attribute must not use feature =.
    const attrs = row.gates.map(gate => gate === 'test-model' ? '#[cfg(any(tagged_index_stack_test, loom))]' : gate === 'loom' || gate === 'r18_sidecar_scan_bench' ? `#[cfg(${gate})]` : `#[cfg(feature = "${gate}")]`);
    const policy = { safe: row.kind === 'unsafe' ? [] : [row.id], unsafe: row.kind === 'unsafe' ? [row.id] : [], surface: [row] };
    // Free functions in the model crate must not accidentally acquire an owner.
    const source = gates => qualified && owner === 'Core' ? `${gates}\n${declaration}` : skeleton(gates);
    assert.deepEqual(verifyHooks(new Map([[file, scanFile(file, source(attrs.join('\n')))]]), policy).errors, []);
    for (let index = 0; index < attrs.length; index++) {
      const mutant = attrs.filter((_, i) => i !== index).join('\n');
      assert(verifyHooks(new Map([[file, scanFile(file, source(mutant))]]), policy).errors.includes(`reviewed hook missing ${row.gates[index]} gate: ${row.id}`));
    }
    if (row.kind === 'unsafe') {
      const noDocs = source(attrs.join('\n')).replace(docs, '');
      assert(verifyHooks(new Map([[file, scanFile(file, noDocs)]]), policy).errors.includes(`unsafe hook lacks its # Safety contract: ${row.id}`));
    }
  }
  function signatureCheck() {
    // R6-T38 signature rule: safe crate-public fns with raw-pointer parameters
    // are inventoried regardless of name.
    const directory = 'crates/core/sefer-segment/src/segment/segment_route/directory/route_directory.rs';
    // Fixture fn is `enroll` (not the real `register`): `register` at this path
    // is a registered ROOT_FORWARDERS name (pass-9d selfCheck surface row), and
    // this check pins the UNNAMED signature-rule path.
    const signature = '#[cfg(feature = "internals")]\nimpl RouteDirectory {\n  pub fn enroll(&self, root: *mut u8, len: usize, route_ptr: *mut u8, owner: usize) {}\n}';
    const signatureHook = scanFile(directory, signature);
    assert.equal(signatureHook.hooks.length, 1);
    assert(signatureHook.hooks[0].pointerParams);
    assert(!signatureHook.hooks[0].named);
    assert(verifyHooks(new Map([[directory, signatureHook]]), { safe: [], unsafe: [], surface: [] }).errors.includes(`unreviewed safe ungated hook: ${directory}::enroll`));
    // The same fn without a raw-pointer parameter stays outside the surface.
    const scalarSignature = '#[cfg(feature = "internals")]\nimpl RouteDirectory {\n  pub fn enroll(&self, root: usize, len: usize) {}\n}';
    assert.equal(scanFile(directory, scalarSignature).hooks.length, 0);
    // A paren inside a generic bound must not hide the parameter-list pointer.
    const genericFn = scanFile('fixture.rs', 'impl Core {\n  pub fn take<F: Fn(*mut u8)>(f: F, p: *mut u8) {}\n}');
    assert(genericFn.hooks[0].pointerParams);
    // A raw pointer only in the return type does not trigger the rule.
    const returnOnly = scanFile('fixture.rs', 'impl Core {\n  pub fn base(index: usize) -> *mut u8 { core::ptr::null_mut() }\n}');
    assert.equal(returnOnly.hooks.length, 0);
    // A gated unsafe raw-pointer fn is inventoried and must carry # Safety.
    const unsafeBare = scanFile('fixture.rs', '#[cfg(feature = "internals")]\nimpl Core {\n  pub unsafe fn foo(p: *mut u8) {}\n}');
    assert.equal(unsafeBare.hooks.length, 1);
    assert(unsafeBare.hooks[0].pointerParams);
    assert(unsafeBare.hooks[0].unsafe);
    assert(!unsafeBare.hooks[0].named);
    assert(verifyHooks(new Map([['fixture.rs', unsafeBare]]), { safe: [], unsafe: ['fixture.rs::foo'], surface: [] }).errors.includes('unsafe hook lacks its # Safety contract: fixture.rs::foo'));
    const unsafeDocs = scanFile('fixture.rs', '#[cfg(feature = "internals")]\nimpl Core {\n  /// # Safety\n  /// Caller owns the allocation.\n  pub unsafe fn foo(p: *mut u8) {}\n}');
    assert.deepEqual(verifyHooks(new Map([['fixture.rs', unsafeDocs]]), { safe: [], unsafe: ['fixture.rs::foo'], surface: [] }).errors, []);
    // A # Safety above the impl documents the impl, not the fn.
    const implDocs = scanFile('fixture.rs', '/// # Safety\n/// Caller owns the allocation.\n#[cfg(feature = "internals")]\nimpl Core {\n  pub unsafe fn foo(p: *mut u8) {}\n}');
    assert(verifyHooks(new Map([['fixture.rs', implDocs]]), { safe: [], unsafe: ['fixture.rs::foo'], surface: [] }).errors.includes('unsafe hook lacks its # Safety contract: fixture.rs::foo'));
    // Outside the gates the same fn is the production unsafe API: skipped.
    const unsafeUngated = scanFile('fixture.rs', 'impl Core {\n  pub unsafe fn foo(p: *mut u8) {}\n}');
    assert.equal(unsafeUngated.hooks.length, 1);
    assert.deepEqual(verifyHooks(new Map([['fixture.rs', unsafeUngated]]), { safe: [], unsafe: [], surface: [] }).errors, []);
    // An arrow inside a generic bound must not terminate the generic list.
    const arrowFn = scanFile('fixture.rs', 'impl Core {\n  pub fn foo<F: Fn() -> ()>(f: F, p: *mut u8) {}\n}');
    assert(arrowFn.hooks[0].pointerParams);
  };
  const duplicatePolicy = { safe: ['fixture.rs::dbg_dup', 'fixture.rs::dbg_dup'], unsafe: [], surface: [] };
  assert(verifyHooks(new Map(), duplicatePolicy).errors.includes('duplicate reviewed safe hook: fixture.rs::dbg_dup'));
  assert(verifyHooks(new Map(), duplicatePolicy).errors.includes('stale safe ungated allowlist entry (review required, never auto-pruned): fixture.rs::dbg_dup'));
  assert(verifyHooks(new Map(), { safe: [], unsafe: ['fixture.rs::dbg_retired'], surface: [] }).errors.includes('stale unsafe allowlist entry (review required, never auto-pruned): fixture.rs::dbg_retired'));
  const duplicateUnsafe = { safe: [], unsafe: ['fixture.rs::dbg_dup', 'fixture.rs::dbg_dup'], surface: [] };
  assert(verifyHooks(new Map(), duplicateUnsafe).errors.includes('duplicate reviewed unsafe hook: fixture.rs::dbg_dup'));
  const duplicateRow = { id: 'fixture.rs::dbg_dup', kind: 'observer', gates: [], reason: 'pure local value' };
  assert(verifyHooks(new Map(), { safe: [], unsafe: [], surface: [duplicateRow, duplicateRow] }).errors.includes('duplicate reviewed surface row: fixture.rs::dbg_dup'));
  const mustGate = ['#[cfg(feature = "bench-internals")]', '#[cfg(all(feature = "x", all(feature = "bench-internals", feature = "y")))]'];
  const mustReject = ['', '#[cfg(not(feature = "bench-internals"))]', '#[cfg(any(feature = "bench-internals", feature = "x"))]', '#[cfg(any(feature = "bench-internals", all(feature = "bench-internals", feature = "x")))]', '#[cfg_attr(feature = "bench-internals", allow(dead_code))]', '#[cfg(target_os = "bench-internals")]', '#[cfg(feature = "bench-internals", nonsense)]'];
  for (const attr of mustGate) assert(attributesRequire([attr], 'bench-internals'), attr);
  for (const attr of mustReject) assert(!attributesRequire([attr], 'bench-internals'), attr);
  const zeroArg = scanFile('fixture.rs', 'impl Core { pub fn dbg_mutate(&mut self) { self.clear(); } }');
  assert.equal(zeroArg.hooks.length, 1); assert(!attributesRequire(zeroArg.hooks[0].attrs, 'bench-internals'));
  assert(verifyHooks(new Map([['fixture.rs', zeroArg]])).errors.includes('unreviewed safe ungated hook: fixture.rs::dbg_mutate'));
  const optional = scanFile('fixture.rs', '#[cfg_attr(feature = "bench-internals", allow(dead_code))]\npub fn dbg_mutate() {}');
  assert(invalidBenchAttribute(optional.hooks[0].attrs[0]));
  const inherited = scanFile('fixture.rs', '#[cfg(all(feature = "x", feature = "bench-internals"))]\nimpl Core { pub fn dbg_mutate() {} }');
  assert(attributesRequire(inherited.hooks[0].attrs, 'bench-internals'));
  assert.equal(scanFile('fixture.rs', '// pub fn dbg_comment() {}\nconst X: &str = r#"pub fn dbg_raw_string() {}"#;\n/* pub fn dbg_block() {} */').hooks.length, 0);
  const safety = scanFile('fixture.rs', '/// # Safety\n/// Must own the allocation.\n#[cfg(feature = "bench-internals")]\npub unsafe fn dbg_publish(ptr: *mut u8) {}');
  assert(safety.hooks[0].unsafe); assert(safety.hooks[0].docs.includes('# Safety'));
  const parent = scanFile('fixture/mod.rs', '#[cfg(feature = "bench-internals")]\nmod child;');
  const child = scanFile('fixture/child.rs', 'pub fn dbg_mutate() {}');
  assert.equal(child.hooks.length, 1);
  assert(!verifyHooks(new Map([['fixture/mod.rs', parent], ['fixture/child.rs', child]])).errors.includes('unreviewed safe ungated hook: fixture/child.rs::dbg_mutate'));
  const alternate = scanFile('fixture/alternate.rs', '#[path = "child.rs"]\nmod child;');
  assert(verifyHooks(new Map([['fixture/mod.rs', parent], ['fixture/alternate.rs', alternate], ['fixture/child.rs', child]])).errors.includes('unreviewed safe ungated hook: fixture/child.rs::dbg_mutate'));
  const fakeSafety = scanFile('fixture.rs', 'const DOC: &str = "/// # Safety";\npub unsafe fn dbg_unsafe() {}');
  assert(verifyHooks(new Map([['fixture.rs', fakeSafety]]), { safe: [], unsafe: ['fixture.rs::dbg_unsafe'], surface: [] }).errors.includes('unsafe hook lacks its # Safety contract: fixture.rs::dbg_unsafe'));
  const observerFile = 'src/global/sefer_alloc/diag.rs';
  const observer = scanFile(observerFile, '/// # Safety\n/// Caller owns the live allocation.\n#[cfg(feature = "internals")]\npub unsafe fn dbg_current_reservation_for_test(ptr: *mut u8) {}');
  const observerErrors = verifyHooks(new Map([[observerFile, observer]])).errors;
  assert(observerErrors.includes(`measurement-only unsafe hook is not bench-internals-gated: ${CURRENT_RESERVATION_HOOK}`));
  const observerBenchOnly = scanFile(observerFile, '/// # Safety\n/// Caller owns the live allocation.\n#[cfg(feature = "bench-internals")]\npub unsafe fn dbg_current_reservation_for_test(ptr: *mut u8) {}');
  assert(verifyHooks(new Map([[observerFile, observerBenchOnly]])).errors.includes(`reservation observer is not internals-gated: ${CURRENT_RESERVATION_HOOK}`));
  const pureFile = 'crates/core/sefer-registry/src/fallback.rs';
  const pureId = `${pureFile}::dbg_fallback_lock_spin_transition`;
  const pureHook = scanFile(pureFile, '#[cfg(all(feature = "internals", feature = "bench-internals"))]\nimpl SeferAlloc { pub fn dbg_fallback_lock_spin_transition(spins: u32) -> (u32, bool) { (spins, false) } }');
  assert.equal(pureHook.hooks.length, 1);
  for (const feature of ['internals', 'bench-internals']) assert(attributesRequire(pureHook.hooks[0].attrs, feature));
  for (const gate of ['internals', 'bench-internals']) {
    const remaining = gate === 'internals' ? 'bench-internals' : 'internals';
    const missing = scanFile(pureFile, `#[cfg(feature = "${remaining}")]\npub fn dbg_fallback_lock_spin_transition() {}`);
    assert(verifyHooks(new Map([[pureFile, missing]])).errors.includes(`pure transition missing ${gate} gate: ${pureId}`));
  }
  const pureResult = verifyHooks(new Map([[pureFile, pureHook], ['fixture.rs', zeroArg]]));
  assert.equal(pureResult.safe, 2);
  assert.equal(pureResult.gated, 1);
  assert(!pureResult.errors.some(error => error.includes(pureId)));
  assert(pureResult.errors.includes('unreviewed safe ungated hook: fixture.rs::dbg_mutate'));
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
if (process.argv.includes('--self-test')) { selfCheck(); console.log('[verify-dbg-hook-safety] parser adversarial self-checks PASS'); }
else {
  const files = new Map([...listRust(join(REPO_ROOT, 'src')), ...listRust(join(REPO_ROOT, 'crates'))].map(path => { const id = idFor(path); return [id, scanFile(id, readFileSync(path, 'utf8'))]; }));
  if (!files.size) throw new Error('no Rust source files found');
  const result = verifyHooks(files);
  if (result.errors.length) { console.error(`[verify-dbg-hook-safety] FAIL\n${result.errors.join('\n')}`); process.exitCode = 1; }
  else console.log(`[verify-dbg-hook-safety] PASS: ${result.safe} reviewed safe, ${result.unsafe} reviewed unsafe, ${result.gated} genuinely bench-gated safe hooks`);
}
}
