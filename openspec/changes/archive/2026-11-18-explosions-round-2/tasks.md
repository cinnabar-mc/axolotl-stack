# Tasks

- [x] 1.1 Claim `minecraft:respawn_anchor` and `world.explosions` for lane BLOCKS in `docs/parity/claims.toml`
- [x] 1.2 Endstone plugin: damage events with actors, knockbacks and explosions per cell; solo items for the scripted player; round-2 families in `tools/bds-sweep/tnt.py`; `explosions_table.py`
- [x] 1.3 Sweeps on one slot; tables in `crates/unastar-data/data/measured/explosions/`
- [x] 2.1 `server/game/explosion.rs`: distances, hurt knockback and push, water, waterlogged blocks, peaceful, igniter, gathered drops, decay, fire
- [x] 2.2 TNT lit by flint and steel and fire charges; typed game rules
- [x] 2.3 `server/game/respawn_anchor.rs`
- [x] 3.1 Tests against every table (`explosion/tests/round2.rs`, `respawn_anchor/tests.rs`)
- [x] 3.2 Parity rows, quirk rows, solution note; `cargo run -p parity -- generate` and `-- check`
- [x] 3.3 Validation: `cargo fmt --all -- --check`, `cargo clippy -p unastar --all-targets -- -D warnings`, `cargo test -p unastar --lib --tests`, `cargo run -p parity -- check`, `openspec validate --all --strict`, `bun tools/agents/check-links.ts`, `python -m unittest discover -s tools/bds-sweep -p "test_*.py"`
