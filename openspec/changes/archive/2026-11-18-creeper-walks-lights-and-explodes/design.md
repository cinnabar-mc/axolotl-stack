# Design: the creeper walks in, lights and explodes

## When a fuse burns

BDS's creepers sent `FUSE_TIME` 29 on the sample they lit and exploded 30 samples later,
whether the sensor or flint and steel lit them. Primed TNT, powered at tick t or lit with
flint and steel, exploded 81 ticks later with a fuse of 80 (`tnt_prime.json`,
`tnt_lit.json`). One rule fits both: a fuse burns once in each of the actor's own ticks and
the actor explodes on the burn after it reaches 0, and an actor has no tick of its own in the
tick it spawned. The creeper already exists when it lights; a primed TNT is a new actor. A
first try (a clock of "has the burn run this tick") fitted the creeper and redstone TNT but
gave flint-lit TNT 80 ticks; review caught it.

## When the sensor reads

The creeper lit the tick after it took its target, and its fuse went out on the very tick
its teleported player showed far away. Run after the AI with the group changes applied a tick
later, as before, the second came a tick late. Run before the AI with the changes applied at
once, both match: the target goal sets the target in tick T, the sensor sees it in T + 1.

The goal selector's order (target goals first) was tried the other way round to explain the
creeper's first step coming a tick after its target; the skeleton's shot replay broke, so the
order stays and the early first step is left open.
