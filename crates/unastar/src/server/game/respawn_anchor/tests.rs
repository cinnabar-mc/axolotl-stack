//! Respawn anchors against BDS 1.26.51 (`tnt_anchor.json`): charging with
//! glowstone, and the explosion of a charged anchor clicked in the
//! overworld.

use bevy_ecs::prelude::*;
use glam::{DVec3, IVec3};
use serde::Deserialize;

use crate::entity::components::{GameMode, HeldSlot, MainInventory, Player, Position};
use crate::item::test_support::stack;
use crate::registry::BlockRegistry;
use crate::registry::block::StateValue;
use crate::server::game::blocks::{block_at, set_block};
use crate::world::chunk::Chunk;
use crate::world::ecs::{ChunkData, ChunkManager, ChunkPosition};

#[derive(Debug, Deserialize)]
struct Table {
    rows: Vec<AnchorRow>,
}

#[derive(Debug, Deserialize)]
struct AnchorRow {
    name: serde_json::Value,
    /// A charging cell's anchor: `[tick, states]` at each change.
    #[serde(default)]
    anchor: Vec<(u64, serde_json::Value)>,
    /// An exploding cell's cells before (the non-air ones).
    #[serde(default)]
    solid_before: Vec<([i32; 3], String)>,
    #[serde(default)]
    explosions: Vec<Explosion>,
}

#[derive(Debug, Deserialize)]
struct Explosion {
    blocks: Vec<[i32; 3]>,
}

