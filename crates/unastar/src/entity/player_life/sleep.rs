//! Sleeping in beds and the night skip, as BDS 1.26.51 does them (the
//! scripted captures `tools/bds-sweep/scripts/sleeping*.txt`, distilled
//! into `testdata/bds-1.26.51-sleeping.json` by
//! `tools/bds-sweep/sleeping_fixture.py`).
//!
//! **A click on a bed** (either half, any held item, [`used`]) runs these
//! checks in order; each refusal is a grey `tile.bed.*` message to the
//! clicking player ([`bed_message`]):
//! 1. the bed is taken (its head's `occupied_bit`): `occupied`;
//! 2. the player's feet are 3 blocks or more from the bottom centre of both
//!    halves on some axis ([`in_reach`]): `tooFar`;
//! 3. the bed becomes the player's respawn point, whatever follows: the
//!    point is the block the player's feet are in, the bed's head is kept
//!    with it ([`PlayerSpawnPoint`]); `SetSpawnPosition` names both, and
//!    `respawnSet` follows when the bed differs from the point's last one;
//! 4. it is day: the sky, darkened by the time of day, rain and lightning
//!    ([`clock::weather_sky_darken`]), is darkened by less than 4:
//!    `noSleep`. In clear weather that is 12542 to 23459 for a sleeper;
//!    rain and lightning at full level make noon dark enough;
//! 5. a monster (the `monster` family, a dying one too) has a box in the
//!    head's block grown by 8, 5 and 8 ([`MONSTER_REACH`]): `notSafe`.
//!
//! A block on top of the bed does not stop a sleeper, and creative players
//! sleep too.
//!
//! **Falling asleep** ([`lie_down`]): the sleeping-players event to every
//! player ([`send_sleeping_players`]); the player's actor data (key 26 bit
//! 1, key 28 the head, key 92 `SLEEPING`, key 133 its eye position before)
//! to it and its viewers; the player held at the head's centre, 0.90625
//! above its floor ([`SLEEP_EYE_HEIGHT`], a correction to its client and
//! `MovePlayer` relays to the others); then the head's `occupied_bit` (the
//! bed's actor data resent) and, at the end of the tick, the 0.2 × 0.2 box.
//! A sleeper's inputs do not move it (`server::game::movement`).
//!
//! **Waking** ([`wake`]): `Animate` wake-up to the sleeper alone, the
//! sleeping-players event unless the night is over, the actor data back
//! (keys 26 and 92), the player stood up ([`stand_up`]), the head's
//! `occupied_bit` cleared and, at the end of the tick, the standing box.
//! A sleeper wakes when it asks to (`PlayerAction` StopSleeping), when its
//! bed's head is gone, when it is day again (`/time set`), when it takes
//! damage (before the hurt event), when it is teleported (the teleport
//! places it), and when it dies or leaves (the event is sent for every death
//! and leave: they no longer count). A sleeper is saved standing up. The server
//! puts a waking player 0.1 above its spot's floor and its movement
//! authority corrects the client from there, as BDS's does.
//!
//! **The night skip** ([`tick_sleepers`]): when as many players sleep as
//! `playersSleepingPercentage` asks for (of the living players outside
//! spectator, rounded up, at least one; [`sleepers_needed`]) have each
//! slept [`SLEEP_TICKS`] ticks, every sleeper wakes and, with
//! `doDaylightCycle` on, the clock jumps to the next sunrise (a multiple of
//! 24000), broadcast at once. Rain and thunder go on (measured with
//! `doWeatherCycle` off).
//!
//! **Respawning** at a bed's point: the player comes back in the middle of
//! the point's block; when the bed's head is gone the point is dropped, the
//! player gets `tile.bed.notValid` and respawns at the world spawn
//! ([`check_spawn_bed`]).
//!
//! Not modelled here: phantoms' rest timer (`mobs.patrols_and_phantoms`)
//! and the respawn screen's packets for a bed point (`player.death_respawn`:
//! BDS sends a searching-for-spawn state with the point's block, and one at
//! height 32767 when the bed is gone).

