import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),repos={before:path.join(root,'worktrees/own-free-before'),after:path.join(root,'worktrees/own-free-context')};
const ev=path.join(repos.after,'docs/perf/own_free_context/wide');assert(!fs.existsSync(path.join(ev,'identity.json')),'Не перезаписывать существующий замер');fs.mkdirSync(ev,{recursive:true});
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function exec(cmd,args,cwd,opts={}){const p=spawnSync(cmd,args,{cwd,encoding:'utf8',maxBuffer:128*1024*1024,...opts});if(p.error||p.status!==0)throw Error(`${cmd}: ${p.error??p.status}\n${p.stdout}\n${p.stderr}`);return p.stdout;}
function wsl(p){return '/mnt/'+p[0].toLowerCase()+p.slice(2).replaceAll(String.fromCharCode(92),'/');}
const manifest={captured_before_build:new Date().toISOString(),features:'production bench-internals internals',bench:'perf_gate_iai',identities:{}};
for(const [phase,repo] of Object.entries(repos)){
 const patch=exec('git',['diff','--binary','--','src','Cargo.toml','Cargo.lock'],repo);if(phase==='before')assert.equal(exec('git',['diff','--','src','Cargo.lock'],repo),'');
 fs.writeFileSync(path.join(ev,`${phase}.patch`),patch);
 manifest.identities[phase]={base:exec('git',['rev-parse','HEAD'],repo).trim(),patch_sha256:sha(patch),bench_sha256:sha(fs.readFileSync(path.join(repo,'benches/perf_gate_iai.rs')))};
}
assert.equal(manifest.identities.before.bench_sha256,manifest.identities.after.bench_sha256);
fs.writeFileSync(path.join(ev,'identity.json'),JSON.stringify(manifest,null,2));
const built={};
for(const [phase,repo] of Object.entries(repos)){
 const out=path.join(root,`.rush/own-free-context/iai-${phase}`);fs.mkdirSync(out,{recursive:true});
 console.log(`Wide IAI build ${phase}`);
 exec('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',wsl(path.join(root,'.rush/own-free-context/iai.sh')),wsl(repo),wsl(out),'build'],root,{stdio:'inherit'});
 const messages=fs.readFileSync(path.join(out,'build.jsonl'),'utf8').trim().split('\n').map(s=>{try{return JSON.parse(s);}catch{return null;}});
 const found=messages.filter(x=>x?.reason==='compiler-artifact'&&x.target?.name==='perf_gate_iai'&&x.executable);assert.equal(found.length,1);
 const linuxExe=found[0].executable;assert(linuxExe.startsWith('/mnt/'));
 const winExe=linuxExe[5].toUpperCase()+':'+linuxExe.slice(6).replaceAll('/',path.sep);
 const binarySha=sha(fs.readFileSync(winExe));built[phase]={out,binarySha,winExe,linuxExe};
 fs.writeFileSync(path.join(ev,`binary-${phase}.json`),JSON.stringify({sha256:binarySha,target:'x86_64-unknown-linux-gnu'},null,2));
}
function clean(s){const r=root.replaceAll(String.fromCharCode(92),'/');for(const p of [root,r,wsl(root)])s=s.split(p).join('<workspace>');return s.replaceAll('/root/','~/');}
function parse(text){const cells={};let current=null;for(const raw of text.split(/\r?\n/)){const line=raw.replace(/\x1b\[[0-9;]*m/g,'');const h=/^\s*([A-Za-z_]\w*::)+([A-Za-z_]\w*)\b/.exec(line);if(h){current=h[2];continue;}const m=/^\s*Instructions:\s*([\d,]+)(?:\||\s|$)/.exec(line);if(m&&current&&cells[current]===undefined)cells[current]=Number(m[1].replaceAll(',',''));}assert(Object.keys(cells).length>20,'missing IAI cells');return cells;}
const observed={};
for(const [phase,repo] of Object.entries(repos)){
 console.log(`Wide IAI measure ${phase}`);
 exec('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',wsl(path.join(root,'.rush/own-free-context/iai.sh')),wsl(repo),wsl(built[phase].out),'measure',built[phase].linuxExe],root,{stdio:'inherit'});
 const text=fs.readFileSync(path.join(built[phase].out,'measure.stdout'),'utf8')+'\n'+fs.readFileSync(path.join(built[phase].out,'measure.stderr'),'utf8');
 fs.writeFileSync(path.join(ev,`${phase}.log`),clean(text));observed[phase]=parse(text);
 assert.equal(sha(fs.readFileSync(built[phase].winExe)),built[phase].binarySha);
 const patch=fs.readFileSync(path.join(ev,`${phase}.patch`),'utf8');assert.equal(exec('git',['diff','--binary','--','src','Cargo.toml','Cargo.lock'],repo),patch);
}
assert.deepEqual(Object.keys(observed.before).sort(),Object.keys(observed.after).sort());
const rows=Object.keys(observed.before).sort().map(cell=>{const a=observed.before[cell],b=observed.after[cell];assert(a>0&&b>0);return {cell,a,b,delta:b-a,delta_pct:(b-a)/a*100};});
fs.writeFileSync(path.join(ev,'summary.json'),JSON.stringify(rows,null,2));
fs.writeFileSync(path.join(ev,'summary.csv'),['cell,a_ir,b_ir,delta_ir,delta_pct',...rows.map(r=>[r.cell,r.a,r.b,r.delta,r.delta_pct].join(','))].join('\n')+'\n');
console.log(JSON.stringify({cells:rows.length,increased:rows.filter(r=>r.delta>0),decreased:rows.filter(r=>r.delta<0).length},null,2));
fs.writeFileSync(path.join(ev,'completed.json'),JSON.stringify({completed:new Date().toISOString(),cells:rows.length,binary_drift:false},null,2));
