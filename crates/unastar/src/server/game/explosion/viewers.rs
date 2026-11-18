//! What viewers are sent when something explodes (BDS 1.26.51 capture,
//! `testdata/bds-placements-tnt.json`): `LevelEvent` 2025 at the centre
//! with the power as data, `LevelEventGeneric` 2026 with the centre, the
//! power and one in eight broken positions, and the explosion's level
//! sound (a pushed player's motion goes with its hurt,
//! `player_life::hurt::push_player`).

use bevy_ecs::prelude::*;
use bytes::{BufMut, BytesMut};
use glam::{DVec3, IVec3};
use jolyne::valentine::types::Vec3;
use jolyne::valentine::{
    LevelEventGenericPacket, LevelEventPacket, LevelSoundEventPacket, McpePacket,
};
use rand::Rng;
use valentine::bedrock::codec::NbtCompoundBody;

use super::{Blast, ExplosionRandom};
use crate::server::game::block_actors;

/// `LevelEvent` of the explosion's particles (data: the power).
const PARTICLES_EXPLOSION: i32 = 2025;
/// `LevelEventGeneric` of the broken blocks' particles.
const PARTICLES_BLOCK_EXPLOSION: i32 = 2026;
/// One broken position in this many goes into the block particles.
const PARTICLE_CHANCE: u32 = 8;

fn wire(v: DVec3) -> Vec3 {
    Vec3 {
        x: v.x as f32,
        y: v.y as f32,
        z: v.z as f32,
    }
}

/// The packets of `blast` breaking `blocks`, to the viewers of its chunk.
pub(super) fn explosion(world: &mut World, blast: &Blast, blocks: &[IVec3]) {
    let sampled: Vec<IVec3> = {
        let mut rng = world.get_resource_or_init::<ExplosionRandom>();
        blocks
            .iter()
            .copied()
            .filter(|_| rng.0.gen_range(0..PARTICLE_CHANCE) == 0)
            .collect()
    };
    let at = blast.centre.floor().as_ivec3();
    let packets = [
        McpePacket::from(LevelEventPacket {
            event_id: PARTICLES_EXPLOSION,
            position: wire(blast.centre),
            data: blast.power as i32,
        }),
        McpePacket::from(LevelEventGenericPacket {
            event_id: PARTICLES_BLOCK_EXPLOSION,
            __ctd__: block_particles(blast, &sampled),
        }),
        McpePacket::from(LevelSoundEventPacket {
            sound_event: blast.sound.as_ref().into(),
            position: wire(blast.centre),
            data: -1,
            actor_identifier: "".into(),
            is_baby: false,
            is_global: false,
            actor_unique_id: -1,
            fire_at_position: None,
        }),
    ];
    for packet in &packets {
        block_actors::broadcast(world, at, packet, None);
    }
}

/// `LevelEvent` of the fuse sound a newly lit TNT plays (data 0).
const FUSE_SOUND: i32 = 1005;

/// `LevelEvent` a fire charge lighting a block sends at a block corner, with
/// this data (BDS 1.26.51: the TNT's own corner in `tnt_lit.json`, the
/// corner of the block on a candle's clicked face in
/// `data/measured/candles/candles_lit.json`; flint and steel sends none).
const FIRE_CHARGE_LIT: i32 = 1008;
const FIRE_CHARGE_LIT_DATA: i32 = 78642;

/// The event of a fire charge lighting a block, at the corner of `pos`.
pub(crate) fn fire_charge_lit(world: &mut World, pos: IVec3) {
    let packet = McpePacket::from(LevelEventPacket {
        event_id: FIRE_CHARGE_LIT,
        position: wire(pos.as_dvec3()),
        data: FIRE_CHARGE_LIT_DATA,
    });
    block_actors::broadcast(world, pos, &packet, None);
}

/// The fuse sound at the feet of a TNT just lit.
pub(super) fn fuse(world: &mut World, feet: DVec3) {
    let packet = McpePacket::from(LevelEventPacket {
        event_id: FUSE_SOUND,
        position: wire(feet),
        data: 0,
    });
    block_actors::broadcast(world, feet.floor().as_ivec3(), &packet, None);
}

fn put_varint(body: &mut BytesMut, mut value: u32) {
    while value >= 0x80 {
        body.put_u8((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    body.put_u8(value as u8);
}

/// The block particles' compound, its fields in name order: `originX`,
/// `originY`, `originZ`, `pos<i>x`/`y`/`z` per sampled position, `radius`
/// (floats) and `size` (an int, the number of positions).
fn block_particles(blast: &Blast, sampled: &[IVec3]) -> NbtCompoundBody {
    const FLOAT: u8 = 5;
    const INT: u8 = 3;
    enum Value {
        Float(f32),
        Int(i32),
    }
    let centre = blast.centre.as_vec3();
    let mut fields: Vec<(String, Value)> = vec![
        ("originX".into(), Value::Float(centre.x)),
        ("originY".into(), Value::Float(centre.y)),
        ("originZ".into(), Value::Float(centre.z)),
        ("radius".into(), Value::Float(blast.power)),
        ("size".into(), Value::Int(sampled.len() as i32)),
    ];
    for (index, pos) in sampled.iter().enumerate() {
        for (axis, value) in [("x", pos.x), ("y", pos.y), ("z", pos.z)] {
            fields.push((format!("pos{index}{axis}"), Value::Float(value as f32)));
        }
    }
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    let mut body = BytesMut::new();
    for (name, value) in fields {
        body.put_u8(match value {
            Value::Float(_) => FLOAT,
            Value::Int(_) => INT,
        });
        put_varint(&mut body, name.len() as u32);
        body.put_slice(name.as_bytes());
        match value {
            Value::Float(float) => body.put_f32_le(float),
            Value::Int(int) => put_varint(&mut body, ((int << 1) ^ (int >> 31)) as u32),
        }
    }
    body.put_u8(0);
    NbtCompoundBody(body.freeze())
}