use bevy_ecs::prelude::*;
use glam::{DVec3, IVec3};
use jolyne::valentine::types::{
    ActorRuntimeId, EnumsAnimatePacketPayloadAction, EnumsRewindType, EnumsSpawnPositionType,
    EnumsTextPacketType, PlayerInputTick, TextPacketBody, TextPacketPayloadMessageOnly, Vec2, Vec3,
};
use jolyne::valentine::{AnimatePacket, CorrectPlayerMovePredictionPacket, McpePacket, TextPacket};

use super::Dead;
use super::actor_data::{self, Keys, PlayerActorData};
use super::authority::PlayerMotion;
use super::respawn::{PlayerSpawnPoint, spawn_position_packet, standing_in};
use crate::ecs::resources::TickCounter;
use crate::entity::PLAYER_EYE_HEIGHT;
use crate::entity::components::{
    GameMode, PhysicsBody, Player, PlayerSession, Position, RuntimeEntityId, input_tick_of,
};
use crate::entity::definition::families::ActorFamilies;
use crate::entity::item_drop::PLAYER_REPORTED_EYE_HEIGHT;
use crate::entity::mob::{MobKind, MobRegistry};
use crate::entity::systems::BlockSource;
use crate::registry::block::StateValue;
use crate::registry::collision::Aabb;
use crate::server::game::placement::{Horizontal, placed_boxes};
use crate::server::game::roster;
use crate::server::game::types::BlockRegistryResource;
use crate::world::clock::{self, DAYTIME_DARKEN_LIMIT};
use crate::world::rules::{GameRules, IntGameRule, WorldTime};

#[cfg(test)]
mod tests;

/// Ticks every sleeper must have slept before the night passes (BDS
/// 1.26.51: the morning came 100 ticks after the last sleeper lay down).
pub const SLEEP_TICKS: u64 = 100;
/// A sleeper's eye height above its bed head's floor.
pub const SLEEP_EYE_HEIGHT: f64 = 0.90625;
/// A bed is in reach while the player's feet are less than this far from
/// the bottom centre of either half on every axis (BDS 1.26.51: 2.5 in
/// reach, 3.0 not, also on a diagonal of 2.5 by 2.0; 2 above in reach,
/// 3 not).
pub const REACH: f64 = 3.0;
/// How far around the bed's head block monsters keep a player awake
/// (x, y, z). BDS 1.26.51: a zombie 7 east and 5 above the head kept the
/// player awake, one 11 east or west, 11 south or 6 above did not; 8, 5, 8
/// (Java Edition's box) fits every case.
pub const MONSTER_REACH: DVec3 = DVec3::new(8.0, 5.0, 8.0);
/// How far above its spot's floor a waking player stands.
const WAKE_RISE: f64 = 0.1;

/// The bed's block states (`minecraft:bed`).
const DIRECTION: &str = "direction";
const HEAD_PIECE: &str = "head_piece_bit";
const OCCUPIED: &str = "occupied_bit";

/// A player asleep in the bed whose head is at `head` since tick `since`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sleeping {
    pub head: IVec3,
    pub since: u64,
}

/// Sleepers to wake at this tick's [`tick_sleepers`], from code that cannot
/// wake them at once: `PlayerAction` StopSleeping ([`WakeCause::Woken`]),
/// plugin teleports ([`WakeCause::Moved`]).
#[derive(Resource, Debug, Default)]
pub struct WakeQueue(pub Vec<(Entity, WakeCause)>);

/// The bed block's registry id, looked up once.
#[derive(Resource, Debug, Clone, Copy)]
struct BedBlock(Option<u32>);

/// The bed block's registry id (`None` without a block registry or a bed).
fn bed_block(world: &mut World) -> Option<u32> {
    if let Some(bed) = world.get_resource::<BedBlock>() {
        return bed.0;
    }
    let id = world
        .get_resource::<BlockRegistryResource>()
        .and_then(|registry| {
            let state = registry.0.default_state_id_by_name("minecraft:bed")?;
            Some(registry.0.get_by_runtime_id(state)?.id)
        });
    // Only cache once a registry exists: tests may add it later.
    if world.get_resource::<BlockRegistryResource>().is_some() {
        world.insert_resource(BedBlock(id));
    }
    id
}

