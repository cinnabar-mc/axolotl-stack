# Tasks

- [x] 1.1 Claim `player.death_respawn` in `docs/parity/claims.toml`
- [x] 1.2 `respawn.txt` captured on a BDS slot; `respawn_fixture.py` and the fixture
- [x] 1.3 The two-step respawn screen, blocked points, `doImmediateRespawn`; the replay test
  `tests/respawn_screen.rs` and the updated older tests
- [x] 1.4 Rows in `docs/parity/systems.toml` and `protocol.toml`, `cargo run -p parity --
  generate` and `-- check`; the quirk row; the bds-sweep README
- [x] 1.5 Validation: `cargo fmt --all -- --check`; `cargo clippy -p unastar -p bds-sweep
  --all-targets -- -D warnings`; `cargo test -p unastar --lib --tests`; `cargo run -p parity --
  check`; `openspec validate --all --strict`; `bun tools/agents/check-links.ts`; the Python suite
