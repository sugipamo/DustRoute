'use strict'

// Bounded diagnostic for the declared door, on the private local test server.
// Does not use or change the MCP mutation policy. Commands are fixture inputs;
// only the separate lever probe uses normal player activation.
const fs = require('node:fs')
const path = require('node:path')
const { execFileSync } = require('node:child_process')
const { once } = require('node:events')
const { createHash } = require('node:crypto')
const mineflayer = require('mineflayer')
const { Vec3 } = require('vec3')

const root = path.resolve(__dirname, '../../../..')
const fixture = 'crates/dustroute-translate/tests/fixtures/3x3_piston_shuttle_fanout.json'
const scenario = JSON.parse(fs.readFileSync(path.join(root, fixture)))
const simulation = JSON.parse(execFileSync(path.join(root, 'target/debug/dustroute-cli'),
  ['run-piston-door', fixture, 'cycle', '--diagnostic'], { cwd: root, maxBuffer: 32 * 1024 * 1024 }))
const records = simulation.initial_state.blocks
const offset = { x: 1000, y: 180, z: 1000 }
const low = { x: -25, y: -2, z: -12 }
const high = { x: 5, y: 6, z: 13 }
const key = p => `${p.x},${p.y},${p.z}`
const pos = p => new Vec3(p.x + offset.x, p.y + offset.y, p.z + offset.z)
const coords = p => { const q = pos(p); return `${q.x} ${q.y} ${q.z}` }
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const opposite = { North: 'south', South: 'north', East: 'west', West: 'east' }
const names = { Solid: 'stone', Piston: 'piston', Repeater: 'repeater',
  RedstoneWire: 'redstone_wire', Lever: 'lever', RedstoneBlock: 'redstone_block' }
const index = new Map(records.map(r => [key(r.position), r.block]))
const supportAudit = records.filter(r => ['Repeater', 'RedstoneWire', 'Lever'].includes(r.block.kind))
  .map(r => ({ position: r.position, kind: r.block.kind,
    below: index.get(key({ ...r.position, y: r.position.y - 1 }))?.kind || 'Air' }))
const report = {
  schema_version: 'dustroute.piston-door-live-diagnostic.v1',
  captured_at: new Date().toISOString(), minecraft_version: '1.21.11',
  git_head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  fixture_sha256: createHash('sha256').update(fs.readFileSync(path.join(root, fixture))).digest('hex'),
  scenario_id: scenario.id, offset, region: { low, high },
  simulation: { status: simulation.status, trace_status: simulation.trace_status, placement_validation: simulation.placement_validation },
  support_audit: supportAudit,
  source_contract: records.filter(r => r.block.kind === 'RedstoneBlock'),
  evidence: 'packet-visible states; wall-clock settling, not server scheduler timing',
  mechanical_cycles: [], lever_cycles: [], events: [], cleanup_verified: false
}
const bot = mineflayer.createBot({ host: '127.0.0.1', port: 25565,
  username: 'dustroutetest', auth: 'offline', version: '1.21.11' })
