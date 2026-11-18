---
title: Tool clicks are answered like placements, in survival only
area: testing
tags: [bds-sweep, item-use, tools, copper, replay, scripted-client]
date: 2026-11-17
origin: items round 5 (tools/bds-sweep/scripts/tool-use.txt)
status: current
---

## Context

The first replay of the tool-use capture (a hoe, shovel, axe, honeycomb or shears clicked once
on a block, `item_use/tool_use_tests.rs`) failed on nearly every case although the blocks came
out right. The differences were in what BDS sends around the change, not in the change.

## What we learned

- A survival use that changed the block is answered like a placement: the six click
  corrections (the clicked block and the block on the face, each with the blocks above and
  below it) and the whole inventory, twice, which is where the wear or the used-up honeycomb
  shows (unastar sends it once). No `InventorySlot` is sent. A refused use gets the
  corrections only, and a creative use gets neither, just the block update. One helper (`answer` in `item_use/on_block.rs`) now does
  this for every tool, flint and steel and fire charges included.
- Copper is scraped, unwaxed and waxed with level events (2032, 2031, 2030 at the block centre,
  data 0), never with a `LevelSoundEvent`. A replay that only compares sounds misses this, so
  `tool_use_tests.rs` compares level events too.
- A block's own use wins over the tool: copper doors, trapdoors and chests open, and the axe does
  not wear.
- A `/setblock` with a name Bedrock does not have (`rooted_dirt`; Bedrock's is
  `dirt_with_roots`) fails silently in a scripted capture: the case clicks whatever stood there.
  The fixture cutter drops such cases by name (`REFUSED_BLOCKS` in `tool_use_fixture.py`), and
  `Block::state` in the replay panics on an unknown name, so a typo cannot pass unnoticed.
- One click per case is enough for behaviour; wear rates need hundreds of repeated uses with a
  fresh item per section, counted from the inventory BDS resends after each use.

## Where it lives

- `tools/bds-sweep/tool_use_script.py`, `tool_use_fixture.py`, `scripts/tool-use.txt`.
- `crates/unastar/src/server/game/item_use/on_block.rs` (`answer`, `wear_and_answer`),
  `item_use/copper.rs`, `item_use/tool_use_tests.rs`.
- Quirk rows "Items / tools on blocks", "Items / copper", "Items / pumpkin carving" in
  `docs/quirks/bds.md`.
