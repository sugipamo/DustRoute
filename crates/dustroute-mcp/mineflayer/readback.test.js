'use strict'
const { test } = require('node:test')
const assert = require('node:assert/strict')
const { EventEmitter } = require('node:events')
const { confirmRegion, exchange, commandScope, cellsOf, literal, stateConditions } = require('./readback')
const registry = require('minecraft-data')('1.21.11')
const pos = { x: 0, y: 180, z: 0 }
const stair = { pos, name: 'minecraft:quartz_stairs', properties: { facing: 'north', half: 'top', shape: 'inner_left', waterlogged: 'false' } }
const snapshot = { min: pos, max: { ...pos, x: 1 }, blocks: [stair] }
const bot = { version: '1.21.11', registry }
const dimension = 'minecraft:overworld'
const response = replies => ({ nonce: 'a'.repeat(32), replies: replies.some(r => r.startsWith('shape:')) ? replies : [replies.includes('match:0') ? 'check:0:1:42' : 'check:0:0:0'] })

test('checks all native properties and every cell including omitted air', async () => {
  const result = await confirmRegion(bot, snapshot, dimension, { requestId: 'test', exchange: async (_, commands) => {
    const rows = commands(s => s, commandScope('a'.repeat(32), 'DustRouteBot'))
    assert.match(rows[0], /if loaded [01] 180 0/)
    assert.match(rows[0], /if block 1 180 0 minecraft:air/)
    assert.match(rows[0], /quartz_stairs\[facing=north,half=top,shape=inner_left,waterlogged=false\]/)
    assert(!rows.some(r => /setblock|fill|tick freeze/.test(r)))
    return response(['match:0'])
  } })
  assert.equal(result.readback.checked_cells, 2)
  assert.equal(result.readback.predicate_batch_cells, 8880)
  assert.equal(result.readback.request_id, 'test')
  assert.equal(result.readback.hidden_runtime_observed, false)
  assert.throws(() => literal({ ...stair, properties: { half: 'top' } }, registry), /incomplete/)
  assert.throws(() => cellsOf({ ...snapshot, blocks: [stair, stair] }, registry), /duplicate/)
  assert.throws(() => literal({ ...stair, properties: { ...stair.properties, waterlogged: 'yes' } }, registry), /invalid/)
})

test('stale stair shape requires independent selection and full-region reconfirmation', async () => {
  let calls = 0
  const result = await confirmRegion(bot, snapshot, dimension, { exchange: async (_, commands) => {
    const rows = commands(s => s, commandScope('a'.repeat(32), 'DustRouteBot'))
    calls++
    if (calls === 1) return response([])
    if (calls === 2) {
      assert.equal(rows.length, 5)
      assert(rows.every(r => r.includes('quartz_stairs[facing=north,half=top,shape=')))
      return response(['shape:0:straight'])
    }
    assert.match(rows[0], /shape=straight/)
    return response(['match:0'])
  } })
  assert.equal(calls, 3)
  assert.equal(stair.properties.shape, 'inner_left') // original client evidence is preserved
  assert.equal(result.blocks[0].properties.shape, 'straight')
  assert.equal(result.readback.corrections[0].client.properties.shape, 'inner_left')
})

test('compressed air checks detect a change at every cell without reading outside the batch', () => {
  const cells = []
  for (let x = -2; x <= 2; x++) for (let y = 180; y <= 185; y++) for (let z = -3; z <= 13; z++) cells.push({
    pos: { x, y, z }, name: x === 0 && y === 182 && z % 3 === 0 ? 'minecraft:stone' : 'minecraft:air', properties: {}
  })
  const key = p => p.join(',')
  for (let i = 0; i < cells.length; i += 192) {
    const batch = cells.slice(i, i + 192)
    const original = new Map(batch.map(b => [key([b.pos.x, b.pos.y, b.pos.z]), b.name]))
    const conditions = stateConditions(batch, registry)
    assert(conditions.includes('if blocks '))
    // Independent interpretation of the native predicates, including both
    // source and destination cells of each cuboid comparison.
    function accepts (world) {
      const read = p => {
        assert(original.has(key(p)), 'predicate consumed a cell outside its batch')
        return world.get(key(p))
      }
      for (const clause of conditions.split('if ').slice(1)) {
        const [kind, ...args] = clause.trim().split(' ')
        if (kind === 'block') {
          if (read(args.slice(0, 3).map(Number)) !== args[3]) return false
        } else {
          assert.equal(kind, 'blocks'); assert.equal(args[9], 'all')
          const n = args.slice(0, 9).map(Number)
          for (let x = n[0]; x <= n[3]; x++) for (let y = n[1]; y <= n[4]; y++) for (let z = n[2]; z <= n[5]; z++) {
            if (read([x, y, z]) !== read([x - n[0] + n[6], y - n[1] + n[7], z - n[2] + n[8]])) return false
          }
        }
      }
      return true
    }
    assert(accepts(original))
    for (const [p, name] of original) {
      const changed = new Map(original)
      changed.set(p, name === 'minecraft:air' ? 'minecraft:stone' : 'minecraft:air')
      assert(!accepts(changed), `missed changed cell ${p}`)
    }
  }
})

