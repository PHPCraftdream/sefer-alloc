// Mergeability analysis + grouping of test targets.
//
// Purpose: given per-target classification facts (from features.classifySource)
// plus discovery + pin data, derive:
//   - mergeable: boolean and the FULL list of violated conditions
//     (unmergeable_reasons, stable machine-sortable slugs)
//   - groups over mergeable targets only; group key
//     (theme, sorted required_features, file-level-cfg predicate or '-')
//   - group stats: members, saved_links = members - 1
//   - pilot candidate: largest saved_links, tie -> lexicographically smallest
//     group name
//   - relaxation ladder: strict; (a) uniform required-features allowed;
//     (b) additionally uniform file-level cfg allowed; (c) additionally
//     externally-pinned names allowed. Each rung reports file count and
//     total saved links.
//
// Slug vocabulary (stable, machine-sortable):
//   global_allocator, first_config_wins:<marker>, process_spawn, abort,
//   loom_only, miri_only, kani_gated, harness_false, test_disabled,
//   required_features:<a+b>, file_cfg,
//   externally_pinned:<kind>@<file>
// should_panic / catch_unwind are informational and NEVER block merging.
//
// Theme prefix: the leading [A-Za-z][A-Za-z0-9]* run of the name (i.e. the
// text before the first underscore/non-alnum char). This reproduces the plan's
// examples: regression_r4_3_config_conflict -> 'regression',
// b4_r1_pin_cache -> 'b4', alloc_core_batch -> 'alloc'; a name with no
// underscore yields the whole (leading-run) name. Documented interpretation:
// the plan's "first '_' followed by a non-alpha char" wording is equivalent
// here because the leading run always terminates at the first underscore.
//
// Relaxation ladder (2026-11-15 relabel):
//   compat           THE task-relevant rung: uniform required-features AND
//                    uniform file-level cfg stay in the group key; hard
//                    blockers still apply; externally-pinned names excluded.
//                    Its pilot IS the pilot candidate (if non-trivial pool).
//   looser-features  additionally allow non-uniform required-features
//                    (still pinned-excluded); NOT pilot-eligible unless the
//                    group's required-features become uniform.
//   looser-cfg       additionally allow non-uniform file-level cfg; same
//                    label treatment.
//   allow-pinned     additionally allow externally-pinned names; NOT
//                    pilot-eligible (members externally name-pinned; listed
//                    for scale only).
//
// All functions are pure; asserts throw on contradictions (group members must
// equal the mergeable set; sum(saved_links) === mergeableCount - groupCount;
// every pairwise intersection size <= min(|A|,|B|)).

/**
 * Compute mergeability + reasons for one target.
 * facts: classifySource(...) result; target: discoverTargets entry;
 * externalPins: {count, hits} or null; cfgKeys include 'loom'|'miri'|'kani'
 * via file-level cfg gating (loom_only/miri_only/kani_gated already carry
 * that). Returns { mergeable, reasons: [slug...] } sorted.
 */
export function mergeability(target, facts, externalPins) {
  const reasons = [];
  if (facts.global_allocator) reasons.push('global_allocator');
  for (const m of facts.first_config_wins) reasons.push(`first_config_wins:${m}`);
  if (facts.process_spawn) reasons.push('process_spawn');
  if (facts.abort_marker) reasons.push('abort');
  if (facts.loom_only) reasons.push('loom_only');
  if (facts.miri_only) reasons.push('miri_only');
  if (facts.kani_gated) reasons.push('kani_gated');
  if (!target.harness) reasons.push('harness_false');
  if (!target.test_enabled) reasons.push('test_disabled');
  if (target.required_features.length > 0) {
    reasons.push(`required_features:${target.required_features.slice().sort().join('+')}`);
  }
  if (facts.file_level_cfg !== null || facts.file_level_cfg_attr !== null) reasons.push('file_cfg');
  if (externalPins && externalPins.count > 0) {
    for (const h of externalPins.hits) reasons.push(`externally_pinned:${h.kind}@${h.file}`);
  }
  reasons.sort((a, b) => a.localeCompare(b));
  return { mergeable: reasons.length === 0, reasons };
}

/** Theme prefix of a target name (leading [A-Za-z][A-Za-z0-9]* run). */
export function themePrefix(name) {
  const m = name.match(/^[A-Za-z][A-Za-z0-9]*/);
  return m ? m[0] : name;
}

/** Group key for a target under strict rules. */
export function groupKey(target, facts) {
  const theme = themePrefix(target.name);
  const feats = target.required_features.slice().sort().join('+') || '-';
  const cfg = facts.file_level_cfg || facts.file_level_cfg_attr || '-';
  return `${theme}|${feats}|${cfg}`;
}

/**
 * Build groups over targets already known mergeable.
 * mergeableRows: [{target, facts}] (all mergeable). Returns
 * { groups: Map key -> {key, members:[names], savedLinks} }.
 */
export function buildGroups(mergeableRows, keyFn = groupKey) {
  const groups = new Map();
  for (const row of mergeableRows) {
    const key = keyFn(row.target, row.facts);
    if (!groups.has(key)) groups.set(key, { key, members: [], savedLinks: 0 });
    groups.get(key).members.push(row.target.name);
  }
  for (const g of groups.values()) {
    g.members.sort((a, b) => a.localeCompare(b));
    g.savedLinks = g.members.length - 1;
  }
  return groups;
}

/** Pilot candidate: largest savedLinks, tie -> lexicographically smallest key. */
export function pilotCandidate(groups) {
  let best = null;
  for (const g of groups.values()) {
    if (!best || g.savedLinks > best.savedLinks ||
        (g.savedLinks === best.savedLinks && g.key.localeCompare(best.key) < 0)) best = g;
  }
  return best;
}

