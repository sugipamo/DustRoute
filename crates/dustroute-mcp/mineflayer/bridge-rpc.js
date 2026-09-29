'use strict'

const net = require('node:net')
const { startRequest, finishRequest, snapshotMetrics } = require('./metrics')

// Owns framing and metrics only. Dispatch and socket binding are supplied.
function createBridgeServer (dispatch, bridgeMetrics) {
  return net.createServer(socket => {
    socket.setEncoding('utf8')
    let buffer = ''
    socket.on('data', chunk => {
      buffer += chunk
      const newline = buffer.indexOf('\n')
      if (newline < 0) return
      const line = buffer.slice(0, newline)
      buffer = buffer.slice(newline + 1)
      const startedAt = process.hrtime.bigint()
      const requestBytes = Buffer.byteLength(line, 'utf8')
      let request = null
      let parseError = null
      try {
        request = JSON.parse(line)
      } catch (error) {
        parseError = error
      }
      const context = startRequest(bridgeMetrics, {
        method: request && request.method,
        params: request && request.params,
        requestBytes
      })
      Promise.resolve()
        .then(() => {
          if (parseError) throw parseError
          return dispatch(request && request.method, (request && request.params) || {})
            .then(result => ({ id: request && request.id, result }))
            .catch(error => ({ id: request && request.id, error: String(error.message || error) }))
        })
        .then(response => {
          // Include the current request in the counters returned by status. Its
          // duration and response size are finalized immediately afterwards.
          if (context.method === 'status' && response.result && typeof response.result === 'object') {
            response.result.metrics = snapshotMetrics(bridgeMetrics)
          }
          const serialized = `${JSON.stringify(response)}\n`
          finishRequest(
            bridgeMetrics,
            context,
            response,
            Buffer.byteLength(serialized.slice(0, -1), 'utf8'),
            Number((process.hrtime.bigint() - startedAt) / 1000n)
          )
          socket.end(serialized)
        })
        .catch(error => {
          const response = { error: String(error.message || error) }
          const serialized = `${JSON.stringify(response)}\n`
          finishRequest(
            bridgeMetrics,
            context,
            response,
            Buffer.byteLength(serialized.slice(0, -1), 'utf8'),
            Number((process.hrtime.bigint() - startedAt) / 1000n)
          )
          socket.end(serialized)
        })
    })
  })
}

module.exports = { createBridgeServer }
