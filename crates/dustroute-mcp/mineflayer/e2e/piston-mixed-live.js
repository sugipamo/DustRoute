'use strict'

// Isolated fixture capture only. Server-side writes, not requested client tick
// delays, establish applied inputs in the companion Python comparator.
const fs = require('node:fs')
const { once } = require('node:events')
const mineflayer = require('mineflayer')
const { Vec3 } = require('vec3')
const fixture = JSON.parse(fs.readFileSync(process.env.DUSTROUTE_MIXED_FIXTURE, 'utf8'))
const output = process.env.DUSTROUTE_MIXED_OUTPUT
if (!output || fs.existsSync(output)) throw new Error('new output path required')
const origin = new Vec3(Number(process.env.DUSTROUTE_MIXED_X), 180, 1000)
if (!Number.isSafeInteger(origin.x)) throw new Error('explicit isolated X coordinate required')
const absolute = p => new Vec3(p.x, p.y, p.z).plus(origin)
const coords = p => { const q = absolute(p); return `${q.x} ${q.y} ${q.z}` }
const { min, max } = fixture.initial
const placementMode = fixture.placement_mode ?? 'replace'
if (!['replace', 'strict'].includes(placementMode)) throw new Error('unsupported placement_mode')
const settlingTicks = fixture.settling_ticks ?? 20
if (!Number.isInteger(settlingTicks) || settlingTicks < 20 || settlingTicks > 200) throw new Error('settling_ticks must be 20..200')
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const bot = mineflayer.createBot({ host: '127.0.0.1', port: 25565, username: 'dustroutetest', auth: 'offline', version: '1.21.11' })
let tick = 0
let disconnected = null
let ownsRegion = false
let ownsForceLoad = false
let phase = 'connect'
const report = {
  schema_version: 'dustroute.mixed-piston-live.v1', minecraft_version: '1.21.11',
  fixture: fixture.id, captured_at: new Date().toISOString(), origin, placement_mode: placementMode,
  known_region: { min: absolute(min), max: absolute(max) },
  evidence: 'client snapshots; authoritative input writes/ticks in separate server artifact',
  activations: [], packet_events: [], samples: [], cleanup: {}, complete: false
}
bot.on('physicsTick', () => { tick += 1 })
bot.on('error', e => { disconnected = e.message })
bot.on('kicked', r => { disconnected = String(r) })
bot.on('end', r => { disconnected = String(r) })
bot.on('blockUpdate', (before, after) => {
  if (!after) return
  const p = after.position.minus(origin)
  if (p.x < min.x || p.x > max.x || p.y < min.y || p.y > max.y || p.z < min.z || p.z > max.z) return
  report.packet_events.push({ phase, client_tick: tick, position: after.position,
    before: before && { name: `minecraft:${before.name}`, properties: before.getProperties() },
    after: { name: `minecraft:${after.name}`, properties: after.getProperties() } })
})
async function command (text, delay = 100) {
  if (disconnected) throw new Error(`disconnected: ${disconnected}`)
  bot.chat(text)
  await sleep(delay)
}
function read (p) {
  const block = bot.blockAt(absolute(p))
  if (!block) throw new Error(`unloaded ${JSON.stringify(p)}`)
  return block
}
function state (p) {
  const b = read(p)
  return { position: b.position, name: `minecraft:${b.name}`, properties: b.getProperties() }
}
function snapshot () {
  const blocks = []
  for (let x = min.x; x <= max.x; x++) {
    for (let y = min.y; y <= max.y; y++) {
      for (let z = min.z; z <= max.z; z++) blocks.push(state({ x, y, z }))
    }
  }
  return blocks
}
function empty () {
  for (const b of snapshot()) if (b.name !== 'minecraft:air') throw new Error(`occupied ${JSON.stringify(b)}`)
}
function sample (label) {
  if (fixture.sample_full_region) {
    report.samples.push({ label, client_tick: tick, blocks: snapshot() })
    return
  }
  const positions = fixture.initial.blocks.map(b => b.pos)
  report.samples.push({ label, client_tick: tick, blocks: positions.map(state) })
}
async function approach (p) {
  bot.creative.startFlying()
  bot.entity.velocity.set(0, 0, 0)
  await command(`/tp dustroutetest ${coords({ x: p.x, y: p.y + 1, z: p.z + 2 })}`)
  await bot.waitForTicks(4)
  bot.entity.velocity.set(0, 0, 0)
  await bot.creative.flyTo(absolute(p).offset(0.5, 0, 2.5))
  await bot.waitForTicks(4)
}
async function main () {
  await Promise.race([once(bot, 'spawn'), sleep(30000).then(() => { throw new Error('spawn timeout') })])
  await command('/gamemode creative dustroutetest')
  const low = absolute(min); const high = absolute(max)
  await command(`/forceload add ${low.x} ${low.z} ${high.x} ${high.z}`)
  ownsForceLoad = true
  await approach(fixture.inputs[0])
  for (let attempt = 0; ; attempt++) {
    try { empty(); break } catch (e) {
      if (!e.message.startsWith('unloaded') || attempt === 29) throw e
      await sleep(1000)
    }
  }
  ownsRegion = true
  phase = 'setup'
  for (const b of fixture.initial.blocks) {
    const properties = Object.entries(b.properties).map(([k, v]) => `${k}=${v}`).join(',')
    await command(`/setblock ${coords(b.pos)} ${b.name}${properties ? `[${properties}]` : ''} ${placementMode}`)
  }
  const warmupTicks = fixture.warmup_ticks ?? 20
  if (!Number.isInteger(warmupTicks) || warmupTicks < 20 || warmupTicks > 200) throw new Error('warmup_ticks must be 20..200')
  await bot.waitForTicks(warmupTicks)
  report.initial = snapshot()
  for (const b of fixture.initial.blocks) {
    const actual = state(b.pos)
    if (actual.name !== b.name || Object.entries(b.properties).some(([k, v]) => String(actual.properties[k]) !== String(v))) {
      throw new Error(`initial fixture mismatch ${JSON.stringify({ expected: b, actual })}`)
    }
  }
  let currentInput = 0
  if (fixture.align_first_input) await bot.waitForTicks(1)
  for (const [index, step] of fixture.steps.entries()) {
    if (step.input !== currentInput && !fixture.shared_viewpoint) await approach(fixture.inputs[step.input])
    currentInput = step.input
    if (step.wait_ticks) await bot.waitForTicks(step.wait_ticks)
    phase = `input_${index}`
    sample(`before_${index}`)
    const pos = fixture.inputs[step.input]
    const playerPosition = bot.entity.position.clone()
    if (playerPosition.distanceTo(absolute(pos)) > 4) throw new Error(`input out of range ${JSON.stringify(playerPosition)}`)
    if (fixture.packet_input) {
      // Avoid client look/ack latency in short-pulse trials. This is the same
      // ordinary interaction packet used by activateBlock; only the server
      // instrumented state write establishes whether/when it was applied.
      bot._client.write('block_place', {
        location: absolute(pos), direction: 1, hand: 0,
        cursorX: 0.5, cursorY: 0.5, cursorZ: 0.5,
        insideBlock: false, sequence: index + 1, worldBorderHit: false
      })
    } else {
      await bot.activateBlock(read(pos))
    }
    report.activations.push({ requested_level: step.powered, position: absolute(pos), player_position: playerPosition, packet_sent_client_tick: tick })
    if (step.wait_ticks > 0 && !fixture.packet_input) {
      for (let attempt = 0; ; attempt++) {
        if (read(pos).getProperties().powered === step.powered) break
        if (attempt === 5) throw new Error(`input was not applied ${JSON.stringify(report.activations.at(-1))}`)
        await bot.waitForTicks(1)
      }
    }
  }
  phase = 'settling'
  for (let i = 0; i < settlingTicks; i++) { await bot.waitForTicks(1); sample(`settling_${i}`) }
  report.final = snapshot()
  report.complete = true
}
const deadline = setTimeout(() => bot.end('mixed capture deadline'), 180000)
main().catch(error => {
  report.error = error.stack || String(error)
  process.exitCode = 1
}).finally(async () => {
  try {
    if (ownsRegion) {
      phase = 'cleanup'
      await command(`/fill ${coords(min)} ${coords(max)} minecraft:air`, 500)
      empty()
      report.cleanup.region_empty = true
    }
    if (ownsForceLoad) {
      const low = absolute(min); const high = absolute(max)
      await command(`/forceload remove ${low.x} ${low.z} ${high.x} ${high.z}`)
      report.cleanup.force_load_removed = true
    }
  } catch (error) {
    report.cleanup.error = error.stack || String(error)
    report.complete = false
    process.exitCode = 1
  }
  report.cleanup.disconnected_before_finish = disconnected
  fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n', { flag: 'wx' })
  clearTimeout(deadline)
  bot.quit('DustRoute mixed capture complete')
  console.log(JSON.stringify({ complete: report.complete, output, cleanup: report.cleanup }))
})
