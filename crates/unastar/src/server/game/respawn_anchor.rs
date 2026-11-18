//! Respawn anchors, as BDS 1.26.51 does them in the overworld
//! (`crates/unastar-data/data/measured/explosions/tnt_anchor.json`, the
//! `tnt_anchor` family of `tools/bds-sweep/tnt.py`):
//!
//! - glowstone on an anchor with fewer than four charges adds one (the
//!   glowstone is used up outside creative) and plays
//!   `respawn_anchor.charge` at the block's centre;
//! - any other click on a charged anchor, or glowstone on a full one, while
//!   `respawnBlocksExplode` is on, removes the anchor and explodes at the
//!   block's centre: power 5, breaking blocks and setting fire, with no
//!   source actor (a block explosion that keeps one drop in five);
//! - an empty anchor ignores everything but glowstone.
//!
//! unastar runs only the overworld (`command::context::Dimension`), so an
//! anchor never sets a spawn point here.

use bevy_ecs::prelude::*;
use glam::{DVec3, IVec3};
use jolyne::valentine::{LevelSoundEventPacket, McpePacket, Vec3};

use super::block_actors;
use super::explosion::{Blast, explode};
use super::item_use::{held, set_slot};
use crate::entity::components::GameMode;
use crate::entity::systems::BlockSource;
use crate::item::ItemStack;
use crate::registry::block::StateValue;
use crate::world::rules::GameRules;

/// The anchor's charge property and its most charges.
const CHARGE: &str = "respawn_anchor_charge";
const MAX_CHARGE: i32 = 4;
/// The power of an anchor's explosion (BDS: the explosion particles' data).
const EXPLOSION_POWER: f32 = 5.0;
const CHARGE_SOUND: &str = "respawn_anchor.charge";
const EXPLOSION_SOUND: &str = "explode";

/// The anchor and glowstone, resolved against the registries once.
#[derive(Resource, Debug, Clone)]
struct AnchorIds {
    /// The anchor's block states (first, last).
    anchor: (u32, u32),
    air: u32,
    /// A glowstone stack, for its item type.
    glowstone: ItemStack,
}

impl AnchorIds {
    fn of(world: &mut World) -> Option<Self> {
        if let Some(ids) = world.get_resource::<Self>() {
            return Some(ids.clone());
        }
        let registry = world.registry();
        let entry = registry.get_by_name("minecraft:respawn_anchor")?;
        let ids = Self {
            anchor: (entry.min_state_id, entry.max_state_id),
            air: registry.default_state_id_by_name("minecraft:air")?,
            glowstone: super::world_stack(world, "minecraft:glowstone", 1)?,
        };
        world.insert_resource(ids.clone());
        Some(ids)
    }

    fn is_anchor(&self, state: u32) -> bool {
        (self.anchor.0..=self.anchor.1).contains(&state)
    }
}

/// Whether the held stack is glowstone.
fn holds_glowstone(world: &World, player: Entity, ids: &AnchorIds) -> bool {
    held(world, player).is_some_and(|(_, stack)| stack.comparable(&ids.glowstone))
}

/// A player clicked `pos`: `Some(false)` when it is a respawn anchor that
/// reacted (charged or exploded), `None` otherwise (the click goes on).
pub(crate) fn used(world: &mut World, player: Entity, pos: IVec3) -> Option<bool> {
    let ids = AnchorIds::of(world)?;
    let state = world.block_at(pos).filter(|state| ids.is_anchor(*state))?;
    let entry = world.registry().get_by_runtime_id(state)?.clone();
    let charge = match entry.state_value(state, CHARGE) {
        Some(StateValue::Int(charge)) => charge,
        _ => return None,
    };
    if charge < MAX_CHARGE && holds_glowstone(world, player, &ids) {
        let charged = entry.with_state_value(state, CHARGE, StateValue::Int(charge + 1))?;
        super::blocks::set_block(world, pos, charged, false);
        charge_sound(world, pos);
        if world.get::<GameMode>(player) != Some(&GameMode::Creative)
            && let Some((slot, stack)) = held(world, player)
        {
            set_slot(world, player, slot, stack.grow(-1));
        }
        return Some(false);
    }
    let explodes = world
        .get_resource::<GameRules>()
        .is_none_or(|rules| rules.respawn_blocks_explode);
    if charge == 0 || !explodes {
        return None;
    }
    super::blocks::set_block(world, pos, ids.air, false);
    explode(world, &anchor_blast(pos));
    Some(false)
}

/// The explosion of the anchor at `pos`.
pub(crate) fn anchor_blast(pos: IVec3) -> Blast {
    Blast {
        centre: pos.as_dvec3() + DVec3::splat(0.5),
        damage_centre: pos.as_dvec3() + DVec3::splat(0.5),
        power: EXPLOSION_POWER,
        source: None,
        breaks_blocks: true,
        causes_fire: true,
        max_resistance: f32::MAX,
        damage_scaling: 1.0,
        knockback_scaling: 1.0,
        drop_chance: Blast::decayed_drops(EXPLOSION_POWER),
        sound: EXPLOSION_SOUND.into(),
    }
}

fn charge_sound(world: &World, pos: IVec3) {
    block_actors::broadcast(
        world,
        pos,
        &McpePacket::from(LevelSoundEventPacket {
            sound_event: CHARGE_SOUND.into(),
            position: Vec3 {
                x: pos.x as f32 + 0.5,
                y: pos.y as f32 + 0.5,
                z: pos.z as f32 + 0.5,
            },
            data: -1,
            actor_identifier: "".into(),
            is_baby: false,
            is_global: false,
            actor_unique_id: -1,
            fire_at_position: None,
        }),
        None,
    );
}

#[cfg(test)]
mod tests;
