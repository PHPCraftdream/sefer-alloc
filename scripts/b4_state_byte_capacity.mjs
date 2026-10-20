#!/usr/bin/env node
/** Exact state-byte payload capacity by the builder's actual class table. */
'use strict';

const SEGMENT = 4 * 1024 * 1024;
const PAGE = 4096;
const MIN_BLOCK = 16;
const GEO_COUNT = 40;
const GROWTH = [5, 4];
const BASE_META_END = 73728;
const PRE_F6_META_END = BASE_META_END + 256 * 1024;
const POST_F6_META_END = 270336;
const EXTRAS = [256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];
const MEDIUM = [256, 320, 384, 512, 768, 1024].map((n) => n * 1024);
const WIDE = [1280, 1536, 1792].map((n) => n * 1024);

function actualClasses(extras) {
  const geo = [MIN_BLOCK];
  while (geo.length < GEO_COUNT) {
    const prev = geo[geo.length - 1];
    const ceilRatio = Math.ceil((prev * GROWTH[0]) / GROWTH[1]);
    const next = Math.ceil(ceilRatio / MIN_BLOCK) * MIN_BLOCK;
    geo.push(Math.max(prev + MIN_BLOCK, next));
  }
  const merged = [...geo, ...extras].sort((a, b) => a - b);
  if (new Set(merged).size !== merged.length) throw new Error('class overlap');
  return merged;
}

function alignUp(value, align) {
  return Math.ceil(value / align) * align;
}
function count(metaEnd, size) {
  // Count only complete blocks whose full class-sized extent fits in the segment.
  const first = alignUp(metaEnd, size);
  if (first >= SEGMENT || first + size > SEGMENT) return 0;
  return Math.floor((SEGMENT - first) / size);
}
function rows(label, classes) {
  console.log(`\n### ${label} (${classes.length} classes)`);
  console.log('| Class size | Base count | Pre-F6 count | Post-F6 count | Post vs base |');
  console.log('|---:|---:|---:|---:|---:|');
  for (const size of classes) {
    const base = count(BASE_META_END, size);
    const pre = count(PRE_F6_META_END, size);
    const post = count(POST_F6_META_END, size);
    const delta = ((post - base) / base) * 100;
    console.log(`| ${size} | ${base} | ${pre} | ${post} | ${delta.toFixed(2)}% |`);
  }
}

const def = actualClasses(EXTRAS);
const med = actualClasses([...EXTRAS, ...MEDIUM]);
const wide = actualClasses([...EXTRAS, ...MEDIUM, ...WIDE]);
console.log('# B4 state-byte exact capacity');
console.log(`SEGMENT=${SEGMENT}; metadata ends base=${BASE_META_END}, pre-F6=${PRE_F6_META_END}, post-F6=${POST_F6_META_END}.`);
console.log('Counts use first block offset align_up(meta_end, class_size), then all absolute class-size strides below SEGMENT; no payload/size approximation.');
rows('Default feature table', def);
rows('medium-classes', med);
rows('medium-classes-wide', wide);
console.log('\nMedium-class loss against base (six exact medium classes):');
for (const size of MEDIUM) console.log(`${size}: ${count(BASE_META_END, size)} -> ${count(POST_F6_META_END, size)} blocks/segment`);
console.log(`Class count verification: default=${def.length}, medium=${med.length}, wide=${wide.length}.`);
if (count(BASE_META_END, 320 * 1024) !== 11
  || count(PRE_F6_META_END, 320 * 1024) !== 10
  || count(POST_F6_META_END, 320 * 1024) !== 11
  || count(BASE_META_END, 256 * 1024) !== 15
  || count(PRE_F6_META_END, 256 * 1024) !== 14
  || count(POST_F6_META_END, 256 * 1024) !== 14
  || count(BASE_META_END, 512 * 1024) !== 7) {
  throw new Error('capacity arithmetic boundary assertions failed');
}
console.log('Capacity arithmetic assertions: 320 KiB 11/10/11; 256 KiB 15/14/14; 512 KiB 7.');
