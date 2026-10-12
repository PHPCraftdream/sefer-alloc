// PRF01 two-phase collecting infrastructure; diagnostics never enter judge rows.
import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, existsSync, lstatSync, statSync, realpathSync, readdirSync, rmSync } from 'node:fs';
import { resolve, dirname, relative, isAbsolute, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';
import { deflateSync } from 'node:zlib';
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const JUDGE = 'scripts/r19_prf01_sidecar_summary_gate.mjs';
const PREREG = 'docs/perf/R19_PRF01_SIDECAR_SUMMARY_PREREG.md';
const FREEZE = PREREG.replace('.md', '.sha256');
const PRIMARY = ['production','bench-internals','internals'];
const SYMBOL = '<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window';
const PRODUCER = 'r19_prf01_joined_producer_same_word';
const PUBLICATION = 'r19_prf01_publication';
const BENEFIT = ['multiseg_cold_256k','seg_cycle_decommit_256k','growth16_n','growth16_2n','growth16_4n'];
const HOT = ['small_churn_16b','churn_256b','churn_write_256b','aligned_churn_640b_a128'];
const MI = ['mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n','mimalloc_churn_256b','mimalloc_cold_alloc_free_256x16b','mimalloc_cold_alloc_free_256x16b_2n','mimalloc_cold_alloc_free_256x16b_4n','mimalloc_cold_alloc_only_256x16b','mimalloc_cold_alloc_only_256x16b_2n','mimalloc_cold_alloc_only_256x16b_4n','mimalloc_cold_alloc_free_256x64b','mimalloc_recycle_alloc_free_256x16b','mimalloc_recycle_alloc_free_256x64b','mimalloc_bootstrap_proxy'];
const INVENTORY = [...BENEFIT,...HOT,...MI,PUBLICATION,PRODUCER];
const COUNTERS = ['pending_words_visited','pending_words_exchanged','nonempty_cuts','discovery_records_reclaimed','routed_scans','summary_words_visited','summary_rmw','summary_load'];
const ROLES = { iai:['perf_gate_iai',PRIMARY,'bench',[]], oracle:['r19_prf01_oracle',PRIMARY,'example',['r19_sidecar_summary_oracle']], native:['r19_prf01_fanin_native',['production'],'example',[]], twin:['r19_prf01_fanin_native',PRIMARY,'example',[]], control:['perf_gate_iai',PRIMARY,'bench',['r19_prf01_producer_control']] };
const HARNESS = ['Cargo.toml','Cargo.lock','benches/perf_gate_iai.rs','benches/support/r19_prf01.rs','examples/r19_prf01_oracle.rs','examples/r19_prf01_fanin_native.rs',JUDGE,PREREG,FREEZE,'scripts/r19_prf01_sidecar_summary_run.mjs','.cargo/config.toml'];
const sha = b => createHash('sha256').update(b).digest('hex');
const json = x => JSON.stringify(x, null, 2) + '\n';
const now = () => new Date().toISOString();
const hex = x => assert.match(x, /^[a-f0-9]{64}$/);
const text = x => assert.ok(typeof x === 'string' && x.trim());
const int = x => assert.ok(Number.isSafeInteger(x) && x >= 0);
const feature = (got, expected) => assert.deepEqual([...got].sort(), [...expected].sort());
const quote = s => "'" + String(s).replaceAll("'", "'\\''") + "'";
const linuxPath = p => process.platform === 'win32' ? '/mnt/' + p[0].toLowerCase() + '/' + p.slice(3).replaceAll('\\', '/') : p;
export function publicText(s) {
  const preserved=String(s).replace(/(--(?:callgrind-out-file|log-file)=)("[^"\n]*"|'[^'\n]*'|[^\s]+)/g,(_,key,path)=>{
    const q=/^["']/.test(path)?path[0]:'',body=q?path.slice(1,-1):path,normalized=body.replaceAll(String.fromCharCode(92),'/'),suffix=normalized.split('/').slice(-2).join('/');return q+key+'x'.repeat(Math.max(0,normalized.length-suffix.length-1))+'/'+suffix+q;
  }).replace(/"\/(?:[^"\n]|\\")*"|'\/[^'\n]*'/g, token=>token[0]+'<private>'+token[0]);
  return preserved.replace(/[A-Za-z]:[\\/][^\s"'<>|)]+/g, '<private>')
    .replace(/(^|[\s"'(=:,\[{])\/(?!\/)[^\s"'<>|)]+/gm, '$1<private>');
}
function inside(parent, child) {
  const r = relative(parent, child);
  assert.ok(r && !isAbsolute(r) && r !== '..' && !r.startsWith('..' + sep), 'path escapes worktree');
}
function local(parent, name) {
  assert.ok(typeof name === 'string' && !isAbsolute(name) && !name.split(/[\\/]/).includes('..'));
  const p = resolve(parent, name); inside(parent, p);
  let cursor = p;
  while (cursor !== parent) {
    if (existsSync(cursor)) assert.ok(!lstatSync(cursor).isSymbolicLink(), 'symlink rejected');
    cursor = dirname(cursor);
  }
  return p;
}
function put(dir, name, bytes, generated = false) {
  const p = local(dir, name); mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, generated ? publicText(bytes) : bytes, { flag: 'wx' });
  return { path: name, sha256: sha(readFileSync(p)) };
}
const putJson = (dir, name, value) => put(dir, name, json(value));
function get(dir, ref) {
  hex(ref.sha256); const bytes = readFileSync(local(dir, ref.path));
  assert.equal(sha(bytes), ref.sha256, 'retained byte/hash drift'); return bytes;
}
const getJson = (dir, ref) => JSON.parse(get(dir, ref));
function git(cwd, args) {
  assert.ok(['rev-parse', 'diff', 'ls-files', 'rev-list', 'merge-base', 'show'].includes(args[0]));
  const r = spawnSync('git', ['--no-optional-locks', ...args], { cwd, encoding: null,
    env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' }, timeout: 60000, maxBuffer: 128 * 1024 * 1024 });
  assert.equal(r.status, 0, 'read-only git failed'); return r.stdout;
}
function ancestry(a, b, graph) {
  assert.match(a, /^[a-f0-9]{40}$/); assert.match(b, /^[a-f0-9]{40}$/);
  const parents = new Map(graph.trim().split('\n').filter(Boolean).map(l => {
    const [id, ...ps] = l.split(/\s+/); [id, ...ps].forEach(x => assert.match(x, /^[a-f0-9]{40}$/)); return [id, ps];
  }));
  const queue = [b], seen = new Set();
  while (queue.length) { const id = queue.pop(); if (id === a) return;
    if (!seen.has(id)) { seen.add(id); queue.push(...(parents.get(id) ?? [])); } }
  throw Error('A not ancestor of B');
}
function snapshot(cwd,{allowDirty=false}={}) {
  const baseHead = git(cwd, ['rev-parse', 'HEAD']).toString().trim();
  const tree = git(cwd, ['rev-parse', 'HEAD^{tree}']).toString().trim();
  const patch = git(cwd, ['diff', '--binary', 'HEAD', '--']);
  const tracked = git(cwd, ['ls-files', '-z']).toString().split('\0').filter(Boolean).sort();
  const untracked = git(cwd, ['ls-files', '--others', '--exclude-standard', '-z']).toString().split('\0').filter(Boolean).sort();
  const ignored = git(cwd, ['ls-files', '--others', '--ignored', '--exclude-standard', '-z']).toString().split('\0').filter(Boolean)
    .filter(p => /^(src|crates|benches|examples|\.cargo)\//.test(p) && !/(^|\/)target\//.test(p)
      || /^(Cargo\.(toml|lock)|build\.rs|rust-toolchain(\.toml)?)$/.test(p)).sort();
  if(!allowDirty){assert.equal(patch.length, 0, 'clean committed arm required: tracked patch present');
  assert.equal(untracked.length, 0, 'clean committed arm required: untracked inputs present');}
  const paths = [...new Set([...tracked, ...untracked, ...ignored])].sort();
  const bytes = Object.fromEntries(paths.map(p => [p, readFileSync(local(cwd, p))]));
  const manifest = paths.map(p => [p, bytes[p].length, sha(bytes[p])]);
  const u = untracked.map(p => [p, bytes[p].length, sha(bytes[p])]);
  const source = { baseHead, tree, patchSha256: sha(patch), untrackedSha256: sha(JSON.stringify(u)),
    identitySha256: sha(JSON.stringify([baseHead, tree, sha(patch), manifest])), capturedAt: now(),
    manifestSha256: sha(JSON.stringify(manifest)), ignoredInputs: ignored,
    harness: Object.fromEntries(HARNESS.map(p => { assert.ok(bytes[p], 'required harness absent'); return [p, sha(bytes[p])]; })) };
  return { source, patch, manifest, untracked: u, bytes };
}
function sourceStable(expected, actual) {
  for (const k of ['baseHead', 'tree', 'patchSha256', 'untrackedSha256', 'identitySha256', 'manifestSha256', 'ignoredInputs', 'harness'])
    assert.deepEqual(actual[k], expected[k], 'source drift');
}
function retainSource(dir, arm, s) {
  const blobs = {};
  for (const [p, b] of Object.entries(s.bytes)) {
    const name = 'source/' + sha(b) + '.bin';
    blobs[p] = { path: name, sha256: sha(b), byteLength: b.length };
    if (!existsSync(local(dir, name))) put(dir, name, b); // Exact sources are never publicText-filtered.
  }
  return { ...s.source, patch: put(dir, `${arm}.patch.bin`, s.patch),
    manifest: putJson(dir, `${arm}.manifest.json`, s.manifest),
    untracked: putJson(dir, `${arm}.untracked.json`, s.untracked), blobs };
}
function frozen(preregBytes, judgeBytes, companion) {
  const hashes = [{ path: PREREG, sha256: sha(preregBytes) }, { path: JUDGE, sha256: sha(judgeBytes) }];
  assert.equal(companion.toString(), hashes.map(x => `${x.sha256}  ${x.path}\n`).join(''), 'frozen hash drift'); return hashes;
}
export const dirLabel = arm => (arm === 'C0' ? 'C' : arm);
export function assertEqualShape(a, b) { assert.equal(a.length, b.length, 'arm paths must have identical length (alignment confounder, A3)'); }
export function makeTransport({ executable, prefix = [], path = linuxPath } = {}) {
  executable ??= process.platform === 'win32' ? 'wsl.exe' : 'bash';
  prefix = prefix.length ? prefix : process.platform === 'win32' ? ['bash', '-lc'] : ['-lc'];
  return {
    path,
    async hostLoad() {
      if (process.platform !== 'win32') {
        const r=spawnSync('python3',['-c','import time; a=list(map(int,open("/proc/stat").readline().split()[1:])); time.sleep(0.1); b=list(map(int,open("/proc/stat").readline().split()[1:])); d=[y-x for x,y in zip(a,b)]; print(100*(sum(d)-d[3]-d[4])/sum(d))'],{encoding:'utf8',timeout:20000});
        assert.equal(r.status,0,'host CPU unavailable'); const value=Number(r.stdout.trim()); assert.ok(Number.isFinite(value)&&value>=0&&value<=100,'host CPU unavailable'); return value;
      }
      const r=spawnSync('powershell.exe',['-NoProfile','-NonInteractive','-Command','(Get-CimInstance Win32_Processor | Measure-Object -Property LoadPercentage -Average).Average'],{encoding:'utf8',timeout:20000});
      assert.equal(r.status,0,'host CPU unavailable'); text(r.stdout); const value=Number(r.stdout.trim()); assert.ok(Number.isFinite(value)&&value>=0&&value<=100,'host CPU unavailable'); return value;
    },
    async execute(request) {
      const script = `cd ${quote(path(request.cwd))} && ${request.script}`;
      return new Promise((accept, reject) => {
        const child = spawn(executable, [...prefix, script], { cwd: request.cwd, shell: false,
          detached: process.platform !== 'win32', env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } });
        let stdout = '', stderr = '', timedOut = false;
        const timer = setTimeout(() => {
          timedOut = true;
          if (process.platform === 'win32') spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F']);
          else { try { process.kill(-child.pid, 'SIGKILL'); } catch {} }
        }, request.timeoutMs ?? 600000);
        child.stdout.on('data', b => { stdout += b; }); child.stderr.on('data', b => { stderr += b; });
        child.on('error', e => { clearTimeout(timer); reject(e); });
        child.on('close', (code, signal) => { clearTimeout(timer); accept({ code, signal, timedOut, stdout, stderr }); });
      });
    },
  };
}
async function execute(ctx, request) {
  const id = request.id ?? randomUUID(); const startedAt = now();
  const result = await ctx.transport.execute({ ...request, id });
  const rawDir=local(ROOT,'target/prf01-private-diagnostics');mkdirSync(rawDir,{recursive:true});const rawHashes={stdout:sha(result.stdout),stderr:sha(result.stderr)};for(const [stream,bytes] of [['stdout',result.stdout],['stderr',result.stderr]])writeFileSync(local(rawDir,id+'.'+stream+'.log'),bytes,{flag:'wx'});
  const publicStdout=/-disassembly$/.test(request.type)?[...result.stdout.matchAll(/^[0-9a-f]+ <(.+)>:\n[\s\S]*?(?=^[0-9a-f]+ <|$(?![\s\S]))/gm)].filter(m=>m[1]===SYMBOL||/std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure/.test(m[1])).map(m=>m[0]).join('\n'):result.stdout;
  const output = put(ctx.out, `logs/${id}.stdout.log`, publicStdout, true);
  const errors = put(ctx.out, `logs/${id}.stderr.log`, result.stderr, true);
  const receipt = { id, type: request.type, arm: request.arm, role: request.role, name: request.name ?? null,
    startedAt, endedAt: now(), command: publicText(request.script), code: result.code, signal: result.signal ?? null,
    timedOut: result.timedOut ?? false, rawHashes, stdout: output, stderr: errors };
  const ref = putJson(ctx.out, `logs/${id}.process.json`, receipt);
  assert.equal(result.code, request.expectedReject || request.retainFailure ? result.code : 0, 'child failed; exclusive diagnostic logs retained');
  assert.ok(!result.timedOut);
  if (request.expectedReject) {
    assert.notEqual(result.code, 0, 'negative workload unexpectedly accepted');
    assert.match(result.stderr, request.expectedReject, 'negative must fail its activation assertion');
  }
  return { ...result, receipt, ref };
}
function fixtureRepos(base) {
  mkdirSync(base, { recursive: true }); const arms = {}; let parent = null;
  const objects = new Map();
  const object = (type, bytes) => {
    const body = Buffer.concat([Buffer.from(`${type} ${bytes.length}\0`), bytes]);
    const id = createHash('sha1').update(body).digest('hex'); objects.set(id, deflateSync(body)); return id;
  };
  for (const arm of ['A', 'B']) {
    const cwd = resolve(base, arm); mkdirSync(cwd);
    const files = { '.gitignore': Buffer.from('target/\n'), 'src/fixture.rs': Buffer.from(`// synthetic ${arm}\n`) };
    for (const p of HARNESS) files[p] = readFileSync(local(ROOT, p));
    const entries = Object.keys(files).sort().map(path => {
      const destination = local(cwd, path); mkdirSync(dirname(destination), { recursive: true });
      writeFileSync(destination, files[path], { flag: 'wx' });
      return { path, oid: object('blob', files[path]), size: files[path].length };
    });
    function tree(prefix = '') {
      const children = new Map();
      for (const e of entries.filter(e => e.path.startsWith(prefix))) {
        const suffix = e.path.slice(prefix.length), name = suffix.split('/')[0];
        children.set(name, suffix.includes('/') ? { directory: true } : e);
      }
      const body = [...children].sort(([a, x], [b, y]) => (a + (x.directory ? '/' : '')).localeCompare(b + (y.directory ? '/' : ''), 'en'))
        .map(([name, child]) => Buffer.concat([Buffer.from(`${child.directory ? '40000' : '100644'} ${name}\0`),
          Buffer.from(child.directory ? tree(prefix + name + '/') : child.oid, 'hex')]));
      return object('tree', Buffer.concat(body));
    }
    const rootTree = tree();
    const commit = object('commit', Buffer.from(`tree ${rootTree}\n${parent ? `parent ${parent}\n` : ''}author Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\n${arm}\n`));
    const gd = resolve(cwd, '.git'); mkdirSync(resolve(gd, 'refs', 'heads'), { recursive: true });
    for (const [id, bytes] of objects) { mkdirSync(resolve(gd, 'objects', id.slice(0, 2)), { recursive: true });
      writeFileSync(resolve(gd, 'objects', id.slice(0, 2), id.slice(2)), bytes, { flag: 'wx' }); }
    writeFileSync(resolve(gd, 'HEAD'), 'ref: refs/heads/fixture\n', { flag: 'wx' });
    writeFileSync(resolve(gd, 'refs', 'heads', 'fixture'), commit + '\n', { flag: 'wx' });
    const header = Buffer.alloc(12); header.write('DIRC'); header.writeUInt32BE(2, 4); header.writeUInt32BE(entries.length, 8);
    const indexEntries = entries.map(e => {
      const name = Buffer.from(e.path); const size = Math.ceil((62 + name.length + 1) / 8) * 8;
      const bytes = Buffer.alloc(size); bytes.writeUInt32BE(0o100644, 24); bytes.writeUInt32BE(e.size, 36);
      Buffer.from(e.oid, 'hex').copy(bytes, 40); bytes.writeUInt16BE(Math.min(name.length, 0xfff), 60); name.copy(bytes, 62); return bytes;
    });
    const body = Buffer.concat([header, ...indexEntries]);
    writeFileSync(resolve(gd, 'index'), Buffer.concat([body, createHash('sha1').update(body).digest()]), { flag: 'wx' });
    arms[arm] = cwd; parent = commit;
  }
  const bGit = resolve(arms.B, '.git'), aGit = resolve(arms.A, '.git');
  // A's repository needs no B objects; B contains the complete ancestor graph.
  assert.ok(existsSync(aGit) && existsSync(bGit));
  ancestry(git(arms.A, ['rev-parse', 'HEAD']).toString().trim(), git(arms.B, ['rev-parse', 'HEAD']).toString().trim(), git(arms.B, ['rev-list', '--parents', 'HEAD']).toString());
  for (const cwd of Object.values(arms)) snapshot(cwd);
  return arms;
}

function schedule() {
  const jobs=[];
  for(const n of [1,2,4,8])for(let repetition=1;repetition<=9;repetition++)
    (repetition%2?['A','AA','B']:['B','AA','A']).forEach((arm,position)=>jobs.push({n,arm,repetition,position,index:jobs.length}));
  return jobs;
}
function inventory(bytes) {
  const s=bytes.toString(), group=/library_benchmark_group!\(\s*name\s*=\s*perf_gate;\s*benchmarks\s*=([\s\S]*?)\);/.exec(s);
  assert.ok(group,'group missing'); const all=group[1].split(',').map(x=>x.trim()).filter(Boolean);
  assert.equal(new Set(all).size,all.length);
  for(const name of INVENTORY)assert.equal(all.filter(n=>n===name).length,1,'frozen cell missing');
  const constants=Object.fromEntries([...s.matchAll(/const\s+(\w+):\s*usize\s*=\s*([^;]+);/g)].map(m=>[m[1],m[2]]));
  const evaluate=expression=>{const tokens=expression.trim().split(/\s*\*\s*/);return tokens.reduce((value,token)=>{if(/^\d[\d_]*$/.test(token))return value*Number(token.replaceAll('_',''));assert.ok(constants[token],'unknown workload constant');return value*evaluate(constants[token]);},1);};
  const cells=INVENTORY.map(name=>{
    const match=new RegExp('fn '+name+'\\([^)]*\\)[^{]*\\{').exec(s);assert.ok(match,'body missing');let end=match.index+match[0].length,depth=1;while(depth){assert.ok(end<s.length);depth+=(s[end]==='{')-(s[end]==='}');end++;}const body=s.slice(match.index,end);
    const claimPath=/claim_leaked_heap|dbg_claim_lease/.test(body)?'lease':/SeferAlloc::new|prf01_support/.test(body)?'global':/MiMalloc/.test(body)?'mimalloc':null;assert.ok(claimPath,'unknown claim path');
    const layout=/Layout::from_size_align\(([^,]+),\s*([^)]*)\)/.exec(body),loop=/0\.\.(CHURN_OPS(?:_2N)?)/.exec(body),batch=/\[\*mut u8;\s*(COLD_BATCH(?:_2N|_4N)?)\]/.exec(body);
    const observedGrowth=/R19Prf01::growth16\((\d[\d_]*)\)/.exec(body);
    const multiseg=/\[\*mut u8;\s*((?:MULTISEG|SEGCYCLE)_BATCH)\]/.exec(body);
    const round=/0\.\.(SEGCYCLE_ROUNDS|2)/.exec(body);
    const declaredWork=loop?evaluate(loop[1]):batch?evaluate(batch[1])*(name.includes('recycle')?2:1):observedGrowth?evaluate(observedGrowth[1]):multiseg?evaluate(multiseg[1])*evaluate(round[1]):name==='mimalloc_bootstrap_proxy'?1:null;
    return {name,index:all.indexOf(name),claimPath,body,bodySha256:sha(body),declaredWorkCount:declaredWork,blockBytes:layout?evaluate(layout[1]):observedGrowth?16:null,alignment:layout?evaluate(layout[2]):observedGrowth?8:null};
  });
  const cellChecks=Object.entries(HOT_DECL);for(const [name,pair] of cellChecks){const c=cells.find(c=>c.name===name);assert.deepEqual([c.declaredWorkCount,c.blockBytes,c.alignment],pair,'hot source declarations mismatch');assert.equal(c.claimPath,'global');}
  return {all,selected:INVENTORY,sourceSha256:sha(bytes),constants,cells};
}
function rejectEnvironment(env) {
  for(const [k,v] of Object.entries(env))if(v)assert.ok(!/^(RUSTFLAGS|RUSTC|RUSTC_WORKSPACE_WRAPPER|CARGO_.*|IAI_CALLGRIND_.*)$/.test(k),'inherited compiler/cargo/iai override');
}
async function environment(ctx,arm) {
  const script='set -e; if env | grep -Eq '+quote('^(RUSTFLAGS|RUSTC|RUSTC_WORKSPACE_WRAPPER|CARGO_.*|IAI_CALLGRIND_.*)=')+'; then echo effective-environment-override >&2; exit 3; fi; rustc -Vv; cargo -V; uname -srm; python3 -c '+quote('rows=open("/proc/cpuinfo").read().splitlines(); print("\\n".join(sorted(set(l for l in rows if l.split(":")[0].strip() in ["vendor_id","model name","cpu family","model","stepping","siblings","cpu cores","flags"]))))')+'; for f in "$HOME/.cargo/config.toml" "$HOME/.cargo/config"; do if [ -f "$f" ]; then sha256sum "$f"; fi; done; valgrind --version; iai-callgrind-runner --version 2>&1 || true';
  const r=await execute(ctx,{cwd:ctx.arms[arm],script,type:'environment',arm});
  assert.match(r.stdout,/0\.14\.2/); assert.match(r.stdout,/valgrind-/); const target=/^host: (.+)$/m.exec(r.stdout)?.[1]; text(target);
  return {target,raw:r.ref,description:publicText(r.stdout)};
}
async function telemetry(ctx,label) {
  const r=await execute(ctx,{cwd:ctx.arms.A,script:'cat /proc/loadavg; nproc',type:'telemetry',arm:'A',name:label});
  const hostCpuLoadPercent=await ctx.transport.hostLoad();assert.ok(Number.isFinite(hostCpuLoadPercent)&&hostCpuLoadPercent>=0&&hostCpuLoadPercent<=100,'host CPU unavailable');
  const ref=putJson(ctx.out,`telemetry/${ctx.telemetry.length}.json`,{label,at:now(),wsl:r.stdout,hostCpuLoadPercent,process:r.ref}); ctx.telemetry.push(ref);
}
async function build(ctx,arm,role) {
  const sourceArm=arm==='C0'?'A':arm,cwd=ctx.arms[sourceArm], [name,features,kind,cfg]=ROLES[role];
  sourceStable(ctx.snapshots[sourceArm].source,snapshot(cwd,{allowDirty:ctx.diagnostic===true}).source);
  const target=ctx.diagnostic?`target/prf01-prepared-${ctx.prepareId}/${role==='iai'?'p':'c'}`:`target/prf01-${dirLabel(arm)}-${role}`; assert.ok(!existsSync(local(cwd,target)),'target exists');
  const script=`RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/${target}" RUSTFLAGS=${quote(cfg.map(c=>'--cfg '+c).join(' '))} cargo ${kind==='bench'?'bench --no-run':'build --release'} --locked --message-format=json-render-diagnostics --${kind} ${quote(name)} --features ${quote(features.join(' '))}`;
  const r=await execute(ctx,{cwd,script,type:'build',arm,role});
  const artifacts=r.stdout.split(/\r?\n/).filter(l=>l.startsWith('{')).map(l=>JSON.parse(l)).filter(a=>a.reason==='compiler-artifact'&&a.target.name===name&&a.executable);
  assert.equal(artifacts.length,1); const prefix=ctx.transport.path(cwd)+'/',executable=artifacts[0].executable; assert.ok(executable.startsWith(prefix)); const binaryPath=executable.slice(prefix.length); assert.ok(binaryPath.startsWith(target+'/'));
  sourceStable(ctx.snapshots[sourceArm].source,snapshot(cwd,{allowDirty:ctx.diagnostic===true}).source);
  const binary=put(ctx.out,`binaries/${arm}-${role}.bin`,readFileSync(local(cwd,binaryPath)));
  const graph=await execute(ctx,{cwd,script:`RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/${target}" RUSTFLAGS=${quote(cfg.map(c=>'--cfg '+c).join(' '))} cargo tree --locked -e features --features ${quote(features.join(' '))}`,type:'feature-graph',arm,role});
  const sizes=await execute(ctx,{cwd,retainFailure:ctx.diagnostic===true,script:'set -e; size '+quote('./'+binaryPath)+'; nm -C --defined-only '+quote('./'+binaryPath),type:'symbols',arm,role});
  if(role==='iai'&&!ctx.diagnostic)assert.ok(sizes.stdout.split(/\r?\n/).some(l=>l.endsWith(' '+SYMBOL)),'exact producer symbol absent');
  const roleSources=await execute(ctx,{cwd,script:'python3 -c '+quote('import pathlib,os,json; root=pathlib.Path(os.environ.get("CARGO_HOME",str(pathlib.Path.home()/".cargo")))/"registry"/"src"; matches=list(root.glob("*/iai-callgrind-runner-0.14.2/src/runner/callgrind/args.rs")); assert len(matches)==1; p=matches[0]; print(json.dumps({"version":"0.14.2","argsSource":p.read_text(),"libBenchSource":(p.parent.parent/"lib_bench.rs").read_text()}))'),type:'runner-source',arm,role});
  const runnerSource=JSON.parse(roleSources.stdout);assert.equal(runnerSource.version,'0.14.2');assert.match(runnerSource.argsSource,/separate_threads: true/);assert.match(runnerSource.libBenchSource,/EntryPoint::Custom/);
  const buildCtx={...ctx,builds:undefined};if(role==='iai'||role==='control'){const provisional={arm,sourceArm,role,binaryPath,binary,binarySha256:binary.sha256,stat:binaryStat(local(cwd,binaryPath))};var buildBoundary=await boundaryEvidence(buildCtx,provisional,{retainFailure:true,type:'build-boundary'});}
  return {arm,sourceArm,role,buildBoundaryReceipt:buildBoundary?.receipt??null,name,features,cfg,binaryPath,binary,binarySha256:binary.sha256,sourceIdentitySha256:ctx.sources[sourceArm].identitySha256,environmentSha256:ctx.env.sha256,stat:binaryStat(local(cwd,binaryPath)),symbolPresent:sizes.code===0&&sizes.stdout.split(/\r?\n/).some(l=>l.endsWith(' '+SYMBOL)),process:r.ref,graph:graph.ref,symbols:sizes.ref,runnerSource:roleSources.ref,capturedAt:now(),profile:'release',locked:true,jobs:3};
}
function stable(ctx,b) { assert.deepEqual(binaryStat(local(ctx.arms[b.sourceArm],b.binaryPath)),b.stat,'binary rebuilt/stat drift');assert.equal(sha(readFileSync(local(ctx.arms[b.sourceArm],b.binaryPath))),b.binarySha256,'binary drift'); get(ctx.captureOut??ctx.out,b.binary); }
function walk(dir) { return readdirSync(dir).flatMap(n=>{const p=local(dir,n),s=lstatSync(p);assert.ok(!s.isSymbolicLink());return s.isDirectory()?walk(p):[p];}); }

function naturalBig(token) { assert.match(token,/^(?:\d+|0x[0-9a-fA-F]+)$/,'malformed unsigned integer');return BigInt(token); }
function safeNumber(value) { assert.ok(value>=0n&&value<=BigInt(Number.MAX_SAFE_INTEGER),'Ir exceeds safe judge integer');return Number(value); }
function positionToken(token,previous) {
  if(token==='*'){assert.notEqual(previous,null,'relative position without base');return previous;}
  if(/^[+-]/.test(token)){assert.notEqual(previous,null,'relative position without base');const value=previous+(token[0]==='-'?-1n:1n)*naturalBig(token.slice(1));assert.ok(value>=0n,'negative position');return value;}
  return naturalBig(token);
}
function profileFunctions(s) {
  const events=/^events: (.+)$/m.exec(s)?.[1].trim().split(/\s+/);assert.ok(events);const ir=events.indexOf('Ir');assert.ok(ir>=0);assert.equal(new Set(events).size,events.length);
  const positions=/^positions: (.+)$/m.exec(s)?.[1].trim().split(/\s+/)??['line'];assert.ok(positions.every(x=>['line','instr'].includes(x)));assert.equal(new Set(positions).size,positions.length);
  const ids=new Map(),functions=new Map();let current=null,callee=null,pendingCall=false,previous=Array(positions.length).fill(null);
  function symbol(value){const m=/^(?:\((\d+)\)\s*)?(.*)$/.exec(value);if(m[2])return m[2];assert.ok(ids.has(m[1]),'unknown compressed function ID');return ids.get(m[1]);}
  function fn(name){if(!functions.has(name))functions.set(name,{selfIr:0n,edges:new Map(),edgeCalls:new Map(),calls:0n});return functions.get(name);}
  for(const line of s.split(/\r?\n/)){const m=/^(?:c?fn)=\((\d+)\)\s+(.+)$/.exec(line);if(m){if(ids.has(m[1]))assert.equal(ids.get(m[1]),m[2],'function ID redefined');ids.set(m[1],m[2]);}}
  for(const line of s.split(/\r?\n/)) {
    if(line.startsWith('fn=')){assert.ok(!pendingCall,'missing call cost');current=symbol(line.slice(3));fn(current);callee=null;previous=Array(positions.length).fill(null);}
    else if(line.startsWith('cfn=')){assert.ok(!pendingCall,'missing call cost');callee=symbol(line.slice(4));fn(callee);}
    else if(line.startsWith('calls=')){assert.ok(current&&callee&&!pendingCall,'invalid calls record');const fields=line.slice(6).trim().split(/\s+/);assert.equal(fields.length,positions.length+1,'malformed calls positions');const counts=naturalBig(fields[0]);positions.forEach((_,i)=>positionToken(fields[i+1],previous[i]));pendingCall=true;fn(current).calls+=counts;fn(current).edgeCalls.set(callee,(fn(current).edgeCalls.get(callee)??0n)+counts);}
    else if(/^[*+\-\d]/.test(line)) {
      assert.ok(current,'cost without function');const fields=line.trim().split(/\s+/);assert.ok(fields.length>=positions.length,'missing cost positions');assert.ok(fields.length<=positions.length+events.length,'extra event columns');previous=positions.map((_,i)=>positionToken(fields[i],previous[i]));const costs=fields.slice(positions.length).map(naturalBig),count=costs[ir]??0n,f=fn(current);
      if(pendingCall){f.edges.set(callee,(f.edges.get(callee)??0n)+count);pendingCall=false;}else f.selfIr+=count;
    }else if(line&&!/^(?:#|[A-Za-z][A-Za-z_]*(?:=|:))/.test(line))throw Error('malformed Callgrind record');
  }
  assert.ok(!pendingCall,'missing final call cost');return functions;
}
function argvTokens(value) {
  const result=[];let token='',q=null,active=false;
  for(let i=0;i<value.length;i++){const c=value[i];if(q){if(c===q)q=null;else if(c==='\\'&&q==='"'&&['"','\\'].includes(value[i+1]))token+=value[++i];else token+=c;active=true;}else if(c==='"'||c==="'"){q=c;active=true;}else if(/\s/.test(c)){if(active){result.push(token);token='';active=false;}}else{token+=c;active=true;}}
  assert.equal(q,null,'unclosed argv quote');if(active)result.push(token);return result;
}
function dispatch(argv,cell) { assert.ok(cell,'inventory dispatcher linkage missing');assert.equal(argv.indexOf('--iai-run'),1,'measured dispatcher missing');assert.deepEqual(argv.slice(2,5),['perf_gate',String(cell.index),'0'],'measured dispatcher index mismatch');assert.ok(argv.length===5||argv.length===6&&argv[5]==='perf_gate_iai::perf_gate::'+cell.name,'unexpected dispatcher module argument'); }
const PRODUCER_SETTINGS=['--tool=callgrind','--error-exitcode=0','--trace-children=yes','--fair-sched=try','--I1=32768,8,64','--D1=32768,8,64','--LL=8388608,16,64','--cache-sim=yes','--compress-strings=no','--compress-pos=no','--dump-line=yes','--dump-instr=no','--combine-dumps=no','--separate-threads=yes'];
function generatedArguments(debug,log,name,producer,cell) {
  assert.equal(cell?.name,name,'inventory dispatcher linkage missing');const commands=[...log.matchAll(/^==\d+==\s+Command:\s*(.+)$/gm)].map(m=>m[1]);assert.equal(new Set(commands).size,1,'ambiguous measured argv');assert.ok(commands.length);dispatch(argvTokens(commands[0]),cell);
  const records=[...debug.matchAll(/callgrind: Arguments: (.+)$/gm)].map(m=>m[1]);const candidates=records.filter(x=>x.includes('--callgrind-out-file=')&&x.includes('/'+name+'/')&&(!producer||x.includes('--toggle-collect='+SYMBOL)));
  assert.equal(candidates.length,1,'one actual runner argument record required per cell');const tool=candidates[0];assert.ok(tool.includes('--separate-threads=yes'),'separate_threads not captured');assert.ok(tool.includes('--collect-atstart=no')||!tool.includes('--collect-atstart=')&&tool.includes('--toggle-collect='),'collect-atstart not captured');
  // Debug joins unescaped argv; split at option boundaries to retain spaces in the exact symbol.
  const toolVector=tool.trim().split(/\s+(?=--[A-Za-z])/).map(x=>x.replace(/^"(.*)"$/,'$1').replace(/^(--[^=]+=)"(.*)"$/,'$1$2').replace(/^(--[^=]+=)'(.*)'$/,'$1$2'));assert.ok(toolVector.every(x=>x.startsWith('--')),'malformed tool argv');
  if(producer){const collect=toolVector.filter(x=>x.startsWith('--collect-atstart='));assert.ok(collect.length<=1&&collect.every(x=>x==='--collect-atstart=no'),'conflicting collect-atstart flags');for(const setting of PRODUCER_SETTINGS){assert.equal(toolVector.filter(x=>x.split('=')[0]===setting.split('=')[0]).length,1,'missing/duplicate producer setting '+setting);assert.ok(toolVector.includes(setting),'wrong producer setting '+setting);}assert.equal(toolVector.filter(x=>x.startsWith('--toggle-collect=')).length,1);assert.ok(toolVector.includes('--toggle-collect='+SYMBOL));assert.ok(!tool.includes('*::__iai_callgrind_wrapper_mod::*'),'default toggle forbidden');}
  return {toolArguments:tool,measuredArguments:commands[0],generatedArguments:'valgrind '+tool+' '+commands[0],toolVector,dispatchIndex:cell.index,method:'actual runner Debug Arguments plus actual Valgrind Command; numeric indexes bound to source inventory and binary identity'};
}
export function parseProfiles(profiles,log,{producer=false,control=false,name,debug='',cell,expectedExecutable}={}) {
  const pids=[...new Set([...log.matchAll(/^==(\d+)==/gm)].map(m=>Number(m[1])))];assert.equal(pids.length,1,'actual valgrind PID missing/ambiguous');assert.ok(pids[0]>0);
  const args=generatedArguments(debug,log,name,producer,cell),measured=argvTokens(args.measuredArguments);if(expectedExecutable)assert.equal(measured[0],expectedExecutable,'measured binary mismatch');
  let totalIr=0n,childIr=0n,boundaryExecutions=0n,executedFree=false,exactExecuted=false;const threads=new Set(),profileIdentities=[];
  for(const s of profiles){
    const header=k=>{const matches=[...s.matchAll(new RegExp('^'+k+': (.+)$','gm'))];assert.equal(matches.length,1,'ambiguous/missing '+k+' header');return matches[0][1];};
    const pid=safeNumber(naturalBig(header('pid'))),tid=header('thread'),part=safeNumber(naturalBig(header('part')));assert.equal(pid,pids[0]);assert.equal(part,1,'ambiguous/incomplete profile parts/dumps');assert.match(tid,/^[1-9]\d*$/);assert.ok(!threads.has(tid),'duplicate thread profile');threads.add(tid);assert.deepEqual(argvTokens(header('cmd')),measured,'profile/log argv mismatch');
    const dumps=[...s.matchAll(/^desc: (?:Trigger|Dump): (.+)$/gm)].map(m=>m[1]);assert.ok(dumps.length<=1,'ambiguous dump identity');if(dumps.length)assert.match(dumps[0],/termination|program termination/i,'nonfinal dump');profileIdentities.push({pid,thread:Number(tid),part,dump:dumps[0]??'single final part (no trigger header)'});
    const events=header('events').trim().split(/\s+/),index=events.indexOf('Ir');assert.ok(index>=0);const totals=[...s.matchAll(/^totals: (.+)$/gm)];assert.equal(totals.length,1,'missing/ambiguous final totals');const vals=totals[0][1].trim().split(/\s+/).map(naturalBig);assert.ok(vals.length>=1&&vals.length<=events.length,'malformed totals');while(vals.length<events.length)vals.push(0n);totalIr+=vals[index];
    const functions=profileFunctions(s);assert.equal([...functions.values()].reduce((n,f)=>n+f.selfIr,0n),vals[index],'self costs/final totals mismatch');
    if(producer&&tid!=='1'){
      const exact=functions.get(SYMBOL);assert.ok(exact,'exact producer boundary absent');const inclusive=exact.selfIr+[...exact.edges.values()].reduce((a,b)=>a+b,0n);assert.ok(inclusive>0n,'exact producer boundary empty');assert.equal(inclusive,vals[index],'exact inclusive/child total mismatch');childIr+=vals[index];exactExecuted=true;
      let incoming=0n;for(const [caller,f] of functions){const count=f.edgeCalls.get(SYMBOL)??0n;if(count>0n){assert.match(caller,/std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure/,'boundary incoming caller not child context');incoming+=count;}}
      assert.equal(incoming,1n,'boundary executions unproven or not exactly one (incoming edges required)');boundaryExecutions+=incoming;
      if(control){assert.equal(exact.edgeCalls.size,0,'control outgoing work');assert.equal(exact.edges.size,0,'control outgoing work');assert.ok(inclusive>=1n&&inclusive<=16n,'boundary overhead outside 1..16');}
      else {const queue=[SYMBOL],seen=new Set();while(queue.length){const f=queue.pop();if(seen.has(f))continue;seen.add(f);for(const [target,cost] of functions.get(f)?.edges??[])if(cost>0n){if(/(?:dealloc|publish_small|SidecarBitmap.*publish)/.test(target))executedFree=true;queue.push(target);}}assert.ok(executedFree,'exact producer/free call graph not executed');}
    }
  }
  const ids=[...threads].map(Number).sort((a,b)=>a-b);assert.ok(ids.includes(1),'main thread missing');assert.deepEqual(ids,Array.from({length:ids.length},(_,i)=>i+1),'thread profile gaps');assert.ok(childIr<=totalIr,'childIr exceeds totalIr');
  if(producer){assert.equal(ids.length,2,'child profile absent or ambiguous child set');assert.equal(boundaryExecutions,1n,'boundary executions not one');assert.ok(exactExecuted);}
  return {pid:pids[0],totalIr:safeNumber(totalIr),childIr:safeNumber(childIr),boundaryExecutions:safeNumber(boundaryExecutions),profileIdentities,threads:[...threads],executedFree,exactExecuted,...args};
}
function pairCoverage(primary,control) {
  int(control.childIr);assert.ok(control.childIr>=1&&control.childIr<=16,'boundary overhead outside 1..16');assert.equal(primary.boundaryExecutions,1);assert.equal(control.boundaryExecutions,1);assert.ok(primary.childIr<=primary.totalIr&&control.childIr<=control.totalIr,'childIr exceeds totalIr');const workloadIr=primary.childIr-control.childIr;assert.ok(workloadIr>0,'empty primary workload remainder');assert.ok(primary.totalIr>control.totalIr,'non-distinguishing control');return {boundaryOverheadIr:control.childIr,workloadIr,controlWorkloadIr:0};
}
function bindMeasuredBinary(log,profiles,b,cwd) {
  const commands=[...log.matchAll(/^==\d+==\s+Command:\s*(.+)$/gm)].map(m=>argvTokens(m[1]));assert.ok(commands.length);const valid=new Set(['./'+b.binaryPath,cwd+'/'+b.binaryPath]);for(const cmd of commands)assert.ok(valid.has(cmd[0]),'actual measured executable not captured binary');for(const s of profiles){const cmd=argvTokens(/^cmd: (.+)$/m.exec(s)?.[1]??'');assert.deepEqual(cmd,commands[0],'raw profile/log argv mismatch');}return publicText(commands[0][0]);
}
function machineBoundary(nm,assembly,control) {
  const definitions=nm.split(/\r?\n/).filter(l=>l.endsWith(' '+SYMBOL));assert.equal(definitions.length,1,'exact boundary symbol missing/ambiguous');
  const blocks=[...assembly.matchAll(/^([0-9a-f]+) <(.+)>:\n([\s\S]*?)(?=^[0-9a-f]+ <|$(?![\s\S]))/gm)].map(m=>({address:m[1],name:m[2],body:m[3].trimEnd()+'\n'}));const exact=blocks.filter(b=>b.name===SYMBOL);assert.equal(exact.length,1,'exact disassembly definition absent');
  const callers=blocks.filter(b=>/std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure/.test(b.name)&&b.body.split('\n').some(l=>/\bcallq?\s/.test(l)&&l.includes('<'+SYMBOL+'>')));assert.ok(callers.length>0,'child closure disassembly caller absent');
  const decoded=exact[0].body.split('\n').map(l=>/^\s*[0-9a-f]+:\s+([a-z][a-z0-9]*)\b(.*)/.exec(l)).filter(Boolean);assert.ok(decoded.length);const retAt=decoded.findIndex(m=>/^retq?$/.test(m[1])),instructions=retAt<0?decoded:decoded.slice(0,retAt+1);if(control){assert.ok(retAt>=0,'control boundary has no ret');assert.ok(decoded.slice(retAt+1).every(m=>/^(?:int3|nop[lw]?)$/.test(m[1])),'control disassembly has code after ret (only int3/nop alignment padding allowed)');assert.ok(instructions.every(m=>/^(?:retq?|endbr64|nop[lw]?)$/.test(m[1])||/^(?:pushq?|popq?)$/.test(m[1])&&/^\s*%rbp\s*$/.test(m[2])||/^movq?$/.test(m[1])&&/^\s*%rsp,\s*%rbp\s*$/.test(m[2])||/^leaveq?$/.test(m[1])),'control disassembly contains branch/call/work');assert.ok(instructions.some(m=>/^ret/.test(m[1])));assert.ok(instructions.every(m=>!/[<>]/.test(m[2])),'control disassembly helper/publication');}
  return {symbol:SYMBOL,boundaryBody:exact[0].body,childCallers:callers,controlEmpty:control,method:'exact definition + direct call in std::thread/remote_free closure context; conservative x86 entry/exit whitelist, independent review required'};
}
async function boundaryEvidence(ctx,b,{retainFailure=false,type='launch-boundary'}={}) {
  stable(ctx,b);const cwd=ctx.arms[b.sourceArm],nm=await execute(ctx,{cwd,arm:b.arm,role:b.role,type:type+'-nm',retainFailure:true,script:'nm -C --defined-only '+quote('./'+b.binaryPath)}),disasm=await execute(ctx,{cwd,arm:b.arm,role:b.role,type:type+'-disassembly',retainFailure:true,script:'objdump -Cd --no-show-raw-insn '+quote('./'+b.binaryPath)});let verification=null,reason=null;
  try{assert.equal(nm.code,0);assert.equal(disasm.code,0);verification=machineBoundary(nm.stdout,disasm.stdout,b.role==='control');}catch(error){reason=error.message;}
  const evidence={binarySha256:b.binarySha256,stat:b.stat,arm:b.arm,role:b.role,nm:nm.ref,disassembly:disasm.ref,verification,reason};const ref=putJson(ctx.out,'boundary/'+randomUUID()+'.json',evidence);stable(ctx,b);if(!retainFailure)assert.ok(verification,'boundary prelaunch verification failed: '+reason);return {...evidence,receipt:ref};
}
function checkBoundaryEvidence(dir,ref,b) {
  const e=getJson(dir,ref);assert.equal(e.binarySha256,b.binarySha256);assert.deepEqual(e.stat,b.stat);assert.equal(e.arm,b.arm);assert.equal(e.role,b.role);const nm=getJson(dir,e.nm),dis=getJson(dir,e.disassembly);for(const p of [nm,dis]){assert.equal(p.code,0);assert.equal(p.arm,b.arm);assert.equal(p.role,b.role);}assert.deepEqual(machineBoundary(get(dir,nm.stdout).toString(),get(dir,dis.stdout).toString(),b.role==='control'),e.verification);return e;
}
async function iai(ctx,label,role='iai') {
  const arm=label==='C0'?'C0':label.startsWith('B')?'B':'A',b=ctx.builds[`${arm}-${role}`];stable(ctx,b);
  const id=randomUUID(),cwd=ctx.arms[b.sourceArm],home=`target/prf01-${dirLabel(arm)}-${role}-obs-${id}`;
  assert.ok(!existsSync(local(cwd,home)));
  const boundary=await boundaryEvidence(ctx,b);
  const r=await execute(ctx,{cwd,script:'IAI_CALLGRIND_LOG=debug IAI_CALLGRIND_COLOR=never '+[`./${b.binaryPath}`,'--home',ctx.transport.path(local(cwd,home)),'--save-summary=json'].map(quote).join(' '),type:'iai',arm:label,role,id});
  const files=walk(local(cwd,home)),rows=[];
  for(const name of role==='control'?[PRODUCER]:INVENTORY) {
    const own=files.filter(p=>relative(local(cwd,home),dirname(p)).split(/[\\/]/).some(x=>x===name||x.startsWith(name+'.')));
    const allProfiles=own.filter(p=>/\.out(?:[.#-]+\d+)*$/.test(p)),profiles=allProfiles.filter(p=>statSync(p).size>0);assert.ok(profiles.length,'profile missing');
    const logs=own.filter(p=>/\.log(?:[.#-]+\d+)*$/.test(p));assert.ok(logs.length,'retained valgrind log missing');
    const originals=[...allProfiles,...logs].map(p=>readFileSync(p));const rawDir=local(ROOT,'target/prf01-private-diagnostics');const originalHashes=originals.map((bytes,i)=>{writeFileSync(local(rawDir,id+'-'+name+'-'+i+'.bin'),bytes,{flag:'wx'});return sha(bytes);});
    const rawLog=logs.map(p=>readFileSync(p,'utf8')).join('\n');bindMeasuredBinary(rawLog,profiles.map(p=>readFileSync(p,'utf8')),b,ctx.transport.path(cwd));
    const profileBytes=profiles.map(p=>publicText(readFileSync(p,'utf8'))),log=publicText(logs.map(p=>readFileSync(p,'utf8')).join('\n'));
    const parsed=parseProfiles(profileBytes,log,{producer:name===PRODUCER,control:role==='control',name,cell:ctx.inv.cells.find(c=>c.name===name),debug:publicText(r.stderr)});
    const block=r.stdout.split(/(?=^\w+::)/m).find(x=>new RegExp('^(?:\\w+::)+'+name+'\\b').test(x));assert.ok(block,'actual iai name missing');
    assert.equal(Number(/^\s*Instructions:\s*([\d,]+)/m.exec(block)?.[1].replaceAll(',','')),parsed.totalIr,'iai/profile sum mismatch');
    const runId=`${id}-${name}`,profilesRef=profileBytes.map((s,i)=>put(ctx.out,`raw/${runId}-${i}.callgrind`,s)), logRef=put(ctx.out,`raw/${runId}.valgrind`,log);
    const e={id:runId,arm:label,role,name,originalHashes,parsedExecutable:argvTokens(parsed.measuredArguments)[0],boundaryReceipt:boundary.receipt,...parsed,binarySha256:b.binarySha256,process:r.ref,startedAt:r.receipt.startedAt,profiles:profilesRef,log:logRef}; rows.push(e);ctx.processes.push(e);
  }
  const profileReceipt=putJson(ctx.out,`raw/${id}-profile-inventory.json`,rows.map(r=>({id:r.id,name:r.name,profiles:r.profiles,log:r.log,totalIr:r.totalIr,childIr:r.childIr,profileIdentities:r.profileIdentities,boundaryExecutions:r.boundaryExecutions,generatedArguments:r.generatedArguments})));
  for(const row of rows)row.profileReceipt=profileReceipt;
  for(const [i,p] of files.entries())put(ctx.out,`audit/${id}/${i}.bin`,publicText(readFileSync(p,'utf8')));stable(ctx,b);return rows;
}
async function observation(ctx,arm,role,args) {
  const b=ctx.builds[`${arm}-${role}`];stable(ctx,b);const id=randomUUID();
  const r=await execute(ctx,{cwd:ctx.arms[b.sourceArm],script:[`./${b.binaryPath}`,...args,id].map(quote).join(' '),type:'observation',arm,role,args,id});
  const lines=r.stdout.trim().split(/\r?\n/);assert.equal(lines.length,1,'one process one raw row');const row=JSON.parse(lines[0]);assert.equal(row.config.runId,id);assert.ok(row.config.pid>0);
  const raw=put(ctx.out,`raw/${id}.json`,lines[0]+'\n'); const e={id,arm,role,args,raw,process:r.ref,binarySha256:b.binarySha256,startedAt:r.receipt.startedAt,row};ctx.processes.push(e);stable(ctx,b);return e;
}
const ownSource=s=>Object.fromEntries(['baseHead','patchSha256','untrackedSha256','identitySha256','capturedAt'].map(k=>[k,s[k]]));
function configValid(c) { assert.deepEqual(c.requested,c.resolved,'wrong resolved config');assert.equal(c.configConflictsDelta,0);assert.equal(c.processBoundary,'isolated-process');text(c.runId);assert.ok(c.pid>0);assert.ok(c.resolved.segmentCount>0); }
function mode(name) { return name===PUBLICATION?'publication':name===PRODUCER?'producer':name; }
const HOT_DECL = {small_churn_16b:[64,16,8],churn_256b:[64,256,8],churn_write_256b:[64,256,8],aligned_churn_640b_a128:[64,640,128]};
function geometryMethods(bytes) {
  const source=bytes.toString(),match=/pub fn segment_count_method\(\) -> &'static str\s*\{\s*"([^"]+)"/.exec(source);assert.ok(match,'approved geometry method missing');
  return {paired:match[1],probe:'process-wide live Small+Primordial directory census for configuration probe only; not paired workload geometry'};
}
function geometryValid(row,methods=geometryMethods(readFileSync(local(ROOT,'benches/support/r19_prf01.rs')))) {
  const m=row.mode,work=row.config.resolved.workCount,bytes=row.config.resolved.blockBytes;
  const declaration={multiseg_cold_256k:[68,258752],seg_cycle_decommit_256k:[204,258752],growth16_n:[32768,16],growth16_2n:[65536,16],growth16_4n:[131072,16],publication:[68,258752],producer:[4096,16],'probe-lease':[1,16],'probe-global':[1,16]};assert.deepEqual([work,bytes],declaration[m],'oracle workload declaration mismatch');
  const points=m==='publication'?[["publication","prepared-live-batch-before-handoff",0,34],["publication","discovery-live-batch-before-free",0,34],["publication","discovery-live-batch-before-free",1,34]]:m==='producer'?[[m,'selected-live-batch-before-handoff',0,4096]]:m.startsWith('growth16_')?[['growth16','final-live-batch-before-free',0,work]]:BENEFIT.slice(0,2).includes(m)?Array.from({length:m===BENEFIT[0]?2:6},(_,i)=>[m,'live-batch-before-free',i,34]):[];
  assert.ok(Array.isArray(row.checkpoints));assert.equal(row.checkpoints.length,points.length,'checkpoint coverage');text(row.segmentCountMethod);assert.equal(row.segmentCountMethod,points.length?methods.paired:methods.probe,'exact geometry method mismatch');
  const union=new Set();for(const [i,c] of row.checkpoints.entries()){const [name,phase,round,blocks]=points[i];assert.deepEqual([c.name,c.phase,c.point,c.round,c.roundIndexBase,c.blockCount,c.blockBytes],[name,phase,phase,round,0,blocks,bytes],'checkpoint bounds');assert.equal(c.method,row.segmentCountMethod);assert.ok(Array.isArray(c.incarnations)&&c.incarnations.length>0);assert.equal(new Set(c.incarnations).size,c.incarnations.length);const counted=new Set();let count=0;for(const p of c.perIncarnationCounts){int(p.incarnation);assert.ok(p.incarnation>0);int(p.blockCount);assert.ok(p.blockCount>0);assert.ok(!counted.has(p.incarnation),'duplicate incarnation');counted.add(p.incarnation);count+=p.blockCount;int(count);}assert.equal(count,c.blockCount,'checkpoint block sum');assert.deepEqual([...counted].sort((a,b)=>a-b),[...c.incarnations].sort((a,b)=>a-b));c.incarnations.forEach(x=>union.add(x));}
  assert.deepEqual([...union].sort((a,b)=>a-b),row.segmentIncarnations,'checkpoint union');if(points.length)assert.equal(union.size,row.config.resolved.segmentCount,'union cardinality');
  const declared=points.map(([name,phase,round,blockCount])=>({name,phase,point:phase,round,roundIndexBase:0,blockCount,blockBytes:bytes}));assert.deepEqual(row.declaredWorkloadBounds,{workCount:work,blockBytes:bytes,alignment:8,checkpointCount:points.length,checkpoints:declared,producerCandidateLimit:m==='producer'?8192:null,producerCandidatesIncluded:false},'declared geometry bounds');
}
function binaryStat(path) {const s=statSync(path,{bigint:true});return Object.fromEntries(['size','mtimeNs','ctimeNs','ino','dev'].map(k=>[k,String(s[k])]));}
function captureLink(ctx) {return {sources:Object.fromEntries(Object.entries(ctx.sources).map(([a,s])=>[a,s.identitySha256])),binaries:Object.fromEntries(Object.entries(ctx.builds).map(([k,b])=>[k,{sha256:b.binarySha256,stat:b.stat,sourceArm:b.sourceArm,role:b.role,features:b.features,cfg:b.cfg}])),environmentSha256:ctx.env.sha256,inventorySha256:ctx.invRef.sha256};}
function captureBounds(dir,capture) {
  assert.equal(capture.version,1);assert.deepEqual(Object.keys(capture.sources).sort(),['A','B']);const keys=['C0-iai','C0-control',...['A','B'].flatMap(a=>Object.keys(ROLES).map(r=>a+'-'+r))];assert.deepEqual(Object.keys(capture.builds).sort(),keys.sort(),'capture role inventory');
  assert.deepEqual(capture.linkage,captureLink({sources:capture.sources,builds:capture.builds,env:capture.environment,invRef:capture.inventory}),'capture linkage drift');get(dir,capture.environment);get(dir,capture.inventory);
  for(const [key,b] of Object.entries(capture.builds)){assert.equal(key,b.arm+'-'+b.role,'capture role key mismatch');assert.equal(b.sourceArm,b.arm==='B'?'B':'A','capture source arm mismatch');assert.equal(b.name,ROLES[b.role][0]);feature(b.features,ROLES[b.role][1]);assert.deepEqual(b.cfg,ROLES[b.role][3]);assert.equal(b.sourceIdentitySha256,capture.sources[b.sourceArm].identitySha256);assert.equal(b.environmentSha256,capture.environment.sha256);assert.equal(b.binarySha256,sha(get(dir,b.binary)));assert.ok(b.binaryPath.startsWith(`target/prf01-${dirLabel(b.arm)}-${b.role}/`));const p=getJson(dir,b.process);assert.equal(p.type,'build');assert.equal(p.arm,b.arm);assert.equal(p.role,b.role);assert.equal(p.code,0);assert.ok(Date.parse(capture.sources[b.sourceArm].capturedAt)<Date.parse(p.startedAt)&&Date.parse(p.endedAt)<=Date.parse(b.capturedAt)&&Date.parse(b.capturedAt)<=Date.parse(capture.capturedAt),'capture build chronology');}
}
function identityReview(dir,capture,review,at=now()) {
  assert.equal(review.status,'reviewed');text(review.reviewer);assert.equal(review.captureSha256,capture.sha256,'identity capture reference mismatch');assert.deepEqual(review.linkage,capture.linkage,'identity role digest mismatch');assert.ok(Date.parse(capture.capturedAt)<Date.parse(review.reviewedAt)&&Date.parse(review.reviewedAt)<Date.parse(at),'identity review chronology');assert.equal(review.authorization,'full-group-and-native');
  return review;
}
function collectionEquivalent(primary,control,source) {
  assert.deepEqual(primary.toolVector.map(x=>x.replace(/^(--(?:callgrind-out-file|log-file)=).*/,'$1<output>')),control.toolVector.map(x=>x.replace(/^(--(?:callgrind-out-file|log-file)=).*/,'$1<output>')),'collection argv changed');assert.deepEqual(argvTokens(primary.measuredArguments).slice(1),argvTokens(control.measuredArguments).slice(1),'full dispatcher argv changed');assert.equal(primary.dispatchIndex,control.dispatchIndex,'dispatcher changed');
  assert.match(source,/fn producer_free_window[^\n]*\{\s*\/\/ Amendment B:[\s\S]*?std::hint::black_box\(\(\)\);\s*#\[cfg\(r19_prf01_producer_control\)\]/,'frozen anchor changed');
  assert.match(source,/#\[cfg\(r19_prf01_producer_control\)\]\s*let _ = \(allocator, addresses, layout\);\s*#\[cfg\(not\(r19_prf01_producer_control\)\)\]\s*for &address in addresses/,'control source shape changed');assert.match(source,/\.spawn\(\|\| \{\s*Self::producer_free_window\(allocator, &addresses, layout\);\s*\}\)\s*\.join\(\)/,'parent collection setup changed');
}
async function pipeline(ctx) {
  assert.ok(!existsSync(ctx.out),'no overwrite');mkdirSync(ctx.out,{recursive:true});ctx.processes=[];ctx.builds={};ctx.telemetry=[];ctx.snapshots={};ctx.sources={};
  for(const arm of ['A','B']) {ctx.arms[arm]=realpathSync(ctx.arms[arm]);ctx.snapshots[arm]=snapshot(ctx.arms[arm]);ctx.sources[arm]=retainSource(ctx.out,arm,ctx.snapshots[arm]);}
  assertEqualShape(ctx.arms.A,ctx.arms.B);assert.notEqual(ctx.sources.A.identitySha256,ctx.sources.B.identitySha256);assert.deepEqual(ctx.sources.A.harness,ctx.sources.B.harness);
  for(const p of HARNESS)assert.equal(ctx.sources.A.harness[p],sha(readFileSync(local(ROOT,p))),'runner/harness mismatch');
  const graph=git(ctx.arms.B,['rev-list','--parents','HEAD']).toString();ancestry(ctx.sources.A.baseHead,ctx.sources.B.baseHead,graph);const ancestryRef=put(ctx.out,'ancestry.log',graph);
  const hashes=frozen(ctx.snapshots.A.bytes[PREREG],ctx.snapshots.A.bytes[JUDGE],ctx.snapshots.A.bytes[FREEZE]); const inv=inventory(ctx.snapshots.A.bytes['benches/perf_gate_iai.rs']);const invRef=putJson(ctx.out,'inventory-DRAFT.json',inv);ctx.inv=inv;ctx.invRef=invRef;
  const environments={};for(const a of ['A','B'])environments[a]=await environment(ctx,a);assert.equal(environments.A.description,environments.B.description);
  ctx.env=putJson(ctx.out,'environment.json',{...environments.A,arms:environments,allocator:'SeferAlloc',profile:'release/bench inherited',defaultFeatures:true,flags:'only role cfg',workUnits:'Ir and remote free nanoseconds'});
  for(const arm of ['A','B'])for(const role of Object.keys(ROLES))ctx.builds[`${arm}-${role}`]=await build(ctx,arm,role);
  ctx.builds['C0-iai']=await build(ctx,'C0','iai');ctx.builds['C0-control']=await build(ctx,'C0','control');assert.notEqual(ctx.builds['C0-iai'].binarySha256,ctx.builds['A-iai'].binarySha256,'same C0hash');
  ctx.capturedAt=now();for(const arm of ['A','B'])sourceStable(ctx.snapshots[arm].source,snapshot(ctx.arms[arm]).source);for(const b of Object.values(ctx.builds))stable(ctx,b);
  const capture={version:1,capturedAt:ctx.capturedAt,hashes,sources:ctx.sources,environment:ctx.env,ancestry:ancestryRef,inventory:invRef,builds:ctx.builds,linkage:captureLink(ctx)};
  const captureRef=putJson(ctx.out,'capture.json',capture);
  // Machine paths are private transport state, retained only under current target.
  ctx.statePath=local(ROOT,'target/prf01-capture-'+captureRef.sha256+'.json');mkdirSync(dirname(ctx.statePath),{recursive:true});writeFileSync(ctx.statePath,json({arms:ctx.arms,out:ctx.out}),{flag:'wx'});
  return {verdict:'CAPTURED',capture:captureRef,linkage:capture.linkage,capturedAt:ctx.capturedAt,measurements:0};
}
async function runCaptured(ctx) {
  const captureBytes=readFileSync(local(ctx.out,'capture.json')),capture=JSON.parse(captureBytes),captureSha=sha(captureBytes);
  const privateState=JSON.parse(readFileSync(local(ROOT,'target/prf01-capture-'+captureSha+'.json')));assert.equal(privateState.out,ctx.out);if(ctx.arms)for(const a of ['A','B'])if(ctx.arms[a])assert.equal(realpathSync(ctx.arms[a]),realpathSync(privateState.arms[a]),'requested arm path mismatch');ctx.arms=privateState.arms;ctx.sources=capture.sources;ctx.builds=capture.builds;ctx.env=capture.environment;ctx.invRef=capture.inventory;ctx.inv=getJson(ctx.out,capture.inventory);ctx.capturedAt=capture.capturedAt;ctx.processes=[];ctx.telemetry=[];
  assert.ok(!existsSync(local(ctx.out,'run-start.json')),'capture already consumed; no retries');
  captureBounds(ctx.out,capture);
  const review=JSON.parse(readFileSync(ctx.identityReviewFile));identityReview(ctx.out,{...capture,sha256:captureSha},review);
  for(const a of ['A','B'])sourceStable(ctx.sources[a],snapshot(ctx.arms[a]).source);for(const b of Object.values(ctx.builds))stable(ctx,b);
  const originalEnv=getJson(ctx.out,ctx.env);for(const a of ['A','B']){const current=await environment(ctx,a);assert.equal(current.description,originalEnv.arms[a].description,'environment drift');}
  // Review and all source/binary/stat/env checks precede the first workload, not aggregation.
  ctx.identityReviewRef=putJson(ctx.out,'identity-review-before-samples.json',review);putJson(ctx.out,'run-start.json',{at:now(),captureSha256:captureSha,identityReview:ctx.identityReviewRef});
  for(const b of Object.values(ctx.builds).filter(b=>b.role==='control'))assert.ok(b.symbolPresent,'control exact symbol absent; no workload launched');
  const prelaunchOutcomes=[];for(const b of Object.values(ctx.builds).filter(b=>b.role==='iai'||b.role==='control'))prelaunchOutcomes.push(await boundaryEvidence(ctx,b,{retainFailure:true,type:'run-precheck'}));assert.ok(prelaunchOutcomes.every(e=>e.verification),'boundary precheck failed; no workloads launched');
  await telemetry(ctx,'before-measurement');
  const hashes=capture.hashes,inv=ctx.inv,invRef=capture.inventory,ancestryRef=capture.ancestry;
  const probes={};for(const arm of ['A','B'])for(const path of ['lease','global'])probes[`${arm}-${path}`]=await observation(ctx,arm,'oracle',['probe-'+path]);
  const links=[],nativeLinks=[];
  for(const label of ['A1','B1','A2','B2','C0']) {
    const control=(await iai(ctx,label,'control'))[0];
    const measured=await iai(ctx,label);await telemetry(ctx,'after-iai-'+label);
    for(const hit of measured) {const arm=label.startsWith('B')?'B':'A';const oracle=BENEFIT.includes(hit.name)||[PUBLICATION,PRODUCER].includes(hit.name)?await observation(ctx,arm,'oracle',[mode(hit.name)]):null;links.push({hit:hit.id,oracle:oracle?.id??null,probe:MI.includes(hit.name)?null:probes[`${arm}-global`].id,control:hit.name===PRODUCER?control.id:null});}
  }
  for(const job of schedule()) {await telemetry(ctx,'native-'+job.index);const arm=job.arm==='B'?'B':'A',twin=await observation(ctx,arm,'twin',[String(job.n)]),native=await observation(ctx,arm,'native',[String(job.n)]);nativeLinks.push({job,twin:twin.id,native:native.id});}
  await telemetry(ctx,'after-native');for(const arm of ['A','B'])sourceStable(ctx.sources[arm],snapshot(ctx.arms[arm]).source);for(const b of Object.values(ctx.builds))stable(ctx,b);
  const bundle={version:1,hashes,sources:ctx.sources,environment:ctx.env,ancestry:ancestryRef,inventory:invRef,builds:ctx.builds,capturedAt:ctx.capturedAt,capture:{path:'capture.json',sha256:sha(readFileSync(local(ctx.out,'capture.json')))},identityReview:ctx.identityReviewRef,runStart:{path:'run-start.json',sha256:sha(readFileSync(local(ctx.out,'run-start.json')))},processes:ctx.processes.map(({row,...p})=>p),links,nativeLinks,telemetry:ctx.telemetry,limitations:[`Actual iai whole group executes ${inv.all.length} functions; only frozen24 selected judge rows. Pinned 0.14.2 filter applies to bench filename, not functions.`, 'Twin geometry transfer requires reviewed exact shared allocation path; not independent production ownership proof.','Segment count is unique workload route incarnations, not simultaneous census.'],review:null};putJson(ctx.out,'bundle.json',bundle);return aggregate(ctx.out,{synthetic:ctx.synthetic===true});
}

function readProcess(dir,bundle,p,synthetic=false) {
  const arm=p.arm==='C0'?'C0':p.arm.startsWith('B')?'B':'A',b=bundle.builds[`${arm}-${p.role}`];assert.ok(b,'missing build');assert.equal(p.binarySha256,b.binarySha256,'process binary drift');
  const process=getJson(dir,p.process);assert.equal(process.code,0);assert.equal(process.arm,p.arm);assert.equal(process.role,p.role);assert.equal(process.timedOut,false);assert.ok(Date.parse(process.startedAt)<=Date.parse(process.endedAt),'host start after finish');assert.equal(p.startedAt,process.startedAt,'host start mismatch');assert.ok(Date.parse(process.startedAt)>Date.parse(bundle.capturedAt),'process before captures');get(dir,process.stderr);
  const stdout=get(dir,process.stdout).toString();
  if(p.role==='iai'||p.role==='control') {
    const e=checkBoundaryEvidence(dir,p.boundaryReceipt,b);assert.ok(Date.parse(getJson(dir,e.disassembly).endedAt)<=Date.parse(process.startedAt),'boundary evidence after launch');assert.ok(Date.parse(getJson(dir,e.nm).startedAt)>Date.parse(bundle.capturedAt),'boundary nm not fresh');
    assert.ok(p.parsedExecutable==='./'+b.binaryPath||p.parsedExecutable==='<private>','retained executable linkage drift');
    assert.ok(INVENTORY.includes(p.name),'wrong cell name');const parsed=parseProfiles(p.profiles.map(r=>get(dir,r).toString()),get(dir,p.log).toString(),{producer:p.name===PRODUCER,control:p.role==='control',name:p.name,cell:inventory(get(dir,bundle.sources.A.blobs['benches/perf_gate_iai.rs'])).cells.find(c=>c.name===p.name),expectedExecutable:p.parsedExecutable,debug:get(dir,process.stderr).toString()});
    for(const k of ['profileIdentities','boundaryExecutions','pid','totalIr','childIr','threads','executedFree','exactExecuted','generatedArguments','toolArguments','measuredArguments','toolVector','dispatchIndex','method'])assert.deepEqual(p[k],parsed[k],'profile/envelope mismatch');
    const profileInventory=getJson(dir,p.profileReceipt);const inventoryEntry=profileInventory.find(x=>x.id===p.id);assert.ok(inventoryEntry,'missing profile inventory');assert.deepEqual(inventoryEntry.profiles,p.profiles);assert.deepEqual(inventoryEntry.log,p.log);assert.equal(inventoryEntry.totalIr,p.totalIr);assert.deepEqual(inventoryEntry.profileIdentities,p.profileIdentities);assert.equal(inventoryEntry.boundaryExecutions,p.boundaryExecutions);assert.equal(inventoryEntry.childIr,p.childIr);
    const block=stdout.split(/(?=^\w+::)/m).find(x=>new RegExp('^(?:\\w+::)+'+p.name+'\\b').test(x));assert.ok(block,'missing actual name');assert.equal(Number(/^\s*Instructions:\s*([\d,]+)/m.exec(block)?.[1].replaceAll(',','')),p.totalIr);return p;
  }
  const lines=stdout.trim().split(/\r?\n/);assert.equal(lines.length,1,'one process two rows');const row=getJson(dir,p.raw);assert.deepEqual(row,JSON.parse(lines[0]));assert.equal(row.config.runId,p.id);assert.ok(row.config.pid>0);
  const stamp=Number(row.startedAtUnixMs);assert.equal(p.startedAt,process.startedAt);assert.ok(stamp>=Date.parse(process.startedAt)-1000&&stamp<=Date.parse(process.endedAt)+1000,'clock mismatch');
  feature(row.features,b.features);assert.deepEqual(row.cfg,b.cfg);
  if(p.role==='native') {for(const k of ['segmentCount','cachePolicy','cacheSlots','cacheBudget'])assert.equal(row.config.resolved[k],null,'production fabricated readback');text(row.resolvedUnavailableReason);assert.equal(row.geometry,null);}
  else {configValid(row.config);if(p.role==='oracle'){assert.equal(row.mode,p.args[0],'wrong oracle mode');assert.equal(row.claimPath,row.mode==='probe-lease'?'lease':'global','oracle claim path mismatch');geometryValid(row,geometryMethods(get(dir,bundle.sources[b.sourceArm].blobs['benches/support/r19_prf01.rs'])));for(const k of COUNTERS)int(row.oracle[k]);if(row.mode==='publication')int(row.oracle.strict_trim_remaining);else {assert.equal(row.oracle.strict_trim_remaining,null);text(row.oracle.unobservedReasons.strict_trim_remaining);}assert.ok(row.smallSidecarSizeBytes>0);}else {assert.equal(row.n,Number(p.args[0]));assert.equal(row.geometry.segmentCount,1);assert.equal(row.geometry.blockCount,4096);assert.ok(row.geometry.routeIncarnation>0);}}
  return {...p,row};
}
function provenance(p) {return {runId:p.id,pid:p.row.config.pid,binarySha256:p.binarySha256,receipt:p.raw,copiedFields:['segmentCount','cachePolicy','cacheSlots','cacheBudget']};}
export function aggregate(dir,{reviewFile,synthetic=false}={}) {
  const bundle=JSON.parse(readFileSync(local(dir,'bundle.json')));assert.equal(bundle.version,1);const sources=bundle.sources;
  const capture=getJson(dir,bundle.capture),preReview=getJson(dir,bundle.identityReview),runStart=getJson(dir,bundle.runStart);assert.deepEqual(capture.linkage,captureLink({sources:capture.sources,builds:capture.builds,env:capture.environment,invRef:capture.inventory}),'capture linkage drift');identityReview(dir,{...capture,sha256:bundle.capture.sha256},preReview,runStart.at);assert.deepEqual(capture.builds,bundle.builds,'captured build identity drift');assert.deepEqual(capture.sources,bundle.sources);assert.deepEqual(runStart.identityReview,bundle.identityReview);assert.equal(runStart.captureSha256,bundle.capture.sha256);for(const p of bundle.processes)assert.ok(Date.parse(p.startedAt)>=Date.parse(runStart.at),'sample before identity review');
  for(const arm of ['A','B']) {const s=sources[arm],manifest=getJson(dir,s.manifest),u=getJson(dir,s.untracked);assert.deepEqual(u,[]);assert.equal(get(dir,s.patch).length,0);assert.equal(sha(get(dir,s.patch)),s.patchSha256);assert.equal(sha(JSON.stringify(u)),s.untrackedSha256);assert.equal(sha(JSON.stringify(manifest)),s.manifestSha256);assert.equal(sha(JSON.stringify([s.baseHead,s.tree,s.patchSha256,manifest])),s.identitySha256,'source identity drift');assert.equal(manifest.length,Object.keys(s.blobs).length);for(const [path,size,digest] of manifest){assert.equal(s.blobs[path].sha256,digest);assert.equal(get(dir,s.blobs[path]).length,size);}for(const p of HARNESS)assert.equal(s.harness[p],s.blobs[p].sha256);}
  assert.deepEqual(sources.A.harness,sources.B.harness);assert.notEqual(sources.A.identitySha256,sources.B.identitySha256);const graph=get(dir,bundle.ancestry).toString();ancestry(sources.A.baseHead,sources.B.baseHead,graph);
  const hb=p=>get(dir,sources.A.blobs[p]);const hashes=frozen(hb(PREREG),hb(JUDGE),hb(FREEZE));assert.deepEqual(bundle.hashes,hashes);assert.equal(sha(readFileSync(local(ROOT,JUDGE))),hashes[1].sha256);const inv=inventory(hb('benches/perf_gate_iai.rs'));assert.deepEqual(getJson(dir,bundle.inventory),inv);const env=getJson(dir,bundle.environment);text(env.allocator);
  const buildKeys=['C0-iai','C0-control',...['A','B'].flatMap(a=>Object.keys(ROLES).map(r=>`${a}-${r}`))];assert.deepEqual(Object.keys(bundle.builds).sort(),buildKeys.sort());
  for(const [key,b] of Object.entries(bundle.builds)) {assert.equal(b.arm,key.split('-')[0]);assert.equal(b.role,key.slice(key.indexOf('-')+1),'build role/key mismatch');assert.equal(b.sourceArm,key.startsWith('B-')?'B':'A','C0 source B');assert.equal(b.sourceIdentitySha256,sources[b.sourceArm].identitySha256);assert.equal(b.environmentSha256,bundle.environment.sha256);assert.equal(sha(get(dir,b.binary)),b.binarySha256,'binary drift');feature(b.features,ROLES[b.role][1]);assert.deepEqual(b.cfg,ROLES[b.role][3]);assert.equal(b.locked,true);assert.equal(b.jobs,3);assert.equal(b.profile,'release');assert.ok(b.binaryPath.startsWith(`target/prf01-${dirLabel(b.arm)}-${b.role}/`));const proc=getJson(dir,b.process);assert.equal(proc.type,'build');assert.equal(proc.code,0);assert.equal(proc.arm,b.arm);assert.equal(proc.role,b.role);assert.equal(proc.timedOut,false);assert.ok(Date.parse(proc.endedAt)<=Date.parse(b.capturedAt),'build unfinished at capture');assert.ok(Date.parse(sources[b.sourceArm].capturedAt)<Date.parse(proc.startedAt),'source after build');assert.ok(Date.parse(b.capturedAt)<=Date.parse(bundle.capturedAt));for(const ref of [b.process,b.graph,b.symbols,b.runnerSource]){const pr=getJson(dir,ref);assert.equal(pr.code,0);assert.equal(pr.arm,b.arm);assert.equal(pr.role,b.role);assert.ok(Date.parse(pr.endedAt)<=Date.parse(bundle.capturedAt),'build evidence after capture');get(dir,pr.stderr);get(dir,pr.stdout);}const runner=JSON.parse(get(dir,getJson(dir,b.runnerSource).stdout));assert.equal(runner.version,'0.14.2');assert.match(runner.argsSource,/separate_threads: true/);assert.match(runner.libBenchSource,/EntryPoint::Custom/);if(b.role==='iai'||b.role==='control'){checkBoundaryEvidence(dir,b.buildBoundaryReceipt,b);const symbols=get(dir,getJson(dir,b.symbols).stdout).toString();assert.ok(symbols.split(/\r?\n/).some(l=>l.endsWith(' '+SYMBOL)),'wrong exact symbol');}}
  assert.notEqual(bundle.builds['A-iai'].binarySha256,bundle.builds['C0-iai'].binarySha256,'same C0hash');
  const entries=new Map();for(const p of bundle.processes){assert.ok(!entries.has(p.id),'duplicate runId');entries.set(p.id,readProcess(dir,bundle,p,synthetic));}
  const used=new Set(),take=(id,role)=>{const p=entries.get(id);assert.ok(p,'missing process');assert.equal(p.role,role);assert.ok(!used.has(id),'one process two rows');used.add(id);return p;};
  const probes=new Map();for(const p of entries.values()){if(p.role==='oracle'&&p.row.mode.startsWith('probe-')){const key=p.arm+'-'+p.row.mode;assert.ok(!probes.has(key));probes.set(key,p);used.add(p.id);}if(p.role==='control')assert.equal(p.name,PRODUCER);}
  assert.deepEqual([...probes.keys()].sort(),['A-probe-global','A-probe-lease','B-probe-global','B-probe-lease']);assert.equal(bundle.links.length,120);assert.equal(bundle.nativeLinks.length,108);
  const blockers=[];
  const input={version:1,hashes,entry:'SeferAlloc::GlobalAlloc->HeapCore routed',inventory:INVENTORY,strictTrim:{cell:null,reason:'No existing strict-trim iai cell in benches/perf_gate_iai.rs'},environmentSha256:bundle.environment.sha256,environmentReceipt:bundle.environment,identities:{},ancestry:{result:'A ancestor B',chain:[],receipt:bundle.ancestry},sizeOf:{},iai:[],native:[],schedule:schedule()};
  function chain(id,seen=new Set()){if(id===sources.A.baseHead)return [id];assert.ok(!seen.has(id),'ancestry cycle');seen.add(id);const row=graph.trim().split('\n').map(l=>l.split(' ')).find(r=>r[0]===id);assert.ok(row);for(const parent of row.slice(1)){try{return [...chain(parent,new Set(seen)),id];}catch{}}throw Error('not ancestor');}input.ancestry.chain=chain(sources.B.baseHead);
  for(const arm of ['A','B']){const bs=bundle.builds;input.identities[arm]={label:arm,measurementCommit:sources[arm].baseHead,source:ownSource(sources[arm]),environmentSha256:bundle.environment.sha256,binaries:{iai:bs[arm+'-iai'].binarySha256,oracle:bs[arm+'-oracle'].binarySha256,native:bs[arm+'-native'].binarySha256},buildStartedAt:getJson(dir,bs[arm+'-iai'].process).startedAt,buildReceipt:bs[arm+'-iai'].process,features:PRIMARY,nativeFeatures:['production'],primaryCfg:[],oracleCfg:['r19_sidecar_summary_oracle'],toolchain:{rustc:env.description,target:env.target,profile:'release',flags:'role cfg only'}};}
  input.identities.C0={source:ownSource(sources.A),binarySha256:bundle.builds['C0-iai'].binarySha256,buildStartedAt:getJson(dir,bundle.builds['C0-iai'].process).startedAt,buildReceipt:bundle.builds['C0-iai'].process};
  for(const link of bundle.links){const h=take(link.hit,'iai'),arm=h.arm.startsWith('B')?'B':'A',needed=BENEFIT.includes(h.name)||[PUBLICATION,PRODUCER].includes(h.name);let o=null;if(needed){o=take(link.oracle,'oracle');assert.equal(o.arm,arm);assert.equal(o.row.mode,mode(h.name));}else assert.equal(link.oracle,null);
    const probe=MI.includes(h.name)?null:probes.get(arm+'-probe-global');assert.equal(link.probe,probe?.id??null);const c=MI.includes(h.name)?{requested:null,resolved:null,configConflictsDelta:null,notApplicableReason:'mimalloc',processBoundary:'isolated-process',runId:h.id,pid:h.pid}:{...structuredClone((o??probe).row.config),runId:h.id,pid:h.pid,resolvedProvenance:provenance(o??probe)};
    const cell=inv.cells.find(c=>c.name===h.name);assert.ok(cell);if(!o&&probe){assert.ok(HOT.includes(h.name));assert.equal(cell.claimPath,'global');for(const k of ['requested','resolved'])c[k]={maintenance:probe.row.config.resolved.maintenance,workCount:cell.declaredWorkCount,segmentCount:null,blockBytes:cell.blockBytes,cachePolicy:probe.row.config.resolved.cachePolicy};c.unobservedReasons={segmentCount:'measured body has no paired geometry observer; probe census is not workload geometry'};c.declaredProvenance={receipt:bundle.inventory,cell:h.name,fields:['workCount','blockBytes','alignment'],alignment:cell.alignment};c.resolvedProvenance={runId:probe.id,pid:probe.row.config.pid,binarySha256:probe.binarySha256,receipt:probe.raw,arm,claimPath:'global',copiedFields:['cachePolicy','configConflictsDelta','maintenance']};}
    const oracle=o?{...o.row.oracle,linkage:{binarySha256:o.binarySha256,primaryBinarySha256:h.binarySha256,source:ownSource(sources[arm]),environmentSha256:bundle.environment.sha256,features:PRIMARY,cfg:['r19_sidecar_summary_oracle'],startedAt:o.startedAt,config:o.row.config,receipt:o.raw,featureExactReceipt:null}}:{...Object.fromEntries([...COUNTERS,'strict_trim_remaining'].map(k=>[k,null])),unobservedReasons:Object.fromEntries([...COUNTERS,'strict_trim_remaining'].map(k=>[k,'no paired oracle for control'])),linkage:null};
    assert.equal(h.startedAt,getJson(dir,h.process).startedAt,'iai host start drift');
    const row={name:h.name,arm:h.arm,environmentSha256:bundle.environment.sha256,binarySha256:h.binarySha256,source:ownSource(sources[arm]),startedAt:h.startedAt,features:PRIMARY,receipt:h.profiles[0],config:c,Ir:h.totalIr,oracle};
    if(o){
      const counts=o.row.oracle;assert.ok(counts.nonempty_cuts<=counts.pending_words_exchanged&&counts.pending_words_exchanged<=counts.pending_words_visited,'invalid oracle counts');
      if(BENEFIT.includes(h.name)){assert.ok(counts.routed_scans>0,'routed scan missing');assert.equal(counts.nonempty_cuts,0);assert.equal(counts.discovery_records_reclaimed,0);if(arm==='B'){assert.equal(counts.pending_words_visited,0);assert.ok(counts.summary_words_visited>0);}else{assert.ok(counts.pending_words_visited>0);assert.equal(counts.pending_words_exchanged,counts.pending_words_visited);const expected=h.name===BENEFIT[0]?319063:h.name===BENEFIT[1]?1004503:null;if(expected!==null)assert.ok(Math.abs(counts.pending_words_visited-expected)<=Math.floor(expected*.01),'frozen count tolerance');}}
      if(h.name===PRODUCER){assert.equal(o.row.config.resolved.workCount,4096);assert.equal(o.row.config.resolved.segmentCount,1);if(arm==='B'){assert.ok(counts.summary_rmw>0);assert.ok(counts.summary_load>0);}}
      if(h.name===PUBLICATION){assert.ok(counts.discovery_records_reclaimed>0);int(counts.strict_trim_remaining);}
      input.sizeOf[arm]??=o.row.smallSidecarSizeBytes;assert.equal(input.sizeOf[arm],o.row.smallSidecarSizeBytes);
    }
    if(h.name===PRODUCER){const control=take(link.control,'control');collectionEquivalent(h,control,hb('benches/support/r19_prf01.rs').toString());const primaryBuild=bundle.builds[(h.arm==='C0'?'C0':arm)+'-iai'],controlBuild=bundle.builds[(h.arm==='C0'?'C0':arm)+'-control'];assert.deepEqual(primaryBuild.features,controlBuild.features);assert.deepEqual(primaryBuild.cfg,[]);assert.deepEqual(controlBuild.cfg,['r19_prf01_producer_control']);assert.equal(primaryBuild.sourceIdentitySha256,controlBuild.sourceIdentitySha256);assert.equal(control.arm,h.arm);const coverage=pairCoverage(h,control);assert.ok(h.totalIr>control.totalIr,'non-distinguishing control');assert.notEqual(control.binarySha256,h.binarySha256);const b=bundle.builds[(h.arm==='C0'?'C0':arm)+'-iai'];row.scenario='joined remote Small frees; repeated frees in same pending word; no owner discovery during producer window';row.producerIrReceipt={binarySha256:h.binarySha256,source:row.source,environmentSha256:row.environmentSha256,features:PRIMARY,cfg:[],receipt:h.process,symbolReceipt:h.boundaryReceipt,profileReceipt:h.profileReceipt,symbol:SYMBOL,toggleCollect:SYMBOL,runnerVersion:'0.14.2',valgrindVersion:env.description,generatedArguments:h.generatedArguments,collectAtStart:false,defaultToggle:false,entryPoint:'Custom',separateThreads:true,aggregation:'sum of all thread profile Ir',childFreeCount:4096,boundaryExecutions:h.boundaryExecutions,boundaryOverheadIr:coverage.boundaryOverheadIr,workloadIr:coverage.workloadIr,childIr:h.childIr,totalIr:h.totalIr,startedAt:h.startedAt,control:{receipt:control.process,symbolReceipt:control.boundaryReceipt,profileReceipt:control.profileReceipt,symbol:SYMBOL,toggleCollect:SYMBOL,cfg:['r19_prf01_producer_control'],features:PRIMARY,generatedArguments:control.generatedArguments,runnerVersion:'0.14.2',valgrindVersion:env.description,collectAtStart:false,defaultToggle:false,entryPoint:'Custom',separateThreads:true,aggregation:'sum of all thread profile Ir',childFreeCount:0,boundaryExecutions:control.boundaryExecutions,workloadIr:0,workload:'child remote frees removed',source:ownSource(sources[arm]),binarySha256:control.binarySha256,environmentSha256:row.environmentSha256,childIr:control.childIr,totalIr:control.totalIr,deltaIr:h.totalIr-control.totalIr,parentSetupUnchanged:true,collectionUnchanged:true}};}else assert.equal(link.control,null);
    input.iai.push(row);
  }
  for(const [i,link] of bundle.nativeLinks.entries()){assert.deepEqual(link.job,schedule()[i]);const t=take(link.twin,'twin'),n=take(link.native,'native');assert.equal(bundle.processes.findIndex(p=>p.id===n.id),bundle.processes.findIndex(p=>p.id===t.id)+1,'twin not adjacent');const arm=link.job.arm==='B'?'B':'A';assert.equal(t.arm,arm);assert.equal(n.arm,arm);assert.equal(t.row.n,link.job.n);assert.equal(n.row.n,link.job.n);assert.deepEqual(t.row.config.requested,n.row.config.requested);const config=structuredClone(n.row.config);for(const k of ['segmentCount','cachePolicy','cacheSlots','cacheBudget'])config.resolved[k]=t.row.config.resolved[k];config.resolvedProvenance=provenance(t);configValid(config);input.native.push({...link.job,source:ownSource(sources[arm]),startedAt:n.startedAt,environmentSha256:bundle.environment.sha256,binarySha256:n.binarySha256,features:['production'],receipt:n.raw,config,entry:n.row.entry,nsPerOp:n.row.nsPerOp,p50Ns:n.row.p50Ns,p99Ns:n.row.p99Ns});}
const declaredLinks=bundle.links.map(link=>{const h=entries.get(link.hit);assert.ok(h,'missing process');return {name:h.name,arm:h.arm};});for(const name of INVENTORY)for(const arm of ['A1','A2','B1','B2','C0'])assert.equal(declaredLinks.filter(r=>r.name===name&&r.arm===arm).length,1,'missing/duplicate linked cell arm');
  const selectedPairs=INVENTORY.flatMap(name=>['A1','A2','B1','B2','C0'].map(arm=>({name,arm})));if(!blockers.length)for(const {name,arm} of selectedPairs)assert.equal(input.iai.filter(r=>r.name===name&&r.arm===arm).length,1,'missing/duplicate cell arm');
  for(const name of BENEFIT){const rs=input.iai.filter(r=>r.name===name);assert.equal(new Set(rs.map(r=>r.oracle.routed_scans)).size,1,'routed scans arm mismatch');assert.equal(new Set(rs.map(r=>JSON.stringify(r.config.requested))).size,1,'matched config mismatch');}
  assert.equal(used.size,entries.size,'orphan process');const labels=['before-measurement',...['A1','B1','A2','B2','C0'].map(a=>'after-iai-'+a),...schedule().map(j=>'native-'+j.index),'after-native'];const telemetry=bundle.telemetry.map(r=>getJson(dir,r));assert.deepEqual(telemetry.map(t=>t.label),labels,'telemetry schedule missing');for(const t of telemetry){const p=getJson(dir,t.process);get(dir,p.stdout);get(dir,p.stderr);text(t.wsl);assert.ok(Number.isFinite(t.hostCpuLoadPercent)&&t.hostCpuLoadPercent>=0&&t.hostCpuLoadPercent<=100,'host CPU unavailable');}for(const link of bundle.nativeLinks){const t=telemetry.find(t=>t.label==='native-'+link.job.index),proc=getJson(dir,entries.get(link.twin).process);assert.ok(Date.parse(t.at)<=Date.parse(proc.startedAt),'native telemetry after task');}
  const linkage={sources:Object.fromEntries(['A','B'].map(a=>[a,sources[a].identitySha256])),binaries:Object.fromEntries(Object.entries(bundle.builds).map(([k,b])=>[k,b.binarySha256])),processHashes:bundle.processes.map(p=>p.raw?.sha256??p.profiles.map(r=>r.sha256)),inventorySha256:bundle.inventory.sha256,environmentSha256:bundle.environment.sha256,premeasurementIdentityReviewSha256:bundle.identityReview.sha256,captureSha256:bundle.capture.sha256};
  if(!reviewFile)return {verdict:'INCOMPLETE',blockers:[...blockers,'reviewed featureExactReceipt, identityReviewReceipt, inventoryReceipt required; no reviewed evidence fabricated'],linkage,validatedProcesses:entries.size};
  const review=JSON.parse(readFileSync(reviewFile));assert.equal(review.status,'reviewed');text(review.reviewer);assert.deepEqual(review.linkage,linkage);for(const k of ['featureExactReceipt','identityReviewReceipt','inventoryReceipt'])text(review[k]);text(review.nativeSharedAllocationPathReview);text(review.producerProfileReview);text(review.segmentCountInterpretationReview);
  if(blockers.length)return {verdict:'INCOMPLETE',blockers,linkage,validatedProcesses:entries.size};
  const reviewRef={path:`review-${sha(json(review))}.json`,sha256:sha(json(review))};if(!existsSync(local(dir,reviewRef.path)))putJson(dir,reviewRef.path,review);for(const k of ['featureExactReceipt','identityReviewReceipt','inventoryReceipt'])input[k]=reviewRef;input.identityReviewReceipt=bundle.identityReview;input.sizeOf.receipt=reviewRef;for(const r of input.iai)if(r.oracle.linkage)r.oracle.linkage.featureExactReceipt=reviewRef;
  const ref={path:`judge-input-${sha(json(input))}.json`,sha256:sha(json(input))};if(!existsSync(local(dir,ref.path)))putJson(dir,ref.path,input);
  const r=spawnSync(process.execPath,[local(ROOT,JUDGE),'--input',local(dir,ref.path)],{encoding:'utf8',cwd:ROOT});const result=JSON.parse(r.stdout);assert.equal(r.status,result.verdict==='GO'?0:1);return {...result,synthetic,validatedProcesses:entries.size};
}

function fixtureAssembly(control) {return `00000001 <${SYMBOL}>:\n 1: ${control?'ret':'call 30 <SeferAlloc::dealloc>'}\n${control?'':' 2: ret\n'} 8: int3\n 9: int3\n\n00000010 <std::thread::spawn::closure>:\n 10: call 1 <${SYMBOL}>\n 11: ret\n`;}
export async function selfTest() {
  const base=resolve(ROOT,'target/prf01-amendment-b-'+randomUUID());mkdirSync(dirname(base),{recursive:true});let checks=0;
  try {
    const arms=fixtureRepos(base),out=resolve(base,'out'),transport=fixtureTransport();let launches=0,c0nm=0;const run=transport.execute.bind(transport);transport.execute=async r=>{if(r.type==='iai')launches++;if(r.type==='launch-boundary-nm'&&r.arm==='C0')c0nm++;return run(r);};
    const result=await syntheticPipeline({arms,out,transport,synthetic:true});assert.equal(result.verdict,'INCOMPLETE');assert.equal(c0nm,2);assert.equal(launches,10);checks++;
    const review=resolve(base,'review.json');writeFileSync(review,json({status:'reviewed',reviewer:'synthetic',linkage:result.linkage,featureExactReceipt:'fixture',identityReviewReceipt:'fixture',inventoryReceipt:'fixture',nativeSharedAllocationPathReview:'fixture',producerProfileReview:'fixture',segmentCountInterpretationReview:'fixture'}));assert.equal(aggregate(out,{reviewFile:review,synthetic:true}).verdict,'GO');checks++;
    const input=JSON.parse(readFileSync(local(out,readdirSync(out).find(n=>n.startsWith('judge-input-')))));const changes=[p=>p.boundaryOverheadIr=0,p=>p.boundaryOverheadIr=17,p=>p.workloadIr++,p=>p.workloadIr=0,p=>p.boundaryExecutions=0,p=>p.control.boundaryExecutions=2,p=>delete p.control.symbolReceipt,p=>delete p.control.profileReceipt,p=>p.control.symbol='wrong',p=>p.control.toggleCollect='*',p=>p.control.cfg=[],p=>p.control.generatedArguments='',p=>p.control.source.identitySha256='0'.repeat(64),p=>p.childIr=p.totalIr+1];
    for(const change of changes){const d=structuredClone(input);change(d.iai.find(r=>r.name===PRODUCER).producerIrReceipt);const path='negative-'+checks+'.json';putJson(out,path,d);const r=spawnSync(process.execPath,[local(ROOT,JUDGE),'--input',local(out,path)],{cwd:ROOT,encoding:'utf8'});assert.equal(r.status,1);assert.notEqual(JSON.parse(r.stdout).verdict,'GO');checks++;}
    const cell={name:PRODUCER,index:7},cmd='binary --iai-run perf_gate 7 0',log='==42== Command: '+cmd+'\n',debug='callgrind: Arguments: '+PRODUCER_SETTINGS.filter(x=>x!=='--separate-threads=yes').join(' ')+' --separate-threads=yes --collect-atstart=no --callgrind-out-file=/evidence/'+PRODUCER+'/callgrind.out --toggle-collect='+SYMBOL+'\n';const head=t=>`pid: 42\nthread: ${t}\npart: 1\ncmd: ${cmd}\npositions: line instr\nevents: Ir Dr\n`;
    const parent=head(1)+'fn=parent\n1 0x100 10 0\ntotals: 10 0\n',child=head(2)+`fn=std::thread::spawn::closure\n1 0x100 0 0\ncfn=${SYMBOL}\ncalls=1 1 0x100\n* * 25 0\nfn=${SYMBOL}\n1 0x100 5 0\ncfn=SeferAlloc::dealloc\ncalls=0x1000 +1 +0x4\n* * 20\nfn=SeferAlloc::dealloc\n1 0x100 20 0\ntotals: 25 0\n`,control=head(2)+`fn=std::thread::spawn::closure\n1 0x100 0\ncfn=${SYMBOL}\ncalls=1 1 0x100\n* * 8\nfn=${SYMBOL}\n1 0x100 8\ntotals: 8 0\n`;
    const opts={name:PRODUCER,cell,producer:true,debug,expectedExecutable:'binary'},p=parseProfiles([parent,child],log,opts),c=parseProfiles([parent,control],log,{...opts,control:true});assert.deepEqual(pairCoverage(p,c),{boundaryOverheadIr:8,workloadIr:17,controlWorkloadIr:0});checks++;
    const reject=(profiles,pattern,options=opts)=>{assert.throws(()=>parseProfiles(profiles,log,options),pattern);checks++;};
    reject([parent,child.replace('calls=1 1','calls=2 1')],/executions/);reject([parent,child.replace('calls=1 1','calls=0 1')],/executions/);reject([parent,child.replace('part: 1','part: 2')],/parts/);reject([parent,child,child],/duplicate/);reject([parent,child.replace('thread: 2','thread: 3')],/gaps/);reject([child],/main thread/);reject([parent],/child profile/);reject([parent,child.replace('totals: 25','totals: 24')],/inclusive\/child|self costs\/final/);reject([parent,child.replace('cmd: binary','cmd: other')],/argv mismatch/);reject([parent,control.replace('1 0x100 8','1 0x100 17').replace('totals: 8','totals: 17')],/overhead/,{...opts,control:true});reject([parent,child],/control outgoing/,{...opts,control:true});
    for(const overhead of [0,17]){assert.throws(()=>pairCoverage(p,{...c,childIr:overhead}),/overhead/);checks++;}assert.throws(()=>pairCoverage({...p,childIr:8},c),/empty primary/);checks++;
    assert.equal(profileFunctions(child).get('std::thread::spawn::closure').edgeCalls.get(SYMBOL),1n);assert.equal(profileFunctions(child).get(SYMBOL).edgeCalls.get('SeferAlloc::dealloc'),4096n);checks++;
    for(const dis of [fixtureAssembly(true).replace('ret','call 30 <helper>'),fixtureAssembly(true).replace('std::thread::spawn::closure','random_caller'),fixtureAssembly(true).replace(' 9: int3',' 9: call 30 <helper>'),fixtureAssembly(true).replace(' 9: int3',' 9: ret'),fixtureAssembly(true).replace('1: ret\n','1: nop\n')]){assert.throws(()=>machineBoundary('1 T '+SYMBOL+'\n',dis,true),/control disassembly|child closure|no ret/);checks++;}
    assert.ok(machineBoundary('1 T '+SYMBOL+'\n',fixtureAssembly(true),true).controlEmpty);assert.ok(/ 8: int3/.test(fixtureAssembly(true))&&machineBoundary('1 T '+SYMBOL+'\n',fixtureAssembly(false),false).boundaryBody.includes('int3'));checks++;
    const absent=fixtureTransport(),executeAbsent=absent.execute.bind(absent);let samples=0;absent.execute=async r=>{if(r.type==='launch-boundary-nm')return {code:0,stdout:'missing\n',stderr:''};if(r.type==='iai')samples++;return executeAbsent(r);};const builds=JSON.parse(readFileSync(local(out,'capture.json'))).builds,ctx={out,arms,transport:absent,builds,inv:inventory(readFileSync(local(arms.A,'benches/perf_gate_iai.rs')))};await assert.rejects(iai(ctx,'C0'),/prelaunch verification failed/);assert.equal(samples,0);checks++;
    // Real Callgrind omits trailing zero columns of totals; extra columns stay rejected.
    assert.equal(parseProfiles([parent.replace('totals: 10 0','totals: 10'),child.replace('totals: 25 0','totals: 25')],log,opts).childIr,25);checks++;reject([parent,child.replace('totals: 25 0','totals: 25 0 0')],/malformed totals/);
    for(const value of [1,16]){const cp=control.replace('* * 8','* * '+value).replace('1 0x100 8','1 0x100 '+value).replace('totals: 8','totals: '+value);assert.equal(parseProfiles([parent,cp],log,{...opts,control:true}).childIr,value);checks++;}
    reject([parent,child.replace('calls=1 1 0x100','calls=1 1 0x100\nfn=broken')],/missing call cost/);reject([parent,child.replace('std::thread::spawn::closure','random_caller')],/child context/);reject([parent,child.replace('part: 1','part: 1\npart: 1')],/ambiguous/);reject([parent,child.replace('20\nfn=','20 0 1\nfn=')],/extra event/);reject([parent,control.replace('1 0x100 8','1 0x100 0').replace('totals: 8','totals: 0')],/empty|inclusive/,{...opts,control:true});
    assert.throws(()=>parseProfiles([parent,child],log,{...opts,expectedExecutable:'wrong'}),/binary mismatch/);checks++;assert.throws(()=>collectionEquivalent(p,{...c,measuredArguments:c.measuredArguments+' extra'},readFileSync(local(ROOT,'benches/support/r19_prf01.rs'),'utf8')),/dispatcher argv/);checks++;
    const coherent=structuredClone(input);const receipt=coherent.iai.find(r=>r.name===PRODUCER).producerIrReceipt;receipt.childIr=receipt.boundaryOverheadIr;receipt.workloadIr=0;const badPath='empty-primary.json';putJson(out,badPath,coherent);assert.equal(spawnSync(process.execPath,[local(ROOT,JUDGE),'--input',local(out,badPath)],{cwd:ROOT}).status,1);checks++;
    const cap=JSON.parse(readFileSync(local(out,'capture.json')));const prepareCtx={out,arms,transport,builds:cap.builds,inv:inventory(readFileSync(local(arms.A,'benches/perf_gate_iai.rs')))};let noWork=0;const nmMissing=fixtureTransport(),executeMissing=nmMissing.execute.bind(nmMissing);nmMissing.execute=async r=>{if(r.type==='preflight-nm'&&r.role==='control')return {code:0,stdout:'optimized out\n',stderr:''};if(r.type==='producer-diagnostic')noWork++;return executeMissing(r);};
    const pairOutcomes=[];for(const b of [cap.builds['A-iai'],cap.builds['A-control']])pairOutcomes.push(await boundaryEvidence({...prepareCtx,transport:nmMissing},b,{retainFailure:true,type:'preflight'}));assert.equal(pairOutcomes.length,2);assert.ok(pairOutcomes[0].verification);assert.equal(pairOutcomes[1].verification,null);assert.equal(noWork,0);checks++;
    assert.equal(parseProfiles([parent,child],log,{...opts,debug:debug.replace(' --collect-atstart=no','')}).childIr,25);checks++;
    for(const badDebug of [debug.replace('--collect-atstart=no','--collect-atstart=yes'),debug.replace('--collect-atstart=no','--collect-atstart=no --collect-atstart=yes'),debug.replace('--collect-atstart=no','--collect-atstart=no --collect-atstart=no'),debug.replace('--cache-sim=yes','--cache-sim=no'),debug.replace('--combine-dumps=no','--combine-dumps=yes'),debug.replace('--I1=32768,8,64','--I1=1,1,1')]){assert.throws(()=>parseProfiles([parent,child],log,{...opts,debug:badDebug}),/collect-atstart|producer setting/);checks++;}
    const legacyChecks=await legacySelfTest();
    console.log(json({status:'PASS',scope:'synthetic Amendment B and all adapted legacy checks; no actual coverage verified',checks:checks+legacyChecks,amendmentBChecks:checks,legacyChecks,measurements:0}));
  }finally{rmSync(base,{recursive:true,force:true});}
}
function fixtureTransport() {
  let pid=1000;
  return {path:p=>p.replaceAll(String.fromCharCode(92),'/'),hostLoad:async()=>7,async execute(r){
    if(r.type==='environment')return {code:0,stdout:'rustc synthetic\nhost: x86_64-unknown-linux-gnu\ncargo synthetic\nLinux synthetic\nvalgrind-3.22.0\niai-callgrind-runner actual (0.14.2)\n',stderr:''};
    if(r.type==='telemetry')return {code:0,stdout:'0.1 0.1 0.1\n8\n',stderr:''};
    if(r.type==='build'){const [name]=ROLES[r.role],p=local(r.cwd,/CARGO_TARGET_DIR="\$PWD\/([^"\n]+)"/.exec(r.script)[1]+`/release/${name}`);mkdirSync(dirname(p),{recursive:true});writeFileSync(p,`synthetic ${r.arm} ${r.role}`,{flag:'wx'});return {code:0,stdout:JSON.stringify({reason:'compiler-artifact',target:{name},executable:this.path(p)})+'\n',stderr:''};}
    if(r.type==='feature-graph')return {code:0,stdout:'synthetic locked feature graph\n',stderr:''};
    if(r.type==='runner-source')return {code:0,stdout:JSON.stringify({version:'0.14.2',argsSource:'synthetic separate_threads: true',libBenchSource:'synthetic EntryPoint::Custom'})+'\n',stderr:''};
    if(/-nm$/.test(r.type))return {code:0,stdout:'1 T '+SYMBOL+'\n',stderr:''};if(/-disassembly$/.test(r.type))return {code:0,stdout:fixtureAssembly(r.role==='control'),stderr:''};
    if(r.type==='symbols')return {code:0,stdout:'1 T '+SYMBOL+'\n',stderr:''};

    if(r.type==='iai'){
      const home=/'--home'\s+'([^']+)'/.exec(r.script)[1];mkdirSync(home,{recursive:true});let stdout='',stderr='';
      const executable=/'(\.\/target\/[^']+)'/.exec(r.script)[1];
      for(const name of INVENTORY){const dir=resolve(home,name);mkdirSync(dir);const current=++pid,isB=r.arm.startsWith('B');let total=1000;if(BENEFIT.slice(0,2).includes(name)&&isB)total=200;if(name.startsWith('growth16_'))total=[100,200,400][BENEFIT.indexOf(name)-2];if(r.role==='control')total=100;
        const producer=name===PRODUCER,child=producer?(r.role==='control'?8:total-10):0;
        const idx=inventory(readFileSync(local(r.cwd,'benches/perf_gate_iai.rs'))).all.indexOf(name);
        const header=tid=>`pid: ${current}\nthread: ${tid}\npart: 1\ncmd: ${executable} --iai-run perf_gate ${idx} 0\npositions: line\nevents: Ir\n`;
        const parent=header(1)+`fn=(1) wrapper\n1 ${total-child}\nsummary: ${total-child}\ntotals: ${total-child}\n`;
        const incoming=`fn=(9) std::thread::spawn::closure\n1 0\ncfn=(1) ${SYMBOL}\ncalls=1 1\n* ${child}\n`;const body=incoming+(r.role==='control'?`fn=(1)\n1 ${child}\n`:`fn=(1)\n1 1\ncfn=(2) SeferAlloc::dealloc\ncalls=4096 1\n* ${child-1}\nfn=(2)\n1 ${child-1}\n`);
        writeFileSync(resolve(dir,'callgrind.out'),'');writeFileSync(resolve(dir,'callgrind.out.1'),parent);if(producer)writeFileSync(resolve(dir,'callgrind.out.2'),header(2)+body+`summary: ${child}\ntotals: ${child}\n`);
        writeFileSync(resolve(dir,'callgrind.log'),`==${current}== Callgrind synthetic\n==${current}== Command: ${executable} --iai-run perf_gate ${idx} 0\n`);
        stderr+=`iai_callgrind_runner::runner::tool: Debug: callgrind: Arguments: ${PRODUCER_SETTINGS.filter(x=>x!=='--separate-threads=yes').join(' ')} --separate-threads=yes --collect-atstart=no --callgrind-out-file=/evidence/${name}/callgrind.out --toggle-collect=${producer?SYMBOL:'*::__iai_callgrind_wrapper_mod::*'}\n`;
        stdout+=`perf_gate_iai::perf_gate::${name}\n Instructions: ${total}|N/A\n`;
      }return {code:0,stdout,stderr};
    }
    assert.equal(r.type,'observation');const native=r.role==='native',twin=r.role==='twin',m=r.args[0],n=Number(m),work=native||twin?4096:m==='producer'?4096:m.startsWith('growth16_')?[32768,65536,131072][BENEFIT.indexOf(m)-2]:m==='seg_cycle_decommit_256k'?204:m==='multiseg_cold_256k'||m==='publication'?68:1;
    const requested={maintenance:false,workCount:work,segmentCount:1,blockBytes:m.includes('256k')||m==='publication'?258752:16,cachePolicy:'default',cacheSlots:8,cacheBudget:{policy:'unbounded'}},resolved=structuredClone(requested);if(native)for(const k of ['segmentCount','cachePolicy','cacheSlots','cacheBudget'])resolved[k]=null;
    const row={version:1,role:r.role,mode:m,n,startedAtUnixMs:Date.now(),features:ROLES[r.role][1],cfg:ROLES[r.role][3],config:{requested,resolved,configConflictsDelta:0,processBoundary:'isolated-process',runId:r.id,pid:++pid},geometry:native?null:{segmentCount:1,blockCount:4096,routeIncarnation:1},resolvedUnavailableReason:native?'requires twin':null,entry:'installed #[global_allocator] SeferAlloc',nsPerOp:100,p50Ns:100,p99Ns:200,smallSidecarSizeBytes:r.arm==='B'?42496:41984,oracle:Object.fromEntries(COUNTERS.map(k=>[k,0]))};
    row.oracle.strict_trim_remaining=m==='publication'?0:null;row.oracle.unobservedReasons=m==='publication'?{}:{strict_trim_remaining:'only publication'};
    if(BENEFIT.includes(m)){row.oracle.routed_scans=99;if(r.arm==='B')row.oracle.summary_words_visited=64;else {row.oracle.pending_words_visited=m===BENEFIT[0]?319063:m===BENEFIT[1]?1004503:100;row.oracle.pending_words_exchanged=row.oracle.pending_words_visited;}}
    if(m==='publication'){row.oracle.routed_scans=92;row.oracle.discovery_records_reclaimed=34;}if(m==='producer'&&r.arm==='B'){row.oracle.summary_load=4096;row.oracle.summary_rmw=1;}
    if(r.role==='oracle'){
      row.claimPath=m==='probe-lease'?'lease':'global';const pts=m==='publication'?[[m,'prepared-live-batch-before-handoff',0,34],[m,'discovery-live-batch-before-free',0,34],[m,'discovery-live-batch-before-free',1,34]]:m==='producer'?[[m,'selected-live-batch-before-handoff',0,4096]]:m.startsWith('growth16_')?[['growth16','final-live-batch-before-free',0,work]]:BENEFIT.slice(0,2).includes(m)?Array.from({length:m===BENEFIT[0]?2:6},(_,i)=>[m,'live-batch-before-free',i,34]):[];
      row.segmentCountMethod=pts.length?geometryMethods(readFileSync(local(r.cwd,'benches/support/r19_prf01.rs'))).paired:geometryMethods(readFileSync(local(r.cwd,'benches/support/r19_prf01.rs'))).probe;row.segmentIncarnations=pts.length?[1]:[];const declared=pts.map(([name,phase,round,blockCount])=>({name,phase,point:phase,round,roundIndexBase:0,blockCount,blockBytes:requested.blockBytes}));row.checkpoints=declared.map(c=>({...c,incarnations:[1],perIncarnationCounts:[{incarnation:1,blockCount:c.blockCount}],method:row.segmentCountMethod}));row.declaredWorkloadBounds={workCount:work,blockBytes:requested.blockBytes,alignment:8,checkpointCount:pts.length,checkpoints:declared,producerCandidateLimit:m==='producer'?8192:null,producerCandidatesIncluded:false};
    }
    return {code:0,stdout:JSON.stringify(row)+'\n',stderr:''};
  }};
}
async function syntheticPipeline(ctx) {
  const capture=await pipeline(ctx);assert.equal(capture.measurements,0);const reviewFile=resolve(ctx.out,'synthetic-identity.json');const reviewedAt=new Date(Date.parse(capture.capturedAt)+1).toISOString();writeFileSync(reviewFile,json({status:'reviewed',reviewer:'synthetic fixture, not independent approval',captureSha256:capture.capture.sha256,linkage:capture.linkage,reviewedAt,authorization:'full-group-and-native'}),{flag:'wx'});
  // Strict chronology is real even in fixtures; use the next event-loop tick.
  await new Promise(resolve=>setTimeout(resolve,5));return runCaptured({...ctx,identityReviewFile:reviewFile});
}
export async function legacySelfTest() {
  const base=resolve(ROOT,'target','prf01-selftest-'+randomUUID());mkdirSync(dirname(base),{recursive:true});let checks=0;
  try {const arms=fixtureRepos(base),out=resolve(base,'out');const result=await syntheticPipeline({arms,out,transport:fixtureTransport(),synthetic:true});assert.equal(result.verdict,'INCOMPLETE');checks++;
    const review=resolve(base,'review.json');writeFileSync(review,json({status:'reviewed',reviewer:'synthetic only',linkage:result.linkage,featureExactReceipt:'synthetic',identityReviewReceipt:'synthetic',inventoryReceipt:'synthetic',nativeSharedAllocationPathReview:'synthetic',producerProfileReview:'synthetic',segmentCountInterpretationReview:'synthetic'}));const success=aggregate(out,{reviewFile:review,synthetic:true});assert.equal(success.verdict,'GO',success.reason);checks++;
    const path=local(out,'bundle.json'),original=readFileSync(path);
    const mutate=(fn,pattern)=>{const b=JSON.parse(original);fn(b);writeFileSync(path,json(b));assert.throws(()=>aggregate(out,{synthetic:true}),pattern);writeFileSync(path,original);checks++;};
    mutate(b=>b.processes.pop(),/missing process/);mutate(b=>b.links[1].hit=b.links[0].hit,/one process two rows/);mutate(b=>b.builds['C0-iai'].sourceArm='B',/C0 source B|captured build identity drift/);mutate(b=>b.builds['C0-iai'].binarySha256=b.builds['A-iai'].binarySha256,/binary drift|same C0hash|captured build identity drift/);mutate(b=>b.processes.find(p=>p.role==='iai').name='wrong',/wrong cell name/);mutate(b=>b.telemetry.pop(),/telemetry schedule missing/);mutate(b=>b.processes.find(p=>p.role==='iai').childIr=100000,/profile\/envelope mismatch/);
    const mutateRaw=(pick,fn,pattern)=>{
      const b=JSON.parse(original),p=b.processes.find(pick),rawPath=local(out,p.raw.path),oldRaw=readFileSync(rawPath),procPath=local(out,p.process.path),oldProc=readFileSync(procPath),proc=JSON.parse(oldProc),stdoutPath=local(out,proc.stdout.path),oldStdout=readFileSync(stdoutPath);
      const row=JSON.parse(oldRaw);fn(row);const bytes=JSON.stringify(row)+'\n';writeFileSync(rawPath,bytes);writeFileSync(stdoutPath,bytes);p.raw.sha256=sha(bytes);proc.stdout.sha256=sha(bytes);writeFileSync(procPath,json(proc));p.process.sha256=sha(json(proc));writeFileSync(path,json(b));
      assert.throws(()=>aggregate(out,{synthetic:true}),pattern);checks++;writeFileSync(rawPath,oldRaw);writeFileSync(stdoutPath,oldStdout);writeFileSync(procPath,oldProc);writeFileSync(path,original);
    };
    mutateRaw(p=>p.role==='oracle'&&p.args[0]==='growth16_n',r=>r.oracle.routed_scans=0,/routed scan missing/);
    mutateRaw(p=>p.role==='oracle'&&p.args[0]==='multiseg_cold_256k',r=>{r.oracle.pending_words_visited=1;r.oracle.pending_words_exchanged=1;},/frozen count tolerance/);
    mutateRaw(p=>p.role==='oracle',r=>r.config.resolved.cacheSlots=9,/wrong resolved config/);
    mutateRaw(p=>p.role==='oracle',r=>r.mode='wrong',/wrong oracle mode/);
    mutateRaw(p=>p.role==='native',r=>r.config.resolved.segmentCount=1,/fabricated readback/);
    mutateRaw(p=>p.role==='twin',r=>r.geometry.routeIncarnation=0,/AssertionError/);
    const binary=JSON.parse(original).builds['A-iai'].binary,pb=local(out,binary.path),oldBinary=readFileSync(pb);writeFileSync(pb,'binary drift');assert.throws(()=>aggregate(out,{synthetic:true}),/byte\/hash drift/);checks++;writeFileSync(pb,oldBinary);
    assert.throws(()=>ancestry('a'.repeat(40),'b'.repeat(40),'b'.repeat(40)+'\n'),/not ancestor/);checks++;
    const sourcePath=local(arms.A,'src/fixture.rs'),oldSource=readFileSync(sourcePath);writeFileSync(sourcePath,'source drift');assert.throws(()=>snapshot(arms.A),/clean committed/);checks++;writeFileSync(sourcePath,oldSource);

    const name='test_cell', cell={name,index:7}, log='==1== Callgrind\n==1== Command: perf_gate_iai --iai-run perf_gate 7 0\n';
    const debug=`callgrind: Arguments: ${PRODUCER_SETTINGS.filter(x=>x!=='--separate-threads=yes').join(' ')} --separate-threads=yes --collect-atstart=no --callgrind-out-file=/evidence/${name}/callgrind.out --toggle-collect=${SYMBOL}\n`;
    const head=tid=>`pid: 1\nthread: ${tid}\npart: 1\ncmd: perf_gate_iai --iai-run perf_gate 7 0\npositions: line instr\nevents: Dr Ir\n`;
    const parent=head(1)+'fn=(1) parent\n1 100 0 10\ntotals: 0 10\n';
    const incoming=`fn=(9) std::thread::spawn::closure\n1 100 0 0\ncfn=(1) ${SYMBOL}\ncalls=1 1 100\n* * 0 25\n`;const child=head(2)+incoming+`fn=(1) ${SYMBOL}\n1 100 0 2\n+1 +4 0 3\ncfn=(2) SeferAlloc::dealloc\ncalls=4096 1 100\n* * 0 20\nfn=(2)\n1 100 0 20\ntotals: 0 25\n`;
    const trailingZero=child.replace('+1 +4 0 3','+1 +4 0').replace('* * 0 25','* * 0 22').replace('totals: 0 25','totals: 0 22');assert.equal(parseProfiles([parent,trailingZero],log,{name,cell,producer:true,debug}).childIr,22);checks++;
    const forward=child.replace('cfn=(2) SeferAlloc::dealloc','cfn=(2)').replace('fn=(2)\n','fn=(2) SeferAlloc::dealloc\n');assert.equal(parseProfiles([parent,forward],log,{name,cell,producer:true,debug}).childIr,25);checks++;
    const safeDebug=publicText(debug);assert.ok(safeDebug.includes('/test_cell/'));assert.equal(safeDebug.length,debug.length);assert.equal(parseProfiles([parent,child],log,{name,cell,producer:true,debug:safeDebug}).childIr,25);checks++;
    const absoluteExecutable='"/synthetic fixture/bin/allocator \'quoted\' executable"';
    const absoluteLog=log.replace('Command: perf_gate_iai','Command: '+absoluteExecutable);
    const retainedLog=publicText(absoluteLog),retainedProfiles=[parent,child].map(s=>publicText(s.replace('cmd: perf_gate_iai','cmd: '+absoluteExecutable))),retainedDebug=publicText(debug);
    assert.equal(publicText(retainedLog),retainedLog);assert.equal(publicText(retainedDebug),retainedDebug);
    const initial=parseProfiles(retainedProfiles,retainedLog,{name,cell,producer:true,debug:retainedDebug});
    const reread=parseProfiles(retainedProfiles.map(s=>Buffer.from(s).toString()),Buffer.from(retainedLog).toString(),{name,cell,producer:true,debug:retainedDebug});
    assert.deepEqual(initial,reread);assert.ok(!JSON.stringify(initial).includes('/synthetic fixture'));assert.ok(initial.measuredArguments.includes('"<private>"'));checks++;
    const parsed=parseProfiles([parent,child],log,{name,cell,producer:true,debug});assert.equal(parsed.childIr,25);assert.equal(parsed.totalIr,35);checks++;
    assert.throws(()=>parseProfiles([parent,child.replace('totals: 0 25','totals: 0 1')],log,{name,cell,producer:true,debug}),/childIr exceeds totalIr|self costs\/final/);checks++;
    assert.throws(()=>parseProfiles([parent],log,{name,cell,producer:true,control:true,debug}),/child profile absent/);checks++;
    assert.throws(()=>parseProfiles([parent,child],log,{name,cell,producer:true,control:true,debug}),/control outgoing work/);checks++;
    assert.throws(()=>parseProfiles([parent,child.replaceAll(SYMBOL,'other_function')],log,{name,cell,producer:true,debug}),/exact producer/);checks++;
    assert.throws(()=>parseProfiles([parent,child],log,{name,cell,producer:true,debug:debug.replace(SYMBOL,'broad')}),/argument record/);checks++;
    assert.throws(()=>parseProfiles([parent,child],log,{name,cell,producer:true,debug:debug.replace('separate-threads=yes','separate-threads=no')}),/separate_threads/);checks++;
    assert.throws(()=>parseProfiles([parent,child.replace('calls=4096 1 100','calls=4096 1 100\nfn=(3) invalid')],log,{name,cell,producer:true,debug}),/missing call cost/);checks++;
    assert.throws(()=>rejectEnvironment({CARGO_PROFILE_BENCH_DEBUG:'1'}),/inherited/);checks++;assert.throws(()=>assertEqualShape('a','bb'),/identical length/);checks++;
    const failures=['process-failure','source-drift-build','same-C0','nonancestor'];
    for(const failure of failures){const scenario=fixtureRepos(resolve(base,failure)),transport=fixtureTransport(),run=transport.execute.bind(transport);if(failure==='nonancestor'){
        const gd=local(scenario.B,'.git'),tree=git(scenario.B,['rev-parse','HEAD^{tree}']).toString().trim();
        const body=Buffer.from(`tree ${tree}\nauthor Fixture <fixture@example.invalid> 1 +0000\ncommitter Fixture <fixture@example.invalid> 1 +0000\n\nUnrelated fixture\n`);
        const object=Buffer.concat([Buffer.from(`commit ${body.length}\0`),body]),oid=createHash('sha1').update(object).digest('hex');
        mkdirSync(resolve(gd,'objects',oid.slice(0,2)),{recursive:true});writeFileSync(resolve(gd,'objects',oid.slice(0,2),oid.slice(2)),deflateSync(object),{flag:'wx'});writeFileSync(resolve(gd,'refs/heads/fixture'),oid+'\n');
      }
      transport.execute=async r=>{if(failure==='process-failure'&&r.type==='observation')return {code:7,stdout:'',stderr:'fixture failure'};const result=await run(r);if(failure==='source-drift-build'&&r.type==='build')writeFileSync(local(r.cwd,'src/fixture.rs'),'changed during build');if(failure==='same-C0'&&r.type==='build'&&r.arm==='C0'&&r.role==='iai')writeFileSync(JSON.parse(result.stdout).executable,readFileSync(local(scenario.A,'target/prf01-A-iai/release/perf_gate_iai')));return result;};
      await assert.rejects(syntheticPipeline({arms:scenario,out:resolve(base,'failed-'+failure),transport,synthetic:true}),failure==='process-failure'?/child failed/:failure==='source-drift-build'?/clean committed/:failure==='nonancestor'?/not ancestor/:/same C0hash/);checks++;
    }
    for(const failure of ['missing-review','stale-review','late-review','wrong-reference','role-drift','rebuilt-stat','source-drift','environment-drift','missing-control']){
      const scenario=fixtureRepos(resolve(base,'identity-'+failure)),transport=fixtureTransport(),destination=resolve(base,'identity-out-'+failure);let samples=0;const originalExecute=transport.execute.bind(transport);transport.execute=async r=>{if(['observation','iai'].includes(r.type))samples++;return originalExecute(r);};
      if(failure==='missing-control')transport.execute=async r=>r.type==='symbols'&&r.role==='control'?{code:0,stdout:'empty control symbol optimized out\n',stderr:''}:originalExecute(r);
      const cap=await pipeline({arms:scenario,out:destination,transport,synthetic:true});assert.equal(samples,0);const reviewPath=resolve(destination,'approval.json'),review={status:'reviewed',reviewer:'synthetic',captureSha256:cap.capture.sha256,linkage:cap.linkage,reviewedAt:new Date(Date.parse(cap.capturedAt)+1).toISOString(),authorization:'full-group-and-native'};
      if(failure==='stale-review')review.reviewedAt=cap.capturedAt;if(failure==='late-review')review.reviewedAt='2099-01-01T00:00:00.000Z';if(failure==='wrong-reference')review.captureSha256='0'.repeat(64);if(failure==='role-drift')review.linkage.binaries['A-iai'].sourceArm='B';
      if(failure==='rebuilt-stat'){const capture=JSON.parse(readFileSync(local(destination,'capture.json'))),p=local(scenario.A,capture.builds['A-iai'].binaryPath);writeFileSync(p,readFileSync(p));}if(failure==='source-drift')writeFileSync(local(scenario.A,'src/fixture.rs'),'drift');if(failure==='environment-drift')transport.execute=async r=>{const x=await originalExecute(r);if(r.type==='environment')x.stdout+='changed CPU\n';if(['observation','iai'].includes(r.type))samples++;return x;};
      if(failure!=='missing-review')writeFileSync(reviewPath,json(review));await new Promise(resolve=>setTimeout(resolve,5));await assert.rejects(runCaptured({out:destination,transport,identityReviewFile:reviewPath,synthetic:true}),/ENOENT|chronology|reference mismatch|role digest mismatch|stat drift|clean committed|environment drift|control exact symbol absent/);assert.equal(samples,0,'identity failure launched workload');checks++;
    }
    mutateRaw(p=>p.role==='oracle'&&p.args[0]==='publication',r=>r.checkpoints.pop(),/checkpoint coverage/);
    mutateRaw(p=>p.role==='oracle'&&p.args[0]==='producer',r=>r.checkpoints[0].blockCount=8192,/checkpoint bounds/);
    mutateRaw(p=>p.role==='oracle'&&p.args[0]==='growth16_n',r=>r.checkpoints[0].perIncarnationCounts[0].blockCount--,/checkpoint block sum/);
    mutateRaw(p=>p.role==='oracle'&&p.args[0]==='seg_cycle_decommit_256k',r=>r.segmentIncarnations=[1,2],/checkpoint union/);
    mutateRaw(p=>p.role==='oracle'&&p.args[0]==='growth16_2n',r=>{r.config.requested.segmentCount=2;r.config.resolved.segmentCount=2;},/union cardinality/);
    const huge='positions: instr line\nevents: Ir Dr\nfn=(1) huge\n0x20000000000001 0x10 0x20000000000001 0\n+0x4 -1 2 0\n* * 3\n';assert.equal(profileFunctions(huge).get('huge').selfIr,9007199254740998n);checks++;
    for(const malformed of [huge.replace('+0x4','+oops'),huge.replace('3\n','-1\n'),huge.replace('0x10','*'),huge.replace('3\n','3 0 1\n'),huge.replace('0x20000000000001 0x10','0x20000000000001 -0x11')]){assert.throws(()=>profileFunctions(malformed),/malformed|relative position|extra event|negative position/);checks++;}
    assert.throws(()=>parseProfiles([parent,child],log.replace('perf_gate 7 0','perf_gate 8 0'),{name,cell,producer:true,debug}),/dispatcher index mismatch/);checks++;
    assert.throws(()=>collectionEquivalent(parsed,{...parsed,toolVector:[...parsed.toolVector,'--cache-sim=no']},readFileSync(local(ROOT,'benches/support/r19_prf01.rs'),'utf8')),/collection argv changed/);checks++;
    mutateRaw(p=>p.role==='oracle'&&p.args[0]==='publication',r=>{r.segmentCountMethod='union Small/Primordial fake';r.checkpoints.forEach(c=>c.method=r.segmentCountMethod);},/exact geometry method mismatch/);checks++;
    const goodCapture=JSON.parse(readFileSync(local(out,'capture.json')));for(const mutation of [c=>delete c.builds['B-twin'],c=>c.builds['A-iai'].sourceArm='B',c=>c.builds['A-iai'].cfg=['r19_sidecar_summary_oracle']]){const c=structuredClone(goodCapture);mutation(c);c.linkage=captureLink({sources:c.sources,builds:c.builds,env:c.environment,invRef:c.inventory});assert.throws(()=>captureBounds(out,c),/capture role inventory|source arm mismatch|AssertionError/);checks++;}
    const quotedDebug=debug.replace('--callgrind-out-file=/evidence/test_cell/callgrind.out','--callgrind-out-file="/private machine/test_cell/callgrind.out"');assert.ok(publicText(quotedDebug).includes('/test_cell/callgrind.out'));assert.ok(!publicText(quotedDebug).includes('/private machine'));checks++;
    const parsedModule=parseProfiles([parent,child].map(s=>s.replace('perf_gate 7 0\n','perf_gate 7 0 perf_gate_iai::perf_gate::test_cell\n')),log.replace('perf_gate 7 0\n','perf_gate 7 0 perf_gate_iai::perf_gate::test_cell\n'),{name,cell,producer:true,debug});assert.equal(parsedModule.childIr,25);checks++;
    assert.throws(()=>safeNumber(9007199254740993n),/safe judge integer/);checks++;
    const badTelemetry=JSON.parse(original),tr=badTelemetry.telemetry[0],tp=local(out,tr.path),tb=readFileSync(tp),tv=JSON.parse(tb);tv.hostCpuLoadPercent=null;writeFileSync(tp,json(tv));tr.sha256=sha(json(tv));writeFileSync(path,json(badTelemetry));assert.throws(()=>aggregate(out,{synthetic:true}),/host CPU unavailable/);writeFileSync(tp,tb);writeFileSync(path,original);checks++;
    const diagSources={macros:'//! crate docs: the real macros.rs starts with a doc comment, i.e. a JSON string value starting with a slash\nargs_iter --iai-run $group::__run(group_index, bench_index)',libBench:'EntryPoint::Custom insert_toggle_collect OsString::from(self.bench_index.to_string()) OsString::from(self.index.to_string()) OsString::from(self.module_path.to_string())',tool:'let args = tool_args.to_vec()',args:'i1: String::from("32768,8,64"), d1: String::from("32768,8,64"), ll: String::from("8388608,16,64"), cache_sim: true, compress_strings: false, compress_pos: false, dump_line: true, dump_instr: false, combine_dumps: false, separate_threads: true, trace_children: true, verbose: log_enabled!(log::Level::Debug), fair_sched: FairSched::Try,',toolArgs:'pub fn to_vec vec.push(format!("--tool= --error-exitcode= --trace-children= --fair-sched= --verbose vec.extend(self.other'};
    for(const outcome of ['primary-missing','control-missing','nm-failure','approve','parse-failure','wrong-approval']){
      const scenario=fixtureRepos(resolve(base,'preflight-'+outcome)),transport=fixtureTransport(),destination=resolve(base,'preflight-cap-'+outcome),cap=await pipeline({arms:scenario,out:destination,transport,synthetic:true}),diag=resolve(base,'preflight-diag-'+outcome);let launches=0,nms=0;const originalExecute=transport.execute.bind(transport);
      transport.execute=async r=>{if(r.type==='dispatch-source')return {code:0,stdout:JSON.stringify(diagSources),stderr:''};if(r.type==='preflight-nm'){nms++;return {code:outcome==='nm-failure'&&r.role==='iai'?2:0,stdout:(outcome==='primary-missing'&&r.role==='iai'||outcome==='control-missing'&&r.role==='control')?'optimized out\n':'1 T '+SYMBOL+'\n',stderr:''};}if(r.type==='producer-diagnostic'){launches++;const tokens=argvTokens(r.script),output=tokens.find(x=>x.startsWith('--callgrind-out-file=')).slice('--callgrind-out-file='.length).replace('%p','42'),logPath=tokens.find(x=>x.startsWith('--log-file=')).slice('--log-file='.length).replace('%p','42'),binaryIndex=tokens.findIndex(x=>x.startsWith('./target/')),command=tokens.slice(binaryIndex).join(' '),root=dirname(local(r.cwd,output)),header=tid=>`pid: 42\nthread: ${tid}\npart: 1\ncmd: ${command}\npositions: line\nevents: Ir\n`;mkdirSync(root,{recursive:true});writeFileSync(local(r.cwd,output),'');writeFileSync(local(r.cwd,output)+'.1',header(1)+'fn=(1) parent\n1 10\ntotals: 10\n');writeFileSync(local(r.cwd,output)+'.2',header(2)+`fn=(9) std::thread::spawn::closure\n1 0\ncfn=(1) ${SYMBOL}\ncalls=1 1\n* ${r.role==='iai'?21:8}\n`+(r.role==='iai'?(outcome==='parse-failure'?'fn=(1) wrong_boundary\n1 21\ntotals: 21\n':`fn=(1) ${SYMBOL}\n1 1\ncfn=(2) SeferAlloc::dealloc\ncalls=4096 1\n* 20\nfn=(2)\n1 20\ntotals: 21\n`):`fn=(1)\n1 8\ntotals: 8\n`));writeFileSync(local(r.cwd,logPath),`==42== Command: ${command}\n`);return {code:0,stdout:'diagnostic synthetic\n',stderr:''};}return originalExecute(r);};
      const options={captureFile:local(destination,'capture.json'),transport,currentRoot:scenario.A};const planned=await producerPreflight(diag,{...options,planOnly:true});assert.equal(nms,2);assert.equal(launches,0);assert.equal(planned.measurements,0);
      if(['primary-missing','control-missing','nm-failure'].includes(outcome)){assert.equal(planned.verdict,'INCOMPLETE');assert.equal(planned.outcomes.length,2);await assert.rejects(producerPreflight(diag,options),/exact boundary unavailable/);assert.equal(launches,0);checks++;continue;}
      assert.equal(planned.verdict,'PLAN_ONLY');const plan=getJson(diag,planned.plan),approval=resolve(diag,'approval.json');writeFileSync(approval,json({status:'reviewed',reviewer:'synthetic only',authorization:'one-primary-one-control-diagnostic',planSha256:outcome==='wrong-approval'?'0'.repeat(64):planned.plan.sha256,reviewedAt:new Date(Date.parse(plan.capturedAt)+1).toISOString(),argv:plan.argv,sourceReview:'fixture',hashReview:'fixture',nmReview:'fixture',argvReview:'fixture',disassemblyReview:'fixture'}));await new Promise(resolve=>setTimeout(resolve,5));
      if(outcome==='wrong-approval'){await assert.rejects(producerPreflight(diag,{...options,approvalFile:approval}),/AssertionError/);assert.equal(launches,0);}else {const result=await producerPreflight(diag,{...options,approvalFile:approval});assert.equal(result.verdict,outcome==='parse-failure'?'INCOMPLETE':'DIAGNOSTIC_ONLY',JSON.stringify({outcome,results:result.results}));assert.equal(result.results.length,2);if(outcome==='parse-failure'){assert.equal(result.results[0].finding.status,'INCOMPLETE');assert.equal(result.results[1].finding.status,'PARSED');}else {assert.ok(result.results.every(r=>r.finding.status==='PARSED'));assert.equal(result.results[0].finding.parsed.childIr,21);}assert.equal(result.results[1].finding.parsed.childIr,8);assert.equal(result.judgeRows,0);assert.equal(launches,2);await assert.rejects(producerPreflight(diag,{...options,approvalFile:approval}),/consumed/);assert.equal(launches,2);}checks++;
    }
    const preparedScenario=fixtureRepos(resolve(base,'standalone')),preparedTransport=fixtureTransport(),preparedExecute=preparedTransport.execute.bind(preparedTransport);let preparedBuilds=0,preparedWorkloads=0;preparedTransport.execute=async r=>{if(r.type==='build')preparedBuilds++;if(['iai','observation','producer-diagnostic'].includes(r.type))preparedWorkloads++;if(r.type==='dispatch-source')return {code:0,stdout:JSON.stringify(diagSources),stderr:''};if(r.type==='preflight-nm')return {code:0,stdout:r.role==='iai'?'1 T '+SYMBOL+'\n':'optimized out\n',stderr:''};return preparedExecute(r);};writeFileSync(local(preparedScenario.A,'benches/support/r19_prf01.rs'),readFileSync(local(preparedScenario.A,'benches/support/r19_prf01.rs'),'utf8')+'\n// dirty diagnostic fixture\n');writeFileSync(local(preparedScenario.A,'new-input.txt'),'untracked diagnostic fixture');
    const preparedOut=local(preparedScenario.A,'target/standalone-plan'),prepared=await prepareProducer(preparedOut,{transport:preparedTransport,currentRoot:preparedScenario.A});assert.equal(prepared.verdict,'INCOMPLETE');assert.equal(preparedBuilds,2);assert.equal(preparedWorkloads,0);const preparedCap=getJson(preparedOut,prepared.capture);assert.deepEqual(Object.keys(preparedCap.sources),['A']);assert.deepEqual(Object.keys(preparedCap.builds).sort(),['A-control','A-iai']);assert.ok(get(preparedOut,preparedCap.sources.A.patch).length>0);assert.ok(getJson(preparedOut,preparedCap.sources.A.untracked).some(r=>r[0]==='new-input.txt'));assert.equal(prepared.workloadProcesses,0);checks++;
    const judgeFiles=readdirSync(out).filter(n=>n.startsWith('judge-input-'));assert.equal(judgeFiles.length,1);const judgeInput=JSON.parse(readFileSync(local(out,judgeFiles[0])));for(const change of [c=>c.requested.segmentCount=c.resolved.segmentCount=1,c=>delete c.unobservedReasons.segmentCount,c=>delete c.resolvedProvenance,c=>c.declaredProvenance.alignment=16,c=>c.resolvedProvenance.copiedFields.push('segmentCount')]){const input=structuredClone(judgeInput);change(input.iai.find(r=>HOT.includes(r.name)).config);const name='negative-hot-'+checks+'.json';putJson(out,name,input);const result=spawnSync(process.execPath,[local(ROOT,JUDGE),'--input',local(out,name)],{encoding:'utf8',cwd:ROOT});assert.equal(result.status,1);assert.notEqual(JSON.parse(result.stdout).verdict,'GO');checks++;}
    assert.throws(()=>inventory(readFileSync(local(ROOT,'benches/perf_gate_iai.rs'),'utf8').replace('const CHURN_OPS: usize = 64;','const CHURN_OPS: usize = 65;')),/hot source declarations mismatch/);checks++;
    assert.equal(profileFunctions('positions: line instr\nevents: Ir Dr\nfn=(1) caller\n0x10 0x100 1\ncfn=(2) target\ncalls=0x10 +0x1 *\n+1 +0x4 0x20\n').get('caller').edges.get('target'),32n);checks++;
    return checks;
  } finally {rmSync(base,{recursive:true,force:true});}
}
function preflightArgs(source) {
  const s=source.args;const string=k=>{const m=new RegExp(k+': String::from\\("([^"\\n]+)"\\)').exec(s);assert.ok(m,'missing pinned default '+k);return m[1];};const bool=k=>{const m=new RegExp(k+': (true|false),').exec(s);assert.ok(m,'missing pinned default '+k);return m[1]==='true'?'yes':'no';};assert.match(s,/verbose: log_enabled!\(log::Level::Debug\)/);assert.match(s,/fair_sched: FairSched::Try/);assert.match(source.toolArgs,/vec.push\(format!\("--tool=[\s\S]*--error-exitcode=[\s\S]*--trace-children=[\s\S]*--fair-sched=[\s\S]*--verbose[\s\S]*vec.extend\(self.other/);
  return ['--tool=callgrind','--error-exitcode=0','--trace-children='+bool('trace_children'),'--fair-sched=try','--verbose','--I1='+string('i1'),'--D1='+string('d1'),'--LL='+string('ll'),...['cache_sim','compress_strings','compress_pos','dump_line','dump_instr','combine_dumps','separate_threads'].map(k=>'--'+k.replaceAll('_','-')+'='+bool(k)),'--collect-atstart=no','--toggle-collect='+SYMBOL];
}
async function prepareProducer(out,{transport=makeTransport(),currentRoot=ROOT}={}) {
  // Standalone dirty infrastructure diagnostic identity, never a gate measurement commit.
  inside(currentRoot,out);assert.ok(relative(currentRoot,out).replaceAll('\\','/').startsWith('target/'),'diagnostic artifacts must remain under current target');assert.ok(!existsSync(out),'no overwrite');
  const snap=snapshot(currentRoot,{allowDirty:true});mkdirSync(out,{recursive:true});const ctx={out,arms:{A:currentRoot},transport,diagnostic:true,prepareId:randomUUID(),snapshots:{A:snap},sources:{A:retainSource(out,'A',snap)}};
  const hashes=frozen(snap.bytes[PREREG],snap.bytes[JUDGE],snap.bytes[FREEZE]);ctx.inv=inventory(snap.bytes['benches/perf_gate_iai.rs']);ctx.invRef=putJson(out,'inventory.json',ctx.inv);const env=await environment(ctx,'A');ctx.env=putJson(out,'environment.json',{...env,arms:{A:env}});ctx.builds={};
  for(const role of ['iai','control'])ctx.builds['A-'+role]=await build(ctx,'A',role);const p=ctx.builds['A-iai'],c=ctx.builds['A-control'];assertEqualShape(dirname(p.binaryPath),dirname(c.binaryPath));assert.deepEqual(p.features,c.features);sourceStable(snap.source,snapshot(currentRoot,{allowDirty:true}).source);for(const b of [p,c])stable(ctx,b);
  const capture={version:1,diagnosticOnly:true,neverJudgeReceipt:true,capturedAt:now(),hashes,sources:ctx.sources,builds:ctx.builds,environment:ctx.env,inventory:ctx.invRef,linkage:captureLink(ctx)};const ref=putJson(out,'capture.json',capture);
  const planned=await producerPreflight(local(out,'plan'),{captureFile:local(out,'capture.json'),planOnly:true,transport,currentRoot});return {...planned,capture:ref,identityKind:'base+canonical patch+untracked/source manifest; not measurementCommit',workloadProcesses:0};
}
async function producerPreflight(out,{captureFile,approvalFile,planOnly,transport=makeTransport(),currentRoot=ROOT}={}) {
  // Diagnostic only: consumes existing captured A binaries, never builds or dispatches a group.
  text(captureFile);const captureBytes=readFileSync(captureFile),cap=JSON.parse(captureBytes),digest=sha(captureBytes),captureDir=dirname(captureFile);assert.equal(cap.version,1);assert.ok(cap.sources.A&&cap.builds['A-iai']&&cap.builds['A-control'],'A-only captured source and role identities required');
  const ctx={out,arms:{A:currentRoot},transport};assert.deepEqual(Object.keys(cap.builds).filter(k=>k.startsWith('A-')&&['iai','control'].includes(k.slice(2))).sort(),['A-control','A-iai']);const builds=['iai','control'].map(role=>cap.builds['A-'+role]);for(const b of builds){assert.equal(b.sourceArm,'A');assert.equal(b.arm,'A');assert.equal(b.name,'perf_gate_iai');feature(b.features,PRIMARY);assert.deepEqual(b.cfg,b.role==='control'?['r19_prf01_producer_control']:[]);assert.equal(b.environmentSha256,cap.environment.sha256);assert.equal(b.sourceIdentitySha256,cap.sources.A.identitySha256);sourceStable(cap.sources.A,snapshot(currentRoot,{allowDirty:true}).source);stable({...ctx,out:captureDir},b);}
  const inv=getJson(captureDir,cap.inventory),cell=inv.cells.find(c=>c.name===PRODUCER);assert.ok(cell);
  if(planOnly){assert.ok(!existsSync(out),'no overwrite');mkdirSync(out,{recursive:true});
    const r=await execute(ctx,{cwd:currentRoot,type:'dispatch-source',arm:'A',script:'python3 -c '+quote('import pathlib,json; r=pathlib.Path.home()/".cargo/registry/src"; specs={"macros":"iai-callgrind-0.14.2/src/macros.rs","libBench":"iai-callgrind-runner-0.14.2/src/runner/lib_bench.rs","tool":"iai-callgrind-runner-0.14.2/src/runner/tool/mod.rs","args":"iai-callgrind-runner-0.14.2/src/runner/callgrind/args.rs","toolArgs":"iai-callgrind-runner-0.14.2/src/runner/tool/args.rs"}; out={};\nfor k,s in specs.items():\n p=list(r.glob("*/"+s)); assert len(p)==1; out[k]=p[0].read_text()\nprint(json.dumps(out))')});
    const source=JSON.parse(r.stdout);assert.match(source.macros,/args_iter[\s\S]*--iai-run[\s\S]*\$group::__run\(group_index, bench_index\)/);assert.match(source.libBench,/EntryPoint::Custom[\s\S]*insert_toggle_collect/);assert.match(source.tool,/let args = tool_args.to_vec\(\)/);assert.match(source.args,/separate_threads: true/);assert.match(source.toolArgs,/pub fn to_vec/);
    const sourceHashes=Object.fromEntries(Object.entries(source).map(([k,v])=>[k,sha(v)]));
    assert.match(source.libBench,/OsString::from\(self.bench_index.to_string\(\)\)[\s\S]*OsString::from\(self.index.to_string\(\)\)[\s\S]*OsString::from\(self.module_path.to_string\(\)\)/);
    assert.match(get(captureDir,cap.sources.A.blobs['benches/perf_gate_iai.rs']).toString(),/setup = prf01_support::R19Prf01::producer_setup,[\s\S]*EntryPoint::Custom\([\s\S]*producer_free_window[\s\S]*fn r19_prf01_joined_producer_same_word/);
    const defaults=preflightArgs(source);
    assert.match(source.args,/i1: String::from\("32768,8,64"\)/);assert.match(source.args,/d1: String::from\("32768,8,64"\)/);assert.match(source.args,/ll: String::from\("8388608,16,64"\)/);
    // Collect-atstart is a Valgrind consequence of toggle-collect, not invented runner args.
    const outcomes=[];for(const b of builds){const e=await boundaryEvidence({...ctx,out,captureOut:captureDir},b,{retainFailure:true,type:'preflight'});outcomes.push({role:b.role,binarySha256:b.binarySha256,stat:b.stat,receipt:e.nm,boundaryReceipt:e.receipt,disassemblyReceipt:e.disassembly,exactSymbolPresent:!!e.verification,reason:e.reason});}
    const plan={version:1,diagnosticOnly:true,neverJudgeReceipt:true,capturedAt:now(),captureSha256:digest,sourceIdentitySha256:cap.sources.A.identitySha256,inventorySha256:cap.inventory.sha256,sourceHashes,sourceReceipt:putJson(out,'dispatcher-source.json',source),sourceProcessReceipt:r.ref,dispatcher:{group:'perf_gate',functionIndex:cell.index,benchIndex:0},outcomes,argv:builds.map(b=>({role:b.role,binarySha256:b.binarySha256,argv:['valgrind',...defaults,'--callgrind-out-file=target/prf01-preflight-'+digest+'/'+b.role+'/'+PRODUCER+'/callgrind.out.#%p','--log-file=target/prf01-preflight-'+digest+'/'+b.role+'/'+PRODUCER+'/callgrind.log.#%p','./'+b.binaryPath,'--iai-run','perf_gate',String(cell.index),'0','perf_gate_iai::perf_gate::'+PRODUCER]})),launchable:outcomes.every(x=>x.exactSymbolPresent),maxProcesses:2,retries:0};const ref=putJson(out,'preflight-plan.json',plan);return {verdict:plan.launchable?'PLAN_ONLY':'INCOMPLETE',plan:ref,outcomes,reason:plan.launchable?'independent source/hash/nm/argv approval required':'exact primary/control symbol missing; both outcomes retained; no launch',measurements:0};
  }
  assert.ok(!existsSync(local(out,'preflight-launch.json')),'preflight consumed; no retry');const planBytes=readFileSync(local(out,'preflight-plan.json')),plan=JSON.parse(planBytes);assert.equal(plan.captureSha256,digest);assert.ok(plan.launchable,'exact boundary unavailable; no launch');text(approvalFile);const review=JSON.parse(readFileSync(approvalFile));assert.equal(review.status,'reviewed');text(review.reviewer);assert.equal(review.authorization,'one-primary-one-control-diagnostic');assert.equal(review.planSha256,sha(planBytes));assert.ok(Date.parse(plan.capturedAt)<Date.parse(review.reviewedAt)&&Date.parse(review.reviewedAt)<Date.now(),'preflight review chronology');assert.deepEqual(review.argv,plan.argv);for(const field of ['sourceReview','hashReview','nmReview','argvReview','disassemblyReview'])text(review[field]);
  const retainedSource=getJson(out,plan.sourceReceipt);assert.deepEqual(Object.fromEntries(Object.entries(retainedSource).map(([k,v])=>[k,sha(v)])),plan.sourceHashes,'preflight source hash drift');const defaults=preflightArgs(retainedSource);assert.equal(plan.sourceIdentitySha256,cap.sources.A.identitySha256);assert.equal(plan.inventorySha256,cap.inventory.sha256);assert.deepEqual(plan.dispatcher,{group:'perf_gate',functionIndex:cell.index,benchIndex:0});assert.deepEqual(plan.argv,builds.map(b=>({role:b.role,binarySha256:b.binarySha256,argv:['valgrind',...defaults,'--callgrind-out-file=target/prf01-preflight-'+digest+'/'+b.role+'/'+PRODUCER+'/callgrind.out.#%p','--log-file=target/prf01-preflight-'+digest+'/'+b.role+'/'+PRODUCER+'/callgrind.log.#%p','./'+b.binaryPath,'--iai-run','perf_gate',String(cell.index),'0','perf_gate_iai::perf_gate::'+PRODUCER]})),'preflight argv/source/build drift');assert.deepEqual(plan.outcomes.map(x=>x.role),['iai','control']);for(const outcome of plan.outcomes){const b=builds.find(b=>b.role===outcome.role);checkBoundaryEvidence(out,outcome.boundaryReceipt,b);const proc=getJson(out,outcome.receipt);assert.equal(proc.code,0);assert.equal(proc.role,outcome.role);assert.equal(outcome.binarySha256,b.binarySha256);assert.deepEqual(outcome.stat,b.stat);assert.ok(get(out,proc.stdout).toString().split(/\r?\n/).some(l=>l.endsWith(' '+SYMBOL)),'preflight exact nm boundary unavailable');}
  const env=getJson(captureDir,cap.environment);const observedEnv=await environment(ctx,'A');assert.equal(observedEnv.description,env.arms.A.description,'preflight environment drift');
  const ref=putJson(out,'preflight-approved.json',review);putJson(out,'preflight-launch.json',{at:now(),approval:ref});const results=[];
  for(const job of plan.argv){
    const b=builds.find(b=>b.role===job.role);stable({...ctx,out:captureDir},b);sourceStable(cap.sources.A,snapshot(currentRoot,{allowDirty:true}).source);const privateHome=local(currentRoot,'target/prf01-preflight-'+digest+'/'+job.role+'/'+PRODUCER);mkdirSync(privateHome,{recursive:true});
    const fresh=await boundaryEvidence({...ctx,captureOut:captureDir},b,{retainFailure:true});if(!fresh.verification){results.push({role:job.role,finding:{status:'INCOMPLETE',reason:fresh.reason},boundaryReceipt:fresh.receipt});putJson(out,'preflight-result-'+job.role+'.json',results.at(-1));continue;}
    let result;try{result=await execute(ctx,{cwd:currentRoot,type:'producer-diagnostic',arm:'A',role:job.role,retainFailure:true,script:job.argv.map(quote).join(' ')});}catch(error){result={code:null,ref:null,receipt:null,error:error.message};}
    const files=walk(privateHome),raw=files.map((p,i)=>{const bytes=readFileSync(p);return {originalSha256:sha(bytes),public:put(out,'diagnostic/'+job.role+'/'+i+'.log',bytes.toString(),true)};});let finding;
    try {
      assert.equal(result.code,0,'diagnostic process failed');const profiles=files.filter(p=>/\.out(?:[.#-]+\d+)*$/.test(p)&&statSync(p).size>0).map(p=>publicText(readFileSync(p,'utf8'))),logs=files.filter(p=>/\.log(?:[.#-]+\d+)*$/.test(p)).map(p=>publicText(readFileSync(p,'utf8'))).join('\n');assert.ok(profiles.length,'diagnostic profiles missing');
      const expectedTools=job.argv.slice(1,job.argv.indexOf('./'+b.binaryPath)),debug='callgrind: Arguments: '+expectedTools.map(x=>publicText(x)).join(' ')+'\n';
      // Planned source-derived vector is retained as such, never represented as an actual iai runner Debug log.
      bindMeasuredBinary(files.filter(p=>/\.log(?:[.#-]+\d+)*$/.test(p)).map(p=>readFileSync(p,'utf8')).join('\n'),files.filter(p=>/\.out(?:[.#-]+\d+)*$/.test(p)&&statSync(p).size>0).map(p=>readFileSync(p,'utf8')),b,transport.path(currentRoot));
      const parsed=parseProfiles(profiles,logs,{name:PRODUCER,cell,producer:true,control:job.role==='control',expectedExecutable:publicText('./'+b.binaryPath),debug});assert.deepEqual(parsed.toolVector,expectedTools.map(x=>publicText(x)),'diagnostic tool argv drift');
      finding={status:'PARSED',parsed,toolArgumentsReceipt:{source:plan.sourceReceipt,planSha256:sha(planBytes),origin:'source-derived direct diagnostic vector; not actual iai receipt'},effectiveCollectAtStart:false,effectiveReason:'exact toggle-collect implies disabled initial collection; actual exact child graph/bounded positive control profiles validated'};
    }catch(error){finding={status:'INCOMPLETE',reason:error.message};}
    results.push({role:job.role,code:result.code,process:result.ref,error:result.error??null,raw,boundaryReceipt:fresh.receipt,finding});putJson(out,'preflight-result-'+job.role+'.json',results.at(-1));
  }
  const complete=results.every(r=>r.finding.status==='PARSED');if(complete)collectionEquivalent(results[0].finding.parsed,results[1].finding.parsed,get(captureDir,cap.sources.A.blobs['benches/support/r19_prf01.rs']).toString());
  const coverage=complete?pairCoverage(results[0].finding.parsed,results[1].finding.parsed):null;
  return {coverage,verdict:complete?'DIAGNOSTIC_ONLY':'INCOMPLETE',reason:complete?'diagnostic validated; never gate evidence':'both outcomes retained; exact child/bounded control unverified or unachievable; STOP, no retry',results,judgeRows:0,processes:2,retries:0};
}
async function main(args) {
  if(args.length===1&&args[0]==='--self-test')return selfTest();
  const mode=args.shift(),out=args.shift();text(out);assert.ok(!out.startsWith('--'));assert.ok(['--capture','--run','--aggregate','--producer-preflight','--prepare-producer-preflight'].includes(mode));const allowed=mode==='--prepare-producer-preflight'?[]:mode==='--capture'?['--arm-a','--arm-b']:mode==='--run'?['--identity-review','--full-group','--arm-a','--arm-b']:mode==='--producer-preflight'?['--capture-receipt','--approval-receipt','--plan-only']:['--review-receipt'];const options={};while(args.length){const key=args.shift();if(key==='--plan-only'||key==='--full-group'){assert.ok(allowed.includes(key)&&!options[key]);options[key]=true;continue;}const value=args.shift();assert.ok(allowed.includes(key)&&value&&!value.startsWith('--')&&!options[key],'unknown/duplicate/mode-incompatible CLI option');options[key]=value;}
  let result;if(mode==='--aggregate')result=aggregate(resolve(out),{reviewFile:options['--review-receipt']&&resolve(options['--review-receipt'])});else {rejectEnvironment(process.env);if(mode==='--prepare-producer-preflight')result=await prepareProducer(resolve(out));else if(mode==='--capture'){text(options['--arm-a']);text(options['--arm-b']);result=await pipeline({out:resolve(out),arms:{A:resolve(options['--arm-a']),B:resolve(options['--arm-b'])},transport:makeTransport()});}else if(mode==='--run'){assert.equal(options['--full-group'],true,'explicit --full-group required; reporting selection is not runtime filtering');text(options['--identity-review']);result=await runCaptured({out:resolve(out),identityReviewFile:resolve(options['--identity-review']),arms:Object.fromEntries(['A','B'].filter(a=>options[a==='A'?'--arm-a':'--arm-b']).map(a=>[a,resolve(options[a==='A'?'--arm-a':'--arm-b'])])),transport:makeTransport()});}else result=await producerPreflight(resolve(out),{captureFile:options['--capture-receipt']&&resolve(options['--capture-receipt']),approvalFile:options['--approval-receipt']&&resolve(options['--approval-receipt']),planOnly:options['--plan-only']});}
  console.log(json(result));if(!['GO','CAPTURED','PLAN_ONLY','DIAGNOSTIC_ONLY'].includes(result.verdict))process.exitCode=1;
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))try{await main(process.argv.slice(2));}catch(e){if(process.argv.includes('--self-test'))console.error(e.stack);console.log(json({verdict:'INCOMPLETE',reason:e.message}));process.exitCode=1;}
