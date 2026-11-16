---
title: Count a sweep's samples from what the client sent, not from the script
area: testing
tags: [bds-sweep, scripted-client, unbreaking, durability, sampling]
date: 2026-11-16
origin: items round 4 (Unbreaking per wear event)
status: current
---

## Context

The per-event Unbreaking sweep (`tools/bds-sweep/scripts/unbreaking-events.txt`) stalled
partway through its spear section. The client logged nothing for three hours, then BDS closed
the session; the machine had most likely slept. `script.py` still reported `client exited 0`.
The first `unbreaking_table.py` counted hits from the script's `attack-named` lines, so it
reported 200 spear hits when the client had sent 161. A share computed that way is biased
toward "spared", because the hits that were never sent look like hits that cost nothing.

## What we learned

- `client exited 0` does not mean the script finished. Compare the last `script order` in
  `client.log` with the script's last line, and look for gaps in the timestamps.
- Count any event that may cost nothing from the capture: the client's own packets (an attack is
  an `InventoryTransaction` with `ItemUseOnActor`), or what BDS sends back (destroy particles
  `2001` for broken blocks). Reading wear from the item's `Damage` only shows the events that cost
  something.
- `bds-sweep-dump` (not `bds-sweep-client --decode`, which does not exist) prints both
  directions when `--dir` is left out, in recording order, and names packets with their `Packet`
  suffix. A table script that reads client and server packets together has to accept both.
- Glides cannot be counted from the capture this way: the elytra's wear events are every 20
  gliding ticks, and BDS reports only the ones that wore it. Their count comes from a level-0
  run of the same glides (every event wears without Unbreaking).
- When one section of a long script fails, rerun only the sections still needed with
  `unbreaking_script.py --kinds ...` rather than the whole script, and record which run each
  section of the fixture came from (its `source`).

## How to apply

In any sweep table, take the number of trials from the capture. Fall back to the script's count
only when the capture cannot show the trials. In that case, check that the run reached the
script's end before trusting the count.