/**
 * Relaxation ladder. rows: ALL target rows [{target, facts, externalPins}].
 * Returns [{rung, description, pilotEligible, eligibilityReason, fileCount,
 * groupCount, savedLinks, pilot}].
 */
export function relaxationLadder(rows) {
  const rungs = [];
  const mk = (label, keyFn, relax) => {
    const mergeableRows = rows.filter((row) => !relaxBlocked(row, relax));
    const groups = buildGroups(mergeableRows, keyFn);
    const savedLinks = [...groups.values()].reduce((s, g) => s + g.savedLinks, 0);
    const pilot = pilotCandidate(groups);
    rungs.push({
      rung: label.rung,
      description: label.description,
      pilotEligible: label.pilotEligible,
      eligibilityReason: label.eligibilityReason,
      fileCount: mergeableRows.length,
      groupCount: groups.size,
      savedLinks,
      pilot: pilot && { key: pilot.key, members: pilot.members, savedLinks: pilot.savedLinks },
    });
  };

  mk({ rung: 'strict', description: 'strict rules (see report definitions)',
    pilotEligible: false,
    eligibilityReason: 'not eligible: strict rules block any file with required-features or a file-level cfg, so the pool cannot form uniform groups' },
    groupKey, null);

  mk({ rung: 'compat', description: 'uniform required-features + uniform file-level cfg in the group key; hard blockers apply; externally-pinned excluded',
    pilotEligible: true,
    eligibilityReason: 'pilot-eligible: uniform required-features and uniform file-level cfg in the group key, hard blockers apply, externally-pinned names excluded' },
    groupKey, { features: true, fileCfg: true });

  mk({ rung: 'looser-features', description: 'additionally allow non-uniform required-features (still pinned-excluded)',
    pilotEligible: false,
    eligibilityReason: 'not eligible unless group required-features become uniform: members may mix differing required-features' },
    (t, f) => `${themePrefix(t.name)}|${f.file_level_cfg || f.file_level_cfg_attr || '-'}`,
    { features: true, fileCfg: true });

  mk({ rung: 'looser-cfg', description: 'additionally allow non-uniform file-level cfg',
    pilotEligible: false,
    eligibilityReason: 'not eligible unless group file-level cfg becomes uniform: members may mix differing file-level cfgs' },
    (t) => themePrefix(t.name),
    { features: true, fileCfg: true });

  mk({ rung: 'allow-pinned', description: 'additionally allow externally-pinned names',
    pilotEligible: false,
    eligibilityReason: 'NOT pilot-eligible: members are externally name-pinned; listed for scale only' },
    (t) => themePrefix(t.name),
    { features: true, fileCfg: true, externalPins: true });

  return rungs;
}

/** Per-rung block predicate (strict blockers minus the relaxed ones). */
function relaxBlocked(row, relax) {
  const f = row.facts;
  const t = row.target;
  if (f.global_allocator) return true;
  if (f.first_config_wins.length > 0) return true;
  if (f.process_spawn) return true;
  if (f.abort_marker) return true;
  if (f.loom_only) return true;
  if (f.miri_only) return true;
  if (f.kani_gated) return true;
  if (!t.harness || !t.test_enabled) return true;
  if (!relax?.features && t.required_features.length > 0) return true;
  if (!relax?.fileCfg && (f.file_level_cfg !== null || f.file_level_cfg_attr !== null)) return true;
  if (!relax?.externalPins && row.externalPins && row.externalPins.count > 0) return true;
  return false;
}

/**
 * Assert the invariants tying groups to the mergeable set.
 * mergeableNames: sorted names; groups: buildGroups result.
 */
export function assertGroupInvariants(mergeableNames, groups) {
  const memberSet = [];
  for (const g of groups.values()) memberSet.push(...g.members);
  memberSet.sort((a, b) => a.localeCompare(b));
  if (memberSet.length !== mergeableNames.length) {
    throw new Error(`group members (${memberSet.length}) != mergeable set (${mergeableNames.length})`);
  }
  for (let i = 0; i < memberSet.length; i++) {
    if (memberSet[i] !== mergeableNames[i]) {
      throw new Error(`group member mismatch at ${i}: ${memberSet[i]} vs ${mergeableNames[i]}`);
    }
  }
  const savedLinks = [...groups.values()].reduce((s, g) => s + g.savedLinks, 0);
  if (savedLinks !== mergeableNames.length - groups.size) {
    throw new Error(`sum(saved_links)=${savedLinks} != mergeableCount - groupCount=${mergeableNames.length - groups.size}`);
  }
  return true;
}

/**
 * Pairwise intersection sizes of flag sets over all targets.
 * rows: [{name, flags:{flagName:boolean|array}}]; flagNames: ordered keys.
 * A row belongs to set F iff flags[F] is truthy / non-empty array.
 * Returns { sets: {flag: size}, matrix: {flag: {flag: n}} } with
 * |A∩B| <= min(|A|,|B|) asserted.
 */
export function intersections(rows, flagNames) {
  const sets = {};
  const members = {};
  for (const f of flagNames) {
    members[f] = rows.filter((r) => inSet(r.flags, f)).map((r) => r.name);
    sets[f] = members[f].length;
  }
  const matrix = {};
  for (const a of flagNames) {
    matrix[a] = {};
    for (const b of flagNames) {
      const inter = members[a].filter((n) => members[b].includes(n)).length;
      if (inter > Math.min(sets[a], sets[b])) {
        throw new Error(`intersection ${a}∩${b}=${inter} > min(${sets[a]},${sets[b]})`);
      }
      matrix[a][b] = inter;
    }
  }
  return { sets, matrix };
}

function inSet(flags, f) {
  const v = flags[f];
  return Array.isArray(v) ? v.length > 0 : v === true;
}
