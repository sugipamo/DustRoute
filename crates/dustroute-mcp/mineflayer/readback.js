'use strict'

// Block-state confirmation, independent of the simulator. Commands inspect the
// world; only an owned scratch-storage key is written, then removed. No block
// writes, forced neighbor updates, chunk reloads or tick freezing are used.
const crypto = require('node:crypto')
const MAX_CELLS = 262144
const MAX_CONFIRM_CELLS = 8880
// Keep the complete supported region in a single predicate command. Air
// compression makes sparse construction regions fit; fragmented/dense regions
// that exceed the native command-length budget are refused before sending.
const BATCH_CELLS = MAX_CONFIRM_CELLS
const SCHEMA = 'dustroute.server-readback.v1'
const AXES = ['x', 'y', 'z']
const STAIRS = ['straight', 'inner_left', 'inner_right', 'outer_left', 'outer_right']

function bounds (min, max) {
  if (!min || !max || AXES.some(a => !Number.isSafeInteger(min[a]) || !Number.isSafeInteger(max[a]) || min[a] > max[a] || Math.abs(min[a]) > 30000000 || Math.abs(max[a]) > 30000000)) throw new Error('invalid readback bounds')
  const volume = AXES.reduce((n, a) => n * (max[a] - min[a] + 1), 1)
  if (volume > MAX_CELLS) throw new Error(`server readback exceeds ${MAX_CELLS} cells`)
  if (volume > MAX_CONFIRM_CELLS) throw new Error(`server readback exceeds the ${MAX_CONFIRM_CELLS}-cell confirmation limit; select a smaller region`)
  const batches = Math.ceil(volume / BATCH_CELLS)
  if (batches + Math.ceil(batches / 48) + 3 > 192) throw new Error('server readback exceeds the 192-command confirmation budget; select a smaller region')
  return volume
}
const key = p => AXES.map(a => p[a]).join(',')
const coords = p => AXES.map(a => p[a]).join(' ')
const inside = (p, min, max) => AXES.every(a => Number.isSafeInteger(p[a]) && p[a] >= min[a] && p[a] <= max[a])

function literal (block, registry) {
  if (!/^minecraft:[a-z0-9_]+$/.test(block.name)) throw new Error('invalid readback block identity')
  const definition = registry.blocksByName[block.name.slice(10)]
  const properties = block.properties
  const states = definition && (definition.states || [])
  if (!states || !properties || Object.keys(properties).length !== states.length) throw new Error(`incomplete native state: ${block.name}`)
  for (const state of states) {
    const value = properties[state.name]
    const valid = typeof value === 'string' && (state.type === 'bool'
      ? ['true', 'false'].includes(value)
      : state.values ? state.values.map(String).includes(value) : /^(0|[1-9][0-9]*)$/.test(value) && Number(value) < state.num_values)
    if (!valid) throw new Error(`invalid native property ${block.name}.${state.name}`)
  }
  const entries = Object.entries(properties).sort(([a], [b]) => a.localeCompare(b))
  return block.name + (entries.length ? `[${entries.map(([k, v]) => `${k}=${v}`).join(',')}]` : '')
}

function cellsOf (snapshot, registry) {
  bounds(snapshot.min, snapshot.max)
  const indexed = new Map()
  for (const b of snapshot.blocks) {
    if (!inside(b.pos, snapshot.min, snapshot.max) || indexed.has(key(b.pos))) throw new Error('duplicate or out-of-bounds readback cell')
    literal(b, registry)
    indexed.set(key(b.pos), structuredClone(b))
  }
  const cells = []
  for (let x = snapshot.min.x; x <= snapshot.max.x; x++) {
    for (let y = snapshot.min.y; y <= snapshot.max.y; y++) {
      for (let z = snapshot.min.z; z <= snapshot.max.z; z++) {
        const pos = { x, y, z }
        cells.push(indexed.get(key(pos)) || { pos, name: 'minecraft:air', properties: {} })
      }
    }
  }
  return cells
}

// Prove a uniform Air cuboid from one directly checked cell. Each comparison
// extends the previously proved prefix, doubling along one axis at a time.
// All clauses belong to ONE execute command, so no other task can mutate a
// previously checked prefix between these comparisons. No reference-world
// writes, external empty area or client-only assumption is needed.
function airConditions (min, max) {
  const clauses = [`if block ${coords(min)} minecraft:air`]
  const covered = { ...min }
  for (const axis of ['z', 'y', 'x']) {
    while (covered[axis] < max[axis]) {
      const length = Math.min(covered[axis] - min[axis] + 1, max[axis] - covered[axis])
      const sourceMax = { ...covered, [axis]: min[axis] + length - 1 }
      const destination = { ...min, [axis]: covered[axis] + 1 }
      clauses.push(`if blocks ${coords(min)} ${coords(sourceMax)} ${coords(destination)} all`)
      covered[axis] += length
    }
  }
  return clauses
}

