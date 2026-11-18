# Tasks

## 1. Claim and measure

- [x] 1.1 Claim `player.sleeping` and `world.day_cycle` in `docs/parity/claims.toml`
- [x] 1.2 `stop-sleeping` order in `bds-sweep-client`; scripts `sleeping.txt`,
  `sleeping-2.txt` (with `sleeping-2-observer.txt`), `sleeping-3.txt`, `sleeping-4.txt`
  captured on a BDS slot
- [x] 1.3 `tools/bds-sweep/sleeping_fixture.py` and the committed fixture

## 2. Beds

- [x] 2.1 `entity::player_life::sleep`: click checks, lying down, waking, stand-up search,
  night skip, the event at deaths, the respawn point with its bed
- [x] 2.2 Actor data keys 26, 28, 133, the sleeping flag and box; sleepers held by movement;
  teleports wake
- [x] 2.3 Tests replaying the fixture (`sleep/tests.rs`)

## 3. Records

- [x] 3.1 Parity rows in `docs/parity/systems.toml`; `cargo run -p parity -- generate` and
  `-- check`
- [x] 3.2 Quirk rows in `docs/quirks/bds.md`; solution note; tool READMEs
- [x] 3.3 Validation: `cargo fmt --all -- --check`; `cargo clippy -p unastar -p bds-sweep
  --all-targets -- -D warnings`; `cargo test -p unastar --lib --tests`; `cargo test -p
  bds-sweep`; `cargo run -p parity -- check`; `openspec validate --all --strict`;
  `bun tools/agents/check-links.ts`; `python -m unittest discover -s tools/bds-sweep -p
  "test_*.py"`
