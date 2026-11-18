//! Explosions: which blocks an explosion breaks and how hard it hits the
//! actors around it, as BDS 1.26.51 does (measured with the TNT sweeps in
//! `crates/unastar-data/data/measured/explosions/`, `tools/bds-sweep/tnt.py`;
//! `explosion/tests.rs` compares by distribution and per hit).
//!
//! - **Blocks.** One ray leaves the centre through each cell on the outer
//!   shell of a 16 x 16 x 16 grid (1352 rays). A ray starts with
//!   `power x (0.7 + 0.6 U)` and walks in 0.3-block steps; in every block
//!   that is not air it loses `(resistance + 0.3) x 0.3` and breaks the
//!   block while it still has strength; every step costs 0.225 more. A
//!   waterlogged block costs its water's resistance and never breaks
//!   (`tnt_logged.json`). An explosion whose source actor is in water
//!   breaks nothing (`tnt_water.json`).
//! - **Drops.** Every broken block drops what it drops by hand, without
//!   the correct tool, each dropped stack kept with a chance of one in the
//!   power (one per block for single-drop blocks like dirt; a
//!   TNT's always, unless `tntExplosionDropDecay` is on: `tnt_decay.json`).
//!   The drops are gathered over the whole explosion first: a drop joins
//!   the first gathered stack of its item with room, where room is 16 items
//!   and 16 more for every ten stacks gathered (`tnt_drops.json`); each
//!   stack pops after every block is cleared, at the block that started it.
//! - **Fire.** An explosion that causes fire sets one in three of the cells
//!   its rays reached that are air afterwards on fire, where fire can stand
//!   (`tnt_anchor.json`: fire shows only on a solid block).
//! - **Actors.** A mob's distance is from the centre to its feet less half
//!   its width, a player's the plain distance to its feet; within twice the
//!   power the impact is `(1 - distance / (2 x power)) x exposure`, the
//!   share of points on its box that see the centre, and it takes
//!   `(impact^2 + impact) / 2 x 7 x 2 x power + 1` damage (none when the
//!   source is in water) as a hurt with the usual hurt knockback away from
//!   the actor that lit it, else the exploding actor. Then it is pushed by
//!   `impact x (1 - knockback resistance)` from the centre towards its eyes
//!   (a mob's: [`PUSH_HEIGHT_RATIO`] of its height)
//!   (`tnt_knock.json`, `tnt_player.json`). A player is not hurt by an
//!   explosion with a source actor at peaceful, but still pushed.

use bevy_ecs::prelude::*;
use glam::{DVec3, IVec3};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use unastar_data::actor_components::body::ActorDamageCause;
use unastar_data::actor_identifiers::ActorType;

use super::block_drops::{pop_resource, popped_drops};
use crate::entity::PLAYER_EYE_HEIGHT;
use crate::entity::components::{GameMode, PhysicsBody, Player, Position, Velocity};
use crate::entity::mob::MobKind;
use crate::entity::mob::ai::sensing::can_see;
use crate::entity::mob::attributes::KnockbackResistance;
use crate::entity::mob::combat::{Hit, hurt_mob};
use crate::entity::mob::components::KnockbackPower;
use crate::entity::mob::constants::{PLAYER_BOX_HEIGHT, PLAYER_BOX_WIDTH};
use crate::entity::mob::state::Dead;
use crate::entity::systems::{BlockSource, LayeredBlocks};
use crate::item::ItemStack;
use crate::registry::BlockRegistry;
use crate::registry::collision::Aabb;
use crate::world::rules::{Difficulty, GameRules};

mod viewers;

pub(crate) use viewers::fire_charge_lit;

#[cfg(test)]
mod tests;

