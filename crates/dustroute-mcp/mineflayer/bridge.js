'use strict'

const net = require('node:net')
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
    await bot.waitForChunksToLoad()
    process.stderr.write(`[dustroute-bot] ${bot.username} joined ${config.host}:${config.port}\n`)
  })
  bot.on('end', reason => {
    spawned = false
    process.stderr.write(`[dustroute-bot] disconnected: ${String(reason)}; reconnecting in 3s\n`)
    if (!shuttingDown) reconnectTimer = setTimeout(connectBot, 3000)
  })
  bot.on('kicked', reason => process.stderr.write(`[dustroute-bot] kicked: ${String(reason)}\n`))
  bot.on('error', error => process.stderr.write(`[dustroute-bot] ${error.stack || error}\n`))
}

connectBot()

function directionFromRotation (yaw, pitch) {
  const cosPitch = Math.cos(pitch)
  return new Vec3(-Math.sin(yaw) * cosPitch, -Math.sin(pitch), -Math.cos(yaw) * cosPitch)
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

function propertiesOf (block) {
  return Object.fromEntries(
    Object.entries(block.getProperties()).map(([key, value]) => [key, String(value)])
  )
}

function posJson (pos) {
  return { x: pos.x, y: pos.y, z: pos.z }
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
      dimension: spawned ? currentDimension() : null
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
  if (method === 'preview_region') {
    requireDimension(params.dimension)
    return previewRegion(params.player, params.min, params.max)
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
