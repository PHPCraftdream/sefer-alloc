import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
const root = process.cwd();
const phase = process.argv[2] ?? 'baseline';
const repo = path.join(root, phase === 'baseline' ? 'worktrees/own-free-before' : 'worktrees/own-free-split');
assert(['baseline','candidate'].includes(phase));
const out = path.join(root, `.rush/own-free-split/${phase}`);
const evidence = path.join(root, 'worktrees/own-free-split/docs/perf/own_free_split');
fs.mkdirSync(evidence, {recursive:true});
function git(args) {
  const p=spawnSync('git',args,{cwd:repo,encoding:'utf8'});
  assert.equal(p.status,0,p.stderr);return p.stdout;
}
const base=git(['rev-parse','HEAD']).trim();
assert.equal(base,'f80c681b799ef4ef7fa08d79c06896fdf1104fe9');
if (phase === 'baseline') assert.equal(git(['diff','--','src','Cargo.lock']),'');
const sourceDiff = git(['diff','--','src','Cargo.lock']);
const patch=git(['diff','--binary','--','src','Cargo.toml','Cargo.lock']);
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const example=fs.readFileSync(path.join(repo,'examples/own_free_profile.rs'));
fs.writeFileSync(path.join(evidence,`${phase}.patch`),patch);
fs.copyFileSync(path.join(repo,'examples/own_free_profile.rs'),path.join(evidence,'own_free_profile.rs'));
fs.copyFileSync(path.join(root,'.rush/own-free-split/profile.sh'),path.join(evidence,'profile.sh'));
fs.writeFileSync(path.join(evidence,`${phase}_identity.json`),JSON.stringify({captured_before_build:new Date().toISOString(),base,features:'production',patch_sha256:hash(patch),example_sha256:hash(example),profile_script_sha256:hash(fs.readFileSync(path.join(root,'.rush/own-free-split/profile.sh'))),layer:'SeferAlloc GlobalAlloc facade; same-thread runtime-layout and const-layout churn; not installed allocator',environment:'WSL Linux, not native Linux or Windows'},null,2));
function wsl(p){const m=/^([A-Za-z]):[\\/](.*)$/.exec(p);assert(m);return '/mnt/'+m[1].toLowerCase()+'/'+m[2].replaceAll(String.fromCharCode(92),'/');}
const p=spawnSync('wsl.exe',['-e','bash','-lc','exec bash "$@"','bash',wsl(path.join(root,'.rush/own-free-split/profile.sh')),wsl(repo),wsl(out)],{stdio:'inherit'});
assert.equal(p.status,0,'profile failed');
assert.equal(git(['diff','--','src','Cargo.lock']),sourceDiff);
assert.equal(hash(fs.readFileSync(path.join(repo,'examples/own_free_profile.rs'))),hash(example));
