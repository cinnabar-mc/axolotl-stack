---
title: Measure explosion hurts with Endstone's damage, knockback and explode events, not health
area: testing
tags: [explosions, tnt, bds-sweep, endstone, knockback, drops, fire, water]
date: 2026-11-17
origin: lane BLOCKS round 6 (explosions round 2)
status: current
---

## Context

Round 1 ([fit-explosions-by-distribution-and-from-still-mobs.md](fit-explosions-by-distribution-and-from-still-mobs.md))
read whole-number health and could not see knockback (pigs died). Round 2 needed knockback, the
damage cause and attacker, players, water and the drops' stacks.

## What we learned

- Endstone has the facts as events: `ActorDamageEvent` gives the exact float damage (after armour
  for a mob in armour), the cause and both the attacker and the damaging actor;
  `ActorKnockbackEvent` fires for explosions too and gives the hurt knockback alone (the velocity
  change, so a mob resting on the ground shows 0.4784 up: 0.4 plus its -0.0784 resting fall);
  `ActorExplodeEvent` and `BlockExplodeEvent` give the explosion's position and BDS's list of
  removed blocks in its order (a respawn anchor's list also holds every air cell the rays
  reached). The plugin now keeps all of them per cell (`damage_events`, `knockbacks`,
  `explosions`).
- Keep mobs alive with `health_boost 60 24` plus a full heal (undead heal with
  `instant_damage`); Slowness 255 still lets the push move them, so the first tick's position
  change is the hurt knockback plus the push.
- An explosion in timeline tick t happens after that tick's read: the last read before the push is
  t + 1's.
- `dimension.actors` has no players. Measure the scripted player through its client capture: BDS
  sends it the hurt knockback and then the total velocity as two `SetActorMotion`s. Cut the capture
  per item at each `MovePlayer` teleport away from the waiting spot (console teleports send no
  text to the player after the first).
- Scenarios run at peaceful, which refuses monsters and spares players from TNT: set the
  difficulty with a `cmd` step (it is part of the wave signature, so such items run apart).
- After a blast near water, broken cells read as water, not air: compare BDS's block list.
- Item stacks never merged on the ground in 110 ticks: the first post-explosion stacks are the
  explosion's own gathering, so their sizes pin the gathering rule.
- Chained TNT pushed by the first blast leaves the actor box before it explodes; cage it in
  obsidian and take the tick difference of the two explode events.

## Where it lives

`tools/bds-sweep/tnt.py` (round 2), `tools/bds-sweep/explosions_table.py`, the Endstone plugin's
`on_damage`, `on_knockback`, `on_actor_explode`, `on_block_explode` and `jobs.cell_events`,
`crates/unastar/src/server/game/explosion/tests/round2.rs`.
