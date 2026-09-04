'use strict'

// Promote one ignored activate_trace artifact into a reviewed, relative-tick
// scheduler observation.  The packet source is deliberately kept separate
// from the modelled SchedulerProfile: Mineflayer cannot reveal Vanilla's
// internal phase or causal queue.
const fs = require('node:fs')
const path = require('node:path')

const root = path.resolve(__dirname, '../../../..')
const [sourceArg, resultName, fixtureName, ...reasonParts] = process.argv.slice(2)
const reason = reasonParts.join(' ').trim()
if (!sourceArg || !resultName || !fixtureName || !reason) {
  throw new Error('usage: node e2e/promote-scheduler-observation.js <artifact.json> <result> <fixture-name> <reason>')
}
if (!/^[a-z0-9][a-z0-9_-]*$/.test(fixtureName)) {
  throw new Error('fixture-name must contain only lowercase letters, digits, _ and -')
}

const source = path.resolve(sourceArg)
const artifact = JSON.parse(fs.readFileSync(source, 'utf8'))
const trace = artifact.results && artifact.results[resultName]
  ? artifact.results[resultName]
  : artifact
if (trace.source !== 'minecraft_java' || !Array.isArray(trace.events) || trace.events.length === 0) {
  throw new Error('source must contain a non-empty minecraft_java activate_trace result')
}
if (!trace.activation || !Number.isInteger(trace.activation.game_tick)) {
  throw new Error('activate_trace must include an integer activation.game_tick')
}

const destinationDir = path.join(root, 'crates', 'dustroute-translate', 'tests', 'fixtures')
const destination = path.join(destinationDir, `${fixtureName}.json`)
const metadata = path.join(destinationDir, `${fixtureName}.meta.json`)
if (fs.existsSync(destination) || fs.existsSync(metadata)) {
  throw new Error(`fixture ${fixtureName} already exists; review and remove it explicitly before replacement`)
}

function stateName (state) {
  return state && typeof state.name === 'string' ? state.name : 'unknown'
}

function properties (state) {
  return state && state.properties && typeof state.properties === 'object'
    ? state.properties
    : {}
}

function valueChanged (before, after, key) {
  return properties(before)[key] !== properties(after)[key]
}

function eventKind (event) {
  const before = event.before || {}
  const after = event.after || {}
  const name = stateName(after) === 'unknown' ? stateName(before) : stateName(after)
  if (name === 'lever') return valueChanged(before, after, 'powered') ? 'lever_state' : 'lever_state_noop'
  if (name === 'redstone_wire') {
    if (valueChanged(before, after, 'power')) {
      return Number(properties(after).power) > Number(properties(before).power)
        ? 'wire_powered'
        : 'wire_unpowered'
    }
    return 'wire_state'
  }
  if (name === 'repeater') return valueChanged(before, after, 'powered') ? 'repeater_powered' : 'repeater_state'
  if (name === 'observer') {
    if (valueChanged(before, after, 'powered')) {
      return properties(after).powered === true ? 'observer_pulse_start' : 'observer_pulse_end'
    }
    return 'observer_state'
  }
  if (name === 'redstone_lamp') {
    if (valueChanged(before, after, 'lit')) return properties(after).lit === true ? 'lamp_lit' : 'lamp_unlit'
    return 'lamp_state'
  }
  if (name === 'piston' && valueChanged(before, after, 'extended')) {
    return properties(after).extended === true ? 'piston_start' : 'piston_retract'
  }
  if (name === 'piston_head') return 'piston_head_completion'
  if (stateName(before) === 'air' && stateName(after) !== 'air') return 'piston_block_move'
  return `${name}_state`
}

function copyState (state) {
  return { name: stateName(state), properties: properties(state) }
}

