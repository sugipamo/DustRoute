'use strict'
const test = require('node:test')
const assert = require('node:assert/strict')
const { EventEmitter } = require('node:events')
const { validateMutation, nativeState } = require('./mutation-protocol')
const { createBotSession } = require('./bot-session')
const { createBridgeMetrics } = require('./metrics')
const { bridgeConfig } = require('./bridge-config')
const fixture = require('../fixtures/mutation-contract.json')

test('Rust and JS share current mutation records and strict native states', () => {
  validateMutation('submit_command_batch', fixture.command)
  validateMutation('submit_physical_batch', fixture.physical)
  for (const state of fixture.invalid_states) assert.equal(nativeState(state), false)
  assert.throws(() => validateMutation('submit_command_batch', { ...fixture.command, protocol: undefined }), /protocol/)
})

test('invalid suffixes have no effects; interrupted writes never return a success receipt', async () => {
  const commands = []
  const bot = new EventEmitter()
  Object.assign(bot, {
    _client: new EventEmitter(), username: 'Test', time: { age: 0 },
    game: { dimension: 'overworld' }, waitForChunksToLoad: async () => {},
    waitForTicks: async () => {}, quit: () => {}, chat: command => { commands.push(command) }
  })
  const session = createBotSession(bridgeConfig({}), createBridgeMetrics(), { createBot: () => bot, log: () => {} })
  session.connect()
  bot.emit('spawn')
  const bad = { ...fixture.command, changes: [...fixture.command.changes, { pos: { x: 0, y: 0, z: 0 }, state: 'invalid' }] }
  await assert.rejects(session.dispatch('write_blocks', fixture.command))
  await assert.rejects(session.dispatch('place_physical_blocks', fixture.physical))
  await assert.rejects(session.dispatch('submit_command_batch', bad))
  assert.equal(commands.length, 0)
  assert.deepEqual(await session.dispatch('submit_command_batch', fixture.command), fixture.command_receipt)
  assert.equal(commands.length, 1)
  bot.waitForTicks = async () => { throw new Error('connection lost after submission') }
  await assert.rejects(session.dispatch('submit_command_batch', fixture.command), /connection lost/)
  assert.equal(commands.length, 2)
  session.shutdown()
})
