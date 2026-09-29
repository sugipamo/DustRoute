'use strict'

function minecraftEndpoint (env) {
  const configured = env.DUSTROUTE_SERVER_ADDRESS
  if (!configured) {
    return {
      host: env.DUSTROUTE_MC_HOST || '127.0.0.1',
      port: Number(env.DUSTROUTE_MC_PORT || 25565)
    }
  }
  const separator = configured.lastIndexOf(':')
  if (separator <= 0) throw new Error('DUSTROUTE_SERVER_ADDRESS must be host:port')
  const host = configured.slice(0, separator).replace(/^\[|\]$/g, '')
  const port = Number(configured.slice(separator + 1))
  if (!host || !Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error('DUSTROUTE_SERVER_ADDRESS must be host:port')
  }
  return { host, port }
}

function bridgeConfig (env = process.env) {
  const minecraft = minecraftEndpoint(env)
  return {
    host: minecraft.host,
    port: minecraft.port,
    username: env.DUSTROUTE_BOT_NAME || 'DustRouteBot',
    version: env.DUSTROUTE_MC_VERSION || '1.21.11',
    auth: env.DUSTROUTE_MC_AUTH || 'offline',
    bridgeHost: '127.0.0.1',
    bridgePort: Number(env.DUSTROUTE_BRIDGE_PORT || 25580)
  }
}

module.exports = { bridgeConfig }
