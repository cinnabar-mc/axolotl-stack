---
title: A waking player's correction comes from BDS's movement authority, and a script's /tp can hide it
area: server
tags: [sleeping, beds, correction, movement, bds-sweep, fixture, script]
date: 2026-11-17
origin: openspec/changes/archive/2026-11-17-sleeping-in-beds
status: current
---

## Context

Copying sleeping from BDS 1.26.51 meant finding where BDS stands a waking player up. The scripted
captures (`tools/bds-sweep/scripts/sleeping*.txt`) left the bed with `stop-sleeping`, then sent
`/tp` back to the start a few inputs later. Many wakes showed no correction at all, which first
read as "BDS leaves the player in the bed when the side is walled".

## What we learned

- **BDS does not send the stand-up position itself.** It puts its copy of the player 0.1 above
  the spot's floor. The `CorrectPlayerMovePrediction` the client gets is the movement authority
  judging the client's next inputs, which still report the bed. So it arrives with the wake or up
  to three inputs later, and its height is one or two ticks of BDS's simulation (feet -59.905,
  -59.978 or -60.0 for a spot whose floor is at -60, with the matching velocity). Compare only
  the column, or replay the authority.
- **A `/tp` a few inputs after the wake hides it.** The teleport overrides the copy before the
  authority corrects anything. Two inputs without a correction proved nothing. Waiting 20 inputs
  before the next `/tp` (`sleeping-4.txt`) showed every stand-up spot, including the walled cases
  that had looked like "stays in the bed".
- **unastar copies the mechanism, not the packet.** The wake moves the server's copy
  (`PlayerMotion::placed`) and lets the authority correct the client. Lying down is different:
  BDS sends the bed position at once, at rest, on the ground, and unastar does the same.
- **The relays tell you what BDS's copy is doing.** A second player's `MovePlayer` relays of the
  sleeper sat exactly at the head's (x + 0.5, y + 0.90625, z + 0.5) with the client's own yaw.
  The bed does not turn the player, unlike older community implementations.

## How to apply

- When a capture shows "no correction", check what the script sent in the next few inputs before
  concluding the server did nothing.
- For anything that moves a player server-side (beds, knockback, pushes), store the server
  position and let the authority correct. Hand-made correction packets will not match BDS's
  heights.
- The fixture keeps only stand-ups whose correction is not the script's own `/tp`
  (`tools/bds-sweep/sleeping_fixture.py`).
