# Proposal: Explosions round 2, and respawn anchors

## Why

The first explosion change ([2026-11-12-tnt-and-explosions-measured-on-bds](../2026-11-12-tnt-and-explosions-measured-on-bds/proposal.md))
left knockback unmeasured and pushed every actor from the line to its feet, subtracted half the
width from players too, broke waterlogged blocks, did not know water, dropped every stack where its
block stood, had no fire and no difficulty rule, and nothing but power could light TNT. Respawn
anchors could not be charged and never exploded.

Prior work: `docs/solutions/testing/fit-explosions-by-distribution-and-from-still-mobs.md` (fit by
distribution, hold mobs still), the explosion quirk rows in `docs/quirks/bds.md`, and the unmerged
#71 (`f022f305`: a respawn anchor and TNT lighting written from the wiki), re-checked here against
new BDS measurements. No decision or spec covers explosions.

## What Changes

- Measure on BDS 1.26.51 (`tools/bds-sweep/tnt.py` round 2): knockback on held mobs and on the
  scripted player at every difficulty, TNT in water, waterlogged slabs in the blast cube, drops
  traced per tick, `tntexplosiondropdecay`, 64 caged chains, TNT lit with flint and steel and fire
  charges, a charged respawn anchor (fire, damage, drops) and glowstone charging. The Endstone
  plugin records damage events with their actors, knockbacks and explosions (with BDS's block
  list) per cell; `explosions_table.py` writes the tables.
- `server/game/explosion.rs`: players' plain distance; hurt knockback from the igniter or the TNT,
  then a push towards the eyes; water; waterlogged blocks; peaceful; the igniter as attacker;
  drops gathered and popped after clearing, one in the power for non-TNT explosions; fire.
- TNT lit by flint and steel and fire charges; the `tntExplodes`, `tntExplosionDropDecay` and
  `respawnBlocksExplode` game rules are typed.
- `server/game/respawn_anchor.rs`: charging with glowstone and the overworld explosion.

Affected crates: `unastar`, `unastar-data`; harness `tools/bds-sweep`.

## Capabilities

### New Capabilities

None (`skip_specs: true`): vanilla behaviour, tracked in `docs/parity/`.

### Modified Capabilities

None.

## Impact

### Parity rows

- systems: `world.explosions` stays partial (creepers and other sources missing), gaps narrowed.
- blocks: `minecraft:tnt` partial (dispensers, fire, explode_bit left); `minecraft:respawn_anchor`
  partial (charging and explosion; no respawn point while unastar runs only the overworld).
- entities: `minecraft:tnt/explode` notes and gaps.

The systems and blocks rows are claimed by lane BLOCKS (the anchor by this change's claim).

### Out of scope

Creepers, beds in the nether and the end (one dimension), `max_resistance` and
`knockback_scaling` of other sources (no vanilla source measured), a moving source inside a solid
block, armour's knockback resistance on mobs, the exposure of a target standing on a block.
