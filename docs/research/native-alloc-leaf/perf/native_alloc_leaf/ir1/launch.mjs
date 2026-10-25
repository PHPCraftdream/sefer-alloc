import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import os from 'node:os';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),dir='worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/ir1';
assert(!fs.existsSync(dir),'Не перезаписывать evidence');fs.mkdirSync(dir);
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function run(cmd,args,cwd=root){const p=spawnSync(cmd,args,{cwd,encoding:'utf8',maxBuffer:32*1024*1024});assert.equal(p.status,0,p.stderr);return p.stdout;}
function linux(p){return '/mnt/'+p[0].toLowerCase()+p.slice(2).replaceAll('\\','/');}
const repos={before:'worktrees/own-free-before',after:'worktrees/native-alloc-leaf'},ids={};
for(const [phase,repo] of Object.entries(repos)){
 const patch=run('git',['diff','--binary','HEAD','--','src','Cargo.toml','Cargo.lock'],repo);
 if(phase==='before')assert.equal(run('git',['diff','HEAD','--','src','Cargo.lock'],repo),'');
 const base=run('git',['rev-parse','HEAD'],repo).trim();assert.equal(base,'f80c681b799ef4ef7fa08d79c06896fdf1104fe9');
 fs.writeFileSync(`${dir}/${phase}.patch`,patch);
 ids[phase]={base,patch_sha256:hash(patch),files:Object.fromEntries(['examples/own_free_profile.rs','examples/own_free_wall.rs'].map(f=>[f,hash(fs.readFileSync(`${repo}/${f}`))]))};
}
assert.deepEqual(ids.before.files,ids.after.files);
const buildIdentity=JSON.parse(fs.readFileSync('worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/attempt1/identity.json'));
assert.equal(ids.after.patch_sha256,buildIdentity.patch_sha256);
const identity={captured_before_build:new Date().toISOString(),ids,features:'production',cpu:os.cpus()[0].model,windows:os.release(),rustc:run('wsl.exe',['-e','bash','-lc','rustc -Vv']),kernel:run('wsl.exe',['-e','uname','-a']),valgrind:run('wsl.exe',['-e','valgrind','--version']),script_sha256:hash(fs.readFileSync('.rush/native-owner/leaf-ir.sh'))};
fs.writeFileSync(`${dir}/identity.json`,JSON.stringify(identity,null,2));fs.copyFileSync('.rush/native-owner/leaf-ir.sh',`${dir}/leaf-ir.sh`);
for(const [phase,repo] of Object.entries(repos)){
 console.log(`Leaf Ir build/profile ${phase}`);
 const p=spawnSync('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',linux(path.join(root,'.rush/native-owner/leaf-ir.sh')),linux(path.join(root,repo)),linux(path.join(root,`.rush/native-owner/ir-${phase}`))],{stdio:'inherit'});
 assert.equal(p.status,0,`Ir ${phase}`);
 assert.equal(hash(run('git',['diff','--binary','HEAD','--','src','Cargo.toml','Cargo.lock'],repo)),ids[phase].patch_sha256);
}
const rows=[];
for(const phase of ['before','after'])for(const arm of ['sefer','mimalloc'])for(const mode of ['rt','16','64','bulk64','bulk16k'])for(const axis of ['alloc','free','pair']){
 const bulk=mode.startsWith('bulk'),ns=bulk?[100,200]:[50000,100000],values=[];
 for(const n of ns){const stem=`.rush/native-owner/ir-${phase}/${arm}-${mode}-${axis}-${n}`;
 const raw=fs.readFileSync(`${stem}.callgrind`,'utf8'),m=/^summary:\s*(\d+)/m.exec(raw);assert(m);
 const stdout=fs.readFileSync(`${stem}.stdout`,'utf8');assert(JSON.parse(stdout.trim()).completed);
 values.push(Number(m[1]));
 fs.copyFileSync(`${stem}.stdout`,`${dir}/${phase}-${arm}-${mode}-${axis}-${n}.stdout`);
 fs.writeFileSync(`${dir}/${phase}-${arm}-${mode}-${axis}-${n}.receipt.json`,JSON.stringify({phase,arm,mode,axis,n,total_ir:Number(m[1]),callgrind_sha256:hash(raw),stderr:fs.readFileSync(`${stem}.stderr`,'utf8').split(linux(root)).join('<workspace>').split(root).join('<workspace>')},null,2));
 }
 const operations=(ns[1]-ns[0])*(bulk?64:1),marginal=(values[1]-values[0])/operations;
 assert(Number.isFinite(marginal)&&marginal>0);
 rows.push({phase,arm,mode,axis,n1:ns[0],n2:ns[1],ir1:values[0],ir2:values[1],operations,marginal_ir:marginal});
}
fs.writeFileSync(`${dir}/samples.json`,JSON.stringify(rows,null,2));
fs.writeFileSync(`${dir}/completed.json`,JSON.stringify({completed:new Date().toISOString(),profiles:120,tests_executed:false},null,2));
console.log(JSON.stringify(rows,null,2));
