//! Item durability: wear from mining, attacking, item use and armour hits,
//! the Unbreaking enchantment, and breaking.
//!
//! An item's durability is its pack `minecraft:durability.max_durability`
//! (spears) or else its native `maxDamage`; items without either never
//! wear. Damage is stored in the item's `Damage` tag like BDS.
//!
//! Melee and mining wear are measured on BDS 1.26.51 (attack and mining
//! sweeps, `unastar-data/data/measured/`): a hit with a sword, trident or
//! mace costs 1, with an axe, pickaxe, shovel or hoe 2, and with an item
//! whose durability is its `minecraft:durability` component (spears) 1;
//! breaking a block is in `mining::mining_wear`. A hit that hurts the player
//! wears every worn piece with armour points by `max(1, floor(damage / 4))`
//! of the damage before armour (BDS armour-wear sweep; the elytra never
//! wears from hits). An item breaks when its damage passes its durability:
//! damage equal to it still works (BDS threshold sweep).
//!
//! Unbreaking ([`WearChance`]; BDS Unbreaking sweeps,
//! `durability/testdata/bds-unbreaking.json`) rolls once per wear event and
//! spares all of the event's points or none: a pickaxe's 2-point hit and a
//! sword's 2-point broken block cost 0 or 2, never 1, and so does a 2-point
//! armour hit.

use bevy_ecs::prelude::*;
use rand::Rng;
use unastar_data::actor_components::body::ActorDamageCause;
use unastar_data::item_components::IntRange;

use unastar_rules::ids::Enchantment;

use crate::entity::components::{ArmourInventory, GameMode, HeldSlot, MainInventory, Position};
use crate::item::ItemStack;
use crate::item::enchant::{durability_damage, is_unbreakable, with_durability_damage};
use crate::registry::item::ItemEntry;

/// The durability of `entry`, 0 when it does not wear.
pub fn max_durability(entry: &ItemEntry) -> i32 {
    entry
        .components()
        .durability
        .as_ref()
        .map_or(entry.data().max_damage, |d| d.max_durability)
        .max(0)
}

/// Durability a melee hit with `entry` costs.
pub fn attack_wear(entry: &ItemEntry) -> i32 {
    if max_durability(entry) == 0 {
        return 0;
    }
    if entry.has_tag("minecraft:is_sword")
        || entry.has_tag("minecraft:is_trident")
        || entry.identifier() == "minecraft:mace"
    {
        1
    } else if ["axe", "pickaxe", "shovel", "hoe"]
        .iter()
        .any(|kind| entry.has_tag(&format!("minecraft:is_{kind}")))
    {
        2
    } else if entry.components().durability.is_some() {
        // Every hit, whatever the component's `damage_chance` (BDS attack
        // sweep: each spear tier lost 1 on each of three hits).
        1
    } else {
        0
    }
}

/// Whether an armour piece loses durability for a hit of `cause`.
pub fn armour_wears_from(cause: ActorDamageCause) -> bool {
    matches!(
        cause,
        ActorDamageCause::EntityAttack
            | ActorDamageCause::Projectile
            | ActorDamageCause::EntityExplosion
            | ActorDamageCause::BlockExplosion
            | ActorDamageCause::Contact
            | ActorDamageCause::Thorns
            | ActorDamageCause::FallingBlock
            | ActorDamageCause::Anvil
    )
}

/// How Unbreaking lowers the chance that a wear event wears an item: a
/// uniform roll in `0..100` below [`WearChance::percent`] wears it, once
/// per event, by all of the event's points (BDS Unbreaking sweeps:
/// 2-point events cost 0 or 2, never 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WearChance {
    /// Tools, weapons and every other item that wears, the elytra included:
    /// `floor(100 / (level + 1))` percent.
    Tool,
    /// Armour pieces that give armour points: `60 + floor(40 / (level + 1))`
    /// percent.
    Armour,
    /// An item whose durability is its `minecraft:durability` component
    /// (spears): `min + floor((max - min) / (level + 1))` percent of its
    /// `damage_chance`, `{min: 100, max: 100}` when the component has none.
    Component { min: i32, max: i32 },
}

