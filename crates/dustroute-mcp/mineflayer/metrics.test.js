'use strict'

const test = require('node:test')
const assert = require('node:assert/strict')
const {
  createBridgeMetrics,
  finishRequest,
  snapshotMetrics,
  startRequest
} = require('./metrics')

test('records scan request size, duration, and returned block count', () => {
  const metrics = createBridgeMetrics()
  const context = startRequest(metrics, {
    method: 'scan_region',
    params: {
      min: { x: 0, y: 64, z: 0 },
      max: { x: 1, y: 65, z: 2 }
    },
    requestBytes: 123
  })
  finishRequest(metrics, context, {
    id: 1,
    result: { blocks: [{}, {}, {}] }
  }, 456, 789)

  assert.equal(metrics.requests_total, 1)
  assert.equal(metrics.request_bytes, 123)
  assert.equal(metrics.response_bytes, 456)
  assert.equal(metrics.total_duration_micros, 789)
  assert.equal(metrics.max_duration_micros, 789)
  assert.equal(metrics.scan_requests, 1)
  assert.equal(metrics.scan_volume_blocks, 12)
  assert.equal(metrics.scan_non_air_blocks, 3)
  assert.equal(metrics.requests_by_method.scan_region, 1)
})

test('bounds method labels and counts protocol failures', () => {
  const metrics = createBridgeMetrics()
  const context = startRequest(metrics, {
    method: '__proto__',
    params: {},
    requestBytes: 7
  })
  finishRequest(metrics, context, { id: 2, error: 'unknown method' }, 31, 4)

  assert.equal(metrics.requests_by_method.unknown, 1)
  assert.equal(metrics.errors_total, 1)
  assert.equal(Object.keys(metrics.requests_by_method).length, 15)
  assert.deepEqual(snapshotMetrics(metrics).requests_by_method, metrics.requests_by_method)
})
