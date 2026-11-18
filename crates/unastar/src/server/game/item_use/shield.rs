//! Shields, measured on BDS 1.26.51 (`tools/bds-sweep/scripts/items-6-shield.txt`;
//! `item_use/testdata/bds-shield.jsonl`, replayed by `shield_totem_tests.rs`).
//!
//! A player blocks while it sneaks holding a shield in the off hand or the
//! selected hotbar slot, from the sixth input of the sneak on (a hit 6 inputs
//! after the sneak started was blocked in one run and not in another, 8 in
//! both; 5 never), unless the shield is cooling down. It blocks an
//! `entity_attack` or `projectile` hit from an attacker in front of it (on
//! the side it faces: where the hit comes from, else the attacker); hits
//! from behind and other causes (fire) are not
//! blocked. A blocked hit deals nothing (no hurt event) and plays
//! `item.shield.block` at the player's eyes. An off-hand shield wears by 1
//! plus the hit's whole points before difficulty scaling (1 → 2, 4 → 5, 12 →
//! 13; a vindicator's iron-axe hit of 13 → 14), Unbreaking applying; BDS
//! sent no wear for a blocking main-hand shield.
//!
//! A blocked hit from an attacker holding an axe also disables the shield:
//! `PlayerStartItemCooldown` `minecraft:shield` for 100 ticks and the
//! `break` sound 0.745 above the player's feet; the hits after it land.

use bevy_ecs::prelude::*;
use glam::DVec3;
use unastar_data::actor_components::body::ActorDamageCause;

use crate::entity::components::{
    HeldSlot, MainInventory, OffhandSlot, PlayerState, Position, Rotation,
};
use crate::entity::mob::combat::Hit;
use crate::item::ItemStack;
use crate::item::cooldown::{self, Cooldown};

const SHIELD: &str = "minecraft:shield";
/// The shield's cooldown category.
const SHIELD_CATEGORY: &str = "minecraft:shield";
/// Ticks an axe disables a shield for.
pub const DISABLED_TICKS: u32 = 100;
/// Inputs a sneak must last before the shield blocks.
pub const RAISE_INPUTS: u32 = 6;
/// Height of the disabling `break` sound above the feet.
const BREAK_SOUND_HEIGHT: f64 = 0.745;

/// Consecutive server ticks the player has been sneaking (one per input).
#[derive(Component, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SneakTicks(pub u32);

/// System: count each player's sneaking ticks after its input applied.
pub fn count_sneak_ticks(
    mut commands: Commands,
    mut players: Query<(Entity, &PlayerState, Option<&mut SneakTicks>)>,
) {
    for (player, state, ticks) in &mut players {
        match (state.sneaking, ticks) {
            (true, Some(mut ticks)) => ticks.0 = ticks.0.saturating_add(1),
            (true, None) => {
                commands.entity(player).insert(SneakTicks(1));
            }
            (false, Some(mut ticks)) if ticks.0 != 0 => ticks.0 = 0,
            _ => {}
        }
    }
}

/// Where a blocking shield is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShieldHand {
    Offhand,
    Main,
}

fn is_shield(stack: &ItemStack) -> bool {
    !stack.is_empty() && stack.item_id() == SHIELD
}

fn shield_hand(world: &World, player: Entity) -> Option<ShieldHand> {
    if world
        .get::<OffhandSlot>(player)
        .is_some_and(|offhand| is_shield(&offhand.0))
    {
        return Some(ShieldHand::Offhand);
    }
    let slot = usize::from(world.get::<HeldSlot>(player)?.0);
    world
        .get::<MainInventory>(player)?
        .0
        .item(slot)
        .is_some_and(is_shield)
        .then_some(ShieldHand::Main)
}

/// Whether `player` is raising a usable shield.
pub fn blocking(world: &World, player: Entity) -> bool {
    world
        .get::<SneakTicks>(player)
        .is_some_and(|ticks| ticks.0 >= RAISE_INPUTS)
        && shield_hand(world, player).is_some()
        && !cooldown::cooling(world, player, SHIELD_CATEGORY)
}

/// Whether `from` is on the side `player` faces (horizontally).
fn in_front(world: &World, player: Entity, from: DVec3) -> bool {
    let (Some(at), Some(rotation)) = (
        world.get::<Position>(player).map(|p| p.0),
        world.get::<Rotation>(player),
    ) else {
        return false;
    };
    let yaw = f64::from(rotation.yaw).to_radians();
    // Yaw 0 looks south (+z), -90 east (+x).
    let facing = DVec3::new(-yaw.sin(), 0.0, yaw.cos());
    let towards = DVec3::new(from.x - at.x, 0.0, from.z - at.z);
    facing.dot(towards) > 0.0
}

/// The wear a blocked hit of `damage` (before difficulty scaling) costs an
/// off-hand shield.
pub fn block_wear(damage: f32) -> i32 {
    1 + damage.max(0.0).floor() as i32
}

