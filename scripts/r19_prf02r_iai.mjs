// PRF02R only. No cargo in prepare/self-test. External writes are source backups only.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync, mkdirSync, unlinkSync, utimesSync, readdirSync, rmdirSync } from 'node:fs';
import { resolve, dirname, join, relative, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const TARGET = join(ROOT, 'target/r19-prf02r');
const DOC = join(ROOT, 'docs/perf');
const ID = join(DOC, 'R19_PRF02R_identity.json');
const OLD = JSON.parse(readFileSync(join(DOC, 'R19_PRF02_OWN_FREE_SINGLE_RESOLVE_GATE_identity.json')));
const sha = b => createHash('sha256').update(b).digest('hex');
const git = (...a) => execFileSync('git', a, {cwd: ROOT});
const q = s => `'${s.replaceAll("'", "'\\''")}'`;
const linux = p => p.replace(/^([A-Za-z]):[\\/]/, (_, d) => `/mnt/${d.toLowerCase()}/`).replaceAll('\\', '/');
const features = {pure:'production internals', instrumented:'production internals bench-internals'};
const benches = {pure:'r19_prf02_production_iai', instrumented:'r19_prf02_instrumented_iai'};
function fresh(path) { const now = new Date(); utimesSync(path, now, now); }
function harness(axis) {
  let text = readFileSync(join(ROOT, 'scripts/iai.mjs'), 'utf8');
  const change = (a,b) => { assert.equal(text.split(a).length,2); text=text.replace(a,b); };
  change("from './lib.mjs'", `from '${pathToFileURL(join(ROOT,'scripts/lib.mjs')).href}'`);
  change("const LINUX_TARGET = '/tmp/sefer-iai';", 'const LINUX_TARGET = \'"$PWD/target"\';');
  change("      'cargo bench',", "      'CARGO_BUILD_JOBS=3',\n      'cargo bench --locked',");
  change("const BENCH = 'perf_gate_iai';", `const BENCH = '${benches[axis]}';`);
  const start=text.indexOf('  // --locked: use the crate'), end=text.indexOf('\n}',start);
  assert(start>0 && end>start);
  text=text.slice(0,start)+"  throw new Error('Preinstalled matching runner required; installation forbidden');"+text.slice(end);
  return text;
}
function verify(map, root) {
  for(const [p,h] of Object.entries(map)) assert.equal(existsSync(join(root,p))?sha(readFileSync(join(root,p))):null,h,p);
}
function walk(dir) {
  return readdirSync(join(ROOT,dir), {withFileTypes:true}).filter(e => !['target','.git'].includes(e.name)).flatMap(e=>e.isDirectory()?walk(`${dir}/${e.name}`):[`${dir}/${e.name}`]);
}
function prepare(store) {
  assert(!existsSync(ID), 'identity exists; do not overwrite');
  const rel=relative(ROOT,store); assert(rel==='..' || rel.startsWith(`..${sep}`), 'store must be outside repo');
  assert(!existsSync(store), 'use a fresh external backup directory');
  verify(OLD.arms.B,ROOT);
  assert.equal(git('rev-parse','HEAD').toString().trim(),OLD.base);
  mkdirSync(TARGET,{recursive:true});
  for(const arm of ['A','B']) for(const [p,h] of Object.entries(OLD.arms[arm])) {
    if(h===null) continue;
    const dest=join(store,arm,p); mkdirSync(dirname(dest),{recursive:true});
    if(arm==='A') writeFileSync(dest,git('show',`${OLD.base}:${p}`),{flag:'wx'});
    else execFileSync('cp',[join(ROOT,p),dest]);
  }
  verify(OLD.arms.A,join(store,'A')); verify(OLD.arms.B,join(store,'B'));
  // Patch and full input identity are pinned immediately after backups.
}
// git diff --no-index normally exits 1: preserve stdout without treating it as failure.
function patchBytes() {
  const tracked=git('diff','--binary',OLD.base,'--','src');
  const result=spawnSync('git',['diff','--no-index','--binary','--','/dev/null','src/alloc_core/alloc_core/mem/owned_block.rs'],{cwd:ROOT});
  assert.equal(result.status,1); return Buffer.concat([tracked,result.stdout]);
}
function pin(store) {
  const patch=patchBytes(); writeFileSync(join(TARGET,'B.patch'),patch);
  const inputPaths=[...walk('src'), ...walk('crates'), '.cargo/config.toml','Cargo.toml','Cargo.lock',
    'scripts/iai.mjs','scripts/lib.mjs','scripts/r19_prf02r_iai.mjs','scripts/r19_prf02r_gate_table.mjs','scripts/r19_prf02_extract.mjs',
    'benches/perf_gate_iai.rs','benches/r19_prf02_production_iai.rs','benches/r19_prf02_instrumented_iai.rs',
    'examples/r19_prf02_activation.rs','tests/r19_prf02r_realloc_tail.rs','tests/heapcore_own_free_single_canonical_lookup.rs'];
  const id={base:OLD.base, original_b_patch_sha256:OLD.b_patch_sha256, reconstructed_b_patch_sha256:sha(patch),
    arms:{A:OLD.arms.A,B:OLD.arms.B}, inputs:Object.fromEntries([...new Set(inputPaths)].sort().map(p=>[p,sha(readFileSync(join(ROOT,p)))])), harness:{}};
  for(const axis of Object.keys(features)) {
    const text=harness(axis); writeFileSync(join(TARGET,`iai-${axis}.mjs`),text);
    id.harness[axis]=sha(text.replaceAll(pathToFileURL(ROOT).href,'<repo>'));
  }
  writeFileSync(ID,JSON.stringify(id,null,2)+'\n',{flag:'wx'});
  console.log(`PASS: snapshots A/B; identity pinned; B patch ${id.reconstructed_b_patch_sha256}; no cargo`);
}
function copy(store,arm,id) {
  verify(id.arms[arm],join(store,arm));
  for(const [p,h] of Object.entries(id.arms[arm])) {
    if(h===null) {if(existsSync(join(ROOT,p))) unlinkSync(join(ROOT,p));}
    else { execFileSync('cp',[join(store,arm,p),join(ROOT,p)]); fresh(join(ROOT,p)); execFileSync('cmp',[join(store,arm,p),join(ROOT,p)]); }
  }
  verify(id.arms[arm],ROOT);
}
function capture(file,command,args) {
  assert(!existsSync(file),'receipt exists');
  const metadata={arm:args.arm,axis:args.axis,repeat:args.repeat};
  const r=spawnSync(command,args.commandArgs,{cwd:ROOT,encoding:'utf8',maxBuffer:64*1024*1024});
  const text=((r.stdout??'')+(r.stderr??'')).replaceAll(linux(ROOT),'<repo>').replaceAll(ROOT,'<repo>').replaceAll(ROOT.replaceAll('\\','/'),'<repo>')
    .replace(/[A-Z]:[\\/]Users[\\/][^\s\\/]+/gi,'<home>').replace(/\/home\/[^\s/]+/g,'<home>');
  writeFileSync(file,`PRF02R_METADATA ${JSON.stringify({...metadata,identity_sha256:sha(readFileSync(ID)),prereg_sha256:sha(readFileSync(join(DOC,'R19_PRF02R_PREREGISTRATION.md'))),features:metadata.axis==='activation'?'production internals bench-internals medium-classes':features[metadata.axis],env:{RUSTC_WRAPPER:'',CARGO_BUILD_JOBS:'3',CARGO_TARGET_DIR:'<repo>/target',R19_PRF02R_ARM:metadata.axis==='activation'?metadata.arm:null}})}\n${text}\nPRF02R_EXIT ${r.status}\n`,{flag:'wx'}); assert.equal(r.status,0,'command failed; receipt preserved');
}
function run(store,arm,axis,rep) {
  assert(['A','B'].includes(arm)); assert(axis==='activation'||axis in features); assert(/^[12]$/.test(rep));
  const id=JSON.parse(readFileSync(ID)); verify(id.arms.B,ROOT); verify(id.inputs,ROOT);
  assert.equal(sha(readFileSync(join(DOC,'R19_PRF02R_PREREGISTRATION.md'))),readFileSync(join(DOC,'R19_PRF02R_prereg.sha256'),'utf8').trim(),'prereg drift');
  if(axis!=='activation') for(const a of ['A','B']) for(const r of [1,2]) {
    const text=readFileSync(join(DOC,`_raw_r19_prf02r_${a}_activation_${r}.log`),'utf8');
    assert(text.includes('\nPRF02R_EXIT 0\n'),'activation command failed; iai forbidden');
    const tail=a==='A'?1:0;
    const oracle=JSON.parse(text.split(/\r?\n/).find(l=>l.startsWith('{"arm"')));
    assert.equal(oracle.arm,a); assert.equal(oracle.expected,a==='A'?32:16);
    assert.equal(oracle.hits+oracle.misses,oracle.expected); assert.equal(oracle.tcache,16);
    assert.equal(oracle.n,16); assert.equal(oracle.preallocated,64); assert.equal(oracle.routed_scans,0);
    assert(text.includes(`arm=${a} large_free=${2+tail}`));
    assert(text.includes(`arm=${a} inplace=1 alloc=0 move=${1+tail} promote=${2+tail} tail=${tail}`));
    assert(/test result: ok\. 1 passed; 0 failed/.test(text),'activation failed; iai forbidden');
  }
  const lock=join(TARGET,'exclusive.lock'); mkdirSync(lock); // fail closed on concurrent driver
  try {
    const receipt=join(DOC,`_raw_r19_prf02r_${arm}_${axis}_${rep}.log`); assert(!existsSync(receipt));
    for(const a of Object.keys(features)) assert.equal(sha(readFileSync(join(TARGET,`iai-${a}.mjs`),'utf8').replaceAll(pathToFileURL(ROOT).href,'<repo>')),id.harness[a]);
    copy(store,arm,id);
    if(axis==='activation') {
      const env='RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target"';
      const cmd=`cd ${q(linux(ROOT))} && ${env} cargo run --locked --example r19_prf02_activation --features 'production internals bench-internals' -- ${arm} ${arm==='A'?32:16} && R19_PRF02R_ARM=${arm} ${env} cargo test --locked --features 'production internals bench-internals medium-classes' --test r19_prf02r_realloc_tail -- --test-threads=1 --nocapture`;
      capture(receipt,'wsl',{arm,axis,repeat:Number(rep),commandArgs:['bash','-lc',cmd]});
    } else capture(receipt,process.execPath,{arm,axis,repeat:Number(rep),commandArgs:[join(TARGET,`iai-${axis}.mjs`),'--features',features[axis]]});
  } finally {
    try {copy(store,'B',id);} finally {rmdirSync(lock);}
  }
}
const [mode,storeArg,arm,axis,rep]=process.argv.slice(2);
if(mode==='--self-test') {
  for(const axis of Object.keys(features)) {const h=harness(axis); assert(h.includes('cargo bench --locked')); assert(h.includes('"$PWD/target"')); assert(!h.includes('target/r19-linux'));}
  verify(OLD.arms.B,ROOT);
  console.log('PASS: harness anchors, target root, locked, exact B; no switching/cargo');
} else if(mode==='--prepare') {const store=resolve(storeArg); prepare(store); pin(store);}
else if(mode==='--run') {assert(process.env.R19_PRF02R_REVIEWED==='yes','review required'); run(resolve(storeArg),arm,axis,rep);}
else throw new Error('Usage: --self-test | --prepare fresh-external-backup-dir | --run external-backup-dir A|B activation|pure|instrumented 1|2 (review approval required)');
