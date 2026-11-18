//! The creeper replayed against BDS 1.26.51
//! (`mob/ai/testdata/bds-1.26.51-creeper.json`, distilled by
//! `tools/bds-sweep/creeper_fixture.py` from `sweep.py creeper`): its
//! native AddActor, the `minecraft:target_nearby_sensor` decision on the
//! positions BDS sampled, and the running game against every item: the
//! walk in, the fuse lit and put out, flint and steel, the charged
//! creeper's explosion and mob griefing.

use bevy_ecs::prelude::*;
use glam::{DVec3, IVec3};
use jolyne::valentine::{McpePacketData, actor_data_i_ds, actor_flags};
use serde::Deserialize;
use unastar_data::actor_components::body;

use super::facets::{Arena, SURFACE, block, floor};
use super::hostile::{assert_add_actor_matches, native};
use crate::entity::components::{
    ActiveEffect, EffectType, GameMode, Health, MainInventory, Position, Rotation,
};
use crate::entity::definition::flags::ActorFlags;
use crate::entity::definition::metadata::EntityMetadata;
use crate::entity::mob::components::sensors::compile_target_nearby;
use crate::entity::mob::components::{TargetNearbySensorConfig, TargetZone};
use crate::entity::mob::state::AttackTarget;
use crate::entity::systems::BlockSource;
use crate::registry::collision::Aabb;
use crate::world::rules::{Difficulty, GameRules};

/// The creeper's and the player's collision box, both 0.6 × 1.8.
const WIDTH: f64 = 0.6;
const HEIGHT: f64 = 1.8;

#[derive(Debug, Deserialize)]
struct Fixture {
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    key: String,
    frozen: bool,
    target: bool,
    griefing: bool,
    actions: Vec<Action>,
    ticks: Vec<Sample>,
    events: Vec<Event>,
}

#[derive(Debug, Deserialize)]
struct Action {
    t: u32,
    order: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Sample {
    t: u32,
    creeper: Option<[f64; 3]>,
    player: [f64; 3],
}

#[derive(Debug, Deserialize)]
struct Event {
    t: u32,
    event: String,
    #[serde(default)]
    set: Option<bool>,
    #[serde(default)]
    sound: Option<String>,
    #[serde(default)]
    data: Option<i32>,
    #[serde(default)]
    at: Option<[f64; 3]>,
    #[serde(default)]
    value: Option<i32>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("../ai/testdata/bds-1.26.51-creeper.json"))
        .expect("creeper fixture decodes")
}

/// The vanilla creeper's sensor, compiled from its definition.
fn creeper_sensor() -> TargetNearbySensorConfig {
    let creeper = unastar_data::actor_definitions::vanilla()
        .iter()
        .find(|definition| definition.identifier() == "minecraft:creeper")
        .expect("vanilla creeper");
    let json = &creeper.components["minecraft:target_nearby_sensor"];
    let body: body::TargetNearbySensor =
        unastar_data::actor_components::decode(json).expect("sensor body decodes");
    compile_target_nearby(body).expect("the creeper's sensor compiles")
}

fn gap(creeper: [f64; 3], player: [f64; 3]) -> f64 {
    let a = Aabb::from_feet(DVec3::from_array(creeper), WIDTH, HEIGHT);
    let b = Aabb::from_feet(DVec3::from_array(player), WIDTH, HEIGHT);
    (a.min - b.max).max(b.min - a.max).max(DVec3::ZERO).length()
}

/// The ticks BDS lit (`true`) and put out (`false`) the creeper's fuse.
fn bds_fuse(item: &Item) -> Vec<(u32, bool)> {
    item.events
        .iter()
        .filter(|event| event.event == "ignited")
        .map(|event| (event.t, event.set.unwrap_or(false)))
        .collect()
}

