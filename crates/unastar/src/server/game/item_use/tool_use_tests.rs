//! Tools, honeycomb and shears used on blocks, against BDS 1.26.51.
//!
//! `testdata/bds-tool-use.jsonl` and `testdata/bds-tool-use-cases.json` are
//! cut (`tools/bds-sweep/tool_use_fixture.py cases`) from a
//! `bds-sweep-client` capture of `tools/bds-sweep/scripts/tool-use.txt`
//! played against plain BDS 1.26.51 (2026-11-17): a survival or creative
//! player clicks a block at (2,-60,0) (or the bottom of one at (2,-58,0))
//! once with a hoe, shovel, axe, honeycomb or shears. Each case is set up as
//! the script did, the client's packets are routed at their input ticks and
//! BDS's answers are compared kind by kind: block updates near the block
//! (a repeat right after itself dropped), sounds, level events and dropped
//! items, and the held stack's wear and count after the click (BDS shows
//! them in the inventory it resends after a use that did something).
//!
//! `testdata/bds-tool-use-wear.json` (`tool_use_fixture.py table`) counts
//! the wear of repeated clicks with Unbreaking 0 and III and the spears
//! husks dropped; its tests are next to the code they check.

use std::collections::{BTreeMap, BTreeSet};

use glam::IVec3;
use jolyne::valentine::{McpePacket, McpePacketData};
use unastar_data::blocks::StateValue;

use crate::entity::components::{GameMode, MainInventory};
use crate::item::enchant::durability_damage;
use crate::registry::BlockRegistry;
use crate::server::game::capture_replay::{Record, Replay, records};

const BDS: &str = include_str!("testdata/bds-tool-use.jsonl");
const CASES: &str = include_str!("testdata/bds-tool-use-cases.json");

/// Cases whose block's own use answers instead of the tool, which belong to
/// those blocks' rows: only the held stack is compared (the axe must not
/// wear). BDS opened the doors without sending their blocks, and opened the
/// copper chest, which this server does not open yet (the axe leaves it).
const BLOCK_USES: [&str; 3] = [
    "axe_weathered_copper_door",
    "axe_oxidized_copper_trapdoor",
    "axe_waxed_exposed_copper_chest",
];

/// A case as the script set it up.
#[derive(serde::Deserialize)]
struct Case {
    name: String,
    item: String,
    count: u8,
    mode: String,
    block: Block,
    above: Option<Block>,
    inputs: [u64; 2],
}

#[derive(serde::Deserialize)]
struct Block {
    pos: [i32; 3],
    name: String,
    states: BTreeMap<String, serde_json::Value>,
}

impl Block {
    /// The state the script's `/setblock` put: its named states over the
    /// block's default.
    fn state(&self) -> u32 {
        let entry = BlockRegistry::vanilla()
            .get_by_name(&self.name)
            .unwrap_or_else(|| panic!("{}", self.name));
        let mut state = entry.default_state_id;
        for (name, value) in &self.states {
            let property = entry
                .data
                .properties
                .iter()
                .find(|property| property.name == name)
                .unwrap_or_else(|| panic!("{} has no {name}", self.name));
            let value = *property
                .values
                .iter()
                .find(|candidate| match (candidate, value) {
                    (StateValue::Byte(v), serde_json::Value::Bool(b)) => (*v == 1) == *b,
                    (StateValue::Int(v), serde_json::Value::Number(n)) => {
                        n.as_i64() == Some(i64::from(*v))
                    }
                    (StateValue::Str(v), serde_json::Value::String(s)) => v == s,
                    _ => false,
                })
                .unwrap_or_else(|| panic!("{}: {name}={value}", self.name));
            state = entry.with_state_value(state, name, value).unwrap();
        }
        state
    }
}

/// What is compared, near the clicked block only.
#[derive(Debug, Clone, PartialEq)]
enum Event {
    Block([i32; 3], u32),
    /// Name, position in tenths and data.
    Sound(String, [i32; 3], i32),
    /// Event id and position in tenths.
    Level(i32, [i32; 3]),
    /// A dropped item's network id and count.
    Drop(i16, u16),
}

