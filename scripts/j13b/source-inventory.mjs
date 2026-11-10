// Pure tracked-source recipe; the caller must supply verified, stable working-file bytes.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

export function sourceInventory(entries) {
  assert(Array.isArray(entries) && entries.length > 0 && entries.length <= 200000, 'source inventory count');
  const seen = new Set();
  let total = 0;
  const files = entries.map(entry => {
    assert(entry && Object.keys(entry).sort().join(',') === 'bytes,mode,path', 'source entry fields');
    const { path, mode, bytes } = entry;
    assert(typeof path === 'string' && path.length > 0, 'source path missing');
    const encoded = Buffer.from(path, 'utf8');
    assert(encoded.toString('utf8') === path && encoded.length <= 4096, 'source path encoding/bound');
    assert(!/[\\\0\r\n:]/.test(path), 'source path contains unsupported characters');
    assert(path.split('/').every(part => part && part !== '.' && part !== '..' && part.toLowerCase() !== '.git'),
      'source path escapes repository or is not canonical');
    assert(!seen.has(path), 'duplicate source path');
    seen.add(path);
    assert(mode === '100644' || mode === '100755', 'source must be a regular tracked file');
    assert(Buffer.isBuffer(bytes) && bytes.length <= 64 * 1024 * 1024, 'source file byte bound');
    total += bytes.length;
    assert(Number.isSafeInteger(total) && total <= 512 * 1024 * 1024, 'source total byte bound');
    return { path, mode, bytes, encoded };
  }).sort((a, b) => Buffer.compare(a.encoded, b.encoded));
  const source = createHash('sha256').update('sefer-j13b-tracked-source-v1\0');
  const inventory = createHash('sha256').update('sefer-j13b-tracked-inventory-v1\0');
  const frame = (hash, bytes) => {
    const length = Buffer.alloc(8);
    length.writeBigUInt64BE(BigInt(bytes.length));
    hash.update(length).update(bytes);
  };
  const records = files.map(file => {
    for (const hash of [source, inventory]) {
      frame(hash, file.encoded);
      frame(hash, Buffer.from(file.mode, 'ascii'));
    }
    frame(source, file.bytes);
    const sha256 = createHash('sha256').update(file.bytes).digest('hex');
    frame(inventory, Buffer.from(String(file.bytes.length), 'ascii'));
    frame(inventory, Buffer.from(sha256, 'ascii'));
    return Object.freeze({ path: file.path, mode: file.mode, bytes: file.bytes.length, sha256 });
  });
  return Object.freeze({ recipe: 'j13b.tracked-source.v1', source_sha256: source.digest('hex'),
    inventory_sha256: inventory.digest('hex'), total_bytes: total, files: Object.freeze(records) });
}