/// The ticks our sensor fires `start_exploding` (`true`) and
/// `stop_exploding` (`false`), from the sampled positions, with the target
/// in sight. Positions sampled on tick `t` are those the sensor reads that
/// tick; it reads the target from the tick after the creeper took it (BDS:
/// a creeper 2 blocks from the player lit one tick after its `TARGET`).
fn our_fuse(sensor: &TargetNearbySensorConfig, item: &Item, until: u32) -> Vec<(u32, bool)> {
    let targeted = item
        .events
        .iter()
        .find(|event| event.event == "target" && event.set == Some(true))
        .map_or(u32::MAX, |event| event.t);
    let mut previous: Option<TargetZone> = None;
    let mut out = Vec::new();
    for sample in item.ticks.iter().filter(|s| s.t > targeted && s.t < until) {
        let Some(creeper) = sample.creeper else {
            break;
        };
        let zone = sensor.zone(gap(creeper, sample.player), true);
        if let Some(event) = sensor.crossing(previous, zone) {
            let lit = sensor.on_inside_range.as_ref() == Some(event);
            // The first crossing (from no target) is the target being taken.
            if previous.is_some() || lit {
                out.push((sample.t, lit));
            }
        }
        previous = Some(zone);
    }
    out
}

#[test]
fn the_creeper_sensor_lights_and_puts_out_the_fuse_on_bds_ticks() {
    let sensor = creeper_sensor();
    assert_eq!((sensor.inside_range, sensor.outside_range), (2.5, 6.0));
    let fixture = fixture();
    let mut replayed = 0;
    // Items lit with flint and steel light without the sensor.
    for item in fixture
        .items
        .iter()
        .filter(|item| item.target && item.actions.iter().all(|a| a.order.is_none()))
    {
        // The lost-sight item puts a wall up, which these positions do not
        // carry; it is checked below.
        if item.key == "sight" {
            continue;
        }
        let bds = bds_fuse(item);
        let &(lit, true) = bds.first().expect("every sensor item lights") else {
            panic!("{}: put out before it lit", item.key);
        };
        // BDS's fuse runs until the explosion; replay up to it.
        let until = item
            .events
            .iter()
            .find(|event| event.event == "explosion")
            .map_or(u32::MAX, |event| event.t);
        let ours = our_fuse(&sensor, item, until);
        assert_eq!(ours, bds, "{}: first lit on {lit}", item.key);
        replayed += 1;
    }
    assert!(replayed >= 6, "{replayed} items replayed");
}

#[test]
fn losing_sight_inside_the_range_puts_the_fuse_out() {
    let sensor = creeper_sensor();
    let fixture = fixture();
    let item = fixture
        .items
        .iter()
        .find(|item| item.key == "sight")
        .expect("the sight item");
    // BDS: lit as the target is taken, out the tick after the wall went up
    // (on tick 15).
    assert_eq!(bds_fuse(item), [(1, true), (16, false)]);
    let inside = sensor.zone(0.5, true);
    assert_eq!(inside, TargetZone::Inside);
    let hidden = sensor.zone(0.5, false);
    assert_eq!(
        sensor.crossing(Some(inside), hidden),
        sensor.on_vision_lost_inside_range.as_ref()
    );
    assert!(sensor.on_vision_lost_inside_range.is_some());
    // Seen again inside the range, it lights again.
    assert_eq!(
        sensor.crossing(Some(hidden), inside),
        sensor.on_inside_range.as_ref()
    );
}

#[test]
fn the_forced_groups_switch_the_sensor_off() {
    let creeper = unastar_data::actor_definitions::vanilla()
        .iter()
        .find(|definition| definition.identifier() == "minecraft:creeper")
        .expect("vanilla creeper");
    let json =
        &creeper.component_groups["minecraft:forced_exploding"]["minecraft:target_nearby_sensor"];
    let body: body::TargetNearbySensor =
        unastar_data::actor_components::decode(json).expect("sensor body decodes");
    let sensor = compile_target_nearby(body).unwrap();
    assert!(sensor.on_inside_range.is_none());
    assert!(sensor.on_outside_range.is_none());
    assert!(sensor.on_vision_lost_inside_range.is_none());
}

