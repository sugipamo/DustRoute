'use strict'

const net = require('node:net')
const crypto = require('node:crypto')
const mineflayer = require('mineflayer')
const { Vec3 } = require('vec3')

function minecraftEndpoint () {
  const configured = process.env.DUSTROUTE_SERVER_ADDRESS
  if (!configured) {
    return {
      host: process.env.DUSTROUTE_MC_HOST || '127.0.0.1',
      port: Number(process.env.DUSTROUTE_MC_PORT || 25565)
    }
  }
  const separator = configured.lastIndexOf(':')
  if (separator <= 0) throw new Error('DUSTROUTE_SERVER_ADDRESS must be host:port')
  const host = configured.slice(0, separator).replace(/^\[|\]$/g, '')
  const port = Number(configured.slice(separator + 1))
  if (!host || !Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error('DUSTROUTE_SERVER_ADDRESS must be host:port')
  }
  return { host, port }
}

const minecraft = minecraftEndpoint()
const config = {
  host: minecraft.host,
  port: minecraft.port,
  username: process.env.DUSTROUTE_BOT_NAME || 'DustRouteBot',
  version: process.env.DUSTROUTE_MC_VERSION || '1.21.11',
  bridgeHost: '127.0.0.1',
  bridgePort: Number(process.env.DUSTROUTE_BRIDGE_PORT || 25580)
}

let bot = null
let spawned = false
let shuttingDown = false
let reconnectTimer = null
let updateRecording = null
let observedGameTick = 0

function connectBot () {
  if (shuttingDown) return
  bot = mineflayer.createBot({
    host: config.host,
    port: config.port,
    username: config.username,
    auth: process.env.DUSTROUTE_MC_AUTH || 'offline',
    version: config.version,
    hideErrors: false
  })
  bot.once('spawn', async () => {
    spawned = true
    observedGameTick = Number((bot.time && bot.time.age) || 0)
    await bot.waitForChunksToLoad()
    process.stderr.write(`[dustroute-bot] ${bot.username} joined ${config.host}:${config.port}\n`)
  })
  bot.on('physicsTick', () => {
    if (spawned) observedGameTick += 1
  })
  bot.on('end', reason => {
    spawned = false
    updateRecording = null
    process.stderr.write(`[dustroute-bot] disconnected: ${String(reason)}; reconnecting in 3s\n`)
    if (!shuttingDown) reconnectTimer = setTimeout(connectBot, 3000)
  })
  bot.on('kicked', reason => process.stderr.write(`[dustroute-bot] kicked: ${String(reason)}\n`))
  bot.on('error', error => process.stderr.write(`[dustroute-bot] ${error.stack || error}\n`))
  bot.on('blockUpdate', (oldBlock, newBlock) => {
    const recording = updateRecording
    if (!recording || recording.dimension !== currentDimension()) return
    const block = newBlock || oldBlock
    if (!block || !inside(block.position, recording.min, recording.max)) return
    recording.seenEvents += 1
    if (recording.events.length >= recording.maxEvents) {
      recording.truncated = true
      return
    }
    recording.events.push({
      sequence: recording.seenEvents,
      game_tick: observedGameTick,
      pos: posJson(block.position),
      before: blockRecord(oldBlock),
      after: blockRecord(newBlock)
    })
  })
}

connectBot()

function directionFromRotation (yaw, pitch) {
  const cosPitch = Math.cos(pitch)
  return new Vec3(-Math.sin(yaw) * cosPitch, Math.sin(pitch), -Math.cos(yaw) * cosPitch)
}

function targetFromPlayer (username, maxDistance) {
  const entity = Object.values(bot.entities).find(entity => entity.username === username)
  if (!entity) throw new Error(`player is not visible to the bot: ${username}`)
  const eyeHeight = entity.height ? Math.min(entity.height, 1.62) : 1.62
  const origin = entity.position.offset(0, eyeHeight, 0)
  const direction = directionFromRotation(entity.yaw, entity.pitch)
  let previous = null
  for (let distance = 0; distance <= maxDistance; distance += 0.1) {
    const point = origin.plus(direction.scaled(distance))
    const blockPos = point.floored()
    if (previous && previous.equals(blockPos)) continue
    previous = blockPos
    const block = bot.blockAt(blockPos)
    if (block && block.boundingBox !== 'empty') {
      return { entity, origin, block, distance }
    }
  }
  return { entity, origin, block: null, distance: null }
}