/// Rays per edge of the ray grid.
const GRID: usize = 16;
/// A ray's starting strength is `power x (BASE + SPREAD x U)`.
const RAY_BASE: f32 = 0.7;
const RAY_SPREAD: f32 = 0.6;
/// Length of one ray step, in blocks.
const STEP: f32 = 0.3;
/// Strength a ray loses on every step.
const STEP_COST: f32 = 0.225;
/// A block costs `(resistance + OFFSET) x FACTOR` per step inside it.
const RESISTANCE_OFFSET: f32 = 0.3;
const RESISTANCE_FACTOR: f32 = 0.3;
/// Damage of an impact of 1 per block of radius, before the constant 1.
const DAMAGE_FACTOR: f64 = 7.0;
/// Drops gather into stacks of this many items, and this many more for
/// every [`GATHER_STEP`] stacks gathered (BDS 1.26.51, `tnt_drops.json`:
/// 210 dirt drops make four stacks of 32, one of 17, four of 16 and one of
/// 1, the 32s and the 17 started by the first blocks broken).
const GATHER_STACK: u32 = 16;
const GATHER_STEP: usize = 10;
/// An explosion pushes a mob towards this fraction of its height above its
/// feet (BDS 1.26.51 `tnt_knock.json`: pigs and husks, from the push's
/// direction; a player is pushed towards its eyes, 1.62 of 1.8). Not the
/// mobs' eye height of their sounds, 0.85.
pub(crate) const PUSH_HEIGHT_RATIO: f64 = 0.9;
/// One reached cell in this many catches fire (`tnt_anchor.json`: 254 of
/// 709 cells on a solid block).
const FIRE_CHANCE: u32 = 3;

/// The random source of explosions (ray strengths, the sampled block
/// particles, drops and fire, a primed TNT's direction), its own: drawing
/// them from [`crate::entity::EntityRandom`] moved every later entity roll
/// whenever something exploded. Seeded from entropy.
#[derive(Resource)]
pub(crate) struct ExplosionRandom(pub(crate) StdRng);

impl Default for ExplosionRandom {
    fn default() -> Self {
        Self(StdRng::from_entropy())
    }
}

#[cfg(test)]
impl ExplosionRandom {
    pub(crate) fn seeded(seed: u64) -> Self {
        Self(StdRng::seed_from_u64(seed))
    }
}

/// The blocks explosions test, resolved against the block registry once
/// ([`Self::of`] caches them in the world).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExplosionBlocks {
    air: u32,
    /// The fire explosions set.
    fire: Option<u32>,
    /// TNT's block states (first, last): primed instead of dropped.
    tnt: Option<(u32, u32)>,
}

impl ExplosionBlocks {
    /// The blocks in `registry`; `None` when it has no air.
    pub(crate) fn resolve(registry: &BlockRegistry) -> Option<Self> {
        Some(Self {
            air: registry.default_state_id_by_name("minecraft:air")?,
            fire: registry.default_state_id_by_name("minecraft:fire"),
            tnt: registry
                .get_by_name("minecraft:tnt")
                .map(|entry| (entry.min_state_id, entry.max_state_id)),
        })
    }

    /// The blocks of the world's registry, resolved on first use.
    pub(crate) fn of(world: &mut World) -> Option<Self> {
        if let Some(blocks) = world.get_resource::<Self>() {
            return Some(*blocks);
        }
        let blocks = Self::resolve(world.registry())?;
        world.insert_resource(blocks);
        Some(blocks)
    }

    pub(crate) fn is_tnt(&self, state: u32) -> bool {
        self.tnt
            .is_some_and(|(first, last)| (first..=last).contains(&state))
    }
}

/// The directions of the rays: from the centre through every cell on the
/// grid's outer shell.
fn ray_directions() -> impl Iterator<Item = glam::Vec3> {
    let last = (GRID - 1) as f32;
    (0..GRID).flat_map(move |i| {
        (0..GRID).flat_map(move |j| {
            (0..GRID).filter_map(move |k| {
                let shell = [i, j, k].iter().any(|&n| n == 0 || n == GRID - 1);
                shell.then(|| {
                    (glam::Vec3::new(i as f32, j as f32, k as f32) / last * 2.0 - glam::Vec3::ONE)
                        .normalize()
                })
            })
        })
    })
}

/// What a ray meets in one cell: the resistance it pays, and whether the
/// cell breaks while the ray is still strong (a waterlogged block never
/// does).
fn cell_cost(blocks: &impl LayeredBlocks, cell: IVec3, air: u32) -> Option<(f32, bool)> {
    let registry = blocks.registry();
    let resistance_of = |state: u32| {
        registry
            .get_by_runtime_id(state)
            .map_or(f32::MAX, |entry| entry.resistance)
    };
    let Some(state) = blocks.block_at(cell) else {
        return Some((f32::MAX, false));
    };
    if state == air {
        return None;
    }
    let water = blocks
        .second_layer_at(cell)
        .filter(|layer| registry.liquid_state(*layer).is_some());
    Some(match water {
        Some(water) => (resistance_of(state).max(resistance_of(water)), false),
        None => (resistance_of(state), true),
    })
}

