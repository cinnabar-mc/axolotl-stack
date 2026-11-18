//! The world clock, day/night and sky light.
//!
//! The clock ([`WorldTime`]) advances one tick per game tick while the
//! `doDaylightCycle` game rule is on. Clients run their own copy of the
//! overworld clock; BDS replicates it with `SyncWorldClocks`: the clock
//! registry (the overworld clock and its six time markers) once at join,
//! and the clock state every [`CLOCK_SYNC_INTERVAL_TICKS`] ticks to every
//! player, plus immediately after a command changes the time.
//!
//! Sky darkening follows the time of day ([`sky_darken`]); only beds ask
//! for the darkening with rain and lightning added ([`weather_sky_darken`]),
//! the other daylight checks (spawning, burning, fleeing the sun) still
//! ignore the weather.
//!
//! Light is not tracked by the server, so the sky light at a block is
//! derived from its chunk column: at or above the column's heightmap (the
//! first air block above the highest non-air block) it is full sky light;
//! below, sky light falls by the light filtering of every block from the
//! heightmap down to and including the block. Block light is not modelled
//! (0). The model is the overworld's: the server runs one dimension and
//! treats it as having a sky.

use std::f32::consts::PI;

use bevy_ecs::prelude::*;
use glam::IVec3;
use jolyne::valentine::types::{
    SyncWorldClockStateData, SyncWorldClocksPacketData,
    SyncWorldClocksPacketPayloadInitializeRegistryData, SyncWorldClocksPacketPayloadSyncStateData,
    TimeMarkerData, WorldClockData,
};
use jolyne::valentine::{McpePacket, SyncWorldClocksPacket};

use super::chunk::{MIN_Y, SUBCHUNK_COUNT};
use super::ecs::systems::{world_to_chunk_coords, world_to_local_coords};
use super::ecs::{ChunkData, ChunkManager};
use super::rules::{Difficulty, GameRules, StartGameWorldState, WorldTime};
use super::weather::WeatherLevels;
use crate::ecs::resources::TickCounter;
use crate::entity::components::PlayerSession;
use crate::server::game::types::BlockRegistryResource;

/// Ticks between two clock-state broadcasts (BDS syncs the clocks when
/// more than 255 level ticks passed since the last sync; confirmed).
pub const CLOCK_SYNC_INTERVAL_TICKS: u64 = 256;
/// The overworld clock's id and name (confirmed: BDS join capture).
pub const OVERWORLD_CLOCK_ID: u64 = 0x63d7_ede7_3c38_f916;
pub const OVERWORLD_CLOCK_NAME: &str = "minecraft:overworld";
/// The overworld clock's time markers: id, name and time of day, each
/// repeating every [`WorldTime::DAY_TICKS`] (confirmed: BDS join capture).
pub const OVERWORLD_TIME_MARKERS: [(u64, &str, i32); 6] = [
    (0x2470_c11c_bb08_7d2d, "minecraft:sunrise", 23_000),
    (0x4443_9fad_2758_9b66, "minecraft:night", 13_000),
    (0x5ec0_0c5c_cb0d_5ba2, "minecraft:noon", 6_000),
    (0x9c4a_fb7a_67c9_6f60, "minecraft:midnight", 18_000),
    (0xbd47_47ae_fff1_ba42, "minecraft:day", 1_000),
    (0xe745_3cee_6931_8656, "minecraft:sunset", 12_000),
];
/// Full sky light.
pub const SKY_LIGHT_MAX: u8 = 15;
/// The largest sky darkening (midnight under a clear sky); the
/// darkening is `(1 - daylight) * SKY_DARKEN_MAX` truncated (confirmed).
pub const SKY_DARKEN_MAX: f32 = 11.0;
/// Below this darkening it is day (the `is_daytime` filter; confirmed).
pub const DAYTIME_DARKEN_LIMIT: u8 = 4;
/// One above the highest block.
const MAX_Y: i32 = MIN_Y + SUBCHUNK_COUNT as i32 * 16;

/// When the clock state is next broadcast, and whether a command changed
/// the time since the last broadcast.
#[derive(Resource, Debug, Clone, Copy)]
pub struct ClockSync {
    next_sync: u64,
    changed: bool,
}

impl Default for ClockSync {
    fn default() -> Self {
        Self {
            next_sync: CLOCK_SYNC_INTERVAL_TICKS,
            changed: false,
        }
    }
}