impl Event {
    fn kind(&self) -> &'static str {
        match self {
            Event::Block(..) => "block updates",
            Event::Sound(..) => "sounds",
            Event::Level(..) => "level events",
            Event::Drop(..) => "dropped items",
        }
    }
}

fn near(x: i32, y: i32, z: i32) -> bool {
    (1..=3).contains(&x) && (-62..=-56).contains(&y) && (-1..=1).contains(&z)
}

fn tenths(x: f32, y: f32, z: f32) -> [i32; 3] {
    [x, y, z].map(|v| (v * 10.0).round() as i32)
}

fn event(packet: &McpePacketData) -> Option<Event> {
    match packet {
        McpePacketData::UpdateBlockPacket(update) => {
            let p = &update.block_position;
            near(p.x, p.y, p.z).then_some(Event::Block([p.x, p.y, p.z], update.block_runtime_id))
        }
        McpePacketData::LevelSoundEventPacket(sound) if sound.sound_event != "pop" => {
            let p = &sound.position;
            Some(Event::Sound(
                sound.sound_event.to_string(),
                tenths(p.x, p.y, p.z),
                sound.data,
            ))
        }
        McpePacketData::LevelEventPacket(level) => {
            let p = &level.position;
            near(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
                .then_some(Event::Level(level.event_id, tenths(p.x, p.y, p.z)))
        }
        McpePacketData::AddItemActorPacket(item) => {
            Some(Event::Drop(item.item.id, item.item.stacksize))
        }
        _ => None,
    }
}

fn by_kind(events: impl Iterator<Item = Event>) -> BTreeMap<&'static str, Vec<Event>> {
    let mut out: BTreeMap<&'static str, Vec<Event>> = BTreeMap::new();
    for event in events {
        out.entry(event.kind()).or_default().push(event);
    }
    if let Some(blocks) = out.get_mut("block updates") {
        blocks.dedup();
    }
    out
}

/// The held stack's `Damage` and count in the last inventory BDS resent in
/// the window, if it resent one: the user data's little-endian int after
/// the tag name.
fn bds_held(window: &[&Record]) -> Option<(i32, u16)> {
    window.iter().rev().find_map(|record| match &record.packet {
        McpePacketData::InventoryContentPacket(content) if content.container_id == 0 => {
            let slot = content.slots.first()?;
            let user_data = &slot.user_data_buffer;
            let damage = user_data
                .windows(6)
                .position(|name| name == b"Damage")
                .map(|start| start + 6)
                .and_then(|at| user_data.get(at..at + 4)?.try_into().ok())
                .map_or(0, i32::from_le_bytes);
            Some((damage, slot.stacksize))
        }
        _ => None,
    })
}

