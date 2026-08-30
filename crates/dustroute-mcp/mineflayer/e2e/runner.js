'use strict'

const fs = require('node:fs')
const path = require('node:path')
const mineflayer = require('mineflayer')
const { Vec3 } = require('vec3')
const { McpStdioClient, assertExpectation, resolveReferences } = require('./runtime')

const root = path.resolve(__dirname, '../../../..')
const scenarioDir = path.join(__dirname, 'scenarios')
const playerName = process.env.DUSTROUTE_E2E_PLAYER || 'dustroutetest'
const serverAddress = process.env.DUSTROUTE_SERVER_ADDRESS || '127.0.0.1:25565'
const [host, portText] = serverAddress.split(':')
const port = Number(portText)
const version = process.env.DUSTROUTE_MC_VERSION || '1.21.11'
const selected = new Set(process.argv.slice(2))

if (!/^[A-Za-z0-9_]{1,16}$/.test(playerName)) throw new Error('DUSTROUTE_E2E_PLAYER must be a valid 1-16 character Minecraft name')

function connectPlayer () {
  return new Promise((resolve, reject) => {
    const bot = mineflayer.createBot({ host, port, username: playerName, auth: 'offline', version, hideErrors: false })
    const timer = setTimeout(() => reject(new Error('test player spawn timed out')), 30000)
    bot.once('spawn', async () => {
      try {
        await bot.waitForChunksToLoad()
        clearTimeout(timer)
        resolve(bot)
      } catch (error) { reject(error) }
    })
    bot.once('error', reject)
    bot.once('kicked', reason => reject(new Error(`test player kicked: ${String(reason)}`)))
  })
}

function sleep (milliseconds) {
  return new Promise(resolve => setTimeout(resolve, milliseconds))
}

async function command (bot, text) {
  bot.chat(text.replaceAll('${player}', playerName))
  await sleep(300)
}

async function aim (bot, mcp, step) {
  const footing = {
    x: Math.floor(step.position.x),
    y: Math.floor(step.position.y) - 1,
    z: Math.floor(step.position.z)
  }
  await command(bot, `/setblock ${footing.x} ${footing.y} ${footing.z} minecraft:barrier`)
  await command(bot, `/tp ${playerName} ${step.position.x} ${step.position.y} ${step.position.z}`)
  bot.creative.startFlying()
  await sleep(1000)
  await bot.waitForChunksToLoad()
  await bot.lookAt(new Vec3(step.target.x + 0.5, step.target.y + 0.5, step.target.z + 0.5), true)
  await sleep(500)
  let lastTarget = null
  let lastObservation = null
  for (let attempt = 0; attempt < 20; attempt++) {
    const gaze = await mcp.callTool('get_player_gaze', { player: playerName, max_distance: 64 })
    lastObservation = gaze.observation
    const target = gaze.observation && gaze.observation.targeted_block
    lastTarget = target
    if (target && target.x === step.target.x && target.y === step.target.y && target.z === step.target.z) return gaze
    await bot.lookAt(new Vec3(step.target.x + 0.5, step.target.y + 0.5, step.target.z + 0.5), true)
    await sleep(250)
  }
  throw new Error(`gaze did not settle on ${JSON.stringify(step.target)}; last observed ${JSON.stringify(lastTarget)} from ${JSON.stringify(lastObservation)}`)
}

async function runScenario (bot, mcp, scenario) {
  const results = {}
  process.stdout.write(`\n[scenario] ${scenario.name}\n`)
  for (const step of scenario.steps) {
    if (step.kind === 'command') {
      for (const text of step.commands) await command(bot, text)
    } else if (step.kind === 'aim') {
      results[step.save || 'gaze'] = await aim(bot, mcp, step)
    } else if (step.kind === 'mcp') {
      const args = resolveReferences(step.arguments || {}, results)
      results[step.save] = await mcp.callTool(step.tool, args)
      if (process.env.DUSTROUTE_E2E_VERBOSE === 'true') {
        process.stdout.write(`${JSON.stringify({ step: step.save, result: results[step.save] }, null, 2)}\n`)
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
}

async function main () {
  const files = fs.readdirSync(scenarioDir).filter(name => name.endsWith('.json')).sort()
  const scenarios = files.map(name => JSON.parse(fs.readFileSync(path.join(scenarioDir, name), 'utf8')))
    .filter(scenario => selected.size === 0 || selected.has(scenario.name))
  if (!scenarios.length) throw new Error('no matching E2E scenarios')
  const bot = await connectPlayer()
  const mcp = new McpStdioClient(path.join(root, 'target/debug/dustroute-mcp'), [], {
    cwd: root,
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
