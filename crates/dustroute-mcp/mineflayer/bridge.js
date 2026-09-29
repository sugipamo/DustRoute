'use strict'

const { bridgeConfig } = require('./bridge-config')
const { createBotSession } = require('./bot-session')
const { createBridgeServer } = require('./bridge-rpc')
const { createBridgeMetrics } = require('./metrics')

function startBridge (config = bridgeConfig()) {
  const metrics = createBridgeMetrics()
  const session = createBotSession(config, metrics)
  const server = createBridgeServer(session.dispatch, metrics)
  session.connect()
  server.listen(config.bridgePort, config.bridgeHost, () => {
    process.stderr.write(`[dustroute-bot] bridge listening on ${config.bridgeHost}:${config.bridgePort}\n`)
  })
  function shutdown () {
    server.close()
    session.shutdown()
  }
  return { server, session, shutdown }
}

if (require.main === module) {
  const { shutdown } = startBridge()
  process.once('SIGINT', shutdown)
  process.once('SIGTERM', shutdown)
}

module.exports = { startBridge }
