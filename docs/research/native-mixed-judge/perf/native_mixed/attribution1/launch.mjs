import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),repo='worktrees/native-mixed-judge',dir=`${repo}/docs/perf/native_mixed/attribution1`,out='.rush/native-owner/mixed-ir';
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
function linux(p){return '/mnt/'+p[0].toLowerCase()+p.slice(2).replaceAll('\\','/');}
function run(cmd,args,cwd=repo){const p=spawnSync(cmd,args,{cwd,encoding:'utf8',maxBuffer:32*1024*1024});assert.equal(p.status,0,p.stderr);return p.stdout;}
assert(!fs.existsSync(dir));fs.mkdirSync(dir,{recursive:true});
const old=JSON.parse(fs.readFileSync(`${repo}/docs/perf/native_mixed/calibration1/identity.json`));
const patch=run('git',['diff','--binary','HEAD','--','src','Cargo.toml','Cargo.lock','examples']);assert.equal(hash(patch),old.patch_sha256);
for(const [f,sha] of Object.entries(old.files))assert.equal(hash(fs.readFileSync(`${repo}/${f}`)),sha);
fs.writeFileSync(`${dir}/source.patch`,patch);
const metadata={captured_before_build:new Date().toISOString(),base:old.base,patch_sha256:hash(patch),files:old.files,features:'production',layer:'installed global allocator; same churn/Vec workload as calibration',repeats:[120,241],warmup_rounds:1,native_judge:'FAILED CALIBRATION; these profiles cannot establish native speedup',rustc:run('wsl.exe',['-e','bash','-lc','rustc -Vv']),valgrind:run('wsl.exe',['-e','valgrind','--version']),cpu:old.cpu,host_os:old.os};
fs.writeFileSync(`${dir}/identity.json`,JSON.stringify(metadata,null,2));fs.copyFileSync('.rush/native-owner/mixed-attribution.sh',`${dir}/profile.sh`);
console.log('Build/profile installed full workload: Sefer/mimalloc ×120/241, serial');
const p=spawnSync('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',linux(path.join(root,'.rush/native-owner/mixed-attribution.sh')),linux(path.join(root,repo)),linux(path.join(root,out))],{stdio:'inherit'});assert.equal(p.status,0);
const rows=[];
for(const arm of ['sefer','mimalloc'])for(const repeats of [120,241]){
 const stem=`${out}/${arm}-${repeats}`,raw=fs.readFileSync(`${stem}.callgrind`,'utf8'),stdout=fs.readFileSync(`${stem}.stdout`,'utf8'),data=JSON.parse(stdout.trim());assert(data.completed);assert.equal(data.repeats,repeats);assert.equal(data.segments_reserved_total>0,arm==='sefer');
 const total=Number(/^summary:\s*(\d+)/m.exec(raw)[1]);assert(total>0);
 const record={arm,repeats,total_ir:total,raw_sha256:hash(raw),raw_bytes:Buffer.byteLength(raw),...data};rows.push(record);
 fs.writeFileSync(`${dir}/${arm}-${repeats}.receipt.json`,JSON.stringify(record,null,2));
 fs.copyFileSync(`${stem}.stdout`,`${dir}/${arm}-${repeats}.stdout`);
 const normalized=raw.split(linux(root)).join('<workspace>').split(root).join('<workspace>');
 if(Buffer.byteLength(normalized)<200*1024)fs.writeFileSync(`${dir}/${arm}-${repeats}.callgrind`,normalized);
 else{
 const z=await import('node:zlib');assert(Buffer.byteLength(normalized)<=2*1024*1024,'Нужен tier3manifest, не giantartifact');fs.writeFileSync(`${dir}/${arm}-${repeats}.callgrind.gz`,z.gzipSync(normalized));
 }
}
assert.equal(hash(run('git',['diff','--binary','HEAD','--','src','Cargo.toml','Cargo.lock','examples'])),metadata.patch_sha256);
fs.writeFileSync(`${dir}/samples.json`,JSON.stringify(rows,null,2));fs.writeFileSync(`${dir}/completed.json`,JSON.stringify({completed:new Date().toISOString(),profiles:4,source_drift:false,binary_drift:false,native_speedup_claim:false},null,2));
console.log(JSON.stringify(rows,null,2));
