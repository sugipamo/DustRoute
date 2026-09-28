'use strict'
// Isolated custom Assembly trial. All mechanism construction/removal is done
// through public MCP; commands position the actor, test guards, and optionally
// remove one explicitly declared part to exercise reconstruction.
const fs = require('node:fs')
const path = require('node:path')
const assert = require('node:assert/strict')
const { once } = require('node:events')
const mineflayer = require('mineflayer')
const { Vec3 } = require('vec3')
const { confirmRegion } = require('../readback')
const { McpStdioClient } = require('./runtime')
const root = path.resolve(__dirname, '../../../..')
const fixture = JSON.parse(fs.readFileSync(process.env.DUSTROUTE_ASSEMBLY_FIXTURE, 'utf8'))
const output = process.env.DUSTROUTE_ASSEMBLY_OUTPUT
assert(output && !fs.existsSync(output), 'new artifact path required')
const origin = new Vec3(Number(process.env.DUSTROUTE_ASSEMBLY_X), 180, 1000)
assert(Number.isSafeInteger(origin.x), 'explicit isolated coordinate required')
const rotation = process.env.DUSTROUTE_ASSEMBLY_ROTATION || 'r90'
// Target probes are simulated at the actual destination, including directional
// properties. Legacy source-frame snapshots remain valid only for R0.
const targetProbes = fixture.live_probes?.target_transform
if (targetProbes) {
  assert.deepEqual(targetProbes, { source_anchor: { x: 0, y: 0, z: 0 }, target_anchor: { x: origin.x, y: origin.y, z: origin.z }, rotation }, 'probe target differs from live target')
  assert.deepEqual(fixture.target_construction?.transform, targetProbes)
} else if (fixture.live_probes) assert.equal(rotation, 'r0', 'source-frame snapshot probes require R0')
const persistence = process.env.DUSTROUTE_ASSEMBLY_PERSISTENCE === 'true'
const turns = { r0: p => [p.x, p.z], r90: p => [-p.z, p.x], r180: p => [-p.x, -p.z], r270: p => [p.z, -p.x] }
assert(turns[rotation], 'supported rotation required')
const transform = p => { const [x, z] = turns[rotation](p); return new Vec3(x, p.y, z).plus(origin) }
const probePosition = targetProbes ? p => new Vec3(p.x, p.y, p.z) : transform
const region = fixture.request.behavior_context.known_region
const a = transform(region.min); const b = transform(region.max)
const low = new Vec3(Math.min(a.x, b.x), Math.min(a.y, b.y), Math.min(a.z, b.z))
const high = new Vec3(Math.max(a.x, b.x), Math.max(a.y, b.y), Math.max(a.z, b.z))
const coords = p => `${p.x} ${p.y} ${p.z}`
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const bot = mineflayer.createBot({ host: '127.0.0.1', port: 25565, username: 'dustroutetest', auth: 'offline', version: '1.21.11' })
function startMcp () { return new McpStdioClient(path.join(root, 'target/debug/dustroute-mcp'), [], { cwd: root, timeoutMs: 180000, env: {
  ...process.env, DUSTROUTE_SERVER_ADDRESS: '127.0.0.1:25565', DUSTROUTE_MCP_TRANSPORT: 'stdio',
  DUSTROUTE_ASSIST_PLAYER: 'dustroutetest', DUSTROUTE_BOT_BRIDGE: '127.0.0.1:25580',
  DUSTROUTE_READ_ONLY: 'false', DUSTROUTE_PREVIEW_REQUIRED: 'true'
} }) }
let mcp = startMcp()
const report = { schema_version: 'dustroute.custom-assembly-live.v1', captured_at: new Date().toISOString(), origin,
  rotation, known_region: { min: low, max: high }, calls: [], inputs: [], cleanup: {}, status: 'running' }