/// Set the clock (`/time set`, `/time add`) and broadcast it this tick.
pub fn set_time(world: &mut World, ticks: i64) {
    assert!(
        (WorldTime::MIN..=WorldTime::MAX).contains(&ticks),
        "the world clock is an i32"
    );
    world.resource_mut::<WorldTime>().ticks = ticks;
    world.resource_mut::<ClockSync>().changed = true;
}

/// Set the clock after this tick's advance and broadcast it at once (the
/// night skip: BDS 1.26.51 sends the new morning itself, 24000, in the tick
/// the sleepers wake).
pub fn jump_to(world: &mut World, ticks: i64) {
    assert!(
        (WorldTime::MIN..=WorldTime::MAX).contains(&ticks),
        "the world clock is an i32"
    );
    world.resource_mut::<WorldTime>().ticks = ticks;
    let packet = McpePacket::from(clock_state_packet(*world.resource::<WorldTime>()));
    let mut sessions = world.query::<&PlayerSession>();
    for session in sessions.iter(world) {
        let _ = session.send(packet.clone());
    }
}

/// System: advance the clock, broadcast its state when it is due or was
/// changed, and publish the StartGame world state for joining players.
pub fn tick_world_clock(
    mut time: ResMut<WorldTime>,
    mut sync: ResMut<ClockSync>,
    rules: Res<GameRules>,
    difficulty: Res<Difficulty>,
    tick: Res<TickCounter>,
    start_game: Res<StartGameWorldState>,
    sessions: Query<&PlayerSession>,
) {
    if rules.do_daylight_cycle {
        time.advance();
    }
    let due = tick.current >= sync.next_sync;
    if sync.changed || due {
        let packet = McpePacket::from(clock_state_packet(*time));
        for session in &sessions {
            let _ = session.send(packet.clone());
        }
        sync.changed = false;
    }
    if due {
        sync.next_sync = tick.current + CLOCK_SYNC_INTERVAL_TICKS;
    }
    start_game.publish(*time, *difficulty, &rules);
    start_game.set_level_tick(tick.current);
}

/// The overworld clock's state (`SyncWorldClocks` sync state).
pub fn clock_state_packet(time: WorldTime) -> SyncWorldClocksPacket {
    SyncWorldClocksPacket {
        data: SyncWorldClocksPacketData::SyncStateData(SyncWorldClocksPacketPayloadSyncStateData {
            clock_data: vec![SyncWorldClockStateData {
                clock_id: OVERWORLD_CLOCK_ID,
                time: time.wire(),
                is_paused: false,
            }],
        }),
    }
}

/// The clock registry a joining player starts from (`SyncWorldClocks`
/// initialize registry).
pub fn clock_registry_packet(time: WorldTime) -> SyncWorldClocksPacket {
    let day = i32::try_from(WorldTime::DAY_TICKS).expect("a day fits an i32");
    SyncWorldClocksPacket {
        data: SyncWorldClocksPacketData::InitializeRegistryData(
            SyncWorldClocksPacketPayloadInitializeRegistryData {
                clock_data: vec![WorldClockData {
                    id: OVERWORLD_CLOCK_ID,
                    name: OVERWORLD_CLOCK_NAME.into(),
                    time: time.wire(),
                    is_paused: false,
                    time_markers: OVERWORLD_TIME_MARKERS
                        .iter()
                        .map(|&(id, name, time)| TimeMarkerData {
                            id,
                            name: name.into(),
                            time,
                            period: Some(day),
                        })
                        .collect(),
                }],
            },
        ),
    }
}

/// How far the time of day darkens the sky now (0 at noon, 11 at
/// midnight).
pub fn sky_darken(world: &World) -> u8 {
    sky_darken_at(
        *world.resource::<WorldTime>(),
        world.resource::<GameRules>().do_daylight_cycle,
    )
}

/// Whether it is day: the sky is darkened by less than 4.
pub fn is_daytime(world: &World) -> bool {
    sky_darken(world) < DAYTIME_DARKEN_LIMIT
}

/// How much of the daylight rain and lightning take away at full level:
/// the daylight is scaled by `1 - level * 5/16` for each.
///
/// [INFERENCE] The factor is Java Edition's. BDS 1.26.51 agrees where it
/// was measured (`player_life::sleep`): a bed refuses at noon 10 ticks into
/// a thunderstorm (both levels 0.1) and takes a sleeper at noon with rain
/// at 1.0 and lightning at 0.86.
const WEATHER_DAYLIGHT_LOSS: f32 = 5.0 / 16.0;

