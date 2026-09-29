'use strict'

const PROTOCOL = 'dustroute.bridge-mutation.v1'
const token = /^[a-z0-9_]+$/
const name = /^minecraft:[a-z0-9_]+$/
const position = p => p && Object.keys(p).sort().join(',') === 'x,y,z' &&
  [p.x, p.y, p.z].every(n => Number.isInteger(n) && n >= -2147483648 && n <= 2147483647)

function nativeState (state) {
  if (typeof state !== 'string') return false
  const [id, props, ...extra] = state.split('[')
  if (!name.test(id) || extra.length) return false
  if (props === undefined) return true
  if (!props.endsWith(']')) return false
  const seen = new Set()
  for (const entry of props.slice(0, -1).split(',')) {
    const [key, value, ...rest] = entry.split('=')
    if (!key || !value || !token.test(key) || !token.test(value) || rest.length || seen.has(key)) return false
    seen.add(key)
  }
  return true
}

// Validate the whole batch before any bot command, dig or placement. Rejected
// batches have no effects. Runtime failures can still follow partial effects.
function validateMutation (method, params) {
  if (params.protocol !== PROTOCOL) throw new Error('unsupported mutation protocol; update client and bridge together')
  const changes = params.changes
  const physical = method === 'submit_physical_batch'
  if (!['submit_command_batch', 'submit_physical_batch'].includes(method) ||
      !Array.isArray(changes) || changes.length > (physical ? 128 : 32768) ||
      (physical && !changes.length) || typeof params.dimension !== 'string') {
    throw new Error('invalid mutation batch')
  }
  for (const change of changes) {
    if (!change || !position(change.pos)) throw new Error('invalid mutation position')
    let fields
    if (!physical) {
      fields = ['pos', 'state']
      if (!nativeState(change.state)) throw new Error('invalid native block state')
    } else if (change.action === 'dig') {
      fields = ['action', 'pos']
    } else if (change.action === 'place') {
      fields = ['action', 'pos', 'state', 'item', 'reference', 'face']
      if (!nativeState(change.state) || typeof change.item !== 'string' || !name.test(change.item) ||
          !position(change.reference) || !position(change.face) ||
          Math.abs(change.face.x) + Math.abs(change.face.y) + Math.abs(change.face.z) !== 1) {
        throw new Error('invalid physical placement')
      }
    } else throw new Error('invalid physical action')
    if (Object.keys(change).sort().join(',') !== fields.sort().join(',')) throw new Error('unexpected mutation fields')
  }
  return changes
}

module.exports = { PROTOCOL, nativeState, validateMutation }
