//! Using the held item on a block: hoes, shovels, axes, honeycomb, shears,
//! flint and steel, fire charges, bone meal and buckets.
//!
//! The behaviour pack has no data for what these items do to blocks, so the
//! rules are written here. Block identities come from the block registry
//! (stripped logs are the registered `stripped_*` blocks, copper ageing and
//! wax follow the registered `exposed_`/`weathered_`/`oxidized_`/`waxed_`
//! names, [`super::copper`]) so no conversion table is retyped.
//!
//! What hoes, shovels, axes, honeycomb and shears do (blocks, sounds, level
//! events, drops), their wear (1 per use, with Unbreaking's tool chance)
//! and what BDS sends the player match a BDS 1.26.51 capture
//! (`tools/bds-sweep/scripts/tool-use.txt`, `item_use/tool_use_tests.rs`):
//! a survival use that did something is answered like a placement, with
//! the click corrections and the player's inventory (which shows the wear
//! or count); a creative one with neither. Bone meal growth amounts follow
//! public references, recorded in `docs/quirks/bds.md`.

use bevy_ecs::prelude::*;
use glam::{DVec3, IVec3};
use rand::Rng;

use super::{give_or_drop, held, set_slot};
use crate::entity::EntityRandom;
use crate::entity::components::GameMode;
use crate::entity::mob::constants::{
    LEVEL_EVENT_COPPER_SCRAPE, LEVEL_EVENT_COPPER_WAX_OFF, LEVEL_EVENT_COPPER_WAX_ON,
};
use crate::item::ItemStack;
use crate::registry::BlockRegistry;
use crate::registry::block::{BlockEntry, LiquidKind, StateValue};
use crate::server::game::blocks::{block_at, set_block};
use crate::server::game::durability::wear_held_predicted;
use crate::server::game::types::BlockRegistryResource;

/// The bottom face of a block click (protocol order).
const DOWN: u8 = 0;

/// Use the held item on the clicked block. Returns whether the item did
/// something (then nothing is placed).
pub fn use_on_block(world: &mut World, player: Entity, clicked: IVec3, face: u8) -> bool {
    // A jukebox takes a disc or gives one back, also to an empty hand.
    if crate::server::game::jukebox::use_jukebox(world, player, clicked, face) {
        return true;
    }
    let Some((slot, stack)) = held(world, player) else {
        return false;
    };
    let Some(entry) = stack.item().cloned() else {
        return false;
    };
    let Some(registry) = world
        .get_resource::<BlockRegistryResource>()
        .map(|r| r.0.clone())
    else {
        return false;
    };
    let Some(state) = block_at(world, (clicked.x, clicked.y, clicked.z)) else {
        return false;
    };
    let Some(block) = registry.get_by_runtime_id(state).cloned() else {
        return false;
    };
    let ctx = Use {
        player,
        slot,
        stack: &stack,
        registry: &registry,
        clicked,
        face,
        state,
        block: &block,
    };
    let id = entry.identifier();
    if entry.has_tag("minecraft:is_hoe") {
        return till(world, &ctx);
    }
    if entry.has_tag("minecraft:is_shovel") {
        return flatten(world, &ctx);
    }
    if entry.has_tag("minecraft:is_axe") {
        return strip_or_scrape(world, &ctx);
    }
    match id {
        "minecraft:shears" => carve(world, &ctx),
        "minecraft:honeycomb" => wax(world, &ctx),
        "minecraft:flint_and_steel" | "minecraft:fire_charge" => ignite(world, &ctx),
        "minecraft:bone_meal" => fertilize(world, &ctx),
        "minecraft:bucket" => fill_bucket(world, &ctx),
        "minecraft:water_bucket" | "minecraft:lava_bucket" | "minecraft:powder_snow_bucket" => {
            empty_bucket(world, &ctx)
        }
        _ => false,
    }
}

struct Use<'a> {
    player: Entity,
    slot: u8,
    stack: &'a ItemStack,
    registry: &'a BlockRegistry,
    clicked: IVec3,
    face: u8,
    state: u32,
    block: &'a BlockEntry,
}

impl Use<'_> {
    fn block_name(&self) -> &str {
        &self.block.string_id
    }

    fn target(&self) -> IVec3 {
        self.clicked + face_offset(self.face)
    }

    fn is_air(&self, world: &World, pos: IVec3) -> bool {
        block_at(world, (pos.x, pos.y, pos.z))
            .and_then(|id| self.registry.get_by_runtime_id(id))
            .is_some_and(|b| b.string_id == "minecraft:air")
    }

    fn default_state(&self, name: &str) -> Option<u32> {
        self.registry.default_state_id_by_name(name)
    }
}

