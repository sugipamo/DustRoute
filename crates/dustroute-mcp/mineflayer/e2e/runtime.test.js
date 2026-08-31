'use strict'

const test = require('node:test')
const assert = require('node:assert/strict')
const { assertExpectation, getPath, resolveReferences } = require('./runtime')

test('resolves operation IDs from prior MCP results', () => {
  const results = { analysis: { repair_proposals: [{ operation_id: 'abc' }] } }
  assert.deepEqual(
    resolveReferences({ operation_id: '${analysis.repair_proposals.0.operation_id}', confirm: true }, results),
    { operation_id: 'abc', confirm: true }
  )
})

test('evaluates equality and numeric bounds', () => {
  const value = { diagnostic: { health: 'degraded', counts: { probable_faults: 1 } } }
  assert.equal(getPath(value, 'diagnostic.health'), 'degraded')
  assert.doesNotThrow(() => assertExpectation(value, { path: 'diagnostic.counts.probable_faults', at_least: 1 }))
  assert.throws(() => assertExpectation(value, { path: 'diagnostic.health', equals: 'healthy' }))
})
