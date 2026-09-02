'use strict'

// Keep the method cardinality bounded: method names arrive from an untrusted
// JSON client and must not be allowed to grow an unbounded metrics object.
const METRIC_METHODS = Object.freeze([
  'status',
  'visible_players',
  'approach_player',
  'observe_player',
  'scan_region',
  'get_block',
  'activate_lever',
  'approach_lever',
  'wait_ticks',
  'start_update_recording',
  'stop_update_recording',
  'preview_region',
  'write_blocks',
  'place_physical_blocks'
])

const KNOWN_METHODS = new Set(METRIC_METHODS)
const UNKNOWN_METHOD = 'unknown'
const MAX_COUNTER = Number.MAX_SAFE_INTEGER

function createBridgeMetrics () {
  return {
    requests_total: 0,
    errors_total: 0,
    request_bytes: 0,
    response_bytes: 0,
    total_duration_micros: 0,
    max_duration_micros: 0,
    scan_requests: 0,
    scan_volume_blocks: 0,
    scan_non_air_blocks: 0,
    requests_by_method: Object.fromEntries([
      ...METRIC_METHODS,
      UNKNOWN_METHOD
    ].map(method => [method, 0]))
  }
}

function addCounter (metrics, name, amount) {
  if (!Number.isSafeInteger(amount) || amount < 0) return
  metrics[name] = Math.min(MAX_COUNTER, metrics[name] + amount)
}

function normalizedMethod (method) {
  return typeof method === 'string' && KNOWN_METHODS.has(method) ? method : UNKNOWN_METHOD
}

function scanVolume (params) {
  if (!params || typeof params !== 'object') return 0
  const min = params.min || {}
  const max = params.max || {}
  const values = [min.x, min.y, min.z, max.x, max.y, max.z]
  if (!values.every(Number.isSafeInteger)) return 0
  const volume = (Math.abs(max.x - min.x) + 1) *
    (Math.abs(max.y - min.y) + 1) *
    (Math.abs(max.z - min.z) + 1)
  return Number.isSafeInteger(volume) ? volume : 0
}

function startRequest (metrics, { method, params, requestBytes }) {
  const key = normalizedMethod(method)
  addCounter(metrics, 'requests_total', 1)
  addCounter(metrics, 'request_bytes', requestBytes)
  addCounter(metrics.requests_by_method, key, 1)
  const requestedScanVolume = key === 'scan_region' ? scanVolume(params) : 0
  if (key === 'scan_region') {
    addCounter(metrics, 'scan_requests', 1)
    addCounter(metrics, 'scan_volume_blocks', requestedScanVolume)
  }
  return { method: key }
}

function finishRequest (metrics, context, response, responseBytes, durationMicros) {
  if (response && Object.prototype.hasOwnProperty.call(response, 'error')) {
    addCounter(metrics, 'errors_total', 1)
  }
  addCounter(metrics, 'response_bytes', responseBytes)
  addCounter(metrics, 'total_duration_micros', durationMicros)
  if (Number.isSafeInteger(durationMicros) && durationMicros > metrics.max_duration_micros) {
    metrics.max_duration_micros = durationMicros
  }
  if (context.method !== 'scan_region' || !response || !response.result ||
      !Array.isArray(response.result.blocks)) return
  addCounter(metrics, 'scan_non_air_blocks', response.result.blocks.length)
}

function snapshotMetrics (metrics) {
  return {
    ...metrics,
    requests_by_method: { ...metrics.requests_by_method }
  }
}

module.exports = {
  METRIC_METHODS,
  createBridgeMetrics,
  finishRequest,
  snapshotMetrics,
  startRequest
}