#[test]
fn tools_used_on_blocks_answer_like_bds() {
    let records = records(BDS);
    let cases: Vec<Case> = serde_json::from_str(CASES).unwrap();
    assert!(cases.len() > 60, "{}", cases.len());
    let mut h = Replay::new();
    let mut failures = Vec::new();
    for case in &cases {
        h.clear();
        for y in -60..=-56 {
            for x in 1..=3 {
                for z in -1..=1 {
                    h.set(IVec3::new(x, y, z), "minecraft:air");
                }
            }
        }
        let player = h.player;
        *h.world().get_mut::<GameMode>(player).unwrap() = match case.mode.as_str() {
            "creative" => GameMode::Creative,
            _ => GameMode::Survival,
        };
        h.put(0, crate::item::test_support::stack(&case.item, case.count));
        h.select(0);
        for block in std::iter::once(&case.block).chain(&case.above) {
            crate::server::game::blocks::set_block(
                h.world(),
                IVec3::from_array(block.pos),
                block.state(),
                false,
            );
        }
        h.tick();
        h.drain();
        let inputs = case.inputs[0]..case.inputs[1];
        let window: Vec<&Record> = records
            .iter()
            .filter(|record| inputs.contains(&record.input))
            .collect();
        let bds = by_kind(
            window
                .iter()
                .filter(|record| !record.from_client)
                .filter_map(|record| event(&record.packet)),
        );
        let client: Vec<&&Record> = window.iter().filter(|record| record.from_client).collect();
        let mut ours = Vec::new();
        let mut next = 0;
        for input in inputs {
            while next < client.len() && client[next].input < input {
                h.route(McpePacket::from(client[next].packet.clone()));
                next += 1;
            }
            h.tick();
            ours.extend(h.drain().iter().filter_map(event));
        }
        let ours = by_kind(ours.into_iter());
        let held = h
            .world()
            .get::<MainInventory>(player)
            .unwrap()
            .hotbar(0)
            .cloned()
            .unwrap_or_default();
        let ours_held = (durability_damage(&held), u16::from(held.count));
        let bds_held = bds_held(&window).unwrap_or((0, u16::from(case.count)));
        if ours_held != bds_held {
            failures.push(format!(
                "{}: held (damage, count) BDS {bds_held:?}, ours {ours_held:?}",
                case.name
            ));
        }
        if BLOCK_USES.contains(&case.name.as_str()) {
            *h.world().get_mut::<GameMode>(player).unwrap() = GameMode::Survival;
            continue;
        }
        for kind in bds.keys().chain(ours.keys()).collect::<BTreeSet<_>>() {
            if bds.get(kind) != ours.get(kind) {
                failures.push(format!(
                    "{}, {kind}:\n  BDS  {:?}\n  ours {:?}",
                    case.name,
                    bds.get(kind),
                    ours.get(kind)
                ));
            }
        }
        // Leave survival for the next case's setup.
        *h.world().get_mut::<GameMode>(player).unwrap() = GameMode::Survival;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `testdata/bds-tool-use-wear.json` (its spear drops are compared in
/// `entity/mob/gear/drops.rs`).
#[derive(serde::Deserialize)]
struct BdsWear {
    wear: BTreeMap<String, BTreeMap<u8, Uses>>,
}

/// Uses and how many wore the item by 0, 1 and 2 points.
#[derive(serde::Deserialize)]
struct Uses {
    uses: u32,
    worn: [u32; 3],
}

fn bds_wear() -> BdsWear {
    serde_json::from_str(include_str!("testdata/bds-tool-use-wear.json")).unwrap()
}

/// Repeated uses with netherite tools and shears, Unbreaking 0 and III:
/// each use is one wear event of 1 point with the tool chance, as BDS wore
/// them stripping oak logs, tilling dirt, flattening grass and shearing
/// sheep. Rolled from a seeded RNG through
/// [`crate::server::game::durability::wear`].
#[test]
fn tool_use_wear_matches_bds() {
    use rand::SeedableRng;

    use crate::item::enchant::with_enchantments;
    use crate::server::game::durability::{same_share, wear};
    use unastar_rules::ids::Enchantment;

    let bds = bds_wear();
    let mut rng = rand::rngs::StdRng::seed_from_u64(5);
    let rolls = 20_000;
    let mut failures = Vec::new();
    for (kind, item) in [
        ("axe", "minecraft:netherite_axe"),
        ("hoe", "minecraft:netherite_hoe"),
        ("shovel", "minecraft:netherite_shovel"),
        ("shears", "minecraft:shears"),
    ] {
        let levels = &bds.wear[kind];
        assert_eq!(levels.len(), 2, "{kind}");
        for (&level, row) in levels {
            let mut stack = crate::item::test_support::stack(item, 1);
            if level > 0 {
                stack = with_enchantments(&stack, [(Enchantment::Unbreaking, level)]);
            }
            let mut ours = [0u32; 3];
            for _ in 0..rolls {
                let worn = wear(&stack, 1, &mut rng).map_or(0, |out| durability_damage(&out));
                ours[worn as usize] += 1;
            }
            for (points, (&ours, &theirs)) in ours.iter().zip(&row.worn).enumerate() {
                if !same_share(ours, rolls, theirs, row.uses) {
                    failures.push(format!(
                        "{kind} {level}, {points} points: ours {ours}/{rolls}, BDS {theirs}/{}",
                        row.uses
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures.join(
            "
"
        )
    );
}
