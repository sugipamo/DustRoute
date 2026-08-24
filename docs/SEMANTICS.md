# Electrical semantics kernel

The optimizer separates four different concerns that were previously mixed
inside `sim.py` and `connectivity.py`.

## 1. Static block capabilities (`model.py`)

`BlockProperties` describes independent capabilities instead of one broad
"conductive" flag:

- physical component support;
- accepting weak stored block power;
- accepting strong stored block power;
- whether a repeater may read that stored power;
- whether strong stored power may drive adjacent dust.

A redstone block is deliberately **not** an ordinary powered block. It is a
constant direct source. Placing one next to a solid block does not create a
`PoweredBlockState` in that solid block in this model.

## 2. Electrical kernel (`electrical.py`)

The canonical pure rules live here:

- `component_output_level()` — direct outputs such as redstone blocks, levers,
  lit torches, and powered repeaters;
- `compute_powered_blocks()` — weak/strong stored state of ordinary blocks;
- `repeater_input_level()` — what a repeater may read;
- `torch_support_is_powered()` — a torch reads only its explicit support
  block's stored powered state;
- `direct_level_into_dust()` — direct sources and strongly powered blocks that
  may feed dust;
- `solve_instantaneous_electrical_state()` — zero-delay fixed-point solve.

Confirmed stored-power transfers currently modeled:

- dust -> ordinary block: weak power;
- powered lever -> its support block: strong power;
- powered repeater -> block in front: strong power.

Unconfirmed behaviors are not guessed. Comparator semantics and additional
component/block transfer rules should be added only with dedicated probes.

## 3. Explicit time (`sim.py`)

One abstract simulator tick has two phases:

1. sample repeater and torch inputs from the current settled state;
2. change delayed device outputs, then solve the instantaneous network again.

Calling `settle_instantaneous()` never advances a repeater queue or toggles a
torch. This makes construction/update-order issues in generated commands
separate from the electrical model.

## 4. Connectivity (`connectivity.py`)

The extracted graph is a **potential structural graph**. Every conditional edge
has an `EdgeRequirement`, for example:

- dust must currently carry signal;
- an ordinary block must carry stored power;
- a block must specifically carry strong power;
- a device output must currently be active.

The graph therefore offers two different questions:

- `can_potentially_reach()` — is a structural path possible?
- `can_actively_reach(..., state)` — is that path active in this settled state?

Accidental Net-short detection uses `conductive_components()`, which contains
only directly connected dust. Conditional paths through powered blocks or
repeaters do not merge two logical Nets.

## Real-world probe status

The kernel reflects the Minecraft checks performed so far:

- direct source -> dust;
- dust strength decay;
- weak block power does not emerge into adjacent dust;
- repeaters can read weak block power;
- repeaters restore signal strength;
- repeaters strongly power an ordinary block;
- torches invert their explicit support block;
- an adjacent redstone block is a direct source, not propagation of ordinary
  powered-block state through a solid block.


## Physical step continuity

Geometric path adjacency is not sufficient for Minecraft routing.

Every routed adjacent pair must satisfy one confirmed physical step relation:
dust/dust shape connection, dust/repeater input, repeater/dust output, or the
corresponding block-power boundary rule.

Stair geometry additionally reserves its required air clearance. A conductor
or support block may not later occupy this clearance, including another point
from the same routed path.

These rules are now checked before treating a generated physical route as
legal.


## Verification status

The semantics document distinguishes three confidence levels:

- **JAVA-CONFIRMED** — exercised in the `ro_sem` data pack and observed to
  match Java Edition in the current validation world.
- **MODEL-REGRESSION** — covered by Python tests but not independently confirmed
  as a Minecraft rule.
- **UNVERIFIED** — intentionally not modeled as authoritative behavior yet.

### JAVA-CONFIRMED compatibility suite

The real-world compatibility baseline is probes **01 through 20**. These cover
source/dust strength, weak/strong block behavior, torch support behavior,
repeater direction/refresh, corners, stairs, BLOCK_POWER leaf boundaries, and
cell-output/routing boundaries.

Additionally, these complete generated circuits have been confirmed in Java
Edition:

- half adder
- 2:1 multiplexer
- enabled 1-to-2 decoder

Any change to `electrical.py`, `wire.py`, `connectivity.py`,
`port_realization.py`, `routing_resources.py`, `multinet.py`, physical cell
definitions, or Minecraft export is considered capable of invalidating this
baseline and should trigger real-game regression when practical.

### UNVERIFIED / deliberately incomplete

Comparator analog semantics, comparator side inputs, repeater locking,
quasi-connectivity, piston-specific power rules, and exact Java neighbor-update
ordering remain outside the authoritative kernel until probed explicitly.