function stateConditions (batch, registry) {
  const air = new Set(batch.filter(b => b.name === 'minecraft:air').map(b => key(b.pos)))
  const clauses = []
  for (const b of batch) {
    if (b.name !== 'minecraft:air') {
      clauses.push(`if block ${coords(b.pos)} ${literal(b, registry)}`)
      continue
    }
    if (!air.has(key(b.pos))) continue
    const min = b.pos; const max = { ...min }
    while (air.has(key({ ...max, z: max.z + 1 }))) max.z++
    const row = (x, y) => {
      for (let z = min.z; z <= max.z; z++) if (!air.has(key({ x, y, z }))) return false
      return true
    }
    while (row(min.x, max.y + 1)) max.y++
    const plane = x => {
      for (let y = min.y; y <= max.y; y++) if (!row(x, y)) return false
      return true
    }
    while (plane(max.x + 1)) max.x++
    for (let x = min.x; x <= max.x; x++) for (let y = min.y; y <= max.y; y++) for (let z = min.z; z <= max.z; z++) air.delete(key({ x, y, z }))
    clauses.push(...airConditions(min, max))
  }
  return clauses.join(' ')
}

function commandScope (nonce, recipient) {
  const prefix = `dustroute_readback:${nonce}:`
  const storage = 'dustroute:readback'
  const path = `r${nonce}`
  const defaults = {}
  return {
    prefix, storage, path, defaults,
    say: text => `tellraw ${recipient} ${JSON.stringify({ text: prefix + text })}`,
    check: (condition, index) => {
      // A failed execute condition need not invoke its result consumer. Seed
      // it as unconfirmed; only a successful native query can set m to 1.
      defaults[`m${index}`] = 0
      defaults[`t${index}`] = 0
      return `execute store success storage ${storage} ${path}.m${index} byte 1 store result storage ${storage} ${path}.t${index} long 1 run execute ${condition} run time query gametime`
    },
    reports: count => {
      const commands = []
      for (let i = 0; i < count; i += 48) {
        const text = [{ text: prefix }]
        for (let j = i; j < Math.min(i + 48, count); j++) text.push(
          { text: `${j === i ? '' : '|'}check:${j}:` }, { nbt: `${path}.m${j}`, storage },
          { text: ':' }, { nbt: `${path}.t${j}`, storage })
        commands.push(`tellraw ${recipient} ${JSON.stringify(text)}`)
      }
      return commands
    }
  }
}

// A nonce and a private system-chat reply fence associate every response with
// this batch. A permission error, disconnect or missing fence never succeeds.
async function exchange (bot, makeCommands, timeoutMs = 2500) {
  const nonce = crypto.randomBytes(16).toString('hex')
  const scope = commandScope(nonce, bot.username)
  const { prefix, storage, path, say } = scope
  const replies = []
  if (!/^[A-Za-z0-9_]{1,16}$/.test(bot.username)) throw new Error('invalid readback recipient')
  const body = makeCommands(say, scope)
  const commands = [`data modify storage ${storage} ${path} set value ${JSON.stringify(scope.defaults)}`, ...body, `data remove storage ${storage} ${path}`, say('done')]
  if (commands.length > 192) throw new Error('server readback exceeds the 192-command confirmation budget; select a smaller region')
  if (commands.some(c => c.length > 30000)) throw new Error('readback command limit exceeded')
  return new Promise((resolve, reject) => {
    const finish = (error) => {
      clearTimeout(timer)
      bot.removeListener('messagestr', message)
      bot.removeListener('end', disconnected)
      if (error) reject(error)
      else resolve({ nonce, replies })
    }
    const disconnected = () => finish(new Error('server readback disconnected'))
    const message = (text, position) => {
      if (position !== 'system' || !text.startsWith(prefix)) return
      const payload = text.slice(prefix.length)
      if (payload === 'done') finish()
      else replies.push(...payload.split('|'))
    }
    const timer = setTimeout(() => finish(new Error('server readback response missing (permission, timeout or command failure)')), timeoutMs)
    bot.on('messagestr', message)
    bot.once('end', disconnected)
    try { for (const command of commands) bot.chat('/' + command) } catch (error) { finish(error) }
  })
}

function checks (replies, count) {
  const entries = new Map()
  for (const row of replies) {
    const match = /^check:([0-9]+):([01])b?:([0-9]+)L?$/.exec(row)
    if (!match) throw new Error('server predicate/tick evidence missing')
    const index = Number(match[1]); const tick = Number(match[3])
    if (index >= count || entries.has(index) || !Number.isSafeInteger(tick)) throw new Error('ambiguous server predicate evidence')
    entries.set(index, { index, matched: match[2] === '1', tick })
  }
  if (entries.size !== count) throw new Error('server predicate/tick evidence missing')
  return [...entries.values()].sort((a, b) => a.index - b.index)
}