async function approachPlayer (username) {
  if (!/^[A-Za-z0-9_]{1,16}$/.test(username)) {
    throw new Error(`invalid Minecraft player name: ${username}`)
  }
  if (username === bot.username) throw new Error('the assist player cannot be the bot itself')
  const existing = Object.values(bot.entities).find(entity => entity.username === username)
  if (existing) {
    return {
      player: username,
      moved: false,
      position: posJson(bot.entity.position),
      distance: bot.entity.position.distanceTo(existing.position),
      dimension: currentDimension()
    }
  }
  // The dedicated test server grants the visible bot permission to join the
  // configured player. Teleporting only the bot leaves the circuit untouched.
  bot.chat(`/tp ${bot.username} ${username}`)
  await bot.waitForTicks(5)
  const entity = Object.values(bot.entities).find(entity => entity.username === username)
  if (!entity) throw new Error(`player could not be reacquired after moving the bot: ${username}`)
  return {
    player: username,
    moved: true,
    position: posJson(bot.entity.position),
    distance: bot.entity.position.distanceTo(entity.position),
    dimension: currentDimension()
  }
}

function propertiesOf (block) {
  return Object.fromEntries(
    Object.entries(block.getProperties()).map(([key, value]) => [key, String(value)])
  )
}

function posJson (pos) {
  return { x: pos.x, y: pos.y, z: pos.z }
}

function blockRecord (block) {
  if (!block) return null
  return {
    name: `minecraft:${block.name}`,
    properties: propertiesOf(block)
  }
}

function inside (pos, min, max) {
  return pos.x >= Math.min(min.x, max.x) && pos.x <= Math.max(min.x, max.x) &&
    pos.y >= Math.min(min.y, max.y) && pos.y <= Math.max(min.y, max.y) &&
    pos.z >= Math.min(min.z, max.z) && pos.z <= Math.max(min.z, max.z)
}

function currentDimension () {
  const value = bot && bot.game && bot.game.dimension ? String(bot.game.dimension) : 'unknown'
  return value.includes(':') ? value : `minecraft:${value}`
}

function requireDimension (expected) {
  const actual = currentDimension()
  if (expected && expected !== actual) {
    throw new Error(`bot dimension changed: expected ${expected}, currently ${actual}`)
  }
}

async function scanRegion (min, max) {
  const low = {
    x: Math.min(min.x, max.x), y: Math.min(min.y, max.y), z: Math.min(min.z, max.z)
  }
  const high = {
    x: Math.max(min.x, max.x), y: Math.max(min.y, max.y), z: Math.max(min.z, max.z)
  }
  const volume = (high.x - low.x + 1) * (high.y - low.y + 1) * (high.z - low.z + 1)
  if (volume > 262144) throw new Error(`selected volume ${volume} exceeds the 262144 block limit`)
  const blocks = []
  for (let x = low.x; x <= high.x; x++) {
    for (let y = low.y; y <= high.y; y++) {
      for (let z = low.z; z <= high.z; z++) {
        let block = bot.blockAt(new Vec3(x, y, z))
        if (!block) {
          await bot.waitForChunksToLoad()
          block = bot.blockAt(new Vec3(x, y, z))
        }
        if (!block) throw new Error(`chunk unavailable at ${x} ${y} ${z}`)
        if (['air', 'cave_air', 'void_air'].includes(block.name)) continue
        blocks.push({
          pos: { x, y, z },
          name: `minecraft:${block.name}`,
          properties: propertiesOf(block)
        })
      }
    }
  }
  return { min: low, max: high, blocks }
}

async function writeBlocks (changes) {
  if (!Array.isArray(changes)) throw new Error('changes must be an array')
  if (changes.length > 32768) throw new Error('write exceeds the 32768 block limit')
  const statePattern = /^minecraft:[a-z0-9_]+(?:\[[a-z0-9_=,]+\])?$/
  for (let index = 0; index < changes.length; index++) {
    const change = changes[index]
    const { x, y, z } = change.pos || {}
    if (![x, y, z].every(Number.isInteger)) throw new Error('write position must use integers')
    if (typeof change.state !== 'string' || !statePattern.test(change.state)) {
      throw new Error(`invalid Minecraft block state at change ${index}`)
    }
    bot.chat(`/setblock ${x} ${y} ${z} ${change.state} replace`)
    if ((index + 1) % 1000 === 0) await bot.waitForTicks(1)
  }
  await bot.waitForTicks(2)
  return { submitted_changes: changes.length }
}