/// The blocks an explosion of `power` at `centre` breaks, in the order the
/// rays reach them (each once), and every cell the rays reached (`reached`,
/// where fire can catch).
pub(crate) fn blasted_blocks(
    blocks: &impl LayeredBlocks,
    ids: &ExplosionBlocks,
    centre: DVec3,
    power: f32,
    max_resistance: f32,
    rng: &mut impl Rng,
    mut reached: Option<&mut Vec<IVec3>>,
) -> Vec<IVec3> {
    let start = centre.as_vec3();
    let mut seen = std::collections::HashSet::new();
    let mut touched = std::collections::HashSet::new();
    let mut out = Vec::new();
    for direction in ray_directions() {
        let mut strength = power * (RAY_BASE + RAY_SPREAD * rng.r#gen::<f32>());
        let mut point = start;
        while strength > 0.0 {
            let cell = point.floor().as_ivec3();
            if let Some((resistance, breaks)) = cell_cost(blocks, cell, ids.air) {
                strength -=
                    (resistance.min(max_resistance) + RESISTANCE_OFFSET) * RESISTANCE_FACTOR;
                if breaks && strength > 0.0 && seen.insert(cell) {
                    out.push(cell);
                }
            }
            if strength > 0.0
                && let Some(reached) = reached.as_deref_mut()
                && touched.insert(cell)
            {
                reached.push(cell);
            }
            point += direction * STEP;
            strength -= STEP_COST;
        }
    }
    out
}

/// The share of sample points on `aabb` with a clear line to `centre`.
pub(crate) fn exposure(blocks: &impl BlockSource, centre: DVec3, aabb: &Aabb) -> f64 {
    let size = aabb.max - aabb.min;
    let step = DVec3::ONE / (size * 2.0 + DVec3::ONE);
    if step.x < 0.0 || step.y < 0.0 || step.z < 0.0 {
        return 0.0;
    }
    // Centre the sample grid on the box's horizontal faces.
    let offset_x = (1.0 - (1.0 / step.x).floor() * step.x) / 2.0;
    let offset_z = (1.0 - (1.0 / step.z).floor() * step.z) / 2.0;
    let (mut seen, mut total) = (0u32, 0u32);
    let mut x = 0.0;
    while x <= 1.0 {
        let mut y = 0.0;
        while y <= 1.0 {
            let mut z = 0.0;
            while z <= 1.0 {
                let point = DVec3::new(
                    aabb.min.x + size.x * x + offset_x,
                    aabb.min.y + size.y * y,
                    aabb.min.z + size.z * z + offset_z,
                );
                if can_see(blocks, point, centre) {
                    seen += 1;
                }
                total += 1;
                z += step.z;
            }
            y += step.y;
        }
        x += step.x;
    }
    if total == 0 {
        0.0
    } else {
        f64::from(seen) / f64::from(total)
    }
}

/// The impact on an actor `distance` blocks from an explosion of `power`
/// that sees `exposure` of it; 0 out of reach.
pub(crate) fn impact(distance: f64, power: f32, exposure: f64) -> f64 {
    let radius = 2.0 * f64::from(power);
    if distance > radius {
        return 0.0;
    }
    (1.0 - distance / radius) * exposure
}

/// The damage an actor takes from an explosion of `power` with `impact`.
pub(crate) fn damage(impact: f64, power: f32) -> f32 {
    let radius = 2.0 * f64::from(power);
    ((impact * impact + impact) / 2.0 * DAMAGE_FACTOR * radius + 1.0) as f32
}

/// Where an actor `height` tall with its feet at `feet` explodes: a
/// sixteenth of its height above its feet (BDS 1.26.51: the TNT blast
/// sweep's destroyed counts and the hit sweep's damage both fit this point
/// and not the feet or the middle of the actor).
pub(crate) fn blast_centre(feet: DVec3, height: f64) -> DVec3 {
    feet + DVec3::Y * (height / 16.0)
}

/// A mob's distance for its impact: from `centre` to its feet, less half
/// its width. A player's is the plain distance to its feet
/// ([`Target::distance`]; BDS 1.26.51 `tnt_player.json`).
pub(crate) fn distance(centre: DVec3, feet: DVec3, width: f64) -> f64 {
    (centre.distance(feet) - width / 2.0).max(0.0)
}

/// The push an explosion gives an actor with `impact` whose eyes are at
/// `eyes`: from the centre towards the eyes, `impact x (1 - resistance)`
/// long (BDS 1.26.51 `tnt_knock.json`, `tnt_player.json`: each first
/// motion is the hurt knockback plus this push).
pub(crate) fn push(centre: DVec3, eyes: DVec3, impact: f64, resistance: f32) -> DVec3 {
    let share = (1.0 - f64::from(resistance)).max(0.0);
    (eyes - centre).normalize_or_zero() * impact * share
}

/// The actor an explosion comes from (already removed when it explodes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BlastSource {
    pub entity: Entity,
    /// Where it stood: the hurt knockback of an actor nobody lit pushes
    /// away from here.
    pub feet: DVec3,
    /// It was in water: the explosion breaks no blocks and deals no damage.
    pub in_water: bool,
    /// The player who lit it: the hurt's attacker, its knockback pushes
    /// away from them.
    pub igniter: Option<Entity>,
}