async function confirmRegion (bot, snapshot, dimension, options = {}) {
  if (bot.version !== '1.21.11' || !/^minecraft:[a-z0-9_]+$/.test(dimension)) throw new Error('unsupported server readback target')
  const requestId = options.requestId || crypto.randomUUID()
  const cells = cellsOf(snapshot, bot.registry)
  const clientStairs = new Map(cells.filter(b => b.name.endsWith('_stairs')).map(b => [key(b.pos), structuredClone(b)]))
  const attempts = []
  const run = options.exchange || exchange
  for (let attempt = 0; attempt < 3; attempt++) {
    // A bounded pause shifts retries away from the preceding reply boundary,
    // including after disconnects. Native readings alone decide acceptance.
    if (attempt) await new Promise(resolve => setTimeout(resolve, 25))
    const batches = []
    for (let i = 0; i < cells.length; i += BATCH_CELLS) batches.push(cells.slice(i, i + BATCH_CELLS))
    const result = await run(bot, (say, scope) => [...batches.map((batch, i) => {
      const chunks = new Map(batch.map(b => [`${Math.floor(b.pos.x / 16)},${Math.floor(b.pos.z / 16)}`, b.pos]))
      const loaded = [...chunks.values()].map(p => `if loaded ${coords(p)}`).join(' ')
      const matches = stateConditions(batch, bot.registry)
      return scope.check(`in ${dimension} ${loaded} ${matches}`, i)
    }), ...scope.reports(batches.length)])
    // The time is obtained by the successful predicate command itself. Network
    // delivery of a preceding/following clock reply cannot enlarge this interval.
    const observations = checks(result.replies, batches.length)
    const matched = new Set(observations.filter(r => r.matched).map(r => r.index))
    const ticks = observations.filter(r => r.matched).map(r => r.tick)
    const start = ticks.length ? Math.min(...ticks) : null
    const end = ticks.length ? Math.max(...ticks) : null
    attempts.push({ start_game_tick: start, end_game_tick: end, matched_batches: matched.size, batch_count: batches.length,
      unconfirmed_batches: batches.flatMap((_, i) => matched.has(i) ? [] : [i]) })
    if (start === end && matched.size === batches.length && batches.every((_, i) => matched.has(i))) {
      const confirmed = { min: snapshot.min, max: snapshot.max, blocks: cells.filter(b => b.name !== 'minecraft:air') }
      // Report differences from the original client observation to this final
      // confirmed snapshot, not intermediate guesses selected during retries.
      const corrections = cells.flatMap(b => {
        const client = clientStairs.get(key(b.pos))
        return client && client.properties.shape !== b.properties.shape
          ? [{ pos: b.pos, client, confirmed_shape: b.properties.shape }] : []
      })
      return { ...confirmed, readback: {
        schema_version: SCHEMA, kind: 'server_confirmed', request_id: requestId,
        dimension, min: snapshot.min, max: snapshot.max, checked_cells: cells.length,
        start_game_tick: start, end_game_tick: end, nonce: result.nonce,
        snapshot_sha256: crypto.createHash('sha256').update(JSON.stringify(confirmed)).digest('hex'),
        corrections, attempts, predicate_ticks: ticks, predicate_batch_cells: BATCH_CELLS, hidden_runtime_observed: false
      } }
    }
    if (matched.size === batches.length) continue // crossed tick: confirm the entire region again
    // Only enumerate the native stair shape property. No simulated world or
    // expected construction result is accepted as a substitute for observation.
    const stairs = batches.flatMap((b, i) => matched.has(i) ? [] : b).filter(b => b.name.endsWith('_stairs') && STAIRS.includes(b.properties.shape))
    if (!stairs.length || stairs.length > 32) throw new Error('server readback differs from client or region is unloaded')
    const probes = await run(bot, say => stairs.flatMap((b, i) => STAIRS.map(shape =>
      `execute in ${dimension} if loaded ${coords(b.pos)} if block ${coords(b.pos)} ${literal({ ...b, properties: { ...b.properties, shape } }, bot.registry)} run ${say(`shape:${i}:${shape}`)}`)))
    for (const [i, b] of stairs.entries()) {
      const choices = probes.replies.filter(r => r.startsWith(`shape:${i}:`)).map(r => r.split(':')[2])
      if (choices.length !== 1 || !STAIRS.includes(choices[0])) throw new Error('server stair state unavailable or changed during readback')
      if (b.properties.shape !== choices[0]) {
        b.properties.shape = choices[0]
      }
    }
  }
  throw new Error('server readback did not confirm a complete region within one game tick: ' + JSON.stringify(attempts))
}

module.exports = { confirmRegion, exchange, commandScope, checks, cellsOf, literal, bounds, stateConditions, SCHEMA }