fn face_offset(face: u8) -> IVec3 {
    match face {
        0 => IVec3::NEG_Y,
        1 => IVec3::Y,
        2 => IVec3::NEG_Z,
        3 => IVec3::Z,
        4 => IVec3::NEG_X,
        _ => IVec3::X,
    }
}

fn centre(pos: IVec3) -> DVec3 {
    pos.as_dvec3() + DVec3::splat(0.5)
}

/// Replace the clicked block and play the item-use-on sound with the new
/// block, as tilling, flattening and stripping do.
fn convert(world: &mut World, ctx: &Use, new_state: u32) {
    set_block(world, ctx.clicked, new_state, false);
    let wire = crate::server::game::types::block_network_ids(world).to_wire(new_state);
    sound(
        world,
        ctx.player,
        centre(ctx.clicked),
        "item.use.on",
        wire as i32,
    );
}

/// Where a bucket's sound plays: at the player's feet, not at the block
/// (BDS 1.26.51 capture of `tools/bds-sweep/scripts/buckets-eggs.txt`).
fn feet(world: &World, player: Entity) -> DVec3 {
    world
        .get::<crate::entity::components::Position>(player)
        .map_or(DVec3::ZERO, |eye| {
            eye.0 - DVec3::Y * crate::entity::PLAYER_EYE_HEIGHT
        })
}

fn sound(world: &World, player: Entity, at: DVec3, name: &str, data: i32) {
    crate::entity::mob::net::sound::play_level_sound(world, Some(player), at, name, data);
}

/// How BDS answers a survival use that changed the clicked block, as it
/// answers a placement: the click corrections and the player's inventory,
/// which carries the item's new wear or count (the client predicts both, so
/// no slot update is sent). Creative uses get neither.
fn answer(world: &World, ctx: &Use) {
    if world.get::<GameMode>(ctx.player) == Some(&GameMode::Creative) {
        return;
    }
    crate::server::game::blocks::click_corrections(world, ctx.player, ctx.clicked, ctx.face);
    crate::server::game::inventory::resync_inventory(world, ctx.player);
}

/// One point of wear on the held tool, then [`answer`].
fn wear_and_answer(world: &mut World, ctx: &Use) {
    wear_held_predicted(world, ctx.player, 1);
    answer(world, ctx);
}

/// Use up one of the held item outside creative.
fn use_one(world: &mut World, ctx: &Use) {
    if world.get::<GameMode>(ctx.player) != Some(&GameMode::Creative) {
        set_slot(world, ctx.player, ctx.slot, ctx.stack.grow(-1));
    }
}

/// The state of block `to` that keeps every property `from_state` (of
/// `from`) shares with it, such as a log's `pillar_axis` or a copper
/// stair's facing.
fn carry_states(from: &BlockEntry, from_state: u32, target: &BlockEntry) -> Option<u32> {
    let mut state = target.default_state_id;
    for property in from.data.properties {
        if let Some(value) = from.state_value(from_state, property.name)
            && let Some(next) = target.with_state_value(state, property.name, value)
        {
            state = next;
        }
    }
    Some(state)
}

/// Hoe: grass, dirt and dirt paths become farmland, coarse dirt and rooted
/// dirt become dirt (rooted dirt drops hanging roots), from any face but
/// the bottom and only with air above.
fn till(world: &mut World, ctx: &Use) -> bool {
    if ctx.face == DOWN || !ctx.is_air(world, ctx.clicked + IVec3::Y) {
        return false;
    }
    let (into, drop) = match ctx.block_name() {
        "minecraft:grass_block" | "minecraft:dirt" | "minecraft:grass_path" => {
            ("minecraft:farmland", None)
        }
        "minecraft:coarse_dirt" => ("minecraft:dirt", None),
        "minecraft:dirt_with_roots" => ("minecraft:dirt", Some("minecraft:hanging_roots")),
        _ => return false,
    };
    let Some(state) = ctx.default_state(into) else {
        return false;
    };
    convert(world, ctx, state);
    if let Some(drop) = drop.and_then(|name| crate::server::game::world_stack(world, name, 1)) {
        crate::server::game::block_drops::pop_resource(world, ctx.clicked + IVec3::Y, drop);
    }
    wear_and_answer(world, ctx);
    true
}

