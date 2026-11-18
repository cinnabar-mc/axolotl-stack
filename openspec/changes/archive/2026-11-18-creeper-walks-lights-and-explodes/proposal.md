# Proposal: the creeper walks in, lights and explodes

## Why

The creeper is the most common hostile mob unastar could not spawn. Round 8 measured it
(`sweep.py creeper`, the committed `bds-1.26.51-creeper.json`) and implemented its sensor,
`behavior.swell` and `is_charged`; blocks round 4 added `minecraft:explode` and the explosion.
Only `behavior.melee_attack` stopped it from spawning. Its loot table also failed every roll,
as `killed_by_entity` (the music discs) was not implemented.

Prior work: `docs/solutions/server/pair-each-actor-with-its-own-sample-tick-to-find-when-bds-decides.md`,
the creeper quirk rows, the TNT change (`2026-11-12-tnt-and-explosions-measured-on-bds`).
Prior art: neither PR #71 nor the `Jviguy/*` branches implement the creeper.

## What Changes

- `minecraft:behavior.melee_attack` compiles into the melee goal with `MeleeReach::Never`
  when `reach_multiplier` is 0 (the creeper's: chase, never hit); other multipliers are
  refused, as the goal's reach is not measured.
- A fuse lit before the tick's fuse burn burns that tick (`FuseClock`): the creeper explodes
  30 ticks after it lights and `FUSE_TIME` counts 29 to 0; TNT primed by redstone keeps its
  81 ticks. `minecraft:explode` raises `IGNITED` when it is added and lowers it when it is
  removed, so the swell goal stops the creeper on the tick it lights; the hiss (`fuse`) comes
  on the goal's second tick.
- The creeper class explodes at its feet (`NativeTraits::explodes_at_feet`).
- The target-nearby sensor runs before the mobs' AI, its events applied at once.
- The loot condition `killed_by_entity` (the killer's type; a projectile kill counts for its
  shooter).

Out of scope: lightning (and so charging by a strike), the explosion's own packet timing and
knockback (the blocks lane's), the creeper's first step coming a tick early.

## Capabilities

- `mob-runtime` (modified): fuses and the target-nearby sensor.

## Impact

`crates/unastar` (melee goal, explode component and fuse system, native traits, the sensor's
schedule, loot context), `crates/unastar-rules` (loot), `docs/parity`, `docs/quirks`.

### Parity rows

- entities: `minecraft:creeper` to `verified`, with its `behavior.melee_attack`,
  `behavior.swell`, `explode`, `interact`, `is_charged`, `target_nearby_sensor`,
  `navigation.walk`, `movement.basic`, `collision_box`, `health`, `movement` and
  `is_hidden_when_invisible` `verified`, and the facet sub-rows `done`.
- systems: `crafting.loot_tables` no longer lists `killed_by_entity`.
