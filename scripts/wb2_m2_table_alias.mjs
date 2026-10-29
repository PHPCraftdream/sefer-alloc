// Resolve RIP-relative lea targets of named statics in a dumpbin /disasm listing.
// Usage: node scripts/wb2_m2_table_alias.mjs <dumpbin -disasm listing of bench_table_paired.exe>
import fs from "node:fs";
const lines = fs.readFileSync(process.argv[2], "utf8").split(/\r?\n/);
const want = /(SIZE2CLASS_CEIL|SIZE2CLASS|FREE_PARK_CAP)\]/;
const found = {};
for (let i = 0; i < lines.length; i++) {
  const m = /^\s+([0-9A-F]{16}): ((?:[0-9A-F]{2} )+)\s+lea\s+\w+,\[(\S+)\]/.exec(lines[i]);
  if (!m || !want.test(lines[i])) continue;
  let bytes = m[2].trim().split(" ");
  const cont = /^\s+((?:[0-9A-F]{2} ?)+)$/.exec(lines[i + 1] || "");
  if (cont) bytes = bytes.concat(cont[1].trim().split(" "));
  const addr = BigInt("0x" + m[1]);
  const len = bytes.length;
  const d = Buffer.from(bytes.slice(len - 4).map((b) => parseInt(b, 16))).readInt32LE(0);
  const tgt = addr + BigInt(len) + BigInt(d);
  const name = /(SIZE2CLASS_CEIL|SIZE2CLASS|FREE_PARK_CAP)/.exec(m[3])[1];
  (found[name] ??= new Set()).add(tgt);
}
for (const [k, s] of Object.entries(found)) {
  for (const t of s) {
    const rva = t - 0x140000000n;
    const idx = { SIZE2CLASS_CEIL: [64], SIZE2CLASS: [63], FREE_PARK_CAP: [17] }[k];
    console.log(k, "rva=0x" + rva.toString(16), idx.map((j) => `idx${j}: (rva+idx) mod 4096=${Number((rva + BigInt(j)) % 4096n)} mod 1024=${Number((rva + BigInt(j)) % 1024n)}`).join(" "));
  }
}