/// Shovel: grass, dirt, coarse dirt, podzol, mycelium and rooted dirt
/// become a dirt path (not from below, only with air above); a lit
/// campfire is put out.
fn flatten(world: &mut World, ctx: &Use) -> bool {
    if matches!(
        ctx.block_name(),
        "minecraft:campfire" | "minecraft:soul_campfire"
    ) {
        let Some(out) = ctx
            .block
            .with_state_value(ctx.state, "extinguished", StateValue::Byte(1))
            .filter(|out| *out != ctx.state)
        else {
            return false;
        };
        // BDS sends the lit campfire once more before putting it out, and
        // plays the sound at the block's corner (tool-use capture).
        crate::server::game::blocks::correct_block(
            world,
            ctx.player,
            (ctx.clicked.x, ctx.clicked.y, ctx.clicked.z),
        );
        set_block(world, ctx.clicked, out, false);
        sound(
            world,
            ctx.player,
            ctx.clicked.as_dvec3(),
            "extinguish.fire",
            -1,
        );
        wear_and_answer(world, ctx);
        return true;
    }
    if ctx
        .registry
        .get_by_name("minecraft:snow_layer")
        .is_some_and(|snow| snow.id == ctx.block.id)
    {
        return dig_snow(world, ctx);
    }
    if ctx.face == DOWN
        || !ctx.is_air(world, ctx.clicked + IVec3::Y)
        || !matches!(
            ctx.block_name(),
            "minecraft:grass_block"
                | "minecraft:dirt"
                | "minecraft:coarse_dirt"
                | "minecraft:podzol"
                | "minecraft:mycelium"
                | "minecraft:dirt_with_roots"
        )
    {
        return false;
    }
    let Some(state) = ctx.default_state("minecraft:grass_path") else {
        return false;
    };
    convert(world, ctx, state);
    wear_and_answer(world, ctx);
    true
}

/// Shovel on a snow layer: the layer goes, dropping what it drops when
/// broken with the shovel, with the item-use-on sound of the layer (BDS
/// tool-use capture: a one-layer cover dropped one snowball).
fn dig_snow(world: &mut World, ctx: &Use) -> bool {
    let Some(air) = ctx.default_state("minecraft:air") else {
        return false;
    };
    let drops = crate::server::game::block_drops::block_drops(world, ctx.state, Some(ctx.stack));
    set_block(world, ctx.clicked, air, false);
    for drop in drops {
        crate::server::game::block_drops::pop_resource(world, ctx.clicked, drop);
    }
    let wire = crate::server::game::types::block_network_ids(world).to_wire(ctx.state);
    sound(
        world,
        ctx.player,
        centre(ctx.clicked),
        "item.use.on",
        wire as i32,
    );
    wear_and_answer(world, ctx);
    true
}

/// Replace the clicked copper block with the block `into` (keeping the
/// states they share) and play `event`.
fn change_copper(world: &mut World, ctx: &Use, into: u32, event: i32) -> bool {
    let Some(state) = ctx
        .registry
        .get(into)
        .and_then(|target| carry_states(ctx.block, ctx.state, target))
    else {
        return false;
    };
    set_block(world, ctx.clicked, state, false);
    crate::entity::mob::net::sound::level_event(world, centre(ctx.clicked), event, 0);
    true
}

/// Honeycomb: wax a copper block that is not waxed yet, using one.
fn wax(world: &mut World, ctx: &Use) -> bool {
    let Some(waxed) = super::copper::wax(ctx.block.id) else {
        return false;
    };
    if !change_copper(world, ctx, waxed, LEVEL_EVENT_COPPER_WAX_ON) {
        return false;
    }
    use_one(world, ctx);
    answer(world, ctx);
    true
}

/// Axe: strip a log or wood, remove wax, or scrape one oxidation stage.
fn strip_or_scrape(world: &mut World, ctx: &Use) -> bool {
    let name = ctx.block_name().to_owned();
    let stripped = name.strip_prefix("minecraft:").and_then(|base| {
        ctx.registry
            .get_by_name(&format!("minecraft:stripped_{base}"))
    });
    if let Some(stripped) = stripped {
        let Some(state) = carry_states(ctx.block, ctx.state, stripped) else {
            return false;
        };
        convert(world, ctx, state);
        wear_and_answer(world, ctx);
        return true;
    }
    let Some(scrape) = super::copper::scrape(ctx.block.id) else {
        return false;
    };
    let event = match scrape {
        super::copper::Scrape::WaxOff(_) => LEVEL_EVENT_COPPER_WAX_OFF,
        super::copper::Scrape::Deoxidize(_) => LEVEL_EVENT_COPPER_SCRAPE,
    };
    if !change_copper(world, ctx, scrape.into_block(), event) {
        return false;
    }
    wear_and_answer(world, ctx);
    true
}

