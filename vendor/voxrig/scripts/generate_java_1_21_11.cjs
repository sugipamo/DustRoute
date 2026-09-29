// Regenerate only the pinned data adapter; this is not a runtime dependency.
// node scripts/generate_java_1_21_11.cjs /absolute/path/to/minecraft-data [--check]
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const source = path.resolve(process.argv[2]);
const check = process.argv.includes('--check');
const metadata = require(path.join(source, 'package.json'));
assert.equal(metadata.version, '3.114.0');
const data = require(source)('1.21.11');
assert.equal(data.version.version, 774);
// Block-action packets use block IDs, unlike block updates which use state IDs.
data.blocksArray.forEach((block, index) => assert.equal(block.id, index));
const root = path.resolve(__dirname, '..');
const digest = value => crypto.createHash('sha256').update(value).digest('hex');
function output(name, contents) {
  const destination = path.join(root, name);
  if (check) assert.equal(fs.readFileSync(destination, 'utf8'), contents, name);
  else fs.writeFileSync(destination, contents);
}
const blocks = JSON.stringify(data.blocksArray.map(({name, minStateId, maxStateId, states}) => ({name, minStateId, maxStateId, states}))) + '\n';
const items = JSON.stringify(data.itemsArray.map(({id, name, stackSize}) => ({id, name, stackSize}))) + '\n';
const stateShapes = [];
const shapeKeys = Object.keys(data.blockCollisionShapes.shapes).map(Number);
const shapes = Array.from({length:Math.max(...shapeKeys)+1},(_,id)=>data.blockCollisionShapes.shapes[id]);
for (const shape of shapes) {
  assert(Array.isArray(shape));
  for (const box of shape) {
    assert.equal(box.length,6);
    assert(box.every(v=>Number.isFinite(v)&&v>=-0.25&&v<=1.5));
    for(let axis=0;axis<3;axis++) assert(box[axis]<=box[axis+3]);
  }
}
for (const block of data.blocksArray) {
  const mapping = data.blockCollisionShapes.blocks[block.name];
  assert.equal(stateShapes.length,block.minStateId);
  if (Array.isArray(mapping)) assert.equal(mapping.length,block.maxStateId-block.minStateId+1);
  for(let id=block.minStateId;id<=block.maxStateId;id++) {
    const shape=Array.isArray(mapping)?mapping[id-block.minStateId]:mapping;
    assert(Number.isInteger(shape)&&shapes[shape]); stateShapes.push(shape);
  }
}
const collisions=JSON.stringify({state_shapes:stateShapes,shapes})+'\n';
let ids = '// Packet IDs generated from minecraft-data 3.114.0, Java 1.21.11 (774).\n';
let known;
for (const phase of ['login', 'configuration', 'play']) {
  for (const [direction, suffix] of [['toClient', 'clientbound'], ['toServer', 'serverbound']]) {
    const mapping = data.protocol[phase][direction].types.packet[1][0].type[1].mappings;
    ids += `pub(crate) mod ${phase}_${suffix} {\n`;
    for (const [id, name] of Object.entries(mapping)) {
      ids += `    pub const ${name.toUpperCase()}: i32 = ${id};\n`;
    }
    ids += '}\n';
    if (phase === 'play' && direction === 'toClient') known = Object.keys(mapping);
  }
}
ids += '\npub(crate) const KNOWN_PLAY_CLIENTBOUND: &[i32] = &[\n';
for (let i = 0; i < known.length; i += 16) ids += '    ' + known.slice(i, i + 16).join(', ') + ',\n';
ids += '];\n';
const player = data.entitiesByName.player;
// The protocol JSON's attribute mapper omits newer entries. The registry array
// matches EntityAttributes registration order and the retained native trial.
const scale = data.attributesArray.findIndex(attribute => attribute.resource === 'minecraft:scale');
assert.equal(scale, 25);
ids += `\n// Native tracked-player registry fields.\n`;
ids += `pub(crate) const PLAYER_ENTITY_TYPE: i32 = ${player.id};\n`;
ids += `pub(crate) const PLAYER_POSE_METADATA: u8 = ${player.metadataKeys.indexOf('pose')};\n`;
ids += `pub(crate) const SCALE_ATTRIBUTE: i32 = ${scale};\n`;
const particles = data.protocol.types.Particle[1][0].type[1].mappings;
const effectParticle = Object.entries(particles).find(([, name]) => name === 'entity_effect');
assert(effectParticle);
ids += `pub(crate) const ENTITY_EFFECT_PARTICLE: i32 = ${effectParticle[0]};\n`;
output('data/java_1_21_11/blocks.json', blocks);
output('data/java_1_21_11/items.json', items);
output('data/java_1_21_11/collision_shapes.json', collisions);
output('src/versions/java_1_21_11/ids.rs', ids);
output('data/java_1_21_11/source.json', JSON.stringify({
  provider: 'minecraft-data',
  package_version: metadata.version,
  minecraft: '1.21.11', protocol: 774, data_version: 4671,
  block_data_sha256: digest(blocks),
  item_data_sha256: digest(items),
  collision_data_sha256: digest(collisions),
  packet_ids_sha256: digest(ids),
  transformation: 'blocksArray projected to name, minStateId, maxStateId, states; itemsArray to id, name, stackSize; blockCollisionShapes flattened to complete native state IDs and AABBs; packet ID constants generated from login/configuration/play mappings; player entity, pose index, scale attribute and effect particle IDs projected from registries',
  generator: '../../scripts/generate_java_1_21_11.cjs',
  license_notice: '../../THIRD_PARTY_NOTICES.md',
}, null, 2) + '\n');
console.log(check ? 'Pinned 1.21.11 adapter data verified' : 'Pinned 1.21.11 adapter data generated');