/// The player who lit a primed TNT (flint and steel, a fire charge).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Igniter(pub Entity);

/// One explosion in the live world.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Blast {
    pub centre: DVec3,
    pub power: f32,
    /// The actor that exploded: its explosion is an entity explosion;
    /// without one it is a block explosion.
    pub source: Option<BlastSource>,
    pub breaks_blocks: bool,
    pub causes_fire: bool,
    /// Cap on a block's explosion resistance.
    pub max_resistance: f32,
    pub damage_scaling: f32,
    pub knockback_scaling: f32,
    /// The chance each drop of a broken block is kept.
    pub drop_chance: f32,
    /// The level sound viewers hear.
    pub sound: std::sync::Arc<str>,
}

impl Blast {
    /// The chance a drop survives an explosion of `power` that is not a
    /// TNT's (or a TNT's with `tntExplosionDropDecay` on): one in the power
    /// (BDS 1.26.51 `tnt_decay.json`: 879 of 3453 blocks at power 4).
    pub(crate) fn decayed_drops(power: f32) -> f32 {
        if power > 0.0 {
            (1.0 / power).min(1.0)
        } else {
            1.0
        }
    }

    fn in_water(&self) -> bool {
        self.source.is_some_and(|source| source.in_water)
    }
}

/// A target of the actor pass: where it stands, its box and the point the
/// push aims at (a player's eyes, [`PUSH_HEIGHT_RATIO`] of a mob's height).
struct Target {
    entity: Entity,
    feet: DVec3,
    eyes: DVec3,
    width: f64,
    aabb: Aabb,
    player: bool,
}

impl Target {
    /// Its distance for the impact: a player's plain distance to its feet
    /// (BDS 1.26.51 `tnt_player.json`), a mob's less half its width.
    fn distance(&self, centre: DVec3) -> f64 {
        if self.player {
            centre.distance(self.feet)
        } else {
            distance(centre, self.feet, self.width)
        }
    }
}

/// Players that take damage and living mobs whose feet are within `reach`
/// (plus their size) of `centre`.
fn targets(world: &mut World, centre: DVec3, reach: f64) -> Vec<Target> {
    let mut out = Vec::new();
    let mut players =
        world.query_filtered::<(Entity, &Position, Option<&GameMode>), With<Player>>();
    for (entity, position, mode) in players.iter(world) {
        if mode.is_some_and(|mode| !mode.allows_damage()) {
            continue;
        }
        let feet = position.0 - DVec3::Y * PLAYER_EYE_HEIGHT;
        out.push(Target {
            entity,
            feet,
            eyes: position.0,
            width: PLAYER_BOX_WIDTH,
            aabb: Aabb::from_feet(feet, PLAYER_BOX_WIDTH, PLAYER_BOX_HEIGHT),
            player: true,
        });
    }
    let mut mobs =
        world.query_filtered::<(Entity, &Position, &PhysicsBody), (With<MobKind>, Without<Dead>)>();
    for (entity, position, body) in mobs.iter(world) {
        out.push(Target {
            entity,
            feet: position.0,
            eyes: position.0 + DVec3::Y * body.height * PUSH_HEIGHT_RATIO,
            width: body.width,
            aabb: Aabb::from_feet(position.0, body.width, body.height),
            player: false,
        });
    }
    out.retain(|target| {
        target.feet.is_finite() && target.feet.distance(centre) <= reach + target.width
    });
    out
}

