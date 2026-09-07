'use strict'

// Isolated diagnostic only: no MCP policy or placement capability promotion.
const fs = require('node:fs')
const path = require('node:path')
const { execFileSync } = require('node:child_process')
const { createHash } = require('node:crypto')
const { isDeepStrictEqual } = require('node:util')
const mineflayer = require('mineflayer')
const { Vec3 } = require('vec3')
const root = path.resolve(__dirname, '../../../..')
const directory = path.join(root, 'crates/dustroute-translate/tests/fixtures/piston-low-layer')
const requested = new Set(process.argv.slice(2))
const available = fs.readdirSync(directory).filter(f => f.endsWith('.json')).sort()
for (const name of requested) {
  if (!available.includes(`${name}.json`)) throw new Error(`unknown piston case ${name}`)
}
const selected = available.filter(f => requested.size === 0 || requested.has(f.slice(0, -5)))
const offset = new Vec3(1100, 180, 1000)
const low = new Vec3(-3, -2, -4)
const high = new Vec3(7, 4, 6)
const report = { schema_version: 'dustroute.piston-low-layer-live.v1',
  captured_at: new Date().toISOString(), minecraft_version: '1.21.11',
  git_head: execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8' }).trim(),
  offset, timing_evidence: 'wall-clock and packet order only; no exact-tick equivalence claim',
  trials: [], events: [], cleanup_verified: false }