// The running game, replayed against the same captures.

const CREEPER: &str = "minecraft:creeper";
/// BDS's sweep floor tops at y -60 and its player stood at (0.5, -60, 0.5);
/// the arena's floor tops at 64 and its player stands at (12.5, 64, 12.5).
const OFFSET: DVec3 = DVec3::new(12.0, SURFACE + 60.0, 12.0);
/// `LevelEvent` of an explosion's particles (data: the power).
const EXPLOSION_PARTICLES: i32 = 2025;

impl Item {
    fn spawn_at(&self) -> DVec3 {
        DVec3::from_array(self.ticks[0].creeper.expect("sampled at spawn"))
    }

    fn first(&self, event: &str, set: Option<bool>) -> Option<u32> {
        self.events
            .iter()
            .find(|e| e.event == event && (set.is_none() || e.set == set))
            .map(|e| e.t)
    }

    fn sound(&self, sound: &str) -> Option<u32> {
        self.events
            .iter()
            .find(|e| e.event == "sound" && e.sound.as_deref() == Some(sound))
            .map(|e| e.t)
    }

    /// The tick, power and position of the explosion.
    fn explosion(&self) -> Option<(u32, i32, DVec3)> {
        self.events
            .iter()
            .find(|e| e.event == "explosion")
            .map(|e| {
                (
                    e.t,
                    e.data.expect("explosion power"),
                    DVec3::from_array(e.at.expect("explosion position")),
                )
            })
    }

    fn creeper(&self, t: u32) -> Option<DVec3> {
        self.ticks
            .iter()
            .find(|s| s.t == t)
            .and_then(|s| s.creeper)
            .map(DVec3::from_array)
    }

    fn player(&self, t: u32) -> DVec3 {
        DVec3::from_array(
            self.ticks
                .iter()
                .find(|s| s.t == t)
                .expect("sampled tick")
                .player,
        )
    }

    /// Every `FUSE_TIME` BDS sent after the spawn (its first, -1, left out).
    fn fuse_time(&self) -> Vec<(u32, i32)> {
        self.events
            .iter()
            .filter(|e| e.event == "fuse_time" && e.t > 0)
            .map(|e| (e.t, e.value.expect("fuse time value")))
            .collect()
    }

    /// The tick the creeper first left its spawn point.
    fn first_move(&self) -> u32 {
        let spawn = self.spawn_at();
        self.ticks
            .iter()
            .find(|s| s.creeper.is_some_and(|c| DVec3::from_array(c) != spawn))
            .expect("the creeper walks")
            .t
    }

    /// How far the creeper moved along x on each of `count` ticks from
    /// `from`.
    fn steps(&self, from: u32, count: u32) -> Vec<f64> {
        (from..from + count)
            .map_while(|t| Some(self.creeper(t)?.x - self.creeper(t - 1)?.x))
            .collect()
    }

    /// The ticks the player used its item (the sweep's `interact` orders).
    fn interacts(&self) -> Vec<u32> {
        self.actions
            .iter()
            .filter(|a| a.order.as_deref() == Some("interact"))
            .map(|a| a.t)
            .collect()
    }
}

/// What the running game did with one creeper; ticks count from 0, the
/// first tick after the spawn.
#[derive(Debug, Default)]
struct Run {
    /// The creeper's feet after each tick (BDS coordinates), while it lives.
    feet: Vec<DVec3>,
    targeted: Option<u32>,
    ignited: Vec<(u32, bool)>,
    /// Every change of `FUSE_TIME` after the spawn.
    fuse_time: Vec<(u32, i32)>,
    powered: bool,
    sounds: Vec<(u32, &'static str, DVec3)>,
    explosion: Option<(u32, i32, DVec3)>,
    /// The tick viewers were told the creeper was removed.
    removed: Option<u32>,
}

impl Run {
    fn sound(&self, name: &str) -> Option<u32> {
        self.sounds.iter().find(|s| s.1 == name).map(|s| s.0)
    }