/// Shears on a pumpkin carve it facing the clicked side, or as the carved
/// pumpkin's default state (facing south) from the top or bottom, and drop
/// one pumpkin seed there (BDS tool-use capture, the player looking east).
fn carve(world: &mut World, ctx: &Use) -> bool {
    if ctx.block_name() != "minecraft:pumpkin" {
        return false;
    }
    let Some(pumpkin) = ctx.registry.get_by_name("minecraft:carved_pumpkin") else {
        return false;
    };
    let direction = match ctx.face {
        2 => Some("north"),
        3 => Some("south"),
        4 => Some("west"),
        5 => Some("east"),
        _ => None,
    };
    let Some(carved) = direction.map_or(Some(pumpkin.default_state_id), |direction| {
        pumpkin.with_state_value(
            pumpkin.default_state_id,
            "minecraft:cardinal_direction",
            StateValue::Str(direction),
        )
    }) else {
        return false;
    };
    set_block(world, ctx.clicked, carved, false);
    if let Some(seeds) = crate::server::game::world_stack(world, "minecraft:pumpkin_seeds", 1) {
        crate::server::game::block_drops::pop_resource(world, ctx.target(), seeds);
    }
    sound(world, ctx.player, centre(ctx.clicked), "pumpkin.carve", -1);
    wear_and_answer(world, ctx);
    true
}

/// Flint and steel or a fire charge: light a campfire or candle, else set
/// fire to the air next to the clicked face (soul fire on soul sand and
/// soul soil). Flint and steel wears, a fire charge is used up.
fn ignite(world: &mut World, ctx: &Use) -> bool {
    let flint = ctx.stack.item_id() == "minecraft:flint_and_steel";
    let lit = match ctx.block_name() {
        "minecraft:campfire" | "minecraft:soul_campfire" => {
            ctx.block
                .with_state_value(ctx.state, "extinguished", StateValue::Byte(0))
        }
        name if name.ends_with("candle") || name.ends_with("candle_cake") => ctx
            .block
            .with_state_value(ctx.state, "lit", StateValue::Byte(1)),
        _ => None,
    }
    .filter(|lit| *lit != ctx.state);
    let (pos, state) = match lit {
        Some(lit) => (ctx.clicked, lit),
        None => {
            let target = ctx.target();
            if !ctx.is_air(world, target) {
                return false;
            }
            let below = block_at(world, (target.x, target.y - 1, target.z))
                .and_then(|id| ctx.registry.get_by_runtime_id(id))
                .map(|b| b.string_id.as_str());
            let fire = if matches!(below, Some("minecraft:soul_sand" | "minecraft:soul_soil")) {
                "minecraft:soul_fire"
            } else {
                "minecraft:fire"
            };
            let Some(state) = ctx.default_state(fire) else {
                return false;
            };
            (target, state)
        }
    };
    crate::server::game::blocks::set_block_batched(world, pos, state);
    if matches!(
        ctx.block_name(),
        "minecraft:campfire" | "minecraft:soul_campfire"
    ) && pos == ctx.clicked
    {
        announce_campfire(world, pos);
    }
    let sound_name = if flint {
        "ignite"
    } else {
        "mob.ghast.fireball"
    };
    // BDS plays it at the corner of a block lit where it stands and at the
    // centre of new fire (`tools/bds-sweep/scripts/containers-flint.txt`).
    let at = if pos == ctx.clicked {
        pos.as_dvec3()
    } else {
        centre(pos)
    };
    sound(world, ctx.player, at, sound_name, -1);
    if flint {
        wear_and_answer(world, ctx);
    } else {
        use_one(world, ctx);
        answer(world, ctx);
    }
    true
}