/// A bed, as found from either half.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bed {
    head: IVec3,
    head_state: u32,
    /// From the foot towards the head.
    direction: Horizontal,
    occupied: bool,
}

impl Bed {
    fn foot(self) -> IVec3 {
        self.head - self.direction.step()
    }
}

/// The bed with a half at `pos` (its head when `pos` is the foot), if both
/// halves are there.
fn bed_at(world: &World, bed_block: u32, pos: IVec3) -> Option<Bed> {
    let registry = &world.get_resource::<BlockRegistryResource>()?.0;
    let state = world.block_at(pos)?;
    let entry = registry.get_by_runtime_id(state)?;
    if entry.id != bed_block {
        return None;
    }
    let int = |state: u32, name: &str| match entry.state_value(state, name) {
        Some(StateValue::Int(value)) => Some(value),
        Some(StateValue::Byte(value)) => Some(i32::from(value)),
        _ => None,
    };
    let direction = Horizontal::from_legacy(int(state, DIRECTION)?);
    let head = if int(state, HEAD_PIECE)? != 0 {
        pos
    } else {
        pos + direction.step()
    };
    let head_state = world.block_at(head)?;
    let head_entry = registry.get_by_runtime_id(head_state)?;
    if head_entry.id != bed_block || int(head_state, HEAD_PIECE)? == 0 {
        return None;
    }
    Some(Bed {
        head,
        head_state,
        direction,
        occupied: int(head_state, OCCUPIED)? != 0,
    })
}

/// Whether the block at `head` is still a bed's head (`None` while its
/// chunk is not loaded).
fn is_bed_head(world: &mut World, head: IVec3) -> Option<bool> {
    let bed = bed_block(world)?;
    world.block_at(head)?;
    Some(bed_at(world, bed, head).is_some_and(|found| found.head == head))
}

/// The grey `tile.bed.<key>` line BDS sends about a bed.
pub fn bed_message(key: &str) -> TextPacket {
    text_object(serde_json::json!({
        "rawtext": [{ "text": "\u{a7}7" }, { "translate": format!("tile.bed.{key}") }]
    }))
}

fn text_object(rawtext: serde_json::Value) -> TextPacket {
    TextPacket {
        localize: false,
        body: TextPacketBody::MessageOnly(TextPacketPayloadMessageOnly {
            message_type: EnumsTextPacketType::Textobject,
            message: format!("{rawtext}\n").into(),
        }),
        senders_xuid: "".into(),
        platform_id: "".into(),
        filtered_message: None,
    }
}

fn send(world: &World, player: Entity, packet: impl Into<McpePacket>) {
    if let Some(session) = world.get::<PlayerSession>(player) {
        let _ = session.send(packet.into());
    }
}

fn feet_of(world: &World, player: Entity) -> Option<DVec3> {
    Some(world.get::<Position>(player)?.0 - DVec3::Y * PLAYER_EYE_HEIGHT)
}

/// Whether a player with its feet at `feet` reaches `bed` (see [`REACH`]).
fn in_reach(feet: DVec3, bed: Bed) -> bool {
    [bed.head, bed.foot()].into_iter().any(|half| {
        let centre = half.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        let delta = (feet - centre).abs();
        delta.x < REACH && delta.y < REACH && delta.z < REACH
    })
}

/// Whether a monster keeps a sleeper of the bed with its head at `head`
/// awake (see [`MONSTER_REACH`]).
fn monster_near(world: &mut World, head: IVec3) -> bool {
    let Some(monster) = world
        .get_resource::<MobRegistry>()
        .and_then(|registry| registry.families().resolve("monster"))
    else {
        return false;
    };
    let area = Aabb::new(
        head.as_dvec3() - MONSTER_REACH,
        head.as_dvec3() + DVec3::ONE + MONSTER_REACH,
    );
    let mut mobs =
        world.query_filtered::<(&Position, &PhysicsBody, &ActorFamilies), With<MobKind>>();
    mobs.iter(world).any(|(position, body, families)| {
        families.contains(&monster).unwrap_or(false)
            && Aabb::from_feet(position.0, body.width, body.height).intersects(&area)
    })
}

