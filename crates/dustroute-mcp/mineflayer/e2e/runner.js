'use strict'

const fs = require('node:fs')
const path = require('node:path')
const mineflayer = require('mineflayer')
const { Vec3 } = require('vec3')
const { McpStdioClient, assertExpectation, resolveReferences } = require('./runtime')

const root = path.resolve(__dirname, '../../../..')
const scenarioDir = path.join(__dirname, 'scenarios')
const playerName = process.env.DUSTROUTE_E2E_PLAYER || 'dustroutetest'
const assistantName = process.env.DUSTROUTE_BOT_NAME || 'DustRouteBot'
const serverAddress = process.env.DUSTROUTE_SERVER_ADDRESS || '127.0.0.1:25565'
const [host, portText] = serverAddress.split(':')
const port = Number(portText)
const version = process.env.DUSTROUTE_MC_VERSION || '1.21.11'
const selected = new Set(process.argv.slice(2))
const timeoutMs = Number(process.env.DUSTROUTE_E2E_TIMEOUT_MS || 120000)
const artifactDir = path.join(root, '.local', 'e2e-artifacts')

function allocateRunSlot () {
  if (process.env.DUSTROUTE_E2E_RUN_SLOT != null) return Number(process.env.DUSTROUTE_E2E_RUN_SLOT)
  const counter = path.join(root, '.local', 'e2e-run-slot')
  fs.mkdirSync(path.dirname(counter), { recursive: true })
  let previous = 0
  try { previous = Number(fs.readFileSync(counter, 'utf8')) || 0 } catch {}
  const next = (previous + 1) % 32
  fs.writeFileSync(counter, `${next}\n`)
  return next
}

const runSlot = allocateRunSlot()
if (!Number.isInteger(runSlot) || runSlot < 0 || runSlot > 1024) throw new Error('DUSTROUTE_E2E_RUN_SLOT must be 0..1024')
const xOffset = runSlot * 256

if (!/^[A-Za-z0-9_]{1,16}$/.test(playerName)) throw new Error('DUSTROUTE_E2E_PLAYER must be a valid 1-16 character Minecraft name')
if (!Number.isInteger(timeoutMs) || timeoutMs < 1000 || timeoutMs > 600000) throw new Error('DUSTROUTE_E2E_TIMEOUT_MS must be 1000..600000')

function connectPlayer () {
  return new Promise((resolve, reject) => {
    const bot = mineflayer.createBot({ host, port, username: playerName, auth: 'offline', version, hideErrors: false })
    const timer = setTimeout(() => reject(new Error('test player spawn timed out')), 30000)
    bot.once('spawn', async () => {
      try {
        await bot.waitForChunksToLoad()
        bot.chat(`/gamemode creative ${playerName}`)
        await bot.waitForTicks(2)
        clearTimeout(timer)
        resolve(bot)
      } catch (error) { reject(error) }
    })
    bot.once('error', reject)
    bot.once('kicked', reason => reject(new Error(`test player kicked: ${String(reason)}`)))
    bot.on('end', reason => { bot.dustrouteDisconnectReason = String(reason) })
  })
}

function sleep (milliseconds) {
  return new Promise(resolve => setTimeout(resolve, milliseconds))
}

async function command (bot, text) {
  if (bot.dustrouteDisconnectReason) throw new Error(`test player disconnected: ${bot.dustrouteDisconnectReason}`)
  const expanded = text.replaceAll('${player}', playerName).replaceAll('${assistant}', assistantName)
  const parts = expanded.split(' ')
  const shift = index => { parts[index] = String(Number(parts[index]) + xOffset) }
  if (parts[0] === '/fill') {
    shift(1)
    shift(4)
  } else if (parts[0] === '/setblock') {
    shift(1)
  } else if (parts[0] === '/tp' && parts.length >= 5 && Number.isFinite(Number(parts[2]))) {
    shift(2)
  }
  bot.chat(parts.join(' '))
  await sleep(300)
}

async function aim (bot, mcp, step, footings) {
  const footing = {
    x: Math.floor(step.position.x) + xOffset,
    y: Math.floor(step.position.y) - 1,
    z: Math.floor(step.position.z)
  }
  footings.push(footing)
  bot.chat(`/setblock ${footing.x} ${footing.y} ${footing.z} minecraft:barrier`)
  await sleep(300)
  await command(bot, `/gamemode creative ${playerName}`)
  bot.chat(`/tp ${playerName} ${step.position.x + xOffset} ${step.position.y} ${step.position.z}`)
  await sleep(300)
  bot.creative.startFlying()
  await command(bot, `/tp ${assistantName} ${playerName}`)
  await sleep(5000)
  await sleep(1000)
  await bot.waitForChunksToLoad()
  const target = { ...step.target, x: step.target.x + xOffset }
  await bot.lookAt(new Vec3(target.x + 0.5, target.y + 0.5, target.z + 0.5), true)
  await sleep(500)
  let lastTarget = null
  let lastObservation = null
  for (let attempt = 0; attempt < 20; attempt++) {
    const gaze = await mcp.callTool('get_player_gaze', { player: playerName, max_distance: 64 })
    lastObservation = gaze.observation
    const target = gaze.observation && gaze.observation.targeted_block
    lastTarget = target
    if (target && target.x === step.target.x + xOffset && target.y === step.target.y && target.z === step.target.z) return gaze
    await bot.lookAt(new Vec3(step.target.x + xOffset + 0.5, step.target.y + 0.5, step.target.z + 0.5), true)
    await sleep(250)
  }
  throw new Error(`gaze did not settle on ${JSON.stringify(step.target)}; last observed ${JSON.stringify(lastTarget)} from ${JSON.stringify(lastObservation)}`)
}

