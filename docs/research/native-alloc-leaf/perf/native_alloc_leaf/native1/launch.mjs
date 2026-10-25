import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import os from 'node:os';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),dir='worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/native1';
assert(!fs.existsSync(dir),'Не перезаписывать measurements');fs.mkdirSync(dir);
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const repos={before:'worktrees/own-free-before',after:'worktrees/native-alloc-leaf'};
function exec(cmd,args,cwd=root,env=process.env){const p=spawnSync(cmd,args,{cwd,env,encoding:'utf8',maxBuffer:32*1024*1024});assert.equal(p.status,0,p.stderr);return p.stdout;}
const ids={};
for(const [phase,repo] of Object.entries(repos)){
 const patch=exec('git',['diff','--binary','HEAD','--','src','Cargo.toml','Cargo.lock'],repo);
 if(phase==='before')assert.equal(exec('git',['diff','HEAD','--','src','Cargo.lock'],repo),'');
 const base=exec('git',['rev-parse','HEAD'],repo).trim();assert.equal(base,'f80c681b799ef4ef7fa08d79c06896fdf1104fe9');
 ids[phase]={base,patch_sha256:hash(patch),harness_sha256:hash(fs.readFileSync(`${repo}/examples/own_free_wall.rs`))};
 fs.writeFileSync(`${dir}/${phase}.patch`,patch);
}
assert.equal(ids.before.harness_sha256,ids.after.harness_sha256);
const original=JSON.parse(fs.readFileSync('worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/attempt1/identity.json'));
assert.equal(ids.after.patch_sha256,original.patch_sha256);
const rustc=exec('rustc',['-Vv']);assert.equal(rustc,original.rustc);
const modes={rt:1000000,'16':1000000,'64':1000000,bulk64:256,bulk16k:256},sequence=['a','ma','b','mb','mb','b','ma','a'];
fs.writeFileSync(`${dir}/identity.json`,JSON.stringify({captured_before_build:new Date().toISOString(),ids,rustc,cargo:exec('cargo',['-V']),cpu:os.cpus()[0].model,os:os.release(),features:'production',blocks:20,aa_blocks:10,sequence,modes,statistic:'arithmetic mean over full workload_ns/pair; paired Student-t CI over block means; process_ns separately',layer:'direct GlobalAlloc facade; NOT installed allocator'},null,2));
const targets={after:path.join(root,'.rush/native-owner/leaf-target')};
const afterReceipt=JSON.parse(fs.readFileSync('worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/attempt1/binary.json'));
assert.equal(hash(fs.readFileSync(path.join(targets.after,'release/examples/own_free_wall.exe'))),afterReceipt.sha256);
for(const [phase,repo] of Object.entries(repos).filter(([phase])=>phase==='before')){
 const target=path.join(root,`.rush/native-owner/fresh-native-${phase}`);targets[phase]=target;
 const env={...process.env,CARGO_TARGET_DIR:target};delete env.CARGO_PROFILE_RELEASE_DEBUG;
 console.log(`Fresh native build ${phase}`);
 const p=spawnSync('cargo',['build','--locked','--release','--features','production','--example','own_free_wall'],{cwd:repo,env,encoding:'utf8',maxBuffer:32*1024*1024});
 fs.writeFileSync(`${dir}/build-${phase}.log`,`${p.stdout??''}\n${p.stderr??''}`.split(root).join('<workspace>').split(root.replaceAll('\\','/')).join('<workspace>'));
 assert.equal(p.status,0,p.stderr);
}
const binaries=Object.fromEntries(Object.entries(targets).map(([phase,t])=>[phase,hash(fs.readFileSync(path.join(t,'release/examples/own_free_wall.exe')))]));
fs.writeFileSync(`${dir}/binaries.json`,JSON.stringify(binaries,null,2));
const cases={a:['before','sefer'],ma:['before','mimalloc'],b:['after','sefer'],mb:['after','mimalloc']};
function sample(label,mode,n,block,position,control){
 const [phase,arm]=cases[label],exe=path.join(targets[phase],'release/examples/own_free_wall.exe');
 const start=process.hrtime.bigint(),stdout=exec(exe,[arm,mode,String(n)],repos[phase]),process_ns=Number(process.hrtime.bigint()-start);
 const row=JSON.parse(stdout.trim());assert(row.completed);assert.equal(row.mode,mode);assert.equal(row.steps,n);assert(row.workload_ns>0&&process_ns>row.workload_ns);
 fs.appendFileSync(`${dir}/samples.ndjson`,JSON.stringify({label,phase,arm,block,position,control,process_ns,...row})+'\n');
}
for(const [mode,n] of Object.entries(modes)){
 for(let block=0;block<20;block++)for(const [position,label] of sequence.entries())sample(label,mode,n,block,position,false);
 console.log(`Native complete ${mode}`);
}
for(let block=0;block<10;block++)for(let position=0;position<4;position++)sample('a','rt',1000000,block,position,true);
for(const [phase,target] of Object.entries(targets))assert.equal(hash(fs.readFileSync(path.join(target,'release/examples/own_free_wall.exe'))),binaries[phase]);
for(const [phase,repo] of Object.entries(repos)){
 assert.equal(hash(exec('git',['diff','--binary','HEAD','--','src','Cargo.toml','Cargo.lock'],repo)),ids[phase].patch_sha256);
 assert.equal(hash(fs.readFileSync(`${repo}/examples/own_free_wall.rs`)),ids[phase].harness_sha256);
}
fs.writeFileSync(`${dir}/completed.json`,JSON.stringify({completed:new Date().toISOString(),samples:840,tests_executed:false},null,2));
console.log('Native complete: 840 fresh process samples; no tests executed.');
