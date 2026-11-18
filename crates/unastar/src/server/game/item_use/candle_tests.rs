//! Candles and candle cakes lit against BDS 1.26.51
//! (`crates/unastar-data/data/measured/candles/candles_lit.json`, the
//! `candles_lit` family of `tools/bds-sweep/candles.py`): a creative player
//! clicks the top of every candle colour and candle cake, one to four
//! candles, with flint and steel or a fire charge, and a candle already lit.
//! The block afterwards, the sounds and the level events are compared.

use glam::IVec3;
use jolyne::valentine::McpePacketData;
use serde::Deserialize;

use crate::entity::components::GameMode;
use crate::registry::BlockRegistry;
use crate::registry::block::StateValue;
use crate::server::game::capture_replay::Replay;

#[derive(Debug, Deserialize)]
struct Table {
    rows: Vec<Row>,
}

#[derive(Debug, Deserialize)]
struct Row {
    block: String,
    item: String,
    before: serde_json::Map<String, serde_json::Value>,
    after: (String, serde_json::Map<String, serde_json::Value>),
    sounds: Vec<(String, [f64; 3], i32)>,
    events: Vec<(i32, [f64; 3], i32)>,
}

/// Where the candle stands (on the replay's stone floor).
const CLICKED: IVec3 = IVec3::new(2, -60, 0);

/// The state of `block` with `states` (as the sweep wrote them).
fn state(block: &str, states: &serde_json::Map<String, serde_json::Value>) -> u32 {
    let entry = BlockRegistry::vanilla()
        .get_by_name(&format!("minecraft:{block}"))
        .unwrap_or_else(|| panic!("{block}"));
    states
        .iter()
        .fold(entry.default_state_id, |state, (name, value)| {
            let value = match value {
                serde_json::Value::Bool(bit) => StateValue::Byte(u8::from(*bit)),
                serde_json::Value::Number(n) => StateValue::Int(n.as_i64().unwrap() as i32),
                other => panic!("{other}"),
            };
            entry.with_state_value(state, name, value).unwrap()
        })
}

#[test]
fn candles_light_like_bds() {
    let path = format!(
        "{}/../unastar-data/data/measured/candles/candles_lit.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let rows = serde_json::from_str::<Table>(&text).unwrap().rows;
    assert_eq!(rows.len(), 40);
    let mut h = Replay::new();
    let player = h.player;
    *h.world().get_mut::<GameMode>(player).unwrap() = GameMode::Creative;
    let mut failures = Vec::new();
    for row in &rows {
        h.clear();
        h.put(
            0,
            crate::item::test_support::stack(&format!("minecraft:{}", row.item), 1),
        );
        h.select(0);
        crate::server::game::blocks::set_block(
            h.world(),
            CLICKED,
            state(&row.block, &row.before),
            false,
        );
        h.tick();
        h.drain();
        super::on_block::use_on_block(h.world(), player, CLICKED, 1);
        h.tick();
        let mut sounds = Vec::new();
        let mut events = Vec::new();
        for packet in h.drain() {
            match packet {
                McpePacketData::LevelSoundEventPacket(sound) => {
                    let p = sound.position;
                    sounds.push((
                        sound.sound_event.to_string(),
                        [p.x as f64, p.y as f64, p.z as f64],
                        sound.data,
                    ));
                }
                McpePacketData::LevelEventPacket(event) => {
                    let p = event.position;
                    events.push((
                        event.event_id,
                        [p.x as f64, p.y as f64, p.z as f64],
                        event.data,
                    ));
                }
                _ => {}
            }
        }
        let corner = CLICKED.as_dvec3();
        let relative = |p: [f64; 3]| {
            [p[0] - corner.x, p[1] - corner.y, p[2] - corner.z].map(|v| (v * 1000.0).round() as i64)
        };
        let theirs_sounds: Vec<_> = row
            .sounds
            .iter()
            .map(|(name, p, data)| (name.clone(), p.map(|v| (v * 1000.0).round() as i64), *data))
            .collect();
        let ours_sounds: Vec<_> = sounds
            .into_iter()
            .map(|(name, p, data)| (name, relative(p), data))
            .collect();
        let theirs_events: Vec<_> = row
            .events
            .iter()
            .map(|(id, p, data)| (*id, p.map(|v| (v * 1000.0).round() as i64), *data))
            .collect();
        let ours_events: Vec<_> = events
            .into_iter()
            .map(|(id, p, data)| (id, relative(p), data))
            .collect();
        let after = state(&row.after.0, &row.after.1);
        let ours_after =
            crate::server::game::blocks::block_at(h.world(), (CLICKED.x, CLICKED.y, CLICKED.z));
        if ours_after != Some(after) || ours_sounds != theirs_sounds || ours_events != theirs_events
        {
            failures.push(format!(
                "{} {} {:?}: BDS {:?} {theirs_sounds:?} {theirs_events:?}, unastar {:?} {ours_sounds:?} {ours_events:?}",
                row.block,
                row.item,
                row.before,
                row.after,
                ours_after.and_then(|s| BlockRegistry::vanilla().get_by_runtime_id(s)).map(|b| b.string_id.clone())
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
