//! The round-2 TNT sweeps (`data/measured/explosions/tnt_*.json`, written by
//! `tools/bds-sweep/explosions_table.py`): waterlogged blocks, drops, decay,
//! hits and pushes on mobs and players, water, lit TNT, chained fuses and a
//! respawn anchor's fire.

use std::collections::HashSet;

use glam::{DVec2, DVec3, IVec3};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::Deserialize;

use super::super::{
    Blast, ExplosionBlocks, GatheredDrops, blast_centre, blasted_blocks, damage, distance,
    exposure, impact, push,
};
use super::{BLAST_FEET, Cell, TNT_HEIGHT, TNT_POWER, frequencies, registry};
use crate::entity::PLAYER_EYE_HEIGHT;
use crate::entity::mob::combat::knockback_velocity;
use crate::entity::mob::components::KnockbackPower;
use crate::entity::mob::constants::{PLAYER_BOX_HEIGHT, PLAYER_BOX_WIDTH};
use crate::item::test_support::stack;
use crate::registry::collision::Aabb;

#[derive(Debug, Deserialize)]
struct Table<R> {
    rows: Vec<R>,
}

fn table<R: for<'de> Deserialize<'de>>(family: &str) -> Vec<R> {
    let path = format!(
        "{}/../unastar-data/data/measured/explosions/{family}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str::<Table<R>>(&text)
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .rows
}

/// A cell offset as the tables write it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(from = "[i32; 3]")]
struct Offset(IVec3);

impl From<[i32; 3]> for Offset {
    fn from(v: [i32; 3]) -> Self {
        Self(IVec3::from_array(v))
    }
}

/// A position or motion as the tables write it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(from = "[f64; 3]")]
struct Point(DVec3);

impl From<[f64; 3]> for Point {
    fn from(v: [f64; 3]) -> Self {
        Self(DVec3::from_array(v))
    }
}

#[derive(Debug, Deserialize)]
struct Explosion {
    #[serde(default)]
    blocks: Vec<Offset>,
}

impl Explosion {
    fn blocks(&self) -> impl Iterator<Item = IVec3> + '_ {
        self.blocks.iter().map(|o| o.0)
    }
}

#[derive(Debug, Deserialize)]
struct Target {
    #[serde(rename = "type")]
    kind: String,
    feet: Point,
    moved: Option<Point>,
}

#[derive(Debug, Deserialize)]
struct DamageEvent {
    #[serde(rename = "type")]
    kind: String,
    damage: f64,
    cause: String,
    attacker: Option<String>,
    damaging: Option<String>,
}

/// A cell of the hit families (`tnt_knock`, `tnt_player`, `tnt_water`,
/// `tnt_lit`).
#[derive(Debug, Deserialize)]
struct HitRow {
    name: serde_json::Value,
    explosions: Vec<Explosion>,
    tnt: Option<Point>,
    #[serde(default)]
    targets: Vec<Target>,
    damage_events: Vec<DamageEvent>,
    #[serde(default)]
    player_motions: Vec<Point>,
}

/// A blast cube cell (`tnt_logged`, `tnt_drops`, `tnt_decay`).
#[derive(Debug, Deserialize)]
struct CubeRow {
    name: serde_json::Value,
    explosions: Vec<Explosion>,
    items: Vec<(String, u8)>,
}

/// The blast cube's slab spots (`tools/bds-sweep/tnt.py` `SLAB_SPOTS`).
const SLAB_SPOTS: [IVec3; 6] = [
    IVec3::new(-2, 1, 0),
    IVec3::new(0, 1, 2),
    IVec3::new(0, 1, -2),
    IVec3::new(0, 3, 0),
    IVec3::new(0, -1, 0),
    IVec3::new(2, 1, 2),
];

fn in_cube(pos: IVec3) -> bool {
    pos.x.abs() <= 4 && pos.z.abs() <= 4 && (-3..=5).contains(&pos.y)
}