    fn lit(&self) -> u32 {
        self.ignited.first().expect("the fuse lights").0
    }

    fn first_move(&self, spawn: DVec3) -> u32 {
        self.feet
            .iter()
            .position(|feet| *feet != spawn)
            .expect("the creeper walks") as u32
    }

    /// How far the creeper moved along x on each of `count` ticks from
    /// `from` (before tick 0 it stood at `spawn`).
    fn steps(&self, spawn: DVec3, from: u32, count: u32) -> Vec<f64> {
        (from..from + count)
            .map_while(|t| {
                let now = self.feet.get(t as usize)?;
                let before = match t.checked_sub(1) {
                    Some(previous) => *self.feet.get(previous as usize)?,
                    None => spawn,
                };
                Some(now.x - before.x)
            })
            .collect()
    }
}

/// An arena for `item`: a survival player that takes no harm (BDS's had
/// Resistance V) or, for an item without a target, a creative one, and a
/// creeper where BDS summoned it, facing the player, frozen (BDS: hidden
/// Slowness 255) when the item froze it.
fn creeper_arena(seed: u64, item: &Item) -> (Arena, Entity) {
    let mode = if item.target {
        GameMode::Survival
    } else {
        GameMode::Creative
    };
    let mut a = floor(seed, Difficulty::Normal, mode);
    a.world().resource_mut::<GameRules>().mob_griefing = item.griefing;
    let player = a.player;
    let mut health = a.world().get_mut::<Health>(player).unwrap();
    health.max = 10_000.0;
    health.current = 10_000.0;
    let creeper = a.adult(CREEPER, item.spawn_at() + OFFSET);
    a.world().get_mut::<Rotation>(creeper).unwrap().yaw = 90.0;
    if item.frozen {
        crate::entity::effects::give(
            a.world(),
            creeper,
            EffectType::SLOWNESS,
            ActiveEffect::new(255, 100_000),
        );
    }
    (a, creeper)
}

fn wire(position: jolyne::valentine::types::Vec3) -> DVec3 {
    DVec3::new(
        f64::from(position.x),
        f64::from(position.y),
        f64::from(position.z),
    ) - OFFSET
}

/// Run `ticks` ticks, calling `before` ahead of each, and record what the
/// creeper did.
fn record(
    a: &mut Arena,
    creeper: Entity,
    ticks: u32,
    mut before: impl FnMut(&mut Arena, u32),
) -> Run {
    let mut run = Run::default();
    let mut lit = false;
    a.packets();
    for t in 0..ticks {
        before(a, t);
        a.ticks(1);
        let world = a.world();
        if let Ok(entity) = world.get_entity(creeper) {
            run.feet.push(entity.get::<Position>().unwrap().0 - OFFSET);
            if run.targeted.is_none() && entity.contains::<AttackTarget>() {
                run.targeted = Some(t);
            }
            let flags = entity.get::<ActorFlags>().unwrap();
            let now = flags.get(actor_flags::IGNITED).unwrap();
            run.powered |= flags.get(actor_flags::POWERED).unwrap();
            if now != lit {
                run.ignited.push((t, now));
                lit = now;
            }
            let fuse = entity
                .get::<EntityMetadata>()
                .and_then(|data| data.int(actor_data_i_ds::FUSE_TIME));
            if let Some(fuse) = fuse
                && run.fuse_time.last().map(|f| f.1) != Some(fuse)
                && (fuse != -1 || !run.fuse_time.is_empty())
            {
                run.fuse_time.push((t, fuse));
            }
        }
        for packet in a.packets() {
            match packet {
                McpePacketData::LevelSoundEventPacket(event) => {
                    for name in ["mob.warning", "fuse", "ignite", "explode"] {
                        if event.sound_event == name {
                            run.sounds.push((t, name, wire(event.position.clone())));
                        }
                    }
                }
                McpePacketData::RemoveActorPacket(_) if run.removed.is_none() => {
                    run.removed = Some(t);
                }
                McpePacketData::LevelEventPacket(event)
                    if event.event_id == EXPLOSION_PARTICLES && run.explosion.is_none() =>
                {
                    run.explosion = Some((t, event.data, wire(event.position)));
                }
                _ => {}
            }
        }
    }
    run
}

fn near(a: DVec3, b: DVec3, tolerance: f64) -> bool {
    (a - b).abs().max_element() <= tolerance
}

/// The player stands where BDS's stood on each sampled tick (the sweep
/// teleported it).
fn follow_bds_player(item: &Item) -> impl FnMut(&mut Arena, u32) + '_ {
    move |a, t| a.move_player(item.player(t) + OFFSET)
}

