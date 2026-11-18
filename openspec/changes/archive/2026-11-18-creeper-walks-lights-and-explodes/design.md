# Design: the creeper walks in, lights and explodes

## When a fuse burns

BDS's creepers sent `FUSE_TIME` 29 on the sample they lit and exploded 30 samples later,
whether the sensor or flint and steel lit them. Primed TNT, powered at tick t, exploded at
t + 81 with a fuse of 80 (`tnt_prime.json`). One rule fits both: a fuse burns once a tick in
the actor's update and the actor explodes on the burn after it reaches 0; a fuse lit before
that tick's burn burns the same tick. The creeper is lit in its AI or by a packet, before
the burn; redstone primes TNT after it. `FuseClock` records the last tick the fuses burned,
and a fuse lit after that first burns on the next tick.

## When the sensor reads

The creeper lit the tick after it took its target, and its fuse went out on the very tick
its teleported player showed far away. Run after the AI with the group changes applied a tick
later, as before, the second came a tick late. Run before the AI with the changes applied at
once, both match: the target goal sets the target in tick T, the sensor sees it in T + 1.

The goal selector's order (target goals first) was tried the other way round to explain the
creeper's first step coming a tick after its target; the skeleton's shot replay broke, so the
order stays and the early first step is left open.
