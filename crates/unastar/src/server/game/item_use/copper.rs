//! Copper blocks an axe scrapes and honeycomb waxes, resolved once from the
//! block registry.
//!
//! Every copper block is a family member: a base block (`copper_block`,
//! `cut_copper_stairs`, `lightning_rod`, ...), an oxidation stage (none,
//! `exposed_`, `weathered_`, `oxidized_`) and whether it is `waxed_`. The
//! registry's names spell those out (the unaged, unwaxed `copper_block` is
//! `exposed_copper`/`waxed_copper` with the stage or wax prefix), so the
//! relations below are derived from registered names when the registry
//! loads and kept by block id; a relation exists only when both blocks are
//! registered.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::registry::BlockRegistry;
use crate::registry::block::BlockEntry;

/// Oxidation stages, youngest first, by name prefix.
const STAGES: [&str; 4] = ["", "exposed_", "weathered_", "oxidized_"];
const WAXED: &str = "waxed_";
/// The unaged, unwaxed copper block, named apart from its family.
const COPPER_BLOCK: &str = "copper_block";
/// What `COPPER_BLOCK` is called after a stage or wax prefix.
const COPPER_BLOCK_PREFIXED: &str = "copper";
/// Families left alone: BDS opened a waxed copper chest instead of scraping
/// it (this server does not open copper chests yet) and copper golem statues
/// were not measured, so neither scrapes nor waxes here.
const LEFT_ALONE: [&str; 2] = ["copper_chest", "copper_golem_statue"];

/// What an axe does to a copper block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scrape {
    /// Removes the wax: the same stage unwaxed.
    WaxOff(u32),
    /// Removes one oxidation stage.
    Deoxidize(u32),
}

impl Scrape {
    /// The block id the scrape turns the block into.
    pub fn into_block(self) -> u32 {
        match self {
            Self::WaxOff(id) | Self::Deoxidize(id) => id,
        }
    }
}

#[derive(Default)]
struct Relations {
    /// Block id to what an axe makes of it.
    scrape: HashMap<u32, Scrape>,
    /// Unwaxed block id to its waxed block id.
    wax: HashMap<u32, u32>,
}

/// A registered name's family member: base, stage index, waxed.
fn member(name: &str) -> Option<(String, usize, bool)> {
    let base = name.strip_prefix("minecraft:")?;
    let (base, waxed) = match base.strip_prefix(WAXED) {
        Some(rest) => (rest, true),
        None => (base, false),
    };
    let (stage, base) = STAGES
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(stage, prefix)| Some((stage, base.strip_prefix(prefix)?)))
        .unwrap_or((0, base));
    let base = if base == COPPER_BLOCK_PREFIXED && (stage > 0 || waxed) {
        COPPER_BLOCK
    } else {
        base
    };
    Some((base.to_owned(), stage, waxed))
}

/// The registered name of a family member.
fn name(base: &str, stage: usize, waxed: bool) -> String {
    let base = if base == COPPER_BLOCK && (stage > 0 || waxed) {
        COPPER_BLOCK_PREFIXED
    } else {
        base
    };
    let wax = if waxed { WAXED } else { "" };
    format!("minecraft:{wax}{}{base}", STAGES[stage])
}

fn relations(registry: &BlockRegistry) -> Relations {
    let id = |name: &str| {
        registry
            .get_by_name(name)
            .map(|entry: &BlockEntry| entry.id)
    };
    let mut out = Relations::default();
    for entry in registry.iter() {
        let Some((base, stage, waxed)) = member(&entry.string_id) else {
            continue;
        };
        // Only names that round-trip are family members (`copper_ore` is
        // not: no `waxed_copper_ore` or `exposed_copper_ore` exists, so it
        // gains no relation below either).
        if name(&base, stage, waxed) != entry.string_id || LEFT_ALONE.contains(&base.as_str()) {
            continue;
        }
        if waxed {
            if let Some(unwaxed) = id(&name(&base, stage, false)) {
                out.scrape.insert(entry.id, Scrape::WaxOff(unwaxed));
            }
            continue;
        }
        if let Some(waxed) = id(&name(&base, stage, true)) {
            out.wax.insert(entry.id, waxed);
        }
        if stage > 0
            && let Some(younger) = id(&name(&base, stage - 1, false))
        {
            out.scrape.insert(entry.id, Scrape::Deoxidize(younger));
        }
    }
    out
}

static VANILLA: LazyLock<Relations> = LazyLock::new(|| relations(BlockRegistry::vanilla()));

/// What an axe does to the block `block_id`, if anything.
pub fn scrape(block_id: u32) -> Option<Scrape> {
    VANILLA.scrape.get(&block_id).copied()
}

/// The waxed block honeycomb makes of `block_id`, if it waxes.
pub fn wax(block_id: u32) -> Option<u32> {
    VANILLA.wax.get(&block_id).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> u32 {
        BlockRegistry::vanilla().get_by_name(name).expect(name).id
    }

    #[test]
    fn axes_scrape_one_stage_or_the_wax() {
        for (from, to) in [
            ("minecraft:exposed_copper", "minecraft:copper_block"),
            ("minecraft:oxidized_copper", "minecraft:weathered_copper"),
            (
                "minecraft:weathered_cut_copper_stairs",
                "minecraft:exposed_cut_copper_stairs",
            ),
            ("minecraft:exposed_lightning_rod", "minecraft:lightning_rod"),
            ("minecraft:exposed_copper_bulb", "minecraft:copper_bulb"),
        ] {
            assert_eq!(scrape(id(from)), Some(Scrape::Deoxidize(id(to))), "{from}");
        }
        for (from, to) in [
            ("minecraft:waxed_copper", "minecraft:copper_block"),
            (
                "minecraft:waxed_oxidized_cut_copper_slab",
                "minecraft:oxidized_cut_copper_slab",
            ),
        ] {
            assert_eq!(scrape(id(from)), Some(Scrape::WaxOff(id(to))), "{from}");
        }
        for name in [
            "minecraft:copper_block",
            "minecraft:copper_ore",
            "minecraft:stone",
            "minecraft:waxed_exposed_copper_chest",
            "minecraft:exposed_copper_golem_statue",
        ] {
            assert_eq!(scrape(id(name)), None, "{name}");
        }
    }

    #[test]
    fn honeycomb_waxes_every_unwaxed_stage() {
        assert_eq!(
            wax(id("minecraft:copper_block")),
            Some(id("minecraft:waxed_copper"))
        );
        assert_eq!(
            wax(id("minecraft:exposed_cut_copper_stairs")),
            Some(id("minecraft:waxed_exposed_cut_copper_stairs"))
        );
        assert_eq!(wax(id("minecraft:waxed_copper")), None);
        assert_eq!(wax(id("minecraft:copper_ore")), None);
        assert_eq!(wax(id("minecraft:copper_chest")), None);
        // Every waxed block in the registry but the families left alone is
        // reached from its unwaxed one.
        let waxed = BlockRegistry::vanilla()
            .iter()
            .filter(|entry| entry.string_id.starts_with("minecraft:waxed_"))
            .filter(|entry| {
                !LEFT_ALONE
                    .iter()
                    .any(|base| entry.string_id.ends_with(base))
            })
            .count();
        assert_eq!(VANILLA.wax.len(), waxed);
        assert!(waxed > 50, "{waxed}");
    }
}
