import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),repo=path.join(root,'worktrees/own-free-context'),dir=path.join(repo,'docs/perf/own_free_context/focused');
assert(!fs.existsSync(dir),'Не перезаписывать замер');fs.mkdirSync(dir,{recursive:true});
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const cm=JSON.parse(fs.readFileSync(path.join(repo,'docs/perf/own_free_context/canonical/identity.json')));
const bins={},id={captured_before_measure:new Date().toISOString(),features:cm.features,canonical_identity_sha256:hash(fs.readFileSync(path.join(repo,'docs/perf/own_free_context/canonical/identity.json'))),blocks:10,sequence:['before','after','after','before'],filter:'^global_alloc_churn/(SeferAlloc|mimalloc)/16B$',binaries:{}};
for(const phase of ['before','after']){
 const d=path.join(root,'.rush/own-free-context/canonical-'+phase+'/release/deps');const found=fs.readdirSync(d).filter(n=>/^global_alloc-[a-f0-9]+\.exe$/.test(n));assert.equal(found.length,1);bins[phase]=path.join(d,found[0]);id.binaries[phase]=hash(fs.readFileSync(bins[phase]));
}
fs.writeFileSync(path.join(dir,'identity.json'),JSON.stringify(id,null,2));
function clean(s){for(const p of [root,root.replaceAll(String.fromCharCode(92),'/')])s=s.split(p).join('<workspace>');return s;}
function snapshots(d,out,rows){for(const e of fs.readdirSync(d,{withFileTypes:true})){
 const p=path.join(d,e.name),q=path.join(out,e.name);if(e.isDirectory()){if(['report','base','change'].includes(e.name))continue;snapshots(p,q,rows);}
 else if(path.basename(d)==='new'&&['sample.json','estimates.json','benchmark.json'].includes(e.name)){fs.mkdirSync(out,{recursive:true});fs.copyFileSync(p,q);if(e.name==='benchmark.json'){
 const b=JSON.parse(fs.readFileSync(p)),s=JSON.parse(fs.readFileSync(path.join(d,'sample.json'))),est=JSON.parse(fs.readFileSync(path.join(d,'estimates.json')));const mean=s.times.reduce((v,t,i)=>v+t/s.iters[i],0)/s.times.length;assert(Math.abs(mean-est.mean.point_estimate)<mean*1e-8);rows.push({cell:b.full_id,mean_ns_per_pair:mean/1024,samples:s.times.length});
 }}
}}
for(let block=0;block<10;block++)for(const [position,phase] of id.sequence.entries()){
 const target=path.join(root,`.rush/own-free-context/focused-${block}-${position}`),env={...process.env,CARGO_TARGET_DIR:target};
 const p=spawnSync(bins[phase],[id.filter,'--bench','--noplot'],{cwd:path.join(root,'worktrees/own-free-'+(phase==='before'?'before':'context')),env,encoding:'utf8',maxBuffer:32*1024*1024});
 fs.writeFileSync(path.join(dir,`${block}-${position}-${phase}.log`),clean(`${p.stdout??''}\n${p.stderr??''}`));assert.equal(p.status,0);
 const rows=[];snapshots(path.join(target,'criterion'),path.join(dir,`criterion-${block}-${position}`),rows);assert.equal(rows.length,2);assert(rows.every(r=>r.samples>=10));
 fs.appendFileSync(path.join(dir,'samples.ndjson'),JSON.stringify({block,position,phase,rows})+'\n');
 if(position===3)console.log(`Focused block ${block+1}/10 complete`);
}
for(const phase of ['before','after'])assert.equal(hash(fs.readFileSync(bins[phase])),id.binaries[phase]);
fs.writeFileSync(path.join(dir,'completed.json'),JSON.stringify({completed:new Date().toISOString(),processes:40,binary_drift:false},null,2));
