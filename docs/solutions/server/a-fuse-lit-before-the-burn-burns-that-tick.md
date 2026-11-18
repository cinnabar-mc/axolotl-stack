---
title: A fuse lit before the tick's burn burns that tick; find the order from two lighters
area: server
tags: [bds-sweep, creeper, tnt, explode, sensors, tick-order]
date: 2026-11-18
origin: entities round 11 (the creeper)
status: current
---

## Context
The creeper could spawn once `behavior.melee_attack` compiled, and its replay of
`sweep.py creeper` failed by exactly one tick in three places. The explosion came 31 ticks
after the light (BDS: 30). The fuse went out a tick after the target was teleported away. And
the creeper took one more full step after it lit, where BDS's had already stopped. The TNT
replay, sharing the fuse code, was right at 81 ticks for a fuse of 80.

## What we learned
- One counter explains both fuse users: a fuse burns once a tick in the actor's update and
  explodes on the burn after it reaches 0. A fuse lit before that tick's burn burns the same
  tick (the creeper, lit by its sensor or a packet: `FUSE_TIME` 29 on the lit sample). One lit
  after it burns first on the next tick (TNT, primed by redstone after the actors: 81). An
  explicit "has the burn run this tick?" clock (`FuseClock`) gives both without moving any
  system.
- The flag that a goal reads must be set where the event lands, not in a later system. With
  `IGNITED` raised by the fuse system after the AI, the swell goal started a tick late and the
  creeper walked one step too far. Raising it when the component is added fixed the step and
  moved the hiss a tick early; the hiss now comes on the goal's second tick.
- A sensor that runs after the AI with deferred group changes is two ticks behind a teleported
  target. BDS read it before the AI and applied the event at once. The "lit the tick after the
  target" timing that hid this comes out the same either way, so test a target that moves on
  its own, not only one that is taken.
- Changing the goal selector's phase order to explain a one-tick early first step broke the
  skeleton's shot replay. A global order is pinned by every replay; leave a local one-tick gap
  open rather than move it for one mob.
- A loot table whose any pool hits an unsupported condition fails the whole roll: the creeper
  dropped nothing at all, not just no disc. Its disc names are legacy `record_*` items that also
  need an alias.

## How to apply
When a replay is off by one tick, write down when each event lands relative to the system that
reads it (before or after the AI, the burn, the definition barrier) and look for a second
actor with the same component lit from the other side of that system. Then run every replay
that shares the system before committing.