/// The dirt cube with oak slabs at the slab spots, waterlogged or dry.
fn slab_cell(logged: bool) -> Cell {
    let mut cell = super::blast_cell("dirt");
    for spot in SLAB_SPOTS {
        cell.set(spot, "oak_slab");
        if logged {
            cell.waterlog(spot);
        }
    }
    cell
}

/// A waterlogged slab never breaks and is never in BDS's list (16 of 16
/// cells), while a dry one at the same spot always does; the rest of the
/// cube breaks about as often as in BDS (within 0.35 of its 16 cells).
#[test]
fn waterlogged_blocks_never_break_like_bds() {
    let rows: Vec<CubeRow> = table("tnt_logged");
    let ids = ExplosionBlocks::resolve(registry()).unwrap();
    for logged in [false, true] {
        let bds: Vec<Vec<IVec3>> = rows
            .iter()
            .filter(|row| row.name[0] == logged)
            .map(|row| {
                // BDS's list, not the cells read as air: water from the
                // slabs flows into the broken cells before the read.
                let mut out: Vec<IVec3> = row.explosions[0]
                    .blocks()
                    .filter(|pos| in_cube(*pos) && *pos != IVec3::new(0, 1, 0))
                    .collect();
                out.push(IVec3::new(0, 1, 0));
                out
            })
            .collect();
        assert_eq!(bds.len(), 16, "logged {logged}");
        for row in rows.iter().filter(|row| row.name[0] == logged) {
            let listed: HashSet<IVec3> = row.explosions[0].blocks().collect();
            for spot in SLAB_SPOTS {
                assert_eq!(listed.contains(&spot), !logged, "{:?} {spot}", row.name);
            }
        }
        let cell = slab_cell(logged);
        let mut rng = StdRng::seed_from_u64(0x1066);
        let ours: Vec<Vec<IVec3>> = (0..256)
            .map(|_| {
                let mut out: Vec<IVec3> = blasted_blocks(
                    &cell,
                    &ids,
                    blast_centre(BLAST_FEET, TNT_HEIGHT),
                    TNT_POWER,
                    f32::MAX,
                    &mut rng,
                    None,
                )
                .into_iter()
                .filter(|pos| in_cube(*pos))
                .collect();
                out.retain(|pos| *pos != IVec3::new(0, 1, 0));
                out.push(IVec3::new(0, 1, 0));
                out
            })
            .collect();
        let (theirs, mine) = (frequencies(&bds), frequencies(&ours));
        for spot in SLAB_SPOTS {
            let expected = if logged { 0.0 } else { 1.0 };
            assert_eq!(
                theirs.get(&spot).copied().unwrap_or(0.0),
                expected,
                "BDS {spot}"
            );
            assert_eq!(
                mine.get(&spot).copied().unwrap_or(0.0),
                expected,
                "unastar {spot}"
            );
        }
        let mut off: Vec<String> = theirs
            .keys()
            .chain(mine.keys())
            .collect::<HashSet<_>>()
            .into_iter()
            .filter_map(|pos| {
                let (b, u) = (
                    *theirs.get(pos).unwrap_or(&0.0),
                    *mine.get(pos).unwrap_or(&0.0),
                );
                // 16 BDS cells: a rate of 0.5 has a standard error of 0.125.
                ((b - u).abs() > 0.35).then(|| format!("{pos}: BDS {b:.2} unastar {u:.2}"))
            })
            .collect();
        off.sort();
        assert!(
            off.is_empty(),
            "logged {logged}:
{}",
            off.join(
                "
"
            )
        );
    }
}

/// What each block of BDS's list dropped in the blast cube: the cube's
/// material, the floor's dirt (grass drops dirt), the redstone block; air
/// and the TNT's own cell nothing.
fn cube_drop(material: &str, pos: IVec3) -> Option<&'static str> {
    if pos == IVec3::new(1, 1, 0) {
        return Some("minecraft:redstone_block");
    }
    if pos == IVec3::new(0, 1, 0) {
        return None;
    }
    if in_cube(pos) {
        return Some(match material {
            "cobblestone" => "minecraft:cobblestone",
            _ => "minecraft:dirt",
        });
    }
    (-3..=-1).contains(&pos.y).then_some("minecraft:dirt")
}

