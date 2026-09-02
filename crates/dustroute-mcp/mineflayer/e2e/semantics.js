'use strict'

const mineflayer = require('mineflayer')

const serverAddress = process.env.DUSTROUTE_SERVER_ADDRESS || '127.0.0.1:25565'
const [host, portText] = serverAddress.split(':')
const port = Number(portText)
const version = process.env.DUSTROUTE_MC_VERSION || '1.21.11'
const playerName = process.env.DUSTROUTE_E2E_PLAYER || 'dustroutetest'
const functionName = process.env.DUSTROUTE_E2E_SEMANTICS_FUNCTION || 'ro_sem:tests'
const expectedPasses = Number(process.env.DUSTROUTE_E2E_SEMANTICS_ASSERTIONS || 23)
const timeoutMs = Number(process.env.DUSTROUTE_E2E_TIMEOUT_MS || 120000)

function sleep (milliseconds) { return new Promise(resolve => setTimeout(resolve, milliseconds)) }

if (!Number.isInteger(expectedPasses) || expectedPasses < 1) throw new Error('DUSTROUTE_E2E_SEMANTICS_ASSERTIONS must be positive')

async function main () {
  const bot = mineflayer.createBot({ host, port, username: playerName, auth: 'offline', version, hideErrors: false })
  const lines = []
  const observedLines = []
  bot.on('message', message => {
    const line = String(message).replace(/§./g, '').trim()
    observedLines.push(line)
    if (line.startsWith('PASS ') || line.startsWith('FAIL ') || line === 'DUSTROUTE COMPLETE') lines.push(line)
  })
  try {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error('semantic test player spawn timed out')), 30000)
      bot.once('spawn', () => { clearTimeout(timer); resolve() })
      bot.once('error', reject)
      bot.once('kicked', reason => reject(new Error(`semantic test player kicked: ${String(reason)}`)))
    })
    bot.chat(`/gamemode creative ${playerName}`)
    bot.chat(`/tp ${playerName} 0.5 110 0.5`)
    await sleep(1000)
    // The semantic pack executes server-side and reports through chat; it
    // does not inspect client block state, so waiting for Mineflayer's full
    // 25-chunk view would make this check fail on a cold/offline world.
    await sleep(500)
    bot.chat(`/function ${functionName}`)
    const deadline = Date.now() + timeoutMs
    while (!lines.includes('DUSTROUTE COMPLETE') && Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 100))
    }
    const failures = lines.filter(line => line.startsWith('FAIL '))
    const passes = lines.filter(line => line.startsWith('PASS '))
    if (!lines.includes('DUSTROUTE COMPLETE')) throw new Error(`semantic test did not complete; captured ${JSON.stringify(observedLines)}`)
    if (failures.length) throw new Error(`semantic assertions failed: ${JSON.stringify(failures)}`)
    if (passes.length !== expectedPasses) throw new Error(`expected ${expectedPasses} PASS messages, captured ${passes.length}`)
    process.stdout.write(`[pass] semantic Data Pack: ${passes.length} assertions\n`)
  } finally {
    bot.quit('DustRoute semantic E2E complete')
  }
}

main().catch(error => { console.error(error.stack || error); process.exitCode = 1 })
