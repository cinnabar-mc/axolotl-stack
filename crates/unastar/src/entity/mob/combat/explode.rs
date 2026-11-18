//! Fuses (`minecraft:explode`, compiled in `mob::components::explode`).
//!
//! While a fuse burns the actor's `IGNITED` flag is set and its `FUSE_TIME`
//! counts down to 0, one burn in each of the actor's ticks (none in the tick
//! it spawned); on the next burn the actor explodes (a sixteenth of its
//! height above its feet, `server::game::explosion`; a creeper at its feet)
//! and is removed without dying. A creeper lit at tick t explodes at t + 30,
//! a TNT primed at tick t at t + 81 ([`Fuse`]).

use bevy_ecs::prelude::*;
use jolyne::valentine::actor_data_i_ds as ids;
use jolyne::valentine::actor_flags;

use crate::entity::components::{PhysicsBody, Position};
use crate::entity::definition::metadata::EntityMetadata;
use crate::entity::mob::MobKind;
use crate::entity::mob::ai::set_flag;
use crate::entity::mob::components::explode::{Explode, Fuse};
use crate::entity::mob::net::network::MobNetState;
use crate::entity::mob::state::Dead;
use crate::server::game::explosion::{
    Blast, BlastSource, Igniter, PRIMED_TNT, blast_centre, explode, in_water,
};

/// A lit fuse's actor, whether it is dead, and its spawn-tick state.
type FuseQuery = (
    Entity,
    &'static Fuse,
    Has<Dead>,
    Option<&'static MobNetState>,
);

/// System (exclusive, `EntityLogicSet`): burn the lit fuses down and
/// explode the actors whose fuse ran out.
pub fn tick_fuses(
    world: &mut World,
    fuses: &mut QueryState<FuseQuery, With<MobKind>>,
    mut burning: Local<Vec<(Entity, Fuse, bool)>>,
) {
    burning.extend(
        fuses
            .iter(world)
            // An actor spawned this tick (its spawn sync not sent yet) has
            // no tick of its own until the next.
            .filter(|(_, _, _, net)| !net.is_some_and(MobNetState::in_spawn_tick))
            .map(|(entity, fuse, dead, _)| (entity, *fuse, dead)),
    );
    for (entity, fuse, dead) in burning.drain(..) {
        let Ok(mut actor) = world.get_entity_mut(entity) else {
            continue;
        };
        set_flag(&mut actor, actor_flags::IGNITED, true);
        let Some(remaining) = fuse.remaining.checked_sub(1) else {
            if !dead {
                explode_actor(world, entity);
            }
            continue;
        };
        actor.insert(Fuse { remaining });
        if let Some(mut metadata) = actor.get_mut::<EntityMetadata>() {
            metadata.set_int(ids::FUSE_TIME, i32::try_from(remaining).unwrap_or(i32::MAX));
        }
    }
}

/// Explode `actor` with its `minecraft:explode` and remove it.
fn explode_actor(world: &mut World, actor: Entity) {
    let at_feet = world
        .get::<MobKind>(actor)
        .is_some_and(|kind| kind.0.native.explodes_at_feet);
    let (Some(config), Some(feet), Some(body)) = (
        world
            .get::<Explode>(actor)
            .map(|explode| explode.config.clone()),
        world.get::<Position>(actor).map(|position| position.0),
        world.get::<PhysicsBody>(actor).copied(),
    ) else {
        return;
    };
    let rules = world
        .get_resource::<crate::world::rules::GameRules>()
        .cloned()
        .unwrap_or_default();
    let tnt = world
        .get::<MobKind>(actor)
        .is_some_and(|kind| kind.0.actor_type == PRIMED_TNT);
    // A TNT keeps every drop unless `tntExplosionDropDecay` is on; every
    // other explosion keeps one in its power.
    let drop_chance = if tnt && !rules.tnt_explosion_drop_decay {
        1.0
    } else {
        Blast::decayed_drops(config.power)
    };
    let source = BlastSource {
        entity: actor,
        feet,
        in_water: in_water(world, feet),
        igniter: world.get::<Igniter>(actor).map(|igniter| igniter.0),
    };
    let griefing = rules.mob_griefing;
    let blast = Blast {
        centre: if at_feet {
            feet
        } else {
            blast_centre(feet, body.height)
        },
        power: config.power,
        source: Some(source),
        breaks_blocks: config.breaks_blocks && (!config.destroy_affected_by_griefing || griefing),
        causes_fire: false,
        max_resistance: config.max_resistance,
        damage_scaling: config.damage_scaling,
        knockback_scaling: config.knockback_scaling,
        drop_chance,
        sound: config.sound.clone(),
    };
    world.despawn(actor);
    explode(world, &blast);
}