let owned = false
let forced = false
async function command (text) { bot.chat(text); await sleep(150) }
function empty () {
  for (let x = low.x; x <= high.x; x++) for (let y = low.y; y <= high.y; y++) for (let z = low.z; z <= high.z; z++) {
    assert.equal(bot.blockAt(new Vec3(x, y, z))?.name, 'air', `occupied/unloaded ${x},${y},${z}`)
  }
}
function readbackProbe (probe) {
  const key = p => `${p.x},${p.y},${p.z}`
  if (targetProbes) {
    assert.deepEqual(probe.expected.min, { x: low.x, y: low.y, z: low.z })
    assert.deepEqual(probe.expected.max, { x: high.x, y: high.y, z: high.z })
  }
  const expected = new Map(probe.expected.blocks.map(b => [key(probePosition(b.pos)), b]))
  const observed = []
  for (let x = low.x; x <= high.x; x++) for (let y = low.y; y <= high.y; y++) for (let z = low.z; z <= high.z; z++) {
    const pos = new Vec3(x, y, z)
    const actual = bot.blockAt(pos)
    assert(actual, `unloaded probe ${key(pos)}`)
    const wanted = expected.get(key(pos)) || { name: 'minecraft:air', properties: {} }
    assert.equal(`minecraft:${actual.name}`, wanted.name, `probe block ${key(pos)}`)
    const properties = Object.fromEntries(Object.entries(actual.getProperties()).map(([k, v]) => [k, String(v)]))
    assert.deepEqual(properties, wanted.properties, `probe properties ${key(pos)}`)
    if (actual.name !== 'air') observed.push({ pos, name: wanted.name, properties })
  }
  const aperture = fixture.live_probes.aperture.map(p => {
    const pos = probePosition(p)
    const name = bot.blockAt(pos)?.name
    assert.equal(name, probe.powered ? 'smooth_quartz' : 'air', `aperture ${key(pos)}`)
    return { pos, name }
  })
  return { complete_region: true, blocks: observed, aperture,
    completion_certified: false, evidence: 'matching client snapshot; hidden pending events not observed' }
}
function failureSnapshot () {
  const blocks = []; const unknown = []
  for (let x = low.x; x <= high.x; x++) for (let y = low.y; y <= high.y; y++) for (let z = low.z; z <= high.z; z++) {
    const pos = new Vec3(x, y, z); const block = bot.blockAt(pos)
    if (!block) unknown.push(pos)
    else if (block.name !== 'air') blocks.push({ pos, name: `minecraft:${block.name}`,
      properties: Object.fromEntries(Object.entries(block.getProperties()).map(([k, v]) => [k, String(v)])) })
  }
  return { min: low, max: high, blocks, unknown, complete: unknown.length === 0 }
}
async function confirmEmpty (label) {
  const candidate = failureSnapshot()
  assert(candidate.complete, 'complete client coverage required before server confirmation')
  const confirmed = await confirmRegion(bot, candidate, 'minecraft:overworld')
  assert.equal(confirmed.blocks.length, 0, 'server-confirmed empty region required')
  report.empty_readbacks ??= {}
  report.empty_readbacks[label] = confirmed.readback
}
async function call (name, args, ok = true) {
  const response = await mcp.callToolRaw(name, args)
  report.calls.push({ name, args, response })
  assert.equal(response.ok, ok, `${name}: ${JSON.stringify(response)}`)
  return response
}
async function diagnoseUnchanged (instanceId, label) {
  const before = failureSnapshot()
  assert(before.complete)
  const result = await call('manage_assembly', { action: 'diagnose', instance_id: instanceId })
  const after = failureSnapshot()
  assert(after.complete)
  assert.deepEqual(after.blocks, before.blocks, 'diagnosis changed the observed world')
  assert.equal(result.diagnosis.world_writes, false)
  assert.equal(result.diagnosis.functional_test_performed, false)
  assert.equal(result.diagnosis.cause, 'not_inferred')
  report.diagnoses ||= []
  report.diagnoses.push({ label, diagnosis: result.diagnosis, world_unchanged: true })
  return result.diagnosis
}
async function restartMcp () {
  const beforePid = mcp.process.pid
  const exited = once(mcp.process, 'exit')
  mcp.close()
  const [exitCode, signal] = await exited
  mcp = startMcp()
  await mcp.initialize()
  report.process_restarts ||= []
  report.process_restarts.push({ before_pid: beforePid, after_pid: mcp.process.pid, exit_code: exitCode, signal })
}
async function main () {
  await Promise.race([once(bot, 'spawn'), sleep(30000).then(() => { throw new Error('actor spawn timeout') })])
  await command('/gamemode creative dustroutetest')
  bot.creative.startFlying()
  bot.entity.velocity.set(0, 0, 0)
  await command(`/tp dustroutetest ${coords(origin.offset(3, 4, -3))}`)
  await command('/tp DustRouteBot dustroutetest')
  await command(`/forceload add ${low.x} ${low.z} ${high.x} ${high.z}`)
  forced = true
  await bot.waitForChunksToLoad()
  await sleep(1000)
  empty(); await confirmEmpty('before_setup'); owned = true
  await mcp.initialize()
  await call('test_circuit_change', { blueprint: { action: 'import', records: fixture.records } })
  const request = structuredClone(fixture.request)
  const { known_region, input_levers, root_limits } = request.behavior_context
  request.behavior_context = { piston: { known_region, input_levers, root_limits } }
  const proposal = await call('test_circuit_change', { blueprint: { action: 'propose_update', request } })
  await call('show_operation', { operation_id: proposal.operation_id })
  await call('invoke_operation', { operation_id: proposal.operation_id, confirm: true, blueprint_decision: { action: 'adopt' } })
  // Reopen the real persistent archive in a new MCP process before placement.
  await restartMcp()
  report.restart = { after_adoption: true, transport: 'new_stdio_process' }
  const plan = await call('new_placement', { assembly_revision_id: fixture.request.candidate_state.id,
    assembly_target: { source_anchor: { x: 0, y: 0, z: 0 }, target_anchor: origin, rotation } })
  if (fixture.target_construction) {
    assert.deepEqual(plan.construction_steps, fixture.target_construction.build, 'public construction differs from target audit')
    assert.deepEqual(plan.undo_steps, fixture.target_construction.remove, 'public teardown differs from target audit')
    report.target_plan_matches_audit = true
  }
  const id = plan.operation_id
  await call('invoke_operation', { operation_id: id, confirm: true }, false)
  await call('show_operation', { operation_id: id })
  await command(`/setblock ${coords(low)} minecraft:stone`)
  await call('invoke_operation', { operation_id: id, confirm: true }, false)
  await command(`/setblock ${coords(low)} minecraft:air`)
  const applied = await call('invoke_operation', { operation_id: id, confirm: true })
  assert.equal(applied.status, 'verified')
  assert.equal(applied.verified_steps, plan.construction_steps.length)
  if (persistence) {
    assert.equal(applied.instance_id, id)
    await restartMcp()
    report.restart.after_placement = true
    const listed = await call('manage_assembly', { action: 'list' })
    assert.equal(listed.instances.length, 1)
    assert.equal(listed.instances[0].instance_id, id)
    assert.equal(listed.instances[0].state, 'applied')
    const saved = await call('manage_assembly', { action: 'get', instance_id: id })
    assert.equal(saved.fresh_observation, false)
    const matching = await call('manage_assembly', { action: 'observe', instance_id: id })
    assert.equal(matching.observation.status, 'matches')
    assert.equal(matching.observation.revalidation.status, 'passed')
    if (fixture.diagnosis_trial) {
      const diagnosis = await diagnoseUnchanged(id, 'initial_open')
      assert.equal(diagnosis.status, 'matches_reference')
    }
    await call('undo_operation', { operation_id: id, confirm: true }, false)
    // Server chunks stay force-loaded, but the observing client temporarily
    // loses them. Missing observations must not authorize a removal.
    await command(`/tp DustRouteBot ${coords(origin.offset(2048, 0, 0))}`)
    await bot.waitForTicks(40)
    const unavailable = await call('manage_assembly', { action: 'plan_removal', instance_id: id }, false)
    assert.equal(unavailable.observation.status, 'observation_incomplete')
    if (fixture.diagnosis_trial) {
      const diagnosis = await diagnoseUnchanged(id, 'missing_client_chunks')
      assert.equal(diagnosis.status, 'observation_unavailable')
      assert.deepEqual(diagnosis.findings, [])
    }
    await command('/tp DustRouteBot dustroutetest')
    await bot.waitForTicks(30)
    await command(`/setblock ${coords(low)} minecraft:${fixture.diagnosis_trial ? 'chest[facing=north,type=single,waterlogged=false]' : 'stone'}`)
    const changed = await call('manage_assembly', { action: 'plan_removal', instance_id: id }, false)
    assert.equal(changed.observation.status, 'changed')
    if (fixture.diagnosis_trial) {
      const diagnosis = await diagnoseUnchanged(id, 'player_added_chest')
      assert.equal(diagnosis.status, 'differences_found')
      assert.equal(diagnosis.summary.by_kind.unexpected, 1)
      assert.equal(diagnosis.repair.status, 'blocked')
      const finding = diagnosis.findings.find(f => f.kinds.includes('unexpected'))
      assert.deepEqual(finding.position, { x: low.x, y: low.y, z: low.z })
      assert.equal(finding.observed.name, 'minecraft:chest')
      const refused = await call('manage_assembly', { action: 'plan_reconstruction', instance_id: id }, false)
      assert.equal(refused.diagnosis.summary.by_kind.unexpected, 1)
    }
    await command(`/setblock ${coords(low)} minecraft:air`)
    report.persistence_checks = { listed_after_restart: true, fresh_review: true, missing_chunks_refused: true, external_change_detected: true }
  }
  const input = transform(fixture.request.behavior_context.input_levers[0])
  const body = transform(fixture.probe_body || { x: 0, y: 1, z: 0 })
  bot.entity.velocity.set(0, 0, 0)
  await command(`/tp dustroutetest ${coords(input.offset(0.5, 0, 2.5))}`)
  await bot.waitForTicks(6)
  if (fixture.reconstruction_trial) {
    assert(persistence, 'reconstruction trial requires durable instances')
    const position = transform(fixture.reconstruction_trial.remove_position)
    assert.equal(bot.blockAt(position)?.name, 'smooth_quartz', 'declared missing-part trial only')
    const before = failureSnapshot()
    assert(before.complete)
    await command(`/setblock ${coords(position)} minecraft:air replace`)
    await bot.waitForTicks(100)
    const damaged = await call('manage_assembly', { action: 'observe', instance_id: id })
    assert.equal(damaged.observation.status, 'changed')
    if (fixture.diagnosis_trial) {
      const diagnosis = await diagnoseUnchanged(id, 'player_removed_quartz')
      assert.equal(diagnosis.summary.by_kind.missing, 1)
      assert.equal(diagnosis.summary.differing_positions, 1)
      assert.deepEqual(diagnosis.findings[0].position, { x: position.x, y: position.y, z: position.z })
      assert.equal(diagnosis.findings[0].target.name, 'minecraft:smooth_quartz')
      assert.equal(diagnosis.repair.status, 'plan_available')
    }
    const reconstruction = await call('manage_assembly', { action: 'plan_reconstruction', instance_id: id })
    assert.equal(reconstruction.reconstruction_conditions.server_readiness_proven, false)
    assert(reconstruction.differences.length > 0)
    await call('invoke_operation', { operation_id: reconstruction.operation_id, confirm: true }, false)
    await call('show_operation', { operation_id: reconstruction.operation_id })
    await command(`/setblock ${coords(low)} minecraft:stone`)
    await call('invoke_operation', { operation_id: reconstruction.operation_id, confirm: true }, false)
    await command(`/setblock ${coords(low)} minecraft:air`)
    const repaired = await call('invoke_operation', { operation_id: reconstruction.operation_id, confirm: true })
    assert.equal(repaired.instance_id, id)
    assert.equal(repaired.verified_steps, reconstruction.reconstruction.steps.length)
    await restartMcp()
    const restored = await call('manage_assembly', { action: 'observe', instance_id: id })
    assert.equal(restored.instance.state, 'applied')
    assert.equal(restored.observation.status, 'matches')
    assert.equal(restored.instance.attempts.length, 2)
    assert.deepEqual(restored.instance.attempts[1].reconstruction, reconstruction.reconstruction)
    report.reconstruction_trial = { removed_part: position, before, damaged: damaged.observation,
      preview_required: true, changed_after_preview_refused: true, verified_steps: repaired.verified_steps,
      restored_after_restart: true }
  }
  for (const probe of fixture.live_probes?.snapshots || [{ powered: true }, { powered: false }]) {
    const powered = probe.powered
    await bot.activateBlock(bot.blockAt(input))
    report.inputs.push({ position: input, powered, evidence: 'actual application ticks retained in server capture' })
    await bot.waitForTicks(fixture.live_probes?.wait_ticks || 20)
    if (fixture.live_probes) {
      report.inputs.at(-1).whole_region_readback = readbackProbe(probe)
      report.inputs.at(-1).client_wait_ticks = fixture.live_probes.wait_ticks
    } else {
      const actual = bot.blockAt(body)
      assert.equal(actual.getProperties().extended, powered)
      report.inputs.at(-1).body_readback = { name: actual.name, properties: actual.getProperties() }
    }
    if (fixture.diagnosis_trial) {
      const diagnosis = await diagnoseUnchanged(id, powered ? 'normal_closed' : 'normal_open')
      assert.equal(diagnosis.reference.mode, 'observed_inputs')
      assert.equal(diagnosis.status, 'matches_reference')
      assert.equal(diagnosis.summary.differing_positions, 0)
      assert.equal(diagnosis.repair.status, 'not_needed_for_reference_match')
    }
    if (powered) {
      if (fixture.require_stair_readback_correction) {
        const actual = await call('manage_assembly', { action: 'observe', instance_id: id })
        const corrections = actual.observation.readbacks.flatMap(r => r.corrections)
        assert(corrections.some(c => c.confirmed_shape === 'straight'), 'public readback must independently correct the stale stair shape')
        report.stair_readback = actual.observation
      }
      if (persistence) {
        const changed = await call('manage_assembly', { action: 'plan_removal', instance_id: id }, false)
        assert.equal(changed.observation.status, 'changed')
      } else await call('undo_operation', { operation_id: id, confirm: true }, false)
    }
  }
  let undone
  if (persistence) {
    const removal = await call('manage_assembly', { action: 'plan_removal', instance_id: id })
    await call('invoke_operation', { operation_id: removal.operation_id, confirm: true }, false)
    await call('show_operation', { operation_id: removal.operation_id })
    await command(`/setblock ${coords(low)} minecraft:stone`)
    await call('invoke_operation', { operation_id: removal.operation_id, confirm: true }, false)
    await command(`/setblock ${coords(low)} minecraft:air`)
    undone = await call('invoke_operation', { operation_id: removal.operation_id, confirm: true })
    assert.equal(undone.verified_steps, removal.removal_steps.length)
    await restartMcp()
    report.restart.after_removal = true
    const removed = await call('manage_assembly', { action: 'observe', instance_id: id })
    assert.equal(removed.instance.state, 'removed')
    assert.equal(removed.observation.status, 'matches')
    assert.equal(removed.observation.removal_eligible, false)
    report.persistence_checks.preview_required = true
    report.persistence_checks.changed_after_preview_refused = true
    report.persistence_checks.removal_retained = true
  } else undone = await call('undo_operation', { operation_id: id, confirm: true })
  assert.equal(undone.status, 'verified')
  empty()
  await confirmEmpty('after_public_removal')
  report.status = 'passed'
}
const deadline = setTimeout(() => { bot.end('construction trial deadline'); mcp.close() }, persistence ? 900000 : 300000)
main().catch(async error => {
  report.status = 'failed'; report.error = error.stack || String(error); report.mcp_stderr = mcp.stderr; process.exitCode = 1
  if (owned) {
    // Retain the failed world before cleanup. A later matching observation is
    // diagnostic only: it never retries/promotes the consumed MCP operation.
    try {
      report.failure_observations = [{ client_wait_ticks: 0, snapshot: failureSnapshot() }]
      await bot.waitForTicks(100)
      report.failure_observations.push({ client_wait_ticks: 100, snapshot: failureSnapshot() })
    } catch (error) { report.failure_observation_error = String(error) }
  }
}).finally(async () => {
  try {
    if (owned) { await command(`/fill ${coords(low)} ${coords(high)} minecraft:air`); await bot.waitForTicks(20); empty(); await confirmEmpty('after_cleanup'); report.cleanup.region_empty = true }
    if (forced) { await command(`/forceload remove ${low.x} ${low.z} ${high.x} ${high.z}`); report.cleanup.force_load_removed = true }
  } catch (error) { report.cleanup.error = String(error); report.status = 'failed'; process.exitCode = 1 }
  fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n', { flag: 'wx' })
  console.log(JSON.stringify({ status: report.status, error: report.error, cleanup: report.cleanup }))
  clearTimeout(deadline); mcp.close(); bot.quit()
})
