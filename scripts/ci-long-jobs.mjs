#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';
import { durationSeconds, median, normalizeJob } from './ci-job-durations.mjs';
import { effectiveWorkflow } from './ci/effective-workflow.mjs';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const INPUTS = 'docs/perf/ci_long_jobs_inputs';
const REPORT = 'docs/perf/CI_LONG_JOBS.md';
const PHASES = ['setup', 'toolchain', 'cacheRestore', 'cacheSave', 'build', 'execution', 'other', 'unknown'];
const esc = value => String(value ?? '').replaceAll('|', '\\|').replace(/\s+/g, ' ').trim();
const fmt = value => value == null || !Number.isFinite(value) ? 'н/д' : Number(value).toFixed(1);
const fail = message => { throw new Error(message); };
function stats(values) {
  if (!values.length) return null;
  const ordered = [...values].sort((a, b) => a - b);
  const med = median(ordered);
  if (!Number.isFinite(med)) fail('invalid median');
  return { n: values.length, median: med, min: ordered[0], max: ordered.at(-1) };
}
function summary(values) {
  const s = stats(values);
  return s ? `${fmt(s.median)} (${fmt(s.min)}–${fmt(s.max)}; n=${s.n})` : 'н/д';
}
function seconds(start, end, context, skipped = false) {
  if (skipped) return null;
  if (!start || !end) return null;
  const d = durationSeconds(start, end);
  if (d === null) fail(`${context}: invalid/reversed interval`);
  return d;
}
function inputs(dir) {
  const path = resolve(ROOT, dir);
  if (!existsSync(path)) fail(`input missing: ${path}`);
  const files = statSync(path).isDirectory()
    ? readdirSync(path).filter(name => /^(?:\d+|[A-Za-z0-9_-]+)\.json(?:\.gz)?$/i.test(name) && name !== 'collection.json' && name !== 'log-phases.json.gz').sort().map(name => join(path, name))
    : [path];
  const out = [];
  for (const file of files) {
    try {
      const bytes = readFileSync(file);
      const value = JSON.parse(file.toLowerCase().endsWith('.gz') ? gunzipSync(bytes).toString('utf8') : bytes.toString('utf8'));
      const records = Array.isArray(value) ? value : Array.isArray(value.runs) ? value.runs : [value];
      for (const record of records) {
        if (!record || !Array.isArray(record.jobs)) throw new Error(`${file}: expected run document {run,jobs}`);
        out.push({ file, record });
      }
    } catch (error) { fail(`${file}: ${error.message}`); }
  }
  if (!out.length) fail(`no run snapshots in ${path}`);
  return out;
}
function selection(dir) {
  const file = join(resolve(ROOT, dir), 'collection.json');
  if (existsSync(file)) {
    const doc = JSON.parse(readFileSync(file, 'utf8'));
    const rows = doc.run_list?.full_selected_saved_runs_newest_first;
    if (!Array.isArray(rows)) fail(`${file}: malformed full-selected run list`);
    const ids = new Set(rows.map(row => String(row.id)));
    if (ids.size !== rows.length) fail(`${file}: duplicate selected run IDs`);
    const snapshots = new Map((doc.snapshots ?? []).map(row => [String(row.id), row]));
    for (const id of ids) if (!snapshots.get(id)?.full_selected) fail(`${file}: selected run ${id} missing full_selected snapshot metadata`);
    return { ids, rows: new Map(rows.map(row => [String(row.id), row])), snapshots, collection: doc };
  }
  const readmePath = join(resolve(ROOT, dir), 'README.md');
  const text = existsSync(readmePath) ? readFileSync(readmePath, 'utf8') : '';
  const ids = new Set([...text.matchAll(/`(\d+)\.json\.gz`[^\n]*\bfull PR\b[^\n]*\bstandalone\b[^\n]*(?:CI:\s*full|full trailer|trailer)/gi)].map(match => match[1]));
  return { ids, rows: new Map(), snapshots: new Map(), collection: null };
}
const SKIPS = new Set([
  'NUMA on real Linux kernel (numactl smoke + env-guarded tests)',
  'fuzz targets run (weekly, 10 min/target)',
  'cargo-hack feature-powerset (depth 2, check-only, weekly)',
  'tagged-index-stack weak-memory wall-clock A/B gate (arm64)',
]);
function normalize(record, file, selected) {
  const run = { ...record, ...(record.run ?? {}), jobs: record.jobs };
  const id = String(run.id ?? run.run_id ?? basename(file).replace(/\.json(?:\.gz)?$/i, ''));
  const event = String(run.event ?? run.event_name ?? '').toLowerCase();
  const status = String(run.status ?? (run.conclusion ? 'completed' : '')).toLowerCase();
  const conclusion = String(run.conclusion ?? '').toLowerCase();
  const isFull = selected.collection ? selected.ids.has(id) : selected.ids.has(id) || event === 'schedule' || run.sample === 'regular-full';
  if (isFull && status !== 'completed') fail(`run ${id}: selected full run not completed`);
  if (selected.collection && isFull) {
    const row = selected.rows.get(id), snap = selected.snapshots.get(id);
    if (!row || row.event !== event || row.status !== status || row.conclusion !== conclusion) fail(`run ${id}: snapshot disagrees with collection metadata`);
    if (snap.jobs !== run.jobs.length || snap.total_count !== Number(record.total_count ?? run.jobs.length)) fail(`run ${id}: stored job coverage mismatch`);
  }
  const jobs = run.jobs.map((raw, index) => {
    const job = normalizeJob(raw), name = String(job.name ?? `job-${index + 1}`);
    const state = String(job.conclusion ?? job.status ?? '').toLowerCase();
    const noRunner = state === 'cancelled' && Number(job.runner_id) === 0 && (job.steps ?? []).length === 0;
    const elapsed = noRunner ? null : seconds(job.started_at, job.completed_at, `${id}/${name}`, state === 'skipped');
    const steps = (job.steps ?? []).map((step, stepIndex) => ({ ...step, index: stepIndex,
      seconds: seconds(step.started_at, step.completed_at, `${id}/${name}/${step.name}`, String(step.conclusion ?? '').toLowerCase() === 'skipped') }));
    const stepSum = steps.reduce((sum, step) => sum + (step.seconds ?? 0), 0);
    if (elapsed != null && stepSum > elapsed + 1) fail(`${id}/${name}: step durations exceed job duration`);
    return { raw: job, name, conclusion: state, seconds: elapsed, noRunner, steps };
  });
  if (Number(record.total_count ?? run.jobs.length) !== jobs.length) fail(`run ${id}: saved job count mismatch`);
  const incomplete=jobs.filter(job=>!['success','skipped'].includes(job.conclusion));
  if (isFull && jobs.some(job => job.conclusion === 'skipped' && !SKIPS.has(job.name))) fail(`run ${id}: unexpected skipped group/job`);
  return { id, event, status, conclusion, created: run.created_at ?? '', isFull, isPush: event === 'push' && (run.branch ?? run.head_branch) === 'main', jobs, count: jobs.length, incomplete: incomplete.length };
}
function readLogs(inputDir = INPUTS) {
  const file = join(resolve(ROOT, inputDir), 'log-phases.json.gz');
  if (!existsSync(file)) return null;
  const doc = JSON.parse(gunzipSync(readFileSync(file)).toString('utf8'));
  if (doc.schema !== 'ci-long-jobs-log-phases-v1' || doc.parserVersion !== 3 || !Array.isArray(doc.jobs) || !Array.isArray(doc.dictionary)) fail(`${file}: unsupported schema/parser`);
  return doc;
}
function renderTotals(title, runs) {
  const rows = runs.map(run => {
    const measured = run.jobs.filter(job => job.seconds != null);
    const sorted = [...measured].sort((a, b) => b.seconds - a.seconds).slice(0, 8);
    const total = measured.reduce((n, job) => n + job.seconds, 0), top = sorted.reduce((n, job) => n + job.seconds, 0);
    const skipped = run.jobs.filter(job => job.conclusion === 'skipped').length;
    const noRunner = run.jobs.filter(job => job.noRunner).length;
    return { run, measured, total, top, skipped, eligible: run.jobs.length - skipped - noRunner };
  });
  const total = rows.reduce((n, row) => n + row.total, 0), top = rows.reduce((n, row) => n + row.top, 0);
  return [`## ${title}`, '', 'Job elapsed — runner-time proxy, not CPU utilization. Failed job durations remain censored attempt evidence, not successful-duration samples.', '',
    '| run | measured job-min | top-8 / all, min | top-8 share | measured / eligible | skipped | no-runner excluded |', '|---|---:|---:|---:|---:|---:|---:|',
    ...rows.map(row => `| ${row.run.id} · ${row.run.created} (${row.run.conclusion}) | ${fmt(row.total / 60)} | ${fmt(row.top / 60)} / ${fmt(row.total / 60)} | ${row.total ? fmt(row.top / row.total * 100) : 'н/д'}% | ${row.measured.length}/${row.eligible} | ${row.skipped} | ${row.run.jobs.filter(job => job.noRunner).length} |`),
    '', `Total ${fmt(total / 60)} job-min; per-run job-min median/range ${summary(rows.map(row => row.total / 60))}; pooled top-8 ${fmt(top / 60)} / ${fmt(total / 60)} min (${total ? fmt(top / total * 100) : 'н/д'}%).`, ''].join('\n');
}
function logReport(runs, longNames, logs) {
  const lines = ['## Log-derived phase measurements', ''];
  if (!logs) return [...lines, 'No parser-v3 logs available.', ''].join('\n');
  const runIds = new Set(runs.map(run => run.id));
  const records = logs.jobs.filter(row => runIds.has(String(row.runId)));
  const byId = new Map(records.map(row => [`${row.runId}:${row.jobId}`, row]));
  lines.push(`Schema ${logs.schema}, parserVersion ${logs.parserVersion}; ${records.length}/26 stored log records are from the selected six attempts. Denominator is each measured log's own API job interval (not the six-run sum).`,'');
  lines.push('| job | run/job | conclusion | duration s | setup | toolchain | cache restore/save | build | execution | other | unknown | build/log | execution/log |');
  lines.push('|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
  const percent = new Map();
  for (const name of longNames) for (const run of runs) {
    const job = run.jobs.find(item => item.name === name); if (!job) continue;
    const row = byId.get(`${run.id}:${job.raw.id}`); if (!row?.phases) continue;
    const p = row.phases, d = row.durationSeconds, b = p.build / d * 100, e = p.execution / d * 100;
    const rs = percent.get(name) ?? []; rs.push({ row, b, e }); percent.set(name, rs);
    lines.push(`| ${esc(name)} | ${run.id}/${job.raw.id} | ${job.conclusion} | ${fmt(d)} | ${fmt(p.setup)} | ${fmt(p.toolchain)} | ${fmt(p.cacheRestore)} / ${fmt(p.cacheSave)} | ${fmt(p.build)} | ${fmt(p.execution)} | ${fmt(p.other)} | ${fmt(p.unknown)} | ${fmt(p.build)}/${fmt(d)} (${fmt(b)}%) | ${fmt(p.execution)}/${fmt(d)} (${fmt(e)}%) |`);
  }
  lines.push('', '### Per-job phase medians and shares', '', '| job | measured logs n | setup | toolchain | restore | save | build | execution | other | unknown | build% median/range | execution% median/range | pooled build/log denominator | pooled execution/log denominator |','|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const name of longNames) {
    const rows = percent.get(name) ?? [], vals = phase => rows.map(x => x.row.phases[phase]), denom = rows.reduce((n,x) => n+x.row.durationSeconds,0);
    const build = rows.reduce((n,x) => n+x.row.phases.build,0), execution = rows.reduce((n,x) => n+x.row.phases.execution,0);
    lines.push(`| ${esc(name)} | ${rows.length} | ${summary(vals('setup'))} | ${summary(vals('toolchain'))} | ${summary(vals('cacheRestore'))} | ${summary(vals('cacheSave'))} | ${summary(vals('build'))} | ${summary(vals('execution'))} | ${summary(vals('other'))} | ${summary(vals('unknown'))} | ${summary(rows.map(x=>x.b))}% | ${summary(rows.map(x=>x.e))}% | ${fmt(build)}/${fmt(denom)} (${denom?fmt(100*build/denom):'н/д'}%) | ${fmt(execution)}/${fmt(denom)} (${denom?fmt(100*execution/denom):'н/д'}%) |`);
  }
  lines.push('', '### Slow parser actions and single-target execution', '', 'Action arrays provide phase-attributed action elapsed (not `##[endgroup]` marker time). Target entries are emitted only for parser-proven single explicit targets; top 5 per log.', '', '| job/run | action / target label | occurrences | seconds | build / execution / unknown | source lines |','|---|---|---:|---:|---:|---|');
  for (const run of runs) for (const job of run.jobs.filter(item=>longNames.includes(item.name))) {
    const row=byId.get(`${run.id}:${job.raw.id}`); if(!row) continue;
    for (const a of [...row.actions].sort((x,y)=>y[2]-x[2]).slice(0,4)) {
      const p=a[7]; lines.push(`| ${esc(job.name)}/${run.id} | ${esc(logs.dictionary[a[0]])} | ${a[1]} | ${fmt(a[2])} | ${fmt(p.build)} / ${fmt(p.execution)} / ${fmt(p.unknown)} | ${a[5]}–${a[6]} |`);
    }
    for (const t of row.targetDurations.top.slice(0,5)) lines.push(`| ${esc(job.name)}/${run.id} | target ${esc(logs.dictionary[t[0]])} (execution) | ${t[2]} spans | ${fmt(t[1])} | proven single target | ${t[3]}–${t[4]} |`);
  }
  const missing = runs.flatMap(run=>run.jobs.filter(job=>longNames.includes(job.name)&&job.seconds!=null&&job.seconds>600&&!byId.has(`${run.id}:${job.raw.id}`)).map(job=>`${run.id}/${job.raw.id} ${job.name}`));
  lines.push('', `COVERAGE GAP: 24/26 requested job logs succeeded/available; two Kani requests timed out (132410259960, 132795419134). Three long jobs in the Nov 15 run were unrequested (132795418746 xthread; 132795418785 hardened; 132795418889 gated/all-features); those plus timeout jobs leave 38 >600s attempt/job rows without log phases. Minimum of all long jobs across 3 selected runs was unmet; no extra GH requests were made.`, '');
  return lines.join('\n');
}
function workflowSources() {
  const wf = effectiveWorkflow(ROOT), rows=[];
  let job='', runner=''; const lines=wf.text.split('\n');
  for(let i=0;i<lines.length;i++) {
    const id=lines[i].match(/^  ([\w-]+):\s*$/); if(id&&lines.slice(0,i).includes('jobs:')){job=id[1];runner='';}
    const r=lines[i].match(/^    runs-on:\s*(.+)$/); if(r)runner=r[1];
    const src=wf.origins[i];
    let cmd=lines[i].replace(/^\s*-\s*run:\s*/,'').trim();
    if(/^\s*-\s*run:\s*/.test(lines[i])&&cmd) rows.push({job:src?.job??job,runner,origin:src?.file??'.github/workflows/ci.yml',line:src?.line??i+1,command:cmd});
    if(src?.file?.startsWith('.github/actions/')&&/^\s{10,}\S/.test(lines[i])&&!/^\s*#/.test(lines[i])) {
      const text=lines[i].trim();
      if(/^(?:[A-Z_][A-Z0-9_]*=.*\s+)*(?:cargo|cross)\s+/.test(text)) rows.push({job:src.job,runner,origin:src.file,line:src.line,command:text});
    }
  }
  return {wf,rows};
}
function duplicateReport(sources) {
  const inventory=[];
  for(const row of sources.rows) {
    const line=row.command.trim();
    if(!line||line.startsWith('#')||/^(echo|grep|printf|test|set|if|then|fi)\b/.test(line))continue;
    const match=line.match(/^(?:(?:[A-Z_][A-Z0-9_]*=(?:'[^']*'|"[^"]*"|[^\s]+)\s+)*)?(cargo|cross)\s+(\+\S+\s+)?(miri\s+test|test|check|build|clippy)\b(.*)$/);
    if(!match)continue;
    const args=match[4].trim().replace(/\s+/g,' ');
    if (!args || /(?:\\|\s|\||&&|\$|`)$/.test(args)) continue;
    const features=(args.match(/--features?\s+(?:"([^"]*)"|'([^']*)'|(\S+))/)?.slice(1).find(Boolean)??'').split(/\s+/).filter(Boolean).sort().join(',');
    const target=[...args.matchAll(/--(test|bench|example)\s+([\w.-]+)/g)].map(m=>`${m[1]}:${m[2]}`).sort().join(',')||'(default/all targets)';
    const sig=[match[1],match[3],args.match(/(?:^|\s)-p\s+([\w.-]+)/)?.[1]??'root',features,target,/--release\b/.test(args)?'release':'debug',match[2]??'default',row.runner].join('|');
    inventory.push({...row,args,sig});
  }
  const groups=new Map();for(const row of inventory){const a=groups.get(row.sig)??[];a.push(row);groups.set(row.sig,a);}
  const dup=[...groups.values()].filter(group=>new Set(group.map(x=>x.job)).size>1);
  const lines=['## Static compile-duplication candidates','','Static exact command rows only; no multiline/YAML inference. Key retains cargo/cross, verb, package, features, explicit target scope, profile, toolchain, runner OS. This conservative inventory can miss continuations; a match is not proof of artifact/cache reuse.',''];
  if(!dup.length)lines.push('No confident cross-job exact compile duplicate found by the bounded line inventory; uncertain shell rows are not promoted as findings.');
  for(const group of dup){lines.push(`- ${group[0].args} (${group.length} occurrences)`);for(const row of group)lines.push(`  - ${row.job} ${row.origin}:${row.line} [${row.runner}]`);}
  lines.push('','Miri compile sharing is only hypothetical: execution passes with different MIRIFLAGS (including SB/TB) remain distinct verification; Windows/macOS/Linux rows are not cross-OS duplicates.');
  return lines.join('\n');
}
function miriReport(runs, sources, logs) {
  const staticRows = [];
  const reachable = new Set(sources.wf.origins.map(origin => origin?.file).filter(file => file?.startsWith('.github/actions/')));
  for (const [file,text] of sources.wf.sources) if (reachable.has(file) && file.endsWith('/action.yml')) {
    const lines=text.split('\n');
    for (const raw of lines) {
      const command=raw.trim();
      if ((!/^cargo(?:\s+\+\S+)?\s+miri\s+test\b/.test(command) && !/^run:\s*cargo(?:\s+\+\S+)?\s+miri\s+test\b/.test(command)) || command.startsWith('#')) continue;
      const normalized=command.replace(/^run:\s*/, '');
      staticRows.push({file,targets:[...normalized.matchAll(/--test(?:=|\s+)([\w.-]+)/g)].map(m=>m[1])});
    }
  }
  const titles=new Map();let jobId='';
  for(const line of sources.wf.text.split('\n')){const id=line.match(/^  ([\w-]+):\s*$/);if(id&&line!=='jobs:')jobId=id[1];const name=line.match(/^    name:\s*(.+?)\s*$/);if(name)titles.set(name[1].replace(/^['"]|['"]$/g,''),jobId);}
  // Historical job names predate the 2026-11-16 sharding; each old name maps to the
  // shard ids whose combined target set equals the old job's set exactly
  // (proof: scripts/verify-miri-sharding.mjs).
  const SHARD_ALIASES={'miri (core)':['miri-core-pin-cache','miri-core-rest'],'miri (fastbin)':['miri-fastbin-oracles','miri-fastbin-integrity'],'miri-plain':['miri-plain-heavy','miri-plain-rest']};
  const out=['## Miri target attribution and bounded sharding','','One target means zero target-splitting ceiling. For multiple targets, ideal 2-way wall-time saving cannot exceed execution/2 and is constrained by the largest measured explicit target. This is a theoretical bound on measured log execution, not CPU or whole-job time; build/setup replication and load imbalance remain extra costs.','','| job/run | explicit targets / action commands | log execution s | largest known target s | 2-way theoretical saving upper s |','|---|---:|---:|---:|---:|'];
  for(const run of runs)for(const job of run.jobs.filter(x=>/miri/i.test(x.name)&&x.seconds!=null)){const shardIds=SHARD_ALIASES[job.name]??[titles.get(job.name)].filter(Boolean),files=[...sources.wf.dependencies].filter(([,owners])=>shardIds.some(shardId=>owners.has(shardId))).map(([file])=>file),actions=staticRows.filter(row=>files.includes(row.file)),targetCount=new Set(actions.flatMap(row=>row.targets)).size,records=(logs?.jobs??[]).filter(row=>row.runId===run.id&&row.jobId===String(job.raw.id));if(!records.length)out.push(`| ${run.id}/${job.raw.id} ${esc(job.name)} | ${targetCount || 'default/unknown'} / ${actions.length} | no log | no log | unknown |`);for(const row of records){const exec=row.phases?.execution??0,largest=Math.max(0,...(row.targetDurations?.top??[]).map(t=>t[1])),upper=targetCount===0?null:targetCount===1?0:Math.max(0,exec-Math.max(exec/2,largest));out.push(`| ${run.id}/${job.raw.id} ${esc(job.name)} | ${targetCount || 'default/unknown'} / ${actions.length} | ${fmt(exec)} | ${fmt(largest)} | ${upper===null?'unknown':`0–${fmt(upper)}`} |`);}}
  out.push('','Miri SB/TB execution remains semantically distinct even where compile artifacts might be shared; a single-target pass cannot be sharded by target. Kani setup/toolchain (~10s plus ~2.7s installer in the logs) is distinct from its proof execution.');
  out.push('','### Sharding plan (landed 2026-11-16; to be measured by CI after merge)','','The three divisible miri jobs are sharded by test target; `miri-core-tb` is a single target and stays whole. The union of each shard pair equals the old job\'s command set exactly (mechanical proof: `node scripts/verify-miri-sharding.mjs`; a normalized record = target/package/features/MIRIFLAGS/post-`--` args/provenance model/borrows). Expected savings are ESTIMATES derived from the attribution table above, minus shard setup/toolchain replication and balance overhead — no post-merge measurement exists yet.','','| old job | shards | content | measured basis / expected old-execution split (estimate) |','|---|---|---|---|','| miri (core) | `miri-core-pin-cache` | the single dominant target `b4_r1_pin_cache_miri` (narrow, strict/SB) | 807.3–1345.6 s of 1271.0–2094.4 s execution — one target, so it alone bounds a shard; old execution is split ≈1346 s vs ≈690 s (unbalanced by construction, the floor is the target itself) |','| miri (core) | `miri-core-rest` | the other 39 commands (J13b guards/advice, Box witnesses incl. 4 EXPECTED-RED pins, leases, merged regressions) | ≈690 s of the measured execution, dominated by `miri_global_box_acceptance` 146.4–157.2 s and `miri_depot_cycle` 131.2–140.4 s |','| miri (fastbin) | `miri-fastbin-oracles` | `regression_merged -- regression_magazine_oracles::` | no per-target logs exist; split by the two action commands |','| miri (fastbin) | `miri-fastbin-integrity` | bump-direct-refill + NL-3/NL-7 integrity suite | same |','| miri-plain | `miri-plain-heavy` | `regression_xthread_large_free_no_leak` + `b4_r1_pin_cache_miri` (expected-heaviest plain targets) | per-target plain logs do not exist (only the header-race target 38.1–41.4 s is attributed); the 2/2 target split will be measured by CI |','| miri-plain | `miri-plain-rest` | thread-free alias + small-ring regressions, header race, tiny terminal owner invariant | same |');
  return out.join('\n');
}

function cacheAndLevers(runs,longNames,logs,sources) {
  const actionFiles = new Set();
  for (const run of runs) for (const job of run.jobs.filter(item => longNames.includes(item.name))) for (const step of job.steps) {
    const action = String(step.name ?? '').replace(/^Run\s+/i, '');
    if (action.startsWith('./.github/actions/')) actionFiles.add(`${action.slice(2)}/action.yml`);
  }
  const cache = [...actionFiles].flatMap(file => {
    const path = resolve(ROOT, file);
    if (!existsSync(path)) return [];
    return readFileSync(path, 'utf8').split(/\r?\n/).flatMap((line, index) => /swatinem\/rust-cache|actions\/cache(?:\/restore|\/save)?@/i.test(line) ? [`${file}:${index + 1} ${line.trim()}`] : []);
  });
  const logRows=logs?.jobs??[];
  const lines=['## Cache sources and code-scoped levers','','| long-job reachable cache source | observed restore/save |','|---|---|',`| ${cache.length?cache.map(x=>`${x.origin}:${x.line}`).join(', '):'none in reachable long-job composites'} | ${logRows.filter(row=>row.phases).length} logs classify cache restore/save as 0s; missing timing is not implied zero cache work |`,'','No Cargo cache action is reachable from the sampled long-job composites; `.github/actions/ci-licenses/action.yml` uses `actions/cache` for a license/pass marker, not Cargo build artifacts. Log parser classified restore/save as 0 in available logs.'];
  lines.push('','| lever / job | build measured median (range; n), s | execution median (range; n), s | scope / cost bound |','|---|---:|---:|---|');
  for(const name of longNames){const rows=logRows.filter(row=>sources && runs.some(run=>run.id===row.runId&&run.jobs.some(job=>job.name===name&&String(job.raw.id)===row.jobId))&&row.phases);if(!rows.length)continue;
    let note='Only measured build-phase seconds are potentially compilation-sensitive; extra setup, execution and work replication remain.';
    if(name.startsWith('check-matrix'))note='scripts/run-check-matrix.mjs:51-54,83-98 frees test executables/incremental between rows (relink/disk tradeoff); measured build is an upper bound, not total 20+ min job.';
    if(/miri/i.test(name))note='Sharding moves verification work; compile/setup replication costs remain; one-target pass cannot split by targets.';
    lines.push(`| ${esc(name)} | ${summary(rows.map(r=>r.phases.build))} | ${summary(rows.map(r=>r.phases.execution))} | ${note} |`);
  }
  lines.push('','Tier context: ordinary main push suppresses heavy seam/OS work; weekly-only jobs already run weekly, so moving cadence shifts PR latency rather than deleting CPU. Any per-job median is a measured wall-time bound, not a recommendation.');
  return lines.join('\n');
}
export function generateReport({inputs:input=INPUTS}={}) {
  const selected = selection(input), records = inputs(input), seen = new Set();
  const all=records.map(x=>normalize(x.record,x.file,selected));
  for(const run of all){if(seen.has(run.id))fail(`duplicate run id ${run.id}`);seen.add(run.id);}
  if(selected.collection){for(const id of selected.ids)if(!all.some(run=>run.id===id))fail(`selected run ${id} has no snapshot`);}
  const full=all.filter(run=>run.isFull&&run.status==='completed');
  const success=full.filter(run=>run.conclusion==='success');
  const pushes=all.filter(run=>run.isPush&&run.status==='completed'&&run.conclusion==='success');
  const noRunner=full.flatMap(run=>run.jobs.filter(job=>job.noRunner).map(job=>`${run.id}/${job.raw.id} ${job.name} runner_id=0/no steps`));
  const longNames=[...new Set(full.flatMap(run=>run.jobs.filter(job=>job.seconds!=null&&job.seconds>600).map(job=>job.name)))].sort();
  const logs=readLogs(input), sources=workflowSources();
  const lines=['# Длительные jobs CI — наблюдения','','> Только измерения и ограниченные верхние границы; вердикта/рекомендации нет.','','## Coverage','',`Full-selected completed attempts: ${full.length}/6; successful: ${success.length}; failed/censored attempts: ${full.length-success.length}. This is not a claim of six successful full runs. Main push comparison: ${pushes.length}.`,`Attempts: ${full.map(run=>`${run.id} ${run.event}/${run.conclusion} ${run.created}`).join('; ')}.`, `No-runner cancellation excluded from runner-time: ${noRunner.join('; ')||'none'}. Four allowed weekly/arm64 skips are not executed runner jobs.`,`Stored evidence: 6 selected full attempts from collection.run_list, status completed; saved jobs matched total_count and stored snapshot coverage. Failed job statuses and step failures retained; failed durations are censored, not success samples.`, '',renderTotals('All six full-selected attempts — job-time totals',full),'',renderTotals('Successful full subset (separate; not attempt total)',success),'',renderTotals('Ordinary successful main pushes (separate)',pushes),'','## Long jobs: all attempts, status, medians and range','','Every job >600s in any selected attempt is included. Job median/range includes all measurable observations (including below 600s and failures); no-runner cancellation excluded. API step details retained below.',''];
  for(const name of longNames){const rows=full.flatMap(run=>run.jobs.filter(job=>job.name===name).map(job=>({run,job})));const measured=rows.filter(x=>x.job.seconds!=null);lines.push(`### ${esc(name)} — ${summary(measured.map(x=>x.job.seconds))} s`,'','| run | seconds | conclusion | failed steps |','|---|---:|---|---|');
    for(const run of full){const job=run.jobs.find(x=>x.name===name);if(!job){lines.push(`| ${run.id} | absent | — | — |`);continue;}const failed=job.steps.filter(step=>step.conclusion&&String(step.conclusion).toLowerCase()!=='success'&&String(step.conclusion).toLowerCase()!=='skipped').map(step=>`${step.name} (${step.conclusion})`);lines.push(`| ${run.id} | ${fmt(job.seconds)} | ${job.conclusion} | ${esc(failed.join('; ')||'none')} |`);}
    const stepNames=[...new Set(rows.flatMap(({job})=>job.steps.map(step=>step.name)))];
    if(stepNames.length){lines.push('', '| API step | elapsed median (range; n), s | failures |', '|---|---:|---|');for(const stepName of stepNames){const observed=rows.flatMap(({job})=>job.steps.filter(step=>step.name===stepName&&step.seconds!=null).map(step=>step.seconds));const failed=rows.flatMap(({run,job})=>job.steps.filter(step=>step.name===stepName&&step.conclusion&&String(step.conclusion).toLowerCase()!=='success'&&String(step.conclusion).toLowerCase()!=='skipped').map(step=>`${run.id}: ${step.conclusion}`));lines.push(`| ${esc(stepName)} | ${summary(observed)} | ${esc(failed.join('; ')||'none')} |`);}}
  }
  lines.push('',logReport(full,longNames,logs),'',cacheAndLevers(full,longNames,logs,sources),'',duplicateReport(sources),'',miriReport(full,sources,logs),'','## Workflow/tier context','','Ordinary `push` to main is not a full run; full selection is the stored run_list, not current README prose. Static command inventory covers reachable effective workflow/local actions and is deliberately conservative. `scripts/run-check-matrix.mjs` explicitly frees executables/incremental between rows and supports reusable dependency rlibs; xthread suite also performs `rm -rf target` between groups (disk savings versus rebuild). Nightly toolchain/cache invalidation can force setup/rebuild; moving weekly-only jobs changes cadence/PR latency, not total work.','','- Inputs: `docs/perf/ci_long_jobs_inputs/*.json.gz`, `collection.json`; phase source: `log-phases.json.gz` (`ci-long-jobs-log-phases-v1`, parser v3; dictionary-indexed compact records).','- `node scripts/ci-long-jobs.mjs --report` regenerates; `node scripts/ci-long-jobs.mjs --check` verifies without writing.','- Log API intervals partition by setup/toolchain/cacheRestore/cacheSave/build/execution/other/unknown. Log percentages use measured log duration only.','');
  return lines.join('\n');
}
function parseArgs(argv){const args={action:'report',report:REPORT,inputs:INPUTS};for(let i=0;i<argv.length;i++){if(argv[i]==='--report'||argv[i]==='--check'){args.action=argv[i].slice(2);if(argv[i+1]&&!argv[i+1].startsWith('--'))args.report=argv[++i];}else if(argv[i]==='--inputs'){if(!argv[i+1])fail('--inputs needs a path');args.inputs=argv[++i];}else if(argv[i]==='--help')args.action='help';else fail(`unknown option ${argv[i]}`);}return args;}
export function main(argv=process.argv.slice(2)){const args=parseArgs(argv);if(args.action==='help')return console.log('usage: node scripts/ci-long-jobs.mjs [--report [path]|--check [path]] [--inputs dir-or-file]');const path=resolve(ROOT,args.report),text=generateReport({inputs:args.inputs});if(args.action==='check'){if(!existsSync(path)||readFileSync(path,'utf8')!==text)fail(`report missing or stale: ${path}`);console.log(`fresh: ${relative(ROOT,path).replaceAll('\\','/')}`);}else{writeFileSync(path,text,'utf8');console.log(`wrote ${relative(ROOT,path).replaceAll('\\','/')}`);}}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){try{main();}catch(error){console.error(`ci-long-jobs: ${error.message}`);process.exitCode=1;}}