/// [`sky_darken`] with rain and lightning darkening the sky too, at their
/// levels now ([`WeatherLevels`]). Beds ask this one (see
/// `player_life::sleep`).
pub fn weather_sky_darken(world: &World) -> u8 {
    let (rain, lightning) = world
        .get_resource::<WeatherLevels>()
        .map_or((0.0, 0.0), |levels| {
            levels.at(world.resource::<TickCounter>().current)
        });
    let daylight = daylight_at(
        *world.resource::<WorldTime>(),
        world.resource::<GameRules>().do_daylight_cycle,
    ) * (1.0 - rain * WEATHER_DAYLIGHT_LOSS)
        * (1.0 - lightning * WEATHER_DAYLIGHT_LOSS);
    ((1.0 - daylight) * SKY_DARKEN_MAX) as u8
}

/// Sky light at `pos` after the time of day darkened it (block light is
/// not modelled). Below the world it is 0; above it and in unloaded chunks
/// it is full sky light, as BDS reads it.
pub fn sky_brightness(world: &World, pos: IVec3) -> u8 {
    sky_brightness_with_darken(world, pos, sky_darken(world))
}

/// [`sky_brightness`] with an explicit darkening instead of the time of
/// day's (BDS recomputes brightness this way for weather-adjusted checks).
pub fn sky_brightness_with_darken(world: &World, pos: IVec3, darken: u8) -> u8 {
    if pos.y < MIN_Y {
        return 0;
    }
    if pos.y >= MAX_Y {
        return SKY_LIGHT_MAX.saturating_sub(darken);
    }
    let (cx, cz) = world_to_chunk_coords(pos.x, pos.z);
    let Some(chunk) = world
        .resource::<ChunkManager>()
        .get_by_coords(cx, cz)
        .and_then(|entity| world.get::<ChunkData>(entity))
    else {
        return SKY_LIGHT_MAX;
    };
    let blocks = &world.resource::<BlockRegistryResource>().0;
    let (lx, _, lz) = world_to_local_coords(pos.x, pos.y, pos.z);
    let height = i32::from(chunk.inner.height_map().at(lx, lz));
    let mut sky = SKY_LIGHT_MAX;
    for y in (pos.y..height).rev() {
        let (x, ly, z) = world_to_local_coords(pos.x, y, pos.z);
        let filter = blocks
            .get_by_runtime_id(chunk.inner.get_block(x, ly, z))
            .map_or(0, |entry| entry.data.filter_light);
        sky = sky.saturating_sub(filter);
        if sky == 0 {
            return 0;
        }
    }
    sky.saturating_sub(darken)
}

/// The darkening for a clock value. `cycle` is `doDaylightCycle`: while it
/// runs, the time of day is taken one tick ahead (the partial tick BDS
/// passes is 1 then, 0 otherwise).
fn sky_darken_at(time: WorldTime, cycle: bool) -> u8 {
    // Truncation, as the float-to-int conversion does.
    ((1.0 - daylight_at(time, cycle)) * SKY_DARKEN_MAX) as u8
}

/// The daylight of a clear sky for a clock value, `0..=1` (see
/// [`sky_darken_at`] for `cycle`).
fn daylight_at(time: WorldTime, cycle: bool) -> f32 {
    let partial = if cycle { 1.0 } else { 0.0 };
    let day = time_of_day(time, partial);
    let cos = table_cos((day * PI + day * PI) * TABLE_SCALE);
    1.0 - (1.0 - (cos * 2.0 + 0.5)).clamp(0.0, 1.0)
}

/// The sun's progress through the day in `0..1`, 0 being noon, eased the
/// way Bedrock eases it.
fn time_of_day(time: WorldTime, partial: f32) -> f32 {
    let day_ticks = WorldTime::DAY_TICKS as f32;
    let mut day = (time.time_of_day() as f32 + partial) / day_ticks - 0.25;
    if day < 0.0 {
        day += 1.0;
    }
    if day > 1.0 {
        day -= 1.0;
    }
    let cos = table_cos(PI * day * TABLE_SCALE);
    ((cos + 1.0) * -0.5 + 1.0 - day) / 3.0 + day
}

/// Table entries per radian of Bedrock's 65536-entry sine table.
const TABLE_SCALE: f32 = 10_430.378;

/// Cosine from Bedrock's sine table, for an angle already scaled by
/// [`TABLE_SCALE`].
fn table_cos(scaled: f32) -> f32 {
    let index = (scaled + 16_384.0) as i32 & 0xffff;
    (f64::from(index) * std::f64::consts::TAU / 65_536.0).sin() as f32
}

#[cfg(test)]
mod tests;