/// A relit campfire's actor data, as BDS sends it to the block's viewers
/// (`{BlockEntityVersion, ItemTime1-4, id, x, y, z}`, all 0 but the id and
/// position; `tools/bds-sweep/scripts/containers-flint.txt`).
fn announce_campfire(world: &World, pos: IVec3) {
    use std::collections::HashMap;
    use zuri_nbt::{NBTTag, tag};
    let mut fields: HashMap<String, NBTTag> = HashMap::new();
    fields.insert("id".into(), NBTTag::String(tag::String("Campfire".into())));
    for (name, value) in [
        ("BlockEntityVersion", 0),
        ("ItemTime1", 0),
        ("ItemTime2", 0),
        ("ItemTime3", 0),
        ("ItemTime4", 0),
        ("x", pos.x),
        ("y", pos.y),
        ("z", pos.z),
    ] {
        fields.insert(name.into(), NBTTag::Int(tag::Int(value)));
    }
    let Ok(actor) =
        crate::world::block_entity::BlockEntity::from_bedrock_compound(tag::Compound(fields))
    else {
        return;
    };
    let packet = jolyne::valentine::McpePacket::from(jolyne::valentine::BlockActorDataPacket {
        block_position: jolyne::valentine::types::BlockPos {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        },
        actor_data_tags: actor.network_nbt().clone(),
    });
    let (cx, cz) = crate::world::ecs::world_to_chunk_coords(pos.x, pos.z);
    let Some(viewers) = world
        .get_resource::<crate::world::ecs::ChunkManager>()
        .and_then(|chunks| chunks.get_by_coords(cx, cz))
        .and_then(|chunk| world.get::<crate::world::ecs::ChunkViewers>(chunk))
    else {
        return;
    };
    for viewer in viewers.iter() {
        if let Some(session) = world.get::<crate::entity::components::PlayerSession>(viewer) {
            let _ = session.send(packet.clone());
        }
    }
}

/// Growth stages a crop with `growth` 0..=7 advances by from bone meal.
fn bone_meal_growth(rng: &mut impl Rng) -> i32 {
    rng.gen_range(2..=5)
}

/// Bone meal: grow a crop or stem by 2-5 stages, a sweet berry bush by one,
/// or plant grass and flowers around a grass block.
fn fertilize(world: &mut World, ctx: &Use) -> bool {
    let grown = if ctx.block.has_tag("minecraft:crop")
        || matches!(
            ctx.block_name(),
            "minecraft:melon_stem" | "minecraft:pumpkin_stem"
        ) {
        let Some(StateValue::Int(growth)) = ctx.block.state_value(ctx.state, "growth") else {
            return false;
        };
        if growth >= 7 {
            return false;
        }
        let step = bone_meal_growth(&mut world.get_resource_or_init::<EntityRandom>().0);
        let next = (growth + step).min(7);
        let Some(state) = ctx
            .block
            .with_state_value(ctx.state, "growth", StateValue::Int(next))
        else {
            return false;
        };
        set_block(world, ctx.clicked, state, false);
        true
    } else if ctx.block_name() == "minecraft:sweet_berry_bush" {
        let Some(StateValue::Int(age)) = ctx.block.state_value(ctx.state, "growth") else {
            return false;
        };
        let Some(state) = (age < 3)
            .then(|| {
                ctx.block
                    .with_state_value(ctx.state, "growth", StateValue::Int(age + 1))
            })
            .flatten()
        else {
            return false;
        };
        set_block(world, ctx.clicked, state, false);
        true
    } else if ctx.block_name() == "minecraft:grass_block"
        && ctx.is_air(world, ctx.clicked + IVec3::Y)
    {
        grow_grass(world, ctx);
        true
    } else {
        false
    };
    if grown {
        use_one(world, ctx);
    }
    grown
}

/// Grass block bone meal: 128 random walks from the block above, each up
/// to `i / 16` steps over grass, planting short grass (one in eight a
/// dandelion or poppy) on air above grass.
fn grow_grass(world: &mut World, ctx: &Use) {
    let (Some(grass), Some(short), Some(dandelion), Some(poppy)) = (
        ctx.default_state("minecraft:grass_block"),
        ctx.default_state("minecraft:short_grass"),
        ctx.default_state("minecraft:dandelion"),
        ctx.default_state("minecraft:poppy"),
    ) else {
        return;
    };
    let start = ctx.clicked + IVec3::Y;
    'walks: for i in 0..128 {
        let mut pos = start;
        for _ in 0..i / 16 {
            let step = {
                let rng = &mut world.get_resource_or_init::<EntityRandom>().0;
                IVec3::new(
                    rng.gen_range(-1..=1),
                    rng.gen_range(-1..=1) * rng.gen_range(0..=2) / 2,
                    rng.gen_range(-1..=1),
                )
            };
            pos += step;
            if block_at(world, (pos.x, pos.y - 1, pos.z)) != Some(grass) {
                continue 'walks;
            }
        }
        if !ctx.is_air(world, pos) || block_at(world, (pos.x, pos.y - 1, pos.z)) != Some(grass) {
            continue;
        }
        let plant = {
            let rng = &mut world.get_resource_or_init::<EntityRandom>().0;
            match rng.gen_range(0..8) {
                0 if rng.r#gen::<bool>() => dandelion,
                0 => poppy,
                _ => short,
            }
        };
        set_block(world, pos, plant, false);
    }
}