/// The players that count for the night skip, and whether each sleeps:
/// alive and not spectating, `leaving` left out (a player on its way out).
fn counted_players(world: &World, leaving: Option<Entity>) -> Vec<(Entity, bool)> {
    let Some(mut players) = world
        .try_query_filtered::<(Entity, Option<&GameMode>, Has<Dead>, Has<Sleeping>), With<Player>>(
        )
    else {
        return Vec::new();
    };
    players
        .iter(world)
        .filter(|(entity, mode, dead, _)| {
            Some(*entity) != leaving && !dead && *mode != Some(&GameMode::Spectator)
        })
        .map(|(entity, _, _, sleeping)| (entity, sleeping))
        .collect()
}

/// How many of `players` must sleep for the night to pass with
/// `playersSleepingPercentage` at `percentage`: rounded up, at least one.
pub fn sleepers_needed(players: usize, percentage: i32) -> usize {
    let percentage = usize::try_from(percentage.max(0)).unwrap_or(0);
    (players * percentage).div_ceil(100).max(1)
}

/// The sleeping-players event as it stands, `leaving` left out: the
/// sleepers needed and the sleepers there are.
pub fn sleeping_players_event(
    world: &World,
    leaving: Option<Entity>,
) -> jolyne::valentine::LevelEventGenericPacket {
    let counted = counted_players(world, leaving);
    let percentage = world.get_resource::<GameRules>().map_or(100, |rules| {
        rules.int(IntGameRule::PlayersSleepingPercentage)
    });
    let needed = sleepers_needed(counted.len(), percentage);
    let sleeping = counted.iter().filter(|(_, sleeping)| *sleeping).count();
    roster::sleeping_players(
        i32::try_from(needed).unwrap_or(i32::MAX),
        i32::try_from(sleeping).unwrap_or(i32::MAX),
    )
}

/// The sleeping-players event to every player.
pub fn send_sleeping_players(world: &mut World) {
    let packet = McpePacket::from(sleeping_players_event(world, None));
    let mut sessions = world.query_filtered::<&PlayerSession, With<Player>>();
    for session in sessions.iter(world) {
        let _ = session.send(packet.clone());
    }
}

/// A player clicked `pos`: `None` when it is not a bed, else `Some(false)`
/// (the click needs no block corrections).
pub fn used(world: &mut World, player: Entity, pos: IVec3) -> Option<bool> {
    let bed_block = bed_block(world)?;
    let bed = bed_at(world, bed_block, pos)?;
    if world.get::<Sleeping>(player).is_some() || world.get::<Dead>(player).is_some() {
        return Some(false);
    }
    if bed.occupied {
        send(world, player, bed_message("occupied"));
        return Some(false);
    }
    let feet = feet_of(world, player)?;
    if !in_reach(feet, bed) {
        send(world, player, bed_message("tooFar"));
        return Some(false);
    }
    set_spawn(world, player, feet.floor().as_ivec3(), bed.head);
    if clock::weather_sky_darken(world) < DAYTIME_DARKEN_LIMIT {
        send(world, player, bed_message("noSleep"));
        return Some(false);
    }
    if monster_near(world, bed.head) {
        send(world, player, bed_message("notSafe"));
        return Some(false);
    }
    lie_down(world, player, bed);
    Some(false)
}

/// The bed with its head at `head` sets `player`'s respawn point at
/// `block`.
fn set_spawn(world: &mut World, player: Entity, block: IVec3, head: IVec3) {
    let new_bed = world
        .get::<PlayerSpawnPoint>(player)
        .is_none_or(|point| point.bed != Some(head));
    world.entity_mut(player).insert(PlayerSpawnPoint {
        position: standing_in(block),
        bed: Some(head),
    });
    send(
        world,
        player,
        spawn_position_packet(EnumsSpawnPositionType::Playerrespawn, block, Some(head)),
    );
    if new_bed {
        send(world, player, bed_message("respawnSet"));
    }
}

