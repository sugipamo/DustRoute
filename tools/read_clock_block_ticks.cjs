// Read only the probe torch's scheduled block ticks after `save-all flush`.
// The NBT decoder is the already installed Mineflayer dependency. No world data
// is edited, and unrelated chunk contents are not printed.
const fs = require('node:fs')
const path = require('node:path')
const zlib = require('node:zlib')
const crypto = require('node:crypto')
const assert = require('node:assert/strict')
const nbt = require('../crates/dustroute-mcp/mineflayer/node_modules/prismarine-nbt')

const [directory, sx, sy, sz, currentTime] = process.argv.slice(2)
const [x, y, z] = [sx, sy, sz].map(Number)
assert([x, y, z].every(Number.isSafeInteger))
assert(currentTime && /^\d+$/.test(currentTime), 'current frozen game time is required')
const cx = Math.floor(x / 16)
const cz = Math.floor(z / 16)
const file = path.join(directory, 'world', 'region', `r.${Math.floor(cx / 32)}.${Math.floor(cz / 32)}.mca`)
const region = fs.readFileSync(file)
const entry = 4 * (((cx % 32) + 32) % 32 + 32 * (((cz % 32) + 32) % 32))
const sector = region.readUIntBE(entry, 3)
assert(sector >= 2 && region[entry + 3] > 0)
const offset = sector * 4096
const length = region.readUInt32BE(offset)
assert(length > 1 && length + 4 <= region[entry + 3] * 4096)
assert.equal(region[offset + 4], 2, 'probe supports inline zlib-compressed chunks only')
const decoded = nbt.simplify(nbt.parseUncompressed(zlib.inflateSync(region.subarray(offset + 5, offset + 4 + length))))
assert.equal(decoded.xPos, cx)
assert.equal(decoded.zPos, cz)
assert(Array.isArray(decoded.block_ticks))
// `save-all flush` may leave an unchanged chunk's old serialization in place.
// Stored delays are relative to LastUpdate, not necessarily the observation tick.
const age = BigInt(currentTime) - BigInt(String(decoded.LastUpdate))
assert(age >= 0n && age <= BigInt(Number.MAX_SAFE_INTEGER))
console.log(JSON.stringify({
  evidence: 'saved_server_chunk_block_ticks',
  saved_chunk_age_game_ticks: Number(age),
  current_queue_verified: age === 0n,
  region_sha256: crypto.createHash('sha256').update(region).digest('hex'),
  ticks: decoded.block_ticks.filter(t => t.x === x && t.y === y && t.z === z)
    .map(t => ({ block: t.i, stored_delay: t.t, priority: t.p }))
}))
