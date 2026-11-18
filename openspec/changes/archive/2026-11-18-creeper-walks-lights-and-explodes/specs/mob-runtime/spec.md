## ADDED Requirements

### Requirement: A fuse lit before the tick's burn burns that tick
An actor's `minecraft:explode` fuse SHALL burn once a tick and explode the actor on the burn
after it reaches 0. A fuse lit before that tick's burn MUST burn the same tick; one lit after
it MUST first burn on the next tick. `IGNITED` MUST be raised when the fuse lights and lowered
when the component is removed.

#### Scenario: A creeper lit by flint and steel
- **WHEN** a player uses flint and steel on a creeper on tick t
- **THEN** the creeper is `IGNITED` on tick t, sends `FUSE_TIME` 29 then, and explodes on tick t + 30

Evidence: crates/unastar/src/entity/mob/tests/creeper.rs (tests `flint_and_steel_lights_a_creeper_like_bds`), crates/unastar/src/server/game/explosion/tests.rs (test `powered_tnt_primes_and_explodes_81_ticks_later_like_bds`)

### Requirement: The target-nearby sensor reads before the AI
The target-nearby sensor SHALL read the target's position before the mobs' AI runs, and the
group changes its events make MUST apply before that AI.

#### Scenario: The target leaves
- **WHEN** a lit creeper's target is teleported past `outside_range` before tick t
- **THEN** the creeper's fuse is out on tick t

Evidence: crates/unastar/src/entity/mob/tests/creeper.rs (tests `a_lit_creeper_puts_its_fuse_out_like_bds`)