/// BDS's drops gathered in the order of its list of broken blocks make the
/// stacks BDS popped (31 of 32 cells exactly; in the other the redstone
/// block's drop comes one block later than its place in the list, which
/// moves one item between two stacks).
#[test]
fn drops_gather_into_stacks_like_bds() {
    let rows: Vec<CubeRow> = table("tnt_drops");
    let mut exact = 0;
    for row in &rows {
        let material = row.name[0].as_str().unwrap();
        let mut gathered = GatheredDrops::default();
        for pos in row.explosions[0].blocks() {
            if let Some(item) = cube_drop(material, pos) {
                gathered.add(pos, stack(item, 1));
            }
        }
        let mut ours: Vec<u8> = gathered
            .into_stacks()
            .iter()
            .map(|(_, stack)| stack.count)
            .collect();
        let mut theirs: Vec<u8> = row.items.iter().map(|(_, count)| *count).collect();
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(
            ours.iter().map(|&c| u32::from(c)).sum::<u32>(),
            theirs.iter().map(|&c| u32::from(c)).sum::<u32>(),
            "{:?}",
            row.name
        );
        if ours == theirs {
            exact += 1;
        } else {
            let off: i32 = ours
                .iter()
                .zip(&theirs)
                .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
                .sum();
            assert_eq!(off, 2, "{:?}: unastar {ours:?}, BDS {theirs:?}", row.name);
        }
    }
    assert_eq!(rows.len(), 32);
    assert!(exact >= 31, "{exact} of 32 exact");
}

/// With `tntExplosionDropDecay` on, a TNT keeps a block's drop one time in
/// four (BDS: 879 of 3453 broken blocks).
#[test]
fn decayed_drops_are_one_in_the_power_like_bds() {
    let rows: Vec<CubeRow> = table("tnt_decay");
    let (mut broken, mut kept) = (0.0, 0.0);
    for row in &rows {
        broken += row.explosions[0]
            .blocks()
            .filter(|pos| cube_drop("dirt", *pos).is_some())
            .count() as f64;
        kept += row.items.iter().map(|(_, n)| f64::from(*n)).sum::<f64>();
    }
    let chance = f64::from(Blast::decayed_drops(TNT_POWER));
    let rate = kept / broken;
    let se = (chance * (1.0 - chance) / broken).sqrt();
    assert!(
        (rate - chance).abs() < 3.0 * se,
        "BDS keeps {rate:.4}, unastar {chance}"
    );
}

/// The cell of a hit row: the redstone block north of the TNT and a stone
/// under a raised target.
fn hit_cell(feet: DVec3) -> Cell {
    let mut cell = Cell::new();
    cell.set(IVec3::new(0, 0, -1), "redstone_block");
    if feet.y >= 1.0 {
        cell.set(feet.floor().as_ivec3() - IVec3::Y, "stone");
    }
    cell
}

/// Box size of the measured kinds.
fn size(kind: &str) -> (f64, f64) {
    match kind {
        "pig" => (0.9, 0.9),
        "husk" => (0.6, 1.9),
        "iron_golem" => (1.4, 2.9),
        "player" => (PLAYER_BOX_WIDTH, PLAYER_BOX_HEIGHT),
        other => panic!("{other}"),
    }
}

/// Impact and damage unastar gives `kind` standing at `feet`.
fn predicted(cell: &Cell, centre: DVec3, kind: &str, feet: DVec3) -> (f64, f64) {
    let (width, height) = size(kind);
    let aabb = Aabb::from_feet(feet, width, height);
    let seen = exposure(cell, centre, &aabb);
    let reach = if kind == "player" {
        centre.distance(feet)
    } else {
        distance(centre, feet, width)
    };
    let impact = impact(reach, TNT_POWER, seen);
    (impact, f64::from(damage(impact, TNT_POWER)))
}