#[test]
fn spawned_creeper_add_actor_matches_native_summon() {
    let native = native(include_bytes!(
        "../../../../../bds-extractor/captures/1.26.51/operator/summons/creeper.bin"
    ));
    let mut a = floor(1, Difficulty::Normal, GameMode::Creative);
    let feet = DVec3::new(
        f64::from(native.position.x),
        f64::from(native.position.y),
        f64::from(native.position.z),
    );
    let creeper = a.adult(CREEPER, feet);
    assert_add_actor_matches(a.world(), CREEPER, creeper, &native);
}

/// BDS's creepers walked in from 6 blocks with the same speed ramp every
/// time; the fuse lit as the boxes came within 2.5 blocks (29 ticks after
/// the first step) and the creeper stopped walking that tick; `mob.warning`
/// came the tick after the target, `fuse` two ticks after the light, and the
/// explosion (power 3, at the creeper's feet) 30 ticks after it, the
/// creeper gone the tick after. A player stepping back to 4.3 or 5.8
/// blocks between the boxes (`gap`, `gap-far`) did not put the fuse out.
/// The running game does the same, step for step.
#[test]
fn an_approaching_creeper_walks_in_lights_and_explodes_like_bds() {
    let fixture = fixture();
    let mut replayed = 0;
    for item in fixture
        .items
        .iter()
        .filter(|item| item.key.starts_with("approach") || item.key.starts_with("gap"))
    {
        // `approach-2`'s creeper was pushed off its spawn point and turned
        // on its first steps; the rest walked straight in.
        if item.key == "approach-2" {
            continue;
        }
        let bds_target = item.first("target", Some(true)).unwrap();
        let bds_move = item.first_move();
        let bds_lit = item.first("ignited", Some(true)).unwrap();
        let (bds_boom, bds_power, bds_at) = item.explosion().unwrap();
        assert_eq!(bds_move - bds_target, 2, "{}", item.key);
        assert_eq!(bds_lit - bds_move, 28, "{}", item.key);
        assert_eq!(bds_boom - bds_lit, 30, "{}", item.key);
        assert_eq!(item.first("ignited", Some(false)), None, "{}", item.key);
        assert_eq!(item.first("removed", None), Some(bds_boom + 1));
        assert_eq!(item.sound("explode"), Some(bds_boom + 1));
        assert_eq!(item.sound("mob.warning"), Some(bds_target + 1));
        assert_eq!(item.sound("fuse"), Some(bds_lit + 2));
        let bds_feet = item.creeper(bds_boom).unwrap();
        assert!(near(bds_at, bds_feet, 1e-4), "{}: at the feet", item.key);

        let (mut a, creeper) = creeper_arena(replayed + 1, item);
        let run = record(&mut a, creeper, 120, follow_bds_player(item));
        let targeted = run.targeted.expect("the creeper takes the player");
        let first = run.first_move(item.spawn_at());
        let lit = run.lit();
        // BDS samples before the tick's move, so its creeper took its first
        // step on tick `bds_move - 1`: one tick after it took its target,
        // where ours steps on the same tick (an open gap). From the first
        // step on, the light and every step match.
        assert_eq!(lit - first, bds_lit + 1 - bds_move, "{}: light", item.key);
        assert_eq!(run.ignited.len(), 1, "{}: {:?}", item.key, run.ignited);
        let ours = run.steps(item.spawn_at(), first, 34);
        let theirs = item.steps(bds_move, 34);
        assert_eq!(ours.len(), theirs.len());
        for (i, (o, b)) in ours.iter().zip(&theirs).enumerate() {
            assert!(
                (o - b).abs() < 1e-3,
                "{}: step {i}: {ours:?} vs BDS {theirs:?}",
                item.key
            );
        }
        // `FUSE_TIME` counts 29 down to 0 from the light, a step a tick.
        let shift = |fuse: &[(u32, i32)], from: u32| -> Vec<(u32, i32)> {
            fuse.iter().map(|&(t, v)| (t - from, v)).collect()
        };
        assert_eq!(
            shift(&run.fuse_time, lit),
            shift(&item.fuse_time(), bds_lit),
            "{}: FUSE_TIME",
            item.key
        );
        let (boom, power, at) = run.explosion.expect("explodes");
        assert_eq!((boom - lit, power), (30, bds_power), "{}", item.key);
        // BDS sent the creeper's removal and the `explode` sound the tick
        // after the explosion's particles; unastar sends all three with the
        // explosion (an open gap).
        assert_eq!(run.removed, Some(boom), "{}", item.key);
        assert_eq!(run.sound("mob.warning"), Some(targeted + 1));
        assert_eq!(run.sound("fuse"), Some(lit + 2));
        assert_eq!(run.sound("explode"), Some(boom));
        assert!(
            near(at, *run.feet.last().unwrap(), 1e-4),
            "{}: {at}",
            item.key
        );
        replayed += 1;
    }
    assert_eq!(replayed, 4);
}

