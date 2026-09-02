'use strict'

const fs = require('node:fs')
const path = require('node:path')

const root = path.resolve(__dirname, '../../../..')
const [sourceArg, nameArg, ...reasonParts] = process.argv.slice(2)
const reason = reasonParts.join(' ').trim()
if (!sourceArg || !nameArg || !reason) {
  throw new Error('usage: node e2e/promote-differential.js <trace.json> <fixture-name> <reason>')
}
if (!/^[a-z0-9][a-z0-9_-]*$/.test(nameArg)) {
  throw new Error('fixture-name must contain only lowercase letters, digits, _ and -')
}
const source = path.resolve(sourceArg)
const trace = JSON.parse(fs.readFileSync(source, 'utf8'))
if (trace.source !== 'minecraft_java' || !Array.isArray(trace.observations) || trace.observations.length === 0) {
  throw new Error('source must be a non-empty normalized minecraft_java trace')
}
const destinationDir = path.join(root, 'crates', 'dustroute-translate', 'tests', 'differential')
const destination = path.join(destinationDir, `${nameArg}.trace.json`)
const metadata = path.join(destinationDir, `${nameArg}.meta.json`)
if (fs.existsSync(destination) || fs.existsSync(metadata)) {
  throw new Error(`fixture ${nameArg} already exists; review and remove it explicitly before replacement`)
}
fs.mkdirSync(destinationDir, { recursive: true })
fs.writeFileSync(destination, `${JSON.stringify(trace, null, 2)}\n`)
fs.writeFileSync(metadata, `${JSON.stringify({
  schema_version: 'dustroute.differential-fixture.v1',
  minecraft_version: process.env.DUSTROUTE_MC_VERSION || '1.21.11',
  reason,
  source_artifact: path.relative(root, source),
  promoted_at: new Date().toISOString()
}, null, 2)}\n`)
process.stdout.write(`${destination}\n${metadata}\n`)