const activationTick = trace.activation.game_tick
const events = trace.events.map((event, index) => {
  if (!Number.isInteger(event.game_tick) || event.game_tick < activationTick) {
    throw new Error(`event ${index + 1} has an invalid game_tick`)
  }
  if (!Number.isInteger(event.sub_tick_order) || event.sub_tick_order < 0) {
    throw new Error(`event ${index + 1} has an invalid sub_tick_order`)
  }
  const before = event.before || null
  const after = event.after || null
  return {
    sequence: index + 1,
    kind: eventKind(event),
    position: event.position,
    relative_game_tick: event.game_tick - activationTick,
    sub_tick_order: event.sub_tick_order,
    scheduler_phase: null,
    changed: JSON.stringify(before) !== JSON.stringify(after),
    before: copyState(before),
    after: copyState(after)
  }
})

function find (kind) {
  return events.find(event => event.kind === kind)
}

function difference (first, second) {
  return first && second ? second.relative_game_tick - first.relative_game_tick : null
}

const measurements = {}
if (events[0]) measurements.input_to_first_observed_transition_game_ticks = events[0].relative_game_tick
const repeater = find('repeater_powered')
const wire = find('wire_powered')
const observerStart = find('observer_pulse_start')
const observerEnd = find('observer_pulse_end')
const lampOn = find('lamp_lit')
const lampOff = find('lamp_unlit')
const pistonStart = find('piston_start')
const pistonHead = find('piston_head_completion')
if (repeater) measurements.input_to_repeater_power_game_ticks = repeater.relative_game_tick
if (wire && repeater) measurements.wire_to_repeater_power_game_ticks = difference(wire, repeater)
if (lampOn) measurements.input_to_lamp_on_game_ticks = lampOn.relative_game_tick
if (lampOff) measurements.input_to_lamp_off_game_ticks = lampOff.relative_game_tick
if (observerStart && observerEnd) measurements.observer_pulse_game_ticks = difference(observerStart, observerEnd)
if (observerStart && lampOff) measurements.observer_start_to_lamp_off_game_ticks = difference(observerStart, lampOff)
if (repeater && observerStart) measurements.repeater_to_observer_start_game_ticks = difference(repeater, observerStart)
if (pistonStart) measurements.input_to_piston_start_game_ticks = pistonStart.relative_game_tick
if (pistonStart && pistonHead) measurements.piston_start_to_stable_completion_game_ticks = difference(pistonStart, pistonHead)
if (pistonStart && pistonHead) {
  measurements.completion_same_tick_event_count = events.filter(event => event.relative_game_tick === pistonHead.relative_game_tick).length
}

const promoted = {
  schema_version: 'dustroute.scheduler-observation-fixture.v1',
  minecraft_version: process.env.DUSTROUTE_MC_VERSION || '1.21.11',
  profile_id: 'minecraft_java1_21_11_modelled',
  profile_evidence: 'modelled',
  evidence: 'observed',
  source: 'live_mineflayer',
  scenario: artifact.scenario || 'unknown',
  source_artifact: path.relative(root, source),
  clock: {
    unit: 'game_tick',
    origin: 'normal_player_activation',
    absolute_ticks_omitted: true,
    scheduler_phase: null,
    scheduler_phase_evidence: 'unknown'
  },
  input: {
    kind: 'normal_player_activate_block',
    transition: `${trace.activation.before_powered ? 'on' : 'off'}_to_${trace.activation.after_powered ? 'on' : 'off'}`,
    activation_is_baseline: true
  },
  events,
  measurements,
  notes: [
    'Promoted from one bounded packet observation on the pinned Minecraft Java server.',
    'relative_game_tick is measured from normal player activation; absolute server ticks are intentionally omitted.',
    'sub_tick_order is Mineflayer blockUpdate packet order, not the internal vanilla scheduler order.',
    'The modelled profile remains separate and is not promoted by this packet observation.'
  ]
}

fs.mkdirSync(destinationDir, { recursive: true })
fs.writeFileSync(destination, `${JSON.stringify(promoted, null, 2)}\n`)
fs.writeFileSync(metadata, `${JSON.stringify({
  schema_version: 'dustroute.scheduler-observation-metadata.v1',
  minecraft_version: promoted.minecraft_version,
  fixture: path.relative(root, destination),
  source_artifact: promoted.source_artifact,
  source_result: resultName,
  reason,
  promoted_at: new Date().toISOString()
}, null, 2)}\n`)
process.stdout.write(`${destination}\n${metadata}\n`)