impl WearChance {
    /// The chance kind of `entry`.
    pub fn of(entry: &ItemEntry) -> Self {
        if let Some(durability) = &entry.components().durability {
            let range = durability
                .damage_chance
                .unwrap_or(IntRange { min: 100, max: 100 });
            Self::Component {
                min: range.min,
                max: range.max,
            }
        } else if gives_armour(entry.identifier()) {
            Self::Armour
        } else {
            Self::Tool
        }
    }

    /// Percent chance that a wear event wears an item with Unbreaking
    /// `level` (> 0).
    pub fn percent(self, level: u8) -> i32 {
        let divisor = i32::from(level) + 1;
        match self {
            Self::Tool => 100 / divisor,
            Self::Armour => 60 + 40 / divisor,
            Self::Component { min, max } => min + (max - min) / divisor,
        }
    }
}

/// Whether the wearable `identifier` gives armour points (the elytra, with
/// none, does not).
fn gives_armour(identifier: &str) -> bool {
    unastar_data::equipment::get(identifier).is_some_and(|values| values.defense > 0.0)
}

/// `points` of wear (one wear event) on `stack`: the worn stack, or empty
/// when its damage passed its durability. `None` when nothing changed: the
/// item does not wear, is `Unbreakable`, or Unbreaking spared the event.
/// Without Unbreaking no roll is made.
pub fn wear(stack: &ItemStack, points: i32, rng: &mut impl Rng) -> Option<ItemStack> {
    let entry = stack.item()?;
    let max = Some(max_durability(entry)).filter(|max| *max > 0)?;
    if points <= 0 || is_unbreakable(stack) {
        return None;
    }
    let level = stack.enchantment_level(Enchantment::Unbreaking);
    if level > 0 && rng.gen_range(0..100) >= WearChance::of(entry).percent(level) {
        return None;
    }
    // Damage equal to the durability still works; one more breaks it (BDS
    // threshold sweep).
    let damage = durability_damage(stack) + points;
    Some(if damage > max {
        ItemStack::empty()
    } else {
        with_durability_damage(stack, damage)
    })
}

/// Wear the item in the player's selected hotbar slot. Creative players'
/// items never wear. Returns whether the item broke.
pub fn wear_held(world: &mut World, player: Entity, points: i32) -> bool {
    wear_held_sending(world, player, points, true)
}

/// [`wear_held`] without the slot update while the item lasts: the client
/// predicts that wear (BDS sends none after flint and steel lights a block).
pub fn wear_held_predicted(world: &mut World, player: Entity, points: i32) -> bool {
    wear_held_sending(world, player, points, false)
}

fn wear_held_sending(world: &mut World, player: Entity, points: i32, send: bool) -> bool {
    if world.get::<GameMode>(player) == Some(&GameMode::Creative) {
        return false;
    }
    let Some(slot) = world.get::<HeldSlot>(player).map(|s| usize::from(s.0)) else {
        return false;
    };
    let Some(stack) = world
        .get::<MainInventory>(player)
        .and_then(|inv| inv.0.item(slot).cloned())
        .filter(|s| !s.is_empty())
    else {
        return false;
    };
    let Some(worn) = wear(
        &stack,
        points,
        &mut world
            .get_resource_or_init::<crate::entity::EntityRandom>()
            .0,
    ) else {
        return false;
    };
    let broke = worn.is_empty();
    if let Some(mut inventory) = world.get_mut::<MainInventory>(player) {
        let _ = inventory.0.set_item(slot, worn.clone());
    }
    if send || broke {
        super::send_slot(world, player, slot, &worn);
    }
    if broke {
        item_broke(world, player);
    }
    broke
}

