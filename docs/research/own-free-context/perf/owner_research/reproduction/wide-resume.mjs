import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),repos={before:path.join(root,'worktrees/own-free-before'),after:path.join(root,'worktrees/own-free-context')},ev=path.join(repos.after,'docs/perf/own_free_context/wide');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function exec(cmd,args,cwd,opts={}){const p=spawnSync(cmd,args,{cwd,encoding:'utf8',maxBuffer:128*1024*1024,...opts});if(p.error||p.status!==0)throw Error(`${cmd}: ${p.error??p.status}\n${p.stderr}`);return p.stdout;}
const wsl=p=>'/mnt/'+p[0].toLowerCase()+p.slice(2).replaceAll(String.fromCharCode(92),'/');
const receipts={captured_before_measure:new Date().toISOString(),source_identity_sha256:sha(fs.readFileSync(path.join(ev,'identity.json'))),reason:'direct built bench binary; prior cargo-run rebuilt executable and was rejected',binaries:{}};
if(fs.existsSync(path.join(ev,'before.log'))&&!fs.existsSync(path.join(ev,'before-invalid-cargo-rebuild.log')))fs.renameSync(path.join(ev,'before.log'),path.join(ev,'before-invalid-cargo-rebuild.log'));
const bins={};
for(const [phase,repo] of Object.entries(repos)){
 assert.equal(exec('git',['diff','--binary','--','src','Cargo.toml','Cargo.lock'],repo),fs.readFileSync(path.join(ev,`${phase}.patch`),'utf8'));
 const out=path.join(root,`.rush/own-free-context/iai-${phase}`),messages=fs.readFileSync(path.join(out,'build.jsonl'),'utf8').trim().split('\n').map(s=>{try{return JSON.parse(s);}catch{return null;}}),found=messages.filter(x=>x?.reason==='compiler-artifact'&&x.target?.name==='perf_gate_iai'&&x.executable);assert.equal(found.length,1);
 const linux=found[0].executable,win=linux[5].toUpperCase()+':'+linux.slice(6).replaceAll('/',path.sep);bins[phase]={linux,win,out};receipts.binaries[phase]={sha256:sha(fs.readFileSync(win)),target:'x86_64-unknown-linux-gnu'};
}
fs.writeFileSync(path.join(ev,'attempt2-identity.json'),JSON.stringify(receipts,null,2));
const observed={};
function clean(s){for(const p of [root,root.replaceAll(String.fromCharCode(92),'/'),wsl(root)])s=s.split(p).join('<workspace>');return s.replaceAll('/root/','~/');}
function parse(s){const cells={};let cell=null;for(const raw of s.split(/\r?\n/)){const line=raw.replace(/\x1b\[[0-9;]*m/g,''),h=/^\s*([A-Za-z_]\w*::)+([A-Za-z_]\w*)\b/.exec(line);if(h){cell=h[2];continue;}const m=/^\s*Instructions:\s*([\d,]+)(?:\||\s|$)/.exec(line);if(m&&cell&&cells[cell]===undefined)cells[cell]=Number(m[1].replaceAll(',',''));}assert(Object.keys(cells).length>20);return cells;}
for(const [phase,repo] of Object.entries(repos)){
 console.log(`Exact IAI binary measure ${phase}`);
 exec('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',wsl(path.join(root,'.rush/own-free-context/iai.sh')),wsl(repo),wsl(bins[phase].out),'measure',bins[phase].linux],root,{stdio:'inherit'});
 assert.equal(sha(fs.readFileSync(bins[phase].win)),receipts.binaries[phase].sha256);
 const s=fs.readFileSync(path.join(bins[phase].out,'measure.stdout'),'utf8')+'\n'+fs.readFileSync(path.join(bins[phase].out,'measure.stderr'),'utf8');fs.writeFileSync(path.join(ev,`${phase}.log`),clean(s));observed[phase]=parse(s);
}
assert.deepEqual(Object.keys(observed.before).sort(),Object.keys(observed.after).sort());
const rows=Object.keys(observed.before).sort().map(cell=>{const a=observed.before[cell],b=observed.after[cell];assert(a>0&&b>0);return {cell,a,b,delta:b-a,delta_pct:(b-a)/a*100};});
fs.writeFileSync(path.join(ev,'summary.json'),JSON.stringify(rows,null,2));fs.writeFileSync(path.join(ev,'summary.csv'),['cell,a_ir,b_ir,delta_ir,delta_pct',...rows.map(r=>[r.cell,r.a,r.b,r.delta,r.delta_pct].join(','))].join('\n')+'\n');
fs.writeFileSync(path.join(ev,'completed.json'),JSON.stringify({completed:new Date().toISOString(),cells:rows.length,attempt:2,binary_drift:false},null,2));
console.log(JSON.stringify({cells:rows.length,increased:rows.filter(r=>r.delta>0),decreased:rows.filter(r=>r.delta<0).length},null,2));