const bot = mineflayer.createBot({ host: '127.0.0.1', port: 25565,
  username: 'dustroutetest', auth: 'offline', version: '1.21.11' })
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const absolute = p => new Vec3(p.x, p.y, p.z).plus(offset)
const coords = p => { const q = absolute(p); return `${q.x} ${q.y} ${q.z}` }
let ownsRegion = false
let disconnected
let phase = 'connect'
bot.on('error', error => { disconnected = error.message })
bot.on('kicked', reason => { disconnected = String(reason) })
bot.on('end', reason => { disconnected = String(reason) })
bot.on('blockUpdate', (before, after) => {
  if (!after || report.events.length >= 10000) return
  const p = after.position.minus(offset)
  if (p.x < low.x || p.x > high.x || p.y < low.y || p.y > high.y || p.z < low.z || p.z > high.z) return
  report.events.push({ phase, elapsed_ms: Date.now() - startedAt, pos: p,
    before: before?.name, after: after.name, properties: after.getProperties() })
})
const startedAt = Date.now()
async function command (text) {
  if (disconnected) throw new Error(disconnected)
  bot.chat(text)
  await sleep(100)
}
function read (p) {
  const b = bot.blockAt(absolute(p))
  if (!b) throw new Error(`unloaded ${JSON.stringify(p)}`)
  return b
}
function checkEmpty () {
  for (let x = low.x; x <= high.x; x++) for (let y = low.y; y <= high.y; y++) for (let z = low.z; z <= high.z; z++) {
    if (read({ x, y, z }).name !== 'air') throw new Error('test region is not empty')
  }
}
async function clear () {
  await command(`/fill ${coords(low)} ${coords(high)} minecraft:air`)
  await sleep(500)
  checkEmpty()
}
function observe (test, full = false) {
  const records = []
  const { min, max } = test.initial
  for (let x = min.x; x <= max.x; x++) for (let y = min.y; y <= max.y; y++) for (let z = min.z; z <= max.z; z++) {
    const pos = { x, y, z }
    const b = read(pos)
    const properties = Object.fromEntries(Object.entries(b.getProperties()).filter(([k]) => full || test.properties.includes(k)))
    // Mineflayer exposes dust power as a numeric string; replay uses a number.
    if (Object.hasOwn(properties, 'power')) properties.power = Number(properties.power)
    records.push({ pos, name: `minecraft:${b.name}`, properties })
  }
  return records
}
function differences (actual, expected) {
  return actual.flatMap((value, i) => isDeepStrictEqual(value, expected[i]) ? [] : [{ actual: value, expected: expected[i] }])
}
function checkMechanicalExpectation (state, test, powered, index) {
  if (test.expected.phases) {
    return test.expected.phases[index].every(expected => {
      const actual = state.find(r => isDeepStrictEqual(r.pos, expected.pos))
      return actual?.name === expected.name && Object.entries(expected.properties).every(([key, value]) => actual.properties[key] === value)
    })
  }
  const at = x => state.find(r => r.pos.x === x && r.pos.y === 0 && r.pos.z === 0)
  const target = powered ? test.expected.extended_target_x : test.expected.retracted_target_x
  return at(0).properties.extended === powered && at(-1).properties.powered === powered &&
    at(target).name === test.expected.target_name &&
    (powered ? at(1).name === 'minecraft:piston_head' : at(target === 1 ? 2 : 1).name === 'minecraft:air')
}
async function checkTraversal (test, index) {
  const { start, end, open_by_phase: openByPhase } = test.passage
  bot.clearControlStates()
  bot.creative.stopFlying()
  await command('/gamemode adventure dustroutetest')
  await command(`/tp dustroutetest ${coords(start)}`)
  await sleep(400)
  const before = bot.entity.position.minus(offset)
  await bot.lookAt(absolute(end).offset(0, 1.62, 0), true)
  bot.setControlState('forward', true)
  try { await sleep(1000) } finally { bot.clearControlStates() }
  await sleep(300)
  const after = bot.entity.position.minus(offset)
  const crossed = after.z >= 1.2
  const blocked = after.z >= -0.5 && after.z <= -0.29
  const stayedInPassage = Math.abs(after.x - 2.5) < 0.2 && Math.abs(after.y) < 0.1
  const result = { expected_open: openByPhase[index], before, after, crossed, blocked,
    matched: stayedInPassage && (openByPhase[index] ? crossed : blocked),
    evidence: 'adventure mode, forward only, no jump/flight; client position after correction settling' }
  await command('/gamemode creative dustroutetest')
  await command(`/tp dustroutetest ${coords({ x: -1, y: 2, z: 2 })}`)
  bot.creative.startFlying()
  return result
}
async function main () {
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('spawn timeout')), 30000)
    bot.once('spawn', () => { clearTimeout(timer); resolve() })
    bot.once('error', error => { clearTimeout(timer); reject(error) })
  })
  await command('/gamemode creative dustroutetest')
  await command(`/tp dustroutetest ${coords({ x: -1, y: 2, z: 2 })}`)
  await sleep(1000)
  bot.creative.startFlying()
  for (let i = 0; ; i++) {
    try { checkEmpty(); break } catch (error) {
      if (!error.message.startsWith('unloaded') || i === 29) throw error
      await sleep(1000)
    }
  }
  ownsRegion = true
  for (const file of selected) {
    const source = fs.readFileSync(path.join(directory, file))
    const test = JSON.parse(source)
    const simulated = JSON.parse(execFileSync(path.join(root, 'target/debug/examples/piston_low_layer_replay'),
      [path.join(directory, file)], { cwd: root, maxBuffer: 8 * 1024 * 1024 }))
    for (let repetition = 0; repetition < 3; repetition++) {
      phase = `${test.id}/${repetition}/setup`
      await clear()
      for (const block of test.initial.blocks) {
        const props = Object.entries(block.properties).map(([k, v]) => `${k}=${v}`).join(',')
        await command(`/setblock ${coords(block.pos)} ${block.name}${props ? `[${props}]` : ''}`)
      }
      await sleep(test.settle_ms)
      const trial = { case_id: test.id, repetition, fixture_sha256: createHash('sha256').update(source).digest('hex'),
        initial: observe(test), full_initial: observe(test, true), simulator: simulated, phases: [] }
      report.trials.push(trial)
      trial.initial_differences = differences(trial.initial, simulated.initial)
      if (trial.initial_differences.length) {
        report.status = 'stopped_initial_mismatch'
        return
      }
      for (const [index, action] of test.actions.entries()) {
        const powered = typeof action === 'boolean' ? action : action.powered
        const input = typeof action === 'boolean' ? test.input : action.input
        phase = `${test.id}/${repetition}/${index}/${powered ? 'on' : 'off'}`
        if (bot.entity.position.distanceTo(absolute(input)) > 4) {
          await command(`/tp dustroutetest ${coords({ x: input.x, y: input.y + 2, z: input.z + 2 })}`)
          await sleep(200)
        }
        await bot.activateBlock(read(input))
        await sleep(test.settle_ms)
        const state = observe(test)
        const comparison = { input, powered, state, full_state: observe(test, true), expected_mechanics: checkMechanicalExpectation(state, test, powered, index),
          simulator_error: simulated.phases[index]?.error || null,
          differences: differences(state, simulated.phases[index]?.state || []) }
        trial.phases.push(comparison)
        if (comparison.simulator_error || comparison.differences.length || !comparison.expected_mechanics) {
          report.status = 'stopped_first_difference'
          console.log(`${phase}: stopped at first difference`)
          return
        }
        if (test.passage) {
          comparison.traversal = await checkTraversal(test, index)
          if (!comparison.traversal.matched) {
            report.status = 'stopped_traversal_mismatch'
            return
          }
        }
      }
      console.log(`${test.id} trial ${repetition + 1}: matched`)
    }
  }
  report.status = 'all_declared_cases_matched'
}
const deadline = setTimeout(() => bot.end('diagnostic 180-second deadline'), 180000)
main().catch(error => { report.status = 'infrastructure_error'; report.error = error.stack; process.exitCode = 1 })
  .finally(async () => {
    try {
      if (ownsRegion) { phase = 'cleanup'; await clear(); report.cleanup_verified = true }
    } catch (error) { report.cleanup_error = error.message; process.exitCode = 1 }
    const dir = path.join(root, '.local/e2e-artifacts')
    fs.mkdirSync(dir, { recursive: true })
    const serialized = JSON.stringify(report, null, 2) + '\n'
    fs.writeFileSync(path.join(dir, 'piston-low-layer-latest.json'), serialized)
    fs.writeFileSync(path.join(dir, `${report.captured_at.replaceAll(':', '-')}-piston-low-layer.json`), serialized)
    console.log(JSON.stringify({ status: report.status, trials: report.trials.length, cleanup_verified: report.cleanup_verified }))
    clearTimeout(deadline)
    bot.quit()
  })