/// Run `blast`: hurt and push the actors around it, tell the viewers, then
/// break the blocks its rays reach (each drops what it drops by hand,
/// without the correct tool; TNT among them is primed) and set its fire.
pub(crate) fn explode(world: &mut World, blast: &Blast) {
    if blast.power <= 0.0 {
        return;
    }
    let ids = if (blast.breaks_blocks || blast.causes_fire) && !blast.in_water() {
        ExplosionBlocks::of(world)
    } else {
        None
    };
    let mut reached = Vec::new();
    let blocks = match &ids {
        Some(ids) => {
            world.init_resource::<ExplosionRandom>();
            world.resource_scope(|world, mut rng: Mut<ExplosionRandom>| {
                blasted_blocks(
                    &*world,
                    ids,
                    blast.centre,
                    blast.power,
                    blast.max_resistance,
                    &mut rng.0,
                    blast.causes_fire.then_some(&mut reached),
                )
            })
        }
        None => Vec::new(),
    };
    let blocks = if blast.breaks_blocks {
        blocks
    } else {
        Vec::new()
    };
    hurt_actors(world, blast);
    viewers::explosion(world, blast, &blocks);
    if let Some(ids) = ids {
        break_blocks(world, &ids, blast, &blocks, &reached);
    }
}

/// The hurt an actor `distance` from the blast, seeing `seen` of it, takes,
/// and the impact that pushes it (scaled by the blast's knockback scaling;
/// the damage uses the unscaled impact). An explosion whose source is in
/// water deals no damage, still hurting (BDS 1.26.51 `tnt_water.json`: a
/// 0 damage event with its knockback).
fn hit_for(world: &World, blast: &Blast, distance: f64, seen: f64) -> (Hit, f64) {
    let impact = impact(distance, blast.power, seen);
    // A player who lit the TNT and has left since is no attacker: the TNT
    // then hurts as one nobody lit.
    let igniter = blast
        .source
        .and_then(|source| source.igniter)
        .filter(|igniter| world.get::<Player>(*igniter).is_some());
    let knockback_from = blast.source.map(|source| {
        igniter
            .and_then(|igniter| crate::entity::geometry::feet_position(world, igniter))
            .unwrap_or(source.feet)
    });
    let hit = Hit {
        damage: if blast.in_water() {
            0.0
        } else {
            damage(impact, blast.power) * blast.damage_scaling
        },
        cause: if blast.source.is_some() {
            ActorDamageCause::EntityExplosion
        } else {
            ActorDamageCause::BlockExplosion
        },
        attacker: igniter,
        by_player: igniter.is_some(),
        knockback_from,
        knockback_power: KnockbackPower::DEFAULT,
    };
    (hit, impact * f64::from(blast.knockback_scaling))
}

/// Damage and push every actor within reach.
fn hurt_actors(world: &mut World, blast: &Blast) {
    let reach = 2.0 * f64::from(blast.power);
    // BDS 1.26.51 `tnt_player.json`: at peaceful a TNT hurts no player,
    // while a respawn anchor's explosion does (`tnt_anchor.json`).
    let spares_players = blast.source.is_some()
        && world
            .get_resource::<Difficulty>()
            .is_some_and(|difficulty| *difficulty == Difficulty::Peaceful);
    for target in targets(world, blast.centre, reach) {
        let distance = target.distance(blast.centre);
        if distance > reach {
            continue;
        }
        let seen = exposure(&*world, blast.centre, &target.aabb);
        let (mut hit, impact) = hit_for(world, blast, distance, seen);
        // The igniter hurt by its own TNT is only lifted, not pushed
        // sideways (BDS 1.26.51 `tnt_lit.json`: its hurt knockback is
        // (0, 0.4, 0)).
        let own = hit.attacker == Some(target.entity);
        if own {
            hit.knockback_from = None;
        }
        let resistance = if target.player {
            if !spares_players {
                crate::entity::player_life::hurt::hurt_player(world, target.entity, &hit);
                if own && world.get::<Dead>(target.entity).is_none() {
                    crate::entity::player_life::hurt::lift_player(world, target.entity);
                }
            }
            crate::entity::player_life::hurt::worn_knockback_resistance(world, target.entity)
        } else {
            hurt_mob(world, target.entity, hit);
            world
                .get::<KnockbackResistance>(target.entity)
                .copied()
                .unwrap_or(KnockbackResistance::NONE)
        };
        let push = push(blast.centre, target.eyes, impact, resistance.value());
        if target.player {
            if world.get::<Dead>(target.entity).is_none() {
                crate::entity::player_life::hurt::push_player(world, target.entity, push);
            }
        } else if let Some(mut velocity) = world.get_mut::<Velocity>(target.entity) {
            velocity.0 += push;
        }
    }
}