fn eyes(kind: &str, feet: DVec3) -> DVec3 {
    if kind == "player" {
        feet + DVec3::Y * PLAYER_EYE_HEIGHT
    } else {
        feet + DVec3::Y * size(kind).1 * super::super::PUSH_HEIGHT_RATIO
    }
}

/// The hurt knockback of a still target away from `from`.
fn hurt_knockback(feet: DVec3, from: DVec3) -> DVec3 {
    let away = DVec2::new(feet.x - from.x, feet.z - from.z).normalize();
    knockback_velocity(DVec3::ZERO, away, KnockbackPower::DEFAULT)
}

/// Damage and the first motion of mobs and the player around a TNT: the
/// damage is the formula's from a mob's distance less half its width and a
/// player's plain distance (within a point: exposure is sampled), and
/// the first tick's movement is the hurt knockback away from the TNT plus
/// the push towards the eyes (within 0.03; a full knockback resistance
/// stops both). Husks in netherite are left out (BDS reports their damage
/// after armour, and armour's knockback resistance on mobs is not
/// modelled), as are pigs raised on a block (exposure gap).
#[test]
fn explosions_hurt_and_push_like_bds() {
    let mut compared = (0, 0);
    let mut failures = Vec::new();
    for family in ["tnt_knock", "tnt_player"] {
        for row in table::<HitRow>(family) {
            let armoured = family == "tnt_knock" && row.name[0][2].as_u64() != Some(0);
            let Some(Point(tnt)) = row.tnt else { continue };
            let centre = blast_centre(tnt, TNT_HEIGHT);
            let mut targets: Vec<(String, DVec3, Option<DVec3>)> = row
                .targets
                .iter()
                .map(|t| (t.kind.clone(), t.feet.0, t.moved.map(|m| m.0)))
                .collect();
            if family == "tnt_player" {
                let d = row.name[0][0].as_f64().unwrap();
                targets.push(("player".into(), DVec3::new(d + 0.5, 0.0, 0.5), None));
            }
            for (kind, feet, moved) in targets {
                if feet.y >= 1.0 {
                    // On a block the exposure is off (BDS sees more of a pig
                    // raised on stone: a known gap of the exposure sampling).
                    continue;
                }
                let cell = hit_cell(feet);
                let (impact, dealt) = predicted(&cell, centre, &kind, feet);
                let event = row.damage_events.iter().find(|e| e.kind == kind);
                if let Some(event) = event.filter(|_| !armoured) {
                    compared.0 += 1;
                    if (event.damage - dealt).abs() > 1.0 {
                        failures.push(format!(
                            "{family} {:?} {kind}: BDS {} unastar {dealt:.3}",
                            row.name, event.damage
                        ));
                    }
                }
                if armoured {
                    continue;
                }
                let resistance = if kind == "iron_golem" { 1.0 } else { 0.0 };
                let shove = push(centre, eyes(&kind, feet), impact, resistance);
                let knock = if resistance >= 1.0 {
                    DVec3::ZERO
                } else {
                    hurt_knockback(feet, tnt)
                };
                let actual = if kind == "player" {
                    // BDS sends the hurt knockback, then the velocity with
                    // the push added.
                    match row.player_motions.as_slice() {
                        [first, total] => Some((first.0 + shove, total.0)),
                        _ => None,
                    }
                } else {
                    moved.map(|moved| (knock + shove, moved))
                };
                if let Some((ours, theirs)) = actual {
                    compared.1 += 1;
                    if (ours - theirs).abs().max_element() > 0.03 {
                        failures.push(format!(
                            "{family} {:?} {kind}: BDS moves {theirs}, unastar {ours}",
                            row.name
                        ));
                    }
                }
            }
        }
    }
    assert!(
        compared.0 >= 20 && compared.1 >= 20,
        "compared {compared:?}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A TNT exploding in water breaks nothing and hurts with 0 damage, while
/// its push still moves the pig floating above it (BDS `tnt_water.json`).
#[test]
fn tnt_in_water_breaks_nothing_and_deals_no_damage_like_bds() {
    let rows: Vec<HitRow> = table("tnt_water");
    assert_eq!(rows.len(), 4);
    for row in rows {
        assert!(row.explosions[0].blocks.is_empty());
        assert!(!row.damage_events.is_empty());
        for event in &row.damage_events {
            assert_eq!(event.damage, 0.0, "{:?}", row.name);
            assert_eq!(event.cause, "entity_explosion");
        }
        let pig = row.targets.iter().find(|t| t.kind == "pig").unwrap();
        let tnt = row.tnt.unwrap().0;
        let feet = pig.feet.0;
        let centre = blast_centre(tnt, TNT_HEIGHT);
        let (impact, _) = predicted(&Cell::new(), centre, "pig", feet);
        let ours = hurt_knockback(feet, tnt) + push(centre, eyes("pig", feet), impact, 0.0);
        let theirs = pig.moved.unwrap().0;
        // The pig floats: its first move also carries the water's lift.
        assert!(
            (ours.x - theirs.x).abs() < 0.05 && (ours.z - theirs.z).abs() < 0.05,
            "{:?}: BDS {theirs}, unastar {ours}",
            row.name
        );
    }
}

/// A TNT a player lit names the player as attacker and the TNT as the
/// damaging actor; a TNT lit by power names itself (BDS `tnt_lit.json`,
/// `tnt_knock.json`).
#[test]
fn lit_tnt_names_its_igniter_like_bds() {
    for row in table::<HitRow>("tnt_lit") {
        for event in &row.damage_events {
            if event.cause == "entity_explosion" {
                assert_eq!(event.attacker.as_deref(), Some("player"), "{:?}", row.name);
                assert_eq!(event.damaging.as_deref(), Some("tnt"));
            }
        }
    }
    for row in table::<HitRow>("tnt_knock") {
        for event in &row.damage_events {
            assert_eq!(event.attacker.as_deref(), Some("tnt"), "{:?}", row.name);
        }
    }
    // unastar: the hurt names the igniter (a player hit, not scaled by the
    // difficulty) and is knocked away from it.
    let mut world = bevy_ecs::world::World::new();
    let player = world
        .spawn((
            crate::entity::components::Player,
            crate::entity::components::Position(DVec3::new(2.5, PLAYER_EYE_HEIGHT, 0.5)),
        ))
        .id();
    let tnt = world.spawn_empty().id();
    let blast = Blast {
        centre: DVec3::new(0.5, 0.06, 0.5),
        power: TNT_POWER,
        source: Some(super::super::BlastSource {
            entity: tnt,
            feet: DVec3::new(0.5, 0.0, 0.5),
            in_water: false,
            igniter: Some(player),
        }),
        breaks_blocks: true,
        causes_fire: false,
        max_resistance: f32::MAX,
        damage_scaling: 1.0,
        knockback_scaling: 1.0,
        drop_chance: 1.0,
        sound: "explode".into(),
    };
    let (hit, _) = super::super::hit_for(&world, &blast, 2.0, 1.0);
    assert_eq!(hit.attacker, Some(player));
    assert!(hit.by_player);
    assert_eq!(hit.knockback_from, Some(DVec3::new(2.5, 0.0, 0.5)));
}

/// At peaceful a TNT hurts no player (no damage event; only the push's
/// motion), while a respawn anchor's explosion does (BDS `tnt_player.json`,
/// `tnt_anchor.json`).
#[test]
fn peaceful_spares_players_only_from_sourced_explosions_like_bds() {
    for row in table::<HitRow>("tnt_player") {
        let peaceful = row.name[0][1] == "peaceful";
        let hurt = row.damage_events.iter().any(|e| e.kind == "player");
        assert_eq!(hurt, !peaceful, "{:?}", row.name);
        assert_eq!(row.player_motions.len(), if peaceful { 1 } else { 2 });
    }
    let anchors: Vec<HitRow> = table("tnt_anchor");
    let peaceful = anchors
        .iter()
        .find(|row| row.name[1] == 4 && row.name[2] == "peaceful")
        .expect("the peaceful anchor");
    assert!(
        peaceful
            .damage_events
            .iter()
            .any(|e| e.kind == "player" && e.cause == "block_explosion" && e.damage > 30.0)
    );
}

/// A TNT primed by another's explosion explodes 11 to 40 ticks after it,
/// spread evenly (BDS: 64 chains, mean 25.4): unastar's `from_explosion`
/// fuses over 400 primings, compared by range and mean.
#[test]
fn chained_fuses_spread_like_bds() {
    #[derive(Deserialize)]
    struct ChainRow {
        fuse: Option<u64>,
    }
    let bds: Vec<f64> = table::<ChainRow>("tnt_chain")
        .into_iter()
        .filter_map(|row| row.fuse)
        .map(|f| f as f64)
        .collect();
    assert_eq!(bds.len(), 64);
    let mut server = crate::GameServer::with_config(crate::ServerConfig {
        world: crate::world::WorldConfig::void_spawn_platform(1),
        ..crate::ServerConfig::default()
    });
    let world = server.ecs.world_mut();
    world.insert_resource(crate::entity::EntityRandom::seeded(5));
    let now = world
        .resource::<crate::ecs::resources::TickCounter>()
        .current;
    let mut ours = Vec::new();
    for n in 0..400 {
        let tnt = super::super::prime_tnt(world, IVec3::new(n, 64, 0), Some("from_explosion"))
            .expect("primed");
        let fuse = world
            .get::<crate::entity::mob::components::explode::Fuse>(tnt)
            .expect("a lit fuse");
        // It explodes the tick after its fuse runs out.
        ours.push((fuse.explode_at - now + 1) as f64);
        world.despawn(tnt);
    }
    let (bds_min, bds_max) = min_max(&bds);
    let (our_min, our_max) = min_max(&ours);
    assert!(our_min >= 11.0 && our_max <= 41.0, "{our_min}..{our_max}");
    assert!(
        bds_min >= our_min && bds_max <= our_max,
        "BDS {bds_min}..{bds_max}"
    );
    let (bds_mean, bds_sd) = super::mean_and_sd(&bds);
    let (our_mean, our_sd) = super::mean_and_sd(&ours);
    let se = (bds_sd.powi(2) / bds.len() as f64 + our_sd.powi(2) / ours.len() as f64).sqrt();
    assert!(
        (bds_mean - our_mean).abs() < 3.0 * se,
        "BDS {bds_mean:.2}, unastar {our_mean:.2}"
    );
}

fn min_max(values: &[f64]) -> (f64, f64) {
    (
        values.iter().copied().fold(f64::MAX, f64::min),
        values.iter().copied().fold(f64::MIN, f64::max),
    )
}

/// A respawn anchor's fire: one in three of the cells its explosion
/// reached that are air afterwards and stand on a solid block catch fire
/// (BDS: 254 of 709, unastar's chance `FIRE_CHANCE`), and no fire stands
/// over air.
#[test]
fn explosion_fire_catches_one_in_three_like_bds() {
    #[derive(Deserialize)]
    struct AnchorRow {
        explosions: Vec<Explosion>,
        #[serde(default)]
        solid_before: Vec<(Offset, String)>,
        #[serde(default)]
        fire_after: Vec<Offset>,
    }
    let watched = |pos: IVec3| (-2..=2).contains(&pos.y) && pos.x.abs() <= 5 && pos.z.abs() <= 5;
    let (mut cells, mut fires, mut floating) = (0u32, 0u32, 0u32);
    for row in table::<AnchorRow>("tnt_anchor") {
        let Some(explosion) = row.explosions.first() else {
            continue;
        };
        let reached: HashSet<IVec3> = explosion.blocks().collect();
        let solid: HashSet<IVec3> = row
            .solid_before
            .iter()
            .map(|(pos, _)| pos.0)
            .filter(|pos| !reached.contains(pos))
            .collect();
        let fire: HashSet<IVec3> = row.fire_after.iter().map(|o| o.0).collect();
        for pos in &fire {
            let below = *pos - IVec3::Y;
            if watched(below) && !solid.contains(&below) {
                floating += 1;
            }
        }
        for pos in reached
            .iter()
            .filter(|pos| watched(**pos) && !solid.contains(pos))
        {
            let below = *pos - IVec3::Y;
            if watched(below) && solid.contains(&below) {
                cells += 1;
                fires += u32::from(fire.contains(pos));
            }
        }
    }
    assert_eq!(floating, 0, "fire over air");
    assert!(cells > 500, "{cells} cells");
    let chance = 1.0 / f64::from(super::super::FIRE_CHANCE);
    let rate = f64::from(fires) / f64::from(cells);
    let se = (chance * (1.0 - chance) / f64::from(cells)).sqrt();
    assert!(
        (rate - chance).abs() < 3.0 * se,
        "BDS {rate:.3} over {cells} cells, unastar {chance:.3}"
    );
}

/// Flint and steel and a fire charge light a TNT where it stands: the
/// block becomes air and a primed TNT carrying the player takes its place;
/// the flint and steel wears, the fire charge is used up (BDS
/// `tnt_lit.json`: lit at the click, the explosion naming the player).
#[test]
fn flint_and_steel_and_fire_charges_light_tnt() {
    use crate::entity::components::{GameMode, HeldSlot, MainInventory, Player, Position};
    use crate::server::game::blocks::{block_at, set_block};
    for tool in ["minecraft:flint_and_steel", "minecraft:fire_charge"] {
        let mut server = crate::GameServer::with_config(crate::ServerConfig {
            world: crate::world::WorldConfig::void_spawn_platform(1),
            ..crate::ServerConfig::default()
        });
        let world = server.ecs.world_mut();
        let chunk = world
            .spawn(crate::world::ecs::ChunkData::new(
                crate::world::chunk::Chunk::new(0, 0),
            ))
            .id();
        world
            .resource_mut::<crate::world::ecs::ChunkManager>()
            .insert(crate::world::ecs::ChunkPosition::new(0, 0), chunk);
        let tnt_at = IVec3::new(2, 64, 2);
        let tnt = super::id("tnt");
        let ground = super::id("stone");
        set_block(world, tnt_at - IVec3::Y, ground, false);
        set_block(world, tnt_at, tnt, false);
        let mut main = MainInventory::default();
        main.0.set_item(0, stack(tool, 1)).unwrap();
        let player = world
            .spawn((
                Player,
                GameMode::Survival,
                Position(DVec3::new(4.5, 64.0 + PLAYER_EYE_HEIGHT, 2.5)),
                main,
                HeldSlot(0),
            ))
            .id();
        assert!(
            crate::server::game::item_use::on_block::use_on_block(world, player, tnt_at, 1),
            "{tool}"
        );
        assert_eq!(
            block_at(world, (2, 64, 2)),
            Some(super::id("air")),
            "{tool}"
        );
        let lit: Vec<_> = world
            .query::<&super::super::Igniter>()
            .iter(world)
            .map(|igniter| igniter.0)
            .collect();
        assert_eq!(lit, vec![player], "{tool}");
        let held = world
            .get::<MainInventory>(player)
            .and_then(|inventory| inventory.0.item(0).cloned())
            .unwrap();
        if tool == "minecraft:fire_charge" {
            assert!(held.is_empty(), "the fire charge is used up");
        } else {
            assert!(!held.is_empty());
        }
    }
}
