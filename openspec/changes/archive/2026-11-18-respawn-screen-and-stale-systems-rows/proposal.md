# Proposal: the respawn screen as BDS sends it, and systems rows main's work already backs

## Why

unastar opened the respawn screen with searching-for-spawn and ready-to-spawn both at the spawn
point, and respawned a player at once with `doImmediateRespawn` on. BDS 1.26.51 does neither.
Several systems rows (`world.day_cycle`, `interface.titles_hud`, `interface.scoreboard`,
`player.experience`) still said "missing" or listed gaps that main had since closed with
evidence-backed work in other domains.

Prior work: the round-1 sleeping captures (`tools/bds-sweep/scripts/sleeping-2.txt`,
`sleeping-3.txt`) showed the two-step exchange for bed points; `player_life::respawn`; the
commands lane's `/title`, `/titleraw`, `/hud` and `/scoreboard` replays and the level-data
capture (`crates/unastar/tests/fixtures/bds_1_26_51_level_data.json`).

## What Changes

- **Measurement:** `tools/bds-sweep/scripts/respawn.txt` (six deaths) and
  `tools/bds-sweep/respawn_fixture.py` into
  `crates/unastar/src/entity/player_life/testdata/bds-1.26.51-respawn.json`.
- **Respawn screen** (`player_life::respawn`, `death`): searching-for-spawn at the death; on the
  client's answer searching-for-spawn at the point's block or `(0, 32767, 0)`, then
  ready-to-spawn; a blocked point moves up; `doImmediateRespawn` waits for the client too.
- **Rows:** `world.day_cycle` verified; `interface.titles_hud`, `interface.scoreboard` partial
  with their tests; `player.experience` and `player.death_respawn` gaps updated.

## Impact

Crates: `unastar` (player life). Tools: `bds-sweep` (script and distiller).

### Parity rows

- `systems`: `world.day_cycle` to `verified`; `interface.titles_hud` and `interface.scoreboard`
  from `missing` to `partial`; `player.death_respawn` stays `partial` (respawn anchors, BDS's
  spread of world-spawn respawns, the second search for a broken bed's point);
  `player.experience` gaps updated.

## Out of scope

The world-spawn spread (`spawnRadius`), respawn anchors, boss bars.