/// The drops of one explosion, gathered before they pop: each stack and
/// the block that started it.
#[derive(Debug, Default)]
pub(crate) struct GatheredDrops(Vec<(IVec3, ItemStack)>);

impl GatheredDrops {
    /// Add `stack`, dropped by the block at `pos`: into the first gathered
    /// stack of the same item with room for it, else as a new stack.
    pub(crate) fn add(&mut self, pos: IVec3, stack: ItemStack) {
        let room = GATHER_STACK * (1 + (self.0.len() / GATHER_STEP) as u32);
        for (_, gathered) in &mut self.0 {
            let cap = room.min(u32::from(gathered.max_stack_size()));
            if gathered.comparable(&stack)
                && u32::from(gathered.count) + u32::from(stack.count) <= cap
            {
                gathered.count += stack.count;
                return;
            }
        }
        self.0.push((pos, stack));
    }

    pub(crate) fn into_stacks(self) -> Vec<(IVec3, ItemStack)> {
        self.0
    }
}

/// Whether fire can stand on the block under `pos`: its collision covers
/// the whole top face (BDS 1.26.51 `tnt_anchor.json`: explosion fire shows
/// on dirt and grass, never over air).
fn fire_can_stand(world: &World, pos: IVec3) -> bool {
    let Some(below) = world.block_at(pos - IVec3::Y) else {
        return false;
    };
    crate::registry::collision::block_collision_aabbs(below).any(|aabb| {
        aabb.min.x <= 0.0
            && aabb.min.z <= 0.0
            && aabb.max.x >= 1.0
            && aabb.max.z >= 1.0
            && aabb.max.y >= 1.0
    })
}

/// Break the blasted blocks: TNT is primed (with its short fuse), every
/// other block becomes air and its drops are gathered; then the fire is
/// set, the gathered stacks pop and the block updates of every change run.
fn break_blocks(
    world: &mut World,
    ids: &ExplosionBlocks,
    blast: &Blast,
    blocks: &[IVec3],
    reached: &[IVec3],
) {
    let mut changed = Vec::new();
    let mut drops = GatheredDrops::default();
    for &pos in blocks {
        let Some(state) = world.block_at(pos).filter(|state| *state != ids.air) else {
            continue;
        };
        let tnt = ids.is_tnt(state);
        crate::server::game::blocks::write_block(world, pos, ids.air, false);
        if tnt {
            prime_tnt(world, pos, Some(FROM_EXPLOSION));
        } else {
            for stack in popped_drops(world, state) {
                let kept = blast.drop_chance >= 1.0
                    || world
                        .get_resource_or_init::<ExplosionRandom>()
                        .0
                        .r#gen::<f32>()
                        < blast.drop_chance;
                if kept {
                    drops.add(pos, stack);
                }
            }
        }
        changed.push(pos);
    }
    if blast.causes_fire
        && let Some(fire) = ids.fire
    {
        for &pos in reached {
            let catches = world
                .get_resource_or_init::<ExplosionRandom>()
                .0
                .gen_range(0..FIRE_CHANCE)
                == 0;
            if catches && world.block_at(pos) == Some(ids.air) && fire_can_stand(world, pos) {
                crate::server::game::blocks::write_block(world, pos, fire, false);
                changed.push(pos);
            }
        }
    }
    for (pos, stack) in drops.into_stacks() {
        pop_resource(world, pos, stack);
    }
    for pos in changed {
        crate::server::game::block_update::world_changed(world, pos, false);
    }
}