/// Block `hit` with `player`'s shield if it can: the shield wears, the
/// block sound plays, an attacker's axe disables the shield. Returns
/// whether the hit was blocked (then it deals nothing).
pub fn block(world: &mut World, player: Entity, hit: &Hit) -> bool {
    if !matches!(
        hit.cause,
        ActorDamageCause::EntityAttack | ActorDamageCause::Projectile
    ) || !blocking(world, player)
    {
        return false;
    }
    // Where the hit comes from: a projectile's impact, the attacker.
    let Some(from) = hit
        .knockback_from
        .or_else(|| world.get::<Position>(hit.attacker?).map(|p| p.0))
    else {
        return false;
    };
    if !in_front(world, player, from) {
        return false;
    }
    let Some(eyes) = world.get::<Position>(player).map(|p| p.0) else {
        return false;
    };
    crate::entity::mob::net::sound::play_level_sound(
        world,
        Some(player),
        eyes,
        "item.shield.block",
        -1,
    );
    if shield_hand(world, player) == Some(ShieldHand::Offhand) {
        wear_offhand(world, player, block_wear(hit.damage));
    }
    if hit.cause == ActorDamageCause::EntityAttack
        && hit
            .attacker
            .is_some_and(|attacker| attacker_holds_axe(world, attacker))
    {
        cooldown::start(
            world,
            player,
            &Cooldown {
                category: SHIELD_CATEGORY.to_owned(),
                ticks: DISABLED_TICKS,
                attack: false,
            },
        );
        let feet = eyes - DVec3::Y * crate::entity::PLAYER_EYE_HEIGHT;
        crate::entity::mob::net::sound::play_level_sound(
            world,
            Some(player),
            feet + DVec3::Y * BREAK_SOUND_HEIGHT,
            "break",
            -1,
        );
    }
    true
}

fn attacker_holds_axe(world: &World, attacker: Entity) -> bool {
    crate::entity::mob::events::held_item(world, attacker)
        .or_else(|| crate::entity::mob::gear::mainhand(world, attacker))
        .and_then(ItemStack::item)
        .is_some_and(|entry| entry.has_tag("minecraft:is_axe"))
}

fn wear_offhand(world: &mut World, player: Entity, points: i32) {
    let Some(shield) = world.get::<OffhandSlot>(player).map(|o| o.0.clone()) else {
        return;
    };
    let Some(worn) = crate::server::game::durability::wear(
        &shield,
        points,
        &mut world
            .get_resource_or_init::<crate::entity::EntityRandom>()
            .0,
    ) else {
        return;
    };
    if let Some(mut offhand) = world.get_mut::<OffhandSlot>(player) {
        offhand.0 = worn;
    }
    crate::server::game::inventory::send_offhand(world, player);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::mob::components::KnockbackPower;
    use crate::entity::mob::gear::{GearSlot, MobEquipment};
    use crate::item::test_support::stack;
    use crate::server::game::capture_replay::{FEET, Replay};

    fn axe_hit(attacker: Entity) -> Hit {
        Hit {
            damage: 13.0,
            cause: ActorDamageCause::EntityAttack,
            attacker: Some(attacker),
            by_player: false,
            knockback_from: None,
            knockback_power: KnockbackPower::DEFAULT,
        }
    }

    /// The vindicator capture (`scripts/items-6-shield.txt`): its iron-axe
    /// hit on a blocking shield was blocked, wore it by 14 and disabled it
    /// for 100 ticks; its next hits landed.
    #[test]
    fn an_axe_disables_a_blocking_shield() {
        let mut h = Replay::new();
        let player = h.player;
        h.world().get_mut::<OffhandSlot>(player).unwrap().0 = stack(SHIELD, 1);
        h.world()
            .entity_mut(player)
            .insert(SneakTicks(RAISE_INPUTS));
        let mut equipment = MobEquipment::default();
        equipment.set(GearSlot::Mainhand, stack("minecraft:iron_axe", 1), false);
        // Two blocks east, in front of the player (facing east).
        let front = h
            .world()
            .spawn((Position(FEET + DVec3::new(2.0, 0.0, 0.0)), equipment))
            .id();
        assert!(block(h.world(), player, &axe_hit(front)));
        let offhand = h.world().get::<OffhandSlot>(player).unwrap().0.clone();
        assert_eq!(crate::item::enchant::durability_damage(&offhand), 14);
        assert!(cooldown::cooling(h.world(), player, SHIELD_CATEGORY));
        assert!(!blocking(h.world(), player));
        assert!(!block(h.world(), player, &axe_hit(front)));
    }

    #[test]
    fn hits_from_behind_are_not_blocked() {
        let mut h = Replay::new();
        let player = h.player;
        h.world().get_mut::<OffhandSlot>(player).unwrap().0 = stack(SHIELD, 1);
        h.world()
            .entity_mut(player)
            .insert(SneakTicks(RAISE_INPUTS));
        let behind = h
            .world()
            .spawn(Position(FEET + DVec3::new(-2.0, 0.0, 0.0)))
            .id();
        assert!(!block(h.world(), player, &axe_hit(behind)));
        h.world()
            .entity_mut(player)
            .insert(SneakTicks(RAISE_INPUTS - 1));
        let front = h
            .world()
            .spawn(Position(FEET + DVec3::new(2.0, 0.0, 0.0)))
            .id();
        assert!(!block(h.world(), player, &axe_hit(front)));
    }

    #[test]
    fn a_blocked_hit_wears_one_more_than_its_whole_points() {
        assert_eq!(block_wear(1.0), 2);
        assert_eq!(block_wear(4.0), 5);
        assert_eq!(block_wear(12.0), 13);
        assert_eq!(block_wear(13.0), 14);
        assert_eq!(block_wear(2.5), 3);
    }
}