/// An empty bucket on a still water or lava source, or on powder snow,
/// takes it.
fn fill_bucket(world: &mut World, ctx: &Use) -> bool {
    // Water in a block's liquid layer (a waterlogged block) counts too.
    let liquid = ctx.registry.liquid_state(ctx.state).or_else(|| {
        crate::server::game::blocks::layer_at(world, ctx.clicked)
            .and_then(|layer| ctx.registry.liquid_state(layer))
    });
    let filled = match liquid {
        Some(liquid) if liquid.depth == 0 => match liquid.kind {
            LiquidKind::Water => ("minecraft:water_bucket", "bucket.fill.water"),
            LiquidKind::Lava => ("minecraft:lava_bucket", "bucket.fill.lava"),
        },
        Some(_) => return false,
        None if ctx.block_name() == "minecraft:powder_snow" => {
            ("minecraft:powder_snow_bucket", "bucket.fill.powder_snow")
        }
        None => return false,
    };
    let Some(air) = ctx.default_state("minecraft:air") else {
        return false;
    };
    let Some(bucket) = crate::server::game::world_stack(world, filled.0, 1) else {
        return false;
    };
    if liquid.is_some() {
        if crate::server::game::liquids::take_source(world, ctx.clicked).is_none() {
            return false;
        }
    } else {
        set_block(world, ctx.clicked, air, false);
    }
    sound(world, ctx.player, feet(world, ctx.player), filled.1, -1);
    if world.get::<GameMode>(ctx.player) != Some(&GameMode::Creative) {
        let rest = ctx.stack.grow(-1);
        if rest.is_empty() {
            set_slot(world, ctx.player, ctx.slot, bucket);
        } else {
            set_slot(world, ctx.player, ctx.slot, rest);
            give_or_drop(world, ctx.player, bucket);
        }
    }
    true
}

