import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync,existsSync,mkdirSync,unlinkSync,utimesSync} from 'node:fs';
import {resolve,dirname,join,relative,isAbsolute} from 'node:path';
import {fileURLToPath,pathToFileURL} from 'node:url';
import {spawnSync,execFileSync} from 'node:child_process';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..'), doc=join(root,'docs/perf'), target=join(root,'target/r19-prf02c');
const sha=b=>createHash('sha256').update(b).digest('hex');
const load=p=>JSON.parse(readFileSync(p));
const features={pure:'production internals',instrumented:'production internals bench-internals'};
const benches={pure:'r19_prf02_production_iai',instrumented:'r19_prf02_instrumented_iai'};
const idPath=join(doc,'R19_PRF02C_identity.json'), prereg=join(doc,'R19_PRF02C_PREREGISTRATION.md');
function verify(map,base) {for(const [p,h] of Object.entries(map)) assert.equal(existsSync(join(base,p))?sha(readFileSync(join(base,p))):null,h,p);}
function external(store) {const rel=relative(root,store); assert(isAbsolute(rel)||rel==='..'||rel.startsWith('../')||rel.startsWith('..\\'),'source store must be external');}
function harness(axis) {
 let text=readFileSync(join(root,'scripts/iai.mjs'),'utf8');
 const replace=(a,b)=>{assert.equal(text.split(a).length,2); text=text.replace(a,b);};
 replace("from './lib.mjs'",`from '${pathToFileURL(join(root,'scripts/lib.mjs')).href}'`);
 replace("const LINUX_TARGET = '/tmp/sefer-iai';",'const LINUX_TARGET = \'"$PWD/target"\';');
 replace("      'cargo bench',","      'CARGO_BUILD_JOBS=3',\n      'cargo bench --locked',");
 replace("const BENCH = 'perf_gate_iai';",`const BENCH = '${benches[axis]}';`);
 const start=text.indexOf('  // --locked: use the crate'),end=text.indexOf('\n}',start); assert(start>0&&end>start);
 return text.slice(0,start)+"  throw new Error('Preinstalled matching runner required; installation forbidden');"+text.slice(end);
}
function norm(text) {return text.replaceAll(pathToFileURL(root).href,'<repo>');}
function prepare(store) {
 external(store); assert(!existsSync(idPath),'immutable C identity already exists');
 const r=load(join(doc,'R19_PRF02R_identity.json')), snap=load(join(doc,'R19_PRF02C_snapshot.json'));
 verify(r.arms.B,root); verify(r.inputs,root);
 for(const arm of ['B','Cprime']) verify(snap.arms[arm],join(store,arm));
 verify(snap.arms.B,root);
 assert.equal(sha(readFileSync(join(target,'Cprime.patch'))),snap.patch_sha256);
 const id={...snap,r_identity_sha256:sha(readFileSync(join(doc,'R19_PRF02R_identity.json'))),r_inputs:r.inputs,inputs:{},harness:{}};
 for(const p of ['scripts/r19_prf02c_iai.mjs','scripts/r19_prf02c_gate_table.mjs','docs/perf/R19_PRF02C_PREREGISTRATION.md','docs/perf/R19_PRF02C_snapshot.json']) id.inputs[p]=sha(readFileSync(join(root,p)));
 for(const axis of Object.keys(features)) {const text=harness(axis); mkdirSync(target,{recursive:true}); writeFileSync(join(target,`iai-${axis}.mjs`),text); id.harness[axis]=sha(norm(text));}
 writeFileSync(idPath,JSON.stringify(id,null,2)+'\n',{flag:'wx'});
 writeFileSync(join(doc,'R19_PRF02C_prereg.sha256'),sha(readFileSync(prereg))+'\n',{flag:'wx'});
 console.log('PASS: exact B bytes/hashes, external B/Cprime snapshots, patch and C identity pinned; no cargo');
}
function install(store,arm,id) {
 verify(id.arms[arm],join(store,arm));
 for(const [p,h] of Object.entries(id.arms[arm])) {
  const dest=join(root,p);
  if(h===null) {if(existsSync(dest)) unlinkSync(dest);}
  else {execFileSync('cp',[join(store,arm,p),dest]); execFileSync('cmp',[join(store,arm,p),dest]); const now=new Date(); utimesSync(dest,now,now);}
 }
 verify(id.arms[arm],root);
}
function validate(id) {
 verify(id.arms.B,root); verify(id.r_inputs,root); verify(id.inputs,root);
 assert.equal(sha(readFileSync(join(doc,'R19_PRF02R_identity.json'))),id.r_identity_sha256);
 assert.equal(sha(readFileSync(prereg)),readFileSync(join(doc,'R19_PRF02C_prereg.sha256'),'utf8').trim());
 assert.equal(sha(readFileSync(join(target,'Cprime.patch'))),id.patch_sha256);
 for(const axis of Object.keys(features)) assert.equal(sha(norm(readFileSync(join(target,`iai-${axis}.mjs`),'utf8'))),id.harness[axis]);
}
function run(store) {
 assert.equal(process.env.R19_PRF02C_APPROVED,'yes','explicit approval after independent Miri required');
 external(store); const id=load(idPath); validate(id);
 // mkdir is the one-attempt marker, not a Cargo/source lockfile. Never retry.
 mkdirSync(join(target,'attempt-started'));
 try {
  for(const axis of ['pure','instrumented']) for(const [arm,repeat] of [['B',1],['Cprime',1],['B',2],['Cprime',2]]) {
   install(store,arm,id);
   const file=join(doc,`_raw_r19_prf02c_${arm}_${axis}_${repeat}.log`); assert(!existsSync(file));
   const args=[join(target,`iai-${axis}.mjs`),'--features',features[axis]];
   const result=spawnSync(process.execPath,args,{cwd:root,encoding:'utf8',maxBuffer:64*1024*1024,env:{...process.env,RUSTC_WRAPPER:'',CARGO_BUILD_JOBS:'3',CARGO_TARGET_DIR:join(root,'target')}});
   const linux=root.replace(/^([A-Za-z]):[\\/]/,(_,d)=>`/mnt/${d.toLowerCase()}/`).replaceAll('\\','/');
   const text=((result.stdout??'')+(result.stderr??'')).replaceAll(linux,'<repo>').replaceAll(root,'<repo>').replaceAll(root.replaceAll('\\','/'),'<repo>').replace(/\/home\/[^\s/]+/g,'<home>');
   const metadata={arm,axis,repeat,identity_sha256:sha(readFileSync(idPath)),prereg_sha256:sha(readFileSync(prereg)),features:features[axis],env:{RUSTC_WRAPPER:'',CARGO_BUILD_JOBS:'3',CARGO_TARGET_DIR:'<repo>/target'}};
   writeFileSync(file,`PRF02C_METADATA ${JSON.stringify(metadata)}\n${text}\nPRF02C_EXIT ${result.status}\n`,{flag:'wx'});
   verify(id.arms[arm],root); assert.equal(result.status,0,'failed attempt; no retry');
  }
 } finally {install(store,'B',id); validate(id); console.log('RESTORED: B cmp/hashes and all pinned R inputs match');}
}
const [mode,storeArg]=process.argv.slice(2);
if(mode==='--prepare') prepare(resolve(storeArg));
else if(mode==='--run') run(resolve(storeArg));
else if(mode==='--self-test') {for(const axis of Object.keys(features)) {assert(harness(axis).includes('cargo bench --locked')); assert(harness(axis).includes('"$PWD/target"'));} const id=load(idPath); validate(id); console.log('PASS: source/pinned inputs/harness/identity/prereg; no cargo');}
else throw new Error('Usage: --prepare external-source-store | --self-test | --run external-source-store (approval required)');