/// The eye position of a sleeper in the bed with its head at `head`.
pub fn sleeping_eye(head: IVec3) -> DVec3 {
    head.as_dvec3() + DVec3::new(0.5, SLEEP_EYE_HEIGHT, 0.5)
}

/// `player` falls asleep in `bed` (see the module docs).
fn lie_down(world: &mut World, player: Entity, bed: Bed) {
    let Some(entered) = world.get::<Position>(player).map(|position| position.0) else {
        return;
    };
    let now = world.resource::<TickCounter>().current;
    let eye = sleeping_eye(bed.head);
    {
        let mut sleeper = world.entity_mut(player);
        sleeper.insert(Sleeping {
            head: bed.head,
            since: now,
        });
        if let Some(mut data) = sleeper.get_mut::<PlayerActorData>() {
            // Key 133 holds the position as the wire carries it (1.62001
            // above the feet).
            data.lay_down(
                bed.head,
                entered - DVec3::Y * (PLAYER_EYE_HEIGHT - PLAYER_REPORTED_EYE_HEIGHT),
            );
        }
        if let Some(mut position) = sleeper.get_mut::<Position>() {
            position.0 = eye;
        }
        if let Some(mut motion) = sleeper.get_mut::<PlayerMotion>() {
            motion.placed(eye);
        }
    }
    send_sleeping_players(world);
    actor_data::send_changes(world, player, Keys::AllButBox);
    send(world, player, sleeping_correction(world, player, eye));
    set_occupied(world, bed.head, bed.head_state, true);
}

/// The correction putting a sleeper's client in its bed: at `eye`, at rest
/// on the ground (BDS 1.26.51 sleeping captures).
fn sleeping_correction(
    world: &World,
    player: Entity,
    eye: DVec3,
) -> CorrectPlayerMovePredictionPacket {
    let PlayerInputTick { inputtick } = input_tick_of(world, player);
    CorrectPlayerMovePredictionPacket {
        prediction_type: EnumsRewindType::Player,
        pos: Vec3 {
            x: eye.x as f32,
            y: eye.y as f32,
            z: eye.z as f32,
        },
        pos_delta: Vec3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        rotation: Vec2 { x: 0.0, y: 0.0 },
        vehicle_angular_velocity: None,
        on_ground: true,
        tick: PlayerInputTick { inputtick },
    }
}

/// Set or clear the bed head's `occupied_bit` (no block updates) and resend
/// the bed's actor data, as BDS does.
fn set_occupied(world: &mut World, head: IVec3, state: u32, occupied: bool) {
    let Some(next) = world
        .get_resource::<BlockRegistryResource>()
        .and_then(|registry| {
            registry.0.get_by_runtime_id(state)?.with_state_value(
                state,
                OCCUPIED,
                StateValue::Byte(u8::from(occupied)),
            )
        })
    else {
        return;
    };
    if next == state {
        return;
    }
    crate::server::game::blocks::write_block(world, head, next, false);
    crate::server::game::block_actors::resend(world, head);
}

/// The spots a sleeper may stand up at, in the order they are tried, as
/// `(side, up, back)` from the head: `side` blocks towards the side the
/// sleeper came from, `back` blocks towards the foot. The order is the one
/// of PR #71's bed work (commit 55804ec5), re-checked on BDS 1.26.51 for
/// its first three spots: the head's side, then the foot's, then the block
/// beside the one behind the foot (`sleeping-4.txt`).
const STAND_UP: [(i32, i32, i32); 12] = [
    (1, 0, 0),
    (1, 0, 1),
    (1, 0, 2),
    (0, 0, 2),
    (-1, 0, 2),
    (-1, 0, 1),
    (-1, 0, 0),
    (-1, 0, -1),
    (0, 0, -1),
    (1, 0, -1),
    (0, 1, 0),
    (0, 1, 1),
];