/// A filled bucket places its source block in the clicked block when that
/// is a liquid or replaceable (water: into a block that holds water), else
/// next to the clicked face, and becomes an empty bucket. Liquids go
/// through [`crate::server::game::liquids::place_source`], so they flow.
fn empty_bucket(world: &mut World, ctx: &Use) -> bool {
    let (kind, sound_name) = match ctx.stack.item_id() {
        "minecraft:water_bucket" => (Some(LiquidKind::Water), "bucket.empty.water"),
        "minecraft:lava_bucket" => (Some(LiquidKind::Lava), "bucket.empty.lava"),
        _ => (None, "bucket.empty.powder_snow"),
    };
    if let Some(kind) = kind {
        let Some(empty) = crate::server::game::world_stack(world, "minecraft:bucket", 1) else {
            return false;
        };
        let place = crate::server::game::liquids::place_source;
        // The clicked block takes the liquid (waterlogging) before the face.
        if !place(world, ctx.clicked, kind) && !place(world, ctx.target(), kind) {
            return false;
        }
        sound(world, ctx.player, feet(world, ctx.player), sound_name, -1);
        if world.get::<GameMode>(ctx.player) != Some(&GameMode::Creative) {
            set_slot(world, ctx.player, ctx.slot, empty);
        }
        return true;
    }
    let replaceable = |pos: IVec3| {
        block_at(world, (pos.x, pos.y, pos.z))
            .and_then(|id| Some((id, ctx.registry.get_by_runtime_id(id)?)))
            .is_some_and(|(id, b)| {
                b.string_id == "minecraft:air" || ctx.registry.liquid_state(id).is_some()
            })
    };
    let pos = if replaceable(ctx.clicked) {
        ctx.clicked
    } else if replaceable(ctx.target()) {
        ctx.target()
    } else {
        return false;
    };
    let Some(state) = ctx.default_state("minecraft:powder_snow") else {
        return false;
    };
    let Some(empty) = crate::server::game::world_stack(world, "minecraft:bucket", 1) else {
        return false;
    };
    set_block(world, pos, state, false);
    sound(world, ctx.player, feet(world, ctx.player), sound_name, -1);
    if world.get::<GameMode>(ctx.player) != Some(&GameMode::Creative) {
        set_slot(world, ctx.player, ctx.slot, empty);
    }
    true
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::entity::components::{HeldSlot, MainInventory, Position};
    use crate::item::enchant::durability_damage;
    use crate::item::test_support::stack;
    use crate::world::ChunkManager;
    use crate::world::ecs::{ChunkData, ChunkPosition};

    /// The top face of a block click.
    const UP: u8 = 1;

    /// A chunk with `block` at (1, 64, 1) and air above, and a survival
    /// player holding `item`.
    fn world_with(block: &str, item: &str, count: u8) -> (World, Entity, Entity) {
        let mut world = World::new();
        world.insert_resource(ChunkManager::new(
            crate::world::WorldConfig::void_spawn_platform(1),
        ));
        world.insert_resource(crate::ecs::events::EventBuffer::default());
        world.insert_resource(crate::server::game::types::ServerConfigResource(
            Default::default(),
        ));
        world.insert_resource(BlockRegistryResource(Arc::new(
            BlockRegistry::vanilla().clone(),
        )));
        let mut items = crate::registry::ItemRegistry::new();
        items.load_vanilla();
        world.insert_resource(crate::server::game::types::ItemRegistryResource(Arc::new(
            items,
        )));
        world.insert_resource(EntityRandom::seeded(3));
        world
            .init_resource::<bevy_ecs::message::Messages<crate::world::ecs::BlockBroadcastEvent>>();
        let mut chunk = crate::world::chunk::Chunk::new(0, 0);
        chunk.set_block(1, 64, 1, state(block));
        let chunk = world.spawn(ChunkData::new(chunk)).id();
        world
            .resource_mut::<ChunkManager>()
            .insert(ChunkPosition::new(0, 0), chunk);
        let mut main = MainInventory::default();
        main.0.set_item(0, stack(item, count)).unwrap();
        let player = world
            .spawn((
                GameMode::Survival,
                Position(DVec3::new(1.5, 66.62, 3.5)),
                main,
                HeldSlot(0),
            ))
            .id();
        (world, player, chunk)
    }

    fn state(name: &str) -> u32 {
        BlockRegistry::vanilla()
            .default_state_id_by_name(name)
            .expect(name)
    }

    fn block(world: &World, chunk: Entity, y: i16) -> String {
        let id = world
            .get::<ChunkData>(chunk)
            .unwrap()
            .inner
            .get_block(1, y, 1);
        BlockRegistry::vanilla()
            .get_by_runtime_id(id)
            .unwrap()
            .string_id
            .clone()
    }

    fn held_stack(world: &World, player: Entity) -> ItemStack {
        world
            .get::<MainInventory>(player)
            .unwrap()
            .0
            .item(0)
            .unwrap()
            .clone()
    }

    #[test]
    fn hoes_till_grass_from_above_and_wear() {
        let (mut world, player, chunk) =
            world_with("minecraft:grass_block", "minecraft:iron_hoe", 1);
        assert!(!use_on_block(
            &mut world,
            player,
            IVec3::new(1, 64, 1),
            DOWN
        ));
        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        assert_eq!(block(&world, chunk, 64), "minecraft:farmland");
        assert_eq!(durability_damage(&held_stack(&world, player)), 1);
    }

    #[test]
    fn hoes_need_air_above() {
        let (mut world, player, chunk) = world_with("minecraft:dirt", "minecraft:iron_hoe", 1);
        world.get_mut::<ChunkData>(chunk).unwrap().inner.set_block(
            1,
            65,
            1,
            state("minecraft:stone"),
        );
        assert!(!use_on_block(&mut world, player, IVec3::new(1, 64, 1), 2));
        assert_eq!(block(&world, chunk, 64), "minecraft:dirt");
    }

    #[test]
    fn shovels_make_paths() {
        let (mut world, player, chunk) =
            world_with("minecraft:podzol", "minecraft:wooden_shovel", 1);
        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), 3));
        assert_eq!(block(&world, chunk, 64), "minecraft:grass_path");
    }

    #[test]
    fn axes_strip_wax_and_scrape() {
        for (from, to) in [
            ("minecraft:oak_log", "minecraft:stripped_oak_log"),
            ("minecraft:waxed_exposed_copper", "minecraft:exposed_copper"),
            ("minecraft:weathered_copper", "minecraft:exposed_copper"),
        ] {
            let (mut world, player, chunk) = world_with(from, "minecraft:stone_axe", 1);
            assert!(
                use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP),
                "{from}"
            );
            assert_eq!(block(&world, chunk, 64), to);
        }
        let (mut world, player, _) = world_with("minecraft:copper_block", "minecraft:stone_axe", 1);
        assert!(!use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        assert_eq!(durability_damage(&held_stack(&world, player)), 0);
    }

    #[test]
    fn flint_and_steel_sets_fire_next_to_the_face() {
        let (mut world, player, chunk) =
            world_with("minecraft:stone", "minecraft:flint_and_steel", 1);
        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        assert_eq!(block(&world, chunk, 65), "minecraft:fire");
        let (mut world, player, chunk) =
            world_with("minecraft:soul_sand", "minecraft:fire_charge", 2);
        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        assert_eq!(block(&world, chunk, 65), "minecraft:soul_fire");
        assert_eq!(held_stack(&world, player).count, 1);
    }

    #[test]
    fn bone_meal_grows_crops_two_to_five_stages() {
        let (mut world, player, chunk) = world_with("minecraft:wheat", "minecraft:bone_meal", 3);
        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        let id = world
            .get::<ChunkData>(chunk)
            .unwrap()
            .inner
            .get_block(1, 64, 1);
        let wheat = BlockRegistry::vanilla().get_by_runtime_id(id).unwrap();
        let Some(StateValue::Int(growth)) = wheat.state_value(id, "growth") else {
            panic!("wheat growth");
        };
        assert!((2..=5).contains(&growth), "{growth}");
        assert_eq!(held_stack(&world, player).count, 2);
    }

    #[test]
    fn buckets_take_and_place_sources() {
        let (mut world, player, chunk) = world_with("minecraft:water", "minecraft:bucket", 2);
        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        assert_eq!(block(&world, chunk, 64), "minecraft:air");
        assert_eq!(held_stack(&world, player).count, 1);
        let filled = world
            .get::<MainInventory>(player)
            .unwrap()
            .0
            .item(1)
            .unwrap()
            .clone();
        assert_eq!(filled.item_id(), "minecraft:water_bucket");

        let (mut world, player, chunk) = world_with("minecraft:stone", "minecraft:lava_bucket", 1);
        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        assert_eq!(block(&world, chunk, 65), "minecraft:lava");
        assert_eq!(held_stack(&world, player).item_id(), "minecraft:bucket");
    }

    /// A water bucket on a fence fills the fence's liquid layer (BDS 1.26.51,
    /// `liquids/liquids_logged.json` `bucket_on`); an empty bucket takes it back.
    #[test]
    fn buckets_fill_and_empty_a_block_that_holds_water() {
        let layer = |world: &World| {
            let state = crate::server::game::blocks::layer_at(world, IVec3::new(1, 64, 1)).unwrap();
            BlockRegistry::vanilla()
                .get_by_runtime_id(state)
                .unwrap()
                .string_id
                .clone()
        };
        let (mut world, player, chunk) =
            world_with("minecraft:oak_fence", "minecraft:water_bucket", 1);
        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        assert_eq!(block(&world, chunk, 64), "minecraft:oak_fence");
        assert_eq!(block(&world, chunk, 65), "minecraft:air");
        assert_eq!(layer(&world), "minecraft:water");
        assert_eq!(held_stack(&world, player).item_id(), "minecraft:bucket");

        assert!(use_on_block(&mut world, player, IVec3::new(1, 64, 1), UP));
        assert_eq!(block(&world, chunk, 64), "minecraft:oak_fence");
        assert_eq!(layer(&world), "minecraft:air");
        assert_eq!(
            held_stack(&world, player).item_id(),
            "minecraft:water_bucket"
        );
    }

    #[test]
    fn stripping_keeps_the_log_axis() {
        let registry = BlockRegistry::vanilla();
        let log = registry.get_by_name("minecraft:oak_log").unwrap();
        let sideways = log
            .with_state_value(log.default_state_id, "pillar_axis", StateValue::Str("x"))
            .unwrap();
        let target = registry.get_by_name("minecraft:stripped_oak_log").unwrap();
        let stripped = carry_states(log, sideways, target).unwrap();
        let entry = registry.get_by_runtime_id(stripped).unwrap();
        assert_eq!(entry.string_id, "minecraft:stripped_oak_log");
        assert_eq!(
            entry.state_value(stripped, "pillar_axis"),
            Some(StateValue::Str("x"))
        );
    }
}
