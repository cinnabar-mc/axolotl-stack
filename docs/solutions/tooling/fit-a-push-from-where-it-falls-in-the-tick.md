---
title: Fit a push from where it falls in the tick; the rested input after a teleport pushes too
area: tooling
tags: [movement, movement-diff, bubble-column, current, water, ceiling]
date: 2026-11-16
origin: movement round 7 (OpenSpec change movement-sines-and-pushes)
status: current
---

## Context

Round 7 replaced the trust for bubble columns and water currents with simulation, and fitted how
a sneak let go under a ceiling stands up. Each fit took one probe run of four to twelve
scenarios. These were the traps.

## What we learned

- **The first input's move shows the pushes of the input before it.** BDS rests the input after
  the start teleport: no move, no drag. Bubble columns and currents still push in it, so the
  first move of a scenario that starts in a column or a current is already pushed (0.06 down,
  0.028 along a channel). `movement::rest` reproduces that rested input, and both the authority
  and the scenario simulation call it.
- **Where the push sits shows in the corrected velocity.** A correction carries the velocity
  after the tick. A downward column's corrected velocity was exactly -0.3, the column's limit,
  so its push comes after the water's drag. A current's corrected velocity is the move times
  0.8, so its push comes before the move. Solving `m(n+1) = a·m(n) + c` on consecutive moves
  gives the drag (`a`) and the push (`c`) before you guess at a formula.
- **Count the blocks the box overlaps, not those it touches.** A column pushes once per block
  (0.18 a tick from three), and a box ending exactly on a block's bottom is not pushed by it.
  The standing check under a ceiling, by contrast, needs the 1e-6 collision tolerance: a box
  touching a slab's side stands up.
- **Pick the probe geometry so the answer is unambiguous.** A one-wide channel gives the same
  push whether the flows are averaged or normalised. Four cells with different flows
  (`current_diamond_four`) show that BDS normalises the sum.
- **A loaded machine breaks a run, not a rule.** Building with cargo during a BDS run made the
  subject send several scenarios' first active inputs as idle ones, and the run's validation
  failed. Play the harness runs on a quiet machine, and build in another target directory
  (`CARGO_TARGET_DIR=target/mutation`) only between runs: rebuilding `target/debug` replaces
  the client a run is using.
- **A stopped background shell may not stop its children.** Stopping the tool's background task
  left the queued `bds_run.py` calls running. A second queue started on the same slot then
  failed on the world lock and deleted some of the first run's files. Check `tasklist` before
  you start runs again.
