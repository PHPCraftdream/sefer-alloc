import fs from 'node:fs';
import path from 'node:path';
import {gzipSync,gunzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
const root=process.cwd(),dest=path.join(root,'worktrees/own-free-context/docs/perf/owner_research');fs.mkdirSync(dest,{recursive:true});
const phases={scalar:'own-free-single',context:'own-free-context',mir:'own-free-inline'};
const slash=p=>p.replaceAll(String.fromCharCode(92),'/'),win=slash(root),linux='/mnt/'+win[0].toLowerCase()+win.slice(2);
function clean(s){for(const p of [root,win,linux])s=s.split(p).join('<workspace>');return s.replaceAll('/root/','~/');}
function copyProfiles(source,out){fs.mkdirSync(out,{recursive:true});for(const f of fs.readdirSync(source)){
 if(!/\.(callgrind|annotate|stdout|stderr)$/.test(f))continue;
 let s=fs.readFileSync(path.join(source,f),'utf8');
 if(f.endsWith('.annotate')){const at=s.indexOf('-- Auto-annotated source:');if(at>=0)s=s.slice(0,at)+'# TRUNCATED — source listings omitted; full output: rebuild with reproduction/profile.sh then callgrind_annotate --auto=yes --threshold=100 <profile>.callgrind\n';}
 const b=Buffer.from(clean(s));assert(b.length<=2*1024*1024);
 fs.writeFileSync(path.join(out,f+(b.length>200*1024?'.gz':'')),b.length>200*1024?gzipSync(b):b);
 }}
copyProfiles(path.join(root,'.rush/own-free-context/baseline'),path.join(dest,'baseline'));
copyProfiles(path.join(root,'.rush/own-free-context/control-before'),path.join(dest,'control-before'));
const rows=[];
for(const [label,slug] of Object.entries(phases)){
 const source=path.join(root,'worktrees',slug,'docs/perf',slug.replaceAll('-','_')),out=path.join(dest,label);fs.mkdirSync(out,{recursive:true});
 for(const f of ['baseline_identity.json','candidate_identity.json','baseline_summary.json','candidate_summary.json','comparison_identity.json'])fs.copyFileSync(path.join(source,f),path.join(out,f));
 for(const name of ['baseline','candidate','comparison-before','comparison-after']){const plain=path.join(source,name+'.patch');const b=fs.existsSync(plain)?fs.readFileSync(plain):gunzipSync(fs.readFileSync(plain+'.gz'));fs.writeFileSync(path.join(out,name+'.patch.gz'),gzipSync(b));}
 for(const f of ['own_free_profile.rs','own_free_wall.rs'])fs.copyFileSync(path.join(root,'worktrees',slug,'examples',f),path.join(out,f));
 const controls=JSON.parse(fs.readFileSync(path.join(source,'raw/overflow_controls.json')));fs.writeFileSync(path.join(out,'controls.json'),JSON.stringify(controls,null,2));
 copyProfiles(path.join(root,'.rush',slug,'candidate'),path.join(out,'free'));
 copyProfiles(path.join(root,'.rush',slug,'control-after'),path.join(out,'control'));
 for(const tag of ['baseline','candidate']){const p=path.join(root,'.rush',slug,tag,'toolchain.log');if(fs.existsSync(p))fs.writeFileSync(path.join(out,tag+'_toolchain.log'),clean(fs.readFileSync(p,'utf8')));}
 const summary=JSON.parse(fs.readFileSync(path.join(out,'candidate_summary.json')));
 for(const r of summary.rows.filter(r=>r.pass===1))rows.push({round:label,axis:'free',arm:r.arm,mode:r.mode,Ir:r.marginal_ir_per_free});
 for(const r of controls.filter(r=>r.phase==='after'&&r.pass===1))rows.push({round:label,axis:'control',arm:r.arm,mode:r.mode,Ir:r.marginal_ir_per_free});
}
fs.writeFileSync(path.join(dest,'rounds.json'),JSON.stringify(rows,null,2));
const rep=path.join(dest,'reproduction');fs.mkdirSync(rep,{recursive:true});
for(const f of ['profile.sh','controls.sh','profile.mjs','compare.mjs','controls-only.mjs','native-only.mjs','wide.mjs','wide-resume.mjs','iai.sh','alloc-sanity.sh','alloc-sanity.mjs','clear-asm.mjs','sanity-summary.mjs','native-summary.mjs','canonical.mjs','archive-rounds.mjs']){const p=path.join(root,'.rush/own-free-context',f);if(fs.existsSync(p))fs.copyFileSync(p,path.join(rep,f));}
const sanity=path.join(dest,'sanity');fs.mkdirSync(sanity,{recursive:true});
copyProfiles(path.join(root,'.rush/own-free-context/alloc-sanity'),path.join(sanity,'alloc'));
for(const f of ['before.asm','after.asm'])fs.copyFileSync(path.join(root,'.rush/own-free-context/clear-asm',f),path.join(sanity,f));
fs.copyFileSync(path.join(root,'.rush/own-free-context/sanity-summary.json'),path.join(sanity,'summary.json'));
const binaries={};for(const phase of ['before','after'])binaries[phase]=fs.readFileSync(path.join(root,'.rush/own-free-context/alloc-sanity',phase+'.binary.sha256'),'utf8').split(/\s/)[0];
fs.writeFileSync(path.join(sanity,'binary_hashes.json'),JSON.stringify(binaries,null,2));
console.log('Archived three rounds and alloc/clear sanity; shared baseline once, raw totals retained, repeated source listings explicitly truncated.');
