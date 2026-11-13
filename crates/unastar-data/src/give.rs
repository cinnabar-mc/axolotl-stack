//! What `/give` gives for each item name, measured on BDS 1.26.51.
//!
//! Source: `data/measured/give_items.json`, written by the ignored test
//! `writes_the_give_table` in `crates/unastar/src/command/exec/give.rs` from
//! the capture of `/give @s <item>` for every item of the native registry
//! (`crates/unastar/src/command/exec/testdata/bds-give.json`, hotbar slot 0
//! of the inventory BDS resent). For every item BDS gives: the item that
//! lands in the slot (a legacy name gives its first flattened item: `wool`
//! gives `white_wool`). Items the command's item list names but BDS does
//! not give are `not_found`. The block state a given stack carries is
//! [`crate::item_blocks`]'s.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

/// The measured table.
#[derive(Debug, Deserialize)]
pub struct GiveTable {
    /// Item name as typed (namespaced) to what BDS gave.
    pub given: BTreeMap<String, GivenItem>,
    /// Names in the command's item list that BDS answers with
    /// `commands.give.item.notFound`.
    pub not_found: Vec<String>,
}

/// What lands in the slot.
#[derive(Debug, Deserialize)]
pub struct GivenItem {
    /// Namespaced identifier of the item given.
    pub item: String,
}

/// Decode the embedded table; consumers normally use [`vanilla`].
pub fn decode_vanilla() -> Result<GiveTable, serde_json::Error> {
    serde_json::from_str(include_str!("../data/measured/give_items.json"))
}

/// The embedded table.
pub fn vanilla() -> &'static GiveTable {
    static TABLE: LazyLock<GiveTable> =
        LazyLock::new(|| decode_vanilla().expect("embedded BDS give table decodes"));
    &TABLE
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_table_decodes() {
        let table = super::decode_vanilla().unwrap();
        assert!(table.given.len() > 1600);
        assert_eq!(table.given["minecraft:wool"].item, "minecraft:white_wool");
    }
}