/// Where a sleeper of the bed with its head at `head`, lying along
/// `direction`, who lay down from `entered` stands up: the first of
/// [`STAND_UP`] with a collision box below and none in it or above it;
/// the head's block when there is none. The side is the clockwise side of
/// the bed's direction, or the other one when the sleeper came from there
/// (BDS 1.26.51: from the west of an east-facing bed or from in line with
/// it the south side, from the north of it the north side; a bed lying
/// south, from the west its west side, from the east its east side).
fn stand_up(world: &World, head: IVec3, direction: Horizontal, entered: DVec3) -> IVec3 {
    let right = direction.clockwise();
    let centre = head.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
    let side = if (entered - centre).dot(right.step().as_dvec3()) < 0.0 {
        right.opposite()
    } else {
        right
    };
    let Some(registry) = world.get_resource::<BlockRegistryResource>() else {
        return head;
    };
    let solid = |pos: IVec3| {
        world
            .block_at(pos)
            .map(|state| !placed_boxes(&registry.0, pos, state).is_empty())
    };
    STAND_UP
        .into_iter()
        .map(|(across, up, back)| {
            head + side.step() * across + IVec3::Y * up - direction.step() * back
        })
        .find(|&spot| {
            solid(spot - IVec3::Y) == Some(true)
                && solid(spot) == Some(false)
                && solid(spot + IVec3::Y) == Some(false)
        })
        .unwrap_or(head)
}

/// Why a sleeper wakes; it decides the event and whether it stands up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeCause {
    /// `PlayerAction` StopSleeping, its bed gone, the day come, damage:
    /// the event, then it stands up.
    Woken,
    /// The night is over: it stands up, no event.
    Morning,
    /// A teleport moves it: the event, and the teleport places it.
    Moved,
    /// It died or is leaving the game: no event (the caller sends one), and
    /// it stays where it lay.
    Gone,
}

/// Wake `player` if it sleeps (see the module docs).
pub fn wake(world: &mut World, player: Entity, cause: WakeCause) {
    let Some(sleeping) = world
        .get_entity_mut(player)
        .ok()
        .and_then(|mut sleeper| sleeper.take::<Sleeping>())
    else {
        return;
    };
    if let Some(runtime_id) = world.get::<RuntimeEntityId>(player) {
        send(
            world,
            player,
            AnimatePacket {
                action: EnumsAnimatePacketPayloadAction::Wakeup,
                target_actor_runtime_id: ActorRuntimeId {
                    actor_runtime_id: runtime_id.0 as u64,
                },
                data: 0.0,
                swing_source: None,
            },
        );
    }
    if matches!(cause, WakeCause::Woken | WakeCause::Moved) {
        send_sleeping_players(world);
    }
    actor_data::send_changes(world, player, Keys::AllButBox);
    let bed = bed_block(world).and_then(|block| bed_at(world, block, sleeping.head));
    let bed = bed.filter(|bed| bed.head == sleeping.head);
    if matches!(cause, WakeCause::Woken | WakeCause::Morning) {
        let eye = standing_eye(world, player, sleeping.head);
        if let Some(mut position) = world.get_mut::<Position>(player) {
            position.0 = eye;
        }
        // The movement authority corrects the client from there on its
        // next input, as BDS's does.
        if let Some(mut motion) = world.get_mut::<PlayerMotion>(player) {
            motion.placed(eye);
        }
    }
    if let Some(bed) = bed {
        set_occupied(world, bed.head, bed.head_state, false);
    }
}

/// The eye position `player`, asleep in the bed with its head at `head`,
/// stands up at: 0.1 above the floor of its stand-up spot, the head's block
/// when the bed is gone.
fn standing_eye(world: &World, player: Entity, head: IVec3) -> DVec3 {
    let entered = world
        .get::<PlayerActorData>(player)
        .map_or(DVec3::ZERO, |data| {
            DVec3::from_array(data.entered_bed.map(f64::from))
        });
    let bed = world
        .get_resource::<BedBlock>()
        .and_then(|block| block.0)
        .and_then(|block| bed_at(world, block, head))
        .filter(|bed| bed.head == head);
    let spot = bed.map_or(head, |bed| {
        stand_up(world, bed.head, bed.direction, entered)
    });
    spot.as_dvec3() + DVec3::new(0.5, WAKE_RISE + PLAYER_EYE_HEIGHT, 0.5)
}

