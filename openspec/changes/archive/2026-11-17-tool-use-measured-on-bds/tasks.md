# Tasks

- [x] 1.1 Rows are covered by lane ITEMS's existing "Item use" claim (`docs/parity/claims.toml`); no new claim
- [x] 1.2 `interact-named` in `bds-sweep-client`; `tool_use_script.py` and the capture on BDS 1.26.51
- [x] 1.3 `tool_use_fixture.py`: case windows and setups, wear and drop table; Python tests
- [x] 2.1 `item_use/copper.rs`; scraping, wax off and waxing with level events
- [x] 2.2 `item_use/on_block.rs`: the survival answer, predicted wear, snow layers, campfires, pumpkins
- [x] 2.3 `entity/mob/gear/drops.rs`: wear against the item's durability
- [x] 3.1 Replays: `tool_use_tests.rs` (cases, wear), `drops.rs` (spears)
- [x] 3.2 Update `docs/parity/items.toml`; `cargo run -p parity -- generate` and `-- check`
- [x] 3.3 Quirk rows, solution note
- [x] 3.4 Validation: `cargo fmt --all -- --check`, `cargo clippy -p unastar -p bds-sweep --all-targets -- -D warnings`, `cargo test -p unastar --lib --tests`, `cargo test -p bds-sweep`, `cargo run -p parity -- check`, `openspec validate --all --strict`, `bun tools/agents/check-links.ts`, `python -m unittest discover -s tools/bds-sweep -p "test_*.py"`
