import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {gunzipSync} from 'node:zlib';
const root=process.cwd();
const repos={before:path.join(root,'worktrees/own-free-before'),after:path.join(root,'worktrees/own-free-context')};
const evidence=path.join(repos.after,'docs/perf/own_free_context'),raw=path.join(evidence,'raw');
const meta=JSON.parse(fs.readFileSync(path.join(evidence,'comparison_identity.json'),'utf8')),identities=meta.identities;
const input=Object.keys(identities.after.files);
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function exec(cmd,args,cwd,opts={}){const p=spawnSync(cmd,args,{cwd,encoding:'utf8',maxBuffer:32*1024*1024,...opts});if(p.status!==0||p.error)throw Error(cmd+': '+(p.error??p.status)+' '+p.stderr);return p.stdout;}
for(const [phase,repo] of Object.entries(repos)){
 const p=path.join(evidence,'comparison-'+phase+'.patch');const snapshot=fs.existsSync(p)?fs.readFileSync(p):gunzipSync(fs.readFileSync(p+'.gz'));
 assert.equal(exec('git',['diff','--binary','--','src','Cargo.toml','Cargo.lock'],repo),snapshot.toString('utf8'));
 for(const f of input)assert.equal(hash(fs.readFileSync(path.join(repo,f))),identities[phase].files[f]);
}
// Native benchmarks после всех компиляций и Callgrind, без конкурирующих проверок.
const nativeTargets={};
for(const [phase,repo] of Object.entries(repos)){
 const target=path.join(root,`.rush/own-free-context/native-${phase}`);nativeTargets[phase]=target;
 const env={...process.env,CARGO_TARGET_DIR:target};delete env.CARGO_PROFILE_RELEASE_DEBUG;
 console.log(`Native benchmark build ${phase}`);
 const p=spawnSync('cargo',['build','--locked','--release','--example','own_free_wall','--features','production'],{cwd:repo,env,encoding:'utf8',maxBuffer:32*1024*1024});
 fs.writeFileSync(path.join(root,`.rush/own-free-context/native-build-${phase}.log`),`${p.stdout??''}\n${p.stderr??''}`);
 assert.equal(p.status,0,`native build ${phase}`);
}
const binaries=Object.fromEntries(Object.entries(nativeTargets).map(([phase,t])=>[phase,hash(fs.readFileSync(path.join(t,'release/examples/own_free_wall.exe')))]));
fs.writeFileSync(path.join(evidence,'native_binaries.json'),JSON.stringify(binaries,null,2));
const samples=path.join(raw,'native.ndjson');assert(!fs.existsSync(samples),'no overwrite of measurements');
const cases={a:['before','sefer'],b:['after','sefer'],ma:['before','mimalloc'],mb:['after','mimalloc']};
function sample(label,mode,n,block,position,control){
 const [phase,arm]=cases[label];const exe=path.join(nativeTargets[phase],'release/examples/own_free_wall.exe');
 const start=process.hrtime.bigint();const stdout=exec(exe,[arm,mode,String(n)],repos[phase]);const process_ns=Number(process.hrtime.bigint()-start);
 const row=JSON.parse(stdout.trim());assert.equal(row.completed,true);assert.equal(row.mode,mode);assert.equal(row.steps,n);assert(row.workload_ns>0&&process_ns>row.workload_ns);
 fs.appendFileSync(samples,JSON.stringify({label,phase,arm,block,position,control,process_ns,...row})+'\n');
}
for(const [mode,n] of Object.entries(meta.native_plan.modes)){
 for(let block=0;block<20;block++)for(const [position,label] of meta.native_plan.sequence.entries())sample(label,mode,n,block,position,false);
 console.log(`Native complete ${mode}: 160 fresh processes`);
}
for(let block=0;block<10;block++)for(let position=0;position<4;position++)sample('a','rt',1000000,block,position,true);
for(const [phase,target] of Object.entries(nativeTargets))assert.equal(hash(fs.readFileSync(path.join(target,'release/examples/own_free_wall.exe'))),binaries[phase],`binary changed ${phase}`);
for(const [phase,repo] of Object.entries(repos))for(const f of input)assert.equal(hash(fs.readFileSync(path.join(repo,f))),identities[phase].files[f],`source changed ${phase}/${f}`);
fs.writeFileSync(path.join(evidence,'comparison_completed.json'),JSON.stringify({completed:new Date().toISOString(),native_processes:840,control_profiles:32},null,2));
console.log('Control and native comparison complete; source hashes unchanged.');
