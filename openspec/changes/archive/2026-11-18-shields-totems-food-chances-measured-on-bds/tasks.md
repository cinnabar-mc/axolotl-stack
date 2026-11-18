# Tasks

- [x] 1.1 Rows are covered by lane ITEMS's existing "Item use" claim (`docs/parity/claims.toml`); no new claim
- [x] 1.2 `items6_script.py`; food, shield and totem captures on BDS 1.26.51
- [x] 1.3 `items6_table.py`: food chance table, shield and totem replay cuts; Python tests
- [x] 2.1 `item_use/food.rs`: `effect_lands`
- [x] 2.2 `item_use/shield.rs`, `item_use/totem.rs`; hooks in `player_life/hurt.rs`
- [x] 3.1 Tests: `food_tests.rs::chance_food_effects_land_as_often_as_on_bds`, `shield_totem_tests.rs`, `shield.rs` unit tests
- [x] 3.2 Update `docs/parity/items.toml`; `cargo run -p parity -- generate` and `-- check`
- [x] 3.3 Quirk rows
- [x] 3.4 Validation: `cargo fmt --all -- --check`, `cargo clippy -p unastar --all-targets -- -D warnings`, `cargo test -p unastar --lib --tests`, `cargo run -p parity -- check`, `openspec validate --all --strict`, `bun tools/agents/check-links.ts`, `python -m unittest discover -s tools/bds-sweep -p "test_*.py"`