/// Where a player is saved: a sleeper standing up beside its bed, not
/// inside it.
pub fn saved_eye(world: &World, player: Entity) -> Option<DVec3> {
    let sleeping = world.get::<Sleeping>(player)?;
    Some(standing_eye(world, player, sleeping.head))
}

/// System (exclusive, after the clock and this tick's block changes):
/// players leaving their bed, sleepers whose bed is gone, the day come
/// (BDS 1.26.51: `/time set 1000` woke a sleeper in that tick), and the
/// night skip.
pub fn tick_sleepers(world: &mut World) {
    let queued = std::mem::take(&mut world.get_resource_or_init::<WakeQueue>().0);
    for (player, cause) in queued {
        wake(world, player, cause);
    }
    let mut sleepers = world.query::<(Entity, &Sleeping)>();
    let sleepers: Vec<(Entity, Sleeping)> = sleepers
        .iter(world)
        .map(|(entity, sleeping)| (entity, *sleeping))
        .collect();
    let day = !sleepers.is_empty() && clock::weather_sky_darken(world) < DAYTIME_DARKEN_LIMIT;
    for &(player, sleeping) in &sleepers {
        if day || is_bed_head(world, sleeping.head) == Some(false) {
            wake(world, player, WakeCause::Woken);
        }
    }
    let counted = counted_players(world, None);
    let asleep: Vec<Entity> = counted
        .iter()
        .filter(|(_, sleeping)| *sleeping)
        .map(|(entity, _)| *entity)
        .collect();
    if asleep.is_empty() {
        return;
    }
    let percentage = world
        .resource::<GameRules>()
        .int(IntGameRule::PlayersSleepingPercentage);
    let now = world.resource::<TickCounter>().current;
    // As many sleepers as needed must each have slept long enough (BDS
    // 1.26.51: with one of two needed, the morning came 100 ticks after the
    // first sleeper although the second lay down 50 ticks later; with both
    // needed, 100 ticks after the second).
    let rested = asleep
        .iter()
        .filter(|player| {
            world
                .get::<Sleeping>(**player)
                .is_some_and(|sleeping| now.saturating_sub(sleeping.since) >= SLEEP_TICKS)
        })
        .count();
    if rested < sleepers_needed(counted.len(), percentage) {
        return;
    }
    for player in asleep {
        wake(world, player, WakeCause::Morning);
    }
    if world.resource::<GameRules>().do_daylight_cycle {
        let ticks = world.resource::<WorldTime>().ticks;
        let morning = (ticks.div_euclid(WorldTime::DAY_TICKS) + 1) * WorldTime::DAY_TICKS;
        clock::jump_to(world, morning.min(WorldTime::MAX));
    }
}

/// `player` died: it wakes if asleep, and every player gets the
/// sleeping-players event (BDS 1.26.51 sends it at every death).
pub fn player_died(world: &mut World, player: Entity) {
    wake(world, player, WakeCause::Gone);
    send_sleeping_players(world);
}

/// Before `player` respawns: drop a respawn point whose bed is gone and
/// tell the player (`tile.bed.notValid`, without the grey prefix).
pub fn check_spawn_bed(world: &mut World, player: Entity) {
    let Some(head) = world
        .get::<PlayerSpawnPoint>(player)
        .and_then(|point| point.bed)
    else {
        return;
    };
    if is_bed_head(world, head) != Some(false) {
        return;
    }
    world.entity_mut(player).remove::<PlayerSpawnPoint>();
    send(
        world,
        player,
        text_object(serde_json::json!({ "rawtext": [{ "translate": "tile.bed.notValid" }] })),
    );
}