/// A hit of `damage` wears every worn armour piece.
pub fn wear_armour(world: &mut World, player: Entity, damage: f32, cause: ActorDamageCause) {
    if !armour_wears_from(cause)
        || world.get::<GameMode>(player) == Some(&GameMode::Creative)
        || damage <= 0.0
    {
        return;
    }
    let points = ((damage / 4.0).floor() as i32).max(1);
    let Some(pieces) = world
        .get::<ArmourInventory>(player)
        .map(|a| a.slots().to_vec())
    else {
        return;
    };
    let mut broke = false;
    for (slot, piece) in pieces.iter().enumerate() {
        // Only a piece that gives armour points wears (BDS armour-wear
        // sweep: the elytra, measured with no defence, never does).
        if piece.is_empty() || !gives_armour(piece.item_id()) {
            continue;
        }
        let Some(worn) = wear(
            piece,
            points,
            &mut world
                .get_resource_or_init::<crate::entity::EntityRandom>()
                .0,
        ) else {
            continue;
        };
        broke |= worn.is_empty();
        if let Some(mut armour) = world.get_mut::<ArmourInventory>(player) {
            let _ = armour.set_slot(slot, worn);
        }
        super::inventory::send_armour_slot(world, player, slot);
    }
    if broke {
        item_broke(world, player);
    }
}

/// The item break sound at the player.
fn item_broke(world: &mut World, player: Entity) {
    if let Some(at) = world.get::<Position>(player).map(|p| p.0) {
        crate::entity::mob::net::sound::play_level_sound(world, Some(player), at, "break", -1);
    }
}

