//! Shields and the totem of undying against BDS 1.26.51.
//!
//! `testdata/bds-shield.jsonl` and `testdata/bds-totem.jsonl` (with their
//! `-cases.json`) are cut (`tools/bds-sweep/items6_table.py cases`) from
//! `bds-sweep-client` captures of `tools/bds-sweep/scripts/items-6-shield.txt`
//! and `items-6-totem.txt` played against plain BDS 1.26.51 (2026-11-18).
//! Every client packet of the part, its commands included, is routed here
//! at its input tick, the server ticking once per input, and BDS's answers
//! are compared case by case, kind by kind:
//!
//! - the shield's block sound and the disabling break sound;
//! - the player's actor events (hurt, totem);
//! - item cooldowns;
//! - the player's effects added and removed (effect, amplifier, duration);
//! - the off hand after the case (item and damage);
//! - the player's health after the case (BDS's last health update, carried
//!   from case to case; ours from the world: BDS also resends a full health
//!   that the script's heal leaves unchanged, which this server does not).
//!
//! Shield cases: a pig in front of (or behind) a sneaking player with a
//! shield hit by `/damage … entity` the pig for 1-12, standing, a main-hand
//! shield, projectile and fire damage, and hits 1-8 inputs after the sneak
//! started. The vindicator cases (its AI picks the hit ticks) are not
//! replayed; the axe disabling they measured is checked by
//! `an_axe_disables_a_blocking_shield`. Totem cases: off hand, main hand,
//! both, a hit it survives anyway, a pop with effects held.

use std::collections::{BTreeMap, BTreeSet};

use jolyne::valentine::types::EnumsActorEvent;
use jolyne::valentine::{McpePacket, McpePacketData};
use serde::Deserialize;

use crate::server::game::capture_replay::{BDS_PLAYER, Record, Replay, SESSION, records};
use crate::world::rules::Difficulty;

#[derive(Deserialize)]
struct Case {
    name: String,
    inputs: [u64; 2],
}

#[derive(Debug, Clone, PartialEq)]
enum Event {
    /// Name and position in tenths.
    Sound(String, [i32; 3]),
    Actor(EnumsActorEvent),
    Cooldown(String, i32),
    /// Add or remove, effect, amplifier, duration (an add's; a remove's
    /// remaining ticks depend on when the script's `/effect @s clear` ran,
    /// so they are not compared).
    Effect(bool, i32, i32, Option<i32>),
    /// Item id and damage of the off hand.
    Offhand(i32, i32),
    /// Health in tenths.
    Health(i32),
}

impl Event {
    fn kind(&self) -> &'static str {
        match self {
            Event::Sound(..) => "sounds",
            Event::Actor(..) => "actor events",
            Event::Cooldown(..) => "cooldowns",
            Event::Effect(..) => "effects",
            Event::Offhand(..) => "off hand",
            Event::Health(..) => "health",
        }
    }
}

fn tenths(x: f32, y: f32, z: f32) -> [i32; 3] {
    [x, y, z].map(|v| (v * 10.0).round() as i32)
}

/// The `Damage` of a stack's user data, 0 without one.
fn damage(user_data: &[u8]) -> i32 {
    user_data
        .windows(6)
        .position(|name| name == b"Damage")
        .map(|start| start + 6)
        .and_then(|at| user_data.get(at..at + 4)?.try_into().ok())
        .map_or(0, i32::from_le_bytes)
}

