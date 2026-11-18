# Proposal: tools used on blocks measured on BDS

## Why

Hoes, shovels and axes acted on blocks from public references: the block lists, sounds and wear
were unmeasured, and the 21 axe, hoe and shovel durability rows were partial ("wear from use on
blocks is not measured"). Shearing wear was unmeasured, honeycomb did not wax copper, and a mob's
dropped spear was worn against the item's native maximum damage instead of its pack durability.

Prior work: the flint and steel capture and its click corrections
([scripted-clicks-need-a-known-block-and-a-still-arena](../../../../docs/solutions/tooling/scripted-clicks-need-a-known-block-and-a-still-arena.md),
quirk "Items / flint and steel"), the Unbreaking sweeps ("Items / Unbreaking") and
`durability.rs` as the single wear rule (items rounds 1-4). Nothing relevant in
`docs/decisions/` or `openspec/specs/`.

## What Changes

- Capture on BDS 1.26.51 (`tools/bds-sweep/tool_use_script.py`, `scripts/tool-use.txt`): one
  click per case for every block a hoe, shovel, axe, honeycomb or shears converts or refuses;
  wear with Unbreaking 0 and III for each tool and for shearing sheep; spears dropped by husks.
  The scripted client gains `interact-named`.
- `item_use/on_block.rs`: survival uses answered with the click corrections and the inventory
  (creative with neither), wear predicted; copper scraped, unwaxed and waxed with level events;
  honeycomb waxing; a shovel digs a snow layer; a campfire is resent before it goes out; shears
  carve from the top face and drop one seed.
- `item_use/copper.rs`: copper relations derived from registered names once, kept by block id.
- `entity/mob/gear/drops.rs`: dropped gear wears against `durability::max_durability`.

Affected crates: `unastar`, `bds-sweep`; harness `tools/bds-sweep`.

## Capabilities

### New Capabilities

None. Vanilla behaviour, tracked in `docs/parity/` (`skip_specs: true`).

### Modified Capabilities

None.

## Impact

### Parity rows

- items: `minecraft:{wooden,stone,copper,golden,iron,diamond,netherite}_{axe,hoe,shovel}/durability`
  partial -> verified; `minecraft:shears/durability` notes (shearing measured);
  `minecraft:*_spear/durability` notes (drops measured).

Covered by lane ITEMS's existing "Item use" claim (`tools` family) in `docs/parity/claims.toml`.

### Out of scope

Copper chests opening (a container gap; axes and honeycomb leave copper chests and the unmeasured copper golem statues alone), door block updates on open,
snow layers deeper than one layer, carving from the top with other look directions, the
inventory resent twice (sent once).