async function placePhysicalBlocks (changes) {
  if (!Array.isArray(changes) || changes.length === 0) throw new Error('physical changes must be a non-empty array')
  if (changes.length > 128) throw new Error('normal player placement is limited to 128 changes')
  const Item = require('prismarine-item')(config.version)
  let highestY = -64
  let centerX = 0
  let centerZ = 0
  bot.chat(`/gamemode creative ${bot.username}`)
  await bot.waitForTicks(2)
  for (const change of changes) {
    const { x, y, z } = change.pos || {}
    if (![x, y, z].every(Number.isInteger)) throw new Error('physical placement position must use integers')
    highestY = Math.max(highestY, y)
    centerX += x
    centerZ += z
    bot.chat(`/tp ${bot.username} ${x + 0.5} ${y + 3} ${z + 0.5}`)
    await bot.waitForTicks(3)
    bot.creative.startFlying()
    const existing = bot.blockAt(new Vec3(x, y, z))
    if (existing && !['air', 'cave_air', 'void_air'].includes(existing.name)) {
      await bot.dig(existing, true)
      await bot.waitForTicks(1)
    }
    if (change.action === 'dig') continue
    if (change.action !== 'place') throw new Error(`unknown physical action: ${String(change.action)}`)
    const itemName = String(change.item || '').replace(/^minecraft:/, '')
    const itemDefinition = bot.registry.itemsByName[itemName]
    if (!itemDefinition) throw new Error(`unknown placement item: ${itemName}`)
    await bot.creative.setInventorySlot(36, new Item(itemDefinition.id, 1))
    const held = bot.inventory.slots[36]
    if (!held) throw new Error(`failed to prepare placement item: ${itemName}`)
    await bot.equip(held, 'hand')
    const referencePos = new Vec3(change.reference.x, change.reference.y, change.reference.z)
    const reference = bot.blockAt(referencePos)
    if (!reference || reference.boundingBox === 'empty') {
      throw new Error(`placement support is unavailable at ${referencePos.x} ${referencePos.y} ${referencePos.z}`)
    }
    await bot.placeBlock(reference, new Vec3(change.face.x, change.face.y, change.face.z))
    await bot.waitForTicks(2)
    const placed = bot.blockAt(new Vec3(x, y, z))
    if (!placed || ['air', 'cave_air', 'void_air'].includes(placed.name)) {
      throw new Error(`normal placement did not create a block at ${x} ${y} ${z}`)
    }
  }
  centerX = centerX / changes.length
  centerZ = centerZ / changes.length
  const retreat = new Vec3(Math.floor(centerX) + 0.5, highestY + 16, Math.floor(centerZ) + 0.5)
  bot.chat(`/tp ${bot.username} ${retreat.x} ${retreat.y} ${retreat.z}`)
  await bot.waitForTicks(3)
  bot.creative.startFlying()
  return { placed_changes: changes.length, placement_mode: 'mineflayer_player', retreat: posJson(retreat) }
}

function getBlock (pos) {
  const block = bot.blockAt(new Vec3(pos.x, pos.y, pos.z))
  if (!block) throw new Error(`block is unavailable at ${pos.x} ${pos.y} ${pos.z}`)
  return { pos, ...blockRecord(block) }
}