fn rows() -> Vec<AnchorRow> {
    let path = format!(
        "{}/../unastar-data/data/measured/explosions/tnt_anchor.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str::<Table>(&text)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .rows
}

fn id(name: &str) -> u32 {
    BlockRegistry::vanilla()
        .default_state_id_by_name(name)
        .unwrap_or_else(|| panic!("{name}"))
}

fn anchor(charge: i32) -> u32 {
    let entry = BlockRegistry::vanilla()
        .get_by_name("minecraft:respawn_anchor")
        .unwrap();
    entry
        .with_state_value(
            entry.default_state_id,
            "respawn_anchor_charge",
            StateValue::Int(charge),
        )
        .unwrap()
}

/// The anchor's cell origin, on a grass floor at y = 63.
const ORIGIN: IVec3 = IVec3::new(8, 64, 8);

/// A server with a grass floor around the origin and a survival player 4
/// blocks east holding `held`.
fn server(held: &str, count: u8) -> (crate::GameServer, Entity) {
    let mut server = crate::GameServer::with_config(crate::ServerConfig {
        world: crate::world::WorldConfig::void_spawn_platform(1),
        ..crate::ServerConfig::default()
    });
    let world = server.ecs.world_mut();
    world.insert_resource(crate::entity::EntityRandom::seeded(7));
    world.insert_resource(crate::server::game::explosion::ExplosionRandom::seeded(7));
    for cx in -1..=1 {
        for cz in -1..=1 {
            let mut column = Chunk::new(cx, cz);
            for x in 0..16 {
                for z in 0..16 {
                    column.set_block(x, 63, z, id("minecraft:grass_block"));
                    column.set_block(x, 62, z, id("minecraft:dirt"));
                    column.set_block(x, 61, z, id("minecraft:dirt"));
                    column.set_block(x, 60, z, id("minecraft:bedrock"));
                }
            }
            let chunk = world.spawn(ChunkData::new(column)).id();
            world
                .resource_mut::<ChunkManager>()
                .insert(ChunkPosition::new(cx, cz), chunk);
        }
    }
    let mut main = MainInventory::default();
    main.0.set_item(0, stack(held, count)).unwrap();
    let feet = ORIGIN.as_dvec3() + DVec3::new(4.5, 0.0, 0.5);
    let player = world
        .spawn((
            Player,
            GameMode::Survival,
            Position(feet + DVec3::Y * crate::entity::PLAYER_EYE_HEIGHT),
            main,
            HeldSlot(0),
        ))
        .id();
    (server, player)
}

fn at(world: &World, offset: IVec3) -> Option<u32> {
    let pos = ORIGIN + offset;
    block_at(world, (pos.x, pos.y, pos.z))
}

/// Glowstone adds a charge per click up to four and is used up; a click on
/// a full anchor with glowstone explodes it (BDS: charges 0 to 4 clicked
/// twice).
#[test]
fn glowstone_charges_an_anchor_like_bds() {
    for row in rows().iter().filter(|row| row.name[0] == "charge") {
        let start = row.name[1].as_i64().unwrap() as i32;
        let (mut server, player) = server("minecraft:glowstone", 2);
        let world = server.ecs.world_mut();
        set_block(world, ORIGIN, anchor(start), false);
        let mut ours = Vec::new();
        for _ in 0..2 {
            if at(world, IVec3::ZERO) == Some(id("minecraft:air")) {
                break;
            }
            super::used(world, player, ORIGIN);
            ours.push(at(world, IVec3::ZERO));
        }
        // BDS: the anchor's state after each click (its trace after the
        // first read).
        let theirs: Vec<Option<u32>> = row.anchor[1..]
            .iter()
            .map(|(_, states)| {
                Some(match states.get("respawn_anchor_charge") {
                    Some(charge) => anchor(charge.as_i64().unwrap() as i32),
                    None => id("minecraft:air"),
                })
            })
            .collect();
        let compared = ours.len().min(theirs.len());
        assert_eq!(ours[..compared], theirs[..compared], "{:?}", row.name);
        let used = (start..start + 2).filter(|c| *c < 4).count() as u8;
        let left = world
            .get::<MainInventory>(player)
            .and_then(|inventory| inventory.0.item(0).cloned())
            .map_or(0, |stack| stack.count);
        if start + 1 < 4 {
            assert_eq!(left, 2 - used, "{:?}", row.name);
        }
    }
}

/// A charged anchor clicked with anything else explodes: the anchor and
/// every dirt block two blocks out are gone (BDS: all 16 in all 12 cells),
/// and fire stands only on solid blocks.
#[test]
fn a_charged_anchor_explodes_like_bds() {
    let rows = rows();
    let exploding: Vec<&AnchorRow> = rows
        .iter()
        .filter(|row| !row.explosions.is_empty() && row.name[0] != "charge")
        .collect();
    assert_eq!(exploding.len(), 12);
    let ring: Vec<IVec3> = exploding[0]
        .solid_before
        .iter()
        .filter(|(pos, block)| pos[1] == 0 && block == "dirt")
        .map(|(pos, _)| IVec3::from_array(*pos))
        .collect();
    assert_eq!(ring.len(), 16);
    for row in &exploding {
        let broken: Vec<IVec3> = row.explosions[0]
            .blocks
            .iter()
            .map(|p| IVec3::from_array(*p))
            .collect();
        assert!(
            ring.iter().all(|pos| broken.contains(pos)),
            "{:?}",
            row.name
        );
    }
    let (mut server, player) = server("minecraft:stick", 1);
    let world = server.ecs.world_mut();
    set_block(world, ORIGIN, anchor(1), false);
    for pos in &ring {
        set_block(world, ORIGIN + *pos, id("minecraft:dirt"), false);
    }
    assert_eq!(super::used(world, player, ORIGIN), Some(false));
    let air = Some(id("minecraft:air"));
    assert_eq!(at(world, IVec3::ZERO), air, "the anchor is gone");
    for pos in &ring {
        assert_ne!(at(world, *pos), Some(id("minecraft:dirt")), "{pos}");
    }
    let fire = Some(id("minecraft:fire"));
    let mut fires = 0;
    for x in -6..=6 {
        for y in -3..=3 {
            for z in -6..=6 {
                let pos = IVec3::new(x, y, z);
                if at(world, pos) == fire {
                    fires += 1;
                    assert_ne!(at(world, pos - IVec3::Y), air, "fire over air at {pos}");
                }
            }
        }
    }
    assert!(fires > 0, "no fire");
}

/// An empty anchor ignores a click without glowstone (the click goes on).
#[test]
fn an_empty_anchor_ignores_other_items() {
    let (mut server, player) = server("minecraft:stick", 1);
    let world = server.ecs.world_mut();
    set_block(world, ORIGIN, anchor(0), false);
    assert_eq!(super::used(world, player, ORIGIN), None);
    assert_eq!(at(world, IVec3::ZERO), Some(anchor(0)));
}
