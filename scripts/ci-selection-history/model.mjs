// Pure selection + weighting model for the P1.5 selection-potential
// measurement (docs/perf/P1_5_SELECTION_POTENTIAL.md). No IO, no clock, no
// randomness: every function takes explicit inputs so tests and the CLI can
// redirect all sources. Reuses scripts/ci-plan.mjs, never re-implements it.

import { CRATE_GROUPS, plan, cargoTomlBenchOnly } from '../ci-plan.mjs';
import { closureDeps, featuresEnabling } from './manifest.mjs';

export const ROOT_GROUPS = ['fast', 'seam', 'os'];
const EXCLUDED = new Set(['always', 'full']); // `always` runs regardless; excluded from BOTH sides.
export const CARGO_GROUPS = (g) => !EXCLUDED.has(g) && (g === 'fast' || g === 'seam' || g === 'os' || g.startsWith('crate_'));

// ── eligibility ─────────────────────────────────────────────────────────────
// P1.5-eligible: diff paths ALL inside crates/**, docs/**, or *.md. Commits
// touching anything else are NOT eligible: model undefined, savings 0. A
// crates/** path from the pre-restructure FLAT layout (crates/<crate>/...,
// where the second segment is not a workspace member dir) cannot be mapped
// onto the current workspace crate graph either -> its own bucket.
export function eligibility(files, memberDirs) {
  const offenders = [], legacy = [];
  for (const f of files) {
    const p = f.replace(/\\/g, '/');
    if (p.startsWith('docs/') || /\.md$/.test(p)) continue;
    if (!p.startsWith('crates/')) { offenders.push(p); continue; }
    const m = /^crates\/([^/]+)\/([^/]+)\//.exec(p);
    if (!m || !(memberDirs.has(m[2]) || memberDirs.has(m[1]))) legacy.push(p);
  }
  if (offenders.length) return { ok: false, bucket: 'ineligible-paths', offenders };
  if (legacy.length) return { ok: false, bucket: 'legacy-flat-layout', offenders: legacy };
  return { ok: true, bucket: 'eligible', offenders: [] };
}

// ── crate graph ─────────────────────────────────────────────────────────────
/** Distinct changed workspace crates (the crate NAME); both the nested
 *  crates/<category>/<crate>/ and the historical flat crates/<crate>/ shapes. */
export function changedCrates(files, memberDirs) {
  const out = new Set();
  for (const f of files) {
    const m = /^crates\/([^/]+)\/([^/]+)\//.exec(f.replace(/\\/g, '/'));
    if (!m) continue;
    if (memberDirs.has(m[2])) out.add(m[2]);
    else if (memberDirs.has(m[1])) out.add(m[1]);
  }
  return [...out].sort();
}

export const manifestTouch = (files) => files.some(f => /^crates\/[^/]+\/[^/]+\/Cargo\.toml$/.test(f.replace(/\\/g, '/')));

/**
 * consumers: Map dependencyCrate -> Set(consumerCrate) built from the root
 * manifest's [dependencies]/[dev-dependencies]/cfg(loom|kani) tables and each
 * member manifest's [dependencies] table (member-name key with a `path`).
 * Affected = changed crates ∪ transitive workspace consumers.
 */
export function buildConsumers(rootManifest, rootName, memberManifests, memberNames) {
  const consumers = new Map();
  const add = (dep, consumer) => {
    if (!consumers.has(dep)) consumers.set(dep, new Set());
    consumers.get(dep).add(consumer);
  };
  for (const [dep] of rootManifest.deps) add(dep, rootName);
  for (const [dep] of rootManifest.devDeps) add(dep, rootName);
  for (const [dep] of rootManifest.cfgDeps) add(dep, rootName);
  for (const mm of memberManifests) {
    for (const [dep, entry] of mm.deps) {
      if ((memberNames.has(dep) || dep === rootName) && entry.path) add(dep, mm.name);
    }
  }
  return consumers;
}

export function affectedSet(changed, consumers) {
  const affected = new Set(changed);
  const queue = [...changed];
  while (queue.length) {
    for (const c of consumers.get(queue.shift()) ?? []) if (!affected.has(c)) { affected.add(c); queue.push(c); }
  }
  return [...affected].sort();
}

// ── root job feature usage (rule (b)) ───────────────────────────────────────
/**
 * Scan each ROOT job's block (groups fast/seam/os) of the EFFECTIVE workflow
 * text for `--features <tokens>` and `--all-features` run-line occurrences.
 * Returns Map group -> { tokens:Set, all:bool }.
 */
