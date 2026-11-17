# Design: BDS's sines, a crouch under a ceiling, bubble columns and currents

## Context

Every rule here was read from BDS 1.26.51 through the movement differential
(`tools/movement-diff`): small probe scenarios played on one BDS slot, then the rule fitted
offline in numpy `f32` until it gave BDS's bits for every probe, then checked on probes not used
for the fit.

## Decisions

### The C runtime's sine as rounded sine plus shipped exceptions

BDS's sine differs from the correctly rounded one at about one angle in 15,000. The probes
picked at those angles took the Windows C runtime's value every time. Options:

1. call `f32::sin`: right on Windows, but Linux and macOS have other libms, so the server would
   judge players differently by platform;
2. ship a table of every angle: gigabytes;
3. compute the correctly rounded sine (libm's `f64` sine rounded, the same on every platform) and
   ship only the runtime's exceptions: about 120,000 angles to 256 radians, 340 KB
   delta-encoded, written and checked against every angle by an example run on Windows.

We take 3. Angles beyond 256 radians (yaws past about 14,600 degrees) take the rounded value.

### One sine table

`unastar_rules::math` already had a table for the drop direction (entries `sin(i / 10430.378)`).
Movement had its own, built as `sin(i × 2pi / 65536)`, which the probes show is wrong for 18 of
152 entries read. Both now read one table, computed on the fly from `math::sin` (an index
division and a sine, no 256 KB static).

### Pushes at their place in the tick

A bubble column pushes after the water's drag, so the velocity BDS reports after the tick holds
the push. A current pushes before the move, with the input's acceleration. The input BDS rests
after a teleport takes both pushes but no move or drag (`movement::rest`). The authority and the
scenario simulation call the same function.

## Risks

- The current's 0.0045 minimum push for a nearly still body (Java's) is not implemented. No probe
  measured a push that small.
- Falling water and flowing lava stay trusted; their pushes are unmeasured.