fn event(packet: &McpePacketData, player: u64) -> Option<Event> {
    match packet {
        McpePacketData::LevelSoundEventPacket(sound)
            if ["item.shield.block", "break"].contains(&sound.sound_event.as_ref()) =>
        {
            let p = &sound.position;
            Some(Event::Sound(
                sound.sound_event.to_string(),
                tenths(p.x, p.y, p.z),
            ))
        }
        McpePacketData::ActorEventPacket(actor)
            if actor.target_runtime_id.actor_runtime_id == player
                && matches!(
                    actor.event_id,
                    EnumsActorEvent::Hurt | EnumsActorEvent::TalismanActivate
                ) =>
        {
            Some(Event::Actor(actor.event_id))
        }
        McpePacketData::PlayerStartItemCooldownPacket(cooldown) => Some(Event::Cooldown(
            cooldown.item_category.to_string(),
            cooldown.duration_ticks,
        )),
        McpePacketData::MobEffectPacket(effect)
            if effect.target_runtime_id.actor_runtime_id == player =>
        {
            let add =
                effect.event_id == jolyne::valentine::types::EnumsMobEffectPacketPayloadEvent::Add;
            Some(Event::Effect(
                add,
                effect.effect_id,
                effect.effect_amplifier,
                add.then_some(effect.effect_duration_ticks),
            ))
        }
        McpePacketData::InventoryContentPacket(content) if content.container_id == 119 => {
            let slot = content.slots.first()?;
            Some(Event::Offhand(
                i32::from(slot.id),
                damage(&slot.user_data_buffer),
            ))
        }
        McpePacketData::UpdateAttributesPacket(update)
            if update.target_runtime_id.actor_runtime_id == player =>
        {
            update
                .attribute_list
                .iter()
                .rev()
                .find(|attribute| attribute.name == "minecraft:health")
                .map(|health| Event::Health((health.current_value * 10.0).round() as i32))
        }
        _ => None,
    }
}

/// Events by kind; the off hand only as it ended, without health updates.
fn by_kind(events: impl Iterator<Item = Event>) -> BTreeMap<&'static str, Vec<Event>> {
    let mut out: BTreeMap<&'static str, Vec<Event>> = BTreeMap::new();
    for event in events.filter(|event| !matches!(event, Event::Health(_))) {
        out.entry(event.kind()).or_default().push(event);
    }
    if let Some(events) = out.get_mut("off hand") {
        let last = events.pop();
        events.clear();
        events.extend(last);
    }
    out
}

/// Replay `fixture`'s cases, contiguously, from a fresh server at
/// `difficulty`; the failures.
fn replay(fixture: &str, cases: &str, difficulty: Difficulty) -> Vec<String> {
    let records = records(fixture);
    let cases: Vec<Case> = serde_json::from_str(cases).unwrap();
    let mut h = Replay::with_difficulty(difficulty);
    let mut failures = Vec::new();
    let mut next = 0;
    let client: Vec<&Record> = records.iter().filter(|r| r.from_client).collect();
    let mut bds_health = 200;
    for case in &cases {
        let inputs = case.inputs[0]..case.inputs[1];
        let bds_events: Vec<Event> = records
            .iter()
            .filter(|record| !record.from_client && inputs.contains(&record.input))
            .filter_map(|record| event(&record.packet, BDS_PLAYER))
            .collect();
        if let Some(Event::Health(health)) = bds_events
            .iter()
            .rev()
            .find(|event| matches!(event, Event::Health(_)))
        {
            bds_health = *health;
        }
        let bds = by_kind(bds_events.into_iter());
        let mut ours = Vec::new();
        for input in inputs {
            while next < client.len() && client[next].input < input {
                h.route(McpePacket::from(client[next].packet.clone()));
                next += 1;
            }
            h.tick();
            ours.extend(h.drain().iter().filter_map(|p| event(p, SESSION)));
        }
        let ours = by_kind(ours.into_iter());
        let player = h.player;
        let our_health = h
            .world()
            .get::<crate::entity::components::Health>(player)
            .map_or(-1, |health| (health.current * 10.0).round() as i32);
        if our_health != bds_health {
            failures.push(format!(
                "{}: health BDS {bds_health}, ours {our_health} (tenths)",
                case.name
            ));
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
    }
    failures
}

#[test]
fn shields_block_like_bds() {
    let failures = replay(
        include_str!("testdata/bds-shield.jsonl"),
        include_str!("testdata/bds-shield-cases.json"),
        Difficulty::Easy,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn totems_save_like_bds() {
    let failures = replay(
        include_str!("testdata/bds-totem.jsonl"),
        include_str!("testdata/bds-totem-cases.json"),
        Difficulty::Peaceful,
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