export function rootFeatureUsage(expandedCiText, jobs) {
  const lines = expandedCiText.split(/\r?\n/);
  const at = lines.findIndex(l => /^jobs:\s*$/.test(l));
  const starts = [];
  for (let i = at + 1; i < lines.length; i++) {
    const m = /^  ([A-Za-z0-9_-]+):\s*$/.exec(lines[i]);
    if (m) starts.push({ id: m[1], i });
  }
  const blocks = new Map(starts.map((s, i) => [s.id, lines.slice(s.i, starts[i + 1]?.i ?? lines.length)]));
  const usage = new Map(ROOT_GROUPS.map(g => [g, { tokens: new Set(), all: false }]));
  for (const job of jobs) {
    if (!ROOT_GROUPS.includes(job.group)) continue;
    for (const line of blocks.get(job.id) ?? []) {
      if (/^\s*#/.test(line)) continue; // comments must not forge run lines
      if (!/(cargo|cross)\s/.test(line)) continue;
      const u = usage.get(job.group);
      if (line.includes('--all-features')) u.all = true;
      for (const m of line.matchAll(/--features\s+(?:"([^"]*)"|'([^']*)'|(\S+))/g)) {
        const raw = m[1] ?? m[2] ?? m[3];
        for (const tok of raw.split(/\s+/)) if (tok && tok !== '\\') u.tokens.add(tok.replace(/["']/g, ''));
      }
    }
  }
  return usage;
}

/**
 * Which ROOT groups build crate X (per the measurement model):
 *  - mandatory normal [dependency]: ALL root groups;
 *  - optional dep: exactly the groups whose run lines enable a feature whose
 *    closure contains `dep:X` (--all-features enables every optional dep);
 *  - [dev-dependencies] and [target.'cfg(loom|kani)'.dependencies] are NOT
 *    derivable from run lines: conservative ALL root groups + flag.
 */
export function rootGroupBuilds(crate, rootManifest, usage) {
  const all = () => ({ groups: new Set(ROOT_GROUPS), flag: null });
  if (rootManifest.cfgDeps.has(crate)) {
    return { groups: new Set(ROOT_GROUPS), flag: `${crate}: cfg(loom/kani) target dependency — not derivable from run lines; conservative ALL root groups` };
  }
  if (rootManifest.devDeps.has(crate)) {
    return { groups: new Set(ROOT_GROUPS), flag: `${crate}: [dev-dependencies] — not derivable from run lines; conservative ALL root groups` };
  }
  const dep = rootManifest.deps.get(crate);
  if (!dep) return { groups: new Set(), flag: `${crate}: not a root dependency` };
  if (!dep.optional) return all();
  const depNames = new Set([...rootManifest.deps.keys()]);
  const groups = new Set();
  for (const [g, u] of usage) {
    if (u.all || closureDeps(u.tokens, rootManifest.features, depNames).has(crate)) groups.add(g);
  }
  return { groups, flag: null };
}

// ── P1.5 selection ──────────────────────────────────────────────────────────
/**
 * P1.5 selection for one eligible commit. `currentGroups` is the same
 * commit's plan() result (used only for the conservative fallback path).
 */
export function selectP15({ files, event, memberDirs, consumers, rootManifest, rootName, usage, seamTests, cargoBenchOnly, currentGroups }) {
  const changed = changedCrates(files, memberDirs);
  const affected = affectedSet(changed, consumers);
  const groups = new Set();
  const flags = [];
  for (const x of affected) {
    if (CRATE_GROUPS[x]) groups.add(CRATE_GROUPS[x]);
    else if (x !== rootName) flags.push(`${x}: no CRATE_GROUPS entry — conservative fallback to today's selection for its crate paths`);
  }
  const rootDeps = new Set([...rootManifest.deps.keys(), ...rootManifest.devDeps.keys(), ...rootManifest.cfgDeps.keys()]);
  if (affected.some(c => rootDeps.has(c))) {
    for (const x of affected) {
      if (!rootDeps.has(x)) continue;
      const r = rootGroupBuilds(x, rootManifest, usage);
      for (const g of r.groups) groups.add(g);
      if (r.flag) flags.push(r.flag);
    }
  }
  // Unknown/conservative path: fall back to today's selection for this
  // commit's crate paths.
  const crateFiles = files.filter(f => f.replace(/\\/g, '/').startsWith('crates/'));
  if (changed.some(c => !CRATE_GROUPS[c])) {
    const fallback = plan({ files: crateFiles, event, trailerFull: false, seamTests, cargoBenchOnly });
    for (const g of Object.keys(fallback)) if (fallback[g] && CARGO_GROUPS(g)) groups.add(g);
    flags.push('unknown crate: fell back to today\'s selection for the commit\'s crate paths');
  }
  return { changed, affected, groups: new Set([...groups].sort()), flags, manifestTouch: manifestTouch(files) };
}

// ── job weights ─────────────────────────────────────────────────────────────
const nameRx = (name) => new RegExp('^' + name.split(/\$\{\{\s*matrix\.[^}]*\}\}/).map(escape).join('[^)]*') + '$');
const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

/**
 * Map run jobs -> workflow jobs by NAME (exact first, then matrix-pattern for
 * the leftovers, unique-only). weekly/dispatch jobs are excluded from BOTH
 * sides; unmapped jobs go to a synthetic bucket. seconds = completed - started
 * (null timestamps contribute 0; skipped jobs contribute 0 and are counted).
 */
export function buildWeightsFromJobs(runJson, ciJobs) {
  const byName = new Map();
  for (const j of ciJobs) { const k = j.name || j.id; if (k && !byName.has(k)) byName.set(k, j); }
  const pending = ciJobs.filter(j => j.name && /\$\{\{/.test(j.name)).map(j => ({ j, rx: nameRx(j.name) }));
  const weights = new Map(ROOT_GROUPS.concat().map(g => [g, 0]));
  const unmapped = [], skipped = [], excluded = [];
  let mapped = 0, zeroTimestamp = 0;
  for (const run of runJson.jobs ?? []) {
    let job = byName.get(run.name);
    if (!job) {
      const cands = pending.filter(p => p.rx.test(run.name));
      if (cands.length === 1) job = cands[0].j; // matrix rows share one workflow job
    }
    if (!job) { unmapped.push(run.name); continue; }
    mapped++;
    if (job.group === 'weekly' || job.group === 'dispatch') { excluded.push(run.name); continue; }
    if (run.conclusion === 'skipped') { skipped.push(run.name); continue; }
    const dur = (run.started_at && run.completed_at)
      ? Math.max(0, Math.round((Date.parse(run.completed_at) - Date.parse(run.started_at)) / 1000)) : (zeroTimestamp++, 0);
    const g = job.group;
    weights.set(g, (weights.get(g) ?? 0) + dur);
  }
  return { weights, unmapped, skipped, excluded, mapped, zeroTimestamp };
}

/** seconds contributed by the cargo groups in `groups` (always/full excluded; union of groups, a job counts once). */
export function groupSeconds(weights, groups) {
  let s = 0;
  for (const g of groups) if (CARGO_GROUPS(g)) s += weights.get(g) ?? 0;
  return s;
}

// ── per-commit rows ─────────────────────────────────────────────────────────
export const PLAN_ARGS = ({ files, event, seamTests, cargoBenchOnly }) =>
  ({ files, event, trailerFull: false, seamTests, cargoBenchOnly });

export const cargoGroupsOf = (planResult) => new Set(Object.keys(planResult).filter(g => planResult[g] && CARGO_GROUPS(g)).sort());

/**
 * One row per commit. Asserts: every commit exactly once (caller-checked sha
 * uniqueness), P1.5 ⊆ current for the PR policy per commit (hard fail), and
 * aggregates recomputed twice via independent re-aggregation.
 */
export function computeRows({ commits, seamTests, memberDirs, consumers, rootManifest, rootName, usage, weights }) {
  const rows = [];
  const buckets = new Map();
  for (const c of commits) {
    const el = eligibility(c.files, memberDirs);
    if (!el.ok) {
      const b = buckets.get(el.bucket) ?? { count: 0, shas: [], sample: [] };
      b.count++; b.shas.push(c.sha);
      for (const s of el.offenders.slice(0, 3)) if (b.sample.length < 3) b.sample.push(s);
      buckets.set(el.bucket, b);
      rows.push({ sha: c.sha, eligible: false, bucket: el.bucket, changed: [], flags: [], manifestTouch: false, policies: {} });
      continue;
    }
    const policies = {};
    let divergent = null;
    for (const event of ['pull_request', 'push']) {
      const cargoBenchOnly = c.files.includes('Cargo.toml') && c.cargoTomlDiff !== undefined
        ? cargoTomlBenchOnly(c.cargoTomlDiff) : false;
      const current = cargoGroupsOf(plan(PLAN_ARGS({ files: c.files, event, seamTests, cargoBenchOnly })));
      const p15 = selectP15({ files: c.files, event, memberDirs, consumers, rootManifest, rootName, usage, seamTests, cargoBenchOnly, currentGroups: current });
      if (event === 'pull_request') {
        const extra = [...p15.groups].filter(g => !current.has(g));
        // Subset sanity: a P1.5 group today's selection lacks means the
        // today-vs-P1.5 comparison is not meaningful for that commit, because
        // ci-plan's current crate-path rule is INCOMPLETE there: it cannot
        // resolve the historical flat `crates/<crate>/...` layout (so it misses
        // the crate gates and consumer gates the graph model finds), and it
        // adds `os` only for platform crates while root os jobs (e.g. the
        // MSRV job's `--all-features`) build every root dep. Such commits are
        // counted in their own bucket with savings 0 (never silently added to
        // either total); any OTHER extra group is a real model bug and fails
        // loudly.
        const affectedSet = new Set(p15.affected);
        const explainable = (g) => g === 'os' || (g.startsWith('crate_') &&
          [...affectedSet].some(c => CRATE_GROUPS[c] === g));
        if (extra.some(g => !explainable(g))) {
          throw new Error(`P1.5 model bug: commit ${c.sha.slice(0, 12)}: P1.5 groups not a subset of current (extra: ${extra.join(',')})`);
        }
        if (extra.length) divergent = { extra, p15 };
      }
      policies[event] = {
        current, p15: p15.groups,
        currentSeconds: groupSeconds(weights, current),
        p15Seconds: groupSeconds(weights, p15.groups),
        flags: p15.flags, changed: p15.changed, affected: p15.affected,
        manifestTouch: p15.manifestTouch,
      };
    }
    if (divergent) {
      const b = buckets.get('p15-not-subset') ?? { count: 0, shas: [], sample: [] };
      b.count++; b.shas.push(c.sha);
      for (const g of divergent.extra.slice(0, 3)) if (!b.sample.includes(g)) b.sample.push(g);
      buckets.set('p15-not-subset', b);
      rows.push({ sha: c.sha, eligible: false, bucket: 'p15-not-subset', changed: divergent.p15.changed, flags: [...divergent.p15.flags, `P1.5 adds ${divergent.extra.join(',')} — commit routed to the p15-not-subset bucket with savings 0`], manifestTouch: divergent.p15.manifestTouch, policies: {} });
      continue;
    }
    rows.push({ sha: c.sha, eligible: true, bucket: 'eligible', changed: policies.pull_request.changed, flags: policies.pull_request.flags, manifestTouch: policies.pull_request.manifestTouch, policies });
  }
  return { rows, buckets };
}


/** Aggregates for one policy over the eligible rows; computed twice via
 *  independent re-aggregation (loop vs reduce over a re-selection pass) and
 *  compared before returning. */
export function aggregate(rows, policy, weights, reselect) {
  const eligible = rows.filter(r => r.eligible);
  let today = 0, p15 = 0, withDelta = 0;
  const perCrate = new Map();
  for (const r of eligible) {
    const pol = r.policies[policy];
    today += pol.currentSeconds;
    p15 += pol.p15Seconds;
    const delta = pol.currentSeconds - pol.p15Seconds;
    if (delta > 0) withDelta++;
    for (const crate of pol.changed.length ? pol.changed : ['(docs-only)']) {
      const e = perCrate.get(crate) ?? { commits: 0, saved: 0 };
      e.commits++;
      e.saved += Math.round(delta / (pol.changed.length || 1));
      perCrate.set(crate, e);
    }
  }
  // independent re-aggregation: recompute selections and sums from scratch
  let today2 = 0, p152 = 0;
  for (const r of eligible) {
    const { current, p15: g15 } = reselect(r.sha, policy);
    if (current.join(',') !== [...r.policies[policy].current].join(',') ||
        g15.join(',') !== [...r.policies[policy].p15].join(',')) {
      throw new Error(`re-selection mismatch for ${r.sha.slice(0, 12)} (${policy})`);
    }
    today2 += groupSeconds(weights, current);
    p152 += groupSeconds(weights, g15);
  }
  const sum = arr => arr.reduce((a, b) => a + b, 0);
  const today3 = sum(eligible.map(r => r.policies[policy].currentSeconds));
  const p153 = sum(eligible.map(r => r.policies[policy].p15Seconds));
  if (today !== today2 || today !== today3 || p15 !== p152 || p15 !== p153) {
    throw new Error('aggregate re-computation mismatch');
  }
  const delta = today - p15;
  return {
    eligibleCount: eligible.length,
    todaySeconds: today, p15Seconds: p15, deltaSeconds: delta,
    commitsWithDelta: withDelta,
    shareCommits: eligible.length ? withDelta / eligible.length : 0,
    shareSeconds: today ? delta / today : 0,
    perCrate: [...perCrate.entries()].sort((a, b) => a[0].localeCompare(b[0])),
  };
}
