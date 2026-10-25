import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),repos={before:path.join(root,'worktrees/own-free-before'),after:path.join(root,'worktrees/own-free-context')};
const dest=path.join(repos.after,'docs/perf/own_free_context/canonical');
assert(!fs.existsSync(dest),'Не перезаписывать замеры');fs.mkdirSync(dest,{recursive:true});
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function run(cmd,args,cwd,env=process.env){const p=spawnSync(cmd,args,{cwd,env,encoding:'utf8',maxBuffer:128*1024*1024});if(p.error||p.status!==0)throw Error(`${cmd}: ${p.error??p.status}\n${p.stderr}`);return p.stdout;}
const inputs=['benches/global_alloc.rs','scripts/bench-table.mjs','scripts/lib.mjs','Cargo.lock'];
const identity={captured_before_build:new Date().toISOString(),features:'production internals bench-internals',sequence:['before','after','after','before'],cpu:os.cpus()[0].model,os:os.release(),arch:os.arch(),rustc:run('rustc',['-Vv'],root),arms:{}};
for(const [phase,repo] of Object.entries(repos)){
 const patch=run('git',['diff','--binary','--','src','Cargo.toml','Cargo.lock','benches','scripts'],repo);
 fs.writeFileSync(path.join(dest,phase+'.patch'),patch);
 identity.arms[phase]={base:run('git',['rev-parse','HEAD'],repo).trim(),patch_sha256:hash(patch),inputs:Object.fromEntries(inputs.map(f=>[f,hash(fs.readFileSync(path.join(repo,f)))]))};
}
assert.deepEqual(identity.arms.before.inputs,identity.arms.after.inputs);
fs.writeFileSync(path.join(dest,'identity.json'),JSON.stringify(identity,null,2));
function clean(s){for(const p of [root,root.replaceAll(String.fromCharCode(92),'/')])s=s.split(p).join('<workspace>');return s;}
function snapshot(src,to){for(const e of fs.readdirSync(src,{withFileTypes:true})){
 const a=path.join(src,e.name),b=path.join(to,e.name);
 if(e.isDirectory()){if(['report','base','change'].includes(e.name))continue;snapshot(a,b);}
 else if(path.basename(src)==='new'&&['sample.json','estimates.json','benchmark.json'].includes(e.name)){fs.mkdirSync(to,{recursive:true});fs.copyFileSync(a,b);}
}}
for(const [i,phase] of identity.sequence.entries()){
 const target=path.join(root,'.rush/own-free-context/canonical-'+phase),env={...process.env,CARGO_TARGET_DIR:target};delete env.CARGO_PROFILE_RELEASE_DEBUG;
 console.log(`Canonical ${i+1}/4: ${phase}`);
 const p=spawnSync(process.execPath,['scripts/bench-table.mjs'],{cwd:repos[phase],env,encoding:'utf8',maxBuffer:128*1024*1024});
 fs.writeFileSync(path.join(dest,`${i}-${phase}.log`),clean(`${p.stdout??''}\n${p.stderr??''}`));assert.equal(p.status,0,`canonical ${phase}`);
 snapshot(path.join(target,'criterion'),path.join(dest,`criterion-${i}-${phase}`));
}
for(const [phase,repo] of Object.entries(repos)){
 assert.equal(hash(run('git',['diff','--binary','--','src','Cargo.toml','Cargo.lock','benches','scripts'],repo)),identity.arms[phase].patch_sha256);
 for(const f of inputs)assert.equal(hash(fs.readFileSync(path.join(repo,f))),identity.arms[phase].inputs[f]);
}
fs.writeFileSync(path.join(dest,'completed.json'),JSON.stringify({completed:new Date().toISOString(),sweeps:4,source_drift:false},null,2));
console.log('Canonical ABBA завершён; raw samples сохранены.');
