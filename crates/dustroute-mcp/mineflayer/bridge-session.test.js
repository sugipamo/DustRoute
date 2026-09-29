'use strict'

const test = require('node:test')
const assert = require('node:assert/strict')
const { EventEmitter } = require('node:events')
const net = require('node:net')
const { once } = require('node:events')
const { bridgeConfig } = require('./bridge-config')
const { createBotSession } = require('./bot-session')
const { createBridgeServer } = require('./bridge-rpc')
const { createBridgeMetrics } = require('./metrics')

test('importing the entry point neither reads configuration nor opens sockets', () => {
  const previous = process.env.DUSTROUTE_SERVER_ADDRESS
  process.env.DUSTROUTE_SERVER_ADDRESS = 'invalid'
  const original = net.createServer
  net.createServer = () => { throw new Error('unexpected socket creation') }
  try {
    assert.equal(typeof require('./bridge').startBridge, 'function')
  } finally {
    net.createServer = original
    if (previous === undefined) delete process.env.DUSTROUTE_SERVER_ADDRESS
    else process.env.DUSTROUTE_SERVER_ADDRESS = previous
  }
})

test('sessions own independent clocks, feature flags and reconnect timers', async () => {
  const bots = []
  const timers = []
  const cancelled = []
  const config = bridgeConfig({})
  const dependencies = {
    createBot: options => {
      const bot = new EventEmitter()
      Object.assign(bot, {
        _client: new EventEmitter(), username: options.username,
        game: { dimension: 'overworld' }, entity: { position: { x: 0, y: 64, z: 0 } },
        time: { age: 100 }, waitForChunksToLoad: async () => {},
        waitForTicks: async () => {}, quit: () => { bot.emit('end', 'closed') }
      })
      bots.push(bot)
      return bot
    },
    schedule: callback => { timers.push(callback); return timers.length },
    cancel: timer => cancelled.push(timer), log: () => {}
  }
  const a = createBotSession(config, createBridgeMetrics(), dependencies)
  const b = createBotSession(config, createBridgeMetrics(), dependencies)
  assert.equal(bots.length, 0)
  a.connect()
  bots[0].emit('spawn')
  bots[0]._client.emit('feature_flags', { features: ['minecraft:vanilla'] })
  bots[0].emit('physicsTick')
  assert.equal((await a.dispatch('status', {})).connected, true)
  assert.equal((await b.dispatch('status', {})).connected, false)
  assert.deepEqual(await a.dispatch('wait_ticks', { dimension: 'minecraft:overworld', ticks: 1 }), { waited_ticks: 1, game_tick: 101 })
  await assert.rejects(a.dispatch('wait_ticks', { dimension: 'minecraft:overworld', ticks: 201 }), /ticks/)
  bots[0].emit('end', 'lost')
  assert.equal((await a.dispatch('status', {})).enabled_features, null)
  assert.equal(timers.length, 1)
  a.shutdown()
  assert.deepEqual(cancelled, [1])
  timers[0]()
  assert.equal(bots.length, 1)
  b.shutdown()
})

test('RPC framing preserves request IDs, errors and per-session metrics', async () => {
  const metrics = createBridgeMetrics()
  const server = createBridgeServer(async (method, params) => {
    if (method === 'status') return { connected: false, echo: params.value }
    throw new Error('unsupported')
  }, metrics)
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  async function call (line) {
    const socket = net.connect(server.address().port, '127.0.0.1')
    socket.setEncoding('utf8')
    let result = ''
    socket.on('data', data => { result += data })
    socket.write(line + '\n')
    await once(socket, 'end')
    return JSON.parse(result)
  }
  try {
    const response = await call(JSON.stringify({ id: 7, method: 'status', params: { value: 3 } }))
    assert.equal(response.id, 7)
    assert.equal(response.result.echo, 3)
    assert.equal(response.result.metrics.requests_total, 1)
    assert.deepEqual(await call(JSON.stringify({ id: 8, method: 'missing' })), { id: 8, error: 'unsupported' })
    assert.equal(typeof (await call('{')).error, 'string')
    assert.equal(metrics.errors_total, 2)
  } finally {
    server.close()
    await once(server, 'close')
  }
})
