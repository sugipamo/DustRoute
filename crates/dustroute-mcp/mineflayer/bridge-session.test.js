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
const { PROTOCOL } = require('./mutation-protocol')
const { Vec3 } = require('vec3')

test('player acquisition uses clear offset positions and keeps the player viewpoint', async () => {
  for (const mode of ['untracked', 'too_close', 'behind_blocked', 'already_clear', 'all_blocked']) {
    let bot
    const commands = []
    const player = { username: 'Builder', position: new Vec3(0.5, 80, 0.5), yaw: 0, pitch: 0, height: 1.8 }
    const session = createBotSession(bridgeConfig({}), createBridgeMetrics(), {
      createBot: options => {
        bot = new EventEmitter()
        Object.assign(bot, {
          _client: new EventEmitter(), username: options.username,
          game: { dimension: 'overworld' }, time: { age: 0 },
          entities: mode === 'untracked' ? {} : { player },
          entity: { position: mode === 'already_clear' ? new Vec3(5, 80, 0) : new Vec3(0.5, 80, 0.5) },
          waitForChunksToLoad: async () => {}, waitForTicks: async () => {}, quit: () => {},
          blockAt: () => null,
          chat: command => {
            commands.push(command)
            assert.match(command, /^\/execute at Builder rotated as Builder rotated ~ 0 positioned \^/)
            assert.match(command, /if block ~ ~ ~ minecraft:air if block ~ ~1 ~ minecraft:air run tp @s ~ ~ ~$/)
            if (mode === 'all_blocked' || (mode === 'behind_blocked' && commands.length === 1)) return
            bot.entities = { player }
            bot.entity.position = mode === 'behind_blocked' ? new Vec3(4.5, 82, 0.5) : new Vec3(0.5, 82, -3.5)
          }
        })
        return bot
      }, log: () => {}
    })
    session.connect()
    bot.emit('spawn')
    try {
      if (mode === 'all_blocked') {
        await assert.rejects(session.dispatch('approach_player', { player: 'Builder' }), /no clear offset position/)
        assert.equal(commands.length, 3)
        assert.deepEqual(bot.entity.position, player.position)
      } else {
        const observation = await session.dispatch('observe_player', { player: 'Builder', max_distance: 4 })
        assert.deepEqual(observation.eye_position, { x: 0.5, y: 81.62, z: 0.5 })
        assert.equal(observation.reacquired, mode === 'untracked')
        assert.equal(commands.length, mode === 'already_clear' ? 0 : mode === 'behind_blocked' ? 2 : 1)
        assert.ok(bot.entity.position.distanceTo(player.position) >= 3)
        if (mode !== 'already_clear') assert.match(commands[0], /positioned \^0 \^2 \^-4 /)
        if (mode === 'behind_blocked') assert.match(commands[1], /positioned \^4 \^2 \^0 /)
      }
    } finally { session.shutdown() }
  }
})

test('mutation failures expose a submitted prefix while invalid batches report no block effects', async () => {
  let bot
  let commands = 0
  const session = createBotSession(bridgeConfig({}), createBridgeMetrics(), {
    createBot: options => {
      bot = new EventEmitter()
      Object.assign(bot, {
        _client: new EventEmitter(), username: options.username,
        game: { dimension: 'overworld' }, time: { age: 0 },
        waitForChunksToLoad: async () => {}, waitForTicks: async () => {},
        chat: () => { if (++commands === 2) throw new Error('opaque transport failure') },
        quit: () => {}
      })
      return bot
    }, log: () => {}
  })
  session.connect()
  bot.emit('spawn')
  const changes = [0, 1].map(x => ({ pos: { x, y: 80, z: 0 }, state: 'minecraft:stone' }))
  try {
    await assert.rejects(session.dispatch('submit_command_batch', {
      protocol: PROTOCOL, dimension: 'minecraft:overworld', changes: [...changes, { bad: true }]
    }), error => {
      assert.equal(error.submissionFailure.cause.kind, 'invalid_input')
      assert.equal(error.submissionFailure.submitted_changes, 0)
      assert.equal(error.submissionFailure.may_have_changed_world, false)
      return true
    })
    assert.equal(commands, 0)
    await assert.rejects(session.dispatch('submit_command_batch', {
      protocol: PROTOCOL, dimension: 'minecraft:overworld', changes
    }), error => {
      assert.equal(error.submissionFailure.submitted_changes, 1)
      assert.equal(error.submissionFailure.total_changes, 2)
      assert.equal(error.submissionFailure.may_have_changed_world, true)
      assert.equal(error.submissionFailure.cause.kind, 'unknown')
      return true
    })
  } finally { session.shutdown() }
})

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
    if (method === 'partial') {
      const error = new Error('opaque partial failure')
      error.submissionFailure = { submitted_changes: 1, total_changes: 2, may_have_changed_world: true }
      throw error
    }
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
    const partial = await call(JSON.stringify({ id: 9, method: 'partial' }))
    assert.equal(partial.failure_protocol, 'dustroute.bridge-failure.v1')
    assert.equal(partial.submission_failure.submitted_changes, 1)
    assert.equal(partial.error, 'opaque partial failure')
  } finally {
    server.close()
    await once(server, 'close')
  }
})
