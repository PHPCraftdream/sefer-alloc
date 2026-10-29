import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd();
const repos={before:path.join(root,'worktrees/own-free-before'),after:path.join(root,'worktrees/own-free-context')};
const evidence=path.join(repos.after,'docs/perf/own_free_context');
const raw=path.join(evidence,'raw');fs.mkdirSync(raw,{recursive:true});
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function exec(cmd,args,cwd,opts={}){const p=spawnSync(cmd,args,{cwd,encoding:'utf8',maxBuffer:32*1024*1024,...opts});if(p.status!==0||p.error)throw Error(`${cmd}: ${p.error??p.status}\n${p.stdout}\n${p.stderr}`);return p.stdout;}
function wsl(p){return '/mnt/'+p[0].toLowerCase()+p.slice(2).replaceAll(String.fromCharCode(92),'/');}
const input=['src/registry/heap_core/free/dealloc_own_base.rs','Cargo.toml','Cargo.lock','examples/own_free_wall.rs'];
const identities={};
for(const [phase,repo] of Object.entries(repos)){
 const patch=exec('git',['diff','--binary','--','src','Cargo.toml','Cargo.lock'],repo);
 if(phase==='before')assert.equal(exec('git',['diff','--','src','Cargo.lock'],repo),'');
 fs.writeFileSync(path.join(evidence,`comparison-${phase}.patch`),patch);
 identities[phase]={base:exec('git',['rev-parse','HEAD'],repo).trim(),patch_sha256:hash(patch),files:Object.fromEntries(input.map(f=>[f,hash(fs.readFileSync(path.join(repo,f)))]))};
}
assert.equal(identities.before.files['examples/own_free_wall.rs'],identities.after.files['examples/own_free_wall.rs']);
const meta={captured_before_build:new Date().toISOString(),identities,features:'production',native_rustc:exec('rustc',['-Vv'],root),native_cargo:exec('cargo',['-V'],root).trim(),cpu:os.cpus()[0].model,os:`${os.platform()}/${os.release()}/${os.arch()}`,native_plan:{blocks:20,sequence:['a','ma','b','mb','mb','b','ma','a'],aa_blocks:10,modes:{rt:1000000,'16':1000000,'64':1000000,bulk64:256,bulk16k:256}},note:'workload_ns includes setup/churn/teardown inside run; process_ns additionally includes startup/output/exit. Not pure free latency.'};
fs.writeFileSync(path.join(evidence,'comparison_identity.json'),JSON.stringify(meta,null,2));
fs.copyFileSync(path.join(repos.after,'examples/own_free_wall.rs'),path.join(evidence,'own_free_wall.rs'));
for(const [phase,repo] of Object.entries(repos)){
 console.log(`Overflow profile ${phase}`);
 exec('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',wsl(path.join(root,'.rush/own-free-context/controls.sh')),wsl(repo),wsl(path.join(root,`.rush/own-free-context/control-${phase}`))],root,{stdio:'inherit'});
}
const controls=[];
function total(file){const text=fs.readFileSync(file,'utf8');const m=/([\d,]+)\s+\([^)]*\)\s+PROGRAM TOTALS/.exec(text);assert(m);return Number(m[1].replaceAll(',',''));}
for(const phase of ['before','after'])for(const pass of [1,2])for(const arm of ['sefer','mimalloc'])for(const mode of ['bulk64','bulk16k']){
 const dir=path.join(root,`.rush/own-free-context/control-${phase}`);
 const ir1=total(path.join(dir,`p${pass}-${arm}-${mode}-100.annotate`));
 const ir2=total(path.join(dir,`p${pass}-${arm}-${mode}-200.annotate`));
 controls.push({phase,pass,arm,mode,n1:100,n2:200,frees_per_round:64,ir1,ir2,marginal_ir_per_free:(ir2-ir1)/6400});
}
fs.writeFileSync(path.join(raw,'overflow_controls.json'),JSON.stringify(controls,null,2));
console.log(JSON.stringify(controls,null,2));

fs.writeFileSync(path.join(evidence,'controls_completed.json'),JSON.stringify({completed:new Date().toISOString(),native_executed:false,profiles:32},null,2));
console.log('Controls completed; native deferred to best integrated composition.');