/// A frozen creeper two blocks from the player lights as it takes it; the
/// fuse goes out the tick after the player leaves past `outside_range`
/// (`cancel`) or a wall hides it (`sight`), and lights again when it comes
/// back, exploding 30 ticks after that light, as BDS's did.
#[test]
fn a_lit_creeper_puts_its_fuse_out_like_bds() {
    let fixture = fixture();
    for key in ["cancel", "sight"] {
        let item = fixture.items.iter().find(|i| i.key == key).unwrap();
        let bds: Vec<(u32, bool)> = item
            .events
            .iter()
            .filter(|e| e.event == "ignited")
            .map(|e| (e.t, e.set.unwrap_or(false)))
            .collect();
        let bds_target = item.first("target", Some(true)).unwrap();
        let (mut a, creeper) = creeper_arena(7, item);
        // The sweep ran its commands after the tick's sample: the wall is up
        // for the next tick, as the teleported player shows in `cancel`.
        let wall_at = (key == "sight").then(|| item.actions[0].t + 1);
        let mut follow = follow_bds_player(item);
        let run = record(&mut a, creeper, 140, |a, t| {
            follow(a, t);
            if wall_at == Some(t) {
                for y in 0..3 {
                    for z in -1..=1 {
                        let pos = IVec3::new(1, -60 + y, z) + OFFSET.as_ivec3();
                        a.set_block(pos, "minecraft:stone");
                    }
                }
            }
        });
        let targeted = run.targeted.expect("targets");
        // Relative to the target, as the target goal's start is random.
        let shift = |events: &[(u32, bool)], from: u32| -> Vec<(i64, bool)> {
            events
                .iter()
                .map(|&(t, set)| (i64::from(t) - i64::from(from), set))
                .collect()
        };
        let ours = shift(&run.ignited, targeted);
        let theirs = shift(&bds, bds_target);
        // The commands came on fixed ticks: after the first light, compare
        // on absolute ticks.
        assert_eq!(ours[0], theirs[0], "{key}: lit with the target");
        assert_eq!(&run.ignited[1..], &bds[1..], "{key}");
        // A fuse put out keeps its last `FUSE_TIME`; lit again it counts
        // from 29. From the first change on the commands' ticks are fixed.
        let after = |fuse: Vec<(u32, i32)>| -> Vec<(u32, i32)> {
            fuse.into_iter().filter(|&(t, _)| t > 15).collect()
        };
        assert_eq!(
            after(run.fuse_time.clone()),
            after(item.fuse_time()),
            "{key}: FUSE_TIME"
        );
        match item.explosion() {
            Some((boom, power, _)) => {
                assert_eq!(run.explosion.map(|e| (e.0, e.1)), Some((boom, power)));
            }
            None => assert_eq!(run.explosion, None, "{key}"),
        }
    }
}

