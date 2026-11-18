//! The totem of undying, measured on BDS 1.26.51 (`tools/bds-sweep/scripts/items-6-totem.txt`;
//! `item_use/testdata/bds-totem.jsonl`, replayed by `shield_totem_tests.rs`).
//!
//! A hit that would kill a player holding a totem in the off hand or the
//! selected hotbar slot (the off hand's is used first) leaves it at 1
//! health instead: the totem is used up, every effect is removed, then
//! Regeneration II (900 ticks), Absorption II (100) and Fire Resistance
//! (800) are given, and `ActorEvent` `TalismanActivate` goes to the player
//! and its viewers before the hit's hurt event. Void damage and `/kill`
//! (override damage) kill through a totem; a totem elsewhere in the
//! inventory does nothing.

use bevy_ecs::prelude::*;
use jolyne::valentine::{ActorEventPacket, ActorRuntimeId, EnumsActorEvent, McpePacket};
use unastar_data::actor_components::body::ActorDamageCause;

use crate::entity::components::{
    EffectType, Health, HeldSlot, MainInventory, OffhandSlot, RuntimeEntityId,
};
use crate::item::ItemStack;
use crate::server::game::effects;

const TOTEM: &str = "minecraft:totem_of_undying";

/// The effects a used totem gives: type, amplifier, ticks.
pub const TOTEM_EFFECTS: [(EffectType, u8, u32); 3] = [
    (EffectType::REGENERATION, 1, 900),
    (EffectType::ABSORPTION, 1, 100),
    (EffectType::FIRE_RESISTANCE, 0, 800),
];

/// Where the totem that saves a player is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotemHand {
    Offhand,
    /// The selected hotbar slot.
    Main(usize),
}

/// Whether a totem can save a player from a hit of `cause`.
pub fn saves_from(cause: ActorDamageCause) -> bool {
    !matches!(cause, ActorDamageCause::Void | ActorDamageCause::Override)
}

/// The hand holding the totem that would be used, if any.
pub fn held_totem(world: &World, player: Entity) -> Option<TotemHand> {
    let is_totem = |stack: &ItemStack| !stack.is_empty() && stack.item_id() == TOTEM;
    if world
        .get::<OffhandSlot>(player)
        .is_some_and(|offhand| is_totem(&offhand.0))
    {
        return Some(TotemHand::Offhand);
    }
    let slot = usize::from(world.get::<HeldSlot>(player)?.0);
    world
        .get::<MainInventory>(player)?
        .0
        .item(slot)
        .is_some_and(is_totem)
        .then_some(TotemHand::Main(slot))
}

/// Save `player`, whom a hit of `cause` would kill, with a held totem.
/// Returns whether it was saved (its health is then 1).
pub fn revive(world: &mut World, player: Entity, cause: ActorDamageCause) -> bool {
    if !saves_from(cause) {
        return false;
    }
    let Some(hand) = held_totem(world, player) else {
        return false;
    };
    if let Some(mut health) = world.get_mut::<Health>(player) {
        health.current = 1.0;
    }
    effects::clear_effects(world, player);
    for (effect, amplifier, ticks) in TOTEM_EFFECTS {
        effects::add_effect(world, player, effect, amplifier, ticks);
    }
    if let Some(runtime_id) = world.get::<RuntimeEntityId>(player) {
        let packet = McpePacket::from(ActorEventPacket {
            target_runtime_id: ActorRuntimeId {
                actor_runtime_id: runtime_id.0 as u64,
            },
            event_id: EnumsActorEvent::TalismanActivate,
            data: 0,
            fire_at_position: None,
        });
        crate::entity::player_life::send_to_player_and_viewers(world, player, &packet);
    }
    match hand {
        TotemHand::Offhand => {
            if let Some(mut offhand) = world.get_mut::<OffhandSlot>(player) {
                offhand.0 = ItemStack::empty();
            }
            crate::server::game::inventory::send_offhand(world, player);
        }
        TotemHand::Main(slot) => {
            let empty = ItemStack::empty();
            if let Some(mut inventory) = world.get_mut::<MainInventory>(player) {
                let _ = inventory.0.set_item(slot, empty.clone());
            }
            crate::server::game::send_slot(world, player, slot, &empty);
        }
    }
    true
}
