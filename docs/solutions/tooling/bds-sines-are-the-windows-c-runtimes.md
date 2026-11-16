---
title: BDS's sines are the Windows C runtime's, and its sine table inverts its lookup scale
area: tooling
tags: [movement, movement-diff, trigonometry, f32, rounding, windows]
date: 2026-11-16
origin: movement round 7
status: current
---

## Context

Round 6 left a glide at yaw 37.5 and a sprint jump at yaw 60 (`random_172`) a few ULP off BDS
1.26.51. Neither the correctly rounded sine nor any `f32` angle near the table entry's gave
both. Round 7 read the sines one at a time from BDS and fitted them.

## What we learned

- **One input from rest shows a sine at full precision.** Start the player at rest on stone
  at x = z = 1/32, so the box's corners stay exact. A sprint jump's first move is
  `-sin(table) × 0.2 - 0.98 × 0.13 × sin(yaw)` on x, and the matching cosines on z. The
  position and the corrected velocity of input 6 pin the table entry to about one ULP, because
  the walk's part is known. A plain walk gives the yaw's own sine the same way. Put the probe's
  yaw on the input segment, not on the start. The start teleport rounds the yaw, and the
  harness then rejects the setup.
- **The table is `sin(i / 10430.378)`.** Each entry is the sine of `i` divided by the lookup's
  own `f32` scale (65536 / 2pi = 10430.378), not of `i × 2pi / 65536`. For 18 of the 152 entries
  read, those two angles are an ULP apart and give different sines (cos 60 is 0.5000552, not
  0.5000554). The drop direction's table in `unastar_rules::math` already did this. Movement
  had its own table, built the other way; there is now one table.
- **The sine is the platform's, not the correctly rounded one.** BDS runs on Windows, and Rust's
  `f32::sin` on Windows is the same C runtime function. A full sweep of every `f32` angle up to
  256 radians finds the runtime one ULP off the correctly rounded value on about one angle in
  15,000, never by more. Pick probes at those angles, from a list made with Rust on Windows.
  The 24 sprint jumps and 23 walks picked there all took the runtime's value.
- **Ship the exceptions, not the platform.** `unastar_rules::math::sin` is libm's `f64` sine,
  rounded to `f32` (the same on every platform), with the runtime's exceptions applied. The
  exceptions are about 340 KB of delta-encoded angles in `crates/unastar-rules/data/`. The
  `sine_exceptions` example writes them and checks every angle against the runtime. It runs
  only on Windows.
- **The glide's look vector wraps the yaw, the walk and the sprint jump do not.** At yaw 180.33
  the glide moves as at -179.67. Wrapping the walk's or the sprint jump's yaw breaks
  `sprint_turn_jump`.

## Still open

Glides drift 1 to 5 ULP after a dozen ticks at some pitches (`glide_p12`, `glide_pm10`,
`elytra_glide_yaw`). No single-step `f32` reordering of the glide removes it, and doing the
glide in `f64` is far worse.