/// Flint and steel used on a creeper outside its sensor's range, by a
/// survival target or a creative player (BDS's `flint`, `flint-alone`):
/// the `ignite` sound and `IGNITED` come on the tick of the use, the fuse
/// burns on although nothing is inside the range, and the creeper
/// explodes 30 ticks later. The flint and steel takes one point of wear
/// from the survival player, and the second use (on a lit creeper) does
/// nothing.
#[test]
fn flint_and_steel_lights_a_creeper_like_bds() {
    let fixture = fixture();
    for key in ["flint", "flint-alone"] {
        let item = fixture.items.iter().find(|i| i.key == key).unwrap();
        let bds_lit = item.first("ignited", Some(true)).unwrap();
        assert_eq!(item.sound("ignite"), Some(bds_lit), "{key}");
        assert_eq!(item.sound("fuse"), Some(bds_lit + 2), "{key}");
        let (bds_boom, bds_power, _) = item.explosion().unwrap();
        assert_eq!((bds_boom - bds_lit, bds_power), (30, 3), "{key}");
        assert_eq!(item.first("ignited", Some(false)), None, "{key}");

        let (mut a, creeper) = creeper_arena(11, item);
        a.hold("minecraft:flint_and_steel", 1);
        // BDS's first use was refused (the client resynced the slot it had
        // just been given) and the second lit it, three ticks later on the
        // server; use it at the tick BDS lit and once more after.
        let uses = [bds_lit, bds_lit + 5];
        assert!(item.interacts().len() == 2);
        let mut follow = follow_bds_player(item);
        let run = record(&mut a, creeper, 80, |a, t| {
            follow(a, t);
            if uses.contains(&t) {
                a.interact(creeper);
            }
        });
        assert_eq!(run.ignited, [(bds_lit, true)], "{key}");
        assert_eq!(run.sound("ignite"), Some(bds_lit), "{key}");
        assert_eq!(run.sound("fuse"), Some(bds_lit + 2), "{key}");
        let (boom, power, _) = run.explosion.expect("explodes");
        assert_eq!((boom - bds_lit, power), (30, 3), "{key}");
        let player = a.player;
        let held = a
            .world()
            .get::<MainInventory>(player)
            .unwrap()
            .0
            .item(0)
            .unwrap();
        let worn = crate::item::enchant::durability_damage(held);
        assert_eq!(worn, i32::from(item.target), "{key}: wear {worn}");
    }
}

