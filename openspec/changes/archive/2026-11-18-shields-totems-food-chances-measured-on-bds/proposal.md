# Proposal: shields, totems and food chances measured on BDS

## Why

Shields did nothing (`minecraft:shield/shield` and `/durability` missing), a totem of undying did
not save its holder, and the chance foods' effect chances were never compared with BDS
(`minecraft:chicken/food`, `rotten_flesh/food`, `poisonous_potato/food` partial).

Prior work: the food captures and their replay (`item_use/food_tests.rs`,
[replays-compare-random-rolls-with-measured-outcomes](../../../../docs/solutions/testing/replays-compare-random-rolls-with-measured-outcomes.md)),
the shared cooldown (`item/cooldown.rs`) and `durability.rs` as the single wear rule. Nothing in
`docs/decisions/` or `openspec/specs/`.

## What Changes

- Captures on BDS 1.26.51 (`tools/bds-sweep/items6_script.py`, `scripts/items-6-{food,shield,totem}.txt`;
  tables and replay cuts by `items6_table.py`).
- `item_use/food.rs`: the effect roll as one function, compared by rate with BDS.
- `item_use/shield.rs`: blocking (sneak ticks, hand, facing, cause), wear, the block sound, axe
  disabling through the shared item cooldown.
- `item_use/totem.rs`: the totem's save, effects, actor event and consumption.
- Small shared hooks: `player_life/hurt.rs` asks the shield before damage and the totem before
  death; `inventory::send_offhand`; a sneak-tick system; `combat_fixture.cut` can keep client
  packets by name.

Affected crates: `unastar`; harness `tools/bds-sweep`.

## Capabilities

None new or modified: vanilla behaviour, tracked in `docs/parity/` (`skip_specs: true`).

## Impact

### Parity rows

- items: `minecraft:{chicken,rotten_flesh,poisonous_potato}/food` partial -> verified;
  `minecraft:shield/shield` and `minecraft:shield/durability` missing -> verified;
  `minecraft:totem_of_undying` notes.

Covered by lane ITEMS's existing "Item use" claim in `docs/parity/claims.toml`.

### Out of scope

Ender pearls, wind charges and thrown tridents (their projectile actors cannot spawn yet; the
entities lane owns them), pushing the attacker back from a blocked hit, main-hand shield wear,
`player.death_respawn` (entities lane) beyond the totem hook.
