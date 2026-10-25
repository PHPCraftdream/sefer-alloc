import fs from 'node:fs';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
const root=process.cwd(),results=[];
const scripts=[
 'worktrees/scalar-issue-prepared/scripts/scalar_issue_report.mjs',
 'worktrees/own-free-context/docs/perf/owner_research/generate.mjs',
 'worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/ir1/generate.mjs',
 'worktrees/native-alloc-leaf/docs/perf/native_alloc_leaf/native1/generate.mjs',
 'worktrees/native-mixed-judge/docs/perf/native_mixed/calibration1/calibrate.mjs',
 'worktrees/native-mixed-judge/docs/perf/native_mixed/attribution1/generate.mjs',
 'worktrees/native-mixed-judge/docs/perf/native_mixed/cpu2/generate.mjs'
];
for(const script of scripts){
 const p=spawnSync('node',[script,'--check'],{encoding:'utf8',maxBuffer:64*1024*1024});
 const stderr=(p.stderr??'').split(root).join('<workspace>').split(root.replaceAll('\\','/')).join('<workspace>');
 results.push({script,status:p.status,stderr:stderr||undefined});
 fs.writeFileSync('.rush/docs-commit/generator-checks.json',JSON.stringify(results,null,2));
 assert.equal(p.status,0,`${script}: ${stderr}`);console.log(`PASS ${script}`);
}