/// Whether `ours` of `n` and `theirs` of `m` can be the same share: their
/// difference within four standard errors of the pooled share (sampled wear
/// against a BDS sweep).
#[cfg(test)]
pub(crate) fn same_share(ours: u32, n: u32, theirs: u32, m: u32) -> bool {
    let (ours, n, theirs, m) = (
        f64::from(ours),
        f64::from(n),
        f64::from(theirs),
        f64::from(m),
    );
    let pooled = (ours + theirs) / (n + m);
    let error = (pooled * (1.0 - pooled) * (1.0 / n + 1.0 / m)).sqrt();
    (ours / n - theirs / m).abs() <= 4.0 * error.max(1e-9)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use crate::item::enchant::with_enchantments;
    use crate::item::test_support::stack;

    fn entry(name: &str) -> ItemEntry {
        stack(name, 1).item().unwrap().clone()
    }

    #[test]
    fn durability_comes_from_the_pack_component_or_native_max_damage() {
        assert_eq!(max_durability(&entry("minecraft:diamond_pickaxe")), 1561);
        assert_eq!(max_durability(&entry("minecraft:bow")), 384);
        let spear = entry("minecraft:iron_spear");
        assert_eq!(
            max_durability(&spear),
            spear
                .components()
                .durability
                .as_ref()
                .unwrap()
                .max_durability
        );
        assert_eq!(max_durability(&entry("minecraft:stone")), 0);
    }

    #[test]
    fn swords_wear_one_and_tools_two_per_hit() {
        assert_eq!(attack_wear(&entry("minecraft:iron_sword")), 1);
        assert_eq!(attack_wear(&entry("minecraft:iron_axe")), 2);
        assert_eq!(attack_wear(&entry("minecraft:wooden_hoe")), 2);
        assert_eq!(attack_wear(&entry("minecraft:trident")), 1);
        assert_eq!(attack_wear(&entry("minecraft:mace")), 1);
        // A spear's durability is its pack component: one per hit.
        assert_eq!(attack_wear(&entry("minecraft:wooden_spear")), 1);
        assert_eq!(attack_wear(&entry("minecraft:stick")), 0);
        assert_eq!(attack_wear(&entry("minecraft:bow")), 0);
    }

    #[test]
    fn worn_out_items_break() {
        let mut rng = StdRng::seed_from_u64(1);
        let hoe = stack("minecraft:wooden_hoe", 1);
        let max = max_durability(hoe.item().unwrap());
        let almost = with_durability_damage(&hoe, max - 1);
        // Damage equal to the durability still works.
        let at_max = wear(&almost, 1, &mut rng).unwrap();
        assert_eq!(durability_damage(&at_max), max);
        assert!(wear(&at_max, 1, &mut rng).unwrap().is_empty());
        assert!(wear(&almost, 2, &mut rng).unwrap().is_empty());
        assert!(wear(&stack("minecraft:stone", 1), 1, &mut rng).is_none());
    }

    #[test]
    fn unbreaking_chances_are_whole_percents_per_item_kind() {
        let chance = |name: &str| WearChance::of(&entry(name));
        assert_eq!(chance("minecraft:diamond_pickaxe"), WearChance::Tool);
        assert_eq!(chance("minecraft:flint_and_steel"), WearChance::Tool);
        assert_eq!(chance("minecraft:elytra"), WearChance::Tool);
        assert_eq!(chance("minecraft:diamond_chestplate"), WearChance::Armour);
        assert_eq!(chance("minecraft:turtle_helmet"), WearChance::Armour);
        // The spear's pack `damage_chance` is 0-100.
        assert_eq!(
            chance("minecraft:iron_spear"),
            WearChance::Component { min: 0, max: 100 }
        );
        let percents = |chance: WearChance| [1, 2, 3].map(|level| chance.percent(level));
        assert_eq!(percents(WearChance::Tool), [50, 33, 25]);
        assert_eq!(percents(WearChance::Armour), [80, 73, 70]);
        assert_eq!(
            percents(WearChance::Component { min: 0, max: 100 }),
            [50, 33, 25]
        );
        assert_eq!(
            percents(WearChance::Component { min: 100, max: 100 }),
            [100; 3]
        );
    }

    /// Without Unbreaking, or on an `Unbreakable` item, no roll is made.
    #[test]
    fn plain_and_unbreakable_items_roll_nothing() {
        let pickaxe = stack("minecraft:diamond_pickaxe", 1);
        let mut tag = crate::item::enchant::item_tag(&pickaxe).unwrap_or_default();
        tag.0.insert(
            "Unbreakable".to_string(),
            zuri_nbt::NBTTag::Byte(zuri_nbt::tag::Byte(1)),
        );
        let unbreakable = with_enchantments(
            &crate::item::enchant::with_item_tag(&pickaxe, tag),
            [(Enchantment::Unbreaking, 3)],
        );
        let mut rng = StdRng::seed_from_u64(7);
        let mut untouched = rng.clone();
        assert_eq!(durability_damage(&wear(&pickaxe, 2, &mut rng).unwrap()), 2);
        assert!(wear(&unbreakable, 2, &mut rng).is_none());
        assert_eq!(rng.r#gen::<u64>(), untouched.r#gen::<u64>());
    }

    /// `durability/testdata/bds-unbreaking.json`.
    #[derive(serde::Deserialize)]
    struct BdsUnbreaking {
        mining: BTreeMap<u8, Bernoulli>,
        armour: BTreeMap<u8, ArmourHits>,
        #[serde(rename = "use")]
        item_use: BTreeMap<u8, Bernoulli>,
        attack: BTreeMap<u8, Events>,
        sword: BTreeMap<u8, Events>,
        spear: BTreeMap<u8, Events>,
        elytra: BTreeMap<u8, Glides>,
        threshold: Thresholds,
    }

    /// Wear events and how many of them cost a point.
    #[derive(serde::Deserialize)]
    struct Bernoulli {
        samples: u32,
        worn: u32,
    }

    /// Per armour piece, the hits that wore it by 0, 1 and 2 points.
    #[derive(serde::Deserialize)]
    struct ArmourHits {
        hits: u32,
        pieces: BTreeMap<String, [u32; 3]>,
    }

    /// Wear events and how many wore 0, 1 and 2 points.
    #[derive(serde::Deserialize)]
    struct Events {
        events: u32,
        worn: [u32; 3],
    }

    /// Elytra glides and the points they wore (one per wear event).
    #[derive(serde::Deserialize)]
    struct Glides {
        glides: u32,
        points: u32,
    }

    /// Per starting damage, the damage after each 1-point wear event, `None`
    /// once the item broke.
    #[derive(serde::Deserialize)]
    struct Thresholds {
        mining: BTreeMap<i32, Vec<Option<i32>>>,
        armour: BTreeMap<i32, Vec<Option<i32>>>,
    }

    fn bds() -> BdsUnbreaking {
        serde_json::from_str(include_str!("durability/testdata/bds-unbreaking.json")).unwrap()
    }

    /// Unbreaking against the BDS sweeps, at levels 0-III, rolled from a
    /// seeded RNG: a pickaxe breaking stone and flint and steel lighting
    /// fire (1 point each), four diamond armour pieces hit for 2, a pickaxe
    /// hitting pigs and a sword breaking cobweb (2 each), an iron spear
    /// hitting pigs (1) and an elytra gliding (1 per 20 ticks).
    #[test]
    fn unbreaking_matches_the_bds_sweep() {
        let bds = bds();
        let mut rng = StdRng::seed_from_u64(26);
        let rolls = 20_000;
        let mut failures = Vec::new();
        let mut points_worn = |name: &str, level: u8, points: i32| {
            let item = stack(name, 1);
            let item = if level > 0 {
                with_enchantments(&item, [(Enchantment::Unbreaking, level)])
            } else {
                item
            };
            let mut counts = [0u32; 3];
            for _ in 0..rolls {
                let worn = wear(&item, points, &mut rng).map_or(0, |out| durability_damage(&out));
                counts[worn as usize] += 1;
            }
            counts
        };
        let mut compare = |what: String, ours: u32, theirs: u32, samples: u32| {
            if !same_share(ours, rolls, theirs, samples) {
                failures.push(format!(
                    "{what}: ours {ours}/{rolls}, BDS {theirs}/{samples}"
                ));
            }
        };
        for (what, name, table) in [
            ("mining", "minecraft:netherite_pickaxe", &bds.mining),
            ("use", "minecraft:flint_and_steel", &bds.item_use),
        ] {
            for (&level, row) in table {
                let ours = points_worn(name, level, 1)[1];
                compare(format!("{what} {level}"), ours, row.worn, row.samples);
            }
        }
        for (&level, row) in &bds.armour {
            let ours = points_worn("minecraft:diamond_chestplate", level, 2);
            for (piece, theirs) in &row.pieces {
                for points in 0..3 {
                    compare(
                        format!("armour {level} {piece}: {points} points"),
                        ours[points],
                        theirs[points],
                        row.hits,
                    );
                }
            }
        }
        for (what, name, points, table) in [
            ("attack", "minecraft:netherite_pickaxe", 2, &bds.attack),
            ("sword", "minecraft:netherite_sword", 2, &bds.sword),
            ("spear", "minecraft:iron_spear", 1, &bds.spear),
        ] {
            for (&level, row) in table {
                let ours = points_worn(name, level, points);
                for (worn, (&ours, &theirs)) in ours.iter().zip(&row.worn).enumerate() {
                    compare(
                        format!("{what} {level}: {worn} points"),
                        ours,
                        theirs,
                        row.events,
                    );
                }
            }
        }
        // Without Unbreaking every 20 gliding ticks wear 1, so level 0's
        // points per glide are the wear events per glide.
        let plain = &bds.elytra[&0];
        for (&level, row) in &bds.elytra {
            let events = row.glides * plain.points / plain.glides;
            let ours = points_worn("minecraft:elytra", level, 1)[1];
            compare(format!("elytra {level}"), ours, row.points, events);
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// The BDS threshold sweep: a netherite pickaxe breaking stone and a
    /// diamond helmet taking 1-point hits from one short of their durability
    /// and from it.
    #[test]
    fn items_break_past_their_durability_like_bds() {
        let bds = bds();
        let mut rng = StdRng::seed_from_u64(3);
        for (name, table) in [
            ("minecraft:netherite_pickaxe", &bds.threshold.mining),
            ("minecraft:diamond_helmet", &bds.threshold.armour),
        ] {
            for (&start, after) in table {
                let mut item = with_durability_damage(&stack(name, 1), start);
                let mut ours = Vec::new();
                for _ in after {
                    item = wear(&item, 1, &mut rng).unwrap();
                    ours.push((!item.is_empty()).then(|| durability_damage(&item)));
                }
                assert_eq!(&ours, after, "{name} from {start}");
            }
        }
    }
}