let activePhase = 'connect'
let ownsRegion = false
let disconnected = null
bot.on('error', error => { disconnected = error.message })
bot.on('kicked', reason => { disconnected = String(reason) })
bot.on('end', reason => { disconnected = String(reason) })
bot.on('blockUpdate', (before, after) => {
  if (!after || report.events.length >= 20000) return
  const p = after.position.minus(new Vec3(offset.x, offset.y, offset.z))
  if (p.x < low.x || p.x > high.x || p.y < low.y || p.y > high.y || p.z < low.z || p.z > high.z) return
  report.events.push({ phase: activePhase, position: { x: p.x, y: p.y, z: p.z },
    before: before?.name, after: after.name, properties: after.getProperties() })
})
function observe (p) {
  const b = bot.blockAt(pos(p))
  if (!b) throw new Error(`unloaded position ${key(p)}`)
  return { position: p, name: b.name, properties: b.getProperties() }
}
async function command (text) {
  if (disconnected) throw new Error(`disconnected: ${disconnected}`)
  bot.chat(text)
  await sleep(80)
}
async function set (p, block) { await command(`/setblock ${coords(p)} minecraft:${block}`) }
async function clear () {
  await command(`/fill ${coords(low)} ${coords(high)} minecraft:air`)
  await sleep(500)
}
function assertRegionEmpty () {
  for (let x = low.x; x <= high.x; x++) {
    for (let y = low.y; y <= high.y; y++) {
      for (let z = low.z; z <= high.z; z++) {
        if (observe({ x, y, z }).name !== 'air') throw new Error('diagnostic region is not empty')
      }
    }
  }
}
function panel (expectedZ) {
  const cells = scenario.cells.map(({ x, y }) => ({ x, y,
    closed: observe({ x, y, z: 0 }), open: observe({ x, y, z: 1 }),
    open_piston: observe({ x, y, z: -1 }), close_piston: observe({ x, y, z: 2 }) }))
  return { expected_z: expectedZ, cells,
    matches: cells.every(c => c[expectedZ === 0 ? 'closed' : 'open'].name === 'stone' &&
      c[expectedZ === 0 ? 'open' : 'closed'].name === 'air' &&
      c.open_piston.name === 'piston' && c.close_piston.name === 'piston' &&
      c.open_piston.properties.extended === false && c.close_piston.properties.extended === false) }
}
async function directPulse (z) {
  for (const { x, y } of scenario.cells) await set({ x, y, z }, 'redstone_block')
  await sleep(1000)
  for (const { x, y } of scenario.cells) await set({ x, y, z }, 'air')
  await sleep(1000)
}
async function main () {
  await Promise.race([once(bot, 'spawn'), sleep(30000).then(() => { throw new Error('spawn timeout') })])
  await command('/gamemode creative dustroutetest')
  await command(`/tp dustroutetest ${coords({ x: -22, y: 5, z: 2 })}`)
  await sleep(1000)
  bot.creative.startFlying()
  // The local server may send fewer than Mineflayer's default 5x5 chunks.
  // Require the actual diagnostic volume instead of an unrelated chunk count.
  for (let attempt = 0; ; attempt++) {
    try { assertRegionEmpty(); break } catch (error) {
      if (!error.message.startsWith('unloaded position') || attempt === 29) throw error
      await sleep(1000)
    }
  }
  assertRegionEmpty()
  ownsRegion = true
  activePhase = 'mechanical_setup'
  for (const { x, y } of scenario.cells) {
    await set({ x, y, z: 0 }, 'stone')
    await set({ x, y, z: -1 }, 'piston[facing=south,extended=false]')
    await set({ x, y, z: 2 }, 'piston[facing=north,extended=false]')
  }
  report.mechanical_initial = panel(0)
  for (let cycle = 0; cycle < 3; cycle++) {
    activePhase = `mechanical_${cycle}_open`
    await directPulse(-2)
    const open = panel(1)
    activePhase = `mechanical_${cycle}_close`
    await directPulse(3)
    report.mechanical_cycles.push({ cycle, driver: 'command-injected rear power; no lever/fanout', open, closed: panel(0) })
    console.log(`mechanical cycle ${cycle + 1}: ${open.matches && report.mechanical_cycles.at(-1).closed.matches}`)
  }
  await clear()
  activePhase = 'literal_fanout_setup'
  // Materialize the actual coordinates. Do not invent supports or wire links.
  for (const { position: p, block: b } of [...records].sort((a, b) => a.position.y - b.position.y)) {
    let state = names[b.kind]
    if (!state) throw new Error(`unsupported materialization ${b.kind}`)
    if (b.kind === 'Repeater') state += `[facing=${opposite[b.facing]},delay=${b.delay},powered=false,locked=false]`
    if (b.kind === 'Piston') state += `[facing=${b.facing.toLowerCase()},extended=false]`
    if (b.kind === 'Lever') state += '[face=floor,facing=north,powered=false]'
    await set(p, state)
  }
  await sleep(2000)
  report.materialized = records.map(r => ({ expected: r, actual: observe(r.position) }))
  report.materialization_name_mismatches = report.materialized.filter(r => names[r.expected.block.kind] !== r.actual.name)
  // Isolate the missing controller with one explicitly recorded support addition.
  const lever = scenario.control.lever
  report.lever_probe_added_support = { ...lever, y: lever.y - 1 }
  await set(report.lever_probe_added_support, 'stone')
  await set(lever, 'lever[face=floor,facing=north,powered=false]')
  for (let cycle = 0; cycle < 3; cycle++) {
    const states = []
    for (const powered of [true, false]) {
      activePhase = `lever_${cycle}_${powered ? 'open' : 'close'}`
      await bot.activateBlock(bot.blockAt(pos(lever)))
      await sleep(5000)
      const actualLever = observe(lever)
      if (actualLever.properties.powered !== powered) throw new Error('normal lever activation failed')
      states.push({ powered, actual_lever: actualLever, panel: panel(powered ? 1 : 0),
        roots: [observe(scenario.control.open_source), observe(scenario.control.close_source)] })
    }
    report.lever_cycles.push({ cycle, states })
    console.log(`lever cycle ${cycle + 1}: ${states.every(s => s.panel.matches)}`)
  }
  report.status = report.mechanical_initial.matches &&
    report.mechanical_cycles.every(c => c.open.matches && c.closed.matches) &&
    report.materialization_name_mismatches.length === 0 &&
    report.lever_cycles.every(c => c.states.every(s => s.panel.matches))
    ? 'live_cycle_passed' : 'blocked_by_materialization_and_controller_contract'
}
const deadline = setTimeout(() => { report.error = 'overall 180-second deadline'; bot.end() }, 180000)
main().catch(error => { report.status = 'diagnostic_error'; report.error = error.stack; process.exitCode = 1 })
  .finally(async () => {
    try {
      if (ownsRegion) {
        activePhase = 'cleanup'
        await clear()
        assertRegionEmpty()
        report.cleanup_verified = true
      }
    } catch (error) { report.cleanup_error = error.message; process.exitCode = 1 }
    const file = path.join(root, '.local/e2e-artifacts/piston-door-live-diagnostic-latest.json')
    fs.mkdirSync(path.dirname(file), { recursive: true })
    const encoded = JSON.stringify(report, null, 2) + '\n'
    fs.writeFileSync(file, encoded)
    fs.writeFileSync(path.join(path.dirname(file), `${report.captured_at.replaceAll(':', '-')}-piston-door-live-diagnostic.json`), encoded)
    console.log(JSON.stringify({ status: report.status, report: file, cleanup_verified: report.cleanup_verified }))
    clearTimeout(deadline)
    bot.quit()
  })