/// Whether the actor standing at `feet` is in water (its block or the
/// water a waterlogged block holds there).
pub(crate) fn in_water(world: &World, feet: DVec3) -> bool {
    let cell = feet.floor().as_ivec3();
    let registry = world.registry();
    [world.block_at(cell), world.second_layer_at(cell)]
        .into_iter()
        .flatten()
        .filter_map(|state| registry.liquid_state(state))
        .any(|liquid| liquid.kind == crate::registry::block::LiquidKind::Water)
}

/// The primed TNT actor.
pub(crate) const PRIMED_TNT: ActorType = ActorType::Tnt;
/// The event that gives a TNT primed by an explosion its short fuse.
const FROM_EXPLOSION: &str = "from_explosion";
/// A newly primed TNT moves off at this horizontal speed in a random
/// direction, and up at [`PRIME_LIFT`] (BDS 1.26.51, `tnt_prime.json`:
/// |(vx, vz)| = 0.02 and vy = 0.2 at the first read).
pub(crate) const PRIME_SPEED: f64 = 0.02;
pub(crate) const PRIME_LIFT: f64 = 0.2;

/// Light the TNT block at `pos` for `igniter` (flint and steel or a fire
/// charge): it becomes air and a primed TNT carrying the igniter takes its
/// place (BDS 1.26.51 `tnt_lit.json`: the explosion's hurts name the
/// player as attacker). `false` when `pos` holds no TNT or the
/// `tntExplodes` game rule is off.
pub(crate) fn light_tnt(world: &mut World, pos: IVec3, igniter: Entity) -> bool {
    let Some(ids) = ExplosionBlocks::of(world) else {
        return false;
    };
    let lit = world.block_at(pos).is_some_and(|state| ids.is_tnt(state))
        && world
            .get_resource::<GameRules>()
            .is_none_or(|rules| rules.tnt_explodes);
    if !lit {
        return false;
    }
    crate::server::game::blocks::write_block(world, pos, ids.air, false);
    if let Some(tnt) = prime_tnt(world, pos, None) {
        world.entity_mut(tnt).insert(Igniter(igniter));
    }
    crate::server::game::block_update::world_changed(world, pos, false);
    true
}

/// Spawn a primed TNT at the bottom centre of `pos` (the TNT block there is
/// already gone), moving off in a random direction; `event` replaces the
/// spawn event. TNT lit by power plays the fuse sound there first.
pub(crate) fn prime_tnt(world: &mut World, pos: IVec3, event: Option<&str>) -> Option<Entity> {
    use crate::entity::mob::{MobRegistry, SpawnCause, spawn_mob};
    let feet = DVec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    );
    let handle = event.and_then(|name| {
        world
            .get_resource::<MobRegistry>()
            .and_then(|registry| registry.get(PRIMED_TNT.name()))
            .and_then(|archetype| archetype.events.event(name))
    });
    let cause = if handle.is_some() {
        SpawnCause::Loaded
    } else {
        SpawnCause::Spawned
    };
    if handle.is_none() {
        viewers::fuse(world, feet);
    }
    let tnt = match spawn_mob(world, PRIMED_TNT.name(), feet, 0.0, cause) {
        Ok(tnt) => tnt,
        Err(error) => {
            tracing::warn!(%error, "primed TNT could not spawn");
            return None;
        }
    };
    if let Some(handle) = handle {
        if let Err(error) = crate::entity::mob::events::fire_event(world, tnt, &handle, None) {
            tracing::warn!(?error, "primed TNT event failed");
        }
        crate::entity::definition::apply_pending_definitions(world);
    }
    let angle = world
        .get_resource_or_init::<ExplosionRandom>()
        .0
        .r#gen::<f64>()
        * std::f64::consts::TAU;
    let motion = DVec3::new(
        -angle.sin() * PRIME_SPEED,
        PRIME_LIFT,
        -angle.cos() * PRIME_SPEED,
    );
    if let Some(mut velocity) = world.get_mut::<Velocity>(tnt) {
        velocity.0 = motion;
    }
    Some(tnt)
}
