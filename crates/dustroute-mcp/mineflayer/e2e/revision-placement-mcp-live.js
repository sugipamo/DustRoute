'use strict'
// Private Java 1.21.11: observe, branch a revision, apply cumulative edits through common placement, undo.
const fs = require('node:fs')
const path = require('node:path')
const assert = require('node:assert/strict')
const mineflayer = require('mineflayer')
const { Vec3 } = require('vec3')
const { McpStdioClient } = require('./runtime')
const root = path.resolve(__dirname, '../../../..')
const offset = new Vec3(1100, 180, 1000)
const low = new Vec3(-3, -2, -4); const high = new Vec3(7, 3, 6)
const abs = p => new Vec3(p.x, p.y, p.z).plus(offset)
const coords = p => { const a = abs(p); return `${a.x} ${a.y} ${a.z}` }
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const bot = mineflayer.createBot({ host: '127.0.0.1', port: 25565, username: 'dustroutetest', auth: 'offline', version: '1.21.11' })
const mcp = new McpStdioClient(path.join(root, 'target/debug/dustroute-mcp'), [], { cwd: root, timeoutMs: 20000, env: { ...process.env, DUSTROUTE_SERVER_ADDRESS: '127.0.0.1:25565', DUSTROUTE_MCP_TRANSPORT: 'stdio', DUSTROUTE_ASSIST_PLAYER: 'dustroutetest', DUSTROUTE_BOT_BRIDGE: '127.0.0.1:25580', DUSTROUTE_READ_ONLY: 'false', DUSTROUTE_PREVIEW_REQUIRED: 'true', DUSTROUTE_MCP_TOOL_PROFILE: 'debug' } })
const report = { captured_at: new Date().toISOString(), trials: [], cleanup_verified: false }
let owned = false
const command = async text => { bot.chat(text); await sleep(120) }
function empty () {
  for (let x = low.x; x <= high.x; x++) for (let y = low.y; y <= high.y; y++) for (let z = low.z; z <= high.z; z++) {
    assert.equal(bot.blockAt(abs({ x, y, z }))?.name, 'air', `nonempty/unloaded ${x},${y},${z}`)
  }
}
async function clear () { await command(`/fill ${coords(low)} ${coords(high)} air`); await sleep(500); empty() }
async function capture () { return (await mcp.callTool('show_region', {})).circuit_id }
async function run () {
  await new Promise((resolve, reject) => { bot.once('spawn', resolve); bot.once('error', reject) })
  await command(`/tp dustroutetest ${coords({ x: -1, y: 2, z: 2 })}`)
  bot.creative.startFlying()
  await command('/tp DustRouteBot dustroutetest')
  await sleep(1500); empty(); owned = true
  await mcp.initialize()
  for (const [corner, p] of [['first', low], ['second', high]]) {
    await command(`/setblock ${coords(p)} stone`)
    await command(`/tp dustroutetest ${coords(p.offset(-3, 2, 0))}`)
    await sleep(500)
    let gaze
    for (let attempt = 0; attempt < 10; attempt++) {
      const from = p.offset(-3, 2, 0)
      const to = p.offset(0.5, 0.5, 0.5)
      const dx = to.x - from.x; const dz = to.z - from.z
      const yaw = Math.atan2(-dx, dz) * 180 / Math.PI
      const pitch = -Math.atan2(to.y - (from.y + 1.62), Math.hypot(dx, dz)) * 180 / Math.PI
      await command(`/tp dustroutetest ${coords(from)} ${yaw} ${pitch}`)
      await sleep(500)
      gaze = await mcp.callTool('get_player_gaze', { max_distance: 64 })
      const actual = gaze.observation?.targeted_block
      if (actual && actual.x === abs(p).x && actual.y === abs(p).y && actual.z === abs(p).z) break
    }
    report[`${corner}_gaze`] = gaze
    assert.deepEqual(gaze.observation.targeted_block, { x: abs(p).x, y: abs(p).y, z: abs(p).z })
    await mcp.callTool('set_region', { corner, max_distance: 64 })
    await command(`/setblock ${coords(p)} air`)
  }
  for (let trial = 0; trial < 3; trial++) {
    await clear()
    await command(`/setblock ${coords(new Vec3(0, 0, 0))} stone`)
    await command(`/setblock ${coords(new Vec3(1, 0, 0))} stone`)
    await command(`/setblock ${coords(new Vec3(0, 1, 0))} repeater[facing=north,delay=1,powered=false,locked=false]`)
    await sleep(500)
    const circuitId = await capture()
    const position = p => { const a = abs(p); return { x: a.x, y: a.y, z: a.z } }
    const parent = await mcp.callTool('test_circuit_change', { circuit_id: circuitId, changes: [{ position: position(new Vec3(0, 1, 0)), block: 'minecraft:repeater', properties: { facing: 'north', delay: '2', powered: 'false', locked: 'false' } }] })
    const revision = await mcp.callTool('test_circuit_change', { revision_id: parent.revision_id, changes: [{ position: position(new Vec3(1, 0, 0)), block: 'minecraft:air' }, { position: position(new Vec3(2, 0, 0)), block: 'minecraft:stone' }] })
    const proposal = await mcp.callTool('new_placement', { revision_id: revision.revision_id })
    assert.equal(proposal.plan.changes.length, 3)
    const unpreviewed = await mcp.callToolRaw('invoke_operation', { operation_id: proposal.operation_id, confirm: true })
    assert.equal(unpreviewed.ok, false)
    await mcp.callTool('show_operation', { operation_id: proposal.operation_id })
    await command(`/setblock ${coords(low)} stone`)
    const stale = await mcp.callToolRaw('invoke_operation', { operation_id: proposal.operation_id, confirm: true })
    assert.equal(stale.ok, false)
    await command(`/setblock ${coords(low)} air`)
    const applied = await mcp.callTool('invoke_operation', { operation_id: proposal.operation_id, confirm: true })
    assert.equal(applied.verified, true)
    await sleep(300)
    assert.equal(Number(bot.blockAt(abs(new Vec3(0, 1, 0))).getProperties().delay), 2)
    assert.equal(bot.blockAt(abs(new Vec3(1, 0, 0))).name, 'air')
    assert.equal(bot.blockAt(abs(new Vec3(2, 0, 0))).name, 'stone')
    await command(`/setblock ${coords(low)} stone`)
    const staleUndo = await mcp.callToolRaw('undo_operation', { operation_id: proposal.operation_id, confirm: true })
    assert.equal(staleUndo.ok, false)
    await command(`/setblock ${coords(low)} air`)
    const restored = await mcp.callTool('undo_operation', { operation_id: proposal.operation_id, confirm: true })
    assert.equal(restored.verified, true)
    await sleep(300)
    assert.equal(Number(bot.blockAt(abs(new Vec3(0, 1, 0))).getProperties().delay), 1)
    assert.equal(bot.blockAt(abs(new Vec3(1, 0, 0))).name, 'stone')
    assert.equal(bot.blockAt(abs(new Vec3(2, 0, 0))).name, 'air')
    report.trials.push({ parent, revision, proposal, unpreviewed, stale, applied, staleUndo, restored })
    console.log(`Revision placement trial ${trial + 1}: cumulative edit applied and undone`)
  }
  report.status = 'passed'
}
const deadline = setTimeout(() => { bot.end('deadline'); mcp.close() }, 180000)
run().catch(error => { report.status = 'failed'; report.error = error.stack; report.mcp_stderr = mcp.stderr; process.exitCode = 1 }).finally(async () => {
  try { if (owned) { await clear(); report.cleanup_verified = true } } catch (e) { report.cleanup_error = e.message; process.exitCode = 1 }
  fs.mkdirSync(path.join(root, '.local/e2e-artifacts'), { recursive: true })
  fs.writeFileSync(path.join(root, '.local/e2e-artifacts/revision-placement-mcp-latest.json'), JSON.stringify(report, null, 2) + '\n')
  console.log(JSON.stringify({ status: report.status, error: report.error, cleanup_verified: report.cleanup_verified }))
  clearTimeout(deadline); mcp.close(); bot.quit()
})
