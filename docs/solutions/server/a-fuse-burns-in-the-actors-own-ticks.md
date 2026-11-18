---
title: A fuse burns in the actor's own ticks; a new actor has none in its spawn tick
area: server
tags: [bds-sweep, creeper, tnt, explode, sensors, tick-order]
date: 2026-11-18
origin: entities round 11 (the creeper)
status: current
---

## Context
The creeper could spawn once `behavior.melee_attack` compiled, and its replay of
`sweep.py creeper` was off by exactly one tick in three places. The explosion came 31 ticks
after the light, where BDS's came at 30. The fuse went out a tick after the target was
teleported away. And the creeper took one more full step after it lit, where BDS's had already
stopped. The TNT replay shares the fuse code and was right: 81 ticks for a fuse of 80.

## What we learned
- One rule covers every fuse user. A fuse burns once in each of the actor's own ticks, and the
  actor explodes on the burn after the fuse reaches 0. An actor has no tick of its own in the
  tick it spawned.
  - The creeper already exists when it lights, so its fuse burns that same tick: `FUSE_TIME`
    29 on the lit sample, the explosion 30 ticks later.
  - A primed TNT is a new actor, primed by redstone or by flint and steel, so it first burns
    on the next tick and explodes 81 ticks later.
- A first fix keyed on "has the burn already run this tick". It fitted the creeper and
  redstone TNT, but it gave flint-lit TNT 80 ticks, because a player's click is applied before
  the burn. Review caught it. Check every way a component can be added, not only the ones the
  current replays cover.
- Set a flag where the event lands, not in a later system that a goal reads after the fact.
  With `IGNITED` raised by the fuse system (after the AI), the swell goal started a tick late
  and the creeper walked one step too far. Raising it when the component is added fixed the
  step, but moved the hiss a tick early. The hiss now comes on the goal's second tick.
- The target-nearby sensor used to run after the AI, with its group changes deferred to the
  next tick. That left it two ticks behind a teleported target. BDS read the target before the
  AI and applied the event at once. The "lit the tick after the target" timing that hid this
  comes out the same either way. So test with a target that moves on its own, not only one
  that has just been taken.
- I tried swapping the goal selector's phase order to explain a first step that came one tick
  early. It broke the skeleton's shot replay. A global order is pinned by every replay, so
  leave a local one-tick gap open rather than move the order for one mob.
- One unsupported condition in any pool fails the whole loot roll. The creeper dropped nothing
  at all, not just no disc. Its discs are also listed under legacy `record_*` names. Each
  disc's `descriptionId` (`item.record_stal`) resolves them, the same way pack items resolve
  legacy identifiers.

## How to apply
When a replay is off by one tick, write down when each event lands relative to the system
that reads it: before or after the AI, the actor's own tick, or the definition barrier. Then
find a second actor with the same component that gets lit from the other side of that system.
Run every replay that shares the system before committing.
