import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import os from 'node:os';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),repo='worktrees/native-mixed-judge',dir=`${repo}/docs/perf/native_mixed/calibration1`,target=path.join(root,'.rush/native-owner/mixed-target');
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function clean(s){return s.split(root).join('<workspace>').split(root.replaceAll('\\','/')).join('<workspace>');}
function run(cmd,args,cwd=repo){const p=spawnSync(cmd,args,{cwd,encoding:'utf8',maxBuffer:32*1024*1024});assert.equal(p.status,0,p.stderr);return p.stdout;}
const inputs=['examples/native_mixed_sefer.rs','examples/native_mixed_mimalloc.rs','examples/_shared/native_mixed_driver.rs','examples/_shared/paired_ab_workload.rs','docs/design/2026-10-25-native-mixed-calibration-prereg.md'];
if(!process.argv.includes('--check')){
 assert(!fs.existsSync(dir),'Не перезаписывать calibration');fs.mkdirSync(dir,{recursive:true});
 assert.equal(run('git',['diff','HEAD','--','src','Cargo.lock']),'');
 const base=run('git',['rev-parse','HEAD']).trim();assert.equal(base,'f80c681b799ef4ef7fa08d79c06896fdf1104fe9');
 const patch=run('git',['diff','--binary','HEAD','--','src','Cargo.toml','Cargo.lock','examples']);fs.writeFileSync(`${dir}/source.patch`,patch);
 const files=Object.fromEntries(inputs.map(f=>[f,hash(fs.readFileSync(`${repo}/${f}`))]));
 for(const f of inputs){const out=`${dir}/inputs/${f}`;fs.mkdirSync(path.dirname(out),{recursive:true});fs.copyFileSync(`${repo}/${f}`,out);}
 fs.copyFileSync('.rush/native-owner/calibrate-mixed.mjs',`${dir}/calibrate.mjs`);
 fs.writeFileSync(`${dir}/identity.json`,JSON.stringify({captured_before_build:new Date().toISOString(),base,patch_sha256:hash(patch),files,features:'production',cpu:os.cpus()[0].model,os:os.release(),rustc:run('rustc',['-Vv']),cargo:run('cargo',['-V']),pilot_repeats:16,target_ns:100000000,AA_blocks:10,meaningful_fraction:0.05,max_halfwidth_fraction:0.025},null,2));
 const env={...process.env,CARGO_TARGET_DIR:target};delete env.CARGO_PROFILE_RELEASE_DEBUG;
 console.log('Build production-only installed workload benchmarks');
 const p=spawnSync('cargo',['build','--locked','--release','--features','production','--example','native_mixed_sefer','--example','native_mixed_mimalloc'],{cwd:repo,env,encoding:'utf8',maxBuffer:32*1024*1024});
 fs.writeFileSync(`${dir}/build.log`,clean(`${p.stdout??''}\n${p.stderr??''}`));assert.equal(p.status,0,p.stderr);
 const binaries=Object.fromEntries(['sefer','mimalloc'].map(arm=>[arm,hash(fs.readFileSync(path.join(target,`release/examples/native_mixed_${arm}.exe`)))]));
 fs.writeFileSync(`${dir}/binaries.json`,JSON.stringify(binaries,null,2));
 function sample(repeats,stage,block,position){
 const exe=path.join(target,'release/examples/native_mixed_sefer.exe'),start=process.hrtime.bigint();
 const r=spawnSync(exe,[String(repeats)],{cwd:repo,encoding:'utf8'}),process_ns=Number(process.hrtime.bigint()-start);
 const raw={stage,block,position,repeats,status:r.status,process_ns,stdout:clean(r.stdout??''),stderr:clean(r.stderr??'')};
 fs.appendFileSync(`${dir}/processes.ndjson`,JSON.stringify(raw)+'\n');assert.equal(r.status,0,r.stderr);
 const data=JSON.parse(r.stdout.trim());assert(data.completed&&data.arm==='sefer');assert.equal(data.repeats,repeats);assert(data.segments_reserved_total>0);assert.equal(data.warmup_rounds,1);assert(data.workload_ns>0&&process_ns>data.workload_ns);
 fs.appendFileSync(`${dir}/samples.ndjson`,JSON.stringify({...raw,stdout:undefined,stderr:undefined,...data})+'\n');return data;
 }
 const pilot=sample(16,'pilot',-1,0),repeats=Math.max(1,Math.min(4096,Math.ceil(100000000/(pilot.workload_ns/16))));
 fs.writeFileSync(`${dir}/frozen.json`,JSON.stringify({frozen_before_validation:new Date().toISOString(),pilot_workload_ns:pilot.workload_ns,repeats,warmup_workload_rounds:1,AA_blocks:10,positions:[0,1,2,3]},null,2));
 console.log(`Frozen repeats=${repeats}; validate A/A10 blocks, no retuning`);
 for(let block=0;block<10;block++)for(let position=0;position<4;position++)sample(repeats,'validation',block,position);
 assert.equal(hash(run('git',['diff','--binary','HEAD','--','src','Cargo.toml','Cargo.lock','examples'])),hash(patch));
 for(const f of inputs)assert.equal(hash(fs.readFileSync(`${repo}/${f}`)),files[f]);
 for(const [arm,sha] of Object.entries(binaries))assert.equal(hash(fs.readFileSync(path.join(target,`release/examples/native_mixed_${arm}.exe`))),sha);
 fs.writeFileSync(`${dir}/completed.json`,JSON.stringify({completed:new Date().toISOString(),samples:41,binary_drift:false,source_drift:false,tests_executed:false},null,2));
}
assert(JSON.parse(fs.readFileSync(`${dir}/completed.json`)).samples===41);
const meta=JSON.parse(fs.readFileSync(`${dir}/identity.json`)),frozen=JSON.parse(fs.readFileSync(`${dir}/frozen.json`));
const rows=fs.readFileSync(`${dir}/samples.ndjson`,'utf8').trim().split('\n').map(JSON.parse);assert.equal(rows.length,41);
const pilot=rows.filter(r=>r.stage==='pilot');assert.equal(pilot.length,1);assert.equal(frozen.repeats,Math.max(1,Math.min(4096,Math.ceil(100000000/(pilot[0].workload_ns/16)))));
const mean=a=>a.reduce((s,x)=>s+x,0)/a.length,summary=[];
for(const metric of ['workload_ns','process_ns']){
 const diffs=[],all=[];
 for(let block=0;block<10;block++){
 const r=rows.filter(x=>x.stage==='validation'&&x.block===block).sort((a,b)=>a.position-b.position);assert.deepEqual(r.map(x=>x.position),[0,1,2,3]);assert(r.every(x=>x.repeats===frozen.repeats));
 const values=r.map(x=>x[metric]/x.repeats);all.push(...values);diffs.push(mean([values[1],values[2]])-mean([values[0],values[3]]));
 }
 const baseline=mean(all),delta=mean(diffs),halfwidth=2.262157163*Math.sqrt(diffs.reduce((s,x)=>s+(x-delta)**2,0)/9/10),low=delta-halfwidth,high=delta+halfwidth;
 const halfwidth_fraction=halfwidth/baseline;assert(Math.abs(halfwidth_fraction*baseline-halfwidth)<Math.max(1,halfwidth)*1e-12);
 summary.push({metric,samples:40,base:meta.base,features:meta.features,cpu:meta.cpu,os:meta.os,repeats:frozen.repeats,baseline_ns_round:baseline,delta_ns_round:delta,low,high,halfwidth_fraction,sensitivity_pass:halfwidth_fraction<=0.025,position_bias_not_detected:low<=0&&high>=0});
}
const judge=summary[0].sensitivity_pass&&summary[0].position_bias_not_detected,outputs=new Map();
outputs.set(`${dir}/summary.json`,JSON.stringify({judge_pass:judge,summary},null,2)+'\n');
const keys=Object.keys(summary[0]),csv=keys.join(',')+'\n'+summary.map(r=>keys.map(k=>JSON.stringify(r[k])).join(',')).join('\n')+'\n';
outputs.set(`${repo}/docs/perf/NATIVE_MIXED_CALIBRATION_summary.csv`,csv);
outputs.set(`${repo}/docs/perf/NATIVE_MIXED_CALIBRATION.md`,`# Native installed judge calibration — measurement only\n\nВердикт чувствительности: **${judge?'PASS':'FAIL / DO NOT JUDGE PRODUCT CANDIDATE'}**. Это не speedup allocator. Pinned source и raw41 processes: [calibration1](native_mixed/calibration1/identity.json). Один pilot, затем frozen repeats=${frozen.repeats},10A/Ablocks×4processes. Installed Sefer production-only, никакого candidate A/B или mimalloc timing на этом этапе. Механизм установки подтверждён segments_reserved_total>0; refill/flush этим не доказаны.\n\nArithmetic means; Student-t95CI по inner−outer block means. Значимость meaningful effect=5/100 baseline full-workload mean; максимум halfwidth=2.5/100 того же mean. Все samples сохранены, retuning нет.\n\n| Metric | Baseline ns/full workload round | Delta inner−outer | Paired95 CI | Halfwidth/baseline | Sensitivity | Position bias not detected |\n|---|---:|---:|---|---:|---|---|\n${summary.map(r=>`| ${r.metric} | ${r.baseline_ns_round} | ${r.delta_ns_round} | ${r.low} … ${r.high} | ${r.halfwidth_fraction} | ${r.sensitivity_pass} | ${r.position_bias_not_detected} |`).join('\n')}\n\nworkload_ns включает setup/teardown всех workload rounds, но не одну warmup run_workload. process_ns включает startup/warmup/output/exit и post-timing RSS probes; это другой denominator по содержанию. Проба не Criterion; Criterion mean отсутствует. Изменённый judge не исправляет задним числом предыдущий leaf INCONCLUSIVE. Tests/CI acceptance не выполнялась.\n`);
for(const [file,text] of outputs){if(process.argv.includes('--check'))assert.equal(fs.readFileSync(file,'utf8'),text,file);else fs.writeFileSync(file,text);}
console.log(JSON.stringify({judge_pass:judge,summary},null,2));
