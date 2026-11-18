# Proposal: sleeping in beds and the night skip, measured on BDS

## Why

Beds did nothing: a click on a bed fell through to placement, nights could not be skipped and a
bed never became a respawn point. Every survival player meets this on the first night.
`player.sleeping` was `missing` and `world.day_cycle` listed "no sleeping skip".

Prior work:
- PR #71 (`origin/integration/2026-11-04-lanes`, commit 55804ec5) had sleeping in a block
  behaviour module that main does not have. Its constants (the 0.90625 sleeping height, 100
  ticks, the sleeping-players event, the stand-up offsets) were re-checked on BDS; the stand-up
  order and the event were right, the yaw it set, the `MovePlayer` it sent and the wake height
  were not;
- `server::game::roster` already builds the sleeping-players event for joins and leaves;
- `world::clock` has the darkening the beds use, `world::weather` the rain and lightning
  levels;
- `docs/solutions/server/bds-judges-reported-positions-against-its-own-copy.md`: the movement
  authority that corrects a waking player;
- `docs/solutions/server/player-actor-data-updates-follow-bds-sync-points.md`: when actor data
  is sent.

## What Changes

- **Measurement:** five scripted captures of plain BDS 1.26.51
  (`tools/bds-sweep/scripts/sleeping*.txt`, three with a second player), a `stop-sleeping` order
  for the sweep client and `tools/bds-sweep/sleeping_fixture.py`, which distils them into
  `crates/unastar/src/entity/player_life/testdata/bds-1.26.51-sleeping.json` (55 clicks with
  their setting, 38 wakes, 7 nights, the actor data).
- **Beds** (`entity::player_life::sleep`): the click checks (occupied, reach, respawn point,
  day with the weather, monsters), lying down, waking (asked, bed gone, day, damage, teleport,
  death, leaving: the bed is freed and the player saved standing), the
  stand-up search, the night skip with `playersSleepingPercentage`, the sleeping-players event.
- **Shared types extended:** `PlayerSpawnPoint` carries the bed (saved with the player as
  `spawn_bed`); `PlayerActorData` sends keys 26, 28 and 133 and the sleeping flag and box;
  `GameRules::int` with `IntGameRule`; `clock::weather_sky_darken` and `clock::jump_to`;
  `Horizontal::from_legacy`, `step`; `block_actors::resend`.
- **Movement:** a sleeper's inputs only turn it (`server::game::movement`).

## Impact

Crates: `unastar` (player life, movement, commands' teleport and spawn point, clock, storage
record), `bds-sweep` (the `stop-sleeping` order).

### Parity rows

- `systems`: `player.sleeping` to `verified`; `world.day_cycle` stays `partial` (time is not
  persisted) with the skip tested; `player.death_respawn` and `world.weather` gaps updated.

## Out of scope

Phantoms' rest timer, respawn anchors, the respawn screen's packets for a bed point, beds in the
Nether or the End (one dimension), the weather after a skip with `doWeatherCycle` on, saving the
world clock.
