// Frozen inventories and PRF-01-derived profile collection contract shared by P4-1.
import assert from 'node:assert/strict';
import { profileFunctions,parseProfiles } from './r20_prf04_heap_reuse_profile.mjs';
export { profileFunctions,parseProfiles };
export const L1_NAMES=['r20_prf04hr_start',...['cold','recycled','wave','wave_release'].flatMap(f=>['n','2n','4n'].map(s=>`r20_prf04hr_${f}_${s}`))];
export const L2_NAMES=['l2_cold','l2_recycled','l2_wave_release'].flatMap(f=>['n','2n','4n'].map(s=>`r20_prf04hr_${f}_${s}`));
export const GROUPS={L1:'heap_reuse',L2:'heap_reuse_l2'};
export const TARGETS={L1:'r20_prf04_heap_reuse_iai',L2:'r20_prf04_heap_reuse_l2_iai'};
export const ALLOC='r20_prf04_heap_reuse_l2_iai::hr::child_first_alloc';
export const GUARD='<sefer_alloc::global::tls_heap::AbandonGuard as core::ops::Drop>::drop';
export function registeredInventory(source,layer) {
  const group=GROUPS[layer],match=new RegExp(`library_benchmark_group!\\(\\s*name\\s*=\\s*${group};\\s*benchmarks\\s*=([\\s\\S]*?)\\);`).exec(source);
  assert.ok(match,`missing actual benchmark group ${group}`);return match[1].split(',').map(x=>x.trim()).filter(Boolean);
}
export function validateInventory(actual,layer,{releaseLayer='L2'}={}) {const expected=layer==='L1'?L1_NAMES:L2_NAMES;assert.deepEqual(actual,expected,`${layer} benchmark inventory mismatch`);return layer==='L2'&&releaseLayer==='L1'?actual.filter(n=>!n.includes('l2_wave_release')):actual;}
export function validateOracle(row,{arm,family,n,binarySha256,source,environmentSha256}) {
  assert.equal(row.arm,arm);assert.equal(row.family,family);assert.equal(row.n,n);assert.equal(row.ok,true);assert.equal(row.binarySha256,binarySha256);
  assert.deepEqual(row.source,source);assert.equal(row.environmentSha256,environmentSha256);assert.equal(row.configConflictsDelta,0);
  for(const key of ['bumpDelta','hintDelta','reclaimDelta','newHeaps','freeSetDelta','popcount','claimSourcesLen','n'])assert.ok(Number.isSafeInteger(row[key])&&row[key]>=0,`invalid oracle field ${key}`);
  assert.equal(row.claimSourcesLen,arm==='S'?5:4);assert.equal(row.missingFreeSetExplicitZero,arm==='A');assert.equal(row.popcountAvailable,arm==='S');
  if(family==='l2_cold'||family==='l2_recycled')assert.equal(row.targetClaimDelta,n,'L2 target claim activation mismatch');
  if(family==='l2_wave_release')assert.equal(row.targetClaimDelta,64,'L2 release warmup target claim activation mismatch');
  return row;
}
export function validateMeasurement(row,{layer,cell,exe,arm,debug,log,profiles,index,raw,actualInstructions,expectedChildren,allowMissingRelease=false,rehearsal=false,releaseLayer='L2',mode='alloc'}) {
  const n=cell.family==='start'?1:Number(({n:16,'2n':32,'4n':64})[cell.size]);
  if(layer==='L1'){
    const text=raw??log;const m=[...text.matchAll(/^\s*Instructions:\s*([\d,]+)/gm)];assert.equal(m.length,1,'actual raw Instructions missing/ambiguous');const instructions=Number(m[0][1].replaceAll(',',''));assert.ok(Number.isSafeInteger(instructions));const parsed=parseProfiles(profiles,log,debug,{group:GROUPS[layer],index,exe,name:cell.name,toggles:[],expectedChildren:0,expectedIr:instructions,mode:'whole'});return {ir:instructions,profile:parsed};
  }
  const release=cell.family==='l2_wave_release', recycled=cell.family==='l2_recycled';const dual=recycled&&releaseLayer==='L2';const toggles=release?[GUARD]:dual?[ALLOC,GUARD]:[ALLOC];
  const childExpected=release?64:(expectedChildren??n);const parsed=parseProfiles(profiles,log,debug,{group:GROUPS[layer],index,exe,name:cell.name,toggles,targetSymbol:release?GUARD:ALLOC,expectedCalls:n,expectedChildren:childExpected,expectedIr:actualInstructions,release,mode:dual?'recycled':'alloc',allowMissingRelease:rehearsal&&allowMissingRelease});
  if(release&&rehearsal&&allowMissingRelease&&parsed.guardExecutions===0)return {ir:0,profile:parsed,rehearsalGuardFailure:true};assert.equal(parsed.targetExecutions,n);if(release||dual)assert.equal(parsed.guardExecutions,n);assert.ok(parsed.childIr>0);return {ir:parsed.childIr,profile:parsed};
}
