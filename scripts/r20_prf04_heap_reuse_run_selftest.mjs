// Injected E2E adapter tests: fake process/git/build/measurement transport; production orchestration, profile parsing, and validators stay real.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { createRunner, parseCLI, HASHES } from './r20_prf04_heap_reuse_run.mjs';
const sha=x=>createHash('sha256').update(x).digest('hex');
const L1_NAMES=['r20_prf04hr_start',...['cold','recycled','wave','wave_release'].flatMap(f=>['n','2n','4n'].map(s=>`r20_prf04hr_${f}_${s}`))];
const L2_NAMES=['l2_cold','l2_recycled','l2_wave_release'].flatMap(f=>['n','2n','4n'].map(s=>`r20_prf04hr_${f}_${s}`));
const frozen=()=>HASHES.map(path=>({path,sha256:sha(path)}));
const ALLOC='r20_prf04_heap_reuse_l2_iai::hr::child_first_alloc';
const GUARD='<sefer_alloc::global::tls_heap::AbandonGuard as core::ops::Drop>::drop';
function profile(tid,symbol,cost,cmd,{dual=false,release=false}={}){
 const caller=release?'std::sys::thread_local::destructors':'std::thread::{{closure}}';
 let lines;
 if(tid==='1'||symbol===null) lines=`fn=parent\n1 ${cost}\n`;
 else if(release) lines=`fn=std::sys::thread_local::destructors\n1 0\ncfn=${GUARD}\ncalls=1 1\n1 0\nfn=${GUARD}\n1 ${cost}\n`;
 else if(dual&&symbol===ALLOC) lines=`fn=std::thread::{{closure}}\n1 0\ncfn=${ALLOC}\ncalls=1 1\n1 0\nfn=${ALLOC}\n1 10\nfn=std::sys::thread_local::destructors\n1 0\ncfn=${GUARD}\ncalls=1 1\n1 0\nfn=${GUARD}\n1 5\n`;
 else lines=`fn=${caller}\n1 0\ncfn=${symbol}\ncalls=1 1\n1 0\nfn=${symbol}\n1 ${cost}\n`;
 return `pid: 1701\npart: 1\nthread: ${tid}\ncmd: ${cmd}\npositions: line\nevents: Ir\nsummary: 0\n${lines}totals: ${cost}\n`;
}
function synth({cell,index,layer,exe,bad='',releaseLayer='L2',rehearsal=false}){
 const isL2=layer==='L2',release=cell.family==='l2_wave_release',dual=cell.family==='l2_recycled'&&releaseLayer==='L2';
 const n=cell.family==='start'?1:Number(({n:16,'2n':32,'4n':64})[cell.size]);
 const kids=isL2?(release?64:n):0,profileKids=bad==='count'&&isL2&&!release?Math.max(0,kids-1):kids,group=isL2?'heap_reuse_l2':'heap_reuse';
 const cmd=`${exe} --iai-run ${group} ${index} 0`,path=`${exe}/target/iai-callgrind/${group}/${cell.name}/callgrind.out`;
 const toggles=isL2?(release?[GUARD]:dual?[ALLOC,GUARD]:[ALLOC]):[];
 const profiles=[profile('1',null,isL2?0:100,cmd)];
 for(let i=0;i<profileKids;i++){
  const active=release?i<n:false,symbol=release?(active?GUARD:null):ALLOC,cost=release?(active?5:0):dual?15:10;
  let p=profile(String(i+2),symbol,cost,cmd,{dual,release:release&&active});
  if(bad==='extra'&&i===0)p=p.replace(`totals: ${cost}`,`fn=unrelated\n1 1\ntotals: ${cost+1}`);
  if(bad==='zero'&&i===0)p=profile(String(i+2),symbol,0,cmd,{dual,release:release&&active});
  if(bad==='missing'&&i===0)continue;
  profiles.push(p);
 }
 const total=isL2?(release?n*5:kids*(dual?15:10)):100;
 const actualToggles=bad==='toggle'?toggles.filter(x=>x!==GUARD):toggles;
 return{code:0,cells:{[cell.name]:{profiles,log:`==1701== Command: ${cmd}\nInstructions: ${total}\n`,debug:`callgrind: Arguments: --callgrind-out-file=${path} --separate-threads=yes ${actualToggles.map(x=>'--toggle-collect='+x).join(' ')}\n`,raw:`Instructions: ${total}\n`,actualInstructions:total}}};
}
function transport({badMeasurement='',judgeMode='L2'}={}){
 let launches=0,judges=0;
 const t={
  execute:async({argv,env})=>{launches++;const o=argv[0].includes('fake-bins')?spawnSync(process.execPath,argv,{encoding:'utf8',env:{...process.env,...env}}):{status:0,stdout:'{}',stderr:''};return{code:o.status??0,stdout:o.stdout??'',stderr:o.stderr??''};},
  git:async(c,a)=>{const arm=c.split(/[\\/]/).at(-1);if(a[0]==='rev-parse')return{code:0,stdout:a[1]==='HEAD^{tree}'?'b'.repeat(40):arm==='S'?'c'.repeat(40):'a'.repeat(40)};if(a[0]==='merge-base')return{code:0,stdout:'a'.repeat(40)};if(a[0]==='worktree'&&a[1]==='add'){fs.mkdirSync(a[3],{recursive:true});return{code:0,stdout:''};}if(a[0]==='status')return{code:0,stdout:''};if(a[0]==='diff'||a[0]==='ls-files')return{code:0,stdout:''};return{code:0,stdout:''};},
  environment:async()=>({platform:'fake',arch:'fake',node:'fake'}),
  build:async({arm,role,out})=>{const exe=join(out,'fake-bins',`${arm}-${role}.js`);fs.mkdirSync(join(out,'fake-bins'),{recursive:true});const src=role.startsWith('oracle')?`const [family,ns]=process.argv.slice(2),arm=process.env.R20_PRF04HR_ARM,n=Number(ns),A=arm==='A',wave=family.includes('wave'),release=family.endsWith('wave_release'),l2=family.startsWith('l2');console.log(JSON.stringify({arm,family,n,ok:true,configConflictsDelta:0,bumpDelta:family==='start'?1:wave?(A?n-1:0):family.includes('cold')?n:0,hintDelta:wave?1:family.includes('recycled')?n:0,reclaimDelta:family.includes('recycled')?n:0,newHeaps:family==='start'?1:wave&&A?n-1:family.includes('cold')?n:0,freeSetDelta:wave&&!A?n-1:0,popcount:release?(A?0:n-1):wave&&!A?64-n:wave?0:n,claimSourcesLen:A?4:5,missingFreeSetExplicitZero:A,popcountAvailable:!A,targetClaimDelta:release?64:n,firstAllocActivated:true,shippingLayer:l2}))`:`// ${arm}/${role}\nconsole.log('fake')`;fs.writeFileSync(exe,src);return{exe,symbols:[`000 T ${ALLOC}`,`000 T ${GUARD}`]};},
  inventory:async(exe,l)=>(l==='L1'?L1_NAMES:L2_NAMES).map(name=>({name})),
  measureLayer:async a=>{const fixture=join(a.out,`fake-benchmark-${a.arm}-${a.layer}.mjs`);fs.writeFileSync(fixture,`const ALLOC=${JSON.stringify(ALLOC)},GUARD=${JSON.stringify(GUARD)};${profile.toString()}\n${synth.toString()}\nconst a=JSON.parse(process.argv[2]);console.log(JSON.stringify(synth(a)))`);const r=spawnSync(process.execPath,[fixture,JSON.stringify({...a,bad:badMeasurement,releaseLayer:judgeMode})],{encoding:'utf8'});assert.equal(r.status,0,r.stderr);return JSON.parse(r.stdout);},
judge:async({document})=>{judges++;assert.equal(document.releaseLayer,judgeMode);assert.equal(document.releaseMode,judgeMode);assert.equal(document.cells.length,judgeMode==='L2'?110:95);return{code:0,stdout:'verdict: GO\n',stderr:'',result:{verdict:'GO'}};}
 };
 Object.defineProperty(t,'counts',{get:()=>({launches,judges})});return t;
}
const runner=(root,out,tr,o={})=>createRunner({root,out,transport:tr,candidate:o.candidate??'c'.repeat(40),releaseLayer:o.releaseLayer??'L2',frozen});
const primeOracle=out=>{const m=JSON.parse(fs.readFileSync(join(out,'manifest.json'))),b=JSON.parse(fs.readFileSync(join(out,'builds.json')));fs.writeFileSync(join(out,'oracle.json'),JSON.stringify({manifestSha256:sha(fs.readFileSync(join(out,'manifest.json'))),buildsSha256:sha(fs.readFileSync(join(out,'builds.json'))),environmentSha256:b.environmentSha256,records:{A:{},S:{}}}));};
export async function selftest(){
 const root=fs.mkdtempSync(join(process.cwd(),'target','.r20-prf04-selftest-'));let checks=0,measurements=0;
 try{
  assert.equal(parseCLI(['--prepare','--out-dir',join(root,'cli')]).stage,'prepare');assert.throws(()=>parseCLI(['--prepare','--build']));checks+=2;
  const ro=join(root,'rehearsal');fs.mkdirSync(ro);const rt=transport(),rr=runner(root,ro,rt,{candidate:'TBD'});await assert.rejects(rr.prepare(),/candidate placeholder TBD/);const rm=await runner(root,ro,rt).rehearsal();assert.equal(rm,'REHEARSAL:A_ONLY_COMPLETE');const rec=JSON.parse(fs.readFileSync(join(ro,'rehearsal-recommendation.json')));assert.equal(rec.recommendedReleaseLayer,'L2');assert.equal(rec.verdict,'REHEARSAL_ONLY');assert.equal(rt.counts.judges,0);checks+=4;
  for(const mode of ['L2','L1']){const out=join(root,`full-${mode}`);fs.mkdirSync(out);const tr=transport({judgeMode:mode}),r=runner(root,out,tr,{releaseLayer:mode});const m=await r.prepare();assert.equal(m.candidate,'c'.repeat(40));const b=await r.build();assert.ok(b.arms.A.L1.binarySha256);primeOracle(out);await r.oracle();assert.equal((await r.measure()).verdict,'GO');assert.equal(tr.counts.judges,1);assert.equal(JSON.parse(fs.readFileSync(join(out,'judge-input.json'))).cells.length,mode==='L2'?110:95);checks+=6;measurements+=tr.counts.launches;}
  const out=join(root,'drift');fs.mkdirSync(out);const dt=transport(),dr=runner(root,out,dt);await dr.prepare();const db=await dr.build();primeOracle(out);await dr.oracle();const bytes=fs.readFileSync(db.arms.A.L1.exe);fs.appendFileSync(db.arms.A.L1.exe,'drift');const before=dt.counts.launches;await assert.rejects(dr.oracle(),/binary drift|binary changed|receipt mismatch/);assert.equal(dt.counts.launches,before);fs.writeFileSync(db.arms.A.L1.exe,bytes);checks+=2;
  const io=join(root,'invalid-count');fs.mkdirSync(io);const it=transport({badMeasurement:'count'}),ir=runner(root,io,it);await ir.prepare();await ir.build();primeOracle(io);await ir.oracle();await assert.rejects(ir.measure(),/child count mismatch/);assert.equal(it.counts.judges,0);checks+=2;
  for(const bad of ['extra','zero','toggle','missing']){const outBad=join(root,`mutant-${bad}`);fs.mkdirSync(outBad);const tr=transport({badMeasurement:bad}),r=runner(root,outBad,tr);await r.prepare();await r.build();primeOracle(outBad);await r.oracle();await assert.rejects(r.measure(),/profile|toggle|cost|mismatch|execution|Instructions|totals|falsy|inclusive/i);assert.equal(tr.counts.judges,0);checks++;}
  const mo=join(root,'mismatch');fs.mkdirSync(mo);const mt=transport({judgeMode:'L2'}),mr=runner(root,mo,mt,{releaseLayer:'L1'});await mr.prepare();await mr.build();primeOracle(mo);await mr.oracle();await assert.rejects(mr.measure());checks++;
  console.log(JSON.stringify({status:'PASS',checks,measurements,stages:['prepare','build','oracle','measure','rehearsal'],releaseModes:['L2','L1'],mutants:['extra','zero','toggle','missing','count']}));
 }finally{fs.rmSync(root,{recursive:true,force:true});}
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url))selftest().catch(e=>{console.error(e.stack);process.exitCode=1;});