async function ensureLeverReachable (pos) {
  let block = bot.blockAt(new Vec3(pos.x, pos.y, pos.z))
  if (!block) throw new Error(`block is unavailable at ${pos.x} ${pos.y} ${pos.z}`)
  if (block.name !== 'lever') throw new Error(`activation target is not a lever: minecraft:${block.name}`)
  let distance = bot.entity.position.distanceTo(block.position.offset(0.5, 0.5, 0.5))
  let moved = false
  if (distance > 5.5) {
    let approach = null
    for (const dy of [2, 3, 4]) {
      for (const [dx, dz] of [[0, 0], [1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [-1, 1], [1, -1], [-1, -1]]) {
        const feet = block.position.offset(dx, dy, dz)
        const feetBlock = bot.blockAt(feet)
        const headBlock = bot.blockAt(feet.offset(0, 1, 0))
        const center = feet.offset(0.5, 0, 0.5)
        if (feetBlock && headBlock && feetBlock.boundingBox === 'empty' && headBlock.boundingBox === 'empty' && center.distanceTo(block.position.offset(0.5, 0.5, 0.5)) <= 5.5) {
          approach = center
          break
        }
      }
      if (approach) break
    }
    if (!approach) throw new Error(`no safe air position is available within reach of lever at ${pos.x} ${pos.y} ${pos.z}`)
    bot.chat(`/tp ${bot.username} ${approach.x} ${approach.y} ${approach.z}`)
    await bot.waitForTicks(3)
    bot.creative.startFlying()
    block = bot.blockAt(new Vec3(pos.x, pos.y, pos.z))
    if (!block || block.name !== 'lever') throw new Error('lever became unavailable after bot approach')
    distance = bot.entity.position.distanceTo(block.position.offset(0.5, 0.5, 0.5))
    if (distance > 5.5) throw new Error(`bot is still ${distance.toFixed(2)} blocks from the lever after approach`)
    moved = true
  }
  return { block, moved, distance }
}

async function approachLever (pos) {
  const approach = await ensureLeverReachable(pos)
  return { pos, moved: approach.moved, distance: approach.distance }
}

async function activateLever (pos) {
  const approach = await ensureLeverReachable(pos)
  const block = approach.block
  const before = propertiesOf(block).powered === 'true'
  await bot.lookAt(block.position.offset(0.5, 0.5, 0.5), true)
  await bot.activateBlock(block)
  let after = before
  for (let attempt = 0; attempt < 5 && after === before; attempt++) {
    await bot.waitForTicks(1)
    const afterBlock = bot.blockAt(block.position)
    after = afterBlock && propertiesOf(afterBlock).powered === 'true'
  }
  if (after === before) throw new Error('lever state did not change after normal player activation')
  return { pos, before_powered: before, after_powered: after, bot_approached: approach.moved }
}

function startUpdateRecording (params) {
  if (updateRecording) throw new Error('a block update recording is already active')
  const maxEvents = Number(params.max_events || 16384)
  if (!Number.isInteger(maxEvents) || maxEvents < 1 || maxEvents > 65536) {
    throw new Error('max_events must be 1..65536')
  }
  for (const value of [params.min.x, params.min.y, params.min.z, params.max.x, params.max.y, params.max.z]) {
    if (!Number.isInteger(value)) throw new Error('recording bounds must use integers')
  }
  const volume = (Math.abs(params.max.x - params.min.x) + 1) *
    (Math.abs(params.max.y - params.min.y) + 1) *
    (Math.abs(params.max.z - params.min.z) + 1)
  if (volume > 262144) throw new Error(`recording volume ${volume} exceeds the 262144 block limit`)
  const id = crypto.randomUUID()
  updateRecording = {
    id,
    dimension: currentDimension(),
    min: params.min,
    max: params.max,
    maxEvents,
    seenEvents: 0,
    truncated: false,
    startedGameTick: observedGameTick,
    events: []
  }
  return { recording_id: id, started_game_tick: updateRecording.startedGameTick }
}

function stopUpdateRecording (recordingId) {
  if (!updateRecording || updateRecording.id !== recordingId) {
    throw new Error('block update recording does not exist or has a different id')
  }
  const result = {
    recording_id: updateRecording.id,
    started_game_tick: updateRecording.startedGameTick,
    stopped_game_tick: observedGameTick,
    seen_events: updateRecording.seenEvents,
    truncated: updateRecording.truncated,
    events: updateRecording.events
  }
  updateRecording = null
  return result
}

function previewRegion (player, min, max) {
  const low = { x: Math.min(min.x, max.x), y: Math.min(min.y, max.y), z: Math.min(min.z, max.z) }
  const high = { x: Math.max(min.x, max.x), y: Math.max(min.y, max.y), z: Math.max(min.z, max.z) }
  const corners = []
  for (const x of [low.x, high.x + 1]) {
    for (const y of [low.y, high.y + 1]) {
      for (const z of [low.z, high.z + 1]) corners.push({ x, y, z })
    }
  }
  for (const point of corners) {
    bot.chat(`/particle minecraft:end_rod ${point.x} ${point.y} ${point.z} 0.15 0.15 0.15 0.01 12 force ${player}`)
  }
  bot.chat(`/msg ${player} DustRoute selection: (${low.x}, ${low.y}, ${low.z}) to (${high.x}, ${high.y}, ${high.z})`)
  return { min: low, max: high, particle_corners: corners.length }
}

async function dispatch (method, params) {
  if (method === 'status') {
    return {
      connected: spawned,
      username: (bot && bot.username) || config.username,
      host: config.host,
      port: config.port,
      version: config.version,
      dimension: spawned ? currentDimension() : null,
      position: spawned ? posJson(bot.entity.position) : null
    }
  }
  if (!spawned) throw new Error('Minecraft bot has not spawned')
  if (method === 'visible_players') {
    return Object.values(bot.entities)
      .filter(entity => entity.type === 'player' && entity.username && entity.username !== bot.username)
      .map(entity => ({
        player: entity.username,
        position: posJson(entity.position),
        distance_from_bot: bot.entity.position.distanceTo(entity.position),
        dimension: currentDimension()
      }))
  }
  if (method === 'approach_player') {
    return approachPlayer(params.player)
  }
  if (method === 'observe_player') {
    const target = targetFromPlayer(params.player, Number(params.max_distance || 64))
    return {
      player: params.player,
      eye_position: posJson(target.origin),
      yaw: target.entity.yaw,
      pitch: target.entity.pitch,
      targeted_block: target.block ? posJson(target.block.position) : null,
      targeted_face: null,
      distance: target.distance,
      dimension: currentDimension()
    }
  }
  if (method === 'scan_region') {
    requireDimension(params.dimension)
    return scanRegion(params.min, params.max)
  }
  if (method === 'get_block') {
    requireDimension(params.dimension)
    return getBlock(params.pos)
  }
  if (method === 'activate_lever') {
    requireDimension(params.dimension)
    return activateLever(params.pos)
  }
  if (method === 'approach_lever') {
    requireDimension(params.dimension)
    return approachLever(params.pos)
  }
  if (method === 'wait_ticks') {
    requireDimension(params.dimension)
    const ticks = Number(params.ticks)
    if (!Number.isInteger(ticks) || ticks < 1 || ticks > 200) throw new Error('ticks must be 1..200')
    await bot.waitForTicks(ticks)
    return { waited_ticks: ticks, game_tick: observedGameTick }
  }
  if (method === 'start_update_recording') {
    requireDimension(params.dimension)
    return startUpdateRecording(params)
  }
  if (method === 'stop_update_recording') {
    requireDimension(params.dimension)
    return stopUpdateRecording(params.recording_id)
  }
  if (method === 'preview_region') {
    requireDimension(params.dimension)
    return previewRegion(params.player, params.min, params.max)
  }
  if (method === 'write_blocks') {
    requireDimension(params.dimension)
    return writeBlocks(params.changes)
  }
  if (method === 'place_physical_blocks') {
    requireDimension(params.dimension)
    return placePhysicalBlocks(params.changes)
  }
  throw new Error(`unknown method: ${method}`)
}

const server = net.createServer(socket => {
  socket.setEncoding('utf8')
  let buffer = ''
  socket.on('data', chunk => {
    buffer += chunk
    const newline = buffer.indexOf('\n')
    if (newline < 0) return
    const line = buffer.slice(0, newline)
    buffer = buffer.slice(newline + 1)
    Promise.resolve()
      .then(() => JSON.parse(line))
      .then(request => dispatch(request.method, request.params || {})
        .then(result => ({ id: request.id, result }))
        .catch(error => ({ id: request.id, error: String(error.message || error) })))
      .then(response => socket.end(`${JSON.stringify(response)}\n`))
      .catch(error => socket.end(`${JSON.stringify({ error: String(error.message || error) })}\n`))
  })
})

server.listen(config.bridgePort, config.bridgeHost, () => {
  process.stderr.write(`[dustroute-bot] bridge listening on ${config.bridgeHost}:${config.bridgePort}\n`)
})

function shutdown () {
  shuttingDown = true
  if (reconnectTimer) clearTimeout(reconnectTimer)
  server.close()
  if (bot) bot.quit('DustRoute MCP stopped')
}
process.once('SIGINT', shutdown)
process.once('SIGTERM', shutdown)