async function runScenario (bot, mcp, scenario) {
  const results = {}
  const footings = []
  let currentStep = -1
  let primaryError = null
  process.stdout.write(`\n[scenario] ${scenario.name}\n`)
  try {
    const firstAim = scenario.steps.find(step => step.kind === 'aim')
    if (firstAim) {
      bot.chat(`/gamemode creative ${playerName}`)
      bot.chat(`/tp ${playerName} ${firstAim.position.x + xOffset} ${firstAim.position.y} ${firstAim.position.z}`)
      await sleep(300)
      bot.creative.startFlying()
      bot.chat(`/tp ${assistantName} ${playerName}`)
      await sleep(5000)
      await bot.waitForChunksToLoad()
    }
    for (const [index, step] of scenario.steps.entries()) {
      currentStep = index
      if (bot.dustrouteDisconnectReason) throw new Error(`test player disconnected: ${bot.dustrouteDisconnectReason}`)
      if (step.kind === 'command') {
        for (const text of step.commands) await command(bot, text)
      } else if (step.kind === 'aim') {
        results[step.save || 'gaze'] = await aim(bot, mcp, step, footings)
      } else if (step.kind === 'mcp') {
        const args = resolveReferences(step.arguments || {}, results)
        results[step.save] = await mcp.callTool(step.tool, args)
        if (process.env.DUSTROUTE_E2E_VERBOSE === 'true') {
          process.stdout.write(`${JSON.stringify({ step: step.save, result: results[step.save] }, null, 2)}\n`)
        }
      } else if (step.kind === 'mcp_error') {
        const args = resolveReferences(step.arguments || {}, results)
        results[step.save] = await mcp.callToolRaw(step.tool, args)
        if (results[step.save].ok !== false) throw new Error(`${step.tool} unexpectedly succeeded`)
      } else if (step.kind === 'mcp_with_commands') {
        const args = resolveReferences(step.arguments || {}, results)
        const pending = mcp.callToolRaw(step.tool, args)
        await sleep(Number(step.delay_ticks || 1) * 50)
        for (const text of step.commands || []) await command(bot, text)
        results[step.save] = await pending
        if (step.require_error === true && results[step.save].ok !== false) {
          throw new Error(`${step.tool} unexpectedly succeeded`)
        }
      } else if (step.kind === 'assert') {
        for (const expectation of step.expect) assertExpectation(results[step.from], expectation)
      } else if (step.kind === 'wait') {
        await sleep(step.ticks * 50)
      } else {
        throw new Error(`unknown scenario step ${step.kind}`)
      }
    }
    process.stdout.write(`[pass] ${scenario.name}\n`)
  } catch (error) {
    primaryError = error
    fs.mkdirSync(artifactDir, { recursive: true })
    const artifact = path.join(artifactDir, `${Date.now()}-${scenario.name}.json`)
    fs.writeFileSync(artifact, JSON.stringify({
      scenario: scenario.name,
      run_slot: runSlot,
      x_offset: xOffset,
      failed_step_index: currentStep,
      failed_step: scenario.steps[currentStep],
      error: error.stack || String(error),
      actor: { position: bot.entity && bot.entity.position, yaw: bot.entity && bot.entity.yaw, pitch: bot.entity && bot.entity.pitch },
      actor_disconnect_reason: bot.dustrouteDisconnectReason || null,
      results,
      mcp_stderr: mcp.stderr
    }, null, 2))
    error.message = `${error.message} (artifact: ${artifact})`
    error.stack = `${error.stack || error.message}\nArtifact: ${artifact}`
    throw error
  } finally {
    try {
      for (const text of scenario.cleanup || []) await command(bot, text)
      for (const footing of footings) {
        bot.chat(`/setblock ${footing.x} ${footing.y} ${footing.z} minecraft:air`)
        await sleep(300)
      }
    } catch (cleanupError) {
      if (primaryError) process.stderr.write(`[cleanup] ${scenario.name}: ${cleanupError.stack || cleanupError}\n`)
      else throw cleanupError
    }
  }
}

async function main () {
  const files = fs.readdirSync(scenarioDir).filter(name => name.endsWith('.json')).sort()
  const scenarios = files.map(name => JSON.parse(fs.readFileSync(path.join(scenarioDir, name), 'utf8')))
    .filter(scenario => selected.size === 0 || selected.has(scenario.name))
  if (!scenarios.length) throw new Error('no matching E2E scenarios')
  process.stdout.write(`[run] slot=${runSlot} x_offset=${xOffset}\n`)
  const bot = await connectPlayer()
  const mcp = new McpStdioClient(path.join(root, 'target/debug/dustroute-mcp'), [], {
    cwd: root,
    timeoutMs,
    env: {
      ...process.env,
      DUSTROUTE_SERVER_ADDRESS: serverAddress,
      DUSTROUTE_ASSIST_PLAYER: playerName,
      DUSTROUTE_BOT_BRIDGE: process.env.DUSTROUTE_BOT_BRIDGE || '127.0.0.1:25580',
      DUSTROUTE_MCP_TOOL_PROFILE: 'debug',
      DUSTROUTE_READ_ONLY: 'false',
      DUSTROUTE_PREVIEW_REQUIRED: 'true'
    }
  })
  try {
    await mcp.initialize()
    for (const scenario of scenarios) await runScenario(bot, mcp, scenario)
  } finally {
    mcp.close()
    bot.quit('DustRoute E2E complete')
  }
}

main().catch(error => { console.error(error.stack || error); process.exitCode = 1 })
