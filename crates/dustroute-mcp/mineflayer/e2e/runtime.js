'use strict'

const { spawn } = require('node:child_process')
const readline = require('node:readline')

function getPath (value, path) {
  return String(path).split('.').filter(Boolean).reduce((current, part) => {
    if (current == null || !(part in Object(current))) throw new Error(`missing result path ${path}`)
    return current[part]
  }, value)
}

function resolveReferences (value, results) {
  if (Array.isArray(value)) return value.map(item => resolveReferences(item, results))
  if (value && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, resolveReferences(item, results)]))
  }
  if (typeof value !== 'string') return value
  const exact = value.match(/^\$\{([^}]+)\}$/)
  if (exact) {
    const [name, ...path] = exact[1].split('.')
    return getPath(results[name], path.join('.'))
  }
  return value.replace(/\$\{([^}]+)\}/g, (_, expression) => {
    const [name, ...path] = expression.split('.')
    return String(getPath(results[name], path.join('.')))
  })
}

function assertExpectation (actual, expectation) {
  const value = getPath(actual, expectation.path)
  if ('equals' in expectation && value !== expectation.equals) {
    throw new Error(`${expectation.path}: expected ${JSON.stringify(expectation.equals)}, got ${JSON.stringify(value)}`)
  }
  if ('at_least' in expectation && !(value >= expectation.at_least)) {
    throw new Error(`${expectation.path}: expected >= ${expectation.at_least}, got ${value}`)
  }
  if ('at_most' in expectation && !(value <= expectation.at_most)) {
    throw new Error(`${expectation.path}: expected <= ${expectation.at_most}, got ${value}`)
  }
  if ('exists' in expectation && (value !== undefined && value !== null) !== expectation.exists) {
    throw new Error(`${expectation.path}: existence did not equal ${expectation.exists}`)
  }
}

class McpStdioClient {
  constructor (command, args, options = {}) {
    this.nextId = 1
    this.pending = new Map()
    this.process = spawn(command, args, { ...options, stdio: ['pipe', 'pipe', 'pipe'] })
    this.stderr = ''
    this.timeoutMs = Number(options.timeoutMs || 120000)
    this.process.stderr.on('data', chunk => { this.stderr = (this.stderr + chunk).slice(-8192) })
    readline.createInterface({ input: this.process.stdout }).on('line', line => {
      let message
      try { message = JSON.parse(line) } catch { return }
      const pending = this.pending.get(message.id)
      if (!pending) return
      this.pending.delete(message.id)
      if (message.error) pending.reject(new Error(JSON.stringify(message.error)))
      else pending.resolve(message.result)
    })
    this.process.once('exit', code => {
      const error = new Error(`MCP process exited with ${code}: ${this.stderr}`)
      for (const pending of this.pending.values()) pending.reject(error)
      this.pending.clear()
    })
  }

  request (method, params = {}, timeoutMs = this.timeoutMs) {
    const id = this.nextId++
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`MCP ${method} timed out after ${timeoutMs}ms`))
      }, timeoutMs)
      this.pending.set(id, {
        resolve: value => { clearTimeout(timer); resolve(value) },
        reject: error => { clearTimeout(timer); reject(error) }
      })
      this.process.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`)
    })
  }

  async initialize () {
    await this.request('initialize', {
      protocolVersion: '2025-03-26', capabilities: {},
      clientInfo: { name: 'dustroute-e2e', version: '0.2.0' }
    })
    this.process.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`)
  }

  async callTool (name, args = {}, timeoutMs = this.timeoutMs) {
    const parsed = await this.callToolRaw(name, args, timeoutMs)
    if (parsed.ok === false) throw new Error(`${name}: ${parsed.error || JSON.stringify(parsed)}`)
    return parsed
  }

  async callToolRaw (name, args = {}, timeoutMs = this.timeoutMs) {
    const result = await this.request('tools/call', { name, arguments: args }, timeoutMs)
    const text = result.content.find(item => item.type === 'text')
    if (!text) throw new Error(`${name} returned no text content`)
    return JSON.parse(text.text)
  }

  close () {
    this.process.stdin.end()
    this.process.kill('SIGTERM')
  }
}

module.exports = { McpStdioClient, assertExpectation, getPath, resolveReferences }