/// The charged creeper (`minecraft:become_charged`, as BDS's `charged`
/// item sent) is `POWERED` and explodes with power 6; without mob
/// griefing (`no-griefing`) the power 3 explosion breaks no block.
#[test]
fn charged_and_griefing_creepers_explode_like_bds() {
    let fixture = fixture();
    for key in ["charged", "no-griefing"] {
        let item = fixture.items.iter().find(|i| i.key == key).unwrap();
        let (bds_boom, bds_power, _) = item.explosion().unwrap();
        let bds_lit = item.first("ignited", Some(true)).unwrap();
        assert_eq!(bds_boom - bds_lit, 30);
        let (mut a, creeper) = creeper_arena(13, item);
        if key == "charged" {
            let world = a.world();
            let handle = world
                .get::<crate::entity::mob::MobKind>(creeper)
                .unwrap()
                .0
                .events
                .event("minecraft:become_charged")
                .unwrap();
            crate::entity::mob::events::fire_event(world, creeper, &handle, None).unwrap();
            crate::entity::definition::apply_pending_definitions(world);
        }
        let under = (item.spawn_at() + OFFSET).floor().as_ivec3() - IVec3::Y;
        let run = record(&mut a, creeper, 80, follow_bds_player(item));
        assert_eq!(run.powered, key == "charged", "{key}");
        let (boom, power, _) = run.explosion.expect("explodes");
        assert_eq!((boom - run.lit(), power), (30, bds_power), "{key}");
        let grass = block("minecraft:grass_block");
        let kept = a.world().block_at(under) == Some(grass);
        assert_eq!(kept, !item.griefing, "{key}: the block under the creeper");
    }
}

/// `loot`: the creeper's table drops 0 to 2 gunpowder, plus one music disc
/// when a skeleton-family archer (skeleton, stray, bogged, parched) killed
/// it (`killed_by_entity`, its arrow's shooter); a player kill drops no
/// disc.
#[test]
fn a_creeper_shot_dead_by_a_skeleton_drops_a_music_disc() {
    use crate::entity::mob::combat::{Hit, hurt_mob};
    use crate::entity::mob::components::KnockbackPower;
    use unastar_data::actor_components::body::ActorDamageCause;

    let mut a = floor(17, Difficulty::Normal, GameMode::Creative);
    let skeleton = a.adult("minecraft:skeleton", DVec3::new(20.5, SURFACE, 20.5));
    let mut discs = 0;
    for _ in 0..10 {
        let creeper = a.adult(CREEPER, DVec3::new(0.5, SURFACE, 0.5));
        a.take_drops();
        let hit = Hit {
            damage: 100.0,
            cause: ActorDamageCause::Projectile,
            attacker: Some(skeleton),
            by_player: false,
            knockback_from: None,
            knockback_power: KnockbackPower::DEFAULT,
        };
        hurt_mob(a.world(), creeper, hit);
        a.ticks(1);
        let drops = a.take_drops();
        let (records, other): (Vec<_>, Vec<_>) = drops
            .iter()
            .partition(|(item, _)| item.starts_with("minecraft:music_disc_"));
        assert_eq!(records.len(), 1, "one disc: {drops:?}");
        assert!(
            other.iter().all(|(item, _)| item == "minecraft:gunpowder"),
            "{drops:?}"
        );
        discs += records.len();
    }
    assert_eq!(discs, 10);
}

/// The fuse burns in the actor's own ticks, of which a new actor has none
/// in the tick it spawned: a TNT lit with flint and steel (a new primed
/// TNT, its fuse 80) explodes 81 ticks after the use, as BDS's did
/// (`tnt_lit.json`: lit by the click on tick 12, which reached BDS a tick
/// later, exploded on tick 94), where a creeper lit by the same use
/// explodes 30 ticks after it.
#[test]
fn a_tnt_lit_with_flint_and_steel_burns_from_its_next_tick() {
    let mut a = floor(29, Difficulty::Normal, GameMode::Creative);
    let tnt = IVec3::new(14, 64, 12);
    a.set_block(tnt, "minecraft:tnt");
    // What flint and steel used on the block does (the item use itself is
    // covered by the explosion tests).
    let player = a.player;
    assert!(crate::server::game::explosion::light_tnt(
        a.world(),
        tnt,
        player
    ));
    a.packets();
    let mut exploded = None;
    for t in 0..120u32 {
        a.ticks(1);
        let boom = a.packets().into_iter().any(|packet| {
            matches!(packet, McpePacketData::LevelEventPacket(event)
                if event.event_id == EXPLOSION_PARTICLES)
        });
        if boom {
            exploded = Some(t);
            break;
        }
    }
    // Ticks count from 0, the tick of the use.
    assert_eq!(exploded, Some(81));
}