test('repeated shape changes retain only the final difference from the original client', async () => {
  for (const finalShape of ['outer_left', 'inner_left']) {
    let calls = 0
    const actual = await confirmRegion(bot, snapshot, dimension, { exchange: async () => {
      calls++
      if (calls === 2) return response(['shape:0:straight'])
      if (calls === 4) return response([`shape:0:${finalShape}`])
      return response(calls === 5 ? ['match:0'] : [])
    } })
    assert.equal(actual.blocks[0].properties.shape, finalShape)
    assert.equal(actual.readback.corrections.length, finalShape === 'inner_left' ? 0 : 1)
    if (actual.readback.corrections.length) assert.equal(actual.readback.corrections[0].client.properties.shape, 'inner_left')
  }
})

test('missing, ambiguous, extra and subsequently changed states cannot pass', async () => {
  for (const mode of ['missing', 'ambiguous', 'extra', 'changed']) {
    let calls = 0
    await assert.rejects(confirmRegion(bot, snapshot, dimension, { exchange: async (_, commands) => {
      const rows = commands(s => s, commandScope('a'.repeat(32), 'DustRouteBot'))
      calls++
      if (mode === 'extra') return { nonce: 'a'.repeat(32), replies: ['check:0:1:42', 'check:1:1:43'] }
      if (rows.length === 5) return response(mode === 'ambiguous' ? ['shape:0:straight', 'shape:0:inner_left'] : mode === 'changed' ? ['shape:0:straight'] : [])
      return response([])
    } }))
    assert(calls <= 6)
  }
  const air = { min: pos, max: pos, blocks: [] }
  await assert.rejects(confirmRegion(bot, air, dimension, { exchange: async () => response([]) }), /differs.*unloaded/)
  await assert.rejects(confirmRegion(bot, snapshot, dimension, { exchange: async () => ({ nonce: 'x', replies: ['match:0'] }) }), /tick evidence missing/)
})

test('system reply nonce/fence, timeout and disconnect are checked without world writes', async () => {
  const fake = new EventEmitter()
  fake.username = 'DustRouteBot'
  fake.chat = command => {
    assert(!/setblock|fill|tick freeze/.test(command))
    if (command.startsWith('/data modify')) assert.match(command, /"m0":0,"t0":0/)
    if (!command.includes(':done')) return
    const payload = JSON.parse(command.slice(command.indexOf('{'))).text
    fake.emit('messagestr', payload, 'chat') // player chat cannot satisfy the fence
    fake.emit('messagestr', 'dustroute_readback:wrong:done', 'system')
    setImmediate(() => fake.emit('messagestr', payload, 'system'))
  }
  const result = await exchange(fake, (say, scope) => [scope.check('if block 0 180 0 minecraft:air', 0), ...scope.reports(1)], 100)
  assert.equal(result.nonce.length, 32)
  assert.equal(fake.listenerCount('messagestr'), 0)
  fake.chat = () => {}
  await assert.rejects(exchange(fake, () => [], 5), /response missing/)
  fake.chat = () => setImmediate(() => fake.emit('end'))
  await assert.rejects(exchange(fake, () => [], 100), /disconnected/)
  assert.equal(fake.listenerCount('messagestr'), 0)
})

test('large confirmations are refused before sending server commands', async () => {
  const fake = new EventEmitter()
  fake.username = 'DustRouteBot'
  fake.version = bot.version
  fake.registry = registry
  let sent = 0
  fake.chat = () => { sent++ }
  await assert.rejects(exchange(fake, say => Array.from({ length: 190 }, (_, i) => say(`match:${i}`))), /192-command/)
  await assert.rejects(exchange(fake, say => [say('x'.repeat(30000))]), /command limit/)
  await assert.rejects(confirmRegion(bot, { min: pos, max: { ...pos, x: 8880 }, blocks: [] }, dimension, { exchange: async () => { sent++; return response(['match:0']) } }), /8880-cell/)
  const dense = { min: pos, max: { ...pos, x: 1023 }, blocks: Array.from({ length: 1024 }, (_, x) => ({ pos: { ...pos, x }, name: 'minecraft:stone', properties: {} })) }
  await assert.rejects(confirmRegion(fake, dense, dimension), /command limit/)
  assert.equal(sent, 0)
})
